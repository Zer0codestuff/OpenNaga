//! Pure decoders from Naga input to logical controls.
//! 1 to 12 side grid, 13 DPI up, 14 DPI down, 15 tilt left, 16 tilt right,
//! 17 middle, 18 left, 19 right. evdev codes are layout-independent, so the
//! side grid is identified by scan position, not by the character it types.

use crate::model::keys::ev;
use std::collections::{BTreeMap, BTreeSet};

pub fn control_for_key(code: u16, driver_mode: bool) -> Option<i32> {
    match code {
        ev::KEY_1..=ev::KEY_0 => Some((code - ev::KEY_1) as i32 + 1),
        ev::KEY_MINUS => Some(11),
        ev::KEY_EQUAL => Some(12),
        ev::BTN_LEFT => Some(18),
        ev::BTN_RIGHT => Some(19),
        ev::BTN_MIDDLE => Some(17),
        // In driver mode the tilt arrives as HID buttons 6 and 7.
        ev::BTN_FORWARD if driver_mode => Some(15),
        ev::BTN_BACK if driver_mode => Some(16),
        _ => None,
    }
}

/// Wheel tilt from a horizontal wheel value: positive means right.
pub fn control_for_tilt(value: i32) -> Option<i32> {
    match value {
        v if v < 0 => Some(15),
        v if v > 0 => Some(16),
        _ => None,
    }
}

/// Published protocol facts, not a hardware capture:
/// https://github.com/openrazer/openrazer/pull/2850
/// The keyboard-interface driver report is exactly 16 bytes, report ID 04,
/// followed by the set of held codes.
pub fn driver_buttons(report: &[u8]) -> Option<BTreeSet<i32>> {
    if report.len() != 16 || report[0] != 0x04 {
        return None;
    }
    Some(
        report[1..]
            .iter()
            .filter_map(|b| match b {
                0x20 => Some(13),
                0x21 => Some(14),
                0x22 => Some(15),
                0x23 => Some(16),
                _ => None,
            })
            .collect(),
    )
}

/// The PR 2850 mouse report uses bits 5 and 6 of the first byte for tilt.
/// Only for the receiver's mouse interface in confirmed driver mode.
pub fn driver_mouse_buttons(report: &[u8]) -> Option<BTreeSet<i32>> {
    if report.len() != 8 {
        return None;
    }
    let mut result = BTreeSet::new();
    if report[0] & 0x20 != 0 {
        result.insert(15);
    }
    if report[0] & 0x40 != 0 {
        result.insert(16);
    }
    Some(result)
}

/// Held controls from several report sources.
#[derive(Default, Debug)]
pub struct DriverButtonState {
    sources: BTreeMap<String, BTreeSet<i32>>,
}

impl DriverButtonState {
    pub fn held(&self) -> BTreeSet<i32> {
        self.sources.values().flatten().copied().collect()
    }

    pub fn update(&mut self, source: &str, buttons: BTreeSet<i32>) -> (BTreeSet<i32>, BTreeSet<i32>) {
        let previous = self.held();
        self.sources.insert(source.to_string(), buttons);
        let now = self.held();
        (now.difference(&previous).copied().collect(), previous.difference(&now).copied().collect())
    }

    pub fn reset(&mut self) {
        self.sources.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_grid_and_buttons() {
        assert_eq!(control_for_key(2, false), Some(1));
        assert_eq!(control_for_key(11, false), Some(10));
        assert_eq!(control_for_key(12, false), Some(11));
        assert_eq!(control_for_key(13, false), Some(12));
        assert_eq!(control_for_key(ev::BTN_LEFT, false), Some(18));
        assert_eq!(control_for_key(ev::BTN_MIDDLE, false), Some(17));
        assert_eq!(control_for_key(ev::BTN_SIDE, false), None, "buttons 4/5 have no verified identity");
        assert_eq!(control_for_key(ev::BTN_FORWARD, false), None);
        assert_eq!(control_for_key(ev::BTN_FORWARD, true), Some(15));
        assert_eq!(control_for_key(30, false), None);
        assert_eq!(control_for_tilt(-1), Some(15));
        assert_eq!(control_for_tilt(120), Some(16));
    }

    #[test]
    fn driver_reports() {
        let mut report = vec![0u8; 16];
        report[0] = 4;
        report[1] = 0x20;
        report[2] = 0x23;
        assert_eq!(driver_buttons(&report), Some([13, 16].into_iter().collect()));
        report[0] = 5;
        assert_eq!(driver_buttons(&report), None);
        assert_eq!(driver_buttons(&[4; 15]), None);
        assert_eq!(driver_mouse_buttons(&[0x60, 0, 0, 0, 0, 0, 0, 0]), Some([15, 16].into_iter().collect()));
        assert_eq!(driver_mouse_buttons(&[0; 9]), None);
    }

    #[test]
    fn driver_state_merges_sources() {
        let mut state = DriverButtonState::default();
        let (pressed, released) = state.update("a", [13].into_iter().collect());
        assert_eq!((pressed.len(), released.len()), (1, 0));
        let (pressed, _) = state.update("b", [13, 15].into_iter().collect());
        assert_eq!(pressed, [15].into_iter().collect());
        let (_, released) = state.update("a", BTreeSet::new());
        assert!(released.is_empty(), "still held by b");
        let (_, released) = state.update("b", BTreeSet::new());
        assert_eq!(released, [13, 15].into_iter().collect());
    }
}
