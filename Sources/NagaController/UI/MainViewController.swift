import Cocoa
import SwiftUI

final class MainViewController: NSViewController {
    override func loadView() {
        let host = NSHostingController(rootView: NagaPopover())
        addChild(host)
        view = host.view
        preferredContentSize = NSSize(width: 320, height: 336)
    }

    func refreshPermissionStatuses() { WorkspaceModel.shared.refresh() }
}

private struct NagaPopover: View {
    @ObservedObject private var model = WorkspaceModel.shared

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(spacing: 10) {
                Image(systemName: "computermouse.fill").font(.title2).foregroundStyle(UIStyle.accent)
                VStack(alignment: .leading, spacing: 3) {
                    Text("OpenNaga").font(.headline)
                    Text(model.connected ? "Naga V2 HyperSpeed" : "Mouse disconnected")
                        .font(.caption).foregroundStyle(.secondary)
                }
            }
            Picker("Profile", selection: Binding(get: { model.profile }, set: model.selectProfile)) {
                ForEach(model.profiles, id: \.self) { Text($0).tag($0) }
            }
            Toggle("Remapping", isOn: Binding(
                get: { model.remappingEnabled }, set: model.setRemapping
            )).toggleStyle(.switch).controlSize(.small).disabled(model.onboardActive || model.onboardOnly || RazerDeviceController.shared.isBusy)
            VStack(alignment: .leading, spacing: 4) {
                Toggle("Switch profiles by app", isOn: Binding(get: { model.autoSwitch }, set: model.setAutoSwitch))
                    .toggleStyle(.switch).controlSize(.small)
                if model.autoSwitch, model.runtimeProfile != model.profile {
                    Text("Using \(model.runtimeProfile) for \(model.frontmostApp ?? "this app")")
                        .font(.caption).foregroundStyle(.secondary).lineLimit(1)
                }
            }
            HStack(spacing: 7) {
                StatusDot(active: model.connected && model.remappingActive)
                Text(model.serviceStatus)
            }.font(.caption).foregroundStyle(.secondary)
            Divider()
            Button("Configure Mouse…") { MappingWindowController.shared.show() }
                .frame(maxWidth: .infinity, alignment: .leading)
            HStack {
                Button("Status and Permissions…") {
                    model.section = .status
                    MappingWindowController.shared.show()
                }
                Spacer()
                Button("Quit") { NSApp.terminate(nil) }
            }.font(.callout)
        }.padding(20).frame(width: 320, height: 336).tint(UIStyle.accent)
            .onAppear { model.refresh() }
    }
}
