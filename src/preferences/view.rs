// preferences/view.rs — розділ "Перегляд": тло, межі масштабування.

use crate::preferences::settings::{
    parse_hex_color, rgba_to_hex, rgba_to_hex_alpha, BackgroundType, Settings,
};
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

    add_background_preview_styles();
    let row_background_type = background_type_row(settings);
    let row_custom_colors = background_preview_row(settings, false);
    let row_custom_wallpapers = background_preview_row(settings, true);
    let preview_stack = gtk4::Stack::new();
    preview_stack.add_named(&row_custom_colors, Some("custom-colors"));
    preview_stack.add_named(&row_custom_wallpapers, Some("custom-wallpapers"));
    preview_stack.set_margin_top(28);
    preview_stack.set_margin_bottom(28);
    preview_stack.set_margin_start(28);
    preview_stack.set_margin_end(28);

    let preview_row = adw::PreferencesRow::builder().child(&preview_stack).build();
    let effects = wallpaper_row(settings);

    group_bg.add(&row_background_type);
    group_bg.add(&preview_row);
    group_bg.add(&effects);
    update_background_rows(settings, &preview_row, &preview_stack, &effects);
    {
        let preview_row = preview_row.clone();
        let preview_stack = preview_stack.clone();
        let effects = effects.clone();
        let settings_for_update = settings.clone();
        settings
            .inner()
            .connect_changed(Some("background-type"), move |_, _| {
                update_background_rows(
                    &settings_for_update,
                    &preview_row,
                    &preview_stack,
                    &effects,
                );
            });
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
    settings.inner().bind("min-zoom", &row_min, "value").build();
    group_zoom.add(&row_min);

    let row_max = adw::SpinRow::new(
        Some(&gtk4::Adjustment::new(40.0, 1.0, 100.0, 1.0, 5.0, 0.0)),
        1.0,
        1,
    );
    row_max.set_title("Максимальний масштаб");
    settings.inner().bind("max-zoom", &row_max, "value").build();
    group_zoom.add(&row_max);

    page.add(&group_zoom);

    page
}

/// GtkColorDialogButton, прив'язана до рядкового ключа GSettings у форматі
/// "#RRGGBB" (`with_alpha = false`) або "#RRGGBBAA" (`with_alpha = true`,
/// напр. для тіні, де прозорість — частина самого налаштування) — сама
/// кнопка, без обгортки в ActionRow (щоб можна було компонувати кілька
/// таких кнопок у спільному рядку попереднього перегляду кольорів).
///
/// GSettings не має вбудованого типу кольору, а `Settings::bind_with_mapping`
/// відсутній у поточній версії крейта gio-rs, тому міст string <-> gdk::RGBA
/// зроблено вручну двома сигналами з прапорцем `updating` проти
/// зациклення (settings -> button, button -> settings):
fn color_button(
    settings: &Settings,
    key: &'static str,
    with_alpha: bool,
) -> gtk4::ColorDialogButton {
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

fn background_type_row(settings: &Settings) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title("Тип тла").build();
    let dropdown = gtk4::DropDown::from_strings(&[
        "Системні кольори",
        "Користувацькі кольори",
        "Системні шпалери",
        "Користувацькі шпалери",
    ]);
    dropdown.set_valign(gtk4::Align::Center);
    dropdown.set_selected(background_type_index(settings.background_type()));
    row.add_suffix(&dropdown);
    row.set_activatable_widget(Some(&dropdown));

    let updating = Rc::new(Cell::new(false));
    {
        let settings = settings.clone();
        let updating = updating.clone();
        dropdown.connect_notify_local(Some("selected"), move |dropdown, _| {
            if !updating.get() {
                if let Some(background_type) = background_type_at(dropdown.selected()) {
                    settings.set_background_type(background_type);
                }
            }
        });
    }
    {
        let dropdown = dropdown.clone();
        let updating = updating.clone();
        settings
            .inner()
            .connect_changed(Some("background-type"), move |s, _| {
                updating.set(true);
                dropdown.set_selected(background_type_index(BackgroundType::from_str(
                    &s.string("background-type"),
                )));
                updating.set(false);
            });
    }
    row
}

fn background_type_index(background_type: BackgroundType) -> u32 {
    match background_type {
        BackgroundType::SystemColors => 0,
        BackgroundType::CustomColors => 1,
        BackgroundType::SystemWallpapers => 2,
        BackgroundType::CustomWallpapers => 3,
    }
}

fn background_type_at(index: u32) -> Option<BackgroundType> {
    match index {
        0 => Some(BackgroundType::SystemColors),
        1 => Some(BackgroundType::CustomColors),
        2 => Some(BackgroundType::SystemWallpapers),
        3 => Some(BackgroundType::CustomWallpapers),
        _ => None,
    }
}

fn update_background_rows(
    settings: &Settings,
    preview_row: &adw::PreferencesRow,
    preview_stack: &gtk4::Stack,
    effects: &adw::ExpanderRow,
) {
    let background_type = settings.background_type();
    let uses_wallpaper = background_type.uses_wallpaper();

    match background_type {
        BackgroundType::CustomColors => {
            preview_stack.set_visible_child_name("custom-colors");
            preview_row.set_visible(true);
        }
        BackgroundType::CustomWallpapers => {
            preview_stack.set_visible_child_name("custom-wallpapers");
            preview_row.set_visible(true);
        }
        BackgroundType::SystemColors | BackgroundType::SystemWallpapers => {
            preview_row.set_visible(false);
        }
    }
    effects.set_visible(uses_wallpaper);
}

fn background_preview_row(settings: &Settings, wallpapers: bool) -> gtk4::Box {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 30);
    row.set_halign(gtk4::Align::Fill);
    row.set_valign(gtk4::Align::Center);
    row.set_hexpand(true);

    let ratio = monitor_aspect_ratio();

    for (night, label) in [(false, "День"), (true, "Ніч")] {
        let preview = if wallpapers {
            wallpaper_preview_button(settings, night)
        } else {
            let key = if night { "night-color" } else { "day-color" };
            let button = color_button(settings, key, false);
            button.add_css_class("background-preview");
            button.upcast::<gtk4::Widget>()
        };

        preview.set_hexpand(true);
        preview.set_vexpand(true);

        let frame = gtk4::AspectFrame::new(
            0.5,
            0.5,
            ratio,
            false,
        );
        frame.set_hexpand(true);
        frame.set_halign(gtk4::Align::Fill);
        frame.set_valign(gtk4::Align::Center);
        frame.set_child(Some(&preview));

        let title = gtk4::Label::new(Some(label));

        let preview_column =
            gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        preview_column.set_hexpand(true);
        preview_column.set_halign(gtk4::Align::Fill);
        preview_column.set_valign(gtk4::Align::Center);

        preview_column.append(&frame);
        preview_column.append(&title);

        row.append(&preview_column);
    }

    row
}

fn wallpaper_preview_button(settings: &Settings, night: bool) -> gtk4::Widget {
    let button = gtk4::Button::new();

    button.set_overflow(gtk4::Overflow::Hidden);
    button.add_css_class("background-preview");
    button.add_css_class("background-image-preview");

    let picture = gtk4::Picture::new();

    picture.set_content_fit(gtk4::ContentFit::Cover);
    picture.set_can_shrink(true);
    let setting_key = if night {
        "custom-wallpaper-night"
    } else {
        "custom-wallpaper-day"
    };
    set_preview_picture(&picture, &settings.custom_wallpaper(night));
    button.set_child(Some(&picture));

    {
        let picture = picture.clone();
        settings
            .inner()
            .connect_changed(Some(setting_key), move |s, key| {
                set_preview_picture(&picture, &s.string(key));
            });
    }
    {
        let settings = settings.clone();
        button.connect_clicked(move |button| {
            let Some(window) = button
                .root()
                .and_then(|root| root.downcast::<gtk4::Window>().ok())
            else {
                return;
            };
            let filter = gtk4::FileFilter::new();
            filter.set_name(Some("Зображення"));
            filter.add_mime_type("image/*");

            let dialog = gtk4::FileDialog::builder()
                .title("Вибрати шпалеру")
                .modal(true)
                .build();
            dialog.set_default_filter(Some(&filter));
            let settings = settings.clone();
            dialog.open(
                Some(&window),
                None::<&gtk4::gio::Cancellable>,
                move |result| match result {
                    Ok(file) => {
                        let Some(path) = file.path() else {
                            eprintln!("Не вдалося визначити шлях до вибраної шпалери");
                            return;
                        };
                        if let Err(error) = gtk4::gdk_pixbuf::Pixbuf::from_file(&path) {
                            eprintln!("Не вдалося відкрити вибрану шпалеру: {error}");
                            return;
                        }
                        settings.set_custom_wallpaper(night, &path.to_string_lossy());
                    }
                    Err(error) if !error.matches(gtk4::gio::IOErrorEnum::Cancelled) => {
                        eprintln!("Не вдалося вибрати шпалеру: {error}");
                    }
                    Err(_) => {}
                },
            );
        });
    }
    button.upcast()
}

fn monitor_aspect_ratio() -> f32 {
    let Some(display) = gtk4::gdk::Display::default() else {
        return 16.0 / 9.0;
    };

    let monitors = display.monitors();

    let Some(monitor) = monitors
        .item(0)
        .and_then(|item| item.downcast::<gtk4::gdk::Monitor>().ok())
    else {
        return 16.0 / 9.0;
    };

    let geometry = monitor.geometry();

    if geometry.height() > 0 {
        geometry.width() as f32 / geometry.height() as f32
    } else {
        16.0 / 9.0
    }
}

fn set_preview_picture(picture: &gtk4::Picture, path: &str) {
    if path.is_empty() {
        picture.set_file(None::<&gtk4::gio::File>);
    } else {
        picture.set_file(Some(&gtk4::gio::File::for_path(path)));
    }
}

fn add_background_preview_styles() {
    let Some(display) = gtk4::gdk::Display::default() else {
        return;
    };
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(
        "button.background-preview { padding: 0; border: none; outline: none; box-shadow: none; border-radius: 12px; }",
    );
    gtk4::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn wallpaper_row(settings: &Settings) -> adw::ExpanderRow {
    let expander = adw::ExpanderRow::builder().title("Ефекти тла").build();
    expander.add_row(&brightness_row(settings));
    expander.add_row(&blur_row(settings));
    expander.add_row(&saturation_row(settings));
    expander.add_row(&grain_row(settings));
    expander
}

/// ActionRow з повзунком "Корекція яскравості": -100% (темніше) ..
/// 0 (без змін) .. +100% (світліше). Насправді це не гамма-корекція, а
/// дешевий напівпрозорий чорний/білий шар поверх шпалери в renderer.rs —
/// самого повзунка це не стосується, лише зберігає значення в GSettings.
fn brightness_row(settings: &Settings) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title("Корекція яскравості")
        .build();

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

    row.add_row(&slider_row(
        settings,
        "Зміщення по горизонталі",
        "shadow-offset-x",
        -100.0,
        100.0,
    ));
    row.add_row(&slider_row(
        settings,
        "Зміщення по вертикалі",
        "shadow-offset-y",
        -100.0,
        100.0,
    ));
    row.add_row(&slider_row(
        settings,
        "Радіус розмиття",
        "shadow-blur-radius",
        0.0,
        100.0,
    ));
    row.add_row(&slider_row(
        settings,
        "Радіус розтягування",
        "shadow-spread-radius",
        0.0,
        100.0,
    ));

    let row_color = adw::ActionRow::builder().title("Колір").build();
    row_color.add_suffix(&color_button(settings, "shadow-color", true));
    row.add_row(&row_color);

    row.add_row(&slider_row(
        settings,
        "Прозорість",
        "shadow-opacity",
        0.0,
        100.0,
    ));

    row
}

/// ActionRow з повзунком (крок 1px), прив'язаним до ключа `key`, у межах
/// `min..max`. Спільна форма для рядків тіні (зміщення X/Y, розмиття,
/// розтягування, прозорість) — самі лише назва/ключ/діапазон різняться,
/// початкове значення повзунка (перш ніж bind() перепише його реальним з
/// GSettings) тут не важливе.
fn slider_row(
    settings: &Settings,
    title: &str,
    key: &'static str,
    min: f64,
    max: f64,
) -> adw::ActionRow {
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
        .bind("image-padding", &adjustment, "value") // ← адресат: adjustment, не scale
        .build();

    row.add_suffix(&scale);
    row
}

/// ActionRow з повзунком, прив'язаним до ключа "image-corner-radius" —
/// радіус заокруглення кутів зображення при відображенні.
fn corner_radius_row(settings: &Settings) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title("Заокруглення кутів")
        .build();

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
