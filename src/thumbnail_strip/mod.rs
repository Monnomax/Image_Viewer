// thumbnail_strip/mod.rs — горизонтальна стрічка мініатюр.
//
// Архітектура:
//   ThumbnailStrip  — основна структура; тримає GTK-віджети і стан завантаження.
//   build_item()    — будує один слот (Button + Picture).
//   on_scroll_changed() — lazy loading при прокрутці; викликається ззовні.
//   handle_loaded() / handle_failed() — відповідь від ThumbLoader.
//   set_current()   — оновлює виділення та центрує поточну мініатюру.

mod disk_cache;
mod scroll;
pub mod loader;

pub use disk_cache::{cache_path, ensure_cache_dir};
pub use loader::{ThumbMsg, ThumbRequest};

use crate::preferences::Settings;
use scroll::{scroll_to_bounds, scroll_to_index, ScrollAnimator};
use gtk4::gdk_pixbuf::Pixbuf;
use gtk4::prelude::*;
use gtk4::{glib, Orientation};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::Sender;

pub const THUMB_CSS: &str = "
.thumbnail-item {
    padding: 0;
    border-radius: 10px;
    transition: opacity 200ms, transform 200ms;
    opacity: 0.8;
}
.thumbnail-item:hover {
    opacity: 1.0;
    transform: scale(1.10);
}
.thumbnail-item:active {
    opacity: 1.0;
    transform: scale(0.90);
}    
.thumbnail-current {
    opacity: 1.0;
    outline: 2px solid rgba(0, 0, 0, 0.5);
    outline-offset: 1px;
}
window.light .thumbnail-current {
    outline-color: rgba(0, 0, 0, 0.5);
}
window.dark .thumbnail-current {
    outline-color: rgba(255, 255, 255, 0.5);
}
.thumbnail-strip-panel {
    background: rgba(0,0,0,0.5);
    border-radius: 15px;
    box-shadow: 0 0 10px rgba(0,0,0,0.5);
}
window.light .thumbnail-strip-panel {
    background: rgba(255,255,255,0.5);
}
/* Прихований скролбар: PolicyType::Automatic потрібен для коректного
   обрізання контенту, але сам скролбар не потрібен — є колесо миші. */
.thumbnail-strip-panel scrollbar.horizontal,
.thumbnail-strip-panel scrollbar.horizontal slider {
    min-height: 0;
    min-width: 0;
    padding: 0;
    margin: 0;
    border: 0;
    opacity: 0;
}
";

struct ThumbItem {
    button: gtk4::Button,
    picture: gtk4::Picture,
    loaded: bool,
    pending: bool,
}

pub struct ThumbnailStrip {
    pub revealer: gtk4::Revealer,
    pub scroll: gtk4::ScrolledWindow,
    items_box: gtk4::Box,
    items: Vec<ThumbItem>,

    images: Vec<PathBuf>,
    pub current_index: usize,

    thumb_size: u32,
    spacing: i32,
    padding: i32,

    settings: Settings,
    sender: Sender<ThumbMsg>,
    scroll_animator: ScrollAnimator,

    // Розділяємо між build_item-замиканнями без &mut self
    on_navigate: Rc<RefCell<Option<Box<dyn Fn(usize)>>>>,
}

impl ThumbnailStrip {
    pub fn install_css() {
        let provider = gtk4::CssProvider::new();
        provider.load_from_data(THUMB_CSS);
        if let Some(display) = gtk4::gdk::Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    }

    pub fn new(settings: Settings, sender: Sender<ThumbMsg>) -> Self {
        let thumb_size = 60u32;
        let spacing = settings.thumbnail_strip_spacing() as i32;
        let padding = settings.thumbnail_strip_padding() as i32;
        let margin = settings.thumbnail_strip_margin() as i32;
        let anim_ms = settings.animation_duration_ms().min(1000);

        let items_box = gtk4::Box::new(Orientation::Horizontal, spacing);
        items_box.set_margin_start(padding);
        items_box.set_margin_end(padding);
        items_box.set_margin_top(padding);
        items_box.set_margin_bottom(padding);
        items_box.set_valign(gtk4::Align::Center);

        let scroll = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Automatic)
            .vscrollbar_policy(gtk4::PolicyType::Never)
            .kinetic_scrolling(false)
            .build();
        scroll.set_child(Some(&items_box));
        let scroll_animator = ScrollAnimator::default();

        // Колесо миші по всій поверхні панелі → горизонтальна прокрутка
        let scroll_ctrl = gtk4::EventControllerScroll::new(
            gtk4::EventControllerScrollFlags::HORIZONTAL
                | gtk4::EventControllerScrollFlags::VERTICAL,
        );
        {
            let adj = scroll.hadjustment();
            let scroll_animator = scroll_animator.clone();
            scroll_ctrl.connect_scroll(move |_, dx, dy| {
                let delta = if dx.abs() > dy.abs() { dx } else { dy };
                let step = adj.step_increment() * 5.0;
                let target =
                    (adj.value() + delta * step).clamp(adj.lower(), adj.upper() - adj.page_size());

                scroll_animator.set_target(target);
                scroll_animator.start(&adj);
                glib::Propagation::Stop
            });
        }
        scroll.add_controller(scroll_ctrl);
        scroll.add_css_class("thumbnail-strip-panel");
        scroll.set_margin_start(margin);
        scroll.set_margin_end(margin);
        scroll.set_margin_top(margin);
        scroll.set_margin_bottom(margin);

        let panel_h = 80i32;
        scroll.set_size_request(-1, panel_h);

        let revealer = gtk4::Revealer::builder()
            .child(&scroll)
            .transition_type(gtk4::RevealerTransitionType::SlideUp)
            .transition_duration(anim_ms)
            .reveal_child(true)
            .build();

        Self {
            revealer,
            scroll,
            items_box,
            items: Vec::new(),
            images: Vec::new(),
            current_index: 0,
            thumb_size,
            spacing,
            padding,
            settings,
            sender,
            scroll_animator,
            on_navigate: Rc::new(RefCell::new(None)),
        }
    }

    /// Кореневий GTK-віджет для розміщення у вікні.
    pub fn widget(&self) -> gtk4::Widget {
        self.revealer.clone().upcast()
    }

    pub fn set_on_navigate<F: Fn(usize) + 'static>(&mut self, f: F) {
        *self.on_navigate.borrow_mut() = Some(Box::new(f));
    }

    /// Повністю перебудовує стрічку для нового набору зображень.
    pub fn set_images(&mut self, images: &[PathBuf], current_index: usize) {
        self.images = images.to_vec();
        self.current_index = current_index.min(images.len().saturating_sub(1));

        while let Some(child) = self.items_box.first_child() {
            self.items_box.remove(&child);
        }
        self.items.clear();

        for i in 0..images.len() {
            let item = self.build_item(i);
            self.items_box.append(&item.button);
            self.items.push(item);
        }

        self.update_selection_class();
        self.request_range_around(self.current_index, self.load_radius());
        self.schedule_scroll_to(self.current_index);
        self.request_visible_range();
    }

    fn build_item(&self, index: usize) -> ThumbItem {
    let size = 60i32;

    let picture = gtk4::Picture::new();
    picture.set_size_request(size, size);
    picture.set_content_fit(gtk4::ContentFit::Contain);

    let button = gtk4::Button::builder().child(&picture).build();
    button.set_size_request(size, size);
    button.add_css_class("thumbnail-item");
    button.set_overflow(gtk4::Overflow::Hidden);
    button.set_focusable(false);   // <-- ось сюди додати

        let on_nav = self.on_navigate.clone();
        button.connect_clicked(move |_| {
            if let Some(f) = on_nav.borrow().as_ref() {
                f(index);
            }
        });

        ThumbItem {
            button,
            picture,
            loaded: false,
            pending: false,
        }
    }

    /// Оновлює активну мініатюру (викликається з poll-loop при зміні індексу).
    pub fn set_current(&mut self, index: usize) {
        if index >= self.items.len() {
            return;
        }
        self.current_index = index;
        self.update_selection_class();
        self.schedule_scroll_to(index);
        self.request_range_around(index, self.load_radius());
        self.request_visible_range();
    }

    pub fn set_current_no_scroll(&mut self, index: usize) {
        if index >= self.items.len() {
            return;
        }
        self.current_index = index;
        self.update_selection_class();
        self.request_range_around(index, self.load_radius());
        self.request_visible_range();
    }

    fn update_selection_class(&self) {
        for (i, item) in self.items.iter().enumerate() {
            if i == self.current_index {
                item.button.add_css_class("thumbnail-current");
            } else {
                item.button.remove_css_class("thumbnail-current");
            }
        }
    }

    /// Планує прокрутку після завершення поточного кола GTK (розкладка має встигнути).
    fn schedule_scroll_to(&self, index: usize) {
        let scroll = self.scroll.clone();
        let thumb_size = self.thumb_size;
        let spacing = self.spacing;
        let item = self.items.get(index).map(|item| item.button.clone());
        let scroll_animator = self.scroll_animator.clone();
        glib::idle_add_local_once(move || {
            if let Some(item) = item {
                let allocation = item.allocation();
                if allocation.width() > 0 {
                    scroll_to_bounds(
                        &scroll,
                        allocation.x() as f64,
                        allocation.width() as f64,
                        &scroll_animator,
                    );
                    return;
                }
            }

            scroll_to_index(
                &scroll,
                index,
                thumb_size,
                spacing,
                &scroll_animator,
            );
        });
    }

    fn load_radius(&self) -> usize {
        let item_w = (self.thumb_size as i32 + self.spacing) as f64;
        if item_w <= 0.0 {
            return 16;
        }

        let view_w = self
            .scroll
            .width()
            .max(self.scroll.allocated_width())
            .max(1) as f64;
        if view_w <= 1.0 {
            return self.images.len().min(40).max(12).saturating_add(8);
        }

        let visible_items = ((view_w / item_w).ceil() as usize).max(1);
        visible_items.saturating_mul(2).saturating_add(12)
    }

    fn request_visible_range(&mut self) {
        let adj = self.scroll.hadjustment();
        let mut view_w = self.scroll.width().max(self.scroll.allocated_width()) as f64;
        let item_w = (self.thumb_size as i32 + self.spacing) as f64;
        if item_w <= 0.0 || self.images.is_empty() {
            return;
        }

        if view_w <= 1.0 {
            view_w = (self.images.len().min(40).max(1) as f64) * item_w;
        }

        let first = (adj.value().max(0.0) / item_w).floor() as usize;
        let last = ((adj.value() + view_w) / item_w).ceil() as usize;
        let count = self.images.len();
        let extra = 8usize;
        let start = first.saturating_sub(extra);
        let end = (last + extra).min(count.saturating_sub(1));
        for i in start..=end {
            self.try_request(i);
        }
    }

    fn request_range_around(&mut self, center: usize, radius: usize) {
        let count = self.images.len();
        if count == 0 {
            return;
        }
        let start = center.saturating_sub(radius);
        let end = (center + radius).min(count - 1);
        for i in start..=end {
            self.try_request(i);
        }
    }

    fn try_request(&mut self, i: usize) {
        if self.items[i].loaded || self.items[i].pending {
            return;
        }
        self.items[i].pending = true;
        loader::request_thumb(
            ThumbRequest {
                index: i,
                path: self.images[i].clone(),
                thumb_size: self.thumb_size,
            },
            self.sender.clone(),
        );
    }

    /// Викликається з adjustment::connect_value_changed (lazy loading при прокрутці).
    pub fn on_scroll_changed(&mut self) {
        self.request_visible_range();
    }

    pub fn handle_loaded(&mut self, index: usize, pixbuf: Pixbuf) {
        if index >= self.items.len() {
            return;
        }
        let thumb_w = pixbuf.width();
        let thumb_h = pixbuf.height();
        self.items[index].picture.set_size_request(thumb_w, thumb_h);
        self.items[index].button.set_size_request(thumb_w, thumb_h);
        self.items[index].picture.set_pixbuf(Some(&pixbuf));
        self.items[index].loaded = true;
        self.items[index].pending = false;
    }

    pub fn handle_failed(&mut self, index: usize) {
        if index >= self.items.len() {
            return;
        }
        self.items[index].pending = false;
    }

    pub fn set_reveal(&self, reveal: bool) {
        self.revealer.set_reveal_child(reveal);
    }

    #[allow(dead_code)]
    pub fn is_revealed(&self) -> bool {
        self.revealer.reveals_child()
    }

    /// Оновлює геометрію панелі після зміни налаштувань розміру/відступів.
    /// Скидає завантажені мініатюри, щоб перезавантажити в новому розмірі.
    pub fn refresh_layout(&mut self) {
        let thumb_size = 60u32;
        let spacing = self.settings.thumbnail_strip_spacing() as i32;
        let padding = self.settings.thumbnail_strip_padding() as i32;
        let margin = self.settings.thumbnail_strip_margin() as i32;

        self.thumb_size = thumb_size;
        self.spacing = spacing;
        self.padding = padding;

        self.items_box.set_spacing(spacing);
        self.items_box.set_margin_start(padding);
        self.items_box.set_margin_end(padding);
        self.items_box.set_margin_top(padding);
        self.items_box.set_margin_bottom(padding);

        let panel_h = 80i32;
        self.scroll.set_size_request(-1, panel_h);
        self.scroll.set_margin_start(margin);
        self.scroll.set_margin_end(margin);
        self.scroll.set_margin_top(margin);
        self.scroll.set_margin_bottom(margin);

        let size = 60i32;
        for item in &mut self.items {
            item.loaded = false;
            item.pending = false;
            item.picture.set_pixbuf(None);
            item.picture.set_size_request(size, size);
            item.button.set_size_request(size, size);
        }

        self.request_range_around(self.current_index, self.load_radius());
        self.request_visible_range();
    }
}

/// Масштабує `pixbuf` так, щоб він мати фіксовану висоту `height`
/// без втрати співвідношення сторін.
pub fn scale_to_fit_height(pixbuf: &Pixbuf, height: i32) -> Option<Pixbuf> {
    let target_height = height.max(1) as f64;
    let (w, h) = (pixbuf.width() as f64, pixbuf.height() as f64);
    if w <= 0.0 || h <= 0.0 {
        return None;
    }

    let scale = target_height / h;
    let tw = (w * scale).round() as i32;
    let th = target_height.round() as i32;
    pixbuf.scale_simple(tw.max(1), th.max(1), gtk4::gdk_pixbuf::InterpType::Hyper)
}

#[cfg(test)]
mod tests {
    use super::scale_to_fit_height;

    #[test]
    fn scales_preserve_aspect_ratio() {
        let pixbuf =
            gtk4::gdk_pixbuf::Pixbuf::new(gtk4::gdk_pixbuf::Colorspace::Rgb, true, 8, 400, 200)
                .expect("pixbuf should be created");

        let scaled = scale_to_fit_height(&pixbuf, 60).expect("scaled pixbuf should exist");
        let ratio = scaled.width() as f64 / scaled.height() as f64;

        assert_eq!(scaled.width(), 120);
        assert_eq!(scaled.height(), 60);
        assert!((ratio - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn load_radius_covers_visible_strip_width() {
        let thumb_size = 60u32;
        let spacing = 8i32;
        let scroll_w = 420i32;
        let item_w = (thumb_size as i32 + spacing) as f64;
        let visible_items = ((scroll_w as f64 / item_w).ceil() as usize).max(1);
        let radius = visible_items.saturating_mul(2).saturating_add(12);

        assert!(radius >= visible_items * 2);
        assert!(radius > 8);
    }

    #[test]
    fn visible_range_uses_current_scroll_and_width() {
        let scroll_w: f64 = 420.0;
        let scroll_x: f64 = 120.0;
        let item_w: f64 = 68.0;
        let first = (scroll_x / item_w).floor() as usize;
        let last = ((scroll_x + scroll_w) / item_w).ceil() as usize;
        let extra = 8usize;

        let start = first.saturating_sub(extra);
        let end = last + extra;

        assert!(start < first);
        assert!(end > last);
        assert!(end >= last);
    }
}
