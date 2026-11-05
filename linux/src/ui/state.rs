//! Main-thread application state: the Linux counterpart of the macOS
//! AppDelegate, WorkspaceModel and AppProfileSwitcher. Background threads
//! (input engine, hardware worker, focus watcher, tray) send messages here.

use crate::desktop::focus::{self, Backend, Focus};
use crate::desktop::{access, apps};
use crate::hw::controller::{self, DeviceState, Operation, Request, Response, Worker};
use crate::hw::hidraw::HidrawTransport;
use crate::hw::onboard::{self, OnboardProfilePlan, PlanContext};
use crate::hw::protocol::RazerTransport;
use crate::input::engine::{self, Command, DeviceReport, EngineHandle, EngineSender, EngineStatus, Event, Policy};
use crate::model::actions::{ActionType, MappingLayer, SystemAction};
use crate::model::app_profiles;
use crate::model::config::ConfigManager;
use crate::model::data_folder;
use crate::model::layout::LayoutMap;
use crate::model::mapper::SystemPlan;
use crate::model::prefs::Prefs;
use crate::model::profile::Mapping;
use crate::model::scroll::ScrollSensitivity;
use crate::ui::tray::{TrayAction, TrayHandle};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::sync::mpsc;
use adw::prelude::*;
use gtk::glib;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Buttons,
    AppProfiles,
    Sensitivity,
    Status,
}

impl Section {
    pub const ALL: [Section; 4] = [Section::Buttons, Section::AppProfiles, Section::Sensitivity, Section::Status];
    pub fn title(self) -> &'static str {
        match self {
            Section::Buttons => "Buttons",
            Section::AppProfiles => "App Profiles",
            Section::Sensitivity => "Sensitivity",
            Section::Status => "Status",
        }
    }
    pub fn subtitle(self) -> &'static str {
        match self {
            Section::Buttons => "Your mouse, your assignments.",
            Section::AppProfiles => "A profile for each app",
            Section::Sensitivity => "Sensor and hardware controls",
            Section::Status => "Connection and service",
        }
    }
    pub fn icon(self) -> &'static str {
        match self {
            Section::Buttons => "opennaga-buttons-symbolic",
            Section::AppProfiles => "opennaga-apps-symbolic",
            Section::Sensitivity => "opennaga-sensitivity-symbolic",
            Section::Status => "opennaga-status-symbolic",
        }
    }
    /// Accepts the title as on macOS ("App Profiles"), without spaces, with
    /// dashes, or the short form "apps".
    pub fn from_name(name: &str) -> Option<Section> {
        let wanted: String = name.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
        if wanted == "apps" {
            return Some(Section::AppProfiles);
        }
        Section::ALL.iter().copied().find(|s| s.title().replace(' ', "").to_lowercase() == wanted)
    }
}

pub struct UiState {
    pub section: Section,
    pub layer: MappingLayer,
    pub selected: i32,
    pub top_view: bool,
}

pub enum Message {
    Engine(Event),
    Hardware(Response),
    Focus(Focus),
    Tray(TrayAction),
    Quit,
}

/// Runs on the main thread once a hardware operation finishes.
pub type Completion = Box<dyn FnOnce(&Rc<App>)>;

pub struct App {
    pub adw_app: adw::Application,
    pub config: RefCell<ConfigManager>,
    pub device: RefCell<DeviceState>,
    hw: Option<mpsc::Sender<Request>>,
    ticket: Cell<u64>,
    completions: RefCell<HashMap<u64, Completion>>,
    pub engine: RefCell<Option<EngineHandle>>,
    engine_sender: Option<EngineSender>,
    pub engine_status: RefCell<EngineStatus>,
    pub devices: RefCell<DeviceReport>,
    pub last_input: RefCell<String>,
    pub active_button: Cell<Option<i32>>,
    active_serial: Cell<u64>,
    pub macro_count: Cell<usize>,
    pub action_error: RefCell<Option<String>>,
    pub system_result: RefCell<Option<String>>,
    pub focus: RefCell<Option<Focus>>,
    pub focus_backend: Cell<Backend>,
    switcher_signature: RefCell<(bool, BTreeMap<String, String>)>,
    pub ui: RefCell<UiState>,
    pub layout: RefCell<LayoutMap>,
    installed: RefCell<Option<Rc<Vec<apps::InstalledApp>>>>,
    listeners: RefCell<Vec<Rc<dyn Fn()>>>,
    notifying: Cell<bool>,
    dirty: Cell<bool>,
    pub snapshot: bool,
    pub shutting_down: Cell<bool>,
    pub window: RefCell<Option<adw::ApplicationWindow>>,
    tray: RefCell<Option<TrayHandle>>,
    onboard_busy: Cell<bool>,
    pub ui_error: RefCell<Option<String>>,
    pub window_visible: Cell<bool>,
}

impl App {
    pub fn new(adw_app: adw::Application, snapshot: bool, sender: async_channel::Sender<Message>) -> Rc<App> {
        let prefs = if snapshot { Prefs::default() } else { Prefs::load() };
        let mut config = ConfigManager::new(data_folder::file("profiles.json"), prefs, !snapshot);
        config.load();
        let (hw, engine) = if snapshot {
            (None, None)
        } else {
            let hw_sender = sender.clone();
            let worker = Worker::new(Box::new(|| HidrawTransport::open().map(|t| Box::new(t) as Box<dyn RazerTransport>)));
            let hw = controller::spawn(worker, move |response| {
                let _ = hw_sender.send_blocking(Message::Hardware(response));
            });
            let engine_sender = sender.clone();
            let engine = engine::spawn(move |event| {
                let _ = engine_sender.try_send(Message::Engine(event));
            });
            (Some(hw), Some(engine))
        };
        let engine_sender = engine.as_ref().map(|e| e.sender());
        if let Some(sink) = engine_sender.clone() {
            config.set_mapping_sink(Box::new(move |update| sink.send(Command::Mapping(update))));
        }
        let signature = (config.auto_switch_enabled, config.app_profiles.clone());
        let app = Rc::new(App {
            adw_app,
            config: RefCell::new(config),
            device: RefCell::new(DeviceState::default()),
            hw,
            ticket: Cell::new(0),
            completions: RefCell::new(HashMap::new()),
            engine: RefCell::new(engine),
            engine_sender,
            engine_status: RefCell::new(EngineStatus::default()),
            devices: RefCell::new(DeviceReport::default()),
            last_input: RefCell::new("No input detected. The DPI buttons need the receiver's report access.".into()),
            active_button: Cell::new(None),
            active_serial: Cell::new(0),
            macro_count: Cell::new(0),
            action_error: RefCell::new(None),
            system_result: RefCell::new(None),
            focus: RefCell::new(None),
            focus_backend: Cell::new(Backend::Unsupported),
            switcher_signature: RefCell::new(signature),
            ui: RefCell::new(UiState { section: Section::Buttons, layer: MappingLayer::Standard, selected: 1, top_view: false }),
            layout: RefCell::new(LayoutMap::us()),
            installed: RefCell::new(None),
            listeners: RefCell::new(Vec::new()),
            notifying: Cell::new(false),
            dirty: Cell::new(false),
            snapshot,
            shutting_down: Cell::new(false),
            window: RefCell::new(None),
            tray: RefCell::new(None),
            onboard_busy: Cell::new(false),
            ui_error: RefCell::new(None),
            window_visible: Cell::new(false),
        });
        if !snapshot {
            let focus_sender = sender.clone();
            app.focus_backend.set(focus::spawn(move |f| {
                let _ = focus_sender.try_send(Message::Focus(f));
            }));
            let scroll = app.config.borrow().scroll_sensitivity();
            app.send(Command::Scroll(scroll));
            app.update_policy();
            app.hardware(Operation::Refresh, None);
        }
        app
    }

    pub fn set_tray(&self, tray: Option<TrayHandle>) {
        *self.tray.borrow_mut() = tray;
    }

    pub fn has_tray(&self) -> bool {
        self.tray.borrow().is_some()
    }

    // MARK: - Observation

    pub fn listen(&self, listener: impl Fn() + 'static) {
        self.listeners.borrow_mut().push(Rc::new(listener));
    }

    /// Re-renders every view. Nested calls are coalesced.
    pub fn notify(&self) {
        if self.notifying.get() {
            self.dirty.set(true);
            return;
        }
        self.notifying.set(true);
        loop {
            self.dirty.set(false);
            let listeners: Vec<Rc<dyn Fn()>> = self.listeners.borrow().clone();
            for listener in listeners {
                listener();
            }
            if !self.dirty.get() {
                break;
            }
        }
        self.notifying.set(false);
        if let Some(tray) = self.tray.borrow().as_ref() {
            tray.update(self.tray_state());
        }
    }

    pub fn send(&self, command: Command) {
        if let Some(sender) = &self.engine_sender {
            sender.send(command);
        }
    }

    // MARK: - Messages

    pub fn handle(self: &Rc<Self>, message: Message) {
        match message {
            Message::Engine(event) => self.handle_engine(event),
            Message::Hardware(response) => self.handle_hardware(response),
            Message::Focus(focus) => self.handle_focus(focus),
            Message::Tray(action) => crate::ui::tray::perform(self, action),
            Message::Quit => self.request_quit(),
        }
    }

    fn handle_engine(self: &Rc<Self>, event: Event) {
        match event {
            Event::Devices(report) => {
                let changed = {
                    let previous = self.devices.borrow();
                    previous.name != report.name || previous.transport != report.transport
                };
                *self.devices.borrow_mut() = report;
                if changed && !self.shutting_down.get() {
                    // Coalesce the receiver's interfaces before reading values.
                    let app = self.clone();
                    glib::timeout_add_local_once(Duration::from_millis(400), move || app.refresh_hardware_when_idle());
                }
            }
            Event::Status(status) => *self.engine_status.borrow_mut() = status,
            Event::Activity(button) => {
                self.active_button.set(Some(button));
                let serial = self.active_serial.get() + 1;
                self.active_serial.set(serial);
                let app = self.clone();
                glib::timeout_add_local_once(Duration::from_millis(350), move || {
                    if app.active_serial.get() == serial {
                        app.active_button.set(None);
                        app.notify();
                    }
                });
            }
            Event::LastInput(text) => {
                *self.last_input.borrow_mut() = text;
                return self.notify_status_only();
            }
            Event::Macros { active, error } => {
                self.macro_count.set(active);
                *self.action_error.borrow_mut() = error;
            }
            Event::SwitchProfile(name) => self.select_profile(&name),
            Event::SystemResult(result) => *self.system_result.borrow_mut() = result,
            Event::Error(message) => *self.action_error.borrow_mut() = Some(message),
        }
        self.notify();
    }

    /// The status pane's last input line changes on every press; other views
    /// do not depend on it, so only a cheap refresh runs.
    fn notify_status_only(&self) {
        if self.ui.borrow().section == Section::Status {
            self.notify();
        }
    }

    fn refresh_hardware_when_idle(self: &Rc<Self>) {
        if self.shutting_down.get() {
            return;
        }
        if self.device.borrow().is_busy {
            let app = self.clone();
            glib::timeout_add_local_once(Duration::from_millis(400), move || app.refresh_hardware_when_idle());
        } else {
            self.hardware(Operation::Refresh, None);
        }
    }

    fn handle_hardware(self: &Rc<Self>, response: Response) {
        self.device.borrow_mut().complete(&response.outcome);
        self.send(Command::DriverMode(self.device.borrow().driver_mode_enabled));
        match &response.operation {
            Operation::Onboard(_) | Operation::RestoreOnboard => {
                self.onboard_busy.set(false);
                // A save or restore changes what the switcher may do.
                self.apply_focus();
            }
            Operation::ImportOnboard => {
                if let Some(functions) = &response.outcome.imported {
                    let layout = self.layout.borrow().clone();
                    let (standard, issues) = onboard::mapping_from(functions, &layout);
                    let (shifted, shift_issues) =
                        onboard::hypershift_mapping_from(functions, response.outcome.imported_hypershift.as_ref().unwrap_or(&Default::default()), &layout);
                    let created = self.config.borrow_mut().create_profile_from("From Mouse", &standard, &shifted, true);
                    let mut device = self.device.borrow_mut();
                    match created {
                        Some(name) => {
                            let mut lines = vec![format!(
                                "Imported supported assignments into profile {name}. Edit this profile and use Save to Mouse to apply changes. Software remapping is paused for imported profiles."
                            )];
                            lines.extend(issues);
                            lines.extend(shift_issues);
                            device.status_message = lines.join("\n");
                        }
                        None => {
                            device.status_message =
                                self.config.borrow().last_error.clone().unwrap_or_else(|| "Unable to save the imported mouse profile.".into())
                        }
                    }
                }
            }
            _ => {}
        }
        self.update_policy();
        if let Some(completion) = self.completions.borrow_mut().remove(&response.ticket) {
            completion(self);
        }
        self.notify();
    }

    /// Starts a hardware operation unless one is running. Returns false when
    /// it was not admitted.
    pub fn hardware(self: &Rc<Self>, operation: Operation, completion: Option<Completion>) -> bool {
        let Some(hw) = &self.hw else {
            if let Some(completion) = completion {
                completion(self);
            }
            return false;
        };
        if !self.device.borrow_mut().admit(&operation) {
            return false;
        }
        if matches!(operation, Operation::Onboard(_) | Operation::RestoreOnboard) {
            // Software interception stops while mouse memory changes.
            self.onboard_busy.set(true);
            self.update_policy();
        }
        let ticket = self.ticket.get() + 1;
        self.ticket.set(ticket);
        if let Some(completion) = completion {
            self.completions.borrow_mut().insert(ticket, completion);
        }
        let _ = hw.send(Request { operation, ticket });
        self.notify();
        true
    }

    // MARK: - Policy

    pub fn onboard_active(&self) -> bool {
        onboard::is_active()
    }

    pub fn update_policy(&self) {
        let onboard = self.onboard_active() || self.config.borrow().current_profile_is_onboard_only();
        let busy = self.onboard_busy.get();
        let remap = !busy && !onboard && self.config.borrow().remapping_enabled();
        self.send(Command::Policy(Policy { remap, merge_native: !busy && onboard }));
    }

    pub fn remapping_active(&self) -> bool {
        self.engine_status.borrow().remapping
    }

    pub fn permissions_granted(&self) -> bool {
        let status = self.engine_status.borrow();
        status.virtual_error.is_none() && (status.naga_nodes == 0 || status.input_access())
    }

    pub fn connected(&self) -> bool {
        self.devices.borrow().name.is_some()
    }

    pub fn service_status(&self) -> String {
        let config = self.config.borrow();
        if config.profiles_read_only {
            return "Profiles need recovery".into();
        }
        if self.onboard_active() {
            return format!("Mouse memory: {}", onboard::saved_name().unwrap_or_else(|| "restore needed".into()));
        }
        if config.current_profile_is_onboard_only() {
            return "Imported profile: save to mouse".into();
        }
        if !config.remapping_enabled() {
            return "Remapping paused".into();
        }
        if !self.permissions_granted() {
            return "Permissions needed".into();
        }
        if !self.connected() {
            return "Waiting for the mouse".into();
        }
        if !self.remapping_active() {
            return "Service unavailable".into();
        }
        "Remapping active".into()
    }

    // MARK: - Actions

    pub fn select_profile(&self, name: &str) {
        self.config.borrow_mut().set_current_profile(name);
        self.after_config_change();
    }

    pub fn set_remapping(&self, enabled: bool) {
        {
            let config = self.config.borrow();
            if config.profiles_read_only || config.current_profile_is_onboard_only() || self.onboard_active() || self.device.borrow().is_busy {
                drop(config);
                self.notify();
                return;
            }
        }
        self.config.borrow_mut().set_remapping_enabled(enabled);
        self.update_policy();
        self.notify();
    }

    pub fn save_action(&self, button: i32, action: Option<ActionType>, layer: MappingLayer) {
        self.config.borrow_mut().set_action(button, action, layer);
        self.after_config_change();
    }

    pub fn set_auto_switch(&self, enabled: bool) {
        self.config.borrow_mut().set_auto_switch_enabled(enabled);
        self.after_config_change();
    }

    pub fn bind(&self, app_id: &str, profile: Option<&str>) {
        self.config.borrow_mut().set_app_profile(profile, app_id);
        self.after_config_change();
    }

    pub fn set_scroll(&self, sensitivity: ScrollSensitivity) {
        self.config.borrow_mut().set_scroll_sensitivity(sensitivity);
        self.send(Command::Scroll(sensitivity.clamped()));
        self.notify();
    }

    pub fn undo(&self) {
        self.config.borrow_mut().undo();
        self.after_config_change();
    }

    pub fn redo(&self) {
        self.config.borrow_mut().redo();
        self.after_config_change();
    }

    pub fn reload_profiles(&self) {
        self.config.borrow_mut().load();
        // A reload resets the live profile to the selection; re-apply the
        // binding of the app in front.
        self.apply_focus();
        self.after_config_change();
    }

    pub fn restore_profile_backup(&self) {
        self.config.borrow_mut().restore_profile_backup();
        self.after_config_change();
    }

    pub fn cancel_macros(&self) {
        self.send(Command::CancelMacros);
    }

    pub fn restart_input(&self) {
        self.send(Command::Rescan);
    }

    pub fn test_system(&self, action: SystemAction) {
        *self.system_result.borrow_mut() = None;
        self.send(Command::TestSystem(action));
    }

    pub fn set_typing(&self, typing: crate::input::typing::TypingMap) {
        self.send(Command::Typing(typing));
    }

    pub fn set_window_visible(&self, visible: bool) {
        self.window_visible.set(visible);
        self.send(Command::Observe(visible));
    }

    pub fn after_config_change(&self) {
        self.update_policy();
        // A manual choice is not an app change; only a new binding or turning
        // automatic switching on or off re-resolves the focused app.
        let signature = {
            let config = self.config.borrow();
            (config.auto_switch_enabled, config.app_profiles.clone())
        };
        if *self.switcher_signature.borrow() != signature {
            *self.switcher_signature.borrow_mut() = signature;
            self.apply_focus();
        }
        self.notify();
    }

    // MARK: - App profiles

    fn handle_focus(&self, focus: Focus) {
        // Layer shells, empty workspaces and OpenNaga itself never change the
        // profile; the last real app stays in charge.
        if focus.app_id.is_empty() || focus.app_id.eq_ignore_ascii_case(crate::APP_ID) {
            if focus.app_id.eq_ignore_ascii_case(crate::APP_ID) {
                *self.focus.borrow_mut() = Some(focus);
                self.apply_focus();
                self.notify();
            }
            return;
        }
        *self.focus.borrow_mut() = Some(focus);
        self.apply_focus();
        self.notify();
    }

    pub fn apply_focus(&self) {
        let name = {
            let config = self.config.borrow();
            let focus = self.focus.borrow();
            app_profiles::resolve(
                focus.as_ref().map(|f| f.app_id.as_str()),
                Some(crate::APP_ID),
                &config.current_profile_name,
                config.auto_switch_enabled,
                self.onboard_active(),
                &config.app_profiles,
                &config.profiles,
            )
        };
        self.config.borrow_mut().apply_automatic_profile(&name);
    }

    pub fn frontmost_name(&self) -> Option<String> {
        let focus = self.focus.borrow();
        let focus = focus.as_ref()?;
        Some(self.app_name(&focus.app_id))
    }

    pub fn installed_apps(&self) -> Rc<Vec<apps::InstalledApp>> {
        if let Some(apps) = self.installed.borrow().as_ref() {
            return apps.clone();
        }
        let scanned = Rc::new(apps::scan());
        *self.installed.borrow_mut() = Some(scanned.clone());
        scanned
    }

    pub fn rescan_apps(&self) -> Rc<Vec<apps::InstalledApp>> {
        *self.installed.borrow_mut() = None;
        self.installed_apps()
    }

    pub fn app_name(&self, app_id: &str) -> String {
        apps::find(&self.installed_apps(), app_id).map(|a| a.name.clone()).unwrap_or_else(|| app_id.to_string())
    }

    pub fn app_icon(&self, app_id: &str) -> Option<String> {
        apps::find(&self.installed_apps(), app_id).and_then(|a| a.icon.clone())
    }

    // MARK: - Mouse memory

    pub fn onboard_plan(&self) -> OnboardProfilePlan {
        let config = self.config.borrow();
        let layout = self.layout.borrow();
        let env = crate::desktop::system::Environment::shared();
        let system = |action: SystemAction| -> SystemPlan { env.plan(action) };
        let context = PlanContext { layout: &layout, system: &system };
        OnboardProfilePlan::new(
            &config.current_profile_name,
            &config.mapping_for_current(MappingLayer::Standard),
            &config.mapping_for_current(MappingLayer::Hypershift),
            &context,
        )
    }

    pub fn current_mapping(&self, layer: MappingLayer) -> Mapping {
        self.config.borrow().mapping_for_current(layer)
    }

    // MARK: - Lifecycle

    pub fn rule_installed(&self) -> bool {
        access::rule_installed()
    }

    /// Defers quitting until the driver mode changed in this session is
    /// restored, like the macOS app's terminate reply.
    pub fn request_quit(self: &Rc<Self>) {
        if self.shutting_down.get() {
            return;
        }
        self.shutting_down.set(true);
        if let Some(mut engine) = self.engine.borrow_mut().take() {
            engine.shutdown();
        }
        if let Some(tray) = self.tray.borrow_mut().take() {
            tray.shutdown();
        }
        let app = self.clone();
        let finish = move |_: &Rc<App>| app.adw_app.quit();
        if !self.hardware(Operation::Restore, Some(Box::new(finish))) {
            self.adw_app.quit();
        }
        // Never hang on an unresponsive receiver.
        let app = self.clone();
        glib::timeout_add_local_once(Duration::from_secs(6), move || app.adw_app.quit());
    }

    /// Sleep releases input; wake restarts observation and reads values again.
    pub fn suspend(self: &Rc<Self>, sleeping: bool) {
        self.send(Command::Suspend(sleeping));
        if !sleeping {
            let app = self.clone();
            glib::timeout_add_local_once(Duration::from_secs(2), move || app.refresh_hardware_when_idle());
        }
    }

    pub fn tray_state(&self) -> crate::ui::tray::TrayState {
        let config = self.config.borrow();
        crate::ui::tray::TrayState {
            profiles: config.available_profiles(),
            current: config.current_profile_name.clone(),
            remapping: config.remapping_enabled() && !self.onboard_active(),
            remapping_available: !config.profiles_read_only && !config.current_profile_is_onboard_only() && !self.onboard_active(),
            auto_switch: config.auto_switch_enabled,
            status: self.service_status(),
            connected: self.connected(),
            battery: self.device.borrow().battery_level.or(self.devices.borrow().bluetooth_battery),
            runtime: if config.runtime_profile_name != config.current_profile_name { Some(config.runtime_profile_name.clone()) } else { None },
        }
    }
}
