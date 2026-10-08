// viewer/properties.rs — пункт контекстного меню "Властивості": вікно з
// мініатюрою поточного зображення, базовими характеристиками (розмір у
// пікселях/формат/розмір файлу), правами доступу та переходом на другу
// сторінку з повним переліком метаданих файлу, який читається через
// GExiv2/rexiv2.
//
// Базові характеристики файлу (форматування розміру, права доступу тощо)
// винесені в viewer/file_info.rs — спільний модуль, не прив'язаний до
// EXIF/XMP/IPTC (на відміну від viewer/metadata.rs) і не залежний від
// feature "gexiv2-metadata".
//
// Друга сторінка реалізована через adw::NavigationView — це ТЕ САМЕ вікно
// (той самий adw::Window), просто з новою "сторінкою" на стеку навігації;
// кнопка "Назад" (іконка "go-previous-symbolic") з'являється в заголовку
// автоматично, без ручного коду — так само, як вже влаштована бічна
// панель налаштувань у preferences/dialog.rs (AdwNavigationSplitView +
// AdwHeaderBar) у цьому ж проєкті.
//
// На відміну від rename.rs, тут немає жодного редагування — це разовий
// знімок стану на момент відкриття (viewer позичається лише один раз,
// на самому старті show()).

use crate::viewer::file_info;
use crate::viewer::Viewer;
use adw::prelude::*;
use gtk4::gdk;
use gtk4::gdk_pixbuf::Pixbuf;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

pub fn show(viewer: &Rc<RefCell<Viewer>>) {
    let Some(path) = viewer.borrow().model.current_path() else {
        return;
    };

    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    // Мініатюра: якщо зображення вже декодоване (майже завжди так, бо
    // пункт меню діє на поточне, вже показане на екрані фото) — беремо
    // його з ImageCache без повторного читання диска; інакше (рідкісний
    // край: властивості відкрили ще до завершення асинхронного
    // завантаження) декодуємо один раз синхронно тут-таки.
    let pixbuf: Option<Rc<Pixbuf>> = {
        let mut v = viewer.borrow_mut();
        v.image_cache.get(&path)
    }
    .or_else(|| Pixbuf::from_file(&path).ok().map(Rc::new));

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    let window_dialog = adw::Window::builder()
        .title(file_name.clone())
        .default_width(480)
        .default_height(690)
        .destroy_with_parent(true)
        .modal(false)
        .build();

    let nav_view = adw::NavigationView::new();

    let details_page = crate::viewer::metadata::build_metadata_page(&path, &ext, pixbuf.as_ref());
    let permissions_page = file_info::build_permissions_page(&path);
    let main_page = build_main_page(
        &file_name,
        &path,
        &ext,
        pixbuf.as_ref(),
        &nav_view,
        &details_page,
        permissions_page.as_ref(),
    );

    nav_view.push(&main_page);
    window_dialog.set_content(Some(&nav_view));
    window_dialog.present();
}

/// Перша сторінка: мініатюра + основні характеристики + рядки-переходи.
fn build_main_page(
    file_name: &str,
    path: &Path,
    ext: &str,
    pixbuf: Option<&Rc<Pixbuf>>,
    nav_view: &adw::NavigationView,
    details_page: &adw::NavigationPage,
    permissions_page: Option<&adw::NavigationPage>,
) -> adw::NavigationPage {
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new(file_name, "")));

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.set_margin_start(20);
    content.set_margin_end(20);
    content.set_margin_bottom(20);

    // ---- Мініатюра: макс. висота 100px, ширина не обмежується, кути
    // заокруглені на 10px; відступ згори 0, знизу 18px ----
    if let Some(pixbuf) = pixbuf {
        let texture = gdk::Texture::for_pixbuf(pixbuf);
        let picture = gtk4::Picture::for_paintable(&texture);
        picture.set_can_shrink(true);
        picture.set_content_fit(gtk4::ContentFit::Contain);
        picture.set_height_request(100);
        picture.set_halign(gtk4::Align::Center);
        picture.set_margin_top(0);
        picture.set_margin_bottom(18);
        apply_rounded_corners(&picture);
        content.append(&picture);
    }

    // ---- Перша група: розмір зображення / формат / розмір файлу ----
    let group_basic = adw::PreferencesGroup::new();

    let dims_text = pixbuf
        .map(|p| format!("{} × {} пікс.", p.width(), p.height()))
        .unwrap_or_else(|| "Невідомо".to_string());
    group_basic.add(&file_info::param_row("Розмір зображення", dims_text));
    group_basic.add(&file_info::param_row(
        "Формат",
        file_info::format_extension_name(ext),
    ));

    let size_text = std::fs::metadata(path)
        .map(|m| file_info::format_file_size(m.len()))
        .unwrap_or_else(|_| "Невідомо".to_string());
    group_basic.add(&file_info::param_row("Розмір файлу", size_text));

    content.append(&group_basic);

    // ---- Рядок-перехід: Властивості зображення (усі параметри) ----
    let group_details = adw::PreferencesGroup::new();
    group_details.set_margin_top(18);

    let row_details = adw::ActionRow::builder()
        .title("Властивості зображення")
        .activatable(true)
        .build();
    row_details.add_suffix(&gtk4::Image::from_icon_name("go-next-symbolic"));
    {
        let nav_view = nav_view.clone();
        let details_page = details_page.clone();
        row_details.connect_activated(move |_| {
            nav_view.push(&details_page);
        });
    }
    group_details.add(&row_details);
    content.append(&group_details);

    // ---- Права доступу: клікабельний рядок зі стрілкою, переходить на
    // окрему сторінку nav_view з детальним відображенням дозволів
    // (file_info.rs) — так само, як рядок "Властивості зображення" ----
    let group_perms = adw::PreferencesGroup::new();
    group_perms.set_margin_top(18);
    group_perms.add(&file_info::permissions_row(nav_view, permissions_page));
    content.append(&group_perms);

    let scroller = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vexpand(true)
        .child(&content)
        .build();

    toolbar_view.set_content(Some(&scroller));

    adw::NavigationPage::builder()
        .title(file_name)
        .child(&toolbar_view)
        .build()
}

/// Кути мініатюри заокруглені через CSS-клас, а її вміст обрізається
/// властивістю GTK-віджета.
fn apply_rounded_corners(picture: &gtk4::Picture) {
    const CSS_CLASS: &str = "imgviewer-properties-thumbnail";

    let provider = gtk4::CssProvider::new();
    provider.load_from_string(&format!(
        ".{class} {{ border-radius: 10px; }}",
        class = CSS_CLASS
    ));

    if let Some(display) = gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    picture.add_css_class(CSS_CLASS);
    picture.set_overflow(gtk4::Overflow::Hidden);
}
