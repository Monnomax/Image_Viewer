// viewer/share.rs — діалог "Поділитися" для поточного зображення.
//
// Реальне передавання файлу делегується системним утилітам:
//   - blueman-sendto / bluetooth-sendto — Bluetooth
//   - nautilus-sendto — електронна пошта
//
// Застосунок не має власної реалізації Bluetooth або поштового клієнта.
// Це навмисно: ImgViewer лише передає поточний файл відповідному
// системному інструменту.

use crate::viewer::Viewer;
use adw::prelude::*;
use gtk4::gio;
use adw::ApplicationWindow;
use std::cell::RefCell;
use std::env;
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;

#[derive(Clone, Copy)]
enum ShareMethod {
    Bluetooth(&'static str),
    Email(&'static str),
}

/// Показує діалог вибору способу передачі поточного зображення.
///
/// Якщо поточного зображення немає — нічого не робить.
pub fn show(window: &ApplicationWindow, viewer: &Rc<RefCell<Viewer>>) {
    let Some(path) = viewer.borrow().model.current_path() else {
        return;
    };

    let methods = available_methods();

    let dialog = adw::Dialog::builder()
        .title("Поділитися")
        .content_width(400)
        .content_height(0)
        .build();

    let toolbar_view = adw::ToolbarView::new();

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new("Поділитися", "")));
    toolbar_view.add_top_bar(&header);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);

    let filename = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "зображення".to_string());

    let heading = gtk4::Label::new(Some(&format!(
        "Виберіть спосіб передачі «{}»",
        filename
    )));
    heading.set_halign(gtk4::Align::Start);
    heading.set_wrap(true);
    heading.set_margin_start(6);
    heading.set_margin_end(6);
    heading.set_margin_bottom(2);

    content.append(&heading);

    if methods.is_empty() {
        let group = adw::PreferencesGroup::new();

        let row = adw::ActionRow::builder()
            .title("Немає доступних способів")
            .subtitle("Не знайдено системних засобів передачі файлів.")
            .build();

        row.set_sensitive(false);
        group.add(&row);

        content.append(&group);
    } else {
        let group = adw::PreferencesGroup::new();

        for method in methods {
    let row = build_method_row(method);

    let dialog_for_row = dialog.clone();
    let path_for_row = path.clone();
    let window_for_row = window.clone();

    row.connect_activated(move |_| {
        let result = launch_method(method, &path_for_row);

        dialog_for_row.close();

        if let Err(error) = result {
            show_error(&window_for_row, error);
        }
    });

    group.add(&row);
}

        content.append(&group);
    }

    toolbar_view.set_content(Some(&content));
    dialog.set_child(Some(&toolbar_view));

    dialog.present(Some(window));
}

/// Визначає доступні системні засоби передачі.
///
/// Bluetooth підтримує два можливих інструменти:
///   1. blueman-sendto
///   2. bluetooth-sendto
///
/// Якщо встановлено обидва, використовується blueman-sendto.
fn available_methods() -> Vec<ShareMethod> {
    let mut methods = Vec::new();

    if command_exists("blueman-sendto") {
        methods.push(ShareMethod::Bluetooth("blueman-sendto"));
    } else if command_exists("bluetooth-sendto") {
        methods.push(ShareMethod::Bluetooth("bluetooth-sendto"));
    }

    if command_exists("nautilus-sendto") {
        methods.push(ShareMethod::Email("nautilus-sendto"));
    }

    methods
}

/// Створює один рядок способу передачі.
fn build_method_row(method: ShareMethod) -> adw::ActionRow {
    match method {
        ShareMethod::Bluetooth(command) => {
            let row = adw::ActionRow::builder()
                .title("Bluetooth")
                .subtitle("Надіслати зображення через Bluetooth")
                .activatable(true)
                .build();

            row.add_prefix(&gtk4::Image::from_icon_name("bluetooth-symbolic"));
            row.add_suffix(&gtk4::Image::from_icon_name("go-next-symbolic"));

            row.set_tooltip_text(Some(command));

            row
        }

        ShareMethod::Email(command) => {
            let row = adw::ActionRow::builder()
                .title("Електронна пошта")
                .subtitle("Створити лист із зображенням у вкладенні")
                .activatable(true)
                .build();

            row.add_prefix(&gtk4::Image::from_icon_name("mail-send-symbolic"));
            row.add_suffix(&gtk4::Image::from_icon_name("go-next-symbolic"));

            row.set_tooltip_text(Some(command));

            row
        }
    }
}

/// Запускає вибраний спосіб передачі.
fn launch_method(method: ShareMethod, path: &PathBuf) -> Result<(), String> {
    let command = match method {
        ShareMethod::Bluetooth(command) => command,
        ShareMethod::Email(command) => command,
    };

    Command::new(command)
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| {
            format!(
                "Не вдалося запустити {} для файлу «{}»: {}",
                command,
                path.display(),
                error
            )
        })
}

/// Перевіряє, чи існує виконуваний файл у PATH.
///
/// Тут ми навмисно не викликаємо зовнішню команду для самої перевірки:
/// достатньо знайти її в одному з каталогів PATH.
fn command_exists(command: &str) -> bool {
    let Some(path_var) = env::var_os("PATH") else {
        return false;
    };

    env::split_paths(&path_var).any(|directory| {
        let candidate = directory.join(command);

        candidate.is_file()
    })
}

/// Показує помилку запуску вже поверх головного вікна.
fn show_error(window: &ApplicationWindow, message: String) {
    let dialog = adw::AlertDialog::builder()
        .heading("Не вдалося поділитися")
        .body(message)
        .default_response("close")
        .close_response("close")
        .build();

    dialog.add_response("close", "Закрити");

    dialog.choose(window, None::<&gio::Cancellable>, |_| {});
}