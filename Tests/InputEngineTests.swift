import Foundation
import Cocoa
import IOKit.hid

enum InputEngineTests {
    struct Failure: Error, CustomStringConvertible {
        let description: String
    }

    static func run() throws -> Int {
        var count = 0
        func check(_ condition: @autoclosure () -> Bool, _ message: String) throws {
            guard condition() else { throw Failure(description: message) }
            count += 1
        }
        try check(NagaInput.isSupported(vendor: 0x1532, product: 0x00b4, name: nil), "Receiver ID")
        try check(!NagaInput.isSupported(vendor: 0x1532, product: 1, name: "Razer Keyboard"), "Other Razer keyboard excluded")
        try check(!NagaInput.isSupported(vendor: 0x068e, product: 1, name: "Keyboard"), "Unrelated vendor excluded")
        try check(!NagaInput.isSupported(vendor: 1, product: 0x00b4, name: "Naga"), "Product/name alone insufficient")
        let identities: [(Int, Int, String?, Bool)] = [
            (0x068e, 0x00b5, "Naga V2 HS", true),
            (0x068e, 0x00b5, nil, true),
            (0x068e, 0x0001, "Naga V2 HS", false),
            (0x068e, 0x00b4, "Razer Naga V2 HyperSpeed", false),
            (0x0001, 0x00b5, "Naga V2 HS", false),
            (0x1532, 0x00b5, "Razer Naga V2 HyperSpeed", true),
            (0x1532, 0x0001, "RAZER NAGA", true),
            (0x1532, 0x0001, nil, false)
        ]
        func enumerates(vendor: Int, product: Int) -> Bool {
            HIDListener.deviceMatchingCriteria.contains {
                $0[kIOHIDVendorIDKey] == vendor &&
                ($0[kIOHIDProductIDKey] == nil || $0[kIOHIDProductIDKey] == product)
            }
        }
        for (vendor, product, name, supported) in identities {
            try check(NagaInput.isSupported(vendor: vendor, product: product, name: name) == supported,
                      "Naga identity gate: \(vendor):\(product), \(name ?? "no name")")
            if supported {
                try check(enumerates(vendor: vendor, product: product), "Supported identity reaches HID callbacks")
            }
        }
        try check(enumerates(vendor: 0x1532, product: 0x00b4), "USB receiver still enumerated")
        try check(!enumerates(vendor: 0x068e, product: 1), "Bluetooth enumeration excludes unrelated products")
        try check(!enumerates(vendor: 1, product: 0x00b5), "Bluetooth product ID alone is insufficient")
        for index in 1...10 {
            try check(NagaInput.button(page: 7, usage: UInt32(0x1d + index), value: 1) == index, "Side keyboard usage")
        }
        try check(NagaInput.button(page: 7, usage: 0x2d, value: 1) == 11, "Minus")
        try check(NagaInput.button(page: 7, usage: 0x2e, value: 1) == 12, "Equals")
        try check(NagaInput.button(page: 9, usage: 3, value: 1) == 17, "Middle")
        try check(NagaInput.button(page: 9, usage: 1, value: 1) == 18, "Left")
        try check(NagaInput.button(page: 9, usage: 2, value: 1) == 19, "Right")
        try check(NagaInput.button(page: 12, usage: 0x238, value: -1) == 15, "AC Pan left")
        try check(NagaInput.button(page: 12, usage: 0x238, value: 1) == 16, "AC Pan right")
        try check(NagaInput.button(page: 12, usage: 0x238, value: 0) == nil, "Pan zero")
        try check(NagaInput.button(page: 0xff00, usage: 1, value: 0x80) == nil, "No guessed DPI cookies")

        var matcher = InputEdgeMatcher()
        matcher.record(button: 1, down: true, timestamp: 10)
        try check(!matcher.consume(button: 2, down: true, timestamp: 10.001), "Different key")
        try check(!matcher.consume(button: 1, down: false, timestamp: 10.001), "Wrong edge")
        try check(matcher.consume(button: 1, down: true, timestamp: 10.001), "Fresh edge")
        try check(!matcher.consume(button: 1, down: true, timestamp: 10.002), "Single consumption")
        matcher.record(button: 1, down: true, timestamp: 11)
        try check(!matcher.consume(button: 1, down: true, timestamp: 11.1), "100 ms is stale")
        matcher.record(button: 1, down: false, timestamp: 12)
        try check(matcher.consume(button: 1, down: false, timestamp: 12.001), "Independent release")
        matcher.record(button: 1, down: true, timestamp: 13)
        matcher.reset()
        try check(!matcher.consume(button: 1, down: true, timestamp: 13), "Reset clears evidence")

        // Fixtures reconstructed from published PR2850 protocol facts, not device captures.
        let held: [UInt8] = [4, 0x20, 0x21, 0x22, 0x23] + Array(repeating: 0, count: 11)
        try check(NagaInput.driverButtons(report: held) == Set([13, 14, 15, 16]), "Driver held set")
        try check(NagaInput.driverButtons(report: [4] + Array(repeating: 0, count: 15)) == [], "Driver release")
        try check(NagaInput.driverButtons(report: Array(held.prefix(8))) == nil, "Reject short packets")
        try check(NagaInput.driverButtons(report: [5] + Array(repeating: 0, count: 15)) == nil, "Reject other report IDs")
        try check(NagaInput.driverMouseButtons(report: [0x60] + Array(repeating: 0, count: 7)) == [15, 16], "Mouse tilt bits")
        try check(NagaInput.driverMouseButtons(report: [0x60]) == nil, "Reject truncated mouse packet")
        var driverState = DriverButtonState()
        try check(driverState.update(source: "mouse", buttons: [15]).pressed == [15], "First driver press")
        try check(driverState.update(source: "keyboard", buttons: [15]).pressed.isEmpty, "Cross-interface press deduped")
        try check(driverState.update(source: "mouse", buttons: []).released.isEmpty, "Other source still holds")
        try check(driverState.update(source: "keyboard", buttons: []).released == [15], "Final driver release")
        var repeatOwner = InputRepeatOwnership()
        let signature = InputRepeatOwnership.Signature(keyboardType: 1, sourcePID: 0, sourceState: 1)
        repeatOwner.claim(button: 1, signature: signature)
        try check(repeatOwner.matches(button: 1, signature: signature, physicallyHeld: true), "Owned Naga repeat suppressed")
        try check(!repeatOwner.matches(button: 1, signature: signature, physicallyHeld: false), "No repeat ownership after physical release")
        try check(!repeatOwner.matches(button: 2, signature: signature, physicallyHeld: true), "No ownership of other key")
        try check(!repeatOwner.matches(button: 1, signature: .init(keyboardType: 2, sourcePID: 0, sourceState: 1), physicallyHeld: true), "Other keyboard signature")
        repeatOwner.invalidate(button: 1)
        try check(!repeatOwner.matches(button: 1, signature: signature, physicallyHeld: true), "Intervening unqualified keyboard event revokes repeat ownership")

        var events: [CGEvent] = []
        let mapper = ButtonMapper(eventSink: { events.append($0) })
        mapper.modifierKeyState = { _ in false }
        mapper.frontmostBundleIdentifier = { "com.apple.TextEdit" }
        mapper.updateMapping([1: .mouse(action: .button4, description: nil), 2: .disabled])
        mapper.handlePress(buttonIndex: 1)
        mapper.handlePress(buttonIndex: 1)
        try check(events.count == 1, "Duplicate physical down deduplicated")
        try check(events[0].type == .otherMouseDown && events[0].getIntegerValueField(.mouseEventButtonNumber) == 3, "Real mouse button 4")
        mapper.handleRelease(buttonIndex: 1)
        try check(events.last?.type == .otherMouseUp, "Balanced mouse release")
        mapper.handle(buttonIndex: 2)
        mapper.handle(buttonIndex: 19)
        try check(events.count == 2, "Disabled and unmapped emit nothing")
        try check(mapper.hasMapping(buttonIndex: 2) && !mapper.hasMapping(buttonIndex: 19), "Disabled differs from nil")
        mapper.updateMapping([1: .mouse(action: .leftClick, description: nil)])
        mapper.handlePress(buttonIndex: 1)
        let move = CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: CGPoint(x: 40, y: 50), mouseButton: .left)!
        try check(mapper.dragEvent(for: move)?.type == .leftMouseDragged, "Held click becomes drag")
        mapper.updateMapping([:])
        try check(events.last?.type == .leftMouseUp, "Mapping change releases hold")
        try check(mapper.dragEvent(for: move) == nil, "No stale drag after reset")
        mapper.updateMapping([1: .keySequence(keys: [KeyStroke(key: "c", modifiers: ["cmd"])], description: nil)])
        events.removeAll()
        mapper.handlePress(buttonIndex: 1)
        try check(events.suffix(2).map(\.type) == [.flagsChanged, .keyDown], "Held shortcut presses its modifiers first")
        try check(events.last?.flags == .maskCommand, "Held key carries the modifier flags")
        mapper.releaseAll()
        try check(events.suffix(2).map(\.type) == [.keyUp, .flagsChanged], "Cleanup releases held key and its modifiers")
        try check(events.last?.flags == [], "Released modifiers leave no flags behind")
        try check(events.allSatisfy { $0.getIntegerValueField(.eventSourceUserData) == ButtonMapper.syntheticMarker }, "Every emitted event is tagged")

        // Regression for issue 6: modifiers must arrive as real key events,
        // not only as flags on an orphan keyDown.
        events.removeAll()
        mapper.updateMapping([1: .keySequence(keys: [KeyStroke(key: "c", modifiers: ["cmd", "shift"])], description: nil)])
        mapper.handle(buttonIndex: 1)
        try check(events.map(\.type) == [.flagsChanged, .flagsChanged, .keyDown, .keyUp, .flagsChanged, .flagsChanged], "One-shot shortcut wraps the key in modifier edges")
        try check(events[2].flags == [.maskCommand, .maskShift] && events[3].flags == [.maskCommand, .maskShift], "Key edges carry the requested modifiers")
        try check(events.last?.flags == [], "Modifier edges end with a clean flag state")
        events.removeAll()
        mapper.updateMapping([1: .keySequence(keys: [KeyStroke(key: "c", modifiers: ["cmd"]), KeyStroke(key: "v", modifiers: ["cmd"])], description: nil)])
        mapper.handle(buttonIndex: 1)
        try check(events.count == 8 && events[0].type == .flagsChanged && events[4].type == .flagsChanged, "Sequence steps press and release their own modifiers")
        events.removeAll()
        mapper.updateMapping([1: .keySequence(keys: [KeyStroke(key: "c", modifiers: ["cmd"])], description: nil),
                              2: .keySequence(keys: [KeyStroke(key: "v", modifiers: ["cmd"])], description: nil)])
        mapper.handlePress(buttonIndex: 1)
        mapper.handlePress(buttonIndex: 2)
        try check(events.count == 3, "A second hold reuses the modifier already down")
        mapper.handleRelease(buttonIndex: 1)
        try check(events.count == 4 && events.last?.type == .keyUp, "Releasing one hold keeps the shared modifier down")
        mapper.handleRelease(buttonIndex: 2)
        try check(events.count == 6 && events.last?.type == .flagsChanged && events.last?.flags == [], "The last release lets the shared modifier go")
        for action in [MouseAction.button5, .middleClick, .rightClick, .scrollUp, .scrollDown, .scrollLeft, .scrollRight] {
            mapper.updateMapping([1: .mouse(action: action, description: nil)])
            let before = events.count
            mapper.handle(buttonIndex: 1)
            try check(events.count > before, "Mouse action emits event: \(action)")
        }
        mapper.updateMapping([1: .mouse(action: .button4, description: nil)])
        mapper.handle(buttonIndex: 1)
        try check(events.suffix(2).map(\.type) == [.otherMouseDown, .otherMouseUp], "Button 4 is a real click outside browsers")
        try check(events.last?.getIntegerValueField(.mouseEventButtonNumber) == 3, "Button 4 uses button number 3")
        mapper.frontmostBundleIdentifier = { "com.google.Chrome.canary" }
        let beforeBrowser = events.count
        mapper.handle(buttonIndex: 1)
        try check(events.count == beforeBrowser, "Browser navigation is deferred to the main queue, no raw click emitted")
        try check(MouseAction.isBrowser(bundleIdentifier: "org.mozilla.firefox") && !MouseAction.isBrowser(bundleIdentifier: "com.apple.finder"), "Browser detection")
        try check(MouseAction.button4.browserEquivalent == .browserBack && MouseAction.button5.browserEquivalent == .browserForward && MouseAction.middleClick.browserEquivalent == nil, "Browser equivalents")
        let stroke = KeyboardLayoutShortcut.browserStroke(for: .browserBack)
        try check(stroke != nil && stroke!.modifiers.contains("cmd") && !stroke!.modifiers.contains("option"), "Browser shortcut never requires Option")

        let root = FileManager.default.temporaryDirectory.appendingPathComponent("NagaInputTests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let suite = "NagaInputTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suite)!
        defer {
            try? FileManager.default.removeItem(at: root)
            defaults.removePersistentDomain(forName: suite)
            ButtonMapper.shared.updateMapping([:])
        }
        let url = root.appendingPathComponent("profiles.json")
        let config = ConfigManager(storageURL: url, defaults: defaults)
        try check(config.createProfile(name: "Vuoto"), "Create blank profile")
        try check(config.mappingForCurrentProfile().isEmpty, "Blank mapping retained")
        for action in MouseAction.allCases {
            config.setAction(forButton: 1, action: .mouse(action: action, description: nil))
            let restored = ConfigManager(storageURL: url, defaults: defaults)
            restored.load()
            try check(restored.mappingForCurrentProfile()[1] == .mouse(action: action, description: nil), "Mouse JSON roundtrip \(action)")
        }
        config.setAction(forButton: 1, action: .disabled)
        let restored = ConfigManager(storageURL: url, defaults: defaults)
        restored.load()
        try check(restored.mappingForCurrentProfile()[1] == .disabled, "Disabled persisted")
        config.setAction(forButton: 1, action: nil)
        restored.load()
        try check(restored.mappingForCurrentProfile().isEmpty, "Nil removes mapping without fallback")
        try check(config.createProfile(name: "Secondo"), "Create second")
        config.setAction(forButton: 3, action: .profileSwitch(profile: "Vuoto", description: nil))
        config.setCurrentProfile("Vuoto")
        try check(config.renameProfile(from: "Vuoto", to: "Primo"), "Rename")
        restored.load()
        try check(restored.currentProfileName == "Primo", "Selection persisted after rename")
        restored.setCurrentProfile("Secondo")
        try check(restored.mappingForCurrentProfile()[3] == .profileSwitch(profile: "Primo", description: nil), "Renamed profile-switch target updated")
        try check(config.deleteProfile(named: "Secondo"), "Delete")
        restored.load()
        try check(restored.availableProfiles() == ["Primo"], "Deleted profiles do not reappear")
        let emptyURL = root.appendingPathComponent("empty.json")
        try Data(#"{"profiles":{}}"#.utf8).write(to: emptyURL)
        try config.importProfiles(from: emptyURL, merge: false)
        restored.load()
        try check(restored.profiles.isEmpty, "Empty profile collection survives reload")
        let legacyURL = root.appendingPathComponent("legacy.json")
        try Data(#"{"profiles":{"Legacy":{"buttons":{"1":{"type":"keySequence","keys":[{"key":"c","modifiers":["cmd"]}]}}}},"settings":{"currentProfile":"Legacy"}}"#.utf8).write(to: legacyURL)
        try config.importProfiles(from: legacyURL, merge: false)
        try check(config.mappingForCurrentProfile()[1] == .keySequence(keys: [KeyStroke(key: "c", modifiers: ["cmd"])], description: nil), "Legacy JSON accepted")
        // The visual selector must send and persist physical keys, including the active layout's symbols.
        let catalog = KeyboardKeyCatalog.current()
        try check(Set(catalog.map(\.code)).count == catalog.count, "Key picker has unique physical codes")
        try check(KeyboardKeyGroup.allCases.allSatisfy { group in catalog.contains { $0.group == group } }, "Every key group is populated")
        var selectedKeyEvents: [CGEvent] = []
        let selectedKeyMapper = ButtonMapper(eventSink: { selectedKeyEvents.append($0) })
        selectedKeyMapper.modifierKeyState = { _ in false }
        for entry in catalog {
            selectedKeyEvents.removeAll()
            let stroke = entry.stroke(modifiers: ["cmd", "shift"])
            selectedKeyMapper.updateMapping([1: .keySequence(keys: [stroke], description: nil)])
            selectedKeyMapper.handlePress(buttonIndex: 1)
            try check(selectedKeyEvents.count == 3 && selectedKeyEvents[2].type == .keyDown &&
                      selectedKeyEvents[2].getIntegerValueField(.keyboardEventKeycode) == Int64(entry.code) &&
                      selectedKeyEvents[2].flags.contains([.maskCommand, .maskShift]), "Picker key press: \(entry.key)")
            selectedKeyMapper.handleRelease(buttonIndex: 1)
            try check(selectedKeyEvents.count == 6 && selectedKeyEvents[3].type == .keyUp, "Picker key release: \(entry.key)")
        }
        for (audio, expectedCode) in [(AudioAction.volumeUp, 0), (.volumeDown, 1), (.mute, 7)] {
            selectedKeyEvents.removeAll()
            let action = ActionType.audio(action: audio, description: "Audio test")
            selectedKeyMapper.updateMapping([1: action])
            selectedKeyMapper.handlePress(buttonIndex: 1)
            selectedKeyMapper.handlePress(buttonIndex: 1)
            try check(selectedKeyEvents.count == 2, "Audio sends one pair and ignores duplicate press")
            for (offset, state) in [0xA, 0xB].enumerated() {
                let event = NSEvent(cgEvent: selectedKeyEvents[offset])!
                try check(event.type == .systemDefined && event.subtype.rawValue == 8, "Audio uses system media event")
                try check(event.data1 == (expectedCode << 16) | (state << 8), "Audio key and edge encoding")
                try check(selectedKeyEvents[offset].getIntegerValueField(.eventSourceUserData) == ButtonMapper.syntheticMarker, "Audio carries synthetic marker")
            }
            selectedKeyMapper.handleRelease(buttonIndex: 1)
            try check(selectedKeyEvents.count == 2, "Audio release does not repeat the action")
            selectedKeyMapper.handlePress(buttonIndex: 1)
            try check(selectedKeyEvents.count == 4, "Audio works on subsequent press")
            config.setAction(forButton: 2, action: action)
            restored.load()
            try check(restored.mappingForCurrentProfile()[2] == action, "Audio action survives profile reload")
        }
        let symbol = catalog.first { $0.group == .symbols }!
        let selectedAction = ActionType.keySequence(keys: [symbol.stroke(modifiers: ["alt"])], description: "Symbol")
        config.setAction(forButton: 2, action: selectedAction)
        restored.load()
        try check(restored.mappingForCurrentProfile()[2] == selectedAction, "Selected layout key survives profile reload")
        let sequence = ActionType.keySequence(keys: [catalog[0].stroke(), catalog[1].stroke(modifiers: ["ctrl"])], description: nil)
        config.setAction(forButton: 2, action: sequence)
        restored.load()
        try check(restored.mappingForCurrentProfile()[2] == sequence, "Multiple key steps survive reload")
        config.setAction(forButton: 3, action: .textSnippet(text: "Legacy text", description: nil))
        restored.load()
        try check(restored.mappingForCurrentProfile()[3] == .textSnippet(text: "Legacy text", description: nil), "Existing text mapping is not silently migrated")
        try check(Set(MouseHotspot.side.map(\.id)) == Set(1...12), "All side buttons have a photo target")
        try check(MouseHotspot.side.allSatisfy { $0.points.count == 4 && $0.points.allSatisfy { (0...1).contains($0.x) && (0...1).contains($0.y) } }, "Photo targets stay within the image")
        for height: CGFloat in [220, 440] {
            let size = CGSize(width: height * 2 / 3, height: height)
            for target in MouseHotspot.side {
                let center = CGPoint(x: target.points.map(\.x).reduce(0, +) / 4 * size.width,
                                     y: target.points.map(\.y).reduce(0, +) / 4 * size.height)
                try check(MouseHotspot.sideButton(at: center, imageSize: size) == target.id, "Hover identifies button \(target.id) at height \(height)")
            }
            try check(MouseHotspot.sideButton(at: CGPoint(x: size.width / 2, y: size.height / 2), imageSize: size) == nil, "Mouse body does not highlight a side button")
        }
                let blocked = root.appendingPathComponent("file")
        try Data().write(to: blocked)
        let failing = ConfigManager(storageURL: blocked.appendingPathComponent("profiles.json"), defaults: defaults)
        failing.saveUserProfiles()
        try check(failing.lastError != nil, "Save failure visible")
        return count
    }
}
