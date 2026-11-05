//! Text typing through the virtual keyboard. The UI builds the map from the
//! compositor's keymap (GDK), so characters follow the active layout,
//! including AltGr levels. Characters the layout cannot produce go to a
//! helper (`wtype` on Wayland, `xdotool` on X11) when one is installed.

use crate::model::actions::KeyStroke;
use crate::model::keys::{self, ev, Mods};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypedKey {
    pub code: u16,
    /// xkb shift level: 0 plain, 1 Shift, 2 AltGr, 3 Shift and AltGr.
    pub level: u8,
}

impl TypedKey {
    pub fn modifier_keys(self) -> Vec<u16> {
        let mut keys = Vec::new();
        if self.level & 1 != 0 {
            keys.push(ev::KEY_LEFTSHIFT);
        }
        if self.level & 2 != 0 {
            keys.push(ev::KEY_RIGHTALT);
        }
        keys
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TypingMap {
    pub keys: HashMap<char, TypedKey>,
}

impl TypingMap {
    /// US layout, used until the UI reports the real one.
    pub fn us() -> Self {
        let mut map = TypingMap::default();
        for c in (32u8..127).map(char::from).chain(['\n', '\t']) {
            if let Some(stroke) = KeyStroke::from_character(c) {
                if let Some(code) = stroke.effective_code().and_then(keys::mac_to_evdev) {
                    let level = if stroke.mods().contains(Mods::SHIFT) { 1 } else { 0 };
                    map.keys.insert(c, TypedKey { code, level });
                }
            }
        }
        map
    }

    pub fn lookup(&self, c: char) -> Option<TypedKey> {
        let c = if c == '\r' { '\n' } else { c };
        self.keys.get(&c).copied()
    }

    /// The keys for `text`, or None when one character has no key.
    pub fn sequence(&self, text: &str) -> Option<Vec<TypedKey>> {
        text.chars().filter(|c| *c != '\r' || !text.contains("\r\n")).map(|c| self.lookup(c)).collect()
    }
}

/// The external helper used for characters outside the layout.
pub fn helper_command(text: &str) -> Option<Vec<String>> {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    if wayland && which("wtype") {
        return Some(vec!["wtype".into(), "--".into(), text.into()]);
    }
    if std::env::var_os("DISPLAY").is_some() && which("xdotool") {
        return Some(vec!["xdotool".into(), "type".into(), "--clearmodifiers".into(), "--".into(), text.into()]);
    }
    None
}

pub fn which(program: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|p| is_executable(&p.join(program))))
        .unwrap_or(false)
}

fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn us_map_types_ascii() {
        let map = TypingMap::us();
        assert_eq!(map.lookup('a'), Some(TypedKey { code: 30, level: 0 }));
        assert_eq!(map.lookup('A'), Some(TypedKey { code: 30, level: 1 }));
        assert_eq!(map.lookup('?'), Some(TypedKey { code: 53, level: 1 }));
        assert_eq!(map.lookup('\n'), Some(TypedKey { code: 28, level: 0 }));
        assert_eq!(map.lookup('\r'), Some(TypedKey { code: 28, level: 0 }));
        assert_eq!(map.lookup(' '), Some(TypedKey { code: 57, level: 0 }));
        assert!(map.sequence("Hello, world!").is_some());
        assert!(map.sequence("héllo").is_none());
        assert_eq!(map.sequence("a\r\nb").unwrap().len(), 3, "CRLF types one Enter");
        assert_eq!(TypedKey { code: 1, level: 3 }.modifier_keys(), vec![ev::KEY_LEFTSHIFT, ev::KEY_RIGHTALT]);
    }
}
