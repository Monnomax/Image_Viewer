// main.rs — запуск застосунку.

mod app;

mod model;
mod preferences;
mod screen_brightness;
mod thumbnail_strip;
mod utils;
mod viewer;

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::Application;
use std::ffi::OsString;
use std::path::Path;

fn merged_gsettings_schema_dir(
    current: Option<&OsString>,
    project_data_dir: &Path,
) -> Option<OsString> {
    let compiled_path = project_data_dir.join("gschemas.compiled");
    if !compiled_path.is_file() {
        return current.map(|value| value.to_owned());
    }

    let project_dir = project_data_dir.to_path_buf();
    let existing_paths = current
        .map(|value| std::env::split_paths(value).collect::<Vec<_>>())
        .unwrap_or_default();

    let mut merged = Vec::with_capacity(existing_paths.len() + 1);
    if !existing_paths.iter().any(|path| path == &project_dir) {
        merged.push(project_dir);
    }
    merged.extend(existing_paths);

    std::env::join_paths(merged).ok()
}

fn configure_gsettings_schema_dir() {
    let manifest_data_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
    let current = std::env::var_os("GSETTINGS_SCHEMA_DIR");
    let merged = merged_gsettings_schema_dir(current.as_ref(), &manifest_data_dir);

    if let Some(schema_dir) = merged {
        std::env::set_var("GSETTINGS_SCHEMA_DIR", schema_dir);
    }
}

#[cfg(test)]
mod tests {
    use super::merged_gsettings_schema_dir;
    use std::ffi::OsString;

    #[test]
    fn merged_gsettings_schema_dir_appends_project_schema() {
        let temp_dir =
            std::env::temp_dir().join(format!("imgviewer-gsettings-test-{}", std::process::id()));
        let project_dir = temp_dir.join("data");
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::File::create(project_dir.join("gschemas.compiled")).unwrap();

        let existing = OsString::from("/usr/share/glib-2.0/schemas:/other/schema");
        let merged = merged_gsettings_schema_dir(Some(&existing), &project_dir)
            .expect("schema dir should be merged");
        let merged = merged.to_string_lossy().to_string();

        assert!(merged.starts_with(&project_dir.to_string_lossy().to_string()));
        assert!(merged.contains("/usr/share/glib-2.0/schemas"));
        assert!(merged.contains("/other/schema"));
    }

    #[test]
    fn merged_gsettings_schema_dir_skips_duplicate_project_entry() {
        let temp_dir = std::env::temp_dir().join(format!(
            "imgviewer-gsettings-test-dup-{}",
            std::process::id()
        ));
        let project_dir = temp_dir.join("data");
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::File::create(project_dir.join("gschemas.compiled")).unwrap();

        let existing = OsString::from(format!(
            "{}:/usr/share/glib-2.0/schemas",
            project_dir.to_string_lossy()
        ));
        let merged = merged_gsettings_schema_dir(Some(&existing), &project_dir)
            .expect("schema dir should still be provided");

        let count = merged
            .to_string_lossy()
            .split(':')
            .filter(|path| *path == project_dir.to_string_lossy().as_ref())
            .count();
        assert_eq!(count, 1);
    }
}

fn main() -> glib::ExitCode {
    configure_gsettings_schema_dir();

    // 1. Ініціалізація Libadwaita обов'язкова перед створенням віджетів
    adw::init().expect("Не вдалося ініціалізувати Libadwaita");

    // 2. Використовуємо gtk4::Application для точної сумісності з app::build
    let application = Application::builder()
        .application_id("com.example.ImgViewer")
        .flags(gtk4::gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    application.connect_activate(|app| {
        if let Some(window) = app.active_window() {
            window.present();
        } else {
            app::build(app, None, false);
        }
    });

    application.connect_open(|app, files, _hint| {
        let path = files.first().and_then(|file| file.path());
        if let Some(path) = path {
            app::build(app, Some(path.to_string_lossy().into_owned()), true);
        }
    });

    application.run()
}
