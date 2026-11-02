import Foundation

/// Main-thread execution. Restarting one button invalidates only that button's
/// run. Scheduling is injected so cancellation can be tested without sleeps.
final class MacroRunner {
    typealias Schedule = (TimeInterval, @escaping () -> Void) -> Void
    private final class Run {
        let steps: [MacroStep]
        var index = 0
        var text: [UInt16] = []
        var textOffset = 0
        init(_ steps: [MacroStep]) { self.steps = steps }
    }
    private var runs: [Int: Run] = [:]
    private let schedule: Schedule
    private let sendKey: (KeyStroke) -> Void
    private let sendText: ([UInt16]) -> Void
    private let didChange: () -> Void
    var activeButtons: Set<Int> { Set(runs.keys) }

    init(schedule: @escaping Schedule = { delay, body in
        if delay == 0 { DispatchQueue.main.async(execute: body) }
        else { DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: body) }
    }, sendKey: @escaping (KeyStroke) -> Void, sendText: @escaping ([UInt16]) -> Void,
         didChange: @escaping () -> Void = {}) {
        self.schedule = schedule; self.sendKey = sendKey
        self.sendText = sendText; self.didChange = didChange
    }

    func start(_ steps: [MacroStep], button: Int) throws {
        try ProfileValidation.validateMacro(steps)
        let run = Run(steps)
        runs[button] = run
        didChange()
        enqueue(run, button: button)
    }

    func cancelAll() {
        guard !runs.isEmpty else { return }
        runs.removeAll()
        didChange()
    }

    private func enqueue(_ run: Run, button: Int, after delay: TimeInterval = 0) {
        // A cancelled long delay must not retain its steps and text until the
        // deadline. Only the active runs dictionary owns the execution context.
        schedule(delay) { [weak self, weak run] in
            guard let self, let run, self.runs[button] === run else { return }
            self.advance(run, button: button)
        }
    }

    private func advance(_ run: Run, button: Int) {
        guard run.index < run.steps.count else {
            runs.removeValue(forKey: button); didChange(); return
        }
        let step = run.steps[run.index]
        if step.type == "text" {
            if run.text.isEmpty { run.text = Array((step.text ?? "").utf16) }
            let end = Self.chunkEnd(run.text, offset: run.textOffset)
            if end > run.textOffset { sendText(Array(run.text[run.textOffset..<end])) }
            run.textOffset = end
            if end == run.text.count {
                run.text.removeAll(); run.textOffset = 0; run.index += 1
            }
            enqueue(run, button: button)
            return
        }
        run.index += 1
        if step.type == "key", let key = step.keyStroke { sendKey(key) }
        enqueue(run, button: button, after: step.type == "delay" ? Double(step.delayMs ?? 0) / 1000 : 0)
    }

    /// Keep each Unicode event small and never split a surrogate pair.
    static func chunkEnd(_ units: [UInt16], offset: Int) -> Int {
        var end = min(offset + 16, units.count)
        if end < units.count, end > offset, (0xD800...0xDBFF).contains(units[end - 1]),
           (0xDC00...0xDFFF).contains(units[end]) { end -= 1 }
        return end
    }
}
