//! Keyboard identities.
//!
//! Profiles store the macOS virtual key code of a key (`keyCode`), exactly as
//! the macOS app does, so a profile file works on both systems. On Linux the
//! code is translated to a USB HID usage and then to an evdev code at output
//! time. Modifier names are shared too: `cmd` is the Super (logo) key here.

use std::collections::HashMap;
use std::sync::OnceLock;

/// macOS virtual key codes (Carbon `kVK_*`).
pub mod mac {
    pub const A: u16 = 0x00;
    pub const S: u16 = 0x01;
    pub const D: u16 = 0x02;
    pub const F: u16 = 0x03;
    pub const H: u16 = 0x04;
    pub const G: u16 = 0x05;
    pub const Z: u16 = 0x06;
    pub const X: u16 = 0x07;
    pub const C: u16 = 0x08;
    pub const V: u16 = 0x09;
    pub const ISO_SECTION: u16 = 0x0A;
    pub const B: u16 = 0x0B;
    pub const Q: u16 = 0x0C;
    pub const W: u16 = 0x0D;
    pub const E: u16 = 0x0E;
    pub const R: u16 = 0x0F;
    pub const Y: u16 = 0x10;
    pub const T: u16 = 0x11;
    pub const N1: u16 = 0x12;
    pub const N2: u16 = 0x13;
    pub const N3: u16 = 0x14;
    pub const N4: u16 = 0x15;
    pub const N6: u16 = 0x16;
    pub const N5: u16 = 0x17;
    pub const EQUAL: u16 = 0x18;
    pub const N9: u16 = 0x19;
    pub const N7: u16 = 0x1A;
    pub const MINUS: u16 = 0x1B;
    pub const N8: u16 = 0x1C;
    pub const N0: u16 = 0x1D;
    pub const RIGHT_BRACKET: u16 = 0x1E;
    pub const O: u16 = 0x1F;
    pub const U: u16 = 0x20;
    pub const LEFT_BRACKET: u16 = 0x21;
    pub const I: u16 = 0x22;
    pub const P: u16 = 0x23;
    pub const RETURN: u16 = 0x24;
    pub const L: u16 = 0x25;
    pub const J: u16 = 0x26;
    pub const QUOTE: u16 = 0x27;
    pub const K: u16 = 0x28;
    pub const SEMICOLON: u16 = 0x29;
    pub const BACKSLASH: u16 = 0x2A;
    pub const COMMA: u16 = 0x2B;
    pub const SLASH: u16 = 0x2C;
    pub const N: u16 = 0x2D;
    pub const M: u16 = 0x2E;
    pub const PERIOD: u16 = 0x2F;
    pub const TAB: u16 = 0x30;
    pub const SPACE: u16 = 0x31;
    pub const GRAVE: u16 = 0x32;
    pub const DELETE: u16 = 0x33;
    pub const ESCAPE: u16 = 0x35;
    pub const CAPS_LOCK: u16 = 0x39;
    pub const F17: u16 = 0x40;
    pub const KP_DECIMAL: u16 = 0x41;
    pub const KP_MULTIPLY: u16 = 0x43;
    pub const KP_PLUS: u16 = 0x45;
    pub const KP_CLEAR: u16 = 0x47;
    pub const KP_DIVIDE: u16 = 0x4B;
    pub const KP_ENTER: u16 = 0x4C;
    pub const KP_MINUS: u16 = 0x4E;
    pub const F18: u16 = 0x4F;
    pub const F19: u16 = 0x50;
    pub const KP_EQUALS: u16 = 0x51;
    pub const KP0: u16 = 0x52;
    pub const KP1: u16 = 0x53;
    pub const KP2: u16 = 0x54;
    pub const KP3: u16 = 0x55;
    pub const KP4: u16 = 0x56;
    pub const KP5: u16 = 0x57;
    pub const KP6: u16 = 0x58;
    pub const KP7: u16 = 0x59;
    pub const F20: u16 = 0x5A;
    pub const KP8: u16 = 0x5B;
    pub const KP9: u16 = 0x5C;
    pub const F5: u16 = 0x60;
    pub const F6: u16 = 0x61;
    pub const F7: u16 = 0x62;
    pub const F3: u16 = 0x63;
    pub const F8: u16 = 0x64;
    pub const F9: u16 = 0x65;
    pub const F11: u16 = 0x67;
    pub const F13: u16 = 0x69;
    pub const F16: u16 = 0x6A;
    pub const F14: u16 = 0x6B;
    pub const F10: u16 = 0x6D;
    pub const F12: u16 = 0x6F;
    pub const F15: u16 = 0x71;
    pub const HELP: u16 = 0x72;
    pub const HOME: u16 = 0x73;
    pub const PAGE_UP: u16 = 0x74;
    pub const FORWARD_DELETE: u16 = 0x75;
    pub const F4: u16 = 0x76;
    pub const END: u16 = 0x77;
    pub const F2: u16 = 0x78;
    pub const PAGE_DOWN: u16 = 0x79;
    pub const F1: u16 = 0x7A;
    pub const LEFT: u16 = 0x7B;
    pub const RIGHT: u16 = 0x7C;
    pub const DOWN: u16 = 0x7D;
    pub const UP: u16 = 0x7E;
}

/// Linux input event codes used by OpenNaga (linux/input-event-codes.h).
pub mod ev {
    pub const KEY_ESC: u16 = 1;
    pub const KEY_1: u16 = 2;
    pub const KEY_0: u16 = 11;
    pub const KEY_MINUS: u16 = 12;
    pub const KEY_EQUAL: u16 = 13;
    pub const KEY_LEFTCTRL: u16 = 29;
    pub const KEY_LEFTSHIFT: u16 = 42;
    pub const KEY_RIGHTSHIFT: u16 = 54;
    pub const KEY_LEFTALT: u16 = 56;
    pub const KEY_RIGHTCTRL: u16 = 97;
    pub const KEY_SYSRQ: u16 = 99;
    pub const KEY_RIGHTALT: u16 = 100;
    pub const KEY_LEFT: u16 = 105;
    pub const KEY_RIGHT: u16 = 106;
    pub const KEY_PAGEUP: u16 = 104;
    pub const KEY_PAGEDOWN: u16 = 109;
    pub const KEY_MUTE: u16 = 113;
    pub const KEY_VOLUMEDOWN: u16 = 114;
    pub const KEY_VOLUMEUP: u16 = 115;
    pub const KEY_LEFTMETA: u16 = 125;
    pub const KEY_RIGHTMETA: u16 = 126;
    pub const KEY_TAB: u16 = 15;
    pub const KEY_D: u16 = 32;
    pub const KEY_H: u16 = 35;
    pub const KEY_V: u16 = 47;
    pub const KEY_NEXTSONG: u16 = 163;
    pub const KEY_PLAYPAUSE: u16 = 164;
    pub const KEY_PREVIOUSSONG: u16 = 165;
    pub const KEY_BRIGHTNESSDOWN: u16 = 224;
    pub const KEY_BRIGHTNESSUP: u16 = 225;
    pub const BTN_LEFT: u16 = 0x110;
    pub const BTN_RIGHT: u16 = 0x111;
    pub const BTN_MIDDLE: u16 = 0x112;
    pub const BTN_SIDE: u16 = 0x113;
    pub const BTN_EXTRA: u16 = 0x114;
    pub const BTN_FORWARD: u16 = 0x115;
    pub const BTN_BACK: u16 = 0x116;
    pub const BTN_TASK: u16 = 0x117;
    pub const BTN_MISC: u16 = 0x100;
    pub const KEY_OK: u16 = 0x160;
    pub const KEY_MAX: u16 = 0x2ff;

    pub const REL_X: u16 = 0x00;
    pub const REL_Y: u16 = 0x01;
    pub const REL_HWHEEL: u16 = 0x06;
    pub const REL_WHEEL: u16 = 0x08;
    pub const REL_WHEEL_HI_RES: u16 = 0x0b;
    pub const REL_HWHEEL_HI_RES: u16 = 0x0c;

    pub const EV_SYN: u16 = 0x00;
    pub const EV_KEY: u16 = 0x01;
    pub const EV_REL: u16 = 0x02;
    pub const SYN_REPORT: u16 = 0;
    pub const SYN_DROPPED: u16 = 3;

    pub fn is_pointer_button(code: u16) -> bool {
        (BTN_MISC..KEY_OK).contains(&code)
    }
}

/// Modifier bit set shared by strokes, holds and the output devices.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Mods(pub u8);

impl Mods {
    pub const CTRL: Mods = Mods(1);
    pub const SHIFT: Mods = Mods(2);
    pub const ALT: Mods = Mods(4);
    pub const SUPER: Mods = Mods(8);
    pub const FN: Mods = Mods(16);
    pub const NONE: Mods = Mods(0);

    /// Press order. Releases run in reverse.
    pub const ORDER: [(Mods, u16, &'static str); 4] = [
        (Mods::SUPER, ev::KEY_LEFTMETA, "cmd"),
        (Mods::CTRL, ev::KEY_LEFTCTRL, "ctrl"),
        (Mods::SHIFT, ev::KEY_LEFTSHIFT, "shift"),
        (Mods::ALT, ev::KEY_LEFTALT, "alt"),
    ];

    pub fn contains(self, other: Mods) -> bool { self.0 & other.0 == other.0 }
    pub fn is_empty(self) -> bool { self.0 == 0 }
    pub fn union(self, other: Mods) -> Mods { Mods(self.0 | other.0) }
    pub fn intersection(self, other: Mods) -> Mods { Mods(self.0 & other.0) }
    pub fn minus(self, other: Mods) -> Mods { Mods(self.0 & !other.0) }

    pub fn from_names<S: AsRef<str>>(names: &[S]) -> Mods {
        let mut mods = Mods::NONE;
        for name in names {
            mods = mods.union(match name.as_ref().to_lowercase().as_str() {
                "cmd" | "command" | "super" | "meta" => Mods::SUPER,
                "shift" => Mods::SHIFT,
                "alt" | "option" => Mods::ALT,
                "ctrl" | "control" => Mods::CTRL,
                "fn" => Mods::FN,
                _ => Mods::NONE,
            });
        }
        mods
    }

    /// The modifier a left or right evdev modifier key belongs to.
    pub fn for_key(code: u16) -> Mods {
        match code {
            ev::KEY_LEFTCTRL | ev::KEY_RIGHTCTRL => Mods::CTRL,
            ev::KEY_LEFTSHIFT | ev::KEY_RIGHTSHIFT => Mods::SHIFT,
            ev::KEY_LEFTALT | ev::KEY_RIGHTALT => Mods::ALT,
            ev::KEY_LEFTMETA | ev::KEY_RIGHTMETA => Mods::SUPER,
            _ => Mods::NONE,
        }
    }
}

/// USB HID keyboard usages paired with macOS virtual key positions. Same
/// table as the macOS app's `OnboardProfilePlan.keyboardUsages`.
pub fn mac_to_usage_table() -> &'static HashMap<u16, u8> {
    static TABLE: OnceLock<HashMap<u16, u8>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut result = HashMap::new();
        let mut add = |codes: &[u16], start: u8| {
            for (offset, code) in codes.iter().enumerate() {
                result.insert(*code, start + offset as u8);
            }
        };
        add(&[0, 11, 8, 2, 14, 3, 5, 4, 34, 38, 40, 37, 46, 45, 31, 35, 12, 15, 1, 17, 32, 9, 13, 7, 16, 6], 4);
        add(&[18, 19, 20, 21, 23, 22, 26, 28, 25, 29], 0x1e);
        add(&[36, 53, 51, 48, 49, 27, 24, 33, 30, 42], 0x28);
        add(&[41, 39, 50, 43, 47, 44, 57], 0x33);
        add(&[122, 120, 99, 118, 96, 97, 98, 100, 101, 109, 103, 111], 0x3a);
        add(&[114, 115, 116, 117, 119, 121, 124, 123, 125, 126], 0x49);
        add(&[75, 67, 78, 69, 76, 83, 84, 85, 86, 87, 88, 89, 91, 92, 82, 65], 0x54);
        add(&[105, 107, 113, 106, 64, 79, 80, 90], 0x68);
        result.insert(10, 0x64);
        result.insert(81, 0x67);
        result
    })
}

pub fn mac_to_usage(code: u16) -> Option<u8> {
    mac_to_usage_table().get(&code).copied()
}

/// Reverse of `mac_to_usage`. Where two codes share a usage the lower wins,
/// like the macOS app.
pub fn usage_to_mac(usage: u8) -> Option<u16> {
    static TABLE: OnceLock<HashMap<u8, u16>> = OnceLock::new();
    TABLE
        .get_or_init(|| {
            let mut map: HashMap<u8, u16> = HashMap::new();
            for (code, usage) in mac_to_usage_table() {
                map.entry(*usage).and_modify(|c| *c = (*c).min(*code)).or_insert(*code);
            }
            map
        })
        .get(&usage)
        .copied()
}

/// USB HID keyboard usage to Linux evdev code (the kernel's hid-input table
/// for the usages OpenNaga can produce).
pub fn usage_to_evdev(usage: u8) -> Option<u16> {
    const LETTERS: [u16; 26] = [30, 48, 46, 32, 18, 33, 34, 35, 23, 36, 37, 38, 50, 49, 24, 25, 16, 19, 31, 20, 22, 47, 17, 45, 21, 44];
    Some(match usage {
        0x04..=0x1d => LETTERS[(usage - 0x04) as usize],
        0x1e..=0x27 => 2 + (usage - 0x1e) as u16,
        0x28 => 28,
        0x29 => 1,
        0x2a => 14,
        0x2b => 15,
        0x2c => 57,
        0x2d => 12,
        0x2e => 13,
        0x2f => 26,
        0x30 => 27,
        0x31 | 0x32 => 43,
        0x33 => 39,
        0x34 => 40,
        0x35 => 41,
        0x36 => 51,
        0x37 => 52,
        0x38 => 53,
        0x39 => 58,
        0x3a..=0x43 => 59 + (usage - 0x3a) as u16,
        0x44 => 87,
        0x45 => 88,
        0x46 => 99,
        0x47 => 70,
        0x48 => 119,
        0x49 => 110,
        0x4a => 102,
        0x4b => 104,
        0x4c => 111,
        0x4d => 107,
        0x4e => 109,
        0x4f => 106,
        0x50 => 105,
        0x51 => 108,
        0x52 => 103,
        0x53 => 69,
        0x54 => 98,
        0x55 => 55,
        0x56 => 74,
        0x57 => 78,
        0x58 => 96,
        0x59 => 79,
        0x5a => 80,
        0x5b => 81,
        0x5c => 75,
        0x5d => 76,
        0x5e => 77,
        0x5f => 71,
        0x60 => 72,
        0x61 => 73,
        0x62 => 82,
        0x63 => 83,
        0x64 => 86,
        0x65 => 127,
        0x67 => 117,
        0x68..=0x73 => 183 + (usage - 0x68) as u16,
        0xe0 => 29,
        0xe1 => 42,
        0xe2 => 56,
        0xe3 => 125,
        0xe4 => 97,
        0xe5 => 54,
        0xe6 => 100,
        0xe7 => 126,
        _ => return None,
    })
}

/// macOS key code to evdev code. Keypad Clear has no HID usage in the macOS
/// table and maps to Num Lock, the key in its position.
pub fn mac_to_evdev(code: u16) -> Option<u16> {
    if code == mac::KP_CLEAR {
        return Some(69);
    }
    mac_to_usage(code).and_then(usage_to_evdev)
}

/// evdev code back to the macOS key code a profile stores.
pub fn evdev_to_mac(code: u16) -> Option<u16> {
    static TABLE: OnceLock<HashMap<u16, u16>> = OnceLock::new();
    TABLE
        .get_or_init(|| {
            let mut map: HashMap<u16, u16> = HashMap::new();
            for mac in 0u16..128 {
                if let Some(evdev) = mac_to_evdev(mac) {
                    map.entry(evdev).and_modify(|c| *c = (*c).min(mac)).or_insert(mac);
                }
            }
            map
        })
        .get(&code)
        .copied()
}

struct KeyNames {
    codes: HashMap<String, u16>,
    primary: HashMap<u16, String>,
}

fn key_names() -> &'static KeyNames {
    static NAMES: OnceLock<KeyNames> = OnceLock::new();
    NAMES.get_or_init(|| {
        use mac::*;
        let mut names = KeyNames { codes: HashMap::new(), primary: HashMap::new() };
        // The first name listed for a code is the one a stroke stores.
        let mut add = |list: &[&str], code: u16| {
            for name in list {
                names.codes.insert(name.to_string(), code);
            }
            names.primary.entry(code).or_insert_with(|| list[0].to_string());
        };
        let letters = [
            ("a", A), ("b", B), ("c", C), ("d", D), ("e", E), ("f", F), ("g", G), ("h", H), ("i", I),
            ("j", J), ("k", K), ("l", L), ("m", M), ("n", N), ("o", O), ("p", P), ("q", Q), ("r", R),
            ("s", S), ("t", T), ("u", U), ("v", V), ("w", W), ("x", X), ("y", Y), ("z", Z),
        ];
        for (name, code) in letters {
            add(&[name], code);
        }
        for (name, code) in [("1", N1), ("2", N2), ("3", N3), ("4", N4), ("5", N5), ("6", N6), ("7", N7), ("8", N8), ("9", N9), ("0", N0)] {
            add(&[name], code);
        }
        add(&["minus", "-"], MINUS);
        add(&["equals", "=", "equal"], EQUAL);
        add(&["left bracket", "["], LEFT_BRACKET);
        add(&["right bracket", "]"], RIGHT_BRACKET);
        add(&["backslash", "\\"], BACKSLASH);
        add(&["semicolon", ";"], SEMICOLON);
        add(&["quote", "'", "apostrophe"], QUOTE);
        add(&["comma", ","], COMMA);
        add(&["period", "."], PERIOD);
        add(&["slash", "/"], SLASH);
        add(&["grave", "`", "tilde"], GRAVE);
        add(&["space"], SPACE);
        add(&["return"], RETURN);
        add(&["enter", "keypad enter"], KP_ENTER);
        add(&["tab"], TAB);
        add(&["escape", "esc"], ESCAPE);
        add(&["delete", "backspace"], DELETE);
        add(&["forward delete", "fn delete", "del"], FORWARD_DELETE);
        add(&["caps lock", "capslock"], CAPS_LOCK);
        add(&["help", "insert"], HELP);
        add(&["home"], HOME);
        add(&["end"], END);
        add(&["page up"], PAGE_UP);
        add(&["page down"], PAGE_DOWN);
        add(&["left arrow", "left"], LEFT);
        add(&["right arrow", "right"], RIGHT);
        add(&["up arrow", "up"], UP);
        add(&["down arrow", "down"], DOWN);
        let functions = [F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12, F13, F14, F15, F16, F17, F18, F19, F20];
        for (index, code) in functions.iter().enumerate() {
            let name = format!("f{}", index + 1);
            add(&[name.as_str()], *code);
        }
        let keypad = [KP0, KP1, KP2, KP3, KP4, KP5, KP6, KP7, KP8, KP9];
        for (index, code) in keypad.iter().enumerate() {
            let short = format!("kp{index}");
            let long = format!("keypad {index}");
            add(&[short.as_str(), long.as_str()], *code);
        }
        add(&["kp."], KP_DECIMAL);
        add(&["kp*"], KP_MULTIPLY);
        add(&["kp+"], KP_PLUS);
        add(&["kp-"], KP_MINUS);
        add(&["kp/"], KP_DIVIDE);
        add(&["kp="], KP_EQUALS);
        names
    })
}

fn canonical_key_codes() -> &'static HashMap<String, u16> {
    &key_names().codes
}

pub fn normalize_identifier(identifier: &str) -> String {
    identifier.trim().to_lowercase()
}

pub fn key_code_for(key: &str) -> Option<u16> {
    canonical_key_codes().get(&normalize_identifier(key)).copied()
}

pub fn primary_name(code: u16) -> Option<&'static str> {
    key_names().primary.get(&code).map(|s| s.as_str())
}

/// Linux-style names for keys without a printable label.
pub fn special_key_name(code: u16) -> Option<&'static str> {
    use mac::*;
    Some(match code {
        RETURN => "Enter",
        KP_ENTER => "KP Enter",
        SPACE => "Space",
        DELETE => "Backspace",
        FORWARD_DELETE => "Delete",
        ESCAPE => "Esc",
        TAB => "Tab",
        CAPS_LOCK => "Caps Lock",
        HELP => "Insert",
        HOME => "Home",
        END => "End",
        PAGE_UP => "Page Up",
        PAGE_DOWN => "Page Down",
        LEFT => "←",
        RIGHT => "→",
        UP => "↑",
        DOWN => "↓",
        KP_CLEAR => "Num Lock",
        F1 => "F1",
        F2 => "F2",
        F3 => "F3",
        F4 => "F4",
        F5 => "F5",
        F6 => "F6",
        F7 => "F7",
        F8 => "F8",
        F9 => "F9",
        F10 => "F10",
        F11 => "F11",
        F12 => "F12",
        F13 => "F13",
        F14 => "F14",
        F15 => "F15",
        F16 => "F16",
        F17 => "F17",
        F18 => "F18",
        F19 => "F19",
        F20 => "F20",
        _ => return None,
    })
}

/// Characters typed with Shift on a US layout, used to describe text as keys.
pub fn shifted_character(c: char) -> Option<(&'static str, bool)> {
    Some(match c {
        '!' => ("1", true),
        '@' => ("2", true),
        '#' => ("3", true),
        '$' => ("4", true),
        '%' => ("5", true),
        '^' => ("6", true),
        '&' => ("7", true),
        '*' => ("8", true),
        '(' => ("9", true),
        ')' => ("0", true),
        '_' => ("minus", true),
        '+' => ("equal", true),
        ':' => ("semicolon", true),
        '"' => ("quote", true),
        '<' => ("comma", true),
        '>' => ("period", true),
        '?' => ("slash", true),
        '|' => ("backslash", true),
        '~' => ("grave", true),
        '{' => ("left bracket", true),
        '}' => ("right bracket", true),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_and_digits_reach_the_right_evdev_codes() {
        assert_eq!(mac_to_evdev(mac::A), Some(30));
        assert_eq!(mac_to_evdev(mac::C), Some(46));
        assert_eq!(mac_to_evdev(mac::Z), Some(44));
        assert_eq!(mac_to_evdev(mac::N1), Some(2));
        assert_eq!(mac_to_evdev(mac::N0), Some(11));
        assert_eq!(mac_to_evdev(mac::MINUS), Some(12));
        assert_eq!(mac_to_evdev(mac::EQUAL), Some(13));
        assert_eq!(mac_to_evdev(mac::RETURN), Some(28));
        assert_eq!(mac_to_evdev(mac::KP_ENTER), Some(96));
        assert_eq!(mac_to_evdev(mac::HELP), Some(110));
        assert_eq!(mac_to_evdev(mac::F13), Some(183));
        assert_eq!(mac_to_evdev(mac::F20), Some(190));
        assert_eq!(mac_to_evdev(mac::LEFT), Some(105));
        assert_eq!(mac_to_evdev(mac::ISO_SECTION), Some(86));
        assert_eq!(mac_to_evdev(mac::KP_EQUALS), Some(117));
        assert_eq!(mac_to_evdev(mac::KP_CLEAR), Some(69));
    }

    #[test]
    fn every_catalog_key_round_trips_through_evdev() {
        for code in canonical_key_codes().values() {
            let evdev = mac_to_evdev(*code).unwrap_or_else(|| panic!("no evdev code for mac {code}"));
            assert_eq!(evdev_to_mac(evdev), Some(*code), "round trip of mac {code}");
        }
    }

    #[test]
    fn usage_table_matches_macos_app() {
        assert_eq!(mac_to_usage(mac::A), Some(4));
        assert_eq!(mac_to_usage(mac::N1), Some(0x1e));
        assert_eq!(mac_to_usage(mac::F1), Some(0x3a));
        assert_eq!(mac_to_usage(mac::HELP), Some(0x49));
        assert_eq!(mac_to_usage(mac::KP_DIVIDE), Some(0x54));
        assert_eq!(mac_to_usage(mac::F20), Some(0x6f));
        assert_eq!(usage_to_mac(0x1e), Some(mac::N1));
        assert_eq!(mac_to_usage_table().len(), 101);
    }

    #[test]
    fn modifier_names_accept_legacy_aliases() {
        let mods = Mods::from_names(&["command", "Option", "control", "shift"]);
        assert!(mods.contains(Mods::SUPER) && mods.contains(Mods::ALT) && mods.contains(Mods::CTRL) && mods.contains(Mods::SHIFT));
        assert_eq!(Mods::for_key(ev::KEY_RIGHTALT), Mods::ALT);
    }

    #[test]
    fn canonical_names_resolve() {
        assert_eq!(key_code_for(" C "), Some(mac::C));
        assert_eq!(key_code_for("keypad 6"), Some(mac::KP6));
        assert_eq!(key_code_for("insert"), Some(mac::HELP));
        assert_eq!(primary_name(mac::MINUS), Some("minus"));
        assert_eq!(primary_name(mac::KP6), Some("kp6"));
        assert_eq!(primary_name(mac::LEFT), Some("left arrow"));
        assert_eq!(primary_name(mac::RETURN), Some("return"));
        assert_eq!(primary_name(mac::DELETE), Some("delete"));
    }
}
