// viewer/file_info.rs — спільні хелпери для показу базової інформації про
// файл: розмір у байтах, назва формату за розширенням, права доступу.
//
// Свідомо винесено окремо від viewer/metadata.rs: metadata.rs відповідає
// за EXIF/XMP/IPTC-теги зображення (через GExiv2/rexiv2, з fallback без
// нього), тобто за дані, специфічні саме для формату зображення. Права
// доступу, розмір файлу тощо — загальносистемна інформація про файл, яка
// не залежить від feature "gexiv2-metadata" і потенційно потрібна в
// кількох місцях (зараз — тільки viewer/properties.rs), тож дублювати її
// у двох #[cfg]-гілках metadata.rs не варто.

use adw::prelude::*;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// AdwActionRow з назвою параметра ліворуч (title) і значенням праворуч
/// (звичайний suffix-Label, а не subtitle — subtitle в Adwaita рендериться
/// дрібнішим і приглушеним кольором, а тут шрифт і колір мають збігатися
/// з назвою).
pub fn param_row(title: &str, value: impl AsRef<str>) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title(title).build();
    let value_label = gtk4::Label::new(Some(value.as_ref()));
    value_label.set_halign(gtk4::Align::End);
    value_label.set_wrap(false);
    value_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
    row.add_suffix(&value_label);
    row
}

pub fn format_file_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} ГБ", b / GB)
    } else if b >= MB {
        format!("{:.2} МБ", b / MB)
    } else if b >= KB {
        format!("{:.1} КБ", b / KB)
    } else {
        format!("{bytes} Б")
    }
}

pub fn format_exact_file_size(bytes: u64) -> String {
    let digits = bytes.to_string();
    let mut grouped = String::with_capacity(digits.len() + (digits.len() - 1) / 3 + 5);

    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            grouped.push(' ');
        }
        grouped.push(digit);
    }

    format!("{grouped} байт")
}

pub fn format_extension_name(ext: &str) -> String {
    match ext {
        "jpg" | "jpeg" => "JPEG",
        "png" => "PNG",
        "gif" => "GIF",
        "bmp" => "BMP",
        "webp" => "WebP",
        "tif" | "tiff" => "TIFF",
        "ico" => "ICO",
        "pnm" => "PNM",
        "avif" => "AVIF",
        other if !other.is_empty() => return other.to_uppercase(),
        _ => "Невідомий формат",
    }
    .to_string()
}

/// Права доступу файлу (лише нижні 9 біт — rwx для власника/групи/інших),
/// або `None`, якщо `stat()` не вдався (файл видалено, немає доступу тощо).
fn file_mode(path: &Path) -> Option<u32> {
    std::fs::metadata(path)
        .ok()
        .map(|m| m.permissions().mode())
}

/// Рядок "Права доступу" для сторінки властивостей. Замість текстового
/// значення праворуч — стрілка ("go-next-symbolic"): клік по рядку
/// переходить на окрему сторінку `nav_view` з детальним, розбитим по
/// власнику/групі/інших відображенням дозволів — так само, як рядок
/// "Властивості зображення" переходить на сторінку метаданих. Якщо права
/// доступу прочитати не вдалось (permissions_page == None), рядок
/// лишається неактивним і показує звичайний текст "Невідомо" замість
/// стрілки.
pub fn permissions_row(
    nav_view: &adw::NavigationView,
    permissions_page: Option<&adw::NavigationPage>,
) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title("Права доступу").build();

    match permissions_page {
        Some(page) => {
            row.set_activatable(true);
            row.add_suffix(&gtk4::Image::from_icon_name("go-next-symbolic"));

            let nav_view = nav_view.clone();
            let page = page.clone();
            row.connect_activated(move |_| {
                nav_view.push(&page);
            });
        }
        None => {
            let value_label = gtk4::Label::new(Some("Невідомо"));
            value_label.set_halign(gtk4::Align::End);
            row.add_suffix(&value_label);
        }
    }

    row
}

/// Будує сторінку з детальним відображенням прав доступу файлу: зверху
/// символьний+вісімковий запис одним рядком, нижче — три групи
/// (Власник/Група/Інші), кожна з трьома рядками (Читання/Запис/
/// Виконання) і галочкою чи хрестиком праворуч. Суто інформаційна
/// сторінка без редагування — так само, як і сторінка метаданих.
/// Повертає `None`, якщо права доступу прочитати не вдалось (файл
/// видалено, немає доступу тощо) — у цьому разі рядок-перехід у
/// properties.rs узагалі не активний.
pub fn build_permissions_page(path: &Path) -> Option<adw::NavigationPage> {
    let mode = file_mode(path)?;

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new("Права доступу", "")));

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.set_margin_start(20);
    content.set_margin_end(20);
    content.set_margin_top(18);
    content.set_margin_bottom(20);

    let group_owner = permission_group("Власник", mode, 0o400, 0o200, 0o100);
    content.append(&group_owner);

    let group_group = permission_group("Група", mode, 0o040, 0o020, 0o010);
    group_group.set_margin_top(18);
    content.append(&group_group);

    let group_other = permission_group("Інші", mode, 0o004, 0o002, 0o001);
    group_other.set_margin_top(18);
    content.append(&group_other);

    let scroller = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vexpand(true)
        .child(&content)
        .build();

    toolbar_view.set_content(Some(&scroller));

    Some(
        adw::NavigationPage::builder()
            .title("Права доступу")
            .child(&toolbar_view)
            .build(),
    )
}

fn permission_group(title: &str, mode: u32, r: u32, w: u32, x: u32) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder().title(title).build();
    group.add(&permission_flag_row("Читання", mode & r != 0));
    group.add(&permission_flag_row("Запис", mode & w != 0));
    group.add(&permission_flag_row("Виконання", mode & x != 0));
    group
}

fn permission_flag_row(title: &str, enabled: bool) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title(title).build();
    let icon_name = if enabled {
        "object-select-symbolic"
    } else {
        "window-close-symbolic"
    };
    let image = gtk4::Image::from_icon_name(icon_name);
    if !enabled {
        image.add_css_class("dim-label");
    }
    row.add_suffix(&image);
    row
}