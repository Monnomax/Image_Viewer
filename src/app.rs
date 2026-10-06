// app.rs — створює Application/вікно і зв'язує всі модулі докупи.

use adw::prelude::*;
use adw::ApplicationWindow;
use gtk4::prelude::FileExt;
use gtk4::{gdk, glib, Application};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use crate::model::Model;
use crate::preferences::auto_hide::VisibilityMode;
use crate::preferences::Settings;
use crate::thumbnail_strip::{ThumbMsg, ThumbnailStrip};
use crate::utils;
use crate::viewer::{input, Canvas, Viewer};

mod overlays;
mod ui_tick;

use overlays::{
    animation_duration as interface_animation_duration_ms, build_button as build_overlay_button,
    schedule_reveal, should_show_close_button, should_show_element,
    sync_window_theme as sync_window_theme_class, track_hover as track_hover_flag, OverlayWidgets,
};
use ui_tick::UiTick;

/// Перетягування фото у вікно: поки триває drag-over — ховає геть усі
/// елементи інтерфейсу (шапку, кнопки навігації/закриття, підпис файлу,
/// стрічку мініатюр), щоб нічого не заважало бачити майбутнє зображення.
/// При виході курсора за межі вікна без скидання чи одразу після drop —
/// повертає їх до стану, який диктують налаштування. Якщо скинутий файл
/// вдається розпізнати, одразу відкриває його через той самий
/// `utils::resolve_path_arg`, що й аргумент командного рядка при старті.
fn setup_drag_and_drop(
    window: &ApplicationWindow,
    viewer: &Rc<RefCell<Viewer>>,
    canvas: &Canvas,
    settings: &Settings,
    overlays: OverlayWidgets,
) {
    let drop_target = gtk4::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);

    {
        let overlays = overlays.clone();
        drop_target.connect_enter(move |_, _, _| {
            overlays.hide_all();
            gdk::DragAction::COPY
        });
    }

    {
        let overlays = overlays.clone();
        let settings = settings.clone();
        let window = window.clone();
        drop_target.connect_leave(move |_| {
            overlays.restore(&settings, &window);
        });
    }

    {
        let overlays = overlays.clone();
        let settings = settings.clone();
        let window = window.clone();
        let viewer = viewer.clone();
        let canvas = canvas.clone();
        drop_target.connect_drop(move |_, value, _, _| {
            // Елементи повертаємо одразу, незалежно від того, чи вдасться
            // розпізнати й відкрити скинутий файл.
            overlays.restore(&settings, &window);

            let Ok(file_list) = value.get::<gdk::FileList>() else {
                return false;
            };
            let Some(path) = file_list.files().into_iter().next().and_then(|f| f.path()) else {
                return false;
            };

            let (images, index) =
                utils::resolve_path_arg(Some(path.to_string_lossy().into_owned()));
            if images.is_empty() {
                return false;
            }

            viewer.borrow_mut().load_new_source(images, index);
            canvas.queue_draw();
            canvas.grab_focus();
            true
        });
    }

    window.add_controller(drop_target);
}

pub fn build(app: &Application, path_arg: Option<String>, replace_existing: bool) {
    overlays::install_css();

    let settings = Settings::new();
    let windows_to_close = if replace_existing && settings.single_instance() {
        app.windows()
    } else {
        Vec::new()
    };

    let (images, index) = utils::resolve_path_arg(path_arg);

    let mut model = Model::new(images, index);
    model.sort_by(settings.sort_mode());

    let (sender, receiver) = std::sync::mpsc::channel();
    let (thumb_sender, thumb_receiver) = std::sync::mpsc::channel::<ThumbMsg>();

    let viewer = Rc::new(RefCell::new(Viewer::new(model, sender, settings.clone())));
    viewer.borrow_mut().ensure_current_and_neighbors_loaded();

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Переглядач зображень")
        .default_width(800)
        .default_height(600)
        .build();

    let header = adw::HeaderBar::new();
    let window_title = adw::WindowTitle::new("Переглядач зображень", "");
    header.set_title_widget(Some(&window_title));

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);

    let canvas = Canvas::new();
    canvas.set_hexpand(true);
    canvas.set_vexpand(true);
    canvas.set_can_focus(true);
    canvas.set_focusable(true);
    canvas.set_viewer(viewer.clone());
    canvas.set_settings(settings.clone());
    canvas.watch_desktop_wallpaper();
    {
        let window = window.clone();
        let viewer = viewer.clone();
        let canvas = canvas.clone();
        canvas.clone().connect_open_file(move |_| {
            let filter = gtk4::FileFilter::new();
            filter.set_name(Some("Зображення"));
            filter.add_mime_type("image/*");

            let dialog = gtk4::FileDialog::builder()
                .title("Відкрити зображення")
                .modal(true)
                .build();
            dialog.set_default_filter(Some(&filter));

            let viewer = viewer.clone();
            let canvas = canvas.clone();
            dialog.open(Some(&window), None::<&gtk4::gio::Cancellable>, move |result| {
                let Ok(file) = result else {
                    return;
                };
                let Some(path) = file.path() else {
                    return;
                };
                let (images, index) =
                    utils::resolve_path_arg(Some(path.to_string_lossy().into_owned()));
                if images.is_empty() {
                    return;
                }
                viewer.borrow_mut().load_new_source(images, index);
                canvas.queue_draw();
                canvas.grab_focus();
            });
        });
    }

    // --- Кнопка "Назад" у Revealer ---
    let prev_button = build_overlay_button("go-previous-symbolic");
    let prev_revealer = gtk4::Revealer::builder()
        .child(&prev_button)
        .transition_type(gtk4::RevealerTransitionType::Crossfade)
        .transition_duration(interface_animation_duration_ms(&settings))
        .halign(gtk4::Align::Start)
        .valign(gtk4::Align::Center)
        .margin_start(10)
        .reveal_child(should_show_element(&settings, "nav-buttons"))
        .build();

    // --- Кнопка "Вперед" у Revealer ---
    let next_button = build_overlay_button("go-next-symbolic");
    let next_revealer = gtk4::Revealer::builder()
        .child(&next_button)
        .transition_type(gtk4::RevealerTransitionType::Crossfade)
        .transition_duration(interface_animation_duration_ms(&settings))
        .halign(gtk4::Align::End)
        .valign(gtk4::Align::Center)
        .margin_end(10)
        .reveal_child(should_show_element(&settings, "nav-buttons"))
        .build();

    // --- Кнопка "Закрити" у Revealer ---
    let close_button = build_overlay_button("window-close-symbolic");
    let close_revealer = gtk4::Revealer::builder()
        .child(&close_button)
        .transition_type(gtk4::RevealerTransitionType::Crossfade)
        .transition_duration(interface_animation_duration_ms(&settings))
        .halign(gtk4::Align::End)
        .valign(gtk4::Align::Start)
        .margin_top(10)
        .margin_end(10)
        .reveal_child(should_show_close_button(&settings, &window))
        .build();

    // --- Назва відкритого файлу та додаткова інформація у Revealer ---
    let file_info_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    file_info_box.add_css_class("overlay-file-label");
    file_info_box.set_halign(gtk4::Align::Start);
    file_info_box.set_valign(gtk4::Align::Start);
    file_info_box.set_size_request(-1, 50);

    let filename_label = gtk4::Label::new(None);
    file_info_box.append(&filename_label);

    let extra_info_label = gtk4::Label::new(None);
    extra_info_label.set_opacity(0.8); // Зробимо додаткову інформацію трохи прозорішою

    let extra_info_revealer = gtk4::Revealer::builder()
        .child(&extra_info_label)
        .transition_type(gtk4::RevealerTransitionType::SlideRight)
        .transition_duration(interface_animation_duration_ms(&settings))
        .reveal_child(false)
        .build();
    file_info_box.append(&extra_info_revealer);

    // Обробник наведення курсора для розгортання додаткової інформації
    let extra_rev_clone_enter = extra_info_revealer.clone();
    let extra_rev_clone_leave = extra_info_revealer.clone();
    let motion_extra = gtk4::EventControllerMotion::new();
    motion_extra.connect_enter(move |_, _, _| {
        extra_rev_clone_enter.set_reveal_child(true);
    });
    motion_extra.connect_leave(move |_| {
        extra_rev_clone_leave.set_reveal_child(false);
    });
    file_info_box.add_controller(motion_extra);

    // Головний Revealer для автоприховування всього блоку
    let filename_revealer = gtk4::Revealer::builder()
        .child(&file_info_box)
        .transition_type(gtk4::RevealerTransitionType::Crossfade)
        .transition_duration(interface_animation_duration_ms(&settings))
        .halign(gtk4::Align::Start)
        .valign(gtk4::Align::Start)
        .margin_top(10)
        .margin_start(10)
        .reveal_child(should_show_element(&settings, "filename-label"))
        .build();

    {
        let viewer = viewer.clone();
        let canvas_clone = canvas.clone();
        prev_button.connect_clicked(move |_| {
            viewer.borrow_mut().navigate(-1);
            canvas_clone.queue_draw();
        });
    }
    {
        let viewer = viewer.clone();
        let canvas_clone = canvas.clone();
        next_button.connect_clicked(move |_| {
    viewer.borrow_mut().navigate(1);
    canvas_clone.queue_draw();
    canvas_clone.grab_focus();
});
    }
    {
        let window_for_close = window.clone();
        close_button.connect_clicked(move |_| {
            window_for_close.close();
        });
    }

    let canvas_overlay = gtk4::Overlay::new();
    canvas_overlay.set_child(Some(&canvas));
    // Додаємо в оверлей Revealer'и, а не самі кнопки
    canvas_overlay.add_overlay(&prev_revealer);
    canvas_overlay.add_overlay(&next_revealer);
    canvas_overlay.add_overlay(&close_revealer);
    canvas_overlay.add_overlay(&filename_revealer);

    // ---- Стрічка мініатюр ----
    ThumbnailStrip::install_css();
    let strip = Rc::new(RefCell::new(ThumbnailStrip::new(
        settings.clone(),
        thumb_sender,
    )));
    let last_strip_index = Rc::new(RefCell::new(usize::MAX));
    {
    let viewer_c = viewer.clone();
    let canvas_c = canvas.clone();
    let strip_c = strip.clone();
    let last_strip_index_c = last_strip_index.clone();
    strip.borrow_mut().set_on_navigate(move |index| {
        // Оновлюємо мініатюру одразу й без прокрутки — клік уже підтверджує,
        // що вона видима. `last_strip_index` виставляємо тут-таки, щоб
        // наступний тік `sync_strip_state` (рядок ~433) не вирішив, що
        // індекс "змінився ззовні", і не запланував scroll-to-index ще раз.
        strip_c.borrow_mut().set_current_no_scroll(index);
        *last_strip_index_c.borrow_mut() = index;
        viewer_c.borrow_mut().navigate_to(index);
        canvas_c.queue_draw();
    });
}
    {
        let strip_for_scroll = strip.clone();
        strip
            .borrow()
            .scroll
            .hadjustment()
            .connect_value_changed(move |_| {
                strip_for_scroll.borrow_mut().on_scroll_changed();
            });
    }
    {
        let strip_for_size = strip.clone();
        strip
            .borrow()
            .scroll
            .connect_notify_local(Some("width"), move |_, _| {
                strip_for_size.borrow_mut().on_scroll_changed();
            });
    }
    {
        let v = viewer.borrow();
        let images = v.model.images.clone();
        let idx = v.model.index;
        drop(v);
        strip.borrow_mut().set_images(&images, idx);
    }
    let strip_rev_for_autohide = strip.borrow().revealer.clone();

    let prev_hover = Rc::new(Cell::new(false));
    let next_hover = Rc::new(Cell::new(false));
    let close_hover = Rc::new(Cell::new(false));
    let filename_hover = Rc::new(Cell::new(false));
    let strip_hover = Rc::new(Cell::new(false));
    track_hover_flag(&prev_button, prev_hover.clone());
    track_hover_flag(&next_button, next_hover.clone());
    track_hover_flag(&close_button, close_hover.clone());
    track_hover_flag(&file_info_box, filename_hover.clone());
    track_hover_flag(&strip.borrow().scroll, strip_hover.clone());

    // Таймери відкладеної появи елементів (керуються "show-delay-ms").
    // Приховування завжди миттєве, тож окремих таймерів для нього не треба.
    let show_timer_prev: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let show_timer_next: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let show_timer_close: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let show_timer_filename: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let show_timer_strip: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));

    let ui_activity = {
        let prev_rev = prev_revealer.clone();
        let next_rev = next_revealer.clone();
        let close_rev = close_revealer.clone();
        let filename_rev = filename_revealer.clone();
        let strip_rev = strip_rev_for_autohide.clone();
        let prev_hover = prev_hover.clone();
        let next_hover = next_hover.clone();
        let close_hover = close_hover.clone();
        let filename_hover = filename_hover.clone();
        let strip_hover = strip_hover.clone();
        let settings_cb = settings.clone();
        let window_for_autohide = window.clone();
        let show_timer_prev = show_timer_prev.clone();
        let show_timer_next = show_timer_next.clone();
        let show_timer_close = show_timer_close.clone();
        let show_timer_filename = show_timer_filename.clone();
        let show_timer_strip = show_timer_strip.clone();
        move |state: input::AutoHideUiState| {
            let delay_ms = settings_cb.show_delay_ms();
            let nav_mode = VisibilityMode::from_setting(&settings_cb.nav_buttons_visibility_mode());
            if nav_mode == VisibilityMode::AutoHide {
                schedule_reveal(&prev_rev, &show_timer_prev, state.previous || prev_hover.get(), delay_ms);
                schedule_reveal(&next_rev, &show_timer_next, state.next || next_hover.get(), delay_ms);
            }
            let close_mode =
                VisibilityMode::from_setting(&settings_cb.close_button_visibility_mode());
            if close_mode == VisibilityMode::AutoHide {
                schedule_reveal(
                    &close_rev,
                    &show_timer_close,
                    (state.close || close_hover.get()) && window_for_autohide.is_fullscreen(),
                    delay_ms,
                );
            }
            let filename_mode = VisibilityMode::from_setting(&settings_cb.filename_label_visibility_mode());
            if filename_mode == VisibilityMode::AutoHide {
                schedule_reveal(&filename_rev, &show_timer_filename, state.filename || filename_hover.get(), delay_ms);
            }
            let strip_mode =
                VisibilityMode::from_setting(&settings_cb.thumbnail_strip_visibility_mode());
            if strip_mode == VisibilityMode::AutoHide {
                schedule_reveal(&strip_rev, &show_timer_strip, state.strip || strip_hover.get(), delay_ms);
            }
        }
    };

    let is_pointer_over_ui_element = {
        let prev_hover = prev_hover.clone();
        let next_hover = next_hover.clone();
        let close_hover = close_hover.clone();
        let filename_hover = filename_hover.clone();
        let strip_hover = strip_hover.clone();
        move || {
            prev_hover.get()
                || next_hover.get()
                || close_hover.get()
                || filename_hover.get()
                || strip_hover.get()
        }
    };

    let overlays = OverlayWidgets {
        header: header.clone(),
        prev_revealer: prev_revealer.clone(),
        next_revealer: next_revealer.clone(),
        close_revealer: close_revealer.clone(),
        filename_revealer: filename_revealer.clone(),
        strip_revealer: strip_rev_for_autohide.clone(),
    };

    let on_pan_activity = {
        let overlays = overlays.clone();
        let settings = settings.clone();
        let window = window.clone();
        move |hidden: bool| {
            if hidden {
                overlays.hide_all();
            } else {
                overlays.restore(&settings, &window);
            }
        }
    };

    input::setup(
        &window,
        canvas.upcast_ref::<gtk4::Widget>(),
        viewer.clone(),
        settings.clone(),
        ui_activity.clone(),
        is_pointer_over_ui_element,
        on_pan_activity,
    );

    setup_drag_and_drop(&window, &viewer, &canvas, &settings, overlays);

    {
        // Початковий стан кнопок/стрічки/назви файлу: `sync_title()`
        // вимикає їх лише в момент ЗМІНИ шляху (`path_changed`), а якщо
        // застосунок відкрито без жодного зображення (клік по іконці),
        // шлях і до, і після лишається `None` — переходу не відбувається,
        // і без цього явного виклику елементи так і лишились би
        // ввімкненими за замовчуванням.
        let has_image = viewer.borrow().model.current_path().is_some();
        prev_button.set_sensitive(has_image);
        next_button.set_sensitive(has_image);
        filename_label.set_sensitive(has_image);
        strip.borrow().revealer.set_sensitive(has_image);

        prev_revealer.set_visible(has_image);
        next_revealer.set_visible(has_image);
        filename_revealer.set_visible(has_image);
        strip.borrow().revealer.set_visible(has_image);
    }

    {
        let mut tick = UiTick {
            viewer: viewer.clone(),
            canvas: canvas.clone(),
            window_title: window_title.clone(),
            filename_label: filename_label.clone(),
            extra_info_label: extra_info_label.clone(), // Передаємо новий віджет
            strip: strip.clone(),
            prev_button: prev_button.clone(),
            next_button: next_button.clone(),
            prev_revealer: prev_revealer.clone(),
            next_revealer: next_revealer.clone(),
            filename_revealer: filename_revealer.clone(),
            last_title_path: Rc::new(RefCell::new(None)),
            dims_ready: Rc::new(RefCell::new(false)),
            // (len, перший шлях) — сигнатура поточного списку зображень для стрічки
            last_dir_sig: Rc::new(RefCell::new((0, None))),
            last_strip_index: last_strip_index.clone(),
            receiver: Rc::new(RefCell::new(receiver)),
            thumb_receiver: Rc::new(RefCell::new(thumb_receiver)),
        };
        glib::timeout_add_local(Duration::from_millis(16), move || tick.run());
    }

    {
        let canvas_clone = canvas.clone();
        let viewer_clone = viewer.clone();
        let settings_for_changes = settings.clone();
        let prev_rev_clone = prev_revealer.clone();
        let next_rev_clone = next_revealer.clone();
        let close_rev_clone = close_revealer.clone();
        let filename_rev_clone = filename_revealer.clone();
        let strip_for_settings = strip.clone();
        let window_for_settings = window.clone();
        settings.connect_changed(move |key| {
            if key == "image-padding" {
                let mut v = viewer_clone.borrow_mut();
                if !v.is_100 {
                    v.fit_to_screen();
                }
            }
            if key == "smooth-zoom-enabled" || key == "animation-duration-ms" {
                let mut v = viewer_clone.borrow_mut();
                v.animation_enabled = settings_for_changes.smooth_zoom_enabled();
                v.animation_duration_ms = settings_for_changes.animation_duration_ms();
                v.zoom_animation.duration = Duration::from_millis(v.animation_duration_ms as u64);
                v.zoom_animation.enabled = v.animation_enabled;
            }
            if key == "animation-duration-ms" {
                let duration = interface_animation_duration_ms(&settings_for_changes);
                prev_rev_clone.set_transition_duration(duration);
                next_rev_clone.set_transition_duration(duration);
                close_rev_clone.set_transition_duration(duration);
                filename_rev_clone.set_transition_duration(duration);
            }
            if key == "preload-mode" {
                viewer_clone
                    .borrow_mut()
                    .ensure_current_and_neighbors_loaded();
            }
            if key == "remember-zoom" || key == "remember-position" {
                viewer_clone.borrow_mut().restore_current_view_state();
            }
            if key == "min-zoom" || key == "max-zoom" {
                viewer_clone.borrow_mut().clamp_zoom_to_limits();
            }
            if key == "nav-buttons-visibility-mode"
                || key == "show-nav-buttons"
                || key == "hide-cursor-timeout-ms"
            {
                let visible = should_show_element(&settings_for_changes, "nav-buttons");
                prev_rev_clone.set_reveal_child(visible);
                next_rev_clone.set_reveal_child(visible);
            }
            if key == "close-button-visibility-mode"
                || key == "show-close-button"
                || key == "hide-cursor-timeout-ms"
            {
                close_rev_clone.set_reveal_child(should_show_close_button(
                    &settings_for_changes,
                    &window_for_settings,
                ));
            }
            if key == "filename-label-visibility-mode" || key == "hide-cursor-timeout-ms" {
                filename_rev_clone
                    .set_reveal_child(should_show_element(&settings_for_changes, "filename-label"));
            }
            if key == "thumbnail-strip-enabled"
                || key == "thumbnail-strip-visibility-mode"
                || key == "hide-cursor-timeout-ms"
            {
                strip_for_settings.borrow().set_reveal(should_show_element(
                    &settings_for_changes,
                    "thumbnail-strip",
                ));
            }
            if key == "thumbnail-strip-position" {
                let (valign, transition) =
                    if settings_for_changes.thumbnail_strip_position() == "top" {
                        (gtk4::Align::Start, gtk4::RevealerTransitionType::SlideDown)
                    } else {
                        (gtk4::Align::End, gtk4::RevealerTransitionType::SlideUp)
                    };
                strip_for_settings.borrow().widget().set_valign(valign);
                strip_for_settings
                    .borrow()
                    .revealer
                    .set_transition_type(transition);
            }
            if key == "thumbnail-strip-size"
                || key == "thumbnail-strip-padding"
                || key == "thumbnail-strip-margin"
                || key == "thumbnail-strip-spacing"
            {
                strip_for_settings.borrow_mut().refresh_layout();
            }
            if key == "animation-duration-ms" {
                let dur = interface_animation_duration_ms(&settings_for_changes);
                strip_for_settings
                    .borrow()
                    .revealer
                    .set_transition_duration(dur);
            }
            canvas_clone.queue_draw();
        });
    }

    // Стрічка — оверлей поверх канвасу (як кнопки навігації), тому прозорість працює.
    {
        let strip_widget = strip.borrow().widget();
        strip_widget.set_hexpand(true);
        strip_widget.set_valign(if settings.thumbnail_strip_position() == "top" {
            gtk4::Align::Start
        } else {
            gtk4::Align::End
        });
        if settings.thumbnail_strip_position() == "top" {
            strip
                .borrow()
                .revealer
                .set_transition_type(gtk4::RevealerTransitionType::SlideDown);
        }
        canvas_overlay.add_overlay(&strip_widget);
        strip
            .borrow()
            .set_reveal(should_show_element(&settings, "thumbnail-strip"));
    }

    window.set_content(Some(&toolbar_view));
    toolbar_view.set_content(Some(&canvas_overlay));

    header.set_visible(!settings.fullscreen_on_start());
    {
        let header_clone = header.clone();
        let close_rev_clone = close_revealer.clone();
        let settings_for_fullscreen = settings.clone();
        window.connect_notify_local(Some("fullscreened"), move |win, _| {
            header_clone.set_visible(!win.is_fullscreen());
            close_rev_clone
                .set_reveal_child(should_show_close_button(&settings_for_fullscreen, win));
        });
    }

    sync_window_theme_class(&window);
    {
        let canvas_clone = canvas.clone();
        let window_clone = window.clone();
        adw::StyleManager::default().connect_notify_local(Some("dark"), move |_, _| {
            sync_window_theme_class(&window_clone);
            canvas_clone.queue_draw();
        });
    }

    // Якщо на поточному зображенні лишився незбережений поворот (клікнули
    // "Повернути", але не встигли перейти до іншого фото — див.
    // viewer/rotate.rs), закриття вікна — останній момент, коли його
    // можна записати на диск. Без цього обробника такий поворот просто
    // губився б разом із процесом.
    {
        let viewer = viewer.clone();
        window.connect_close_request(move |_| {
            viewer.borrow_mut().commit_pending_rotation();
            glib::Propagation::Proceed
        });
    }

    window.present();
    if settings.fullscreen_on_start() {
        window.fullscreen();
    }
    canvas.grab_focus();

    for window in windows_to_close {
        window.close();
    }
}
