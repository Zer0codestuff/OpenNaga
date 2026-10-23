import Cocoa
import SwiftUI
import UniformTypeIdentifiers

/// Binds profiles to apps. The live mapping follows the frontmost app.
struct AppProfilesPane: View {
    @ObservedObject private var model = WorkspaceModel.shared
    @State private var adding = false

    private var boundIDs: [String] {
        model.appBindings.keys.sorted { label(for: $0).localizedCaseInsensitiveCompare(label(for: $1)) == .orderedAscending }
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                Text("App profiles").font(.title2.weight(.semibold))
                Text("Use a different profile while a given app is in front. The profile is chosen by OpenNaga on this Mac, so it works over Bluetooth and nothing is written to the mouse.")
                    .foregroundStyle(.secondary)
                GroupBox {
                    VStack(alignment: .leading, spacing: 12) {
                        Toggle("Switch profiles by app", isOn: Binding(get: { model.autoSwitch }, set: model.setAutoSwitch))
                            .toggleStyle(.switch)
                        if model.onboardActive || model.onboardOnly {
                            Text("Switching is paused while a profile saved in the mouse or imported from it is active.")
                                .font(.caption).foregroundStyle(.secondary)
                        } else if model.autoSwitch {
                            HStack(spacing: 7) {
                                StatusDot(active: model.remappingActive)
                                Text(liveStatus)
                            }.font(.caption).foregroundStyle(.secondary)
                        }
                        Divider()
                        HStack {
                            Text("All other apps use")
                            Spacer()
                            Text(model.profile).fontWeight(.medium)
                        }
                        Text("Pick this profile in the menu bar or in the Buttons section. A profile you choose by hand applies until you switch to another app.")
                            .font(.caption).foregroundStyle(.secondary)
                    }.padding(12)
                }
                GroupBox {
                    VStack(alignment: .leading, spacing: 0) {
                        if boundIDs.isEmpty {
                            Text("No apps yet. Add an app to give it its own profile.")
                                .foregroundStyle(.secondary).padding(12)
                        }
                        ForEach(boundIDs, id: \.self) { bundleID in
                            if bundleID != boundIDs.first { Divider() }
                            AppBindingRow(bundleID: bundleID)
                        }
                    }
                }.opacity(model.autoSwitch ? 1 : 0.6)
                HStack {
                    Button("Add Application…") { adding = true }
                    Spacer()
                }
            }.padding(24)
        }
        .sheet(isPresented: $adding) { AppPickerSheet() }
    }

    private var liveStatus: String {
        guard let app = model.frontmostApp else { return "Waiting for the first app to come to the front" }
        return model.runtimeProfile == model.profile
            ? "\(app) is in front, using \(model.runtimeProfile)"
            : "\(app) is in front, using \(model.runtimeProfile) (automatic)"
    }

    private func label(for bundleID: String) -> String { AppIdentity.name(for: bundleID) }
}

enum AppIdentity {
    static func url(for bundleID: String) -> URL? { NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleID) }

    static func name(for bundleID: String) -> String {
        url(for: bundleID).flatMap(InstalledAppCatalog.app(at:))?.name ?? bundleID
    }

    static func icon(for bundleID: String) -> NSImage {
        url(for: bundleID).map { NSWorkspace.shared.icon(forFile: $0.path) } ?? NSWorkspace.shared.icon(for: .applicationBundle)
    }
}

private struct AppBindingRow: View {
    let bundleID: String
    @ObservedObject private var model = WorkspaceModel.shared

    var body: some View {
        HStack(spacing: 12) {
            Image(nsImage: AppIdentity.icon(for: bundleID)).resizable().frame(width: 32, height: 32)
            VStack(alignment: .leading, spacing: 2) {
                Text(AppIdentity.name(for: bundleID)).fontWeight(.medium)
                Text(bundleID).font(.caption).foregroundStyle(.secondary).textSelection(.enabled)
            }
            Spacer()
            Picker("Profile", selection: Binding(get: { model.appBindings[bundleID] ?? "" }, set: { model.bind(bundleID, to: $0) })) {
                ForEach(assignableProfiles(), id: \.self) { Text($0).tag($0) }
            }.labelsHidden().frame(width: 170)
            Button { model.bind(bundleID, to: nil) } label: { Image(systemName: "minus.circle") }
                .buttonStyle(.borderless).help("Remove this app")
                .accessibilityLabel("Remove \(AppIdentity.name(for: bundleID))")
        }.padding(12)
    }
}

/// Profiles imported from the mouse never remap in software, so they cannot be bound.
private func assignableProfiles() -> [String] {
    ConfigManager.shared.profiles.filter { $0.value.onboardOnly != true }.keys.sorted()
}

private struct AppPickerSheet: View {
    @Environment(\.dismiss) private var dismiss
    @ObservedObject private var model = WorkspaceModel.shared
    @State private var apps: [InstalledApp] = []
    @State private var loading = true
    @State private var search = ""
    @State private var chosen: InstalledApp?
    @State private var profile = ""
    @State private var running: Set<String> = []
    @State private var notice: String?

    private var visible: [InstalledApp] {
        let query = search.trimmingCharacters(in: .whitespacesAndNewlines)
        return query.isEmpty ? apps : apps.filter {
            $0.name.localizedCaseInsensitiveContains(query) || $0.bundleID.localizedCaseInsensitiveContains(query)
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Add application").font(.title2.weight(.semibold))
            TextField("Search apps", text: $search).textFieldStyle(.roundedBorder)
            ScrollViewReader { proxy in
            List(selection: Binding(get: { chosen?.id }, set: { id in chosen = apps.first { $0.id == id } })) {
                ForEach(visible) { app in
                    HStack(spacing: 10) {
                        Image(nsImage: NSWorkspace.shared.icon(forFile: app.path)).resizable().frame(width: 24, height: 24)
                        Text(app.name)
                        Spacer()
                        if model.appBindings[app.bundleID] != nil {
                            Text("Already added").font(.caption).foregroundStyle(.secondary)
                        } else if running.contains(app.bundleID) {
                            Text("Running").font(.caption).foregroundStyle(UIStyle.accent)
                        }
                    }.tag(app.id)
                }
            }.frame(height: 280).overlay {
                if loading { ProgressView() }
                else if visible.isEmpty { Text("No apps found").foregroundStyle(.secondary) }
            }.onChange(of: chosen?.id) { id in
                if let id { proxy.scrollTo(id, anchor: .center) }
            }
            }
            HStack {
                Button("Choose Other…", action: chooseOther)
                Spacer()
                if let chosen { Text(chosen.name).foregroundStyle(.secondary).lineLimit(1) }
            }
            if let notice {
                Text(notice).font(.caption).foregroundStyle(.orange).fixedSize(horizontal: false, vertical: true)
            }
            Picker("Profile", selection: $profile) {
                ForEach(assignableProfiles(), id: \.self) { Text($0).tag($0) }
            }
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Add") {
                    if let chosen { model.bind(chosen.bundleID, to: profile) }
                    dismiss()
                }.keyboardShortcut(.defaultAction).disabled(chosen == nil || profile.isEmpty)
            }
        }.padding(24).frame(width: 480)
            .task {
                let choices = assignableProfiles()
                profile = choices.contains(model.profile) ? model.profile : choices.first ?? ""
                apps = await InstalledAppCatalog.load()
                running = Set(InstalledAppCatalog.runningApps().map(\.bundleID))
                loading = false
            }
    }

    private func chooseOther() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.applicationBundle]
        panel.canChooseDirectories = false
        panel.directoryURL = URL(fileURLWithPath: "/Applications")
        guard panel.runModal() == .OK, let url = panel.url else { return }
        guard let app = InstalledAppCatalog.app(at: url) else {
            // A launcher such as a Steam shortcut is a script in an app wrapper. It never comes to
            // the front itself, and there is no identifier to bind a profile to.
            notice = "\(url.deletingPathExtension().lastPathComponent) cannot be added: it has no bundle identifier, "
                + "so OpenNaga cannot tell when it is in front. This is typical of Steam shortcuts. "
                + "Start the game and pick it from the list, where running apps are marked."
            return
        }
        notice = nil
        search = ""
        if !apps.contains(where: { $0.bundleID == app.bundleID }) { apps = InstalledAppCatalog.merged(apps + [app]) }
        chosen = app
    }
}
