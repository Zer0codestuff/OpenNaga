import Foundation
import Darwin

/// Optimistic concurrency plus an advisory lock prevents two app instances
/// from replacing each other's profiles. No mouse backup is handled here.
final class ProfileFileStore {
    let url: URL
    var backupURL: URL { url.deletingPathExtension().appendingPathExtension("backup.json") }
    private var expectedData: Data?
    private var loaded = false
    private var readFailed = false

    init(url: URL) { self.url = url }

    static func readDocument(at url: URL) throws -> ProfilesFile {
        try decode(readBytes(at: url))
    }

    func load() throws -> ProfilesFile? {
        do {
            guard try Self.itemExists(at: url) else {
                expectedData = nil; loaded = true; readFailed = false
                return nil
            }
            let data = try Self.readBytes(at: url)
            let file = try Self.decode(data)
            expectedData = data; loaded = true; readFailed = false
            return file
        } catch {
            readFailed = true
            throw error
        }
    }

    func save(_ file: ProfilesFile) throws {
        let data = try Self.encode(file)
        try withLock {
            guard !readFailed else {
                throw ProfileDataError(message: "Profiles are protected after a read error. Reload the file or restore its backup before editing.")
            }
            let exists = try Self.itemExists(at: url)
            let current = exists ? try Self.readBytes(at: url) : nil
            guard (loaded || !exists), current == expectedData else {
                throw ProfileDataError(message: "The profiles file changed outside this window. Reload Profiles before saving; your file has not been replaced.")
            }
            if current == data { loaded = true; return }
            // Preserve only a valid previous file. A failed candidate never
            // replaces the primary file and never turns corrupt data into a backup.
            if let current {
                _ = try Self.decode(current)
                try Self.writePrivate(current, to: backupURL)
            }
            try Self.writePrivate(data, to: url)
            expectedData = data; loaded = true
        }
    }

    /// An explicit restore preserves the current file under a unique name,
    /// including corrupt bytes, and leaves the recovery backup intact.
    func restoreBackup() throws -> ProfilesFile {
        try withLock {
            let data = try Self.readBytes(at: backupURL)
            let file = try Self.decode(data)
            if try Self.itemExists(at: url) {
                let values = try url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey])
                guard values.isRegularFile == true, values.isSymbolicLink != true else {
                    throw ProfileDataError(message: "Move the linked or non-file profiles path before restoring a backup.")
                }
                let preserved = url.deletingLastPathComponent().appendingPathComponent("profiles.before-restore.\(UUID().uuidString).json")
                try FileManager.default.copyItem(at: url, to: preserved)
                try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: preserved.path)
            }
            try Self.writePrivate(data, to: url)
            expectedData = data; loaded = true; readFailed = false
            return file
        }
    }

    static func encode(_ file: ProfilesFile) throws -> Data {
        try ProfileValidation.validate(file)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        var versioned = file
        versioned.formatVersion = 1
        let data = try encoder.encode(versioned)
        guard data.count <= ProfileValidation.maximumFileBytes else {
            throw ProfileDataError(message: "Profiles exceed the 2 MiB file limit. Export and remove unused profiles first.")
        }
        return data
    }

    private static func decode(_ data: Data) throws -> ProfilesFile {
        let file = try JSONDecoder().decode(ProfilesFile.self, from: data)
        try ProfileValidation.validate(file)
        return file
    }

    private static func readBytes(at url: URL) throws -> Data {
        let values = try url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey])
        guard values.isRegularFile == true, values.isSymbolicLink != true else {
            throw ProfileDataError(message: "Profiles must be stored in a regular file, not a link or folder.")
        }
        guard (values.fileSize ?? Int.max) <= ProfileValidation.maximumFileBytes else {
            throw ProfileDataError(message: "The profiles file exceeds the 2 MiB limit.")
        }
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        let data = try handle.read(upToCount: ProfileValidation.maximumFileBytes + 1) ?? Data()
        guard data.count <= ProfileValidation.maximumFileBytes else {
            throw ProfileDataError(message: "The profiles file exceeds the 2 MiB limit.")
        }
        return data
    }

    private static func itemExists(at url: URL) throws -> Bool {
        var info = stat()
        if lstat(url.path, &info) == 0 { return true }
        if errno == ENOENT { return false }
        throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
    }

    private func withLock<T>(_ operation: () throws -> T) throws -> T {
        let folder = url.deletingLastPathComponent()
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
        let fd = Darwin.open(url.appendingPathExtension("lock").path, O_CREAT | O_RDWR | O_NOFOLLOW | O_CLOEXEC, S_IRUSR | S_IWUSR)
        guard fd >= 0 else { throw ProfileDataError(message: "Cannot reserve the profiles file for saving.") }
        defer { Darwin.close(fd) }
        guard flock(fd, LOCK_EX | LOCK_NB) == 0 else {
            throw ProfileDataError(message: "Another OpenNaga process is saving profiles. Try again.")
        }
        defer { flock(fd, LOCK_UN) }
        return try operation()
    }

    /// A same-directory rename publishes a fully written, private file. Both
    /// the temporary file and final file have owner-only permissions.
    private static func writePrivate(_ data: Data, to destination: URL) throws {
        var template = Array(destination.deletingLastPathComponent().appendingPathComponent(".profiles.XXXXXX").path.utf8CString)
        let fd = mkstemp(&template)
        guard fd >= 0 else { throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO) }
        let temporary = String(cString: template)
        defer { Darwin.close(fd); unlink(temporary) }
        try data.withUnsafeBytes { buffer in
            guard let base = buffer.baseAddress else { return }
            var offset = 0
            while offset < buffer.count {
                let count = Darwin.write(fd, base.advanced(by: offset), buffer.count - offset)
                if count < 0, errno == EINTR { continue }
                guard count > 0 else { throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO) }
                offset += count
            }
        }
        guard fsync(fd) == 0, Darwin.rename(temporary, destination.path) == 0 else {
            throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
        }
    }
}
