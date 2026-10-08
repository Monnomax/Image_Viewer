// viewer/viewer.rs — стан перегляду: масштаб, зсув (панорамування), навігація
// між зображеннями та координація з ImageCache/TextureCache/ImageLoader.
// Це "мозок" застосунку: input.rs і app.rs викликають його методи,
// renderer.rs лише читає його стан для малювання.

use super::animated_gif::{AnimatedGifFrames, AnimatedGifPlayer};
use super::cache::{ImageCache, TextureCache};
use super::crossfade::Crossfade;
use super::image_loader::{self, DecodedFrame, LoadedImage, LoaderMsg};
use super::rotate::PendingRotation;
use super::view_state::{StoredViewState, ViewStateStore};
use super::{ZoomAnimation, ZoomEasing};
use crate::model::{Model, SortMode};
use crate::preferences::Settings;
use gtk4::gdk;
use gtk4::gdk_pixbuf::Pixbuf;
use gtk4::gio;
use gtk4::prelude::FileExt;
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

struct FrozenFrame {
    path: PathBuf,
    scale: f64,
    offset_x: f64,
    offset_y: f64,
    direction: isize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewState {
    pub scale: f64,
    pub offset_x: f64,
    pub offset_y: f64,
}

impl ViewState {
    fn new(scale: f64, offset_x: f64, offset_y: f64) -> Self {
        Self {
            scale,
            offset_x,
            offset_y,
        }
    }
}

pub struct Viewer {
    pub model: Model,
    pub image_cache: ImageCache,
    pub texture_cache: TextureCache,
    animated_gif_player: AnimatedGifPlayer,
    pub loading: HashSet<PathBuf>,
    pub loader_sender: Sender<LoaderMsg>,
    pub settings: Settings,

    view_state: ViewState,
    render_state: ViewState,
    pub is_100: bool,
    pub zoom_animation: ZoomAnimation,
    pub animation_enabled: bool,
    pub animation_duration_ms: u32,

    /// Стан переходу-розчинення між зображеннями (розділ "Анімації" →
    /// "Увімкнути Crossfade"). На відміну від zoom_animation, тут немає
    /// окремих enabled/duration-полів, синхронізованих через
    /// Settings::connect_changed — перевіряється й читається напряму з
    /// Settings у момент навігації/завантаження (це рідкісна подія, а не
    /// щокадровий виклик, тож зайвий стан-дублікат ні до чого).
    pub crossfade: Crossfade,
    /// "Заморожений" стан старого зображення, що чекає на запуск
    /// crossfade.start() — потрібен лише тоді, коли на момент навігації
    /// нове зображення ще НЕ було в кеші (тобто buде отримане пізніше,
    /// асинхронно, через handle_loaded()). Якщо ж воно вже кешоване,
    /// crossfade запускається одразу в navigate() і це поле лишається None.
    crossfade_pending_old: Option<FrozenFrame>,

    pub window_width: f64,
    pub window_height: f64,
    pub win_w: i32,
    pub win_h: i32,

    pub last_pointer: (f64, f64),
    pub drag_offset_start: (f64, f64),

    /// true, доки поточне зображення ще не вписано в екран уперше
    /// (наприклад, поки триває його асинхронне завантаження).
    pub needs_initial_fit: bool,

    /// Стан масштабу/позиції для поточної сесії (без запису у налаштування).
    view_state_store: ViewStateStore,

    /// Незбережений поворот поточного зображення (див. rotate.rs). Живе,
    /// доки саме цей `path` лишається поточним — комітиться в
    /// commit_pending_rotation() на початку navigate() і має так само
    /// комітитись при закритті переглядача (виклик з app.rs, поза цим
    /// файлом — переконайтесь, що він там доданий).
    pub pending_rotation: Option<PendingRotation>,
}

impl Viewer {
    pub fn render_view_state(&self) -> ViewState {
        self.render_state
    }

    pub fn current_view_state(&self) -> ViewState {
        self.view_state
    }

    fn sync_zoom_animation_to_render_state(&mut self) {
        self.zoom_animation.sync_from_state(
            self.render_state.scale,
            self.render_state.offset_x,
            self.render_state.offset_y,
        );
    }

    pub fn restore_current_view_state(&mut self) {
        let Some(path) = self.model.current_path() else {
            return;
        };
        let Some(stored) = self.view_state_store.get(&path) else {
            return;
        };

        if self.settings.remember_zoom() {
            let min_zoom = self.settings.min_zoom();
            let max_zoom = self.settings.max_zoom();
            let scale = if stored.scale.is_finite() {
                stored.scale.clamp(min_zoom, max_zoom)
            } else {
                1.0
            };
            self.view_state.scale = scale;
            self.render_state.scale = scale;
        }
        if self.settings.remember_position() {
            self.view_state.offset_x = stored.offset_x;
            self.view_state.offset_y = stored.offset_y;
            self.render_state.offset_x = stored.offset_x;
            self.render_state.offset_y = stored.offset_y;
        }

        self.sync_zoom_animation_to_render_state();
        self.clamp_zoom_to_limits();
    }

    pub fn save_current_view_state(&mut self) {
        let Some(path) = self.model.current_path() else {
            return;
        };

        self.view_state_store.save(
            &path,
            StoredViewState {
                scale: self.render_state.scale,
                offset_x: self.render_state.offset_x,
                offset_y: self.render_state.offset_y,
            },
            self.settings.remember_zoom(),
            self.settings.remember_position(),
        );
    }

    pub fn new(model: Model, loader_sender: Sender<LoaderMsg>, settings: Settings) -> Self {
        let animation_enabled = settings.smooth_zoom_enabled();
        let animation_duration_ms = settings.animation_duration_ms();
        let mut zoom_animation = ZoomAnimation::new(1.0, 0.0, 0.0);
        zoom_animation.duration = Duration::from_millis(animation_duration_ms as u64);
        zoom_animation.enabled = animation_enabled;
        // Історія станів має жити лише в межах поточного запуску програми.
        settings.set_remembered_view_state("");
        Self {
            model,
            image_cache: ImageCache::new(),
            texture_cache: TextureCache::new(),
            animated_gif_player: AnimatedGifPlayer::default(),
            loading: HashSet::new(),
            loader_sender,
            settings,
            view_state: ViewState::new(1.0, 0.0, 0.0),
            render_state: ViewState::new(1.0, 0.0, 0.0),
            is_100: false,
            zoom_animation,
            animation_enabled,
            animation_duration_ms,
            crossfade: Crossfade::new(),
            crossfade_pending_old: None,
            window_width: 0.0,
            window_height: 0.0,
            win_w: 0,
            win_h: 0,
            last_pointer: (0.0, 0.0),
            drag_offset_start: (0.0, 0.0),
            needs_initial_fit: true,
            view_state_store: ViewStateStore::default(),
            pending_rotation: None,
        }
    }

    // ---- Взаємодія з ImageLoader / кешами ----------------------------

    /// Розміри (ширина, висота) поточного зображення в пікселях, якщо
    /// воно вже декодоване й лежить у `image_cache`. Поки зображення ще
    /// вантажиться асинхронно (див. `ensure_loaded`/`handle_loaded`),
    /// повертає `None` — викликач (app.rs) у цьому разі не повинен
    /// показувати розмір як "0 x 0", а дочекатись наступного оновлення,
    /// коли зображення вже буде в кеші.
    pub fn current_image_dimensions(&mut self) -> Option<(i32, i32)> {
        let path = self.model.current_path()?;
        let pixbuf = self.image_cache.get(&path)?;
        Some((pixbuf.width(), pixbuf.height()))
    }

    /// Наскільки близько (у кроках навігації від поточного зображення)
    /// файл повинен бути, щоб для нього одразу готувалась GPU-текстура,
    /// коли увімкнено "Попередньо створювати текстури". Раніше це було
    /// жорстко зашите число (1), через що більшість уже продекодованого
    /// про запас вікна preload'у все одно лишалась без текстури аж до
    /// моменту фактичної навігації. Тепер радіус збігається з тим самим
    /// вікном, що й попереднє декодування (`ensure_current_and_neighbors_loaded`)
    /// — якщо зображення вже варте того, щоб тримати його декодований
    /// pixbuf напоготові, то й GPU-текстуру для нього варто тримати
    /// напоготові теж.
    fn pretexture_radius(&self) -> usize {
        self.settings
            .preload_mode()
            .resolved_count(self.settings.cache_capacity())
    }

    /// Заздалегідь створює (і кладе в texture_cache) GPU-текстуру для
    /// вже декодованого зображення за шляхом `path`, якщо вона там ще
    /// відсутня. `gdk::Texture::for_pixbuf()` коштує тим дорожче, чим
    /// важче зображення — тому цей метод свідомо викликається ЗАВЖДИ
    /// безпосередньо ПЕРЕД `crossfade.start()` (тобто до фіксації
    /// `started_at`): якщо аплоад і забере помітний час, це станеться
    /// один раз, до старту таймлайну переходу, а не посеред уже
    /// запущеної анімації, де це виглядало б як ривок або, для дуже
    /// важких зображень, як анімація, що взагалі не встигла показати
    /// жодного проміжного кадру. Якщо pixbuf ще не в image_cache
    /// (малоймовірно тут, бо викликається лише для вже перевірених
    /// шляхів) — тихо нічого не робить, `TextureCache::get_or_create` у
    /// snapshot() лишається підстраховкою на цей випадок.
    fn warm_texture(&mut self, path: &PathBuf) {
        if let Some(pixbuf) = self.image_cache.get(path) {
            self.texture_cache.get_or_create(path, &pixbuf);
        }
    }

    fn crossfade_durations(&self) -> (Duration, Duration) {
        let (fade_out_ms, fade_in_ms) = match self.settings.transition_animation() {
            crate::preferences::settings::TransitionAnimation::Crossfade => (
                self.settings.crossfade_duration_ms(),
                self.settings.crossfade_duration_ms(),
            ),
            crate::preferences::settings::TransitionAnimation::ZoomFade => (
                self.settings.zoom_fade_out_duration_ms(),
                self.settings.zoom_fade_in_duration_ms(),
            ),
            crate::preferences::settings::TransitionAnimation::HorizontalSlide => {
                let duration = self.settings.horizontal_slide_duration_ms();
                (duration, duration)
            }
            crate::preferences::settings::TransitionAnimation::VerticalSlide => {
                let duration = self.settings.vertical_slide_duration_ms();
                (duration, duration)
            }
            crate::preferences::settings::TransitionAnimation::HorizontalFadeSlide => {
                let duration = self.settings.horizontal_fade_slide_duration_ms();
                (duration, duration)
            }
            crate::preferences::settings::TransitionAnimation::VerticalFadeSlide => {
                let duration = self.settings.vertical_fade_slide_duration_ms();
                (duration, duration)
            }
            crate::preferences::settings::TransitionAnimation::HorizontalScaleSlide => {
                let duration = self.settings.horizontal_scale_slide_duration_ms();
                (duration, duration)
            }
            crate::preferences::settings::TransitionAnimation::VerticalScaleSlide => {
                let duration = self.settings.vertical_scale_slide_duration_ms();
                (duration, duration)
            }
        };
        (
            Duration::from_millis(fade_out_ms as u64),
            Duration::from_millis(fade_in_ms as u64),
        )
    }

    fn start_crossfade(&mut self, old_frame: FrozenFrame, new_path: &PathBuf) {
        self.warm_texture(&old_frame.path);
        self.warm_texture(new_path);
        let (fade_out_duration, fade_in_duration) = self.crossfade_durations();
        self.crossfade.start(
            old_frame.path,
            old_frame.scale,
            old_frame.offset_x,
            old_frame.offset_y,
            old_frame.direction,
            fade_out_duration,
            fade_in_duration,
        );
    }

    pub fn is_current_path(&self, path: &PathBuf) -> bool {
    self.model.current_path().as_ref() == Some(path)
}

    pub fn ensure_loaded(&mut self, path: &PathBuf) {
        if self.model.current_path().as_ref() == Some(path) {
            self.animated_gif_player.clear_active();
        }
        if self.image_cache.contains(path) || self.loading.contains(path) {
            return;
        }
        self.loading.insert(path.clone());
        image_loader::request_load(path.clone(), self.loader_sender.clone());
    }

    fn activate_animated_gif(&mut self, path: &PathBuf) {
        self.animated_gif_player.activate(path);
    }

    /// Запитує завантаження поточного зображення і попередньо підвантажує
    /// сусідні, щоб навігація відчувалась миттєвою. Кількість сусідів (і
    /// як вона ділиться між "вперед"/"назад") визначається налаштуванням
    /// "Попереднє завантаження зображень" (розділ "Продуктивність"):
    /// сумарна кількість береться з `PreloadMode::resolved_count`, а
    /// наступних зображень підвантажується на 1 більше за попередні, коли
    /// сума непарна (навігація вперед — типовий напрямок перегляду).
    pub fn ensure_current_and_neighbors_loaded(&mut self) {
        let Some(cur) = self.model.current_path() else {
            return;
        };
        self.ensure_loaded(&cur);

        let total = self
            .settings
            .preload_mode()
            .resolved_count(self.settings.cache_capacity());
        let forward = (total + 1) / 2;
        let backward = total / 2;

        for p in self.model.neighbor_paths_extended(forward, backward) {
            self.ensure_loaded(&p);
        }
    }

    /// Викликається з app.rs, коли ImageLoader повернув декодоване
    /// зображення для якогось шляху.
    pub fn handle_loaded(&mut self, path: PathBuf, image: LoadedImage) {
        self.loading.remove(&path);

        let (pixbuf, animated_frames): (Pixbuf, Option<AnimatedGifFrames>) = match image {
            LoadedImage::Static(decoded) => (image_loader::to_pixbuf(decoded), None),
            LoadedImage::AnimatedGif(frames) => {
                let mut pixbufs = Vec::with_capacity(frames.len());
                let mut delays = Vec::with_capacity(frames.len());
                for DecodedFrame { image, delay_ms } in frames {
                    pixbufs.push(Rc::new(image_loader::to_pixbuf(image)));
                    delays.push(Duration::from_millis(delay_ms.max(10)));
                }
                let first = pixbufs[0].clone();
                ((*first).clone(), Some((pixbufs, delays)))
            }
        };

        // "Попередньо створювати текстури": для сусідів у межах вікна
        // попереднього завантаження (те саме вікно, що й декодування —
        // див. pretexture_radius()) GPU-текстура готується одразу тут,
        // а не відкладається до моменту, коли зображення стане поточним
        // — саме це прибирає помітну затримку на першому кадрі анімації
        // переходу. Раніше радіус був жорстко зашитий в 1 крок, тож
        // будь-яке зображення, продекодоване про запас на 2+ кроки
        // наперед, усе одно чекало на дорогий синхронний аплоад текстури
        // аж до моменту фактичного малювання — саме це й спричиняло
        // гальмування/пропуски анімації для важких зображень.
        if self.settings.preload_textures_enabled() {
            if let Some(dist) = self.model.cyclic_distance(&path) {
                if dist <= self.pretexture_radius() {
                    self.texture_cache.get_or_create(&path, &pixbuf);
                }
            }
        }

        self.image_cache.insert(path.clone(), pixbuf);
        self.animated_gif_player.cache_frames(path.clone(), animated_frames);
        if self.model.current_path().as_ref() == Some(&path) {
            self.activate_animated_gif(&path);
        }

        if self.model.current_path().as_ref() == Some(&path) {
            // Якщо саме на це зображення чекав crossfade (навігація
            // сталась до того, як воно встигло довантажитись асинхронно)
            // — запускаємо перехід тепер, до fit_to_screen(), яке одразу
            // перепише render_scale/offset під нове фото.
            if let Some(old_frame) = self.crossfade_pending_old.take()
            {
                if self.settings.crossfade_enabled()
                    && old_frame.path != path
                    && self.image_cache.contains(&old_frame.path)
                {
                    // Прогріваємо текстури ОБОХ кадрів переходу тут, до
                    // crossfade.start() — тобто до того, як зафіксується
                    // started_at. Якщо аплоад для важкого зображення й
                    // забере помітний час, він "з'їсть" його ще до
                    // старту таймлайну анімації, а не посеред уже
                    // запущеного переходу (де це виглядало б як ривок
                    // або взагалі пропущена анімація).
                    self.start_crossfade(old_frame, &path);
                }
            }

            let had_saved_state =
                self.settings.remember_zoom() || self.settings.remember_position();
            if had_saved_state && self.view_state_store.contains(&path) {
                self.restore_current_view_state();
            } else {
                self.fit_to_screen();
            }
            self.needs_initial_fit = false;
        }
    }

    pub fn animated_gif_step(&mut self, now: Instant) -> bool {
        self.animated_gif_player.step(now)
    }

    pub fn animated_gif_texture(&self) -> Option<gdk::Texture> {
        self.animated_gif_player.texture()
    }

    pub fn handle_load_failed(&mut self, path: &PathBuf) {
        self.loading.remove(path);
    }

    /// Впорядковує список зображень за обраним режимом (пункт
    /// "Упорядкування" контекстного меню) і підвантажує сусідів заново,
    /// оскільки після сортування вони можуть відрізнятися.
    pub fn sort_images(&mut self, mode: SortMode) {
        self.model.sort_by(mode);
        self.crossfade.cancel();
        self.crossfade_pending_old = None;
        self.ensure_current_and_neighbors_loaded();
    }

    /// Переміщує поточне зображення до кошика замість повного видалення.
    /// У разі помилки список не змінюється, а повідомлення пишеться в stderr.
    pub fn delete_current_image(&mut self) {
        let Some(path) = self.model.current_path() else {
            return;
        };

        let file = gio::File::for_path(&path);
        if let Err(err) = file.trash(None::<&gio::Cancellable>) {
            eprintln!("Не вдалося перемістити файл до кошика {:?}: {}", path, err);
            return;
        }

        self.on_deleted_current_image(&path);
    }

    /// Видаляє поточне зображення назавжди.
    /// У разі помилки список не змінюється, а повідомлення пишеться в stderr.
    pub fn delete_current_image_permanently(&mut self) {
        let Some(path) = self.model.current_path() else {
            return;
        };

        let file = gio::File::for_path(&path);
        if let Err(err) = file.delete(None::<&gio::Cancellable>) {
            eprintln!("Не вдалося видалити файл назавжди {:?}: {}", path, err);
            return;
        }

        self.on_deleted_current_image(&path);
    }

    fn on_deleted_current_image(&mut self, path: &PathBuf) {
        // Файл або вже в кошику, або видалений назавжди — записувати на
        // диск накопичений поворот тепер нема сенсу (і нема куди), тож
        // просто скидаємо pending-стан, не комітячи його.
        self.discard_pending_rotation_for(path);
        self.loading.remove(path);
        self.image_cache.remove(path);
        self.texture_cache.remove(path);
        self.crossfade.cancel();
        self.crossfade_pending_old = None;
        self.model.remove_current();

        self.needs_initial_fit = true;
        self.ensure_current_and_neighbors_loaded();
        self.fit_to_screen();
    }

    // ---- Навігація -----------------------------------------------------

    /// Переходить до зображення з конкретним індексом (наприклад, кліком
    /// по мініатюрі). Обирає найкоротший циклічний шлях і делегує navigate().
    pub fn navigate_to(&mut self, target: usize) {
        if self.model.is_empty() {
            return;
        }
        let len = self.model.images.len();
        let target = target % len;
        if target == self.model.index {
            return;
        }
        let fwd = (target + len - self.model.index) % len;
        let bwd = (self.model.index + len - target) % len;
        let delta = if fwd <= bwd {
            fwd as isize
        } else {
            -(bwd as isize)
        };
        self.navigate(delta);
    }

    pub fn navigate(&mut self, direction: isize) {
        if self.model.is_empty() {
            return;
        }

        // Зображення, що було поточним, зараз перестає бути таким — якщо
        // на ньому лишився незбережений поворот (клік(и) "Повернути" без
        // подальшого коміту), саме зараз єдиний момент його записати:
        // рівно одним зверненням до диска, незалежно від кількості
        // кліків, що до цього накопичились.
        self.commit_pending_rotation();

        // "Заморожуємо" те, що зараз реально на екрані — саме з цим
        // кадром (а не з абстрактним "попереднім зображенням") crossfade
        // буде розчиняти новий, коли/якщо перехід увімкнено.
        let old_frame = self.model.current_path().map(|p| {
            FrozenFrame {
                path: p,
                scale: self.render_state.scale,
                offset_x: self.render_state.offset_x,
                offset_y: self.render_state.offset_y,
                direction,
            }
        });

        self.model.navigate(direction);
        self.needs_initial_fit = true;
        self.ensure_current_and_neighbors_loaded();
        if let Some(path) = self.model.current_path().as_ref() {
            self.activate_animated_gif(path);
        }

        let new_path = self.model.current_path();
        let target_already_loaded = new_path
            .as_ref()
            .map(|p| self.image_cache.contains(p))
            .unwrap_or(false);

        self.fit_to_screen();

        if target_already_loaded {
            let had_saved_state =
                self.settings.remember_zoom() || self.settings.remember_position();
            let has_saved_state = self
                .model
                .current_path()
                .map(|path| self.view_state_store.contains(&path))
                .unwrap_or(false);
            if had_saved_state && has_saved_state {
                self.restore_current_view_state();
            }
        }

        self.crossfade_pending_old = None;
        if let (Some(old_frame), Some(new_path)) = (old_frame, new_path)
        {
            if self.settings.crossfade_enabled()
                && old_frame.path != new_path
                && self.image_cache.contains(&old_frame.path)
            {
                if target_already_loaded {
                    // Нове зображення вже в кеші (типовий випадок завдяки
                    // попередньому завантаженню) — fit_to_screen() вище
                    // вже виставив render_scale/offset під нього, тож
                    // перехід можна запускати негайно. Прогріваємо
                    // текстури обох кадрів ДО crossfade.start() з тієї ж
                    // причини, що й у handle_loaded() — щоб можливий
                    // дорогий аплоад не крав час з таймлайну вже
                    // запущеної анімації.
                    self.start_crossfade(old_frame, &new_path);
                } else {
                    // Ще вантажиться — запустимо перехід пізніше, з
                    // handle_loaded(), коли з'явиться що показувати замість
                    // плейсхолдера "Завантаження…" (інакше старе фото
                    // встигло б розчинитись у порожнечу).
                    self.crossfade_pending_old = Some(old_frame);
                }
            } else if !self.settings.crossfade_enabled() {
                self.crossfade.cancel();
            }
        }
    }

    // ---- Масштаб і панорамування ---------------------------------------

    /// Примусово повертає поточний ручний масштаб у дозволений діапазон.
    /// Режим "вписати в екран" сам по собі цими межами не обмежується.
    pub fn clamp_zoom_to_limits(&mut self) {
        let min_zoom = self.settings.min_zoom();
        let max_zoom = self.settings.max_zoom().max(min_zoom);
        let current_scale = self.render_state.scale;

        if !current_scale.is_finite() {
            return;
        }

        let new_scale = current_scale.clamp(min_zoom, max_zoom);
        if (new_scale - current_scale).abs() <= self.zoom_animation.epsilon {
            return;
        }

        let win_w = if self.window_width > 0.0 {
            self.window_width
        } else {
            self.win_w as f64
        };
        let win_h = if self.window_height > 0.0 {
            self.window_height
        } else {
            self.win_h as f64
        };
        let (ax, ay) = (win_w / 2.0, win_h / 2.0);
        let old_scale = current_scale.max(1e-6);
        let img_x = (ax - self.render_state.offset_x) / old_scale;
        let img_y = (ay - self.render_state.offset_y) / old_scale;
        let new_offset_x = ax - img_x * new_scale;
        let new_offset_y = ay - img_y * new_scale;

        self.view_state = ViewState::new(new_scale, new_offset_x, new_offset_y);
        self.is_100 = (new_scale - 1.0).abs() <= self.zoom_animation.epsilon;
        self.zoom_animation
            .set_target(new_scale, new_offset_x, new_offset_y);
        self.save_current_view_state();
    }

    pub fn fit_to_screen(&mut self) {
        self.is_100 = false;
        let Some(state) = self.fit_view_state() else {
            return;
        };
        self.view_state = state;
        self.render_state = state;
        self.sync_zoom_animation_to_render_state();
    }

    fn fit_view_state(&mut self) -> Option<ViewState> {
        let Some(path) = self.model.current_path() else {
            return None;
        };
        let Some(pixbuf) = self.image_cache.get(&path) else {
            return None;
        };

        let img_w = pixbuf.width() as f64;
        let img_h = pixbuf.height() as f64;
        let win_w = if self.window_width > 0.0 {
            self.window_width
        } else {
            self.win_w as f64
        };
        let win_h = if self.window_height > 0.0 {
            self.window_height
        } else {
            self.win_h as f64
        };

        if win_w <= 0.0 || win_h <= 0.0 || img_w <= 0.0 || img_h <= 0.0 {
            return None;
        }

        let padding = self.settings.image_padding();
        let avail_w = (win_w - padding * 2.0).max(1.0);
        let avail_h = (win_h - padding * 2.0).max(1.0);

        let scale_x = avail_w / img_w;
        let scale_y = avail_h / img_h;
        let new_scale = scale_x.min(scale_y).min(1.0);
        let new_offset_x = (win_w - img_w * new_scale) / 2.0;
        let new_offset_y = (win_h - img_h * new_scale) / 2.0;
        Some(ViewState::new(new_scale, new_offset_x, new_offset_y))
    }

    /// Масштаб 100%, зберігаючи нерухомою точку anchor (координати вікна).
    pub fn set_zoom_100(&mut self, anchor: Option<(f64, f64)>) {
        let win_w = if self.window_width > 0.0 {
            self.window_width
        } else {
            self.win_w as f64
        };
        let win_h = if self.window_height > 0.0 {
            self.window_height
        } else {
            self.win_h as f64
        };
        let (ax, ay) = anchor.unwrap_or((win_w / 2.0, win_h / 2.0));
        let old_scale = if self.view_state.scale != 0.0 {
            self.view_state.scale
        } else {
            1.0
        };
        let img_x = (ax - self.view_state.offset_x) / old_scale;
        let img_y = (ay - self.view_state.offset_y) / old_scale;
        self.view_state = ViewState::new(1.0, ax - img_x, ay - img_y);
        self.render_state = self.view_state;
        self.is_100 = true;
        self.sync_zoom_animation_to_render_state();
        self.save_current_view_state();
    }

    /// Перемикач для середньої кнопки миші: 100% <-> "вписати в екран".
    pub fn toggle_zoom(&mut self, anchor: (f64, f64)) {
        let Some(path) = self.model.current_path() else {
            return;
        };
        if !self.image_cache.contains(&path) {
            return;
        }
        if !self.is_100 {
            self.animate_zoom_100(anchor);
        } else {
            self.animate_fit_to_screen();
        }
    }

    /// Анімований перехід для середньої кнопки: 300 мс, easeInOutQuad.
    fn animate_zoom_100(&mut self, anchor: (f64, f64)) {
        let (ax, ay) = anchor;
        let old_scale = self.render_state.scale.max(1e-6);
        let img_x = (ax - self.render_state.offset_x) / old_scale;
        let img_y = (ay - self.render_state.offset_y) / old_scale;
        let state = ViewState::new(1.0, ax - img_x, ay - img_y);
        self.view_state = state;
        self.is_100 = true;
        self.zoom_animation.set_target_with_easing(
            state.scale,
            state.offset_x,
            state.offset_y,
            Duration::from_millis(300),
            ZoomEasing::InOutQuad,
        );
        self.save_current_view_state();
    }

    /// Анімоване вписування в екран для середньої кнопки.
    fn animate_fit_to_screen(&mut self) {
        let Some(state) = self.fit_view_state() else {
            return;
        };
        self.view_state = state;
        self.is_100 = false;
        self.zoom_animation.set_target_with_easing(
            state.scale,
            state.offset_x,
            state.offset_y,
            Duration::from_millis(300),
            ZoomEasing::InOutQuad,
        );
        self.save_current_view_state();
    }

    /// Масштабування колесом миші з прив'язкою до точки anchor.
    pub fn zoom_by(&mut self, factor: f64, anchor: (f64, f64)) {
        let Some(path) = self.model.current_path() else {
            return;
        };
        if !self.image_cache.contains(&path) {
            return;
        }
        let (ax, ay) = anchor;
        let old_scale = self.render_state.scale.max(1e-6);
        let min_zoom = self.settings.min_zoom();
        let max_zoom = self.settings.max_zoom().max(min_zoom);
        let new_scale = (old_scale * factor).clamp(min_zoom, max_zoom);
        let img_x = (ax - self.render_state.offset_x) / old_scale;
        let img_y = (ay - self.render_state.offset_y) / old_scale;
        let new_offset_x = ax - img_x * new_scale;
        let new_offset_y = ay - img_y * new_scale;
        self.zoom_animation
            .set_target(new_scale, new_offset_x, new_offset_y);
        self.view_state = ViewState::new(
            self.zoom_animation.current_scale,
            self.zoom_animation.current_offset_x,
            self.zoom_animation.current_offset_y,
        );
        self.is_100 = false;
        self.save_current_view_state();
    }

    pub fn pan_start(&mut self) {
        self.zoom_animation.finish();
        self.view_state = ViewState::new(
            self.zoom_animation.current_scale,
            self.zoom_animation.current_offset_x,
            self.zoom_animation.current_offset_y,
        );
        self.drag_offset_start = (self.view_state.offset_x, self.view_state.offset_y);
    }

    pub fn pan_update(&mut self, dx: f64, dy: f64) {
        let (sx, sy) = self.drag_offset_start;
        let new_offset_x = sx + dx;
        let new_offset_y = sy + dy;
        self.view_state.offset_x = new_offset_x;
        self.view_state.offset_y = new_offset_y;
        self.render_state.offset_x = new_offset_x;
        self.render_state.offset_y = new_offset_y;
        self.sync_zoom_animation_to_render_state();
        self.save_current_view_state();
    }

    /// Повертає зображення на позицію до початку перетягування (коли
    /// перетягування виявилось звичайним кліком, а не панорамуванням).
    pub fn pan_cancel(&mut self) {
        let (sx, sy) = self.drag_offset_start;
        self.view_state.offset_x = sx;
        self.view_state.offset_y = sy;
        self.render_state.offset_x = sx;
        self.render_state.offset_y = sy;
        self.sync_zoom_animation_to_render_state();
        self.save_current_view_state();
    }

    /// Замінює поточний список зображень новим (наприклад, після
    /// перетягування файлу у вікно — див. app.rs, `setup_drag_and_drop`) і
    /// перезапускає завантаження з чистими кешами. Незбережений поворот
    /// попереднього зображення спершу комітиться, щоб не загубився.
    pub fn load_new_source(&mut self, images: Vec<PathBuf>, index: usize) {
        self.commit_pending_rotation();

        let mut model = Model::new(images, index);
        model.sort_by(self.settings.sort_mode());
        self.model = model;

        self.image_cache = ImageCache::new();
        self.texture_cache = TextureCache::new();
        self.loading.clear();
        self.view_state_store.clear();
        self.crossfade.cancel();
        self.crossfade_pending_old = None;
        self.needs_initial_fit = true;
        self.is_100 = false;

        self.ensure_current_and_neighbors_loaded();
    }

    pub fn resize(&mut self, w: i32, h: i32) {
        // Правий клік ставить popover.set_parent(area) — коли контекстне
        // меню з'являється вперше, GTK змушує Canvas пройти зайвий
        // цикл measure/allocate, і сигнал "resize" спрацьовує ще раз
        // навіть якщо фактичні w/h не змінились. Раніше resize() у
        // такому разі все одно безумовно викликав fit_to_screen(), що й
        // скидало масштаб та позицію зображення до "вписати в екран" —
        // виглядало так, ніби правий клік "повертає фото в межі вікна".
        // Якщо розмір справді не змінився — виходимо без перефітовування.
        if self.win_w == w && self.win_h == h {
            return;
        }

        self.window_width = w as f64;
        self.window_height = h as f64;
        self.win_w = w;
        self.win_h = h;
        self.fit_to_screen();
    }

    pub fn apply_animation_step(&mut self, now: std::time::Instant) -> bool {
        let previous_render_state = self.render_state;
        let needs_redraw = self.zoom_animation.update(now);

        self.render_state = ViewState::new(
            self.zoom_animation.current_scale,
            self.zoom_animation.current_offset_x,
            self.zoom_animation.current_offset_y,
        );

        if !self.zoom_animation.is_active() {
            self.view_state = self.render_state;
        }

        needs_redraw
            || previous_render_state.scale != self.render_state.scale
            || previous_render_state.offset_x != self.render_state.offset_x
            || previous_render_state.offset_y != self.render_state.offset_y
    }
}
