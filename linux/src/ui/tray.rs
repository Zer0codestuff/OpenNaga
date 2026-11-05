//! The tray menu (StatusNotifierItem), the Linux counterpart of the macOS
//! menu bar popover: profile, Remapping, per-app switching, status, and
//! entries to open the window or quit. It runs on its own D-Bus thread and
//! talks to the main thread through messages.

use super::state::{App, Message, Section};
use ksni::blocking::TrayMethods;
use ksni::menu::{CheckmarkItem, RadioGroup, RadioItem, StandardItem, SubMenu};
use ksni::{Icon, MenuItem, ToolTip};
use std::rc::Rc;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrayState {
    pub profiles: Vec<String>,
    pub current: String,
    pub remapping: bool,
    pub remapping_available: bool,
    pub auto_switch: bool,
    pub status: String,
    pub connected: bool,
    pub battery: Option<i32>,
    pub runtime: Option<String>,
}

#[derive(Clone, Debug)]
pub enum TrayAction {
    ShowWindow,
    ShowStatus,
    SelectProfile(String),
    SetRemapping(bool),
    SetAutoSwitch(bool),
    Quit,
}

struct NagaTray {
    state: TrayState,
    sender: async_channel::Sender<Message>,
    icons: Vec<Icon>,
}

impl NagaTray {
    fn send(&self, action: TrayAction) {
        let _ = self.sender.try_send(Message::Tray(action));
    }
}

impl ksni::Tray for NagaTray {
    fn id(&self) -> String {
        "opennaga".into()
    }

    fn title(&self) -> String {
        "OpenNaga".into()
    }

    fn icon_name(&self) -> String {
        "input-mouse-symbolic".into()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        self.icons.clone()
    }

    fn tool_tip(&self) -> ToolTip {
        let mut description = self.state.status.clone();
        if let Some(battery) = self.state.battery {
            description.push_str(&format!(" · Battery {battery}%"));
        }
        ToolTip { title: "OpenNaga".into(), description, ..Default::default() }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(TrayAction::ShowWindow);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let state = &self.state;
        let mut items: Vec<MenuItem<Self>> = Vec::new();
        let header = if state.connected { "Naga V2 HyperSpeed" } else { "Mouse disconnected" };
        let battery = state.battery.map(|b| format!(" · {b}%")).unwrap_or_default();
        items.push(StandardItem { label: format!("OpenNaga · {header}{battery}"), enabled: false, ..Default::default() }.into());
        items.push(StandardItem { label: state.status.clone(), enabled: false, ..Default::default() }.into());
        items.push(MenuItem::Separator);
        let selected = state.profiles.iter().position(|p| *p == state.current).unwrap_or(0);
        let profiles = state.profiles.clone();
        items.push(
            SubMenu {
                label: format!("Profile: {}", state.current.replace('_', "__")),
                submenu: vec![RadioGroup {
                    selected,
                    select: Box::new(move |tray: &mut Self, index| {
                        if let Some(name) = profiles.get(index) {
                            tray.send(TrayAction::SelectProfile(name.clone()));
                        }
                    }),
                    options: state.profiles.iter().map(|p| RadioItem { label: p.replace('_', "__"), ..Default::default() }).collect(),
                }
                .into()],
                ..Default::default()
            }
            .into(),
        );
        if let Some(runtime) = &state.runtime {
            items.push(StandardItem { label: format!("Using {} for this app", runtime.replace('_', "__")), enabled: false, ..Default::default() }.into());
        }
        let remapping = state.remapping;
        items.push(
            CheckmarkItem {
                label: "Remapping".into(),
                checked: remapping,
                enabled: state.remapping_available,
                activate: Box::new(move |tray: &mut Self| tray.send(TrayAction::SetRemapping(!remapping))),
                ..Default::default()
            }
            .into(),
        );
        let auto = state.auto_switch;
        items.push(
            CheckmarkItem {
                label: "Switch profiles by app".into(),
                checked: auto,
                activate: Box::new(move |tray: &mut Self| tray.send(TrayAction::SetAutoSwitch(!auto))),
                ..Default::default()
            }
            .into(),
        );
        items.push(MenuItem::Separator);
        items.push(
            StandardItem { label: "Configure Mouse…".into(), activate: Box::new(|tray: &mut Self| tray.send(TrayAction::ShowWindow)), ..Default::default() }
                .into(),
        );
        items.push(
            StandardItem {
                label: "Status and Permissions…".into(),
                activate: Box::new(|tray: &mut Self| tray.send(TrayAction::ShowStatus)),
                ..Default::default()
            }
            .into(),
        );
        items.push(MenuItem::Separator);
        items.push(
            StandardItem {
                label: "Quit OpenNaga".into(),
                icon_name: "application-exit-symbolic".into(),
                activate: Box::new(|tray: &mut Self| tray.send(TrayAction::Quit)),
                ..Default::default()
            }
            .into(),
        );
        items
    }
}

pub struct TrayHandle {
    handle: ksni::blocking::Handle<NagaTray>,
}

impl TrayHandle {
    pub fn update(&self, state: TrayState) {
        self.handle.update(move |tray| {
            if tray.state != state {
                tray.state = state;
            }
        });
    }

    pub fn shutdown(self) {
        let _ = self.handle.shutdown();
    }
}

/// Starts the tray. Returns None when no tray host is running; the app then
/// keeps running in the background and opens its window when launched again.
pub fn spawn(state: TrayState, sender: async_channel::Sender<Message>, icons: Vec<Icon>) -> Option<TrayHandle> {
    let tray = NagaTray { state, sender, icons };
    match tray.spawn() {
        Ok(handle) => Some(TrayHandle { handle }),
        Err(error) => {
            eprintln!("[Tray] Not available: {error}");
            None
        }
    }
}

/// ARGB32 (network byte order) icons from RGBA pixels.
pub fn icon_from_rgba(width: i32, height: i32, rgba: &[u8]) -> Icon {
    let mut data = Vec::with_capacity(rgba.len());
    for pixel in rgba.chunks_exact(4) {
        data.extend_from_slice(&[pixel[3], pixel[0], pixel[1], pixel[2]]);
    }
    Icon { width, height, data }
}

pub fn perform(app: &Rc<App>, action: TrayAction) {
    match action {
        TrayAction::ShowWindow => super::window::present(app),
        TrayAction::ShowStatus => {
            app.ui.borrow_mut().section = Section::Status;
            super::window::present(app);
            app.notify();
        }
        TrayAction::SelectProfile(name) => app.select_profile(&name),
        TrayAction::SetRemapping(on) => app.set_remapping(on),
        TrayAction::SetAutoSwitch(on) => app.set_auto_switch(on),
        TrayAction::Quit => app.request_quit(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_to_argb() {
        let icon = icon_from_rgba(1, 1, &[1, 2, 3, 4]);
        assert_eq!(icon.data, vec![4, 1, 2, 3]);
    }
}
