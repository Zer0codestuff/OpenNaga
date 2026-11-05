//! The only place that names OpenNaga's data folder on Linux:
//! `$XDG_CONFIG_HOME/opennaga`, normally `~/.config/opennaga`. It holds the
//! same files as the macOS folder: profiles, their backup, the mouse memory
//! backup (`onboard-profile.json`), the driver mode journal and the hardware
//! lock, plus `settings.json` for app preferences.

use std::path::PathBuf;
use std::sync::OnceLock;

pub const NAME: &str = "opennaga";

static OVERRIDE: OnceLock<PathBuf> = OnceLock::new();

pub fn root() -> PathBuf {
    if let Some(path) = OVERRIDE.get() {
        return path.clone();
    }
    if let Some(path) = std::env::var_os("OPENNAGA_HOME").filter(|p| !p.is_empty()) {
        return PathBuf::from(path);
    }
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(".config"));
    config.join(NAME)
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"))
}

pub fn file(name: &str) -> PathBuf {
    root().join(name)
}
