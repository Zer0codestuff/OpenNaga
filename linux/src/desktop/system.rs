//! How each System function runs on this desktop. Media, volume, brightness
//! and screenshot functions send the standard keys every Linux desktop
//! handles. Window and workspace functions use the compositor's own command
//! on Hyprland and Sway, and the default shortcuts on GNOME and KDE. A
//! function with no reliable equivalent says so instead of guessing.

use crate::input::typing::which;
use crate::model::actions::SystemAction;
use crate::model::keys::{ev, Mods};
use crate::model::mapper::SystemPlan;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Desktop {
    Hyprland,
    Sway,
    Gnome,
    Kde,
    Other,
}

impl Desktop {
    pub fn title(self) -> &'static str {
        match self {
            Desktop::Hyprland => "Hyprland",
            Desktop::Sway => "Sway",
            Desktop::Gnome => "GNOME",
            Desktop::Kde => "KDE Plasma",
            Desktop::Other => "this desktop",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Environment {
    pub desktop: Desktop,
    pub launcher: Option<Vec<String>>,
    pub settings: Option<Vec<String>>,
    pub notifications: Option<Vec<String>>,
    pub do_not_disturb: Option<Vec<String>>,
}

pub fn detect_desktop() -> Desktop {
    let var = |name: &str| std::env::var(name).unwrap_or_default();
    if !var("HYPRLAND_INSTANCE_SIGNATURE").is_empty() {
        return Desktop::Hyprland;
    }
    if !var("SWAYSOCK").is_empty() {
        return Desktop::Sway;
    }
    let current = var("XDG_CURRENT_DESKTOP").to_lowercase();
    if current.contains("gnome") || current.contains("unity") || current.contains("pantheon") {
        Desktop::Gnome
    } else if current.contains("kde") {
        Desktop::Kde
    } else if current.contains("hyprland") {
        Desktop::Hyprland
    } else if current.contains("sway") {
        Desktop::Sway
    } else {
        Desktop::Other
    }
}

fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

fn first_available(options: &[&[&str]]) -> Option<Vec<String>> {
    options.iter().find(|o| which(o[0])).map(|o| argv(o))
}

impl Environment {
    pub fn detect() -> Self {
        let desktop = detect_desktop();
        Environment {
            desktop,
            launcher: first_available(&[
                &["walker"],
                &["fuzzel"],
                &["anyrun"],
                &["wofi", "--show", "drun"],
                &["rofi", "-show", "drun"],
                &["tofi-drun", "--drun-launch=true"],
            ]),
            settings: first_available(&[
                &["gnome-control-center"],
                &["systemsettings"],
                &["xfce4-settings-manager"],
                &["cinnamon-settings"],
                &["mate-control-center"],
                &["lxqt-config"],
            ]),
            notifications: first_available(&[&["swaync-client", "-t", "-sw"]]),
            do_not_disturb: first_available(&[&["makoctl", "mode", "-t", "do-not-disturb"], &["swaync-client", "-d", "-sw"], &["dunstctl", "set-paused", "toggle"]]),
        }
    }

    pub fn shared() -> &'static Environment {
        static ENV: OnceLock<Environment> = OnceLock::new();
        ENV.get_or_init(Environment::detect)
    }

    pub fn plan(&self, action: SystemAction) -> SystemPlan {
        use SystemAction::*;
        let key = |code: u16, mods: Mods| SystemPlan::Key { code, mods };
        let unavailable = |what: &str| SystemPlan::Unavailable(what.to_string());
        let desktop = self.desktop;
        match action {
            VolumeUp => key(ev::KEY_VOLUMEUP, Mods::NONE),
            VolumeDown => key(ev::KEY_VOLUMEDOWN, Mods::NONE),
            Mute => key(ev::KEY_MUTE, Mods::NONE),
            PlayPause => key(ev::KEY_PLAYPAUSE, Mods::NONE),
            PreviousTrack => key(ev::KEY_PREVIOUSSONG, Mods::NONE),
            NextTrack => key(ev::KEY_NEXTSONG, Mods::NONE),
            BrightnessUp => key(ev::KEY_BRIGHTNESSUP, Mods::NONE),
            BrightnessDown => key(ev::KEY_BRIGHTNESSDOWN, Mods::NONE),
            Screenshot => key(ev::KEY_SYSRQ, Mods::NONE),
            ScreenshotSelection => key(ev::KEY_SYSRQ, Mods::SHIFT),
            ScreenshotOptions => key(ev::KEY_SYSRQ, Mods::ALT),
            ScreenshotClipboard => key(ev::KEY_SYSRQ, Mods::CTRL),
            ScreenshotSelectionClipboard => key(ev::KEY_SYSRQ, Mods::CTRL.union(Mods::SHIFT)),
            MissionControl => match desktop {
                Desktop::Gnome => key(ev::KEY_LEFTMETA, Mods::NONE),
                Desktop::Kde => key(17, Mods::SUPER), // Super+W: Overview
                _ => unavailable("This desktop has no overview shortcut. Bind one in your compositor and assign it with Keys."),
            },
            ApplicationWindows => match desktop {
                Desktop::Kde => key(65, Mods::CTRL), // Ctrl+F7: windows of the current app
                _ => unavailable("No standard shortcut on this desktop. Assign your own shortcut with Keys."),
            },
            ShowDesktop => match desktop {
                Desktop::Kde => key(ev::KEY_D, Mods::SUPER),
                _ => unavailable("No standard shortcut on this desktop. Assign your own shortcut with Keys."),
            },
            PreviousSpace | NextSpace => {
                let next = action == NextSpace;
                match desktop {
                    Desktop::Hyprland => SystemPlan::Command(argv(&["hyprctl", "dispatch", "workspace", if next { "e+1" } else { "e-1" }])),
                    Desktop::Sway => SystemPlan::Command(argv(&["swaymsg", "workspace", if next { "next_on_output" } else { "prev_on_output" }])),
                    Desktop::Gnome => key(if next { ev::KEY_PAGEDOWN } else { ev::KEY_PAGEUP }, Mods::SUPER),
                    Desktop::Kde => key(if next { ev::KEY_RIGHT } else { ev::KEY_LEFT }, Mods::SUPER.union(Mods::CTRL)),
                    Desktop::Other => unavailable("No standard workspace shortcut on this desktop. Assign your own with Keys."),
                }
            }
            HideApplication => match desktop {
                Desktop::Gnome => key(ev::KEY_H, Mods::SUPER),
                Desktop::Kde => key(ev::KEY_PAGEDOWN, Mods::SUPER),
                _ => unavailable("Tiling compositors have no standard way to hide a window. Assign your own shortcut with Keys."),
            },
            SwitchApplication => match desktop {
                Desktop::Hyprland => SystemPlan::Command(argv(&["hyprctl", "dispatch", "focuscurrentorlast"])),
                Desktop::Sway => unavailable("Sway has no command for the last window. Assign your own shortcut with Keys."),
                _ => key(ev::KEY_TAB, Mods::ALT),
            },
            Spotlight => match desktop {
                Desktop::Gnome | Desktop::Kde => key(ev::KEY_LEFTMETA, Mods::NONE),
                _ => match &self.launcher {
                    Some(command) => SystemPlan::Command(command.clone()),
                    None => unavailable("No app launcher found (walker, fuzzel, anyrun, wofi, rofi or tofi)."),
                },
            },
            Finder => SystemPlan::Command(vec!["xdg-open".into(), crate::model::data_folder::home().to_string_lossy().to_string()]),
            SystemSettings => match &self.settings {
                Some(command) => SystemPlan::Command(command.clone()),
                None => unavailable("No settings app found for this desktop."),
            },
            NotificationCenter => match (&self.notifications, desktop) {
                (Some(command), _) => SystemPlan::Command(command.clone()),
                (None, Desktop::Gnome) => key(ev::KEY_V, Mods::SUPER),
                _ => unavailable("No notification center found (swaync)."),
            },
            DoNotDisturb => match (&self.do_not_disturb, desktop) {
                (Some(command), _) => SystemPlan::Command(command.clone()),
                (None, Desktop::Gnome) => SystemPlan::Command(argv(&[
                    "sh",
                    "-c",
                    "v=$(gsettings get org.gnome.desktop.notifications show-banners); \
                     if [ \"$v\" = true ]; then gsettings set org.gnome.desktop.notifications show-banners false; \
                     else gsettings set org.gnome.desktop.notifications show-banners true; fi",
                ])),
                _ => unavailable("No notification daemon with a Do Not Disturb command found (mako, swaync or dunst)."),
            },
        }
    }

    /// What the inspector shows under a System function.
    pub fn describe(&self, action: SystemAction) -> String {
        match self.plan(action) {
            SystemPlan::Key { code, mods } => {
                let mut parts: Vec<&str> = Vec::new();
                for (flag, label) in [(Mods::CTRL, "Ctrl"), (Mods::ALT, "Alt"), (Mods::SHIFT, "Shift"), (Mods::SUPER, "Super")] {
                    if mods.contains(flag) {
                        parts.push(label);
                    }
                }
                let key = key_label(code);
                parts.push(&key);
                format!("Sends {} on {}.", parts.join("+"), self.desktop.title())
            }
            SystemPlan::Command(argv) => format!("Runs {}.", argv.join(" ")),
            SystemPlan::Unavailable(message) => message,
        }
        .replace("Sends Super on", "Taps Super on")
    }
}

fn key_label(code: u16) -> String {
    match code {
        ev::KEY_VOLUMEUP => "Volume Up".into(),
        ev::KEY_VOLUMEDOWN => "Volume Down".into(),
        ev::KEY_MUTE => "Mute".into(),
        ev::KEY_PLAYPAUSE => "Play/Pause".into(),
        ev::KEY_PREVIOUSSONG => "Previous".into(),
        ev::KEY_NEXTSONG => "Next".into(),
        ev::KEY_BRIGHTNESSUP => "Brightness Up".into(),
        ev::KEY_BRIGHTNESSDOWN => "Brightness Down".into(),
        ev::KEY_SYSRQ => "Print".into(),
        ev::KEY_LEFTMETA => "Super".into(),
        ev::KEY_PAGEUP => "Page Up".into(),
        ev::KEY_PAGEDOWN => "Page Down".into(),
        ev::KEY_LEFT => "←".into(),
        ev::KEY_RIGHT => "→".into(),
        ev::KEY_TAB => "Tab".into(),
        65 => "F7".into(),
        other => crate::model::keys::evdev_to_mac(other)
            .map(|mac| crate::model::actions::display_name(mac, crate::model::keys::primary_name(mac).unwrap_or("")))
            .unwrap_or_else(|| format!("key {other}")),
    }
}

/// The standard help line under a System function.
pub fn help(action: SystemAction) -> &'static str {
    use SystemAction::*;
    match action {
        BrightnessUp | BrightnessDown => "Works like the brightness keys. External displays need DDC/CI support in your desktop.",
        PlayPause | PreviousTrack | NextTrack => "Controls the active media player, like the media keys.",
        Screenshot | ScreenshotSelection | ScreenshotOptions | ScreenshotClipboard | ScreenshotSelectionClipboard => {
            "Your desktop decides what each Print combination captures."
        }
        PreviousSpace | NextSpace => "Moves to the adjacent workspace, if there is one.",
        SwitchApplication => "Switches to the last used window, like a quick Alt+Tab.",
        _ => "One action per press. Holding the button does not repeat it.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(desktop: Desktop) -> Environment {
        Environment { desktop, launcher: None, settings: None, notifications: None, do_not_disturb: None }
    }

    #[test]
    fn media_keys_everywhere() {
        for desktop in [Desktop::Hyprland, Desktop::Gnome, Desktop::Other] {
            assert_eq!(env(desktop).plan(SystemAction::VolumeUp), SystemPlan::Key { code: ev::KEY_VOLUMEUP, mods: Mods::NONE });
        }
        assert_eq!(env(Desktop::Other).describe(SystemAction::ScreenshotSelectionClipboard), "Sends Ctrl+Shift+Print on this desktop.");
    }

    #[test]
    fn workspaces_follow_the_compositor() {
        assert_eq!(
            env(Desktop::Hyprland).plan(SystemAction::NextSpace),
            SystemPlan::Command(argv(&["hyprctl", "dispatch", "workspace", "e+1"]))
        );
        assert_eq!(env(Desktop::Gnome).plan(SystemAction::PreviousSpace), SystemPlan::Key { code: ev::KEY_PAGEUP, mods: Mods::SUPER });
        assert!(matches!(env(Desktop::Other).plan(SystemAction::NextSpace), SystemPlan::Unavailable(_)));
        assert!(env(Desktop::Gnome).describe(SystemAction::MissionControl).starts_with("Taps Super"));
    }

    #[test]
    fn every_action_has_a_plan_and_description() {
        for desktop in [Desktop::Hyprland, Desktop::Sway, Desktop::Gnome, Desktop::Kde, Desktop::Other] {
            for action in SystemAction::ALL {
                assert!(!env(desktop).describe(action).is_empty());
            }
        }
    }
}
