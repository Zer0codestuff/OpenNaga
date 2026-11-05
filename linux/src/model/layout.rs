//! The keys a user can pick, labelled for the active keyboard layout.
//! Physical codes are persisted; labels follow the layout. The UI fills
//! the map from the compositor's keymap through GDK; the US layout is the
//! fallback for command-line use and tests.

use super::actions::KeyStroke;
use super::keys::{self, mac};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyGroup {
    Letters,
    Numbers,
    Symbols,
    Navigation,
    Function,
    Keypad,
}

impl KeyGroup {
    pub const ALL: [KeyGroup; 6] = [KeyGroup::Letters, KeyGroup::Numbers, KeyGroup::Symbols, KeyGroup::Navigation, KeyGroup::Function, KeyGroup::Keypad];
    pub fn title(self) -> &'static str {
        match self {
            KeyGroup::Letters => "Letters",
            KeyGroup::Numbers => "Numbers",
            KeyGroup::Symbols => "Symbols",
            KeyGroup::Navigation => "Special keys",
            KeyGroup::Function => "Function keys",
            KeyGroup::Keypad => "Numeric keypad",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyboardKey {
    pub code: u16,
    pub key: String,
    pub label: String,
    pub group: KeyGroup,
}

impl KeyboardKey {
    pub fn stroke(&self, modifiers: &[String]) -> KeyStroke {
        KeyStroke { key: self.key.clone(), modifiers: modifiers.to_vec(), key_code: Some(self.code) }
    }
}

/// Unshifted characters of the printable keys, by macOS key code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutMap {
    pub chars: BTreeMap<u16, String>,
}

const LETTERS: &str = "abcdefghijklmnopqrstuvwxyz";
const NUMBERS: &str = "1234567890";
const SYMBOL_NAMES: [&str; 11] =
    ["grave", "minus", "equals", "left bracket", "right bracket", "backslash", "semicolon", "quote", "comma", "period", "slash"];

impl LayoutMap {
    pub fn us() -> Self {
        let mut chars = BTreeMap::new();
        for c in LETTERS.chars().chain(NUMBERS.chars()) {
            let name = c.to_string();
            chars.insert(keys::key_code_for(&name).unwrap(), name);
        }
        for (name, c) in SYMBOL_NAMES.iter().zip(["`", "-", "=", "[", "]", "\\", ";", "'", ",", ".", "/"]) {
            chars.insert(keys::key_code_for(name).unwrap(), c.to_string());
        }
        LayoutMap { chars }
    }

    pub fn character(&self, code: u16) -> Option<&str> {
        self.chars.get(&code).map(|s| s.as_str())
    }

    pub fn catalog(&self) -> Vec<KeyboardKey> {
        let mut result = Vec::new();
        let mut add = |names: &[String], group: KeyGroup, localized: bool| {
            for name in names {
                let Some(code) = keys::key_code_for(name) else { continue };
                let character = if localized { self.character(code).map(str::to_string) } else { None };
                let label = character.as_ref().map(|c| c.to_uppercase()).unwrap_or_else(|| label_for(name));
                result.push(KeyboardKey { code, key: character.unwrap_or_else(|| name.clone()), label, group });
            }
        };
        let strings = |s: &str| s.chars().map(|c| c.to_string()).collect::<Vec<_>>();
        add(&strings(LETTERS), KeyGroup::Letters, true);
        add(&strings(NUMBERS), KeyGroup::Numbers, true);
        add(&SYMBOL_NAMES.iter().map(|s| s.to_string()).collect::<Vec<_>>(), KeyGroup::Symbols, true);
        if let Some(c) = self.character(mac::ISO_SECTION) {
            result.push(KeyboardKey { code: mac::ISO_SECTION, key: c.to_string(), label: c.to_uppercase(), group: KeyGroup::Symbols });
        }
        let navigation = [
            "space", "tab", "return", "escape", "delete", "forward delete", "insert", "left arrow", "right arrow", "up arrow",
            "down arrow", "home", "end", "page up", "page down",
        ];
        let mut push = |names: Vec<String>, group: KeyGroup| {
            for name in names {
                let Some(code) = keys::key_code_for(&name) else { continue };
                let key = keys::primary_name(code).unwrap_or(&name).to_string();
                result.push(KeyboardKey { code, key, label: label_for(&name), group });
            }
        };
        push(navigation.iter().map(|s| s.to_string()).collect(), KeyGroup::Navigation);
        push((1..=20).map(|n| format!("f{n}")).collect(), KeyGroup::Function);
        let mut keypad: Vec<String> = (0..=9).map(|n| format!("kp{n}")).collect();
        keypad.extend(["kp.", "kp+", "kp-", "kp*", "kp/", "kp=", "keypad enter"].iter().map(|s| s.to_string()));
        push(keypad, KeyGroup::Keypad);
        result
    }

    /// A recorded or imported key, named like the catalog entry.
    pub fn captured_stroke(&self, code: u16, modifiers: Vec<String>) -> KeyStroke {
        if let Some(entry) = self.catalog().into_iter().find(|k| k.code == code) {
            return entry.stroke(&modifiers);
        }
        KeyStroke { key: KeyStroke::canonical_key_string(Some(code), None), modifiers, key_code: Some(code) }
    }

    /// The key that types `text` on its own, when it is one printable key.
    pub fn key_for_text(&self, text: &str) -> Option<KeyboardKey> {
        if text.chars().count() != 1 {
            return None;
        }
        self.catalog()
            .into_iter()
            .find(|k| matches!(k.group, KeyGroup::Letters | KeyGroup::Numbers | KeyGroup::Symbols) && k.key == text)
    }
}

pub fn label_for(key: &str) -> String {
    let label = match key {
        "space" => "Space",
        "tab" => "Tab",
        "return" => "Enter",
        "escape" => "Esc",
        "delete" => "Backspace",
        "forward delete" => "Delete",
        "insert" => "Insert",
        "left arrow" => "←",
        "right arrow" => "→",
        "up arrow" => "↑",
        "down arrow" => "↓",
        "home" => "Home",
        "end" => "End",
        "page up" => "Page Up",
        "page down" => "Page Down",
        "keypad enter" => "Enter",
        _ => "",
    };
    if !label.is_empty() {
        return label.to_string();
    }
    if let Some(rest) = key.strip_prefix("kp") {
        return rest.to_string();
    }
    key.to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn us_catalog() {
        let layout = LayoutMap::us();
        let catalog = layout.catalog();
        assert_eq!(catalog.iter().filter(|k| k.group == KeyGroup::Letters).count(), 26);
        assert_eq!(catalog.iter().filter(|k| k.group == KeyGroup::Symbols).count(), 11);
        assert_eq!(catalog.iter().filter(|k| k.group == KeyGroup::Function).count(), 20);
        assert_eq!(catalog.iter().filter(|k| k.group == KeyGroup::Keypad).count(), 17);
        let minus = catalog.iter().find(|k| k.code == mac::MINUS).unwrap();
        assert_eq!((minus.key.as_str(), minus.label.as_str()), ("-", "-"));
        let insert = catalog.iter().find(|k| k.code == mac::HELP).unwrap();
        assert_eq!((insert.key.as_str(), insert.label.as_str()), ("help", "Insert"));
        assert_eq!(layout.key_for_text("q").unwrap().code, mac::Q);
        assert!(layout.key_for_text("Q").is_none());
        assert_eq!(layout.captured_stroke(mac::KP6, vec![]).key, "kp6");
    }

    #[test]
    fn other_layouts_relabel_letters() {
        let mut layout = LayoutMap::us();
        layout.chars.insert(mac::Y, "z".into());
        layout.chars.insert(mac::Z, "y".into());
        let catalog = layout.catalog();
        assert_eq!(catalog.iter().find(|k| k.code == mac::Y).unwrap().label, "Z");
        assert_eq!(layout.key_for_text("z").unwrap().code, mac::Y);
    }
}
