//! Mouse memory: binding descriptors, the plan that turns a profile into
//! firmware functions, and the store that saves with a backup and restores.
//!
//! Wire facts from OpenRazer issue 2845 and OpenSnek USB_PROFILE_CRUD_SPEC.md,
//! independently verified on receiver 1532:00b4 on 2026-09-11 and
//! 2026-10-21 by the macOS app. No foreign code copied. Profile 0 is the
//! active view, profile 1 the stored bank. Request byte 2 of 0x8c / 0x0c
//! selects the layer (0 normal, 1 Hypershift).

use super::protocol::{RazerCommand, RazerError, Result, Session};
use crate::model::actions::*;
use crate::model::data_folder;
use crate::model::keys::{self, Mods};
use crate::model::layout::LayoutMap;
use crate::model::mapper::SystemPlan;
use crate::model::profile::Mapping;
use crate::model::store;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

pub type Functions = BTreeMap<u8, Vec<u8>>;

/// A 10-byte descriptor: profile, control ID, layer/status byte, 7 function bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub bytes: Vec<u8>,
}

impl Binding {
    pub fn new(bytes: &[u8], profile: u8, button_id: u8) -> Result<Binding> {
        if bytes.len() != 10 || bytes[0] != profile || bytes[1] != button_id {
            return Err(RazerError::Malformed("Invalid hardware assignment or one that refers to another button.".into()));
        }
        // Byte 2 selects the layer in requests. Replies do not echo it: a
        // Synapse-configured mouse reports 01 on some controls in both layers.
        // Store 0 and pass the layer separately; keeping the reply value would
        // send a restore to the wrong layer.
        let mut normalized = bytes.to_vec();
        normalized[2] = 0;
        Ok(Binding { bytes: normalized })
    }
}

pub fn get_button_ids() -> RazerCommand {
    RazerCommand::new(0x1f, 2, 0x84, vec![0; 22])
}

pub fn read_command(profile: u8, button_id: u8, layer: u8) -> Result<RazerCommand> {
    if profile > 1 || layer > 1 {
        return Err(RazerError::InvalidValue("Only the active and stored profiles and two layers are supported.".into()));
    }
    Ok(RazerCommand::new(0x1f, 2, 0x8c, vec![profile, button_id, layer, 0, 0, 0, 0, 0, 0, 0]))
}

pub fn write_command(binding: &Binding, layer: u8) -> Result<RazerCommand> {
    if layer > 1 {
        return Err(RazerError::InvalidValue("Unknown assignment layer.".into()));
    }
    let mut arguments = binding.bytes.clone();
    arguments[2] = layer;
    Ok(RazerCommand::new(0x1f, 2, 0x0c, arguments))
}

pub fn decode_button_ids(bytes: &[u8]) -> Result<Vec<u8>> {
    let invalid = || RazerError::Malformed("Invalid hardware button list.".into());
    if bytes.len() != 22 {
        return Err(invalid());
    }
    let count = bytes[0] as usize;
    if count == 0 || count >= bytes.len() {
        return Err(invalid());
    }
    let ids = bytes[1..=count].to_vec();
    if ids.contains(&0) || ids.iter().collect::<BTreeSet<_>>().len() != ids.len() {
        return Err(RazerError::Malformed("Duplicate or incomplete hardware button list.".into()));
    }
    Ok(ids)
}

pub fn read_bindings(session: &mut Session, profile: u8, layer: u8) -> Result<Vec<Binding>> {
    // Validate the bank before any USB traffic.
    read_command(profile, 1, layer)?;
    let ids = decode_button_ids(&session.execute(&get_button_ids())?)?;
    ids.iter()
        .map(|id| {
            let bytes = session.execute(&read_command(profile, *id, layer)?)?;
            Binding::new(&bytes, profile, *id)
        })
        .collect()
}

/// Read-only view for importing: control ID to its seven function bytes.
pub fn read_functions(session: &mut Session, profile: u8, layer: u8) -> Result<Functions> {
    let bindings = read_bindings(session, profile, layer)?;
    let mut expected: BTreeSet<u8> = BUTTON_IDS.iter().map(|(_, id)| *id).collect();
    expected.extend([9u8, 10]);
    if bindings.iter().map(|b| b.bytes[1]).collect::<BTreeSet<_>>() != expected {
        return Err(RazerError::Malformed("Incomplete or unexpected mouse button layout.".into()));
    }
    Ok(bindings.into_iter().map(|b| (b.bytes[1], b.bytes[3..].to_vec())).collect())
}

/// Hold-to-shift, verified on 1532:00b4 on 2026-10-21. The `0c 01 02`
/// variant stays latched after release, so it is never written.
pub const HYPERSHIFT_FUNCTION: [u8; 7] = [0x0c, 1, 1, 0, 0, 0, 0];

/// Grid IDs increase with the printed button numbers, not enumeration order.
pub const BUTTON_IDS: [(i32, u8); 19] = [
    (1, 0x40),
    (2, 0x41),
    (3, 0x42),
    (4, 0x43),
    (5, 0x44),
    (6, 0x45),
    (7, 0x46),
    (8, 0x47),
    (9, 0x48),
    (10, 0x49),
    (11, 0x4a),
    (12, 0x4b),
    (13, 0x0b),
    (14, 0x0c),
    (15, 0x34),
    (16, 0x35),
    (17, 3),
    (18, 1),
    (19, 2),
];

pub fn button_id(index: i32) -> Option<u8> {
    BUTTON_IDS.iter().find(|(i, _)| *i == index).map(|(_, id)| *id)
}

/// Layout and desktop facts a plan needs.
pub struct PlanContext<'a> {
    pub layout: &'a LayoutMap,
    pub system: &'a dyn Fn(SystemAction) -> SystemPlan,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OnboardProfilePlan {
    pub name: String,
    pub functions: Functions,
    /// Hypershift entries, written only when a standard control holds the
    /// Hypershift function; other controls then mirror their standard one.
    pub hypershift_functions: Functions,
    pub issues: Vec<String>,
}

fn unsupported(text: &str) -> RazerError {
    RazerError::InvalidValue(text.into())
}

impl OnboardProfilePlan {
    pub fn is_supported(&self) -> bool {
        !self.functions.is_empty() && self.issues.is_empty()
    }

    pub fn has_hypershift_key(&self) -> bool {
        self.functions.values().any(|f| f.as_slice() == HYPERSHIFT_FUNCTION)
    }

    pub fn new(name: &str, mapping: &Mapping, hypershift: &Mapping, context: &PlanContext) -> Self {
        let mut plan = OnboardProfilePlan { name: name.into(), ..Default::default() };
        for (index, action) in mapping {
            let Some(id) = button_id(*index) else {
                plan.issues.push(format!("Button {index}: unknown hardware control."));
                continue;
            };
            match function(action, context) {
                Ok(f) => {
                    plan.functions.insert(id, f);
                }
                Err(e) => plan.issues.push(format!("Button {index}: {e}")),
            }
        }
        if mapping.is_empty() {
            plan.issues.push("The profile is empty.".into());
        }
        if mapping.get(&18).map(|a| a.is_hypershift()).unwrap_or(false) {
            plan.issues.push("Button 18: left click cannot be the Hypershift key in mouse memory.".into());
        }
        if !plan.has_hypershift_key() {
            return plan;
        }
        for (id, f) in &plan.functions {
            if f.as_slice() == HYPERSHIFT_FUNCTION {
                plan.hypershift_functions.insert(*id, f.clone());
            }
        }
        for (index, action) in hypershift {
            let Some(id) = button_id(*index) else {
                plan.issues.push(format!("Hypershift button {index}: unknown hardware control."));
                continue;
            };
            if action.is_hypershift() || plan.hypershift_functions.contains_key(&id) {
                // A Hypershift key keeps its function in both layers.
                continue;
            }
            match function(action, context) {
                Ok(f) => {
                    plan.hypershift_functions.insert(id, f);
                }
                Err(e) => plan.issues.push(format!("Hypershift button {index}: {e}")),
            }
        }
        plan
    }
}

pub fn function(action: &ActionType, context: &PlanContext) -> Result<Vec<u8>> {
    match action {
        ActionType::Disabled => Ok(vec![0; 7]),
        ActionType::KeySequence { keys, .. } => {
            if keys.len() != 1 {
                return Err(unsupported("key sequences need the app."));
            }
            keyboard(&keys[0])
        }
        ActionType::TextSnippet { text, .. } => match context.layout.key_for_text(text) {
            Some(key) => keyboard(&key.stroke(&[])),
            None => Err(unsupported("the text cannot be typed as a single key in the current layout.")),
        },
        ActionType::Mouse { action, .. } => Ok(match action {
            MouseAction::DpiUp => vec![6, 1, 1, 0, 0, 0, 0],
            MouseAction::DpiDown => vec![6, 1, 2, 0, 0, 0, 0],
            MouseAction::LeftClick => vec![1, 1, 1, 0, 0, 0, 0],
            MouseAction::RightClick => vec![1, 1, 2, 0, 0, 0, 0],
            MouseAction::MiddleClick => vec![1, 1, 3, 0, 0, 0, 0],
            MouseAction::Button4 => vec![1, 1, 4, 0, 0, 0, 0],
            MouseAction::Button5 => vec![1, 1, 5, 0, 0, 0, 0],
            MouseAction::ScrollUp => vec![1, 1, 9, 0, 0, 0, 0],
            MouseAction::ScrollDown => vec![1, 1, 10, 0, 0, 0, 0],
            MouseAction::ScrollLeft => vec![0x0e, 3, 0x68, 0, 0x14, 0, 0],
            MouseAction::ScrollRight => vec![0x0e, 3, 0x69, 0, 0x14, 0, 0],
            // Alt with the arrows, as in the app.
            MouseAction::BrowserBack => keyboard_usage(0x50, Mods::ALT)?,
            MouseAction::BrowserForward => keyboard_usage(0x4f, Mods::ALT)?,
        }),
        ActionType::Audio { action, .. } => system(action.system(), context),
        ActionType::System { action, .. } => system(*action, context),
        ActionType::Hypershift { .. } => Ok(HYPERSHIFT_FUNCTION.to_vec()),
        ActionType::Application { .. } | ActionType::SystemCommand { .. } | ActionType::Macro { .. } | ActionType::ProfileSwitch { .. } => {
            Err(unsupported("this action needs OpenNaga running."))
        }
    }
}

fn system(action: SystemAction, context: &PlanContext) -> Result<Vec<u8>> {
    if let Some(usage) = action.media_usage() {
        return Ok(vec![0x0a, 2, 0, usage, 0, 0, 0]);
    }
    if let SystemPlan::Key { code, mods } = (context.system)(action) {
        if let Some(usage) = evdev_to_usage(code) {
            return keyboard_usage(usage, mods);
        }
    }
    Err(unsupported("this action cannot be stored in the mouse; use a keyboard shortcut."))
}

/// USB HID keyboard usage for an evdev key code (reverse of the kernel table).
pub fn evdev_to_usage(code: u16) -> Option<u8> {
    static TABLE: std::sync::OnceLock<HashMap<u16, u8>> = std::sync::OnceLock::new();
    TABLE
        .get_or_init(|| {
            let mut map = HashMap::new();
            for usage in (0x04u8..=0x73).chain(0xe0..=0xe7) {
                if let Some(evdev) = keys::usage_to_evdev(usage) {
                    map.entry(evdev).or_insert(usage);
                }
            }
            map
        })
        .get(&code)
        .copied()
}

fn modifier_mask(mods: Mods) -> u8 {
    let mut mask = 0;
    if mods.contains(Mods::CTRL) {
        mask |= 1;
    }
    if mods.contains(Mods::SHIFT) {
        mask |= 2;
    }
    if mods.contains(Mods::ALT) {
        mask |= 4;
    }
    if mods.contains(Mods::SUPER) {
        mask |= 8;
    }
    mask
}

fn keyboard_usage(usage: u8, mods: Mods) -> Result<Vec<u8>> {
    if mods.contains(Mods::FN) {
        return Err(unsupported("the Fn key needs the app."));
    }
    Ok(vec![2, 2, modifier_mask(mods), usage, 0, 0, 0])
}

pub fn keyboard(key: &KeyStroke) -> Result<Vec<u8>> {
    let usage = key
        .effective_code()
        .and_then(keys::mac_to_usage)
        .ok_or_else(|| unsupported("key has no supported USB HID equivalent."))?;
    for name in &key.modifiers {
        if Mods::from_names(&[name]).is_empty() || name.eq_ignore_ascii_case("fn") {
            return Err(unsupported(&format!("modifier {name} is not supported by the mouse.")));
        }
    }
    keyboard_usage(usage, key.mods())
}

/// Imported profiles run on the mouse, so native assignments are kept too.
pub fn mapping_from(functions: &Functions, layout: &LayoutMap) -> (Mapping, Vec<String>) {
    let mut mapping = Mapping::new();
    let mut issues = Vec::new();
    for (index, id) in BUTTON_IDS {
        let Some(f) = functions.get(&id) else { continue };
        if let Some(action) = action_from(f, layout) {
            mapping.insert(index, action);
        } else if f.len() == 7 && f[0] == 2 && (0xe0..=0xe7).contains(&f[3]) {
            issues.push(format!("Button {index}: a modifier key on its own cannot be imported."));
        } else {
            issues.push(format!("Button {index}: unsupported assignment {}.", hex(f)));
        }
    }
    // Wheel rotation has no editable index. Report a custom binding rather
    // than presenting a partial import as a complete editable layout.
    let editable: BTreeSet<u8> = BUTTON_IDS.iter().map(|(_, id)| *id).collect();
    for (id, f) in functions {
        if editable.contains(id) {
            continue;
        }
        let default: Option<Vec<u8>> = match id {
            9 => Some(vec![1, 1, 9, 0, 0, 0, 0]),
            10 => Some(vec![1, 1, 10, 0, 0, 0, 0]),
            _ => None,
        };
        if default.as_ref() != Some(f) {
            issues.push(format!("Control 0x{id:02x}: this assignment cannot be edited in OpenNaga."));
        }
    }
    (mapping, issues)
}

/// Hypershift entries that differ from the standard layer. Entries equal to
/// the standard function read as "Same as standard".
pub fn hypershift_mapping_from(standard: &Functions, hypershift: &Functions, layout: &LayoutMap) -> (Mapping, Vec<String>) {
    if !standard.values().any(|f| f.as_slice() == HYPERSHIFT_FUNCTION) {
        return (Mapping::new(), Vec::new());
    }
    let mut mapping = Mapping::new();
    let mut issues = Vec::new();
    for (index, id) in BUTTON_IDS {
        let Some(f) = hypershift.get(&id) else { continue };
        if standard.get(&id) == Some(f) || standard.get(&id).map(|s| s.as_slice() == HYPERSHIFT_FUNCTION).unwrap_or(false) {
            continue;
        }
        match action_from(f, layout) {
            Some(action) if !action.is_hypershift() => {
                mapping.insert(index, action);
            }
            _ => issues.push(format!("Hypershift button {index}: unsupported assignment {}.", hex(f))),
        }
    }
    (mapping, issues)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

pub fn action_from(f: &[u8], layout: &LayoutMap) -> Option<ActionType> {
    if f.len() != 7 {
        return None;
    }
    if f.iter().all(|b| *b == 0) {
        return Some(ActionType::Disabled);
    }
    let mouse = |action: MouseAction| Some(ActionType::Mouse { action, description: None });
    let action = match (f[0], f[1]) {
        (1, 1) => match f[2] {
            1 => mouse(MouseAction::LeftClick),
            2 => mouse(MouseAction::RightClick),
            3 => mouse(MouseAction::MiddleClick),
            4 => mouse(MouseAction::Button4),
            5 => mouse(MouseAction::Button5),
            9 => mouse(MouseAction::ScrollUp),
            10 => mouse(MouseAction::ScrollDown),
            _ => None,
        },
        (6, 1) => match f[2] {
            1 => mouse(MouseAction::DpiUp),
            2 => mouse(MouseAction::DpiDown),
            _ => None,
        },
        (0x0e, 3) => match f[2] {
            0x68 => mouse(MouseAction::ScrollLeft),
            0x69 => mouse(MouseAction::ScrollRight),
            _ => None,
        },
        (0x0c, 1) if f[2] == 1 => Some(ActionType::Hypershift { description: None }),
        (0x0a, 2) => SystemAction::from_media_usage(f[3]).map(|action| ActionType::System { action, description: None }),
        (2, 1) | (2, 2) => {
            // Factory bindings use length 1 without a modifier byte but keep
            // the usage in byte 3.
            let mask = if f[1] == 2 { f[2] } else { 0 };
            let code = keys::usage_to_mac(f[3]);
            match code {
                Some(code) if mask & 0xf0 == 0 => {
                    let names: Vec<String> = [(1u8, "ctrl"), (2, "shift"), (4, "alt"), (8, "cmd")]
                        .iter()
                        .filter(|(bit, _)| mask & bit != 0)
                        .map(|(_, n)| n.to_string())
                        .collect();
                    Some(ActionType::KeySequence { keys: vec![layout.captured_stroke(code, names)], description: None })
                }
                _ => None,
            }
        }
        _ => None,
    }?;
    // Every byte must round trip, including consumer usage high bytes and
    // scroll-repeat parameters; unknown data is not guessed.
    let context = PlanContext { layout, system: &|_| SystemPlan::Unavailable(String::new()) };
    let mut encoded = function(&action, &context).ok()?;
    if f[0] == 2 && f[1] == 1 {
        encoded[1] = 1;
    }
    if encoded.as_slice() == f {
        Some(action)
    } else {
        None
    }
}

// MARK: - Store

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub identity: String,
    pub original: Vec<Vec<u8>>,
    pub name: String,
    pub pending: bool,
    /// Hypershift backup, captured before OpenNaga first writes that layer.
    #[serde(rename = "originalHypershift", default, skip_serializing_if = "Option::is_none")]
    pub original_hypershift: Option<Vec<Vec<u8>>>,
}

pub fn journal_path() -> PathBuf {
    data_folder::file("onboard-profile.json")
}

/// A pending or unreadable journal also blocks software interception.
pub fn is_active() -> bool {
    journal_path().exists()
}

pub fn saved_name() -> Option<String> {
    load(&journal_path()).ok().filter(|s| !s.pending).map(|s| s.name)
}

/// The backup holds this model's original assignments, so it follows the
/// receiver model (vendor:product), not the USB port it was saved on.
pub fn same_receiver(a: &str, b: &str) -> bool {
    let prefix = |s: &str| s.split(':').take(2).map(str::to_lowercase).collect::<Vec<_>>();
    prefix(a) == prefix(b)
}

pub fn load(path: &Path) -> Result<State> {
    let data = std::fs::read(path).map_err(|e| RazerError::Malformed(format!("Mouse backup unreadable: {e}.")))?;
    let state: State = serde_json::from_slice(&data).map_err(|e| RazerError::Malformed(format!("Mouse backup unreadable: {e}.")))?;
    if state.identity.is_empty() {
        return Err(RazerError::Malformed("Incomplete mouse backup.".into()));
    }
    let original = validated(&state.original)?;
    let hypershift = state.original_hypershift.as_ref().map(|h| validated(h)).transpose()?;
    if let Some(h) = &hypershift {
        let ids = |layer: &[Vec<u8>]| layer.iter().map(|b| b[1]).collect::<BTreeSet<_>>();
        if ids(h) != ids(&original) {
            return Err(RazerError::Malformed("The Hypershift backup does not match the mouse controls.".into()));
        }
    }
    Ok(State { original, original_hypershift: hypershift, ..state })
}

fn validated(layer: &[Vec<u8>]) -> Result<Vec<Vec<u8>>> {
    if layer.len() != 21 {
        return Err(RazerError::Malformed("Incomplete mouse backup.".into()));
    }
    let mut ids = BTreeSet::new();
    layer
        .iter()
        .map(|bytes| {
            if bytes.len() != 10 {
                return Err(RazerError::Malformed("Invalid mouse backup.".into()));
            }
            if !ids.insert(bytes[1]) {
                return Err(RazerError::Malformed("Duplicate button in the backup.".into()));
            }
            Ok(Binding::new(bytes, 1, bytes[1])?.bytes)
        })
        .collect()
}

fn persist(state: &State, path: &Path) -> Result<()> {
    store::write_json(state, path).map_err(|e| RazerError::Transport(format!("Could not write the mouse backup: {e}")))
}

struct Write {
    bytes: Vec<u8>,
    layer: u8,
    previous: Option<Vec<u8>>,
}

pub fn save(plan: &OnboardProfilePlan, session: &mut Session, identity: &str, path: &Path) -> Result<()> {
    if !plan.is_supported() {
        return Err(RazerError::InvalidValue(plan.issues.join("\n")));
    }
    if session.read_mode()? != 0 {
        return Err(RazerError::InvalidValue("Turn off driver mode before saving to the mouse.".into()));
    }
    let previous_data = if path.exists() { Some(std::fs::read(path).map_err(|e| RazerError::Transport(e.to_string()))?) } else { None };
    let previous = if previous_data.is_some() { Some(load(path)?) } else { None };
    if let Some(p) = &previous {
        if !same_receiver(&p.identity, identity) || p.pending {
            return Err(RazerError::InvalidValue("Restore the pending backup with the original receiver first.".into()));
        }
    }
    let before: Vec<Vec<u8>> = read_bindings(session, 1, 0)?.into_iter().map(|b| b.bytes).collect();
    if before.len() != 21 {
        return Err(RazerError::Malformed("Unexpected button layout.".into()));
    }
    let original = previous.as_ref().map(|p| p.original.clone()).unwrap_or_else(|| before.clone());
    let ids = |layer: &[Vec<u8>]| layer.iter().map(|b| b[1]).collect::<BTreeSet<_>>();
    let before_ids = ids(&before);
    if ids(&original) != before_ids || !plan.functions.keys().all(|id| before_ids.contains(id)) {
        return Err(RazerError::Malformed("The backup or profile does not match the mouse buttons.".into()));
    }
    let targets: Vec<Vec<u8>> = original
        .iter()
        .map(|bytes| match plan.functions.get(&bytes[1]) {
            Some(f) => [vec![1, bytes[1], 0], f.clone()].concat(),
            None => bytes.clone(),
        })
        .collect();
    // The Hypershift layer is read and written only when this save uses it or
    // an earlier save changed it. Otherwise it is never touched.
    let mut original_hypershift = previous.as_ref().and_then(|p| p.original_hypershift.clone());
    let mut hypershift_before: Vec<Vec<u8>> = Vec::new();
    let mut hypershift_targets: Vec<Vec<u8>> = Vec::new();
    if plan.has_hypershift_key() || original_hypershift.is_some() {
        hypershift_before = read_bindings(session, 1, 1)?.into_iter().map(|b| b.bytes).collect();
        if ids(&hypershift_before) != before_ids || !plan.hypershift_functions.keys().all(|id| before_ids.contains(id)) {
            return Err(RazerError::Malformed("The Hypershift layer does not match the mouse buttons.".into()));
        }
        if original_hypershift.is_none() {
            original_hypershift = Some(hypershift_before.clone());
        }
        hypershift_targets = if plan.has_hypershift_key() {
            // Controls without a Hypershift action keep their standard function.
            targets
                .iter()
                .map(|t| match plan.hypershift_functions.get(&t[1]) {
                    Some(f) => [vec![1, t[1], 0], f.clone()].concat(),
                    None => t.clone(),
                })
                .collect()
        } else {
            original_hypershift.clone().unwrap_or_default()
        };
    }
    let before_by_id: HashMap<u8, Vec<u8>> = before.iter().map(|b| (b[1], b.clone())).collect();
    let hypershift_before_by_id: HashMap<u8, Vec<u8>> = hypershift_before.iter().map(|b| (b[1], b.clone())).collect();
    // A Hypershift layer write sets reply byte 2 to 01 on that control, and a
    // later normal-layer write clears it (observed on 1532:00b4, 2026-10-21).
    // Write the Hypershift layer first, then the normal layer of every
    // control touched in either layer, so the flag returns to 00.
    let hypershift_changed: Vec<&Vec<u8>> = hypershift_targets.iter().filter(|t| hypershift_before_by_id.get(&t[1]) != Some(t)).collect();
    let shifted_ids: BTreeSet<u8> = hypershift_changed.iter().map(|t| t[1]).collect();
    let mut changed: Vec<Write> = hypershift_changed
        .iter()
        .map(|t| Write { bytes: (*t).clone(), layer: 1, previous: hypershift_before_by_id.get(&t[1]).cloned() })
        .collect();
    changed.extend(
        targets
            .iter()
            .filter(|t| before_by_id.get(&t[1]) != Some(t) || shifted_ids.contains(&t[1]))
            .map(|t| Write { bytes: t.clone(), layer: 0, previous: before_by_id.get(&t[1]).cloned() }),
    );
    let mut state = State {
        identity: previous.as_ref().map(|p| p.identity.clone()).unwrap_or_else(|| identity.to_string()),
        original,
        name: plan.name.clone(),
        pending: true,
        original_hypershift,
    };
    persist(&state, path)?;
    let attempt = (|| -> Result<()> {
        for change in &changed {
            write(&change.bytes, change.layer, session)?;
        }
        // Also verify the active mirror, including an unchanged saved profile.
        for target in &targets {
            verify(target, 0, 0, session)?;
        }
        for target in &hypershift_targets {
            verify(target, 0, 1, session)?;
        }
        Ok(())
    })();
    match attempt {
        Ok(()) => {
            state.pending = false;
            persist(&state, path)
        }
        Err(failure) => {
            let rollback = (|| -> Result<()> {
                // Include the uncertain write: an ACK timeout can follow a successful SET.
                for change in changed.iter().rev() {
                    if let Some(old) = &change.previous {
                        write(old, change.layer, session)?;
                    }
                }
                match &previous_data {
                    Some(data) => store::write_private(data, path).map_err(|e| RazerError::Transport(e.0)),
                    None => std::fs::remove_file(path).map_err(|e| RazerError::Transport(e.to_string())),
                }
            })();
            if let Err(error) = rollback {
                return Err(RazerError::Transport(format!(
                    "Save interrupted. The backup was kept; use Restore Previous Assignments. {error}"
                )));
            }
            Err(failure)
        }
    }
}

pub fn restore(session: &mut Session, identity: &str, path: &Path) -> Result<()> {
    let mut state = load(path)?;
    if !same_receiver(&state.identity, identity) {
        return Err(RazerError::InvalidValue("Connect the receiver that was used for the save.".into()));
    }
    if session.read_mode()? != 0 {
        return Err(RazerError::InvalidValue("Turn off driver mode before restoring.".into()));
    }
    let current: Vec<Vec<u8>> = read_bindings(session, 1, 0)?.into_iter().map(|b| b.bytes).collect();
    let ids = |layer: &[Vec<u8>]| layer.iter().map(|b| b[1]).collect::<BTreeSet<_>>();
    if ids(&current) != ids(&state.original) {
        return Err(RazerError::Malformed("The backup does not match the mouse controls.".into()));
    }
    state.pending = true;
    persist(&state, path)?;
    let current_by_id: HashMap<u8, Vec<u8>> = current.iter().map(|b| (b[1], b.clone())).collect();
    // Hypershift layer first; its controls then get a normal-layer write
    // that clears the reply flag (see save).
    let mut shifted_ids = BTreeSet::new();
    if let Some(hypershift) = &state.original_hypershift {
        let current_hypershift: HashMap<u8, Vec<u8>> = read_bindings(session, 1, 1)?.into_iter().map(|b| (b.bytes[1], b.bytes)).collect();
        for original in hypershift {
            if current_hypershift.get(&original[1]) != Some(original) {
                write(original, 1, session)?;
                shifted_ids.insert(original[1]);
            }
            verify(original, 0, 1, session)?;
        }
    }
    for original in &state.original {
        if current_by_id.get(&original[1]) != Some(original) || shifted_ids.contains(&original[1]) {
            write(original, 0, session)?;
        }
        verify(original, 0, 0, session)?;
    }
    std::fs::remove_file(path).map_err(|e| RazerError::Transport(e.to_string()))
}

pub fn write(bytes: &[u8], layer: u8, session: &mut Session) -> Result<()> {
    if bytes.len() != 10 {
        return Err(RazerError::InvalidValue("Invalid assignment to write.".into()));
    }
    let binding = Binding::new(bytes, 1, bytes[1])?;
    session.execute(&write_command(&binding, layer)?)?;
    verify(&binding.bytes, 1, layer, session)
}

fn verify(bytes: &[u8], profile: u8, layer: u8, session: &mut Session) -> Result<()> {
    let mut expected = bytes.to_vec();
    expected[0] = profile;
    let actual = session.execute(&read_command(profile, bytes[1], layer)?)?;
    // Compare as normalized bindings: reply byte 2 is a status, not the layer.
    match Binding::new(&actual, profile, bytes[1]) {
        Ok(binding) if binding.bytes == expected => Ok(()),
        _ => Err(RazerError::Readback),
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::hw::protocol::{checksum, RazerTransport, REPORT_LENGTH};
    use crate::model::keys::mac;

    pub const IDS: [u8; 21] = [1, 2, 3, 9, 10, 0x0b, 0x0c, 0x34, 0x35, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b];

    /// A mouse with a stored bank mirrored to the active view, two layers,
    /// driver mode and optional failure injection.
    pub struct FakeMouse {
        pub layers: [BTreeMap<u8, Vec<u8>>; 2],
        pub mode: u8,
        pub fail_after_writes: Option<usize>,
        pub writes: usize,
        pub hypershift_reads: usize,
    }

    impl FakeMouse {
        pub fn factory() -> Self {
            let mut normal = BTreeMap::new();
            for id in IDS {
                let function = match id {
                    1 => vec![1, 1, 1, 0, 0, 0, 0],
                    2 => vec![1, 1, 2, 0, 0, 0, 0],
                    3 => vec![1, 1, 3, 0, 0, 0, 0],
                    9 => vec![1, 1, 9, 0, 0, 0, 0],
                    10 => vec![1, 1, 10, 0, 0, 0, 0],
                    0x0b => vec![6, 1, 1, 0, 0, 0, 0],
                    0x0c => vec![6, 1, 2, 0, 0, 0, 0],
                    0x34 => vec![0x0e, 3, 0x68, 0, 0x14, 0, 0],
                    0x35 => vec![0x0e, 3, 0x69, 0, 0x14, 0, 0],
                    // Factory keyboard bindings use length 1: 1 to 0, minus, equals.
                    0x4a => vec![2, 1, 0, 0x2d, 0, 0, 0],
                    0x4b => vec![2, 1, 0, 0x2e, 0, 0, 0],
                    _ => vec![2, 1, 0, 0x1e + (id - 0x40), 0, 0, 0],
                };
                normal.insert(id, function);
            }
            let hypershift = normal.clone();
            FakeMouse { layers: [normal, hypershift], mode: 0, fail_after_writes: None, writes: 0, hypershift_reads: 0 }
        }

        fn reply(&self, request: &[u8], status: u8, arguments: &[u8]) -> Vec<u8> {
            let mut bytes = vec![0u8; REPORT_LENGTH];
            bytes[0] = status;
            bytes[1] = request[1];
            bytes[5] = arguments.len() as u8;
            bytes[6] = request[6];
            bytes[7] = request[7];
            bytes[8..8 + arguments.len()].copy_from_slice(arguments);
            bytes[88] = checksum(&bytes);
            bytes
        }
    }

    impl RazerTransport for FakeMouse {
        fn identity(&self) -> String {
            "1532:00b4:usb-test".into()
        }
        fn exchange(&mut self, request: &[u8]) -> Result<Vec<u8>> {
            let args = &request[8..8 + request[5] as usize];
            match (request[6], request[7]) {
                (0, 0x84) => Ok(self.reply(request, 2, &[self.mode, 0])),
                (2, 0x84) => {
                    let mut list = vec![IDS.len() as u8];
                    list.extend(IDS);
                    list.resize(22, 0);
                    Ok(self.reply(request, 2, &list))
                }
                (2, 0x8c) => {
                    let (profile, id, layer) = (args[0], args[1], args[2] as usize);
                    if layer == 1 {
                        self.hypershift_reads += 1;
                    }
                    let mut bytes = vec![profile, id, 0];
                    bytes.extend(self.layers[layer][&id].clone());
                    Ok(self.reply(request, 2, &bytes))
                }
                (2, 0x0c) => {
                    if let Some(limit) = self.fail_after_writes {
                        if self.writes >= limit {
                            self.fail_after_writes = None;
                            return Err(RazerError::Transport("USB timeout. Reconnect the receiver.".into()));
                        }
                    }
                    self.writes += 1;
                    let layer = args[2] as usize;
                    self.layers[layer].insert(args[1], args[3..10].to_vec());
                    Ok(self.reply(request, 2, args))
                }
                _ => Ok(self.reply(request, 5, args)),
            }
        }
    }

    fn context() -> (LayoutMap, Box<dyn Fn(SystemAction) -> SystemPlan>) {
        (
            LayoutMap::us(),
            Box::new(|action| match action {
                SystemAction::Screenshot => SystemPlan::Key { code: 99, mods: Mods::NONE },
                SystemAction::NextSpace => SystemPlan::Command(vec!["hyprctl".into()]),
                _ => SystemPlan::Unavailable(String::new()),
            }),
        )
    }

    fn plan(mapping: Vec<(i32, ActionType)>, hypershift: Vec<(i32, ActionType)>) -> OnboardProfilePlan {
        let (layout, system) = context();
        let ctx = PlanContext { layout: &layout, system: system.as_ref() };
        OnboardProfilePlan::new("Test", &mapping.into_iter().collect(), &hypershift.into_iter().collect(), &ctx)
    }

    fn key(name: &str, code: u16, mods: &[&str]) -> ActionType {
        ActionType::KeySequence { keys: vec![KeyStroke::new(name, mods, Some(code))], description: None }
    }

    #[test]
    fn functions_for_supported_actions() {
        let p = plan(
            vec![
                (1, key("escape", mac::ESCAPE, &[])),
                (2, key("c", mac::C, &["ctrl", "shift"])),
                (3, ActionType::System { action: SystemAction::Mute, description: None }),
                (4, ActionType::System { action: SystemAction::Screenshot, description: None }),
                (5, ActionType::TextSnippet { text: "q".into(), description: None }),
                (13, ActionType::Mouse { action: MouseAction::DpiUp, description: None }),
                (6, ActionType::Disabled),
            ],
            vec![],
        );
        assert!(plan_is_ok(&p), "{:?}", p.issues);
        assert_eq!(p.functions[&0x40], vec![2, 2, 0, 0x29, 0, 0, 0]);
        assert_eq!(p.functions[&0x41], vec![2, 2, 3, 0x06, 0, 0, 0]);
        assert_eq!(p.functions[&0x42], vec![0x0a, 2, 0, 0xe2, 0, 0, 0]);
        assert_eq!(p.functions[&0x43], vec![2, 2, 0, 0x46, 0, 0, 0]);
        assert_eq!(p.functions[&0x44], vec![2, 2, 0, 0x14, 0, 0, 0]);
        assert_eq!(p.functions[&0x0b], vec![6, 1, 1, 0, 0, 0, 0]);
        assert_eq!(p.functions[&0x45], vec![0; 7]);
    }

    fn plan_is_ok(p: &OnboardProfilePlan) -> bool {
        p.is_supported()
    }

    #[test]
    fn unsupported_actions_block_the_whole_save() {
        let p = plan(
            vec![
                (1, ActionType::Macro { steps: vec![], description: None }),
                (2, ActionType::System { action: SystemAction::NextSpace, description: None }),
                (3, ActionType::KeySequence { keys: vec![KeyStroke::new("a", &[], Some(mac::A)); 2], description: None }),
                (4, ActionType::TextSnippet { text: "ab".into(), description: None }),
                (18, ActionType::Hypershift { description: None }),
            ],
            vec![],
        );
        assert!(!p.is_supported());
        assert_eq!(p.issues.len(), 5, "{:?}", p.issues);
        assert!(!plan(vec![], vec![]).is_supported());
    }

    #[test]
    fn imported_functions_round_trip() {
        let mouse = FakeMouse::factory();
        let functions: Functions = mouse.layers[0].clone();
        let (mapping, issues) = mapping_from(&functions, &LayoutMap::us());
        assert!(issues.is_empty(), "{issues:?}");
        assert_eq!(mapping.len(), 19);
        assert_eq!(mapping[&1], key("1", mac::N1, &[]));
        assert_eq!(mapping[&11], key("-", mac::MINUS, &[]));
        assert_eq!(mapping[&15], ActionType::Mouse { action: MouseAction::ScrollLeft, description: None });
        let mut custom = functions.clone();
        custom.insert(0x40, vec![2, 2, 0, 0xe0, 0, 0, 0]);
        custom.insert(0x41, vec![0x0e, 3, 0x68, 0, 0x15, 0, 0]);
        custom.insert(9, vec![1, 1, 10, 0, 0, 0, 0]);
        let (mapping, issues) = mapping_from(&custom, &LayoutMap::us());
        assert_eq!(mapping.len(), 17);
        assert_eq!(issues.len(), 3, "{issues:?}");
    }

    #[test]
    fn save_writes_only_changes_and_verifies() {
        let dir = crate::model::store::tests::temp_dir("onboard-save");
        let path = dir.join("onboard-profile.json");
        let mut mouse = FakeMouse::factory();
        let p = plan(vec![(1, key("escape", mac::ESCAPE, &[])), (13, ActionType::Mouse { action: MouseAction::DpiUp, description: None })], vec![]);
        {
            let mut session = Session::without_pauses(&mut mouse);
            save(&p, &mut session, "1532:00b4:usb-a", &path).unwrap();
        }
        assert_eq!(mouse.writes, 1, "DPI up was already stored");
        assert_eq!(mouse.hypershift_reads, 0, "the Hypershift layer is untouched");
        assert_eq!(mouse.layers[0][&0x40], vec![2, 2, 0, 0x29, 0, 0, 0]);
        let state = load(&path).unwrap();
        assert!(!state.pending);
        assert_eq!(saved_name_at(&path), Some("Test".into()));
        assert_eq!(state.original.iter().find(|b| b[1] == 0x40).unwrap()[3..], [2, 1, 0, 0x1e, 0, 0, 0]);
        // A second save keeps the first original and accepts another port.
        let p2 = plan(vec![(2, key("tab", mac::TAB, &[]))], vec![]);
        {
            let mut session = Session::without_pauses(&mut mouse);
            save(&p2, &mut session, "1532:00b4:usb-b", &path).unwrap();
        }
        assert_eq!(load(&path).unwrap().original, state.original);
        {
            let mut session = Session::without_pauses(&mut mouse);
            restore(&mut session, "1532:00b4:usb-c", &path).unwrap();
        }
        assert!(!path.exists());
        assert_eq!(mouse.layers[0], FakeMouse::factory().layers[0]);
    }

    fn saved_name_at(path: &Path) -> Option<String> {
        load(path).ok().filter(|s| !s.pending).map(|s| s.name)
    }

    #[test]
    fn failed_save_rolls_back() {
        let dir = crate::model::store::tests::temp_dir("onboard-rollback");
        let path = dir.join("onboard-profile.json");
        let mut mouse = FakeMouse::factory();
        mouse.fail_after_writes = Some(1);
        let p = plan(vec![(1, key("escape", mac::ESCAPE, &[])), (2, key("tab", mac::TAB, &[]))], vec![]);
        let result = {
            let mut session = Session::without_pauses(&mut mouse);
            save(&p, &mut session, "1532:00b4:usb-a", &path)
        };
        assert!(result.is_err());
        assert!(!path.exists(), "no backup remains after a clean rollback");
        assert_eq!(mouse.layers[0], FakeMouse::factory().layers[0]);
    }

    #[test]
    fn hypershift_layer_saves_first_and_restores() {
        let dir = crate::model::store::tests::temp_dir("onboard-hyper");
        let path = dir.join("onboard-profile.json");
        let mut mouse = FakeMouse::factory();
        let p = plan(
            vec![(12, ActionType::Hypershift { description: None }), (1, key("a", mac::A, &[]))],
            vec![(1, key("b", mac::B, &[])), (12, key("z", mac::Z, &[]))],
        );
        assert!(p.is_supported(), "{:?}", p.issues);
        assert_eq!(p.hypershift_functions[&0x4b], HYPERSHIFT_FUNCTION.to_vec(), "the key keeps its function in both layers");
        {
            let mut session = Session::without_pauses(&mut mouse);
            save(&p, &mut session, "1532:00b4:usb-a", &path).unwrap();
        }
        assert_eq!(mouse.layers[1][&0x40], vec![2, 2, 0, 0x05, 0, 0, 0]);
        assert_eq!(mouse.layers[1][&0x41], mouse.layers[0][&0x41], "other controls mirror the standard layer");
        assert!(load(&path).unwrap().original_hypershift.is_some());
        let (shifted, issues) = hypershift_mapping_from(&mouse.layers[0].clone(), &mouse.layers[1].clone(), &LayoutMap::us());
        assert!(issues.is_empty());
        assert_eq!(shifted.keys().copied().collect::<Vec<_>>(), vec![1]);
        {
            let mut session = Session::without_pauses(&mut mouse);
            restore(&mut session, "1532:00b4:usb-a", &path).unwrap();
        }
        let factory = FakeMouse::factory();
        assert_eq!(mouse.layers, factory.layers);
    }

    #[test]
    fn save_refuses_driver_mode_and_foreign_receivers() {
        let dir = crate::model::store::tests::temp_dir("onboard-guard");
        let path = dir.join("onboard-profile.json");
        let mut mouse = FakeMouse::factory();
        mouse.mode = 3;
        let p = plan(vec![(1, key("escape", mac::ESCAPE, &[]))], vec![]);
        let mut session = Session::without_pauses(&mut mouse);
        assert!(save(&p, &mut session, "1532:00b4:a", &path).unwrap_err().to_string().contains("driver mode"));
        assert!(same_receiver("1532:00b4:x", "1532:00B4:y"));
        assert!(!same_receiver("1532:00b4:x", "1532:00b5:x"));
    }

    #[test]
    fn journal_validation() {
        let dir = crate::model::store::tests::temp_dir("onboard-journal");
        let path = dir.join("j.json");
        std::fs::write(&path, r#"{"identity":"1532:00b4","original":[[1,1,0,1,1,1,0,0,0,0]],"name":"x","pending":false}"#).unwrap();
        assert!(load(&path).is_err());
        let layer: Vec<Vec<u8>> = IDS.iter().map(|id| vec![1, *id, 1, 2, 2, 0, 4, 0, 0, 0]).collect();
        let state = State { identity: "1532:00b4:1".into(), original: layer, name: "x".into(), pending: false, original_hypershift: None };
        std::fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();
        let loaded = load(&path).unwrap();
        assert!(loaded.original.iter().all(|b| b[2] == 0), "reply status bytes are normalized");
        assert!(decode_button_ids(&[2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]).is_err());
    }

    #[test]
    fn evdev_usage_reverse() {
        assert_eq!(evdev_to_usage(99), Some(0x46));
        assert_eq!(evdev_to_usage(30), Some(0x04));
        assert_eq!(evdev_to_usage(43), Some(0x31));
        assert_eq!(evdev_to_usage(125), Some(0xe3));
    }
}
