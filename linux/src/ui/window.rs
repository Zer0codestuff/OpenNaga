//! The settings window: sidebar, toolbar, section pages and status footer,
//! laid out like the macOS NagaWorkspace.

use super::state::{App, Section};
use super::widgets::*;
use super::{buttons, dialogs, panes};
use adw::prelude::*;
use gtk::{gio, glib};
use std::cell::Cell;
use std::rc::Rc;

pub fn present(app: &Rc<App>) {
    if app.window.borrow().is_none() {
        let window = build(app);
        *app.window.borrow_mut() = Some(window);
    }
    if let Some(window) = app.window.borrow().as_ref() {
        window.present();
    }
    app.set_window_visible(true);
    app.notify();
}

fn build(app: &Rc<App>) -> adw::ApplicationWindow {
    let prefs = app.config.borrow().prefs.clone();
    let window = adw::ApplicationWindow::builder()
        .application(&app.adw_app)
        .title("OpenNaga")
        .default_width(prefs.window_width.max(980))
        .default_height(prefs.window_height.max(700))
        .width_request(980)
        .height_request(700)
        .build();

    let split = adw::OverlaySplitView::new();
    split.set_min_sidebar_width(184.0);
    split.set_max_sidebar_width(184.0);
    split.set_collapsed(false);
    split.set_enable_hide_gesture(false);
    split.set_enable_show_gesture(false);
    split.set_sidebar(Some(&sidebar(app)));
    split.set_content(Some(&content(app, &window)));
    window.set_content(Some(&split));

    install_actions(app, &window);

    let close_app = app.clone();
    window.connect_close_request(move |window| {
        // Closing keeps OpenNaga running in the background, like the macOS
        // menu bar app. Quit from the tray menu or with Ctrl+Q.
        let (width, height) = (window.default_width(), window.default_height());
        {
            let mut config = close_app.config.borrow_mut();
            if config.prefs.window_width != width || config.prefs.window_height != height {
                config.prefs.window_width = width;
                config.prefs.window_height = height;
                if !close_app.snapshot {
                    config.prefs.save();
                }
            }
        }
        window.set_visible(false);
        close_app.set_window_visible(false);
        glib::Propagation::Stop
    });
    window
}

fn sidebar(app: &Rc<App>) -> gtk::Widget {
    let view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.add_css_class("naga-header");
    header.set_show_end_title_buttons(false);
    header.set_show_start_title_buttons(false);
    header.set_title_widget(Some(&hbox(0)));
    let brand = hbox(10);
    let icon = gtk::Image::from_icon_name("opennaga-mouse-filled-symbolic");
    icon.set_pixel_size(24);
    icon.add_css_class("naga-brand-icon");
    brand.append(&icon);
    brand.append(&label("OpenNaga", &["naga-brand"]));
    brand.set_margin_start(8);
    header.pack_start(&brand);
    view.add_top_bar(&header);

    let body = vbox(0);
    let sections = vbox(4);
    sections.set_margin_start(10);
    sections.set_margin_end(10);
    sections.set_margin_top(10);
    let mut section_buttons = Vec::new();
    for section in Section::ALL {
        let row = hbox(10);
        let image = gtk::Image::from_icon_name(section.icon());
        image.set_size_request(18, -1);
        row.append(&image);
        row.append(&label(section.title(), &[]));
        let button = gtk::Button::new();
        button.set_child(Some(&row));
        button.add_css_class("flat");
        button.add_css_class("naga-section");
        let target = app.clone();
        button.connect_clicked(move |_| {
            target.ui.borrow_mut().section = section;
            target.notify();
        });
        sections.append(&button);
        section_buttons.push((section, button));
    }
    body.append(&sections);
    let fill = vbox(0);
    fill.set_vexpand(true);
    body.append(&fill);

    let bottom = vbox(14);
    bottom.set_margin_start(18);
    bottom.set_margin_end(18);
    bottom.set_margin_bottom(18);
    let device = vbox(5);
    device.append(&label("Naga V2 HyperSpeed", &["naga-device"]));
    let connection = hbox(6);
    let connection_dot = dot(false);
    let connection_label = label("Mouse disconnected", &["dim-label", "caption"]);
    connection.append(&connection_dot);
    connection.append(&connection_label);
    device.append(&connection);
    bottom.append(&device);
    bottom.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    let toggle_row = hbox(8);
    let toggle_label = label("Remapping", &["naga-small"]);
    toggle_label.set_hexpand(true);
    let toggle = gtk::Switch::new();
    toggle.set_valign(gtk::Align::Center);
    toggle_row.append(&toggle_label);
    toggle_row.append(&toggle);
    bottom.append(&toggle_row);
    let toggle_caption = label("", &["dim-label", "caption"]);
    bottom.append(&toggle_caption);
    body.append(&bottom);
    view.set_content(Some(&body));

    let updating = Rc::new(Cell::new(false));
    let switch_app = app.clone();
    let guard = updating.clone();
    toggle.connect_state_set(move |_, on| {
        if !guard.get() {
            switch_app.set_remapping(on);
        }
        glib::Propagation::Proceed
    });

    let refresh_app = app.clone();
    app.listen(move || {
        let app = &refresh_app;
        let current = app.ui.borrow().section;
        for (section, button) in &section_buttons {
            if *section == current {
                button.add_css_class("selected");
            } else {
                button.remove_css_class("selected");
            }
        }
        let devices = app.devices.borrow();
        set_dot(&connection_dot, devices.name.is_some());
        connection_label.set_text(&match (&devices.name, &devices.transport) {
            (Some(_), Some(transport)) => transport.clone(),
            (Some(_), None) => "Connected".into(),
            _ => "Mouse disconnected".into(),
        });
        drop(devices);
        let config = app.config.borrow();
        let onboard = app.onboard_active();
        let only = config.current_profile_is_onboard_only();
        updating.set(true);
        toggle.set_state(config.remapping_enabled() && !onboard);
        toggle.set_active(config.remapping_enabled() && !onboard);
        updating.set(false);
        toggle.set_sensitive(!config.profiles_read_only && !onboard && !only && !app.device.borrow().is_busy);
        toggle_caption.set_visible(onboard || only);
        toggle_caption.set_text(if onboard { "Saved in mouse" } else { "Use Save to Mouse" });
    });
    view.upcast()
}

fn content(app: &Rc<App>, window: &adw::ApplicationWindow) -> gtk::Widget {
    let view = adw::ToolbarView::new();
    view.set_top_bar_style(adw::ToolbarStyle::Flat);
    let header = adw::HeaderBar::new();
    header.add_css_class("naga-header");
    header.set_title_widget(Some(&hbox(0)));
    let titles = vbox(2);
    titles.set_valign(gtk::Align::Center);
    titles.set_margin_start(12);
    let title = label("Buttons", &["naga-title"]);
    let subtitle = label("", &["dim-label", "naga-subtitle"]);
    titles.append(&title);
    titles.append(&subtitle);
    header.pack_start(&titles);

    let menu = gio::Menu::new();
    let history = gio::Menu::new();
    history.append(Some("Undo Profile Change"), Some("win.undo"));
    history.append(Some("Redo Profile Change"), Some("win.redo"));
    menu.append_section(None, &history);
    let memory = gio::Menu::new();
    memory.append(Some("Mouse Memory…"), Some("win.mouse-memory"));
    menu.append_section(None, &memory);
    let manage = gio::Menu::new();
    manage.append(Some("Manage Profiles…"), Some("win.manage-profiles"));
    menu.append_section(None, &manage);
    let files = gio::Menu::new();
    files.append(Some("Import Profiles…"), Some("win.import"));
    files.append(Some("Export Profiles…"), Some("win.export"));
    menu.append_section(None, &files);
    let quit = gio::Menu::new();
    quit.append(Some("Quit OpenNaga"), Some("app.quit"));
    menu.append_section(None, &quit);
    let menu_button = gtk::MenuButton::new();
    menu_button.set_icon_name("opennaga-more-symbolic");
    menu_button.set_menu_model(Some(&menu));
    menu_button.set_tooltip_text(Some("Manage, import or export profiles"));
    menu_button.set_valign(gtk::Align::Center);
    header.pack_end(&menu_button);

    let save = button("Save to Mouse…");
    save.set_valign(gtk::Align::Center);
    save.set_action_name(Some("win.mouse-memory"));
    header.pack_end(&save);

    let profile = gtk::DropDown::from_strings(&[]);
    profile.set_size_request(205, -1);
    profile.set_valign(gtk::Align::Center);
    header.pack_end(&profile);
    let profile_label = label("Profile", &[]);
    profile_label.set_valign(gtk::Align::Center);
    header.pack_end(&profile_label);
    view.add_top_bar(&header);

    let body = vbox(0);
    body.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    let permissions = hbox(10);
    permissions.add_css_class("naga-banner");
    permissions.append(&gtk::Image::from_icon_name("opennaga-allow-symbolic"));
    let permissions_text = label("Allow OpenNaga to use the mouse to apply your assignments.", &[]);
    permissions_text.set_hexpand(true);
    permissions.append(&permissions_text);
    let set_up = button("Set Up");
    let set_up_app = app.clone();
    set_up.connect_clicked(move |_| {
        set_up_app.ui.borrow_mut().section = Section::Status;
        set_up_app.notify();
    });
    permissions.append(&set_up);
    body.append(&permissions);

    let error_row = hbox(10);
    error_row.add_css_class("naga-error-banner");
    let error_icon = gtk::Image::from_icon_name("opennaga-warning-symbolic");
    error_icon.set_valign(gtk::Align::Start);
    error_row.append(&error_icon);
    let error_text = wrapped("", &[]);
    error_text.set_hexpand(true);
    error_text.set_selectable(true);
    error_row.append(&error_text);
    let recovery = button("Recovery");
    recovery.set_valign(gtk::Align::Start);
    let recovery_app = app.clone();
    recovery.connect_clicked(move |_| {
        *recovery_app.ui_error.borrow_mut() = None;
        recovery_app.ui.borrow_mut().section = Section::Status;
        recovery_app.notify();
    });
    error_row.append(&recovery);
    body.append(&error_row);

    let stack = gtk::Stack::new();
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    stack.set_transition_duration(120);
    stack.set_vexpand(true);
    stack.add_named(&buttons::build(app), Some("Buttons"));
    stack.add_named(&panes::app_profiles(app), Some("App Profiles"));
    stack.add_named(&panes::sensitivity(app), Some("Sensitivity"));
    stack.add_named(&panes::status(app), Some("Status"));
    body.append(&stack);
    view.set_content(Some(&body));

    let footer = hbox(8);
    footer.add_css_class("naga-footer");
    footer.add_css_class("dim-label");
    let footer_dot = dot(false);
    let footer_status = label("", &[]);
    footer_status.set_hexpand(true);
    let footer_icon = gtk::Image::from_icon_name("opennaga-background-symbolic");
    let footer_hint = label("", &[]);
    footer.append(&footer_dot);
    footer.append(&footer_status);
    footer.append(&footer_icon);
    footer.append(&footer_hint);
    let bottom = vbox(0);
    bottom.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    bottom.append(&footer);
    view.add_bottom_bar(&bottom);

    let updating = Rc::new(Cell::new(false));
    let select_app = app.clone();
    let guard = updating.clone();
    profile.connect_selected_notify(move |dropdown| {
        if guard.get() {
            return;
        }
        if let Some(name) = dropdown_text(dropdown) {
            if name != select_app.config.borrow().current_profile_name {
                select_app.select_profile(&name);
            }
        }
    });

    let refresh_app = app.clone();
    let window = window.clone();
    app.listen(move || {
        let app = &refresh_app;
        let section = app.ui.borrow().section;
        title.set_text(section.title());
        subtitle.set_text(section.subtitle());
        stack.set_visible_child_name(section.title());
        let config = app.config.borrow();
        let names = config.available_profiles();
        updating.set(true);
        set_dropdown_items(&profile, &names);
        if let Some(index) = names.iter().position(|n| *n == config.current_profile_name) {
            if profile.selected() != index as u32 {
                profile.set_selected(index as u32);
            }
        }
        updating.set(false);
        profile.set_sensitive(!config.profiles_read_only);
        let error = app.ui_error.borrow().clone().or_else(|| config.last_error.clone()).or_else(|| app.action_error.borrow().clone());
        error_row.set_visible(error.is_some());
        error_text.set_text(error.as_deref().unwrap_or(""));
        let onboard = app.onboard_active();
        let only = config.current_profile_is_onboard_only();
        drop(config);
        permissions.set_visible(!app.snapshot && !app.permissions_granted());
        set_dot(&footer_dot, app.connected() && app.remapping_active());
        footer_status.set_text(&app.service_status());
        footer_icon.set_icon_name(Some(if onboard || only { "opennaga-memory-symbolic" } else { "opennaga-background-symbolic" }));
        footer_hint.set_text(if onboard {
            if onboard_name_missing() {
                "Mouse restore needed"
            } else {
                "Works even after Quit"
            }
        } else if only {
            "Apply edits with Save to Mouse"
        } else {
            "Keeps running when you close the window"
        });
        update_action_state(app, &window);
    });
    view.upcast()
}

fn onboard_name_missing() -> bool {
    crate::hw::onboard::saved_name().is_none()
}

fn install_actions(app: &Rc<App>, window: &adw::ApplicationWindow) {
    let add = |name: &str, handler: Box<dyn Fn()>| {
        let action = gio::SimpleAction::new(name, None);
        action.connect_activate(move |_, _| handler());
        window.add_action(&action);
    };
    let a = app.clone();
    add("undo", Box::new(move || a.undo()));
    let a = app.clone();
    add("redo", Box::new(move || a.redo()));
    let a = app.clone();
    add("mouse-memory", Box::new(move || dialogs::mouse_memory(&a)));
    let a = app.clone();
    add("manage-profiles", Box::new(move || dialogs::manage_profiles(&a)));
    let a = app.clone();
    add("import", Box::new(move || dialogs::import_profiles(&a)));
    let a = app.clone();
    add("export", Box::new(move || dialogs::export_profiles(&a)));
    let w = window.clone();
    add("close", Box::new(move || w.close()));
    app.adw_app.set_accels_for_action("win.undo", &["<Ctrl><Alt>z"]);
    app.adw_app.set_accels_for_action("win.redo", &["<Ctrl><Shift><Alt>z"]);
    app.adw_app.set_accels_for_action("win.close", &["<Ctrl>w"]);
    app.adw_app.set_accels_for_action("app.quit", &["<Ctrl>q"]);
}

fn update_action_state(app: &Rc<App>, window: &adw::ApplicationWindow) {
    let config = app.config.borrow();
    let set = |name: &str, enabled: bool| {
        if let Some(action) = window.lookup_action(name).and_then(|a| a.downcast::<gio::SimpleAction>().ok()) {
            action.set_enabled(enabled);
        }
    };
    set("undo", config.can_undo());
    set("redo", config.can_redo());
    set("import", !config.profiles_read_only);
}
