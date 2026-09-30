import Cocoa
import Carbon.HIToolbox

enum KeyboardKeyGroup: String, CaseIterable {
    case letters = "Letters"
    case numbers = "Numbers"
    case symbols = "Symbols"
    case navigation = "Special keys"
    case function = "Function keys"
    case keypad = "Numeric keypad"
}

struct KeyboardKey: Identifiable, Equatable {
    let code: UInt16
    let key: String
    let label: String
    let group: KeyboardKeyGroup
    var id: UInt16 { code }
    func stroke(modifiers: [String] = []) -> KeyStroke {
        KeyStroke(key: key, modifiers: modifiers, keyCode: code)
    }
}

enum KeyboardKeyCatalog {
    /// Physical codes are persisted, labels follow the active macOS input source.
    static func current() -> [KeyboardKey] {
        var result: [KeyboardKey] = []
        let data: CFData? = (TISCopyCurrentKeyboardLayoutInputSource()?.takeRetainedValue()).flatMap { source in
            guard let property = TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData) else { return nil }
            return Unmanaged<CFData>.fromOpaque(property).takeUnretainedValue()
        }
        func add(_ names: [String], _ group: KeyboardKeyGroup, localized: Bool = false) {
            for name in names {
                guard let code = KeyStroke.keyCode(for: name) else { continue }
                let character = localized ? character(for: code, data: data) : nil
                result.append(KeyboardKey(code: code, key: character ?? name,
                                          label: character?.uppercased() ?? label(for: name), group: group))
            }
        }
        add(Array("abcdefghijklmnopqrstuvwxyz").map(String.init), .letters, localized: true)
        add(Array("1234567890").map(String.init), .numbers, localized: true)
        add(["grave", "minus", "equals", "left bracket", "right bracket", "backslash", "semicolon", "quote", "comma", "period", "slash"], .symbols, localized: true)
        if let character = character(for: 10, data: data) {
            result.append(KeyboardKey(code: 10, key: character, label: character.uppercased(), group: .symbols))
        }
        add(["space", "tab", "return", "escape", "delete", "forward delete", "left arrow", "right arrow", "up arrow", "down arrow", "home", "end", "page up", "page down"], .navigation)
        add((1...20).map { "f\($0)" }, .function)
        add((0...9).map { "kp\($0)" } + ["kp.", "kp+", "kp-", "kp*", "kp/", "kp=", "keypad enter"], .keypad)
        return result
    }

    static func label(for key: String) -> String {
        let labels = [
            "space": "Space", "tab": "Tab", "return": "Return", "escape": "Esc",
            "delete": "⌫", "forward delete": "⌦", "left arrow": "←", "right arrow": "→",
            "up arrow": "↑", "down arrow": "↓", "home": "↖", "end": "↘",
            "page up": "Page ↑", "page down": "Page ↓", "keypad enter": "Enter"
        ]
        if let label = labels[key] { return label }
        if key.hasPrefix("kp") { return String(key.dropFirst(2)) }
        return key.uppercased()
    }

    static func capturedStroke(code: UInt16, characters: String?, modifiers: [String]) -> KeyStroke {
        if let entry = current().first(where: { $0.code == code }) { return entry.stroke(modifiers: modifiers) }
        return KeyStroke(key: KeyStroke.canonicalKeyString(for: code, characters: characters), modifiers: modifiers, keyCode: code)
    }

    private static func character(for code: UInt16, data: CFData?) -> String? {
        guard let data, let bytes = CFDataGetBytePtr(data) else { return nil }
        let layout = UnsafeRawPointer(bytes).assumingMemoryBound(to: UCKeyboardLayout.self)
        var deadKey: UInt32 = 0
        var length = 0
        var characters = [UniChar](repeating: 0, count: 8)
        let status = UCKeyTranslate(layout, code, UInt16(kUCKeyActionDown), 0, UInt32(LMGetKbdType()),
                                    OptionBits(kUCKeyTranslateNoDeadKeysBit), &deadKey, characters.count, &length, &characters)
        guard status == noErr, length > 0 else { return nil }
        let string = String(utf16CodeUnits: characters, count: length)
        guard !string.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) }) else { return nil }
        return string
    }
}
