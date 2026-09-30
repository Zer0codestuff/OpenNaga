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
    case macro = "Macro"
    case disabled = "Disabled"
}

struct ActionInspector: View {
    let button: Int
    @ObservedObject private var model = WorkspaceModel.shared
    @State private var kind: EditorKind = .original
    @State private var text = ""
    @State private var legacyText: String?
    @State private var description = ""
    @State private var keys: [KeyStroke] = []
    @State private var selectedStroke = 0
    @State private var mouse: MouseAction = .browserBack
    @State private var system: SystemAction = .volumeUp
    @State private var systemShortcuts: [String: Any] = [:]
    @State private var recording = false
    @State private var validationError: String?


    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack(spacing: 12) {
                Text("\(button)").font(.system(size: 20, weight: .medium, design: .rounded))
                    .foregroundStyle(UIStyle.accent).frame(width: 44, height: 44)
                    .background(UIStyle.selection).clipShape(RoundedRectangle(cornerRadius: 10))
                VStack(alignment: .leading, spacing: 4) {
                    Text(buttonName(button)).font(.system(size: 17, weight: .semibold))
                    Text("\(model.profile)").font(.caption).foregroundStyle(.secondary)
                }
            }
            HStack(spacing: 5) {
                Image(systemName: "checkmark.circle")
                Text(model.mapping[button]?.displayName ?? "Original function")
                    .lineLimit(2)
            }.font(.callout).foregroundStyle(.secondary)
            if button >= 18 {
                Label("Keep a primary click available so you can still use the Mac.", systemImage: "exclamationmark.triangle")
                    .font(.callout).foregroundStyle(.orange)
            }
            Picker("Action", selection: Binding(get: { kind }, set: changeKind)) {
                ForEach(EditorKind.allCases, id: \.self) { Text($0.rawValue).tag($0) }
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
            if let validationError {
                Text(validationError).font(.callout).foregroundStyle(.red)
            }
            Spacer(minLength: 12)
        }
        .textFieldStyle(.roundedBorder)
        .frame(maxWidth: .infinity, alignment: .leading)
        .onAppear { load() }
        .onChange(of: model.mapping[button]) { _ in
            if !recording { load() }
        }
        .onDisappear { recording = false }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            systemShortcuts = MacSystemShortcut.preferences()
        }
    }

    @ViewBuilder private var editor: some View {
        switch kind {
        case .original:
            Label("No remapping", systemImage: "arrow.uturn.backward")
            Text("The button keeps its default function. Choose Keys to assign a keyboard key.")
                .foregroundStyle(.secondary)
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
            Text("Mouse buttons 4 and 5 send real clicks. Browsers on macOS ignore them, so there they are converted to Back and Forward automatically.")
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
        case .shell:
            Text("The command runs when you press the button. Only use commands you trust.")
                .font(.callout).foregroundStyle(.secondary)
            TextEditor(text: $text).font(.system(.body, design: .monospaced)).frame(minHeight: 100)
            Button("Apply Command") { persist() }
        case .macro:
            Text("JSON steps. The existing macro stays unchanged until you apply a valid version.")
                .font(.callout).foregroundStyle(.secondary)
            TextEditor(text: $text).font(.system(.body, design: .monospaced)).frame(minHeight: 230)
            Button("Apply Macro") { persist() }
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
            if legacyText != nil {
                Text("The previous text stays saved until you choose a key.")
                    .font(.caption).foregroundStyle(.secondary)
            }
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
                            get: { currentStroke?.modifiers.contains(modifier) ?? false },
                            set: { enabled in
                                guard keys.indices.contains(selectedStroke) else { return }
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
        legacyText = nil
        text = newKind == .macro ? "[]" : ""
        keys = []
        selectedStroke = 0
        description = ""
        validationError = nil
        if [.original, .disabled, .mouse, .system].contains(newKind) { persist() }
    }

    private func load() {
        legacyText = nil
        description = ""
        keys = []
        systemShortcuts = MacSystemShortcut.preferences()
        switch model.mapping[button] {
        case nil: kind = .original
        case .audio(let action, let label): kind = .system; system = SystemAction(audio: action); description = label ?? ""
        case .system(let action, let label): kind = .system; system = action; description = label ?? ""
        case .disabled: kind = .disabled
        case .mouse(let action, let label): kind = .mouse; mouse = action; description = label ?? ""
        case .keySequence(let strokes, let label):
            kind = .keys; keys = strokes; description = label ?? ""
            selectedStroke = min(selectedStroke, max(0, keys.count - 1))
        case .textSnippet(let value, let label): kind = .keys; legacyText = value; description = label ?? ""
        case .application(let value, let label): kind = .application; text = value; description = label ?? ""
        case .profileSwitch(let value, let label): kind = .profile; text = value; description = label ?? ""
        case .systemCommand(let value, let label): kind = .shell; text = value; description = label ?? ""
        case .macro(let steps, let label):
            kind = .macro; description = label ?? ""
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
            text = (try? encoder.encode(steps)).flatMap { String(data: $0, encoding: .utf8) } ?? "[]"
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
        case .shell: action = .systemCommand(command: text, description: label)
        case .macro:
            do {
                let steps = try JSONDecoder().decode([MacroStep].self, from: Data(text.utf8))
                guard steps.allSatisfy({ ["key", "text", "delay"].contains($0.type) &&
                    ($0.type != "key" || $0.keyStroke != nil) &&
                    ($0.type != "text" || $0.text != nil) &&
                    ($0.type != "delay" || ($0.delayMs ?? -1) >= 0) }) else {
                    validationError = "Invalid steps: use key, text or delay with its value."
                    return
                }
                action = .macro(steps: steps, description: label)
            } catch { validationError = "Invalid JSON: \(error.localizedDescription)"; return }
        }
        model.save(action, button: button)
    }
}

private func modifierSymbol(_ value: String) -> String {
    ["cmd": "⌘", "shift": "⇧", "alt": "⌥", "ctrl": "⌃"][value] ?? value
}

/// A local monitor exists only while the user explicitly arms this control.
private struct ShortcutCapture: NSViewRepresentable {
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

private final class CaptureButton: NSButton {
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
