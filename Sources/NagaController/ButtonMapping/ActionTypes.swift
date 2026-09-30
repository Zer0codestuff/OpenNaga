import Foundation
import Carbon.HIToolbox

enum ActionType: Equatable {
    case mouse(action: MouseAction, description: String?)
    case audio(action: AudioAction, description: String?)
    case system(action: SystemAction, description: String?)
    case disabled
    case keySequence(keys: [KeyStroke], description: String?)
    case application(path: String, description: String?)
    case systemCommand(command: String, description: String?)
    case textSnippet(text: String, description: String?)
    case macro(steps: [MacroStep], description: String?)
    case profileSwitch(profile: String, description: String?)
}

enum MouseAction: String, Codable, CaseIterable {
    case browserBack, browserForward, leftClick, rightClick, middleClick, button4, button5
    case scrollUp, scrollDown, scrollLeft, scrollRight, dpiUp, dpiDown

    var title: String {
        switch self {
        case .browserBack: return "Browser back"
        case .browserForward: return "Browser forward"
        case .leftClick: return "Left click"
        case .rightClick: return "Right click"
        case .middleClick: return "Middle click"
        case .button4: return "Mouse button 4"
        case .button5: return "Mouse button 5"
        case .scrollUp: return "Scroll up"
        case .scrollDown: return "Scroll down"
        case .scrollLeft: return "Scroll left"
        case .scrollRight: return "Scroll right"
        case .dpiUp: return "DPI up"
        case .dpiDown: return "DPI down"
        }
    }

    /// macOS browsers, except Firefox, ignore mouse buttons 4/5 for navigation.
    /// A real click would silently do nothing, so browsers get the shortcut.
    var browserEquivalent: MouseAction? {
        switch self {
        case .button4: return .browserBack
        case .button5: return .browserForward
        default: return nil
        }
    }

    static let browserBundlePrefixes = [
        "com.apple.Safari", "com.google.Chrome", "org.chromium.Chromium", "org.mozilla.",
        "com.microsoft.edgemac", "com.brave.Browser", "company.thebrowser.Browser",
        "com.vivaldi.Vivaldi", "com.operasoftware.", "com.kagi.kagimacOS",
        "app.zen-browser.zen", "com.duckduckgo.macos.browser", "com.sigmaos.sigmaos"
    ]

    static func isBrowser(bundleIdentifier: String?) -> Bool {
        guard let bundleIdentifier else { return false }
        return browserBundlePrefixes.contains { bundleIdentifier.hasPrefix($0) }
    }
}

extension ActionType {
    var displayName: String {
        switch self {
        case .audio(let action, let description): return description ?? action.title
        case .system(let action, let description): return description ?? action.title
        case .disabled: return "Disabled"
        case .mouse(let action, let description): return description ?? action.title
        case .keySequence(let keys, let description): return description ?? keys.map { $0.formattedShortcut() }.joined(separator: ", ")
        case .application(let path, let description): return description ?? URL(fileURLWithPath: path).lastPathComponent
        case .systemCommand(_, let description): return description ?? "Shell command"
        case .textSnippet(_, let description): return description ?? "Text"
        case .macro(_, let description): return description ?? "Macro"
        case .profileSwitch(let profile, let description): return description ?? "Profile: \(profile)"
        }
    }
}

struct KeyStroke: Equatable, Codable {
    var key: String // canonical identifier (e.g., "c", "delete")
    var modifiers: [String] // e.g., ["cmd", "shift"]
    var keyCode: UInt16? = nil // hardware key code when known
}

extension KeyStroke {
    var displayLabel: String {
        if let code = keyCode {
            return KeyStroke.displayName(for: code, fallback: key)
        }
        return key.count == 1 ? key.uppercased() : key.capitalized
    }

    func formattedShortcut() -> String {
        let symbols = modifiers.map { KeyStroke.modifierSymbol(for: $0) }.joined()
        return symbols + displayLabel
    }

    static func canonicalKeyString(for keyCode: UInt16?, characters: String?) -> String {
        if let code = keyCode, let primary = primaryKeyNames[code] {
            return primary
        }
        if let chars = characters, !chars.isEmpty {
            return normalizeIdentifier(chars)
        }
        return ""
    }

    static func displayName(for keyCode: UInt16, fallback: String) -> String {
        if let special = specialKeyNames[keyCode] {
            return special
        }
        if fallback.count == 1 {
            return fallback.uppercased()
        }
        return fallback.split(separator: " ").map { $0.capitalized }.joined(separator: " ")
    }

    static func keyCode(for key: String) -> UInt16? {
        canonicalKeyCodes[normalizeIdentifier(key)]
    }

    static func fromCharacter(_ scalar: UnicodeScalar) -> KeyStroke? {
        switch scalar {
        case "\n":
            return KeyStroke(key: "return", modifiers: [], keyCode: UInt16(kVK_Return))
        case "\r":
            return KeyStroke(key: "return", modifiers: [], keyCode: UInt16(kVK_Return))
        case "\t":
            return KeyStroke(key: "tab", modifiers: [], keyCode: UInt16(kVK_Tab))
        case " ":
            return KeyStroke(key: "space", modifiers: [], keyCode: UInt16(kVK_Space))
        default:
            break
        }

        let char = Character(scalar)

        if char.isLetter {
            let lower = String(char).lowercased()
            guard let code = keyCode(for: lower) else { return nil }
            var mods: [String] = []
            if char.isUppercase { mods.append("shift") }
            let canonical = primaryKeyNames[code] ?? lower
            return KeyStroke(key: canonical, modifiers: mods, keyCode: code)
        }

        if let mapping = shiftedCharacterMap[char] {
            guard let code = keyCode(for: mapping.key) else { return nil }
            return KeyStroke(key: mapping.key, modifiers: mapping.modifiers, keyCode: code)
        }

        let string = String(char)
        if let code = keyCode(for: string) {
            let canonical = primaryKeyNames[code] ?? normalizeIdentifier(string)
            return KeyStroke(key: canonical, modifiers: [], keyCode: code)
        }

        return nil
    }

    private static func modifierSymbol(for modifier: String) -> String {
        modifierSymbolMap[modifier.lowercased()] ?? ""
    }

    private static func normalizeIdentifier(_ identifier: String) -> String {
        identifier.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
    }

    private static let canonicalKeyCodes: [String: UInt16] = {
        var map: [String: UInt16] = [:]

        func add(_ names: [String], code: Int) {
            for name in names {
                map[name] = UInt16(code)
            }
        }

        let letters = [
            ("a", kVK_ANSI_A), ("b", kVK_ANSI_B), ("c", kVK_ANSI_C), ("d", kVK_ANSI_D),
            ("e", kVK_ANSI_E), ("f", kVK_ANSI_F), ("g", kVK_ANSI_G), ("h", kVK_ANSI_H),
            ("i", kVK_ANSI_I), ("j", kVK_ANSI_J), ("k", kVK_ANSI_K), ("l", kVK_ANSI_L),
            ("m", kVK_ANSI_M), ("n", kVK_ANSI_N), ("o", kVK_ANSI_O), ("p", kVK_ANSI_P),
            ("q", kVK_ANSI_Q), ("r", kVK_ANSI_R), ("s", kVK_ANSI_S), ("t", kVK_ANSI_T),
            ("u", kVK_ANSI_U), ("v", kVK_ANSI_V), ("w", kVK_ANSI_W), ("x", kVK_ANSI_X),
            ("y", kVK_ANSI_Y), ("z", kVK_ANSI_Z)
        ]
        for (name, code) in letters { add([name], code: code) }

        let digits = [
            ("1", kVK_ANSI_1), ("2", kVK_ANSI_2), ("3", kVK_ANSI_3), ("4", kVK_ANSI_4),
            ("5", kVK_ANSI_5), ("6", kVK_ANSI_6), ("7", kVK_ANSI_7), ("8", kVK_ANSI_8),
            ("9", kVK_ANSI_9), ("0", kVK_ANSI_0)
        ]
        for (name, code) in digits { add([name], code: code) }

        add(["minus", "-"], code: kVK_ANSI_Minus)
        add(["equals", "=", "equal"], code: kVK_ANSI_Equal)
        add(["left bracket", "["], code: kVK_ANSI_LeftBracket)
        add(["right bracket", "]"], code: kVK_ANSI_RightBracket)
        add(["backslash", "\\"], code: kVK_ANSI_Backslash)
        add(["semicolon", ";"], code: kVK_ANSI_Semicolon)
        add(["quote", "'", "apostrophe"], code: kVK_ANSI_Quote)
        add(["comma", ","], code: kVK_ANSI_Comma)
        add(["period", "."], code: kVK_ANSI_Period)
        add(["slash", "/"], code: kVK_ANSI_Slash)
        add(["grave", "`", "tilde"], code: kVK_ANSI_Grave)

        add(["space"], code: kVK_Space)
        add(["return"], code: kVK_Return)
        add(["enter", "keypad enter"], code: kVK_ANSI_KeypadEnter)
        add(["tab"], code: kVK_Tab)
        add(["escape", "esc"], code: kVK_Escape)
        add(["delete", "backspace"], code: kVK_Delete)
        add(["forward delete", "fn delete", "del"], code: kVK_ForwardDelete)
        add(["caps lock", "capslock"], code: kVK_CapsLock)
        add(["help"], code: kVK_Help)
        add(["home"], code: kVK_Home)
        add(["end"], code: kVK_End)
        add(["page up"], code: kVK_PageUp)
        add(["page down"], code: kVK_PageDown)
        add(["left arrow", "left"], code: kVK_LeftArrow)
        add(["right arrow", "right"], code: kVK_RightArrow)
        add(["up arrow", "up"], code: kVK_UpArrow)
        add(["down arrow", "down"], code: kVK_DownArrow)

        add(["f1"], code: kVK_F1)
        add(["f2"], code: kVK_F2)
        add(["f3"], code: kVK_F3)
        add(["f4"], code: kVK_F4)
        add(["f5"], code: kVK_F5)
        add(["f6"], code: kVK_F6)
        add(["f7"], code: kVK_F7)
        add(["f8"], code: kVK_F8)
        add(["f9"], code: kVK_F9)
        add(["f10"], code: kVK_F10)
        add(["f11"], code: kVK_F11)
        add(["f12"], code: kVK_F12)
        add(["f13"], code: kVK_F13)
        add(["f14"], code: kVK_F14)
        add(["f15"], code: kVK_F15)
        add(["f16"], code: kVK_F16)
        add(["f17"], code: kVK_F17)
        add(["f18"], code: kVK_F18)
        add(["f19"], code: kVK_F19)
        add(["f20"], code: kVK_F20)

        add(["kp0", "keypad 0"], code: kVK_ANSI_Keypad0)
        add(["kp1", "keypad 1"], code: kVK_ANSI_Keypad1)
        add(["kp2", "keypad 2"], code: kVK_ANSI_Keypad2)
        add(["kp3", "keypad 3"], code: kVK_ANSI_Keypad3)
        add(["kp4", "keypad 4"], code: kVK_ANSI_Keypad4)
        add(["kp5", "keypad 5"], code: kVK_ANSI_Keypad5)
        add(["kp6", "keypad 6"], code: kVK_ANSI_Keypad6)
        add(["kp7", "keypad 7"], code: kVK_ANSI_Keypad7)
        add(["kp8", "keypad 8"], code: kVK_ANSI_Keypad8)
        add(["kp9", "keypad 9"], code: kVK_ANSI_Keypad9)
        add(["kp."], code: kVK_ANSI_KeypadDecimal)
        add(["kp*"], code: kVK_ANSI_KeypadMultiply)
        add(["kp+"], code: kVK_ANSI_KeypadPlus)
        add(["kp-"], code: kVK_ANSI_KeypadMinus)
        add(["kp/"], code: kVK_ANSI_KeypadDivide)
        add(["kp="], code: kVK_ANSI_KeypadEquals)

        return map
    }()

    private static let primaryKeyNames: [UInt16: String] = {
        var reverse: [UInt16: String] = [:]
        for (name, code) in canonicalKeyCodes where reverse[code] == nil {
            reverse[code] = name
        }
        return reverse
    }()

    private static let specialKeyNames: [UInt16: String] = [
        UInt16(kVK_Return): "Return",
        UInt16(kVK_ANSI_KeypadEnter): "Enter",
        UInt16(kVK_Space): "Space",
        UInt16(kVK_Delete): "⌫",
        UInt16(kVK_ForwardDelete): "⌦",
        UInt16(kVK_Escape): "Esc",
        UInt16(kVK_Tab): "Tab",
        UInt16(kVK_CapsLock): "Caps Lock",
        UInt16(kVK_Help): "Help",
        UInt16(kVK_Home): "Home",
        UInt16(kVK_End): "End",
        UInt16(kVK_PageUp): "Page ↑",
        UInt16(kVK_PageDown): "Page ↓",
        UInt16(kVK_LeftArrow): "←",
        UInt16(kVK_RightArrow): "→",
        UInt16(kVK_UpArrow): "↑",
        UInt16(kVK_DownArrow): "↓",
        UInt16(kVK_F1): "F1",
        UInt16(kVK_F2): "F2",
        UInt16(kVK_F3): "F3",
        UInt16(kVK_F4): "F4",
        UInt16(kVK_F5): "F5",
        UInt16(kVK_F6): "F6",
        UInt16(kVK_F7): "F7",
        UInt16(kVK_F8): "F8",
        UInt16(kVK_F9): "F9",
        UInt16(kVK_F10): "F10",
        UInt16(kVK_F11): "F11",
        UInt16(kVK_F12): "F12",
        UInt16(kVK_F13): "F13",
        UInt16(kVK_F14): "F14",
        UInt16(kVK_F15): "F15",
        UInt16(kVK_F16): "F16",
        UInt16(kVK_F17): "F17",
        UInt16(kVK_F18): "F18",
        UInt16(kVK_F19): "F19",
        UInt16(kVK_F20): "F20"
    ]

    private static let modifierSymbolMap: [String: String] = [
        "cmd": "⌘",
        "command": "⌘",
        "shift": "⇧",
        "alt": "⌥",
        "option": "⌥",
        "ctrl": "⌃",
        "control": "⌃",
        "fn": "fn"
    ]

    private static let shiftedCharacterMap: [Character: (key: String, modifiers: [String])] = [
        "!": ("1", ["shift"]),
        "@": ("2", ["shift"]),
        "#": ("3", ["shift"]),
        "$": ("4", ["shift"]),
        "%": ("5", ["shift"]),
        "^": ("6", ["shift"]),
        "&": ("7", ["shift"]),
        "*": ("8", ["shift"]),
        "(": ("9", ["shift"]),
        ")": ("0", ["shift"]),
        "_": ("minus", ["shift"]),
        "+": ("equal", ["shift"]),
        ":": ("semicolon", ["shift"]),
        "\"": ("quote", ["shift"]),
        "<": ("comma", ["shift"]),
        ">": ("period", ["shift"]),
        "?": ("slash", ["shift"]),
        "|": ("backslash", ["shift"]),
        "~": ("grave", ["shift"]),
        "{": ("left bracket", ["shift"]),
        "}": ("right bracket", ["shift"])
    ]
}

struct MacroStep: Equatable, Codable {
    var type: String // "key", "text", "delay"
    var keyStroke: KeyStroke? = nil
    var text: String? = nil
    var delayMs: Int? = nil
}
