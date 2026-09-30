import Cocoa
import SwiftUI
import Combine
import UniformTypeIdentifiers

final class MappingViewController: NSHostingController<NagaWorkspace> {
    init() { super.init(rootView: NagaWorkspace()) }
    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is unavailable") }
}

enum WorkspaceSection: String, CaseIterable {
    case buttons = "Buttons"
    case sensitivity = "Sensitivity"
    case status = "Status"

    var symbol: String {
        switch self {
        case .buttons: return "square.grid.3x3"
        case .sensitivity: return "speedometer"
        case .status: return "info.circle"
        }
    }
}

struct NagaWorkspace: View {
    @ObservedObject private var model = WorkspaceModel.shared
    @State private var selectedButton = 1
    @State private var manageProfiles = false
    @State private var showOnboard = false

    var body: some View {
        HStack(spacing: 0) {
            sidebar
            Divider()
            VStack(spacing: 0) {
                toolbar
                Divider()
                if !model.permissionsGranted {
                    HStack(spacing: 10) {
                        Image(systemName: "hand.raised")
                        Text("Grant the permissions to use your assignments.")
                        Spacer()
                        Button("Set Up") { model.section = .status }
                    }.font(.callout).padding(.horizontal, 24).padding(.vertical, 10)
                        .background(UIStyle.inset)
                    Divider()
                }
                if let error = model.error {
                    Label(error, systemImage: "exclamationmark.triangle")
                        .foregroundStyle(.red).font(.callout).padding(12)
                }
                Group {
                    switch model.section {
                    case .buttons:
                        HStack(spacing: 0) {
                            MouseWorkspace(selectedButton: $selectedButton)
                                .frame(minWidth: 390, maxWidth: .infinity, maxHeight: .infinity)
                            Divider()
                            ScrollView {
                                ActionInspector(button: selectedButton)
                                    .id("\(model.profile)-\(selectedButton)")
                                    .padding(24)
                            }.frame(width: 340)
                                .background(.background.opacity(0.45))
                        }
                    case .sensitivity: SensitivityPane()
                    case .status: StatusPane()
                    }
                }.frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                Divider()
                HStack(spacing: 8) {
                    StatusDot(active: model.connected && model.remappingActive)
                    Text(model.serviceStatus)
                    Spacer()
                    Image(systemName: model.onboardActive ? "memorychip" : "menubar.rectangle")
                    Text(model.onboardActive ? (model.onboardName == nil ? "Mouse restore needed" : "Works even after Quit") : "Keeps running when you close the window")
                }.font(.system(size: 11)).foregroundStyle(.secondary)
                    .padding(.horizontal, 20).frame(height: 32)
            }
        }
        .background {
            if CommandLine.arguments.contains("--snapshot") { Color(nsColor: .windowBackgroundColor) }
        }
        .tint(UIStyle.accent)
        .sheet(isPresented: $showOnboard) { OnboardProfilePane() }
        .sheet(isPresented: $manageProfiles) { ProfileManagerPane() }
        .onAppear {
            model.refresh()
            let args = CommandLine.arguments
            if args.contains("--snapshot"), let index = args.firstIndex(of: "--snapshot-button"),
               args.indices.contains(index + 1), let button = Int(args[index + 1]), (1...19).contains(button) {
                selectedButton = button
            }
        }
    }

    private var sidebar: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 10) {
                Image(systemName: "computermouse.fill").font(.system(size: 24, weight: .light))
                    .foregroundStyle(UIStyle.accent)
                Text("OpenNaga").font(.system(size: 22, weight: .semibold))
            }.padding(.horizontal, 20).padding(.top, 22).padding(.bottom, 34)
            VStack(spacing: 4) {
                ForEach(WorkspaceSection.allCases, id: \.self) { section in
                    Button { model.section = section } label: {
                        HStack(spacing: 10) {
                            Image(systemName: section.symbol).frame(width: 18)
                            Text(section.rawValue).fontWeight(model.section == section ? .medium : .regular)
                            Spacer()
                        }.padding(.horizontal, 12).frame(height: 36)
                            .foregroundStyle(model.section == section ? UIStyle.accent : .primary)
                            .background(model.section == section ? UIStyle.selection : .clear)
                            .clipShape(RoundedRectangle(cornerRadius: 7))
                    }.buttonStyle(.plain)
                        .accessibilityAddTraits(model.section == section ? [.isSelected] : [])
                }
            }.padding(.horizontal, 10)
            Spacer()
            VStack(alignment: .leading, spacing: 14) {
                VStack(alignment: .leading, spacing: 5) {
                    Text("Naga V2 HyperSpeed").font(.system(size: 12, weight: .medium))
                    HStack(spacing: 6) {
                        StatusDot(active: model.connected)
                        Text(model.connected ? model.transport ?? "Connected" : "Mouse disconnected")
                    }.font(.caption).foregroundStyle(.secondary)
                }
                Divider()
                Toggle("Remapping", isOn: Binding(
                    get: { model.remappingEnabled }, set: model.setRemapping
                )).toggleStyle(.switch).controlSize(.small).font(.system(size: 12))
                    .disabled(model.onboardActive || RazerDeviceController.shared.isBusy)
                if model.onboardActive { Text("Saved in mouse").font(.caption).foregroundStyle(.secondary) }
            }.padding(18)
        }.frame(width: 184).frame(maxHeight: .infinity)
            .background(SidebarMaterial())
    }

    private var toolbar: some View {
        HStack(spacing: 16) {
            VStack(alignment: .leading, spacing: 4) {
                Text(model.section.rawValue).font(.system(size: 22, weight: .semibold))
                Text(model.section == .buttons ? "Your mouse, your assignments." : model.section == .sensitivity ? "Sensor and hardware controls" : "Connection and service")
                    .font(.callout).foregroundStyle(.secondary)
            }
            Spacer()
            Picker("Profile", selection: Binding(get: { model.profile }, set: model.selectProfile)) {
                ForEach(model.profiles, id: \.self) { Text($0).tag($0) }
            }.frame(width: 205)
            Button("Save to Mouse…") { showOnboard = true }
            Menu {
                Button("Mouse Memory…") { showOnboard = true }
                Divider()
                Button("Manage Profiles…") { manageProfiles = true }
                Divider()
                Button("Import Profiles…") { model.importProfiles() }
                Button("Export Profiles…") { model.exportProfiles() }
            } label: { Image(systemName: "ellipsis.circle").font(.system(size: 17)) }
            .menuStyle(.borderlessButton).frame(width: 24).help("Manage, import or export profiles")
        }.padding(.horizontal, 24).frame(height: 88)
    }
}

func buttonName(_ index: Int) -> String {
    switch index {
    case 13: return "DPI up"
    case 14: return "DPI down"
    case 15: return "Wheel tilt left"
    case 16: return "Wheel tilt right"
    case 17: return "Middle click"
    case 18: return "Left click"
    case 19: return "Right click"
    default: return "Side button \(index)"
    }
}
