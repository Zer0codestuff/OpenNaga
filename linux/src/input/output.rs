//! Remapped output: a virtual keyboard and a virtual pointer created with
//! uinput, plus helper processes. Keys and buttons are reference-counted,
//! so an output pressed by a mapping and by a forwarded native key is
//! released only when both let go.

use super::evdev::{self, UinputDevice};
use super::typing::{helper_command, TypingMap};
use crate::desktop::system::Environment;
use crate::model::actions::SystemAction;
use crate::model::keys::{ev, Mods};
use crate::model::macro_runner::TextResult;
use crate::model::mapper::{MapperOutput, SystemPlan};
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Instant;

pub const KEYBOARD_NAME: &str = "OpenNaga Virtual Keyboard";
pub const POINTER_NAME: &str = "OpenNaga Virtual Mouse";

pub enum OutputEvent {
    SwitchProfile(String),
    Error(String),
}

fn is_program(path: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

pub struct Output {
    keyboard: Option<UinputDevice>,
    pointer: Option<UinputDevice>,
    pub virtual_error: Option<String>,
    key_counts: HashMap<u16, u32>,
    button_counts: HashMap<u16, u32>,
    /// Other keyboards, read only for their modifier state.
    pub keyboards: Vec<File>,
    pub typing: TypingMap,
    /// Native keys re-sent with modifiers held on another keyboard.
    rerouted: BTreeMap<u16, Mods>,
    children: Vec<(Child, Option<u64>)>,
    next_token: u64,
    notify: Arc<dyn Fn(OutputEvent) + Send + Sync>,
}

impl Output {
    pub fn new(notify: Arc<dyn Fn(OutputEvent) + Send + Sync>) -> Self {
        let mut output = Output {
            keyboard: None,
            pointer: None,
            virtual_error: None,
            key_counts: HashMap::new(),
            button_counts: HashMap::new(),
            keyboards: Vec::new(),
            typing: TypingMap::us(),
            rerouted: BTreeMap::new(),
            children: Vec::new(),
            next_token: 1,
            notify,
        };
        output.create_devices();
        output
    }

    pub fn create_devices(&mut self) {
        if self.keyboard.is_some() && self.pointer.is_some() {
            return;
        }
        let keyboard = UinputDevice::create(KEYBOARD_NAME, &evdev::keyboard_keys(), &[]);
        let pointer = UinputDevice::create(POINTER_NAME, &evdev::pointer_buttons(), &evdev::pointer_axes());
        match (keyboard, pointer) {
            (Ok(k), Ok(p)) => {
                self.keyboard = Some(k);
                self.pointer = Some(p);
                self.virtual_error = None;
            }
            (Err(e), _) | (_, Err(e)) => {
                self.virtual_error = Some(if e.kind() == std::io::ErrorKind::PermissionDenied || e.kind() == std::io::ErrorKind::NotFound {
                    "No access to /dev/uinput. Install the OpenNaga access rule from Status.".into()
                } else {
                    format!("Virtual input unavailable: {e}.")
                });
            }
        }
    }

    pub fn is_ready(&self) -> bool {
        self.keyboard.is_some() && self.pointer.is_some()
    }

    fn write_key(&mut self, code: u16, value: i32) {
        if let Some(keyboard) = self.keyboard.as_mut() {
            keyboard.push(ev::EV_KEY, code, value);
            keyboard.flush();
        }
    }

    pub fn rel(&mut self, code: u16, value: i32) {
        if value != 0 {
            if let Some(pointer) = self.pointer.as_mut() {
                pointer.push(ev::EV_REL, code, value);
            }
        }
    }

    pub fn flush(&mut self) {
        if let Some(pointer) = self.pointer.as_mut() {
            pointer.flush();
        }
        if let Some(keyboard) = self.keyboard.as_mut() {
            keyboard.flush();
        }
    }

    fn virtual_mods(&self) -> Mods {
        let mut mods = Mods::NONE;
        for (code, count) in &self.key_counts {
            if *count > 0 {
                mods = mods.union(Mods::for_key(*code));
            }
        }
        mods
    }

    /// Forwards a key the mouse sent. When a modifier is held on another
    /// keyboard, it is added for this key, so Ctrl on the keyboard plus a
    /// native side key still makes a shortcut on compositors that keep
    /// modifier state per device.
    pub fn native_key(&mut self, code: u16, down: bool, merge: bool) {
        if down {
            if merge && Mods::for_key(code).is_empty() {
                let missing = self.modifiers_elsewhere().minus(self.virtual_mods());
                if !missing.is_empty() {
                    for (flag, modifier, _) in Mods::ORDER {
                        if missing.contains(flag) {
                            self.key(modifier, true);
                        }
                    }
                    self.rerouted.insert(code, missing);
                }
            }
            self.key(code, true);
        } else {
            self.key(code, false);
            if let Some(missing) = self.rerouted.remove(&code) {
                for (flag, modifier, _) in Mods::ORDER.iter().rev() {
                    if missing.contains(*flag) {
                        self.key(*modifier, false);
                    }
                }
            }
        }
    }

    /// Releases every key and button still down on the virtual devices.
    pub fn release_everything(&mut self) {
        let keys: Vec<u16> = self.key_counts.iter().filter(|(_, c)| **c > 0).map(|(k, _)| *k).collect();
        for code in keys {
            self.key_counts.insert(code, 0);
            self.write_key(code, 0);
        }
        let buttons: Vec<u16> = self.button_counts.iter().filter(|(_, c)| **c > 0).map(|(k, _)| *k).collect();
        for code in buttons {
            self.button_counts.insert(code, 0);
            if let Some(pointer) = self.pointer.as_mut() {
                pointer.push(ev::EV_KEY, code, 0);
            }
        }
        self.rerouted.clear();
        self.flush();
    }

    fn spawn_with_token(&mut self, argv: &[String], token: Option<u64>) -> bool {
        let Some((program, args)) = argv.split_first() else { return false };
        let mut command = Command::new(program);
        command.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).process_group(0);
        match command.spawn() {
            Ok(child) => {
                self.children.push((child, token));
                true
            }
            Err(error) => {
                (self.notify)(OutputEvent::Error(format!("Could not run {program}: {error}.")));
                false
            }
        }
    }

    /// Reaps finished helpers. Returns the macro tokens that can continue.
    pub fn reap(&mut self) -> Vec<u64> {
        let mut finished = Vec::new();
        self.children.retain_mut(|(child, token)| match child.try_wait() {
            Ok(None) => true,
            _ => {
                if let Some(token) = token {
                    finished.push(*token);
                }
                false
            }
        });
        finished
    }

    pub fn waiting_on_helpers(&self) -> bool {
        !self.children.is_empty()
    }
}

impl MapperOutput for Output {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn key(&mut self, code: u16, down: bool) {
        let count = self.key_counts.entry(code).or_insert(0);
        if down {
            *count += 1;
            if *count == 1 {
                self.write_key(code, 1);
            }
        } else if *count > 0 {
            *count -= 1;
            if *count == 0 {
                self.write_key(code, 0);
            }
        }
    }

    fn button(&mut self, code: u16, down: bool) {
        let count = self.button_counts.entry(code).or_insert(0);
        let write = if down {
            *count += 1;
            *count == 1
        } else if *count > 0 {
            *count -= 1;
            *count == 0
        } else {
            false
        };
        if write {
            if let Some(pointer) = self.pointer.as_mut() {
                pointer.push(ev::EV_KEY, code, down as i32);
                pointer.flush();
            }
        }
    }

    fn scroll(&mut self, vertical_hi_res: i32, vertical_detents: i32, horizontal_hi_res: i32, horizontal_detents: i32) {
        self.rel(ev::REL_WHEEL, vertical_detents);
        self.rel(ev::REL_WHEEL_HI_RES, vertical_hi_res);
        self.rel(ev::REL_HWHEEL, horizontal_detents);
        self.rel(ev::REL_HWHEEL_HI_RES, horizontal_hi_res);
        if let Some(pointer) = self.pointer.as_mut() {
            pointer.flush();
        }
    }

    fn modifiers_elsewhere(&mut self) -> Mods {
        let mut mods = Mods::NONE;
        for keyboard in &self.keyboards {
            let keys = evdev::held_keys(keyboard.as_raw_fd());
            for code in [ev::KEY_LEFTCTRL, ev::KEY_RIGHTCTRL, ev::KEY_LEFTSHIFT, ev::KEY_RIGHTSHIFT, ev::KEY_LEFTALT, ev::KEY_LEFTMETA, ev::KEY_RIGHTMETA] {
                if evdev::is_held(&keys, code) {
                    mods = mods.union(Mods::for_key(code));
                }
            }
            // Right Alt is AltGr on most layouts; it is not a shortcut modifier.
        }
        mods
    }

    fn type_text(&mut self, text: &str) -> TextResult {
        if let Some(sequence) = self.typing.sequence(text) {
            for typed in sequence {
                let modifiers = typed.modifier_keys();
                for m in &modifiers {
                    self.key(*m, true);
                }
                self.key(typed.code, true);
                self.key(typed.code, false);
                for m in modifiers.iter().rev() {
                    self.key(*m, false);
                }
            }
            return TextResult::Sent;
        }
        match helper_command(text) {
            Some(argv) => {
                let token = self.next_token;
                self.next_token += 1;
                if self.spawn_with_token(&argv, Some(token)) {
                    TextResult::Waiting(token)
                } else {
                    TextResult::Sent
                }
            }
            None => {
                (self.notify)(OutputEvent::Error(
                    "Some characters cannot be typed with this keyboard layout. Install wtype (Wayland) or xdotool (X11) to type them.".into(),
                ));
                TextResult::Sent
            }
        }
    }

    fn plan_system(&mut self, action: SystemAction) -> SystemPlan {
        Environment::shared().plan(action)
    }

    fn spawn(&mut self, argv: Vec<String>) {
        self.spawn_with_token(&argv, None);
    }

    fn shell(&mut self, command: &str) {
        self.spawn_with_token(&["/bin/sh".to_string(), "-c".to_string(), command.to_string()], None);
    }

    /// Desktop entries start through GIO, like a launcher would; programs
    /// run directly; anything else opens with its default app.
    fn launch(&mut self, path: &str) {
        let argv: Vec<String> = if path.ends_with(".desktop") {
            vec!["gio".into(), "launch".into(), path.into()]
        } else if is_program(path) {
            vec![path.into()]
        } else {
            vec!["xdg-open".into(), path.into()]
        };
        self.spawn_with_token(&argv, None);
    }

    fn switch_profile(&mut self, name: &str) {
        (self.notify)(OutputEvent::SwitchProfile(name.to_string()));
    }
}

impl super::frame::FrameSink for Output {
    fn rel(&mut self, code: u16, value: i32) {
        Output::rel(self, code, value)
    }
    fn native_key(&mut self, code: u16, down: bool, merge: bool) {
        Output::native_key(self, code, down, merge)
    }
    fn flush(&mut self) {
        Output::flush(self)
    }
}
