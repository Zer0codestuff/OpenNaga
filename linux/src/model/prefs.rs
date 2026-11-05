//! App preferences (the macOS app keeps these in user defaults). The profiles
//! file stays authoritative for the selected profile; this file only keeps a
//! fallback copy, the Remapping switch and the scroll multipliers.

use super::data_folder;
use super::scroll::ScrollSensitivity;
use super::store;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    #[serde(rename = "remappingEnabled")]
    pub remapping_enabled: bool,
    #[serde(rename = "currentProfile")]
    pub current_profile: Option<String>,
    #[serde(rename = "scrollSensitivityVertical")]
    pub scroll_vertical: f64,
    #[serde(rename = "scrollSensitivityHorizontal")]
    pub scroll_horizontal: f64,
    #[serde(rename = "windowWidth")]
    pub window_width: i32,
    #[serde(rename = "windowHeight")]
    pub window_height: i32,
    #[serde(rename = "trayIcon")]
    pub tray_icon: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            // Off until the user turns it on, like the macOS app.
            remapping_enabled: false,
            current_profile: None,
            scroll_vertical: 1.0,
            scroll_horizontal: 1.0,
            window_width: 1180,
            window_height: 820,
            tray_icon: true,
        }
    }
}

impl Prefs {
    pub fn path() -> PathBuf {
        data_folder::file("settings.json")
    }

    pub fn load() -> Prefs {
        std::fs::read(Self::path()).ok().and_then(|data| serde_json::from_slice(&data).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        if let Err(error) = store::write_json(self, &Self::path()) {
            eprintln!("[Prefs] Could not save settings: {error}");
        }
    }

    pub fn scroll(&self) -> ScrollSensitivity {
        ScrollSensitivity { vertical: self.scroll_vertical, horizontal: self.scroll_horizontal }.clamped()
    }
}
