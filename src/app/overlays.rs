use crate::preferences::auto_hide::VisibilityMode;
use crate::preferences::Settings;
use adw::prelude::*;
use adw::ApplicationWindow;
use gtk4::{gdk, glib};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

const CSS: &str = "
.overlay-button {
    min-width: 40px;
    min-height: 40px;
    padding: 5px;
    border-radius: 15px;
    box-shadow: 0px 0px 10px rgba(0, 0, 0, 0.5);
    transition: 150ms;
}
.overlay-button:hover {
    transform: scale(1.10);
}
.overlay-button:active {
    transform: scale(0.90);
}
window.dark .overlay-button {
    background: rgba(0, 0, 0, 0.5);
    color: white;
}
window.dark .overlay-button:hover {
    background: rgba(0, 0, 0, 0.5);
}
window.light .overlay-button {
    background: rgba(255, 255, 255, 0.5);
    color: black;
}
window.light .overlay-button:hover {
    background: rgba(255, 255, 255, 0.5);
}
.overlay-file-label {
    box-shadow: 0px 0px 10px rgba(0, 0, 0, 0.5);
    padding: 8px 20px;
    border-radius: 15px;
    font-weight: 600;
}
window.dark .overlay-file-label {
    background: rgba(0, 0, 0, 0.5);
    color: white;
}
window.light .overlay-file-label {
    background: rgba(255, 255, 255, 0.5);
    color: black;
}
";

pub(super) fn install_css() {
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(CSS);

    if let Some(display) = gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

pub(super) fn build_button(icon_name: &str) -> gtk4::Button {
    let image = gtk4::Image::from_icon_name(icon_name);
    image.set_pixel_size(20);

    let button = gtk4::Button::builder().child(&image).build();
    button.set_size_request(50, 50);
    button.add_css_class("overlay-button");
    button
}

pub(super) fn track_hover<W: IsA<gtk4::Widget>>(widget: &W, flag: Rc<Cell<bool>>) {
    let motion = gtk4::EventControllerMotion::new();
    {
        let flag = flag.clone();
        motion.connect_enter(move |_, _, _| flag.set(true));
    }
    motion.connect_leave(move |_| flag.set(false));
    widget.add_controller(motion);
}

pub(super) fn sync_window_theme(window: &ApplicationWindow) {
    if adw::StyleManager::default().is_dark() {
        window.remove_css_class("light");
        window.add_css_class("dark");
    } else {
        window.remove_css_class("dark");
        window.add_css_class("light");
    }
}

pub(super) fn animation_duration(settings: &Settings) -> u32 {
    settings.animation_duration_ms().min(1000)
}

pub(super) fn should_show_element(settings: &Settings, element: &str) -> bool {
    let mode = match element {
        "nav-buttons" => VisibilityMode::from_setting(&settings.nav_buttons_visibility_mode()),
        "close-button" => VisibilityMode::from_setting(&settings.close_button_visibility_mode()),
        "filename-label" => {
            VisibilityMode::from_setting(&settings.filename_label_visibility_mode())
        }
        "thumbnail-strip" => {
            if !settings.thumbnail_strip_enabled() {
                return false;
            }
            VisibilityMode::from_setting(&settings.thumbnail_strip_visibility_mode())
        }
        _ => return true,
    };
    mode != VisibilityMode::AutoHide && mode != VisibilityMode::NeverShow
}

pub(super) fn should_show_close_button(settings: &Settings, window: &ApplicationWindow) -> bool {
    window.is_fullscreen() && should_show_element(settings, "close-button")
}

pub(super) fn schedule_reveal(
    revealer: &gtk4::Revealer,
    timer: &Rc<RefCell<Option<glib::SourceId>>>,
    desired_visible: bool,
    delay_ms: u32,
) {
    if desired_visible {
        if revealer.reveals_child() || timer.borrow().is_some() {
            return;
        }
        if delay_ms == 0 {
            revealer.set_reveal_child(true);
            return;
        }
        let revealer_for_timer = revealer.clone();
        let timer_for_clear = timer.clone();
        let id = glib::timeout_add_local(Duration::from_millis(delay_ms as u64), move || {
            revealer_for_timer.set_reveal_child(true);
            *timer_for_clear.borrow_mut() = None;
            glib::ControlFlow::Break
        });
        *timer.borrow_mut() = Some(id);
    } else {
        if let Some(id) = timer.borrow_mut().take() {
            id.remove();
        }
        revealer.set_reveal_child(false);
    }
}

#[derive(Clone)]
pub(super) struct OverlayWidgets {
    pub(super) header: adw::HeaderBar,
    pub(super) prev_revealer: gtk4::Revealer,
    pub(super) next_revealer: gtk4::Revealer,
    pub(super) close_revealer: gtk4::Revealer,
    pub(super) filename_revealer: gtk4::Revealer,
    pub(super) strip_revealer: gtk4::Revealer,
}

impl OverlayWidgets {
    pub(super) fn hide_all(&self) {
        self.header.set_visible(false);
        self.prev_revealer.set_reveal_child(false);
        self.next_revealer.set_reveal_child(false);
        self.close_revealer.set_reveal_child(false);
        self.filename_revealer.set_reveal_child(false);
        self.strip_revealer.set_reveal_child(false);
    }

    pub(super) fn restore(&self, settings: &Settings, window: &ApplicationWindow) {
        self.header.set_visible(!window.is_fullscreen());
        let nav_visible = should_show_element(settings, "nav-buttons");
        self.prev_revealer.set_reveal_child(nav_visible);
        self.next_revealer.set_reveal_child(nav_visible);
        self.close_revealer
            .set_reveal_child(should_show_close_button(settings, window));
        self.filename_revealer
            .set_reveal_child(should_show_element(settings, "filename-label"));
        self.strip_revealer
            .set_reveal_child(should_show_element(settings, "thumbnail-strip"));
    }
}
