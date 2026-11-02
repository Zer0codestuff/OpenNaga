import SwiftUI

struct ProfileRecoveryPane: View {
    @ObservedObject private var model = WorkspaceModel.shared
    var body: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 12) {
                Text("Profiles and recovery").font(.headline)
                Text(model.profilesReadOnly
                     ? "Editing and software remapping are paused to protect an unreadable file. Reload a repaired file or restore the previous save."
                     : "Each successful change keeps the previous valid save. Undo and redo keep up to 12 changes during this session.")
                    .font(.callout).foregroundStyle(.secondary)
                HStack {
                    Button("Undo", action: model.undoProfileChange).disabled(!model.canUndo)
                    Button("Redo", action: model.redoProfileChange).disabled(!model.canRedo)
                    Spacer()
                    Button("Show Folder", action: model.showProfilesFolder)
                    Button("Reload Profiles", action: model.reloadProfiles)
                    Button("Restore Backup…", action: model.restoreProfileBackup)
                        .disabled(!ConfigManager.shared.hasProfileBackup)
                }
            }.padding(12).frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}
