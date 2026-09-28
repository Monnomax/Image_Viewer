// viewer/rename.rs — пункт контекстного меню "Перейменувати": діалог, що
// дозволяє змінити ім'я файлу поточного зображення просто з переглядача,
// без виходу у файловий менеджер.
//
// Увесь код (побудова діалогу, виділення імені без розширення, сама
// операція перейменування на диску та оновлення стану Viewer) навмисно
// зібраний в одному цьому файлі — виклик іззовні лише один: rename::show().
//
// Після перейменування на диску запис у ImageCache/TextureCache під
// старим шляхом прибирається (окремого "перейменування" ключа кешу немає —
// файл однаково перечитується з диска майже миттєво, бо це той самий inode,
// просто під новим ім'ям), а модель (Model::images) оновлюється на новий
// шлях за поточним індексом.

use crate::viewer::Viewer;
use adw::prelude::*;
use gtk4::glib;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

/// Показує модальний діалог перейменування поточного зображення.
/// Якщо список зображень порожній (немає поточного файлу) — нічого не робить.
pub fn show(window: &adw::ApplicationWindow, area: &gtk4::Widget, viewer: Rc<RefCell<Viewer>>) {
    let Some(old_path) = viewer.borrow().model.current_path() else {
        return;
    };

    let file_name = old_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    let dialog = adw::Dialog::builder()
        .title("Перейменувати")
        .content_width(300)
        .content_height(150)
        .build();

    // AdwDialog презентується як оверлей поверх батьківського вікна.
    // Кнопку закриття (X) у шапці навмисно вимкнено нижче — закриття лише
    // через "Скасувати"/"Застосувати" (або Escape).
    let toolbar_view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new("Перейменувати", "")));
    // Без кнопки закриття (X) — закрити діалог можна лише "Скасувати",
    // "Застосувати" або Escape.
    header.set_show_start_title_buttons(false);
    header.set_show_end_title_buttons(false);
    toolbar_view.add_top_bar(&header);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
    content.set_margin_start(16);
    content.set_margin_end(16);
    content.set_margin_top(4);
    content.set_margin_bottom(16);

    let entry = gtk4::Entry::new();
    entry.set_text(&file_name);
    entry.set_hexpand(true);
    content.append(&entry);

    // Коли поле отримує фокус, залишаємо виділеною лише базову назву.
    {
        let file_name = file_name.clone();
        entry.connect_has_focus_notify(move |e| {
            if e.has_focus() {
                select_stem(e, &file_name);
            }
        });
    }

    let error_label = gtk4::Label::new(None);
    error_label.add_css_class("error");
    error_label.set_halign(gtk4::Align::Start);
    error_label.set_wrap(true);
    error_label.set_visible(false);
    content.append(&error_label);

    let button_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    button_box.set_halign(gtk4::Align::End);

    let cancel_button = gtk4::Button::with_label("Скасувати");
    let apply_button = gtk4::Button::with_label("Застосувати");
    apply_button.add_css_class("suggested-action");
    apply_button.set_sensitive(!file_name.trim().is_empty());

    button_box.append(&cancel_button);
    button_box.append(&apply_button);
    content.append(&button_box);

    toolbar_view.set_content(Some(&content));
    dialog.set_child(Some(&toolbar_view));

    // "Застосувати" вимкнена, поки поле порожнє — порожня назва файлу
    // неприпустима.
    {
        let apply_button = apply_button.clone();
        entry.connect_changed(move |e| {
            apply_button.set_sensitive(!e.text().trim().is_empty());
        });
    }

    {
        let dialog = dialog.clone();
        cancel_button.connect_clicked(move |_| {
            dialog.close();
        });
    }

    let do_apply = {
        let dialog = dialog.clone();
        let entry = entry.clone();
        let error_label = error_label.clone();
        let viewer = viewer.clone();
        let area = area.clone();
        let old_path = old_path.clone();
        move || {
            let new_name = entry.text().trim().to_string();
            if new_name.is_empty() {
                return;
            }
            match rename_current(&viewer, &old_path, &new_name) {
                Ok(()) => {
                    area.queue_draw();
                    dialog.close();
                }
                Err(msg) => {
                    error_label.set_label(&msg);
                    error_label.set_visible(true);
                }
            }
        }
    };

    {
        let do_apply = do_apply.clone();
        apply_button.connect_clicked(move |_| do_apply());
    }
    {
        let do_apply = do_apply.clone();
        // Enter у полі редагування теж застосовує перейменування.
        entry.connect_activate(move |_| do_apply());
    }

    dialog.present(Some(window));

    // Відразу після появи віконця виділяємо назву файлу без розширення —
    // типове перейменування "змінити лише ім'я" не повинно вимагати
    // спершу стирати весь текст вручну. Прив'язуємось до сигналу "map":
    // на відміну від idle_add (де точний момент — здогадка), "map"
    // гарантовано спрацьовує вже після того, як entry стало частиною
    // змапованого вікна і grab_focus()/select_region() почали працювати
    // надійно. connect_has_focus_notify вище лишається запобіжником: якщо
    // щось (сам AdwDialog чи система) ще раз перехопить фокус пізніше —
    // виділення стемy буде застосоване знову.
    {
        let file_name = file_name.clone();
        let cancel_button = cancel_button.clone();
        entry.connect_map(move |e| {
            // GTK при *першому* встановленні фокуса в щойно показаному
            // вікні, якщо це GtkEditable, сам виділяє ввесь текст
            // (0..-1) — саме тому лишалось виділеним і розширення.
            // Витрачаємо цей одноразовий "початковий фокус" на кнопці
            // (вона не Editable, тож нічого не виділяє), і вже тоді
            // фокусуємось на entry — це вже не перший фокус, тож
            // вбудоване "виділити все" не спрацьовує і наш вибір стemу
            // лишається останнім словом.
            cancel_button.grab_focus();
            e.grab_focus();
            select_stem(e, &file_name);
        });
    }

    // Останній, "гарантовано пізній" запобіжник: щось (найімовірніше —
    // саме завершення анімації відкриття AdwDialog) ще раз перевиділяє
    // весь текст уже ПІСЛЯ map/grab_focus вище. Через невелику затримку
    // перевиділяємо лише стем ще раз — цей виклик гарантовано пізніший за
    // будь-яку внутрішню логіку відкриття діалогу. Спрацьовує лише якщо
    // користувач ще нічого не встиг ввести (текст не змінився), щоб не
    // затерти реальне редагування.
    {
        let entry = entry.clone();
        let file_name = file_name.clone();
        glib::timeout_add_local_once(Duration::from_millis(250), move || {
            if entry.text() == file_name {
                select_stem(&entry, &file_name);
            }
        });
    }
}

/// Виділяє в полі `entry` назву файлу без розширення (без завершальної
/// крапки й самого розширення). Файли без крапки, а також "приховані"
/// імена без "справжнього" розширення (напр. ".bashrc" — крапка лише на
/// початку) виділяються повністю: відрізняти там "розширення" нема сенсу.
fn select_stem(entry: &gtk4::Entry, file_name: &str) {
    let char_count = file_name.chars().count() as i32;
    let stem_end = match file_name.rfind('.') {
        Some(byte_pos) if byte_pos > 0 => file_name[..byte_pos].chars().count() as i32,
        _ => char_count,
    };
    entry.select_region(0, stem_end);
}

/// Перейменовує файл поточного зображення на диску і оновлює стан
/// переглядача (Model + кеші), щоб перегляд одразу відобразив файл під
/// новою назвою.
fn rename_current(
    viewer: &Rc<RefCell<Viewer>>,
    old_path: &PathBuf,
    new_name: &str,
) -> Result<(), String> {
    if new_name.contains('/') {
        return Err("Назва файлу не може містити символ \"/\"".to_string());
    }

    let parent = old_path
        .parent()
        .ok_or_else(|| "Не вдалося визначити каталог файлу".to_string())?;
    let new_path = parent.join(new_name);

    if new_path == *old_path {
        return Ok(());
    }

    if new_path.exists() {
        return Err(format!("Файл \"{}\" вже існує", new_name));
    }

    std::fs::rename(old_path, &new_path).map_err(|e| e.to_string())?;

    let mut v = viewer.borrow_mut();

    // Прибираємо записи під старим шляхом з обох кешів — зображення
    // підвантажиться заново вже під новим шляхом (з диска це майже
    // миттєво, бо фізично це той самий файл).
    v.image_cache.remove(old_path);
    v.texture_cache.remove(old_path);
    v.loading.remove(old_path);

    // Зберігаємо індекс в окрему змінну, щоб уникнути конфлікту запозичень (Borrow Checker)
    let current_index = v.model.index;

    if let Some(entry) = v.model.images.get_mut(current_index) {
        *entry = new_path.clone();
    }

    v.needs_initial_fit = true;
    v.ensure_loaded(&new_path);

    Ok(())
}
