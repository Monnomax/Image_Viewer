// screen_brightness.rs — плавне керування яскравістю екрана
// через systemd-logind D-Bus.
//
// Працює з native Intel backlight:
//   /sys/class/backlight/intel_backlight
//
// systemd-logind SetBrightness() використовується замість прямого
// запису в /sys, тому ImgViewer не потребує sudo/root.

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::fs;
use std::rc::Rc;
use std::time::{Duration, Instant};

const BACKLIGHT_DEVICE: &str = "intel_backlight";
const BACKLIGHT_PATH: &str = "/sys/class/backlight/intel_backlight";

const ANIMATION_DURATION: Duration = Duration::from_millis(400);
const TICK_INTERVAL: Duration = Duration::from_millis(16);

pub struct BrightnessController {
    proxy: gio::DBusProxy,
    animation_source: Option<glib::SourceId>,

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

    fn set_brightness(proxy: &gio::DBusProxy, value: u32) -> Result<(), glib::Error> {
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
        if let Some(source) = self.animation_source.take() {
            source.remove();
        }
    }

    fn animate_to(&mut self, from: u32, to: u32) {
        self.cancel_animation();

        if from == to {
            let _ = Self::set_brightness(&self.proxy, to);
            return;
        }

        let proxy = self.proxy.clone();
        let started = Instant::now();
        let last_value = Rc::new(RefCell::new(None::<u32>));

        let source = glib::timeout_add_local(TICK_INTERVAL, move || {
            let elapsed = started.elapsed();
            let mut t = elapsed.as_secs_f64() / ANIMATION_DURATION.as_secs_f64();

            if t >= 1.0 {
                t = 1.0;
            }

            // EASE_OUT_CUBIC:
            // швидко стартуємо, м'яко підходимо до 100%.
            let eased = 1.0 - (1.0 - t).powi(3);

            let value = from as f64 + (to as f64 - from as f64) * eased;
            let value = value.round() as u32;

            // Не відправляємо однакове значення повторно.
            let mut last = last_value.borrow_mut();

            if *last != Some(value) {
                if let Err(err) = Self::set_brightness(&proxy, value) {
                    eprintln!(
                        "Не вдалося змінити яскравість до {}: {}",
                        value, err
                    );
                    return glib::ControlFlow::Break;
                }

                *last = Some(value);
            }

            if t >= 1.0 {
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });

        self.animation_source = Some(source);
    }

    /// Входить у режим максимальної яскравості.
    ///
    /// Повторний виклик нічого не робить — це важливо при навігації
    /// між сусідніми зображеннями.
    pub fn boost(&mut self) {
        if self.original_brightness.is_some() {
            return;
        }

        let Some(current) = Self::current_brightness() else {
            eprintln!("ImgViewer: не вдалося прочитати поточну яскравість");
            return;
        };

        let Some(maximum) = Self::max_brightness() else {
            eprintln!("ImgViewer: не вдалося прочитати max_brightness");
            return;
        };

        self.original_brightness = Some(current);

        eprintln!(
            "ImgViewer: яскравість {} → {}",
            current, maximum
        );

        self.animate_to(current, maximum);
    }

    /// Повертає яскравість, яка була до входу в режим.
    pub fn restore(&mut self) {
        let Some(original) = self.original_brightness.take() else {
            return;
        };

        self.cancel_animation();

        let Some(current) = Self::current_brightness() else {
            return;
        };

        eprintln!(
            "ImgViewer: відновлення яскравості {} → {}",
            current, original
        );

        // Поки що відновлюємо одразу.
        // При закритті програми це краще, ніж запускати ще одну
        // анімацію, яка може бути перервана завершенням процесу.
        let _ = Self::set_brightness(&self.proxy, original);
    }
}

impl Drop for BrightnessController {
    fn drop(&mut self) {
        self.cancel_animation();

        if let Some(original) = self.original_brightness.take() {
            let _ = Self::set_brightness(&self.proxy, original);
        }
    }
}