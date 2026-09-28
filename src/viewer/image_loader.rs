// image_loader.rs — читання і декодування файлів зображень.
//
// ВАЖЛИВО: на відміну від попередньої версії, тут ДВІ різні стежки
// декодування:
//
//   1. `request_load()`/`decode_pixels()`/`to_pixbuf()` — основний шлях
//      для Viewer (навігація, crossfade). І читання байтів, і саме
//      декодування пікселів (крейт `image`, чистий Rust, Send) тепер
//      виконуються у фоновому потоці. Розпакування JPEG/PNG — основна
//      CPU-вартість для важких/великих файлів, і раніше вона виконувалась
//      синхронно на головному потоці GTK (бо GdkPixbuf/PixbufLoader не
//      Send), причому саме в момент навігації/старту crossfade — це й
//      було рештою причини гальмування анімації для важких зображень
//      (перше — синхронний аплоад GPU-текстури — уже виправлено окремо
//      в Viewer::warm_texture). GdkPixbuf як тип і надалі збирається
//      лише на головному потоці, але вже з готових пікселів
//      (`Pixbuf::from_bytes` — дешева операція, без повторного
//      розпакування).
//
//      EXIF Orientation у цьому шляху читається окремо, легким
//      pure-Rust рідером (крейт `exif`/kamadak-exif, теж Send), і
//      застосовується як поворот/дзеркалення до вже декодованих
//      пікселів — емулюючи те, що раніше безкоштовно робив
//      `Pixbuf::apply_embedded_orientation()` (метод самого GdkPixbuf,
//      недоступний поза головним потоком).
//
//   2. Стара `decode()` (bytes -> Pixbuf через PixbufLoader, синхронно)
//      лишена як є — нею й далі користується стрічка мініатюр
//      (thumbnail_strip.rs), яку цей фікс не зачіпає.

use gtk4::gdk_pixbuf::{Colorspace, Pixbuf, PixbufLoader};
use gtk4::glib;
use gtk4::prelude::PixbufLoaderExt;
use image::AnimationDecoder;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

/// Повідомлення від фонового потоку до головного циклу GTK.
pub enum LoaderMsg {
    Loaded(PathBuf, Result<LoadedImage, String>),
}

pub enum LoadedImage {
    Static(DecodedImage),
    AnimatedGif(Vec<DecodedFrame>),
}

pub struct DecodedFrame {
    pub image: DecodedImage,
    pub delay_ms: u64,
}

/// Уже декодовані у фоновому потоці сирі пікселі (RGBA8, з урахуванням
/// EXIF-орієнтації) — усе, що потрібно для дешевої збірки Pixbuf на
/// головному потоці через `to_pixbuf()`.
pub struct DecodedImage {
    width: i32,
    height: i32,
    rowstride: i32,
    pixels: Vec<u8>,
}

/// Асинхронно запитує читання й декодування файлу для Viewer. Результат
/// приходить через `sender`. І читання, і декодування (найдорожча
/// частина для важких зображень) відбуваються тут, у фоновому потоці.
pub fn request_load(path: PathBuf, sender: Sender<LoaderMsg>) {
    std::thread::spawn(move || {
        let result = std::fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| decode_image(&bytes));
        let _ = sender.send(LoaderMsg::Loaded(path, result));
    });
}

/// Декодує сирі байти файлу зображення в RGBA8-пікселі, застосовуючи
/// EXIF Orientation (якщо тег присутній). Викликається у фоновому
/// потоці — усі типи тут (`image::DynamicImage`, `Vec<u8>`) є `Send`.
fn decode_image(bytes: &[u8]) -> Result<LoadedImage, String> {
    if bytes.starts_with(b"GIF8") {
        let decoder = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))
            .map_err(|e| e.to_string())?;
        let frames = decoder
            .into_frames()
            .collect_frames()
            .map_err(|e| e.to_string())?;
        if frames.len() > 1 {
            let frames = frames
                .into_iter()
                .map(|frame| {
                    let delay = frame.delay().numer_denom_ms();
                    let image = frame.into_buffer();
                    let (width, height) = image.dimensions();
                    DecodedFrame {
                        image: DecodedImage {
                            width: width as i32,
                            height: height as i32,
                            rowstride: width as i32 * 4,
                            pixels: image.into_raw(),
                        },
                        delay_ms: (delay.0 as u64 / delay.1 as u64).max(10),
                    }
                })
                .collect();
            return Ok(LoadedImage::AnimatedGif(frames));
        }
    }

    Ok(LoadedImage::Static(decode_pixels(bytes)?))
}

fn decode_pixels(bytes: &[u8]) -> Result<DecodedImage, String> {
    let mut img = image::load_from_memory(bytes).map_err(|e| e.to_string())?;

    if let Some(orientation) = read_exif_orientation(bytes) {
        img = apply_orientation(img, orientation);
    }

    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    // RGBA8, без вирівнювання рядків — тому rowstride завжди width * 4.
    let rowstride = width as i32 * 4;

    Ok(DecodedImage {
        width: width as i32,
        height: height as i32,
        rowstride,
        pixels: rgba.into_raw(),
    })
}

/// Читає значення тега EXIF Orientation (1..=8), якщо він присутній.
/// Помилки парсингу EXIF (файл без EXIF, пошкоджений блок, формат без
/// підтримки EXIF тощо) — не фатальні: просто немає що застосовувати,
/// зображення показується як є (`None`).
fn read_exif_orientation(bytes: &[u8]) -> Option<u32> {
    let mut cursor = Cursor::new(bytes);
    let exif = exif::Reader::new().read_from_container(&mut cursor).ok()?;
    let field = exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)?;
    field.value.get_uint(0)
}

/// Застосовує поворот/дзеркалення відповідно до значення тега EXIF
/// Orientation — стандартні 8 варіантів TIFF/EXIF (те саме перетворення,
/// яке раніше застосовував `Pixbuf::apply_embedded_orientation()`).
fn apply_orientation(img: image::DynamicImage, orientation: u32) -> image::DynamicImage {
    match orientation {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate90().fliph(),
        6 => img.rotate90(),
        7 => img.rotate270().fliph(),
        8 => img.rotate270(),
        _ => img,
    }
}

/// Збирає GdkPixbuf з уже декодованих (у фоновому потоці) пікселів.
/// ОБОВ'ЯЗКОВО викликати лише на головному потоці (GdkPixbuf не Send) —
/// але це вже дешева операція без повторного розпакування зображення.
pub fn to_pixbuf(decoded: DecodedImage) -> Pixbuf {
    let bytes = glib::Bytes::from_owned(decoded.pixels);
    Pixbuf::from_bytes(
        &bytes,
        Colorspace::Rgb,
        true,
        8,
        decoded.width,
        decoded.height,
        decoded.rowstride,
    )
}

/// Декодує сирі байти файлу зображення у Pixbuf синхронно, через
/// PixbufLoader. Лишено для стрічки мініатюр (thumbnail_strip.rs) —
/// мініатюри вже маленькі, тож синхронний виклик тут не створює того
/// самого гальмування, що й для повнорозмірних зображень у Viewer.
///
/// Викликає apply_embedded_orientation(): для JPEG/TIFF поворот з
/// контекстного меню "Повернути" (viewer/rotate.rs) реалізований як зміна
/// самого лише EXIF/TIFF-тега Orientation, без перекодування пікселів
/// (щоб не втрачати якість) — тож без цього виклику сам переглядач
/// показував би зображення так, ніби нічого не змінилось, хоча файлові
/// менеджери й інші програми, що читають EXIF, уже бачили б новий поворот.
pub fn decode(bytes: &[u8]) -> Result<Pixbuf, String> {
    let loader = PixbufLoader::new();
    loader.write(bytes).map_err(|e| e.to_string())?;
    loader.close().map_err(|e| e.to_string())?;
    let pixbuf = loader
        .pixbuf()
        .ok_or_else(|| "Порожній результат декодування зображення".to_string())?;

    Ok(pixbuf.apply_embedded_orientation().unwrap_or(pixbuf))
}

/// Запасний шлях декодування мініатюр через ту саму пару
/// `decode_pixels()`/`to_pixbuf()`, якою користується основний
/// переглядач (крейт `image`, а не GdkPixbuf/PixbufLoader).
///
/// Навіщо: `decode()` вище залежить від того, які формати вміє
/// PixbufLoader — а це залежить від опційних системних плагінів
/// gdk-pixbuf, встановлених у конкретному дистрибутиві. Підтримка .avif
/// у "великому" перегляді (Viewer) вже йде через крейт `image`
/// (фіча "avif-native", див. Cargo.toml) незалежно від системних
/// плагінів gdk-pixbuf — але для мініатюр і досі викликався саме
/// GdkPixbuf-шлях. Якщо основний decode() не впорався (найчастіше саме
/// .avif на системі без gdk-pixbuf-плагіна для нього), пробуємо ще раз
/// цим шляхом, перш ніж остаточно вважати мініатюру непридатною для
/// показу.
pub fn decode_fallback(bytes: &[u8]) -> Result<Pixbuf, String> {
    let decoded = decode_pixels(bytes)?;
    Ok(to_pixbuf(decoded))
}