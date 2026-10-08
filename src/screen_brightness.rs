// screen_brightness.rs — плавне керування яскравістю екрана
// через systemd-logind D-Bus.
//
// Працює з native Intel backlight:
//   /sys/class/backlight/intel_backlight
//
// systemd-logind SetBrightness() використовується замість прямого
// запису в /sys, тому ImgViewer не потребує sudo/root.

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::fs;
use std::rc::Rc;
use std::time::{Duration, Instant};

const BACKLIGHT_DEVICE: &str = "intel_backlight";
const BACKLIGHT_PATH: &str = "/sys/class/backlight/intel_backlight";

const ANIMATION_DURATION: Duration = Duration::from_millis(800);
const TICK_INTERVAL: Duration = Duration::from_millis(16);

pub struct BrightnessController {
    proxy: gio::DBusProxy,
    animation_source: Option<glib::SourceId>,
    animation_running: Option<Rc<Cell<bool>>>,

    /// Яскравість, яка була до входу в режим 100%.
    ///
    /// Some(...) означає, що режим зараз активний.
    original_brightness: Option<u32>,
}

impl BrightnessController {
    pub fn new() -> Result<Self, glib::Error> {
        let proxy = gio::DBusProxy::for_bus_sync(
            gio::BusType::System,
            gio::DBusProxyFlags::NONE,
            None,
            "org.freedesktop.login1",
            "/org/freedesktop/login1/session/auto",
            "org.freedesktop.login1.Session",
            None::<&gio::Cancellable>,
        )?;

        Ok(Self {
            proxy,
            animation_source: None,
            animation_running: None,
            original_brightness: None,
        })
    }

    fn read_brightness(path: &str) -> Option<u32> {
        fs::read_to_string(path).ok()?.trim().parse().ok()
    }

    fn current_brightness() -> Option<u32> {
        Self::read_brightness(&format!("{BACKLIGHT_PATH}/brightness"))
    }

    fn max_brightness() -> Option<u32> {
        Self::read_brightness(&format!("{BACKLIGHT_PATH}/max_brightness"))
    }

    /// Одноразова синхронна зміна яскравості.
    ///
    /// Синхронне виконання зберігає порядок кадрів анімації та
    /// не дозволяє їм перезаписати яскравість після відновлення.
    fn set_brightness(
        proxy: &gio::DBusProxy,
        value: u32,
    ) -> Result<(), glib::Error> {
        let parameters = (
            "backlight",
            BACKLIGHT_DEVICE,
            value,
        )
            .to_variant();

        proxy
            .call_sync(
                "SetBrightness",
                Some(&parameters),
                gio::DBusCallFlags::NONE,
                1000,
                None::<&gio::Cancellable>,
            )
            .map(|_| ())
    }

    fn cancel_animation(&mut self) {
        let should_remove = self
            .animation_running
            .take()
            .is_some_and(|running| running.replace(false));

        if let Some(source) = self.animation_source.take() {
            if should_remove {
                source.remove();
            }
        }
    }

    fn animate_to<F>(&mut self, from: u32, to: u32, on_complete: F)
    where
        F: FnOnce() + 'static,
    {
        self.cancel_animation();

        if from == to {
            if let Err(err) = Self::set_brightness(&self.proxy, to) {
                eprintln!("Не вдалося встановити яскравість {}: {}", to, err);
            }
            on_complete();
            return;
        }

        let proxy = self.proxy.clone();
        let started = Instant::now();
        let last_value = Rc::new(RefCell::new(None::<u32>));
        let animation_running = Rc::new(Cell::new(true));
        let callback_running = animation_running.clone();
        let mut on_complete = Some(on_complete);

        let source = glib::timeout_add_local(TICK_INTERVAL, move || {
            let elapsed = started.elapsed();

            let mut t =
                elapsed.as_secs_f64() / ANIMATION_DURATION.as_secs_f64();

            if t >= 1.0 {
                t = 1.0;
            }

            // Smoothstep cubic: м'яко починаємо та завершуємо перехід.
            let eased = t * t * (3.0 - 2.0 * t);

            let value = from as f64
                + (to as f64 - from as f64) * eased;

            let value = value.round() as u32;

            // Не відправляємо однакове значення повторно.
            let mut last = last_value.borrow_mut();

            if *last != Some(value) {
                if let Err(err) = Self::set_brightness(&proxy, value) {
                    eprintln!("Не вдалося змінити яскравість до {}: {}", value, err);
                    callback_running.set(false);
                    if let Some(on_complete) = on_complete.take() {
                        on_complete();
                    }
                    return glib::ControlFlow::Break;
                }

                *last = Some(value);
            }

            if t >= 1.0 {
                callback_running.set(false);
                if let Some(on_complete) = on_complete.take() {
                    on_complete();
                }
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
        self.animation_source = Some(source);
        self.animation_running = Some(animation_running);
    }

    /// Входить у режим максимальної яскравості.
    ///
    /// Повторний виклик нічого не робить — це важливо при
    /// 16-мс UiTick та навігації між зображеннями.
    pub fn boost(&mut self) {
        if self.original_brightness.is_some() {
            return;
        }

        let Some(current) = Self::current_brightness() else {
            eprintln!(
                "ImgViewer: не вдалося прочитати поточну яскравість"
            );
            return;
        };

        let Some(maximum) = Self::max_brightness() else {
            eprintln!(
                "ImgViewer: не вдалося прочитати max_brightness"
            );
            return;
        };

        self.original_brightness = Some(current);

        eprintln!(
            "ImgViewer: яскравість {} → {}",
            current, maximum
        );

        self.animate_to(current, maximum, || {});
    }

    /// Плавно повертає яскравість та викликає `on_complete` після завершення.
    pub fn restore_animated<F>(&mut self, on_complete: F)
    where
        F: FnOnce() + 'static,
    {
        let Some(original) = self.original_brightness else {
            on_complete();
            return;
        };

        self.cancel_animation();

        if let Some(current) = Self::current_brightness() {
            eprintln!("ImgViewer: плавне відновлення яскравості {} → {}", current, original);
            self.animate_to(current, original, on_complete);
        } else {
            eprintln!("ImgViewer: не вдалося прочитати яскравість перед відновленням");
            self.restore();
            on_complete();
        }
    }

    /// Синхронно повертає яскравість як запасний варіант при помилці читання.
    pub fn restore(&mut self) {
        let Some(original) = self.original_brightness else {
            return;
        };

        self.cancel_animation();

        match Self::set_brightness(&self.proxy, original) {
            Ok(()) => {
                eprintln!("ImgViewer: відновлено попередню яскравість: {}", original);
                self.original_brightness = None;
            }
            Err(err) => eprintln!("ImgViewer: не вдалося відновити яскравість: {}", err),
        }
    }
}

impl Drop for BrightnessController {
    fn drop(&mut self) {
        self.cancel_animation();

        if let Some(original) = self.original_brightness {
            if let Err(err) = Self::set_brightness(&self.proxy, original) {
                eprintln!(
                    "ImgViewer: не вдалося відновити яскравість при завершенні: {}",
                    err
                );
            } else if Self::current_brightness() != Some(original) {
                eprintln!(
                    "ImgViewer: значення яскравості після завершального відновлення не збігається з початковим ({})",
                    original
                );
            } else {
                eprintln!(
                    "ImgViewer: відновлено початкову яскравість при завершенні: {}",
                    original
                );
            }
        }
    }
}