import Foundation
import Darwin

enum ProfileProtectionTests {
    struct Failure: Error, CustomStringConvertible { let description: String }

    static func run() throws -> Int {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let domain = "OpenNaga.ProtectionTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: domain)!
        defer {
            try? FileManager.default.removeItem(at: root)
            defaults.removePersistentDomain(forName: domain)
        }
        let url = root.appendingPathComponent("profiles.json")
        let config = ConfigManager(storageURL: url, defaults: defaults)
        var count = 0
        var failures: [String] = []
        func check(_ condition: Bool, _ message: String) {
            count += 1
            if !condition { failures.append(message) }
        }
        let copy = ActionType.keySequence(keys: [KeyStroke(key: "c", modifiers: ["cmd"])], description: nil)
        check(config.createProfile(name: "Keep"), "Create initial profile")
        config.setAction(forButton: 1, action: copy)
        let damaged = Data("{ damaged profile data".utf8)
        try damaged.write(to: url)
        let reloaded = ConfigManager(storageURL: url, defaults: defaults)
        reloaded.load()
        reloaded.setAction(forButton: 2, action: .disabled)
        check(try Data(contentsOf: url) == damaged, "An edit after a load failure must preserve the unreadable file")

        // Separate storage so the import case is independent of recovery.
        let imports = ConfigManager(storageURL: root.appendingPathComponent("import-target.json"), defaults: defaults)
        check(imports.createProfile(name: "Keep"), "Create import target")
        imports.setAction(forButton: 1, action: copy)
        let source = ConfigManager(storageURL: root.appendingPathComponent("source.json"), defaults: defaults)
        check(source.createProfile(name: "Keep"), "Create import source")
        source.setAction(forButton: 1, action: .disabled)
        let exported = root.appendingPathComponent("export.json")
        try source.exportAllProfiles(to: exported)
        try imports.importProfiles(from: exported)
        check(imports.profiles["Keep"]?.buttons["1"]?.type == "keySequence", "Default import must preserve an existing same-name profile")
        check(imports.profiles["Keep 2"]?.buttons["1"]?.type == "disabled", "Default import must keep the imported profile under a unique name")

        check(reloaded.profilesReadOnly && !reloaded.getRemappingEnabled(), "A load error blocks remapping and edits")
        check(reloaded.restoreProfileBackup(), "A validated backup can be restored explicitly")
        check(!reloaded.profilesReadOnly && reloaded.profiles["Keep"] != nil, "Restoring clears protection and recovers a profile")
        let preserved = try FileManager.default.contentsOfDirectory(at: root, includingPropertiesForKeys: nil)
            .filter { $0.lastPathComponent.hasPrefix("profiles.before-restore.") }
        let preservedBytes = try preserved.first.map { try Data(contentsOf: $0) }
        check(preserved.count == 1 && preservedBytes == damaged, "Restore retains the damaged bytes in a separate file")
        let mode = try FileManager.default.attributesOfItem(atPath: url.path)[.posixPermissions] as? NSNumber
        check(mode?.intValue == 0o600, "Profiles have owner-only permissions")

        // Persistence failure must leave memory, selection and defaults alone.
        let blocker = root.appendingPathComponent("not-a-directory")
        try Data([1]).write(to: blocker)
        let unavailable = ConfigManager(storageURL: blocker.appendingPathComponent("profiles.json"), defaults: defaults)
        check(!unavailable.createProfile(name: "Unsaved"), "A failed save reports failure")
        check(unavailable.profiles.isEmpty && unavailable.currentProfileName == "Default", "A failed save does not publish an unsaved profile")

        // Another instance is allowed to read, but cannot silently replace a
        // newer document after it has loaded an older revision.
        let second = ConfigManager(storageURL: url, defaults: defaults)
        second.load()
        reloaded.setAction(forButton: 3, action: .disabled)
        let latest = try Data(contentsOf: url)
        second.setAction(forButton: 4, action: .disabled)
        check(try Data(contentsOf: url) == latest, "A stale instance does not overwrite the newer file")
        check(second.mappingForCurrentProfile()[4] == nil && second.lastError != nil, "Stale-write failure rolls back the live candidate")
        second.load()
        second.setAction(forButton: 4, action: .disabled)
        check(second.mappingForCurrentProfile()[4] == .disabled, "Reloading allows a deliberate later edit")

        check(second.canUndo && second.undo(), "A successful edit can be undone")
        check(second.mappingForCurrentProfile()[4] == nil && second.mappingForCurrentProfile()[3] == .disabled, "Undo restores only the previous document")
        check(second.canRedo && second.redo() && second.mappingForCurrentProfile()[4] == .disabled, "Redo restores the edit")
        check(second.undo(), "An edit can be undone again")
        second.setAction(forButton: 5, action: .disabled)
        check(!second.canRedo, "A new edit clears the redo branch")
        let beforeInvalid = try Data(contentsOf: url)
        second.setAction(forButton: 6, action: .macro(steps: [MacroStep(type: "delay", delayMs: -1)], description: nil))
        check(try Data(contentsOf: url) == beforeInvalid, "An invalid macro never reaches disk")
        check(second.mappingForCurrentProfile()[6] == nil && second.lastError != nil, "An invalid action rolls back in memory")

        source.setAction(forButton: 2, action: .profileSwitch(profile: "Keep", description: nil))
        source.setAction(forButton: 3, action: .profileSwitch(profile: "Keep", description: nil), layer: .hypershift)
        source.setAppProfile("Keep", forBundleIdentifier: "app.incoming")
        try source.exportAllProfiles(to: exported)
        try imports.importProfiles(from: exported)
        check(imports.profiles["Keep 3"]?.buttons["2"]?.profile == "Keep 3", "Keeping both rewrites imported Standard layer switches")
        check(imports.profiles["Keep 3"]?.hypershift?["3"]?.profile == "Keep 3", "Keeping both rewrites imported Hypershift switches")
        check(imports.appProfiles["app.incoming"] == "Keep 3", "Incoming app bindings target the renamed incoming profile")
        check(imports.currentProfileName == "Keep", "Importing does not activate an incoming action")
        check(imports.deleteProfile(named: "Keep 3"), "An imported profile can be deleted")
        check(imports.appProfiles["app.incoming"] == nil, "Deleting removes its app bindings")
        check(imports.undo() && imports.profiles["Keep 3"]?.onboardOnly == source.profiles["Keep"]?.onboardOnly,
              "Undo recovers a deleted profile and its protection metadata")
        imports.setAction(forButton: 7, action: .profileSwitch(profile: "Keep 2", description: nil))
        imports.setAction(forButton: 8, action: .profileSwitch(profile: "Keep 2", description: nil), layer: .hypershift)
        check(imports.deleteProfile(named: "Keep 2"), "A referenced target can be deleted")
        check(imports.mappingForCurrentProfile()[7] == .disabled && imports.mappingForCurrentProfile(layer: .hypershift)[8] == .disabled,
              "Deleting a target disables dangling switches in both layers")

        let empty = root.appendingPathComponent("empty.json")
        try Data(#"{"profiles":{}}"#.utf8).write(to: empty)
        let beforeEmpty = try Data(contentsOf: root.appendingPathComponent("import-target.json"))
        do { try imports.importProfiles(from: empty, merge: false); check(false, "An empty replacement must be rejected") }
        catch { check(true, "An empty replacement is rejected") }
        check(try Data(contentsOf: root.appendingPathComponent("import-target.json")) == beforeEmpty, "Invalid replacement preserves the current file")

        guard let mouse = imports.createProfile(named: "Mouse", mapping: [1: copy], onboardOnly: true) else {
            throw Failure(description: "Mouse fixture can be saved")
        }
        imports.setCurrentProfile("Keep")
        imports.applyAutomaticProfile(mouse)
        check(imports.runtimeProfileName == "Keep", "Direct automatic switching cannot activate imported firmware mappings")
        imports.setAction(forButton: 9, action: .mouse(action: .dpiUp, description: nil), layer: .hypershift)
        check(imports.mappingForCurrentProfile(layer: .hypershift)[9] == .mouse(action: .dpiUp, description: nil),
              "Validation preserves hardware DPI functions in the Hypershift layer for Save to Mouse")
        let overwrite = root.appendingPathComponent("overwrite.json")
        let unprotected = ProfilesFile(profiles: [mouse: Profile(buttons: [:])], settings: nil)
        try JSONEncoder().encode(unprotected).write(to: overwrite)
        try imports.importProfiles(from: overwrite, policy: .replaceMatching)
        check(imports.profiles[mouse]?.onboardOnly == true, "Explicit replacement never removes existing mouse-import protection")

        // A backup write failure is still a failed transaction, even when the
        // primary file itself would be writable.
        let backupStore = ProfileFileStore(url: root.appendingPathComponent("backup-failure.json"))
        let backupConfig = ConfigManager(storageURL: backupStore.url, defaults: defaults)
        check(backupConfig.createProfile(name: "Saved"), "Backup failure fixture is saved")
        let beforeBackupFailure = try Data(contentsOf: backupStore.url)
        try FileManager.default.createDirectory(at: backupStore.backupURL, withIntermediateDirectories: false)
        backupConfig.setAction(forButton: 1, action: .disabled)
        check(try Data(contentsOf: backupStore.url) == beforeBackupFailure, "A backup failure preserves the primary file")
        check(backupConfig.mappingForCurrentProfile().isEmpty && backupConfig.lastError != nil, "A backup failure rolls back memory")

        // Refuse linked primary and lock paths rather than following them.
        let sentinel = root.appendingPathComponent("sentinel.json")
        try JSONEncoder().encode(unprotected).write(to: sentinel)
        try FileManager.default.setAttributes([.posixPermissions: 0o644], ofItemAtPath: sentinel.path)
        let linked = root.appendingPathComponent("linked.json")
        try FileManager.default.createSymbolicLink(at: linked, withDestinationURL: sentinel)
        let linkedConfig = ConfigManager(storageURL: linked, defaults: defaults)
        linkedConfig.load()
        check(linkedConfig.profilesReadOnly, "Loading a linked profile path enters protection mode")
        try JSONEncoder().encode(unprotected).write(to: root.appendingPathComponent("linked.backup.json"))
        check(!linkedConfig.restoreProfileBackup(), "Restore refuses a linked primary path")
        let sentinelMode = try FileManager.default.attributesOfItem(atPath: sentinel.path)[.posixPermissions] as? NSNumber
        check(sentinelMode?.intValue == 0o644, "A rejected restore never changes the linked target's permissions")
        let dangling = root.appendingPathComponent("dangling.json")
        try FileManager.default.createSymbolicLink(at: dangling, withDestinationURL: root.appendingPathComponent("missing-target.json"))
        let danglingConfig = ConfigManager(storageURL: dangling, defaults: defaults)
        danglingConfig.load()
        check(danglingConfig.profilesReadOnly && !danglingConfig.createProfile(name: "Never Replace Link"), "Dangling profile links are protected instead of treated as absent files")
        check(try FileManager.default.destinationOfSymbolicLink(atPath: dangling.path).hasSuffix("missing-target.json"), "A rejected edit preserves a dangling profile link")
        let lockedURL = root.appendingPathComponent("locked.json")
        try FileManager.default.createSymbolicLink(at: lockedURL.appendingPathExtension("lock"), withDestinationURL: sentinel)
        let lockedConfig = ConfigManager(storageURL: lockedURL, defaults: defaults)
        check(!lockedConfig.createProfile(name: "Never Saved"), "A linked lock file cannot be followed")
        let heldURL = root.appendingPathComponent("held-lock.json")
        let held = ConfigManager(storageURL: heldURL, defaults: defaults)
        check(held.createProfile(name: "Locked"), "Lock contention fixture is saved")
        let fd = Darwin.open(heldURL.appendingPathExtension("lock").path, O_RDWR)
        guard fd >= 0, flock(fd, LOCK_EX | LOCK_NB) == 0 else { throw Failure(description: "Test can reserve a profile lock") }
        let beforeContention = try Data(contentsOf: heldURL)
        held.setAction(forButton: 1, action: .disabled)
        check(try Data(contentsOf: heldURL) == beforeContention, "A contended profile lock returns without replacing the document")
        check(held.mappingForCurrentProfile().isEmpty, "A contended save rolls back the in-memory edit")
        flock(fd, LOCK_UN); Darwin.close(fd)

        let history = ConfigManager(storageURL: root.appendingPathComponent("history.json"), defaults: defaults)
        check(history.createProfile(name: "History"), "History fixture is saved")
        for index in 0..<16 { history.setAction(forButton: 1, action: .keySequence(keys: [KeyStroke(key: "a", modifiers: [])], description: String(index))) }
        var undos = 0
        while history.canUndo && undos < 20 { if history.undo() { undos += 1 } else { break } }
        check(undos == 12 && !history.canUndo, "Session history is bounded to twelve complete changes")

        let longName = String(repeating: "é", count: 128)
        let unique = ProfileImportDocument.uniqueName(longName, reserved: [longName])
        check(unique.utf8.count <= 256 && unique.hasSuffix(" 2"), "Import collisions preserve a valid bounded Unicode profile name")
        defaults.set("Keep 2", forKey: "currentProfile")
        let selection = ConfigManager(storageURL: root.appendingPathComponent("import-target.json"), defaults: defaults)
        selection.load()
        check(selection.currentProfileName == "Keep", "The saved document selection wins over a stale defaults selection")
        check(KeyStroke(key: "a", modifiers: ["COMMAND", "cmd", "option", "control"]).normalizedModifiers == ["ctrl", "alt", "cmd"],
              "Editors normalize legacy modifier aliases and duplicate spellings")
        let inMemory = second.profiles
        let validBeforeReload = try Data(contentsOf: url)
        try damaged.write(to: url)
        second.load()
        check(second.profiles == inMemory && second.profilesReadOnly, "A failed reload retains the previous in-memory profiles in protection mode")
        try validBeforeReload.write(to: url)
        second.load()
        check(!second.profilesReadOnly && second.profiles == inMemory, "Reloading a repaired file clears protection without losing data")
        let future = root.appendingPathComponent("future.json")
        try Data(#"{"formatVersion":2,"profiles":{"Future":{"buttons":{}}}}"#.utf8).write(to: future)
        let futureConfig = ConfigManager(storageURL: future, defaults: defaults)
        futureConfig.load()
        check(futureConfig.profilesReadOnly, "A newer unsupported file format enters protection mode")
        let futureBytes = try Data(contentsOf: future)
        check(!futureConfig.createProfile(name: "Do Not Replace"), "Editing cannot erase a newer format")
        check(try Data(contentsOf: future) == futureBytes, "Unsupported-format bytes remain unchanged")
        let encodedVersion = try JSONDecoder().decode(ProfilesFile.self, from: ProfileFileStore.encode(unprotected)).formatVersion
        check(encodedVersion == 1, "New files and exports identify their supported format version")

        let store = ProfileFileStore(url: url)
        _ = try store.load()
        let oversized = root.appendingPathComponent("oversized.json")
        try Data(repeating: 0x20, count: ProfileValidation.maximumFileBytes + 1).write(to: oversized)
        do { _ = try ProfileImportDocument(url: oversized); check(false, "Oversized import must fail") }
        catch { check(true, "Oversized import is bounded before decoding") }
        do { try ProfileValidation.validateMacro([MacroStep(type: "key", keyStroke: KeyStroke(key: "bad-key", modifiers: []))]); check(false, "Unknown macro keys must fail") }
        catch { check(true, "Unknown macro keys are rejected") }
        do { try ProfileValidation.validateMacro([MacroStep(type: "delay", delayMs: Int.max)]); check(false, "Overflowing delays must fail") }
        catch { check(true, "Overflowing delays are rejected before summing") }
        if !failures.isEmpty { throw Failure(description: failures.joined(separator: "; ")) }
        return count
    }
}
