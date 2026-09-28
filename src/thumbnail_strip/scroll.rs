use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

const WHEEL_SCROLL_TICK_MS: u64 = 16;
const WHEEL_SCROLL_RAMP_MS: f64 = 220.0;
const WHEEL_SCROLL_STEP_MIN: f64 = 0.10;
const WHEEL_SCROLL_STEP_MAX: f64 = 0.22;
const WHEEL_SCROLL_EPSILON: f64 = 0.5;
const AUTO_SCROLL_MARGIN_RATIO: f64 = 0.18;
const AUTO_SCROLL_MIN_MARGIN_ITEMS: f64 = 1.5;

#[derive(Clone, Default)]
pub(super) struct ScrollAnimator {
    animation: Rc<RefCell<Option<glib::SourceId>>>,
    target_x: Rc<RefCell<Option<f64>>>,
    started_at: Rc<RefCell<Option<Instant>>>,
}

impl ScrollAnimator {
    pub(super) fn set_target(&self, target: f64) {
        *self.target_x.borrow_mut() = Some(target);
        *self.started_at.borrow_mut() = Some(Instant::now());
    }

    pub(super) fn start(&self, adjustment: &gtk4::Adjustment) {
        if let Some(id) = self.animation.borrow_mut().take() {
            id.remove();
        }

        let Some(target) = *self.target_x.borrow() else {
            return;
        };
        let start_value = adjustment.value();
        if (target - start_value).abs() < WHEEL_SCROLL_EPSILON {
            adjustment.set_value(target);
            *self.target_x.borrow_mut() = None;
            return;
        }

        let adjustment = adjustment.clone();
        let animation = self.animation.clone();
        let target_x = self.target_x.clone();
        let started_at = self.started_at.clone();
        let start_time = Instant::now();
        let source =
            glib::timeout_add_local(Duration::from_millis(WHEEL_SCROLL_TICK_MS), move || {
                let Some(target) = *target_x.borrow() else {
                    *animation.borrow_mut() = None;
                    return glib::ControlFlow::Break;
                };

                let current = adjustment.value();
                let distance = target - current;
                if distance.abs() < WHEEL_SCROLL_EPSILON {
                    adjustment.set_value(target);
                    *target_x.borrow_mut() = None;
                    *started_at.borrow_mut() = None;
                    *animation.borrow_mut() = None;
                    return glib::ControlFlow::Break;
                }

                let elapsed = started_at
                    .borrow()
                    .as_ref()
                    .map(|started| started.elapsed())
                    .unwrap_or_else(|| start_time.elapsed());
                let progress = (elapsed.as_millis() as f64 / WHEEL_SCROLL_RAMP_MS).clamp(0.0, 1.0);
                let eased = ease_out_cubic(progress);
                let step =
                    WHEEL_SCROLL_STEP_MIN + (WHEEL_SCROLL_STEP_MAX - WHEEL_SCROLL_STEP_MIN) * eased;
                adjustment.set_value(current + distance * step);
                glib::ControlFlow::Continue
            });
        *self.animation.borrow_mut() = Some(source);
    }
}

pub(super) fn scroll_to_index(
    scroll: &gtk4::ScrolledWindow,
    index: usize,
    thumb_size: u32,
    spacing: i32,
    animator: &ScrollAnimator,
) {
    let item_width = (thumb_size as i32 + spacing) as f64;
    scroll_to_bounds(scroll, item_width * index as f64, item_width, animator);
}

pub(super) fn scroll_to_bounds(
    scroll: &gtk4::ScrolledWindow,
    item_start: f64,
    item_width: f64,
    animator: &ScrollAnimator,
) {
    let adjustment = scroll.hadjustment();
    let view_width = adjustment.page_size().max(scroll.width() as f64).max(1.0);
    let item_end = item_start + item_width;
    let current_left = adjustment.value();
    let current_right = current_left + view_width;
    let margin =
        (view_width * AUTO_SCROLL_MARGIN_RATIO).max(item_width * AUTO_SCROLL_MIN_MARGIN_ITEMS);
    let safe_left = current_left + margin;
    let safe_right = current_right - margin;
    if item_start >= safe_left && item_end <= safe_right {
        return;
    }

    let target = if item_start < safe_left {
        item_start - margin
    } else {
        item_end + margin - view_width
    };
    let max = (adjustment.upper() - adjustment.page_size()).max(adjustment.lower());
    animator.set_target(target.clamp(adjustment.lower(), max));
    animator.start(&adjustment);
}

fn ease_out_cubic(progress: f64) -> f64 {
    1.0 - (1.0 - progress).powi(3)
}

#[cfg(test)]
mod tests {
    use super::ease_out_cubic;

    #[test]
    fn cubic_easing_preserves_endpoints() {
        assert_eq!(ease_out_cubic(0.0), 0.0);
        assert_eq!(ease_out_cubic(1.0), 1.0);
    }

    #[test]
    fn cubic_easing_accelerates_toward_target() {
        assert!(ease_out_cubic(0.5) > 0.5);
    }
}
