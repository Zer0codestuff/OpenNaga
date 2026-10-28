import Foundation

// Issue 7: scroll wheel sensitivity, pure checks without hardware.
enum ScrollSensitivityTests {
    struct Failure: Error, CustomStringConvertible {
        let description: String
    }

    static func run() throws -> Int {
        var count = 0
        func check(_ condition: @autoclosure () -> Bool, _ message: String) throws {
            guard condition() else { throw Failure(description: message) }
            count += 1
        }

        // Clamping keeps the sliders and stored values inside the documented range.
        try check(ScrollSensitivity(vertical: -2, horizontal: 99).clamped == ScrollSensitivity(vertical: 0.1, horizontal: 5), "Clamp to documented range")
        try check(ScrollSensitivity(vertical: 1, horizontal: 1) == ScrollSensitivity.baseline, "Baseline is neutral")
        try check(ScrollSensitivity.range.contains(0.1) && ScrollSensitivity.range.contains(5), "Range endpoints are valid")
        try check(!ScrollSensitivity.range.contains(0.05) && !ScrollSensitivity.range.contains(5.1), "Values outside range are rejected by clamp")

        // Synthetic scroll line scaling: direction is preserved, never zero.
        try check(ScrollSensitivity.scaledLine(3, factor: 1) == 3, "Neutral factor keeps line count")
        try check(ScrollSensitivity.scaledLine(3, factor: 2) == 6, "Doubled lines")
        try check(ScrollSensitivity.scaledLine(3, factor: 0.5) == 2, "Halved lines round to nearest")
        try check(ScrollSensitivity.scaledLine(1, factor: 0.25) == 1, "Slow factor still emits one line")
        try check(ScrollSensitivity.scaledLine(-3, factor: 2) == -6, "Direction preserved")
        try check(ScrollSensitivity.scaledLine(0, factor: 3) == 0, "Zero input stays zero")

        // Residual scaling: slow settings accumulate instead of dropping ticks.
        var residual = 0.0
        var out = ScrollScaler.emit(1, 0.25, &residual)
        try check(out == 0 && residual > 0, "First slow tick is banked")
        out = ScrollScaler.emit(1, 0.25, &residual)
        try check(out == 0 && residual > 0, "Second slow tick still banked")
        out = ScrollScaler.emit(1, 0.25, &residual)
        try check(out == 0 && residual > 0, "Third slow tick still banked")
        out = ScrollScaler.emit(1, 0.25, &residual)
        try check(out == 1 && residual == 0, "Fourth slow tick emits one line")
        out = ScrollScaler.emit(2, 1.5, &residual)
        try check(out == 3, "Fast factor emits immediately")
        out = ScrollScaler.emit(1, 0.25, &residual)
        try check(out == 0 && residual > 0, "Bank rebuilds after an emit")
        out = ScrollScaler.emit(-1, 0.25, &residual)
        try check(out == 0 && residual < 0, "Reverse tick starts a fresh bank instead of spending the old one")

        // Neutral factor is a passthrough with no residual drift.
        var neutral = 0.0
        try check(ScrollScaler.emit(5, 1, &neutral) == 5 && neutral == 0, "Neutral factor leaves no residual")
        try check(ScrollScaler.emit(-5, 1, &neutral) == -5, "Neutral reverse passthrough")

        // Long-run accuracy: the emitted average converges on the factor.
        var drift = 0.0
        var emittedTotal = 0
        for _ in 0..<1000 { emittedTotal += Int(ScrollScaler.emit(1, 0.3, &drift)) }
        try check(emittedTotal == 300, "1000 ticks at 0.3x emit exactly 300 lines")

        // Persistence roundtrip through a separate defaults domain.
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("NagaScrollTests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let suite = "NagaScrollTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suite)!
        defer {
            try? FileManager.default.removeItem(at: root)
            defaults.removePersistentDomain(forName: suite)
        }
        let config = ConfigManager(storageURL: root.appendingPathComponent("profiles.json"), defaults: defaults)
        try check(config.scrollSensitivity == ScrollSensitivity.baseline, "Defaults are neutral without stored values")
        config.scrollSensitivity = ScrollSensitivity(vertical: 2.5, horizontal: 0.4)
        let restored = ConfigManager(storageURL: root.appendingPathComponent("profiles.json"), defaults: defaults)
        try check(restored.scrollSensitivity == ScrollSensitivity(vertical: 2.5, horizontal: 0.4), "Sensitivity survives reload")
        restored.scrollSensitivity = ScrollSensitivity(vertical: 99, horizontal: -1)
        try check(restored.scrollSensitivity == ScrollSensitivity(vertical: 5, horizontal: 0.1), "Stored values are clamped")
        restored.resetScrollSensitivity()
        let cleared = ConfigManager(storageURL: root.appendingPathComponent("profiles.json"), defaults: defaults)
        try check(cleared.scrollSensitivity == ScrollSensitivity.baseline, "Reset returns to neutral")

        return count
    }
}
