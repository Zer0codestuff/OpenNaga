import Foundation

enum ProfileImportPolicy: Equatable { case keepBoth, replaceMatching }

struct ProfileImportDocument {
    let file: ProfilesFile
    let shellCommands: Int
    init(url: URL) throws {
        file = try ProfileFileStore.readDocument(at: url)
        shellCommands = file.profiles.values.reduce(0) { total, profile in
            total + (profile.buttons.values.filter { $0.type == "systemCommand" }.count)
                + ((profile.hypershift ?? [:]).values.filter { $0.type == "systemCommand" }.count)
        }
    }
    func matchingNames(in profiles: [String: Profile]) -> [String] {
        file.profiles.keys.filter { profiles[$0] != nil }.sorted()
    }

    /// Rewrite references within the incoming set so keeping both profiles
    /// never makes an imported switch point to an unrelated existing profile.
    func merged(with existing: ProfilesFile, policy: ProfileImportPolicy) -> ProfilesFile {
        var result = existing
        var names: [String: String] = [:]
        var reserved = Set(existing.profiles.keys).union(file.profiles.keys)
        for name in file.profiles.keys.sorted() {
            var unique = name
            if policy == .keepBoth, existing.profiles[name] != nil {
                unique = Self.uniqueName(name, reserved: reserved)
            }
            names[name] = unique; reserved.insert(unique)
        }
        for (name, profile) in file.profiles {
            var updated = profile
            if policy == .replaceMatching, existing.profiles[name]?.onboardOnly == true { updated.onboardOnly = true }
            func rewrite(_ layer: [String: ButtonAction]) -> [String: ButtonAction] {
                layer.mapValues { action in
                    var action = action
                    if action.type == "profileSwitch", let target = action.profile, let renamed = names[target] { action.profile = renamed }
                    return action
                }
            }
            updated.buttons = rewrite(profile.buttons)
            updated.hypershift = profile.hypershift.map(rewrite)
            result.profiles[names[name] ?? name] = updated
        }
        var bindings = result.settings?.appProfiles ?? [:]
        for (bundleID, target) in file.settings?.appProfiles ?? [:] {
            guard let renamed = names[target] else { continue }
            if bindings[bundleID] == nil || policy == .replaceMatching { bindings[bundleID] = renamed }
        }
        result.settings?.appProfiles = bindings.isEmpty ? nil : bindings
        // Merge preserves the current selection and automatic-switching mode.
        // An imported shell command cannot become active just by importing.
        return result
    }

    static func uniqueName(_ base: String, reserved: Set<String>) -> String {
        var suffix = 2
        while true {
            let ending = " \(suffix)"
            var prefix = base
            while prefix.utf8.count + ending.utf8.count > 256 { prefix.removeLast() }
            let candidate = prefix + ending
            if !reserved.contains(candidate) { return candidate }
            suffix += 1
        }
    }
}
