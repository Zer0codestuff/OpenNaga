import Foundation

enum HardwareProtocolTests {
    struct Failure: Error, CustomStringConvertible {
        let description: String
    }
    private final class FakeTransport: RazerTransport {
        var identity = "fixture"
        var requests: [[UInt8]] = []
        var replies: [[UInt8]] = []
        func open() throws {}
        func close() {}
        func exchange(_ request: [UInt8]) throws -> [UInt8] {
            requests.append(request)
            guard !replies.isEmpty else { throw Failure(description: "Unexpected request") }
            return replies.removeFirst()
        }
    }
    static func run() throws -> Int {
        var passed = 0
        func check(_ value: Bool, _ label: String) throws {
            guard value else { throw Failure(description: label) }
            passed += 1
        }
        func rejects(_ label: String, _ operation: () throws -> Void) throws {
            do { try operation() }
            catch { passed += 1; return }
            throw Failure(description: "Expected rejection: \(label)")
        }
        func packet(_ prefix: [UInt8], crc: UInt8) -> [UInt8] {
            prefix + [UInt8](repeating: 0, count: 88 - prefix.count) + [crc, 0]
        }
        // Literal wire fixtures, not generated from the implementation under test.
        let fixtures: [(RazerCommand, [UInt8])] = [
            (.getDPI, packet([0, 0x1f, 0, 0, 0, 7, 4, 0x85], crc: 0x86)),
            (.getPolling, packet([0, 0x1f, 0, 0, 0, 1, 0, 0x85], crc: 0x84)),
            (.getBattery, packet([0, 0x1f, 0, 0, 0, 2, 7, 0x80], crc: 0x85)),
            (.getMode, packet([0, 0xff, 0, 0, 0, 2, 0, 0x84], crc: 0x86)),
            (try .setDPI(x: 800, y: 1600), packet([0, 0x1f, 0, 0, 0, 7, 4, 5, 1, 3, 0x20, 6, 0x40], crc: 0x62)),
            (try .setPolling(1000), packet([0, 0x1f, 0, 0, 0, 1, 0, 5, 1], crc: 5)),
            (try .setPolling(500), packet([0, 0x1f, 0, 0, 0, 1, 0, 5, 2], crc: 6)),
            (try .setPolling(125), packet([0, 0x1f, 0, 0, 0, 1, 0, 5, 8], crc: 12)),
            (try .setMode(3), packet([0, 0x1f, 0, 0, 0, 2, 0, 4, 3], crc: 5)),
            (try .setMode(0), packet([0, 0x1f, 0, 0, 0, 2, 0, 4], crc: 6))
        ]
        for (command, expected) in fixtures {
            try check(RazerReportCodec.encode(command) == expected, "Command fixture \(command)")
        }
        for bad in [Int.min, -1, 0, 99, 30001, Int.max] {
            try rejects("DPI \(bad)") { _ = try RazerCommand.setDPI(x: bad, y: 800) }
            try rejects("DPI Y \(bad)") { _ = try RazerCommand.setDPI(x: 800, y: bad) }
        }
        for good in [100, 30000] {
            try check(RazerCommand.setDPI(x: good, y: good).arguments.count == 7, "DPI boundary")
        }
        for hz in [0, 250, 2000, 4000, 8000] {
            try rejects("polling \(hz)") { _ = try RazerCommand.setPolling(hz) }
        }
        try rejects("unknown mode") { _ = try RazerCommand.setMode(2) }
        let goodPolling = packet([2, 0x1f, 0, 0, 0, 1, 0, 0x85, 1], crc: 0x85)
        try check(RazerReportCodec.decode(goodPolling, for: .getPolling) == [1], "Decode fixture")
        for size in [0, 1, 89, 91] {
            try rejects("length \(size)") { _ = try RazerReportCodec.decode([UInt8](repeating: 0, count: size), for: .getPolling) }
        }
        for offset in [1, 2, 3, 4, 5, 6, 7, 88, 89] {
            var invalid = goodPolling
            invalid[offset] ^= 1
            if offset != 88 { invalid[88] = RazerReportCodec.checksum(invalid) }
            try rejects("field \(offset)") { _ = try RazerReportCodec.decode(invalid, for: .getPolling) }
        }
        for status: UInt8 in [0, 1, 3, 4, 5, 255] {
            var invalid = goodPolling
            invalid[0] = status
            try rejects("status \(status)") { _ = try RazerReportCodec.decode(invalid, for: .getPolling) }
        }
        var oversized = goodPolling
        oversized[5] = 81
        oversized[88] = RazerReportCodec.checksum(oversized)
        try rejects("oversized arguments") { _ = try RazerReportCodec.decode(oversized, for: .getPolling) }
        let fake = FakeTransport()
        let session = RazerHardwareSession(transport: fake, pause: { _ in })
        var busy = goodPolling
        busy[0] = 1
        fake.replies = [busy, busy, goodPolling]
        try check(session.readPolling() == 1000, "Busy then success")
        try check(fake.requests.count == 3, "Bounded retry count")
        fake.requests = []
        fake.replies = [busy, busy, busy, goodPolling]
        try rejects("busy exhausted") { _ = try session.readPolling() }
        try check(fake.requests.count == 3, "Busy stops after three attempts")
        fake.requests = []
        var corrupt = goodPolling
        corrupt[88] ^= 1
        fake.replies = [corrupt]
        try rejects("corrupt not retried") { _ = try session.readPolling() }
        try check(fake.requests.count == 1, "Do not replay malformed commands")

        let setPollingACK = packet([2, 0x1f, 0, 0, 0, 1, 0, 5, 1], crc: 5)
        fake.requests = []
        fake.replies = [setPollingACK, goodPolling]
        try session.setPolling(1000)
        try check(fake.requests.map { $0[7] } == [5, 0x85], "SET must read back")
        let wrongPolling = packet([2, 0x1f, 0, 0, 0, 1, 0, 0x85, 2], crc: 0x86)
        fake.replies = [setPollingACK, wrongPolling]
        try rejects("readback mismatch") { try session.setPolling(1000) }
        fake.requests = []
        try rejects("invalid set never transported") { try session.setPolling(250) }
        try check(fake.requests.isEmpty, "Invalid SET produces no IO")

        func reply(_ command: RazerCommand, _ arguments: [UInt8]) throws -> [UInt8] {
            var bytes = try RazerReportCodec.encode(command)
            bytes[0] = 2
            bytes.replaceSubrange(8..<(8 + arguments.count), with: arguments)
            bytes[88] = RazerReportCodec.checksum(bytes)
            return bytes
        }
        fake.requests = []
        fake.replies = [
            try reply(.getDPI, [0, 3, 0x20, 6, 0x40, 0, 0]),
            goodPolling,
            try reply(.getBattery, [0, 255]),
            try reply(.getMode, [0, 0])
        ]
        let snapshot = try session.readSnapshot()
        try check(snapshot.dpiX == 800 && snapshot.dpiY == 1600, "DPI endian")
        try check(snapshot.batteryLevel == 100 && snapshot.mode == 0, "Battery/mode")
        try check(fake.requests.allSatisfy { $0[7] & 0x80 != 0 }, "Snapshot never sends SET")
        fake.replies = [try reply(.getMode, [2, 0])]
        try rejects("unsafe existing mode") { _ = try session.readMode() }
        fake.replies = [try reply(.getDPI, [0, 0, 0, 0, 0, 0, 0])]
        try rejects("zero DPI response") { _ = try session.readDPI() }
        var unsupportedBattery = try reply(.getBattery, [0, 0])
        unsupportedBattery[0] = 5
        fake.replies = [
            try reply(.getDPI, [0, 3, 0x20, 6, 0x40, 0, 0]),
            goodPolling, unsupportedBattery, try reply(.getMode, [0, 0])
        ]
        let partial = try session.readSnapshot()
        try check(partial.dpiX == 800 && partial.pollingRate == 1000, "Keep supported features")
        try check(partial.batteryLevel == nil && partial.warnings.count == 1, "Expose unavailable feature")
        fake.requests = []
        fake.replies = [
            try reply(.setDPI(x: 800, y: 1600), [1, 3, 0x20, 6, 0x40, 0, 0]),
            try reply(.getDPI, [0, 3, 0x20, 6, 0x40, 0, 0])
        ]
        try session.setDPI(x: 800, y: 1600)
        try check(fake.requests.map { $0[7] } == [5, 0x85], "DPI readback required")
        fake.replies = [
            try reply(.setDPI(x: 800, y: 1600), [1, 3, 0x20, 6, 0x40, 0, 0]),
            try reply(.getDPI, [0, 3, 0x20, 3, 0x20, 0, 0])
        ]
        try rejects("DPI readback mismatch") { try session.setDPI(x: 800, y: 1600) }
        fake.requests = []
        fake.replies = [try reply(.setMode(3), [3, 0]), try reply(.getMode, [3, 0])]
        try session.setMode(3)
        try check(fake.requests.map { $0[1] } == [0x1f, 0xff], "Mode GET/SET transaction asymmetry")
        fake.replies = []
        try rejects("all snapshot reads fail") { _ = try session.readSnapshot() }
        let onboard = try RazerOnboardBindings.readCommand(profile: 1, buttonID: 0x40)
        try check(onboard.arguments == [1, 64, 0, 0, 0, 0, 0, 0, 0, 0], "Onboard stored-bank query")
        try check(onboard.id == 0x8c && onboard.commandClass == 2 && onboard.transaction == 0x1f, "Onboard query header")
        // Captured on the user's 1532:00b4 receiver, including physical grid order.
        let buttonIDs: [UInt8] = [21, 1, 2, 3, 52, 53, 11, 12, 64, 67, 70, 73, 65, 68, 71, 74, 66, 69, 72, 75, 9, 10]
        try check(RazerOnboardBindings.decodeButtonIDs(buttonIDs) == Array(buttonIDs.dropFirst()), "Captured button enumeration")
        for invalid in [[], [0], [22] + Array(repeating: UInt8(1), count: 21), [2, 1, 1] + Array(repeating: UInt8(0), count: 19), [1] + Array(repeating: UInt8(0), count: 21)] {
            try rejects("Malformed onboard inventory") { _ = try RazerOnboardBindings.decodeButtonIDs(invalid) }
        }
        let factory: [UInt8] = [1, 64, 0, 2, 1, 0, 30, 0, 0, 0]
        try check(RazerOnboardBinding(bytes: factory, profile: 1, buttonID: 64).bytes == factory, "Preserve factory descriptor including nonstandard length")
        for field in [0, 1, 2] {
            var wrong = factory
            wrong[field] ^= 1
            try rejects("Onboard identity field") { _ = try RazerOnboardBinding(bytes: wrong, profile: 1, buttonID: 64) }
        }
        try rejects("Truncated onboard descriptor") { _ = try RazerOnboardBinding(bytes: Array(factory.dropLast()), profile: 1, buttonID: 64) }
        fake.requests = []
        try rejects("Unsupported bank") { _ = try RazerOnboardBindings.read(session: session, profile: 2) }
        try check(fake.requests.isEmpty, "Unsupported bank makes no USB request")
        fake.replies = [try reply(RazerOnboardBindings.getButtonIDs, [1, 64] + Array(repeating: 0, count: 20)), try reply(onboard, factory)]
        let bindings = try RazerOnboardBindings.read(session: session, profile: 1)
        try check(bindings.count == 1 && bindings[0].bytes == factory, "Read stored descriptor")
        try check(fake.requests.allSatisfy { $0[7] & 0x80 != 0 }, "Onboard inspection never changes the mouse")
        return passed
    }
}
