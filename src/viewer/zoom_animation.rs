use std::time::{Duration, Instant};

/// Осередок плавного масштабування для переглядача зображень.
///
/// Модуль зберігає два стани: поточне значення, яке відображається, і
/// цільове значення, до якого анімація має дійти. Коли користувач прокручує
/// колесо, він змінює лише цільове значення; кожен кадр інтерполюється
/// поточне значення в бік цілі.
#[derive(Debug, Clone, Copy)]
pub struct ZoomAnimation {
    pub current_scale: f64,
    pub current_offset_x: f64,
    pub current_offset_y: f64,
    pub target_scale: f64,
    pub target_offset_x: f64,
    pub target_offset_y: f64,
    pub enabled: bool,
    pub duration: Duration,
    pub epsilon: f64,
    transition_duration: Duration,
    easing: ZoomEasing,
    start_scale: f64,
    start_offset_x: f64,
    start_offset_y: f64,
    started_at: Option<Instant>,
}

/// Криві інтерполяції масштабу.
#[derive(Debug, Clone, Copy)]
pub enum ZoomEasing {
    OutQuint,
    InOutQuad,
}

impl ZoomAnimation {
    pub fn new(scale: f64, offset_x: f64, offset_y: f64) -> Self {
        Self {
            current_scale: scale,
            current_offset_x: offset_x,
            current_offset_y: offset_y,
            target_scale: scale,
            target_offset_x: offset_x,
            target_offset_y: offset_y,
            enabled: true,
            duration: Duration::from_millis(180),
            epsilon: 1e-4,
            transition_duration: Duration::from_millis(180),
            easing: ZoomEasing::OutQuint,
            start_scale: scale,
            start_offset_x: offset_x,
            start_offset_y: offset_y,
            started_at: None,
        }
    }

    pub fn set_target(&mut self, scale: f64, offset_x: f64, offset_y: f64) {
        self.easing = ZoomEasing::OutQuint;
        self.transition_duration = self.duration;
        self.animate_to(scale, offset_x, offset_y);
    }

    /// Запускає анімацію з окремою кривою інтерполяції.
    pub fn set_target_with_easing(
        &mut self,
        scale: f64,
        offset_x: f64,
        offset_y: f64,
        duration: Duration,
        easing: ZoomEasing,
    ) {
        self.easing = easing;
        self.transition_duration = duration;
        self.animate_to(scale, offset_x, offset_y);
    }

    pub fn animate_to(&mut self, scale: f64, offset_x: f64, offset_y: f64) {
        self.target_scale = scale;
        self.target_offset_x = offset_x;
        self.target_offset_y = offset_y;

        if !self.enabled {
            self.current_scale = scale;
            self.current_offset_x = offset_x;
            self.current_offset_y = offset_y;
            self.start_scale = scale;
            self.start_offset_x = offset_x;
            self.start_offset_y = offset_y;
            self.started_at = None;
            return;
        }

        self.start_scale = self.current_scale;
        self.start_offset_x = self.current_offset_x;
        self.start_offset_y = self.current_offset_y;
        self.started_at = Some(Instant::now());
    }

    pub fn sync_from_state(&mut self, scale: f64, offset_x: f64, offset_y: f64) {
        self.current_scale = scale;
        self.current_offset_x = offset_x;
        self.current_offset_y = offset_y;
        self.target_scale = scale;
        self.target_offset_x = offset_x;
        self.target_offset_y = offset_y;
        self.start_scale = scale;
        self.start_offset_x = offset_x;
        self.start_offset_y = offset_y;
        self.started_at = None;
    }

    pub fn finish(&mut self) {
        self.current_scale = self.target_scale;
        self.current_offset_x = self.target_offset_x;
        self.current_offset_y = self.target_offset_y;
        self.start_scale = self.current_scale;
        self.start_offset_x = self.current_offset_x;
        self.start_offset_y = self.current_offset_y;
        self.started_at = None;
    }

    pub fn pan_by(&mut self, dx: f64, dy: f64) {
        self.current_offset_x += dx;
        self.current_offset_y += dy;
        self.target_offset_x = self.current_offset_x;
        self.target_offset_y = self.current_offset_y;
        self.start_offset_x = self.current_offset_x;
        self.start_offset_y = self.current_offset_y;

        if (self.current_scale - self.target_scale).abs() <= self.epsilon {
            self.started_at = None;
        }
    }

    pub fn update(&mut self, now: Instant) -> bool {
        if !self.enabled {
            self.current_scale = self.target_scale;
            self.current_offset_x = self.target_offset_x;
            self.current_offset_y = self.target_offset_y;
            self.started_at = None;
            return false;
        }

        let Some(started_at) = self.started_at else {
            self.current_scale = self.target_scale;
            self.current_offset_x = self.target_offset_x;
            self.current_offset_y = self.target_offset_y;
            return false;
        };

        let elapsed = now.saturating_duration_since(started_at);
        if elapsed >= self.transition_duration {
            self.current_scale = self.target_scale;
            self.current_offset_x = self.target_offset_x;
            self.current_offset_y = self.target_offset_y;
            self.started_at = None;
            return false;
        }

        let progress = (elapsed.as_secs_f64() / self.transition_duration.as_secs_f64())
            .clamp(0.0, 1.0);
        let eased = match self.easing {
            ZoomEasing::OutQuint => Self::ease_out_quint(progress),
            ZoomEasing::InOutQuad => Self::ease_in_out_quad(progress),
        };
        self.current_scale = lerp(self.start_scale, self.target_scale, eased);
        self.current_offset_x = lerp(self.start_offset_x, self.target_offset_x, eased);
        self.current_offset_y = lerp(self.start_offset_y, self.target_offset_y, eased);

        if (self.current_scale - self.target_scale).abs() <= self.epsilon
            && (self.current_offset_x - self.target_offset_x).abs() <= self.epsilon
            && (self.current_offset_y - self.target_offset_y).abs() <= self.epsilon
        {
            self.current_scale = self.target_scale;
            self.current_offset_x = self.target_offset_x;
            self.current_offset_y = self.target_offset_y;
            self.started_at = None;
            return false;
        }

        true
    }

    fn ease_out_quint(progress: f64) -> f64 {
        1.0 - (1.0 - progress).powi(5)
    }

    fn ease_in_out_quad(progress: f64) -> f64 {
        if progress < 0.5 {
            2.0 * progress * progress
        } else {
            1.0 - (-2.0 * progress + 2.0).powi(2) / 2.0
        }
    }

    pub fn is_active(&self) -> bool {
        self.started_at.is_some()
            && ((self.current_scale - self.target_scale).abs() > self.epsilon
                || (self.current_offset_x - self.target_offset_x).abs() > self.epsilon
                || (self.current_offset_y - self.target_offset_y).abs() > self.epsilon)
    }
}

fn lerp(start: f64, end: f64, t: f64) -> f64 {
    start + (end - start) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changing_target_keeps_animation_active() {
        let mut zoom = ZoomAnimation::new(1.0, 0.0, 0.0);
        zoom.set_target(1.5, 10.0, 20.0);
        assert!(zoom.is_active());

        zoom.set_target(2.0, 15.0, 25.0);
        assert!(zoom.is_active());
    }

    #[test]
    fn update_moves_toward_target() {
        let mut zoom = ZoomAnimation::new(1.0, 0.0, 0.0);
        zoom.set_target(2.0, 20.0, 30.0);
        let now = Instant::now();
        let later = now + Duration::from_millis(90);
        zoom.update(later);
        assert!(zoom.current_scale > 1.0);
        assert!(zoom.current_scale < 2.0);
    }
}
