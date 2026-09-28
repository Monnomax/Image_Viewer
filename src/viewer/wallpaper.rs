// viewer/wallpaper.rs — пункт контекстного меню "Встановити як тло":
// встановлює поточне зображення як шпалери робочого столу GNOME.
//
// Перед власне встановленням завжди показується діалог підтвердження
// (adw::AlertDialog, за тим самим взірцем, що й підтвердження видалення
// в menu.rs) — дія одразу перезаписує системні шпалери користувача, тож
// випадковий клік не повинен мати наслідків без явного підтвердження.
//
// Саме встановлення виконується через схему GSettings
// "org.gnome.desktop.background": ключі "picture-uri" та "picture-uri-dark"
// виставляються в file:// URI поточного зображення (обидва — щоб шпалери
// не "стрибали" при перемиканні системної світлої/темної теми), а
// "picture-options" — у "zoom", якщо перед тим там стояло "none"
// (порожні шпалери з попереднього стану виглядали б дивно поруч зі
// щойно встановленим фото).

use crate::viewer::Viewer;
use adw::prelude::*;
use gtk4::gio;
use std::cell::RefCell;
use std::rc::Rc;

const BACKGROUND_SCHEMA: &str = "org.gnome.desktop.background";

/// Показує діалог підтвердження і, у разі згоди, встановлює поточне
/// зображення переглядача як тло робочого столу.
/// Якщо список зображень порожній (немає поточного файлу) — нічого не робить.
pub fn show(window: &adw::ApplicationWindow, viewer: &Rc<RefCell<Viewer>>) {
    let Some(path) = viewer.borrow().model.current_path() else {
        return;
    };

    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    let dialog = adw::AlertDialog::builder()
        .heading("Встановити як тло робочого столу?")
        .body(format!(
            "Поточні шпалери робочого столу буде замінено на «{file_name}». Продовжити?"
        ))
        .default_response("accept")
        .close_response("cancel")
        .build();
    dialog.add_response("cancel", "Скасувати");
    dialog.add_response("accept", "Встановити");
    dialog.set_response_appearance("accept", adw::ResponseAppearance::Suggested);

    dialog.choose(window, None::<&gio::Cancellable>, move |response| {
        if response == "accept" {
            if let Err(err) = set_as_wallpaper(&path) {
                eprintln!("Не вдалося встановити тло робочого столу {:?}: {}", path, err);
            }
        }
    });
}

/// Виставляє ключі схеми "org.gnome.desktop.background" так, щоб файл за
/// шляхом `path` став тлом робочого столу і у світлій, і в темній темі.
///
/// Як і в `preferences::settings::desktop_wallpaper_path`, наявність схеми
/// перевіряється заздалегідь через `SettingsSchemaSource` — на не-GNOME
/// оточеннях, де цієї схеми немає, голий `gio::Settings::new()` аварійно
/// завершив би процес.
fn set_as_wallpaper(path: &std::path::Path) -> Result<(), String> {
    gio::SettingsSchemaSource::default()
        .and_then(|source| source.lookup(BACKGROUND_SCHEMA, true))
        .ok_or_else(|| format!("Схема \"{BACKGROUND_SCHEMA}\" недоступна в цьому оточенні"))?;

    let uri = gio::File::for_path(path).uri();

    let settings = gio::Settings::new(BACKGROUND_SCHEMA);

    // Якщо раніше тло було вимкнене ("none"), самого лише URI недостатньо —
    // без "zoom"/іншого реального режиму показу зображення не буде видно.
    if settings.string("picture-options") == "none" {
        settings
            .set_string("picture-options", "zoom")
            .map_err(|e| e.to_string())?;
    }

    settings
        .set_string("picture-uri", &uri)
        .map_err(|e| e.to_string())?;
    settings
        .set_string("picture-uri-dark", &uri)
        .map_err(|e| e.to_string())?;

    Ok(())
}
