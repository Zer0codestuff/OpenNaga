//! Importing a profiles file. Keep Both is the default; incoming references
//! are rewritten so an imported switch never targets an unrelated profile.

use super::profile::{ButtonAction, ProfilesFile};
use super::store::ProfileFileStore;
use super::validation::Result;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportPolicy {
    KeepBoth,
    ReplaceMatching,
}

#[derive(Clone, Debug)]
pub struct ProfileImportDocument {
    pub file: ProfilesFile,
    pub shell_commands: usize,
}

impl ProfileImportDocument {
    pub fn open(path: &Path) -> Result<Self> {
        Ok(Self::new(ProfileFileStore::read_document(path)?))
    }

    pub fn new(file: ProfilesFile) -> Self {
        let shell_commands = file
            .profiles
            .values()
            .map(|p| {
                p.buttons.values().filter(|a| a.kind == "systemCommand").count()
                    + p.hypershift.as_ref().map(|h| h.values().filter(|a| a.kind == "systemCommand").count()).unwrap_or(0)
            })
            .sum();
        ProfileImportDocument { file, shell_commands }
    }

    pub fn matching_names(&self, existing: &BTreeMap<String, super::profile::Profile>) -> Vec<String> {
        self.file.profiles.keys().filter(|n| existing.contains_key(*n)).cloned().collect()
    }

    pub fn merged(&self, existing: &ProfilesFile, policy: ImportPolicy) -> ProfilesFile {
        let mut result = existing.clone();
        let mut names: HashMap<String, String> = HashMap::new();
        let mut reserved: BTreeSet<String> = existing.profiles.keys().chain(self.file.profiles.keys()).cloned().collect();
        for name in self.file.profiles.keys() {
            let mut unique = name.clone();
            if policy == ImportPolicy::KeepBoth && existing.profiles.contains_key(name) {
                unique = unique_name(name, &reserved);
            }
            reserved.insert(unique.clone());
            names.insert(name.clone(), unique);
        }
        let rewrite = |layer: &BTreeMap<String, ButtonAction>| -> BTreeMap<String, ButtonAction> {
            layer
                .iter()
                .map(|(key, action)| {
                    let mut action = action.clone();
                    if action.kind == "profileSwitch" {
                        if let Some(renamed) = action.profile.as_ref().and_then(|t| names.get(t)) {
                            action.profile = Some(renamed.clone());
                        }
                    }
                    (key.clone(), action)
                })
                .collect()
        };
        for (name, profile) in &self.file.profiles {
            let mut updated = profile.clone();
            if policy == ImportPolicy::ReplaceMatching && existing.profiles.get(name).map(|p| p.is_onboard_only()).unwrap_or(false) {
                updated.onboard_only = Some(true);
            }
            updated.buttons = rewrite(&profile.buttons);
            updated.hypershift = profile.hypershift.as_ref().map(rewrite);
            result.profiles.insert(names.get(name).cloned().unwrap_or_else(|| name.clone()), updated);
        }
        let mut bindings = result.settings.as_ref().and_then(|s| s.app_profiles.clone()).unwrap_or_default();
        if let Some(incoming) = self.file.settings.as_ref().and_then(|s| s.app_profiles.as_ref()) {
            for (app, target) in incoming {
                let Some(renamed) = names.get(target) else { continue };
                if !bindings.contains_key(app) || policy == ImportPolicy::ReplaceMatching {
                    bindings.insert(app.clone(), renamed.clone());
                }
            }
        }
        // Merge preserves the current selection and automatic switching. An
        // imported shell command cannot become active just by importing.
        if let Some(settings) = result.settings.as_mut() {
            settings.app_profiles = if bindings.is_empty() { None } else { Some(bindings) };
        } else if !bindings.is_empty() {
            result.settings = Some(super::profile::Settings { app_profiles: Some(bindings), ..Default::default() });
        }
        result
    }
}

pub fn unique_name(base: &str, reserved: &BTreeSet<String>) -> String {
    let mut suffix = 2;
    loop {
        let ending = format!(" {suffix}");
        let mut prefix = base.to_string();
        while prefix.len() + ending.len() > 256 {
            prefix.pop();
        }
        let candidate = prefix + &ending;
        if !reserved.contains(&candidate) {
            return candidate;
        }
        suffix += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::profile::{default_profiles, Profile, Settings};

    fn switch(to: &str) -> ButtonAction {
        ButtonAction { kind: "profileSwitch".into(), profile: Some(to.into()), ..Default::default() }
    }

    fn incoming() -> ProfilesFile {
        let mut file = ProfilesFile::default();
        let mut default = Profile::default();
        default.buttons.insert("1".into(), switch("Games"));
        default.buttons.insert("2".into(), ButtonAction { kind: "systemCommand".into(), command: Some("ls".into()), ..Default::default() });
        file.profiles.insert("Default".into(), default);
        file.profiles.insert("Games".into(), Profile::default());
        let mut apps = BTreeMap::new();
        apps.insert("steam".to_string(), "Games".to_string());
        apps.insert("firefox".to_string(), "Default".to_string());
        file.settings = Some(Settings { app_profiles: Some(apps), current_profile: Some("Games".into()), ..Default::default() });
        file
    }

    #[test]
    fn keep_both_renames_and_rewrites_references() {
        let document = ProfileImportDocument::new(incoming());
        assert_eq!(document.shell_commands, 1);
        let mut existing = default_profiles();
        existing.profiles.insert("Games".into(), Profile::default());
        let mut apps = BTreeMap::new();
        apps.insert("firefox".to_string(), "Navigation".to_string());
        existing.settings.as_mut().unwrap().app_profiles = Some(apps);
        let merged = document.merged(&existing, ImportPolicy::KeepBoth);
        assert!(merged.profiles.contains_key("Default 2"));
        assert!(merged.profiles.contains_key("Games 2"));
        assert_eq!(merged.profiles["Default 2"].buttons["1"].profile.as_deref(), Some("Games 2"));
        assert_eq!(merged.profiles["Default"], existing.profiles["Default"], "existing profile untouched");
        let bindings = merged.settings.as_ref().unwrap().app_profiles.as_ref().unwrap();
        assert_eq!(bindings["firefox"], "Navigation", "existing binding kept");
        assert_eq!(bindings["steam"], "Games 2");
        assert_eq!(merged.settings.as_ref().unwrap().current_profile.as_deref(), Some("Default"), "selection kept");
    }

    #[test]
    fn replace_keeps_mouse_import_protection() {
        let document = ProfileImportDocument::new(incoming());
        let mut existing = default_profiles();
        existing.profiles.get_mut("Default").unwrap().onboard_only = Some(true);
        let merged = document.merged(&existing, ImportPolicy::ReplaceMatching);
        assert!(merged.profiles["Default"].is_onboard_only());
        assert_eq!(merged.profiles["Default"].buttons["1"].profile.as_deref(), Some("Games"));
        assert_eq!(merged.settings.unwrap().app_profiles.unwrap()["firefox"], "Default");
    }

    #[test]
    fn unique_names_stay_within_limits() {
        let mut reserved = BTreeSet::new();
        reserved.insert("A 2".to_string());
        assert_eq!(unique_name("A", &reserved), "A 3");
        let long = "é".repeat(128);
        let name = unique_name(&long, &BTreeSet::new());
        assert!(name.len() <= 256 && name.ends_with(" 2"));
    }
}
