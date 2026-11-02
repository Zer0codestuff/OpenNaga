import Foundation

enum ImportedProfileTests {
    struct Failure: Error, CustomStringConvertible { let description: String }

    static func run() throws -> Int {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let domain = "OpenNaga.ImportTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: domain)!
        defer {
            try? FileManager.default.removeItem(at: root)
            defaults.removePersistentDomain(forName: domain)
        }
        let url = root.appendingPathComponent("profiles.json")
        let config = ConfigManager(storageURL: url, defaults: defaults)
        var count = 0
        func check(_ condition: Bool, _ label: String) throws {
            guard condition else { throw Failure(description: label) }
            count += 1
        }
        // Duplicate native outputs have no physical source identity in CGEvent.
        let mapping: [Int: ActionType] = [1: .mouse(action: .leftClick, description: nil),
                                          18: .mouse(action: .rightClick, description: nil)]
        guard let imported = config.createProfile(named: "From Mouse", mapping: mapping, onboardOnly: true) else {
            throw Failure(description: "Imported mouse profile can be saved")
        }
        config.setRemappingEnabled(true)
        try check(config.currentProfileIsOnboardOnly && !config.getRemappingEnabled(), "Imported assignments cannot activate software remapping")
        try check(!ButtonMapper.shared.hasMapping(buttonIndex: 1) && !ButtonMapper.shared.hasMapping(buttonIndex: 18), "Imported duplicate clicks never reach the runtime mapper")
        try check(config.mappingForCurrentProfile() == mapping, "All imported assignments remain available to the editor and mouse save")
        let second = config.createProfile(named: "From Mouse", mapping: mapping, onboardOnly: true)
        try check(second == "From Mouse 2", "Repeated mouse imports keep the first profile")
        let loaded = ConfigManager(storageURL: url, defaults: defaults)
        loaded.load()
        try check(loaded.currentProfileIsOnboardOnly && !loaded.getRemappingEnabled(), "The import gate survives a restart")
        try check(loaded.duplicateProfile(source: imported, as: "Copy") && loaded.currentProfileIsOnboardOnly, "Duplicating an imported profile preserves its gate")
        let exported = root.appendingPathComponent("export.json")
        try loaded.exportCurrentProfile(to: exported)
        try check(JSONDecoder().decode(ProfilesFile.self, from: Data(contentsOf: exported)).profiles["Copy"]?.onboardOnly == true, "Export retains the import gate")
        try check(loaded.createProfile(name: "Software") && !loaded.currentProfileIsOnboardOnly && loaded.getRemappingEnabled(), "Ordinary profiles keep the user's software remapping preference")
        loaded.setAction(forButton: 1, action: .mouse(action: .middleClick, description: nil))
        try check(ButtonMapper.shared.hasMapping(buttonIndex: 1), "Ordinary profiles reach the runtime mapper")
        loaded.setCurrentProfile(imported)
        try check(!ButtonMapper.shared.hasMapping(buttonIndex: 1), "Selecting an imported profile clears a previous runtime mapping")
        let legacy = Data("{\"buttons\":{}}".utf8)
        try check(JSONDecoder().decode(Profile.self, from: legacy).onboardOnly == nil, "Existing profile files need no migration")
        return count
    }
}
