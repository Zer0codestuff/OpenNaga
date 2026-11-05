//! Press, hold and release semantics for remapped controls, ported from the
//! macOS `ButtonMapper`. Runs on the input thread. Output goes through
//! `MapperOutput`, which the engine implements with uinput devices and the
//! tests implement with a recorder.

use super::actions::*;
use super::keys::{self, ev, Mods};
use super::macro_runner::{MacroOutput, MacroRunner, TextResult};
use super::profile::Mapping;
use super::scroll::{ScrollSensitivity, WheelScaler, HI_RES_PER_DETENT};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

/// How this desktop runs a system function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SystemPlan {
    /// Press an evdev key with modifiers on the virtual keyboard.
    Key { code: u16, mods: Mods },
    /// Run a program (argv, no shell).
    Command(Vec<String>),
    /// Not available here; the message says why.
    Unavailable(String),
}

pub trait MapperOutput {
    fn now(&self) -> Instant;
    /// evdev key on the virtual keyboard. The output reference-counts keys,
    /// so a key pressed by two sources is released only by the last one.
    fn key(&mut self, code: u16, down: bool);
    fn button(&mut self, code: u16, down: bool);
    fn scroll(&mut self, vertical_hi_res: i32, vertical_detents: i32, horizontal_hi_res: i32, horizontal_detents: i32);
    /// Modifiers held on other keyboards, when they can be observed.
    fn modifiers_elsewhere(&mut self) -> Mods;
    fn type_text(&mut self, text: &str) -> TextResult;
    fn plan_system(&mut self, action: SystemAction) -> SystemPlan;
    fn spawn(&mut self, argv: Vec<String>);
    fn shell(&mut self, command: &str);
    fn launch(&mut self, path: &str);
    fn switch_profile(&mut self, name: &str);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hold {
    Key(u16, Mods),
    Mouse(u16),
}

#[derive(Default)]
pub struct ButtonMapper {
    mapping: Mapping,
    hypershift_mapping: Mapping,
    hypershift_triggers: BTreeSet<i32>,
    holds: BTreeMap<i32, Hold>,
    pressed: BTreeSet<i32>,
    synthetic: Mods,
    runner: MacroRunner,
    synthetic_scroll: WheelScaler,
    pub scroll_sensitivity: ScrollSensitivity,
    pub last_action_error: Option<String>,
    /// Set when the macro set or the action error changed.
    pub status_changed: bool,
}

fn without_hypershift(layer: Mapping) -> Mapping {
    layer.into_iter().filter(|(_, a)| !a.is_hypershift()).collect()
}

impl ButtonMapper {
    pub fn is_hypershift_active(&self) -> bool {
        !self.hypershift_triggers.is_empty()
    }

    /// The action a press performs now. While Hypershift is held, a control
    /// with a Hypershift assignment uses it and any other control keeps its
    /// standard action. A Hypershift control always stays one.
    pub fn action_for(&self, index: i32) -> Option<&ActionType> {
        let standard = self.mapping.get(&index);
        if let Some(action) = standard {
            if action.is_hypershift() {
                return Some(action);
            }
        }
        if self.is_hypershift_active() {
            if let Some(shifted) = self.hypershift_mapping.get(&index) {
                return Some(shifted);
            }
        }
        standard
    }

    pub fn has_mapping(&self, index: i32) -> bool {
        matches!(self.action_for(index), Some(action) if !action.is_hardware_dpi())
    }

    pub fn active_macro_buttons(&self) -> Vec<i32> {
        self.runner.active_buttons()
    }

    pub fn next_macro_deadline(&self) -> Option<Instant> {
        self.runner.next_deadline()
    }

    pub fn update_mapping(&mut self, standard: Mapping, hypershift: Mapping, out: &mut dyn MapperOutput) {
        self.release_all(out);
        self.mapping = standard;
        self.hypershift_mapping = without_hypershift(hypershift);
    }

    /// Replaces the assignments for the next press without touching what is
    /// held now. Outputs release by button index, so a key started under the
    /// old mapping still ends with its own key-up. Used for automatic
    /// profile changes, which can land while a mapped button is down.
    pub fn swap_mapping(&mut self, standard: Mapping, hypershift: Mapping) {
        self.cancel_macros();
        self.mapping = standard;
        self.hypershift_mapping = without_hypershift(hypershift);
    }

    pub fn release_all(&mut self, out: &mut dyn MapperOutput) {
        self.synthetic_scroll.reset();
        self.cancel_macros();
        let held: Vec<i32> = self.holds.keys().copied().collect();
        for button in held {
            self.handle_release(button, out);
        }
        self.pressed.clear();
        self.hypershift_triggers.clear();
        if self.holds.is_empty() && !self.synthetic.is_empty() {
            self.release_mods(self.synthetic, out);
        }
    }

    /// A press immediately followed by its release (wheel tilt ticks).
    pub fn handle(&mut self, index: i32, out: &mut dyn MapperOutput) {
        self.handle_press(index, out);
        self.handle_release(index, out);
    }

    pub fn handle_press(&mut self, index: i32, out: &mut dyn MapperOutput) {
        let Some(action) = self.action_for(index).cloned() else { return };
        if !self.pressed.insert(index) {
            return;
        }
        // The layer is chosen at press time; holds release by button index,
        // so an output started in one layer ends correctly in the other.
        if action.is_hypershift() {
            self.hypershift_triggers.insert(index);
            return;
        }
        match &action {
            ActionType::KeySequence { keys, .. } if keys.len() == 1 => {
                let Some(code) = evdev_code(&keys[0]) else { return };
                let mods = keys[0].mods().minus(Mods::FN).union(out.modifiers_elsewhere());
                self.press_mods(mods, out);
                let already_held = self.holds.values().any(|h| matches!(h, Hold::Key(c, _) if *c == code));
                self.holds.insert(index, Hold::Key(code, mods));
                if !already_held {
                    out.key(code, true);
                }
            }
            ActionType::Mouse { action, .. } => {
                if let Some(button) = action.button() {
                    self.holds.insert(index, Hold::Mouse(button));
                    out.button(button, true);
                } else {
                    self.perform_mouse(*action, out);
                }
            }
            _ => self.perform(&action, index, out),
        }
    }

    pub fn handle_release(&mut self, index: i32, out: &mut dyn MapperOutput) {
        self.pressed.remove(&index);
        self.hypershift_triggers.remove(&index);
        let Some(hold) = self.holds.remove(&index) else { return };
        // Never release an output another physical input still holds.
        match hold {
            Hold::Key(code, mods) => {
                if !self.holds.values().any(|h| matches!(h, Hold::Key(c, _) if *c == code)) {
                    out.key(code, false);
                }
                self.release_mods(mods, out);
            }
            Hold::Mouse(button) => {
                if !self.holds.values().any(|h| matches!(h, Hold::Mouse(b) if *b == button)) {
                    out.button(button, false);
                }
            }
        }
    }

    fn perform(&mut self, action: &ActionType, index: i32, out: &mut dyn MapperOutput) {
        match action {
            ActionType::Audio { action, .. } => {
                self.perform_system(action.system(), out);
            }
            ActionType::System { action, .. } => {
                self.perform_system(*action, out);
            }
            ActionType::Disabled | ActionType::Hypershift { .. } => {}
            ActionType::Mouse { action, .. } => self.perform_mouse(*action, out),
            ActionType::KeySequence { keys, .. } => {
                for key in keys {
                    self.send_stroke(key, out);
                }
            }
            ActionType::Application { path, .. } => out.launch(path),
            ActionType::SystemCommand { command, .. } => out.shell(command),
            ActionType::TextSnippet { text, .. } => self.run_macro(&[MacroStep::text(text)], index, out),
            ActionType::Macro { steps, .. } => self.run_macro(steps, index, out),
            ActionType::ProfileSwitch { profile, .. } => out.switch_profile(profile),
        }
    }

    /// Runs a system function now. Returns a message when it is unavailable.
    pub fn perform_system(&mut self, action: SystemAction, out: &mut dyn MapperOutput) -> Option<String> {
        match out.plan_system(action) {
            SystemPlan::Key { code, mods } => {
                self.send_code(code, mods, out);
                None
            }
            SystemPlan::Command(argv) => {
                out.spawn(argv);
                None
            }
            SystemPlan::Unavailable(message) => Some(message),
        }
    }

    fn perform_mouse(&mut self, action: MouseAction, out: &mut dyn MapperOutput) {
        if let Some(button) = action.button() {
            out.button(button, true);
            out.button(button, false);
            return;
        }
        match action {
            // Alt with the arrow keys navigates in every Linux browser and
            // file manager, whatever the keyboard layout.
            MouseAction::BrowserBack => self.send_code(ev::KEY_LEFT, Mods::ALT, out),
            MouseAction::BrowserForward => self.send_code(ev::KEY_RIGHT, Mods::ALT, out),
            MouseAction::ScrollUp | MouseAction::ScrollDown | MouseAction::ScrollLeft | MouseAction::ScrollRight => {
                let amount = 3 * HI_RES_PER_DETENT;
                let (vertical, horizontal) = match action {
                    MouseAction::ScrollUp => (amount, 0),
                    MouseAction::ScrollDown => (-amount, 0),
                    MouseAction::ScrollLeft => (0, -amount),
                    _ => (0, amount),
                };
                let sensitivity = self.scroll_sensitivity.clamped();
                let (vh, vd) = self.synthetic_scroll.vertical.apply(vertical, sensitivity.vertical);
                let (hh, hd) = self.synthetic_scroll.horizontal.apply(horizontal, sensitivity.horizontal);
                if vh != 0 || hh != 0 {
                    out.scroll(vh, vd, hh, hd);
                }
            }
            _ => {}
        }
    }

    pub fn send_stroke(&mut self, stroke: &KeyStroke, out: &mut dyn MapperOutput) {
        if let Some(code) = evdev_code(stroke) {
            self.send_code(code, stroke.mods().minus(Mods::FN), out);
        }
    }

    fn send_code(&mut self, code: u16, mods: Mods, out: &mut dyn MapperOutput) {
        let mods = mods.union(out.modifiers_elsewhere());
        self.press_mods(mods, out);
        let already_held = self.holds.values().any(|h| matches!(h, Hold::Key(c, _) if *c == code));
        if !already_held {
            out.key(code, true);
            out.key(code, false);
        }
        self.release_mods(mods, out);
    }

    /// Presses the modifiers a stroke needs that this mapper does not hold yet.
    fn press_mods(&mut self, mods: Mods, out: &mut dyn MapperOutput) {
        for (flag, code, _) in Mods::ORDER {
            if mods.contains(flag) && !self.synthetic.contains(flag) {
                self.synthetic = self.synthetic.union(flag);
                out.key(code, true);
            }
        }
    }

    /// Releases the modifiers a stroke required, but only the ones this
    /// mapper pressed and no other active hold still needs.
    fn release_mods(&mut self, mods: Mods, out: &mut dyn MapperOutput) {
        let still_needed = self.holds.values().fold(Mods::NONE, |acc, h| match h {
            Hold::Key(_, needed) => acc.union(*needed),
            Hold::Mouse(_) => acc,
        });
        let releasable = mods.intersection(self.synthetic).minus(still_needed);
        for (flag, code, _) in Mods::ORDER.iter().rev() {
            if releasable.contains(*flag) {
                self.synthetic = self.synthetic.minus(*flag);
                out.key(*code, false);
            }
        }
    }

    pub fn run_macro(&mut self, steps: &[MacroStep], index: i32, out: &mut dyn MapperOutput) {
        match self.runner.start(steps, index, out.now()) {
            Ok(()) => self.last_action_error = None,
            Err(error) => self.last_action_error = Some(error.0),
        }
        self.status_changed = true;
    }

    pub fn cancel_macros(&mut self) {
        if self.runner.cancel_all() {
            self.status_changed = true;
        }
        if self.last_action_error.take().is_some() {
            self.status_changed = true;
        }
    }

    pub fn resume_macro(&mut self, token: u64, now: Instant) {
        self.runner.resume(token, now);
    }

    /// Advances due macro steps. Called by the engine between input events.
    pub fn tick_macros(&mut self, out: &mut dyn MapperOutput) {
        if self.runner.is_idle() {
            return;
        }
        let now = out.now();
        let mut runner = std::mem::take(&mut self.runner);
        let finished = runner.tick(now, &mut MacroAdapter { mapper: self, out });
        // A step cannot start another run, so nothing was added meanwhile.
        self.runner = runner;
        if finished {
            self.status_changed = true;
        }
    }
}

struct MacroAdapter<'a> {
    mapper: &'a mut ButtonMapper,
    out: &'a mut dyn MapperOutput,
}

impl MacroOutput for MacroAdapter<'_> {
    fn send_key(&mut self, stroke: &KeyStroke) {
        self.mapper.send_stroke(stroke, self.out);
    }
    fn send_text(&mut self, text: &str) -> TextResult {
        self.out.type_text(text)
    }
}

pub fn evdev_code(stroke: &KeyStroke) -> Option<u16> {
    stroke.effective_code().and_then(keys::mac_to_evdev)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::model::keys::mac;
    use std::time::Duration;

    #[derive(Default)]
    pub struct Recorder {
        pub log: Vec<String>,
        pub elsewhere: Mods,
        pub now: Option<Instant>,
        pub counts: BTreeMap<u16, i32>,
    }

    impl Recorder {
        pub fn take(&mut self) -> Vec<String> {
            std::mem::take(&mut self.log)
        }
    }

    impl MapperOutput for Recorder {
        fn now(&self) -> Instant {
            self.now.unwrap_or_else(Instant::now)
        }
        fn key(&mut self, code: u16, down: bool) {
            *self.counts.entry(code).or_default() += if down { 1 } else { -1 };
            self.log.push(format!("{}{code}", if down { "+" } else { "-" }));
        }
        fn button(&mut self, code: u16, down: bool) {
            self.log.push(format!("{}b{code:x}", if down { "+" } else { "-" }));
        }
        fn scroll(&mut self, v: i32, vd: i32, h: i32, hd: i32) {
            self.log.push(format!("scroll {v} {vd} {h} {hd}"));
        }
        fn modifiers_elsewhere(&mut self) -> Mods {
            self.elsewhere
        }
        fn type_text(&mut self, text: &str) -> TextResult {
            self.log.push(format!("text {text}"));
            TextResult::Sent
        }
        fn plan_system(&mut self, action: SystemAction) -> SystemPlan {
            match action {
                SystemAction::VolumeUp => SystemPlan::Key { code: ev::KEY_VOLUMEUP, mods: Mods::NONE },
                SystemAction::Finder => SystemPlan::Command(vec!["xdg-open".into(), "~".into()]),
                _ => SystemPlan::Unavailable("no".into()),
            }
        }
        fn spawn(&mut self, argv: Vec<String>) {
            self.log.push(format!("spawn {}", argv.join(" ")));
        }
        fn shell(&mut self, command: &str) {
            self.log.push(format!("shell {command}"));
        }
        fn launch(&mut self, path: &str) {
            self.log.push(format!("launch {path}"));
        }
        fn switch_profile(&mut self, name: &str) {
            self.log.push(format!("profile {name}"));
        }
    }

    fn keys(stroke: KeyStroke) -> ActionType {
        ActionType::KeySequence { keys: vec![stroke], description: None }
    }

    fn mapper(entries: Vec<(i32, ActionType)>) -> ButtonMapper {
        let mut mapper = ButtonMapper::default();
        mapper.swap_mapping(entries.into_iter().collect(), Mapping::new());
        mapper
    }

    const C: u16 = 46;

    #[test]
    fn single_key_is_held_with_its_modifiers() {
        let mut out = Recorder::default();
        let mut m = mapper(vec![(1, keys(KeyStroke::new("c", &["ctrl", "shift"], Some(mac::C))))]);
        m.handle_press(1, &mut out);
        assert_eq!(out.take(), vec!["+29", "+42", &format!("+{C}")]);
        m.handle_press(1, &mut out);
        assert!(out.take().is_empty(), "a second press of a held control does nothing");
        m.handle_release(1, &mut out);
        assert_eq!(out.take(), vec![format!("-{C}"), "-42".into(), "-29".into()]);
        assert!(out.counts.values().all(|c| *c == 0));
    }

    #[test]
    fn modifiers_held_elsewhere_are_merged() {
        let mut out = Recorder { elsewhere: Mods::CTRL, ..Default::default() };
        let mut m = mapper(vec![(1, keys(KeyStroke::new("c", &[], Some(mac::C))))]);
        m.handle_press(1, &mut out);
        m.handle_release(1, &mut out);
        assert_eq!(out.take(), vec!["+29".to_string(), format!("+{C}"), format!("-{C}"), "-29".into()]);
    }

    #[test]
    fn overlapping_holds_share_keys_and_modifiers() {
        let mut out = Recorder::default();
        let mut m = mapper(vec![
            (1, keys(KeyStroke::new("c", &["ctrl"], Some(mac::C)))),
            (2, keys(KeyStroke::new("c", &["ctrl"], Some(mac::C)))),
        ]);
        m.handle_press(1, &mut out);
        m.handle_press(2, &mut out);
        m.handle_release(1, &mut out);
        assert_eq!(out.take(), vec!["+29".to_string(), format!("+{C}")], "still held by button 2");
        m.handle_release(2, &mut out);
        assert_eq!(out.take(), vec![format!("-{C}"), "-29".into()]);
    }

    #[test]
    fn sequences_tap_each_key() {
        let mut out = Recorder::default();
        let sequence = ActionType::KeySequence {
            keys: vec![KeyStroke::new("a", &[], Some(mac::A)), KeyStroke::new("tab", &["alt"], Some(mac::TAB))],
            description: None,
        };
        let mut m = mapper(vec![(5, sequence)]);
        m.handle_press(5, &mut out);
        assert_eq!(out.take(), vec!["+30", "-30", "+56", "+15", "-15", "-56"]);
        m.handle_release(5, &mut out);
        assert!(out.take().is_empty());
    }

    #[test]
    fn mouse_buttons_are_held() {
        let mut out = Recorder::default();
        let mut m = mapper(vec![(3, ActionType::Mouse { action: MouseAction::MiddleClick, description: None })]);
        m.handle_press(3, &mut out);
        m.handle_release(3, &mut out);
        assert_eq!(out.take(), vec!["+b112", "-b112"]);
    }

    #[test]
    fn browser_navigation_and_scroll() {
        let mut out = Recorder::default();
        let mut m = mapper(vec![
            (1, ActionType::Mouse { action: MouseAction::BrowserBack, description: None }),
            (2, ActionType::Mouse { action: MouseAction::ScrollLeft, description: None }),
            (3, ActionType::Mouse { action: MouseAction::DpiUp, description: None }),
        ]);
        m.handle(1, &mut out);
        assert_eq!(out.take(), vec!["+56", "+105", "-105", "-56"]);
        m.handle(2, &mut out);
        assert_eq!(out.take(), vec!["scroll 0 0 -360 -3"]);
        assert!(!m.has_mapping(3), "DPI functions exist only in mouse memory");
        m.scroll_sensitivity = ScrollSensitivity { vertical: 1.0, horizontal: 0.5 };
        m.handle(2, &mut out);
        assert_eq!(out.take(), vec!["scroll 0 0 -180 -1"]);
    }

    #[test]
    fn hypershift_layer_applies_while_held() {
        let mut out = Recorder::default();
        let mut m = ButtonMapper::default();
        let mut standard = Mapping::new();
        standard.insert(12, ActionType::Hypershift { description: None });
        standard.insert(1, keys(KeyStroke::new("a", &[], Some(mac::A))));
        let mut shifted = Mapping::new();
        shifted.insert(1, keys(KeyStroke::new("b", &[], Some(mac::B))));
        shifted.insert(12, keys(KeyStroke::new("z", &[], Some(mac::Z))));
        m.update_mapping(standard, shifted, &mut out);
        m.handle(1, &mut out);
        assert_eq!(out.take(), vec!["+30", "-30"]);
        m.handle_press(12, &mut out);
        assert!(m.is_hypershift_active());
        assert!(out.take().is_empty());
        m.handle_press(1, &mut out);
        m.handle_release(12, &mut out);
        assert!(!m.is_hypershift_active());
        m.handle_release(1, &mut out);
        assert_eq!(out.take(), vec!["+48", "-48"], "the output started in the Hypershift layer ends there");
    }

    #[test]
    fn release_all_releases_holds_and_cancels_macros() {
        let mut out = Recorder::default();
        let mut m = mapper(vec![
            (1, keys(KeyStroke::new("c", &["cmd"], Some(mac::C)))),
            (2, ActionType::Macro { steps: vec![MacroStep::delay(1000), MacroStep::text("x")], description: None }),
        ]);
        m.handle_press(1, &mut out);
        m.handle_press(2, &mut out);
        assert_eq!(m.active_macro_buttons(), vec![2]);
        m.release_all(&mut out);
        assert!(m.active_macro_buttons().is_empty());
        assert!(out.counts.values().all(|c| *c == 0), "{:?}", out.counts);
    }

    #[test]
    fn swap_keeps_held_output() {
        let mut out = Recorder::default();
        let mut m = mapper(vec![(1, keys(KeyStroke::new("a", &[], Some(mac::A))))]);
        m.handle_press(1, &mut out);
        m.swap_mapping([(1, keys(KeyStroke::new("b", &[], Some(mac::B))))].into_iter().collect(), Mapping::new());
        m.handle_release(1, &mut out);
        assert_eq!(out.take(), vec!["+30", "-30"]);
        m.handle(1, &mut out);
        assert_eq!(out.take(), vec!["+48", "-48"]);
    }

    #[test]
    fn other_actions_reach_the_output() {
        let mut out = Recorder::default();
        let mut m = mapper(vec![
            (1, ActionType::Application { path: "/app.desktop".into(), description: None }),
            (2, ActionType::SystemCommand { command: "true".into(), description: None }),
            (3, ActionType::ProfileSwitch { profile: "Games".into(), description: None }),
            (4, ActionType::System { action: SystemAction::VolumeUp, description: None }),
            (5, ActionType::Audio { action: AudioAction::VolumeUp, description: None }),
            (6, ActionType::System { action: SystemAction::Finder, description: None }),
            (7, ActionType::Disabled),
        ]);
        for index in 1..=7 {
            m.handle(index, &mut out);
        }
        assert_eq!(
            out.take(),
            vec!["launch /app.desktop", "shell true", "profile Games", "+115", "-115", "+115", "-115", "spawn xdg-open ~"]
        );
        assert_eq!(m.perform_system(SystemAction::DoNotDisturb, &mut out), Some("no".into()));
    }

    #[test]
    fn text_and_macros_run_through_ticks() {
        let t0 = Instant::now();
        let mut out = Recorder { now: Some(t0), ..Default::default() };
        let steps = vec![MacroStep::key(KeyStroke::new("a", &["shift"], Some(mac::A))), MacroStep::delay(50), MacroStep::text("ok")];
        let mut m = mapper(vec![(1, ActionType::Macro { steps, description: None }), (2, ActionType::TextSnippet { text: "hey".into(), description: None })]);
        m.handle(1, &mut out);
        m.handle(2, &mut out);
        assert!(m.status_changed);
        m.tick_macros(&mut out);
        m.tick_macros(&mut out);
        assert_eq!(out.take(), vec!["+42", "+30", "-30", "-42", "text hey"]);
        out.now = Some(t0 + Duration::from_millis(50));
        m.tick_macros(&mut out);
        m.tick_macros(&mut out);
        assert_eq!(out.take(), vec!["text ok"]);
        m.tick_macros(&mut out);
        assert!(m.active_macro_buttons().is_empty());
    }

    #[test]
    fn invalid_macro_reports_an_error() {
        let mut out = Recorder::default();
        let mut m = mapper(vec![(1, ActionType::Macro { steps: vec![MacroStep::delay(-1)], description: None })]);
        m.handle(1, &mut out);
        assert!(m.last_action_error.is_some());
        m.cancel_macros();
        assert!(m.last_action_error.is_none());
    }
}
