use adw::prelude::*;
use gtk4::glib;

pub(crate) fn button_regulator_row(
    title: &str,
    adjustment: &gtk4::Adjustment,
    step: f64,
    digits: u32,
) -> adw::ActionRow {
    button_regulator_row_custom(
        title,
        adjustment,
        move |value| format!("{:.*}", digits as usize, value),
        move |_| step,
    )
}

pub(crate) fn button_regulator_row_custom<F, G>(
    title: &str,
    adjustment: &gtk4::Adjustment,
    format_value: F,
    step_for_value: G,
) -> adw::ActionRow
where
    F: Fn(f64) -> String + Clone + 'static,
    G: Fn(f64) -> f64 + Clone + 'static,
{
    let row = adw::ActionRow::builder().title(title).build();
    let button = gtk4::Button::with_label(&format_value(adjustment.value()));
    button.set_valign(gtk4::Align::Center);
    button.set_size_request(72, -1);

    {
        let button = button.clone();
        let format_value = format_value.clone();
        adjustment.connect_value_changed(move |adjustment| {
            button.set_label(&format_value(adjustment.value()));
        });
    }

    let scroll = gtk4::EventControllerScroll::new(
        gtk4::EventControllerScrollFlags::VERTICAL | gtk4::EventControllerScrollFlags::HORIZONTAL,
    );
    {
        let adjustment = adjustment.clone();
        let step_for_value = step_for_value.clone();
        scroll.connect_scroll(move |_, dx, dy| {
            let delta_sign = if dy < 0.0 || dx < 0.0 {
                1.0
            } else if dy > 0.0 || dx > 0.0 {
                -1.0
            } else {
                return glib::Propagation::Proceed;
            };

            let step = step_for_value(adjustment.value()).max(f64::EPSILON);
            let digits = if step >= 1.0 {
                0
            } else if step >= 0.1 {
                1
            } else {
                2
            };
            let precision = 10_f64.powi(digits);
            let value =
                ((adjustment.value() + delta_sign * step) * precision).round() / precision;
            adjustment.set_value(value.clamp(adjustment.lower(), adjustment.upper()));
            glib::Propagation::Stop
        });
    }
    button.add_controller(scroll);

    row.add_suffix(&button);
    row
}
