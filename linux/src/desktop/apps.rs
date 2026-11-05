//! Installed applications from their desktop entries, for binding profiles
//! to apps and for the Application action. A binding uses the window class
//! the app reports (StartupWMClass), falling back to the desktop file ID,
//! which is what Wayland apps use as their app_id.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledApp {
    pub app_id: String,
    pub name: String,
    pub desktop_id: String,
    pub path: PathBuf,
    pub icon: Option<String>,
}

pub fn application_dirs() -> Vec<PathBuf> {
    let home = crate::model::data_folder::home();
    let data_home = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".local/share"));
    let data_dirs = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    let mut dirs = vec![data_home.join("applications")];
    dirs.extend(data_dirs.split(':').filter(|d| !d.is_empty()).map(|d| PathBuf::from(d).join("applications")));
    for extra in [home.join(".local/share/flatpak/exports/share/applications"), PathBuf::from("/var/lib/flatpak/exports/share/applications")] {
        if !dirs.contains(&extra) {
            dirs.push(extra);
        }
    }
    dirs
}

/// Parses the `[Desktop Entry]` group. Returns None for entries that never
/// show up as windows of their own (hidden, NoDisplay, links, broken files).
pub fn parse_desktop_entry(text: &str, desktop_id: &str, path: &Path) -> Option<InstalledApp> {
    let mut in_entry = false;
    let mut fields: BTreeMap<String, String> = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            fields.entry(key.trim().to_string()).or_insert_with(|| value.trim().to_string());
        }
    }
    let flag = |key: &str| fields.get(key).map(|v| v.eq_ignore_ascii_case("true")).unwrap_or(false);
    if fields.get("Type").map(String::as_str) != Some("Application") || flag("NoDisplay") || flag("Hidden") {
        return None;
    }
    let name = fields.get("Name")?.clone();
    let stem = desktop_id.strip_suffix(".desktop").unwrap_or(desktop_id).to_string();
    let app_id = fields.get("StartupWMClass").filter(|c| !c.is_empty()).cloned().unwrap_or(stem);
    Some(InstalledApp {
        app_id,
        name,
        desktop_id: desktop_id.to_string(),
        path: path.to_path_buf(),
        icon: fields.get("Icon").filter(|i| !i.is_empty()).cloned(),
    })
}

fn visit(dir: &Path, prefix: &str, seen: &mut BTreeMap<String, InstalledApp>, hidden: &mut std::collections::BTreeSet<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let file = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            visit(&path, &format!("{prefix}{file}-"), seen, hidden);
            continue;
        }
        if !file.ends_with(".desktop") {
            continue;
        }
        let desktop_id = format!("{prefix}{file}");
        // The first directory wins, so a user override hides the system file.
        if seen.contains_key(&desktop_id) || hidden.contains(&desktop_id) {
            continue;
        }
        match std::fs::read_to_string(&path).ok().and_then(|text| parse_desktop_entry(&text, &desktop_id, &path)) {
            Some(app) => {
                seen.insert(desktop_id, app);
            }
            None => {
                hidden.insert(desktop_id);
            }
        }
    }
}

/// Installed apps, one per app identifier, sorted by name.
pub fn scan() -> Vec<InstalledApp> {
    let mut seen = BTreeMap::new();
    let mut hidden = std::collections::BTreeSet::new();
    for dir in application_dirs() {
        visit(&dir, "", &mut seen, &mut hidden);
    }
    merged(seen.into_values().collect())
}

pub fn merged(apps: Vec<InstalledApp>) -> Vec<InstalledApp> {
    let mut by_id: BTreeMap<String, InstalledApp> = BTreeMap::new();
    for app in apps {
        by_id.entry(app.app_id.to_lowercase()).or_insert(app);
    }
    let mut list: Vec<InstalledApp> = by_id.into_values().collect();
    list.sort_by_cached_key(|a| a.name.to_lowercase());
    list
}

/// The installed app for a bound identifier, matched like the resolver.
pub fn find<'a>(apps: &'a [InstalledApp], app_id: &str) -> Option<&'a InstalledApp> {
    apps.iter().find(|a| a.app_id == app_id).or_else(|| {
        apps.iter().find(|a| a.app_id.eq_ignore_ascii_case(app_id) || a.desktop_id.trim_end_matches(".desktop").eq_ignore_ascii_case(app_id))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_entries() {
        let text = "[Desktop Entry]\nType=Application\nName=Firefox\nName[it]=Firefox IT\nIcon=firefox\nExec=firefox %u\n[Desktop Action new]\nName=New Window\n";
        let app = parse_desktop_entry(text, "firefox.desktop", Path::new("/x/firefox.desktop")).unwrap();
        assert_eq!((app.app_id.as_str(), app.name.as_str(), app.icon.as_deref()), ("firefox", "Firefox", Some("firefox")));
        let steam = "[Desktop Entry]\nType=Application\nName=Steam\nStartupWMClass=steam\n";
        assert_eq!(parse_desktop_entry(steam, "com.valvesoftware.Steam.desktop", Path::new("/s")).unwrap().app_id, "steam");
        assert!(parse_desktop_entry("[Desktop Entry]\nType=Application\nName=X\nNoDisplay=true\n", "x.desktop", Path::new("/x")).is_none());
        assert!(parse_desktop_entry("[Desktop Entry]\nType=Link\nName=X\n", "x.desktop", Path::new("/x")).is_none());
    }

    #[test]
    fn user_entries_override_system_ones() {
        let root = crate::model::store::tests::temp_dir("apps");
        let user = root.join("user/applications");
        let system = root.join("system/applications/kde");
        std::fs::create_dir_all(&user).unwrap();
        std::fs::create_dir_all(&system).unwrap();
        std::fs::write(user.join("kde-dolphin.desktop"), "[Desktop Entry]\nType=Application\nName=Hidden\nHidden=true\n").unwrap();
        std::fs::write(system.join("dolphin.desktop"), "[Desktop Entry]\nType=Application\nName=Dolphin\n").unwrap();
        std::fs::write(system.join("kate.desktop"), "[Desktop Entry]\nType=Application\nName=Kate\n").unwrap();
        let mut seen = BTreeMap::new();
        let mut hidden = std::collections::BTreeSet::new();
        visit(&root.join("user/applications"), "", &mut seen, &mut hidden);
        visit(&root.join("system/applications"), "", &mut seen, &mut hidden);
        let apps = merged(seen.into_values().collect());
        assert_eq!(apps.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), vec!["Kate"]);
        assert_eq!(apps[0].desktop_id, "kde-kate.desktop");
        assert_eq!(find(&apps, "KDE-KATE").unwrap().name, "Kate");
    }
}
