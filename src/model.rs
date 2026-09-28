// model.rs — список файлів зображень і поточна позиція перегляду.
// Жодної логіки масштабу/панорамування/кешування тут немає навмисно —
// лише "яка колекція файлів" і "який зараз індекс".

use crate::utils;
use std::path::PathBuf;

/// Режими впорядкування списку зображень (пункт "Упорядкування" контекстного меню).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortMode {
    /// А - Я
    NameAsc,
    /// Я - А
    NameDesc,
    /// Остання зміна (найновіші — спочатку)
    ModifiedDesc,
    /// Перша зміна (найстаріші — спочатку)
    ModifiedAsc,
    /// Розмір (за зростанням)
    Size,
    /// Тип (за розширенням файлу, потім за назвою)
    Type,
}

impl SortMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            SortMode::NameAsc => "NameAsc",
            SortMode::NameDesc => "NameDesc",
            SortMode::ModifiedDesc => "ModifiedDesc",
            SortMode::ModifiedAsc => "ModifiedAsc",
            SortMode::Size => "Size",
            SortMode::Type => "Type",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "NameDesc" => SortMode::NameDesc,
            "ModifiedDesc" => SortMode::ModifiedDesc,
            "ModifiedAsc" => SortMode::ModifiedAsc,
            "Size" => SortMode::Size,
            "Type" => SortMode::Type,
            _ => SortMode::NameAsc,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SortMode;
    use std::path::PathBuf;

    #[test]
    fn sort_mode_round_trip_uses_stable_strings() {
        let values = [
            SortMode::NameAsc,
            SortMode::NameDesc,
            SortMode::ModifiedDesc,
            SortMode::ModifiedAsc,
            SortMode::Size,
            SortMode::Type,
        ];

        for mode in values {
            assert_eq!(SortMode::from_str(mode.as_str()), mode);
        }
    }

    #[test]
    fn neighbor_paths_extended_wraps_and_dedupes() {
        let images: Vec<PathBuf> = (0..5).map(|i| PathBuf::from(format!("{i}.png"))).collect();
        let model = super::Model::new(images, 0);

        // forward=3, backward=3 у списку з 5 елементів: без поточного
        // лишається лише 4 унікальні сусіди, повторів бути не повинно.
        let result = model.neighbor_paths_extended(3, 3);
        let unique: std::collections::HashSet<_> = result.iter().collect();
        assert_eq!(result.len(), unique.len());
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn neighbor_paths_extended_empty_for_single_image() {
        let model = super::Model::new(vec![PathBuf::from("only.png")], 0);
        assert!(model.neighbor_paths_extended(5, 5).is_empty());
    }

    #[test]
    fn cyclic_distance_picks_shorter_direction() {
        let images: Vec<PathBuf> = (0..6).map(|i| PathBuf::from(format!("{i}.png"))).collect();
        let model = super::Model::new(images, 0);
        // Індекс 5 у списку з 6 елементів: вперед — 5 кроків, назад — 1.
        assert_eq!(model.cyclic_distance(&PathBuf::from("5.png")), Some(1));
        assert_eq!(model.cyclic_distance(&PathBuf::from("2.png")), Some(2));
        assert_eq!(model.cyclic_distance(&PathBuf::from("missing.png")), None);
    }
}

pub struct Model {
    pub images: Vec<PathBuf>,
    pub index: usize,
}

impl Model {
    pub fn new(images: Vec<PathBuf>, index: usize) -> Self {
        Self { images, index }
    }

    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }

    pub fn current_path(&self) -> Option<PathBuf> {
        self.images.get(self.index).cloned()
    }

    /// Шляхи до попереднього і наступного зображень (для попереднього
    /// завантаження в кеш), з урахуванням зациклення списку.
    pub fn neighbor_paths(&self) -> (Option<PathBuf>, Option<PathBuf>) {
        let len = self.images.len();
        if len < 2 {
            return (None, None);
        }
        let prev = self.images[(self.index + len - 1) % len].clone();
        let next = self.images[(self.index + 1) % len].clone();
        (Some(prev), Some(next))
    }

    /// Повертає до `forward` наступних і до `backward` попередніх зображень
    /// (із зацикленням списку), без повторів і без самого поточного
    /// зображення. На відміну від `neighbor_paths` (завжди рівно 1 з
    /// кожного боку), використовується налаштуванням "Попереднє
    /// завантаження зображень" — кількість береться з `PreloadMode`.
    pub fn neighbor_paths_extended(&self, forward: usize, backward: usize) -> Vec<PathBuf> {
        let len = self.images.len();
        if len < 2 {
            return Vec::new();
        }
        let max_forward = forward.min(len - 1);
        let max_backward = backward.min(len - 1);
        let steps = max_forward.max(max_backward);

        let mut seen = std::collections::HashSet::new();
        seen.insert(self.index);
        let mut result = Vec::new();

        for d in 1..=steps {
            if d <= max_forward {
                let idx = (self.index + d) % len;
                if seen.insert(idx) {
                    result.push(self.images[idx].clone());
                }
            }
            if d <= max_backward {
                let idx = (self.index + len - d) % len;
                if seen.insert(idx) {
                    result.push(self.images[idx].clone());
                }
            }
        }

        result
    }

    /// Циклічна відстань (у кроках навігації) від поточного зображення до
    /// файлу за шляхом `path`. `None`, якщо такого шляху немає у списку.
    /// Використовується для вирішення, чи файл достатньо "близько", щоб
    /// одразу готувати для нього GPU-текстуру (налаштування "Попередньо
    /// створювати текстури").
    pub fn cyclic_distance(&self, path: &PathBuf) -> Option<usize> {
        let len = self.images.len();
        if len == 0 {
            return None;
        }
        let idx = self.images.iter().position(|p| p == path)?;
        let diff = idx.abs_diff(self.index);
        Some(diff.min(len - diff))
    }

    /// Прибирає поточний файл зі списку (використовується після успішного
    /// видалення файлу з диска) і повертає його шлях. Індекс після цього
    /// вказує на наступне зображення (або зсувається на останнє, якщо
    /// видалили останній елемент списку).
    pub fn remove_current(&mut self) -> Option<PathBuf> {
        if self.images.is_empty() {
            return None;
        }
        let removed = self.images.remove(self.index);
        if self.index >= self.images.len() && !self.images.is_empty() {
            self.index = self.images.len() - 1;
        }
        Some(removed)
    }

    /// Переміщує індекс на `delta` позицій із зацикленням.
    pub fn navigate(&mut self, delta: isize) {
        if self.images.is_empty() {
            return;
        }
        let len = self.images.len() as isize;
        let mut idx = self.index as isize + delta;
        idx = ((idx % len) + len) % len;
        self.index = idx as usize;
    }

    /// Переупорядковує список зображень за обраним режимом. Поточне
    /// зображення (за шляхом) залишається відкритим — індекс перераховується
    /// після сортування, щоб перегляд не "стрибав" на інший файл.
    pub fn sort_by(&mut self, mode: SortMode) {
        let current = self.current_path();

        match mode {
            SortMode::NameAsc => self.images.sort_by(|a, b| utils::compare_names(a, b)),
            SortMode::NameDesc => self.images.sort_by(|a, b| utils::compare_names(b, a)),
            SortMode::ModifiedDesc => self
                .images
                .sort_by(|a, b| utils::file_modified(b).cmp(&utils::file_modified(a))),
            SortMode::ModifiedAsc => self
                .images
                .sort_by(|a, b| utils::file_modified(a).cmp(&utils::file_modified(b))),
            SortMode::Size => self
                .images
                .sort_by(|a, b| utils::file_size(a).cmp(&utils::file_size(b))),
            SortMode::Type => self.images.sort_by(|a, b| {
                let ext_a = utils::file_ext(a);
                let ext_b = utils::file_ext(b);
                ext_a.cmp(&ext_b).then_with(|| utils::compare_names(a, b))
            }),
        }

        if let Some(cur) = current {
            if let Some(idx) = self.images.iter().position(|p| *p == cur) {
                self.index = idx;
            }
        }
    }
}
