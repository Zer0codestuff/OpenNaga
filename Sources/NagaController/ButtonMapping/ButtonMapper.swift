import Cocoa
import Carbon.HIToolbox

final class ButtonMapper {
    static let shared = ButtonMapper()

    static let syntheticMarker: Int64 = 0x4e4147414354524c
    private var mapping: [Int: ActionType] = [:]
    private enum Hold { case key(CGKeyCode, CGEventFlags), mouse(CGMouseButton) }
    private var activeHolds: [Int: Hold] = [:]
    private var pressed: Set<Int> = []
    private var generation = 0
    private let eventSink: ((CGEvent) -> Void)?
    private let workspaceActionSink: ((SystemAction) -> Void)?
    var systemShortcutPreferences: () -> [String: Any] = MacSystemShortcut.preferences

    init(eventSink: ((CGEvent) -> Void)? = nil, workspaceActionSink: ((SystemAction) -> Void)? = nil) {
        self.eventSink = eventSink
        self.workspaceActionSink = workspaceActionSink
    }

    func hasMapping(buttonIndex: Int) -> Bool {
        if case .mouse(let action, _) = mapping[buttonIndex], action == .dpiUp || action == .dpiDown { return false }
        return mapping[buttonIndex] != nil
    }

    func updateMapping(_ newMapping: [Int: ActionType]) {
        releaseAll()
        EventTapManager.shared.resetInputState()
        mapping = newMapping
    }

    func releaseAll() {
        generation += 1
        for button in Array(activeHolds.keys) { handleRelease(buttonIndex: button) }
        pressed.removeAll()
    }

    func handle(buttonIndex: Int) {
        handlePress(buttonIndex: buttonIndex)
        handleRelease(buttonIndex: buttonIndex)
    }

    func handlePress(buttonIndex: Int) {
        guard let action = mapping[buttonIndex], pressed.insert(buttonIndex).inserted else { return }
        switch action {
        case .keySequence(let keys, _) where keys.count == 1:
            if let code = effectiveKeyCode(for: keys[0]) {
                let flags = modifierFlags(from: keys[0].modifiers)
                activeHolds[buttonIndex] = .key(code, flags)
                postKey(code, flags: flags, down: true)
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

    func handleRelease(buttonIndex: Int) {
        pressed.remove(buttonIndex)
        guard let hold = activeHolds.removeValue(forKey: buttonIndex) else { return }
        // Do not release an output another physical input still holds.
        switch hold {
        case .key(let code, let flags):
            if !activeHolds.values.contains(where: { if case .key(let c, _) = $0 { return c == code }; return false }) {
                postKey(code, flags: flags, down: false)
            }
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
            postKey(stroke.keyCode, flags: stroke.flags, down: true)
            postKey(stroke.keyCode, flags: stroke.flags, down: false)
            if action == .switchApplication {
                // Command-Tab must release Command or the app switcher remains open.
                let release = CGEvent(keyboardEventSource: nil, virtualKey: CGKeyCode(kVK_Command), keyDown: false)
                release?.type = .flagsChanged
                release?.flags = []
                post(release)
            }
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

    private func postKey(_ code: CGKeyCode, flags: CGEventFlags, down: Bool) {
        let event = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: down)
        event?.flags = flags
        post(event)
    }

    private func sendKeyStroke(_ stroke: KeyStroke) {
        guard let code = effectiveKeyCode(for: stroke) else { return }
        let flags = modifierFlags(from: stroke.modifiers)
        postKey(code, flags: flags, down: true)
        postKey(code, flags: flags, down: false)
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
            post(CGEvent(scrollWheelEvent2Source: nil, units: .line, wheelCount: 2, wheel1: vertical, wheel2: horizontal, wheel3: 0))
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
