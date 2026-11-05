//! Sensitivity, Status and App Profiles sections.

use super::state::App;
use super::widgets::*;
use crate::desktop::access;
use crate::desktop::focus::Backend;
use crate::hw::controller::Operation;
use crate::model::scroll::ScrollSensitivity;
use adw::prelude::*;
use gtk::glib;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

fn section_header(title: &str, detail: &str) -> gtk::Box {
    let header = vbox(6);
    header.append(&label(title, &["title-2"]));
    if !detail.is_empty() {
        header.append(&dim(detail));
    }
    header
}

// MARK: - Sensitivity

pub fn sensitivity(app: &Rc<App>) -> gtk::Widget {
    let page = padded(24, 24);
    let top = hbox(12);
    let header = section_header("Sensitivity", "Hardware settings change only when you apply them.");
    header.set_hexpand(true);
    top.append(&header);
    let refresh = labelled_icon_button("opennaga-refresh-symbolic", "Refresh");
    refresh.set_valign(gtk::Align::Start);
    let a = app.clone();
    refresh.connect_clicked(move |_| {
        a.hardware(Operation::Refresh, None);
    });
    top.append(&refresh);
    page.append(&top);
    let unavailable = info("opennaga-usb-symbolic", "Hardware control unavailable. Connect the Naga V2 HyperSpeed USB receiver.");
    unavailable.add_css_class("dim-label");
    page.append(&unavailable);

    // DPI
    let (dpi_card, dpi) = group(16);
    dpi.append(&label("DPI", &["heading"]));
    let dpi_current = label("", &["dim-label"]);
    dpi.append(&dpi_current);
    let dpi_row = hbox(10);
    let x_entry = gtk::Entry::new();
    x_entry.set_placeholder_text(Some("X axis"));
    x_entry.set_width_chars(10);
    let y_entry = gtk::Entry::new();
    y_entry.set_placeholder_text(Some("Y axis"));
    y_entry.set_width_chars(10);
    dpi_row.append(&x_entry);
    dpi_row.append(&y_entry);
    dpi_row.append(&spacer());
    let apply_dpi = button("Apply DPI");
    dpi_row.append(&apply_dpi);
    dpi.append(&dpi_row);
    let presets = hbox(6);
    presets.append(&label("Presets", &["dim-label"]));
    let edited_dpi = Rc::new(Cell::new(false));
    for value in [400, 800, 1600, 3200, 6400] {
        let b = button(&value.to_string());
        let (x, y, edited) = (x_entry.clone(), y_entry.clone(), edited_dpi.clone());
        b.connect_clicked(move |_| {
            x.set_text(&value.to_string());
            y.set_text(&value.to_string());
            edited.set(true);
        });
        presets.append(&b);
    }
    dpi.append(&presets);
    dpi.append(&caption("100 to 30,000 DPI per axis. Presets fill in the fields without changing the mouse."));
    page.append(&dpi_card);

    // Polling rate
    let (rate_card, rate) = group(16);
    rate.append(&label("Polling rate", &["heading"]));
    let rate_current = label("", &["dim-label"]);
    rate.append(&rate_current);
    let rate_row = hbox(10);
    let rates = hbox(0);
    rates.add_css_class("linked");
    rates.add_css_class("naga-segmented");
    let mut rate_buttons: Vec<(i32, gtk::ToggleButton)> = Vec::new();
    for hz in [125, 500, 1000] {
        let b = gtk::ToggleButton::with_label(&format!("{hz} Hz"));
        if let Some((_, first)) = rate_buttons.first() {
            b.set_group(Some(first));
        }
        rates.append(&b);
        rate_buttons.push((hz, b));
    }
    rate_row.append(&rates);
    rate_row.append(&spacer());
    let apply_rate = button("Apply Rate");
    rate_row.append(&apply_rate);
    rate.append(&rate_row);
    page.append(&rate_card);

    // Scroll sensitivity
    let (scroll_card, scroll) = group(12);
    let scroll_top = hbox(8);
    let scroll_title = label("Scroll sensitivity", &["heading"]);
    scroll_title.set_hexpand(true);
    scroll_top.append(&scroll_title);
    let reset = button("Reset");
    scroll_top.append(&reset);
    scroll.append(&scroll_top);
    let mut sliders = Vec::new();
    for (title, vertical) in [("Vertical wheel", true), ("Horizontal / wheel tilt", false)] {
        let block = vbox(6);
        let row = hbox(8);
        let name = label(title, &[]);
        name.set_hexpand(true);
        row.append(&name);
        let value = label("1.0×", &["dim-label", "numeric"]);
        row.append(&value);
        block.append(&row);
        let slider = gtk::Scale::with_range(gtk::Orientation::Horizontal, ScrollSensitivity::MIN, ScrollSensitivity::MAX, 0.1);
        slider.set_round_digits(1);
        slider.set_draw_value(false);
        block.append(&slider);
        let ends = hbox(8);
        let slower = label("Slower", &["dim-label", "caption"]);
        slower.set_hexpand(true);
        ends.append(&slower);
        ends.append(&label("Faster", &["dim-label", "caption"]));
        block.append(&ends);
        scroll.append(&block);
        sliders.push((vertical, slider, value));
    }
    scroll.append(&caption("Applies to the Naga's wheel, tilt and assigned scroll actions while OpenNaga is running with remapping enabled. These settings are not saved to the mouse."));
    page.append(&scroll_card);

    // Driver mode
    let (driver_card, driver) = group(12);
    let driver_row = hbox(10);
    let driver_title = label("Driver mode for the top buttons", &[]);
    driver_title.set_hexpand(true);
    driver_row.append(&driver_title);
    let driver_switch = gtk::Switch::new();
    driver_switch.set_valign(gtk::Align::Center);
    driver_row.append(&driver_switch);
    driver.append(&driver_row);
    driver.append(&dim("Optional. Lets the software handle the top DPI buttons and the wheel tilt. It can replace their built-in DPI function. Turn it off to restore normal mode."));
    let recover = button("Restore Original Mode");
    recover.set_halign(gtk::Align::Start);
    let a = app.clone();
    recover.connect_clicked(move |_| {
        a.hardware(Operation::Recover, None);
    });
    driver.append(&recover);
    let recover_hint = caption("Use the same receiver in its original USB port. Restoring does not happen automatically after an unexpected quit.");
    driver.append(&recover_hint);
    page.append(&driver_card);

    let status_row = hbox(10);
    let spinner = gtk::Spinner::new();
    status_row.append(&spinner);
    let status = wrapped("", &["dim-label"]);
    status.set_selectable(true);
    status_row.append(&status);
    page.append(&status_row);

    // Handlers.
    let updating = Rc::new(Cell::new(false));
    for entry in [&x_entry, &y_entry] {
        let edited = edited_dpi.clone();
        let guard = updating.clone();
        entry.connect_changed(move |_| {
            if !guard.get() {
                edited.set(true);
            }
        });
    }
    let (a, x, y, edited) = (app.clone(), x_entry.clone(), y_entry.clone(), edited_dpi.clone());
    apply_dpi.connect_clicked(move |_| {
        if let (Ok(xv), Ok(yv)) = (x.text().trim().parse::<i32>(), y.text().trim().parse::<i32>()) {
            if a.hardware(Operation::Dpi(xv, yv), None) {
                edited.set(false);
            }
        }
    });
    let edited_rate = Rc::new(Cell::new(false));
    for (_, b) in &rate_buttons {
        let (edited, guard) = (edited_rate.clone(), updating.clone());
        b.connect_toggled(move |_| {
            if !guard.get() {
                edited.set(true);
            }
        });
    }
    let (a, buttons, edited) = (app.clone(), rate_buttons.iter().map(|(h, b)| (*h, b.clone())).collect::<Vec<_>>(), edited_rate.clone());
    apply_rate.connect_clicked(move |_| {
        if let Some((hz, _)) = buttons.iter().find(|(_, b)| b.is_active()) {
            if a.hardware(Operation::Polling(*hz), None) {
                edited.set(false);
            }
        }
    });
    for (vertical, slider, _) in &sliders {
        let (a, guard, vertical) = (app.clone(), updating.clone(), *vertical);
        slider.connect_value_changed(move |s| {
            if guard.get() {
                return;
            }
            let mut sensitivity = a.config.borrow().scroll_sensitivity();
            let value = (s.value() * 10.0).round() / 10.0;
            if vertical {
                sensitivity.vertical = value;
            } else {
                sensitivity.horizontal = value;
            }
            a.set_scroll(sensitivity);
        });
    }
    let a = app.clone();
    reset.connect_clicked(move |_| a.set_scroll(ScrollSensitivity::BASELINE));
    let (a, guard) = (app.clone(), updating.clone());
    driver_switch.connect_state_set(move |_, on| {
        if !guard.get() && !a.onboard_active() {
            a.hardware(Operation::Mode(on), None);
        }
        glib::Propagation::Stop
    });

    let refresh_app = app.clone();
    app.listen(move || {
        let app = &refresh_app;
        let device = app.device.borrow().clone();
        let available = device.is_connected && !device.is_busy;
        unavailable.set_visible(!device.is_connected && !device.is_busy);
        refresh.set_sensitive(!device.is_busy && !app.snapshot);
        updating.set(true);
        dpi_current.set_text(&format!(
            "Current value: X {} · Y {}",
            device.dpi_x.map(|v| v.to_string()).unwrap_or_else(|| "unavailable".into()),
            device.dpi_y.map(|v| v.to_string()).unwrap_or_else(|| "unavailable".into())
        ));
        if !edited_dpi.get() {
            x_entry.set_text(&device.dpi_x.map(|v| v.to_string()).unwrap_or_default());
            y_entry.set_text(&device.dpi_y.map(|v| v.to_string()).unwrap_or_default());
        }
        let valid = |e: &gtk::Entry| e.text().trim().parse::<i32>().map(|v| (100..=30000).contains(&v)).unwrap_or(false);
        apply_dpi.set_sensitive(available && valid(&x_entry) && valid(&y_entry) && device.dpi_x.is_some());
        rate_current.set_text(&device.polling_rate.map(|r| format!("Current value: {r} Hz")).unwrap_or_else(|| "Value unavailable".into()));
        if !edited_rate.get() {
            let current = device.polling_rate.unwrap_or(1000);
            for (hz, b) in &rate_buttons {
                if *hz == current {
                    b.set_active(true);
                }
            }
        }
        apply_rate.set_sensitive(available && device.polling_rate.is_some());
        let sensitivity = app.config.borrow().scroll_sensitivity();
        for (vertical, slider, value) in &sliders {
            let v = if *vertical { sensitivity.vertical } else { sensitivity.horizontal };
            if (slider.value() - v).abs() > 0.001 {
                slider.set_value(v);
            }
            value.set_text(&format!("{v:.1}×"));
        }
        reset.set_sensitive(sensitivity != ScrollSensitivity::BASELINE);
        driver_switch.set_state(device.driver_mode_enabled);
        driver_switch.set_active(device.driver_mode_enabled);
        driver_switch.set_sensitive(available && !device.recovery_pending && !app.onboard_active());
        updating.set(false);
        recover.set_visible(device.recovery_pending);
        recover.set_sensitive(!device.is_busy);
        recover_hint.set_visible(device.recovery_pending);
        spinner.set_spinning(device.is_busy);
        spinner.set_visible(device.is_busy);
        status.set_text(&device.status_message);
    });
    scrolled(&page).upcast()
}

// MARK: - Status

fn access_row(title: &str, detail: &str) -> (gtk::Box, gtk::Image, gtk::Label) {
    let row = hbox(12);
    let icon = gtk::Image::from_icon_name("opennaga-warning-symbolic");
    icon.set_pixel_size(24);
    row.append(&icon);
    let texts = vbox(4);
    texts.set_hexpand(true);
    texts.append(&label(title, &["heading"]));
    texts.append(&caption(detail));
    row.append(&texts);
    let state = label("", &["dim-label"]);
    row.append(&state);
    (row, icon, state)
}

fn set_access(icon: &gtk::Image, state: &gtk::Label, granted: bool, text: &str) {
    icon.set_icon_name(Some(if granted { "opennaga-check-symbolic" } else { "opennaga-warning-symbolic" }));
    icon.remove_css_class("naga-ok");
    icon.remove_css_class("naga-warning");
    icon.add_css_class(if granted { "naga-ok" } else { "naga-warning" });
    state.set_text(text);
}

fn status_line(title: &str) -> (gtk::Box, gtk::Label) {
    let row = hbox(12);
    let name = label(title, &["dim-label"]);
    name.set_size_request(170, -1);
    name.set_valign(gtk::Align::Start);
    row.append(&name);
    let value = wrapped("", &[]);
    value.set_selectable(true);
    value.set_hexpand(true);
    row.append(&value);
    (row, value)
}

pub fn status(app: &Rc<App>) -> gtk::Widget {
    let page = padded(24, 24);
    let top = hbox(12);
    let header = section_header("Permissions and connection", "Access is checked again automatically when devices change.");
    header.set_hexpand(true);
    top.append(&header);
    let again = button("Check Again");
    again.set_valign(gtk::Align::Start);
    let a = app.clone();
    again.connect_clicked(move |_| a.restart_input());
    top.append(&again);
    page.append(&top);

    let (access_card, access_box) = group(18);
    let (input_row, input_icon, input_state) = access_row("Mouse input", "Lets OpenNaga recognize and intercept the mouse buttons (the Naga's /dev/input nodes).");
    let (virtual_row, virtual_icon, virtual_state) = access_row("Virtual input", "Lets OpenNaga send keys, clicks and scrolling for your assignments (/dev/uinput).");
    let (hidraw_row, hidraw_icon, hidraw_state) = access_row("Receiver control", "DPI, polling rate, mouse memory and the DPI buttons (the receiver's hidraw nodes).");
    let (keys_row, keys_icon, keys_state) = access_row(
        "Keyboard modifiers (optional)",
        "Reads which modifiers you hold on other keyboards, so Ctrl on the keyboard plus a side button makes a shortcut on compositors that keep modifiers per device. Needs membership in the input group.",
    );
    access_box.append(&input_row);
    access_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    access_box.append(&virtual_row);
    access_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    access_box.append(&hidraw_row);
    access_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    access_box.append(&keys_row);
    access_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    let rule_row = hbox(10);
    let rule_text = wrapped("", &["dim-label", "caption"]);
    rule_text.set_hexpand(true);
    rule_row.append(&rule_text);
    let install = button("Install Access Rule…");
    install.set_valign(gtk::Align::Center);
    rule_row.append(&install);
    access_box.append(&rule_row);
    page.append(&access_card);

    // Profiles and recovery.
    let (recovery_card, recovery) = group(12);
    recovery.append(&label("Profiles and recovery", &["heading"]));
    let recovery_text = dim("");
    recovery.append(&recovery_text);
    let recovery_buttons = hbox(8);
    let undo = button("Undo");
    let redo = button("Redo");
    let show_folder = button("Show Folder");
    let reload = button("Reload Profiles");
    let restore = button("Restore Backup…");
    recovery_buttons.append(&undo);
    recovery_buttons.append(&redo);
    recovery_buttons.append(&spacer());
    recovery_buttons.append(&show_folder);
    recovery_buttons.append(&reload);
    recovery_buttons.append(&restore);
    recovery.append(&recovery_buttons);
    page.append(&recovery_card);

    // Service.
    let (service_card, service) = group(16);
    let (device_row, device_value) = status_line("Device");
    let (transport_row, transport_value) = status_line("Transport");
    let (battery_row, battery_value) = status_line("Battery");
    let (interception_row, interception_value) = status_line("Input interception");
    let (remap_row, remap_value) = status_line("Remapping");
    let (focus_row, focus_value) = status_line("App detection");
    let (last_row, last_value) = status_line("Last input");
    for row in [&device_row, &transport_row, &battery_row] {
        service.append(row);
    }
    service.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    for row in [&interception_row, &remap_row, &focus_row, &last_row] {
        service.append(row);
    }
    let macros_row = hbox(10);
    let macros_label = label("", &["dim-label"]);
    macros_row.append(&macros_label);
    let stop = button("Stop Macros");
    macros_row.append(&stop);
    macros_row.append(&spacer());
    let restart = button("Restart Input Service");
    macros_row.append(&restart);
    service.append(&macros_row);
    page.append(&service_card);

    // Background.
    let (background_card, background) = group(10);
    let background_row = hbox(14);
    let background_icon = gtk::Image::from_icon_name("opennaga-background-symbolic");
    background_icon.set_pixel_size(24);
    background_icon.add_css_class("naga-accent");
    background_icon.set_valign(gtk::Align::Start);
    background_row.append(&background_icon);
    let background_texts = vbox(6);
    background_texts.set_hexpand(true);
    background_texts.append(&label("Keeps running in the background", &["heading"]));
    let background_text = dim("");
    background_texts.append(&background_text);
    background_row.append(&background_texts);
    background.append(&background_row);
    let autostart_row = hbox(10);
    let autostart_label = label("Start OpenNaga when you sign in", &[]);
    autostart_label.set_hexpand(true);
    autostart_row.append(&autostart_label);
    let autostart = gtk::Switch::new();
    autostart.set_valign(gtk::Align::Center);
    autostart_row.append(&autostart);
    background.append(&autostart_row);
    page.append(&background_card);

    let message = wrapped("", &["dim-label"]);
    message.set_selectable(true);
    page.append(&message);
    page.append(&caption("The left and right main buttons keep their original function until you assign an action."));

    // Handlers.
    let a = app.clone();
    install.connect_clicked(move |b| {
        b.set_sensitive(false);
        let (sender, receiver) = async_channel::bounded::<Result<(), String>>(1);
        std::thread::spawn(move || {
            let _ = sender.send_blocking(access::install_rule());
        });
        let (a, b) = (a.clone(), b.clone());
        glib::spawn_future_local(async move {
            if let Ok(result) = receiver.recv().await {
                *a.ui_error.borrow_mut() = result.err();
                b.set_sensitive(true);
                // udev applies the new permissions a moment later.
                let again = a.clone();
                glib::timeout_add_local_once(std::time::Duration::from_millis(800), move || {
                    again.restart_input();
                    again.notify();
                });
                a.notify();
            }
        });
    });
    let a = app.clone();
    undo.connect_clicked(move |_| a.undo());
    let a = app.clone();
    redo.connect_clicked(move |_| a.redo());
    let a = app.clone();
    show_folder.connect_clicked(move |_| {
        let folder = a.config.borrow().profiles_folder();
        let _ = crate::model::store::create_private_dir(&folder);
        let _ = std::process::Command::new("xdg-open").arg(folder).spawn();
    });
    let a = app.clone();
    reload.connect_clicked(move |_| a.reload_profiles());
    let a = app.clone();
    restore.connect_clicked(move |_| super::dialogs::restore_backup(&a));
    let a = app.clone();
    stop.connect_clicked(move |_| a.cancel_macros());
    let a = app.clone();
    restart.connect_clicked(move |_| a.restart_input());
    let updating = Rc::new(Cell::new(false));
    let (a, guard) = (app.clone(), updating.clone());
    autostart.connect_state_set(move |_, on| {
        if !guard.get() {
            if let Err(error) = access::set_autostart(on) {
                *a.ui_error.borrow_mut() = Some(format!("Could not change the login item: {error}"));
            }
            a.notify();
        }
        glib::Propagation::Proceed
    });

    let refresh_app = app.clone();
    app.listen(move || {
        let app = &refresh_app;
        let status = app.engine_status.borrow().clone();
        let devices = app.devices.borrow().clone();
        let input_ok = status.naga_nodes > 0 && status.readable_nodes == status.naga_nodes;
        set_access(
            &input_icon,
            &input_state,
            input_ok,
            if status.naga_nodes == 0 {
                "No mouse found"
            } else if input_ok {
                "Granted"
            } else {
                "Not granted"
            },
        );
        let virtual_ok = status.running && status.virtual_error.is_none();
        set_access(&virtual_icon, &virtual_state, virtual_ok, if virtual_ok { "Granted" } else { "Not granted" });
        let hidraw_ok = status.hidraw_nodes > 0 && status.hidraw_readable == status.hidraw_nodes;
        set_access(
            &hidraw_icon,
            &hidraw_state,
            hidraw_ok,
            if status.hidraw_nodes == 0 {
                "No receiver found"
            } else if hidraw_ok {
                "Granted"
            } else {
                "Not granted"
            },
        );
        let keys_ok = status.keyboards_total > 0 && status.keyboards_readable == status.keyboards_total;
        set_access(&keys_icon, &keys_state, keys_ok, &format!("{} of {} keyboards", status.keyboards_readable, status.keyboards_total));
        let installed = app.rule_installed();
        rule_text.set_text(if installed {
            "The OpenNaga udev rule is installed. If access is still missing, reconnect the receiver or sign out and back in."
        } else {
            "The OpenNaga udev rule gives the signed-in user access to this mouse only, plus /dev/uinput for remapped output. Installing it asks for the administrator password."
        });
        install.set_label(if installed { "Reinstall Access Rule…" } else { "Install Access Rule…" });
        install.set_visible(!app.snapshot);

        let config = app.config.borrow();
        recovery_text.set_text(if config.profiles_read_only {
            "Editing and software remapping are paused to protect an unreadable file. Reload a repaired file or restore the previous save."
        } else {
            "Each successful change keeps the previous valid save. Undo and redo keep up to 12 changes during this session."
        });
        undo.set_sensitive(config.can_undo());
        redo.set_sensitive(config.can_redo());
        restore.set_sensitive(config.has_profile_backup());
        let onboard_only = config.current_profile_is_onboard_only();
        drop(config);

        device_value.set_text(&devices.name.clone().unwrap_or_else(|| "No mouse detected".into()));
        transport_value.set_text(&devices.transport.clone().unwrap_or_else(|| "Unavailable".into()));
        let battery = app.device.borrow().battery_level.or(devices.bluetooth_battery);
        battery_value.set_text(&battery.map(|b| format!("{b}%")).unwrap_or_else(|| "Unavailable".into()));
        interception_value.set_text(&if !status.running {
            "Unavailable".into()
        } else if status.suspended {
            "Paused while the computer sleeps".to_string()
        } else if status.grabbed_nodes > 0 {
            format!("Running, {} of {} mouse inputs captured", status.grabbed_nodes, status.naga_nodes)
        } else {
            "Running, observing only".into()
        });
        remap_value.set_text(if app.remapping_active() { "On" } else { "Off" });
        let backend = app.focus_backend.get();
        focus_value.set_text(&match backend {
            Backend::Unsupported => "Not available on this desktop. Per-app profiles need Hyprland, Sway, i3 or X11.".to_string(),
            other => match app.frontmost_name() {
                Some(name) => format!("{}: {name} is in front", other.title()),
                None => format!("{}: waiting for the first app", other.title()),
            },
        });
        last_value.set_text(&app.last_input.borrow());
        macros_label.set_text(&format!("Running macros: {}", app.macro_count.get()));
        stop.set_sensitive(app.macro_count.get() > 0);
        restart.set_sensitive(!app.device.borrow().is_busy && !app.snapshot);

        let onboard = app.onboard_active();
        background_text.set_text(&if onboard {
            if crate::hw::onboard::saved_name().is_none() {
                "The last save is incomplete. Restore the previous assignments from Mouse Memory.".to_string()
            } else {
                "The profile saved in the mouse keeps working after Quit. To go back to software remapping, restore the previous assignments from Mouse Memory.".to_string()
            }
        } else if onboard_only {
            "This profile was imported from the mouse. Edit it and use Save to Mouse to apply changes. Software remapping is paused because custom firmware assignments cannot identify physical controls reliably.".to_string()
        } else if app.has_tray() {
            "Close the window to keep OpenNaga in the tray. Your assignments keep working in other apps. To stop the service, turn off Remapping or choose Quit.".to_string()
        } else {
            "Close the window to keep OpenNaga running; open it again from your app launcher. Your assignments keep working in other apps. To stop the service, turn off Remapping or press Ctrl+Q.".to_string()
        });
        updating.set(true);
        let enabled = access::autostart_enabled();
        autostart.set_state(enabled);
        autostart.set_active(enabled);
        updating.set(false);
        autostart.set_sensitive(!app.snapshot);
        message.set_text(&app.device.borrow().status_message);
    });
    scrolled(&page).upcast()
}

// MARK: - App profiles

pub fn app_profiles(app: &Rc<App>) -> gtk::Widget {
    let page = padded(24, 24);
    page.append(&section_header(
        "App profiles",
        "Use a different profile while a given app is in front. OpenNaga chooses the profile on this computer, so it works over Bluetooth and nothing is written to the mouse.",
    ));
    let (main_card, main) = group(12);
    let toggle_row = hbox(10);
    let toggle_label = label("Switch profiles by app", &[]);
    toggle_label.set_hexpand(true);
    toggle_row.append(&toggle_label);
    let toggle = gtk::Switch::new();
    toggle.set_valign(gtk::Align::Center);
    toggle_row.append(&toggle);
    main.append(&toggle_row);
    let live_row = hbox(7);
    let live_dot = dot(false);
    let live_text = label("", &["dim-label", "caption"]);
    live_row.append(&live_dot);
    live_row.append(&live_text);
    main.append(&live_row);
    let paused = caption("Switching is paused while a profile saved in the mouse or imported from it is active.");
    main.append(&paused);
    let unsupported = warning("This desktop does not report the focused app. Per-app profiles need Hyprland, Sway, i3 or X11.");
    main.append(&unsupported);
    main.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    let others = hbox(8);
    let others_label = label("All other apps use", &[]);
    others_label.set_hexpand(true);
    others.append(&others_label);
    let default_name = label("", &["heading"]);
    others.append(&default_name);
    main.append(&others);
    main.append(&caption("Pick this profile in the tray menu or in the Buttons section. A profile you choose by hand applies until you switch to another app."));
    page.append(&main_card);

    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    page.append(&list);
    let add_row = hbox(8);
    let add = button("Add Application…");
    add_row.append(&add);
    page.append(&add_row);

    let a = app.clone();
    add.connect_clicked(move |_| super::dialogs::add_application(&a));
    let updating = Rc::new(Cell::new(false));
    let (a, guard) = (app.clone(), updating.clone());
    toggle.connect_state_set(move |_, on| {
        if !guard.get() {
            a.set_auto_switch(on);
        }
        glib::Propagation::Proceed
    });

    // Bindings and assignable profiles last rendered; the list rebuilds only on change.
    type Signature = (Vec<(String, String)>, Vec<String>);
    let signature: RefCell<Option<Signature>> = RefCell::new(None);
    let refresh_app = app.clone();
    app.listen(move || {
        let app = &refresh_app;
        let config = app.config.borrow();
        updating.set(true);
        toggle.set_state(config.auto_switch_enabled);
        toggle.set_active(config.auto_switch_enabled);
        updating.set(false);
        let blocked = app.onboard_active() || config.current_profile_is_onboard_only();
        paused.set_visible(blocked);
        unsupported.set_visible(app.focus_backend.get() == Backend::Unsupported && !app.snapshot);
        live_row.set_visible(!blocked && config.auto_switch_enabled);
        set_dot(&live_dot, app.remapping_active());
        live_text.set_text(&match app.frontmost_name() {
            None => "Waiting for the first app to come to the front".into(),
            Some(name) if config.runtime_profile_name == config.current_profile_name => format!("{name} is in front, using {}", config.runtime_profile_name),
            Some(name) => format!("{name} is in front, using {} (automatic)", config.runtime_profile_name),
        });
        default_name.set_text(&config.current_profile_name);
        list.set_opacity(if config.auto_switch_enabled { 1.0 } else { 0.6 });
        let bindings: Vec<(String, String)> = config.app_profiles.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let assignable = config.assignable_profiles();
        drop(config);
        let key = (bindings.clone(), assignable.clone());
        if signature.borrow().as_ref() == Some(&key) {
            return;
        }
        *signature.borrow_mut() = Some(key);
        while let Some(child) = list.first_child() {
            list.remove(&child);
        }
        if bindings.is_empty() {
            let empty = dim("No apps yet. Add an app to give it its own profile.");
            empty.set_margin_top(12);
            empty.set_margin_bottom(12);
            empty.set_margin_start(12);
            list.append(&empty);
        }
        let mut sorted = bindings;
        sorted.sort_by_key(|(id, _)| app.app_name(id).to_lowercase());
        for (app_id, profile) in sorted {
            let row = hbox(12);
            row.set_margin_top(10);
            row.set_margin_bottom(10);
            row.set_margin_start(12);
            row.set_margin_end(12);
            let icon = gtk::Image::from_icon_name(&app.app_icon(&app_id).unwrap_or_else(|| "opennaga-app-symbolic".into()));
            icon.set_pixel_size(32);
            row.append(&icon);
            let texts = vbox(2);
            texts.set_hexpand(true);
            texts.append(&label(&app.app_name(&app_id), &["heading"]));
            let id_label = label(&app_id, &["dim-label", "caption"]);
            id_label.set_selectable(true);
            texts.append(&id_label);
            row.append(&texts);
            let dropdown = string_dropdown(&assignable);
            dropdown.set_size_request(170, -1);
            dropdown.set_valign(gtk::Align::Center);
            if let Some(index) = assignable.iter().position(|p| *p == profile) {
                dropdown.set_selected(index as u32);
            }
            let (a, id) = (app.clone(), app_id.clone());
            dropdown.connect_selected_notify(move |d| {
                if let Some(name) = dropdown_text(d) {
                    if a.config.borrow().app_profiles.get(&id) != Some(&name) {
                        a.bind(&id, Some(&name));
                    }
                }
            });
            row.append(&dropdown);
            let remove = icon_button("opennaga-remove-symbolic", "Remove this app");
            remove.add_css_class("flat");
            remove.set_valign(gtk::Align::Center);
            let (a, id) = (app.clone(), app_id.clone());
            remove.connect_clicked(move |_| a.bind(&id, None));
            row.append(&remove);
            list.append(&row);
        }
    });
    scrolled(&page).upcast()
}
