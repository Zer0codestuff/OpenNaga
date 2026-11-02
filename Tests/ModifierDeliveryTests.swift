import Cocoa
import Carbon.HIToolbox

enum ModifierDeliveryTests {
    struct Failure: Error, CustomStringConvertible {
        let description: String
    }

    @MainActor
    static func run() async throws -> Int {
        var count = 0
        func check(_ condition: @autoclosure () -> Bool, _ message: String) throws {
            guard condition() else { throw Failure(description: message) }
            count += 1
        }
        func physicalEdge(code: CGKeyCode, flags: CGEventFlags) -> CGEvent {
            let event = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: !flags.isEmpty)!
            event.type = .flagsChanged
            event.flags = flags
            return event
        }
        let physicalKeys: [(CGKeyCode, String, CGEventFlags)] = [
            (CGKeyCode(kVK_Command), "cmd", .maskCommand),
            (CGKeyCode(kVK_RightCommand), "cmd", .maskCommand),
            (CGKeyCode(kVK_Control), "ctrl", .maskControl),
            (CGKeyCode(kVK_RightControl), "ctrl", .maskControl),
            (CGKeyCode(kVK_Shift), "shift", .maskShift),
            (CGKeyCode(kVK_RightShift), "shift", .maskShift),
            (CGKeyCode(kVK_Option), "alt", .maskAlternate),
            (CGKeyCode(kVK_RightOption), "alt", .maskAlternate),
            (CGKeyCode(kVK_Function), "fn", .maskSecondaryFn)
        ]
        for (code, name, flag) in physicalKeys {
            var events: [CGEvent] = []
            let mapper = ButtonMapper(eventSink: { events.append($0) })
            mapper.modifierKeyState = { $0 == code }
            mapper.updateMapping([1: .keySequence(keys: [KeyStroke(key: "c", modifiers: [name])], description: nil)])
            mapper.handle(buttonIndex: 1)
            try check(events.map(\.type) == [.keyDown, .keyUp], "Physical modifier \(code) is borrowed without synthetic edges")
            try check(events.allSatisfy { $0.flags.contains(flag) }, "Borrowed physical modifier \(code) remains on both key edges")
        }

        // A plain mapping must combine with keyboard modifiers even when the
        // event carries only aggregate flags or an unrelated device clears them.
        for (code, _, flag) in physicalKeys where flag != .maskSecondaryFn {
            for stroke in [KeyStroke(key: "c", modifiers: []), KeyStroke(key: "kp1", modifiers: [])] {
                var plainEvents: [CGEvent] = []
                let plainMapper = ButtonMapper(eventSink: { plainEvents.append($0) })
                plainMapper.modifierKeyState = { $0 == code }
                plainMapper.updateMapping([1: .keySequence(keys: [stroke], description: nil)])
                plainMapper.observePhysicalModifier(physicalEdge(code: code, flags: flag))
                plainMapper.handle(buttonIndex: 1)
                try check(plainEvents.map(\.type) == [.keyDown, .keyUp], "Plain \(stroke.key) borrows physical modifier \(code) without synthetic edges")
                try check(plainEvents.allSatisfy { $0.flags.contains(flag) }, "Plain \(stroke.key) combines with aggregate-only physical modifier \(code)")
                try check(plainEvents.allSatisfy { $0.flags.contains(.maskNumericPad) == (stroke.key == "kp1") }, "Plain \(stroke.key) preserves keypad classification with physical modifier \(code)")
            }
        }

        var physicalEvents: [CGEvent] = []
        var physicalKeysDown: Set<CGKeyCode> = [CGKeyCode(kVK_Command)]
        let physicalMapper = ButtonMapper(eventSink: { physicalEvents.append($0) })
        physicalMapper.modifierKeyState = { physicalKeysDown.contains($0) }
        physicalMapper.updateMapping([1: .keySequence(keys: [KeyStroke(key: "c", modifiers: [])], description: nil)])
        physicalMapper.observePhysicalModifier(physicalEdge(code: CGKeyCode(kVK_Command), flags: .maskCommand))
        physicalKeysDown.insert(CGKeyCode(kVK_RightCommand))
        physicalMapper.observePhysicalModifier(physicalEdge(code: CGKeyCode(kVK_RightCommand), flags: .maskCommand))
        physicalKeysDown.remove(CGKeyCode(kVK_Command))
        physicalMapper.observePhysicalModifier(physicalEdge(code: CGKeyCode(kVK_Command), flags: .maskCommand))
        physicalMapper.handlePress(buttonIndex: 1, inputFlags: .maskCommand)
        try check(physicalEvents.last?.flags.contains(.maskCommand) == true, "Aggregate-only left release preserves physical right Command")
        physicalKeysDown.removeAll()
        physicalMapper.observePhysicalModifier(physicalEdge(code: CGKeyCode(kVK_RightCommand), flags: []))
        physicalMapper.handleRelease(buttonIndex: 1)
        try check(physicalEvents.last?.flags.contains(.maskCommand) == false, "A plain held key release does not resurrect a released physical modifier")

        physicalEvents.removeAll()
        physicalKeysDown = [CGKeyCode(kVK_Command)]
        physicalMapper.observePhysicalModifier(physicalEdge(code: CGKeyCode(kVK_Command), flags: []))
        physicalMapper.handle(buttonIndex: 1)
        try check(physicalEvents.count == 2 && physicalEvents.allSatisfy { $0.flags.contains(.maskCommand) }, "An unowned foreign clear does not hide the hardware keyboard's held Command")

        var ownedEvents: [CGEvent] = []
        let ownedMapper = ButtonMapper(eventSink: { ownedEvents.append($0) })
        ownedMapper.modifierKeyState = { _ in false }
        ownedMapper.updateMapping([1: .keySequence(keys: [KeyStroke(key: "c", modifiers: ["cmd"])], description: nil)])
        ownedMapper.handlePress(buttonIndex: 1)
        ownedMapper.modifierKeyState = { $0 == CGKeyCode(kVK_Command) }
        ownedMapper.observePhysicalModifier(physicalEdge(code: CGKeyCode(kVK_Command), flags: .maskCommand))
        ownedMapper.observePhysicalModifier(physicalEdge(code: CGKeyCode(kVK_Command), flags: []))
        ownedMapper.handleRelease(buttonIndex: 1)
        try check(ownedEvents.last?.flags.contains(.maskCommand) == false, "An owned synthetic modifier is not adopted as physical ownership after an aggregate release")

        for key in (0...9).map({ "kp\($0)" }) + ["keypad enter", "left arrow", "right arrow", "up arrow", "down arrow"] {
            var nativeEvents: [CGEvent] = []
            let nativeMapper = ButtonMapper(eventSink: { nativeEvents.append($0) })
            nativeMapper.modifierKeyState = { _ in false }
            nativeMapper.updateMapping([1: .keySequence(keys: [KeyStroke(key: key, modifiers: [])], description: nil)])
            nativeMapper.handle(buttonIndex: 1)
            try check(nativeEvents.map(\.type) == [.keyDown, .keyUp] && nativeEvents.allSatisfy { $0.flags.contains(.maskNumericPad) }, "Plain \(key) preserves its native numeric-pad flags on both key edges")
        }

        var events: [CGEvent] = []
        let mapper = ButtonMapper(eventSink: { events.append($0) })
        mapper.modifierKeyState = { _ in false }
        mapper.updateMapping([
            1: .keySequence(keys: [KeyStroke(key: "c", modifiers: ["cmd"])], description: nil),
            2: .keySequence(keys: [KeyStroke(key: "v", modifiers: ["shift"])], description: nil)
        ])
        mapper.handlePress(buttonIndex: 1)
        mapper.handlePress(buttonIndex: 2)
        try check(events[2].flags.contains([.maskCommand, .maskShift]), "New modifier edge preserves an existing held modifier")
        try check(events[3].flags.contains([.maskCommand, .maskShift]), "Overlapping key holds carry all active modifiers")
        mapper.handleRelease(buttonIndex: 1)
        try check(events[4].flags.contains([.maskCommand, .maskShift]) && events[5].flags == .maskShift,
                  "Releasing one hold keeps the remaining modifier on key and modifier edges")
        mapper.handleRelease(buttonIndex: 2)
        try check(events.last?.flags == [], "Overlapping holds release all owned modifiers")

        events.removeAll()
        mapper.updateMapping([
            1: .keySequence(keys: [KeyStroke(key: "c", modifiers: ["cmd"])], description: nil),
            2: .keySequence(keys: [KeyStroke(key: "c", modifiers: ["cmd"])], description: nil)
        ])
        mapper.handlePress(buttonIndex: 1)
        mapper.handlePress(buttonIndex: 2)
        try check(events.count == 2, "Two controls holding the same key share one key press")
        mapper.handleRelease(buttonIndex: 1)
        try check(events.count == 2, "A shared held key stays pressed after the first control releases")
        mapper.handleRelease(buttonIndex: 2)
        try check(events.map(\.type) == [.flagsChanged, .keyDown, .keyUp, .flagsChanged], "The last shared hold balances key and modifier edges")

        events.removeAll()
        mapper.updateMapping([
            1: .keySequence(keys: [KeyStroke(key: "c", modifiers: ["cmd"])], description: nil),
            2: .keySequence(keys: [KeyStroke(key: "c", modifiers: ["ctrl"]), KeyStroke(key: "v", modifiers: ["cmd"])], description: nil)
        ])
        mapper.handlePress(buttonIndex: 1)
        mapper.handle(buttonIndex: 2)
        let cEvents = events.filter { $0.getIntegerValueField(.keyboardEventKeycode) == kVK_ANSI_C }
        try check(cEvents.map(\.type) == [.keyDown, .keyDown], "A sequence does not release a key held by another control")
        try check(cEvents.last?.getIntegerValueField(.keyboardEventAutorepeat) == 1, "A sequence retrigger of a held key is marked as repeat")
        try check(cEvents.last?.flags.contains([.maskCommand, .maskControl]) == true, "Sequence strokes preserve other held modifiers")
        mapper.releaseAll()
        try check(events.last?.flags == [], "Sequence and hold cleanup leaves no owned modifier")

        events.removeAll()
        mapper.updateMapping([1: .keySequence(keys: [KeyStroke(key: "c", modifiers: ["cmd"])], description: nil)])
        mapper.handlePress(buttonIndex: 1)
        let physicalCommand: CGEventFlags = [.maskCommand, CGEventFlags(rawValue: 0x08)]
        mapper.observePhysicalModifier(physicalEdge(code: CGKeyCode(kVK_Command), flags: physicalCommand))
        mapper.handleRelease(buttonIndex: 1)
        try check(events.last?.flags == physicalCommand, "A physical Command pressed during a synthetic hold survives its release")
        mapper.observePhysicalModifier(physicalEdge(code: CGKeyCode(kVK_Command), flags: []))
        events.removeAll()
        mapper.handle(buttonIndex: 1)
        try check(events.count == 4 && events.last?.flags == [], "A later physical release does not leave stale modifier ownership")

        events.removeAll()
        mapper.handlePress(buttonIndex: 1)
        mapper.observePhysicalModifier(physicalEdge(code: CGKeyCode(kVK_RightShift), flags: [.maskShift, CGEventFlags(rawValue: 0x04)]))
        mapper.handleRelease(buttonIndex: 1)
        try check(events[2].flags.contains([.maskCommand, .maskShift]), "Key release preserves an unrelated physical modifier")
        try check(events.last?.flags.contains(.maskShift) == true && events.last?.flags.contains(.maskCommand) == false,
                  "Synthetic modifier release preserves physical right Shift")
        mapper.observePhysicalModifier(physicalEdge(code: CGKeyCode(kVK_RightShift), flags: []))

        var borrowedEvents: [CGEvent] = []
        let borrowed = ButtonMapper(eventSink: { borrowedEvents.append($0) })
        borrowed.modifierKeyState = { $0 == CGKeyCode(kVK_Command) }
        borrowed.updateMapping([1: .keySequence(keys: [KeyStroke(key: "c", modifiers: ["cmd"])], description: nil)])
        borrowed.handlePress(buttonIndex: 1)
        borrowed.modifierKeyState = { _ in false }
        borrowed.observePhysicalModifier(physicalEdge(code: CGKeyCode(kVK_Command), flags: []))
        try check(borrowedEvents.map(\.type) == [.keyDown, .flagsChanged], "Releasing a borrowed physical modifier reasserts the modifier a hold still requires")
        borrowed.handleRelease(buttonIndex: 1)
        try check(borrowedEvents.last?.flags == [], "A reasserted modifier is released with its final hold")

        events.removeAll()
        let ignored = physicalEdge(code: CGKeyCode(kVK_Command), flags: physicalCommand)
        ignored.setIntegerValueField(.eventSourceUserData, value: ButtonMapper.syntheticMarker)
        mapper.observePhysicalModifier(ignored)
        mapper.handle(buttonIndex: 1)
        try check(events.count == 4 && events.last?.flags == [], "Synthetic modifier edges cannot be mistaken for physical ownership")

        events.removeAll()
        mapper.modifierKeyState = { $0 == CGKeyCode(kVK_RightCommand) }
        mapper.resetPhysicalModifierObservation()
        mapper.handle(buttonIndex: 1)
        try check(events.map(\.type) == [.keyDown, .keyUp], "Restarting modifier observation refreshes keys changed while a tap was stopped")
        mapper.modifierKeyState = { _ in false }
        mapper.resetPhysicalModifierObservation()

        events.removeAll()
        mapper.runMacro([
            MacroStep(type: "key", keyStroke: KeyStroke(key: "c", modifiers: ["cmd"])),
            MacroStep(type: "delay", delayMs: 10),
            MacroStep(type: "key", keyStroke: KeyStroke(key: "v", modifiers: ["ctrl", "shift"]))
        ])
        for _ in 0..<100 where events.count < 10 { try await Task.sleep(nanoseconds: 5_000_000) }
        try check(events.count == 10, "Macro key steps emit balanced modifier and key edges across a delay")
        try check(events[1].flags == .maskCommand && events[6].flags == [.maskControl, .maskShift], "Each macro key step receives its own modifiers")
        try check(events.last?.flags == [], "Macro completion leaves no synthetic modifiers")

        events.removeAll()
        mapper.runMacro([MacroStep(type: "delay", delayMs: 30), MacroStep(type: "key", keyStroke: KeyStroke(key: "c", modifiers: ["cmd"]))])
        mapper.updateMapping([:])
        try await Task.sleep(nanoseconds: 50_000_000)
        try check(events.isEmpty, "Changing mappings cancels queued macro steps before modifier injection")
        // Modifiers the user holds on the triggering input combine with the
        // mapped key even when no physical modifier edge was observed.
        let commandHeld = CGEventFlags(rawValue: CGEventFlags.maskCommand.rawValue | 0x08)
        func heldInputMapper(_ mapping: [Int: ActionType]) -> (ButtonMapper, () -> [CGEvent]) {
            var captured: [CGEvent] = []
            let mapper = ButtonMapper(eventSink: { captured.append($0) })
            mapper.modifierKeyState = { _ in false }
            mapper.updateMapping(mapping)
            return (mapper, { captured })
        }
        do {
            let (mapper, events) = heldInputMapper([1: .keySequence(keys: [KeyStroke(key: "c", modifiers: [])], description: nil)])
            mapper.handlePress(buttonIndex: 1, inputFlags: commandHeld)
            try check(events().map(\.type) == [.keyDown] && events()[0].flags.contains(.maskCommand),
                      "Held Command combines with a side button mapped to C")
            try check(events()[0].flags.rawValue & 0x08 != 0, "Held Command keeps its left/right device bit")
            mapper.handleRelease(buttonIndex: 1, inputFlags: commandHeld)
            try check(events().map(\.type) == [.keyDown, .keyUp] && events()[1].flags.contains(.maskCommand),
                      "The key release matches the combined key press")
        }
        do {
            let (mapper, events) = heldInputMapper([1: .keySequence(keys: [KeyStroke(key: "c", modifiers: ["cmd"])], description: nil)])
            mapper.handle(buttonIndex: 1, inputFlags: commandHeld)
            try check(events().map(\.type) == [.keyDown, .keyUp] && events().allSatisfy { $0.flags.contains(.maskCommand) },
                      "A held Command the mapping also needs is borrowed, not pressed again")
        }
        do {
            let (mapper, events) = heldInputMapper([1: .keySequence(keys: [KeyStroke(key: "v", modifiers: ["shift"])], description: nil)])
            mapper.handle(buttonIndex: 1, inputFlags: commandHeld)
            let keys = events().filter { $0.type == .keyDown || $0.type == .keyUp }
            try check(keys.count == 2 && keys.allSatisfy { $0.flags.contains([.maskCommand, .maskShift]) },
                      "Held modifiers combine with the mapping's own modifiers")
            try check(events().filter { $0.type == .flagsChanged }.count == 2 && events().last?.flags == commandHeld,
                      "The mapping's modifier edges preserve borrowed Command")
        }
        do {
            let (mapper, events) = heldInputMapper([1: .keySequence(keys: [KeyStroke(key: "kp1", modifiers: [])], description: nil)])
            mapper.handle(buttonIndex: 1, inputFlags: commandHeld)
            try check(events().count == 2 && events().allSatisfy {
                $0.getIntegerValueField(.keyboardEventKeycode) == kVK_ANSI_Keypad1 && $0.flags.contains([.maskCommand, .maskNumericPad])
            }, "Held Command combines with a keypad mapping, which keeps the numeric pad flag")
        }
        do {
            let (mapper, events) = heldInputMapper([1: .keySequence(keys: [KeyStroke(key: "a", modifiers: []), KeyStroke(key: "b", modifiers: [])], description: nil)])
            mapper.handle(buttonIndex: 1, inputFlags: [.maskShift])
            try check(events().count == 4 && events().allSatisfy { $0.flags.contains(.maskShift) },
                      "Held modifiers apply to every key of a sequence")
            mapper.handle(buttonIndex: 1)
            try check(events().count == 8 && events().dropFirst(4).allSatisfy { !$0.flags.contains(.maskShift) },
                      "A press without held modifiers sends plain keys")
        }
        do {
            let (mapper, events) = heldInputMapper([
                1: .keySequence(keys: [KeyStroke(key: "c", modifiers: ["cmd"])], description: nil),
                2: .keySequence(keys: [KeyStroke(key: "v", modifiers: [])], description: nil)
            ])
            mapper.handle(buttonIndex: 1)
            let release = events().last!
            let before = events().count
            mapper.handle(buttonIndex: 2, inputFlags: commandHeld)
            try check(events().dropFirst(before).allSatisfy { !$0.flags.contains(.maskCommand) },
                      "A queued synthetic Command is not inherited before its release reaches the event tap")
            mapper.observeSyntheticModifier(release)
            let later = events().count
            mapper.handle(buttonIndex: 2, inputFlags: commandHeld)
            try check(events().count == later + 2 && events().dropFirst(later).allSatisfy { $0.flags.contains(.maskCommand) },
                      "Fresh input modifiers combine after the synthetic release is acknowledged")
        }
        do {
            let (mapper, events) = heldInputMapper([
                1: .keySequence(keys: [KeyStroke(key: "c", modifiers: ["cmd"])], description: nil),
                2: .keySequence(keys: [KeyStroke(key: "v", modifiers: [])], description: nil)
            ])
            mapper.handle(buttonIndex: 1)
            let release = events().last!
            mapper.modifierKeyState = { $0 == CGKeyCode(kVK_Command) }
            mapper.observePhysicalModifier(physicalEdge(code: CGKeyCode(kVK_Command), flags: .maskCommand))
            let before = events().count
            mapper.handle(buttonIndex: 2, inputFlags: .maskCommand)
            try check(events().dropFirst(before).allSatisfy { $0.flags.contains(.maskCommand) },
                      "A new physical Command combines immediately while a synthetic release is queued")
            mapper.observeSyntheticModifier(release)
            try check(release.flags.contains(.maskCommand), "A queued synthetic release cannot drop a newly pressed physical Command")
            mapper.modifierKeyState = { _ in false }
            mapper.observePhysicalModifier(physicalEdge(code: CGKeyCode(kVK_Command), flags: []))
            let afterRelease = events().count
            mapper.handle(buttonIndex: 2)
            try check(events().dropFirst(afterRelease).allSatisfy { !$0.flags.contains(.maskCommand) },
                      "The fresh physical Command releases without sticking to later input")
        }
        do {
            let (mapper, events) = heldInputMapper([
                1: .keySequence(keys: [KeyStroke(key: "c", modifiers: ["cmd"])], description: nil),
                2: .keySequence(keys: [KeyStroke(key: "v", modifiers: [])], description: nil)
            ])
            mapper.handle(buttonIndex: 1)
            mapper.resetPhysicalModifierObservation()
            let before = events().count
            mapper.handle(buttonIndex: 2, inputFlags: commandHeld)
            try check(events().dropFirst(before).allSatisfy { $0.flags.contains(.maskCommand) },
                      "A recreated tap does not retain an acknowledgment that the old tap cannot receive")
        }
        let unrelatedFlags: CGEventFlags = [.maskAlphaShift, .maskNumericPad, .maskNonCoalesced]
        do {
            var captured: [CGEvent] = []
            var rightShiftDown = true
            let mapper = ButtonMapper(eventSink: { captured.append($0) })
            mapper.modifierKeyState = { rightShiftDown && $0 == CGKeyCode(kVK_RightShift) }
            mapper.updateMapping([1: .keySequence(keys: [KeyStroke(key: "c", modifiers: ["cmd"])], description: nil)])
            mapper.handle(buttonIndex: 1)
            let release = captured.last!
            release.flags.formUnion(unrelatedFlags)
            rightShiftDown = false
            mapper.observePhysicalModifier(physicalEdge(code: CGKeyCode(kVK_RightShift), flags: []))
            mapper.observeSyntheticModifier(release)
            try check(release.flags == unrelatedFlags, "A queued acknowledgment clears released right Shift and its device bit while preserving unrelated flags")
        }
        do {
            var captured: [CGEvent] = []
            var commandDown = false
            let mapper = ButtonMapper(eventSink: { captured.append($0) })
            mapper.modifierKeyState = { commandDown && $0 == CGKeyCode(kVK_Command) }
            mapper.updateMapping([
                1: .keySequence(keys: [KeyStroke(key: "c", modifiers: ["cmd"])], description: nil),
                2: .keySequence(keys: [KeyStroke(key: "v", modifiers: ["ctrl"])], description: nil)
            ])
            mapper.handlePress(buttonIndex: 1)
            commandDown = true
            mapper.observePhysicalModifier(physicalEdge(code: CGKeyCode(kVK_Command), flags: commandHeld))
            mapper.handleRelease(buttonIndex: 1, inputFlags: commandHeld)
            let release = captured.last!
            release.flags.formUnion(unrelatedFlags)
            mapper.handlePress(buttonIndex: 2)
            commandDown = false
            mapper.observePhysicalModifier(physicalEdge(code: CGKeyCode(kVK_Command), flags: []))
            mapper.observeSyntheticModifier(release)
            try check(release.flags == unrelatedFlags.union(.maskControl), "Acknowledgment clears a physical Command pressed during an owned hold and released before delivery, preserving current owned Control and unrelated flags")
            mapper.releaseAll()
        }
        return count
    }
}
