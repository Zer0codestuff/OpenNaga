//! Assignments a control can carry. Raw values match the macOS app, so the
//! profile files are interchangeable.

use super::keys::{self, mac, Mods};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionType {
    Mouse { action: MouseAction, description: Option<String> },
    Audio { action: AudioAction, description: Option<String> },
    System { action: SystemAction, description: Option<String> },
    Disabled,
    KeySequence { keys: Vec<KeyStroke>, description: Option<String> },
    Application { path: String, description: Option<String> },
    SystemCommand { command: String, description: Option<String> },
    TextSnippet { text: String, description: Option<String> },
    Macro { steps: Vec<MacroStep>, description: Option<String> },
    ProfileSwitch { profile: String, description: Option<String> },
    /// Holding this control switches the other controls to the Hypershift layer.
    Hypershift { description: Option<String> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MappingLayer {
    Standard,
    Hypershift,
}

impl MappingLayer {
    pub const ALL: [MappingLayer; 2] = [MappingLayer::Standard, MappingLayer::Hypershift];
    pub fn title(self) -> &'static str {
        match self {
            MappingLayer::Standard => "Standard",
            MappingLayer::Hypershift => "Hypershift",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MouseAction {
    BrowserBack,
    BrowserForward,
    LeftClick,
    RightClick,
    MiddleClick,
    Button4,
    Button5,
    ScrollUp,
    ScrollDown,
    ScrollLeft,
    ScrollRight,
    DpiUp,
    DpiDown,
}

impl MouseAction {
    pub const ALL: [MouseAction; 13] = [
        MouseAction::BrowserBack,
        MouseAction::BrowserForward,
        MouseAction::LeftClick,
        MouseAction::RightClick,
        MouseAction::MiddleClick,
        MouseAction::Button4,
        MouseAction::Button5,
        MouseAction::ScrollUp,
        MouseAction::ScrollDown,
        MouseAction::ScrollLeft,
        MouseAction::ScrollRight,
        MouseAction::DpiUp,
        MouseAction::DpiDown,
    ];

    pub fn title(self) -> &'static str {
        match self {
            MouseAction::BrowserBack => "Browser back",
            MouseAction::BrowserForward => "Browser forward",
            MouseAction::LeftClick => "Left click",
            MouseAction::RightClick => "Right click",
            MouseAction::MiddleClick => "Middle click",
            MouseAction::Button4 => "Mouse button 4",
            MouseAction::Button5 => "Mouse button 5",
            MouseAction::ScrollUp => "Scroll up",
            MouseAction::ScrollDown => "Scroll down",
            MouseAction::ScrollLeft => "Scroll left",
            MouseAction::ScrollRight => "Scroll right",
            MouseAction::DpiUp => "DPI up",
            MouseAction::DpiDown => "DPI down",
        }
    }

    pub fn is_hardware_only(self) -> bool {
        matches!(self, MouseAction::DpiUp | MouseAction::DpiDown)
    }

    /// The evdev button a held mouse action presses.
    pub fn button(self) -> Option<u16> {
        use keys::ev::*;
        Some(match self {
            MouseAction::LeftClick => BTN_LEFT,
            MouseAction::RightClick => BTN_RIGHT,
            MouseAction::MiddleClick => BTN_MIDDLE,
            MouseAction::Button4 => BTN_SIDE,
            MouseAction::Button5 => BTN_EXTRA,
            _ => return None,
        })
    }
}

/// Retained for profiles saved before the System editor existed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AudioAction {
    VolumeUp,
    VolumeDown,
    Mute,
}

impl AudioAction {
    pub fn system(self) -> SystemAction {
        match self {
            AudioAction::VolumeUp => SystemAction::VolumeUp,
            AudioAction::VolumeDown => SystemAction::VolumeDown,
            AudioAction::Mute => SystemAction::Mute,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SystemActionGroup {
    Audio,
    Playback,
    Brightness,
    Screenshots,
    Windows,
    Tools,
}

impl SystemActionGroup {
    pub const ALL: [SystemActionGroup; 6] = [
        SystemActionGroup::Audio,
        SystemActionGroup::Playback,
        SystemActionGroup::Brightness,
        SystemActionGroup::Screenshots,
        SystemActionGroup::Windows,
        SystemActionGroup::Tools,
    ];

    pub fn title(self) -> &'static str {
        match self {
            SystemActionGroup::Audio => "Audio",
            SystemActionGroup::Playback => "Playback",
            SystemActionGroup::Brightness => "Brightness",
            SystemActionGroup::Screenshots => "Screenshots",
            SystemActionGroup::Windows => "Windows and workspaces",
            SystemActionGroup::Tools => "Tools",
        }
    }

    pub fn actions(self) -> Vec<SystemAction> {
        SystemAction::ALL.iter().copied().filter(|a| a.group() == self).collect()
    }
}

/// The same 25 functions as the macOS catalog. Linux titles describe what is
/// sent here; `desktop::system` decides how each one runs on this desktop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SystemAction {
    VolumeUp,
    VolumeDown,
    Mute,
    PlayPause,
    PreviousTrack,
    NextTrack,
    BrightnessUp,
    BrightnessDown,
    Screenshot,
    ScreenshotSelection,
    ScreenshotOptions,
    ScreenshotClipboard,
    ScreenshotSelectionClipboard,
    MissionControl,
    ApplicationWindows,
    ShowDesktop,
    PreviousSpace,
    NextSpace,
    HideApplication,
    SwitchApplication,
    Spotlight,
    Finder,
    SystemSettings,
    NotificationCenter,
    DoNotDisturb,
}

impl SystemAction {
    pub const ALL: [SystemAction; 25] = [
        SystemAction::VolumeUp,
        SystemAction::VolumeDown,
        SystemAction::Mute,
        SystemAction::PlayPause,
        SystemAction::PreviousTrack,
        SystemAction::NextTrack,
        SystemAction::BrightnessUp,
        SystemAction::BrightnessDown,
        SystemAction::Screenshot,
        SystemAction::ScreenshotSelection,
        SystemAction::ScreenshotOptions,
        SystemAction::ScreenshotClipboard,
        SystemAction::ScreenshotSelectionClipboard,
        SystemAction::MissionControl,
        SystemAction::ApplicationWindows,
        SystemAction::ShowDesktop,
        SystemAction::PreviousSpace,
        SystemAction::NextSpace,
        SystemAction::HideApplication,
        SystemAction::SwitchApplication,
        SystemAction::Spotlight,
        SystemAction::Finder,
        SystemAction::SystemSettings,
        SystemAction::NotificationCenter,
        SystemAction::DoNotDisturb,
    ];

    pub fn group(self) -> SystemActionGroup {
        use SystemAction::*;
        match self {
            VolumeUp | VolumeDown | Mute => SystemActionGroup::Audio,
            PlayPause | PreviousTrack | NextTrack => SystemActionGroup::Playback,
            BrightnessUp | BrightnessDown => SystemActionGroup::Brightness,
            Screenshot | ScreenshotSelection | ScreenshotOptions | ScreenshotClipboard | ScreenshotSelectionClipboard => {
                SystemActionGroup::Screenshots
            }
            MissionControl | ApplicationWindows | ShowDesktop | PreviousSpace | NextSpace | HideApplication | SwitchApplication => {
                SystemActionGroup::Windows
            }
            Spotlight | Finder | SystemSettings | NotificationCenter | DoNotDisturb => SystemActionGroup::Tools,
        }
    }

    pub fn title(self) -> &'static str {
        use SystemAction::*;
        match self {
            VolumeUp => "Volume up",
            VolumeDown => "Volume down",
            Mute => "Mute/unmute",
            PlayPause => "Play/pause",
            PreviousTrack => "Previous track",
            NextTrack => "Next track",
            BrightnessUp => "Brightness up",
            BrightnessDown => "Brightness down",
            Screenshot => "Screenshot (Print)",
            ScreenshotSelection => "Screenshot (Shift+Print)",
            ScreenshotOptions => "Screenshot (Alt+Print)",
            ScreenshotClipboard => "Screenshot (Ctrl+Print)",
            ScreenshotSelectionClipboard => "Screenshot (Ctrl+Shift+Print)",
            MissionControl => "Overview",
            ApplicationWindows => "Show application windows",
            ShowDesktop => "Show desktop",
            PreviousSpace => "Previous workspace",
            NextSpace => "Next workspace",
            HideApplication => "Hide current window",
            SwitchApplication => "Switch to last window",
            Spotlight => "Open app launcher",
            Finder => "Open file manager",
            SystemSettings => "Open system settings",
            NotificationCenter => "Show notifications",
            DoNotDisturb => "Toggle Do Not Disturb",
        }
    }

    /// freedesktop icon names.
    pub fn icon(self) -> &'static str {
        use SystemAction::*;
        match self {
            VolumeUp => "audio-volume-high-symbolic",
            VolumeDown => "audio-volume-low-symbolic",
            Mute => "audio-volume-muted-symbolic",
            PlayPause => "media-playback-start-symbolic",
            PreviousTrack => "media-skip-backward-symbolic",
            NextTrack => "media-skip-forward-symbolic",
            BrightnessUp => "display-brightness-symbolic",
            BrightnessDown => "display-brightness-symbolic",
            Screenshot | ScreenshotSelection | ScreenshotOptions | ScreenshotClipboard | ScreenshotSelectionClipboard => {
                "camera-photo-symbolic"
            }
            MissionControl | ApplicationWindows => "view-grid-symbolic",
            ShowDesktop => "user-desktop-symbolic",
            PreviousSpace => "go-previous-symbolic",
            NextSpace => "go-next-symbolic",
            HideApplication => "window-minimize-symbolic",
            SwitchApplication => "media-playlist-repeat-symbolic",
            Spotlight => "system-search-symbolic",
            Finder => "folder-symbolic",
            SystemSettings => "emblem-system-symbolic",
            NotificationCenter => "preferences-system-notifications-symbolic",
            DoNotDisturb => "weather-clear-night-symbolic",
        }
    }

    /// The media usage the mouse itself can store (consumer page).
    pub fn media_usage(self) -> Option<u8> {
        use SystemAction::*;
        Some(match self {
            VolumeUp => 0xe9,
            VolumeDown => 0xea,
            Mute => 0xe2,
            PlayPause => 0xcd,
            NextTrack => 0xb5,
            PreviousTrack => 0xb6,
            _ => return None,
        })
    }

    pub fn from_media_usage(usage: u8) -> Option<SystemAction> {
        SystemAction::ALL.iter().copied().find(|a| a.media_usage() == Some(usage))
    }
}

/// One key press, with modifiers. `key_code` is a macOS virtual key code.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyStroke {
    pub key: String,
    pub modifiers: Vec<String>,
    #[serde(rename = "keyCode", default, skip_serializing_if = "Option::is_none")]
    pub key_code: Option<u16>,
}

impl KeyStroke {
    pub fn new(key: &str, modifiers: &[&str], key_code: Option<u16>) -> Self {
        KeyStroke { key: key.to_string(), modifiers: modifiers.iter().map(|m| m.to_string()).collect(), key_code }
    }

    /// Legacy files may spell modifiers as command/option/control. Editors
    /// show their actual state and avoid adding duplicate aliases.
    pub fn normalized_modifiers(&self) -> Vec<String> {
        let mods = Mods::from_names(&self.modifiers);
        ["ctrl", "alt", "shift", "cmd", "fn"]
            .iter()
            .filter(|name| mods.contains(Mods::from_names(&[**name])))
            .map(|s| s.to_string())
            .collect()
    }

    pub fn mods(&self) -> Mods {
        Mods::from_names(&self.modifiers)
    }

    pub fn effective_code(&self) -> Option<u16> {
        self.key_code.or_else(|| keys::key_code_for(&self.key))
    }

    pub fn display_label(&self) -> String {
        match self.key_code {
            Some(code) => display_name(code, &self.key),
            None => {
                if self.key.chars().count() == 1 {
                    self.key.to_uppercase()
                } else {
                    capitalize_words(&self.key)
                }
            }
        }
    }

    /// Linux-style label, for example `Ctrl+Shift+C` or `Super+Tab`.
    pub fn formatted_shortcut(&self) -> String {
        let mut parts: Vec<String> = self
            .normalized_modifiers()
            .iter()
            .filter_map(|m| modifier_label(m))
            .map(|s| s.to_string())
            .collect();
        parts.push(self.display_label());
        parts.join("+")
    }

    pub fn canonical_key_string(code: Option<u16>, characters: Option<&str>) -> String {
        if let Some(name) = code.and_then(keys::primary_name) {
            return name.to_string();
        }
        characters.map(keys::normalize_identifier).unwrap_or_default()
    }

    /// The key that types `c` on a US layout. Used for single-key text.
    pub fn from_character(c: char) -> Option<KeyStroke> {
        match c {
            '\n' | '\r' => return Some(KeyStroke::new("return", &[], Some(mac::RETURN))),
            '\t' => return Some(KeyStroke::new("tab", &[], Some(mac::TAB))),
            ' ' => return Some(KeyStroke::new("space", &[], Some(mac::SPACE))),
            _ => {}
        }
        if c.is_alphabetic() {
            let lower: String = c.to_lowercase().collect();
            let code = keys::key_code_for(&lower)?;
            let mods: Vec<&str> = if c.is_uppercase() { vec!["shift"] } else { vec![] };
            let canonical = keys::primary_name(code).unwrap_or(&lower).to_string();
            return Some(KeyStroke { key: canonical, modifiers: mods.iter().map(|s| s.to_string()).collect(), key_code: Some(code) });
        }
        if let Some((key, _)) = keys::shifted_character(c) {
            let code = keys::key_code_for(key)?;
            return Some(KeyStroke::new(key, &["shift"], Some(code)));
        }
        let string = c.to_string();
        let code = keys::key_code_for(&string)?;
        let canonical = keys::primary_name(code).map(|s| s.to_string()).unwrap_or(string);
        Some(KeyStroke { key: canonical, modifiers: vec![], key_code: Some(code) })
    }
}

pub fn modifier_label(name: &str) -> Option<&'static str> {
    Some(match name.to_lowercase().as_str() {
        "cmd" | "command" | "super" => "Super",
        "shift" => "Shift",
        "alt" | "option" => "Alt",
        "ctrl" | "control" => "Ctrl",
        "fn" => "Fn",
        _ => return None,
    })
}

pub fn display_name(code: u16, fallback: &str) -> String {
    if let Some(special) = keys::special_key_name(code) {
        return special.to_string();
    }
    if fallback.chars().count() == 1 {
        return fallback.to_uppercase();
    }
    if let Some(rest) = fallback.strip_prefix("kp") {
        if !fallback.starts_with("keypad") {
            return format!("KP {rest}");
        }
    }
    capitalize_words(fallback)
}

fn capitalize_words(text: &str) -> String {
    text.split(' ')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroStep {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "keyStroke", default, skip_serializing_if = "Option::is_none")]
    pub key_stroke: Option<KeyStroke>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(rename = "delayMs", default, skip_serializing_if = "Option::is_none")]
    pub delay_ms: Option<i64>,
}

impl MacroStep {
    pub fn key(stroke: KeyStroke) -> Self {
        MacroStep { kind: "key".into(), key_stroke: Some(stroke), text: None, delay_ms: None }
    }
    pub fn text(text: &str) -> Self {
        MacroStep { kind: "text".into(), key_stroke: None, text: Some(text.into()), delay_ms: None }
    }
    pub fn delay(ms: i64) -> Self {
        MacroStep { kind: "delay".into(), key_stroke: None, text: None, delay_ms: Some(ms) }
    }

    pub fn summary(&self, index: usize) -> String {
        let detail = match self.kind.as_str() {
            "key" => self.key_stroke.as_ref().map(|k| k.formatted_shortcut()).unwrap_or_else(|| "Choose key".into()),
            "delay" => format!("Delay {} ms", self.delay_ms.unwrap_or(0)),
            _ => format!("Text ({} characters)", self.text.as_deref().unwrap_or("").chars().count()),
        };
        format!("{}. {}", index + 1, detail)
    }
}

impl ActionType {
    pub fn display_name(&self) -> String {
        match self {
            ActionType::Audio { action, description } => description.clone().unwrap_or_else(|| action.system().title().into()),
            ActionType::System { action, description } => description.clone().unwrap_or_else(|| action.title().into()),
            ActionType::Disabled => "Disabled".into(),
            ActionType::Mouse { action, description } => description.clone().unwrap_or_else(|| action.title().into()),
            ActionType::KeySequence { keys, description } => description
                .clone()
                .unwrap_or_else(|| keys.iter().map(|k| k.formatted_shortcut()).collect::<Vec<_>>().join(", ")),
            ActionType::Application { path, description } => description.clone().unwrap_or_else(|| {
                let name = std::path::Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or(path);
                name.strip_suffix(".desktop").unwrap_or(name).to_string()
            }),
            ActionType::SystemCommand { description, .. } => description.clone().unwrap_or_else(|| "Shell command".into()),
            ActionType::TextSnippet { description, .. } => description.clone().unwrap_or_else(|| "Text".into()),
            ActionType::Macro { description, .. } => description.clone().unwrap_or_else(|| "Macro".into()),
            ActionType::ProfileSwitch { profile, description } => description.clone().unwrap_or_else(|| format!("Profile: {profile}")),
            ActionType::Hypershift { description } => description.clone().unwrap_or_else(|| "Hypershift".into()),
        }
    }

    pub fn is_hypershift(&self) -> bool {
        matches!(self, ActionType::Hypershift { .. })
    }

    pub fn is_hardware_dpi(&self) -> bool {
        matches!(self, ActionType::Mouse { action, .. } if action.is_hardware_only())
    }

}

/// Logical control names. 1 to 12 are the side grid.
pub fn button_name(index: i32) -> String {
    match index {
        13 => "DPI up".into(),
        14 => "DPI down".into(),
        15 => "Wheel tilt left".into(),
        16 => "Wheel tilt right".into(),
        17 => "Middle click".into(),
        18 => "Left click".into(),
        19 => "Right click".into(),
        _ => format!("Side button {index}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_labels_are_linux_style() {
        let stroke = KeyStroke::new("c", &["shift", "command", "control"], Some(mac::C));
        assert_eq!(stroke.formatted_shortcut(), "Ctrl+Shift+Super+C");
        assert_eq!(KeyStroke::new("left arrow", &["alt"], Some(mac::LEFT)).formatted_shortcut(), "Alt+←");
        assert_eq!(KeyStroke::new("kp6", &[], Some(mac::KP6)).formatted_shortcut(), "KP 6");
        assert_eq!(KeyStroke::new("page up", &[], None).display_label(), "Page Up");
    }

    #[test]
    fn normalized_modifiers_drop_aliases() {
        let stroke = KeyStroke::new("c", &["command", "cmd", "option"], None);
        assert_eq!(stroke.normalized_modifiers(), vec!["alt".to_string(), "cmd".to_string()]);
    }

    #[test]
    fn characters_map_to_us_keys() {
        assert_eq!(KeyStroke::from_character('A'), Some(KeyStroke::new("a", &["shift"], Some(mac::A))));
        assert_eq!(KeyStroke::from_character('?'), Some(KeyStroke::new("slash", &["shift"], Some(mac::SLASH))));
        assert_eq!(KeyStroke::from_character('\n').unwrap().key_code, Some(mac::RETURN));
        assert_eq!(KeyStroke::from_character('-').unwrap().key, "minus");
        assert_eq!(KeyStroke::from_character('€'), None);
    }

    #[test]
    fn display_names_match_macos_app() {
        let action = ActionType::Application { path: "/usr/share/applications/firefox.desktop".into(), description: None };
        assert_eq!(action.display_name(), "firefox");
        assert_eq!(ActionType::ProfileSwitch { profile: "Games".into(), description: None }.display_name(), "Profile: Games");
        assert_eq!(ActionType::Disabled.display_name(), "Disabled");
        assert_eq!(button_name(15), "Wheel tilt left");
        assert_eq!(button_name(3), "Side button 3");
        assert_eq!(SystemAction::ALL.len(), 25);
        let grouped: usize = SystemActionGroup::ALL.iter().map(|g| g.actions().len()).sum();
        assert_eq!(grouped, 25);
    }

    #[test]
    fn raw_values_match_macos_json() {
        assert_eq!(serde_json::to_string(&MouseAction::BrowserBack).unwrap(), "\"browserBack\"");
        assert_eq!(serde_json::to_string(&SystemAction::ScreenshotSelectionClipboard).unwrap(), "\"screenshotSelectionClipboard\"");
        assert_eq!(serde_json::to_string(&AudioAction::Mute).unwrap(), "\"mute\"");
        let step: MacroStep = serde_json::from_str(r#"{"type":"delay","delayMs":5}"#).unwrap();
        assert_eq!(step, MacroStep::delay(5));
    }
}
