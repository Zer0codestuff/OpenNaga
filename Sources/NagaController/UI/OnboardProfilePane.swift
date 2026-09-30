import SwiftUI

struct OnboardProfilePane: View {
    @Environment(\.dismiss) private var dismiss
    @ObservedObject private var model = WorkspaceModel.shared
    private var device: RazerDeviceController { .shared }
    private var plan: OnboardProfilePlan { .init(name: model.profile, mapping: model.mapping) }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Text("Mouse Memory").font(.title2.weight(.semibold))
            Text("Save a profile to the Naga V2 HyperSpeed so it works even after you quit the app.")
                .foregroundStyle(.secondary)
            LabeledContent("Profile to save", value: model.profile)
            if model.onboardActive {
                LabeledContent("In the mouse", value: model.onboardName ?? "Restore needed")
            }
            Text("The mouse itself runs keys, shortcuts, media controls and clicks. Buttons 4 and 5 stay standard mouse buttons. Editor changes and profile switches need a new save.")
                .font(.callout).foregroundStyle(.secondary)
            if !plan.issues.isEmpty {
                ScrollView {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Fix before saving").font(.headline)
                        ForEach(plan.issues, id: \.self) { Text($0).font(.callout) }
                    }.frame(maxWidth: .infinity, alignment: .leading)
                }.frame(maxHeight: 150)
            }
            if device.driverModeEnabled || device.recoveryPending {
                Text("Turn off or restore driver mode in Sensitivity before saving.").font(.callout)
            }
            if !device.isConnected { Text("Connect the USB receiver and switch the mouse to 2.4 GHz mode.").font(.callout) }
            Text("Saving keeps a backup and pauses software remapping. Key repeat and long presses follow the mouse's own behavior. Restoring needs the same receiver.")
                .font(.caption).foregroundStyle(.secondary)
            Text(device.statusMessage).font(.callout).foregroundStyle(.secondary).textSelection(.enabled)
            Divider()
            HStack {
                Button("Restore Previous Assignments") { device.restoreOnboard() }
                    .disabled(!model.onboardActive || !device.isConnected || device.isBusy)
                Spacer()
                Button("Refresh") { device.refresh() }.disabled(device.isBusy)
            }
            HStack {
                Button("Save to Mouse") { device.saveOnboard(plan) }
                    .disabled(!plan.isSupported || !device.isConnected || device.isBusy || device.driverModeEnabled || device.recoveryPending)
                Spacer()
                Button("Done") { dismiss() }.keyboardShortcut(.cancelAction)
            }
        }.padding(24).frame(width: 560)
    }
}
