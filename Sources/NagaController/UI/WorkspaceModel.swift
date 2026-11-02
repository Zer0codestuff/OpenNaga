import Cocoa
import Combine
import UniformTypeIdentifiers

@MainActor
final class WorkspaceModel: ObservableObject {
    static let shared = WorkspaceModel()
    static let restartInputNotification = Notification.Name("NagaRestartInputService")
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
    @Published var profilesReadOnly = false
    @Published var canUndo = false
    @Published var canRedo = false
    @Published var activeMacros = 0
    private var subscriptions = Set<AnyCancellable>()

    private init() {
        let names = [
            ConfigManager.didChangeNotification, HIDListener.didUpdateNotification,
            RazerDeviceController.didUpdateNotification,
            NSApplication.didBecomeActiveNotification,
            EventTapManager.didUpdateNotification,
            PermissionManager.didUpdateNotification,
            AppProfileSwitcher.didUpdateNotification, ButtonMapper.didUpdateNotification
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
        error = config.lastError ?? ButtonMapper.shared.lastActionError
        profilesReadOnly = config.profilesReadOnly
        canUndo = config.canUndo
        canRedo = config.canRedo
        activeMacros = ButtonMapper.shared.activeMacroButtons.count
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
        if profilesReadOnly { return "Profiles need recovery" }
        if onboardActive { return "Mouse memory: \(onboardName ?? "restore needed")" }
        if onboardOnly { return "Imported profile: save to mouse" }
        if !remappingEnabled { return "Remapping paused" }
        if !permissionsGranted { return "Permissions needed" }
        if !remappingActive { return "Service unavailable" }
        if !connected { return "Waiting for the mouse" }
        return "Remapping active"
    }

    func setRemapping(_ value: Bool) {
        guard !profilesReadOnly, !onboardActive, !onboardOnly, !RazerDeviceController.shared.isBusy else { return }
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
        do {
            let config = ConfigManager.shared
            let document = try ProfileImportDocument(url: url)
            let matches = document.matchingNames(in: config.profiles)
            let alert = NSAlert()
            alert.messageText = "Import \(document.file.profiles.count) profiles?"
            var detail = "Existing profiles and your current selection are preserved by default."
            if !matches.isEmpty { detail += "\n\nMatching names: \(matches.prefix(6).joined(separator: ", "))\(matches.count > 6 ? " and more" : "")." }
            if document.shellCommands > 0 {
                detail += "\n\nContains \(document.shellCommands) shell \(document.shellCommands == 1 ? "command" : "commands"). Review their actions before using the imported profiles. Commands run with your account's access when a mapped button is pressed."
            }
            alert.informativeText = detail
            alert.addButton(withTitle: matches.isEmpty ? "Import" : "Keep Both")
            alert.addButton(withTitle: "Cancel")
            if !matches.isEmpty { alert.addButton(withTitle: "Replace Matching…") }
            let response = alert.runModal()
            guard response != .alertSecondButtonReturn else { return }
            let policy: ProfileImportPolicy = response == .alertThirdButtonReturn ? .replaceMatching : .keepBoth
            if policy == .replaceMatching {
                let replace = NSAlert()
                replace.messageText = "Replace \(matches.count) matching profiles?"
                replace.informativeText = "Their assignments will be replaced. Mouse-import protection is preserved. Undo Profile Change can restore the previous document."
                replace.addButton(withTitle: "Replace"); replace.addButton(withTitle: "Cancel")
                guard replace.runModal() == .alertFirstButtonReturn else { return }
            }
            try config.importProfiles(document, policy: policy)
            refresh()
        }
        catch { self.error = error.localizedDescription }
    }

    func undoProfileChange() { _ = ConfigManager.shared.undo(); refresh() }
    func redoProfileChange() { _ = ConfigManager.shared.redo(); refresh() }
    func reloadProfiles() { ConfigManager.shared.load(); refresh() }
    func showProfilesFolder() { NSWorkspace.shared.open(ConfigManager.shared.profilesFolder) }
    func cancelMacros() { ButtonMapper.shared.cancelMacros(); refresh() }
    func restartInputService() {
        guard permissionsGranted else { error = "Grant Accessibility and Input Monitoring before restarting input."; return }
        guard !RazerDeviceController.shared.isBusy else { error = "Wait for the mouse operation to finish before restarting input."; return }
        NotificationCenter.default.post(name: Self.restartInputNotification, object: nil)
    }
    func restoreProfileBackup() {
        let alert = NSAlert()
        alert.messageText = "Restore the previous profile save?"
        alert.informativeText = "The current file will be preserved separately, including unreadable data. This restores local profiles only; mouse memory is unchanged."
        alert.addButton(withTitle: "Restore Backup"); alert.addButton(withTitle: "Cancel")
        guard alert.runModal() == .alertFirstButtonReturn else { return }
        _ = ConfigManager.shared.restoreProfileBackup()
        refresh()
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
