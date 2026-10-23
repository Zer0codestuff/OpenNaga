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
    @Published var hypershiftMapping: [Int: ActionType] = [:]
    @Published var layer: MappingLayer = .standard
    @Published var onboardActive = false
    @Published var onboardOnly = false
    @Published var onboardName: String?
    @Published var remappingActive = false
    @Published var remappingEnabled = false
    @Published var connected = false
    @Published var permissionsGranted = false
    @Published var transport: String?
    @Published var deviceName = "No mouse detected"
    @Published var error: String?
    @Published var revision = 0
    @Published var activeButton: Int?
    @Published var autoSwitch = false
    @Published var appBindings: [String: String] = [:]
    /// The profile whose mapping is live; differs from `profile` while an app binding applies.
    @Published var runtimeProfile = ""
    @Published var frontmostApp: String?
    private var subscriptions = Set<AnyCancellable>()

    private init() {
        let names = [
            ConfigManager.didChangeNotification, HIDListener.didUpdateNotification,
            RazerDeviceController.didUpdateNotification,
            NSApplication.didBecomeActiveNotification,
            EventTapManager.didUpdateNotification,
            PermissionManager.didUpdateNotification,
            AppProfileSwitcher.didUpdateNotification
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
        hypershiftMapping = config.mappingForCurrentProfile(layer: .hypershift)
        error = config.lastError
        deviceName = HIDListener.shared.connectedDeviceName ?? "No mouse detected"
        onboardActive = OnboardProfileStore.isActive
        onboardOnly = config.currentProfileIsOnboardOnly
        onboardName = OnboardProfileStore.savedName
        remappingEnabled = config.getRemappingEnabled() && !onboardActive
        connected = HIDListener.shared.connectedDeviceName != nil
        transport = HIDListener.shared.transport
        permissionsGranted = PermissionManager.shared.hasAccessibilityPermission() && PermissionManager.shared.hasInputMonitoringPermission()
        remappingActive = EventTapManager.shared.isRunning && EventTapManager.shared.isRemappingEnabled
        autoSwitch = config.autoSwitchEnabled
        appBindings = config.appProfiles
        runtimeProfile = config.runtimeProfileName
        frontmostApp = AppProfileSwitcher.shared.frontmostName
        revision += 1
    }

    var serviceStatus: String {
        if onboardActive { return "Mouse memory: \(onboardName ?? "restore needed")" }
        if onboardOnly { return "Imported profile: save to mouse" }
        if !remappingEnabled { return "Remapping paused" }
        if !permissionsGranted { return "Permissions needed" }
        if !remappingActive { return "Service unavailable" }
        if !connected { return "Waiting for the mouse" }
        return "Remapping active"
    }

    func setRemapping(_ value: Bool) {
        guard !onboardActive, !onboardOnly, !RazerDeviceController.shared.isBusy else { return }
        ConfigManager.shared.setRemappingEnabled(value)
        if permissionsGranted { EventTapManager.shared.isRemappingEnabled = value }
        refresh()
    }

    func save(_ action: ActionType?, button: Int, layer: MappingLayer = .standard) {
        ConfigManager.shared.setAction(forButton: button, action: action, layer: layer)
        refresh()
    }

    func action(for button: Int, layer: MappingLayer) -> ActionType? {
        layer == .standard ? mapping[button] : hypershiftMapping[button]
    }

    func isHypershiftKey(_ button: Int) -> Bool {
        if case .hypershift? = mapping[button] { return true }
        return false
    }

    var hasHypershiftKey: Bool { mapping.keys.contains { isHypershiftKey($0) } }

    /// The text the assignment list shows for a control in a layer.
    func summary(for button: Int, layer: MappingLayer) -> String {
        let standard = mapping[button]?.displayName ?? "Original"
        guard layer == .hypershift else { return standard }
        if isHypershiftKey(button) { return "Hypershift key" }
        return hypershiftMapping[button]?.displayName ?? "Standard: \(standard)"
    }

    func setAutoSwitch(_ enabled: Bool) {
        ConfigManager.shared.setAutoSwitchEnabled(enabled)
        refresh()
    }

    func bind(_ bundleID: String, to profile: String?) {
        ConfigManager.shared.setAppProfile(profile, forBundleIdentifier: bundleID)
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
