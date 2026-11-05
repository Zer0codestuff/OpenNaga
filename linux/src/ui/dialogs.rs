//! Sheets and confirmations: Mouse Memory, Manage Profiles, import and
//! export, backup restore and Add Application.

use super::state::App;
use super::widgets::*;
use crate::desktop::{apps, focus};
use crate::hw::controller::Operation;
use crate::model::import::{ImportPolicy, ProfileImportDocument};
use adw::prelude::*;
use gtk::gio;
use std::cell::RefCell;
use std::rc::Rc;

fn parent(app: &Rc<App>) -> Option<gtk::Widget> {
    app.window.borrow().as_ref().map(|w| w.clone().upcast())
}

fn sheet(title: &str, width: i32) -> (adw::Dialog, gtk::Box) {
    let dialog = adw::Dialog::new();
    dialog.set_title(title);
    dialog.set_content_width(width);
    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());
    let body = padded(16, 24);
    body.set_margin_top(6);
    view.set_content(Some(&scrolled(&body)));
    dialog.set_child(Some(&view));
    (dialog, body)
}

fn alert(app: &Rc<App>, heading: &str, body: &str, responses: &[(&str, &str)], destructive: Option<&str>, done: impl Fn(&str) + 'static) {
    let dialog = adw::AlertDialog::new(Some(heading), Some(body));
    dialog.add_responses(responses);
    if let Some(id) = destructive {
        dialog.set_response_appearance(id, adw::ResponseAppearance::Destructive);
    }
    dialog.set_close_response("cancel");
    dialog.set_default_response(Some("cancel"));
    dialog.connect_response(None, move |_, response| done(response));
    dialog.present(parent(app).as_ref());
}

// MARK: - Mouse Memory

pub fn mouse_memory(app: &Rc<App>) {
    let (dialog, body) = sheet("Mouse Memory", 560);
    body.append(&dim("Save a profile to the Naga V2 HyperSpeed so it works even after you quit the app."));
    let profile_row = hbox(10);
    profile_row.append(&label("Profile to save", &[]));
    profile_row.append(&spacer());
    let profile_value = label("", &["heading"]);
    profile_row.append(&profile_value);
    body.append(&profile_row);
    let stored_row = hbox(10);
    stored_row.append(&label("In the mouse", &[]));
    stored_row.append(&spacer());
    let stored_value = label("", &["heading"]);
    stored_row.append(&stored_value);
    body.append(&stored_row);
    body.append(&dim("The mouse itself runs keys, shortcuts, media controls, clicks and the Hypershift layer. Buttons 4 and 5 stay standard mouse buttons. Editor changes and profile switches need a new save."));
    let issues = vbox(8);
    body.append(&issues);
    let driver_note = wrapped("Turn off or restore driver mode in Sensitivity before saving.", &[]);
    body.append(&driver_note);
    let connect_note = wrapped("Connect the USB receiver and switch the mouse to 2.4 GHz mode.", &[]);
    body.append(&connect_note);
    body.append(&caption("Saving keeps a backup and pauses software remapping. Key repeat and long presses follow the mouse's own behavior. Restoring needs the same receiver model."));
    body.append(&caption("Imported profiles keep software remapping paused. Edit their assignments here, then use Save to Mouse."));
    let status_row = hbox(10);
    let spinner = gtk::Spinner::new();
    status_row.append(&spinner);
    let status = wrapped("", &["dim-label"]);
    status.set_selectable(true);
    status_row.append(&status);
    body.append(&status_row);
    body.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    let row1 = hbox(8);
    let restore = button("Restore Previous Assignments");
    row1.append(&restore);
    row1.append(&spacer());
    let import = button("Import from Mouse");
    import.set_tooltip_text(Some("Create a profile for editing and saving to the mouse. Imported profiles keep software remapping paused."));
    row1.append(&import);
    let refresh = button("Refresh");
    row1.append(&refresh);
    body.append(&row1);
    let row2 = hbox(8);
    let save = button("Save to Mouse");
    save.add_css_class("suggested-action");
    row2.append(&save);
    row2.append(&spacer());
    let done = button("Done");
    row2.append(&done);
    body.append(&row2);

    let a = app.clone();
    restore.connect_clicked(move |_| {
        a.hardware(Operation::RestoreOnboard, None);
    });
    let a = app.clone();
    import.connect_clicked(move |_| {
        a.hardware(Operation::ImportOnboard, None);
    });
    let a = app.clone();
    refresh.connect_clicked(move |_| {
        a.hardware(Operation::Refresh, None);
    });
    let a = app.clone();
    save.connect_clicked(move |_| {
        let plan = a.onboard_plan();
        if plan.is_supported() {
            a.hardware(Operation::Onboard(Box::new(plan)), None);
        }
    });
    let d = dialog.clone();
    done.connect_clicked(move |_| {
        d.close();
    });

    let weak_dialog = dialog.downgrade();
    let refresh_app = app.clone();
    let update = move || {
        if weak_dialog.upgrade().is_none() {
            return;
        }
        let app = &refresh_app;
        let plan = app.onboard_plan();
        let device = app.device.borrow().clone();
        let read_only = app.config.borrow().profiles_read_only;
        let onboard = app.onboard_active();
        profile_value.set_text(&plan.name);
        stored_row.set_visible(onboard);
        stored_value.set_text(&crate::hw::onboard::saved_name().unwrap_or_else(|| "Restore needed".into()));
        clear(&issues);
        if !plan.issues.is_empty() {
            issues.append(&label("Fix before saving", &["heading"]));
            for issue in &plan.issues {
                issues.append(&wrapped(issue, &[]));
            }
        }
        issues.set_visible(!plan.issues.is_empty());
        driver_note.set_visible(device.driver_mode_enabled || device.recovery_pending);
        connect_note.set_visible(!device.is_connected);
        spinner.set_spinning(device.is_busy);
        spinner.set_visible(device.is_busy);
        status.set_text(&device.status_message);
        restore.set_sensitive(onboard && device.is_connected && !device.is_busy);
        import.set_sensitive(!read_only && device.is_connected && !device.is_busy);
        refresh.set_sensitive(!device.is_busy && !app.snapshot);
        save.set_sensitive(
            !read_only && plan.is_supported() && device.is_connected && !device.is_busy && !device.driver_mode_enabled && !device.recovery_pending,
        );
    };
    update();
    app.listen(update);
    dialog.present(parent(app).as_ref());
}

// MARK: - Manage profiles

pub fn manage_profiles(app: &Rc<App>) {
    let (dialog, body) = sheet("Manage Profiles", 520);
    let current_row = hbox(10);
    current_row.append(&label("Current profile", &[]));
    let current = gtk::DropDown::from_strings(&[]);
    current.set_hexpand(true);
    current_row.append(&current);
    body.append(&current_row);
    let name = gtk::Entry::new();
    name.set_placeholder_text(Some("Profile name"));
    body.append(&name);
    let actions = hbox(8);
    let create = button("Create Empty");
    let duplicate = button("Duplicate Current");
    let rename = button("Rename");
    actions.append(&create);
    actions.append(&duplicate);
    actions.append(&rename);
    body.append(&actions);
    body.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    let bottom = hbox(8);
    let delete = button("Delete Profile…");
    delete.add_css_class("destructive-action");
    bottom.append(&delete);
    bottom.append(&spacer());
    let done = button("Done");
    done.add_css_class("suggested-action");
    bottom.append(&done);
    body.append(&bottom);
    let error = wrapped("", &["naga-error"]);
    body.append(&error);
    let local_error: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

    let perform = {
        let (a, name, local_error) = (app.clone(), name.clone(), local_error.clone());
        Rc::new(move |op: &dyn Fn(&Rc<App>, &str) -> bool| {
            let text = name.text().to_string();
            if op(&a, &text) {
                name.set_text("");
                *local_error.borrow_mut() = None;
                a.after_config_change();
            } else {
                *local_error.borrow_mut() = Some(
                    a.config.borrow().last_error.clone().unwrap_or_else(|| "The operation failed. Choose a different, non-empty name.".into()),
                );
                a.notify();
            }
        })
    };
    let p = perform.clone();
    create.connect_clicked(move |_| p(&|a, n| a.config.borrow_mut().create_profile(n, None)));
    let p = perform.clone();
    duplicate.connect_clicked(move |_| {
        p(&|a, n| {
            let source = a.config.borrow().current_profile_name.clone();
            a.config.borrow_mut().duplicate_profile(&source, n)
        })
    });
    let p = perform.clone();
    rename.connect_clicked(move |_| {
        p(&|a, n| {
            let source = a.config.borrow().current_profile_name.clone();
            a.config.borrow_mut().rename_profile(&source, n)
        })
    });
    let a = app.clone();
    delete.connect_clicked(move |_| {
        let target = a.config.borrow().current_profile_name.clone();
        let app = a.clone();
        alert(
            &a,
            &format!("Delete {target}?"),
            "This profile and its app bindings will be removed. Buttons that switch to it will be disabled. Undo Profile Change can recover them during this session.",
            &[("cancel", "Cancel"), ("delete", "Delete")],
            Some("delete"),
            move |response| {
                if response == "delete" {
                    app.config.borrow_mut().delete_profile(&target);
                    app.after_config_change();
                }
            },
        );
    });
    let d = dialog.clone();
    done.connect_clicked(move |_| {
        d.close();
    });
    let a = app.clone();
    current.connect_selected_notify(move |d| {
        if let Some(n) = dropdown_text(d) {
            if n != a.config.borrow().current_profile_name {
                a.select_profile(&n);
            }
        }
    });
    let update_name = {
        let (create, duplicate, rename, name) = (create.clone(), duplicate.clone(), rename.clone(), name.clone());
        let a = app.clone();
        Rc::new(move || {
            let enabled = !name.text().trim().is_empty() && !a.config.borrow().profiles_read_only;
            create.set_sensitive(enabled);
            duplicate.set_sensitive(enabled);
            rename.set_sensitive(enabled);
        })
    };
    let u = update_name.clone();
    name.connect_changed(move |_| u());
    let weak_dialog = dialog.downgrade();
    let refresh_app = app.clone();
    let update = move || {
        if weak_dialog.upgrade().is_none() {
            return;
        }
        let config = refresh_app.config.borrow();
        let names = config.available_profiles();
        set_dropdown_items(&current, &names);
        if let Some(index) = names.iter().position(|n| *n == config.current_profile_name) {
            if current.selected() != index as u32 {
                current.set_selected(index as u32);
            }
        }
        current.set_sensitive(!config.profiles_read_only);
        delete.set_sensitive(!config.profiles_read_only && names.len() > 1);
        let message = local_error.borrow().clone().or_else(|| config.last_error.clone());
        error.set_visible(message.is_some());
        error.set_text(message.as_deref().unwrap_or(""));
        drop(config);
        update_name();
    };
    update();
    app.listen(update);
    dialog.present(parent(app).as_ref());
}

// MARK: - Import and export

fn json_filters() -> gio::ListStore {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("JSON profiles"));
    filter.add_suffix("json");
    filter.add_mime_type("application/json");
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    filters
}

pub fn import_profiles(app: &Rc<App>) {
    let dialog = gtk::FileDialog::new();
    dialog.set_title("Import Profiles");
    dialog.set_filters(Some(&json_filters()));
    let window = app.window.borrow().clone();
    let a = app.clone();
    dialog.open(window.as_ref(), gio::Cancellable::NONE, move |result| {
        let Some(path) = result.ok().and_then(|f| f.path()) else { return };
        let document = match ProfileImportDocument::open(&path) {
            Ok(document) => document,
            Err(error) => {
                *a.ui_error.borrow_mut() = Some(error.0);
                a.notify();
                return;
            }
        };
        let matches = document.matching_names(&a.config.borrow().profiles);
        let mut detail = "Existing profiles and your current selection are preserved by default.".to_string();
        if !matches.is_empty() {
            let shown: Vec<&str> = matches.iter().take(6).map(String::as_str).collect();
            detail.push_str(&format!("\n\nMatching names: {}{}.", shown.join(", "), if matches.len() > 6 { " and more" } else { "" }));
        }
        if document.shell_commands > 0 {
            detail.push_str(&format!(
                "\n\nContains {} shell {}. Review their actions before using the imported profiles. Commands run with your account's access when a mapped button is pressed.",
                document.shell_commands,
                if document.shell_commands == 1 { "command" } else { "commands" }
            ));
        }
        let mut responses = vec![("cancel", "Cancel"), ("keep", if matches.is_empty() { "Import" } else { "Keep Both" })];
        if !matches.is_empty() {
            responses.push(("replace", "Replace Matching…"));
        }
        let document = Rc::new(document);
        let app = a.clone();
        let count = document.file.profiles.len();
        alert(&a, &format!("Import {count} profiles?"), &detail, &responses, None, move |response| match response {
            "keep" => finish_import(&app, &document, ImportPolicy::KeepBoth),
            "replace" => {
                let (inner_app, inner_doc) = (app.clone(), document.clone());
                alert(
                    &app,
                    &format!("Replace {} matching profiles?", matches.len()),
                    "Their assignments will be replaced. Mouse-import protection is preserved. Undo Profile Change can restore the previous document.",
                    &[("cancel", "Cancel"), ("replace", "Replace")],
                    Some("replace"),
                    move |r| {
                        if r == "replace" {
                            finish_import(&inner_app, &inner_doc, ImportPolicy::ReplaceMatching);
                        }
                    },
                );
            }
            _ => {}
        });
    });
}

fn finish_import(app: &Rc<App>, document: &ProfileImportDocument, policy: ImportPolicy) {
    let result = app.config.borrow_mut().import_document(document, policy);
    *app.ui_error.borrow_mut() = result.err().map(|e| e.0);
    app.after_config_change();
}

pub fn export_profiles(app: &Rc<App>) {
    let dialog = gtk::FileDialog::new();
    dialog.set_title("Export Profiles");
    dialog.set_initial_name(Some("OpenNaga-profiles.json"));
    dialog.set_filters(Some(&json_filters()));
    let window = app.window.borrow().clone();
    let a = app.clone();
    dialog.save(window.as_ref(), gio::Cancellable::NONE, move |result| {
        let Some(path) = result.ok().and_then(|f| f.path()) else { return };
        let outcome = a.config.borrow().export_all(&path);
        *a.ui_error.borrow_mut() = outcome.err().map(|e| e.0);
        a.notify();
    });
}

pub fn restore_backup(app: &Rc<App>) {
    let a = app.clone();
    alert(
        app,
        "Restore the previous profile save?",
        "The current file will be preserved separately, including unreadable data. This restores local profiles only; mouse memory is unchanged.",
        &[("cancel", "Cancel"), ("restore", "Restore Backup")],
        Some("restore"),
        move |response| {
            if response == "restore" {
                a.restore_profile_backup();
            }
        },
    );
}

// MARK: - Add application

pub fn add_application(app: &Rc<App>) {
    let (dialog, body) = sheet("Add Application", 480);
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Search apps"));
    body.append(&search);
    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::Single);
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_child(Some(&list));
    scroll.set_min_content_height(280);
    scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
    body.append(&scroll);
    let manual_row = hbox(8);
    let manual = gtk::Entry::new();
    manual.set_placeholder_text(Some("Or type a window class, for example steam_app_570"));
    manual.set_hexpand(true);
    manual_row.append(&manual);
    body.append(&manual_row);
    body.append(&caption("The window class is what your compositor reports for the app (hyprctl clients, swaymsg -t get_tree or xprop). Running apps are marked in the list; start a game first to pick its own window."));
    let chosen_label = label("", &["dim-label"]);
    body.append(&chosen_label);
    let profile_row = hbox(10);
    profile_row.append(&label("Profile", &[]));
    let assignable = app.config.borrow().assignable_profiles();
    let profile = string_dropdown(&assignable);
    profile.set_hexpand(true);
    let current = app.config.borrow().current_profile_name.clone();
    if let Some(index) = assignable.iter().position(|p| *p == current) {
        profile.set_selected(index as u32);
    }
    profile_row.append(&profile);
    body.append(&profile_row);
    let buttons = hbox(8);
    buttons.append(&spacer());
    let cancel = button("Cancel");
    let add = button("Add");
    add.add_css_class("suggested-action");
    add.set_sensitive(false);
    buttons.append(&cancel);
    buttons.append(&add);
    body.append(&buttons);

    let running = focus::running_app_ids();
    let mut entries: Vec<apps::InstalledApp> = (*app.rescan_apps()).clone();
    for id in &running {
        if apps::find(&entries, id).is_none() {
            entries.push(apps::InstalledApp { app_id: id.clone(), name: id.clone(), desktop_id: String::new(), path: Default::default(), icon: None });
        }
    }
    let entries = Rc::new(apps::merged(entries));
    let bound = app.config.borrow().app_profiles.clone();
    for entry in entries.iter() {
        let row = hbox(10);
        row.set_margin_top(6);
        row.set_margin_bottom(6);
        row.set_margin_start(10);
        row.set_margin_end(10);
        let icon = gtk::Image::from_icon_name(entry.icon.as_deref().unwrap_or("opennaga-app-symbolic"));
        icon.set_pixel_size(24);
        row.append(&icon);
        let name = label(&entry.name, &[]);
        name.set_hexpand(true);
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        row.append(&name);
        if bound.keys().any(|k| k.eq_ignore_ascii_case(&entry.app_id)) {
            row.append(&label("Already added", &["dim-label", "caption"]));
        } else if running.iter().any(|r| r.eq_ignore_ascii_case(&entry.app_id)) {
            row.append(&label("Running", &["naga-accent", "caption"]));
        }
        row.set_tooltip_text(Some(&entry.app_id));
        list.append(&row);
    }
    if entries.is_empty() {
        list.append(&dim("No apps found"));
    }

    let chosen: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let refresh_add = {
        let (add, chosen, chosen_label, profile) = (add.clone(), chosen.clone(), chosen_label.clone(), profile.clone());
        let a = app.clone();
        Rc::new(move || {
            let id = chosen.borrow().clone();
            chosen_label.set_text(&id.as_ref().map(|i| format!("{} ({i})", a.app_name(i))).unwrap_or_default());
            add.set_sensitive(id.is_some() && dropdown_text(&profile).is_some());
        })
    };
    let (c, e, r, m) = (chosen.clone(), entries.clone(), refresh_add.clone(), manual.clone());
    list.connect_row_selected(move |_, row| {
        if let Some(entry) = row.and_then(|r| e.get(r.index() as usize)) {
            m.set_text("");
            *c.borrow_mut() = Some(entry.app_id.clone());
            r();
        }
    });
    let (c, r, l) = (chosen.clone(), refresh_add.clone(), list.clone());
    manual.connect_changed(move |m| {
        let text = m.text().trim().to_string();
        if !text.is_empty() {
            l.unselect_all();
            *c.borrow_mut() = Some(text);
        } else if l.selected_row().is_none() {
            *c.borrow_mut() = None;
        }
        r();
    });
    let (l, e) = (list.clone(), entries.clone());
    search.connect_search_changed(move |s| {
        let query = s.text().to_lowercase();
        let entries = e.clone();
        l.set_filter_func(move |row| {
            let Some(entry) = entries.get(row.index() as usize) else { return true };
            query.is_empty() || entry.name.to_lowercase().contains(&query) || entry.app_id.to_lowercase().contains(&query)
        });
    });
    let d = dialog.clone();
    cancel.connect_clicked(move |_| {
        d.close();
    });
    let (d, a, c) = (dialog.clone(), app.clone(), chosen.clone());
    add.connect_clicked(move |_| {
        if let (Some(id), Some(name)) = (c.borrow().clone(), dropdown_text(&profile)) {
            a.bind(&id, Some(&name));
        }
        d.close();
    });
    refresh_add();
    dialog.present(parent(app).as_ref());
}
