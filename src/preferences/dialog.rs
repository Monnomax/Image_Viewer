use crate::model::SortMode;
use crate::preferences::settings::Settings;
use crate::preferences::{general, navigation, view};
use crate::viewer::animations;
use adw::prelude::*;
use gtk4::glib;
use std::cell::RefCell;
use std::rc::Rc;

struct PageEntry {
    id: &'static str,
    title: &'static str,
    icon_name: &'static str,
    page: adw::PreferencesPage,
}

pub fn show<F>(
    settings: Settings,
    existing: &Rc<RefCell<Option<adw::Window>>>,
    on_sort_changed: F,
) where
    F: Fn(SortMode) + 'static,
{
    if let Some(window) = existing.borrow().as_ref() {
        window.present();
        return;
    }

    let on_sort_changed = Rc::new(on_sort_changed);

    let pages = vec![
        PageEntry {
            id: "general",
            title: "Загальні",
            icon_name: "preferences-system-symbolic",
            page: general::build_page(&settings, on_sort_changed.clone()),
        },
        PageEntry {
            id: "view",
            title: "Вигляд",
            icon_name: "image-x-generic-symbolic",
            page: view::build_page(&settings),
        },
        PageEntry {
            id: "navigation",
            title: "Навігація",
            icon_name: "input-mouse-symbolic",
            page: navigation::build_page(&settings),
        },
        PageEntry {
            id: "animations",
            title: "Анімації",
            icon_name: "preferences-desktop-display-symbolic",
            page: animations::build_page(&settings),
        },
    ];

    let stack = gtk4::Stack::new();
    stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
    for entry in &pages {
        stack.add_named(&entry.page, Some(entry.id));
    }

    let listbox = gtk4::ListBox::new();
    listbox.add_css_class("navigation-sidebar");
    listbox.set_selection_mode(gtk4::SelectionMode::Single);

    for entry in &pages {
        listbox.append(&sidebar_row(entry.icon_name, entry.title, entry.id));
    }

    let sidebar_scroller = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(&listbox)
        .build();

    let sidebar_header = adw::HeaderBar::new();
    sidebar_header.set_title_widget(Some(&adw::WindowTitle::new("Налаштування", "")));
    sidebar_header.set_show_end_title_buttons(false);

    let sidebar_toolbar = adw::ToolbarView::new();
    sidebar_toolbar.add_top_bar(&sidebar_header);
    sidebar_toolbar.set_content(Some(&sidebar_scroller));

    let sidebar_nav_page = adw::NavigationPage::builder()
        .title("Налаштування")
        .child(&sidebar_toolbar)
        .build();

    let content_title = adw::WindowTitle::new(pages[0].title, "");

    let content_header = adw::HeaderBar::new();
    content_header.set_title_widget(Some(&content_title));
    content_header.set_show_start_title_buttons(false);

    let content_toolbar = adw::ToolbarView::new();
    content_toolbar.add_top_bar(&content_header);
    content_toolbar.set_content(Some(&stack));

    let content_nav_page = adw::NavigationPage::builder()
        .title(pages[0].title)
        .child(&content_toolbar)
        .build();

    let split_view = adw::NavigationSplitView::builder()
        .sidebar(&sidebar_nav_page)
        .content(&content_nav_page)
        .min_sidebar_width(212.0)
        .max_sidebar_width(212.0)
        .sidebar_width_fraction(0.32)
        .build();

    let window = adw::Window::builder()
    .title("Налаштування")
    .default_width(853)
        .default_height(640)
    .destroy_with_parent(true)
    .modal(false)
    .build();

    window.set_modal(false);

    window.set_content(Some(&split_view));

    let breakpoint = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        500.0,
        adw::LengthUnit::Sp,
    ));
    breakpoint.add_setter(&split_view, "collapsed", Some(&true.to_value()));
    window.add_breakpoint(breakpoint);
    {
        let stack = stack.clone();
        let content_title = content_title.clone();
        let content_nav_page = content_nav_page.clone();
        let titles: Vec<(&'static str, &'static str)> =
            pages.iter().map(|e| (e.id, e.title)).collect();

        listbox.connect_row_selected(move |_, row| {
            let Some(row) = row else { return };
            let id = row.widget_name();
            stack.set_visible_child_name(&id);
            if let Some((_, title)) = titles.iter().find(|(pid, _)| *pid == id.as_str()) {
                content_title.set_title(title);
                content_nav_page.set_title(title);
            }
        });
    }

    {
        let split_view = split_view.clone();
        listbox.connect_row_activated(move |_, _row| {
            split_view.set_show_content(true);
        });
    }

    if let Some(first_row) = listbox.row_at_index(0) {
        listbox.select_row(Some(&first_row));
    }

    *existing.borrow_mut() = Some(window.clone());

    {
        let existing = existing.clone();
        window.connect_close_request(move |_| {
            *existing.borrow_mut() = None;
            glib::Propagation::Proceed
        });
    }

    window.present();
}

fn sidebar_row(icon_name: &str, title: &str, id: &'static str) -> gtk4::ListBoxRow {
    let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.append(&gtk4::Image::from_icon_name(icon_name));
    content.append(&gtk4::Label::new(Some(title)));

    let row = gtk4::ListBoxRow::new();
    row.set_child(Some(&content));
    row.set_widget_name(id);
    row
}
