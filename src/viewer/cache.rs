// cache.rs — два кеші пайплайну:
//   ImageCache   — декодовані Pixbuf у пам'яті CPU;
//   TextureCache — GPU-текстури (gdk::Texture), створені з Pixbuf.
// Обидва — LRU з обмеженою місткістю: витісняється завжди запис,
// доступ до якого був НАЙДАВНІШЕ (а не той, що просто раніше за всіх
// вставлений — див. touch() нижче). Це важливо саме для навігації:
// при русі вперед-назад по каталогу нещодавно переглянуті зображення
// (в обидва боки) мають лишатись "гарячими" в кеші довше, ніж просто
// послідовність вставок дозволила б чистому FIFO.

use gtk4::gdk;
use gtk4::gdk_pixbuf::Pixbuf;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;

const CACHE_CAPACITY: usize = 12;

/// Кеш декодованих зображень (результат роботи ImageLoader).
pub struct ImageCache {
    map: HashMap<PathBuf, Rc<Pixbuf>>,
    order: VecDeque<PathBuf>,
}

impl ImageCache {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub fn get(&mut self, path: &PathBuf) -> Option<Rc<Pixbuf>> {
        let value = self.map.get(path).cloned();
        if value.is_some() {
            self.touch(path);
        }
        value
    }

    pub fn contains(&self, path: &PathBuf) -> bool {
        self.map.contains_key(path)
    }

    pub fn insert(&mut self, path: PathBuf, pixbuf: Pixbuf) {
        if !self.map.contains_key(&path) {
            self.order.push_back(path.clone());
            if self.order.len() > CACHE_CAPACITY {
                if let Some(old) = self.order.pop_front() {
                    self.map.remove(&old);
                }
            }
        } else {
            self.touch(&path);
        }
        self.map.insert(path, Rc::new(pixbuf));
    }

    /// Переносить `path` в кінець черги витіснення (найсвіжіше
    /// використання) — викликається і при читанні (`get`), і при
    /// повторній вставці вже наявного запису.
    fn touch(&mut self, path: &PathBuf) {
        if let Some(pos) = self.order.iter().position(|p| p == path) {
            if let Some(entry) = self.order.remove(pos) {
                self.order.push_back(entry);
            }
        }
    }

    /// Прибирає запис про файл, якого більше не існує (видалений з диска).
    pub fn remove(&mut self, path: &PathBuf) {
        self.map.remove(path);
        self.order.retain(|p| p != path);
    }
}

/// Кеш GPU-текстур. Текстура створюється з Pixbuf один раз і надалі
/// перевикористовується при кожному кадрі малювання (Renderer не повинен
/// завантажувати дані в GPU щоразу заново).
pub struct TextureCache {
    map: HashMap<PathBuf, gdk::Texture>,
    order: VecDeque<PathBuf>,
}

impl TextureCache {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    /// Повертає готову текстуру з кешу (без створення), якщо вона там
    /// уже є — використовується там, де промах кешу неприйнятний
    /// (наприклад, у `snapshot()` під час активного crossfade: якщо
    /// текстуру не вдалось "прогріти" заздалегідь, краще пропустити
    /// кадр і повторити спробу на наступному, ніж стопорити малювання
    /// синхронним аплоадом у GPU).
    pub fn get(&mut self, path: &PathBuf) -> Option<gdk::Texture> {
        let tex = self.map.get(path).cloned();
        if tex.is_some() {
            self.touch(path);
        }
        tex
    }

    /// Повертає готову текстуру або створює нову синхронно, якщо її ще
    /// немає в кеші. Створення (`gdk::Texture::for_pixbuf`) — це
    /// завантаження пікселів у GPU-пам'ять, вартість якого росте з
    /// розміром/вагою зображення; тому викликачі, чутливі до таймінгу
    /// кадру (переходи-анімації), мають заздалегідь "прогрівати" запис
    /// через цей самий метод ДО старту анімації (див.
    /// `Viewer::warm_texture`), а не покладатись на створення тут.
    pub fn get_or_create(&mut self, path: &PathBuf, pixbuf: &Pixbuf) -> gdk::Texture {
        if let Some(tex) = self.get(path) {
            return tex;
        }
        let texture = gdk::Texture::for_pixbuf(pixbuf);
        self.order.push_back(path.clone());
        if self.order.len() > CACHE_CAPACITY {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
        }
        self.map.insert(path.clone(), texture.clone());
        texture
    }

    /// Прибирає запис про файл, якого більше не існує (видалений з диска).
    pub fn remove(&mut self, path: &PathBuf) {
        self.map.remove(path);
        self.order.retain(|p| p != path);
    }

    fn touch(&mut self, path: &PathBuf) {
        if let Some(pos) = self.order.iter().position(|p| p == path) {
            if let Some(entry) = self.order.remove(pos) {
                self.order.push_back(entry);
            }
        }
    }
}
