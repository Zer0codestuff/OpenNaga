//! The action inspector: edits one control in one layer of the selected
//! profile. Edits to keys, functions and toggles save immediately; text,
//! commands and macros save with their Apply button, like the macOS app.

use super::state::App;
use super::widgets::*;
use crate::desktop::system::{self, Environment};
use crate::model::actions::*;
use crate::model::keys::Mods;
use crate::model::layout::{KeyGroup, KeyboardKey};
use crate::model::mapper::SystemPlan;
use crate::model::validation;
use adw::prelude::*;
use gtk::{gdk, gio, glib};
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Original,
    Mouse,
    Keys,
    System,
    Application,
    Profile,
    Shell,
    Text,
    Macro,
    Hypershift,
    Disabled,
}

impl Kind {
    const ALL: [Kind; 11] =
        [Kind::Original, Kind::Mouse, Kind::Keys, Kind::System, Kind::Application, Kind::Profile, Kind::Shell, Kind::Text, Kind::Macro, Kind::Hypershift, Kind::Disabled];
    fn title(self, shifted: bool) -> &'static str {
        match self {
            Kind::Original if shifted => "Same as standard",
            Kind::Original => "Original function",
            Kind::Mouse => "Mouse action",
            Kind::Keys => "Keys",
            Kind::System => "System",
            Kind::Application => "Application",
            Kind::Profile => "Switch profile",
            Kind::Shell => "Shell command",
            Kind::Text => "Text",
            Kind::Macro => "Macro",
            Kind::Hypershift => "Hypershift key",
            Kind::Disabled => "Disabled",
        }
    }
}

const MODIFIERS: [(&str, &str, &str); 4] = [("ctrl", "Ctrl", "Control"), ("alt", "Alt", "Alt"), ("shift", "Shift", "Shift"), ("cmd", "Super", "Super (logo key)")];

#[derive(Clone)]
struct Draft {
    kind: Kind,
    text: String,
    description: String,
    keys: Vec<KeyStroke>,
    selected_stroke: usize,
    mouse: MouseAction,
    system: SystemAction,
    steps: Vec<MacroStep>,
    selected_step: usize,
    key_group: KeyGroup,
    validation: Option<String>,
}

impl Default for Draft {
    fn default() -> Self {
        Draft {
            kind: Kind::Original,
            text: String::new(),
            description: String::new(),
            keys: Vec::new(),
            selected_stroke: 0,
            mouse: MouseAction::BrowserBack,
            system: SystemAction::VolumeUp,
            steps: Vec::new(),
            selected_step: 0,
            key_group: KeyGroup::Letters,
            validation: None,
        }
    }
}

#[derive(Clone, PartialEq)]
struct Key {
    profile: String,
    button: i32,
    layer: MappingLayer,
    action: Option<ActionType>,
    read_only: bool,
    has_hypershift: bool,
    profiles: Vec<String>,
}

/// What Record Key is filling in.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Recording {
    Off,
    Stroke,
    MacroStep,
}

pub struct Inspector {
    pub root: gtk::Box,
    app: Weak<App>,
    draft: RefCell<Draft>,
    loaded: RefCell<Option<Key>>,
    recording: Cell<Recording>,
    capture: RefCell<Option<(gtk::Window, gtk::EventControllerKey)>>,
    rebuild_pending: Cell<bool>,
    result_label: gtk::Label,
    me: RefCell<Weak<Inspector>>,
}

impl Inspector {
    pub fn new(app: &Rc<App>) -> Rc<Inspector> {
        let root = padded(18, 24);
        let inspector = Rc::new(Inspector {
            root,
            app: Rc::downgrade(app),
            draft: RefCell::new(Draft::default()),
            loaded: RefCell::new(None),
            recording: Cell::new(Recording::Off),
            capture: RefCell::new(None),
            rebuild_pending: Cell::new(false),
            result_label: wrapped("", &["naga-small"]),
            me: RefCell::new(Weak::new()),
        });
        *inspector.me.borrow_mut() = Rc::downgrade(&inspector);
        inspector
    }

    fn app(&self) -> Rc<App> {
        self.app.upgrade().expect("app outlives the inspector")
    }

    fn me(&self) -> Rc<Inspector> {
        self.me.borrow().upgrade().expect("inspector")
    }

    fn key(&self) -> Key {
        let app = self.app();
        let ui = app.ui.borrow();
        let config = app.config.borrow();
        let standard = config.mapping_for_current(MappingLayer::Standard);
        let layer_map = config.mapping_for_current(ui.layer);
        Key {
            profile: config.current_profile_name.clone(),
            button: ui.selected,
            layer: ui.layer,
            action: layer_map.get(&ui.selected).cloned(),
            read_only: config.profiles_read_only,
            has_hypershift: standard.values().any(|a| a.is_hypershift()),
            profiles: config.assignable_profiles(),
        }
    }

    pub fn refresh(&self) {
        let key = self.key();
        if self.loaded.borrow().as_ref() != Some(&key) {
            let control_changed = self.loaded.borrow().as_ref().map(|k| (k.button, k.layer, &k.profile) != (key.button, key.layer, &key.profile)).unwrap_or(true);
            if control_changed {
                self.stop_recording();
            }
            if self.recording.get() == Recording::Off || control_changed {
                self.load(&key);
            }
            *self.loaded.borrow_mut() = Some(key);
            self.schedule_rebuild();
        }
        let app = self.app();
        let result = app.system_result.borrow().clone();
        self.result_label.set_visible(result.is_some());
        self.result_label.set_text(result.as_deref().unwrap_or(""));
    }

    fn schedule_rebuild(&self) {
        if self.rebuild_pending.replace(true) {
            return;
        }
        let me = self.me();
        glib::idle_add_local_once(move || {
            me.rebuild_pending.set(false);
            me.rebuild();
        });
    }

    fn load(&self, key: &Key) {
        let mut d = Draft { key_group: self.draft.borrow().key_group, ..Draft::default() };
        match &key.action {
            None => d.kind = Kind::Original,
            Some(ActionType::Hypershift { description }) => {
                d.kind = Kind::Hypershift;
                d.description = description.clone().unwrap_or_default();
            }
            Some(ActionType::Audio { action, description }) => {
                d.kind = Kind::System;
                d.system = action.system();
                d.description = description.clone().unwrap_or_default();
            }
            Some(ActionType::System { action, description }) => {
                d.kind = Kind::System;
                d.system = *action;
                d.description = description.clone().unwrap_or_default();
            }
            Some(ActionType::Disabled) => d.kind = Kind::Disabled,
            Some(ActionType::Mouse { action, description }) => {
                d.kind = Kind::Mouse;
                d.mouse = *action;
                d.description = description.clone().unwrap_or_default();
            }
            Some(ActionType::KeySequence { keys, description }) => {
                d.kind = Kind::Keys;
                d.keys = keys.clone();
                d.description = description.clone().unwrap_or_default();
                d.selected_stroke = self.draft.borrow().selected_stroke.min(keys.len().saturating_sub(1));
            }
            Some(ActionType::TextSnippet { text, description }) => {
                d.kind = Kind::Text;
                d.text = text.clone();
                d.description = description.clone().unwrap_or_default();
            }
            Some(ActionType::Application { path, description }) => {
                d.kind = Kind::Application;
                d.text = path.clone();
                d.description = description.clone().unwrap_or_default();
            }
            Some(ActionType::ProfileSwitch { profile, description }) => {
                d.kind = Kind::Profile;
                d.text = profile.clone();
                d.description = description.clone().unwrap_or_default();
            }
            Some(ActionType::SystemCommand { command, description }) => {
                d.kind = Kind::Shell;
                d.text = command.clone();
                d.description = description.clone().unwrap_or_default();
            }
            Some(ActionType::Macro { steps, description }) => {
                d.kind = Kind::Macro;
                d.steps = steps.clone();
                d.description = description.clone().unwrap_or_default();
            }
        }
        if let Some(code) = d.keys.get(d.selected_stroke).and_then(|k| k.key_code) {
            if let Some(entry) = self.app().layout.borrow().catalog().into_iter().find(|k| k.code == code) {
                d.key_group = entry.group;
            }
        }
        *self.draft.borrow_mut() = d;
    }

    // MARK: - Saving

    fn persist(&self) {
        let (button, layer) = {
            let app = self.app();
            let ui = app.ui.borrow();
            (ui.selected, ui.layer)
        };
        let action = {
            let mut d = self.draft.borrow_mut();
            d.validation = None;
            let label = if d.description.trim().is_empty() { None } else { Some(d.description.clone()) };
            match d.kind {
                Kind::Original => None,
                Kind::System => Some(ActionType::System { action: d.system, description: label }),
                Kind::Disabled => Some(ActionType::Disabled),
                Kind::Mouse => Some(ActionType::Mouse { action: d.mouse, description: label }),
                Kind::Keys => {
                    if d.keys.is_empty() {
                        return;
                    }
                    Some(ActionType::KeySequence { keys: d.keys.clone(), description: label })
                }
                Kind::Application => Some(ActionType::Application { path: d.text.clone(), description: label }),
                Kind::Profile => Some(ActionType::ProfileSwitch { profile: d.text.clone(), description: label }),
                Kind::Hypershift => {
                    if layer == MappingLayer::Hypershift {
                        return;
                    }
                    Some(ActionType::Hypershift { description: label })
                }
                Kind::Shell => Some(ActionType::SystemCommand { command: d.text.clone(), description: label }),
                Kind::Text => Some(ActionType::TextSnippet { text: d.text.clone(), description: label }),
                Kind::Macro => match validation::validate_macro(&d.steps) {
                    Ok(()) => Some(ActionType::Macro { steps: d.steps.clone(), description: label }),
                    Err(error) => {
                        d.validation = Some(error.0);
                        drop(d);
                        self.schedule_rebuild();
                        return;
                    }
                },
            }
        };
        // Validate like the file store does, so the message lands here.
        if let Some(action) = &action {
            if let Err(error) = validation::validate_action(&crate::model::profile::ButtonAction::from_action(action), layer) {
                self.draft.borrow_mut().validation = Some(error.0);
                self.schedule_rebuild();
                return;
            }
        }
        self.app().save_action(button, action, layer);
        self.schedule_rebuild();
    }

    fn change_kind(&self, kind: Kind) {
        let (current, sequence) = {
            let d = self.draft.borrow();
            (d.kind, d.kind == Kind::Macro || d.keys.len() > 1)
        };
        if kind == current {
            return;
        }
        if sequence {
            let me = self.me();
            let dialog = adw::AlertDialog::new(Some("Replace the existing action?"), Some("The macro or sequence will be replaced in the current profile."));
            dialog.add_responses(&[("cancel", "Cancel"), ("replace", "Replace")]);
            dialog.set_response_appearance("replace", adw::ResponseAppearance::Destructive);
            dialog.set_default_response(Some("cancel"));
            dialog.connect_response(None, move |_, response| {
                if response == "replace" {
                    me.apply_kind(kind);
                } else {
                    me.schedule_rebuild();
                }
            });
            dialog.present(self.root.root().as_ref());
            return;
        }
        self.apply_kind(kind);
    }

    fn apply_kind(&self, kind: Kind) {
        self.stop_recording();
        {
            let mut d = self.draft.borrow_mut();
            let group = d.key_group;
            *d = Draft { kind, key_group: group, ..Draft::default() };
        }
        if matches!(kind, Kind::Original | Kind::Disabled | Kind::Mouse | Kind::System | Kind::Hypershift) {
            self.persist();
        } else {
            self.schedule_rebuild();
        }
    }

    // MARK: - Recording

    fn start_recording(&self, target: Recording) {
        self.stop_recording();
        let Some(window) = self.root.root().and_then(|r| r.downcast::<gtk::Window>().ok()) else { return };
        let controller = gtk::EventControllerKey::new();
        controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        let me = Rc::downgrade(&self.me());
        controller.connect_key_pressed(move |_, _keyval, keycode, state| {
            let Some(me) = me.upgrade() else { return glib::Propagation::Proceed };
            let evdev = keycode.saturating_sub(8) as u16;
            if !Mods::for_key(evdev).is_empty() || evdev == 0 || evdev == 58 {
                return glib::Propagation::Stop; // Wait for the key itself.
            }
            let Some(code) = crate::model::keys::evdev_to_mac(evdev) else {
                me.draft.borrow_mut().validation = Some("This key has no supported equivalent. Choose it from the list instead.".into());
                me.stop_recording();
                me.schedule_rebuild();
                return glib::Propagation::Stop;
            };
            let mut names = Vec::new();
            if state.contains(gdk::ModifierType::CONTROL_MASK) {
                names.push("ctrl".to_string());
            }
            if state.contains(gdk::ModifierType::ALT_MASK) {
                names.push("alt".to_string());
            }
            if state.contains(gdk::ModifierType::SHIFT_MASK) {
                names.push("shift".to_string());
            }
            if state.intersects(gdk::ModifierType::SUPER_MASK | gdk::ModifierType::META_MASK) {
                names.push("cmd".to_string());
            }
            let stroke = me.app().layout.borrow().captured_stroke(code, names);
            let target = me.recording.get();
            me.stop_recording();
            match target {
                Recording::Stroke => me.replace_stroke(stroke),
                Recording::MacroStep => {
                    let mut d = me.draft.borrow_mut();
                    let index = d.selected_step;
                    if let Some(step) = d.steps.get_mut(index) {
                        step.key_stroke = Some(stroke);
                    }
                    drop(d);
                    me.schedule_rebuild();
                }
                Recording::Off => {}
            }
            glib::Propagation::Stop
        });
        window.add_controller(controller.clone());
        let me = Rc::downgrade(&self.me());
        window.connect_is_active_notify(move |w| {
            if !w.is_active() {
                if let Some(me) = me.upgrade() {
                    if me.recording.get() != Recording::Off {
                        me.stop_recording();
                        me.schedule_rebuild();
                    }
                }
            }
        });
        *self.capture.borrow_mut() = Some((window, controller));
        self.recording.set(target);
    }

    fn stop_recording(&self) {
        self.recording.set(Recording::Off);
        if let Some((window, controller)) = self.capture.borrow_mut().take() {
            window.remove_controller(&controller);
        }
    }

    fn replace_stroke(&self, stroke: KeyStroke) {
        {
            let mut d = self.draft.borrow_mut();
            let index = d.selected_stroke;
            if index < d.keys.len() {
                d.keys[index] = stroke;
            } else {
                d.keys.push(stroke);
                d.selected_stroke = d.keys.len() - 1;
            }
        }
        self.persist();
    }

    // MARK: - Building

    fn rebuild(&self) {
        clear(&self.root);
        let app = self.app();
        let key = self.loaded.borrow().clone().unwrap_or_else(|| self.key());
        let shifted = key.layer == MappingLayer::Hypershift;
        let standard = app.current_mapping(MappingLayer::Standard);
        let is_hypershift_key = standard.get(&key.button).map(|a| a.is_hypershift()).unwrap_or(false);
        self.root.set_sensitive(!key.read_only);

        let header = hbox(12);
        let badge = gtk::Label::new(Some(&key.button.to_string()));
        badge.add_css_class("naga-inspector-badge");
        header.append(&badge);
        let titles = vbox(4);
        titles.set_valign(gtk::Align::Center);
        titles.append(&label(&button_name(key.button), &["naga-inspector-title"]));
        titles.append(&label(&if shifted { format!("{}, Hypershift layer", key.profile) } else { key.profile.clone() }, &["dim-label", "caption"]));
        header.append(&titles);
        self.root.append(&header);

        let summary = if shifted {
            super::buttons::summary(&standard, &app.current_mapping(MappingLayer::Hypershift), key.button, key.layer)
        } else {
            key.action.as_ref().map(|a| a.display_name()).unwrap_or_else(|| "Original function".into())
        };
        let current = info("opennaga-check-symbolic", &summary);
        current.add_css_class("dim-label");
        self.root.append(&current);

        if key.button >= 18 && !(shifted && is_hypershift_key) {
            self.root.append(&warning("Keep a primary click available so you can still use the computer."));
        }

        if shifted && is_hypershift_key {
            self.root.append(&info("opennaga-shift-symbolic", "This control is the Hypershift key."));
            self.root.append(&dim("Hold it to use the Hypershift layer. To change its own action, switch to the Standard layer."));
            let edit = button("Edit Standard Layer");
            edit.set_halign(gtk::Align::Start);
            let a = app.clone();
            edit.connect_clicked(move |_| {
                a.ui.borrow_mut().layer = MappingLayer::Standard;
                a.notify();
            });
            self.root.append(&edit);
        } else {
            if shifted && !key.has_hypershift {
                self.root.append(&warning("No Hypershift key yet. In the Standard layer, set a control's action to Hypershift key."));
            }
            self.build_kind_picker(shifted);
            self.root.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
            self.build_editor(&app, &key, shifted);
            let kind = self.draft.borrow().kind;
            if kind != Kind::Original && kind != Kind::Disabled {
                self.build_custom_name();
            }
        }
        if let Some(message) = self.draft.borrow().validation.clone() {
            self.root.append(&wrapped(&message, &["naga-error"]));
        }
        self.root.append(&self.result_label);
    }

    fn build_kind_picker(&self, shifted: bool) {
        let kinds: Vec<Kind> = Kind::ALL.iter().copied().filter(|k| !shifted || *k != Kind::Hypershift).collect();
        let titles: Vec<String> = kinds.iter().map(|k| k.title(shifted).to_string()).collect();
        let row = hbox(10);
        row.append(&label("Action", &[]));
        let dropdown = string_dropdown(&titles);
        dropdown.set_hexpand(true);
        let current = self.draft.borrow().kind;
        dropdown.set_selected(kinds.iter().position(|k| *k == current).unwrap_or(0) as u32);
        let me = Rc::downgrade(&self.me());
        dropdown.connect_selected_notify(move |d| {
            if let (Some(me), Some(kind)) = (me.upgrade(), kinds.get(d.selected() as usize)) {
                me.change_kind(*kind);
            }
        });
        row.append(&dropdown);
        self.root.append(&row);
    }

    fn build_custom_name(&self) {
        let expander = gtk::Expander::new(Some("Custom name"));
        let content = vbox(6);
        content.set_margin_top(8);
        let entry = gtk::Entry::new();
        entry.set_placeholder_text(Some("Optional name"));
        entry.set_text(&self.draft.borrow().description);
        let me = Rc::downgrade(&self.me());
        entry.connect_activate(move |e| {
            if let Some(me) = me.upgrade() {
                me.draft.borrow_mut().description = e.text().to_string();
                me.persist();
            }
        });
        content.append(&entry);
        content.append(&caption("Press Enter to save the name."));
        expander.set_child(Some(&content));
        expander.set_expanded(!self.draft.borrow().description.is_empty());
        self.root.append(&expander);
    }

    fn build_editor(&self, app: &Rc<App>, key: &Key, shifted: bool) {
        let kind = self.draft.borrow().kind;
        let root = &self.root;
        match kind {
            Kind::Original => {
                if shifted {
                    let standard = app.current_mapping(MappingLayer::Standard);
                    root.append(&info("opennaga-original-symbolic", "Same as standard"));
                    root.append(&dim(&format!(
                        "While Hypershift is held, this control keeps its Standard layer action: {}.",
                        standard.get(&key.button).map(|a| a.display_name()).unwrap_or_else(|| "Original function".into())
                    )));
                } else {
                    root.append(&info("opennaga-original-symbolic", "No remapping"));
                    root.append(&dim("The button keeps its default function. Choose Keys to assign a keyboard key."));
                }
            }
            Kind::Hypershift => {
                root.append(&info("opennaga-shift-symbolic", "Hypershift key"));
                root.append(&dim("While you hold this control, the other controls use their Hypershift layer actions. Controls without one keep their standard action. Pick a control you can hold while pressing the others, for example a side button held with your thumb while you click or tilt the wheel. Save to Mouse stores both layers, so Hypershift also works with the app closed."));
                let edit = button("Edit Hypershift Layer");
                edit.set_halign(gtk::Align::Start);
                let a = app.clone();
                edit.connect_clicked(move |_| {
                    a.ui.borrow_mut().layer = MappingLayer::Hypershift;
                    a.notify();
                });
                root.append(&edit);
            }
            Kind::Disabled => {
                root.append(&info("opennaga-disabled-symbolic", "Button disabled"));
                root.append(&dim("The signal is blocked only while remapping is on."));
            }
            Kind::Mouse => self.build_mouse(app),
            Kind::System => self.build_system(app),
            Kind::Keys => self.build_keys(app),
            Kind::Application => self.build_application(),
            Kind::Profile => self.build_profile(key),
            Kind::Shell => {
                root.append(&wrapped("The command runs with /bin/sh when you press the button. Only use commands you trust.", &["dim-label"]));
                let (scroll, view) = text_view(&self.draft.borrow().text, 100);
                root.append(&scroll);
                let apply = button("Apply Command");
                apply.set_halign(gtk::Align::Start);
                let me = Rc::downgrade(&self.me());
                apply.connect_clicked(move |_| {
                    if let Some(me) = me.upgrade() {
                        me.draft.borrow_mut().text = text_of(&view);
                        me.persist();
                    }
                });
                root.append(&apply);
            }
            Kind::Text => {
                let (scroll, view) = text_view(&self.draft.borrow().text, 120);
                root.append(&scroll);
                root.append(&caption("Types text without changing the clipboard. Characters your keyboard layout cannot type use wtype or xdotool when installed. Press again to restart; Stop Macros interrupts it."));
                let apply = button("Apply Text");
                apply.set_halign(gtk::Align::Start);
                let me = Rc::downgrade(&self.me());
                apply.connect_clicked(move |_| {
                    if let Some(me) = me.upgrade() {
                        me.draft.borrow_mut().text = text_of(&view);
                        me.persist();
                    }
                });
                root.append(&apply);
            }
            Kind::Macro => self.build_macro(app),
        }
    }

    fn build_mouse(&self, app: &Rc<App>) {
        let row = hbox(8);
        for (title, action) in [("Back", MouseAction::BrowserBack), ("Forward", MouseAction::BrowserForward)] {
            let b = button(title);
            let me = Rc::downgrade(&self.me());
            b.connect_clicked(move |_| {
                if let Some(me) = me.upgrade() {
                    me.draft.borrow_mut().mouse = action;
                    me.persist();
                }
            });
            row.append(&b);
        }
        self.root.append(&row);
        self.root.append(&caption("Recommended for browser navigation. Sends Alt+← and Alt+→."));
        let titles: Vec<String> = MouseAction::ALL.iter().map(|m| m.title().to_string()).collect();
        let dropdown = string_dropdown(&titles);
        let current = self.draft.borrow().mouse;
        dropdown.set_selected(MouseAction::ALL.iter().position(|m| *m == current).unwrap_or(0) as u32);
        let me = Rc::downgrade(&self.me());
        dropdown.connect_selected_notify(move |d| {
            if let Some(me) = me.upgrade() {
                let action = MouseAction::ALL[d.selected() as usize];
                if me.draft.borrow().mouse != action {
                    me.draft.borrow_mut().mouse = action;
                    me.persist();
                }
            }
        });
        let function = hbox(10);
        function.append(&label("Function", &[]));
        dropdown.set_hexpand(true);
        function.append(&dropdown);
        self.root.append(&function);
        let help = if current.is_hardware_only() {
            "Hardware function: choose Save to Mouse to apply it. It changes the DPI stage inside the mouse, even when the app is closed."
        } else if app.onboard_active() {
            "Mouse buttons 4 and 5 stay standard clicks. Browsers treat them as Back and Forward."
        } else {
            "Mouse buttons 4 and 5 send real clicks. Linux browsers and file managers use them for Back and Forward."
        };
        self.root.append(&dim(help));
    }

    fn build_system(&self, app: &Rc<App>) {
        let system = self.draft.borrow().system;
        let groups: Vec<String> = SystemActionGroup::ALL.iter().map(|g| g.title().to_string()).collect();
        let category = string_dropdown(&groups);
        category.set_selected(SystemActionGroup::ALL.iter().position(|g| *g == system.group()).unwrap_or(0) as u32);
        let me = Rc::downgrade(&self.me());
        category.connect_selected_notify(move |d| {
            let Some(me) = me.upgrade() else { return };
            let group = SystemActionGroup::ALL[d.selected() as usize];
            if me.draft.borrow().system.group() == group {
                return;
            }
            if let Some(first) = group.actions().first() {
                me.draft.borrow_mut().system = *first;
                me.persist();
            }
        });
        let row = hbox(10);
        row.append(&label("Category", &[]));
        category.set_hexpand(true);
        row.append(&category);
        self.root.append(&row);
        self.root.append(&caption("Function"));
        let actions = system.group().actions();
        let list = gtk::ListBox::new();
        list.add_css_class("boxed-list");
        list.set_selection_mode(gtk::SelectionMode::Single);
        for action in &actions {
            let row = hbox(10);
            row.set_margin_top(8);
            row.set_margin_bottom(8);
            row.set_margin_start(10);
            // Theme icons only when the theme has them; never a placeholder.
            if gtk::IconTheme::for_display(&row.display()).has_icon(action.icon()) {
                row.append(&gtk::Image::from_icon_name(action.icon()));
            }
            row.append(&label(action.title(), &[]));
            list.append(&row);
        }
        if let Some(index) = actions.iter().position(|a| *a == system) {
            list.select_row(list.row_at_index(index as i32).as_ref());
        }
        let me = Rc::downgrade(&self.me());
        list.connect_row_activated(move |_, row| {
            if let Some(me) = me.upgrade() {
                if let Some(action) = actions.get(row.index() as usize) {
                    me.draft.borrow_mut().system = *action;
                    me.persist();
                }
            }
        });
        self.root.append(&list);
        self.root.append(&dim(system::help(system)));
        let env = Environment::shared();
        let plan = env.plan(system);
        let description = env.describe(system);
        if matches!(plan, SystemPlan::Unavailable(_)) {
            self.root.append(&warning(&description));
        } else {
            self.root.append(&caption(&description));
        }
        let test = labelled_icon_button("opennaga-play-symbolic", "Test");
        test.set_halign(gtk::Align::Start);
        test.set_tooltip_text(Some("Run the selected system function now"));
        test.set_sensitive(!matches!(plan, SystemPlan::Unavailable(_)) && (!matches!(plan, SystemPlan::Key { .. }) || app.engine_status.borrow().virtual_error.is_none()));
        let a = app.clone();
        test.connect_clicked(move |_| a.test_system(system));
        self.root.append(&test);
    }

    fn build_keys(&self, app: &Rc<App>) {
        let (keys, selected) = {
            let d = self.draft.borrow();
            (d.keys.clone(), d.selected_stroke)
        };
        if keys.len() > 1 {
            let titles: Vec<String> = keys.iter().enumerate().map(|(i, k)| format!("{}. {}", i + 1, k.formatted_shortcut())).collect();
            let steps = string_dropdown(&titles);
            steps.set_selected(selected.min(keys.len() - 1) as u32);
            let me = Rc::downgrade(&self.me());
            steps.connect_selected_notify(move |d| {
                if let Some(me) = me.upgrade() {
                    me.draft.borrow_mut().selected_stroke = d.selected() as usize;
                    me.schedule_rebuild();
                }
            });
            let row = hbox(10);
            row.append(&label("Step", &[]));
            steps.set_hexpand(true);
            row.append(&steps);
            self.root.append(&row);
        }
        let current = keys.get(selected).cloned();
        let display = vbox(6);
        display.add_css_class("naga-keydisplay");
        display.set_valign(gtk::Align::Center);
        let text = gtk::Label::new(Some(&current.as_ref().map(|k| k.formatted_shortcut()).unwrap_or_else(|| "Choose a key".into())));
        text.add_css_class(if current.is_some() { "naga-keydisplay-big" } else { "naga-keydisplay-empty" });
        if current.is_none() {
            text.add_css_class("dim-label");
        }
        text.set_vexpand(true);
        text.set_valign(gtk::Align::End);
        display.append(&text);
        if keys.len() <= 1 {
            let hint = gtk::Label::new(Some("Held down while you press the mouse button"));
            hint.add_css_class("dim-label");
            hint.add_css_class("naga-tiny");
            hint.set_vexpand(true);
            hint.set_valign(gtk::Align::Start);
            display.append(&hint);
        }
        self.root.append(&display);

        let me = Rc::downgrade(&self.me());
        let modifiers = current.as_ref().map(|k| k.modifiers.clone()).unwrap_or_default();
        self.root.append(&key_selector(app, current.as_ref().and_then(|k| k.key_code), self.draft.borrow().key_group, {
            let me = me.clone();
            move |group| {
                if let Some(me) = me.upgrade() {
                    me.draft.borrow_mut().key_group = group;
                    me.schedule_rebuild();
                }
            }
        }, {
            let me = me.clone();
            move |entry: KeyboardKey| {
                if let Some(me) = me.upgrade() {
                    me.replace_stroke(entry.stroke(&modifiers));
                }
            }
        }));

        self.root.append(&caption("Combine with"));
        let toggles = hbox(6);
        toggles.set_homogeneous(true);
        let normalized = current.as_ref().map(|k| k.normalized_modifiers()).unwrap_or_default();
        for (name, title, tip) in MODIFIERS {
            let toggle = gtk::ToggleButton::with_label(title);
            toggle.add_css_class("naga-modifier");
            toggle.set_active(normalized.iter().any(|m| m == name));
            toggle.set_tooltip_text(Some(tip));
            toggle.set_sensitive(current.is_some());
            let me = me.clone();
            toggle.connect_toggled(move |t| {
                let Some(me) = me.upgrade() else { return };
                {
                    let mut d = me.draft.borrow_mut();
                    let index = d.selected_stroke;
                    let Some(stroke) = d.keys.get_mut(index) else { return };
                    let mut mods = stroke.normalized_modifiers();
                    mods.retain(|m| m != name);
                    if t.is_active() {
                        mods.push(name.to_string());
                    }
                    stroke.modifiers = mods;
                }
                me.persist();
            });
            toggles.append(&toggle);
        }
        self.root.append(&toggles);
        self.root.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        let recording = self.recording.get() == Recording::Stroke;
        let record = button(if recording { "Cancel Recording" } else { "Record Key" });
        if recording {
            record.add_css_class("suggested-action");
        }
        let me2 = me.clone();
        record.connect_clicked(move |_| {
            if let Some(me) = me2.upgrade() {
                if me.recording.get() == Recording::Stroke {
                    me.stop_recording();
                } else {
                    me.start_recording(Recording::Stroke);
                }
                me.schedule_rebuild();
            }
        });
        self.root.append(&record);
        self.root.append(&caption(if recording {
            "Press a key or combination. Esc is assigned as a key."
        } else {
            "You can also press the key on your keyboard with Record Key."
        }));

        let expander = gtk::Expander::new(Some("Key sequence"));
        let content = vbox(10);
        content.set_margin_top(8);
        content.append(&caption("Multiple steps run in order when you press the button."));
        let buttons = hbox(8);
        let add = button("Add Step");
        let me3 = me.clone();
        add.connect_clicked(move |_| {
            if let Some(me) = me3.upgrade() {
                {
                    let mut d = me.draft.borrow_mut();
                    d.keys.push(KeyStroke::new("tab", &[], Some(crate::model::keys::mac::TAB)));
                    d.selected_stroke = d.keys.len() - 1;
                }
                me.persist();
            }
        });
        buttons.append(&add);
        if keys.len() > 1 {
            let remove = button("Remove");
            let me4 = me.clone();
            remove.connect_clicked(move |_| {
                if let Some(me) = me4.upgrade() {
                    {
                        let mut d = me.draft.borrow_mut();
                        let index = d.selected_stroke.min(d.keys.len() - 1);
                        d.keys.remove(index);
                        d.selected_stroke = index.saturating_sub(1);
                    }
                    me.persist();
                }
            });
            buttons.append(&remove);
        }
        content.append(&buttons);
        expander.set_child(Some(&content));
        expander.set_expanded(keys.len() > 1);
        self.root.append(&expander);
    }

    fn build_application(&self) {
        let entry = gtk::Entry::new();
        entry.set_placeholder_text(Some("Desktop entry, program or file"));
        entry.set_text(&self.draft.borrow().text);
        let me = Rc::downgrade(&self.me());
        entry.connect_activate(move |e| {
            if let Some(me) = me.upgrade() {
                me.draft.borrow_mut().text = e.text().to_string();
                me.persist();
            }
        });
        self.root.append(&entry);
        let choose = button("Choose Application…");
        choose.set_halign(gtk::Align::Start);
        let me = Rc::downgrade(&self.me());
        choose.connect_clicked(move |b| {
            let Some(me) = me.upgrade() else { return };
            let dialog = gtk::FileDialog::new();
            dialog.set_title("Choose Application");
            dialog.set_initial_folder(Some(&gio::File::for_path("/usr/share/applications")));
            let filter = gtk::FileFilter::new();
            filter.set_name(Some("Applications"));
            filter.add_suffix("desktop");
            filter.add_mime_type("application/x-executable");
            filter.add_mime_type("application/x-sharedlib");
            filter.add_mime_type("text/x-shellscript");
            let any = gtk::FileFilter::new();
            any.set_name(Some("All files"));
            any.add_pattern("*");
            let filters = gio::ListStore::new::<gtk::FileFilter>();
            filters.append(&filter);
            filters.append(&any);
            dialog.set_filters(Some(&filters));
            let window = b.root().and_then(|r| r.downcast::<gtk::Window>().ok());
            dialog.open(window.as_ref(), gio::Cancellable::NONE, move |result| {
                if let Some(path) = result.ok().and_then(|f| f.path()) {
                    me.draft.borrow_mut().text = path.to_string_lossy().to_string();
                    me.persist();
                }
            });
        });
        self.root.append(&choose);
        self.root.append(&caption("Desktop entries start like they do from your launcher. Programs run directly; other files open with their default app. Press Enter to save a typed path."));
    }

    fn build_profile(&self, key: &Key) {
        let target = self.draft.borrow().text.clone();
        let mut items = vec!["Choose a profile".to_string()];
        items.extend(key.profiles.iter().cloned());
        let dropdown = string_dropdown(&items);
        dropdown.set_selected(key.profiles.iter().position(|p| *p == target).map(|i| i + 1).unwrap_or(0) as u32);
        let me = Rc::downgrade(&self.me());
        dropdown.connect_selected_notify(move |d| {
            let Some(me) = me.upgrade() else { return };
            if d.selected() == 0 {
                return;
            }
            if let Some(name) = dropdown_text(d) {
                me.draft.borrow_mut().text = name;
                me.persist();
            }
        });
        let row = hbox(10);
        row.append(&label("Target profile", &[]));
        dropdown.set_hexpand(true);
        row.append(&dropdown);
        self.root.append(&row);
        if !target.is_empty() && !key.profiles.contains(&target) {
            self.root.append(&warning("This target profile no longer exists. Choose another profile."));
        }
    }

    fn build_macro(&self, app: &Rc<App>) {
        let me = Rc::downgrade(&self.me());
        let (steps, selected) = {
            let d = self.draft.borrow();
            (d.steps.clone(), d.selected_step.min(d.steps.len().saturating_sub(1)))
        };
        let top = hbox(8);
        let count = label(&format!("{} steps", steps.len()), &["dim-label"]);
        count.set_hexpand(true);
        top.append(&count);
        let add_menu = gio::Menu::new();
        add_menu.append(Some("Key"), Some("macro.add-key"));
        add_menu.append(Some("Text"), Some("macro.add-text"));
        add_menu.append(Some("Delay"), Some("macro.add-delay"));
        let add = gtk::MenuButton::new();
        add.set_label("Add Step");
        add.set_menu_model(Some(&add_menu));
        add.set_sensitive(steps.len() < validation::MAXIMUM_MACRO_STEPS);
        let group = gio::SimpleActionGroup::new();
        for (name, step) in [
            ("add-key", MacroStep::key(KeyStroke::new("space", &[], Some(crate::model::keys::mac::SPACE)))),
            ("add-text", MacroStep::text("")),
            ("add-delay", MacroStep::delay(100)),
        ] {
            let action = gio::SimpleAction::new(name, None);
            let me = me.clone();
            action.connect_activate(move |_, _| {
                if let Some(me) = me.upgrade() {
                    {
                        let mut d = me.draft.borrow_mut();
                        d.steps.push(step.clone());
                        d.selected_step = d.steps.len() - 1;
                    }
                    me.schedule_rebuild();
                }
            });
            group.add_action(&action);
        }
        self.root.insert_action_group("macro", Some(&group));
        top.append(&add);
        self.root.append(&top);

        if steps.is_empty() {
            self.root.append(&dim("Add a key, text or delay step."));
        } else {
            let titles: Vec<String> = steps.iter().enumerate().map(|(i, s)| s.summary(i)).collect();
            let picker = string_dropdown(&titles);
            picker.set_selected(selected as u32);
            let me2 = me.clone();
            picker.connect_selected_notify(move |d| {
                if let Some(me) = me2.upgrade() {
                    me.draft.borrow_mut().selected_step = d.selected() as usize;
                    me.schedule_rebuild();
                }
            });
            self.root.append(&picker);
            self.build_step_editor(app, &steps[selected]);
            let row = hbox(8);
            let up = icon_button("opennaga-up-symbolic", "Move step up");
            up.set_sensitive(selected > 0);
            let down = icon_button("opennaga-down-symbolic", "Move step down");
            down.set_sensitive(selected + 1 < steps.len());
            for (b, delta) in [(&up, -1i64), (&down, 1)] {
                let me = me.clone();
                b.connect_clicked(move |_| {
                    if let Some(me) = me.upgrade() {
                        {
                            let mut d = me.draft.borrow_mut();
                            let from = d.selected_step;
                            let to = (from as i64 + delta) as usize;
                            if to < d.steps.len() {
                                d.steps.swap(from, to);
                                d.selected_step = to;
                            }
                        }
                        me.schedule_rebuild();
                    }
                });
            }
            row.append(&up);
            row.append(&down);
            row.append(&spacer());
            let remove = button("Remove Step");
            let me3 = me.clone();
            remove.connect_clicked(move |_| {
                if let Some(me) = me3.upgrade() {
                    {
                        let mut d = me.draft.borrow_mut();
                        let index = d.selected_step;
                        if index < d.steps.len() {
                            d.steps.remove(index);
                        }
                        d.selected_step = index.min(d.steps.len().saturating_sub(1));
                    }
                    me.schedule_rebuild();
                }
            });
            row.append(&remove);
            self.root.append(&row);
        }
        self.root.append(&caption("Pressing this button again restarts its macro. Macros on other buttons stay independent."));
        let apply = button("Apply Macro");
        apply.add_css_class("suggested-action");
        apply.set_halign(gtk::Align::Start);
        let me4 = me.clone();
        apply.connect_clicked(move |_| {
            if let Some(me) = me4.upgrade() {
                me.persist();
            }
        });
        self.root.append(&apply);

        let expander = gtk::Expander::new(Some("Advanced JSON"));
        let content = vbox(8);
        content.set_margin_top(8);
        let json = serde_json::to_value(&steps).and_then(|v| serde_json::to_string_pretty(&v)).unwrap_or_else(|_| "[]".into());
        let (scroll, view) = text_view(&json, 140);
        content.append(&scroll);
        let buttons = hbox(8);
        let copy = button("Copy Draft Here");
        let view_copy = view.clone();
        let me5 = me.clone();
        copy.connect_clicked(move |_| {
            if let Some(me) = me5.upgrade() {
                let steps = me.draft.borrow().steps.clone();
                let text = serde_json::to_value(&steps).and_then(|v| serde_json::to_string_pretty(&v)).unwrap_or_else(|_| "[]".into());
                view_copy.buffer().set_text(&text);
            }
        });
        let load = button("Load JSON");
        let me6 = me.clone();
        load.connect_clicked(move |_| {
            let Some(me) = me6.upgrade() else { return };
            let text = text_of(&view);
            let result = if text.len() > validation::MAXIMUM_FILE_BYTES {
                Err("The JSON draft is too large.".to_string())
            } else {
                serde_json::from_str::<Vec<MacroStep>>(&text)
                    .map_err(|e| e.to_string())
                    .and_then(|steps| validation::validate_macro(&steps).map(|_| steps).map_err(|e| e.0))
            };
            {
                let mut d = me.draft.borrow_mut();
                match result {
                    Ok(steps) => {
                        d.steps = steps;
                        d.selected_step = 0;
                        d.validation = None;
                    }
                    Err(error) => d.validation = Some(error),
                }
            }
            me.schedule_rebuild();
        });
        buttons.append(&copy);
        buttons.append(&load);
        content.append(&buttons);
        expander.set_child(Some(&content));
        self.root.append(&expander);
    }

    fn build_step_editor(&self, app: &Rc<App>, step: &MacroStep) {
        let me = Rc::downgrade(&self.me());
        match step.kind.as_str() {
            "key" => {
                let stroke = step.key_stroke.clone().unwrap_or_else(|| KeyStroke::new("space", &[], Some(crate::model::keys::mac::SPACE)));
                let catalog = app.layout.borrow().catalog();
                let titles: Vec<String> = catalog
                    .iter()
                    .map(|k| if k.group == KeyGroup::Keypad { format!("Keypad {}", k.label) } else { k.label.clone() })
                    .collect();
                let dropdown = string_dropdown(&titles);
                dropdown.set_enable_search(true);
                let code = stroke.effective_code();
                if let Some(index) = catalog.iter().position(|k| Some(k.code) == code) {
                    dropdown.set_selected(index as u32);
                }
                let modifiers = stroke.modifiers.clone();
                let me2 = me.clone();
                dropdown.connect_selected_notify(move |d| {
                    let Some(me) = me2.upgrade() else { return };
                    if let Some(entry) = catalog.get(d.selected() as usize) {
                        let mut draft = me.draft.borrow_mut();
                        let index = draft.selected_step;
                        if let Some(step) = draft.steps.get_mut(index) {
                            step.key_stroke = Some(entry.stroke(&modifiers));
                        }
                        drop(draft);
                        me.schedule_rebuild();
                    }
                });
                let row = hbox(10);
                row.append(&label("Key", &[]));
                dropdown.set_hexpand(true);
                row.append(&dropdown);
                self.root.append(&row);
                let toggles = hbox(6);
                toggles.set_homogeneous(true);
                let normalized = stroke.normalized_modifiers();
                for (name, title, tip) in MODIFIERS {
                    let toggle = gtk::ToggleButton::with_label(title);
                    toggle.add_css_class("naga-modifier");
                    toggle.set_active(normalized.iter().any(|m| m == name));
                    toggle.set_tooltip_text(Some(tip));
                    let me = me.clone();
                    toggle.connect_toggled(move |t| {
                        let Some(me) = me.upgrade() else { return };
                        let mut d = me.draft.borrow_mut();
                        let index = d.selected_step;
                        if let Some(stroke) = d.steps.get_mut(index).and_then(|s| s.key_stroke.as_mut()) {
                            let mut mods = stroke.normalized_modifiers();
                            mods.retain(|m| m != name);
                            if t.is_active() {
                                mods.push(name.to_string());
                            }
                            stroke.modifiers = mods;
                        }
                        drop(d);
                        me.schedule_rebuild();
                    });
                    toggles.append(&toggle);
                }
                self.root.append(&toggles);
                let recording = self.recording.get() == Recording::MacroStep;
                let record = button(if recording { "Cancel Recording" } else { "Record Key" });
                if recording {
                    record.add_css_class("suggested-action");
                }
                record.connect_clicked(move |_| {
                    if let Some(me) = me.upgrade() {
                        if me.recording.get() == Recording::MacroStep {
                            me.stop_recording();
                        } else {
                            me.start_recording(Recording::MacroStep);
                        }
                        me.schedule_rebuild();
                    }
                });
                self.root.append(&record);
            }
            "text" => {
                let (scroll, view) = text_view(step.text.as_deref().unwrap_or(""), 90);
                view.buffer().connect_changed(move |buffer| {
                    if let Some(me) = me.upgrade() {
                        let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false).to_string();
                        let mut d = me.draft.borrow_mut();
                        let index = d.selected_step;
                        if let Some(step) = d.steps.get_mut(index) {
                            step.text = Some(text);
                        }
                    }
                });
                self.root.append(&scroll);
            }
            _ => {
                let spin = gtk::SpinButton::with_range(0.0, 600_000.0, 10.0);
                spin.set_value(step.delay_ms.unwrap_or(0) as f64);
                spin.connect_value_changed(move |s| {
                    if let Some(me) = me.upgrade() {
                        let mut d = me.draft.borrow_mut();
                        let index = d.selected_step;
                        if let Some(step) = d.steps.get_mut(index) {
                            step.delay_ms = Some(s.value() as i64);
                        }
                    }
                });
                let row = hbox(10);
                row.append(&label("Milliseconds", &[]));
                spin.set_hexpand(true);
                row.append(&spin);
                self.root.append(&row);
                self.root.append(&caption("0 to 600,000 ms"));
            }
        }
    }
}

/// The key picker: a group menu and a grid of keys labelled for the layout.
fn key_selector(
    app: &Rc<App>,
    selected: Option<u16>,
    group: KeyGroup,
    on_group: impl Fn(KeyGroup) + 'static,
    on_select: impl Fn(KeyboardKey) + Clone + 'static,
) -> gtk::Box {
    let container = vbox(10);
    let titles: Vec<String> = KeyGroup::ALL.iter().map(|g| g.title().to_string()).collect();
    let groups = string_dropdown(&titles);
    groups.set_selected(KeyGroup::ALL.iter().position(|g| *g == group).unwrap_or(0) as u32);
    groups.connect_selected_notify(move |d| on_group(KeyGroup::ALL[d.selected() as usize]));
    let row = hbox(10);
    row.append(&label("Choose a key", &[]));
    groups.set_hexpand(true);
    row.append(&groups);
    container.append(&row);
    let grid = gtk::Grid::new();
    grid.set_column_homogeneous(true);
    grid.set_row_spacing(5);
    grid.set_column_spacing(5);
    let columns = if group == KeyGroup::Navigation { 4 } else { 6 };
    let keys: Vec<KeyboardKey> = app.layout.borrow().catalog().into_iter().filter(|k| k.group == group).collect();
    for (i, key) in keys.into_iter().enumerate() {
        let b = gtk::Button::with_label(&key.label);
        b.add_css_class("naga-keycap");
        if Some(key.code) == selected {
            b.add_css_class("selected");
        }
        b.set_tooltip_text(Some(&if key.group == KeyGroup::Keypad { format!("Keypad: {}", key.label) } else { key.label.clone() }));
        if let Some(child) = b.child().and_then(|c| c.downcast::<gtk::Label>().ok()) {
            child.set_ellipsize(gtk::pango::EllipsizeMode::End);
        }
        let select = on_select.clone();
        b.connect_clicked(move |_| select(key.clone()));
        grid.attach(&b, (i % columns) as i32, (i / columns) as i32, 1, 1);
    }
    container.append(&grid);
    container
}
