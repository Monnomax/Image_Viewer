// input.rs — уся робота з мишею/клавіатурою/жестами зібрана тут.
// Контролери лише зчитують події і викликають методи Viewer; жодної
// бізнес-логіки масштабу/навігації в цьому файлі немає.

use crate::preferences::auto_hide::VisibilityMode;
use crate::preferences::Settings;
use crate::viewer::menu;
use crate::viewer::Viewer;
use adw::ApplicationWindow;
use gtk4::prelude::*;
use gtk4::{gdk, glib, PopoverMenu, Widget};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AutoHideUiState {
    pub cursor: bool,
    pub close: bool,
    pub previous: bool,
    pub next: bool,
    pub filename: bool,
    pub strip: bool,
}

impl AutoHideUiState {
    pub fn all(visible: bool) -> Self {
        Self {
            cursor: visible,
            close: visible,
            previous: visible,
            next: visible,
            filename: visible,
            strip: visible,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SensitivityZone {
    TopLeft,
    TopRight,
    MiddleLeft,
    MiddleRight,
    Bottom,
}

#[allow(dead_code)]
pub fn pointer_in_sensitivity_zone(
    x: f64,
    y: f64,
    area_w: f64,
    area_h: f64,
    zone: SensitivityZone,
) -> bool {
    if area_w <= 0.0 || area_h <= 0.0 {
        return false;
    }

    let x_ratio = x / area_w;
    let y_ratio = y / area_h;

    match zone {
        SensitivityZone::TopLeft => x_ratio <= 0.5 && y_ratio <= 0.15,
        SensitivityZone::TopRight => x_ratio > 0.5 && y_ratio <= 0.15,
        SensitivityZone::MiddleLeft => x_ratio <= 0.5 && y_ratio > 0.15 && y_ratio <= 0.85,
        SensitivityZone::MiddleRight => x_ratio > 0.5 && y_ratio > 0.15 && y_ratio <= 0.85,
        SensitivityZone::Bottom => y_ratio > 0.85,
    }
}

pub fn pointer_in_auto_hide_zone_for_element(
    element: &str,
    x: f64,
    y: f64,
    area_w: f64,
    area_h: f64,
) -> bool {
    if area_w <= 0.0 || area_h <= 0.0 {
        return false;
    }

    let x_ratio = x / area_w;
    let y_ratio = y / area_h;

    match element {
        "filename" => x_ratio <= 0.5 && y_ratio <= 0.15,
        "close" => x_ratio > 0.5 && y_ratio <= 0.15,
        "previous" => x_ratio <= 0.5 && y_ratio > 0.15 && y_ratio <= 0.85,
        "next" => x_ratio > 0.5 && y_ratio > 0.15 && y_ratio <= 0.85,
        "strip" => y_ratio > 0.85,
        _ => false,
    }
}

pub fn setup<
    F: Fn(AutoHideUiState) + Clone + 'static,
    G: Fn() -> bool + Clone + 'static,
    H: Fn(bool) + Clone + 'static,
>(
    window: &ApplicationWindow,
    area: &Widget,
    viewer: Rc<RefCell<Viewer>>,
    settings: Settings,
    on_ui_activity: F,
    is_pointer_over_ui_element: G,
    on_pan_activity: H,
) {
    // Прапорець "зараз триває панорамування" — спільний для
    // setup_pointer_tracking і setup_drag_pan. Поки він true, рух миші не
    // має повертати назад елементи, приховані на початку перетягування
    // (інакше кожен рух курсора під час пана одразу ж скасовував би
    // приховування через звичайну auto-hide-логіку наведення).
    let panning: Rc<std::cell::Cell<bool>> = Rc::new(std::cell::Cell::new(false));
    // Таймер приховування курсора (hide-cursor-timeout-ms) — спільний з
    // setup_drag_pan, щоб на початку перетягування можна було скасувати
    // вже заплановане приховування курсора: інакше воно могло б спрацювати
    // посеред пана й приховати курсор поверх виставленого "grabbing".
    let cursor_timeout_id: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));

    setup_scroll_zoom(area, &viewer, &settings);
    setup_pointer_tracking(
        area,
        &viewer,
        &settings,
        on_ui_activity.clone(),
        is_pointer_over_ui_element.clone(),
        panning.clone(),
        cursor_timeout_id.clone(),
    );
    setup_drag_pan(
        area,
        &viewer,
        &settings,
        on_ui_activity,
        is_pointer_over_ui_element,
        on_pan_activity,
        panning,
        cursor_timeout_id,
    );
    setup_middle_click_zoom_toggle(area, &viewer);
    setup_escape_to_quit(window);

    let popover = menu::setup(window, area, viewer.clone(), settings.clone());
    setup_right_click_press(area, popover);
}

fn ui_state_for_pointer(x: f64, y: f64, area_w: f64, area_h: f64) -> AutoHideUiState {
    let mut state = AutoHideUiState::all(false);
    state.cursor = true;

    if pointer_in_auto_hide_zone_for_element("filename", x, y, area_w, area_h) {
        state.filename = true;
    }
    if pointer_in_auto_hide_zone_for_element("close", x, y, area_w, area_h) {
        state.close = true;
    }
    if pointer_in_auto_hide_zone_for_element("previous", x, y, area_w, area_h) {
        state.previous = true;
    }
    if pointer_in_auto_hide_zone_for_element("next", x, y, area_w, area_h) {
        state.next = true;
    }
    if pointer_in_auto_hide_zone_for_element("strip", x, y, area_w, area_h) {
        state.strip = true;
    }

    state
}

fn timeout_hidden_state(
    auto_hide_cursor: bool,
    auto_hide_close: bool,
    auto_hide_nav: bool,
    auto_hide_filename: bool,
    auto_hide_strip: bool,
) -> AutoHideUiState {
    let mut state = AutoHideUiState::all(false);

    // Після таймера елементи з режимом AutoHide мають бути приховані,
    // незалежно від того, чи курсор залишився в чутливій зоні.
    state.cursor = !auto_hide_cursor;
    state.close = !auto_hide_close;
    state.previous = !auto_hide_nav;
    state.next = !auto_hide_nav;
    state.filename = !auto_hide_filename;
    state.strip = !auto_hide_strip;

    state
}

fn any_auto_hide_enabled(
    cursor: bool,
    close: bool,
    nav: bool,
    filename: bool,
    strip: bool,
) -> bool {
    cursor || close || nav || filename || strip
}

/// Колесо миші.
///
/// У межах вузької зони навігації біля правого краю вікна (ширина
/// nav_zone_width із налаштувань, на всю висоту) колесо перемикає зображення: вгору —
/// попереднє, вниз — наступне (або навпаки, якщо увімкнено "Інвертувати
/// колесо"). Поза цією зоною колесо масштабує зображення з коефіцієнтом
/// zoom-speed, прив'язуючись до останньої відомої позиції курсора.
fn setup_scroll_zoom(area: &Widget, viewer: &Rc<RefCell<Viewer>>, settings: &Settings) {
    let viewer = viewer.clone();
    let settings = settings.clone();
    let area_clone = area.clone();
    let scroll = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
    scroll.connect_scroll(move |_, _dx, dy| {
        let area_width = area_clone.width() as f64;
        let nav_zone_width = settings.nav_zone_width();
        let mut v = viewer.borrow_mut();
        let in_nav_zone = area_width > 0.0 && v.last_pointer.0 >= area_width - nav_zone_width;

        let dy = if settings.invert_scroll() { dy } else { -dy };

        if in_nav_zone {
            if dy < 0.0 {
                v.navigate(1);
            } else {
                v.navigate(-1);
            }
        } else {
            let anchor = v.last_pointer;
            let speed = settings.zoom_speed();
            let factor = if dy < 0.0 { speed } else { 1.0 / speed };
            v.zoom_by(factor, anchor);
        }

        drop(v);
        area_clone.queue_draw();
        glib::Propagation::Stop
    });
    area.add_controller(scroll);
}

/// Відстежує позицію курсора для прив'язки масштабування колесом і для
/// визначення, чи курсор перебуває в зоні навігації біля правого краю.
/// Також приховує курсор після затримки hide_cursor_timeout_ms із налаштувань.
/// Спільна логіка "показати курсор і UI зараз, запланувати приховування
/// через hide-cursor-timeout-ms" — використовується і рухом миші, і
/// закінченням перетягування (`setup_drag_pan`), щоб курсор почав
/// поводитись за налаштуваннями авто-приховування навіть якщо після
/// відпускання кнопки він лишився на місці й нових рухів миші не було.
#[allow(clippy::too_many_arguments)]
fn schedule_cursor_autohide<F, G>(
    area: &Widget,
    settings: &Settings,
    timeout_id: &Rc<RefCell<Option<glib::SourceId>>>,
    on_ui_activity: &F,
    is_pointer_over_ui_element: &G,
    panning: &Rc<std::cell::Cell<bool>>,
    ui_state_now: AutoHideUiState,
) where
    F: Fn(AutoHideUiState) + Clone + 'static,
    G: Fn() -> bool + Clone + 'static,
{
    area.set_cursor_from_name(None);

    if let Some(id) = timeout_id.borrow_mut().take() {
        id.remove();
    }

    let timeout_ms = settings.hide_cursor_timeout_ms();

    let auto_hide_cursor = matches!(
        VisibilityMode::from_setting(&settings.cursor_visibility_mode()),
        VisibilityMode::AutoHide
    );
    let auto_hide_close = matches!(
        VisibilityMode::from_setting(&settings.close_button_visibility_mode()),
        VisibilityMode::AutoHide
    );
    let auto_hide_nav = matches!(
        VisibilityMode::from_setting(&settings.nav_buttons_visibility_mode()),
        VisibilityMode::AutoHide
    );
    let auto_hide_filename = matches!(
        VisibilityMode::from_setting(&settings.filename_label_visibility_mode()),
        VisibilityMode::AutoHide
    );
    let auto_hide_strip = matches!(
        VisibilityMode::from_setting(&settings.thumbnail_strip_visibility_mode()),
        VisibilityMode::AutoHide
    );
    let any_auto_hide = any_auto_hide_enabled(
        auto_hide_cursor,
        auto_hide_close,
        auto_hide_nav,
        auto_hide_filename,
        auto_hide_strip,
    );

    on_ui_activity(ui_state_now);

    if any_auto_hide && timeout_ms > 0 {
        let area_for_timer = area.clone();
        let timeout_id_callback = timeout_id.clone();
        let on_ui_activity_timer = on_ui_activity.clone();
        let is_pointer_over_ui_timer = is_pointer_over_ui_element.clone();
        let panning_for_timer = panning.clone();

        *timeout_id.borrow_mut() = Some(glib::timeout_add_local(
            Duration::from_millis(timeout_ms as u64),
            move || {
                if !panning_for_timer.get() {
                    let pointer_over_ui = is_pointer_over_ui_timer();
                    if auto_hide_cursor && !pointer_over_ui {
                        area_for_timer.set_cursor_from_name(Some("none"));
                    } else {
                        area_for_timer.set_cursor_from_name(None);
                    }
                }

                let hidden_state = timeout_hidden_state(
                    auto_hide_cursor,
                    auto_hide_close,
                    auto_hide_nav,
                    auto_hide_filename,
                    auto_hide_strip,
                );

                on_ui_activity_timer(hidden_state);

                *timeout_id_callback.borrow_mut() = None;
                glib::ControlFlow::Break
            },
        ));
    }
}

fn setup_pointer_tracking<
    F: Fn(AutoHideUiState) + Clone + 'static,
    G: Fn() -> bool + Clone + 'static,
>(
    area: &Widget,
    viewer: &Rc<RefCell<Viewer>>,
    settings: &Settings,
    on_ui_activity: F,
    is_pointer_over_ui_element: G,
    panning: Rc<std::cell::Cell<bool>>,
    timeout_id: Rc<RefCell<Option<glib::SourceId>>>,
) {
    let viewer = viewer.clone();
    let settings = settings.clone();
    let area_clone = area.clone();
    let motion = gtk4::EventControllerMotion::new();

    let timeout_id_motion = timeout_id.clone();
    let on_ui_activity_motion = on_ui_activity.clone();
    let settings_motion = settings.clone();

    motion.connect_motion(move |_, x, y| {
        // Поки триває панорамування (ліва кнопка затиснена й тягне
        // зображення) — рух курсора не повинен повертати назад елементи,
        // приховані setup_drag_pan; on_pan_activity(false) зробить це сам,
        // коли перетягування завершиться.
        if panning.get() {
            return;
        }

        // Запобігаємо циклічному блиманню через синтетичні події GTK:
        // продовжуємо роботу тільки якщо курсор дійсно змінив позицію
        let last_ptr = viewer.borrow().last_pointer;
        if last_ptr.0 == x && last_ptr.1 == y {
            return;
        }

        viewer.borrow_mut().last_pointer = (x, y);

        let ui_state_now = if settings_motion.sensitivity_zones_enabled() {
            let width = area_clone.width() as f64;
            let height = area_clone.height() as f64;
            ui_state_for_pointer(x, y, width, height)
        } else {
            AutoHideUiState::all(true)
        };

        schedule_cursor_autohide(
            &area_clone,
            &settings_motion,
            &timeout_id_motion,
            &on_ui_activity_motion,
            &is_pointer_over_ui_element,
            &panning,
            ui_state_now,
        );
    });

    area.add_controller(motion);
}

/// Ліва кнопка: перетягування панорамує зображення. Поки триває
/// перетягування — `on_pan_activity(true)` ховає всі елементи інтерфейсу
/// (щоб не заважали дивитись на зображення), а по завершенню —
/// `on_pan_activity(false)` повертає їх назад.
/// Ліва кнопка: перетягування панорамує зображення. Поки триває
/// перетягування — `on_pan_activity(true)` ховає всі елементи інтерфейсу
/// (щоб не заважали дивитись на зображення), а по завершенню —
/// `on_pan_activity(false)` повертає їх назад. Курсор одразу після
/// відпускання кнопки ставиться на розрахунок таймера авто-приховування
/// (`schedule_cursor_autohide`) — без цього, якщо після пана курсор
/// лишити на місці, він назавжди залишався б видимим: без нового руху
/// миші не було б і нового запланованого приховування.
#[allow(clippy::too_many_arguments)]
fn setup_drag_pan<
    F: Fn(AutoHideUiState) + Clone + 'static,
    G: Fn() -> bool + Clone + 'static,
    H: Fn(bool) + Clone + 'static,
>(
    area: &Widget,
    viewer: &Rc<RefCell<Viewer>>,
    settings: &Settings,
    on_ui_activity: F,
    is_pointer_over_ui_element: G,
    on_pan_activity: H,
    panning: Rc<std::cell::Cell<bool>>,
    cursor_timeout_id: Rc<RefCell<Option<glib::SourceId>>>,
) {
    let drag = gtk4::GestureDrag::new();
    drag.set_button(1);

    {
        let viewer = viewer.clone();
        let area_clone = area.clone();
        let on_pan_activity = on_pan_activity.clone();
        let panning = panning.clone();
        let cursor_timeout_id = cursor_timeout_id.clone();
        drag.connect_drag_begin(move |_, _x, _y| {
            viewer.borrow_mut().pan_start();
            panning.set(true);
            // Скасовуємо вже заплановане приховування курсора (могло бути
            // виставлене ще до натискання кнопки) — інакше воно спрацює
            // просто посеред пана й приховає курсор "grabbing".
            if let Some(id) = cursor_timeout_id.borrow_mut().take() {
                id.remove();
            }
            area_clone.set_cursor_from_name(Some("grabbing"));
            on_pan_activity(true);
        });
    }
    {
        let viewer = viewer.clone();
        let area_clone = area.clone();
        drag.connect_drag_update(move |_, dx, dy| {
            viewer.borrow_mut().pan_update(dx, dy);
            area_clone.queue_draw();
        });
    }
    {
        let viewer = viewer.clone();
        let area_clone = area.clone();
        let settings = settings.clone();
        let on_ui_activity = on_ui_activity.clone();
        let is_pointer_over_ui_element = is_pointer_over_ui_element.clone();
        let on_pan_activity = on_pan_activity.clone();
        let panning = panning.clone();
        let cursor_timeout_id = cursor_timeout_id.clone();
        const CLICK_THRESHOLD: f64 = 6.0;
        drag.connect_drag_end(move |gesture, dx, dy| {
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < CLICK_THRESHOLD {
                viewer.borrow_mut().pan_cancel();
            }
            area_clone.queue_draw();
            panning.set(false);
            on_pan_activity(false);

            // Справжня позиція курсора в момент відпускання кнопки —
            // початкова точка перетягування плюс зміщення. Вона потрібна,
            // щоб одразу розрахувати той самий стан, який дав би звичайний
            // рух миші сюди.
            let (x, y) = gesture
                .start_point()
                .map(|(sx, sy)| (sx + dx, sy + dy))
                .unwrap_or((0.0, 0.0));
            viewer.borrow_mut().last_pointer = (x, y);

            let ui_state_now = if settings.sensitivity_zones_enabled() {
                let width = area_clone.width() as f64;
                let height = area_clone.height() as f64;
                ui_state_for_pointer(x, y, width, height)
            } else {
                AutoHideUiState::all(true)
            };

            schedule_cursor_autohide(
                &area_clone,
                &settings,
                &cursor_timeout_id,
                &on_ui_activity,
                &is_pointer_over_ui_element,
                &panning,
                ui_state_now,
            );
        });
    }
    area.add_controller(drag);
}

/// Права кнопка миші відкриває контекстне меню в точці натискання.
fn setup_right_click_press(area: &Widget, popover: PopoverMenu) {
    let click = gtk4::GestureClick::new();
    click.set_button(3);

    let area_clone = area.clone();
    click.connect_pressed(move |_, _n, x, y| {
        menu::popup_at(&popover, &area_clone, x, y);
    });

    area.add_controller(click);
}

/// Середня кнопка миші — перемикач 100% / "вписати в екран".
fn setup_middle_click_zoom_toggle(area: &Widget, viewer: &Rc<RefCell<Viewer>>) {
    let viewer = viewer.clone();
    let area_clone = area.clone();
    let click = gtk4::GestureClick::new();
    click.set_button(2);
    click.connect_pressed(move |_, _n, x, y| {
        viewer.borrow_mut().toggle_zoom((x, y));
        area_clone.queue_draw();
    });
    area.add_controller(click);
}

/// Esc — вихід із програми.
fn setup_escape_to_quit(window: &ApplicationWindow) {
    let window_clone = window.clone();
    let key = gtk4::EventControllerKey::new();
    key.connect_key_pressed(move |_, keyval, _keycode, _modifiers| {
        if keyval == gdk::Key::Escape {
            window_clone.close();
        }
        glib::Propagation::Proceed
    });
    window.add_controller(key);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitivity_zones_match_expected_screen_regions() {
        assert!(pointer_in_sensitivity_zone(
            10.0,
            10.0,
            100.0,
            100.0,
            SensitivityZone::TopLeft
        ));
        assert!(pointer_in_sensitivity_zone(
            90.0,
            10.0,
            100.0,
            100.0,
            SensitivityZone::TopRight
        ));
        assert!(pointer_in_sensitivity_zone(
            25.0,
            50.0,
            100.0,
            100.0,
            SensitivityZone::MiddleLeft
        ));
        assert!(pointer_in_sensitivity_zone(
            75.0,
            50.0,
            100.0,
            100.0,
            SensitivityZone::MiddleRight
        ));
        assert!(pointer_in_sensitivity_zone(
            50.0,
            95.0,
            100.0,
            100.0,
            SensitivityZone::Bottom
        ));
        assert!(!pointer_in_sensitivity_zone(
            60.0,
            10.0,
            100.0,
            100.0,
            SensitivityZone::TopLeft
        ));
        assert!(!pointer_in_sensitivity_zone(
            40.0,
            50.0,
            100.0,
            100.0,
            SensitivityZone::MiddleRight
        ));
    }

    #[test]
    fn auto_hide_elements_only_react_inside_their_zones() {
        assert!(pointer_in_auto_hide_zone_for_element(
            "filename", 10.0, 10.0, 100.0, 100.0
        ));
        assert!(pointer_in_auto_hide_zone_for_element(
            "close", 90.0, 10.0, 100.0, 100.0
        ));
        assert!(pointer_in_auto_hide_zone_for_element(
            "previous", 10.0, 50.0, 100.0, 100.0
        ));
        assert!(pointer_in_auto_hide_zone_for_element(
            "next", 90.0, 50.0, 100.0, 100.0
        ));
        assert!(pointer_in_auto_hide_zone_for_element(
            "strip", 50.0, 95.0, 100.0, 100.0
        ));
        assert!(!pointer_in_auto_hide_zone_for_element(
            "filename", 90.0, 10.0, 100.0, 100.0
        ));
        assert!(!pointer_in_auto_hide_zone_for_element(
            "close", 10.0, 10.0, 100.0, 100.0
        ));
        assert!(!pointer_in_auto_hide_zone_for_element(
            "strip", 50.0, 50.0, 100.0, 100.0
        ));
    }

    #[test]
    fn auto_hide_timeout_is_enabled_for_any_auto_hide_element() {
        assert!(any_auto_hide_enabled(true, false, false, false, false));
        assert!(any_auto_hide_enabled(false, true, false, false, false));
        assert!(any_auto_hide_enabled(false, false, true, false, false));
        assert!(any_auto_hide_enabled(false, false, false, true, false));
        assert!(any_auto_hide_enabled(false, false, false, false, true));
        assert!(!any_auto_hide_enabled(false, false, false, false, false));
    }

    #[test]
    fn timeout_hides_auto_hide_elements() {
        let state = timeout_hidden_state(true, true, true, true, true);

        assert!(!state.cursor);
        assert!(!state.close);
        assert!(!state.previous);
        assert!(!state.next);
        assert!(!state.filename);
        assert!(!state.strip);
    }
}
