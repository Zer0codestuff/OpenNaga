import Cocoa
import Carbon.HIToolbox

final class ButtonMapper {
    static let shared = ButtonMapper()

    static let syntheticMarker: Int64 = 0x4e4147414354524c
    private var mapping: [Int: ActionType] = [:]
    private var hypershiftMapping: [Int: ActionType] = [:]
    // Controls assigned to Hypershift that are held right now.
    private var hypershiftTriggers: Set<Int> = []
    private enum Hold { case key(CGKeyCode, CGEventFlags), mouse(CGMouseButton) }
    private var activeHolds: [Int: Hold] = [:]
    private var pressed: Set<Int> = []
    private var generation = 0
    private let eventSink: ((CGEvent) -> Void)?
    private let workspaceActionSink: ((SystemAction) -> Void)?
    private let syntheticScrollScaler = ScrollEventScaler()
    var scrollSensitivityProvider: () -> ScrollSensitivity = { ConfigManager.shared.scrollSensitivity }
    var systemShortcutPreferences: () -> [String: Any] = MacSystemShortcut.preferences
    // Synthetic keys come from a real event source so posting them updates the
    // system modifier state; apps that track that state ignore orphan flags.
    private let keyboardSource = CGEventSource(stateID: .hidSystemState)
    private var syntheticModifiers: CGEventFlags = []
    private var physicalModifierStates: [CGKeyCode: Bool] = [:]
    // Modifiers the user holds on the input currently being performed.
    private var inputModifiers: CGEventFlags = []
    private var pendingSyntheticReleases: [UInt64: CGEventTimestamp] = [:]
    var modifierKeyState: (CGKeyCode) -> Bool = { CGEventSource.keyState(.hidSystemState, key: $0) }

    private struct ModifierKey {
        let flag: CGEventFlags
        let code: CGKeyCode
        let otherCode: CGKeyCode?
        let deviceMask: UInt64
        let otherDeviceMask: UInt64
    }

    // Device masks distinguish a physical modifier's left and right keys.
    private static let modifierKeys: [ModifierKey] = [
        ModifierKey(flag: .maskCommand, code: CGKeyCode(kVK_Command), otherCode: CGKeyCode(kVK_RightCommand), deviceMask: 0x08, otherDeviceMask: 0x10),
        ModifierKey(flag: .maskControl, code: CGKeyCode(kVK_Control), otherCode: CGKeyCode(kVK_RightControl), deviceMask: 0x01, otherDeviceMask: 0x2000),
        ModifierKey(flag: .maskShift, code: CGKeyCode(kVK_Shift), otherCode: CGKeyCode(kVK_RightShift), deviceMask: 0x02, otherDeviceMask: 0x04),
        ModifierKey(flag: .maskAlternate, code: CGKeyCode(kVK_Option), otherCode: CGKeyCode(kVK_RightOption), deviceMask: 0x20, otherDeviceMask: 0x40),
        ModifierKey(flag: .maskSecondaryFn, code: CGKeyCode(kVK_Function), otherCode: nil, deviceMask: CGEventFlags.maskSecondaryFn.rawValue, otherDeviceMask: 0)
    ]

    init(eventSink: ((CGEvent) -> Void)? = nil, workspaceActionSink: ((SystemAction) -> Void)? = nil) {
        self.eventSink = eventSink
        self.workspaceActionSink = workspaceActionSink
    }

    /// True while at least one Hypershift control is held.
    var isHypershiftActive: Bool { !hypershiftTriggers.isEmpty }

    /// The action a press performs now. While Hypershift is held, a control
    /// with a Hypershift assignment uses it and any other control keeps its
    /// standard action. A Hypershift control always stays a Hypershift control.
    func action(forButton buttonIndex: Int) -> ActionType? {
        if case .hypershift = mapping[buttonIndex] { return mapping[buttonIndex] }
        if isHypershiftActive, let shifted = hypershiftMapping[buttonIndex] { return shifted }
        return mapping[buttonIndex]
    }

    func hasMapping(buttonIndex: Int) -> Bool {
        guard let action = self.action(forButton: buttonIndex) else { return false }
        if case .mouse(let mouse, _) = action, mouse == .dpiUp || mouse == .dpiDown { return false }
        return true
    }

    func updateMapping(_ newMapping: [Int: ActionType], hypershift: [Int: ActionType] = [:]) {
        releaseAll()
        EventTapManager.shared.resetInputState()
        mapping = newMapping
        hypershiftMapping = hypershift.filter { if case .hypershift = $0.value { return false }; return true }
    }

    /// Replaces the assignments for the next press without touching what is
    /// held now. Outputs release by button index, so a key started under the
    /// old mapping still ends with its own key-up. Used for automatic profile
    /// changes, which can land while a mapped button is physically down.
    func swapMapping(_ newMapping: [Int: ActionType], hypershift: [Int: ActionType] = [:]) {
        mapping = newMapping
        hypershiftMapping = hypershift.filter { if case .hypershift = $0.value { return false }; return true }
    }

    func releaseAll() {
        syntheticScrollScaler.reset()
        generation += 1
        for button in Array(activeHolds.keys) { handleRelease(buttonIndex: button) }
        pressed.removeAll()
        hypershiftTriggers.removeAll()
        if activeHolds.isEmpty { syntheticModifiers = [] }
    }

    func handle(buttonIndex: Int, inputFlags: CGEventFlags = []) {
        handlePress(buttonIndex: buttonIndex, inputFlags: inputFlags)
        handleRelease(buttonIndex: buttonIndex, inputFlags: inputFlags)
    }

    /// `inputFlags` are the flags of the event that triggered the press. The
    /// keyboard modifiers in them are the ones the user is holding right now,
    /// so they are merged into the keys this press sends (Command held with a
    /// side button mapped to C sends Command-C).
    func handlePress(buttonIndex: Int, inputFlags: CGEventFlags = []) {
        guard let action = self.action(forButton: buttonIndex), pressed.insert(buttonIndex).inserted else { return }
        // The layer is chosen at press time; holds release by button index,
        // so an output started in one layer ends correctly in the other.
        if case .hypershift = action { hypershiftTriggers.insert(buttonIndex); return }
        let held = userModifiers(in: inputFlags)
        inputModifiers = held
        defer { inputModifiers = [] }
        switch action {
        case .keySequence(let keys, _) where keys.count == 1:
            if let code = effectiveKeyCode(for: keys[0]) {
                let flags = modifierFlags(from: keys[0].modifiers)
                pressModifiers(flags)
                let alreadyHeld = activeHolds.values.contains { if case .key(let heldCode, _) = $0 { return heldCode == code }; return false }
                activeHolds[buttonIndex] = .key(code, flags)
                if !alreadyHeld { postKey(code, flags: flags.union(held), down: true) }
            }
        case .mouse(let action, _):
            if let navigation = action.browserEquivalent, frontmostIsBrowser() {
                performMouse(navigation)
            } else if let button = mouseButton(action) {
                activeHolds[buttonIndex] = .mouse(button)
                postMouse(button, down: true)
            } else { performMouse(action) }
        default: perform(action: action)
        }
    }

    func handleRelease(buttonIndex: Int, inputFlags: CGEventFlags? = nil) {
        pressed.remove(buttonIndex)
        hypershiftTriggers.remove(buttonIndex)
        guard let hold = activeHolds.removeValue(forKey: buttonIndex) else { return }
        // Do not release an output another physical input still holds.
        switch hold {
        case .key(let code, let flags):
            let held = inputFlags.map { userModifiers(in: $0) } ?? []
            inputModifiers = held
            defer { inputModifiers = [] }
            if !activeHolds.values.contains(where: { if case .key(let c, _) = $0 { return c == code }; return false }) {
                postKey(code, flags: flags.union(held), down: false)
            }
            releaseModifiers(flags)
        case .mouse(let button):
            if !activeHolds.values.contains(where: { if case .mouse(let b) = $0 { return b == button }; return false }) {
                postMouse(button, down: false)
            }
        }
    }

    func dragEvent(for event: CGEvent) -> CGEvent? {
        guard let button = activeHolds.values.compactMap({ hold -> CGMouseButton? in
            if case .mouse(let button) = hold { return button }; return nil
        }).sorted(by: { $0.rawValue < $1.rawValue }).first else { return nil }
        let type: CGEventType = button == .left ? .leftMouseDragged : button == .right ? .rightMouseDragged : .otherMouseDragged
        let drag = CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: event.location, mouseButton: button)
        drag?.setIntegerValueField(.eventSourceUserData, value: Self.syntheticMarker)
        drag?.setIntegerValueField(.mouseEventDeltaX, value: event.getIntegerValueField(.mouseEventDeltaX))
        drag?.setIntegerValueField(.mouseEventDeltaY, value: event.getIntegerValueField(.mouseEventDeltaY))
        return drag
    }

    private func perform(action: ActionType) {
        switch action {
        case .audio(let action, _): performAudio(action)
        case .system(let action, _): performSystem(action)
        case .disabled: break
        case .mouse(let action, _): performMouse(action)
        case .keySequence(let keys, _): keys.forEach(sendKeyStroke)
        case .application(let path, _): DispatchQueue.main.async { NSWorkspace.shared.open(URL(fileURLWithPath: path)) }
        case .systemCommand(let command, _): DispatchQueue.global(qos: .userInitiated).async { self.runShell(command) }
        case .textSnippet(let text, _): runMacro([MacroStep(type: "text", text: text)])
        case .macro(let steps, _): runMacro(steps)
        case .profileSwitch(let profile, _): DispatchQueue.main.async { ConfigManager.shared.setCurrentProfile(profile) }
        case .hypershift: break // Handled as a held layer in handlePress.
        }
    }

    func performAudio(_ action: AudioAction) {
        performSystem(SystemAction(audio: action))
    }

    @discardableResult
    func performSystem(_ action: SystemAction) -> String? {
        if action.mediaKey != nil {
            for down in [true, false] { post(action.mediaEvent(down: down)) }
        } else if let shortcut = action.shortcut {
            guard let stroke = shortcut.resolve(in: systemShortcutPreferences()) else {
                return action.shortcutSetupMessage
            }
            // Modifier edges keep the system modifier state in sync, which
            // Command-Tab needs to close the app switcher and shortcuts in
            // general need to register their modifiers.
            sendKeyStroke(stroke.keyCode, flags: stroke.flags)
        } else if let workspaceActionSink {
            workspaceActionSink(action)
        } else {
            DispatchQueue.main.async {
                if let bundleID = action.applicationBundleIdentifier,
                   let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleID) {
                    NSWorkspace.shared.openApplication(at: url, configuration: .init())
                } else if action == .hideApplication {
                    NSWorkspace.shared.frontmostApplication?.hide()
                }
            }
        }
        return nil
    }

    private func post(_ event: CGEvent?) {
        guard let event else { return }
        event.setIntegerValueField(.eventSourceUserData, value: Self.syntheticMarker)
        if let eventSink { eventSink(event) }
        else { event.post(tap: .cghidEventTap) }
    }

    private func postKey(_ code: CGKeyCode, flags: CGEventFlags, down: Bool, autorepeat: Bool = false) {
        let event = CGEvent(keyboardEventSource: keyboardSource, virtualKey: code, keyDown: down)
        let intrinsicFlags = event?.flags.intersection(.maskNumericPad) ?? []
        event?.flags = flags.union(syntheticModifiers).union(physicalModifierFlags()).union(intrinsicFlags)
        if autorepeat { event?.setIntegerValueField(.keyboardEventAutorepeat, value: 1) }
        post(event)
    }

    private func postModifier(_ flag: CGEventFlags, _ code: CGKeyCode, down: Bool) {
        syntheticModifiers = down ? syntheticModifiers.union(flag) : syntheticModifiers.subtracting(flag)
        let event = CGEvent(keyboardEventSource: keyboardSource, virtualKey: code, keyDown: down)
        event?.type = .flagsChanged
        event?.flags = syntheticModifiers.union(physicalModifierFlags()).union(inputModifiers)
        if down {
            pendingSyntheticReleases[flag.rawValue] = nil
        } else if let event {
            pendingSyntheticReleases[flag.rawValue] = event.timestamp
        }
        post(event)
    }

    /// Acknowledge the release when it reaches the downstream event tap. A
    /// real modifier pressed in the meantime must survive the queued release.
    func observeSyntheticModifier(_ event: CGEvent) {
        guard event.type == .flagsChanged,
              event.getIntegerValueField(.eventSourceUserData) == Self.syntheticMarker else { return }
        let code = CGKeyCode(event.getIntegerValueField(.keyboardEventKeycode))
        guard let modifier = Self.modifierKeys.first(where: { $0.code == code }),
              pendingSyntheticReleases[modifier.flag.rawValue] == event.timestamp else { return }
        pendingSyntheticReleases[modifier.flag.rawValue] = nil
        let trackedMask = Self.modifierKeys.reduce(CGEventFlags()) { flags, key in
            flags.union(key.flag).union(CGEventFlags(rawValue: key.deviceMask | key.otherDeviceMask))
        }
        event.flags = event.flags.subtracting(trackedMask).union(syntheticModifiers).union(physicalModifierFlags())
    }

    /// A recreated tap must sample keys that changed while it was stopped.
    func resetPhysicalModifierObservation() {
        physicalModifierStates.removeAll()
        pendingSyntheticReleases.removeAll()
    }

    /// Observe physical edges without consuming them. HID state alone cannot
    /// distinguish our own modifier press from a later physical key press.
    func observePhysicalModifier(_ event: CGEvent) {
        guard event.type == .flagsChanged,
              event.getIntegerValueField(.eventSourceUserData) != Self.syntheticMarker else { return }
        let code = CGKeyCode(event.getIntegerValueField(.keyboardEventKeycode))
        for modifier in Self.modifierKeys {
            let mask: UInt64
            if code == modifier.code { mask = modifier.deviceMask }
            else if code == modifier.otherCode { mask = modifier.otherDeviceMask }
            else { continue }
            let deviceBits = event.flags.rawValue & (modifier.deviceMask | modifier.otherDeviceMask)
            if !event.flags.contains(modifier.flag) {
                // A device's clear can coexist with a modifier on another
                // keyboard. An owned synthetic key must not be borrowed.
                physicalModifierStates[code] = !syntheticModifiers.contains(modifier.flag) && modifierKeyState(code)
            } else if deviceBits != 0 {
                physicalModifierStates[code] = event.flags.rawValue & mask != 0
            } else {
                physicalModifierStates[code] = modifierKeyState(code)
            }
            let required = activeHolds.values.reduce(CGEventFlags()) { flags, hold in
                if case .key(_, let needed) = hold { return flags.union(needed) }
                return flags
            }
            pressModifiers(required)
            return
        }
    }

    /// Held keyboard modifiers missing from a key event the mouse sent
    /// natively. Modifiers owned by an active mapping are not reported.
    func heldModifiersMissing(from flags: CGEventFlags) -> CGEventFlags {
        let physical = physicalModifierFlags()
        var missing = CGEventFlags()
        for modifier in Self.modifierKeys where physical.contains(modifier.flag)
            && !flags.contains(modifier.flag) && !syntheticModifiers.contains(modifier.flag) {
            missing.formUnion(modifier.flag)
            missing.formUnion(CGEventFlags(rawValue: physical.rawValue & (modifier.deviceMask | modifier.otherDeviceMask)))
        }
        return missing
    }

    /// Re-sends a native mouse key through the synthetic keyboard source with
    /// the modifiers currently held, as a remapped key would be delivered.
    func postNativeKey(_ code: CGKeyCode, flags: CGEventFlags, down: Bool, autorepeat: Bool = false) {
        postKey(code, flags: flags, down: down, autorepeat: autorepeat)
    }

    var observedModifierFlags: CGEventFlags { physicalModifierFlags() }

    private func physicalModifierFlags() -> CGEventFlags {
        var flags = CGEventFlags()
        for modifier in Self.modifierKeys {
            if physicalModifierStates[modifier.code] ?? (!syntheticModifiers.contains(modifier.flag) && modifierKeyState(modifier.code)) {
                flags.formUnion(modifier.flag)
                flags.formUnion(CGEventFlags(rawValue: modifier.deviceMask))
            }
            if let code = modifier.otherCode,
               physicalModifierStates[code] ?? modifierKeyState(code) {
                flags.formUnion(modifier.flag)
                flags.formUnion(CGEventFlags(rawValue: modifier.otherDeviceMask))
            }
        }
        return flags
    }

    /// Exclude owned modifiers and flags from a queued synthetic release,
    /// while accepting independently observed physical keys.
    private func userModifiers(in inputFlags: CGEventFlags) -> CGEventFlags {
        var held = CGEventFlags()
        let physicalFlags = physicalModifierFlags()
        for modifier in Self.modifierKeys where inputFlags.contains(modifier.flag) {
            if !physicalFlags.contains(modifier.flag) {
                guard !syntheticModifiers.contains(modifier.flag), pendingSyntheticReleases[modifier.flag.rawValue] == nil else { continue }
            }
            held.formUnion(modifier.flag)
            held.formUnion(CGEventFlags(rawValue: inputFlags.rawValue & (modifier.deviceMask | modifier.otherDeviceMask)))
        }
        return held
    }

    /// Presses the modifiers a stroke needs. A modifier already down,
    /// physically or synthetically, is left alone so a release here can never
    /// drop the user's own modifiers.
    private func pressModifiers(_ flags: CGEventFlags) {
        for modifier in Self.modifierKeys where flags.contains(modifier.flag) {
            guard !syntheticModifiers.contains(modifier.flag), !physicalModifierFlags().contains(modifier.flag),
                  !inputModifiers.contains(modifier.flag) else { continue }
            // Snapshot before posting, while HID state still reflects only the
            // physical key. Later non-synthetic edges update this observation.
            if physicalModifierStates[modifier.code] == nil { physicalModifierStates[modifier.code] = modifierKeyState(modifier.code) }
            if let other = modifier.otherCode, physicalModifierStates[other] == nil { physicalModifierStates[other] = modifierKeyState(other) }
            postModifier(modifier.flag, modifier.code, down: true)
        }
    }

    /// Releases the modifiers a stroke required, but only the ones this event
    /// stream pressed and no other active hold still needs.
    private func releaseModifiers(_ flags: CGEventFlags) {
        let stillNeeded = activeHolds.values.reduce(CGEventFlags()) { partial, hold in
            if case .key(_, let required) = hold { return partial.union(required) }
            return partial
        }
        let releasable = flags.intersection(syntheticModifiers).subtracting(stillNeeded)
        for modifier in Self.modifierKeys.reversed() where releasable.contains(modifier.flag) {
            postModifier(modifier.flag, modifier.code, down: false)
        }
    }

    private func sendKeyStroke(_ stroke: KeyStroke) {
        guard let code = effectiveKeyCode(for: stroke) else { return }
        let flags = modifierFlags(from: stroke.modifiers)
        sendKeyStroke(code, flags: flags)
    }

    private func sendKeyStroke(_ code: CGKeyCode, flags: CGEventFlags) {
        pressModifiers(flags)
        let alreadyHeld = activeHolds.values.contains { if case .key(let heldCode, _) = $0 { return heldCode == code }; return false }
        let keyFlags = flags.union(inputModifiers)
        postKey(code, flags: keyFlags, down: true, autorepeat: alreadyHeld)
        if !alreadyHeld { postKey(code, flags: keyFlags, down: false) }
        releaseModifiers(flags)
    }

    private func mouseButton(_ action: MouseAction) -> CGMouseButton? {
        switch action {
        case .leftClick: return .left
        case .rightClick: return .right
        case .middleClick: return .center
        case .button4: return CGMouseButton(rawValue: 3)
        case .button5: return CGMouseButton(rawValue: 4)
        default: return nil
        }
    }

    private func postMouse(_ button: CGMouseButton, down: Bool) {
        let type: CGEventType = button == .left ? (down ? .leftMouseDown : .leftMouseUp) : button == .right ? (down ? .rightMouseDown : .rightMouseUp) : (down ? .otherMouseDown : .otherMouseUp)
        let event = CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: CGEvent(source: nil)?.location ?? .zero, mouseButton: button)
        event?.setIntegerValueField(.mouseEventClickState, value: 1)
        post(event)
    }

    var frontmostBundleIdentifier: () -> String? = { NSWorkspace.shared.frontmostApplication?.bundleIdentifier }

    private func frontmostIsBrowser() -> Bool {
        MouseAction.isBrowser(bundleIdentifier: frontmostBundleIdentifier())
    }

    private func performMouse(_ action: MouseAction) {
        if let navigation = action.browserEquivalent, frontmostIsBrowser() { performMouse(navigation); return }
        if let button = mouseButton(action) { postMouse(button, down: true); postMouse(button, down: false); return }
        switch action {
        case .browserBack, .browserForward:
            DispatchQueue.main.async {
                if let stroke = KeyboardLayoutShortcut.browserStroke(for: action) { self.sendKeyStroke(stroke) }
            }
        case .scrollUp, .scrollDown, .scrollLeft, .scrollRight:
            let vertical: Int32 = action == .scrollUp ? 3 : action == .scrollDown ? -3 : 0
            let horizontal: Int32 = action == .scrollLeft ? 3 : action == .scrollRight ? -3 : 0
            guard let event = CGEvent(scrollWheelEvent2Source: nil, units: .line, wheelCount: 2,
                wheel1: vertical, wheel2: horizontal, wheel3: 0) else { return }
            syntheticScrollScaler.apply(to: event, sensitivity: scrollSensitivityProvider())
            if ScrollEventScaler.hasMotion(event, axis: .vertical) || ScrollEventScaler.hasMotion(event, axis: .horizontal) {
                post(event)
            }
        default: break
        }
    }

    private func effectiveKeyCode(for stroke: KeyStroke) -> CGKeyCode? {
        if let code = stroke.keyCode {
            return CGKeyCode(code)
        }
        return KeyStroke.keyCode(for: stroke.key).map { CGKeyCode($0) }
    }

    private func modifierFlags(from modifiers: [String]) -> CGEventFlags {
        var flags: CGEventFlags = []
        for m in modifiers.map({ $0.lowercased() }) {
            switch m {
            case "cmd", "command": flags.insert(.maskCommand)
            case "shift": flags.insert(.maskShift)
            case "alt", "option": flags.insert(.maskAlternate)
            case "ctrl", "control": flags.insert(.maskControl)
            case "fn": flags.insert(.maskSecondaryFn)
            default: break
            }
        }
        return flags
    }

    private func runShell(_ command: String) {
        let task = Process()
        task.launchPath = "/bin/zsh"
        task.arguments = ["-lc", command]
        do {
            try task.run()
        } catch {
            NSLog("[Mapping] Failed to run command: \(command), error: \(error.localizedDescription)")
        }
    }

    func runMacro(_ steps: [MacroStep]) {
        let token = generation
        func next(_ index: Int) {
            guard token == self.generation, index < steps.count else { return }
            let step = steps[index]
            if step.type == "delay" {
                let seconds = Double(max(0, min(step.delayMs ?? 0, 600_000))) / 1000
                DispatchQueue.main.asyncAfter(deadline: .now() + seconds) { next(index + 1) }
                return
            }
            if step.type == "key", let key = step.keyStroke { self.sendKeyStroke(key) }
            if step.type == "text", let text = step.text { self.typeText(text) }
            DispatchQueue.main.async { next(index + 1) }
        }
        DispatchQueue.main.async { next(0) }
    }

    private func typeText(_ text: String) {
        // Unicode events preserve the clipboard and non-Latin text.
        for character in text {
            let units = Array(String(character).utf16)
            for down in [true, false] {
                let event = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: down)
                units.withUnsafeBufferPointer { buffer in
                    event?.keyboardSetUnicodeString(stringLength: buffer.count, unicodeString: buffer.baseAddress)
                }
                post(event)
            }
        }
    }
}
