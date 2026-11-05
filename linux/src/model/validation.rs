//! The same limits apply to disk files, imports and the action editor.

use super::actions::{KeyStroke, MacroStep, MappingLayer};
use super::keys;
use super::profile::{ButtonAction, ProfilesFile};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileDataError(pub String);

impl fmt::Display for ProfileDataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ProfileDataError {}

pub type Result<T> = std::result::Result<T, ProfileDataError>;

pub const MAXIMUM_FILE_BYTES: usize = 2 * 1024 * 1024;
pub const MAXIMUM_PROFILES: usize = 128;
pub const MAXIMUM_TEXT_BYTES: usize = 16_384;
pub const MAXIMUM_MACRO_STEPS: usize = 256;

fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(ProfileDataError(message.into()))
    }
}

fn bounded(value: Option<&str>, label: &str, maximum: usize, allow_empty: bool) -> Result<()> {
    match value {
        Some(value) if value.len() <= maximum && (allow_empty || !value.trim().is_empty()) => Ok(()),
        _ => Err(ProfileDataError(format!(
            "{label} must contain {} to {maximum} UTF-8 bytes.",
            if allow_empty { "0" } else { "1" }
        ))),
    }
}

pub fn validate_file(file: &ProfilesFile) -> Result<()> {
    require(
        file.format_version.is_none() || file.format_version == Some(1),
        "This profile format is not supported by this OpenNaga build. Keep the file and use a compatible version.",
    )?;
    if file.profiles.is_empty() || file.profiles.len() > MAXIMUM_PROFILES {
        return Err(ProfileDataError(format!("A profile file must contain 1 to {MAXIMUM_PROFILES} profiles.")));
    }
    for (name, profile) in &file.profiles {
        require(!name.trim().is_empty() && name.len() <= 256, "Profile names must contain 1 to 256 UTF-8 bytes.")?;
        for layer in MappingLayer::ALL {
            let Some(buttons) = profile.layer_ref(layer) else { continue };
            require(buttons.len() <= 19, "A layer can contain at most 19 controls.")?;
            for (button, action) in buttons {
                let valid = button.parse::<i32>().ok().filter(|i| (1..=19).contains(i) && i.to_string() == *button);
                require(valid.is_some(), "Profile controls must be numbered 1 to 19.")?;
                validate_action(action, layer).map_err(|e| {
                    ProfileDataError(format!("Profile {name}, {} control {button}: {}", layer.title(), e.0))
                })?;
            }
        }
    }
    let bindings = file.settings.as_ref().and_then(|s| s.app_profiles.as_ref()).map(|b| b.len()).unwrap_or(0);
    require(bindings <= 1024, "A profile file can contain at most 1024 app bindings.")
}

pub fn validate_action(action: &ButtonAction, layer: MappingLayer) -> Result<()> {
    match action.kind.as_str() {
        "disabled" => {}
        "audio" => require(action.audio_action.is_some(), "Choose an audio function.")?,
        "system" => require(action.system_action.is_some(), "Choose a system function.")?,
        "mouse" => require(action.mouse_action.is_some(), "Choose a mouse function.")?,
        "hypershift" => require(layer == MappingLayer::Standard, "The Hypershift layer cannot contain another Hypershift key.")?,
        "keySequence" => {
            let keys = action.keys.as_deref().unwrap_or(&[]);
            require(!keys.is_empty() && keys.len() <= 64, "A key sequence must contain 1 to 64 keys.")?;
            for key in keys {
                validate_key(key)?;
            }
        }
        "application" => bounded(action.path.as_deref(), "Application path", 4096, false)?,
        "systemCommand" => bounded(action.command.as_deref(), "Command", MAXIMUM_TEXT_BYTES, false)?,
        "textSnippet" => bounded(action.text.as_deref(), "Text", MAXIMUM_TEXT_BYTES, true)?,
        "profileSwitch" => bounded(action.profile.as_deref(), "Target profile", 256, false)?,
        "macro" => {
            let steps = action.steps.as_ref().ok_or_else(|| ProfileDataError("Macro steps are missing.".into()))?;
            validate_macro(steps)?;
        }
        other => {
            let shown: String = other.chars().take(80).collect();
            return Err(ProfileDataError(format!("Unsupported action type: {shown}.")));
        }
    }
    bounded(Some(action.description.as_deref().unwrap_or("")), "Custom name", 1024, true)
}

pub fn validate_key(key: &KeyStroke) -> Result<()> {
    let known = match key.key_code {
        Some(code) => code <= 127,
        None => keys::key_code_for(&key.key).is_some(),
    };
    require(known, "Choose a supported keyboard key.")?;
    const NAMES: [&str; 8] = ["cmd", "command", "ctrl", "control", "shift", "alt", "option", "fn"];
    require(
        key.modifiers.len() <= 8 && key.modifiers.iter().all(|m| NAMES.contains(&m.to_lowercase().as_str())),
        "Unsupported keyboard modifier.",
    )
}

pub fn validate_macro(steps: &[MacroStep]) -> Result<()> {
    require(steps.len() <= MAXIMUM_MACRO_STEPS, &format!("A macro can contain at most {MAXIMUM_MACRO_STEPS} steps."))?;
    let mut duration: i64 = 0;
    let mut text_bytes = 0usize;
    for (index, step) in steps.iter().enumerate() {
        let number = index + 1;
        match step.kind.as_str() {
            "key" => {
                let key = step.key_stroke.as_ref().ok_or_else(|| ProfileDataError(format!("Step {number} needs a key.")))?;
                validate_key(key)?;
            }
            "text" => {
                bounded(step.text.as_deref(), &format!("Step {number} text"), MAXIMUM_TEXT_BYTES, true)?;
                text_bytes += step.text.as_deref().map(str::len).unwrap_or(0);
            }
            "delay" => match step.delay_ms {
                Some(delay) if (0..=600_000).contains(&delay) => duration += delay,
                _ => return Err(ProfileDataError(format!("Step {number} delay must be 0 to 600,000 ms."))),
            },
            _ => return Err(ProfileDataError(format!("Step {number} must be key, text or delay."))),
        }
    }
    require(duration <= 3_600_000, "A macro's total delay cannot exceed one hour.")?;
    require(
        text_bytes <= MAXIMUM_TEXT_BYTES,
        &format!("A macro can contain at most {MAXIMUM_TEXT_BYTES} UTF-8 bytes of text."),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::profile::Profile;
    use std::collections::BTreeMap;

    fn file_with(buttons: BTreeMap<String, ButtonAction>) -> ProfilesFile {
        let mut profiles = BTreeMap::new();
        profiles.insert("P".to_string(), Profile { buttons, ..Default::default() });
        ProfilesFile { profiles, settings: None, format_version: None }
    }

    #[test]
    fn rejects_unsupported_versions_and_empty_files() {
        let mut file = file_with(BTreeMap::new());
        file.format_version = Some(2);
        assert!(validate_file(&file).unwrap_err().0.contains("not supported"));
        assert!(validate_file(&ProfilesFile::default()).is_err());
        file.format_version = Some(1);
        assert!(validate_file(&file).is_ok());
    }

    #[test]
    fn rejects_bad_control_numbers() {
        for key in ["0", "20", "01", "x", "+1"] {
            let mut buttons = BTreeMap::new();
            buttons.insert(key.to_string(), ButtonAction { kind: "disabled".into(), ..Default::default() });
            assert!(validate_file(&file_with(buttons)).is_err(), "{key} accepted");
        }
    }

    #[test]
    fn hypershift_layer_cannot_hold_hypershift() {
        let action = ButtonAction { kind: "hypershift".into(), ..Default::default() };
        assert!(validate_action(&action, MappingLayer::Standard).is_ok());
        assert!(validate_action(&action, MappingLayer::Hypershift).is_err());
    }

    #[test]
    fn macro_limits() {
        assert!(validate_macro(&vec![MacroStep::delay(1); 257]).is_err());
        assert!(validate_macro(&[MacroStep::delay(600_001)]).is_err());
        assert!(validate_macro(&[MacroStep::delay(-1)]).is_err());
        assert!(validate_macro(&vec![MacroStep::delay(600_000); 7]).is_err(), "over one hour");
        assert!(validate_macro(&vec![MacroStep::delay(600_000); 6]).is_ok());
        assert!(validate_macro(&[MacroStep::text(&"a".repeat(16_385))]).is_err());
        assert!(validate_macro(&[MacroStep::text(&"a".repeat(9000)), MacroStep::text(&"a".repeat(9000))]).is_err());
        assert!(validate_macro(&[MacroStep { kind: "key".into(), key_stroke: None, text: None, delay_ms: None }]).is_err());
        assert!(validate_macro(&[MacroStep { kind: "jump".into(), key_stroke: None, text: None, delay_ms: None }]).is_err());
    }

    #[test]
    fn key_validation() {
        assert!(validate_key(&KeyStroke::new("c", &["cmd", "Control"], None)).is_ok());
        assert!(validate_key(&KeyStroke::new("c", &["hyper"], None)).is_err());
        assert!(validate_key(&KeyStroke::new("nope", &[], None)).is_err());
        assert!(validate_key(&KeyStroke::new("x", &[], Some(128))).is_err());
        assert!(validate_key(&KeyStroke::new("x", &[], Some(127))).is_ok());
    }

    #[test]
    fn text_fields_are_bounded() {
        let empty_command = ButtonAction { kind: "systemCommand".into(), command: Some("  ".into()), ..Default::default() };
        assert!(validate_action(&empty_command, MappingLayer::Standard).is_err());
        let long_name = ButtonAction { kind: "disabled".into(), description: Some("x".repeat(1025)), ..Default::default() };
        assert!(validate_action(&long_name, MappingLayer::Standard).is_err());
        let empty_text = ButtonAction { kind: "textSnippet".into(), text: Some(String::new()), ..Default::default() };
        assert!(validate_action(&empty_text, MappingLayer::Standard).is_ok());
    }
}
