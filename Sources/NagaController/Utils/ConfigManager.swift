import Foundation

// Keys for persistence
private let kRemappingEnabledKey = "remappingEnabled"
private let kCurrentProfileKey = "currentProfile"

struct ProfilesFile: Codable {
    var profiles: [String: Profile]
    var settings: Settings?
}

struct Settings: Codable {
    var currentProfile: String?
    var autoSwitchProfiles: Bool?
    var showNotifications: Bool?
    // Bundle identifier -> profile name, used while that app is frontmost.
    // Absent in files written before per-app profiles existed.
    var appProfiles: [String: String]? = nil
}

struct Profile: Codable {
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

struct ButtonAction: Codable {
    let type: String
    let keys: [KeyStroke]? // for keySequence
    let description: String?
    let path: String? // for application
    let command: String? // for systemCommand
    let text: String? // for textSnippet
    let steps: [MacroStep]? // for macro
    let profile: String? // for profileSwitch
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
        if let userURL = try? userProfilesURL(), FileManager.default.fileExists(atPath: userURL.path) {
            do {
                let userData = try Data(contentsOf: userURL)
                let upf = try JSONDecoder().decode(ProfilesFile.self, from: userData)
                // Overlay: replace/merge profiles
                mergedProfiles = upf.profiles
                // Overlay settings
                if let s = upf.settings { mergedSettings = s }
            } catch {
                lastError = "Unable to load profiles: \(error.localizedDescription)"
            }
        }

        // Adopt merged
        self.profiles = mergedProfiles

        // Preferred profile: UserDefaults > settings.currentProfile > "Default"
        let ud = defaults
        if let saved = ud.string(forKey: kCurrentProfileKey) {
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
        appProfiles = mergedSettings?.appProfiles ?? [:]
        showNotifications = mergedSettings?.showNotifications
        runtimeProfileName = currentProfileName
        updateRuntimeMapping()
        notify()
    }

    /// An explicit choice. It also takes over the live mapping until the next
    /// app change, so a manual switch is never undone by an app binding.
    func setCurrentProfile(_ name: String) {
        guard profiles[name] != nil else { return }
        currentProfileName = name
        runtimeProfileName = name
        defaults.set(name, forKey: kCurrentProfileKey)
        updateRuntimeMapping()
        saveUserProfiles()
    }

    /// Makes `name` the live profile without selecting it. Held buttons keep
    /// the output they started. Nothing is saved.
    func applyAutomaticProfile(_ name: String) {
        guard profiles[name] != nil, name != runtimeProfileName else { return }
        runtimeProfileName = name
        updateRuntimeMapping(preservingHolds: true)
        notify()
    }

    func setAutoSwitchEnabled(_ enabled: Bool) {
        guard enabled != autoSwitchEnabled else { return }
        autoSwitchEnabled = enabled
        saveUserProfiles()
    }

    func setAppProfile(_ profile: String?, forBundleIdentifier bundleID: String) {
        guard !bundleID.isEmpty else { return }
        if let profile {
            guard profiles[profile] != nil else { return }
            appProfiles[bundleID] = profile
        } else {
            appProfiles.removeValue(forKey: bundleID)
        }
        saveUserProfiles()
    }

    func getRemappingEnabled() -> Bool {
        return !currentProfileIsOnboardOnly && defaults.bool(forKey: kRemappingEnabledKey)
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
        if let base = base, let p = profiles[base] {
            profiles[trimmed] = p
        } else {
            profiles[trimmed] = Profile(buttons: [:])
        }
        setCurrentProfile(trimmed)
        return true
    }

    // Creates and selects a profile from a complete mapping, e.g. one read from the mouse.
    // Returns the name used, adding a number when the name is taken.
    @discardableResult
    func createProfile(named base: String, mapping: [Int: ActionType], hypershift: [Int: ActionType] = [:], onboardOnly: Bool = false) -> String {
        var name = base
        var suffix = 2
        while profiles[name] != nil { name = "\(base) \(suffix)"; suffix += 1 }
        func stored(_ layer: [Int: ActionType]) -> [String: ButtonAction] {
            Dictionary(uniqueKeysWithValues: layer.map { (String($0.key), toButtonAction($0.value)) })
        }
        profiles[name] = Profile(buttons: stored(mapping), onboardOnly: onboardOnly ? true : nil,
                                 hypershift: hypershift.isEmpty ? nil : stored(hypershift))
        setCurrentProfile(name)
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
        defaults.set(currentProfileName, forKey: kCurrentProfileKey)
        updateRuntimeMapping()
        saveUserProfiles()
        return true
    }

    @discardableResult
    func deleteProfile(named name: String) -> Bool {
        guard profiles[name] != nil else { return false }
        // Prevent deleting the last profile
        if profiles.count <= 1 { return false }
        profiles.removeValue(forKey: name)
        appProfiles = appProfiles.filter { $0.value != name }
        if currentProfileName == name {
            // Switch to an arbitrary remaining profile
            if let next = profiles.keys.sorted().first {
                setCurrentProfile(next)
            }
        } else {
            if runtimeProfileName == name { runtimeProfileName = currentProfileName }
            // refresh mapping for the live profile
            updateRuntimeMapping()
        }
        saveUserProfiles()
        return true
    }

    // MARK: - Import / Export

    func importProfiles(from url: URL, merge: Bool = true) throws {
        let data = try Data(contentsOf: url)
        let pf = try JSONDecoder().decode(ProfilesFile.self, from: data)
        if merge {
            for (k, v) in pf.profiles { profiles[k] = v }
        } else {
            profiles = pf.profiles
        }
        let importedBindings = (pf.settings?.appProfiles ?? [:]).filter { profiles[$0.value] != nil }
        appProfiles = merge ? appProfiles.merging(importedBindings) { _, imported in imported } : importedBindings
        appProfiles = appProfiles.filter { profiles[$0.value] != nil }
        if let cp = pf.settings?.currentProfile, profiles[cp] != nil {
            setCurrentProfile(cp)
        } else {
            if profiles[currentProfileName] == nil { currentProfileName = profiles.keys.sorted().first ?? "Default" }
            if profiles[runtimeProfileName] == nil { runtimeProfileName = currentProfileName }
            updateRuntimeMapping()
        }
        saveUserProfiles()
    }

    func exportCurrentProfile(to url: URL) throws {
        guard let p = profiles[currentProfileName] else { return }
        let pf = ProfilesFile(profiles: [currentProfileName: p], settings: Settings(currentProfile: currentProfileName, autoSwitchProfiles: nil, showNotifications: nil))
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let data = try encoder.encode(pf)
        try data.write(to: url, options: .atomic)
    }

    func exportAllProfiles(to url: URL) throws {
        let pf = ProfilesFile(profiles: profiles, settings: persistedSettings())
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let data = try encoder.encode(pf)
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
        updateRuntimeMapping()
        saveUserProfiles()
    }

    // Persist current profiles to Application Support
    func saveUserProfiles() {
        defaults.set(currentProfileName, forKey: kCurrentProfileKey)
        do {
            let url = try userProfilesURL()
            try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            let pf = ProfilesFile(profiles: profiles, settings: persistedSettings())
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
            let data = try encoder.encode(pf)
            try data.write(to: url, options: .atomic)
            lastError = nil
        } catch {
            lastError = "Unable to save profiles: \(error.localizedDescription)"
        }
        notify()
    }

    private func notify() {
        DispatchQueue.main.async { NotificationCenter.default.post(name: Self.didChangeNotification, object: self) }
    }

    private func updateRuntimeMapping(preservingHolds: Bool = false) {
        let onboard = currentProfileIsOnboardOnly
        let standard = onboard ? [:] : mappingForRuntimeProfile()
        let shifted = onboard ? [:] : mappingForRuntimeProfile(layer: .hypershift)
        if preservingHolds {
            ButtonMapper.shared.swapMapping(standard, hypershift: shifted)
        } else {
            ButtonMapper.shared.updateMapping(standard, hypershift: shifted)
        }
    }

    private func userProfilesURL() throws -> URL {
        if let storageURL { return storageURL }
        return DataFolder.file("profiles.json")
    }

}
