import Cocoa
import Combine
import UniformTypeIdentifiers

@MainActor
final class WorkspaceModel: ObservableObject {
    static let shared = WorkspaceModel()
    @Published var section: WorkspaceSection = .buttons
    @Published var profile = ""
    @Published var profiles: [String] = []
    @Published var mapping: [Int: ActionType] = [:]
    @Published var remappingActive = false
    @Published var remappingEnabled = false
    @Published var connected = false
    @Published var permissionsGranted = false
    @Published var transport: String?
    @Published var deviceName = "No mouse detected"
    @Published var error: String?
    @Published var revision = 0
    @Published var activeButton: Int?
    private var subscriptions = Set<AnyCancellable>()

    private init() {
        let names = [
            ConfigManager.didChangeNotification, HIDListener.didUpdateNotification,
            RazerDeviceController.didUpdateNotification,
            NSApplication.didBecomeActiveNotification,
            EventTapManager.didUpdateNotification,
            PermissionManager.didUpdateNotification
        ]
        for name in names {
            NotificationCenter.default.publisher(for: name)
                .receive(on: RunLoop.main)
                .sink { [weak self] _ in self?.refresh() }
                .store(in: &subscriptions)
        }
        NotificationCenter.default.publisher(for: Notification.Name("NagaButtonActivity"))
            .receive(on: RunLoop.main)
            .sink { [weak self] note in
                guard let self, let index = note.userInfo?["buttonIndex"] as? Int else { return }
                self.activeButton = index
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.35) { [weak self] in
                    if self?.activeButton == index { self?.activeButton = nil }
                }
            }.store(in: &subscriptions)
        refresh()
    }

    func refresh() {
        let config = ConfigManager.shared
        profile = config.currentProfileName
        profiles = config.availableProfiles()
        mapping = config.mappingForCurrentProfile()
        error = config.lastError
        deviceName = HIDListener.shared.connectedDeviceName ?? "No mouse detected"
        remappingEnabled = config.getRemappingEnabled()
        connected = HIDListener.shared.connectedDeviceName != nil
        transport = HIDListener.shared.transport
        permissionsGranted = PermissionManager.shared.hasAccessibilityPermission() && PermissionManager.shared.hasInputMonitoringPermission()
        remappingActive = EventTapManager.shared.isRunning && EventTapManager.shared.isRemappingEnabled
        revision += 1
    }

    var serviceStatus: String {
        if !remappingEnabled { return "Remapping paused" }
        if !permissionsGranted { return "Permissions needed" }
        if !remappingActive { return "Service unavailable" }
        if !connected { return "Waiting for the mouse" }
        return "Remapping active"
    }

    func setRemapping(_ value: Bool) {
        ConfigManager.shared.setRemappingEnabled(value)
        if permissionsGranted { EventTapManager.shared.isRemappingEnabled = value }
        refresh()
    }

    func save(_ action: ActionType?, button: Int) {
        ConfigManager.shared.setAction(forButton: button, action: action)
        refresh()
    }

    func selectProfile(_ name: String) {
        ConfigManager.shared.setCurrentProfile(name)
        refresh()
    }

    func importProfiles() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.json]
        panel.canChooseDirectories = false
        guard panel.runModal() == .OK, let url = panel.url else { return }
        do { try ConfigManager.shared.importProfiles(from: url); refresh() }
        catch { self.error = error.localizedDescription }
    }

    func exportProfiles() {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.json]
        panel.nameFieldStringValue = "OpenNaga-profiles.json"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        do { try ConfigManager.shared.exportAllProfiles(to: url) }
        catch { self.error = error.localizedDescription }
    }
}

