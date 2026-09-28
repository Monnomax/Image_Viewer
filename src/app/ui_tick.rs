use crate::thumbnail_strip::{ThumbMsg, ThumbnailStrip};
use crate::viewer::image_loader::{self, LoaderMsg};
use crate::viewer::{Canvas, Viewer};
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

pub(super) struct UiTick {
    pub(super) viewer: Rc<RefCell<Viewer>>,
    pub(super) canvas: Canvas,
    pub(super) window_title: adw::WindowTitle,
    pub(super) filename_label: gtk4::Label,
    pub(super) extra_info_label: gtk4::Label,
    pub(super) strip: Rc<RefCell<ThumbnailStrip>>,
    pub(super) prev_button: gtk4::Button,
    pub(super) next_button: gtk4::Button,
    pub(super) prev_revealer: gtk4::Revealer,
    pub(super) next_revealer: gtk4::Revealer,
    pub(super) filename_revealer: gtk4::Revealer,
    pub(super) last_title_path: Rc<RefCell<Option<PathBuf>>>,
    pub(super) dims_ready: Rc<RefCell<bool>>,
    pub(super) last_dir_sig: Rc<RefCell<(usize, Option<PathBuf>)>>,
    pub(super) last_strip_index: Rc<RefCell<usize>>,
    pub(super) receiver: Rc<RefCell<std::sync::mpsc::Receiver<LoaderMsg>>>,
    pub(super) thumb_receiver: Rc<RefCell<std::sync::mpsc::Receiver<ThumbMsg>>>,
}

impl UiTick {
    fn handle_image_loader_messages(&mut self) {
        while let Ok(msg) = self.receiver.borrow_mut().try_recv() {
            let LoaderMsg::Loaded(path, result) = msg;
            match result {
                Ok(image) => self.viewer.borrow_mut().handle_loaded(path, image),
                Err(error) => {
                    eprintln!("Не вдалося завантажити зображення {:?}: {}", path, error);
                    self.viewer.borrow_mut().handle_load_failed(&path);
                }
            }
            self.canvas.queue_draw();
        }
    }

    fn handle_thumbnail_messages(&mut self) {
        while let Ok(message) = self.thumb_receiver.borrow_mut().try_recv() {
            match message {
                ThumbMsg::Cached { index, bytes } => {
                    if let Ok(pixbuf) = image_loader::decode(&bytes) {
                        self.strip.borrow_mut().handle_loaded(index, pixbuf);
                    } else {
                        self.strip.borrow_mut().handle_failed(index);
                    }
                }
                ThumbMsg::Original {
                    index,
                    path,
                    bytes,
                    cache_key,
                } => {
                    // Спершу GdkPixbuf (швидше, і покриває всі формати, які
                    // вміє система), а якщо не вдалось — запасний шлях через
                    // `image` (потрібен переважно для .avif на системах без
                    // відповідного плагіна gdk-pixbuf — див. коментар біля
                    // decode_fallback()).
                    let decoded = image_loader::decode(&bytes)
                        .or_else(|_| image_loader::decode_fallback(&bytes));
                    match decoded {
                        Ok(full) => {
                            if let Some(scaled) =
                                crate::thumbnail_strip::scale_to_fit_height(&full, 60)
                            {
                                let cache_path = crate::thumbnail_strip::cache_path(&cache_key);
                                if crate::thumbnail_strip::ensure_cache_dir().is_ok() {
                                    let _ = scaled.savev(&cache_path, "png", &[]);
                                }
                                self.strip.borrow_mut().handle_loaded(index, scaled);
                            } else {
                                self.strip.borrow_mut().handle_failed(index);
                            }
                        }
                        Err(_) => self.strip.borrow_mut().handle_failed(index),
                    }
                    drop(path);
                }
                ThumbMsg::Failed { index } => self.strip.borrow_mut().handle_failed(index),
            }
        }
    }

    fn sync_strip_state(&mut self) {
        let (current_len, current_first, current_index) = {
            let viewer = self.viewer.borrow();
            (
                viewer.model.images.len(),
                viewer.model.images.first().cloned(),
                viewer.model.index,
            )
        };
        let current_signature = (current_len, current_first);
        if current_signature != *self.last_dir_sig.borrow() {
            *self.last_dir_sig.borrow_mut() = current_signature;
            *self.last_strip_index.borrow_mut() = current_index;
            let images = self.viewer.borrow().model.images.clone();
            self.strip.borrow_mut().set_images(&images, current_index);
        } else if current_index != *self.last_strip_index.borrow() {
            *self.last_strip_index.borrow_mut() = current_index;
            self.strip.borrow_mut().set_current(current_index);
        }
    }

    fn sync_title(&mut self) {
        let current_path = self.viewer.borrow().model.current_path();
        let path_changed = current_path != *self.last_title_path.borrow();

        if path_changed {
            let has_image = current_path.is_some();
            self.prev_button.set_sensitive(has_image);
            self.next_button.set_sensitive(has_image);
            self.filename_label.set_sensitive(has_image);
            self.strip.borrow().revealer.set_sensitive(has_image);
            self.prev_revealer.set_visible(has_image);
            self.next_revealer.set_visible(has_image);
            self.filename_revealer.set_visible(has_image);
            self.strip.borrow().revealer.set_visible(has_image);

            if let Some(path) = current_path.as_ref() {
                let title = path
                    .file_stem()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if title.is_empty() {
                    self.window_title.set_title("Переглядач зображень");
                } else {
                    self.window_title.set_title(&title);
                }
                self.filename_label.set_label(&title);
            } else {
                self.window_title.set_title("Переглядач зображень");
                self.filename_label.set_label("");
                self.extra_info_label.set_label("");
            }
            *self.last_title_path.borrow_mut() = current_path.clone();
            *self.dims_ready.borrow_mut() = false;
        }

        if let Some(path) = current_path.as_ref() {
            if !*self.dims_ready.borrow() {
                if let Some((width, height)) = self.viewer.borrow_mut().current_image_dimensions() {
                    let size_bytes = std::fs::metadata(path).map(|metadata| metadata.len()).unwrap_or(0);
                    let size_kb = size_bytes as f64 / 1024.0;
                    let size = if size_kb >= 1024.0 {
                        format!("{:.1} Mb", size_kb / 1024.0)
                    } else {
                        format!("{:.1} Kb", size_kb)
                    };
                    let extension = path
                        .extension()
                        .map(|extension| extension.to_string_lossy().to_uppercase())
                        .unwrap_or_default();
                    self.extra_info_label.set_label(&format!(
                        "   •   {} x {}   •   {}   •   {}",
                        width, height, size, extension
                    ));
                    *self.dims_ready.borrow_mut() = true;
                }
            }
        }
    }

    fn update_animations(&mut self) {
        let mut viewer = self.viewer.borrow_mut();

        if viewer.animation_enabled && viewer.zoom_animation.enabled {
            if viewer.apply_animation_step(std::time::Instant::now()) {
                self.canvas.queue_draw();
            }
        } else if viewer.zoom_animation.is_active() {
            let render = viewer.render_view_state();
            viewer
                .zoom_animation
                .sync_from_state(render.scale, render.offset_x, render.offset_y);
        }

        if viewer.crossfade.is_active() {
            viewer.crossfade.update(std::time::Instant::now());
            self.canvas.queue_draw();
        }
        if viewer.animated_gif_step(std::time::Instant::now()) {
            self.canvas.queue_draw();
        }
    }

    pub(super) fn run(&mut self) -> glib::ControlFlow {
        self.handle_image_loader_messages();
        self.handle_thumbnail_messages();
        self.sync_strip_state();
        self.sync_title();
        self.update_animations();
        glib::ControlFlow::Continue
    }
}