// disk_cache.rs — адреси та ключі кешу мініатюр на диску.
// Каталог: $XDG_CACHE_HOME/imgviewer/thumbnails/  (або ~/.cache/…)
// Ключ:    FNV-1a 64-bit хеш від (шлях : mtime : розмір файлу : thumb_size).

use std::path::{Path, PathBuf};

fn fnv1a_64(data: &[u8]) -> u64 {
    let mut hash: u64 = 14695981039346656037;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}

/// Унікальний ключ мініатюри для комбінації (шлях, mtime, розмір, thumb_size).
pub fn cache_key(path: &Path, mtime_secs: u64, file_size: u64, thumb_size: u32) -> String {
    let s = format!("{}:{}:{}:{}", path.display(), mtime_secs, file_size, thumb_size);
    format!("{:016x}", fnv1a_64(s.as_bytes()))
}

fn cache_dir() -> PathBuf {
    let base = std::env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::var("HOME")
                .map(|h| PathBuf::from(h).join(".cache"))
                .unwrap_or_else(|_| PathBuf::from("/tmp"))
        });
    base.join("imgviewer").join("thumbnails")
}

pub fn cache_path(key: &str) -> PathBuf {
    cache_dir().join(format!("{}.png", key))
}

pub fn ensure_cache_dir() -> std::io::Result<()> {
    std::fs::create_dir_all(cache_dir())
}
