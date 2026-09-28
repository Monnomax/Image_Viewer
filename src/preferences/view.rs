// preferences/view.rs — розділ "Перегляд": тло, межі масштабування.

use crate::preferences::settings::{parse_hex_color, rgba_to_hex, rgba_to_hex_alpha, Settings};
use adw::prelude::*;
use gtk4::gdk;
use std::cell::Cell;
use std::rc::Rc;

pub fn build_page(settings: &Settings) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("Перегляд")
        .icon_name("image-x-generic-symbolic")
        .build();

    // ---- Група: тло ----
    let group_bg = adw::PreferencesGroup::builder().title("Тло").build();

    let row_background_color = color_row(settings, "background-color", "Колір тла");
    let row_day_night = day_night_row(settings);
    let row_wallpaper = wallpaper_row(settings, &row_day_night);

    group_bg.add(&row_background_color);
    group_bg.add(&row_day_night);
    group_bg.add(&row_wallpaper);

    // "Колір тла" неактивний, якщо увімкнено хоч "День / Ніч", хоч
    // "Шпалеру робочого столу" — два незалежних .bind() на ту саму
    // властивість "sensitive" конфліктували б (кожен перезаписував би її
    // лише на основі свого ключа, ігноруючи стан другого), тому тут
    // свідомо перераховуємо комбіновану умову вручну при зміні будь-якого
    // з двох ключів.
    {
        let row_background_color = row_background_color.clone();
        let settings_for_update = settings.clone();
        let update = move || {
            let overridden =
                settings_for_update.day_night_enabled() || settings_for_update.use_desktop_wallpaper();
            row_background_color.set_sensitive(!overridden);
        };
        update();

        let update_a = update.clone();
        settings
            .inner()
            .connect_changed(Some("day-night-enabled"), move |_, _| update_a());

        let update_b = update.clone();
        settings
            .inner()
            .connect_changed(Some("use-desktop-wallpaper"), move |_, _| update_b());
    }

    page.add(&group_bg);

    // ---- Група: зображення ----
    let group_image = adw::PreferencesGroup::builder().title("Зображення").build();
    group_image.add(&padding_row(settings));
    group_image.add(&corner_radius_row(settings));
    page.add(&group_image);

    // ---- Група: тінь ----
    let group_shadow = adw::PreferencesGroup::builder().title("Тінь").build();
    group_shadow.add(&shadow_row(settings));
    page.add(&group_shadow);

    // ---- Група: масштаб ----
    let group_zoom = adw::PreferencesGroup::builder().title("Масштаб").build();

    let row_min = adw::SpinRow::new(
        Some(&gtk4::Adjustment::new(0.02, 0.01, 1.0, 0.01, 0.05, 0.0)),
        0.01,
        2,
    );
    row_min.set_title("Мінімальний масштаб");
    settings
        .inner()
        .bind("min-zoom", &row_min, "value")
        .build();
    group_zoom.add(&row_min);

    let row_max = adw::SpinRow::new(
        Some(&gtk4::Adjustment::new(40.0, 1.0, 100.0, 1.0, 5.0, 0.0)),
        1.0,
        1,
    );
    row_max.set_title("Максимальний масштаб");
    settings
        .inner()
        .bind("max-zoom", &row_max, "value")
        .build();
    group_zoom.add(&row_max);

    page.add(&group_zoom);

    page
}

/// GtkColorDialogButton, прив'язана до рядкового ключа GSettings у форматі
/// "#RRGGBB" (`with_alpha = false`) або "#RRGGBBAA" (`with_alpha = true`,
/// напр. для тіні, де прозорість — частина самого налаштування) — сама
/// кнопка, без обгортки в ActionRow (щоб можна було компонувати кілька
/// таких кнопок з іконками в одному рядку, як у day_night_row нижче).
///
/// GSettings не має вбудованого типу кольору, а `Settings::bind_with_mapping`
/// відсутній у поточній версії крейта gio-rs, тому міст string <-> gdk::RGBA
/// зроблено вручну двома сигналами з прапорцем `updating` проти
/// зациклення (settings -> button, button -> settings):
fn color_button(settings: &Settings, key: &'static str, with_alpha: bool) -> gtk4::ColorDialogButton {
    let dialog = gtk4::ColorDialog::builder().with_alpha(with_alpha).build();
    let button = gtk4::ColorDialogButton::new(Some(dialog));
    button.set_valign(gtk4::Align::Center);
    button.set_rgba(&parse_hex_color(&settings.inner().string(key)).unwrap_or(gdk::RGBA::BLACK));

    let updating = Rc::new(Cell::new(false));

    // GSettings -> кнопка (наприклад, значення змінили ззовні через dconf-editor)
    {
        let button = button.clone();
        let updating = updating.clone();
        settings
            .inner()
            .connect_changed(Some(key), move |s, changed_key| {
                if updating.get() {
                    return;
                }
                if let Some(rgba) = parse_hex_color(&s.string(changed_key)) {
                    updating.set(true);
                    button.set_rgba(&rgba);
                    updating.set(false);
                }
            });
    }

    // Кнопка -> GSettings
    {
        let settings = settings.clone();
        let updating = updating.clone();
        button.connect_notify_local(Some("rgba"), move |b, _pspec| {
            if updating.get() {
                return;
            }
            updating.set(true);
            let hex = if with_alpha {
                rgba_to_hex_alpha(&b.rgba())
            } else {
                rgba_to_hex(&b.rgba())
            };
            let _ = settings.inner().set_string(key, &hex);
            updating.set(false);
        });
    }

    button
}

/// ActionRow з однією GtkColorDialogButton як суфіксом (без альфа-каналу).
fn color_row(settings: &Settings, key: &'static str, title: &str) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title(title).build();
    let button = color_button(settings, key, false);
    row.add_suffix(&button);
    row.set_activatable_widget(Some(&button));
    row
}

/// ActionRow "День / Ніч": дві пари іконка+colorpicker (день/ніч) і
/// перемикач автоматичного режиму в кінці. Коли перемикач увімкнено,
/// `Settings::background_color()` сам підставляє day-color/night-color
/// залежно від системної схеми кольорів. Чутливість самого рядка "Колір
/// тла" від цього ключа рахується централізовано в build_page() — разом
/// із залежністю від "Використовувати шпалеру робочого столу".
fn day_night_row(settings: &Settings) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title("День / Ніч").build();

    row.add_suffix(&gtk4::Image::from_icon_name("weather-clear-symbolic"));
    row.add_suffix(&color_button(settings, "day-color", false));

    row.add_suffix(&gtk4::Image::from_icon_name("weather-clear-night-symbolic"));
    row.add_suffix(&color_button(settings, "night-color", false));

    let switch = gtk4::Switch::new();
    switch.set_valign(gtk4::Align::Center);
    settings
        .inner()
        .bind("day-night-enabled", &switch, "active")
        .build();
    row.add_suffix(&switch);

    row
}

/// ExpanderRow "Використовувати шпалеру робочого столу": перемикач у
/// кінці головного рядка вмикає режим, дві вкладені сторінки-рядки
/// ("Корекція яскравості", "Розмиття") — самі повзунки. Коли перемикач
/// увімкнено, рядок "День / Ніч" (переданий як `day_night_row`) стає
/// неактивним — тут залежність лише від ОДНОГО ключа, тож звичайний
/// `.bind(...).invert_boolean()` цілком коректний (на відміну від "Колір
/// тла", де залежність від ДВОХ ключів одразу — див. build_page()).
fn wallpaper_row(settings: &Settings, day_night_row: &adw::ActionRow) -> adw::ExpanderRow {
    let row = adw::ExpanderRow::builder()
        .title("Використовувати шпалеру робочого столу")
        .build();

    let switch = gtk4::Switch::new();
    switch.set_valign(gtk4::Align::Center);
    settings
        .inner()
        .bind("use-desktop-wallpaper", &switch, "active")
        .build();
    row.add_suffix(&switch);

    row.add_row(&brightness_row(settings));
    row.add_row(&blur_row(settings));
    row.add_row(&saturation_row(settings));
    row.add_row(&grain_row(settings));

    settings
        .inner()
        .bind("use-desktop-wallpaper", day_night_row, "sensitive")
        .invert_boolean()
        .build();

    row
}

/// ActionRow з повзунком "Корекція яскравості": -100% (темніше) ..
/// 0 (без змін) .. +100% (світліше). Насправді це не гамма-корекція, а
/// дешевий напівпрозорий чорний/білий шар поверх шпалери в renderer.rs —
/// самого повзунка це не стосується, лише зберігає значення в GSettings.
fn brightness_row(settings: &Settings) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title("Корекція яскравості").build();

    let adjustment = gtk4::Adjustment::new(0.0, -100.0, 100.0, 1.0, 5.0, 0.0);
    let scale = gtk4::Scale::new(gtk4::Orientation::Horizontal, Some(&adjustment));
    scale.set_valign(gtk4::Align::Center);
    scale.set_size_request(160, -1);
    scale.set_draw_value(true);
    scale.set_digits(0);
    scale.set_value_pos(gtk4::PositionType::Right);
    scale.add_mark(0.0, gtk4::PositionType::Bottom, None);

    settings
        .inner()
        .bind("wallpaper-brightness", &adjustment, "value")
        .build();

    row.add_suffix(&scale);
    row
}

/// ActionRow з повзунком "Розмиття": 0 (без розмиття) .. 100 (максимум).
fn blur_row(settings: &Settings) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title("Розмиття").build();

    let adjustment = gtk4::Adjustment::new(0.0, 0.0, 100.0, 1.0, 5.0, 0.0);
    let scale = gtk4::Scale::new(gtk4::Orientation::Horizontal, Some(&adjustment));
    scale.set_valign(gtk4::Align::Center);
    scale.set_size_request(160, -1);
    scale.set_draw_value(true);
    scale.set_digits(0);
    scale.set_value_pos(gtk4::PositionType::Right);

    settings
        .inner()
        .bind("wallpaper-blur", &adjustment, "value")
        .build();

    row.add_suffix(&scale);
    row
}

/// ActionRow з повзунком "Насиченість": 0.0 (чорно-біла шпалера) ..
/// 1.0 (без змін, типово) .. 2.0 (подвоєна насиченість кольору).
fn saturation_row(settings: &Settings) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title("Насиченість").build();

    let adjustment = gtk4::Adjustment::new(1.0, 0.0, 2.0, 0.1, 0.1, 0.0);
    let scale = gtk4::Scale::new(gtk4::Orientation::Horizontal, Some(&adjustment));
    scale.set_valign(gtk4::Align::Center);
    scale.set_size_request(160, -1);
    scale.set_draw_value(true);
    scale.set_digits(1);
    scale.set_value_pos(gtk4::PositionType::Right);
    scale.add_mark(1.0, gtk4::PositionType::Bottom, None);

    settings
        .inner()
        .bind("wallpaper-saturation", &adjustment, "value")
        .build();

    row.add_suffix(&scale);
    row
}

/// ActionRow з повзунком "Зернистість": 0 (без зерна) .. 100 (максимум).
fn grain_row(settings: &Settings) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title("Зернистість").build();

    let adjustment = gtk4::Adjustment::new(0.0, 0.0, 100.0, 5.0, 5.0, 0.0);
    let scale = gtk4::Scale::new(gtk4::Orientation::Horizontal, Some(&adjustment));
    scale.set_valign(gtk4::Align::Center);
    scale.set_size_request(160, -1);
    scale.set_draw_value(true);
    scale.set_digits(0);
    scale.set_value_pos(gtk4::PositionType::Right);

    // bind на adjustment, а не на scale: "value" — властивість
    // GtkAdjustment; ключ схеми "u" (uint), а adjustment.value — double,
    // GSettings::bind() коректно конвертує між ними (той самий прийом,
    // що й для "image-corner-radius" у corner_radius_row() вище).
    settings
        .inner()
        .bind("wallpaper-grain", &adjustment, "value")
        .build();

    row.add_suffix(&scale);
    row
}

/// ExpanderRow "Показувати тінь": перемикач у кінці головного рядка,
/// п'ять вкладених рядків — чотири однакові за формою повзунки (0..100px,
/// крок 1px) і рядок з colorpicker'ом кольору тіні (з альфа-каналом).
/// Саме малювання тіні під зображенням — у renderer.rs; тут лише
/// зберігаються значення в GSettings.
fn shadow_row(settings: &Settings) -> adw::ExpanderRow {
    let row = adw::ExpanderRow::builder().title("Показувати тінь").build();

    let switch = gtk4::Switch::new();
    switch.set_valign(gtk4::Align::Center);
    settings
        .inner()
        .bind("shadow-enabled", &switch, "active")
        .build();
    row.add_suffix(&switch);

    row.add_row(&slider_row(settings, "Зміщення по горизонталі", "shadow-offset-x", -100.0, 100.0));
    row.add_row(&slider_row(settings, "Зміщення по вертикалі", "shadow-offset-y", -100.0, 100.0));
    row.add_row(&slider_row(settings, "Радіус розмиття", "shadow-blur-radius", 0.0, 100.0));
    row.add_row(&slider_row(settings, "Радіус розтягування", "shadow-spread-radius", 0.0, 100.0));

    let row_color = adw::ActionRow::builder().title("Колір").build();
    row_color.add_suffix(&color_button(settings, "shadow-color", true));
    row.add_row(&row_color);

    row.add_row(&slider_row(settings, "Прозорість", "shadow-opacity", 0.0, 100.0));

    row
}

/// ActionRow з повзунком (крок 1px), прив'язаним до ключа `key`, у межах
/// `min..max`. Спільна форма для рядків тіні (зміщення X/Y, розмиття,
/// розтягування, прозорість) — самі лише назва/ключ/діапазон різняться,
/// початкове значення повзунка (перш ніж bind() перепише його реальним з
/// GSettings) тут не важливе.
fn slider_row(settings: &Settings, title: &str, key: &'static str, min: f64, max: f64) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title(title).build();

    let adjustment = gtk4::Adjustment::new(min, min, max, 1.0, 5.0, 0.0);
    let scale = gtk4::Scale::new(gtk4::Orientation::Horizontal, Some(&adjustment));
    scale.set_valign(gtk4::Align::Center);
    scale.set_size_request(160, -1);
    scale.set_draw_value(true);
    scale.set_digits(0);
    scale.set_value_pos(gtk4::PositionType::Right);
    if min < 0.0 && max > 0.0 {
        scale.add_mark(0.0, gtk4::PositionType::Bottom, None);
    }

    settings.inner().bind(key, &adjustment, "value").build();

    row.add_suffix(&scale);
    row
}

/// ActionRow з повзунком (GtkScale), прив'язаним до ключа "image-padding" —
/// відступ зображення від країв вікна при вписуванні в екран.
fn padding_row(settings: &Settings) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title("Відступ").build();

    let adjustment = gtk4::Adjustment::new(50.0, 0.0, 100.0, 5.0, 5.0, 0.0);
    let scale = gtk4::Scale::new(gtk4::Orientation::Horizontal, Some(&adjustment));
    scale.set_valign(gtk4::Align::Center);
    scale.set_size_request(160, -1);
    scale.set_draw_value(true);
    scale.set_digits(0);
    scale.set_value_pos(gtk4::PositionType::Right);

    settings
        .inner()
        .bind("image-padding", &adjustment, "value")   // ← адресат: adjustment, не scale
        .build();

    row.add_suffix(&scale);
    row
}

/// ActionRow з повзунком, прив'язаним до ключа "image-corner-radius" —
/// радіус заокруглення кутів зображення при відображенні.
fn corner_radius_row(settings: &Settings) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title("Заокруглення кутів").build();

    let adjustment = gtk4::Adjustment::new(0.0, 0.0, 20.0, 1.0, 1.0, 0.0);
    let scale = gtk4::Scale::new(gtk4::Orientation::Horizontal, Some(&adjustment));
    scale.set_valign(gtk4::Align::Center);
    scale.set_size_request(160, -1);
    scale.set_draw_value(true);
    scale.set_digits(0);
    scale.set_value_pos(gtk4::PositionType::Right);

    // bind на adjustment, а не на scale: у GtkScale/GtkRange властивості
    // "value" немає — вона живе на GtkAdjustment (див. попередній баг
    // з "Відступом").
    settings
        .inner()
        .bind("image-corner-radius", &adjustment, "value")
        .build();

    row.add_suffix(&scale);
    row
}
