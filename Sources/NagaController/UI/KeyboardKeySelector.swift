import SwiftUI
import Carbon.HIToolbox

struct KeyboardKeySelector: View {
    let selectedCode: UInt16?
    let onSelect: (KeyboardKey) -> Void
    @State private var group: KeyboardKeyGroup = .letters
    @State private var catalog: [KeyboardKey] = []

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Picker("Choose a key", selection: $group) {
                ForEach(KeyboardKeyGroup.allCases, id: \.self) { Text($0.rawValue).tag($0) }
            }
            LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 5), count: group == .navigation ? 4 : 6), spacing: 5) {
                ForEach(catalog.filter { $0.group == group }) { key in
                    Button { onSelect(key) } label: {
                        Text(key.label).font(.system(size: 12, weight: .medium))
                            .lineLimit(1).minimumScaleFactor(0.8)
                            .frame(maxWidth: .infinity).frame(height: 33)
                            .foregroundStyle(selectedCode == key.code ? UIStyle.accent : .primary)
                            .background(selectedCode == key.code ? UIStyle.selection : Color(nsColor: .controlBackgroundColor))
                            .clipShape(RoundedRectangle(cornerRadius: 5))
                            .overlay(RoundedRectangle(cornerRadius: 5).stroke(selectedCode == key.code ? UIStyle.accent.opacity(0.6) : UIStyle.separator))
                    }.buttonStyle(.plain)
                        .help(key.group == .keypad ? "Keypad: \(key.label)" : key.label)
                        .accessibilityLabel(key.group == .keypad ? "Keypad: \(key.label)" : key.label)
                        .accessibilityAddTraits(selectedCode == key.code ? [.isSelected] : [])
                }
            }
        }
        .onAppear {
            catalog = KeyboardKeyCatalog.current()
            if let entry = catalog.first(where: { $0.code == selectedCode }) { group = entry.group }
        }
        .onChange(of: selectedCode) { code in
            if let entry = catalog.first(where: { $0.code == code }) { group = entry.group }
        }
        .onReceive(DistributedNotificationCenter.default().publisher(for: Notification.Name(kTISNotifySelectedKeyboardInputSourceChanged as String))) { _ in
            catalog = KeyboardKeyCatalog.current()
        }
    }
}
