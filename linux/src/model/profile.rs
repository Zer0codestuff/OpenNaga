//! The profiles document (`profiles.json`), byte-compatible in meaning with
//! the macOS app: the same keys, raw values, limits and format version.

use super::actions::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfilesFile {
    pub profiles: BTreeMap<String, Profile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<Settings>,
    #[serde(rename = "formatVersion", default, skip_serializing_if = "Option::is_none")]
    pub format_version: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(rename = "currentProfile", default, skip_serializing_if = "Option::is_none")]
    pub current_profile: Option<String>,
    #[serde(rename = "autoSwitchProfiles", default, skip_serializing_if = "Option::is_none")]
    pub auto_switch_profiles: Option<bool>,
    #[serde(rename = "showNotifications", default, skip_serializing_if = "Option::is_none")]
    pub show_notifications: Option<bool>,
    /// App identifier (a Linux window class, or a macOS bundle identifier in
    /// files from a Mac) to profile name, used while that app is in front.
    #[serde(rename = "appProfiles", default, skip_serializing_if = "Option::is_none")]
    pub app_profiles: Option<BTreeMap<String, String>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub buttons: BTreeMap<String, ButtonAction>,
    /// Imported firmware assignments cannot reliably identify physical
    /// controls through the factory event decoder.
    #[serde(rename = "onboardOnly", default, skip_serializing_if = "Option::is_none")]
    pub onboard_only: Option<bool>,
    /// Assignments used while a Hypershift control is held.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hypershift: Option<BTreeMap<String, ButtonAction>>,
}

impl Profile {

    pub fn buttons_in(&self, layer: MappingLayer) -> BTreeMap<String, ButtonAction> {
        match layer {
            MappingLayer::Standard => self.buttons.clone(),
            MappingLayer::Hypershift => self.hypershift.clone().unwrap_or_default(),
        }
    }

    pub fn layer_ref(&self, layer: MappingLayer) -> Option<&BTreeMap<String, ButtonAction>> {
        match layer {
            MappingLayer::Standard => Some(&self.buttons),
            MappingLayer::Hypershift => self.hypershift.as_ref(),
        }
    }

    pub fn is_onboard_only(&self) -> bool {
        self.onboard_only == Some(true)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ButtonAction {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keys: Option<Vec<KeyStroke>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steps: Option<Vec<MacroStep>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(rename = "mouseAction", default, skip_serializing_if = "Option::is_none")]
    pub mouse_action: Option<MouseAction>,
    #[serde(rename = "audioAction", default, skip_serializing_if = "Option::is_none")]
    pub audio_action: Option<AudioAction>,
    #[serde(rename = "systemAction", default, skip_serializing_if = "Option::is_none")]
    pub system_action: Option<SystemAction>,
}

impl ButtonAction {
    fn of(kind: &str) -> Self {
        ButtonAction { kind: kind.into(), ..Default::default() }
    }

    pub fn from_action(action: &ActionType) -> ButtonAction {
        match action {
            ActionType::Audio { action, description } => {
                ButtonAction { audio_action: Some(*action), description: description.clone(), ..Self::of("audio") }
            }
            ActionType::System { action, description } => {
                ButtonAction { system_action: Some(*action), description: description.clone(), ..Self::of("system") }
            }
            ActionType::Disabled => Self::of("disabled"),
            ActionType::Mouse { action, description } => {
                ButtonAction { mouse_action: Some(*action), description: description.clone(), ..Self::of("mouse") }
            }
            ActionType::KeySequence { keys, description } => {
                ButtonAction { keys: Some(keys.clone()), description: description.clone(), ..Self::of("keySequence") }
            }
            ActionType::Application { path, description } => {
                ButtonAction { path: Some(path.clone()), description: description.clone(), ..Self::of("application") }
            }
            ActionType::SystemCommand { command, description } => {
                ButtonAction { command: Some(command.clone()), description: description.clone(), ..Self::of("systemCommand") }
            }
            ActionType::TextSnippet { text, description } => {
                ButtonAction { text: Some(text.clone()), description: description.clone(), ..Self::of("textSnippet") }
            }
            ActionType::Macro { steps, description } => {
                ButtonAction { steps: Some(steps.clone()), description: description.clone(), ..Self::of("macro") }
            }
            ActionType::ProfileSwitch { profile, description } => {
                ButtonAction { profile: Some(profile.clone()), description: description.clone(), ..Self::of("profileSwitch") }
            }
            ActionType::Hypershift { description } => ButtonAction { description: description.clone(), ..Self::of("hypershift") },
        }
    }

    pub fn to_action(&self) -> Option<ActionType> {
        let description = self.description.clone();
        Some(match self.kind.as_str() {
            "audio" => ActionType::Audio { action: self.audio_action?, description },
            "system" => ActionType::System { action: self.system_action?, description },
            "disabled" => ActionType::Disabled,
            "mouse" => ActionType::Mouse { action: self.mouse_action?, description },
            "keySequence" => ActionType::KeySequence { keys: self.keys.clone().unwrap_or_default(), description },
            "application" => ActionType::Application { path: self.path.clone()?, description },
            "systemCommand" => ActionType::SystemCommand { command: self.command.clone()?, description },
            "textSnippet" => ActionType::TextSnippet { text: self.text.clone()?, description },
            "macro" => ActionType::Macro { steps: self.steps.clone().unwrap_or_default(), description },
            "profileSwitch" => ActionType::ProfileSwitch { profile: self.profile.clone()?, description },
            "hypershift" => ActionType::Hypershift { description },
            _ => return None,
        })
    }
}

/// Runtime mapping: logical control index to its action.
pub type Mapping = BTreeMap<i32, ActionType>;

pub fn mapping_of(profile: &Profile, layer: MappingLayer) -> Mapping {
    let mut result = Mapping::new();
    if let Some(buttons) = profile.layer_ref(layer) {
        for (key, action) in buttons {
            let Ok(index) = key.parse::<i32>() else { continue };
            if !(1..=19).contains(&index) {
                continue;
            }
            let Some(mapped) = action.to_action() else { continue };
            if layer == MappingLayer::Hypershift && mapped.is_hypershift() {
                continue;
            }
            result.insert(index, mapped);
        }
    }
    result
}

pub fn stored(layer: &Mapping) -> BTreeMap<String, ButtonAction> {
    layer.iter().map(|(index, action)| (index.to_string(), ButtonAction::from_action(action))).collect()
}

/// Profiles shipped with the app, used when no profiles file exists yet.
/// Linux shortcuts use Ctrl where the macOS defaults use Command.
pub const DEFAULT_PROFILES_JSON: &str = r#"{
  "profiles": {
    "Navigation": {
      "buttons": {
        "1": { "type": "mouse", "mouseAction": "browserBack", "description": "Back" },
        "2": { "type": "mouse", "mouseAction": "browserForward", "description": "Forward" },
        "3": { "type": "mouse", "mouseAction": "middleClick", "description": "Middle click" }
      }
    },
    "Default": {
      "buttons": {
        "1": { "type": "keySequence", "keys": [ { "key": "c", "modifiers": ["ctrl"], "keyCode": 8 } ], "description": "Copy" },
        "2": { "type": "keySequence", "keys": [ { "key": "v", "modifiers": ["ctrl"], "keyCode": 9 } ], "description": "Paste" }
      }
    }
  },
  "settings": { "currentProfile": "Default", "autoSwitchProfiles": false, "showNotifications": true }
}"#;

pub fn default_profiles() -> ProfilesFile {
    serde_json::from_str(DEFAULT_PROFILES_JSON).expect("bundled default profiles are valid")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::keys::mac;

    #[test]
    fn every_action_round_trips_through_the_file_format() {
        let actions = vec![
            ActionType::Mouse { action: MouseAction::ScrollLeft, description: Some("Left".into()) },
            ActionType::Audio { action: AudioAction::VolumeUp, description: None },
            ActionType::System { action: SystemAction::NextSpace, description: None },
            ActionType::Disabled,
            ActionType::KeySequence { keys: vec![KeyStroke::new("c", &["ctrl"], Some(mac::C))], description: None },
            ActionType::Application { path: "/usr/bin/foot".into(), description: None },
            ActionType::SystemCommand { command: "echo hi".into(), description: None },
            ActionType::TextSnippet { text: "héllo".into(), description: None },
            ActionType::Macro { steps: vec![MacroStep::delay(10), MacroStep::text("x")], description: None },
            ActionType::ProfileSwitch { profile: "Games".into(), description: None },
            ActionType::Hypershift { description: Some("Shift".into()) },
        ];
        for action in actions {
            let stored = ButtonAction::from_action(&action);
            let json = serde_json::to_string(&stored).unwrap();
            let decoded: ButtonAction = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded.to_action(), Some(action));
        }
    }

    #[test]
    fn macos_profiles_decode() {
        let json = r#"{"profiles":{"Mac":{"buttons":{"1":{"type":"keySequence","keys":[{"key":"c","modifiers":["cmd"]}]},
            "20":{"type":"disabled"},"x":{"type":"disabled"}},"onboardOnly":true,
            "hypershift":{"2":{"type":"hypershift"},"3":{"type":"system","systemAction":"missionControl"}}}},
            "settings":{"currentProfile":"Mac","appProfiles":{"com.apple.Safari":"Mac"}},"formatVersion":1}"#;
        let file: ProfilesFile = serde_json::from_str(json).unwrap();
        let profile = &file.profiles["Mac"];
        assert!(profile.is_onboard_only());
        let standard = mapping_of(profile, MappingLayer::Standard);
        assert_eq!(standard.len(), 1, "out-of-range and non-numeric controls are ignored");
        let shifted = mapping_of(profile, MappingLayer::Hypershift);
        assert_eq!(shifted.keys().copied().collect::<Vec<_>>(), vec![3], "Hypershift layer cannot hold a Hypershift key");
        assert_eq!(file.format_version, Some(1));
    }

    #[test]
    fn defaults_use_ctrl_on_linux() {
        let file = default_profiles();
        let mapping = mapping_of(&file.profiles["Default"], MappingLayer::Standard);
        match &mapping[&1] {
            ActionType::KeySequence { keys, .. } => assert_eq!(keys[0].formatted_shortcut(), "Ctrl+C"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn missing_payload_is_not_an_action() {
        assert_eq!(ButtonAction { kind: "application".into(), ..Default::default() }.to_action(), None);
        assert_eq!(ButtonAction { kind: "unknown".into(), ..Default::default() }.to_action(), None);
    }
}
