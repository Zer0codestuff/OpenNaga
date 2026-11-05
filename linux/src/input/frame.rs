//! What one input frame or driver report does: the decision logic of the
//! engine, free of file descriptors so it can be tested with a recorder.

use super::decode::{self, DriverButtonState};
use super::evdev::InputEvent;
use crate::model::keys::ev;
use crate::model::mapper::{ButtonMapper, MapperOutput};
use crate::model::scroll::{ScrollSensitivity, WheelScaler, HI_RES_PER_DETENT};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Policy {
    /// Perform the mapping (the Remapping switch with a normal profile).
    pub remap: bool,
    /// Forward native keys with modifiers held on other keyboards (mouse
    /// memory and imported profiles, which never remap in software).
    pub merge_native: bool,
}

/// Output beyond the mapper: forwarding of unmapped input.
pub trait FrameSink: MapperOutput {
    fn rel(&mut self, code: u16, value: i32);
    fn native_key(&mut self, code: u16, down: bool, merge: bool);
    fn flush(&mut self);
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    Activity(i32),
    LastInput(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawKind {
    /// Keyboard interface with vendor report 04 (DPI buttons, driver tilt).
    Vendor,
    /// Mouse interface; its reports matter only in driver mode.
    Mouse,
}

/// Per-node state the engine keeps for each input device.
#[derive(Default, Debug)]
pub struct NodeState {
    pub keys_down: BTreeSet<u16>,
    /// Keys and buttons this node forwarded down and has not released.
    pub forwarded: BTreeSet<u16>,
}

#[derive(Default)]
pub struct Remapper {
    pub mapper: ButtonMapper,
    pub policy: Policy,
    pub driver_mode: bool,
    pub scroll: ScrollSensitivity,
    pub wheel: WheelScaler,
    pub active_down: BTreeSet<i32>,
    pub driver_state: DriverButtonState,
}

impl Remapper {
    pub fn release_mapped(&mut self, out: &mut dyn FrameSink) {
        self.active_down.clear();
        self.mapper.release_all(out);
        self.wheel.reset();
        out.flush();
    }

    /// Handles one SYN_REPORT frame from a Naga node. Only a grabbed node's
    /// events are performed or forwarded; any node reports activity.
    pub fn frame(&mut self, events: &[InputEvent], grabbed: bool, node: &mut NodeState, out: &mut dyn FrameSink, notices: &mut Vec<Notice>) {
        let (mut v_detents, mut v_hi, mut v_has_hi) = (0, 0, false);
        let (mut h_detents, mut h_hi, mut h_has_hi) = (0, 0, false);
        for event in events {
            match event.type_ {
                ev::EV_KEY => {
                    if event.value == 2 {
                        continue; // Kernel auto-repeat; desktops repeat on their own.
                    }
                    let down = event.value == 1;
                    let code = event.code;
                    if down {
                        node.keys_down.insert(code);
                    } else {
                        node.keys_down.remove(&code);
                    }
                    let control = decode::control_for_key(code, self.driver_mode);
                    if let Some(c) = control {
                        if down {
                            notices.push(Notice::Activity(c));
                        }
                        notices.push(Notice::LastInput(format!("Button {c}: {}", if down { "pressed" } else { "released" })));
                    }
                    if !grabbed {
                        continue;
                    }
                    if let (true, Some(c)) = (self.policy.remap, control) {
                        // Driver reports alone perform tilt in driver mode.
                        let raw_tilt = self.driver_mode && (15..=16).contains(&c);
                        if down && self.mapper.has_mapping(c) {
                            self.active_down.insert(c);
                            if !raw_tilt {
                                self.mapper.handle_press(c, out);
                            }
                            continue;
                        }
                        if !down && self.active_down.remove(&c) {
                            if !raw_tilt {
                                self.mapper.handle_release(c, out);
                            }
                            continue;
                        }
                    }
                    // A release whose press was never forwarded (the node was
                    // grabbed while it was held) must not reach the desktop.
                    if !down && !node.forwarded.contains(&code) {
                        continue;
                    }
                    let merge = self.policy.remap || self.policy.merge_native;
                    if ev::is_pointer_button(code) {
                        out.button(code, down);
                    } else {
                        out.native_key(code, down, merge);
                    }
                    if down {
                        node.forwarded.insert(code);
                    } else {
                        node.forwarded.remove(&code);
                    }
                }
                ev::EV_REL => match event.code {
                    ev::REL_WHEEL => v_detents += event.value,
                    ev::REL_WHEEL_HI_RES => {
                        v_hi += event.value;
                        v_has_hi = true;
                    }
                    ev::REL_HWHEEL => h_detents += event.value,
                    ev::REL_HWHEEL_HI_RES => {
                        h_hi += event.value;
                        h_has_hi = true;
                    }
                    code if grabbed => out.rel(code, event.value),
                    _ => {}
                },
                _ => {}
            }
        }
        let h_total = if h_has_hi { h_hi } else { h_detents * HI_RES_PER_DETENT };
        // A tilt reports whole detents; high-resolution remainders alone are
        // smooth horizontal scrolling, never a button press.
        let tilt_tick = h_detents != 0;
        if tilt_tick {
            if let Some(c) = decode::control_for_tilt(h_total) {
                notices.push(Notice::Activity(c));
            }
        }
        if grabbed {
            let scale = self.policy.remap;
            if v_detents != 0 || v_hi != 0 {
                if scale && self.scroll.vertical != 1.0 {
                    let total = if v_has_hi { v_hi } else { v_detents * HI_RES_PER_DETENT };
                    let (hi, detents) = self.wheel.vertical.apply(total, self.scroll.vertical);
                    out.rel(ev::REL_WHEEL, detents);
                    out.rel(ev::REL_WHEEL_HI_RES, hi);
                } else {
                    // Exact pass-through at 1x.
                    out.rel(ev::REL_WHEEL, v_detents);
                    out.rel(ev::REL_WHEEL_HI_RES, v_hi);
                }
            }
            if h_total != 0 {
                let control = decode::control_for_tilt(h_total);
                let mapped = scale && control.map(|c| self.mapper.has_mapping(c)).unwrap_or(false);
                if let (true, Some(c)) = (mapped, control) {
                    // The tilt is consumed; each tick performs the action once.
                    if tilt_tick && !self.driver_mode && !self.driver_state.held().contains(&c) {
                        self.mapper.handle(c, out);
                    }
                } else if scale && self.scroll.horizontal != 1.0 {
                    let (hi, detents) = self.wheel.horizontal.apply(h_total, self.scroll.horizontal);
                    out.rel(ev::REL_HWHEEL, detents);
                    out.rel(ev::REL_HWHEEL_HI_RES, hi);
                } else {
                    out.rel(ev::REL_HWHEEL, h_detents);
                    out.rel(ev::REL_HWHEEL_HI_RES, h_hi);
                }
            }
        }
        out.flush();
    }

    /// Handles a hidraw input report. `active` is true while remapping runs.
    pub fn report(&mut self, report: &[u8], kind: RawKind, source: &str, active: bool, out: &mut dyn FrameSink, notices: &mut Vec<Notice>) {
        let decoded = match kind {
            RawKind::Vendor => decode::driver_buttons(report).map(|set| {
                if self.driver_mode {
                    set
                } else {
                    // In normal mode only the DPI buttons are reported here.
                    set.into_iter().filter(|c| *c == 13 || *c == 14).collect()
                }
            }),
            RawKind::Mouse if self.driver_mode => decode::driver_mouse_buttons(report),
            RawKind::Mouse => None,
        };
        let Some(buttons) = decoded else { return };
        let (pressed, released) = self.driver_state.update(source, buttons);
        for button in released {
            notices.push(Notice::LastInput(format!("Button {button}: released")));
            if active {
                self.mapper.handle_release(button, out);
            }
        }
        for button in pressed {
            notices.push(Notice::Activity(button));
            notices.push(Notice::LastInput(format!("Button {button}: recognized driver report")));
            if active {
                self.mapper.handle_press(button, out);
            }
        }
        out.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::evdev::event;
    use crate::model::actions::*;
    use crate::model::keys::mac;
    use crate::model::mapper::tests::Recorder;
    use crate::model::profile::Mapping;

    impl FrameSink for Recorder {
        fn rel(&mut self, code: u16, value: i32) {
            if value != 0 {
                self.log.push(format!("rel {code} {value}"));
            }
        }
        fn native_key(&mut self, code: u16, down: bool, merge: bool) {
            self.log.push(format!("native {}{code}{}", if down { "+" } else { "-" }, if merge { " merge" } else { "" }));
        }
        fn flush(&mut self) {}
    }

    fn key(code: u16, value: i32) -> InputEvent {
        event(ev::EV_KEY, code, value)
    }
    fn rel(code: u16, value: i32) -> InputEvent {
        event(ev::EV_REL, code, value)
    }

    fn remapper(entries: Vec<(i32, ActionType)>) -> Remapper {
        let mut r = Remapper { policy: Policy { remap: true, merge_native: false }, scroll: ScrollSensitivity::BASELINE, ..Default::default() };
        let mut out = Recorder::default();
        r.mapper.update_mapping(entries.into_iter().collect(), Mapping::new(), &mut out);
        r
    }

    fn copy() -> ActionType {
        ActionType::KeySequence { keys: vec![KeyStroke::new("c", &["ctrl"], Some(mac::C))], description: None }
    }

    #[test]
    fn mapped_side_key_is_replaced_and_unmapped_is_forwarded() {
        let mut r = remapper(vec![(1, copy())]);
        let (mut out, mut node, mut notices) = (Recorder::default(), NodeState::default(), Vec::new());
        r.frame(&[key(2, 1)], true, &mut node, &mut out, &mut notices);
        r.frame(&[key(2, 2)], true, &mut node, &mut out, &mut notices);
        r.frame(&[key(2, 0)], true, &mut node, &mut out, &mut notices);
        assert_eq!(out.take(), vec!["+29", "+46", "-46", "-29"]);
        r.frame(&[key(3, 1)], true, &mut node, &mut out, &mut notices);
        r.frame(&[key(3, 0)], true, &mut node, &mut out, &mut notices);
        assert_eq!(out.take(), vec!["native +3 merge", "native -3 merge"]);
        assert!(notices.contains(&Notice::Activity(1)) && notices.contains(&Notice::Activity(2)));
        assert!(node.keys_down.is_empty() && node.forwarded.is_empty());
    }

    #[test]
    fn ungrabbed_nodes_only_report_activity() {
        let mut r = remapper(vec![(1, copy())]);
        let (mut out, mut node, mut notices) = (Recorder::default(), NodeState::default(), Vec::new());
        r.frame(&[key(2, 1), rel(ev::REL_X, 4), rel(ev::REL_WHEEL, 1)], false, &mut node, &mut out, &mut notices);
        assert!(out.take().is_empty());
        assert_eq!(notices[0], Notice::Activity(1));
    }

    #[test]
    fn release_held_before_the_grab_is_dropped() {
        let mut r = remapper(vec![]);
        let (mut out, mut node, mut notices) = (Recorder::default(), NodeState::default(), Vec::new());
        r.frame(&[key(ev::BTN_LEFT, 0)], true, &mut node, &mut out, &mut notices);
        assert!(out.take().is_empty());
    }

    #[test]
    fn pointer_motion_and_buttons_pass_through() {
        let mut r = remapper(vec![]);
        let (mut out, mut node, mut notices) = (Recorder::default(), NodeState::default(), Vec::new());
        r.frame(&[rel(ev::REL_X, 3), rel(ev::REL_Y, -2), key(ev::BTN_LEFT, 1)], true, &mut node, &mut out, &mut notices);
        r.frame(&[rel(ev::REL_WHEEL, 1), rel(ev::REL_WHEEL_HI_RES, 120)], true, &mut node, &mut out, &mut notices);
        r.frame(&[key(ev::BTN_LEFT, 0)], true, &mut node, &mut out, &mut notices);
        assert_eq!(out.take(), vec!["rel 0 3", "rel 1 -2", "+b110", "rel 8 1", "rel 11 120", "-b110"]);
    }

    #[test]
    fn scroll_is_scaled_when_remapping() {
        let mut r = remapper(vec![]);
        r.scroll = ScrollSensitivity { vertical: 0.5, horizontal: 2.0 };
        let (mut out, mut node, mut notices) = (Recorder::default(), NodeState::default(), Vec::new());
        r.frame(&[rel(ev::REL_WHEEL, 1), rel(ev::REL_WHEEL_HI_RES, 120)], true, &mut node, &mut out, &mut notices);
        r.frame(&[rel(ev::REL_WHEEL, 1), rel(ev::REL_WHEEL_HI_RES, 120)], true, &mut node, &mut out, &mut notices);
        assert_eq!(out.take(), vec!["rel 11 60", "rel 8 1", "rel 11 60"]);
        r.frame(&[rel(ev::REL_HWHEEL, -1)], true, &mut node, &mut out, &mut notices);
        assert_eq!(out.take(), vec!["rel 6 -2", "rel 12 -240"], "detent-only devices are scaled too");
    }

    #[test]
    fn mapped_tilt_is_consumed_and_performed_per_tick() {
        let mut r = remapper(vec![(15, ActionType::Mouse { action: MouseAction::BrowserBack, description: None })]);
        let (mut out, mut node, mut notices) = (Recorder::default(), NodeState::default(), Vec::new());
        r.frame(&[rel(ev::REL_HWHEEL, -1), rel(ev::REL_HWHEEL_HI_RES, -120)], true, &mut node, &mut out, &mut notices);
        assert_eq!(out.take(), vec!["+56", "+105", "-105", "-56"]);
        r.frame(&[rel(ev::REL_HWHEEL, 1), rel(ev::REL_HWHEEL_HI_RES, 120)], true, &mut node, &mut out, &mut notices);
        assert_eq!(out.take(), vec!["rel 6 1", "rel 12 120"], "the unmapped direction still scrolls");
        r.driver_mode = true;
        r.frame(&[rel(ev::REL_HWHEEL, -1)], true, &mut node, &mut out, &mut notices);
        assert!(out.take().is_empty(), "driver reports perform tilt in driver mode");
    }

    #[test]
    fn hypershift_through_frames() {
        let mut r = Remapper { policy: Policy { remap: true, merge_native: false }, ..Default::default() };
        let mut standard = Mapping::new();
        standard.insert(12, ActionType::Hypershift { description: None });
        let mut shifted = Mapping::new();
        shifted.insert(18, ActionType::Mouse { action: MouseAction::RightClick, description: None });
        let (mut out, mut node, mut notices) = (Recorder::default(), NodeState::default(), Vec::new());
        r.mapper.update_mapping(standard, shifted, &mut out);
        r.frame(&[key(ev::BTN_LEFT, 1)], true, &mut node, &mut out, &mut notices);
        r.frame(&[key(ev::BTN_LEFT, 0)], true, &mut node, &mut out, &mut notices);
        assert_eq!(out.take(), vec!["+b110", "-b110"], "left click is unmapped in the standard layer");
        r.frame(&[key(ev::KEY_EQUAL, 1)], true, &mut node, &mut out, &mut notices);
        r.frame(&[key(ev::BTN_LEFT, 1)], true, &mut node, &mut out, &mut notices);
        r.frame(&[key(ev::KEY_EQUAL, 0)], true, &mut node, &mut out, &mut notices);
        r.frame(&[key(ev::BTN_LEFT, 0)], true, &mut node, &mut out, &mut notices);
        assert_eq!(out.take(), vec!["+b111", "-b111"], "held Hypershift turns the left click into a right click");
    }

    #[test]
    fn merging_follows_the_policy() {
        let mut r = Remapper { policy: Policy { remap: false, merge_native: true }, ..Default::default() };
        let (mut out, mut node, mut notices) = (Recorder::default(), NodeState::default(), Vec::new());
        r.frame(&[key(30, 1), key(30, 0)], true, &mut node, &mut out, &mut notices);
        assert_eq!(out.take(), vec!["native +30 merge", "native -30 merge"]);
    }

    #[test]
    fn driver_reports() {
        let mut r = remapper(vec![(13, copy()), (15, copy())]);
        let (mut out, mut notices) = (Recorder::default(), Vec::new());
        let mut report = vec![0u8; 16];
        report[0] = 4;
        report[1] = 0x20;
        report[2] = 0x22;
        r.report(&report, RawKind::Vendor, "a", true, &mut out, &mut notices);
        assert_eq!(out.take(), vec!["+29", "+46"], "normal mode reports only DPI buttons");
        r.report(&[4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], RawKind::Vendor, "a", true, &mut out, &mut notices);
        assert_eq!(out.take(), vec!["-46", "-29"]);
        r.report(&[0x20, 0, 0, 0, 0, 0, 0, 0], RawKind::Mouse, "b", true, &mut out, &mut notices);
        assert!(out.take().is_empty(), "mouse reports matter only in driver mode");
        r.driver_mode = true;
        r.report(&[0x20, 0, 0, 0, 0, 0, 0, 0], RawKind::Mouse, "b", true, &mut out, &mut notices);
        assert_eq!(out.take(), vec!["+29", "+46"]);
        r.report(&[0x20, 0, 0, 0, 0, 0, 0, 0], RawKind::Mouse, "b", false, &mut out, &mut notices);
        assert!(out.take().is_empty(), "a held control does not repeat");
    }
}
