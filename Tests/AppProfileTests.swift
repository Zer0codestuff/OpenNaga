import Cocoa

enum AppProfileTests {
    struct Failure: Error, CustomStringConvertible { let description: String }

    static func run() throws -> Int {
        var count = 0
        func check(_ condition: Bool, _ label: String) throws {
            guard condition else { throw Failure(description: label) }
            count += 1
        }
        try resolver(check)
        try switcher(check)
        try heldOutput(check)
        try persistence(check)
        try catalog(check)
        return count
    }

    private static func key(_ name: String, _ code: UInt16) -> ActionType {
        .keySequence(keys: [KeyStroke(key: name, modifiers: [], keyCode: code)], description: nil)
    }

    private static func resolver(_ check: (Bool, String) throws -> Void) throws {
        let profiles = ["A": Profile(buttons: [:]), "B": Profile(buttons: [:]), "M": Profile(buttons: [:], onboardOnly: true)]
        func resolve(bundle: String? = "app.safari", own: String? = "app.me", selected: String = "A", auto: Bool = true,
                     memory: Bool = false, bindings: [String: String] = ["app.safari": "B"]) -> String {
            AppProfileResolver.resolve(bundleID: bundle, ownBundleID: own, selected: selected, autoSwitch: auto,
                                       mouseMemoryActive: memory, bindings: bindings, profiles: profiles)
        }
        try check(resolve() == "B", "A bound app uses its profile")
        try check(resolve(auto: false) == "A", "Nothing switches while automatic switching is off")
        try check(resolve(bundle: "app.me", bindings: ["app.me": "B"]) == "A", "OpenNaga itself keeps the selected profile")
        try check(resolve(memory: true) == "A", "Nothing switches while the mouse memory profile is active")
        try check(resolve(bundle: "app.notes") == "A", "An unbound app uses the selected profile")
        try check(resolve(bundle: nil) == "A", "No frontmost app uses the selected profile")
        try check(resolve(bindings: ["app.safari": "Gone"]) == "A", "A binding to a missing profile is ignored")
        try check(resolve(bindings: ["app.safari": "M"]) == "A", "A binding to an imported mouse profile is ignored")
        try check(resolve(selected: "M") == "M", "An imported mouse profile selected by hand is never replaced")
    }

    private static func makeConfig(_ check: (Bool, String) throws -> Void) throws -> (ConfigManager, URL, () -> Void) {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let domain = "OpenNaga.AppProfileTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: domain)!
        let url = root.appendingPathComponent("profiles.json")
        let config = ConfigManager(storageURL: url, defaults: defaults)
        let cleanup = {
            try? FileManager.default.removeItem(at: root)
            defaults.removePersistentDomain(forName: domain)
        }
        guard config.createProfile(name: "A"), config.createProfile(name: "B") else { throw Failure(description: "Profiles can be created") }
        config.setAction(forButton: 2, action: key("b", 11))
        config.setCurrentProfile("A")
        config.setAction(forButton: 1, action: key("a", 0))
        return (config, url, cleanup)
    }

    private static func switcher(_ check: (Bool, String) throws -> Void) throws {
        let (config, _, cleanup) = try makeConfig(check)
        defer { cleanup() }
        config.setAppProfile("app.safari", forBundleIdentifier: "app.safari")
        try check(config.appProfiles.isEmpty, "A binding to the same name as a bundle id is not a profile")
        config.setAppProfile("B", forBundleIdentifier: "app.safari")
        config.setAutoSwitchEnabled(true)
        let switcher = AppProfileSwitcher(config: config, ownBundleID: "app.me", mouseMemoryActive: { false })

        switcher.handleActivation(bundleID: "app.safari", name: "Safari", regular: true)
        try check(config.runtimeProfileName == "B" && config.currentProfileName == "A", "A bound app loads its profile without selecting it")
        try check(ButtonMapper.shared.hasMapping(buttonIndex: 2) && !ButtonMapper.shared.hasMapping(buttonIndex: 1), "The live mapping is the bound profile's")
        try check(config.mappingForCurrentProfile()[1] == key("a", 0), "The editor keeps showing the selected profile")
        try check(switcher.frontmostName == "Safari", "The frontmost app is recorded")

        switcher.handleActivation(bundleID: "app.spotlight", name: "Spotlight", regular: false)
        try check(config.runtimeProfileName == "B" && switcher.frontmostBundleID == "app.safari", "A non-regular app does not change the profile")

        switcher.handleActivation(bundleID: "app.notes", name: "Notes", regular: true)
        try check(config.runtimeProfileName == "A" && ButtonMapper.shared.hasMapping(buttonIndex: 1), "An unbound app returns to the selected profile")

        switcher.handleActivation(bundleID: "app.safari", name: "Safari", regular: true)
        switcher.handleActivation(bundleID: "app.me", name: "OpenNaga", regular: true)
        try check(config.runtimeProfileName == "A", "OpenNaga's own window shows the selected profile live")

        // A manual choice wins until the next app change.
        switcher.handleActivation(bundleID: "app.safari", name: "Safari", regular: true)
        config.setCurrentProfile("A")
        try check(config.runtimeProfileName == "A", "A manual choice replaces the bound profile at once")
        switcher.configDidChange()
        try check(config.runtimeProfileName == "A", "A manual choice is not undone by the configuration change it causes")
        switcher.handleActivation(bundleID: "app.safari", name: "Safari", regular: true)
        try check(config.runtimeProfileName == "B", "The binding applies again at the next app change")

        config.setAutoSwitchEnabled(false)
        switcher.configDidChange()
        try check(config.runtimeProfileName == "A", "Turning switching off returns to the selected profile at once")
        config.setAutoSwitchEnabled(true)
        switcher.configDidChange()
        try check(config.runtimeProfileName == "B", "Turning switching on applies the frontmost app's profile at once")

        let mouseMemory = AppProfileSwitcher(config: config, ownBundleID: "app.me", mouseMemoryActive: { true })
        mouseMemory.handleActivation(bundleID: "app.safari", name: "Safari", regular: true)
        try check(config.runtimeProfileName == "A", "Mouse memory pauses switching")
        config.applyAutomaticProfile("Gone")
        try check(config.runtimeProfileName == "A", "An unknown profile is never applied")
    }

    private static func heldOutput(_ check: (Bool, String) throws -> Void) throws {
        var events: [CGEvent] = []
        let mapper = ButtonMapper(eventSink: { events.append($0) })
        mapper.modifierKeyState = { _ in false }
        func codes(_ type: CGEventType) -> [Int64] {
            defer { events.removeAll(where: { $0.type == type }) }
            return events.filter { $0.type == type }.map { $0.getIntegerValueField(.keyboardEventKeycode) }
        }
        mapper.updateMapping([1: key("a", 0)])
        mapper.handlePress(buttonIndex: 1)
        mapper.swapMapping([1: key("b", 11)])
        try check(codes(.keyUp).isEmpty, "Swapping the mapping does not release a held button")
        mapper.handleRelease(buttonIndex: 1)
        try check(codes(.keyUp) == [0], "A held key ends with its own key-up after a swap")
        mapper.handlePress(buttonIndex: 1)
        mapper.handleRelease(buttonIndex: 1)
        try check(codes(.keyDown) == [0, 11], "The next press uses the new mapping")

        mapper.updateMapping([1: key("a", 0)])
        mapper.handlePress(buttonIndex: 1)
        events.removeAll()
        mapper.updateMapping([1: key("b", 11)])
        try check(codes(.keyUp) == [0], "An edit still releases held buttons")
    }

    private static func persistence(_ check: (Bool, String) throws -> Void) throws {
        let (config, url, cleanup) = try makeConfig(check)
        defer { cleanup() }
        config.setAppProfile("B", forBundleIdentifier: "")
        try check(config.appProfiles.isEmpty, "An empty bundle identifier is ignored")
        config.setAppProfile("Gone", forBundleIdentifier: "app.safari")
        try check(config.appProfiles.isEmpty, "A binding to a missing profile is refused")
        config.setAppProfile("B", forBundleIdentifier: "app.safari")
        config.setAutoSwitchEnabled(true)
        config.setAction(forButton: 3, action: key("c", 8)) // An unrelated save must keep the settings.

        func reload() -> ConfigManager {
            let loaded = ConfigManager(storageURL: url, defaults: UserDefaults(suiteName: "OpenNaga.AppProfileTests.\(UUID().uuidString)")!)
            loaded.load()
            return loaded
        }
        let loaded = reload()
        try check(loaded.autoSwitchEnabled && loaded.appProfiles == ["app.safari": "B"], "Bindings and the switch survive a restart")

        try check(config.renameProfile(from: "B", to: "Browsing") && config.appProfiles == ["app.safari": "Browsing"], "Renaming a profile updates its bindings")
        try check(reload().appProfiles == ["app.safari": "Browsing"], "The renamed binding is saved")

        config.applyAutomaticProfile("Browsing")
        try check(config.runtimeProfileName == "Browsing" && config.currentProfileName == "A", "The live profile can differ from the selected one")
        try check(config.deleteProfile(named: "Browsing") && config.appProfiles.isEmpty, "Deleting a profile removes its bindings")
        try check(config.runtimeProfileName == "A", "Deleting the live profile falls back to the selected one")
        try check(reload().appProfiles.isEmpty && reload().autoSwitchEnabled, "The removal is saved and the switch stays on")

        let old = FileManager.default.temporaryDirectory.appendingPathComponent("\(UUID().uuidString).json")
        defer { try? FileManager.default.removeItem(at: old) }
        try Data(#"{"profiles":{"A":{"buttons":{}}},"settings":{"currentProfile":"A"}}"#.utf8).write(to: old)
        let legacy = ConfigManager(storageURL: old, defaults: UserDefaults(suiteName: "OpenNaga.AppProfileTests.\(UUID().uuidString)")!)
        legacy.load()
        try check(!legacy.autoSwitchEnabled && legacy.appProfiles.isEmpty, "Files written before app profiles still load, with switching off")

        config.setAppProfile("A", forBundleIdentifier: "app.notes")
        let export = FileManager.default.temporaryDirectory.appendingPathComponent("\(UUID().uuidString).json")
        defer { try? FileManager.default.removeItem(at: export) }
        try config.exportAllProfiles(to: export)
        let target = reload()
        target.setAppProfile(nil, forBundleIdentifier: "app.notes")
        try target.importProfiles(from: export)
        try check(target.appProfiles["app.notes"] == "A", "Importing all profiles brings their app bindings")
    }

    private static func catalog(_ check: (Bool, String) throws -> Void) throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        func makeApp(_ path: String, id: String?, name: String? = nil, agent: Bool = false) throws {
            let contents = root.appendingPathComponent(path).appendingPathComponent("Contents")
            try FileManager.default.createDirectory(at: contents, withIntermediateDirectories: true)
            var info: [String: Any] = [:]
            if let id { info["CFBundleIdentifier"] = id }
            if let name { info["CFBundleName"] = name }
            if agent { info["LSUIElement"] = true }
            try PropertyListSerialization.data(fromPropertyList: info, format: .xml, options: 0).write(to: contents.appendingPathComponent("Info.plist"))
        }
        try makeApp("Beta.app", id: "test.beta", name: "Beta")
        try makeApp("Suite/Alpha.app", id: "test.alpha")
        try makeApp("Suite/Duplicate.app", id: "test.beta", name: "Beta Copy")
        try makeApp("Agent.app", id: "test.agent", agent: true)
        try makeApp("Anonymous.app", id: nil)
        try makeApp("Deep/Deeper/Gamma.app", id: "test.gamma")

        let apps = InstalledAppCatalog.scan(roots: [root])
        try check(apps.map(\.bundleID) == ["test.alpha", "test.beta"], "The scan finds apps one folder down, sorted by name, once per bundle identifier")
        try check(apps.first?.name == "Alpha", "An app without a display name uses its file name")
        try check(!apps.contains { $0.bundleID == "test.agent" || $0.bundleID == "test.gamma" }, "Menu bar agents and deeply nested apps are left out")
        try check(InstalledAppCatalog.app(at: root.appendingPathComponent("Anonymous.app")) == nil, "A bundle without an identifier cannot be bound")
        try check(InstalledAppCatalog.app(at: root.appendingPathComponent("Beta.app"))?.path == root.appendingPathComponent("Beta.app").path, "A chosen app keeps its path")
        try check(InstalledAppCatalog.defaultRoots.contains { $0.path.hasSuffix(InstalledAppCatalog.steamLibrary) }, "The scan includes Steam's game folder")
        let merged = InstalledAppCatalog.merged(apps + apps)
        try check(merged.count == 2, "Merging installed and running apps removes duplicates")
    }
}
