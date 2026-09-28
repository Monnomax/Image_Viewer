// viewer/rotate.rs — пункт контекстного меню "Повернути": поворот
// поточного зображення на 90° без діалогів (миттєва дія, на відміну від
// rename.rs, де потрібне підтвердження нової назви).
//
// ПОВОРОТ ЯК СТАН, А НЕ НЕГАЙНИЙ ЗАПИС. Поки поточне зображення (те саме
// path) лишається поточним, клік по "Повернути" лише обертає вже
// закешований Pixbuf/GPU-текстуру в пам'яті — на диск нічого не
// пишеться. Кількість накопичених поворотів (у чвертях, зі знаком)
// зберігається у `Viewer::pending_rotation`. Реальний файловий запис
// (rotate_file — EXIF-тег Orientation для jpg/tif, фізичний поворот
// пікселів для png/bmp) відбувається рівно один раз — у
// `Viewer::commit_pending_rotation()`, — коли зображення перестає бути
// поточним: на початку navigate() (виклик до next/prev) і при закритті
// переглядача (див. TODO нижче — виклик з app.rs при закритті вікна).
// Якщо сумарна кількість поворотів кратна 4 (наприклад, CW і одразу CCW
// назад), commit — no-op: на диск не пишеться нічого, бо файл фактично
// не змінився. Це прибирає головну ваду попередньої версії: швидке
// клацання туди-сюди більше не породжує купу зайвих EXIF-записів чи
// (для png/bmp) повних перезаписів файлу при кожному натисканні.
//
// СТРАТЕГІЯ "БЕЗ ВТРАТИ ЯКОСТІ" при коміті на диск залежить від формату:
//
//   jpg/jpeg/tif/tiff — пікселі взагалі не чіпаються. Змінюється лише
//   EXIF/TIFF-тег Orientation (через little_exif — читає й переписує сам
//   сегмент метаданих у файлі, без декодування-перекодування зображення).
//   Це справжній нуль втрат: жодного повторного JPEG-стиснення. Усі
//   накопичені чверті повороту композуються в ОДНЕ підсумкове значення
//   тега перед єдиним записом файлу — навіть якщо користувач клацнув
//   "Повернути" десять разів, файл переписується один раз.
//   GdkPixbuf::apply_embedded_orientation() (викликається в
//   image_loader::decode) означає, що й сам переглядач одразу показує
//   картинку вже повернутою, а не лише файлові менеджери, що читають EXIF.
//
//   png/bmp — контейнер лишається лослес-форматом, тож фізичний поворот
//   пікселів (Pixbuf::rotate_simple — проста перестановка рядків/стовпців,
//   не DCT-кодек) з подальшим ОДНИМ перезаписом у той самий формат теж не
//   вносить жодних втрат.
//
//   gif/webp/avif/ico/pnm — свідомо не підтримуються: GdkPixbuf або не
//   вміє зберігати ці формати назад (gif, pnm), або підтримка запису
//   залежить від опційних плагінів системи (webp, ico), або формат
//   лослес-перезапис в принципі не гарантує (avif — по суті кадр AV1,
//   рекодування завжди означає повторне лослес-стиснення), тож
//   гарантувати безвтратний результат тут не можна. Перевірка відбувається одразу
//   при кліку "Повернути" (ще до того, як з'явиться pending-стан), щоб
//   користувач одразу побачив повідомлення в stderr, а не лише в момент
//   коміту, можливо, значно пізніше.

use crate::viewer::Viewer;
use gtk4::gdk_pixbuf::{Pixbuf, PixbufRotation};
use little_exif::exif_tag::ExifTag;
use little_exif::metadata::Metadata;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Direction {
    Clockwise,
    CounterClockwise,
}

/// Незбережений поворот поточного (на момент останнього кліку
/// "Повернути") зображення. Живе, доки це зображення лишається поточним
/// у моделі — при переході до іншого зображення чи закритті переглядача
/// його потрібно "закомітити" через `Viewer::commit_pending_rotation()`.
#[derive(Clone, Debug)]
pub struct PendingRotation {
    pub path: PathBuf,
    /// Накопичена кількість поворотів на 90° за годинниковою стрілкою.
    /// Не обмежена діапазоном 0..=3 — нормалізується (`rem_euclid(4)`)
    /// лише в момент коміту. Від'ємні значення — сумарний поворот проти
    /// годинникової стрілки.
    pub turns_cw: i32,
}

/// Повертає поточне зображення переглядача на 90° у вказаному напрямку.
/// Змінюється лише стан у пам'яті (кешований Pixbuf і GPU-текстура) —
/// файл на диску не чіпається, доки не спрацює commit_pending_rotation().
/// У разі непідтримуваного формату повідомлення пишеться в stderr одразу,
/// а стан переглядача лишається незмінним.
pub fn rotate_current(viewer: &Rc<RefCell<Viewer>>, direction: Direction) {
    let mut v = viewer.borrow_mut();

    let Some(path) = v.model.current_path() else {
        return;
    };

    if let Err(err) = check_format_supported(&path) {
        eprintln!("Не вдалося повернути зображення {:?}: {}", path, err);
        return;
    }

    let Some(pixbuf) = v.image_cache.get(&path) else {
        // Пункт меню доступний лише для вже показаного зображення, тож
        // сюди потрапити не мало б — але про всяк випадок нема що
        // обертати, якщо декодований Pixbuf ще не готовий.
        return;
    };

    let delta: i32 = match direction {
        Direction::Clockwise => 1,
        Direction::CounterClockwise => -1,
    };

    let continues_existing_pending = v
        .pending_rotation
        .as_ref()
        .map(|pending| pending.path == path)
        .unwrap_or(false);

    if continues_existing_pending {
        // unwrap безпечний: continues_existing_pending == true гарантує Some
        v.pending_rotation.as_mut().unwrap().turns_cw += delta;
    } else {
        v.pending_rotation = Some(PendingRotation {
            path: path.clone(),
            turns_cw: delta,
        });
    }

    let gdk_rotation = match direction {
        Direction::Clockwise => PixbufRotation::Clockwise,
        Direction::CounterClockwise => PixbufRotation::Counterclockwise,
    };
    let Some(rotated) = pixbuf.rotate_simple(gdk_rotation) else {
        eprintln!(
            "Не вдалося виконати поворот пікселів у пам'яті для {:?}",
            path
        );
        return;
    };

    // Заміщуємо кешовані Pixbuf і GPU-текстуру повернутою версією — без
    // жодного звернення до диска. get_or_create одразу перебудовує
    // текстуру (а не лишає її "протухлою" до наступного кадру), як і при
    // звичайному довантаженні в handle_loaded().
    v.texture_cache.remove(&path);
    v.texture_cache.get_or_create(&path, &rotated);
    v.image_cache.insert(path.clone(), rotated);

    // Розміри могли помінятись місцями (90°/270°) — перевписуємо під
    // екран. Збережений zoom/позиція для цього шляху свідомо ігноруються
    // тут: після зміни орієнтації вони все одно не відповідали б новій
    // геометрії зображення.
    v.fit_to_screen();
}

/// Перевіряє, що формат файлу підтримує безвтратний поворот, не читаючи
/// й не змінюючи сам файл. Використовується і тут (щоб одразу
/// відмовити на непідтримуваному форматі, ще до появи pending-стану), і
/// в rotate_file() як єдине джерело істини щодо переліку форматів.
fn check_format_supported(path: &Path) -> Result<(), String> {
    normalized_ext(path).map(|_| ()).ok_or_else(|| {
        let other = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        format!("Поворот без перекодування не підтримується для формату \".{other}\"")
    })
}

fn normalized_ext(path: &Path) -> Option<String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "jpg" | "jpeg" | "tif" | "tiff" | "png" | "bmp" => Some(ext),
        _ => None,
    }
}

impl Viewer {
    /// Записує на диск усі накопичені з моменту попереднього коміту
    /// повороти зображення, яке було поточним, — рівно ОДНИМ викликом
    /// rotate_file(), незалежно від кількості кліків "Повернути" між
    /// комітами. Викликається щоразу, коли зображення перестає бути
    /// поточним: на початку navigate() і при закритті переглядача.
    ///
    /// Якщо сумарний поворот кратний 4 (зображення візуально повернулось
    /// до вихідної орієнтації), на диск нічого не пишеться — pending-стан
    /// просто скидається.
    pub fn commit_pending_rotation(&mut self) {
        let Some(pending) = self.pending_rotation.take() else {
            return;
        };

        let quarter_turns = pending.turns_cw.rem_euclid(4) as u8;
        if quarter_turns == 0 {
            return;
        }

        if let Err(err) = rotate_file(&pending.path, quarter_turns) {
            eprintln!(
                "Не вдалося зберегти поворот зображення {:?}: {}",
                pending.path, err
            );
            // Файл на диску лишається у попередній (не повернутій)
            // орієнтації, тоді як кеш/текстура в пам'яті вже показують
            // повернуту версію — навмисно: заново читати з диска й
            // "відкочувати" картинку користувачу на екрані було б гірше,
            // ніж просто повідомити про помилку запису. Наступний
            // успішний rotate чи перезапуск програми синхронізують стан.
        }
    }

    /// Скасовує ще не збережений поворот для вказаного шляху без запису
    /// на диск — викликається, коли файл видаляється (комітити вже
    /// нема сенсу: перед trash/delete лишити файл у вихідній орієнтації
    /// не важливо, а зайвий запис перед видаленням — марна робота).
    pub fn discard_pending_rotation_for(&mut self, path: &Path) {
        let matches = self
            .pending_rotation
            .as_ref()
            .map(|p| p.path.as_path() == path)
            .unwrap_or(false);
        if matches {
            self.pending_rotation = None;
        }
    }
}

/// Записує на диск підсумковий поворот на `quarter_turns` чвертей (1..=3)
/// за годинниковою стрілкою — одним записом файлу, незалежно від
/// кількості чвертей.
fn rotate_file(path: &Path, quarter_turns: u8) -> Result<(), String> {
    let ext = normalized_ext(path).ok_or_else(|| {
        let other = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        format!("Поворот без перекодування не підтримується для формату \".{other}\"")
    })?;

    match ext.as_str() {
        "jpg" | "jpeg" | "tif" | "tiff" => rotate_via_exif_orientation(path, quarter_turns),
        "png" | "bmp" => rotate_pixels_in_place(path, quarter_turns, &ext),
        _ => unreachable!("normalized_ext() уже відфільтрував формат"),
    }
}

/// jpg/jpeg/tif/tiff: пікселі не чіпаються, лише композиція нового
/// значення EXIF-тега Orientation (0x0112) з тим, що вже було у файлі —
/// так само, як це роблять jhead/exiftran. `quarter_turns` застосовується
/// послідовно до значення тега (це чиста арифметика над одним числом, без
/// I/O), а сам файл переписується один раз наприкінці.
fn rotate_via_exif_orientation(path: &Path, quarter_turns: u8) -> Result<(), String> {
    let mut metadata = Metadata::new_from_path(path).map_err(|e| e.to_string())?;

    let mut orientation = metadata
        .get_tag(&ExifTag::Orientation(Vec::new()))
        .next()
        .and_then(|tag| match tag {
            ExifTag::Orientation(values) => values.first().copied(),
            _ => None,
        })
        .unwrap_or(1);

    for _ in 0..quarter_turns {
        orientation = rotate_orientation_cw(orientation);
    }

    metadata.set_tag(ExifTag::Orientation(vec![orientation]));
    metadata.write_to_file(path).map_err(|e| e.to_string())
}

/// Стандартна таблиця переходів значень EXIF Orientation (1..=8) при
/// повороті на 90° за годинниковою стрілкою — той самий підхід, що й у
/// jhead/exiftran. Коректно враховує й випадок, коли зображення вже мало
/// дзеркальну orientation (2/4/5/7), а не лише "чисті" повороти (1/3/6/8).
fn rotate_orientation_cw(o: u16) -> u16 {
    match o {
        1 => 6,
        2 => 7,
        3 => 8,
        4 => 5,
        5 => 2,
        6 => 3,
        7 => 4,
        8 => 1,
        _ => 6, // невідоме/відсутнє значення — трактуємо як "1" і повертаємо з нього
    }
}

/// png/bmp: лослес-контейнери, тож фізичний поворот пікселів і повторний
/// запис у той самий формат нічого не втрачає. `quarter_turns` (1..=3)
/// застосовується до Pixbuf одним викликом rotate_simple() з відповідним
/// PixbufRotation — це чиста операція в пам'яті, тож немає різниці між
/// "одним поворотом на 270°" і "трьома поворотами по 90°" з точки зору
/// продуктивності; головне, що файл на диску переписується один раз.
/// Пишемо у тимчасовий файл поруч і атомарно підміняємо оригінал через
/// rename — щоб у разі помилки запису (напр. диск переповнено) оригінал
/// лишився цілим.
fn rotate_pixels_in_place(path: &Path, quarter_turns: u8, ext: &str) -> Result<(), String> {
    let pixbuf = Pixbuf::from_file(path).map_err(|e| e.to_string())?;

    let rotation = match quarter_turns {
        1 => PixbufRotation::Clockwise,
        2 => PixbufRotation::Upsidedown,
        3 => PixbufRotation::Counterclockwise,
        _ => return Ok(()), // 0 сюди не потрапляє — відсіюється в commit_pending_rotation()
    };

    let rotated = pixbuf
        .rotate_simple(rotation)
        .ok_or_else(|| "Не вдалося виконати поворот пікселів".to_string())?;

    let tmp_path: PathBuf = {
        let mut file_name = path.file_name().unwrap_or_default().to_os_string();
        file_name.push(".rotate-tmp");
        path.with_file_name(file_name)
    };

    rotated
        .savev(&tmp_path, ext, &[])
        .map_err(|e| e.to_string())?;
    std::fs::rename(&tmp_path, path).map_err(|e| e.to_string())?;

    Ok(())
}