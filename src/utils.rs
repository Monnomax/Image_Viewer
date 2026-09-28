// utils.rs — допоміжні функції, не пов'язані напряму з жодним конкретним
// модулем пайплайну: розпізнавання розширень, сканування каталогу,
// розбір аргументу командного рядка.

use natural_sort::compare;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

// ".avif" додається до списку лише коли увімкнена фіча "avif" (за
// замовчуванням — увімкнена, див. Cargo.toml): декодування avif у
// image_loader.rs залежить від фічі крейта `image` "avif-native", яка
// лінкується проти системної libdav1d. Без цієї фічі (наприклад,
// `--no-default-features`) файли .avif краще одразу не показувати в
// списку зображень каталогу, ніж показувати і провалюватись при спробі
// відкрити.
#[cfg(feature = "avif")]
const IMAGE_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "bmp", "webp", "tif", "tiff", "ico", "pnm", "avif",
];
#[cfg(not(feature = "avif"))]
const IMAGE_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "bmp", "webp", "tif", "tiff", "ico", "pnm",
];

pub fn is_image_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| IMAGE_EXTS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

pub fn collect_images_in_dir(dir: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && is_image_file(&path) {
                result.push(path);
            }
        }
    }
    result.sort_by(|a, b| compare_names(a, b));
    result
}

/// Природне порівняння імен файлів (з урахуванням чисел усередині назви),
/// використовується і для початкового сканування каталогу, і для пункту
/// "Упорядкування" контекстного меню.
pub fn compare_names(a: &Path, b: &Path) -> std::cmp::Ordering {
    let name_a = a.file_name().unwrap_or_default().to_string_lossy();
    let name_b = b.file_name().unwrap_or_default().to_string_lossy();
    compare(&name_a, &name_b)
}

/// Час останньої зміни файлу. Якщо метадані недоступні — повертає
/// SystemTime::UNIX_EPOCH, щоб такі файли осідали на початку списку.
pub fn file_modified(path: &Path) -> SystemTime {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

/// Розмір файлу в байтах. Недоступні метадані трактуються як 0.
pub fn file_size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// Розширення файлу в нижньому регістрі (для впорядкування за типом).
pub fn file_ext(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default()
}

pub fn canonical_path_key(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::canonical_path_key;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn canonical_path_key_uses_same_value_for_equivalent_paths() {
        let temp_dir =
            std::env::temp_dir().join(format!("imgviewer-path-key-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let previous = std::env::current_dir().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let file = temp_dir.join("test-image.png");
        fs::write(&file, b"png").unwrap();

        let relative = PathBuf::from("test-image.png");
        let canonical = canonical_path_key(&file);
        let same = canonical_path_key(relative.as_path());

        assert_ne!(relative.to_string_lossy(), canonical);
        assert_eq!(canonical, same);

        std::env::set_current_dir(previous).unwrap();
    }
}

/// Визначає список зображень і початковий індекс за аргументом командного
/// рядка: це може бути файл (тоді сканується його каталог і шукається сам
/// файл у списку) або каталог (тоді показуються всі зображення в ньому).
pub fn resolve_path_arg(path_arg: Option<String>) -> (Vec<PathBuf>, usize) {
    match path_arg {
        Some(arg) => {
            let p = PathBuf::from(&arg);
            if p.is_dir() {
                (collect_images_in_dir(&p), 0)
            } else if p.is_file() {
                let dir = p
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from("."));
                let images = collect_images_in_dir(&dir);
                let canon = std::fs::canonicalize(&p).unwrap_or_else(|_| p.clone());
                let idx = images
                    .iter()
                    .position(|x| std::fs::canonicalize(x).unwrap_or_else(|_| x.clone()) == canon)
                    .unwrap_or(0);
                (images, idx)
            } else {
                eprintln!("Шлях не знайдено: {}", arg);
                (Vec::new(), 0)
            }
        }
        None => {
            eprintln!("Використання: imgviewer <файл-зображення|каталог>");
            (Vec::new(), 0)
        }
    }
}