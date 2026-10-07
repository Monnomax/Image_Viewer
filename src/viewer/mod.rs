// viewer/mod.rs — оголошує підмодулі перегляду/рендерингу і реекспортує
// публічний API, яким користується решта застосунку (app.rs) та сусідній
// модуль preferences/ (animations.rs — розділ "Анімації" вікна налаштувань
// навмисно живе тут, поруч із рендерером, а не в preferences/).
//
// ВАЖЛИВО: тут немає жодного коду побудови вікна налаштувань
// (AdwPreferencesDialog тощо) — це відповідальність виключно
// preferences/dialog.rs. Якщо такий код колись сюди потрапив — це наслідок
// помилкового копіювання під час перенесення файлів у нову структуру
// каталогів; тут йому не місце, і його треба прибрати.
//
// Примітка щодо іменування: файл viewer/viewer.rs визначає struct Viewer
// усередині підмодуля, теж названого `viewer` — без реекспорту нижче
// повний шлях був би незручним `crate::viewer::viewer::Viewer`.

mod animated_gif;
mod cache;
mod crossfade;
mod menu;
mod metadata;
mod properties;
mod rename;
mod renderer;
mod share;
mod rotate;
mod transition;
mod view_state;
#[allow(clippy::module_inception)]
mod viewer;
mod wallpaper;
mod zoom_animation;


pub mod animations;
pub mod image_loader;
pub mod input;
pub mod file_info;
pub use renderer::Canvas;
pub use viewer::Viewer;
pub use zoom_animation::{ZoomAnimation, ZoomEasing};
