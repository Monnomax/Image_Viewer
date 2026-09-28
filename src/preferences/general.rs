// preferences/general.rs — розділ "Загальні": поведінка запуску та
// збереження стану перегляду в межах поточного сеансу.

use crate::model::SortMode;
use crate::preferences::settings::{DeleteAction, PreloadMode, Settings};
use adw::prelude::*;
use std::rc::Rc;

pub fn build_page<F>(settings: &Settings, _on_sort_changed: Rc<F>) -> adw::PreferencesPage
where
    F: Fn(SortMode) + 'static,
{
    let page = adw::PreferencesPage::builder()
        .title("Загальні")
        .icon_name("preferences-system-symbolic")
        .build();

    // ---- Група: запуск і стан ----
    let group_state = adw::PreferencesGroup::builder()
        .title("Запуск і стан")
        .build();

    let row_fullscreen = adw::SwitchRow::builder()
        .title("Запускати одразу у повноекранному режимі")
        .build();
    settings
        .inner()
        .bind("fullscreen-on-start", &row_fullscreen, "active")
        .build();
    group_state.add(&row_fullscreen);

    let row_remember_zoom = adw::SwitchRow::builder()
        .title("Запам'ятовувати масштаб")
        .build();
    settings
        .inner()
        .bind("remember-zoom", &row_remember_zoom, "active")
        .build();
    group_state.add(&row_remember_zoom);

    let row_remember_pos = adw::SwitchRow::builder()
        .title("Запам'ятовувати положення")
        .build();
    settings
        .inner()
        .bind("remember-position", &row_remember_pos, "active")
        .build();
    group_state.add(&row_remember_pos);

    page.add(&group_state);

    // ---- Група: продуктивність ----
    let group_performance = adw::PreferencesGroup::builder()
        .title("Продуктивність")
        .build();

    let preload_options = [
        "Автоматично",
        "2 зображення",
        "5 зображень",
        "10 зображень",
        "20 зображень",
    ];
    let preload_model = gtk4::StringList::new(&preload_options);
    let row_preload = adw::ComboRow::builder()
        .title("Попереднє завантаження зображень")
        .model(&preload_model)
        .selected(preload_mode_index(settings.preload_mode()))
        .build();

    {
        let settings = settings.clone();
        row_preload.connect_selected_notify(move |c| {
            settings.set_preload_mode(preload_mode_from_index(c.selected()));
        });
    }
    group_performance.add(&row_preload);

    let row_pretexture = adw::SwitchRow::builder()
        .title("Попередньо створювати текстури")
        .build();
    settings
        .inner()
        .bind("preload-textures-enabled", &row_pretexture, "active")
        .build();
    group_performance.add(&row_pretexture);

    page.add(&group_performance);

    // ---- Група: видалення ----
    let group_deletion = adw::PreferencesGroup::builder().title("Видалення").build();

    let delete_action_model = gtk4::StringList::new(&["Переміщувати в кошик", "Видаляти назавжди"]);
    let row_delete_action = adw::ComboRow::builder()
        .title("Дія під час видалення")
        .model(&delete_action_model)
        .selected(delete_action_index(settings.delete_action()))
        .build();
    {
        let settings = settings.clone();
        row_delete_action.connect_selected_notify(move |row| {
            settings.set_delete_action(delete_action_from_index(row.selected()));
        });
    }
    group_deletion.add(&row_delete_action);

    let row_confirm_delete = adw::SwitchRow::builder()
        .title("Показувати вікно підтвердження видалення")
        .build();
    settings
        .inner()
        .bind("confirm-delete", &row_confirm_delete, "active")
        .build();
    group_deletion.add(&row_confirm_delete);

    page.add(&group_deletion);

    page
}

fn delete_action_index(action: DeleteAction) -> u32 {
    match action {
        DeleteAction::Trash => 0,
        DeleteAction::Permanent => 1,
    }
}

fn delete_action_from_index(i: u32) -> DeleteAction {
    match i {
        1 => DeleteAction::Permanent,
        _ => DeleteAction::Trash,
    }
}

fn preload_mode_index(mode: PreloadMode) -> u32 {
    match mode {
        PreloadMode::Auto => 0,
        PreloadMode::Count(2) => 1,
        PreloadMode::Count(5) => 2,
        PreloadMode::Count(10) => 3,
        PreloadMode::Count(20) => 4,
        // Значення, якого немає серед пунктів комбобоксу (напр. хтось
        // вручну прописав інше число через dconf-editor) — трактуємо як
        // "Автоматично", щоб рядок налаштувань не показував порожній вибір.
        PreloadMode::Count(_) => 0,
    }
}

fn preload_mode_from_index(i: u32) -> PreloadMode {
    match i {
        1 => PreloadMode::Count(2),
        2 => PreloadMode::Count(5),
        3 => PreloadMode::Count(10),
        4 => PreloadMode::Count(20),
        _ => PreloadMode::Auto,
    }
}
