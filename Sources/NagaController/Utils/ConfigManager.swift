import Foundation

// Keys for persistence
private let kRemappingEnabledKey = "remappingEnabled"
private let kCurrentProfileKey = "currentProfile"

struct ProfilesFile: Codable, Equatable {
    var profiles: [String: Profile]
    var settings: Settings?
    var formatVersion: Int? = nil
}

struct Settings: Codable, Equatable {
    var currentProfile: String?
    var autoSwitchProfiles: Bool?
    var showNotifications: Bool?
    // Bundle identifier -> profile name, used while that app is frontmost.
    // Absent in files written before per-app profiles existed.
    var appProfiles: [String: String]? = nil
}

struct Profile: Codable, Equatable {
    var buttons: [String: ButtonAction]
    // Imported firmware assignments cannot reliably identify physical controls
    // through the factory keyboard/mouse event decoder.
    var onboardOnly: Bool? = nil
    // Assignments used while a Hypershift control is held. Absent in files
    // written before the layer existed.
    var hypershift: [String: ButtonAction]? = nil

    func buttons(in layer: MappingLayer) -> [String: ButtonAction] {
        layer == .standard ? buttons : hypershift ?? [:]
    }
}

struct ButtonAction: Codable, Equatable {
    let type: String
    let keys: [KeyStroke]? // for keySequence
    let description: String?
    let path: String? // for application
    let command: String? // for systemCommand
    let text: String? // for textSnippet
    let steps: [MacroStep]? // for macro
    var profile: String? // for profileSwitch
    var mouseAction: MouseAction? = nil
    var audioAction: AudioAction? = nil
    var systemAction: SystemAction? = nil
}

final class ConfigManager {
    static let shared = ConfigManager()
    static let didChangeNotification = Notification.Name("NagaConfigDidChange")
    private(set) var lastError: String?
    private var storageURL: URL?
    private var defaults = UserDefaults.standard
    private lazy var fileStore = ProfileFileStore(url: storageURL ?? DataFolder.file("profiles.json"))
    private struct State: Equatable {
        let file: ProfilesFile
        let runtime: String
    }
    private var undoHistory: [State] = []
    private var redoHistory: [State] = []
    private(set) var profilesReadOnly = false
    var canUndo: Bool { !undoHistory.isEmpty && !profilesReadOnly }
    var canRedo: Bool { !redoHistory.isEmpty && !profilesReadOnly }
    var profilesFolder: URL { fileStore.url.deletingLastPathComponent() }
    var hasProfileBackup: Bool { FileManager.default.fileExists(atPath: fileStore.backupURL.path) }

    private(set) var profiles: [String: Profile] = [:]
    /// The profile the user selected. It is what the editor shows, what is
    /// persisted and what Save to Mouse writes.
    private(set) var currentProfileName: String = "Default"
    /// The profile whose mapping is live. It differs from the selected one
    /// only while an app binding applies; it is never persisted.
    private(set) var runtimeProfileName: String = "Default"
    private(set) var autoSwitchEnabled = false
    private(set) var appProfiles: [String: String] = [:]
    private var showNotifications: Bool?
    private(set) var scrollSensitivity: ScrollSensitivity

    init(storageURL: URL? = nil, defaults: UserDefaults = .standard) {
        self.storageURL = storageURL
        self.defaults = defaults
        self.scrollSensitivity = ScrollSensitivity(
            vertical: (defaults.object(forKey: ScrollSensitivity.verticalKey) as? NSNumber)?.doubleValue ?? 1,
            horizontal: (defaults.object(forKey: ScrollSensitivity.horizontalKey) as? NSNumber)?.doubleValue ?? 1
        ).clamped()
    }

    func load() {
        lastError = nil
        // Load bundled defaults first
        var mergedProfiles: [String: Profile] = [:]
        var mergedSettings: Settings? = nil
        var fileSelection: String?
        if let url = Bundle.main.url(forResource: "default-profiles", withExtension: "json") {
            do {
                let data = try Data(contentsOf: url)
                let pf = try JSONDecoder().decode(ProfilesFile.self, from: data)
                mergedProfiles = pf.profiles
                mergedSettings = pf.settings
            } catch {
                NSLog("[Config] Failed to load bundled defaults: \(error.localizedDescription)")
            }
        } else {
            NSLog("[Config] default-profiles.json not found in bundle")
        }

        // Overlay with user profiles if present
        do {
            if let user = try fileStore.load() {
                mergedProfiles = user.profiles
                if let settings = user.settings { mergedSettings = settings }
                fileSelection = user.settings?.currentProfile
            }
            profilesReadOnly = false
        } catch {
            profilesReadOnly = true
            lastError = "Unable to load profiles. The file is protected: \(error.localizedDescription)"
            // A failed reload must retain the last usable in-memory profiles.
            if !profiles.isEmpty { updateRuntimeMapping(); notify(); return }
        }

        // Adopt merged
        self.profiles = mergedProfiles

        // The transactional document is authoritative. Defaults are only a
        // fallback for old files without a saved selection.
        let ud = defaults
        if let saved = fileSelection, profiles[saved] != nil {
            currentProfileName = saved
        } else if let saved = ud.string(forKey: kCurrentProfileKey), profiles[saved] != nil {
            currentProfileName = saved
        } else if let bundled = mergedSettings?.currentProfile {
            currentProfileName = bundled
        } else {
            currentProfileName = "Default"
        }

        if profiles[currentProfileName] == nil {
            currentProfileName = profiles.keys.sorted().first ?? "Default"
        }
        autoSwitchEnabled = mergedSettings?.autoSwitchProfiles ?? false
        appProfiles = (mergedSettings?.appProfiles ?? [:]).filter { profiles[$0.value] != nil }
        showNotifications = mergedSettings?.showNotifications
        runtimeProfileName = currentProfileName
        undoHistory.removeAll(); redoHistory.removeAll()
        updateRuntimeMapping()
        notify()
    }

    /// An explicit choice. It also takes over the live mapping until the next
    /// app change, so a manual switch is never undone by an app binding.
    func setCurrentProfile(_ name: String) {
        guard profiles[name] != nil else { return }
        change(recordUndo: false) {
            self.currentProfileName = name
            self.runtimeProfileName = name
        }
    }

    /// Makes `name` the live profile without selecting it. Held buttons keep
    /// the output they started. Nothing is saved.
    func applyAutomaticProfile(_ name: String) {
        guard !profilesReadOnly, profiles[name] != nil, profiles[name]?.onboardOnly != true,
              !currentProfileIsOnboardOnly, name != runtimeProfileName else { return }
        runtimeProfileName = name
        updateRuntimeMapping(preservingHolds: true)
        notify()
    }

    func setAutoSwitchEnabled(_ enabled: Bool) {
        guard enabled != autoSwitchEnabled else { return }
        change { self.autoSwitchEnabled = enabled }
    }

    func setAppProfile(_ profile: String?, forBundleIdentifier bundleID: String) {
        guard !bundleID.isEmpty else { return }
        if let profile, profiles[profile] == nil { return }
        change {
            if let profile { self.appProfiles[bundleID] = profile }
            else { self.appProfiles.removeValue(forKey: bundleID) }
        }
    }

    func getRemappingEnabled() -> Bool {
        return !profilesReadOnly && !currentProfileIsOnboardOnly && defaults.bool(forKey: kRemappingEnabledKey)
    }

    var currentProfileIsOnboardOnly: Bool { profiles[currentProfileName]?.onboardOnly == true }

    func setRemappingEnabled(_ enabled: Bool) {
        defaults.set(enabled, forKey: kRemappingEnabledKey)
        if !enabled { EventTapManager.shared.resetInputState() }
        notify()
    }

    func setScrollSensitivity(_ sensitivity: ScrollSensitivity) {
        scrollSensitivity = sensitivity.clamped()
        defaults.set(scrollSensitivity.vertical, forKey: ScrollSensitivity.verticalKey)
        defaults.set(scrollSensitivity.horizontal, forKey: ScrollSensitivity.horizontalKey)
        notify()
    }

    func availableProfiles() -> [String] {
        return Array(profiles.keys).sorted()
    }

    func mappingForCurrentProfile(layer: MappingLayer = .standard) -> [Int: ActionType] {
        mapping(ofProfile: currentProfileName, layer: layer)
    }

    func mappingForRuntimeProfile(layer: MappingLayer = .standard) -> [Int: ActionType] {
        mapping(ofProfile: runtimeProfileName, layer: layer)
    }

    private func mapping(ofProfile name: String, layer: MappingLayer) -> [Int: ActionType] {
        guard let profile = profiles[name] else { return [:] }
        var result: [Int: ActionType] = [:]
        for (key, action) in profile.buttons(in: layer) {
            if let idx = Int(key), (1...19).contains(idx), let mapped = convert(action: action) {
                if layer == .hypershift, case .hypershift = mapped { continue }
                result[idx] = mapped
            }
        }
        return result
    }

    // MARK: - Profile Management

    @discardableResult
    func createProfile(name: String, basedOn base: String? = nil) -> Bool {
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, profiles[trimmed] == nil else { return false }
        return change {
            self.profiles[trimmed] = base.flatMap { self.profiles[$0] } ?? Profile(buttons: [:])
            self.currentProfileName = trimmed
            self.runtimeProfileName = trimmed
        }
    }

    // Creates and selects a profile from a complete mapping, e.g. one read from the mouse.
    // Returns the name used, adding a number when the name is taken.
    @discardableResult
    func createProfile(named base: String, mapping: [Int: ActionType], hypershift: [Int: ActionType] = [:], onboardOnly: Bool = false) -> String? {
        var name = base
        if profiles[name] != nil { name = ProfileImportDocument.uniqueName(base, reserved: Set(profiles.keys)) }
        func stored(_ layer: [Int: ActionType]) -> [String: ButtonAction] {
            Dictionary(uniqueKeysWithValues: layer.map { (String($0.key), toButtonAction($0.value)) })
        }
        guard change({
            self.profiles[name] = Profile(buttons: stored(mapping), onboardOnly: onboardOnly ? true : nil,
                                         hypershift: hypershift.isEmpty ? nil : stored(hypershift))
            self.currentProfileName = name
            self.runtimeProfileName = name
        }) else { return nil }
        return name
    }

    @discardableResult
    func duplicateProfile(source: String, as newName: String) -> Bool {
        guard profiles[source] != nil else { return false }
        return createProfile(name: newName, basedOn: source)
    }

    @discardableResult
    func renameProfile(from oldName: String, to newName: String) -> Bool {
        let newTrim = newName.trimmingCharacters(in: .whitespacesAndNewlines)
        guard oldName != newTrim, !newTrim.isEmpty, let existing = profiles[oldName], profiles[newTrim] == nil else { return false }
        return change {
            profiles.removeValue(forKey: oldName)
            profiles[newTrim] = existing
            for name in Array(profiles.keys) {
                guard var profile = profiles[name] else { continue }
                for (button, action) in profile.buttons where action.type == "profileSwitch" && action.profile == oldName {
                    profile.buttons[button] = toButtonAction(.profileSwitch(profile: newTrim, description: action.description))
                }
                for (button, action) in profile.hypershift ?? [:] where action.type == "profileSwitch" && action.profile == oldName {
                    profile.hypershift?[button] = toButtonAction(.profileSwitch(profile: newTrim, description: action.description))
                }
                profiles[name] = profile
            }
            for (bundleID, bound) in appProfiles where bound == oldName { appProfiles[bundleID] = newTrim }
            if currentProfileName == oldName { currentProfileName = newTrim }
            if runtimeProfileName == oldName { runtimeProfileName = newTrim }
        }
    }

    @discardableResult
    func deleteProfile(named name: String) -> Bool {
        guard profiles[name] != nil else { return false }
        // Prevent deleting the last profile
        if profiles.count <= 1 { return false }
        return change {
            profiles.removeValue(forKey: name)
            appProfiles = appProfiles.filter { $0.value != name }
            // Never leave a switch pointing at a deleted profile. Disabled actions
            // avoid leaking the original button's key after a target is removed.
            for key in Array(profiles.keys) {
                guard var profile = profiles[key] else { continue }
                for button in profile.buttons.keys where profile.buttons[button]?.type == "profileSwitch" && profile.buttons[button]?.profile == name {
                    profile.buttons[button] = toButtonAction(.disabled)
                }
                for button in (profile.hypershift ?? [:]).keys where profile.hypershift?[button]?.type == "profileSwitch" && profile.hypershift?[button]?.profile == name {
                    profile.hypershift?[button] = toButtonAction(.disabled)
                }
                profiles[key] = profile
            }
            if currentProfileName == name {
                // Switch to an arbitrary remaining profile
                if let next = profiles.keys.sorted().first {
                    currentProfileName = next
                    runtimeProfileName = next
                }
            } else {
                if runtimeProfileName == name { runtimeProfileName = currentProfileName }
            }
        }
    }

    // MARK: - Import / Export

    func importProfiles(from url: URL, merge: Bool = true, policy: ProfileImportPolicy = .keepBoth) throws {
        try importProfiles(ProfileImportDocument(url: url), merge: merge, policy: policy)
    }

    func importProfiles(_ document: ProfileImportDocument, merge: Bool = true, policy: ProfileImportPolicy = .keepBoth) throws {
        let previous = state()
        let candidate = merge ? document.merged(with: previous.file, policy: policy) : document.file
        guard change({
            self.adopt(candidate)
            // Imported mouse profiles cannot remain an automatic runtime target.
            if self.profiles[self.runtimeProfileName]?.onboardOnly == true { self.runtimeProfileName = self.currentProfileName }
        }) else { throw ProfileDataError(message: lastError ?? "Unable to import profiles.") }
    }

    func exportCurrentProfile(to url: URL) throws {
        guard let p = profiles[currentProfileName] else { throw ProfileDataError(message: "Select a valid profile before exporting.") }
        let pf = ProfilesFile(profiles: [currentProfileName: p], settings: Settings(currentProfile: currentProfileName, autoSwitchProfiles: nil, showNotifications: nil))
        let data = try ProfileFileStore.encode(pf)
        try data.write(to: url, options: .atomic)
    }

    func exportAllProfiles(to url: URL) throws {
        let pf = ProfilesFile(profiles: profiles, settings: persistedSettings())
        let data = try ProfileFileStore.encode(pf)
        try data.write(to: url, options: .atomic)
    }

    private func persistedSettings() -> Settings {
        Settings(currentProfile: currentProfileName, autoSwitchProfiles: autoSwitchEnabled, showNotifications: showNotifications,
                 appProfiles: appProfiles.isEmpty ? nil : appProfiles)
    }

    private func convert(action: ButtonAction) -> ActionType? {
        switch action.type {
        case "audio":
            return action.audioAction.map { .audio(action: $0, description: action.description) }
        case "system":
            return action.systemAction.map { .system(action: $0, description: action.description) }
        case "disabled": return .disabled
        case "mouse":
            return action.mouseAction.map { .mouse(action: $0, description: action.description) }
        case "keySequence":
            return .keySequence(keys: action.keys ?? [], description: action.description)
        case "application":
            if let path = action.path { return .application(path: path, description: action.description) }
            return nil
        case "systemCommand":
            if let cmd = action.command { return .systemCommand(command: cmd, description: action.description) }
            return nil
        case "textSnippet":
            if let text = action.text { return .textSnippet(text: text, description: action.description) }
            return nil
        case "macro":
            return .macro(steps: action.steps ?? [], description: action.description)
        case "profileSwitch":
            if let p = action.profile { return .profileSwitch(profile: p, description: action.description) }
            return nil
        case "hypershift":
            return .hypershift(description: action.description)
        default:
            return nil
        }
    }

    private func toButtonAction(_ action: ActionType) -> ButtonAction {
        switch action {
        case .audio(let audio, let description):
            return ButtonAction(type: "audio", keys: nil, description: description, path: nil, command: nil, text: nil, steps: nil, profile: nil, audioAction: audio)
        case .system(let system, let description):
            return ButtonAction(type: "system", keys: nil, description: description, path: nil, command: nil, text: nil, steps: nil, profile: nil, systemAction: system)
        case .disabled:
            return ButtonAction(type: "disabled", keys: nil, description: nil, path: nil, command: nil, text: nil, steps: nil, profile: nil)
        case .mouse(let mouse, let description):
            return ButtonAction(type: "mouse", keys: nil, description: description, path: nil, command: nil, text: nil, steps: nil, profile: nil, mouseAction: mouse)
        case .keySequence(let keys, let description):
            return ButtonAction(type: "keySequence", keys: keys, description: description, path: nil, command: nil, text: nil, steps: nil, profile: nil)
        case .application(let path, let description):
            return ButtonAction(type: "application", keys: nil, description: description, path: path, command: nil, text: nil, steps: nil, profile: nil)
        case .systemCommand(let command, let description):
            return ButtonAction(type: "systemCommand", keys: nil, description: description, path: nil, command: command, text: nil, steps: nil, profile: nil)
        case .textSnippet(let text, let description):
            return ButtonAction(type: "textSnippet", keys: nil, description: description, path: nil, command: nil, text: text, steps: nil, profile: nil)
        case .macro(let steps, let description):
            return ButtonAction(type: "macro", keys: nil, description: description, path: nil, command: nil, text: nil, steps: steps, profile: nil)
        case .profileSwitch(let profile, let description):
            return ButtonAction(type: "profileSwitch", keys: nil, description: description, path: nil, command: nil, text: nil, steps: nil, profile: profile)
        case .hypershift(let description):
            return ButtonAction(type: "hypershift", keys: nil, description: description, path: nil, command: nil, text: nil, steps: nil, profile: nil)
        }
    }

    // Update a single button's action in the current profile and refresh mapping.
    // The Hypershift layer cannot hold another Hypershift control.
    func setAction(forButton index: Int, action: ActionType?, layer: MappingLayer = .standard) {
        guard (1...19).contains(index) else { return }
        if layer == .hypershift, case .hypershift? = action { return }
        change {
            var profile = profiles[currentProfileName] ?? Profile(buttons: [:])
            let key = String(index)
            var buttons = profile.buttons(in: layer)
            if let action = action {
                buttons[key] = toButtonAction(action)
            } else {
                buttons.removeValue(forKey: key)
            }
            switch layer {
            case .standard: profile.buttons = buttons
            case .hypershift: profile.hypershift = buttons.isEmpty ? nil : buttons
            }
            profiles[currentProfileName] = profile
        }
    }

    /// Commit before publishing a mapping or updating defaults. Failed saves
    /// roll back memory, selection and history to the previous usable state.
    @discardableResult
    private func change(recordUndo: Bool = true, _ edit: () -> Void) -> Bool {
        guard !profilesReadOnly else { notify(); return false }
        let previous = state()
        edit()
        if state() == previous { lastError = nil; notify(); return true }
        do {
            try fileStore.save(state().file)
            defaults.set(currentProfileName, forKey: kCurrentProfileKey)
            if recordUndo && !previous.file.profiles.isEmpty {
                undoHistory.append(previous)
                if undoHistory.count > 12 { undoHistory.removeFirst() }
                redoHistory.removeAll()
            }
            updateRuntimeMapping()
            lastError = nil
            notify()
            return true
        } catch {
            restore(previous)
            lastError = "Unable to save profiles: \(error.localizedDescription)"
            notify()
            return false
        }
    }

    /// Explicit persistence for callers that only need to flush the current
    /// document. Normal editing uses change so memory also rolls back on error.
    func saveUserProfiles() {
        do {
            try fileStore.save(state().file)
            defaults.set(currentProfileName, forKey: kCurrentProfileKey)
            lastError = nil
        } catch { lastError = "Unable to save profiles: \(error.localizedDescription)" }
        notify()
    }

    @discardableResult
    func undo() -> Bool { moveHistory(undoing: true) }
    @discardableResult
    func redo() -> Bool { moveHistory(undoing: false) }

    private func moveHistory(undoing: Bool) -> Bool {
        guard let target = undoing ? undoHistory.last : redoHistory.last else { return false }
        let current = state()
        guard change(recordUndo: false, { self.restore(target) }) else { return false }
        if undoing { undoHistory.removeLast(); redoHistory.append(current) }
        else { redoHistory.removeLast(); undoHistory.append(current) }
        notify()
        return true
    }

    @discardableResult
    func restoreProfileBackup() -> Bool {
        do {
            let recovered = try fileStore.restoreBackup()
            adopt(recovered)
            runtimeProfileName = currentProfileName
            profilesReadOnly = false
            undoHistory.removeAll(); redoHistory.removeAll()
            defaults.set(currentProfileName, forKey: kCurrentProfileKey)
            updateRuntimeMapping()
            lastError = nil
            notify()
            return true
        } catch {
            lastError = "Unable to restore profiles: \(error.localizedDescription)"
            notify()
            return false
        }
    }

    private func state() -> State {
        State(file: ProfilesFile(profiles: profiles, settings: persistedSettings()), runtime: runtimeProfileName)
    }

    private func restore(_ state: State) {
        adopt(state.file)
        runtimeProfileName = state.runtime
    }

    private func adopt(_ file: ProfilesFile) {
        profiles = file.profiles
        let selected = file.settings?.currentProfile ?? currentProfileName
        currentProfileName = profiles[selected] != nil ? selected : profiles.keys.sorted().first ?? "Default"
        if profiles[runtimeProfileName] == nil { runtimeProfileName = currentProfileName }
        autoSwitchEnabled = file.settings?.autoSwitchProfiles ?? false
        appProfiles = (file.settings?.appProfiles ?? [:]).filter { profiles[$0.value] != nil }
        showNotifications = file.settings?.showNotifications
    }

    private func notify() {
        DispatchQueue.main.async { NotificationCenter.default.post(name: Self.didChangeNotification, object: self) }
    }

    private func updateRuntimeMapping(preservingHolds: Bool = false) {
        let onboard = profilesReadOnly || currentProfileIsOnboardOnly || profiles[runtimeProfileName]?.onboardOnly == true
        let standard = onboard ? [:] : mappingForRuntimeProfile()
        let shifted = onboard ? [:] : mappingForRuntimeProfile(layer: .hypershift)
        if preservingHolds {
            ButtonMapper.shared.swapMapping(standard, hypershift: shifted)
        } else {
            ButtonMapper.shared.updateMapping(standard, hypershift: shifted)
        }
    }

}
