// viewer/crossfade.rs — стан переходу-розчинення (crossfade) між
// зображеннями при навігації.
//
// На відміну від ZoomAnimation (там анімується сам scale/offset — тобто
// "куди рухається" зображення), тут анімується лише прозорість: старе
// зображення тане з opacity 1→0, нове одночасно проявляється з opacity
// 0→1, кожне лишається на своїй власній позиції вписування (image fit).
// Тобто це суто альфа-розчинення, без "морфінгу" позиції/масштабу між
// кадрами — так само, як Crossfade реалізований у більшості переглядачів
// зображень (gThumb, Eye of GNOME тощо).
//
// Стан старого зображення (шлях + scale/offset, з яким воно було
// вписано в екран на момент початку переходу) навмисно "заморожується"
// тут окремо від Viewer::render_scale/render_offset_*, бо ці поля одразу
// переписуються під нове зображення в fit_to_screen().

use std::path::PathBuf;
use std::time::{Duration, Instant};

pub struct Crossfade {
    /// Шлях і "заморожений" стан вписування зображення, яке зникає.
    /// `None`, коли перехід неактивний.
    pub old_path: Option<PathBuf>,
    pub old_scale: f64,
    pub old_offset_x: f64,
    pub old_offset_y: f64,
    pub slide_direction: isize,

    fade_out_duration: Duration,
    fade_in_duration: Duration,
    total_duration: Duration,
    started_at: Option<Instant>,
}

impl Crossfade {
    pub fn new() -> Self {
        Self {
            old_path: None,
            old_scale: 1.0,
            old_offset_x: 0.0,
            old_offset_y: 0.0,
            slide_direction: 1,
            fade_out_duration: Duration::from_millis(250),
            fade_in_duration: Duration::from_millis(250),
            total_duration: Duration::from_millis(250),
            started_at: None,
        }
    }

    /// Запускає перехід від зображення, яке щойно було на екрані
    /// (`old_path` з його scale/offset на момент навігації), до того, що
    /// вже є (чи ось-ось стане) поточним у Viewer.
    pub fn start(
        &mut self,
        old_path: PathBuf,
        old_scale: f64,
        old_offset_x: f64,
        old_offset_y: f64,
        slide_direction: isize,
        fade_out_duration: Duration,
        fade_in_duration: Duration,
    ) {
        self.old_path = Some(old_path);
        self.old_scale = old_scale;
        self.old_offset_x = old_offset_x;
        self.old_offset_y = old_offset_y;
        self.slide_direction = if slide_direction < 0 { -1 } else { 1 };
        self.fade_out_duration = fade_out_duration;
        self.fade_in_duration = fade_in_duration;
        self.total_duration = fade_out_duration.max(fade_in_duration);
        self.started_at = Some(Instant::now());
    }

    /// Негайно перериває перехід (напр. при вимкненні crossfade у
    /// налаштуваннях чи при видаленні/перейменуванні поточного файлу —
    /// старий кадр більше не має сенсу домальовувати).
    pub fn cancel(&mut self) {
        self.old_path = None;
        self.started_at = None;
    }

    pub fn is_active(&self) -> bool {
        self.started_at.is_some()
    }

    pub fn fade_out_progress(&self, now: Instant) -> f64 {
        self.progress_for_duration(now, self.fade_out_duration)
    }

    pub fn fade_in_progress(&self, now: Instant) -> f64 {
        self.progress_for_duration(now, self.fade_in_duration)
    }

    fn progress_for_duration(&self, now: Instant, duration: Duration) -> f64 {
        let Some(started_at) = self.started_at else {
            return 1.0;
        };
        if duration.is_zero() {
            return 1.0;
        }
        let elapsed = now.saturating_duration_since(started_at);
        (elapsed.as_secs_f64() / duration.as_secs_f64()).clamp(0.0, 1.0)
    }

    /// Прогрес переходу 0.0..=1.0 на момент `now` (0 — щойно почався, 1 —
    /// завершений), у лінійному часі. Яку саме криву використати для
    /// візуальної інтерполяції (наприклад, linear або ease-in-out)
    /// визначає рендерер за поточними налаштуваннями.
    pub fn progress(&self, now: Instant) -> f64 {
        let Some(started_at) = self.started_at else {
            return 1.0;
        };
        if self.total_duration.is_zero() {
            return 1.0;
        }
        let elapsed = now.saturating_duration_since(started_at);
        (elapsed.as_secs_f64() / self.total_duration.as_secs_f64()).clamp(0.0, 1.0)
    }

    /// Просуває стан переходу на момент `now`. Повертає `true`, якщо цей
    /// кадр (перехідний чи фінальний) усе ще потребує перемальовування.
    /// Коли прогрес досягає 1.0, перехід завершується тут-таки (старий
    /// кадр більше не малюється жодного разу з opacity=0 — нема сенсу).
    pub fn update(&mut self, now: Instant) -> bool {
        if self.started_at.is_none() {
            return false;
        }
        if self.progress(now) >= 1.0 {
            self.old_path = None;
            self.started_at = None;
        }
        true
    }
}

impl Default for Crossfade {
    fn default() -> Self {
        Self::new()
    }
}
