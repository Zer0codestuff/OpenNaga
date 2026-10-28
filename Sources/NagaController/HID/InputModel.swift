import Foundation

enum NagaInput {
    // Observed on macOS as "Naga V2 HS" over Bluetooth Low Energy.
    static let bluetoothIdentity = (vendor: 0x068e, product: 0x00b5)

    static func isSupported(vendor: Int, product: Int, name: String?) -> Bool {
        if vendor == bluetoothIdentity.vendor && product == bluetoothIdentity.product { return true }
        return vendor == 0x1532 && (product == 0x00b4 || (name?.localizedCaseInsensitiveContains("naga") == true))
    }

    static func button(page: UInt32, usage: UInt32, value: Int) -> Int? {
        if page == 0x07 {
            if (0x1e...0x27).contains(usage) { return Int(usage - 0x1e) + 1 }
            if usage == 0x2d { return 11 }
            if usage == 0x2e { return 12 }
        }
        if page == 0x09 {
            switch usage {
            case 1: return 18
            case 2: return 19
            case 3: return 17
            default: return nil // Other usages have no verified physical control identity.
            }
        }
        // USB HID Usage Tables, Consumer AC Pan, positive means right.
        if page == 0x0c && usage == 0x238 && value != 0 { return value > 0 ? 16 : 15 }
        return nil
    }

    static func scrollAxis(page: UInt32, usage: UInt32, value: Int) -> ScrollAxis? {
        guard value != 0 else { return nil }
        if page == 0x01 && usage == 0x38 { return .vertical }
        if page == 0x0c && usage == 0x238 { return .horizontal }
        return nil
    }

    /// Published protocol facts, not a hardware-validated capture:
    /// https://github.com/openrazer/openrazer/pull/2850
    /// Keyboard-interface driver report is exactly 16 bytes, report ID 04,
    /// followed by a set of held codes. No IOHID element cookies are stable IDs.
    /// Deliberately do not decode arbitrary normal keyboard or mouse packets.
    static func driverButtons(report: [UInt8]) -> Set<Int>? {
        guard report.count == 16, report.first == 0x04 else { return nil }
        let controls: [UInt8: Int] = [0x20: 13, 0x21: 14, 0x22: 15, 0x23: 16]
        return Set(report.dropFirst().compactMap { controls[$0] })
    }

    /// PR2850 mouse protocol uses bits 5/6 of the first payload byte for tilt.
    /// Only call for the 00b4 mouse interface in explicitly confirmed driver mode.
    static func driverMouseButtons(report: [UInt8]) -> Set<Int>? {
        guard report.count == 8 else { return nil }
        var result = Set<Int>()
        if report[0] & 0x20 != 0 { result.insert(15) }
        if report[0] & 0x40 != 0 { result.insert(16) }
        return result
    }
}
struct DriverButtonState {
    private var sources: [String: Set<Int>] = [:]
    var held: Set<Int> { sources.values.reduce(into: Set<Int>()) { $0.formUnion($1) } }

    mutating func update(source: String, buttons: Set<Int>) -> (pressed: Set<Int>, released: Set<Int>) {
        let previous = held
        sources[source] = buttons
        return (held.subtracting(previous), previous.subtracting(held))
    }

    mutating func reset() { sources.removeAll() }
}

struct InputRepeatOwnership {
    // CGEvent exposes no unique device ID. Matching metadata plus a live HID hold
    // reduces collisions but cannot prove ownership of perfectly simultaneous
    // same-key events from identical keyboard types. Unqualified edges revoke it.
    struct Signature: Equatable {
        var keyboardType: Int64
        var sourcePID: Int64
        var sourceState: Int64
    }
    private var owners: [Int: Signature] = [:]
    mutating func claim(button: Int, signature: Signature) { owners[button] = signature }
    mutating func invalidate(button: Int) { owners.removeValue(forKey: button) }
    mutating func reset() { owners.removeAll() }
    func matches(button: Int, signature: Signature, physicallyHeld: Bool) -> Bool {
        physicallyHeld && owners[button] == signature
    }
}

/// Both timestamps use monotonic seconds since boot. A matching edge is removed,
/// even when the caller elects to leave an unmapped input untouched.
struct InputEdgeMatcher {
    struct Edge {
        let button: Int
        let down: Bool
        let timestamp: TimeInterval
    }
    var window: TimeInterval = 0.025
    private var edges: [Edge] = []

    mutating func record(button: Int, down: Bool, timestamp: TimeInterval) {
        edges.removeAll { timestamp - $0.timestamp > window }
        edges.append(Edge(button: button, down: down, timestamp: timestamp))
        if edges.count > 128 { edges.removeFirst(edges.count - 128) }
    }

    mutating func consume(button: Int, down: Bool, timestamp: TimeInterval) -> Bool {
        edges.removeAll { timestamp - $0.timestamp > window }
        guard let index = edges.firstIndex(where: {
            $0.button == button && $0.down == down && abs(timestamp - $0.timestamp) <= window
        }) else { return false }
        edges.remove(at: index)
        return true
    }

    mutating func reset() { edges.removeAll() }
}
