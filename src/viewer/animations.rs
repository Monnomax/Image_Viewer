// preferences/animations.rs — розділ "Анімації": перехід між зображеннями
// (crossfade) і плавне масштабування.

use crate::preferences::settings::{CrossfadeEasing, Settings, TransitionAnimation};
use adw::prelude::*;

pub fn build_page(settings: &Settings) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("Анімації")
        .icon_name("preferences-desktop-display-symbolic")
        .build();

    // ---- Група: анімація переходу ----
    let group_transition = adw::PreferencesGroup::builder()
        .title("Анімація переходу")
        .build();

    let row_transition = adw::ExpanderRow::builder()
        .title("Тип анімації")
        .subtitle("Оберіть тип анімації переходу")
        .build();

    let animation_options = [
        "Crossfade",
        "Horizontal Slide",
        "Vertical Slide",
        "Horizontal Fade Slide",
        "Vertical Fade Slide",
        "Horizontal Scale Slide",
        "Vertical Scale Slide",
        "Zoom Fade",
    ];
    let animation_model = gtk4::StringList::new(&animation_options);
    let animation_dropdown = gtk4::DropDown::builder()
        .model(&animation_model)
        .selected(transition_animation_index(settings.transition_animation()))
        .build();
    animation_dropdown.set_valign(gtk4::Align::Center);
    let transition_switch = gtk4::Switch::new();
    transition_switch.set_valign(gtk4::Align::Center);
    settings
        .inner()
        .bind("crossfade-enabled", &transition_switch, "active")
        .build();
    row_transition.add_suffix(&transition_switch);
    row_transition.add_suffix(&animation_dropdown);

    let zoom_in_row = zoom_scale_row(settings, "Масштаб вхідної анімації", "zoom-fade-in-scale");
    let zoom_out_row = zoom_scale_row(settings, "Масштаб вихідної анімації", "zoom-fade-out-scale");
    let zoom_in_easing_row = easing_row(
        settings,
        "Крива вхідної анімації",
        "zoom-fade-in-easing",
    );
    let zoom_out_easing_row = easing_row(
        settings,
        "Крива вихідної анімації",
        "zoom-fade-out-easing",
    );
    let zoom_in_duration_row = zoom_duration_row(
        settings,
        "Тривалість вхідної анімації",
        "zoom-fade-in-duration-ms",
    );
    let zoom_out_duration_row = zoom_duration_row(
        settings,
        "Тривалість вихідної анімації",
        "zoom-fade-out-duration-ms",
    );
    let crossfade_easing_row = easing_row(settings, "Крива анімації", "crossfade-easing");
    let crossfade_duration_row = transition_duration_row(settings, "Тривалість анімації");
    let horizontal_slide_duration_row = slide_duration_row(
        settings,
        "Тривалість анімації",
        TransitionAnimation::HorizontalSlide,
    );
    let vertical_slide_duration_row = slide_duration_row(
        settings,
        "Тривалість анімації",
        TransitionAnimation::VerticalSlide,
    );
    let horizontal_fade_slide_duration_row = slide_duration_row(
        settings,
        "Тривалість анімації",
        TransitionAnimation::HorizontalFadeSlide,
    );
    let vertical_fade_slide_duration_row = slide_duration_row(
        settings,
        "Тривалість анімації",
        TransitionAnimation::VerticalFadeSlide,
    );
    let horizontal_scale_slide_duration_row = slide_duration_row(
        settings,
        "Тривалість анімації",
        TransitionAnimation::HorizontalScaleSlide,
    );
    let horizontal_scale_slide_start_scale_row = zoom_scale_row(
        settings,
        "Початковий масштаб",
        "horizontal-scale-slide-start-scale",
    );
    let horizontal_scale_slide_easing_row = easing_row(
        settings,
        "Крива анімації",
        "horizontal-scale-slide-easing",
    );
    let vertical_scale_slide_duration_row = slide_duration_row(
        settings,
        "Тривалість анімації",
        TransitionAnimation::VerticalScaleSlide,
    );
    let vertical_scale_slide_start_scale_row = zoom_scale_row(
        settings,
        "Початковий масштаб",
        "vertical-scale-slide-start-scale",
    );
    let vertical_scale_slide_easing_row = easing_row(
        settings,
        "Крива анімації",
        "vertical-scale-slide-easing",
    );
    let animation = settings.transition_animation();
    let is_zoom_fade = matches!(animation, TransitionAnimation::ZoomFade);
    let is_crossfade = matches!(animation, TransitionAnimation::Crossfade);
    zoom_in_row.set_visible(is_zoom_fade);
    zoom_out_row.set_visible(is_zoom_fade);
    zoom_in_easing_row.set_visible(is_zoom_fade);
    zoom_out_easing_row.set_visible(is_zoom_fade);
    zoom_in_duration_row.set_visible(is_zoom_fade);
    zoom_out_duration_row.set_visible(is_zoom_fade);
    crossfade_easing_row.set_visible(is_crossfade);
    crossfade_duration_row.set_visible(is_crossfade);
    set_slide_duration_visibility(
        animation,
        &horizontal_slide_duration_row,
        &vertical_slide_duration_row,
        &horizontal_fade_slide_duration_row,
        &vertical_fade_slide_duration_row,
        &horizontal_scale_slide_duration_row,
        &horizontal_scale_slide_start_scale_row,
        &horizontal_scale_slide_easing_row,
        &vertical_scale_slide_duration_row,
        &vertical_scale_slide_start_scale_row,
        &vertical_scale_slide_easing_row,
    );
    row_transition.add_row(&zoom_in_row);
    row_transition.add_row(&zoom_out_row);
    row_transition.add_row(&zoom_in_easing_row);
    row_transition.add_row(&zoom_out_easing_row);
    row_transition.add_row(&zoom_in_duration_row);
    row_transition.add_row(&zoom_out_duration_row);
    row_transition.add_row(&crossfade_easing_row);
    row_transition.add_row(&crossfade_duration_row);
    row_transition.add_row(&horizontal_slide_duration_row);
    row_transition.add_row(&vertical_slide_duration_row);
    row_transition.add_row(&horizontal_fade_slide_duration_row);
    row_transition.add_row(&vertical_fade_slide_duration_row);
    row_transition.add_row(&horizontal_scale_slide_start_scale_row);
    row_transition.add_row(&horizontal_scale_slide_easing_row);
    row_transition.add_row(&horizontal_scale_slide_duration_row);
    row_transition.add_row(&vertical_scale_slide_start_scale_row);
    row_transition.add_row(&vertical_scale_slide_easing_row);
    row_transition.add_row(&vertical_scale_slide_duration_row);

    {
        let settings = settings.clone();
        let zoom_in_row = zoom_in_row.clone();
        let zoom_out_row = zoom_out_row.clone();
        let zoom_in_easing_row = zoom_in_easing_row.clone();
        let zoom_out_easing_row = zoom_out_easing_row.clone();
        let zoom_in_duration_row = zoom_in_duration_row.clone();
        let zoom_out_duration_row = zoom_out_duration_row.clone();
        let crossfade_easing_row = crossfade_easing_row.clone();
        let crossfade_duration_row = crossfade_duration_row.clone();
        let horizontal_slide_duration_row = horizontal_slide_duration_row.clone();
        let vertical_slide_duration_row = vertical_slide_duration_row.clone();
        let horizontal_fade_slide_duration_row = horizontal_fade_slide_duration_row.clone();
        let vertical_fade_slide_duration_row = vertical_fade_slide_duration_row.clone();
        let horizontal_scale_slide_duration_row = horizontal_scale_slide_duration_row.clone();
        let horizontal_scale_slide_start_scale_row = horizontal_scale_slide_start_scale_row.clone();
        let horizontal_scale_slide_easing_row = horizontal_scale_slide_easing_row.clone();
        let vertical_scale_slide_duration_row = vertical_scale_slide_duration_row.clone();
        let vertical_scale_slide_start_scale_row = vertical_scale_slide_start_scale_row.clone();
        let vertical_scale_slide_easing_row = vertical_scale_slide_easing_row.clone();
        animation_dropdown.connect_selected_notify(move |dropdown| {
            let is_zoom = matches!(
                transition_animation_from_index(dropdown.selected()),
                TransitionAnimation::ZoomFade
            );
            zoom_in_row.set_visible(is_zoom);
            zoom_out_row.set_visible(is_zoom);
            zoom_in_easing_row.set_visible(is_zoom);
            zoom_out_easing_row.set_visible(is_zoom);
            zoom_in_duration_row.set_visible(is_zoom);
            zoom_out_duration_row.set_visible(is_zoom);
            let is_crossfade = matches!(
                transition_animation_from_index(dropdown.selected()),
                TransitionAnimation::Crossfade
            );
            crossfade_easing_row.set_visible(is_crossfade);
            crossfade_duration_row.set_visible(is_crossfade);
            set_slide_duration_visibility(
                transition_animation_from_index(dropdown.selected()),
                &horizontal_slide_duration_row,
                &vertical_slide_duration_row,
                &horizontal_fade_slide_duration_row,
                &vertical_fade_slide_duration_row,
                &horizontal_scale_slide_duration_row,
                &horizontal_scale_slide_start_scale_row,
                &horizontal_scale_slide_easing_row,
                &vertical_scale_slide_duration_row,
                &vertical_scale_slide_start_scale_row,
                &vertical_scale_slide_easing_row,
            );
            settings.set_transition_animation(transition_animation_from_index(dropdown.selected()));
        });
    }

    group_transition.add(&row_transition);

    page.add(&group_transition);

    // ---- Група: масштабування ----
    let group_zoom = adw::PreferencesGroup::builder()
        .title("Масштабування")
        .build();

    let row_smooth = adw::SwitchRow::builder()
        .title("Плавне масштабування")
        .build();
    settings
        .inner()
        .bind("smooth-zoom-enabled", &row_smooth, "active")
        .build();
    group_zoom.add(&row_smooth);

    let row_anim_dur = adw::SpinRow::new(
        Some(&gtk4::Adjustment::new(200.0, 0.0, 2000.0, 10.0, 50.0, 0.0)),
        10.0,
        0,
    );
    row_anim_dur.set_title("Тривалість анімацій (мс)");
    settings
        .inner()
        .bind("animation-duration-ms", &row_anim_dur, "value")
        .build();
    settings
        .inner()
        .bind("smooth-zoom-enabled", &row_anim_dur, "sensitive")
        .build();
    group_zoom.add(&row_anim_dur);

    page.add(&group_zoom);

    page
}

fn transition_animation_index(animation: TransitionAnimation) -> u32 {
    match animation {
        TransitionAnimation::Crossfade => 0,
        TransitionAnimation::HorizontalSlide => 1,
        TransitionAnimation::VerticalSlide => 2,
        TransitionAnimation::HorizontalFadeSlide => 3,
        TransitionAnimation::VerticalFadeSlide => 4,
        TransitionAnimation::HorizontalScaleSlide => 5,
        TransitionAnimation::VerticalScaleSlide => 6,
        TransitionAnimation::ZoomFade => 7,
    }
}

fn transition_animation_from_index(i: u32) -> TransitionAnimation {
    match i {
        1 => TransitionAnimation::HorizontalSlide,
        2 => TransitionAnimation::VerticalSlide,
        3 => TransitionAnimation::HorizontalFadeSlide,
        4 => TransitionAnimation::VerticalFadeSlide,
        5 => TransitionAnimation::HorizontalScaleSlide,
        6 => TransitionAnimation::VerticalScaleSlide,
        7 => TransitionAnimation::ZoomFade,
        _ => TransitionAnimation::Crossfade,
    }
}

fn zoom_scale_row(settings: &Settings, title: &str, key: &'static str) -> adw::SpinRow {
    let initial_value = match key {
        "zoom-fade-in-scale" => settings.zoom_fade_in_scale(),
        "zoom-fade-out-scale" => settings.zoom_fade_out_scale(),
        "horizontal-scale-slide-start-scale" => settings.horizontal_scale_slide_start_scale(),
        "vertical-scale-slide-start-scale" => settings.vertical_scale_slide_start_scale(),
        _ => 0.25,
    };

    let min = 0.0;
    let step = 5.0;
    let adjustment = gtk4::Adjustment::new(initial_value * 100.0, min, 100.0, step, 5.0, 0.0);
    let spin_row = adw::SpinRow::new(Some(&adjustment), step, 0);
    spin_row.set_title(title);

    {
        let settings = settings.clone();
        adjustment.connect_value_changed(move |adj| {
            let value = adj.value() / 100.0;
            match key {
                "zoom-fade-in-scale" => settings.set_zoom_fade_in_scale(value),
                "zoom-fade-out-scale" => settings.set_zoom_fade_out_scale(value),
                "horizontal-scale-slide-start-scale" => {
                    settings.set_horizontal_scale_slide_start_scale(value)
                }
                "vertical-scale-slide-start-scale" => {
                    settings.set_vertical_scale_slide_start_scale(value)
                }
                _ => {}
            }
        });
    }

    spin_row
}

fn easing_row(settings: &Settings, title: &str, key: &'static str) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title(title).build();
    let options = [
        "Back", "Bounce", "Circ", "Cubic", "Elastic", "Expo", "Linear", "Quad", "Quart",
        "Quint", "Sine",
    ];
    let model = gtk4::StringList::new(&options);
    let current = match key {
        "zoom-fade-in-easing" => settings.zoom_fade_in_easing(),
        "zoom-fade-out-easing" => settings.zoom_fade_out_easing(),
        "crossfade-easing" => settings.crossfade_easing(),
        "horizontal-scale-slide-easing" => settings.horizontal_scale_slide_easing(),
        "vertical-scale-slide-easing" => settings.vertical_scale_slide_easing(),
        _ => CrossfadeEasing::Cubic,
    };
    let dropdown = gtk4::DropDown::builder()
        .model(&model)
        .selected(easing_index(current))
        .build();
    dropdown.set_valign(gtk4::Align::Center);
    {
        let settings = settings.clone();
        dropdown.connect_selected_notify(move |dropdown| {
            let easing = easing_from_index(dropdown.selected());
            match key {
                "zoom-fade-in-easing" => settings.set_zoom_fade_in_easing(easing),
                "zoom-fade-out-easing" => settings.set_zoom_fade_out_easing(easing),
                "crossfade-easing" => settings.set_crossfade_easing(easing),
                "horizontal-scale-slide-easing" => {
                    settings.set_horizontal_scale_slide_easing(easing)
                }
                "vertical-scale-slide-easing" => {
                    settings.set_vertical_scale_slide_easing(easing)
                }
                _ => {}
            }
        });
    }
    row.add_suffix(&dropdown);
    row
}

fn transition_duration_row(settings: &Settings, title: &str) -> adw::SpinRow {
    let adjustment = gtk4::Adjustment::new(
        settings.crossfade_duration_ms() as f64,
        0.0,
        1000.0,
        50.0,
        100.0,
        0.0,
    );
    let row = adw::SpinRow::new(Some(&adjustment), 50.0, 0);
    row.set_title(title);
    {
        let settings = settings.clone();
        adjustment.connect_value_changed(move |adjustment| {
            let value = adjustment.value().round() as u32;
            settings.set_crossfade_duration_ms(value);
        });
    }
    row
}

fn slide_duration_row(
    settings: &Settings,
    title: &str,
    animation: TransitionAnimation,
) -> adw::SpinRow {
    let initial_value = match animation {
        TransitionAnimation::HorizontalSlide => settings.horizontal_slide_duration_ms(),
        TransitionAnimation::VerticalSlide => settings.vertical_slide_duration_ms(),
        TransitionAnimation::HorizontalFadeSlide => settings.horizontal_fade_slide_duration_ms(),
        TransitionAnimation::VerticalFadeSlide => settings.vertical_fade_slide_duration_ms(),
        TransitionAnimation::HorizontalScaleSlide => settings.horizontal_scale_slide_duration_ms(),
        TransitionAnimation::VerticalScaleSlide => settings.vertical_scale_slide_duration_ms(),
        _ => 500,
    };
    let adjustment = gtk4::Adjustment::new(initial_value as f64, 0.0, 1000.0, 50.0, 100.0, 0.0);
    let row = adw::SpinRow::new(Some(&adjustment), 50.0, 0);
    row.set_title(title);
    {
        let settings = settings.clone();
        adjustment.connect_value_changed(move |adjustment| {
            let value = adjustment.value().round() as u32;
            match animation {
                TransitionAnimation::HorizontalSlide => settings.set_horizontal_slide_duration_ms(value),
                TransitionAnimation::VerticalSlide => settings.set_vertical_slide_duration_ms(value),
                TransitionAnimation::HorizontalFadeSlide => settings.set_horizontal_fade_slide_duration_ms(value),
                TransitionAnimation::VerticalFadeSlide => settings.set_vertical_fade_slide_duration_ms(value),
                TransitionAnimation::HorizontalScaleSlide => settings.set_horizontal_scale_slide_duration_ms(value),
                TransitionAnimation::VerticalScaleSlide => settings.set_vertical_scale_slide_duration_ms(value),
                _ => {}
            }
        });
    }
    row
}

fn set_slide_duration_visibility(
    animation: TransitionAnimation,
    horizontal: &adw::SpinRow,
    vertical: &adw::SpinRow,
    horizontal_fade: &adw::SpinRow,
    vertical_fade: &adw::SpinRow,
    horizontal_scale: &adw::SpinRow,
    horizontal_scale_start: &adw::SpinRow,
    horizontal_scale_easing: &adw::ActionRow,
    vertical_scale: &adw::SpinRow,
    vertical_scale_start: &adw::SpinRow,
    vertical_scale_easing: &adw::ActionRow,
) {
    horizontal.set_visible(matches!(animation, TransitionAnimation::HorizontalSlide));
    vertical.set_visible(matches!(animation, TransitionAnimation::VerticalSlide));
    horizontal_fade.set_visible(matches!(animation, TransitionAnimation::HorizontalFadeSlide));
    vertical_fade.set_visible(matches!(animation, TransitionAnimation::VerticalFadeSlide));
    horizontal_scale.set_visible(matches!(animation, TransitionAnimation::HorizontalScaleSlide));
    let is_horizontal_scale = matches!(animation, TransitionAnimation::HorizontalScaleSlide);
    horizontal_scale_start.set_visible(is_horizontal_scale);
    horizontal_scale_easing.set_visible(is_horizontal_scale);
    vertical_scale.set_visible(matches!(animation, TransitionAnimation::VerticalScaleSlide));
    let is_vertical_scale = matches!(animation, TransitionAnimation::VerticalScaleSlide);
    vertical_scale_start.set_visible(is_vertical_scale);
    vertical_scale_easing.set_visible(is_vertical_scale);
}

fn zoom_duration_row(settings: &Settings, title: &str, key: &'static str) -> adw::SpinRow {
    let initial_value = match key {
        "zoom-fade-in-duration-ms" => settings.zoom_fade_in_duration_ms(),
        "zoom-fade-out-duration-ms" => settings.zoom_fade_out_duration_ms(),
        _ => 500,
    };
    let adjustment = gtk4::Adjustment::new(initial_value as f64, 0.0, 1000.0, 50.0, 100.0, 0.0);
    let row = adw::SpinRow::new(Some(&adjustment), 50.0, 0);
    row.set_title(title);
    {
        let settings = settings.clone();
        adjustment.connect_value_changed(move |adjustment| {
            let value = adjustment.value().round() as u32;
            match key {
                "zoom-fade-in-duration-ms" => settings.set_zoom_fade_in_duration_ms(value),
                "zoom-fade-out-duration-ms" => settings.set_zoom_fade_out_duration_ms(value),
                _ => {}
            }
        });
    }
    row
}

fn easing_index(easing: CrossfadeEasing) -> u32 {
    match easing {
        CrossfadeEasing::Back => 0,
        CrossfadeEasing::Bounce => 1,
        CrossfadeEasing::Circ => 2,
        CrossfadeEasing::Cubic => 3,
        CrossfadeEasing::Elastic => 4,
        CrossfadeEasing::Expo => 5,
        CrossfadeEasing::Linear => 6,
        CrossfadeEasing::Quad => 7,
        CrossfadeEasing::Quart => 8,
        CrossfadeEasing::Quint => 9,
        CrossfadeEasing::Sine => 10,
    }
}

fn easing_from_index(index: u32) -> CrossfadeEasing {
    match index {
        0 => CrossfadeEasing::Back,
        1 => CrossfadeEasing::Bounce,
        2 => CrossfadeEasing::Circ,
        4 => CrossfadeEasing::Elastic,
        5 => CrossfadeEasing::Expo,
        6 => CrossfadeEasing::Linear,
        7 => CrossfadeEasing::Quad,
        8 => CrossfadeEasing::Quart,
        9 => CrossfadeEasing::Quint,
        10 => CrossfadeEasing::Sine,
        _ => CrossfadeEasing::Cubic,
    }
}
