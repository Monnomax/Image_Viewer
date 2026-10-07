use crate::model::SortMode;
use crate::preferences::settings::DeleteAction;
use crate::preferences::Settings;
use crate::viewer::Viewer;
use adw::prelude::*;
use adw::ApplicationWindow;
use gtk4::glib::prelude::ToVariant;
use gtk4::{gdk, gio, glib, EventControllerKey, PopoverMenu, Widget};
use std::cell::RefCell;
use std::rc::Rc;

pub fn popup_at(popover: &PopoverMenu, area: &Widget, x: f64, y: f64) {
    position_within(popover, area, x, y);

    let popover = popover.clone();
    glib::idle_add_local_once(move || popover.popup());
}

fn position_within(popover: &PopoverMenu, _area: &Widget, x: f64, y: f64) {
    let rect = gdk::Rectangle::new(x.round() as i32, y.round() as i32, 0, 0);
    popover.set_pointing_to(Some(&rect));
}

pub fn setup(
    window: &ApplicationWindow,
    area: &Widget,
    viewer: Rc<RefCell<Viewer>>,
    settings: Settings,
) -> PopoverMenu {
    let popover = build_popover(window, area, &viewer, &settings);

    let key = EventControllerKey::new();
    let area_for_rect = area.clone();
    let popover_for_key = popover.clone();
    key.connect_key_pressed(move |_, keyval, _keycode, _modifiers| {
        if keyval == gdk::Key::Menu {
            let cx = (area_for_rect.width() / 2) as f64;
            let cy = (area_for_rect.height() / 2) as f64;
            popup_at(&popover_for_key, &area_for_rect, cx, cy);
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    area.add_controller(key);

    popover
}

fn build_popover(
    window: &ApplicationWindow,
    area: &Widget,
    viewer: &Rc<RefCell<Viewer>>,
    settings: &Settings,
) -> PopoverMenu {
    let actions = gio::SimpleActionGroup::new();

    add_sort_actions(&actions, area, viewer, settings);

    add_rotate_actions(&actions, area, viewer);
    add_set_wallpaper_action(&actions, window, viewer);
    add_rename_action(&actions, window, area, viewer);
    add_copy_action(&actions, area, viewer);
    add_share_action(&actions, window, viewer);
    add_delete_action(&actions, window, area, viewer, settings);
    add_properties_action(&actions, viewer);
    add_preferences_action(&actions, area, viewer, settings);

    area.insert_action_group("ctxmenu", Some(&actions));

    let sort_submenu = gio::Menu::new();
    sort_submenu.append(Some("А - Я"), Some("ctxmenu.sort-name-asc"));
    sort_submenu.append(Some("Я - А"), Some("ctxmenu.sort-name-desc"));
    sort_submenu.append(Some("Остання зміна"), Some("ctxmenu.sort-modified-desc"));
    sort_submenu.append(Some("Перша зміна"), Some("ctxmenu.sort-modified-asc"));
    sort_submenu.append(Some("Розмір"), Some("ctxmenu.sort-size"));
    sort_submenu.append(Some("Тип"), Some("ctxmenu.sort-type"));

    let root_menu = gio::Menu::new();

    let section_sort = gio::Menu::new();
    section_sort.append_submenu(Some("Упорядкувати"), &sort_submenu);
    root_menu.append_section(None, &section_sort);

    let section_rotate = gio::Menu::new();
    let rotate_item = gio::MenuItem::new(Some("Повернути"), None);
    rotate_item.set_attribute_value("custom", Some(&"rotate-row".to_variant()));
    section_rotate.append_item(&rotate_item);
    root_menu.append_section(None, &section_rotate);

    let section_actions = gio::Menu::new();
    section_actions.append(Some("Встановити як тло"), Some("ctxmenu.set-wallpaper"));
    section_actions.append(Some("Перейменувати"), Some("ctxmenu.rename"));
    section_actions.append(Some("Копіювати"), Some("ctxmenu.copy"));
    section_actions.append(Some("Поділитися"), Some("ctxmenu.share"));
    section_actions.append(Some("Видалити"), Some("ctxmenu.delete"));
    section_actions.append(Some("Властивості"), Some("ctxmenu.properties"));
    section_actions.append(Some("Налаштувати"), Some("ctxmenu.open-settings"));
    root_menu.append_section(None, &section_actions);

    // ОНОВЛЕНО: Створюємо PopoverMenu одразу з моделлю.
    // У нових версіях GTK виклик set_menu_model на існуючому об'єкті
    // стирає попередньо додані кастомні дочірні елементи.
    let popover = PopoverMenu::builder()
        .menu_model(&root_menu)
        .has_arrow(false)
        .build();
    
    popover.set_parent(area);
    popover.add_child(&build_rotate_row(), "rotate-row");

    popover.set_margin_start(6);
    popover.set_margin_end(6);
    popover.set_margin_top(6);
    popover.set_margin_bottom(6);

    {
        let area_for_resize = area.clone();
        popover.connect_visible_notify(|popover| {
            if popover.is_visible() {
                popover.queue_resize();
            }
        });
        popover.connect_visible_submenu_notify(move |popover| {
            popover.queue_resize();
            let (has_rect, rect) = popover.pointing_to();
            if has_rect {
                position_within(popover, &area_for_resize, rect.x() as f64, rect.y() as f64);
            }
        });
    }

    popover
}

fn add_sort_actions(
    actions: &gio::SimpleActionGroup,
    area: &Widget,
    viewer: &Rc<RefCell<Viewer>>,
    settings: &Settings,
) {
    let modes = [
        ("name-asc", SortMode::NameAsc),
        ("name-desc", SortMode::NameDesc),
        ("modified-desc", SortMode::ModifiedDesc),
        ("modified-asc", SortMode::ModifiedAsc),
        ("size", SortMode::Size),
        ("type", SortMode::Type),
    ];

    for (name, mode) in modes.iter() {
        let action_name = format!("sort-{}", name);
        // Встановлюємо "Остання зміна" як дефолтну активну, подібно до скріншота
        let is_active = *name == "modified-desc";

        // Створюємо булеву дію (boolean state). У GTK4 це відрендериться як галочка (✓)
        let action = gio::SimpleAction::new_stateful(&action_name, None, &is_active.to_variant());

        let viewer = viewer.clone();
        let area = area.clone();
        let settings = settings.clone();
        let actions_clone = actions.clone();
        let current_mode = *mode;
        let current_name = name.to_string();

        action.connect_activate(move |action, _| {
            // Вмикаємо поточну галочку
            action.set_state(&true.to_variant());

            // Вимикаємо всі інші
            let all_names = [
                "name-asc",
                "name-desc",
                "modified-desc",
                "modified-asc",
                "size",
                "type",
            ];
            for other_name in all_names {
                if other_name != current_name {
                    if let Some(other_action) =
                        actions_clone.lookup_action(&format!("sort-{}", other_name))
                    {
                        if let Some(simple) = other_action.downcast_ref::<gio::SimpleAction>() {
                            simple.set_state(&false.to_variant());
                        }
                    }
                }
            }

            viewer.borrow_mut().sort_images(current_mode);
            settings.set_sort_mode(current_mode);
            area.queue_draw();
        });

        actions.add_action(&action);
    }
}

fn build_rotate_row() -> gtk4::Box {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 20);
    row.set_margin_start(12);
    row.set_margin_end(0);
    row.set_margin_top(0);
    row.set_margin_bottom(0);

    let label = gtk4::Label::new(Some("Повернути"));
    label.set_halign(gtk4::Align::Start);
    label.set_hexpand(true);
    row.append(&label);

    let buttons = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);

    let btn_left = gtk4::Button::from_icon_name("object-rotate-left-symbolic");
    btn_left.add_css_class("flat");
    btn_left.set_tooltip_text(Some("Вліво"));
    btn_left.set_action_name(Some("ctxmenu.rotate-left"));
    buttons.append(&btn_left);

    let btn_right = gtk4::Button::from_icon_name("object-rotate-right-symbolic");
    btn_right.add_css_class("flat");
    btn_right.set_tooltip_text(Some("Вправо"));
    btn_right.set_action_name(Some("ctxmenu.rotate-right"));
    buttons.append(&btn_right);

    row.append(&buttons);

    row
}

fn add_rotate_actions(
    actions: &gio::SimpleActionGroup,
    area: &Widget,
    viewer: &Rc<RefCell<Viewer>>,
) {
    let action_left = gio::SimpleAction::new("rotate-left", None);
    action_left.set_enabled(true);
    {
        let viewer = viewer.clone();
        let area = area.clone();
        action_left.connect_activate(move |_, _| {
            crate::viewer::rotate::rotate_current(
                &viewer,
                crate::viewer::rotate::Direction::CounterClockwise,
            );
            area.queue_draw();
        });
    }
    actions.add_action(&action_left);

    let action_right = gio::SimpleAction::new("rotate-right", None);
    action_right.set_enabled(true);
    {
        let viewer = viewer.clone();
        let area = area.clone();
        action_right.connect_activate(move |_, _| {
            crate::viewer::rotate::rotate_current(
                &viewer,
                crate::viewer::rotate::Direction::Clockwise,
            );
            area.queue_draw();
        });
    }
    actions.add_action(&action_right);
}

fn add_set_wallpaper_action(
    actions: &gio::SimpleActionGroup,
    window: &ApplicationWindow,
    viewer: &Rc<RefCell<Viewer>>,
) {
    let action = gio::SimpleAction::new("set-wallpaper", None);
    action.set_enabled(true);
    let window = window.clone();
    let viewer = viewer.clone();
    action.connect_activate(move |_, _| {
        crate::viewer::wallpaper::show(&window, &viewer);
    });
    actions.add_action(&action);
}

fn add_rename_action(
    actions: &gio::SimpleActionGroup,
    window: &ApplicationWindow,
    area: &Widget,
    viewer: &Rc<RefCell<Viewer>>,
) {
    let action = gio::SimpleAction::new("rename", None);
    action.set_enabled(true);
    let window = window.clone();
    let area = area.clone();
    let viewer = viewer.clone();
    action.connect_activate(move |_, _| {
        crate::viewer::rename::show(&window, &area, viewer.clone());
    });
    actions.add_action(&action);
}

fn add_copy_action(actions: &gio::SimpleActionGroup, area: &Widget, viewer: &Rc<RefCell<Viewer>>) {
    let action = gio::SimpleAction::new("copy", None);
    action.set_enabled(true);
    let area = area.clone();
    let viewer = viewer.clone();
    action.connect_activate(move |_, _| {
        let Some(path) = viewer.borrow().model.current_path() else {
            return;
        };

        let uri = gio::File::for_path(&path).uri();
        let uri_list = format!("{uri}\r\n");
        let bytes = glib::Bytes::from(uri_list.as_bytes());
        let provider = gdk::ContentProvider::for_bytes("text/uri-list", &bytes);
        if let Err(err) = area.display().clipboard().set_content(Some(&provider)) {
            eprintln!("Не вдалося скопіювати файл у clipboard {:?}: {}", path, err);
        }
    });
    actions.add_action(&action);
}

fn add_share_action(
    actions: &gio::SimpleActionGroup,
    window: &ApplicationWindow,
    viewer: &Rc<RefCell<Viewer>>,
) {
    let action = gio::SimpleAction::new("share", None);
    action.set_enabled(true);

    let window = window.clone();
    let viewer = viewer.clone();

    action.connect_activate(move |_, _| {
        crate::viewer::share::show(&window, &viewer);
    });

    actions.add_action(&action);
}

fn add_properties_action(actions: &gio::SimpleActionGroup, viewer: &Rc<RefCell<Viewer>>) {
    let action = gio::SimpleAction::new("properties", None);
    action.set_enabled(true);
    let viewer = viewer.clone();
    action.connect_activate(move |_, _| {
        crate::viewer::properties::show(&viewer);
    });
    actions.add_action(&action);
}

fn add_delete_action(
    actions: &gio::SimpleActionGroup,
    window: &ApplicationWindow,
    area: &Widget,
    viewer: &Rc<RefCell<Viewer>>,
    settings: &Settings,
) {
    let action = gio::SimpleAction::new("delete", None);
    action.set_enabled(true);
    let window = window.clone();
    let viewer = viewer.clone();
    let area = area.clone();
    let settings = settings.clone();
    action.connect_activate(move |_, _| {
        let mode = settings.delete_action();
        let do_delete = {
            let viewer = viewer.clone();
            let area = area.clone();
            move || {
                match mode {
                    DeleteAction::Trash => viewer.borrow_mut().delete_current_image(),
                    DeleteAction::Permanent => {
                        viewer.borrow_mut().delete_current_image_permanently()
                    }
                }
                area.queue_draw();
            }
        };

        if !settings.confirm_delete() {
            do_delete();
            return;
        }

        let secondary_text = match mode {
            DeleteAction::Trash => "Файл буде переміщено в кошик. Ви дійсно хочете продовжити?",
            DeleteAction::Permanent => {
                "Файл буде видалено назавжди без можливості відновлення. Продовжити?"
            }
        };

        let confirm_label = match mode {
            DeleteAction::Trash => "Перемістити в кошик",
            DeleteAction::Permanent => "Видалити назавжди",
        };

        let dialog = adw::AlertDialog::builder()
            .heading("Підтвердьте видалення")
            .body(secondary_text)
            .default_response("accept")
            .close_response("cancel")
            .build();
        dialog.add_response("cancel", "Скасувати");
        dialog.add_response("accept", confirm_label);
        if mode == DeleteAction::Permanent {
            dialog.set_response_appearance("accept", adw::ResponseAppearance::Destructive);
        }

        dialog.choose(&window, None::<&gio::Cancellable>, move |response| {
            if response == "accept" {
                do_delete();
            }
        });
    });
    actions.add_action(&action);
}

fn add_preferences_action(
    actions: &gio::SimpleActionGroup,
    area: &Widget,
    viewer: &Rc<RefCell<Viewer>>,
    settings: &Settings,
) {
    let action = gio::SimpleAction::new("open-settings", None);
    action.set_enabled(true);

    let prefs_window: Rc<RefCell<Option<adw::Window>>> = Rc::new(RefCell::new(None));

    let area_clone = area.clone();
    let viewer_clone = viewer.clone();
    let settings_clone = settings.clone();
    let actions_clone = actions.clone();

    action.connect_activate(move |_, _| {
        let viewer = viewer_clone.clone();
        let area = area_clone.clone();
        let actions = actions_clone.clone();

        crate::preferences::show(settings_clone.clone(), &prefs_window, move |mode| {
            viewer.borrow_mut().sort_images(mode);
            area.queue_draw();

            let target_str = match mode {
                SortMode::NameAsc => "name-asc",
                SortMode::NameDesc => "name-desc",
                SortMode::ModifiedDesc => "modified-desc",
                SortMode::ModifiedAsc => "modified-asc",
                SortMode::Size => "size",
                SortMode::Type => "type",
            };

            // Синхронізація: якщо тип сортування змінюється у налаштуваннях, оновлюємо галочку в меню
            let all_names = [
                "name-asc",
                "name-desc",
                "modified-desc",
                "modified-asc",
                "size",
                "type",
            ];
            for name in all_names {
                if let Some(sort_action) = actions.lookup_action(&format!("sort-{}", name)) {
                    if let Some(simple_action) = sort_action.downcast_ref::<gio::SimpleAction>() {
                        simple_action.set_state(&(name == target_str).to_variant());
                    }
                }
            }
        });
    });
    actions.add_action(&action);
}
