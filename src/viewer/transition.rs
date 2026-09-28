// viewer/transition.rs — обчислює параметри анімації переходу між
// зображеннями. Модуль не малює GTK-вузли: renderer.rs лише застосовує
// готовий стан до старої та нової текстур.

use super::Viewer;
use crate::preferences::settings::{CrossfadeEasing, TransitionAnimation};

#[derive(Clone, Copy)]
pub(super) struct TransitionState {
    pub(super) old_opacity: f64,
    pub(super) new_opacity: f64,
    pub(super) old_shift_x: f64,
    pub(super) new_shift_x: f64,
    pub(super) old_shift_y: f64,
    pub(super) new_shift_y: f64,
    pub(super) old_scale_mul: f64,
    pub(super) new_scale_mul: f64,
}

pub(super) fn calculate_transition(
    viewer: &Viewer,
    width: f32,
    height: f32,
) -> Option<TransitionState> {
    if !viewer.crossfade.is_active() {
        return None;
    }

    let now = std::time::Instant::now();
    let fade_out_t = viewer.crossfade.fade_out_progress(now);
    let fade_in_t = viewer.crossfade.fade_in_progress(now);
    let slide_t = viewer.crossfade.progress(now);
    let mut old_opacity = 1.0 - fade_out_t;
    let mut new_opacity = fade_in_t;
    let (old_shift_x, new_shift_x, old_shift_y, new_shift_y, old_scale_mul, new_scale_mul) =
        match viewer.settings.transition_animation() {
            TransitionAnimation::HorizontalSlide => {
                old_opacity = 1.0;
                new_opacity = 1.0;
                let shift = width as f64 * slide_t;
                if viewer.crossfade.slide_direction < 0 {
                    (shift, -width as f64 + shift, 0.0, 0.0, 1.0, 1.0)
                } else {
                    (-shift, width as f64 - shift, 0.0, 0.0, 1.0, 1.0)
                }
            }
            TransitionAnimation::HorizontalScaleSlide => {
                old_opacity = 1.0;
                new_opacity = 1.0;
                let scale_t =
                    apply_easing(slide_t, viewer.settings.horizontal_scale_slide_easing());
                let start_scale = viewer
                    .settings
                    .horizontal_scale_slide_start_scale()
                    .clamp(0.0, 1.0);
                let shift = width as f64 * slide_t;
                let old_scale = 1.0 - (1.0 - start_scale) * scale_t;
                let new_scale = start_scale + (1.0 - start_scale) * scale_t;
                if viewer.crossfade.slide_direction < 0 {
                    (shift, -width as f64 + shift, 0.0, 0.0, old_scale, new_scale)
                } else {
                    (-shift, width as f64 - shift, 0.0, 0.0, old_scale, new_scale)
                }
            }
            TransitionAnimation::VerticalScaleSlide => {
                old_opacity = 1.0;
                new_opacity = 1.0;
                let scale_t = apply_easing(slide_t, viewer.settings.vertical_scale_slide_easing());
                let start_scale = viewer
                    .settings
                    .vertical_scale_slide_start_scale()
                    .clamp(0.5, 1.0);
                let shift = height as f64 * slide_t;
                let old_scale = 1.0 - (1.0 - start_scale) * scale_t;
                let new_scale = start_scale + (1.0 - start_scale) * scale_t;
                if viewer.crossfade.slide_direction < 0 {
                    (
                        0.0,
                        0.0,
                        shift,
                        -height as f64 + shift,
                        old_scale,
                        new_scale,
                    )
                } else {
                    (
                        0.0,
                        0.0,
                        -shift,
                        height as f64 - shift,
                        old_scale,
                        new_scale,
                    )
                }
            }
            TransitionAnimation::HorizontalFadeSlide => {
                let shift = width as f64 * slide_t;
                if viewer.crossfade.slide_direction < 0 {
                    (shift, -width as f64 + shift, 0.0, 0.0, 1.0, 1.0)
                } else {
                    (-shift, width as f64 - shift, 0.0, 0.0, 1.0, 1.0)
                }
            }
            TransitionAnimation::VerticalSlide => {
                old_opacity = 1.0;
                new_opacity = 1.0;
                let shift = height as f64 * slide_t;
                if viewer.crossfade.slide_direction < 0 {
                    (0.0, 0.0, shift, -height as f64 + shift, 1.0, 1.0)
                } else {
                    (0.0, 0.0, -shift, height as f64 - shift, 1.0, 1.0)
                }
            }
            TransitionAnimation::VerticalFadeSlide => {
                let shift = height as f64 * slide_t;
                if viewer.crossfade.slide_direction < 0 {
                    (0.0, 0.0, shift, -height as f64 + shift, 1.0, 1.0)
                } else {
                    (0.0, 0.0, -shift, height as f64 - shift, 1.0, 1.0)
                }
            }
            TransitionAnimation::Crossfade => {
                let easing = viewer.settings.crossfade_easing();
                old_opacity = 1.0 - apply_easing(fade_out_t, easing);
                new_opacity = apply_easing(fade_in_t, easing);
                (0.0, 0.0, 0.0, 0.0, 1.0, 1.0)
            }
            TransitionAnimation::ZoomFade => {
                let incoming_scale = viewer.settings.zoom_fade_in_scale().clamp(0.0, 1.0);
                let outgoing_scale = viewer.settings.zoom_fade_out_scale().clamp(0.0, 1.0);
                let incoming_t = apply_easing(fade_in_t, viewer.settings.zoom_fade_in_easing());
                let outgoing_t = apply_easing(fade_out_t, viewer.settings.zoom_fade_out_easing());
                old_opacity = 1.0 - outgoing_t;
                new_opacity = incoming_t;
                let new_zoom = 1.0 + incoming_scale * (1.0 - incoming_t);
                let old_zoom = 1.0 + outgoing_scale * outgoing_t;
                (0.0, 0.0, 0.0, 0.0, old_zoom, new_zoom)
            }
        };

    Some(TransitionState {
        old_opacity: old_opacity.clamp(0.0, 1.0),
        new_opacity: new_opacity.clamp(0.0, 1.0),
        old_shift_x,
        new_shift_x,
        old_shift_y,
        new_shift_y,
        old_scale_mul,
        new_scale_mul,
    })
}

fn apply_easing(progress: f64, easing: CrossfadeEasing) -> f64 {
    let t = progress.clamp(0.0, 1.0);
    match easing {
        CrossfadeEasing::Back => {
            let overshoot = 1.70158;
            let value = t * t * ((overshoot + 1.0) * t - overshoot);
            value.clamp(0.0, 1.0)
        }
        CrossfadeEasing::Bounce => {
            if t < 1.0 / 2.75 {
                7.5625 * t * t
            } else if t < 2.0 / 2.75 {
                let t = t - 1.5 / 2.75;
                7.5625 * t * t + 0.75
            } else if t < 2.5 / 2.75 {
                let t = t - 2.25 / 2.75;
                7.5625 * t * t + 0.9375
            } else {
                let t = t - 2.625 / 2.75;
                7.5625 * t * t + 0.984375
            }
        }
        CrossfadeEasing::Circ => 1.0 - (1.0 - t * t).sqrt(),
        CrossfadeEasing::Cubic => t * t * (3.0 - 2.0 * t),
        CrossfadeEasing::Elastic => {
            if t == 0.0 || t == 1.0 {
                t
            } else {
                let phase = (t - 0.075) * (2.0 * std::f64::consts::PI) / 0.3;
                (-(2.0_f64).powf(10.0 * (t - 1.0)) * phase.sin() + 1.0).clamp(0.0, 1.0)
            }
        }
        CrossfadeEasing::Expo => {
            if t == 0.0 {
                0.0
            } else {
                (2.0_f64).powf(10.0 * (t - 1.0))
            }
        }
        CrossfadeEasing::Linear => t,
        CrossfadeEasing::Quad => t * t,
        CrossfadeEasing::Quart => t * t * t * t,
        CrossfadeEasing::Quint => t * t * t * t * t,
        CrossfadeEasing::Sine => 1.0 - (t * std::f64::consts::FRAC_PI_2).cos(),
    }
    .clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f64 = 1e-12;
    const EASINGS: [CrossfadeEasing; 11] = [
        CrossfadeEasing::Back,
        CrossfadeEasing::Bounce,
        CrossfadeEasing::Circ,
        CrossfadeEasing::Cubic,
        CrossfadeEasing::Elastic,
        CrossfadeEasing::Expo,
        CrossfadeEasing::Linear,
        CrossfadeEasing::Quad,
        CrossfadeEasing::Quart,
        CrossfadeEasing::Quint,
        CrossfadeEasing::Sine,
    ];

    #[test]
    fn easing_preserves_endpoints() {
        for easing in EASINGS {
            assert!(
                apply_easing(0.0, easing).abs() < EPSILON,
                "{easing:?} must start at 0"
            );
            assert!(
                (apply_easing(1.0, easing) - 1.0).abs() < EPSILON,
                "{easing:?} must end at 1"
            );
        }
    }

    #[test]
    fn easing_clamps_input_and_output_to_unit_interval() {
        for easing in EASINGS {
            for progress in [-0.5, 0.0, 0.125, 0.5, 0.875, 1.0, 1.5] {
                let value = apply_easing(progress, easing);
                assert!(
                    (0.0..=1.0).contains(&value),
                    "{easing:?} returned {value} for progress {progress}"
                );
            }
        }
    }
}
