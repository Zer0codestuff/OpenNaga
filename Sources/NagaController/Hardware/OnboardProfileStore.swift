import Foundation

// Invoked only by explicit save/restore operations on the hardware worker queue.
enum OnboardProfileStore {
    struct State: Codable {
        let identity: String
        let original: [[UInt8]]
        var name: String
        var pending: Bool
    }
    static var url: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("NagaController/onboard-profile.json")
    }
    // A pending or unreadable journal also blocks software interception after a crash.
    static var isActive: Bool { FileManager.default.fileExists(atPath: url.path) }
    static var savedName: String? {
        guard let state = try? load(from: url), !state.pending else { return nil }
        return state.name
    }

    // The backup holds this model's original assignments, so it follows the
    // receiver model (vendor:product) rather than the USB port it was saved on.
    static func sameReceiver(_ a: String, _ b: String) -> Bool {
        a.split(separator: ":").prefix(2) == b.split(separator: ":").prefix(2)
    }

    static func load(from url: URL) throws -> State {
        let state = try JSONDecoder().decode(State.self, from: Data(contentsOf: url))
        guard !state.identity.isEmpty, state.original.count == 21 else {
            throw RazerHardwareError.malformed("Incomplete mouse backup.")
        }
        var ids = Set<UInt8>()
        for bytes in state.original {
            guard bytes.count == 10 else { throw RazerHardwareError.malformed("Invalid mouse backup.") }
            _ = try RazerOnboardBinding(bytes: bytes, profile: 1, buttonID: bytes[1])
            guard ids.insert(bytes[1]).inserted else { throw RazerHardwareError.malformed("Duplicate button in the backup.") }
        }
        return state
    }

    static func save(_ plan: OnboardProfilePlan, session: RazerHardwareSession, identity: String, at url: URL = url) throws {
        guard plan.isSupported else { throw RazerHardwareError.invalidValue(plan.issues.joined(separator: "\n")) }
        guard try session.readMode() == 0 else { throw RazerHardwareError.invalidValue("Turn off driver mode before saving to the mouse.") }
        let previousData = FileManager.default.fileExists(atPath: url.path) ? try Data(contentsOf: url) : nil
        let previous = previousData == nil ? nil : try load(from: url)
        guard previous.map({ sameReceiver($0.identity, identity) && !$0.pending }) ?? true else {
            throw RazerHardwareError.invalidValue("Restore the pending backup with the original receiver first.")
        }
        let before = try RazerOnboardBindings.read(session: session, profile: 1).map(\.bytes)
        guard before.count == 21 else { throw RazerHardwareError.malformed("Unexpected button layout.") }
        let original = previous?.original ?? before
        guard Set(original.map { $0[1] }) == Set(before.map { $0[1] }),
              Set(plan.functions.keys).isSubset(of: Set(before.map { $0[1] })) else {
            throw RazerHardwareError.malformed("The backup or profile does not match the mouse buttons.")
        }
        let targets = original.map { bytes -> [UInt8] in
            guard let function = plan.functions[bytes[1]] else { return bytes }
            return [1, bytes[1], 0] + function
        }
        let beforeByID = Dictionary(uniqueKeysWithValues: before.map { ($0[1], $0) })
        let changed = targets.filter { beforeByID[$0[1]] != $0 }
        var state = State(identity: previous?.identity ?? identity, original: original, name: plan.name, pending: true)
        try persist(state, at: url)
        do {
            for target in changed { try write(target, session: session) }
            // Also verify the active mirror, including an unchanged saved profile.
            for target in targets { try verify(target, profile: 0, session: session) }
            state.pending = false
            try persist(state, at: url)
        } catch {
            let failure = error
            do {
                // Include the uncertain write: an ACK timeout can follow a successful SET.
                for target in changed.reversed() {
                    if let old = beforeByID[target[1]] { try write(old, session: session) }
                }
                if let previousData { try previousData.write(to: url, options: .atomic) }
                else { try FileManager.default.removeItem(at: url) }
            } catch {
                throw RazerHardwareError.transport("Save interrupted. The backup was kept; use Restore Previous Assignments. \(error.localizedDescription)")
            }
            throw failure
        }
    }

    static func restore(session: RazerHardwareSession, identity: String, at url: URL = url) throws {
        var state = try load(from: url)
        guard sameReceiver(state.identity, identity) else { throw RazerHardwareError.invalidValue("Connect the receiver that was used for the save.") }
        guard try session.readMode() == 0 else { throw RazerHardwareError.invalidValue("Turn off driver mode before restoring.") }
        let current = try RazerOnboardBindings.read(session: session, profile: 1).map(\.bytes)
        guard Set(current.map { $0[1] }) == Set(state.original.map { $0[1] }) else {
            throw RazerHardwareError.malformed("The backup does not match the mouse controls.")
        }
        state.pending = true
        try persist(state, at: url)
        let currentByID = Dictionary(uniqueKeysWithValues: current.map { ($0[1], $0) })
        for original in state.original {
            if currentByID[original[1]] != original { try write(original, session: session) }
            try verify(original, profile: 0, session: session)
        }
        try FileManager.default.removeItem(at: url)
    }

    private static func persist(_ state: State, at url: URL) throws {
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try JSONEncoder().encode(state).write(to: url, options: .atomic)
    }

    static func write(_ bytes: [UInt8], session: RazerHardwareSession) throws {
        guard bytes.count == 10 else { throw RazerHardwareError.invalidValue("Invalid assignment to write.") }
        _ = try RazerOnboardBinding(bytes: bytes, profile: 1, buttonID: bytes[1])
        let command = RazerCommand(transaction: 0x1f, commandClass: 2, id: 0x0c, arguments: bytes)
        _ = try session.execute(command)
        try verify(bytes, profile: 1, session: session)
    }

    private static func verify(_ bytes: [UInt8], profile: UInt8, session: RazerHardwareSession) throws {
        var expected = bytes
        expected[0] = profile
        let actual = try session.execute(RazerOnboardBindings.readCommand(profile: profile, buttonID: bytes[1]))
        guard actual == expected else { throw RazerHardwareError.readback }
    }
}
