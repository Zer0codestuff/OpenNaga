import Foundation

struct ProfileDataError: LocalizedError {
    let message: String
    var errorDescription: String? { message }
}

/// The same limits apply to disk files, imports and the action editor.
enum ProfileValidation {
    static let maximumFileBytes = 2 * 1024 * 1024
    static let maximumProfiles = 128
    static let maximumTextBytes = 16_384
    static let maximumMacroSteps = 256

    static func validate(_ file: ProfilesFile) throws {
        guard file.formatVersion == nil || file.formatVersion == 1 else {
            throw ProfileDataError(message: "This profile format is not supported by this OpenNaga build. Keep the file and use a compatible version.")
        }
        guard !file.profiles.isEmpty, file.profiles.count <= maximumProfiles else {
            throw ProfileDataError(message: "A profile file must contain 1 to \(maximumProfiles) profiles.")
        }
        for (name, profile) in file.profiles {
            guard !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, name.utf8.count <= 256 else {
                throw ProfileDataError(message: "Profile names must contain 1 to 256 UTF-8 bytes.")
            }
            for layer in MappingLayer.allCases {
                let buttons = profile.buttons(in: layer)
                guard buttons.count <= 19 else { throw ProfileDataError(message: "A layer can contain at most 19 controls.") }
                for (button, action) in buttons {
                    guard let index = Int(button), (1...19).contains(index), String(index) == button else {
                        throw ProfileDataError(message: "Profile controls must be numbered 1 to 19.")
                    }
                    do { try validate(action, layer: layer) }
                    catch { throw ProfileDataError(message: "Profile \(name), \(layer.rawValue) control \(button): \(error.localizedDescription)") }
                }
            }
        }
        guard (file.settings?.appProfiles?.count ?? 0) <= 1024 else {
            throw ProfileDataError(message: "A profile file can contain at most 1024 app bindings.")
        }
    }

    static func validate(_ action: ButtonAction, layer: MappingLayer = .standard) throws {
        switch action.type {
        case "disabled": break
        case "audio": try require(action.audioAction != nil, "Choose an audio function.")
        case "system": try require(action.systemAction != nil, "Choose a system function.")
        case "mouse":
            try require(action.mouseAction != nil, "Choose a mouse function.")
        case "hypershift": try require(layer == .standard, "The Hypershift layer cannot contain another Hypershift key.")
        case "keySequence":
            let keys = action.keys ?? []
            try require(!keys.isEmpty && keys.count <= 64, "A key sequence must contain 1 to 64 keys.")
            try keys.forEach(validate)
        case "application": try bounded(action.path, label: "Application path", maximum: 4096, allowEmpty: false)
        case "systemCommand": try bounded(action.command, label: "Command", allowEmpty: false)
        case "textSnippet": try bounded(action.text, label: "Text")
        case "profileSwitch": try bounded(action.profile, label: "Target profile", maximum: 256, allowEmpty: false)
        case "macro":
            guard let steps = action.steps else { throw ProfileDataError(message: "Macro steps are missing.") }
            try validateMacro(steps)
        default: throw ProfileDataError(message: "Unsupported action type: \(action.type.prefix(80)).")
        }
        try bounded(action.description ?? "", label: "Custom name", maximum: 1024)
    }

    static func validate(_ key: KeyStroke) throws {
        try require(key.keyCode.map { $0 <= 127 } ?? (KeyStroke.keyCode(for: key.key) != nil), "Choose a supported keyboard key.")
        let modifiers: Set<String> = ["cmd", "command", "ctrl", "control", "shift", "alt", "option", "fn"]
        try require(key.modifiers.count <= 8 && key.modifiers.allSatisfy { modifiers.contains($0.lowercased()) }, "Unsupported keyboard modifier.")
    }

    static func validateMacro(_ steps: [MacroStep]) throws {
        try require(steps.count <= maximumMacroSteps, "A macro can contain at most \(maximumMacroSteps) steps.")
        var duration = 0
        var textBytes = 0
        for (index, step) in steps.enumerated() {
            switch step.type {
            case "key":
                guard let key = step.keyStroke else { throw ProfileDataError(message: "Step \(index + 1) needs a key.") }
                try validate(key)
            case "text":
                try bounded(step.text, label: "Step \(index + 1) text")
                textBytes += step.text?.utf8.count ?? 0
            case "delay":
                guard let delay = step.delayMs, (0...600_000).contains(delay) else {
                    throw ProfileDataError(message: "Step \(index + 1) delay must be 0 to 600,000 ms.")
                }
                duration += delay
            default: throw ProfileDataError(message: "Step \(index + 1) must be key, text or delay.")
            }
        }
        try require(duration <= 3_600_000, "A macro's total delay cannot exceed one hour.")
        try require(textBytes <= maximumTextBytes, "A macro can contain at most \(maximumTextBytes) UTF-8 bytes of text.")
    }

    private static func bounded(_ value: String?, label: String, maximum: Int = maximumTextBytes, allowEmpty: Bool = true) throws {
        guard let value, value.utf8.count <= maximum,
              allowEmpty || !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw ProfileDataError(message: "\(label) must contain \(allowEmpty ? "0" : "1") to \(maximum) UTF-8 bytes.")
        }
    }
    private static func require(_ condition: Bool, _ message: String) throws {
        if !condition { throw ProfileDataError(message: message) }
    }
}
