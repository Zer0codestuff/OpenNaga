import Cocoa

struct InstalledApp: Identifiable, Hashable {
    let bundleID: String
    let name: String
    let path: String
    var id: String { bundleID }
}

/// Finds the apps a profile can be bound to.
enum InstalledAppCatalog {
    /// Steam installs each game as an app inside its own folder here. The
    /// shortcuts Steam puts in ~/Applications are scripts with no bundle
    /// identifier, so these are the apps that actually come to the front.
    static let steamLibrary = "/Library/Application Support/Steam/steamapps/common"

    static var defaultRoots: [URL] {
        ["/Applications", "/System/Applications", "/System/Applications/Utilities", "/Applications/Utilities",
         NSHomeDirectory() + "/Applications", NSHomeDirectory() + steamLibrary]
            .map { URL(fileURLWithPath: $0, isDirectory: true) }
    }

    /// The app bundle at `url`, or nil when it has no bundle identifier.
    static func app(at url: URL) -> InstalledApp? {
        guard let bundle = Bundle(url: url), let bundleID = bundle.bundleIdentifier, !bundleID.isEmpty else { return nil }
        let info = bundle.infoDictionary ?? [:]
        let name = (info["CFBundleDisplayName"] as? String).flatMap { $0.isEmpty ? nil : $0 }
            ?? (info["CFBundleName"] as? String).flatMap { $0.isEmpty ? nil : $0 }
            ?? url.deletingPathExtension().lastPathComponent
        return InstalledApp(bundleID: bundleID, name: name, path: url.path)
    }

    /// Apps found under `roots`, looking one folder level down for suites and
    /// Utilities. Background-only and menu bar agents are left out: they never
    /// become the frontmost app, so a binding would never apply.
    static func scan(roots: [URL] = defaultRoots, fileManager: FileManager = .default) -> [InstalledApp] {
        var found: [InstalledApp] = []
        func visit(_ folder: URL, depth: Int) {
            let entries = (try? fileManager.contentsOfDirectory(at: folder, includingPropertiesForKeys: [.isDirectoryKey],
                                                                options: [.skipsHiddenFiles])) ?? []
            for url in entries {
                if url.pathExtension == "app" {
                    guard let app = app(at: url), !isBackgroundOnly(url) else { continue }
                    found.append(app)
                } else if depth > 0, (try? url.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true {
                    visit(url, depth: depth - 1)
                }
            }
        }
        roots.forEach { visit($0, depth: 1) }
        return merged(found)
    }

    static func runningApps(_ workspace: NSWorkspace = .shared) -> [InstalledApp] {
        merged(workspace.runningApplications.compactMap { running in
            guard running.activationPolicy == .regular, let url = running.bundleURL else { return nil }
            return app(at: url)
        })
    }

    /// Installed plus running apps, one entry per bundle identifier, sorted by name.
    static func load() async -> [InstalledApp] {
        let installed = await Task.detached(priority: .userInitiated) { scan() }.value
        return merged(installed + runningApps())
    }

    static func merged(_ apps: [InstalledApp]) -> [InstalledApp] {
        var seen = Set<String>()
        return apps.filter { seen.insert($0.bundleID).inserted }
            .sorted { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
    }

    private static func isBackgroundOnly(_ url: URL) -> Bool {
        let info = Bundle(url: url)?.infoDictionary ?? [:]
        func flag(_ key: String) -> Bool { (info[key] as? Bool) == true || (info[key] as? String) == "1" }
        return flag("LSBackgroundOnly") || flag("LSUIElement")
    }
}
