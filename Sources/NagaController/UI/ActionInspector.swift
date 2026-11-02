import Cocoa
import SwiftUI

private enum EditorKind: String, CaseIterable {
    case original = "Original function"
    case mouse = "Mouse action"
    case keys = "Keys"
    case system = "System"
    case application = "Application"
    case profile = "Switch profile"
    case shell = "Shell command"
    case text = "Text"
    case macro = "Macro"
    case hypershift = "Hypershift key"
    case disabled = "Disabled"
}

struct ActionInspector: View {
    let button: Int
    @ObservedObject private var model = WorkspaceModel.shared
    @State private var kind: EditorKind = .original
    @State private var text = ""
    @State private var description = ""
    @State private var keys: [KeyStroke] = []
    @State private var selectedStroke = 0
    @State private var mouse: MouseAction = .browserBack
    @State private var system: SystemAction = .volumeUp
    @State private var systemShortcuts: [String: Any] = [:]
    @State private var recording = false
    @State private var validationError: String?
    @State private var macroSteps: [MacroStep] = []

    private var layer: MappingLayer { model.layer }
    private var current: ActionType? { model.action(for: button, layer: layer) }
    private var shiftedLayer: Bool { layer == .hypershift }
    // The Hypershift layer cannot hold a second Hypershift key.
    private var kinds: [EditorKind] { EditorKind.allCases.filter { !shiftedLayer || $0 != .hypershift } }

    private func title(_ kind: EditorKind) -> String {
        kind == .original && shiftedLayer ? "Same as standard" : kind.rawValue
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack(spacing: 12) {
                Text("\(button)").font(.system(size: 20, weight: .medium, design: .rounded))
                    .foregroundStyle(UIStyle.accent).frame(width: 44, height: 44)
                    .background(UIStyle.selection).clipShape(RoundedRectangle(cornerRadius: 10))
                VStack(alignment: .leading, spacing: 4) {
                    Text(buttonName(button)).font(.system(size: 17, weight: .semibold))
                    Text(shiftedLayer ? "\(model.profile), Hypershift layer" : "\(model.profile)")
                        .font(.caption).foregroundStyle(.secondary)
                }
            }
            HStack(spacing: 5) {
                Image(systemName: "checkmark.circle")
                Text(shiftedLayer ? model.summary(for: button, layer: layer) : current?.displayName ?? "Original function")
                    .lineLimit(2)
            }.font(.callout).foregroundStyle(.secondary)
            if button >= 18 && !(shiftedLayer && model.isHypershiftKey(button)) {
                Label("Keep a primary click available so you can still use the Mac.", systemImage: "exclamationmark.triangle")
                    .font(.callout).foregroundStyle(.orange)
            }
            if shiftedLayer && model.isHypershiftKey(button) {
                Label("This control is the Hypershift key.", systemImage: "shift")
                Text("Hold it to use the Hypershift layer. To change its own action, switch to the Standard layer.")
                    .foregroundStyle(.secondary)
                Button("Edit Standard Layer") { model.layer = .standard }
            } else {
                if shiftedLayer && !model.hasHypershiftKey {
                    Label("No Hypershift key yet. In the Standard layer, set a control's action to Hypershift key.", systemImage: "exclamationmark.triangle")
                        .font(.callout).foregroundStyle(.orange)
                }
                actionEditor
            }
            if let validationError {
                Text(validationError).font(.callout).foregroundStyle(.red)
            }
            Spacer(minLength: 12)
        }
        .textFieldStyle(.roundedBorder)
        .disabled(model.profilesReadOnly)
        .frame(maxWidth: .infinity, alignment: .leading)
        .onAppear { load() }
        .onChange(of: current) { _ in
            if !recording { load() }
        }
        .onDisappear { recording = false }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            systemShortcuts = MacSystemShortcut.preferences()
        }
    }

    @ViewBuilder private var actionEditor: some View {
        Picker("Action", selection: Binding(get: { kind }, set: changeKind)) {
            ForEach(kinds, id: \.self) { Text(title($0)).tag($0) }
        }
        Divider()
        editor
        if kind != .original && kind != .disabled {
            DisclosureGroup("Custom name") {
                TextField("Optional name", text: $description)
                    .onSubmit { persist() }.padding(.top, 8)
                Text("Press Return to save the name.").font(.caption).foregroundStyle(.secondary)
            }.font(.callout)
        }
    }

    @ViewBuilder private var editor: some View {
        switch kind {
        case .original:
            if shiftedLayer {
                Label("Same as standard", systemImage: "arrow.uturn.backward")
                Text("While Hypershift is held, this control keeps its Standard layer action: \(model.mapping[button]?.displayName ?? "Original function").")
                    .foregroundStyle(.secondary)
            } else {
                Label("No remapping", systemImage: "arrow.uturn.backward")
                Text("The button keeps its default function. Choose Keys to assign a keyboard key.")
                    .foregroundStyle(.secondary)
            }
        case .hypershift:
            Label("Hypershift key", systemImage: "shift")
            Text("While you hold this control, the other controls use their Hypershift layer actions. Controls without one keep their standard action. Pick a control you can hold while pressing the others, for example a side button held with your thumb while you click or tilt the wheel. Save to Mouse stores both layers, so Hypershift also works with the app closed.")
                .foregroundStyle(.secondary)
            Button("Edit Hypershift Layer") { model.layer = .hypershift }
        case .disabled:
            Label("Button disabled", systemImage: "nosign")
            Text("The signal is blocked only while remapping is on.")
                .foregroundStyle(.secondary)
        case .mouse:
            HStack {
                Button("Back") { mouse = .browserBack; persist() }
                Button("Forward") { mouse = .browserForward; persist() }
            }
            Text("Recommended for browser navigation.").font(.caption).foregroundStyle(.secondary)
            Picker("Function", selection: Binding(get: { mouse }, set: { mouse = $0; persist() })) {
                ForEach(MouseAction.allCases, id: \.rawValue) { Text($0.title).tag($0) }
            }
            Text(mouse == .dpiUp || mouse == .dpiDown
                 ? "Hardware function: choose Save to Mouse to apply it. It changes the DPI stage inside the mouse, even when the app is closed."
                 : model.onboardActive
                    ? "Mouse buttons 4 and 5 stay standard clicks. Browsers may treat them as Back and Forward."
                    : "Mouse buttons 4 and 5 send real clicks. Browsers on macOS ignore them, so there they are converted to Back and Forward automatically.")
                .font(.callout).foregroundStyle(.secondary)
        case .system:
            systemEditor
        case .keys:
            shortcutEditor
        case .application:
            TextField("Application path", text: $text).onSubmit { persist() }
            Button("Choose Application…") {
                let panel = NSOpenPanel()
                panel.allowedContentTypes = [.applicationBundle]
                panel.canChooseDirectories = false
                if panel.runModal() == .OK, let url = panel.url { text = url.path; persist() }
            }
        case .profile:
            Picker("Target profile", selection: Binding(get: { text }, set: { text = $0; if !text.isEmpty { persist() } })) {
                Text("Choose a profile").tag("")
                ForEach(model.profiles, id: \.self) { Text($0).tag($0) }
            }
            if !text.isEmpty && !model.profiles.contains(text) {
                Label("This target profile no longer exists. Choose another profile.", systemImage: "exclamationmark.triangle")
                    .font(.caption).foregroundStyle(.orange)
            }
        case .shell:
            Text("The command runs when you press the button. Only use commands you trust.")
                .font(.callout).foregroundStyle(.secondary)
            TextEditor(text: $text).font(.system(.body, design: .monospaced)).frame(minHeight: 100)
            Button("Apply Command") { persist() }
        case .macro:
            MacroEditor(steps: $macroSteps) { persist() }
        case .text:
            TextEditor(text: $text).font(.system(.body, design: .monospaced)).frame(minHeight: 120)
            Text("Types text without changing the clipboard. Press again to restart; Stop Macros interrupts it.")
                .font(.caption).foregroundStyle(.secondary)
            Button("Apply Text") { persist() }
        }
    }

    private var systemEditor: some View {
        VStack(alignment: .leading, spacing: 16) {
            Picker("Category", selection: Binding(get: { system.group }, set: { group in
                guard let action = group.actions.first else { return }
                system = action
                persist()
            })) {
                ForEach(SystemActionGroup.allCases, id: \.self) { Text($0.rawValue).tag($0) }
            }
            VStack(alignment: .leading, spacing: 8) {
                Text("Function").font(.caption).foregroundStyle(.secondary)
                Picker("Function", selection: Binding(get: { system }, set: { system = $0; persist() })) {
                    ForEach(system.group.actions, id: \.self) { action in
                        Label(action.title, systemImage: action.symbol).tag(action)
                    }
                }.labelsHidden().frame(maxWidth: .infinity)
            }
            Text(system.help).font(.callout).foregroundStyle(.secondary)
            if let shortcut = system.shortcut {
                if let stroke = shortcut.resolve(in: systemShortcuts) {
                    Text("macOS shortcut: \(stroke.displayName)")
                        .font(.caption).foregroundStyle(.secondary)
                } else {
                    Label(system.shortcutSetupMessage, systemImage: "exclamationmark.triangle")
                        .font(.callout).foregroundStyle(.orange)
                    Button("Set Up Shortcut…") { MacSystemShortcut.openSettings() }
                }
            }
            Button { validationError = ButtonMapper.shared.performSystem(system) } label: {
                Label("Test", systemImage: "play.fill")
            }
            .help("Run the selected system function now")
            .disabled((system.needsAccessibility && !model.permissionsGranted) ||
                      (system.shortcut != nil && system.shortcut?.resolve(in: systemShortcuts) == nil))
        }
    }

    private var shortcutEditor: some View {
        VStack(alignment: .leading, spacing: 16) {
            if keys.count > 1 {
                Picker("Step", selection: $selectedStroke) {
                    ForEach(keys.indices, id: \.self) { index in
                        Text("\(index + 1). \(keys[index].formattedShortcut())").tag(index)
                    }
                }
            }
            VStack(spacing: 6) {
                Text(currentStroke?.formattedShortcut() ?? "Choose a key")
                    .font(.system(size: currentStroke == nil ? 17 : 28, weight: .medium))
                    .foregroundStyle(currentStroke == nil ? Color.secondary : .primary)
                if keys.count <= 1 {
                    Text("Held down while you press the mouse button")
                        .font(.system(size: 10)).foregroundStyle(.secondary)
                }
            }.frame(maxWidth: .infinity).frame(height: 84)
                .background(UIStyle.inset).clipShape(RoundedRectangle(cornerRadius: 10))
            KeyboardKeySelector(selectedCode: currentStroke?.keyCode) { key in
                replaceStroke(key.stroke(modifiers: currentStroke?.modifiers ?? []))
            }
            VStack(alignment: .leading, spacing: 8) {
                Text("Combine with").font(.caption).foregroundStyle(.secondary)
                HStack(spacing: 6) {
                    ForEach(["ctrl", "alt", "shift", "cmd"], id: \.self) { modifier in
                        Toggle(modifierSymbol(modifier), isOn: Binding(
                            get: { currentStroke?.normalizedModifiers.contains(modifier) ?? false },
                            set: { enabled in
                                guard keys.indices.contains(selectedStroke) else { return }
                                keys[selectedStroke].modifiers = keys[selectedStroke].normalizedModifiers
                                keys[selectedStroke].modifiers.removeAll { $0 == modifier }
                                if enabled { keys[selectedStroke].modifiers.append(modifier) }
                                persist()
                            }
                        )).toggleStyle(.button).frame(maxWidth: .infinity)
                            .help(["ctrl": "Control", "alt": "Option", "shift": "Shift", "cmd": "Command"][modifier] ?? modifier)
                            .disabled(currentStroke == nil)
                    }
                }
            }
            Divider()
            ShortcutCapture(isRecording: $recording) { replaceStroke($0) }.frame(height: 28)
            Text(recording ? "Press a key or combination. Esc is assigned as a key." : "You can also press the key on your keyboard with Record Key.")
                .font(.caption).foregroundStyle(.secondary)
            DisclosureGroup("Key sequence") {
                VStack(alignment: .leading, spacing: 10) {
                    Text("Multiple steps run in order when you press the button.")
                        .font(.caption).foregroundStyle(.secondary)
                    HStack {
                        Button("Add Step") {
                            keys.append(KeyStroke(key: "tab", modifiers: [], keyCode: 48))
                            selectedStroke = keys.count - 1
                            persist()
                        }
                        if keys.count > 1 {
                            Button("Remove") {
                                keys.remove(at: selectedStroke)
                                selectedStroke = max(0, selectedStroke - 1)
                                persist()
                            }
                        }
                    }
                }.padding(.top, 8)
            }.font(.callout)
        }
    }

    private var currentStroke: KeyStroke? {
        keys.indices.contains(selectedStroke) ? keys[selectedStroke] : nil
    }

    private func replaceStroke(_ stroke: KeyStroke) {
        recording = false
        if keys.indices.contains(selectedStroke) { keys[selectedStroke] = stroke }
        else { keys.append(stroke); selectedStroke = keys.count - 1 }
        persist()
    }

    private func changeKind(_ newKind: EditorKind) {
        guard newKind != kind else { return }
        if kind == .macro || keys.count > 1 {
            let alert = NSAlert()
            alert.messageText = "Replace the existing action?"
            alert.informativeText = "The macro or sequence will be replaced in the current profile."
            alert.addButton(withTitle: "Replace")
            alert.addButton(withTitle: "Cancel")
            guard alert.runModal() == .alertFirstButtonReturn else { return }
        }
        recording = false
        kind = newKind
        text = newKind == .macro ? "[]" : ""
        macroSteps = []
        keys = []
        selectedStroke = 0
        description = ""
        validationError = nil
        if [.original, .disabled, .mouse, .system, .hypershift].contains(newKind) { persist() }
    }

    private func load() {
        description = ""
        keys = []
        systemShortcuts = MacSystemShortcut.preferences()
        switch current {
        case nil: kind = .original
        case .hypershift(let label): kind = .hypershift; description = label ?? ""
        case .audio(let action, let label): kind = .system; system = SystemAction(audio: action); description = label ?? ""
        case .system(let action, let label): kind = .system; system = action; description = label ?? ""
        case .disabled: kind = .disabled
        case .mouse(let action, let label): kind = .mouse; mouse = action; description = label ?? ""
        case .keySequence(let strokes, let label):
            kind = .keys; keys = strokes; description = label ?? ""
            selectedStroke = min(selectedStroke, max(0, keys.count - 1))
        case .textSnippet(let value, let label): kind = .text; text = value; description = label ?? ""
        case .application(let value, let label): kind = .application; text = value; description = label ?? ""
        case .profileSwitch(let value, let label): kind = .profile; text = value; description = label ?? ""
        case .systemCommand(let value, let label): kind = .shell; text = value; description = label ?? ""
        case .macro(let steps, let label):
            kind = .macro; description = label ?? ""; macroSteps = steps
        }
    }

    private func persist() {
        validationError = nil
        let label = description.isEmpty ? nil : description
        let action: ActionType?
        switch kind {
        case .original: action = nil
        case .system: action = .system(action: system, description: label)
        case .disabled: action = .disabled
        case .mouse: action = .mouse(action: mouse, description: label)
        case .keys:
            guard !keys.isEmpty else { return }
            action = .keySequence(keys: keys, description: label)
        case .application: action = .application(path: text, description: label)
        case .profile: action = .profileSwitch(profile: text, description: label)
        case .hypershift:
            guard !shiftedLayer else { return }
            action = .hypershift(description: label)
        case .shell: action = .systemCommand(command: text, description: label)
        case .text: action = .textSnippet(text: text, description: label)
        case .macro:
            do {
                try ProfileValidation.validateMacro(macroSteps)
                action = .macro(steps: macroSteps, description: label)
            } catch { validationError = error.localizedDescription; return }
        }
        model.save(action, button: button, layer: layer)
    }
}

private func modifierSymbol(_ value: String) -> String {
    ["cmd": "⌘", "shift": "⇧", "alt": "⌥", "ctrl": "⌃"][value] ?? value
}

/// A local monitor exists only while the user explicitly arms this control.
struct ShortcutCapture: NSViewRepresentable {
    @Binding var isRecording: Bool
    let onCapture: (KeyStroke) -> Void

    func makeNSView(context: Context) -> CaptureButton { CaptureButton() }
    func updateNSView(_ view: CaptureButton, context: Context) {
        view.onCapture = { stroke in isRecording = false; onCapture(stroke) }
        view.onRecordingChange = { isRecording = $0 }
        view.setRecording(isRecording)
    }
    static func dismantleNSView(_ nsView: CaptureButton, coordinator: ()) { nsView.setRecording(false) }
}

final class CaptureButton: NSButton {
    var onCapture: ((KeyStroke) -> Void)?
    var onRecordingChange: ((Bool) -> Void)?
    private var monitor: Any?
    private var resignation: NSObjectProtocol?
    private var recording = false

    init() {
        super.init(frame: .zero)
        bezelStyle = .rounded
        title = "Record Key"
        target = self
        action = #selector(toggleRecording)
        resignation = NotificationCenter.default.addObserver(forName: NSWindow.didResignKeyNotification, object: nil, queue: .main) { [weak self] note in
            guard let self, let window = note.object as? NSWindow, window === self.window else { return }
            self.setRecording(false)
            self.onRecordingChange?(false)
        }
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) is unavailable") }
    @objc private func toggleRecording() { setRecording(!recording); onRecordingChange?(recording) }
    func setRecording(_ value: Bool) {
        guard value != recording else { return }
        recording = value
        title = value ? "Cancel Recording" : "Record Key"
        if let monitor { NSEvent.removeMonitor(monitor); self.monitor = nil }
        guard value else { return }
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self, self.recording, event.window === self.window, self.window?.isKeyWindow == true else { return event }
            let flags = event.modifierFlags
            var modifiers: [String] = []
            if flags.contains(.command) { modifiers.append("cmd") }
            if flags.contains(.shift) { modifiers.append("shift") }
            if flags.contains(.option) { modifiers.append("alt") }
            if flags.contains(.control) { modifiers.append("ctrl") }
            let stroke = KeyboardKeyCatalog.capturedStroke(code: event.keyCode, characters: event.charactersIgnoringModifiers, modifiers: modifiers)
            self.setRecording(false)
            self.onCapture?(stroke)
            return nil
        }
    }
    deinit {
        if let monitor { NSEvent.removeMonitor(monitor) }
        if let resignation { NotificationCenter.default.removeObserver(resignation) }
    }
}
