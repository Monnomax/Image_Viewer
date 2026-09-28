// settings.rs — типізована обгортка над GSettings.
//
// gio::Settings сам по собі є спільним об'єктом (внутрішньо — GObject із
// refcounting), тому окремий Rc<RefCell<...>> навколо нього більше не
// потрібен, як це було з ручним GKeyFile: досить дешево клонувати
// `Settings` скрізь, де він потрібен, а зміни зберігаються на диск
// (dconf) синхронно й одразу видимі всім клонам та всім прив'язаним
// віджетам через bind()/bind_with_mapping().
//
// Для реакції на зміни поза вікном налаштувань (напр. Canvas повинен
// перемалюватися при зміні кольору тла) використовуйте connect_changed().

use crate::model::SortMode;
use crate::preferences::auto_hide::VisibilityMode;
use adw::prelude::*;
use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use std::path::PathBuf;

/// Має збігатися з `id` схеми в data/com.example.ImgViewer.gschema.xml
/// та з application_id у main.rs.
pub const APP_ID: &str = "com.example.ImgViewer";

/// Режим попереднього завантаження сусідніх зображень (розділ
/// "Продуктивність" вікна налаштувань, поле "Попереднє завантаження
/// зображень"). "Auto" сам підбирає кількість під місткість
/// `cache-capacity`, решта варіантів — фіксована кількість сусідів
/// (сумарно в обидва боки від поточного зображення).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PreloadMode {
    Auto,
    Count(u32),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CrossfadeEasing {
    Back,
    Bounce,
    Circ,
    Cubic,
    Elastic,
    Expo,
    Linear,
    Quad,
    Quart,
    Quint,
    Sine,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TransitionAnimation {
    Crossfade,
    HorizontalSlide,
    VerticalSlide,
    HorizontalFadeSlide,
    VerticalFadeSlide,
    HorizontalScaleSlide,
    VerticalScaleSlide,
    ZoomFade,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DeleteAction {
    Trash,
    Permanent,
}

impl DeleteAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            DeleteAction::Trash => "trash",
            DeleteAction::Permanent => "permanent",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "permanent" => DeleteAction::Permanent,
            _ => DeleteAction::Trash,
        }
    }
}

impl CrossfadeEasing {
    pub fn as_str(&self) -> &'static str {
        match self {
            CrossfadeEasing::Back => "back",
            CrossfadeEasing::Bounce => "bounce",
            CrossfadeEasing::Circ => "circ",
            CrossfadeEasing::Cubic => "cubic",
            CrossfadeEasing::Elastic => "elastic",
            CrossfadeEasing::Expo => "expo",
            CrossfadeEasing::Linear => "linear",
            CrossfadeEasing::Quad => "quad",
            CrossfadeEasing::Quart => "quart",
            CrossfadeEasing::Quint => "quint",
            CrossfadeEasing::Sine => "sine",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "back" => CrossfadeEasing::Back,
            "bounce" => CrossfadeEasing::Bounce,
            "circ" => CrossfadeEasing::Circ,
            "cubic" => CrossfadeEasing::Cubic,
            "elastic" => CrossfadeEasing::Elastic,
            "expo" => CrossfadeEasing::Expo,
            "quad" => CrossfadeEasing::Quad,
            "quart" => CrossfadeEasing::Quart,
            "quint" => CrossfadeEasing::Quint,
            "sine" => CrossfadeEasing::Sine,
            // Сумісність зі старими значеннями, зведеними до базового cubic.
            "linear" => CrossfadeEasing::Linear,
            "ease-in-out" => CrossfadeEasing::Cubic,
            _ => CrossfadeEasing::Cubic,
        }
    }
}

impl TransitionAnimation {
    pub const ALL: [Self; 8] = [
        Self::Crossfade,
        Self::HorizontalSlide,
        Self::VerticalSlide,
        Self::HorizontalFadeSlide,
        Self::VerticalFadeSlide,
        Self::HorizontalScaleSlide,
        Self::VerticalScaleSlide,
        Self::ZoomFade,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            TransitionAnimation::Crossfade => "crossfade",
            TransitionAnimation::HorizontalSlide => "horizontal-slide",
            TransitionAnimation::VerticalSlide => "vertical-slide",
            TransitionAnimation::HorizontalFadeSlide => "horizontal-fade-slide",
            TransitionAnimation::VerticalFadeSlide => "vertical-fade-slide",
            TransitionAnimation::HorizontalScaleSlide => "horizontal-scale-slide",
            TransitionAnimation::VerticalScaleSlide => "vertical-scale-slide",
            TransitionAnimation::ZoomFade => "zoom-fade",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "slide" | "slide-left" | "slide-right" | "horizontal-slide" => {
                TransitionAnimation::HorizontalSlide
            }
            "vertical-slide" => TransitionAnimation::VerticalSlide,
            "fade-slide" | "horizontal-fade-slide" => TransitionAnimation::HorizontalFadeSlide,
            "vertical-fade-slide" => TransitionAnimation::VerticalFadeSlide,
            "horizontal-scale-slide" => TransitionAnimation::HorizontalScaleSlide,
            "vertical-scale-slide" => TransitionAnimation::VerticalScaleSlide,
            "zoom-fade" => TransitionAnimation::ZoomFade,
            _ => TransitionAnimation::Crossfade,
        }
    }
}

impl PreloadMode {
    pub fn as_str(&self) -> String {
        match self {
            PreloadMode::Auto => "auto".to_string(),
            PreloadMode::Count(n) => n.to_string(),
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "2" => PreloadMode::Count(2),
            "5" => PreloadMode::Count(5),
            "10" => PreloadMode::Count(10),
            "20" => PreloadMode::Count(20),
            _ => PreloadMode::Auto,
        }
    }

    /// Фактична сумарна кількість сусідніх зображень для попереднього
    /// завантаження (в обидва боки разом). Для "Автоматично" підлаштовується
    /// під місткість кешу декодованих зображень (`cache-capacity`), лишаючи
    /// в кеші запас під саме поточне зображення й трохи вільного місця,
    /// щоб кеш не переповнювався одразу після переходу.
    pub fn resolved_count(&self, cache_capacity: u32) -> usize {
        match self {
            PreloadMode::Auto => (cache_capacity as usize).saturating_sub(2).clamp(2, 8),
            PreloadMode::Count(n) => *n as usize,
        }
    }
}

trait SettingCodec: Copy {
    fn from_setting_value(value: &str) -> Self;
    fn to_setting_value(self) -> String;
}

impl SettingCodec for SortMode {
    fn from_setting_value(value: &str) -> Self {
        SortMode::from_str(value)
    }

    fn to_setting_value(self) -> String {
        self.as_str().to_string()
    }
}

impl SettingCodec for PreloadMode {
    fn from_setting_value(value: &str) -> Self {
        PreloadMode::from_str(value)
    }

    fn to_setting_value(self) -> String {
        self.as_str()
    }
}

impl SettingCodec for DeleteAction {
    fn from_setting_value(value: &str) -> Self {
        DeleteAction::from_str(value)
    }

    fn to_setting_value(self) -> String {
        self.as_str().to_string()
    }
}

impl SettingCodec for VisibilityMode {
    fn from_setting_value(value: &str) -> Self {
        VisibilityMode::from_setting(value)
    }

    fn to_setting_value(self) -> String {
        self.as_setting().to_string()
    }
}

impl SettingCodec for TransitionAnimation {
    fn from_setting_value(value: &str) -> Self {
        TransitionAnimation::from_str(value)
    }

    fn to_setting_value(self) -> String {
        self.as_str().to_string()
    }
}

impl SettingCodec for CrossfadeEasing {
    fn from_setting_value(value: &str) -> Self {
        CrossfadeEasing::from_str(value)
    }

    fn to_setting_value(self) -> String {
        self.as_str().to_string()
    }
}

#[derive(Clone)]
pub struct Settings(gio::Settings);

impl Settings {
    pub fn new() -> Self {
        Self(gio::Settings::new(APP_ID))
    }

    /// Доступ до сирого gio::Settings — потрібен вікну налаштувань для
    /// bind()/bind_with_mapping() та для підписки на "changed".
    pub fn inner(&self) -> &gio::Settings {
        &self.0
    }

    pub fn connect_changed<F: Fn(&str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.0.connect_changed(None, move |_, key| f(key))
    }

    fn read_enum<E: SettingCodec>(&self, key: &str) -> E {
        E::from_setting_value(&self.0.string(key))
    }

    fn write_enum<E: SettingCodec>(&self, key: &str, value: E) {
        let _ = self.0.set_string(key, &value.to_setting_value());
    }

    fn read_clamped_double(&self, key: &str, min: f64, max: f64, fallback: f64) -> f64 {
        let value = self.0.double(key);
        if value.is_finite() {
            value.clamp(min, max)
        } else {
            fallback
        }
    }

    fn read_clamped_uint(&self, key: &str, min: u32, max: u32, fallback: u32) -> u32 {
        let value = self.0.uint(key);
        if (min..=max).contains(&value) {
            value
        } else {
            fallback
        }
    }

    // ---------------- Загальні ----------------

    pub fn fullscreen_on_start(&self) -> bool {
        self.0.boolean("fullscreen-on-start")
    }

    pub fn remember_zoom(&self) -> bool {
        self.0.boolean("remember-zoom")
    }

    pub fn remember_position(&self) -> bool {
        self.0.boolean("remember-position")
    }

    pub fn remembered_view_state(&self) -> String {
        self.0.string("remembered-view-state").to_string()
    }

    pub fn set_remembered_view_state(&self, state: &str) {
        let _ = self.0.set_string("remembered-view-state", state);
    }

    pub fn sort_mode(&self) -> SortMode {
        self.read_enum("sort-mode")
    }

    pub fn set_sort_mode(&self, mode: SortMode) {
        self.write_enum("sort-mode", mode);
    }

    pub fn cache_capacity(&self) -> u32 {
        self.0.uint("cache-capacity")
    }

    pub fn preload_mode(&self) -> PreloadMode {
        self.read_enum("preload-mode")
    }

    pub fn set_preload_mode(&self, mode: PreloadMode) {
        self.write_enum("preload-mode", mode);
    }

    pub fn preload_textures_enabled(&self) -> bool {
        self.0.boolean("preload-textures-enabled")
    }

    pub fn delete_action(&self) -> DeleteAction {
        self.read_enum("delete-action")
    }

    pub fn set_delete_action(&self, action: DeleteAction) {
        self.write_enum("delete-action", action);
    }

    pub fn confirm_delete(&self) -> bool {
        self.0.boolean("confirm-delete")
    }

    // ---------------- Перегляд ----------------

    /// Колір тла для ручного режиму, або — якщо в розділі "Перегляд"
    /// увімкнено перемикач "День / Ніч" — автоматично підставлений
    /// `day-color`/`night-color` залежно від поточної системної схеми
    /// кольорів (AdwStyleManager::is_dark(), яка сама стежить за системним
    /// портал-налаштуванням "Темний стиль" і оновлюється в реальному часі
    /// при перемиканні системи — canvas.queue_draw() на цю зміну підписаний
    /// окремо в app.rs).
    pub fn background_color(&self) -> gdk::RGBA {
        if self.day_night_enabled() {
            if adw::StyleManager::default().is_dark() {
                self.night_color()
            } else {
                self.day_color()
            }
        } else {
            parse_hex_color(&self.0.string("background-color"))
                .unwrap_or(gdk::RGBA::new(0.05, 0.05, 0.05, 1.0))
        }
    }

    pub fn day_night_enabled(&self) -> bool {
        self.0.boolean("day-night-enabled")
    }

    pub fn day_color(&self) -> gdk::RGBA {
        parse_hex_color(&self.0.string("day-color"))
            .unwrap_or(gdk::RGBA::new(0.96, 0.96, 0.96, 1.0))
    }

    pub fn night_color(&self) -> gdk::RGBA {
        parse_hex_color(&self.0.string("night-color"))
            .unwrap_or(gdk::RGBA::new(0.12, 0.12, 0.12, 1.0))
    }

    pub fn use_desktop_wallpaper(&self) -> bool {
        self.0.boolean("use-desktop-wallpaper")
    }

    /// -100.0 (найтемніше) .. 0.0 (без корекції) .. +100.0 (найсвітліше).
    pub fn wallpaper_brightness(&self) -> f64 {
        self.0.double("wallpaper-brightness")
    }

    /// 0.0 (без розмиття) .. 100.0 (максимальна інтенсивність).
    pub fn wallpaper_blur(&self) -> f64 {
        self.0.double("wallpaper-blur")
    }

    /// 0.0 (чорно-біла шпалера) .. 1.0 (без змін, типово) .. 2.0
    /// (подвоєна насиченість кольору).
    pub fn wallpaper_saturation(&self) -> f64 {
        self.0.double("wallpaper-saturation")
    }

    /// 0 (без зерна) .. 100 (максимальна інтенсивність шумового оверлею).
    pub fn wallpaper_grain(&self) -> u32 {
        self.0.uint("wallpaper-grain")
    }

    pub fn shadow_enabled(&self) -> bool {
        self.0.boolean("shadow-enabled")
    }

    pub fn shadow_offset_x(&self) -> f64 {
        self.0.double("shadow-offset-x")
    }

    pub fn shadow_offset_y(&self) -> f64 {
        self.0.double("shadow-offset-y")
    }

    pub fn shadow_blur_radius(&self) -> f64 {
        self.0.double("shadow-blur-radius")
    }

    pub fn shadow_spread_radius(&self) -> f64 {
        self.0.double("shadow-spread-radius")
    }

    /// Колір тіні — на відміну від background-color/day-color/night-color
    /// (завжди непрозорі), тут зберігається й альфа-канал ("#RRGGBBAA"),
    /// бо м'яка напівпрозора тінь виглядає природніше за суцільну.
    pub fn shadow_color(&self) -> gdk::RGBA {
        parse_hex_color(&self.0.string("shadow-color"))
            .unwrap_or(gdk::RGBA::new(0.0, 0.0, 0.0, 0.5))
    }

    /// 0.0 .. 100.0 — окремий множник поверх альфи, обраної в пікері
    /// кольору тіні (shadow_color): дає змогу швидко "притлумити" тінь, не
    /// відкриваючи щоразу колірний діалог.
    pub fn shadow_opacity(&self) -> f64 {
        self.0.double("shadow-opacity")
    }

    pub fn image_padding(&self) -> f64 {
        self.0.uint("image-padding") as f64
    }

    pub fn image_corner_radius(&self) -> f64 {
        self.0.uint("image-corner-radius") as f64
    }

    pub fn min_zoom(&self) -> f64 {
        self.read_clamped_double("min-zoom", 0.01, 1.0, 0.02)
    }

    pub fn max_zoom(&self) -> f64 {
        self.read_clamped_double("max-zoom", 1.0, 100.0, 40.0)
    }

    // ---------------- Навігація ----------------

    pub fn zoom_speed(&self) -> f64 {
        self.read_clamped_double("zoom-speed", 1.01, 2.0, 1.10)
    }

    pub fn invert_scroll(&self) -> bool {
        self.0.boolean("invert-scroll")
    }

    pub fn nav_zone_width(&self) -> f64 {
        self.0.double("nav-zone-width")
    }

    pub fn show_nav_buttons(&self) -> bool {
        self.0.boolean("show-nav-buttons")
    }

    pub fn show_close_button(&self) -> bool {
        self.0.boolean("show-close-button")
    }

    pub fn hide_cursor_timeout_ms(&self) -> u32 {
        self.read_clamped_uint("hide-cursor-timeout-ms", 0, 5000, 0)
    }

    pub fn show_delay_ms(&self) -> u32 {
        self.read_clamped_uint("show-delay-ms", 0, 1000, 0)
    }

    pub fn sensitivity_zones_enabled(&self) -> bool {
        self.0.boolean("sensitivity-zones-enabled")
    }

    pub fn set_sensitivity_zones_enabled(&self, enabled: bool) {
        let _ = self.0.set_boolean("sensitivity-zones-enabled", enabled);
    }

    pub fn cursor_visibility_mode(&self) -> String {
        self.0.string("cursor-visibility-mode").to_string()
    }

    pub fn close_button_visibility_mode(&self) -> String {
        self.0.string("close-button-visibility-mode").to_string()
    }

    pub fn nav_buttons_visibility_mode(&self) -> String {
        self.0.string("nav-buttons-visibility-mode").to_string()
    }

    pub fn filename_label_visibility_mode(&self) -> String {
        self.0.string("filename-label-visibility-mode").to_string()
    }

    pub fn thumbnail_strip_visibility_mode(&self) -> String {
        self.0.string("thumbnail-strip-visibility-mode").to_string()
    }

    pub fn thumbnail_strip_enabled(&self) -> bool {
        self.0.boolean("thumbnail-strip-enabled")
    }

    pub fn thumbnail_strip_position(&self) -> String {
        self.0.string("thumbnail-strip-position").to_string()
    }

    pub fn thumbnail_strip_size(&self) -> u32 {
        self.0.uint("thumbnail-strip-size")
    }

    pub fn thumbnail_strip_padding(&self) -> u32 {
        self.0.uint("thumbnail-strip-padding")
    }

    pub fn thumbnail_strip_margin(&self) -> u32 {
        self.0.uint("thumbnail-strip-margin")
    }

    pub fn thumbnail_strip_spacing(&self) -> u32 {
        self.0.uint("thumbnail-strip-spacing")
    }

    pub fn set_cursor_visibility_mode(&self, mode: VisibilityMode) {
        self.write_enum("cursor-visibility-mode", mode);
    }

    pub fn set_close_button_visibility_mode(&self, mode: VisibilityMode) {
        self.write_enum("close-button-visibility-mode", mode);
    }

    pub fn set_nav_buttons_visibility_mode(&self, mode: VisibilityMode) {
        self.write_enum("nav-buttons-visibility-mode", mode);
    }

    pub fn set_filename_label_visibility_mode(&self, mode: VisibilityMode) {
        self.write_enum("filename-label-visibility-mode", mode);
    }

    pub fn set_thumbnail_strip_visibility_mode(&self, mode: VisibilityMode) {
        self.write_enum("thumbnail-strip-visibility-mode", mode);
    }

    // ---------------- Анімації ----------------

    pub fn crossfade_enabled(&self) -> bool {
        self.0.boolean("crossfade-enabled")
    }

    pub fn transition_animation(&self) -> TransitionAnimation {
        self.read_enum("transition-animation")
    }

    pub fn set_transition_animation(&self, animation: TransitionAnimation) {
        self.write_enum("transition-animation", animation);
    }

    pub fn crossfade_duration_ms(&self) -> u32 {
        self.read_clamped_uint("crossfade-duration-ms", 0, 1000, 500)
    }

    pub fn set_crossfade_duration_ms(&self, duration_ms: u32) {
        let _ = self.0.set_uint("crossfade-duration-ms", duration_ms);
    }

    pub fn horizontal_slide_duration_ms(&self) -> u32 {
        self.read_clamped_uint("horizontal-slide-duration-ms", 0, 1000, 500)
    }

    pub fn set_horizontal_slide_duration_ms(&self, duration_ms: u32) {
        let _ = self.0.set_uint("horizontal-slide-duration-ms", duration_ms.min(1000));
    }

    pub fn vertical_slide_duration_ms(&self) -> u32 {
        self.read_clamped_uint("vertical-slide-duration-ms", 0, 1000, 500)
    }

    pub fn set_vertical_slide_duration_ms(&self, duration_ms: u32) {
        let _ = self.0.set_uint("vertical-slide-duration-ms", duration_ms.min(1000));
    }

    pub fn horizontal_fade_slide_duration_ms(&self) -> u32 {
        self.read_clamped_uint("horizontal-fade-slide-duration-ms", 0, 1000, 500)
    }

    pub fn set_horizontal_fade_slide_duration_ms(&self, duration_ms: u32) {
        let _ = self
            .0
            .set_uint("horizontal-fade-slide-duration-ms", duration_ms.min(1000));
    }

    pub fn vertical_fade_slide_duration_ms(&self) -> u32 {
        self.read_clamped_uint("vertical-fade-slide-duration-ms", 0, 1000, 500)
    }

    pub fn set_vertical_fade_slide_duration_ms(&self, duration_ms: u32) {
        let _ = self
            .0
            .set_uint("vertical-fade-slide-duration-ms", duration_ms.min(1000));
    }

    pub fn horizontal_scale_slide_duration_ms(&self) -> u32 {
        self.read_clamped_uint("horizontal-scale-slide-duration-ms", 0, 1000, 200)
    }

    pub fn set_horizontal_scale_slide_duration_ms(&self, duration_ms: u32) {
        let _ = self
            .0
            .set_uint("horizontal-scale-slide-duration-ms", duration_ms.min(1000));
    }

    pub fn horizontal_scale_slide_start_scale(&self) -> f64 {
        self.read_clamped_double("horizontal-scale-slide-start-scale", 0.0, 1.0, 0.75)
    }

    pub fn set_horizontal_scale_slide_start_scale(&self, scale: f64) {
        let _ = self
            .0
            .set_double("horizontal-scale-slide-start-scale", scale.clamp(0.0, 1.0));
    }

    pub fn horizontal_scale_slide_easing(&self) -> CrossfadeEasing {
        self.read_enum("horizontal-scale-slide-easing")
    }

    pub fn set_horizontal_scale_slide_easing(&self, easing: CrossfadeEasing) {
        self.write_enum("horizontal-scale-slide-easing", easing);
    }

    pub fn vertical_scale_slide_duration_ms(&self) -> u32 {
        self.read_clamped_uint("vertical-scale-slide-duration-ms", 0, 1000, 200)
    }

    pub fn set_vertical_scale_slide_duration_ms(&self, duration_ms: u32) {
        let _ = self
            .0
            .set_uint("vertical-scale-slide-duration-ms", duration_ms.min(1000));
    }

    pub fn vertical_scale_slide_start_scale(&self) -> f64 {
        self.read_clamped_double("vertical-scale-slide-start-scale", 0.0, 1.0, 0.75)
    }

    pub fn set_vertical_scale_slide_start_scale(&self, scale: f64) {
        let _ = self
            .0
            .set_double("vertical-scale-slide-start-scale", scale.clamp(0.0, 1.0));
    }

    pub fn vertical_scale_slide_easing(&self) -> CrossfadeEasing {
        self.read_enum("vertical-scale-slide-easing")
    }

    pub fn set_vertical_scale_slide_easing(&self, easing: CrossfadeEasing) {
        self.write_enum("vertical-scale-slide-easing", easing);
    }

    pub fn zoom_fade_in_scale(&self) -> f64 {
        self.read_clamped_double("zoom-fade-in-scale", 0.0, 1.0, 0.25)
    }

    pub fn set_zoom_fade_in_scale(&self, scale: f64) {
        let _ = self.0.set_double("zoom-fade-in-scale", scale.clamp(0.0, 1.0));
    }

    pub fn zoom_fade_out_scale(&self) -> f64 {
        self.read_clamped_double("zoom-fade-out-scale", 0.0, 1.0, 0.25)
    }

    pub fn set_zoom_fade_out_scale(&self, scale: f64) {
        let _ = self.0.set_double("zoom-fade-out-scale", scale.clamp(0.0, 1.0));
    }

    pub fn zoom_fade_in_easing(&self) -> CrossfadeEasing {
        self.read_enum("zoom-fade-in-easing")
    }

    pub fn set_zoom_fade_in_easing(&self, easing: CrossfadeEasing) {
        self.write_enum("zoom-fade-in-easing", easing);
    }

    pub fn zoom_fade_out_easing(&self) -> CrossfadeEasing {
        self.read_enum("zoom-fade-out-easing")
    }

    pub fn set_zoom_fade_out_easing(&self, easing: CrossfadeEasing) {
        self.write_enum("zoom-fade-out-easing", easing);
    }

    pub fn zoom_fade_in_duration_ms(&self) -> u32 {
        self.read_clamped_uint("zoom-fade-in-duration-ms", 0, 1000, 500)
    }

    pub fn set_zoom_fade_in_duration_ms(&self, duration_ms: u32) {
        let _ = self.0.set_uint("zoom-fade-in-duration-ms", duration_ms.min(1000));
    }

    pub fn zoom_fade_out_duration_ms(&self) -> u32 {
        self.read_clamped_uint("zoom-fade-out-duration-ms", 0, 1000, 500)
    }

    pub fn set_zoom_fade_out_duration_ms(&self, duration_ms: u32) {
        let _ = self.0.set_uint("zoom-fade-out-duration-ms", duration_ms.min(1000));
    }

    pub fn crossfade_easing(&self) -> CrossfadeEasing {
        self.read_enum("crossfade-easing")
    }

    pub fn set_crossfade_easing(&self, easing: CrossfadeEasing) {
        self.write_enum("crossfade-easing", easing);
    }

    pub fn crossfade_fade_out_easing(&self) -> CrossfadeEasing {
        self.read_enum("crossfade-fade-out-easing")
    }

    pub fn set_crossfade_fade_out_easing(&self, easing: CrossfadeEasing) {
        self.write_enum("crossfade-fade-out-easing", easing);
    }

    pub fn crossfade_fade_out_duration_ms(&self) -> u32 {
        self.read_clamped_uint("crossfade-fade-out-duration-ms", 0, 2000, 250)
    }

    pub fn set_crossfade_fade_out_duration_ms(&self, duration_ms: u32) {
        let _ = self
            .0
            .set_uint("crossfade-fade-out-duration-ms", duration_ms);
    }

    pub fn smooth_zoom_enabled(&self) -> bool {
        self.0.boolean("smooth-zoom-enabled")
    }

    pub fn animation_duration_ms(&self) -> u32 {
        self.read_clamped_uint("animation-duration-ms", 0, 1000, 200)
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self::new()
    }
}

/// Розбирає рядок "#RRGGBB" або "#RRGGBBAA" у gdk::RGBA.
/// GSettings не має нативного типу кольору, тому колірні ключі схеми
/// зберігаються рядком, а перетворення в/з gdk::RGBA лежить тут.
pub fn parse_hex_color(s: &str) -> Option<gdk::RGBA> {
    let s = s.trim().trim_start_matches('#');
    let (r, g, b, a) = match s.len() {
        6 => (
            u8::from_str_radix(&s[0..2], 16).ok()?,
            u8::from_str_radix(&s[2..4], 16).ok()?,
            u8::from_str_radix(&s[4..6], 16).ok()?,
            255u8,
        ),
        8 => (
            u8::from_str_radix(&s[0..2], 16).ok()?,
            u8::from_str_radix(&s[2..4], 16).ok()?,
            u8::from_str_radix(&s[4..6], 16).ok()?,
            u8::from_str_radix(&s[6..8], 16).ok()?,
        ),
        _ => return None,
    };
    Some(gdk::RGBA::new(
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        a as f32 / 255.0,
    ))
}

/// Повертає шлях до поточної шпалери робочого столу GNOME
/// (`org.gnome.desktop.background`), обираючи світлий/темний варіант
/// відповідно до системної схеми кольорів.
///
/// `None`, якщо ця схема відсутня в системі (не-GNOME оточення — на
/// відміну від власної схеми застосунку, тут `gio::Settings::new()` без
/// попередньої перевірки просто аварійно завершив би процес), ключ
/// порожній, або файл шпалери не знайдено на диску.
pub fn desktop_wallpaper_path() -> Option<PathBuf> {
    const SCHEMA_ID: &str = "org.gnome.desktop.background";
    gio::SettingsSchemaSource::default()?.lookup(SCHEMA_ID, true)?;

    let bg_settings = gio::Settings::new(SCHEMA_ID);

    let dark_uri = bg_settings.string("picture-uri-dark");
    let light_uri = bg_settings.string("picture-uri");
    let uri = if adw::StyleManager::default().is_dark() && !dark_uri.is_empty() {
        dark_uri
    } else {
        light_uri
    };
    if uri.is_empty() {
        return None;
    }

    let path = gio::File::for_uri(&uri).path()?;
    if path.is_file() {
        Some(path)
    } else {
        None
    }
}

pub fn rgba_to_hex(c: &gdk::RGBA) -> String {
    format!(
        "#{:02X}{:02X}{:02X}",
        (c.red() * 255.0).round() as u8,
        (c.green() * 255.0).round() as u8,
        (c.blue() * 255.0).round() as u8,
    )
}

/// Як `rgba_to_hex`, але зі збереженим альфа-каналом ("#RRGGBBAA") — для
/// пікерів кольору, де прозорість — частина самого налаштування (напр.
/// колір тіні), на відміну від background-color/day-color/night-color.
pub fn rgba_to_hex_alpha(c: &gdk::RGBA) -> String {
    format!(
        "#{:02X}{:02X}{:02X}{:02X}",
        (c.red() * 255.0).round() as u8,
        (c.green() * 255.0).round() as u8,
        (c.blue() * 255.0).round() as u8,
        (c.alpha() * 255.0).round() as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        let c = parse_hex_color("#3C7AF0").unwrap();
        assert_eq!(rgba_to_hex(&c), "#3C7AF0");
    }

    #[test]
    fn transition_animation_profiles_have_unique_stable_keys() {
        let keys: Vec<_> = TransitionAnimation::ALL
            .iter()
            .map(TransitionAnimation::as_str)
            .collect();

        let unique_keys: std::collections::HashSet<_> = keys.iter().copied().collect();
        assert_eq!(unique_keys.len(), TransitionAnimation::ALL.len());

        for animation in TransitionAnimation::ALL {
            assert_eq!(TransitionAnimation::from_str(animation.as_str()), animation);
        }
    }

    #[test]
    fn vertical_scale_slide_start_scale_accepts_and_restores_profile_value() {
        let settings = Settings::new();
        settings.set_vertical_scale_slide_start_scale(0.0);
        assert!(settings.vertical_scale_slide_start_scale().abs() < f64::EPSILON);

        settings.set_vertical_scale_slide_start_scale(0.85);

        assert!((settings.vertical_scale_slide_start_scale() - 0.85).abs() < f64::EPSILON);
    }

    #[test]
    fn hex_with_alpha_ignores_alpha_on_reencode() {
        let c = parse_hex_color("#3C7AF080").unwrap();
        assert_eq!(rgba_to_hex(&c), "#3C7AF0");
    }

    #[test]
    fn invalid_hex_is_none() {
        assert!(parse_hex_color("not-a-color").is_none());
    }
}
