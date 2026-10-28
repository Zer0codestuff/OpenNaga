import Cocoa

enum SystemActionTests {
    struct Failure: Error, CustomStringConvertible {
        let description: String
    }

    static func run() throws -> Int {
        var count = 0
        func check(_ condition: @autoclosure () -> Bool, _ message: String) throws {
            guard condition() else { throw Failure(description: message) }
            count += 1
        }

        let catalog = SystemAction.allCases
        try check(catalog.count == 25, "All requested system actions are present")
        try check(Set(catalog.map(\.title)).count == catalog.count, "System titles are unique")
        try check(SystemActionGroup.allCases.allSatisfy { !$0.actions.isEmpty }, "Every system category has actions")
        try check(SystemActionGroup.allCases.flatMap(\.actions) == catalog, "System categories preserve catalog order")
        for action in catalog {
            try check(NSImage(systemSymbolName: action.symbol, accessibilityDescription: nil) != nil, "System icon exists: \(action)")
            try check(!action.help.isEmpty, "System action has help: \(action)")
            try check(ActionType.system(action: action, description: nil).displayName == action.title, "Default system label: \(action)")
            try check(ActionType.system(action: action, description: "Custom").displayName == "Custom", "Custom system label: \(action)")
        }

        var events: [CGEvent] = []
        var workspaceActions: [SystemAction] = []
        let mapper = ButtonMapper(eventSink: { events.append($0) }, workspaceActionSink: { workspaceActions.append($0) })
        mapper.modifierKeyState = { _ in false }
        mapper.systemShortcutPreferences = { [:] }
        let mediaKeys: [(SystemAction, Int)] = [
            (.volumeUp, 0), (.volumeDown, 1), (.mute, 7),
            (.playPause, 16), (.previousTrack, 20), (.nextTrack, 19),
            (.brightnessUp, 2), (.brightnessDown, 3)
        ]
        for (action, expectedCode) in mediaKeys {
            events.removeAll()
            mapper.updateMapping([1: .system(action: action, description: nil)])
            mapper.handlePress(buttonIndex: 1)
            mapper.handlePress(buttonIndex: 1)
            try check(events.count == 2, "System media action emits one pair per press: \(action)")
            for (index, state) in [0xA, 0xB].enumerated() {
                let event = NSEvent(cgEvent: events[index])!
                try check(event.type == .systemDefined && event.subtype.rawValue == 8, "System media event type: \(action)")
                try check(event.data1 == (expectedCode << 16) | (state << 8), "System media key and edge: \(action)")
                try check(events[index].getIntegerValueField(.eventSourceUserData) == ButtonMapper.syntheticMarker, "System media marker: \(action)")
            }
            mapper.handleRelease(buttonIndex: 1)
            try check(events.count == 2, "System media action does not repeat on release: \(action)")
            mapper.handle(buttonIndex: 1)
            try check(events.count == 4, "System media action works on next press: \(action)")
            mapper.releaseAll()
            try check(events.count == 4, "System media action leaves no held output: \(action)")
        }

        let keyboardActions: [(SystemAction, CGKeyCode, CGEventFlags)] = [
            (.screenshot, 20, [.maskCommand, .maskShift]),
            (.screenshotClipboard, 20, [.maskCommand, .maskShift, .maskControl]),
            (.screenshotSelection, 21, [.maskCommand, .maskShift]),
            (.screenshotSelectionClipboard, 21, [.maskCommand, .maskShift, .maskControl]),
            (.screenshotOptions, 23, [.maskCommand, .maskShift]),
            (.missionControl, 126, .maskControl),
            (.applicationWindows, 125, .maskControl),
            (.showDesktop, 103, []),
            (.previousSpace, 123, .maskControl),
            (.nextSpace, 124, .maskControl),
            (.switchApplication, 48, .maskCommand),
            (.notificationCenter, 45, .maskSecondaryFn)
        ]
        for (action, code, flags) in keyboardActions {
            events.removeAll()
            mapper.updateMapping([1: .system(action: action, description: nil)])
            mapper.handlePress(buttonIndex: 1)
            mapper.handlePress(buttonIndex: 1)
            let modifierCount = [.maskCommand, .maskControl, .maskShift, .maskAlternate, .maskSecondaryFn].filter { flags.contains($0) }.count
            let expectedCount = 2 + modifierCount * 2
            try check(events.count == expectedCount, "System shortcut ignores duplicate press: \(action)")
            let keyEvents = events.filter { $0.type == .keyDown || $0.type == .keyUp }
            try check(keyEvents.map(\.type) == [.keyDown, .keyUp], "System shortcut balances key edges: \(action)")
            let nativeFlags = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: true)!.flags.intersection(.maskNumericPad)
            try check(keyEvents.allSatisfy { $0.getIntegerValueField(.keyboardEventKeycode) == code && $0.flags == flags.union(nativeFlags) }, "System shortcut key and modifiers: \(action)")
            try check(events.filter { $0.type == .flagsChanged }.count == modifierCount * 2, "System shortcut balances modifier edges: \(action)")
            try check(events.allSatisfy { $0.getIntegerValueField(.eventSourceUserData) == ButtonMapper.syntheticMarker }, "System shortcut events are tagged: \(action)")
            mapper.handleRelease(buttonIndex: 1)
            mapper.releaseAll()
            try check(events.count == expectedCount, "System shortcut leaves no held output: \(action)")
            if action == .switchApplication {
                try check(events.last?.type == .flagsChanged && events.last?.flags == [], "App switching releases Command")
            }
        }

        func override(_ code: Int = 96, _ flags: Int = 786432, enabled: Bool = true) -> [String: Any] {
            ["enabled": enabled, "value": ["type": "standard", "parameters": [65535, code, flags]]]
        }
        let screenshot = SystemAction.screenshot.shortcut!
        try check(screenshot.resolve(in: ["28": override()]) == SystemKeyStroke(keyCode: 96, flags: [.maskControl, .maskAlternate]), "Custom macOS shortcut overrides default")
        try check(screenshot.resolve(in: ["28": override(enabled: false)]) == nil, "Disabled macOS shortcut never falls back")
        try check(screenshot.resolve(in: ["28": override(65535)]) == nil, "Unassigned key is not emitted")
        try check(screenshot.resolve(in: ["28": override(-1)]) == nil, "Negative key is rejected")
        try check(screenshot.resolve(in: ["28": override(128)]) == nil, "Invalid virtual key is rejected")
        try check(screenshot.resolve(in: ["28": override(96, -1)]) == nil, "Negative modifiers are rejected")
        try check(screenshot.resolve(in: ["28": ["enabled": true]]) == nil, "Incomplete shortcut is rejected")
        try check(screenshot.resolve(in: ["28": ["enabled": true, "value": ["type": "mouse", "parameters": [0, 0, 0]]]]) == nil, "Mouse shortcut is not sent as a key")
        try check(screenshot.resolve(in: ["28": ["enabled": true, "value": ["type": "standard", "parameters": [0, 96]]]]) == nil, "Truncated shortcut is rejected")
        try check(screenshot.resolve(in: ["28": "invalid"]) == nil, "Malformed override never falls back")
        try check(screenshot.resolve(in: ["28": override(96, 1114112)])?.flags == .maskCommand, "Caps Lock state is not copied into a shortcut")
        try check(SystemAction.doNotDisturb.shortcut?.resolve(in: [:]) == nil, "Do Not Disturb requires an assigned shortcut")
        try check(SystemAction.doNotDisturb.shortcut?.id == 175, "Do Not Disturb uses its own symbolic shortcut")
        try check(SystemAction.previousSpace.shortcut?.id == 79 && SystemAction.nextSpace.shortcut?.id == 81, "Space navigation uses macOS symbolic IDs")

        events.removeAll()
        mapper.systemShortcutPreferences = { ["175": override()] }
        try check(mapper.performSystem(.doNotDisturb) == nil, "Configured Do Not Disturb can run")
        try check(events.count == 6 && events[2].getIntegerValueField(.keyboardEventKeycode) == 96, "Do Not Disturb sends the configured key")
        events.removeAll()
        mapper.systemShortcutPreferences = { ["175": override(enabled: false)] }
        try check(mapper.performSystem(.doNotDisturb) == SystemAction.doNotDisturb.shortcutSetupMessage, "Missing shortcut returns setup instructions")
        try check(events.isEmpty, "Disabled Do Not Disturb emits no key")
        mapper.systemShortcutPreferences = { ["28": override()] }
        mapper.performSystem(.screenshot)
        try check(events.count == 6 && events[2].flags == [.maskControl, .maskAlternate], "Runner reads current shortcut preferences")

        for action in [SystemAction.spotlight, .finder, .systemSettings, .hideApplication] {
            events.removeAll()
            workspaceActions.removeAll()
            mapper.updateMapping([1: .system(action: action, description: nil)])
            mapper.handlePress(buttonIndex: 1)
            mapper.handlePress(buttonIndex: 1)
            mapper.handleRelease(buttonIndex: 1)
            try check(workspaceActions == [action] && events.isEmpty, "Workspace action runs once without synthesized shortcuts: \(action)")
        }
        try check(SystemAction.spotlight.applicationBundleIdentifier == "com.apple.Spotlight", "Spotlight opens directly, even when its shortcut is disabled")
        try check(SystemAction.finder.applicationBundleIdentifier == "com.apple.finder", "Finder target")
        try check(SystemAction.systemSettings.applicationBundleIdentifier == "com.apple.systempreferences", "System Settings target")

        let root = FileManager.default.temporaryDirectory.appendingPathComponent("NagaSystemTests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let suite = "NagaSystemTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suite)!
        defer {
            try? FileManager.default.removeItem(at: root)
            defaults.removePersistentDomain(forName: suite)
            ButtonMapper.shared.updateMapping([:])
        }
        let url = root.appendingPathComponent("profiles.json")
        let config = ConfigManager(storageURL: url, defaults: defaults)
        let restored = ConfigManager(storageURL: url, defaults: defaults)
        try check(config.createProfile(name: "System test"), "Create isolated system action profile")
        for system in catalog {
            let action = ActionType.system(action: system, description: "Custom")
            config.setAction(forButton: 1, action: action)
            restored.load()
            try check(restored.mappingForCurrentProfile()[1] == action, "System action survives JSON roundtrip: \(system)")
        }
        for audio in AudioAction.allCases {
            let action = ActionType.audio(action: audio, description: "Existing audio")
            config.setAction(forButton: 2, action: action)
            let before = try Data(contentsOf: url)
            restored.load()
            let after = try Data(contentsOf: url)
            try check(restored.mappingForCurrentProfile()[2] == action && before == after, "Loading does not migrate or rewrite existing audio: \(audio)")
            try check(SystemAction(audio: audio).title == audio.title, "Legacy audio keeps its system label: \(audio)")
        }
        let shell = ActionType.systemCommand(command: "echo example", description: "Existing shell")
        config.setAction(forButton: 3, action: shell)
        restored.load()
        try check(restored.mappingForCurrentProfile()[3] == shell, "System actions do not replace shell commands")
        let exported = root.appendingPathComponent("export.json")
        try config.exportAllProfiles(to: exported)
        let imported = ConfigManager(storageURL: root.appendingPathComponent("import.json"), defaults: defaults)
        try imported.importProfiles(from: exported, merge: false)
        try check(imported.mappingForCurrentProfile() == config.mappingForCurrentProfile(), "Mixed system, audio and shell profile imports unchanged")
        return count
    }
}
