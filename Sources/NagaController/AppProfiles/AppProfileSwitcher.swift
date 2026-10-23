import Cocoa

/// Chooses the profile that is live while an app is frontmost.
enum AppProfileResolver {
    /// - Returns: the bound profile, or `selected` when no binding applies.
    ///   Nothing applies when automatic switching is off, the mouse memory
    ///   profile is active, OpenNaga itself is frontmost (so what the user
    ///   edits is what is live), the app has no binding, or either profile is
    ///   missing or imported from the mouse (those never remap in software).
    static func resolve(bundleID: String?, ownBundleID: String?, selected: String, autoSwitch: Bool,
                        mouseMemoryActive: Bool, bindings: [String: String], profiles: [String: Profile]) -> String {
        guard autoSwitch, !mouseMemoryActive, let bundleID, bundleID != ownBundleID,
              profiles[selected]?.onboardOnly != true,
              let bound = bindings[bundleID], let target = profiles[bound], target.onboardOnly != true
        else { return selected }
        return bound
    }
}

/// Follows the frontmost app and loads the profile bound to it. Main thread
/// only, like the event tap callback, so swapping the mapping needs no lock.
final class AppProfileSwitcher {
    static let shared = AppProfileSwitcher()
    static let didUpdateNotification = Notification.Name("NagaAppProfileDidUpdate")

    private let config: ConfigManager
    private let ownBundleID: String?
    private let mouseMemoryActive: () -> Bool
    private var observers: [(center: NotificationCenter, token: NSObjectProtocol)] = []
    private var settingsSignature: SettingsSignature?

    private struct SettingsSignature: Equatable {
        let autoSwitch: Bool
        let bindings: [String: String]
    }

    private(set) var frontmostBundleID: String?
    private(set) var frontmostName: String?
    var isRunning: Bool { !observers.isEmpty }

    init(config: ConfigManager = .shared, ownBundleID: String? = Bundle.main.bundleIdentifier,
         mouseMemoryActive: @escaping () -> Bool = { OnboardProfileStore.isActive }) {
        self.config = config
        self.ownBundleID = ownBundleID
        self.mouseMemoryActive = mouseMemoryActive
        self.settingsSignature = SettingsSignature(autoSwitch: config.autoSwitchEnabled, bindings: config.appProfiles)
    }

    func start() {
        guard !isRunning else { return }
        // Workspace notifications use their own center, not the default one.
        let workspace = NSWorkspace.shared.notificationCenter
        observers.append((workspace, workspace.addObserver(forName: NSWorkspace.didActivateApplicationNotification, object: nil, queue: .main) { [weak self] note in
            let app = note.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication
            self?.handleActivation(bundleID: app?.bundleIdentifier, name: app?.localizedName, regular: app?.activationPolicy == .regular)
        }))
        let center = NotificationCenter.default
        observers.append((center, center.addObserver(forName: ConfigManager.didChangeNotification, object: nil, queue: .main) { [weak self] _ in
            self?.configDidChange()
        }))
        settingsSignature = signature()
        let app = NSWorkspace.shared.frontmostApplication
        handleActivation(bundleID: app?.bundleIdentifier, name: app?.localizedName, regular: app?.activationPolicy == .regular)
    }

    func stop() {
        observers.forEach { $0.center.removeObserver($0.token) }
        observers.removeAll()
    }

    /// Apps that are not regular (Spotlight, menu bar helpers, the login
    /// window) never change the profile; the last regular app stays in charge.
    func handleActivation(bundleID: String?, name: String?, regular: Bool) {
        guard regular, bundleID != nil else { return }
        frontmostBundleID = bundleID
        frontmostName = name
        apply()
        NotificationCenter.default.post(name: Self.didUpdateNotification, object: self)
    }

    func configDidChange() {
        // A manual profile choice is not an app change; only a new binding or
        // switching automatic mode on or off re-resolves the frontmost app.
        guard signature() != settingsSignature else { return }
        settingsSignature = signature()
        apply()
        NotificationCenter.default.post(name: Self.didUpdateNotification, object: self)
    }

    private func signature() -> SettingsSignature {
        SettingsSignature(autoSwitch: config.autoSwitchEnabled, bindings: config.appProfiles)
    }

    private func apply() {
        let name = AppProfileResolver.resolve(
            bundleID: frontmostBundleID, ownBundleID: ownBundleID, selected: config.currentProfileName,
            autoSwitch: config.autoSwitchEnabled, mouseMemoryActive: mouseMemoryActive(),
            bindings: config.appProfiles, profiles: config.profiles)
        config.applyAutomaticProfile(name)
    }
}
