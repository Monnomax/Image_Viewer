// preferences/view.rs — розділ "Перегляд": тло, межі масштабування.

use crate::preferences::settings::{
    parse_hex_color, rgba_to_hex, rgba_to_hex_alpha, BackgroundType, Settings,
};
use crate::preferences::regulator::{
    button_regulator_row as make_button_regulator_row, button_regulator_row_custom,
};
use adw::prelude::*;
use gtk4::gdk;
use gtk4::glib;
use gtk4::subclass::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

mod background_preview_imp {
    use super::*;

    #[derive(Default)]
    pub struct BackgroundPreview {
        pub child: RefCell<Option<gtk4::Widget>>,
        pub ratio: Cell<f32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for BackgroundPreview {
        const NAME: &'static str = "ImgViewerBackgroundPreview";
        type Type = super::BackgroundPreview;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for BackgroundPreview {
        fn dispose(&self) {
            if let Some(child) = self.child.borrow_mut().take() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for BackgroundPreview {
        fn request_mode(&self) -> gtk4::SizeRequestMode {
            gtk4::SizeRequestMode::HeightForWidth
        }

        fn measure(
            &self,
            orientation: gtk4::Orientation,
            for_size: i32,
        ) -> (i32, i32, i32, i32) {
            const NATURAL_WIDTH: i32 = 256;

            match orientation {
                gtk4::Orientation::Horizontal => {
                    // Як у GNOME CcBackgroundPreview:
                    // мінімальна ширина = 0,
                    // природна ширина = 256.
                    (0, NATURAL_WIDTH, -1, -1)
                }

                gtk4::Orientation::Vertical => {
                    let width = if for_size >= 0 {
                        for_size
                    } else {
                        NATURAL_WIDTH
                    };

                    let ratio = self.ratio.get().max(0.01);
                    let height = (width as f32 / ratio).round() as i32;

                    (height, height, -1, -1)
                }

                _ => (0, 0, -1, -1),
            }
        }

        fn size_allocate(
            &self,
            width: i32,
            height: i32,
            baseline: i32,
        ) {
            self.parent_size_allocate(width, height, baseline);

            if let Some(child) = self.child.borrow().as_ref() {
                child.allocate(width, height, baseline, None);
            }
        }
    }
}

glib::wrapper! {
    pub struct BackgroundPreview(
        ObjectSubclass<background_preview_imp::BackgroundPreview>
    ) @extends gtk4::Widget;
}

impl BackgroundPreview {
    fn new(ratio: f32) -> Self {
        let preview: Self = glib::Object::builder().build();

        preview.imp().ratio.set(ratio);
        preview.set_hexpand(true);
        preview.set_halign(gtk4::Align::Fill);

        preview
    }

    fn set_child(&self, child: Option<&gtk4::Widget>) {
        let imp = self.imp();

        if let Some(old_child) = imp.child.borrow_mut().take() {
            old_child.unparent();
        }

        if let Some(child) = child {
            child.set_parent(self);
            child.set_hexpand(true);
            child.set_vexpand(true);
            *imp.child.borrow_mut() = Some(child.clone());
        }

        self.queue_resize();
    }
}

pub fn build_page(settings: &Settings) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("Перегляд")
        .icon_name("image-x-generic-symbolic")
        .build();

    // ---- Група: тло ----
    let group_bg = adw::PreferencesGroup::builder().title("Тло").build();

    add_background_preview_styles();
    let row_background_type = background_type_row(settings);
    let (preview_grid, previews) = background_preview_row(settings);
    let preview_clamp = adw::Clamp::builder()
        .maximum_size(400)
        .tightening_threshold(300)
        .child(&preview_grid)
        .build();

    let preferences_row = adw::PreferencesRow::builder().child(&preview_clamp).build();

    preferences_row.set_hexpand(true);
    let effects = wallpaper_row(settings);

    group_bg.add(&row_background_type);
    group_bg.add(&preferences_row);
    group_bg.add(&effects);
    update_background_rows(settings, &preferences_row, &previews, &effects);
    {
        let preferences_row = preferences_row.clone();
        let previews = previews.clone();
        let effects = effects.clone();
        let settings_for_update = settings.clone();
        settings
            .inner()
            .connect_changed(Some("background-type"), move |_, _| {
                update_background_rows(&settings_for_update, &preferences_row, &previews, &effects);
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

    let min_zoom = gtk4::Adjustment::new(0.02, 0.01, 1.0, 0.01, 0.05, 0.0);
    settings
        .inner()
        .bind("min-zoom", &min_zoom, "value")
        .build();
    let row_min = button_regulator_row_custom(
        "Мінімальний масштаб",
        &min_zoom,
        |value| format!("{:.0} %", value * 100.0),
        |_| 0.01,
    );
    group_zoom.add(&row_min);

    let max_zoom = gtk4::Adjustment::new(40.0, 1.0, 100.0, 1.0, 5.0, 0.0);
    settings
        .inner()
        .bind("max-zoom", &max_zoom, "value")
        .build();
    let row_max = button_regulator_row_custom(
        "Максимальний масштаб",
        &max_zoom,
        |value| format!("{:.0} %", value * 100.0),
        |value| {
            let percent = value * 100.0;
            if percent <= 1000.0 {
                0.1
            } else if percent <= 5000.0 {
                1.0
            } else {
                5.0
            }
        },
    );
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
    button.set_rgba(
        &parse_hex_color(&settings.inner().string(key))
            .unwrap_or(gdk::RGBA::BLACK),
    );

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
    previews: &BackgroundPreviews,
    effects: &adw::ExpanderRow,
) {
    let background_type = settings.background_type();
    let uses_wallpaper = background_type.uses_wallpaper();

    match background_type {
        BackgroundType::CustomColors => {
            previews
                .day_preview
                .set_child(Some(&previews.day_color));

            previews
                .night_preview
                .set_child(Some(&previews.night_color));

            preview_row.set_visible(true);
        }

        BackgroundType::CustomWallpapers => {
            previews
                .day_preview
                .set_child(Some(&previews.day_wallpaper));

            previews
                .night_preview
                .set_child(Some(&previews.night_wallpaper));

            preview_row.set_visible(true);
        }

        BackgroundType::SystemColors | BackgroundType::SystemWallpapers => {
            preview_row.set_visible(false);
        }
    }

    effects.set_visible(uses_wallpaper);
}

#[derive(Clone)]
struct BackgroundPreviews {
    day_preview: BackgroundPreview,
    night_preview: BackgroundPreview,

    day_color: gtk4::Widget,
    day_wallpaper: gtk4::Widget,
    night_color: gtk4::Widget,
    night_wallpaper: gtk4::Widget,
}

fn background_preview_row(settings: &Settings) -> (gtk4::Grid, BackgroundPreviews) {
    let row = gtk4::Grid::new();

    row.set_halign(gtk4::Align::Fill);
    row.set_hexpand(true);
    row.set_column_homogeneous(true);
    row.set_column_spacing(24);
    row.set_row_spacing(12);

    // Відступи такі ж, як у GNOME Settings.
    row.set_margin_start(12);
    row.set_margin_end(12);
    row.set_margin_top(18);
    row.set_margin_bottom(12);

    let ratio = monitor_aspect_ratio();

    let (day_column, day_preview, day_color, day_wallpaper) =
        background_preview_column(settings, false, "День", ratio);

    let (night_column, night_preview, night_color, night_wallpaper) =
        background_preview_column(settings, true, "Ніч", ratio);

    row.attach(&day_column, 0, 0, 1, 1);
    row.attach(&night_column, 1, 0, 1, 1);

    (
        row,
        BackgroundPreviews {
            day_preview,
            night_preview,
            day_color,
            day_wallpaper,
            night_color,
            night_wallpaper,
        },
    )
}

fn background_preview_column(
    settings: &Settings,
    night: bool,
    label: &str,
    ratio: f32,
) -> (
    gtk4::Box,
    BackgroundPreview,
    gtk4::Widget,
    gtk4::Widget,
) {
    let color_key = if night {
        "night-color"
    } else {
        "day-color"
    };

    let color_preview = color_button(settings, color_key, false);
    color_preview.set_hexpand(true);
    color_preview.set_vexpand(true);
    color_preview.set_halign(gtk4::Align::Fill);
    color_preview.set_valign(gtk4::Align::Fill);
    color_preview.add_css_class("background-preview");

    let color_preview = color_preview.upcast::<gtk4::Widget>();

    let wallpaper_preview = wallpaper_preview_button(settings, night);

    let preview = BackgroundPreview::new(ratio);

    // Початковий child.
    preview.set_child(Some(&color_preview));

    let title = gtk4::Label::new(Some(label));

    let column = gtk4::Box::new(
        gtk4::Orientation::Vertical,
        8,
    );

    column.set_halign(gtk4::Align::Fill);
    column.set_valign(gtk4::Align::Start);
    column.set_hexpand(true);

    column.append(&preview);
    column.append(&title);

    (
        column,
        preview,
        color_preview,
        wallpaper_preview,
    )
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

/// ActionRow з кнопкою-регулятором "Корекція яскравості": -100% (темніше) ..
/// 0 (без змін) .. +100% (світліше). Насправді це не гамма-корекція, а
/// дешевий напівпрозорий чорний/білий шар поверх шпалери в renderer.rs —
/// кнопка зберігає значення в GSettings і змінюється прокручуванням.
fn brightness_row(settings: &Settings) -> adw::ActionRow {
    button_regulator_row(
        settings,
        "Корекція яскравості",
        "wallpaper-brightness",
        -100.0,
        100.0,
        1.0,
        0,
    )
}

/// ActionRow з кнопкою-регулятором "Розмиття": 0 (без розмиття) .. 100 (максимум).
fn blur_row(settings: &Settings) -> adw::ActionRow {
    button_regulator_row(settings, "Розмиття", "wallpaper-blur", 0.0, 100.0, 1.0, 0)
}

/// ActionRow з кнопкою-регулятором "Насиченість": 0.0 (чорно-біла шпалера) ..
/// 1.0 (без змін, типово) .. 2.0 (подвоєна насиченість кольору).
fn saturation_row(settings: &Settings) -> adw::ActionRow {
    button_regulator_row(
        settings,
        "Насиченість",
        "wallpaper-saturation",
        0.0,
        2.0,
        0.1,
        1,
    )
}

/// ActionRow з кнопкою-регулятором "Зернистість": 0 (без зерна) .. 100 (максимум).
fn grain_row(settings: &Settings) -> adw::ActionRow {
    button_regulator_row(
        settings,
        "Зернистість",
        "wallpaper-grain",
        0.0,
        100.0,
        5.0,
        0,
    )
}

/// ExpanderRow "Показувати тінь": перемикач у кінці головного рядка,
/// п'ять вкладених рядків — чотири однакові за формою кнопки-регулятори (0..100px,
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

    row.add_row(&button_regulator_row(
        settings,
        "Зміщення по горизонталі",
        "shadow-offset-x",
        -100.0,
        100.0,
        1.0,
        0,
    ));
    row.add_row(&button_regulator_row(
        settings,
        "Зміщення по вертикалі",
        "shadow-offset-y",
        -100.0,
        100.0,
        1.0,
        0,
    ));
    row.add_row(&button_regulator_row(
        settings,
        "Радіус розмиття",
        "shadow-blur-radius",
        0.0,
        100.0,
        1.0,
        0,
    ));
    row.add_row(&button_regulator_row(
        settings,
        "Радіус розтягування",
        "shadow-spread-radius",
        0.0,
        100.0,
        1.0,
        0,
    ));

    let row_color = adw::ActionRow::builder().title("Колір").build();
    row_color.add_suffix(&color_button(settings, "shadow-color", true));
    row.add_row(&row_color);

    row.add_row(&button_regulator_row(
        settings,
        "Прозорість",
        "shadow-opacity",
        0.0,
        100.0,
        1.0,
        0,
    ));

    row
}

/// ActionRow з кнопкою-регулятором, прив'язаною до ключа `key`, у межах
/// `min..max`. Спільна форма для рядків тіні (зміщення X/Y, розмиття,
/// розтягування, прозорість) — змінюється прокручуванням кнопки, значення
/// синхронізується з GSettings через GtkAdjustment.
fn button_regulator_row(
    settings: &Settings,
    title: &str,
    key: &'static str,
    min: f64,
    max: f64,
    step: f64,
    digits: u32,
) -> adw::ActionRow {
    let adjustment = gtk4::Adjustment::new(min, min, max, step, step, 0.0);
    settings.inner().bind(key, &adjustment, "value").build();
    make_button_regulator_row(title, &adjustment, step, digits)
}

/// ActionRow з кнопкою-регулятором, прив'язаною до ключа "image-padding" —
/// відступ зображення від країв вікна при вписуванні в екран.
fn padding_row(settings: &Settings) -> adw::ActionRow {
    button_regulator_row(settings, "Відступ", "image-padding", 0.0, 100.0, 5.0, 0)
}

/// ActionRow з кнопкою-регулятором, прив'язаною до ключа "image-corner-radius" —
/// радіус заокруглення кутів зображення при відображенні.
fn corner_radius_row(settings: &Settings) -> adw::ActionRow {
    button_regulator_row(
        settings,
        "Заокруглення кутів",
        "image-corner-radius",
        0.0,
        20.0,
        1.0,
        0,
    )
}
