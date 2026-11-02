import SwiftUI

/// Edits a draft. Nothing reaches the runtime mapping until Apply Macro.
struct MacroEditor: View {
    @Binding var steps: [MacroStep]
    let apply: () -> Void
    @State private var selected = 0
    @State private var json = ""
    @State private var error: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text("\(steps.count) steps").foregroundStyle(.secondary)
                Spacer()
                Menu("Add Step") {
                    Button("Key") { append(MacroStep(type: "key", keyStroke: KeyStroke(key: "space", modifiers: [], keyCode: 49))) }
                    Button("Text") { append(MacroStep(type: "text", text: "")) }
                    Button("Delay") { append(MacroStep(type: "delay", delayMs: 100)) }
                }.disabled(steps.count >= ProfileValidation.maximumMacroSteps)
            }
            if !steps.isEmpty {
                Picker("Step", selection: $selected) {
                    ForEach(steps.indices, id: \.self) { index in Text(summary(index)).tag(index) }
                }
                MacroStepEditor(step: binding).id(selected)
                HStack {
                    Button { move(-1) } label: { Image(systemName: "arrow.up") }.help("Move step up").disabled(selected == 0)
                    Button { move(1) } label: { Image(systemName: "arrow.down") }.help("Move step down").disabled(selected >= steps.count - 1)
                    Spacer()
                    Button("Remove Step") {
                        guard steps.indices.contains(selected) else { return }
                        steps.remove(at: selected); selected = max(0, min(selected, steps.count - 1))
                    }
                }
            } else {
                Text("Add a key, text or delay step.").foregroundStyle(.secondary)
            }
            Text("Pressing this button again restarts its macro. Macros on other buttons stay independent.")
                .font(.caption).foregroundStyle(.secondary)
            Button("Apply Macro") {
                do { try ProfileValidation.validateMacro(steps); error = nil; apply() }
                catch { self.error = error.localizedDescription }
            }
            DisclosureGroup("Advanced JSON") {
                VStack(alignment: .leading, spacing: 8) {
                    TextEditor(text: $json).font(.system(.body, design: .monospaced)).frame(height: 140)
                    HStack {
                        Button("Copy Draft Here") { encodeDraft() }
                        Button("Load JSON") {
                            do {
                                guard json.utf8.count <= ProfileValidation.maximumFileBytes else { throw ProfileDataError(message: "The JSON draft is too large.") }
                                let decoded = try JSONDecoder().decode([MacroStep].self, from: Data(json.utf8))
                                try ProfileValidation.validateMacro(decoded)
                                steps = decoded; selected = 0; error = nil
                            } catch { self.error = error.localizedDescription }
                        }
                    }
                }.padding(.top, 8)
            }.font(.callout)
            if let error { Text(error).font(.caption).foregroundStyle(.red) }
        }.onAppear { encodeDraft() }
            .onChange(of: steps.count) { count in selected = max(0, min(selected, count - 1)) }
    }

    private var binding: Binding<MacroStep> {
        Binding(get: { steps.indices.contains(selected) ? steps[selected] : MacroStep(type: "delay", delayMs: 0) },
                set: { if steps.indices.contains(selected) { steps[selected] = $0 } })
    }
    private func append(_ step: MacroStep) { steps.append(step); selected = steps.count - 1 }
    private func move(_ delta: Int) {
        let target = selected + delta
        guard steps.indices.contains(selected), steps.indices.contains(target) else { return }
        steps.swapAt(selected, target); selected = target
    }
    private func summary(_ index: Int) -> String {
        let step = steps[index]
        let detail = step.type == "key" ? step.keyStroke?.formattedShortcut() ?? "Choose key"
            : step.type == "delay" ? "Delay \(step.delayMs ?? 0) ms" : "Text (\(step.text?.count ?? 0) characters)"
        return "\(index + 1). \(detail)"
    }
    private func encodeDraft() {
        let encoder = JSONEncoder(); encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        json = (try? encoder.encode(steps)).flatMap { String(data: $0, encoding: .utf8) } ?? "[]"
    }
}

private struct MacroStepEditor: View {
    @Binding var step: MacroStep
    @State private var recording = false
    private var stroke: KeyStroke { step.keyStroke ?? KeyStroke(key: "space", modifiers: [], keyCode: 49) }
    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            switch step.type {
            case "key":
                Picker("Key", selection: Binding(get: { stroke.keyCode ?? KeyStroke.keyCode(for: stroke.key) ?? 49 }, set: { code in
                    if let key = KeyboardKeyCatalog.current().first(where: { $0.code == code }) { step.keyStroke = key.stroke(modifiers: stroke.modifiers) }
                })) {
                    ForEach(KeyboardKeyCatalog.current()) { key in Text(key.label).tag(key.code) }
                }
                HStack {
                    ForEach(["ctrl", "alt", "shift", "cmd"], id: \.self) { modifier in
                        Toggle(["ctrl": "⌃", "alt": "⌥", "shift": "⇧", "cmd": "⌘"][modifier] ?? modifier, isOn: Binding(
                            get: { stroke.normalizedModifiers.contains(modifier) }, set: { enabled in
                                var key = stroke; key.modifiers = key.normalizedModifiers; key.modifiers.removeAll { $0 == modifier }
                                if enabled { key.modifiers.append(modifier) }; step.keyStroke = key
                            })).toggleStyle(.button).help(modifier)
                    }
                }
                ShortcutCapture(isRecording: $recording) { step.keyStroke = $0 }.frame(height: 28)
            case "text":
                TextEditor(text: Binding(get: { step.text ?? "" }, set: { step.text = $0 }))
                    .font(.system(.body, design: .monospaced)).frame(height: 90)
            default:
                TextField("Milliseconds", value: Binding(get: { step.delayMs ?? 0 }, set: { step.delayMs = $0 }), format: .number)
                Text("0 to 600,000 ms").font(.caption).foregroundStyle(.secondary)
            }
        }.onDisappear { recording = false }
    }
}
