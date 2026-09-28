// loader.rs — фоновий потік для читання файлів мініатюр.
//
// Логіка: спочатку перевіряємо диск-кеш (якщо є — надсилаємо готові байти),
// інакше читаємо оригінал і надсилаємо для декодування + масштабування
// в головному потоці (Pixbuf не є Send).

use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::SystemTime;

pub struct ThumbRequest {
    pub index: usize,
    pub path: PathBuf,
    pub thumb_size: u32,
}

pub enum ThumbMsg {
    /// Кеш-попадання: готові PNG-байти (тільки декодувати).
    Cached {
        index: usize,
        bytes: Vec<u8>,
    },
    /// Кеш-промах: оригінальні байти + ключ кешу для збереження після масштабування.
    Original {
        index: usize,
        path: PathBuf,
        bytes: Vec<u8>,
        cache_key: String,
    },
    Failed {
        index: usize,
    },
}

pub fn request_thumb(req: ThumbRequest, sender: Sender<ThumbMsg>) {
    std::thread::spawn(move || {
        let ThumbRequest {
            index,
            path,
            thumb_size,
        } = req;

        let meta = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => {
                let _ = sender.send(ThumbMsg::Failed { index });
                return;
            }
        };

        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let file_size = meta.len();

        let key = super::disk_cache::cache_key(&path, mtime, file_size, thumb_size);
        let cached = super::disk_cache::cache_path(&key);

        if let Ok(bytes) = std::fs::read(&cached) {
            let _ = sender.send(ThumbMsg::Cached { index, bytes });
            return;
        }

        match std::fs::read(&path) {
            Ok(bytes) => {
                let _ = sender.send(ThumbMsg::Original {
                    index,
                    path,
                    bytes,
                    cache_key: key,
                });
            }
            Err(_) => {
                let _ = sender.send(ThumbMsg::Failed { index });
            }
        }
    });
}
