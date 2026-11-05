//! Chooses the profile that is live while an app is in front.

use super::profile::Profile;
use std::collections::BTreeMap;

/// Returns the bound profile, or `selected` when no binding applies.
/// Nothing applies when automatic switching is off, the mouse memory
/// profile is active, OpenNaga itself is in front (so what the user edits is
/// what is live), the app has no binding, or either profile is missing or
/// imported from the mouse (those never remap in software).
///
/// App identifiers are Linux window classes. They are matched exactly first,
/// then without case, since some apps report `Spotify` and others `spotify`.
pub fn resolve(
    app_id: Option<&str>,
    own_id: Option<&str>,
    selected: &str,
    auto_switch: bool,
    mouse_memory_active: bool,
    bindings: &BTreeMap<String, String>,
    profiles: &BTreeMap<String, Profile>,
) -> String {
    let fallback = selected.to_string();
    if !auto_switch || mouse_memory_active {
        return fallback;
    }
    let Some(app) = app_id.filter(|a| !a.is_empty()) else { return fallback };
    if own_id.map(|own| own.eq_ignore_ascii_case(app)).unwrap_or(false) {
        return fallback;
    }
    if profiles.get(selected).map(|p| p.is_onboard_only()).unwrap_or(false) {
        return fallback;
    }
    let bound = bindings
        .get(app)
        .or_else(|| bindings.iter().find(|(id, _)| id.eq_ignore_ascii_case(app)).map(|(_, p)| p));
    match bound {
        Some(target) if profiles.get(target).map(|p| !p.is_onboard_only()).unwrap_or(false) => target.clone(),
        _ => fallback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profiles() -> BTreeMap<String, Profile> {
        let mut map = BTreeMap::new();
        map.insert("Default".to_string(), Profile::default());
        map.insert("Games".to_string(), Profile::default());
        map.insert("Mouse".to_string(), Profile { onboard_only: Some(true), ..Default::default() });
        map
    }

    fn bindings() -> BTreeMap<String, String> {
        let mut map = BTreeMap::new();
        map.insert("steam".to_string(), "Games".to_string());
        map.insert("Spotify".to_string(), "Games".to_string());
        map.insert("firmware".to_string(), "Mouse".to_string());
        map.insert("ghost".to_string(), "Deleted".to_string());
        map
    }

    fn resolve_for(app: &str, auto: bool, memory: bool, selected: &str) -> String {
        resolve(Some(app), Some("io.github.zer0codestuff.OpenNaga"), selected, auto, memory, &bindings(), &profiles())
    }

    #[test]
    fn bound_app_uses_its_profile() {
        assert_eq!(resolve_for("steam", true, false, "Default"), "Games");
        assert_eq!(resolve_for("spotify", true, false, "Default"), "Games", "case-insensitive fallback");
        assert_eq!(resolve_for("firefox", true, false, "Default"), "Default");
    }

    #[test]
    fn nothing_applies_when_disabled_or_protected() {
        assert_eq!(resolve_for("steam", false, false, "Default"), "Default");
        assert_eq!(resolve_for("steam", true, true, "Default"), "Default");
        assert_eq!(resolve_for("steam", true, false, "Mouse"), "Mouse");
        assert_eq!(resolve_for("firmware", true, false, "Default"), "Default");
        assert_eq!(resolve_for("ghost", true, false, "Default"), "Default");
        assert_eq!(resolve_for("io.github.zer0codestuff.OpenNaga", true, false, "Default"), "Default");
        assert_eq!(resolve(None, None, "Default", true, false, &bindings(), &profiles()), "Default");
    }
}
