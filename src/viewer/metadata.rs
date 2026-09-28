use adw::prelude::*;
use crate::viewer::file_info;
use gtk4::gdk_pixbuf::Pixbuf;
#[cfg(not(feature = "gexiv2-metadata"))]
use little_exif::exif_tag::ExifTag;
#[cfg(not(feature = "gexiv2-metadata"))]
use little_exif::metadata::Metadata as ExifMetadata;
use std::path::Path;
use std::rc::Rc;

pub fn build_metadata_page(
    path: &Path,
    ext: &str,
    pixbuf: Option<&Rc<Pixbuf>>,
) -> adw::NavigationPage {
    build_metadata_page_impl(path, ext, pixbuf)
}

#[cfg(feature = "gexiv2-metadata")]
fn build_metadata_page_impl(
    path: &Path,
    ext: &str,
    pixbuf: Option<&Rc<Pixbuf>>,
) -> adw::NavigationPage {
    use rexiv2::Metadata;

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new("Метадані файлу", "")));

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.set_margin_start(20);
    content.set_margin_end(20);
    content.set_margin_top(18);
    content.set_margin_bottom(20);

    let metadata = Metadata::new_from_path(path);
    let dpi = metadata
        .as_ref()
        .ok()
        .and_then(dpi_from_metadata)
        .unwrap_or_else(|| dpi_text(path, pixbuf));

    let group_file = adw::PreferencesGroup::new();
    group_file.add(&row(
        "Повний шлях",
        path.to_string_lossy().into_owned(),
        None,
    ));
    group_file.add(&row(
        "Ім'я файлу",
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        None,
    ));

    let dims_text = pixbuf
        .map(|p| format!("{} × {} пікс.", p.width(), p.height()))
        .unwrap_or_else(|| "Невідомо".to_string());
    group_file.add(&row("Розмір зображення", dims_text, None));
    group_file.add(&row("DPI", dpi, None));
    group_file.add(&row("Формат", format_extension_name(ext), None));

    let exact_size_text = std::fs::metadata(path)
        .map(|m| file_info::format_exact_file_size(m.len()))
        .unwrap_or_else(|_| "Невідомо".to_string());
    group_file.add(&row("Розмір файлу", exact_size_text, None));

    content.append(&group_file);

    match metadata {
        Ok(metadata) => {
            add_tag_group(
                &content,
                "EXIF",
                metadata.get_exif_tags().unwrap_or_default(),
                |tag| metadata_value(&metadata, tag),
            );
            add_tag_group(
                &content,
                "XMP",
                metadata.get_xmp_tags().unwrap_or_default(),
                |tag| metadata_value(&metadata, tag),
            );
            add_tag_group(
                &content,
                "IPTC",
                metadata.get_iptc_tags().unwrap_or_default(),
                |tag| metadata_value(&metadata, tag),
            );
        }
        Err(err) => {
            let group_error = adw::PreferencesGroup::new();
            group_error.set_margin_top(18);
            group_error.add(&row(
                "Метадані",
                format!("Не вдалося прочитати через GExiv2: {err}"),
                Some("Перевір наявність gexiv2 та exiv2 у системі".to_string()),
            ));
            content.append(&group_error);
        }
    }

    let scroller = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vexpand(true)
        .child(&content)
        .build();

    toolbar_view.set_content(Some(&scroller));

    adw::NavigationPage::builder()
        .title("Метадані файлу")
        .child(&toolbar_view)
        .build()
}

#[cfg(not(feature = "gexiv2-metadata"))]
fn build_metadata_page_impl(
    path: &Path,
    ext: &str,
    pixbuf: Option<&Rc<Pixbuf>>,
) -> adw::NavigationPage {
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new("Метадані файлу", "")));

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.set_margin_start(20);
    content.set_margin_end(20);
    content.set_margin_top(18);
    content.set_margin_bottom(20);

    let group_file = adw::PreferencesGroup::new();
    group_file.add(&row(
        "Повний шлях",
        path.to_string_lossy().into_owned(),
        None,
    ));
    group_file.add(&row(
        "Ім'я файлу",
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        None,
    ));

    let dims_text = pixbuf
        .map(|p| format!("{} × {} пікс.", p.width(), p.height()))
        .unwrap_or_else(|| "Невідомо".to_string());
    group_file.add(&row("Розмір зображення", dims_text, None));
    group_file.add(&row("DPI", dpi_text(path, pixbuf), None));
    group_file.add(&row("Формат", format_extension_name(ext), None));

    let exact_size_text = std::fs::metadata(path)
        .map(|m| file_info::format_exact_file_size(m.len()))
        .unwrap_or_else(|_| "Невідомо".to_string());
    group_file.add(&row("Розмір файлу", exact_size_text, None));
    content.append(&group_file);

    let group_note = adw::PreferencesGroup::new();
    group_note.set_margin_top(18);
    group_note.add(&row(
        "GExiv2",
        "Не підключено в цій збірці".to_string(),
        Some("Щоб увімкнути повний список метаданих, зібрай з feature gexiv2-metadata та встановленим системним gexiv2".to_string()),
    ));
    content.append(&group_note);

    let group_exif = adw::PreferencesGroup::new();
    group_exif.set_margin_top(18);
    let exif_row = metadata_expander_row("EXIF", exif_has_data(path, ext));
    if exif_has_data(path, ext) {
        exif_row.add_row(&row("EXIF: орієнтація", exif_orientation_text(path, ext), None));
    }
    group_exif.add(&exif_row);
    content.append(&group_exif);

    let scroller = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vexpand(true)
        .child(&content)
        .build();

    toolbar_view.set_content(Some(&scroller));

    adw::NavigationPage::builder()
        .title("Метадані файлу")
        .child(&toolbar_view)
        .build()
}

#[cfg(feature = "gexiv2-metadata")]
fn add_tag_group<F>(content: &gtk4::Box, title: &str, mut tags: Vec<String>, value_for_tag: F)
where
    F: Fn(&str) -> String,
{
    tags.sort_unstable();

    let group = adw::PreferencesGroup::new();
    group.set_margin_top(18);
    group.add(&metadata_expander_group(title, tags, value_for_tag));
    content.append(&group);
}

#[cfg(feature = "gexiv2-metadata")]
fn metadata_expander_group<F>(title: &str, tags: Vec<String>, value_for_tag: F) -> adw::ExpanderRow
where
    F: Fn(&str) -> String,
{
    let state = metadata_expander_state(title, &tags);
    let expander = adw::ExpanderRow::builder().title(title).build();
    expander.set_sensitive(state.enabled);
    expander.set_activatable(state.enabled);
    expander.set_expanded(false);

    if let Some(subtitle) = state.subtitle {
        expander.set_subtitle(&subtitle);
    }

    if state.enabled {
        for tag in tags {
            let label = rexiv2::get_tag_label(&tag).unwrap_or_else(|_| tag.clone());
            let value = value_for_tag(&tag);
            expander.add_row(&row(&label, value, Some(tag)));
        }
    }

    expander
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MetadataExpanderState {
    enabled: bool,
    subtitle: Option<String>,
}

fn metadata_expander_state(title: &str, tags: &[String]) -> MetadataExpanderState {
    let _ = title;
    let enabled = !tags.is_empty();
    let subtitle = if enabled { None } else { Some("Дані відсутні".to_string()) };
    MetadataExpanderState { enabled, subtitle }
}

#[cfg(not(feature = "gexiv2-metadata"))]
fn metadata_expander_row(title: &str, has_data: bool) -> adw::ExpanderRow {
    let row = adw::ExpanderRow::builder().title(title).build();
    row.set_sensitive(has_data);
    row.set_activatable(has_data);
    row.set_expanded(false);
    if !has_data {
        row.set_subtitle("Дані відсутні");
    }
    row
}

#[cfg(not(feature = "gexiv2-metadata"))]
fn exif_has_data(path: &Path, ext: &str) -> bool {
    if !matches!(ext, "jpg" | "jpeg" | "tif" | "tiff") {
        return false;
    }

    let Ok(metadata) = ExifMetadata::new_from_path(path) else {
        return false;
    };

    matches!(metadata.get_tag(&ExifTag::Orientation(Vec::new())).next(), Some(ExifTag::Orientation(_)))
}

#[cfg(feature = "gexiv2-metadata")]
fn metadata_value(metadata: &rexiv2::Metadata, tag: &str) -> String {
    metadata
        .get_tag_interpreted_string(tag)
        .or_else(|_| metadata.get_tag_string(tag))
        .or_else(|_| {
            metadata
                .get_tag_multiple_strings(tag)
                .map(|values| values.join(", "))
        })
        .unwrap_or_else(|_| "Невідомо".to_string())
}

fn row(title: &str, value: impl AsRef<str>, subtitle: Option<String>) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title(title).build();
    if let Some(subtitle) = subtitle {
        row.set_subtitle(&subtitle);
    }

    let value_label = gtk4::Label::new(Some(value.as_ref()));
    value_label.set_halign(gtk4::Align::End);
    value_label.set_wrap(false);
    value_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
    row.add_suffix(&value_label);
    row
}

fn dpi_text(path: &Path, pixbuf: Option<&Rc<Pixbuf>>) -> String {
    let pixbuf = pixbuf
        .filter(|pixbuf| pixbuf.option("x-dpi").is_some())
        .cloned()
        .or_else(|| Pixbuf::from_file(path).ok().map(Rc::new));

    let Some(pixbuf) = pixbuf else {
        return "Невідомо".to_string();
    };

    let x_dpi = pixbuf.option("x-dpi");
    let y_dpi = pixbuf.option("y-dpi");
    match (x_dpi, y_dpi) {
        (Some(x_dpi), Some(y_dpi)) if x_dpi == y_dpi => format!("{x_dpi} DPI"),
        (Some(x_dpi), Some(y_dpi)) => format!("{x_dpi} × {y_dpi} DPI"),
        (Some(dpi), None) | (None, Some(dpi)) => format!("{dpi} DPI"),
        (None, None) => "Невідомо".to_string(),
    }
}

#[cfg(feature = "gexiv2-metadata")]
fn dpi_from_metadata(metadata: &rexiv2::Metadata) -> Option<String> {
    let x_dpi = metadata
        .get_tag_string("Exif.Image.XResolution")
        .ok()
        .and_then(|value| parse_resolution(&value))?;
    let y_dpi = metadata
        .get_tag_string("Exif.Image.YResolution")
        .ok()
        .and_then(|value| parse_resolution(&value))?;
    let unit = metadata
        .get_tag_string("Exif.Image.ResolutionUnit")
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
        .unwrap_or(2);

    let multiplier = match unit {
        2 => 1.0,
        3 => 2.54,
        _ => return None,
    };
    let x_dpi = x_dpi * multiplier;
    let y_dpi = y_dpi * multiplier;

    if (x_dpi - y_dpi).abs() < 0.01 {
        Some(format!("{} DPI", format_resolution(x_dpi)))
    } else {
        Some(format!(
            "{} × {} DPI",
            format_resolution(x_dpi),
            format_resolution(y_dpi)
        ))
    }
}

#[cfg(feature = "gexiv2-metadata")]
fn parse_resolution(value: &str) -> Option<f64> {
    let value = value.trim();
    if let Some((numerator, denominator)) = value.split_once('/') {
        let numerator = numerator.trim().parse::<f64>().ok()?;
        let denominator = denominator.trim().parse::<f64>().ok()?;
        (denominator != 0.0).then_some(numerator / denominator)
    } else {
        value.parse::<f64>().ok()
    }
}

#[cfg(feature = "gexiv2-metadata")]
fn format_resolution(value: f64) -> String {
    if (value - value.round()).abs() < 0.01 {
        format!("{:.0}", value)
    } else {
        format!("{value:.2}")
    }
}

fn format_extension_name(ext: &str) -> String {
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

#[cfg(not(feature = "gexiv2-metadata"))]
fn exif_orientation_text(path: &Path, ext: &str) -> String {
    if !matches!(ext, "jpg" | "jpeg" | "tif" | "tiff") {
        return "Не підтримується для цього формату".to_string();
    }

    let Ok(metadata) = ExifMetadata::new_from_path(path) else {
        return "Дані відсутні".to_string();
    };

    match metadata.get_tag(&ExifTag::Orientation(Vec::new())).next() {
        Some(ExifTag::Orientation(values)) => {
            let value = values.first().copied().unwrap_or(1);
            orientation_description(value).to_string()
        }
        _ => "Тег відсутній".to_string(),
    }
}

#[cfg(not(feature = "gexiv2-metadata"))]
fn orientation_description(value: u16) -> &'static str {
    match value {
        1 => "стандартна",
        2 => "дзеркально по горизонталі",
        3 => "повернуто на 180°",
        4 => "дзеркально по вертикалі",
        5 => "дзеркально + 90° проти годинникової",
        6 => "повернуто на 90° за годинниковою",
        7 => "дзеркально + 90° за годинниковою",
        8 => "повернуто на 90° проти годинникової",
        _ => "невідома",
    }
}

#[cfg(test)]
mod tests {
    use super::metadata_expander_state;

    #[test]
    fn metadata_expander_state_marks_empty_groups_as_disabled() {
        let state = metadata_expander_state("EXIF", &[]);
        assert!(!state.enabled);
        assert_eq!(state.subtitle, Some("Дані відсутні".to_string()));
    }

    #[test]
    fn metadata_expander_state_keeps_non_empty_groups_enabled() {
        let state = metadata_expander_state("XMP", &["Xmp.dc.title".to_string()]);
        assert!(state.enabled);
        assert_eq!(state.subtitle, None);
    }
}