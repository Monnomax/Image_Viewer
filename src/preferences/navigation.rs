// preferences/navigation.rs — розділ "Навігація": колесо миші, навігаційна
// зона, правила відображення інтерфейсних елементів.

use crate::preferences::auto_hide::VisibilityMode;
use crate::preferences::regulator::button_regulator_row;
use crate::preferences::settings::Settings;
use adw::prelude::*;

pub fn build_page(settings: &Settings) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("Навігація")
        .icon_name("input-mouse-symbolic")
        .build();

    // ---- Група: колесо миші ----
    let group_wheel = adw::PreferencesGroup::builder().title("Колесо миші").build();

    let zoom_speed = gtk4::Adjustment::new(1.10, 1.01, 2.0, 0.01, 0.05, 0.0);
    settings
        .inner()
        .bind("zoom-speed", &zoom_speed, "value")
        .build();
    let row_speed = button_regulator_row("Швидкість масштабування колесом", &zoom_speed, 0.01, 2);
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

    let nav_zone_width = gtk4::Adjustment::new(5.0, 0.0, 200.0, 1.0, 10.0, 0.0);
    settings
        .inner()
        .bind("nav-zone-width", &nav_zone_width, "value")
        .build();
    let row_nav_zone = button_regulator_row(
        "Ширина навігаційної зони біля країв екрана",
        &nav_zone_width,
        1.0,
        0,
    );
    group_zones.add(&row_nav_zone);

    page.add(&group_zones);

    // ---- Група: елементи інтерфейсу ----
    let group_interface = adw::PreferencesGroup::builder()
        .title("Елементи інтерфейсу")
        .build();

    let expander = adw::ExpanderRow::builder()
        .title("Правила відображення елементів інтерфейсу")
        .build();

    let row_cursor = adw::ActionRow::builder().title("Курсор").build();
    let cursor_model = gtk4::StringList::new(&["Автоматично приховувати", "Завжди показувати"]);
    let cursor_dropdown = gtk4::DropDown::builder()
        .model(&cursor_model)
        .selected(
            VisibilityMode::from_setting(&settings.cursor_visibility_mode()).as_cursor_index(),
        )
        .valign(gtk4::Align::Center)
        .build();
    row_cursor.add_suffix(&cursor_dropdown);
    let settings_for_cursor = settings.clone();
    cursor_dropdown.connect_selected_notify(move |dropdown| {
        let mode = VisibilityMode::from_cursor_index(dropdown.selected());
        settings_for_cursor.set_cursor_visibility_mode(mode);
    });
    expander.add_row(&row_cursor);

    let row_close_button = adw::ActionRow::builder().title("Кнопка закриття").build();
    let close_model = gtk4::StringList::new(&[
        "Автоматично приховувати",
        "Завжди показувати",
        "Ніколи не показувати",
    ]);
    let close_dropdown = gtk4::DropDown::builder()
        .model(&close_model)
        .selected(
            VisibilityMode::from_setting(&settings.close_button_visibility_mode()).as_index(),
        )
        .valign(gtk4::Align::Center)
        .build();
    row_close_button.add_suffix(&close_dropdown);
    let settings_for_close = settings.clone();
    close_dropdown.connect_selected_notify(move |dropdown| {
        let mode = VisibilityMode::from_index(dropdown.selected());
        settings_for_close.set_close_button_visibility_mode(mode);
    });
    expander.add_row(&row_close_button);

    let row_nav_buttons = adw::ActionRow::builder().title("Кнопки переходів").build();
    let nav_model = gtk4::StringList::new(&[
        "Автоматично приховувати",
        "Завжди показувати",
        "Ніколи не показувати",
    ]);
    let nav_dropdown = gtk4::DropDown::builder()
        .model(&nav_model)
        .selected(VisibilityMode::from_setting(&settings.nav_buttons_visibility_mode()).as_index())
        .valign(gtk4::Align::Center)
        .build();
    row_nav_buttons.add_suffix(&nav_dropdown);
    let settings_for_nav = settings.clone();
    nav_dropdown.connect_selected_notify(move |dropdown| {
        let mode = VisibilityMode::from_index(dropdown.selected());
        settings_for_nav.set_nav_buttons_visibility_mode(mode);
    });
    expander.add_row(&row_nav_buttons);

    let row_filename_label = adw::ActionRow::builder().title("Назва файлу").build();
    let filename_model = gtk4::StringList::new(&[
        "Автоматично приховувати",
        "Завжди показувати",
        "Ніколи не показувати",
    ]);
    let filename_dropdown = gtk4::DropDown::builder()
        .model(&filename_model)
        .selected(
            VisibilityMode::from_setting(&settings.filename_label_visibility_mode()).as_index(),
        )
        .valign(gtk4::Align::Center)
        .build();
    row_filename_label.add_suffix(&filename_dropdown);
    let settings_for_filename = settings.clone();
    filename_dropdown.connect_selected_notify(move |dropdown| {
        let mode = VisibilityMode::from_index(dropdown.selected());
        settings_for_filename.set_filename_label_visibility_mode(mode);
    });
    expander.add_row(&row_filename_label);

    let row_thumbnail_strip = adw::ActionRow::builder().title("Стрічка мініатюр").build();
    let thumbnail_model = gtk4::StringList::new(&[
        "Автоматично приховувати",
        "Завжди показувати",
        "Ніколи не показувати",
    ]);
    let thumbnail_dropdown = gtk4::DropDown::builder()
        .model(&thumbnail_model)
        .selected(
            VisibilityMode::from_setting(&settings.thumbnail_strip_visibility_mode()).as_index(),
        )
        .valign(gtk4::Align::Center)
        .build();
    row_thumbnail_strip.add_suffix(&thumbnail_dropdown);
    let settings_for_thumbnail = settings.clone();
    thumbnail_dropdown.connect_selected_notify(move |dropdown| {
        let mode = VisibilityMode::from_index(dropdown.selected());
        settings_for_thumbnail.set_thumbnail_strip_visibility_mode(mode);
    });
    expander.add_row(&row_thumbnail_strip);

    // Затримка перед появою елементів інтерфейсу (мс)
    let show_delay_adjustment = gtk4::Adjustment::new(0.0, 0.0, 1000.0, 10.0, 100.0, 0.0);
    settings
        .inner()
        .bind("show-delay-ms", &show_delay_adjustment, "value")
        .build();
    let show_delay = button_regulator_row("Затримка перед появою", &show_delay_adjustment, 10.0, 0);
    expander.add_row(&show_delay);

    let hide_delay_adjustment = gtk4::Adjustment::new(0.0, 0.0, 5000.0, 50.0, 250.0, 0.0);
    settings
        .inner()
        .bind("hide-cursor-timeout-ms", &hide_delay_adjustment, "value")
        .build();
    let hide_delay = button_regulator_row(
        "Затримка перед приховуванням",
        &hide_delay_adjustment,
        50.0,
        0,
    );
    expander.add_row(&hide_delay);

    let animation_duration_adjustment = gtk4::Adjustment::new(200.0, 0.0, 1000.0, 50.0, 50.0, 0.0);
    settings
        .inner()
        .bind(
            "animation-duration-ms",
            &animation_duration_adjustment,
            "value",
        )
        .build();
    let animation_duration = button_regulator_row(
        "Тривалість анімації",
        &animation_duration_adjustment,
        50.0,
        0,
    );
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
