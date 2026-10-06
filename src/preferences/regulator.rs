use adw::prelude::*;
use gtk4::glib;

pub(crate) fn button_regulator_row(
    title: &str,
    adjustment: &gtk4::Adjustment,
    step: f64,
    digits: u32,
) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title(title).build();
    let format_value = move |value: f64| format!("{:.*}", digits as usize, value);
    let button = gtk4::Button::with_label(&format_value(adjustment.value()));
    button.set_valign(gtk4::Align::Center);
    button.set_size_request(72, -1);

    {
        let button = button.clone();
        adjustment.connect_value_changed(move |adjustment| {
            button.set_label(&format_value(adjustment.value()));
        });
    }

    let scroll = gtk4::EventControllerScroll::new(
        gtk4::EventControllerScrollFlags::VERTICAL | gtk4::EventControllerScrollFlags::HORIZONTAL,
    );
    {
        let adjustment = adjustment.clone();
        scroll.connect_scroll(move |_, dx, dy| {
            let delta = if dy < 0.0 || dx < 0.0 {
                step
            } else if dy > 0.0 || dx > 0.0 {
                -step
            } else {
                return glib::Propagation::Proceed;
            };
            let precision = 10_f64.powi(digits as i32);
            let value = ((adjustment.value() + delta) * precision).round() / precision;
            adjustment.set_value(value.clamp(adjustment.lower(), adjustment.upper()));
            glib::Propagation::Stop
        });
    }
    button.add_controller(scroll);

    row.add_suffix(&button);
    row
}
