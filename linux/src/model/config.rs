//! Profiles, selection and settings: a port of the macOS `ConfigManager`.
//! Every change is a validated document transaction: it is saved first and
//! only then published. A failed save rolls memory back.

use super::actions::{ActionType, MappingLayer};
use super::import::{unique_name, ImportPolicy, ProfileImportDocument};
use super::prefs::Prefs;
use super::profile::{self, mapping_of, stored, ButtonAction, Mapping, Profile, ProfilesFile, Settings};
use super::scroll::ScrollSensitivity;
use super::store::{self, ProfileFileStore};
use super::validation::ProfileDataError;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// What the input engine should run now.
#[derive(Clone, Debug, PartialEq)]
pub struct MappingUpdate {
    pub standard: Mapping,
    pub hypershift: Mapping,
    /// Automatic switches keep held outputs; everything else releases them.
    pub preserve_holds: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct State {
    file: ProfilesFile,
    runtime: String,
}

pub struct ConfigManager {
    store: ProfileFileStore,
    pub prefs: Prefs,
    persist_prefs: bool,
    undo_history: Vec<State>,
    redo_history: Vec<State>,
    pub profiles: BTreeMap<String, Profile>,
    /// The profile the user selected: what the editor shows, what is saved
    /// and what Save to Mouse writes.
    pub current_profile_name: String,
    /// The profile whose mapping is live. Differs from the selected one only
    /// while an app binding applies; never persisted.
    pub runtime_profile_name: String,
    pub auto_switch_enabled: bool,
    pub app_profiles: BTreeMap<String, String>,
    show_notifications: Option<bool>,
    pub profiles_read_only: bool,
    pub last_error: Option<String>,
    mapping_sink: Option<Box<dyn FnMut(MappingUpdate)>>,
}

impl ConfigManager {
    pub fn new(path: PathBuf, prefs: Prefs, persist_prefs: bool) -> Self {
        ConfigManager {
            store: ProfileFileStore::new(path),
            prefs,
            persist_prefs,
            undo_history: Vec::new(),
            redo_history: Vec::new(),
            profiles: BTreeMap::new(),
            current_profile_name: "Default".into(),
            runtime_profile_name: "Default".into(),
            auto_switch_enabled: false,
            app_profiles: BTreeMap::new(),
            show_notifications: None,
            profiles_read_only: false,
            last_error: None,
            mapping_sink: None,
        }
    }

    pub fn set_mapping_sink(&mut self, sink: Box<dyn FnMut(MappingUpdate)>) {
        self.mapping_sink = Some(sink);
        self.update_runtime_mapping(false);
    }

    pub fn profiles_folder(&self) -> PathBuf {
        self.store.folder()
    }

    pub fn has_profile_backup(&self) -> bool {
        self.store.has_backup()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_history.is_empty() && !self.profiles_read_only
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_history.is_empty() && !self.profiles_read_only
    }

    pub fn scroll_sensitivity(&self) -> ScrollSensitivity {
        self.prefs.scroll()
    }

    pub fn load(&mut self) {
        self.last_error = None;
        let defaults = profile::default_profiles();
        let mut merged_profiles = defaults.profiles.clone();
        let mut merged_settings = defaults.settings.clone();
        let mut file_selection = None;
        match self.store.load() {
            Ok(Some(user)) => {
                merged_profiles = user.profiles;
                if user.settings.is_some() {
                    merged_settings = user.settings.clone();
                }
                file_selection = user.settings.and_then(|s| s.current_profile);
                self.profiles_read_only = false;
            }
            Ok(None) => self.profiles_read_only = false,
            Err(error) => {
                self.profiles_read_only = true;
                self.last_error = Some(format!("Unable to load profiles. The file is protected: {error}"));
                // A failed reload keeps the last usable in-memory profiles.
                if !self.profiles.is_empty() {
                    self.update_runtime_mapping(false);
                    return;
                }
            }
        }
        self.profiles = merged_profiles;
        // The document is authoritative; the preference is only a fallback for
        // old files without a saved selection.
        let saved_pref = self.prefs.current_profile.clone();
        self.current_profile_name = [file_selection, saved_pref, merged_settings.as_ref().and_then(|s| s.current_profile.clone())]
            .into_iter()
            .flatten()
            .find(|name| self.profiles.contains_key(name))
            .unwrap_or_else(|| "Default".into());
        if !self.profiles.contains_key(&self.current_profile_name) {
            self.current_profile_name = self.profiles.keys().next().cloned().unwrap_or_else(|| "Default".into());
        }
        self.auto_switch_enabled = merged_settings.as_ref().and_then(|s| s.auto_switch_profiles).unwrap_or(false);
        self.app_profiles = merged_settings
            .as_ref()
            .and_then(|s| s.app_profiles.clone())
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, p)| self.profiles.contains_key(p))
            .collect();
        self.show_notifications = merged_settings.and_then(|s| s.show_notifications);
        self.runtime_profile_name = self.current_profile_name.clone();
        self.undo_history.clear();
        self.redo_history.clear();
        self.update_runtime_mapping(false);
    }

    pub fn available_profiles(&self) -> Vec<String> {
        self.profiles.keys().cloned().collect()
    }

    pub fn current_profile_is_onboard_only(&self) -> bool {
        self.profiles.get(&self.current_profile_name).map(|p| p.is_onboard_only()).unwrap_or(false)
    }

    pub fn remapping_enabled(&self) -> bool {
        !self.profiles_read_only && !self.current_profile_is_onboard_only() && self.prefs.remapping_enabled
    }

    pub fn set_remapping_enabled(&mut self, enabled: bool) {
        self.prefs.remapping_enabled = enabled;
        self.save_prefs();
    }

    pub fn set_scroll_sensitivity(&mut self, sensitivity: ScrollSensitivity) {
        let s = sensitivity.clamped();
        self.prefs.scroll_vertical = s.vertical;
        self.prefs.scroll_horizontal = s.horizontal;
        self.save_prefs();
    }

    fn save_prefs(&self) {
        if self.persist_prefs {
            self.prefs.save();
        }
    }

    pub fn mapping_for_current(&self, layer: MappingLayer) -> Mapping {
        self.mapping_of(&self.current_profile_name, layer)
    }

    pub fn mapping_for_runtime(&self, layer: MappingLayer) -> Mapping {
        self.mapping_of(&self.runtime_profile_name, layer)
    }

    fn mapping_of(&self, name: &str, layer: MappingLayer) -> Mapping {
        self.profiles.get(name).map(|p| mapping_of(p, layer)).unwrap_or_default()
    }

    /// An explicit choice. It also takes over the live mapping until the next
    /// app change, so a manual switch is never undone by an app binding.
    pub fn set_current_profile(&mut self, name: &str) {
        if !self.profiles.contains_key(name) {
            return;
        }
        let name = name.to_string();
        self.change(false, |c| {
            c.current_profile_name = name.clone();
            c.runtime_profile_name = name;
        });
    }

    /// Makes `name` the live profile without selecting it. Held buttons keep
    /// the output they started. Nothing is saved.
    pub fn apply_automatic_profile(&mut self, name: &str) {
        if self.profiles_read_only
            || !self.profiles.contains_key(name)
            || self.profiles[name].is_onboard_only()
            || self.current_profile_is_onboard_only()
            || name == self.runtime_profile_name
        {
            return;
        }
        self.runtime_profile_name = name.to_string();
        self.update_runtime_mapping(true);
    }

    pub fn set_auto_switch_enabled(&mut self, enabled: bool) {
        if enabled != self.auto_switch_enabled {
            self.change(true, |c| c.auto_switch_enabled = enabled);
        }
    }

    pub fn set_app_profile(&mut self, profile: Option<&str>, app_id: &str) {
        if app_id.is_empty() || profile.map(|p| !self.profiles.contains_key(p)).unwrap_or(false) {
            return;
        }
        let profile = profile.map(str::to_string);
        let app = app_id.to_string();
        self.change(true, |c| match profile {
            Some(p) => {
                c.app_profiles.insert(app, p);
            }
            None => {
                c.app_profiles.remove(&app);
            }
        });
    }

    pub fn create_profile(&mut self, name: &str, based_on: Option<&str>) -> bool {
        let trimmed = name.trim().to_string();
        if trimmed.is_empty() || self.profiles.contains_key(&trimmed) {
            return false;
        }
        let base = based_on.and_then(|b| self.profiles.get(b).cloned()).unwrap_or_default();
        self.change(true, |c| {
            c.profiles.insert(trimmed.clone(), base);
            c.current_profile_name = trimmed.clone();
            c.runtime_profile_name = trimmed;
        })
    }

    /// Creates and selects a profile from a complete mapping, for example one
    /// read from the mouse. Returns the name used.
    pub fn create_profile_from(&mut self, base: &str, standard: &Mapping, hypershift: &Mapping, onboard_only: bool) -> Option<String> {
        let mut name = base.to_string();
        if self.profiles.contains_key(&name) {
            name = unique_name(base, &self.profiles.keys().cloned().collect());
        }
        let profile = Profile {
            buttons: stored(standard),
            onboard_only: if onboard_only { Some(true) } else { None },
            hypershift: if hypershift.is_empty() { None } else { Some(stored(hypershift)) },
        };
        let chosen = name.clone();
        if self.change(true, |c| {
            c.profiles.insert(chosen.clone(), profile);
            c.current_profile_name = chosen.clone();
            c.runtime_profile_name = chosen;
        }) {
            Some(name)
        } else {
            None
        }
    }

    pub fn duplicate_profile(&mut self, source: &str, new_name: &str) -> bool {
        self.profiles.contains_key(source) && self.create_profile(new_name, Some(source))
    }

    pub fn rename_profile(&mut self, old_name: &str, new_name: &str) -> bool {
        let new_trim = new_name.trim().to_string();
        if old_name == new_trim || new_trim.is_empty() || !self.profiles.contains_key(old_name) || self.profiles.contains_key(&new_trim) {
            return false;
        }
        let old = old_name.to_string();
        self.change(true, |c| {
            let existing = c.profiles.remove(&old).unwrap_or_default();
            c.profiles.insert(new_trim.clone(), existing);
            let retarget = |layer: &mut BTreeMap<String, ButtonAction>| {
                for action in layer.values_mut() {
                    if action.kind == "profileSwitch" && action.profile.as_deref() == Some(old.as_str()) {
                        *action = ButtonAction::from_action(&ActionType::ProfileSwitch {
                            profile: new_trim.clone(),
                            description: action.description.clone(),
                        });
                    }
                }
            };
            for profile in c.profiles.values_mut() {
                retarget(&mut profile.buttons);
                if let Some(h) = profile.hypershift.as_mut() {
                    retarget(h);
                }
            }
            for bound in c.app_profiles.values_mut() {
                if *bound == old {
                    *bound = new_trim.clone();
                }
            }
            if c.current_profile_name == old {
                c.current_profile_name = new_trim.clone();
            }
            if c.runtime_profile_name == old {
                c.runtime_profile_name = new_trim.clone();
            }
        })
    }

    pub fn delete_profile(&mut self, name: &str) -> bool {
        if !self.profiles.contains_key(name) || self.profiles.len() <= 1 {
            return false;
        }
        let name = name.to_string();
        self.change(true, |c| {
            c.profiles.remove(&name);
            c.app_profiles.retain(|_, p| *p != name);
            // Never leave a switch pointing at a deleted profile. Disabled
            // avoids leaking the original button's key after a target is removed.
            let disable = |layer: &mut BTreeMap<String, ButtonAction>| {
                for action in layer.values_mut() {
                    if action.kind == "profileSwitch" && action.profile.as_deref() == Some(name.as_str()) {
                        *action = ButtonAction::from_action(&ActionType::Disabled);
                    }
                }
            };
            for profile in c.profiles.values_mut() {
                disable(&mut profile.buttons);
                if let Some(h) = profile.hypershift.as_mut() {
                    disable(h);
                }
            }
            if c.current_profile_name == name {
                if let Some(next) = c.profiles.keys().next().cloned() {
                    c.current_profile_name = next.clone();
                    c.runtime_profile_name = next;
                }
            } else if c.runtime_profile_name == name {
                c.runtime_profile_name = c.current_profile_name.clone();
            }
        })
    }

    pub fn import_document(&mut self, document: &ProfileImportDocument, policy: ImportPolicy) -> Result<(), ProfileDataError> {
        let previous = self.state();
        let candidate = document.merged(&previous.file, policy);
        if self.change(true, |c| {
            c.adopt(&candidate);
            // Imported mouse profiles cannot remain an automatic runtime target.
            if c.profiles.get(&c.runtime_profile_name).map(|p| p.is_onboard_only()).unwrap_or(false) {
                c.runtime_profile_name = c.current_profile_name.clone();
            }
        }) {
            Ok(())
        } else {
            Err(ProfileDataError(self.last_error.clone().unwrap_or_else(|| "Unable to import profiles.".into())))
        }
    }

    pub fn export_all(&self, path: &Path) -> Result<(), ProfileDataError> {
        let data = store::encode(&self.state().file)?;
        store::write_private(&data, path)
    }

    /// Updates one control of the selected profile. The Hypershift layer
    /// cannot hold another Hypershift control.
    pub fn set_action(&mut self, index: i32, action: Option<ActionType>, layer: MappingLayer) {
        if !(1..=19).contains(&index) {
            return;
        }
        if layer == MappingLayer::Hypershift && action.as_ref().map(|a| a.is_hypershift()).unwrap_or(false) {
            return;
        }
        self.change(true, |c| {
            let name = c.current_profile_name.clone();
            let mut profile = c.profiles.get(&name).cloned().unwrap_or_default();
            let mut buttons = profile.buttons_in(layer);
            match &action {
                Some(action) => {
                    buttons.insert(index.to_string(), ButtonAction::from_action(action));
                }
                None => {
                    buttons.remove(&index.to_string());
                }
            }
            match layer {
                MappingLayer::Standard => profile.buttons = buttons,
                MappingLayer::Hypershift => profile.hypershift = if buttons.is_empty() { None } else { Some(buttons) },
            }
            c.profiles.insert(name, profile);
        });
    }

    pub fn undo(&mut self) -> bool {
        self.move_history(true)
    }

    pub fn redo(&mut self) -> bool {
        self.move_history(false)
    }

    fn move_history(&mut self, undoing: bool) -> bool {
        let target = if undoing { self.undo_history.last().cloned() } else { self.redo_history.last().cloned() };
        let Some(target) = target else { return false };
        let current = self.state();
        if !self.change(false, |c| c.restore(&target)) {
            return false;
        }
        if undoing {
            self.undo_history.pop();
            self.redo_history.push(current);
        } else {
            self.redo_history.pop();
            self.undo_history.push(current);
        }
        true
    }

    pub fn restore_profile_backup(&mut self) -> bool {
        match self.store.restore_backup() {
            Ok(recovered) => {
                self.adopt(&recovered);
                self.runtime_profile_name = self.current_profile_name.clone();
                self.profiles_read_only = false;
                self.undo_history.clear();
                self.redo_history.clear();
                self.prefs.current_profile = Some(self.current_profile_name.clone());
                self.save_prefs();
                self.update_runtime_mapping(false);
                self.last_error = None;
                true
            }
            Err(error) => {
                self.last_error = Some(format!("Unable to restore profiles: {error}"));
                false
            }
        }
    }

    /// Commit before publishing a mapping. Failed saves roll back memory,
    /// selection and history to the previous usable state.
    fn change(&mut self, record_undo: bool, edit: impl FnOnce(&mut ConfigManager)) -> bool {
        if self.profiles_read_only {
            return false;
        }
        let previous = self.state();
        edit(self);
        if self.state() == previous {
            self.last_error = None;
            return true;
        }
        match self.store.save(&self.state().file) {
            Ok(()) => {
                if self.prefs.current_profile.as_deref() != Some(self.current_profile_name.as_str()) {
                    self.prefs.current_profile = Some(self.current_profile_name.clone());
                    self.save_prefs();
                }
                if record_undo && !previous.file.profiles.is_empty() {
                    self.undo_history.push(previous);
                    if self.undo_history.len() > 12 {
                        self.undo_history.remove(0);
                    }
                    self.redo_history.clear();
                }
                self.update_runtime_mapping(false);
                self.last_error = None;
                true
            }
            Err(error) => {
                self.restore(&previous);
                self.last_error = Some(format!("Unable to save profiles: {error}"));
                false
            }
        }
    }

    fn persisted_settings(&self) -> Settings {
        Settings {
            current_profile: Some(self.current_profile_name.clone()),
            auto_switch_profiles: Some(self.auto_switch_enabled),
            show_notifications: self.show_notifications,
            app_profiles: if self.app_profiles.is_empty() { None } else { Some(self.app_profiles.clone()) },
        }
    }

    fn state(&self) -> State {
        State {
            file: ProfilesFile { profiles: self.profiles.clone(), settings: Some(self.persisted_settings()), format_version: None },
            runtime: self.runtime_profile_name.clone(),
        }
    }

    fn restore(&mut self, state: &State) {
        self.adopt(&state.file);
        self.runtime_profile_name = state.runtime.clone();
    }

    fn adopt(&mut self, file: &ProfilesFile) {
        self.profiles = file.profiles.clone();
        let selected = file.settings.as_ref().and_then(|s| s.current_profile.clone()).unwrap_or_else(|| self.current_profile_name.clone());
        self.current_profile_name = if self.profiles.contains_key(&selected) {
            selected
        } else {
            self.profiles.keys().next().cloned().unwrap_or_else(|| "Default".into())
        };
        if !self.profiles.contains_key(&self.runtime_profile_name) {
            self.runtime_profile_name = self.current_profile_name.clone();
        }
        self.auto_switch_enabled = file.settings.as_ref().and_then(|s| s.auto_switch_profiles).unwrap_or(false);
        self.app_profiles = file
            .settings
            .as_ref()
            .and_then(|s| s.app_profiles.clone())
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, p)| self.profiles.contains_key(p))
            .collect();
        self.show_notifications = file.settings.as_ref().and_then(|s| s.show_notifications);
    }

    fn update_runtime_mapping(&mut self, preserve_holds: bool) {
        let onboard = self.profiles_read_only
            || self.current_profile_is_onboard_only()
            || self.profiles.get(&self.runtime_profile_name).map(|p| p.is_onboard_only()).unwrap_or(false);
        let update = MappingUpdate {
            standard: if onboard { Mapping::new() } else { self.mapping_for_runtime(MappingLayer::Standard) },
            hypershift: if onboard { Mapping::new() } else { self.mapping_for_runtime(MappingLayer::Hypershift) },
            preserve_holds,
        };
        if let Some(sink) = self.mapping_sink.as_mut() {
            sink(update);
        }
    }

    /// Names that a profile switch or an app binding can target.
    pub fn assignable_profiles(&self) -> Vec<String> {
        self.profiles.iter().filter(|(_, p)| !p.is_onboard_only()).map(|(n, _)| n.clone()).collect()
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::actions::{KeyStroke, MouseAction};
    use crate::model::keys::mac;
    use crate::model::store::tests::temp_dir;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn manager(name: &str) -> (ConfigManager, PathBuf, Rc<RefCell<Vec<MappingUpdate>>>) {
        let dir = temp_dir(name);
        let path = dir.join("profiles.json");
        let mut config = ConfigManager::new(path.clone(), Prefs::default(), false);
        config.load();
        let updates = Rc::new(RefCell::new(Vec::new()));
        let sink = updates.clone();
        config.set_mapping_sink(Box::new(move |u| sink.borrow_mut().push(u)));
        updates.borrow_mut().clear();
        (config, path, updates)
    }

    fn copy_key() -> ActionType {
        ActionType::KeySequence { keys: vec![KeyStroke::new("c", &["ctrl"], Some(mac::C))], description: None }
    }

    #[test]
    fn first_launch_uses_defaults_without_writing() {
        let (config, path, _) = manager("first");
        assert_eq!(config.current_profile_name, "Default");
        assert_eq!(config.available_profiles(), vec!["Default", "Navigation"]);
        assert!(!path.exists());
        assert!(!config.remapping_enabled(), "remapping starts off");
    }

    #[test]
    fn edits_persist_publish_and_undo() {
        let (mut config, path, updates) = manager("edits");
        config.set_action(5, Some(copy_key()), MappingLayer::Standard);
        assert!(path.exists());
        assert_eq!(updates.borrow().last().unwrap().standard.get(&5), Some(&copy_key()));
        assert!(config.can_undo());
        assert!(config.undo());
        assert!(!config.mapping_for_current(MappingLayer::Standard).contains_key(&5));
        assert!(config.redo());
        assert_eq!(config.mapping_for_current(MappingLayer::Standard).get(&5), Some(&copy_key()));
        let mut reloaded = ConfigManager::new(path, Prefs::default(), false);
        reloaded.load();
        assert_eq!(reloaded.mapping_for_current(MappingLayer::Standard).get(&5), Some(&copy_key()));
    }

    #[test]
    fn history_is_bounded() {
        let (mut config, _, _) = manager("history");
        for i in 0..20 {
            config.set_action(1 + (i % 19), Some(ActionType::Disabled), MappingLayer::Standard);
            config.set_action(1 + (i % 19), None, MappingLayer::Standard);
        }
        let mut count = 0;
        while config.undo() {
            count += 1;
        }
        assert_eq!(count, 12);
    }

    #[test]
    fn hypershift_layer_rules() {
        let (mut config, _, _) = manager("hyper");
        config.set_action(2, Some(ActionType::Hypershift { description: None }), MappingLayer::Hypershift);
        assert!(config.profiles["Default"].hypershift.is_none());
        config.set_action(2, Some(copy_key()), MappingLayer::Hypershift);
        assert_eq!(config.mapping_for_current(MappingLayer::Hypershift).len(), 1);
        config.set_action(2, None, MappingLayer::Hypershift);
        assert!(config.profiles["Default"].hypershift.is_none(), "empty layer is removed");
    }

    #[test]
    fn rename_and_delete_cascade() {
        let (mut config, _, _) = manager("cascade");
        config.set_action(1, Some(ActionType::ProfileSwitch { profile: "Navigation".into(), description: None }), MappingLayer::Standard);
        config.set_app_profile(Some("Navigation"), "firefox");
        assert!(config.rename_profile("Navigation", " Browse "));
        assert_eq!(config.profiles["Default"].buttons["1"].profile.as_deref(), Some("Browse"));
        assert_eq!(config.app_profiles["firefox"], "Browse");
        assert!(!config.rename_profile("Browse", "Default"), "names stay unique");
        assert!(config.delete_profile("Browse"));
        assert_eq!(config.profiles["Default"].buttons["1"].kind, "disabled");
        assert!(config.app_profiles.is_empty());
        assert!(!config.delete_profile("Default"), "the last profile stays");
    }

    #[test]
    fn automatic_profile_preserves_holds_and_never_saves() {
        let (mut config, path, updates) = manager("auto");
        config.set_auto_switch_enabled(true);
        let before = std::fs::read(&path).unwrap();
        config.apply_automatic_profile("Navigation");
        assert_eq!(config.runtime_profile_name, "Navigation");
        assert_eq!(config.current_profile_name, "Default");
        let last = updates.borrow().last().cloned().unwrap();
        assert!(last.preserve_holds);
        assert_eq!(last.standard.get(&1), Some(&ActionType::Mouse { action: MouseAction::BrowserBack, description: Some("Back".into()) }));
        assert_eq!(std::fs::read(&path).unwrap(), before);
        config.set_current_profile("Default");
        assert_eq!(config.runtime_profile_name, "Default", "a manual choice takes over");
    }

    #[test]
    fn onboard_only_profiles_never_remap() {
        let (mut config, _, updates) = manager("onboard");
        let mut mapping = Mapping::new();
        mapping.insert(1, copy_key());
        let name = config.create_profile_from("From Mouse", &mapping, &Mapping::new(), true).unwrap();
        assert_eq!(name, "From Mouse");
        assert!(config.current_profile_is_onboard_only());
        assert!(updates.borrow().last().unwrap().standard.is_empty());
        assert_eq!(config.mapping_for_current(MappingLayer::Standard).len(), 1, "the editor still shows it");
        config.set_remapping_enabled(true);
        assert!(!config.remapping_enabled());
        let second = config.create_profile_from("From Mouse", &mapping, &Mapping::new(), true).unwrap();
        assert_eq!(second, "From Mouse 2");
        assert!(!config.assignable_profiles().contains(&second));
    }

    #[test]
    fn corrupt_file_pauses_editing_and_recovers() {
        let (mut config, path, _) = manager("recover");
        config.set_action(5, Some(ActionType::Disabled), MappingLayer::Standard);
        config.set_action(6, Some(ActionType::Disabled), MappingLayer::Standard);
        std::fs::write(&path, b"garbage").unwrap();
        config.load();
        assert!(config.profiles_read_only);
        assert!(config.last_error.is_some());
        assert_eq!(config.mapping_for_current(MappingLayer::Standard).len(), 4, "last usable profiles stay in memory");
        config.set_action(7, Some(ActionType::Disabled), MappingLayer::Standard);
        assert_eq!(std::fs::read(&path).unwrap(), b"garbage");
        assert!(config.restore_profile_backup());
        assert!(!config.profiles_read_only);
        let restored = config.mapping_for_current(MappingLayer::Standard);
        assert!(restored.contains_key(&5) && !restored.contains_key(&6), "the previous save is back");
    }

    #[test]
    fn import_keeps_both_by_default() {
        let (mut config, _, _) = manager("import");
        let mut incoming = ProfilesFile::default();
        incoming.profiles.insert("Default".into(), Profile::default());
        config.import_document(&ProfileImportDocument::new(incoming), ImportPolicy::KeepBoth).unwrap();
        assert!(config.profiles.contains_key("Default 2"));
        assert_eq!(config.current_profile_name, "Default");
    }
}
