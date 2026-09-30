import Foundation

// MIT implementation from wire-format facts, not translated OpenRazer code.
// References, accessed 2026-09-08:
// https://github.com/openrazer/openrazer/blob/master/driver/razerchromacommon.c
// https://github.com/openrazer/openrazer/blob/master/driver/razermouse_driver.c
// https://github.com/openrazer/openrazer/blob/master/driver/razercommon.c
// https://github.com/openrazer/openrazer/pull/2850
// Scoped to Naga V2 HyperSpeed receiver 1532:00b4, not Bluetooth.
enum RazerHardwareError: LocalizedError {
    case invalidValue(String), malformed(String), status(UInt8), disconnected, transport(String), readback
    var errorDescription: String? {
        switch self {
        case .invalidValue(let text), .malformed(let text), .transport(let text): return text
        case .disconnected: return "Naga V2 HyperSpeed USB receiver unavailable."
        case .status(let value):
            switch value {
            case 1: return "Mouse busy. Try again."
            case 3: return "The mouse rejected the command."
            case 4: return "Mouse timeout. Move it to wake it up."
            case 5: return "Command not supported by the mouse."
            default: return "Unknown response status: \(value)."
            }
        case .readback: return "The value read back does not confirm the requested change."
        }
    }
}

struct RazerCommand: Equatable {
    let transaction: UInt8
    let commandClass: UInt8
    let id: UInt8
    let arguments: [UInt8]

    static let getDPI = RazerCommand(transaction: 0x1f, commandClass: 4, id: 0x85, arguments: [UInt8](repeating: 0, count: 7))
    static let getPolling = RazerCommand(transaction: 0x1f, commandClass: 0, id: 0x85, arguments: [0])
    static let getBattery = RazerCommand(transaction: 0x1f, commandClass: 7, id: 0x80, arguments: [0, 0])
    // OpenRazer reads mode with 0xff on this receiver, unlike SET mode. The
    // Naga V2 HyperSpeed answers status 4 (timeout) to that variant, so the
    // session retries with 0x1f, the ID every other command uses here.
    static let getMode = RazerCommand(transaction: 0xff, commandClass: 0, id: 0x84, arguments: [0, 0])
    static let getModeAlternate = RazerCommand(transaction: 0x1f, commandClass: 0, id: 0x84, arguments: [0, 0])
    static func setDPI(x: Int, y: Int) throws -> Self {
        guard (100...30000).contains(x), (100...30000).contains(y) else {
            throw RazerHardwareError.invalidValue("Allowed DPI: 100...30000.")
        }
        // The published SET builder uses storage=1 even when passed NOSTORE.
        return Self(transaction: 0x1f, commandClass: 4, id: 5,
                    arguments: [1, UInt8(x >> 8), UInt8(x & 255), UInt8(y >> 8), UInt8(y & 255), 0, 0])
    }
    static func setPolling(_ hz: Int) throws -> Self {
        guard let value = [125: UInt8(8), 500: 2, 1000: 1][hz] else {
            throw RazerHardwareError.invalidValue("Allowed polling rates: 125, 500 or 1000 Hz.")
        }
        return Self(transaction: 0x1f, commandClass: 0, id: 5, arguments: [value])
    }
    static func setMode(_ mode: UInt8) throws -> Self {
        guard mode == 0 || mode == 3 else { throw RazerHardwareError.invalidValue("Unsafe hardware mode.") }
        return Self(transaction: 0x1f, commandClass: 0, id: 4, arguments: [mode, 0])
    }
}

enum RazerReportCodec {
    static let length = 90
    static func checksum(_ bytes: [UInt8]) -> UInt8 {
        bytes[2..<88].reduce(0, ^)
    }
    static func encode(_ command: RazerCommand) throws -> [UInt8] {
        guard command.arguments.count <= 80 else { throw RazerHardwareError.invalidValue("Command too long.") }
        var bytes = [UInt8](repeating: 0, count: length)
        bytes[1] = command.transaction
        bytes[5] = UInt8(command.arguments.count)
        bytes[6] = command.commandClass
        bytes[7] = command.id
        bytes.replaceSubrange(8..<(8 + command.arguments.count), with: command.arguments)
        bytes[88] = checksum(bytes)
        return bytes
    }
    static func decode(_ bytes: [UInt8], for command: RazerCommand) throws -> [UInt8] {
        guard bytes.count == length else { throw RazerHardwareError.malformed("USB response: expected 90 bytes, received \(bytes.count).") }
        guard bytes[88] == checksum(bytes) else { throw RazerHardwareError.malformed("Invalid response checksum.") }
        guard bytes[1] == command.transaction, bytes[6] == command.commandClass, bytes[7] == command.id,
              bytes[2] == 0, bytes[3] == 0, bytes[4] == 0, bytes[89] == 0 else {
            throw RazerHardwareError.malformed("The response does not match the command.")
        }
        guard bytes[5] <= 80 else { throw RazerHardwareError.malformed("Invalid argument size.") }
        guard bytes[0] == 2 else { throw RazerHardwareError.status(bytes[0]) }
        guard Int(bytes[5]) == command.arguments.count else { throw RazerHardwareError.malformed("Incomplete response.") }
        return Array(bytes[8..<(8 + Int(bytes[5]))])
    }
}

// A session owns its transport. Call from one serial queue, never the UI thread.
protocol RazerTransport: AnyObject {
    var identity: String { get }
    func open() throws
    func exchange(_ request: [UInt8]) throws -> [UInt8]
    func close()
}

struct RazerHardwareSnapshot {
    let dpiX: Int?
    let dpiY: Int?
    let pollingRate: Int?
    let batteryLevel: Int?
    let mode: UInt8?
    let warnings: [String]
}

final class RazerHardwareSession {
    let transport: RazerTransport
    private let pause: (TimeInterval) -> Void
    init(transport: RazerTransport, pause: @escaping (TimeInterval) -> Void = Thread.sleep(forTimeInterval:)) {
        self.transport = transport
        self.pause = pause
    }
    func execute(_ command: RazerCommand) throws -> [UInt8] {
        let request = try RazerReportCodec.encode(command)
        for attempt in 0..<3 {
            do { return try RazerReportCodec.decode(transport.exchange(request), for: command) }
            catch RazerHardwareError.status(1) where attempt < 2 { pause(0.02 * Double(attempt + 1)) }
        }
        throw RazerHardwareError.status(1)
    }
    func readDPI() throws -> (Int, Int) {
        let a = try execute(.getDPI)
        let x = Int(a[1]) * 256 + Int(a[2]), y = Int(a[3]) * 256 + Int(a[4])
        guard (100...30000).contains(x), (100...30000).contains(y) else {
            throw RazerHardwareError.malformed("The mouse returned invalid DPI values.")
        }
        return (x, y)
    }
    func readPolling() throws -> Int {
        let a = try execute(.getPolling)
        guard let hz = [UInt8(1): 1000, 2: 500, 8: 125][a[0]] else {
            throw RazerHardwareError.malformed("Unknown hardware polling rate.")
        }
        return hz
    }
    func readMode() throws -> UInt8 {
        let a: [UInt8]
        do { a = try execute(.getMode) }
        catch RazerHardwareError.status(4) { a = try execute(.getModeAlternate) }
        guard (a[0] == 0 || a[0] == 3), a[1] == 0 else {
            throw RazerHardwareError.malformed("Unknown hardware mode. No change was made.")
        }
        return a[0]
    }
    func readSnapshot() throws -> RazerHardwareSnapshot {
        var warnings: [String] = []
        func read<T>(_ label: String, _ action: () throws -> T) -> T? {
            do { return try action() }
            catch { warnings.append("\(label): \(error.localizedDescription)"); return nil }
        }
        let dpi = read("DPI", readDPI)
        let polling = read("Polling rate", readPolling)
        let battery = read("Battery") {
            let arguments = try execute(.getBattery)
            return Int((Double(arguments[1]) * 100 / 255).rounded())
        }
        let mode = read("Mode", readMode)
        guard dpi != nil || polling != nil || battery != nil || mode != nil else {
            throw RazerHardwareError.transport(warnings.joined(separator: "\n"))
        }
        return RazerHardwareSnapshot(dpiX: dpi?.0, dpiY: dpi?.1, pollingRate: polling,
                                     batteryLevel: battery, mode: mode, warnings: warnings)
    }
    func setDPI(x: Int, y: Int) throws {
        _ = try execute(.setDPI(x: x, y: y))
        let actual = try readDPI()
        guard actual == (x, y) else { throw RazerHardwareError.readback }
    }
    func setPolling(_ hz: Int) throws {
        _ = try execute(.setPolling(hz))
        guard try readPolling() == hz else { throw RazerHardwareError.readback }
    }
    func setMode(_ mode: UInt8) throws {
        _ = try execute(.setMode(mode))
        guard try readMode() == mode else { throw RazerHardwareError.readback }
    }
}
