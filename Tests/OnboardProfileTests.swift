import Foundation

enum OnboardProfileTests {
    struct Failure: Error, CustomStringConvertible { let description: String }
    final class Mouse: RazerTransport {
        var identity = "fixture"
        var bindings: [UInt8: [UInt8]] = [:]
        var sets: [[UInt8]] = []
        var failNextSet = false
        var failAllSets = false
        let ids: [UInt8] = [1,2,3,52,53,11,12,64,67,70,73,65,68,71,74,66,69,72,75,9,10]
        init() {
            for id in ids {
                bindings[id] = [1, id, 0, 1, 1, id, 0, 0, 0, 0]
            }
            for id in UInt8(64)...75 { bindings[id] = [1,id,0,2,1,0,0x1e + id - 64,0,0,0] }
        }
        func open() throws {}
        func close() {}
        func exchange(_ request: [UInt8]) throws -> [UInt8] {
            var response = request
            response[0] = 2
            var args = Array(request[8..<(8 + Int(request[5]))])
            switch (request[6], request[7]) {
            case (0, 0x84): args = [0, 0]
            case (2, 0x84): args = [21] + ids
            case (2, 0x8c):
                guard var value = bindings[args[1]] else { throw Failure(description: "Unknown control") }
                value[0] = args[0]
                args = value
            case (2, 0x0c):
                sets.append(args)
                bindings[args[1]] = args // A lost ACK may still follow a successful write.
                if failNextSet || failAllSets {
                    failNextSet = false
                    throw Failure(description: "Lost write ACK")
                }
            default: throw Failure(description: "Unexpected command")
            }
            response.replaceSubrange(8..<(8 + args.count), with: args)
            response[88] = RazerReportCodec.checksum(response)
            return response
        }
    }

    static func run() throws -> Int {
        var count = 0
        func check(_ value: Bool, _ text: String) throws {
            guard value else { throw Failure(description: text) }; count += 1
        }
        func rejects(_ body: () throws -> Void) throws {
            do { try body() } catch { count += 1; return }
            throw Failure(description: "Expected failure")
        }
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let url = root.appendingPathComponent("onboard.json")
        defer { try? FileManager.default.removeItem(at: root) }
        let mouse = Mouse(), session: RazerHardwareSession
        session = RazerHardwareSession(transport: mouse, pause: { _ in })
        let original = mouse.bindings
        let escape = OnboardProfilePlan(name: "Escape", mapping: [1: .keySequence(keys: [KeyStroke(key: "escape", modifiers: [], keyCode: 53)], description: nil)])
        try check(escape.functions[64] == [2,2,0,41,0,0,0], "Escape HID fixture")
        try check(OnboardProfilePlan.keyboard(KeyStroke(key: "c", modifiers: ["cmd", "shift"], keyCode: 8)) == [2,2,10,6,0,0,0], "Modifier and physical-key encoding")
        try check(OnboardProfilePlan.keyboard(KeyStroke(key: "tab", modifiers: ["ctrl"], keyCode: 48)) == [2,2,1,43,0,0,0], "Control Tab")
        try check(OnboardProfilePlan.function(.mouse(action: .button4, description: nil)) == [1,1,4,0,0,0,0], "Button 4 remains a standard mouse button")
        try check(OnboardProfilePlan.function(.mouse(action: .button5, description: nil)) == [1,1,5,0,0,0,0], "Button 5 remains a standard mouse button")
        try check(OnboardProfilePlan.function(.mouse(action: .scrollLeft, description: nil)) == [14,3,104,0,20,0,0], "Horizontal left uses repeat-scroll function, not mouse click")
        try check(OnboardProfilePlan.function(.mouse(action: .scrollRight, description: nil)) == [14,3,105,0,20,0,0], "Horizontal right uses repeat-scroll function, not mouse click")
        try check(OnboardProfilePlan.function(.mouse(action: .dpiUp, description: nil)) == [6,1,1,0,0,0,0], "DPI stage up encoding")
        try check(OnboardProfilePlan.function(.mouse(action: .dpiDown, description: nil)) == [6,1,2,0,0,0,0], "DPI stage down encoding")
        let mapper = ButtonMapper(eventSink: { _ in })
        mapper.updateMapping([13: .mouse(action: .dpiUp, description: nil), 14: .mouse(action: .dpiDown, description: nil)])
        try check(!mapper.hasMapping(buttonIndex: 13) && !mapper.hasMapping(buttonIndex: 14), "Native DPI actions are not intercepted by software")
        try check(OnboardProfilePlan.keyboardUsages[123] == 0x50, "Left arrow position")
        try check(OnboardProfilePlan.buttonIDs[10] == 0x49 && OnboardProfilePlan.buttonIDs[12] == 0x4b, "Grid order independent of enumeration")
        for action: ActionType in [.systemCommand(command: "true", description: nil), .application(path: "/test", description: nil), .macro(steps: [], description: nil), .profileSwitch(profile: "Other", description: nil), .keySequence(keys: [], description: nil), .keySequence(keys: [KeyStroke(key: "a", modifiers: ["fn"])], description: nil)] {
            let invalid = OnboardProfilePlan(name: "Invalid", mapping: [1: action])
            try check(!invalid.isSupported && !invalid.issues.isEmpty, "Unsupported action is visible")
            try rejects { try OnboardProfileStore.save(invalid, session: session, identity: mouse.identity, at: url) }
        }
        try check(mouse.sets.isEmpty && !FileManager.default.fileExists(atPath: url.path), "Invalid plans leave hardware and journal untouched")
        try OnboardProfileStore.save(escape, session: session, identity: mouse.identity, at: url)
        try check(mouse.sets.count == 1, "Only changed control is written")
        let state = try OnboardProfileStore.load(from: url)
        try check(!state.pending && state.name == "Escape" && state.original.count == 21, "Successful save stores a complete backup")
        try check(mouse.bindings[64] == [1,64,0,2,2,0,41,0,0,0], "Stored Escape descriptor")
        try OnboardProfileStore.save(escape, session: session, identity: mouse.identity, at: url)
        try check(mouse.sets.count == 1, "Identical save avoids flash writes")
        let next = OnboardProfilePlan(name: "Next", mapping: [2: .mouse(action: .button5, description: nil)])
        try OnboardProfileStore.save(next, session: session, identity: mouse.identity, at: url)
        try check(mouse.bindings[64] == original[64], "Unassigned controls return to the original baseline on profile replacement")
        try check(OnboardProfileStore.load(from: url).original == state.original, "Original backup survives profile replacements")
        let priorData = try Data(contentsOf: url), priorBindings = mouse.bindings
        mouse.failNextSet = true
        try rejects { try OnboardProfileStore.save(escape, session: session, identity: mouse.identity, at: url) }
        try check(mouse.bindings == priorBindings, "Lost ACK rolls back actual hardware changes")
        try check(Data(contentsOf: url) == priorData, "Rollback preserves the previous journal byte for byte")
        let setCount = mouse.sets.count
        try rejects { try OnboardProfileStore.restore(session: session, identity: "other-device", at: url) }
        try check(mouse.sets.count == setCount, "Wrong receiver cannot restore")
        try check(OnboardProfileStore.sameReceiver("1532:00b4:18026496", "1532:00b4:18022400"), "Same receiver on another USB port or hub")
        try check(!OnboardProfileStore.sameReceiver("1532:00b4:18022400", "1532:00b5:18022400"), "Different receiver model is rejected")
        try OnboardProfileStore.restore(session: session, identity: mouse.identity, at: url)
        try check(mouse.bindings == original, "Restore recovers every original descriptor")
        try check(!FileManager.default.fileExists(atPath: url.path), "Verified restoration removes the hardware-mode gate")
        mouse.failAllSets = true
        try rejects { try OnboardProfileStore.save(escape, session: session, identity: mouse.identity, at: url) }
        try check(OnboardProfileStore.load(from: url).pending, "Failed rollback keeps a pending recovery journal")
        mouse.failAllSets = false
        try rejects { try OnboardProfileStore.save(escape, session: session, identity: mouse.identity, at: url) }
        try OnboardProfileStore.restore(session: session, identity: mouse.identity, at: url)
        try check(mouse.bindings == original, "Manual recovery after interrupted writes")
        return count
    }
}
