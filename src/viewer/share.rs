// viewer/share.rs — діалог "Поділитися" для поточного зображення.
//
// Доступні способи:
//   - Bluetooth — blueman-sendto / bluetooth-sendto
//   - Електронна пошта — nautilus-sendto
//   - Інша програма… — системний GTK app chooser через FileLauncher
//
// ImgViewer не реалізує сам протоколи передачі. Він лише передає поточний
// файл відповідному системному механізму.

use crate::viewer::Viewer;
use adw::prelude::*;
use adw::ApplicationWindow;
use gtk4::gio;
use std::cell::RefCell;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

#[derive(Clone, Copy)]
enum ShareMethod {
    Bluetooth(&'static str),
    Email(&'static str),
    OtherProgram,
}

/// Показує діалог вибору способу передачі поточного зображення.
pub fn show(window: &ApplicationWindow, viewer: &Rc<RefCell<Viewer>>) {
    let Some(path) = viewer.borrow().model.current_path() else {
        return;
    };

    let methods = available_methods();

    let dialog = adw::Dialog::builder()
        .title("Поділитися")
        .content_width(400)
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

    let group = adw::PreferencesGroup::new();

    for method in methods {
        let row = build_method_row(method);

        let dialog_for_row = dialog.clone();
        let path_for_row = path.clone();
        let window_for_row = window.clone();

        row.connect_activated(move |_| {
            match method {
                ShareMethod::OtherProgram => {
                    // Спочатку закриваємо наше діалогове вікно, після чого
                    // GTK показує системний вибір застосунку.
                    dialog_for_row.close();

                    launch_with_app_chooser(&window_for_row, &path_for_row);
                }

                ShareMethod::Bluetooth(_) | ShareMethod::Email(_) => {
                    let result = launch_method(method, &path_for_row);

                    dialog_for_row.close();

                    if let Err(error) = result {
                        show_error(&window_for_row, error);
                    }
                }
            }
        });

        group.add(&row);
    }

    content.append(&group);

    toolbar_view.set_content(Some(&content));
    dialog.set_child(Some(&toolbar_view));

    dialog.present(Some(window));
}

/// Визначає доступні системні засоби передачі.
///
/// "Інша програма…" доступна завжди, оскільки її обробляє GTK.
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

    methods.push(ShareMethod::OtherProgram);

    methods
}

/// Створює рядок одного способу передачі.
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

        ShareMethod::OtherProgram => {
            let row = adw::ActionRow::builder()
                .title("Інша програма…")
                .subtitle("Вибрати програму для відкриття зображення")
                .activatable(true)
                .build();

            row.add_prefix(&gtk4::Image::from_icon_name(
                "application-x-executable-symbolic",
            ));
            row.add_suffix(&gtk4::Image::from_icon_name("go-next-symbolic"));

            row
        }
    }
}

/// Запускає Bluetooth або email helper.
fn launch_method(method: ShareMethod, path: &PathBuf) -> Result<(), String> {
    let command = match method {
        ShareMethod::Bluetooth(command) => command,
        ShareMethod::Email(command) => command,
        ShareMethod::OtherProgram => {
            return Err("Внутрішня помилка: для цього способу потрібен GTK FileLauncher".to_string());
        }
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

/// Відкриває системний chooser програм.
///
/// always_ask=true змушує GTK запитати, якою програмою відкрити файл,
/// замість автоматичного використання типової програми.
fn launch_with_app_chooser(window: &ApplicationWindow, path: &Path) {
    let file = gio::File::for_path(path);

    let launcher = gtk4::FileLauncher::new(Some(&file));
    launcher.set_always_ask(true);

    let window_for_callback = window.clone();

    launcher.launch(
        Some(window),
        None::<&gio::Cancellable>,
        move |result| {
            if let Err(error) = result {
                show_error(
                    &window_for_callback,
                    format!(
                        "Не вдалося відкрити вибрану програму для цього файлу: {}",
                        error
                    ),
                );
            }
        },
    );
}

/// Перевіряє, чи існує виконуваний файл у PATH.
fn command_exists(command: &str) -> bool {
    let Some(path_var) = env::var_os("PATH") else {
        return false;
    };

    env::split_paths(&path_var).any(|directory| directory.join(command).is_file())
}

/// Показує помилку запуску.
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