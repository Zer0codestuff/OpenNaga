import Foundation

enum MacroRunnerTests {
    struct Failure: Error, CustomStringConvertible { let description: String }
    private final class Clock {
        var now: Double = 0
        var pending: [(time: Double, order: Int, body: () -> Void)] = []
        var nextOrder = 0
        func schedule(_ delay: Double, _ body: @escaping () -> Void) {
            pending.append((now + delay, nextOrder, body)); nextOrder += 1
        }
        func advance(to deadline: Double) throws {
            var budget = 10_000
            while let next = pending.indices.min(by: {
                pending[$0].time == pending[$1].time ? pending[$0].order < pending[$1].order : pending[$0].time < pending[$1].time
            }), pending[next].time <= deadline {
                budget -= 1
                guard budget > 0 else { throw Failure(description: "Macro scheduler failed to yield or finish") }
                let item = pending.remove(at: next)
                now = item.time; item.body()
            }
            now = deadline
        }
        func runNext() {
            guard !pending.isEmpty else { return }
            let item = pending.removeFirst(); now = item.time; item.body()
        }
    }

    static func run() throws -> Int {
        var count = 0
        func check(_ condition: Bool, _ message: String) throws {
            guard condition else { throw Failure(description: message) }; count += 1
        }
        func key(_ value: String) -> MacroStep { MacroStep(type: "key", keyStroke: KeyStroke(key: value, modifiers: [])) }
        let clock = Clock()
        var keys: [String] = []
        var chunks: [[UInt16]] = []
        let runner = MacroRunner(schedule: clock.schedule, sendKey: { keys.append($0.key) }, sendText: { chunks.append($0) })
        try runner.start([key("a"), MacroStep(type: "delay", delayMs: 100), key("b")], button: 1)
        try clock.advance(to: 0)
        try check(keys == ["a"] && runner.activeButtons == [1], "A macro executes up to its delay")
        try runner.start([key("c")], button: 1)
        try clock.advance(to: 1)
        try check(keys == ["a", "c"], "A new press restarts its own macro and invalidates the old delayed continuation")
        try check(runner.activeButtons.isEmpty, "Completed macros release their contexts")

        keys.removeAll()
        try runner.start([MacroStep(type: "delay", delayMs: 100), key("a")], button: 1)
        try runner.start([MacroStep(type: "delay", delayMs: 200), key("b")], button: 2)
        try clock.advance(to: 1)
        try runner.start([key("c")], button: 1)
        try clock.advance(to: 2)
        try check(keys == ["c", "b"], "Restarting one button preserves a different button's macro")

        keys.removeAll()
        try runner.start([MacroStep(type: "delay", delayMs: 100), key("a")], button: 1)
        try clock.advance(to: 2)
        runner.cancelAll()
        try clock.advance(to: 3)
        try check(keys.isEmpty && runner.activeButtons.isEmpty, "Stop Macros cancels delayed work without waiting for its deadline")

        let text = String(repeating: "123456789012345🙂", count: 80)
        try runner.start([MacroStep(type: "text", text: text)], button: 3)
        try clock.advance(to: 4)
        try check(chunks.flatMap { $0 } == Array(text.utf16), "Chunked Unicode text preserves every UTF-16 unit in order")
        try check(chunks.count > 1 && chunks.allSatisfy { $0.count <= 16 }, "Long text yields between bounded chunks")
        try check(chunks.allSatisfy { chunk in chunk.last.map { !(0xD800...0xDBFF).contains($0) } ?? true }, "A chunk never ends with half a surrogate pair")
        chunks.removeAll()
        try runner.start([MacroStep(type: "text", text: text)], button: 3)
        clock.runNext()
        try check(chunks.count == 1, "Only one text chunk is sent in a scheduled turn")
        runner.cancelAll()
        try clock.advance(to: 5)
        try check(chunks.count == 1, "Stopping a running text snippet prevents all remaining chunks")

        try runner.start([MacroStep(type: "delay", delayMs: 100), key("a")], button: 1)
        do {
            try runner.start([MacroStep(type: "delay", delayMs: -1)], button: 1)
            throw Failure(description: "Invalid macro unexpectedly started")
        } catch is ProfileDataError {}
        try clock.advance(to: 6)
        try check(keys == ["a"], "An invalid replacement does not destroy a previously validated macro")
        return count
    }
}
