//! Reads the compositor's keymap through GDK: key labels for the active
//! layout and the character map used to type text.

use crate::input::typing::{TypedKey, TypingMap};
use crate::model::keys;
use crate::model::layout::{KeyGroup, LayoutMap};
use gtk::gdk;
use gtk::prelude::*;

pub fn read(display: &gdk::Display) -> (LayoutMap, TypingMap) {
    // The active layout index exists since GTK 4.18; older versions use group 0.
    let group = display
        .default_seat()
        .and_then(|seat| seat.keyboard())
        .filter(|k| k.find_property("active-layout-index").is_some())
        .map(|k| k.property::<i32>("active-layout-index").max(0))
        .unwrap_or(0);
    let mut typing = TypingMap::default();
    let mut base: std::collections::HashMap<u16, char> = std::collections::HashMap::new();
    for keycode in 8u32..256 {
        let Some(entries) = display.map_keycode(keycode) else { continue };
        let evdev = (keycode - 8) as u16;
        for (key, value) in entries {
            if key.group() != group || key.level() > 3 {
                continue;
            }
            let Some(c) = value.to_unicode() else { continue };
            if c.is_control() {
                continue;
            }
            let level = key.level() as u8;
            if level == 0 {
                base.entry(evdev).or_insert(c);
            }
            let typed = TypedKey { code: evdev, level };
            typing
                .keys
                .entry(c)
                .and_modify(|existing| {
                    if existing.level > level {
                        *existing = typed;
                    }
                })
                .or_insert(typed);
        }
    }
    let us = TypingMap::us();
    if typing.keys.len() < 20 {
        // No usable keymap (for example a headless snapshot): stay with US.
        return (LayoutMap::us(), us);
    }
    for c in ['\n', '\t', ' '] {
        if let Some(key) = us.lookup(c) {
            typing.keys.insert(c, key);
        }
    }
    let mut layout = LayoutMap::us();
    let printable: Vec<u16> = LayoutMap::us()
        .catalog()
        .into_iter()
        .filter(|k| matches!(k.group, KeyGroup::Letters | KeyGroup::Numbers | KeyGroup::Symbols))
        .map(|k| k.code)
        .chain(std::iter::once(keys::mac::ISO_SECTION))
        .collect();
    for code in printable {
        let Some(evdev) = keys::mac_to_evdev(code) else { continue };
        match base.get(&evdev) {
            Some(c) => {
                layout.chars.insert(code, c.to_string());
            }
            None => {
                layout.chars.remove(&code);
            }
        }
    }
    (layout, typing)
}
