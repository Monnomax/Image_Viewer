// preferences/navigation.rs — розділ "Навігація": колесо миші, навігаційна
// зона, правила відображення інтерфейсних елементів.

use crate::preferences::auto_hide::VisibilityMode;
use crate::preferences::settings::Settings;
use adw::prelude::*;

pub fn build_page(settings: &Settings) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("Навігація")
        .icon_name("input-mouse-symbolic")
        .build();

    // ---- Група: колесо миші ----
    let group_wheel = adw::PreferencesGroup::builder().title("Колесо миші").build();

    let row_speed = adw::SpinRow::new(
        Some(&gtk4::Adjustment::new(1.10, 1.01, 2.0, 0.01, 0.05, 0.0)),
        0.01,
        2,
    );
    row_speed.set_title("Швидкість масштабування колесом");
    settings
        .inner()
        .bind("zoom-speed", &row_speed, "value")
        .build();
    group_wheel.add(&row_speed);

    let row_invert = adw::SwitchRow::builder().title("Інвертувати колесо").build();
    settings
        .inner()
        .bind("invert-scroll", &row_invert, "active")
        .build();
    group_wheel.add(&row_invert);

    page.add(&group_wheel);

    // ---- Група: екранні зони ----
    let group_zones = adw::PreferencesGroup::builder()
        .title("Екранні зони")
        .build();

    let row_nav_zone = adw::SpinRow::new(
        Some(&gtk4::Adjustment::new(5.0, 0.0, 200.0, 1.0, 10.0, 0.0)),
        1.0,
        0,
    );
    row_nav_zone.set_title("Ширина навігаційної зони біля країв екрана");
    settings
        .inner()
        .bind("nav-zone-width", &row_nav_zone, "value")
        .build();
    group_zones.add(&row_nav_zone);

    page.add(&group_zones);

    // ---- Група: елементи інтерфейсу ----
    let group_interface = adw::PreferencesGroup::builder()
        .title("Елементи інтерфейсу")
        .build();

    let expander = adw::ExpanderRow::builder()
        .title("Правила відображення елементів інтерфейсу")
        .build();

    let row_cursor = adw::ComboRow::builder()
        .title("Курсор")
        .build();
    let cursor_model = gtk4::StringList::new(&["Автоматично приховувати", "Завжди показувати"]);
    row_cursor.set_model(Some(&cursor_model));
    row_cursor.set_selected(VisibilityMode::from_setting(&settings.cursor_visibility_mode()).as_cursor_index());
    let settings_for_cursor = settings.clone();
    row_cursor.connect_selected_notify(move |row| {
        let mode = VisibilityMode::from_cursor_index(row.selected());
        settings_for_cursor.set_cursor_visibility_mode(mode);
    });
    expander.add_row(&row_cursor);

    let row_close_button = adw::ComboRow::builder()
        .title("Кнопка закриття")
        .build();
    let close_model = gtk4::StringList::new(&[
        "Автоматично приховувати",
        "Завжди показувати",
        "Ніколи не показувати",
    ]);
    row_close_button.set_model(Some(&close_model));
    row_close_button.set_selected(VisibilityMode::from_setting(&settings.close_button_visibility_mode()).as_index());
    let settings_for_close = settings.clone();
    row_close_button.connect_selected_notify(move |row| {
        let mode = VisibilityMode::from_index(row.selected());
        settings_for_close.set_close_button_visibility_mode(mode);
    });
    expander.add_row(&row_close_button);

    let row_nav_buttons = adw::ComboRow::builder()
        .title("Кнопки переходів")
        .build();
    let nav_model = gtk4::StringList::new(&[
        "Автоматично приховувати",
        "Завжди показувати",
        "Ніколи не показувати",
    ]);
    row_nav_buttons.set_model(Some(&nav_model));
    row_nav_buttons.set_selected(VisibilityMode::from_setting(&settings.nav_buttons_visibility_mode()).as_index());
    let settings_for_nav = settings.clone();
    row_nav_buttons.connect_selected_notify(move |row| {
        let mode = VisibilityMode::from_index(row.selected());
        settings_for_nav.set_nav_buttons_visibility_mode(mode);
    });
    expander.add_row(&row_nav_buttons);

    let row_filename_label = adw::ComboRow::builder()
        .title("Назва файлу")
        .build();
    let filename_model = gtk4::StringList::new(&[
        "Автоматично приховувати",
        "Завжди показувати",
        "Ніколи не показувати",
    ]);
    row_filename_label.set_model(Some(&filename_model));
    row_filename_label.set_selected(VisibilityMode::from_setting(&settings.filename_label_visibility_mode()).as_index());
    let settings_for_filename = settings.clone();
    row_filename_label.connect_selected_notify(move |row| {
        let mode = VisibilityMode::from_index(row.selected());
        settings_for_filename.set_filename_label_visibility_mode(mode);
    });
    expander.add_row(&row_filename_label);

    let row_thumbnail_strip = adw::ComboRow::builder()
        .title("Стрічка мініатюр")
        .build();
    let thumbnail_model = gtk4::StringList::new(&[
        "Автоматично приховувати",
        "Завжди показувати",
        "Ніколи не показувати",
    ]);
    row_thumbnail_strip.set_model(Some(&thumbnail_model));
    row_thumbnail_strip.set_selected(VisibilityMode::from_setting(&settings.thumbnail_strip_visibility_mode()).as_index());
    let settings_for_thumbnail = settings.clone();
    row_thumbnail_strip.connect_selected_notify(move |row| {
        let mode = VisibilityMode::from_index(row.selected());
        settings_for_thumbnail.set_thumbnail_strip_visibility_mode(mode);
    });
    expander.add_row(&row_thumbnail_strip);

    // Затримка перед появою елементів інтерфейсу (мс)
    let show_delay = adw::SpinRow::new(
        Some(&gtk4::Adjustment::new(0.0, 0.0, 1000.0, 10.0, 100.0, 0.0)),
        10.0,
        0,
    );
    show_delay.set_title("Затримка перед появою");
    settings
        .inner()
        .bind("show-delay-ms", &show_delay, "value")
        .build();
    expander.add_row(&show_delay);

    // SpinRow додається напряму, без обгортки в ActionRow
    let hide_delay = adw::SpinRow::new(
        Some(&gtk4::Adjustment::new(0.0, 0.0, 5000.0, 50.0, 250.0, 0.0)),
        50.0,
        0,
    );
    hide_delay.set_title("Затримка перед приховуванням");
    settings
        .inner()
        .bind("hide-cursor-timeout-ms", &hide_delay, "value")
        .build();
    expander.add_row(&hide_delay);

    let animation_duration = adw::SpinRow::new(
        Some(&gtk4::Adjustment::new(200.0, 0.0, 1000.0, 50.0, 50.0, 0.0)),
        50.0,
        0,
    );
    animation_duration.set_title("Тривалість анімації");
    settings
        .inner()
        .bind("animation-duration-ms", &animation_duration, "value")
        .build();
    expander.add_row(&animation_duration);

    let row_sensitivity_zones = adw::SwitchRow::builder()
        .title("Увімкнути зони чутливості")
        .build();
    settings
        .inner()
        .bind("sensitivity-zones-enabled", &row_sensitivity_zones, "active")
        .build();
    expander.add_row(&row_sensitivity_zones);

    group_interface.add(&expander);
    page.add(&group_interface);

    page
}
