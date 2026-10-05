// viewer/renderer.rs — кастомний віджет Canvas, що використовує gtk4::Snapshot
// для апаратного прискорення (через GPU) замість програмного рендерингу.

use crate::preferences::settings::BackgroundType;
use crate::preferences::Settings;
use crate::viewer::Viewer;
use gtk4::gdk;
use gtk4::glib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

mod drawing;
mod background;

mod imp {
    use super::*;
    use super::super::transition::{calculate_transition, TransitionState};

    /// Фіксований розмір індикатора завантаження (GtkSpinner). Задається
    /// напряму, а не через measure() — measure() повертає (0, 0) для
    /// невидимого віджета в GTK4, тож поки спіннер прихований (типовий
    /// стан поза завантаженням), вимірювана площа завжди була б нульовою.
    const SPINNER_SIZE: i32 = 32;
    const OPEN_FILE_BUTTON_WIDTH: i32 = 180;
    const OPEN_FILE_BUTTON_HEIGHT: i32 = 50;

    #[derive(Default)]
    pub struct Canvas {
        pub viewer: RefCell<Option<Rc<RefCell<Viewer>>>>,
        pub settings: RefCell<Option<Settings>>,
        pub(super) desktop_background_settings: RefCell<Option<gtk4::gio::Settings>>,
        background: RefCell<super::background::BackgroundRenderer>,
        /// Індикатор завантаження — замінює текстовий плейсхолдер
        /// "Завантаження…", що раніше малювався через Cairo. Живе як
        /// звичайний дочірній віджет Canvas (пропарентований у
        /// `constructed()`), тож анімацію обертання рухає власний
        /// frame clock GtkSpinner, а не наш ручний Cairo-рендер.
        pub spinner: gtk4::Spinner,
        /// Поточний стан видимості спіннера — щоб не смикати
        /// `set_visible`/`set_spinning` щокадрово в snapshot(), а лише
        /// коли стан справді змінився.
        pub spinner_active: std::cell::Cell<bool>,
        pub open_file_button: gtk4::Button,
        pub open_file_button_active: std::cell::Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Canvas {
        const NAME: &'static str = "ImageCanvas";
        type Type = super::Canvas;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for Canvas {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().set_overflow(gtk4::Overflow::Hidden);
            self.spinner.set_parent(&*self.obj());
            self.spinner.set_width_request(SPINNER_SIZE);
            self.spinner.set_height_request(SPINNER_SIZE);
            self.spinner.set_visible(false);

            let icon = gtk4::Image::from_icon_name("document-open-symbolic");
            icon.set_pixel_size(20);
            let label = gtk4::Label::new(Some("Відкрити файл"));
            let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
            content.set_halign(gtk4::Align::Center);
            content.append(&icon);
            content.append(&label);
            self.open_file_button.set_child(Some(&content));
            self.open_file_button
                .set_size_request(OPEN_FILE_BUTTON_WIDTH, OPEN_FILE_BUTTON_HEIGHT);
            self.open_file_button.add_css_class("overlay-button");
            self.open_file_button.add_css_class("overlay-file-label");
            self.open_file_button.set_visible(false);
            self.open_file_button.set_parent(&*self.obj());
        }

        fn dispose(&self) {
            self.spinner.unparent();
            self.open_file_button.unparent();
        }
    }

    impl WidgetImpl for Canvas {
        // Відслідковуємо зміну розмірів віджета і передаємо їх у Viewer
        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            self.parent_size_allocate(width, height, baseline);
            if let Some(viewer_rc) = self.viewer.borrow().as_ref() {
                viewer_rc.borrow_mut().resize(width, height);
            }

            // GtkSpinner центруємо за ФІКСОВАНИМ розміром (SPINNER_SIZE) —
            // НЕ через measure(), бо той повертає (0, 0) для невидимого
            // віджета, і саме тому спіннер раніше отримував 0×0.
            let spinner_x = ((width - SPINNER_SIZE) / 2).max(0);
            let spinner_y = ((height - SPINNER_SIZE) / 2).max(0);
            self.spinner.allocate(
                SPINNER_SIZE,
                SPINNER_SIZE,
                baseline,
                Some(gsk::Transform::new().translate(&graphene::Point::new(
                    spinner_x as f32,
                    spinner_y as f32,
                ))),
            );

            let open_file_x = ((width - OPEN_FILE_BUTTON_WIDTH) / 2).max(0);
            let open_file_y = ((height - OPEN_FILE_BUTTON_HEIGHT) / 2).max(0);
            self.open_file_button.allocate(
                OPEN_FILE_BUTTON_WIDTH,
                OPEN_FILE_BUTTON_HEIGHT,
                baseline,
                Some(gsk::Transform::new().translate(&graphene::Point::new(
                    open_file_x as f32,
                    open_file_y as f32,
                ))),
            );

            // Виділяємо місце для дочірніх віджетів. Popover сам керує
            // позиціонуванням через popup_at(), але custom child усе одно
            // повинен отримати власний allocation.
            let mut child = self.obj().first_child();
            while let Some(c) = child {
                if c.downcast_ref::<gtk4::Popover>().is_some() {
                    if c.is_visible() {
                        let (_, popover_width, _, _) =
                            c.measure(gtk4::Orientation::Horizontal, -1);
                        let (_, popover_height, _, _) =
                            c.measure(gtk4::Orientation::Vertical, popover_width);
                        if popover_width > 0 && popover_height > 0 {
                            c.allocate(popover_width, popover_height, baseline, None);
                        }
                    }
                } else if c.downcast_ref::<gtk4::Spinner>().is_some()
                    || c.downcast_ref::<gtk4::Button>().is_some()
                {
                    // Spinner і кнопка виділені вище.
                } else {
                    c.allocate(width, height, baseline, None);
                }
                child = c.next_sibling();
            }
        }

        // Метод snapshot викликається GTK при кожному перемальовуванні.
        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let width = self.obj().width() as f32;
            let height = self.obj().height() as f32;

            // 1. Малюємо фон: системні/користувацькі шпалери з ефектами або
            // системний/користувацький суцільний колір. Якщо Settings ще не
            // прив'язано (напр. на дуже ранньому кадрі до set_settings()),
            // лишається розумний дефолт темного тла.
            let background_drawn = {
                let settings = self.settings.borrow();
                self.background.borrow_mut().draw(
                    snapshot,
                    self.obj().upcast_ref(),
                    settings.as_ref(),
                    width,
                    height,
                )
            };
            if !background_drawn {
                snapshot.append_color(
                    &self.effective_background_color(),
                    &graphene::Rect::new(0.0, 0.0, width, height),
                );
            }

            let viewer_opt = self.viewer.borrow();
            let Some(viewer_rc) = viewer_opt.as_ref() else {
                return;
            };
            let mut viewer = viewer_rc.borrow_mut();

            let Some(path) = viewer.model.current_path() else {
                self.set_spinner_active(false);
                self.set_open_file_button_active(true);

                let mut child = self.obj().first_child();
                while let Some(c) = child {
                    self.obj().snapshot_child(&c, snapshot);
                    child = c.next_sibling();
                }
                return;
            };

            self.set_open_file_button_active(false);

            // 2. Малюємо GPU-текстуру. Якщо зараз активний crossfade
            // (розділ "Анімації" → "Увімкнути Crossfade") — старий кадр
            // домальовується внизу з опацією, що спадає до 0, а новий
            // (нижче за кодом) — зверху з опацією, що росте до 1. Кожен
            // лишається на своїй "замороженій" позиції вписування —
            // жодного морфінгу масштабу/зсуву між кадрами, суто
            // альфа-розчинення.
            let transition_state = calculate_transition(&viewer, width, height);
            let corner_radius = self.effective_corner_radius();

            if let Some(TransitionState {
                old_opacity,
                old_shift_x,
                old_shift_y,
                old_scale_mul,
                ..
            }) = transition_state {
                if let Some(old_path) = viewer.crossfade.old_path.clone() {
                    if let Some(old_pixbuf) = viewer.image_cache.get(&old_path) {
                        let old_texture =
                            viewer.texture_cache.get_or_create(&old_path, &old_pixbuf);
                        let old_scale = viewer.crossfade.old_scale * old_scale_mul;
                        let old_center_x = viewer.crossfade.old_offset_x + (old_pixbuf.width() as f64 * viewer.crossfade.old_scale) / 2.0;
                        let old_center_y = viewer.crossfade.old_offset_y + (old_pixbuf.height() as f64 * viewer.crossfade.old_scale) / 2.0;
                        let old_adjusted_x = old_center_x - (old_pixbuf.width() as f64 * old_scale) / 2.0 + old_shift_x;
                        let old_adjusted_y = old_center_y - (old_pixbuf.height() as f64 * old_scale) / 2.0 + old_shift_y;
                        super::drawing::draw_texture(
                            snapshot,
                            &old_texture,
                            old_scale,
                            old_adjusted_x,
                            old_adjusted_y,
                            old_opacity,
                            corner_radius,
                        );
                    }
                }
            }

            if let Some(pixbuf) = viewer.image_cache.get(&path) {
                self.set_spinner_active(false);
                let texture = viewer
                    .animated_gif_texture()
                    .unwrap_or_else(|| viewer.texture_cache.get_or_create(&path, &pixbuf));
                let render = viewer.render_view_state();
                let (opacity, new_shift_x, new_shift_y, new_scale_mul) = transition_state
                    .map(|state| {
                        (
                            state.new_opacity,
                            state.new_shift_x,
                            state.new_shift_y,
                            state.new_scale_mul,
                        )
                    })
                    .unwrap_or((1.0, 0.0, 0.0, 1.0));
                let new_scale = render.scale * new_scale_mul;
                let new_center_x = render.offset_x + (pixbuf.width() as f64 * render.scale) / 2.0;
                let new_center_y = render.offset_y + (pixbuf.height() as f64 * render.scale) / 2.0;
                let new_adjusted_x = new_center_x - (pixbuf.width() as f64 * new_scale) / 2.0 + new_shift_x;
                let new_adjusted_y = new_center_y - (pixbuf.height() as f64 * new_scale) / 2.0 + new_shift_y;
                let settings = self.settings.borrow();
                super::drawing::draw_image_shadow(
                    snapshot,
                    &texture,
                    new_scale,
                    new_adjusted_x,
                    new_adjusted_y,
                    opacity,
                    settings.as_ref(),
                    corner_radius,
                );
                super::drawing::draw_texture(
                    snapshot,
                    &texture,
                    new_scale,
                    new_adjusted_x,
                    new_adjusted_y,
                    opacity,
                    corner_radius,
                );
            } else {
                self.set_spinner_active(true);
            }

            // 3. Малюємо дочірні віджети (включаючи контекстне меню)
            let mut child = self.obj().first_child();
            while let Some(c) = child {
                self.obj().snapshot_child(&c, snapshot);
                child = c.next_sibling();
            }
        }
    }

    impl Canvas {
        /// Системний або користувацький суцільний колір тла.
        pub(super) fn effective_background_color(&self) -> gdk::RGBA {
            match self.settings.borrow().as_ref() {
                Some(settings) if settings.background_type() == BackgroundType::CustomColors => {
                    settings.background_color()
                }
                _ => self.system_background_color(),
            }
        }

        #[allow(deprecated)]
        fn system_background_color(&self) -> gdk::RGBA {
            self.obj()
                .style_context()
                .lookup_color("window_bg_color")
                .unwrap_or_else(|| {
                    if adw::StyleManager::default().is_dark() {
                        gdk::RGBA::new(0.141, 0.141, 0.141, 1.0)
                    } else {
                        gdk::RGBA::new(0.965, 0.961, 0.949, 1.0)
                    }
                })
        }
        /// Радіус заокруглення кутів зображення з GSettings
        /// (`image-corner-radius`). 0, якщо Settings ще не прив'язано.
        pub(super) fn effective_corner_radius(&self) -> f64 {
            match self.settings.borrow().as_ref() {
                Some(settings) => settings.image_corner_radius(),
                None => 0.0,
            }
        }

        /// Вмикає/вимикає GtkSpinner індикатор завантаження. Сам виклик
        /// відбувається з snapshot() (фаза малювання), а `set_visible`/
        /// `set_spinning` неявно запускають queue_resize() дочірнього
        /// віджета — GTK4 забороняє змінювати дерево віджетів під час
        /// власного проходу вимірювання/виділення/малювання, тож такий
        /// виклик синхронно звідти може або зникнути, або дати
        /// "Gtk-CRITICAL" і не спрацювати. Тому реальну мутацію віджета
        /// відкладаємо через idle-колбек, який виконається вже ПІСЛЯ
        /// поточного кадру.
        /// Вмикає/вимикає GtkSpinner індикатор завантаження синхронно.
        /// Важливо: НЕ відкладати через idle/queue — при швидкому
        /// (локальному) декодуванні `true` і `false` можуть настати в
        /// межах одного цільного циклу подій ще до того, як GTK встигне
        /// скомпонувати кадр із видимим спіннером, і його "блимання" не
        /// потрапить на екран узагалі. Синхронний виклик тут безпечний,
        /// бо розмір/позиція спіннера в size_allocate() більше не
        /// залежать від його видимості (SPINNER_SIZE — стала).
        pub(super) fn set_spinner_active(&self, active: bool) {
            if self.spinner_active.replace(active) == active {
                return;
            }
            self.spinner.set_visible(active);
            self.spinner.set_spinning(active);
        }

        fn set_open_file_button_active(&self, active: bool) {
            if self.open_file_button_active.replace(active) == active {
                return;
            }
            self.open_file_button.set_visible(active);
        }

    }
}

glib::wrapper! {
    pub struct Canvas(ObjectSubclass<imp::Canvas>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Canvas {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_viewer(&self, viewer: Rc<RefCell<Viewer>>) {
        *self.imp().viewer.borrow_mut() = Some(viewer);
        self.queue_draw();
    }

    /// Прив'язує Canvas до Settings для вибору тла та його перемальовування.
    pub fn set_settings(&self, settings: Settings) {
        *self.imp().settings.borrow_mut() = Some(settings);
        self.queue_draw();
    }

    pub fn watch_desktop_wallpaper(&self) {
        const BACKGROUND_SCHEMA: &str = "org.gnome.desktop.background";
        let schema_exists = gtk4::gio::SettingsSchemaSource::default()
            .and_then(|source| source.lookup(BACKGROUND_SCHEMA, true))
            .is_some();
        if !schema_exists {
            return;
        }
        let settings = gtk4::gio::Settings::new(BACKGROUND_SCHEMA);
        let canvas = self.downgrade();
        settings.connect_changed(None, move |_, key| {
            if matches!(key, "picture-uri" | "picture-uri-dark" | "picture-options") {
                if let Some(canvas) = canvas.upgrade() {
                    canvas.queue_draw();
                }
            }
        });
        *self.imp().desktop_background_settings.borrow_mut() = Some(settings);
    }

    pub fn connect_open_file<F: Fn(&gtk4::Button) + 'static>(&self, callback: F) {
        self.imp().open_file_button.connect_clicked(callback);
    }
}

impl Default for Canvas {
    fn default() -> Self {
        Self::new()
    }
}
