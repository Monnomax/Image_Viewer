use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;

use std::fs;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc::{self, Receiver, Sender},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

const BACKLIGHT_DEVICE: &str = "intel_backlight";
const BACKLIGHT_PATH: &str = "/sys/class/backlight/intel_backlight";

const ANIMATION_DURATION: Duration = Duration::from_millis(800);
const TICK_INTERVAL: Duration = Duration::from_millis(16);

#[derive(Debug)]
struct AnimationCompletion {
    generation: u64,
    success: bool,
    clear_original: bool,
}

pub struct BrightnessController {
    generation: Arc<AtomicU64>,

    dbus_lock: Arc<Mutex<()>>,

    completion_tx: Sender<AnimationCompletion>,
    completion_rx: Receiver<AnimationCompletion>,

    completion_callback: Option<Box<dyn FnOnce() + 'static>>,

    active_generation: Option<u64>,

    original_brightness: Option<u32>,
}

impl BrightnessController {
    pub fn new() -> Result<Self, glib::Error> {
        let _ = gio::DBusProxy::for_bus_sync(
            gio::BusType::System,
            gio::DBusProxyFlags::NONE,
            None,
            "org.freedesktop.login1",
            "/org/freedesktop/login1/session/auto",
            "org.freedesktop.login1.Session",
            None::<&gio::Cancellable>,
        )?;

        let (completion_tx, completion_rx) = mpsc::channel();

        Ok(Self {
            generation: Arc::new(AtomicU64::new(0)),
            dbus_lock: Arc::new(Mutex::new(())),
            completion_tx,
            completion_rx,
            completion_callback: None,
            active_generation: None,
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
        let parameters = ("backlight", BACKLIGHT_DEVICE, value).to_variant();

        let started = Instant::now();

        let result = proxy.call_sync(
            "SetBrightness",
            Some(&parameters),
            gio::DBusCallFlags::NONE,
            1000,
            None::<&gio::Cancellable>,
        );

        eprintln!(
            "ImgViewer: SetBrightness({}) завершився за {:?}",
            value,
            started.elapsed()
        );

        result.map(|_| ())
    }

    pub fn poll_completions(&mut self) {
        while let Ok(completion) = self.completion_rx.try_recv() {
            if self.active_generation != Some(completion.generation) {
                // Це завершення старої, вже скасованої анімації.
                continue;
            }

            self.active_generation = None;

            if completion.success && completion.clear_original {
                self.original_brightness = None;
            }

            if let Some(callback) = self.completion_callback.take() {
                callback();
            }
        }
    }

    fn cancel_animation(&mut self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.active_generation = None;
        self.completion_callback = None;
    }

    fn animate_to<F>(&mut self, from: u32, to: u32, clear_original_on_success: bool, on_complete: F)
    where
        F: FnOnce() + 'static,
    {
        self.cancel_animation();

        if from == to {
            if clear_original_on_success {
                self.original_brightness = None;
            }

            on_complete();
            return;
        }

        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;

        let shared_generation = self.generation.clone();
        let dbus_lock = self.dbus_lock.clone();
        let completion_tx = self.completion_tx.clone();

        self.active_generation = Some(generation);
        self.completion_callback = Some(Box::new(on_complete));

        eprintln!("ImgViewer: brightness animation started");

        thread::spawn(move || {
            let proxy = match gio::DBusProxy::for_bus_sync(
                gio::BusType::System,
                gio::DBusProxyFlags::NONE,
                None,
                "org.freedesktop.login1",
                "/org/freedesktop/login1/session/auto",
                "org.freedesktop.login1.Session",
                None::<&gio::Cancellable>,
            ) {
                Ok(proxy) => proxy,

                Err(err) => {
                    eprintln!("ImgViewer: не вдалося створити D-Bus proxy: {}", err);

                    let _ = completion_tx.send(AnimationCompletion {
                        generation,
                        success: false,
                        clear_original: clear_original_on_success,
                    });

                    return;
                }
            };

            let started = Instant::now();

            let mut last_value = None::<u32>;
            let mut next_tick = started + TICK_INTERVAL;

            loop {
                if shared_generation.load(Ordering::SeqCst) != generation {
                    return;
                }

                let now = Instant::now();

                if now < next_tick {
                    thread::sleep(next_tick - now);
                    continue;
                }

                let elapsed = now.saturating_duration_since(started);

                let mut t = elapsed.as_secs_f64() / ANIMATION_DURATION.as_secs_f64();

                if t >= 1.0 {
                    t = 1.0;
                }

                let eased = t * t * (3.0 - 2.0 * t);

                let value = from as f64 + (to as f64 - from as f64) * eased;

                let value = value.round() as u32;

                if last_value != Some(value) {
                    // Серіалізуємо D-Bus виклики різних animation worker-ів.
                    let _guard = match dbus_lock.lock() {
                        Ok(guard) => guard,

                        Err(_) => {
                            eprintln!("ImgViewer: mutex яскравості пошкоджений");

                            let _ = completion_tx.send(AnimationCompletion {
                                generation,
                                success: false,
                                clear_original: clear_original_on_success,
                            });

                            return;
                        }
                    };

                    if shared_generation.load(Ordering::SeqCst) != generation {
                        return;
                    }

                    if let Err(err) = Self::set_brightness(&proxy, value) {
                        eprintln!(
                            "ImgViewer: не вдалося змінити яскравість до {}: {}",
                            value, err
                        );

                        let _ = completion_tx.send(AnimationCompletion {
                            generation,
                            success: false,
                            clear_original: clear_original_on_success,
                        });

                        return;
                    }

                    last_value = Some(value);
                }

                if t >= 1.0 {
                    let _ = completion_tx.send(AnimationCompletion {
                        generation,
                        success: true,
                        clear_original: clear_original_on_success,
                    });

                    return;
                }

                next_tick += TICK_INTERVAL;
            }
        });
    }

    pub fn boost(&mut self) {
        self.poll_completions();

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

        eprintln!("ImgViewer: яскравість {} → {}", current, maximum);

        self.animate_to(current, maximum, false, || {});
    }

    pub fn restore_animated<F>(&mut self, on_complete: F)
    where
        F: FnOnce() + 'static,
    {
        self.poll_completions();

        let Some(original) = self.original_brightness else {
            on_complete();
            return;
        };

        let Some(current) = Self::current_brightness() else {
            self.original_brightness = None;
            on_complete();
            return;
        };

        eprintln!(
            "ImgViewer: плавне відновлення яскравості {} → {}",
            current, original
        );

        self.animate_to(current, original, true, on_complete);
    }

    pub fn restore(&mut self) {
        self.poll_completions();

        let Some(original) = self.original_brightness else {
            return;
        };

        self.cancel_animation();

        let proxy = match gio::DBusProxy::for_bus_sync(
            gio::BusType::System,
            gio::DBusProxyFlags::NONE,
            None,
            "org.freedesktop.login1",
            "/org/freedesktop/login1/session/auto",
            "org.freedesktop.login1.Session",
            None::<&gio::Cancellable>,
        ) {
            Ok(proxy) => proxy,

            Err(err) => {
                eprintln!("ImgViewer: не вдалося створити D-Bus proxy: {}", err);
                return;
            }
        };

        let _guard = match self.dbus_lock.lock() {
            Ok(guard) => guard,

            Err(_) => {
                eprintln!("ImgViewer: mutex яскравості пошкоджений");
                return;
            }
        };

        match Self::set_brightness(&proxy, original) {
            Ok(()) => {
                self.original_brightness = None;
            }

            Err(err) => {
                eprintln!("Не вдалося відновити яскравість {}: {}", original, err);
            }
        }
    }
}

impl Drop for BrightnessController {
    fn drop(&mut self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.active_generation = None;
        self.completion_callback = None;

        let Some(original) = self.original_brightness else {
            return;
        };

        let proxy = match gio::DBusProxy::for_bus_sync(
            gio::BusType::System,
            gio::DBusProxyFlags::NONE,
            None,
            "org.freedesktop.login1",
            "/org/freedesktop/login1/session/auto",
            "org.freedesktop.login1.Session",
            None::<&gio::Cancellable>,
        ) {
            Ok(proxy) => proxy,

            Err(err) => {
                eprintln!(
                    "ImgViewer: не вдалося створити D-Bus proxy під час завершення: {}",
                    err
                );
                return;
            }
        };

        let _guard = match self.dbus_lock.lock() {
            Ok(guard) => guard,

            Err(_) => return,
        };

        if let Err(err) = Self::set_brightness(&proxy, original) {
            eprintln!(
                "Не вдалося відновити яскравість {} під час завершення: {}",
                original, err
            );
        }
    }
}
