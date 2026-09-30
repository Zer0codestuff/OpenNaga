import Cocoa
import SwiftUI

struct SensitivityPane: View {
    @ObservedObject private var model = WorkspaceModel.shared
    @State private var dpiX = ""
    @State private var dpiY = ""
    @State private var rate = 1000
    @State private var editedDPI = false
    @State private var editedRate = false
    private var device: RazerDeviceController { .shared }
    private var available: Bool { device.isConnected && !device.isBusy }
    private var validDPI: Bool {
        guard let x = Int(dpiX), let y = Int(dpiY) else { return false }
        return (100...30000).contains(x) && (100...30000).contains(y)
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                HStack {
                    VStack(alignment: .leading, spacing: 6) {
                        Text("Sensor sensitivity").font(.title2.weight(.semibold))
                        Text("Hardware settings change only when you apply them.")
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    Button { device.refresh() } label: { Label("Refresh", systemImage: "arrow.clockwise") }
                        .disabled(device.isBusy)
                }
                if !device.isConnected {
                    Label("Hardware control unavailable. Connect the Naga V2 HyperSpeed USB receiver.",
                          systemImage: "cable.connector").foregroundStyle(.secondary)
                }
                GroupBox {
                    VStack(alignment: .leading, spacing: 16) {
                        Text("DPI").font(.headline)
                        Text("Current value: X \(device.dpiX.map(String.init) ?? "unavailable") · Y \(device.dpiY.map(String.init) ?? "unavailable")")
                            .font(.callout).foregroundStyle(.secondary)
                        HStack {
                            TextField("X axis", text: Binding(get: { dpiX }, set: { dpiX = $0; editedDPI = true })).frame(width: 140)
                            TextField("Y axis", text: Binding(get: { dpiY }, set: { dpiY = $0; editedDPI = true })).frame(width: 140)
                            Spacer()
                            Button("Apply DPI") {
                                guard let x = Int(dpiX), let y = Int(dpiY), validDPI else { return }
                                device.setDPI(x: x, y: y)
                                editedDPI = false
                            }.disabled(!available || !validDPI || device.dpiX == nil || device.dpiY == nil)
                        }
                        HStack {
                            Text("Presets").foregroundStyle(.secondary)
                            ForEach([400, 800, 1600, 3200, 6400], id: \.self) { value in
                                Button("\(value)") { dpiX = String(value); dpiY = String(value); editedDPI = true }
                            }
                        }
                        Text("100 to 30,000 DPI per axis. Presets fill in the fields without changing the mouse.")
                            .font(.caption).foregroundStyle(.secondary)
                    }.padding(12).frame(maxWidth: .infinity, alignment: .leading)
                }
                GroupBox {
                    VStack(alignment: .leading, spacing: 16) {
                        Text("Polling rate").font(.headline)
                        Text(device.pollingRate.map { "Current value: \($0) Hz" } ?? "Value unavailable")
                            .foregroundStyle(.secondary)
                        HStack {
                            Picker("Rate", selection: Binding(get: { rate }, set: { rate = $0; editedRate = true })) {
                                ForEach([125, 500, 1000], id: \.self) { Text("\($0) Hz").tag($0) }
                            }.pickerStyle(.segmented).frame(maxWidth: 340)
                            Spacer()
                            Button("Apply Rate") {
                                device.setPollingRate(rate)
                                editedRate = false
                            }.disabled(!available || device.pollingRate == nil)
                        }
                    }.padding(12).frame(maxWidth: .infinity, alignment: .leading)
                }
                GroupBox {
                    VStack(alignment: .leading, spacing: 12) {
                        Toggle("Driver mode for the top buttons", isOn: Binding(
                            get: { device.driverModeEnabled },
                            set: { device.setDriverModeEnabled($0) }
                        )).disabled(!available || device.recoveryPending || model.onboardActive)
                        Text("Optional. Lets the software handle the top DPI buttons on compatible devices. It can replace their built-in DPI function. Turn it off to restore normal mode.")
                            .font(.callout).foregroundStyle(.secondary)
                        if device.recoveryPending {
                            Button("Restore Original Mode") { device.recoverOriginalMode() }
                                .disabled(device.isBusy)
                            Text("Use the same receiver in its original USB port. Restoring does not happen automatically after an unexpected quit.")
                                .font(.caption).foregroundStyle(.secondary)
                        }
                    }.padding(12).frame(maxWidth: .infinity, alignment: .leading)
                }
                HStack(spacing: 10) {
                    if device.isBusy { ProgressView().controlSize(.small) }
                    Text(device.statusMessage).font(.callout).foregroundStyle(.secondary)
                }
            }.padding(24)
        }
        .textFieldStyle(.roundedBorder)
        .onAppear { updateFields() }
        .onChange(of: model.revision) { _ in updateFields() }
    }

    private func updateFields() {
        if !editedDPI {
            dpiX = device.dpiX.map(String.init) ?? ""
            dpiY = device.dpiY.map(String.init) ?? ""
        }
        if !editedRate, let value = device.pollingRate, [125, 500, 1000].contains(value) { rate = value }
    }
}

struct StatusPane: View {
    @ObservedObject private var model = WorkspaceModel.shared
    private var permissions: PermissionManager { .shared }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                HStack {
                    Text("Permissions and connection").font(.title2.weight(.semibold))
                    Spacer()
                    Button("Check Again") { model.refresh() }
                }
                Text("Permissions are checked again automatically. If macOS asks, reopen the app.")
                    .foregroundStyle(.secondary)
                GroupBox {
                    VStack(spacing: 20) {
                        permissionRow("Accessibility", granted: permissions.hasAccessibilityPermission(),
                                      detail: "Allows sending the configured actions.",
                                      open: permissions.openAccessibilityPreferences)
                        Divider()
                        permissionRow("Input Monitoring", granted: permissions.hasInputMonitoringPermission(),
                                      detail: "Allows recognizing and intercepting the buttons.",
                                      open: permissions.openInputMonitoringPreferences)
                    }.padding(12)
                }
                GroupBox {
                    VStack(alignment: .leading, spacing: 16) {
                        statusRow("Device", model.deviceName)
                        statusRow("Transport", HIDListener.shared.transport ?? "Unavailable")
                        statusRow("Battery", RazerDeviceController.shared.batteryLevel.map { "\($0)%" } ?? "Unavailable")
                        Divider()
                        statusRow("Input interception", EventTapManager.shared.isRunning ? "Running" : "Unavailable")
                        statusRow("Remapping", model.remappingActive ? "On" : "Off")
                        statusRow("Last input", HIDListener.shared.lastInputDescription)
                    }.padding(12)
                }
                GroupBox {
                    HStack(alignment: .top, spacing: 14) {
                        Image(systemName: "menubar.rectangle").font(.title2).foregroundStyle(UIStyle.accent)
                        VStack(alignment: .leading, spacing: 6) {
                            Text("Keeps running in the background").font(.headline)
                            Text(model.onboardActive ? (model.onboardName == nil ? "The last save is incomplete. Restore the previous assignments from Mouse Memory." : "The profile saved in the mouse keeps working after Quit. To go back to software remapping, restore the previous assignments from Mouse Memory.") : "Close the window to keep OpenNaga in the menu bar. Your assignments keep working in other apps. To stop the service, turn off Remapping or choose Quit.")
                                .font(.callout).foregroundStyle(.secondary)
                        }
                        Spacer()
                    }.padding(12)
                }
                Text(RazerDeviceController.shared.statusMessage).font(.callout).foregroundStyle(.secondary)
                Text("The left and right main buttons keep their original function until you assign an action.")
                    .font(.caption).foregroundStyle(.secondary)
            }.padding(24)
        }
    }

    private func permissionRow(_ title: String, granted: Bool, detail: String, open: @escaping () -> Void) -> some View {
        HStack(spacing: 12) {
            Image(systemName: granted ? "checkmark.circle.fill" : "exclamationmark.circle")
                .foregroundStyle(granted ? Color.green : .orange).font(.title2)
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(.headline)
                Text(detail).font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            Text(granted ? "Granted" : "Not granted").foregroundStyle(.secondary)
            Button("Open Settings", action: open)
        }
    }

    private func statusRow(_ title: String, _ value: String) -> some View {
        HStack(alignment: .top) {
            Text(title).foregroundStyle(.secondary).frame(width: 160, alignment: .leading)
            Text(value.isEmpty ? "No data" : value).textSelection(.enabled)
            Spacer()
        }
    }
}

struct ProfileManagerPane: View {
    @Environment(\.dismiss) private var dismiss
    @ObservedObject private var model = WorkspaceModel.shared
    @State private var name = ""
    @State private var error: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("Manage profiles").font(.title2.weight(.semibold))
            Picker("Current profile", selection: Binding(get: { model.profile }, set: model.selectProfile)) {
                ForEach(model.profiles, id: \.self) { Text($0).tag($0) }
            }
            TextField("Profile name", text: $name).textFieldStyle(.roundedBorder)
            HStack {
                Button("Create Empty") { perform { ConfigManager.shared.createProfile(name: name) } }
                Button("Duplicate Current") { perform { ConfigManager.shared.duplicateProfile(source: model.profile, as: name) } }
                Button("Rename") { perform { ConfigManager.shared.renameProfile(from: model.profile, to: name) } }
            }.disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            Divider()
            HStack {
                Button("Delete Profile…") {
                    let alert = NSAlert()
                    alert.messageText = "Delete \(model.profile)?"
                    alert.informativeText = "This profile's assignments will be deleted."
                    alert.addButton(withTitle: "Delete")
                    alert.addButton(withTitle: "Cancel")
                    if alert.runModal() == .alertFirstButtonReturn {
                        perform { ConfigManager.shared.deleteProfile(named: model.profile) }
                    }
                }.disabled(model.profiles.count <= 1)
                Spacer()
                Button("Done") { dismiss() }.keyboardShortcut(.defaultAction)
            }
            if let message = error ?? model.error { Text(message).foregroundStyle(.red).font(.callout) }
        }.padding(24).frame(width: 520)
    }

    private func perform(_ action: () -> Bool) {
        guard action() else { error = "The operation failed. Choose a different, non-empty name."; return }
        name = ""
        error = nil
        model.refresh()
    }
}
