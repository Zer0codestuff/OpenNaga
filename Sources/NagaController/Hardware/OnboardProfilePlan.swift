import Cocoa

struct OnboardProfilePlan {
    let name: String
    var functions: [UInt8: [UInt8]] = [:]
    var issues: [String] = []
    var isSupported: Bool { !functions.isEmpty && issues.isEmpty }

    // Grid IDs increase with the printed button numbers, not enumeration order.
    static let buttonIDs: [Int: UInt8] = Dictionary(uniqueKeysWithValues:
        (1...12).map { ($0, UInt8(0x40 + $0 - 1)) } +
        [(13, 0x0b), (14, 0x0c), (15, 0x34), (16, 0x35), (17, 3), (18, 1), (19, 2)])

    init(name: String, mapping: [Int: ActionType]) {
        self.name = name
        for index in mapping.keys.sorted() {
            guard let id = Self.buttonIDs[index], let action = mapping[index] else {
                issues.append("Button \(index): unknown hardware control."); continue
            }
            do { functions[id] = try Self.function(action) }
            catch { issues.append("Button \(index): \(error.localizedDescription)") }
        }
        if mapping.isEmpty { issues.append("The profile is empty.") }
    }

    private static func unsupported(_ text: String) -> RazerHardwareError { .invalidValue(text) }

    static func function(_ action: ActionType) throws -> [UInt8] {
        switch action {
        case .disabled: return Array(repeating: 0, count: 7)
        case .keySequence(let keys, _):
            guard keys.count == 1, let key = keys.first else { throw unsupported("key sequences need the app.") }
            return try keyboard(key)
        case .textSnippet(let text, _):
            guard text.count == 1, let key = KeyboardKeyCatalog.current().first(where: {
                [.letters, .numbers, .symbols].contains($0.group) && $0.key == text
            }) else { throw unsupported("the text cannot be typed as a single key in the current layout.") }
            return try keyboard(key.stroke())
        case .mouse(let mouse, _):
            switch mouse {
            case .dpiUp: return [6, 1, 1, 0, 0, 0, 0]
            case .dpiDown: return [6, 1, 2, 0, 0, 0, 0]
            case .leftClick: return [1, 1, 1, 0, 0, 0, 0]
            case .rightClick: return [1, 1, 2, 0, 0, 0, 0]
            case .middleClick: return [1, 1, 3, 0, 0, 0, 0]
            case .button4: return [1, 1, 4, 0, 0, 0, 0]
            case .button5: return [1, 1, 5, 0, 0, 0, 0]
            case .scrollUp: return [1, 1, 9, 0, 0, 0, 0]
            case .scrollDown: return [1, 1, 10, 0, 0, 0, 0]
            case .scrollLeft: return [0x0e, 3, 0x68, 0, 0x14, 0, 0]
            case .scrollRight: return [0x0e, 3, 0x69, 0, 0x14, 0, 0]
            case .browserBack, .browserForward:
                guard let stroke = KeyboardLayoutShortcut.browserStroke(for: mouse) else { throw unsupported("browser shortcut not available.") }
                return try keyboard(stroke)
            }
        case .audio(let action, _): return try system(SystemAction(audio: action))
        case .system(let action, _): return try system(action)
        case .application, .systemCommand, .macro, .profileSwitch:
            throw unsupported("this action needs OpenNaga running.")
        }
    }

    private static func system(_ action: SystemAction) throws -> [UInt8] {
        let media: [SystemAction: UInt8] = [.volumeUp: 0xe9, .volumeDown: 0xea, .mute: 0xe2,
                                           .playPause: 0xcd, .nextTrack: 0xb5, .previousTrack: 0xb6]
        if let usage = media[action] { return [0x0a, 2, 0, usage, 0, 0, 0] }
        if let shortcut = action.shortcut?.resolve(in: MacSystemShortcut.preferences()) {
            guard !shortcut.flags.contains(.maskSecondaryFn) else { throw unsupported("the Mac Fn key needs the app.") }
            let modifiers: [(CGEventFlags, String)] = [(.maskCommand, "cmd"), (.maskControl, "ctrl"), (.maskShift, "shift"), (.maskAlternate, "alt")]
            return try keyboard(KeyStroke(key: "", modifiers: modifiers.compactMap { shortcut.flags.contains($0.0) ? $0.1 : nil }, keyCode: shortcut.keyCode))
        }
        throw unsupported("this action cannot be stored in the mouse; use a keyboard shortcut.")
    }

    static func keyboard(_ key: KeyStroke) throws -> [UInt8] {
        guard let code = key.keyCode ?? KeyStroke.keyCode(for: key.key), let usage = keyboardUsages[code] else {
            throw unsupported("key has no supported USB HID equivalent.")
        }
        let bits: [String: UInt8] = ["ctrl": 1, "control": 1, "shift": 2, "alt": 4, "option": 4, "cmd": 8, "command": 8]
        var mask: UInt8 = 0
        for name in key.modifiers {
            guard let value = bits[name.lowercased()] else { throw unsupported("modifier \(name) is not supported by the mouse.") }
            mask |= value
        }
        return [2, 2, mask, usage, 0, 0, 0]
    }

    // USB HID keyboard usages paired with Apple's virtual key positions.
    static let keyboardUsages: [UInt16: UInt8] = {
        var result: [UInt16: UInt8] = [:]
        func add(_ codes: [UInt16], startingAt usage: UInt8) {
            for (offset, code) in codes.enumerated() { result[code] = usage + UInt8(offset) }
        }
        add([0,11,8,2,14,3,5,4,34,38,40,37,46,45,31,35,12,15,1,17,32,9,13,7,16,6], startingAt: 4)
        add([18,19,20,21,23,22,26,28,25,29], startingAt: 0x1e)
        add([36,53,51,48,49,27,24,33,30,42], startingAt: 0x28)
        add([41,39,50,43,47,44,57], startingAt: 0x33)
        add([122,120,99,118,96,97,98,100,101,109,103,111], startingAt: 0x3a)
        add([114,115,116,117,119,121,124,123,125,126], startingAt: 0x49)
        add([75,67,78,69,76,83,84,85,86,87,88,89,91,92,82,65], startingAt: 0x54)
        add([105,107,113,106,64,79,80,90], startingAt: 0x68)
        result[10] = 0x64
        result[81] = 0x67
        return result
    }()
}
