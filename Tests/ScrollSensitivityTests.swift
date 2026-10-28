import Foundation
import CoreGraphics

enum ScrollSensitivityTests {
    struct Failure: Error, CustomStringConvertible { let description: String }

    static func run() throws -> Int {
        var count = 0
        func check(_ condition: @autoclosure () -> Bool, _ message: String) throws {
            guard condition() else { throw Failure(description: message) }
            count += 1
        }
        try check(NagaInput.scrollAxis(page: 1, usage: 0x38, value: 1) == .vertical, "HID Wheel identifies vertical input")
        try check(NagaInput.scrollAxis(page: 12, usage: 0x238, value: -1) == .horizontal, "HID AC Pan identifies horizontal input")
        try check(NagaInput.scrollAxis(page: 1, usage: 0x38, value: 0) == nil, "Zero wheel value is not scroll evidence")
        try check(NagaInput.scrollAxis(page: 1, usage: 0x30, value: 1) == nil, "Pointer motion is not wheel evidence")
        try check(NagaInput.scrollAxis(page: 9, usage: 0x38, value: 1) == nil, "Wheel usage requires its correct HID page")

        var matcher = ScrollInputMatcher()
        matcher.record(axis: .vertical, timestamp: 10)
        try check(matcher.consume(vertical: false, horizontal: true, timestamp: 10.001).isEmpty, "Wheel cannot attribute an unrelated horizontal event")
        try check(matcher.consume(vertical: true, horizontal: false, timestamp: 10.001).vertical, "Recent wheel attributes one vertical event")
        try check(matcher.consume(vertical: true, horizontal: false, timestamp: 10.002).isEmpty, "Wheel evidence is consumed only once")
        matcher.record(axis: .horizontal, button: 15, timestamp: 11)
        let tilt = matcher.consume(vertical: false, horizontal: true, timestamp: 11.001)
        try check(tilt.horizontalButton == 15 && !tilt.vertical, "Tilt identity comes from HID rather than natural-scroll CGEvent direction")
        matcher.record(axis: .vertical, timestamp: 12)
        try check(matcher.consume(vertical: true, horizontal: false, timestamp: 12.1).isEmpty, "Stale wheel evidence cannot affect another mouse")
        matcher.record(axis: .vertical, timestamp: 13)
        matcher.record(axis: .horizontal, button: 16, timestamp: 13.001)
        let combined = matcher.consume(vertical: true, horizontal: true, timestamp: 13.002)
        try check(combined.vertical && combined.horizontalButton == 16, "Combined event correlates its axes independently")
        matcher.record(axis: .horizontal, button: 15, timestamp: 14)
        matcher.record(axis: .horizontal, button: 16, timestamp: 14.01)
        try check(matcher.consume(vertical: false, horizontal: true, timestamp: 14.011).horizontalButton == 16, "Nearest tilt evidence wins")
        matcher.reset()
        try check(matcher.consume(vertical: false, horizontal: true, timestamp: 14.012).isEmpty, "Reset clears wheel evidence")
        matcher.record(axis: .vertical, timestamp: 15)
        let shiftedWheel = matcher.consume(vertical: false, horizontal: true, shifted: true, timestamp: 15.001)
        try check(shiftedWheel.shiftedWheel && shiftedWheel.horizontalButton == nil,
            "Shift-wheel horizontal scrolling scales without invoking a tilt mapping")
        matcher.record(axis: .vertical, timestamp: 16)
        matcher.record(axis: .horizontal, button: 15, timestamp: 16)
        let shiftedTilt = matcher.consume(vertical: false, horizontal: true, shifted: true, timestamp: 16.001)
        try check(shiftedTilt.horizontalButton == 15 && !shiftedTilt.shiftedWheel,
            "Physical tilt evidence takes priority over Shift-wheel fallback")

        try check(ScrollSensitivity(vertical: -1, horizontal: 9).clamped() == .init(vertical: 0.1, horizontal: 5), "Multipliers stay within the slider range")
        try check(ScrollSensitivity(vertical: .nan, horizontal: .infinity).clamped() == .baseline, "Nonfinite persisted settings fall back to 1x")
        let scaler = ScrollScaler()
        let slow = ScrollSensitivity(vertical: 0.3, horizontal: 0.1)
        var total: Int64 = 0
        for _ in 0..<100 { total += scaler.apply(vertical: 1, horizontal: 0, sensitivity: slow).vertical }
        try check(total == 30, "Fractional vertical ticks preserve the target average")
        total = 0
        for _ in 0..<100 { total += scaler.apply(vertical: 0, horizontal: -1, sensitivity: slow).horizontal }
        try check(total == -10, "Fractional negative tilt ticks preserve the target average")
        scaler.reset()
        _ = scaler.apply(vertical: 1, horizontal: 0, sensitivity: .init(vertical: 0.5, horizontal: 1))
        try check(scaler.apply(vertical: -1, horizontal: 0, sensitivity: .init(vertical: 0.5, horizontal: 1)).vertical == 0,
            "Reversing the wheel does not spend the opposite direction's remainder")
        try check(scaler.apply(vertical: -1, horizontal: 0, sensitivity: .init(vertical: 0.5, horizontal: 1)).vertical == -1,
            "Reverse wheel direction accumulates its own remainder")
        scaler.reset()
        _ = scaler.apply(vertical: 1, horizontal: 0, sensitivity: .init(vertical: 0.5, horizontal: 1))
        _ = scaler.apply(vertical: 1, horizontal: 0, sensitivity: .baseline)
        try check(scaler.apply(vertical: 1, horizontal: 0, sensitivity: .init(vertical: 0.5, horizontal: 1)).vertical == 0,
            "Changing sensitivity clears the previous factor's remainder")

        func event(vertical: Int32 = 3, horizontal: Int32 = -2) -> CGEvent {
            CGEvent(scrollWheelEvent2Source: nil, units: .line, wheelCount: 2,
                wheel1: vertical, wheel2: horizontal, wheel3: 0)!
        }
        let fields: [CGEventField] = [.scrollWheelEventDeltaAxis1, .scrollWheelEventFixedPtDeltaAxis1,
            .scrollWheelEventPointDeltaAxis1, .scrollWheelEventDeltaAxis2,
            .scrollWheelEventFixedPtDeltaAxis2, .scrollWheelEventPointDeltaAxis2]
        let floatingFields: [CGEventField] = [.scrollWheelEventRawDeltaAxis1, .scrollWheelEventAcceleratedDeltaAxis1,
            .scrollWheelEventRawDeltaAxis2, .scrollWheelEventAcceleratedDeltaAxis2]
        let original = event()
        original.setDoubleValueField(.scrollWheelEventRawDeltaAxis1, value: 1.5)
        original.setDoubleValueField(.scrollWheelEventAcceleratedDeltaAxis1, value: 2.5)
        original.setDoubleValueField(.scrollWheelEventRawDeltaAxis2, value: -0.5)
        original.setDoubleValueField(.scrollWheelEventAcceleratedDeltaAxis2, value: -4.5)
        let originalIntegers = fields.map { original.getIntegerValueField($0) }
        let originalFloating = floatingFields.map { original.getDoubleValueField($0) }
        let baseline = original.copy()!
        ScrollEventScaler().apply(to: baseline, sensitivity: .baseline)
        try check(fields.map { baseline.getIntegerValueField($0) } == originalIntegers,
            "Baseline leaves every integer scroll representation unchanged")
        try check(floatingFields.map { baseline.getDoubleValueField($0) } == originalFloating,
            "Baseline leaves raw and accelerated scroll representations unchanged")
        let fast = original.copy()!
        ScrollEventScaler().apply(to: fast, sensitivity: .init(vertical: 2, horizontal: 3))
        for (index, field) in fields.enumerated() {
            let multiplier: Int64 = index < 3 ? 2 : 3
            try check(fast.getIntegerValueField(field) == originalIntegers[index] * multiplier,
                "Scale scroll field \(field.rawValue) from its original value, respecting Core Graphics setter side effects")
        }
        for (index, field) in floatingFields.enumerated() {
            try check(fast.getDoubleValueField(field) == originalFloating[index] * (index < 2 ? 2 : 3),
                "Scale floating scroll field \(field.rawValue)")
        }
        let verticalOnly = original.copy()!
        ScrollEventScaler().apply(to: verticalOnly, sensitivity: .init(vertical: 2, horizontal: 3), vertical: true, horizontal: false)
        try check(Array(fields.suffix(3)).map { verticalOnly.getIntegerValueField($0) } == Array(originalIntegers.suffix(3)),
            "Uncorrelated horizontal motion remains untouched")
        let suppressTilt = original.copy()!
        ScrollEventScaler().apply(to: suppressTilt, sensitivity: .baseline, vertical: false, horizontal: true, suppressHorizontal: true)
        try check(!ScrollEventScaler.hasMotion(suppressTilt, axis: .horizontal), "Mapped tilt clears every horizontal representation")
        try check(Array(fields.prefix(3)).map { suppressTilt.getIntegerValueField($0) } == Array(originalIntegers.prefix(3)),
            "Consuming tilt retains simultaneous vertical motion")
        let tiny = event(vertical: 0, horizontal: 0)
        tiny.setIntegerValueField(.scrollWheelEventFixedPtDeltaAxis2, value: 1)
        try check(ScrollEventScaler.hasMotion(tiny, axis: .horizontal), "Sub-line fixed-point tilt is recognized")

        let discrete = event()
        discrete.setIntegerValueField(.eventSourceUnixProcessID, value: 0)
        try check(ScrollEventScaler.isDiscreteHardwareEvent(discrete), "Discrete hardware scroll can use HID correlation")
        discrete.setIntegerValueField(.scrollWheelEventIsContinuous, value: 1)
        try check(!ScrollEventScaler.isDiscreteHardwareEvent(discrete), "Trackpad scroll cannot consume Naga evidence")
        discrete.setIntegerValueField(.scrollWheelEventIsContinuous, value: 0)
        discrete.setIntegerValueField(.scrollWheelEventMomentumPhase, value: 1)
        try check(!ScrollEventScaler.isDiscreteHardwareEvent(discrete), "Momentum scroll cannot consume Naga evidence")
        discrete.setIntegerValueField(.scrollWheelEventMomentumPhase, value: 0)
        discrete.setIntegerValueField(.eventSourceUnixProcessID, value: 42)
        try check(!ScrollEventScaler.isDiscreteHardwareEvent(discrete), "Another process's synthetic scroll is left alone")

        let suite = "OpenNagaScrollTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suite)!
        defer { defaults.removePersistentDomain(forName: suite) }
        let config = ConfigManager(defaults: defaults)
        try check(config.scrollSensitivity == .baseline, "Older installations start at 1x")
        config.setScrollSensitivity(.init(vertical: 0.3, horizontal: 2.4))
        try check(ConfigManager(defaults: defaults).scrollSensitivity == .init(vertical: 0.3, horizontal: 2.4),
            "Both multipliers survive relaunch without rewriting profiles")
        defaults.set(-2, forKey: ScrollSensitivity.verticalKey)
        defaults.set(100, forKey: ScrollSensitivity.horizontalKey)
        try check(ConfigManager(defaults: defaults).scrollSensitivity == .init(vertical: 0.1, horizontal: 5), "Invalid stored ranges are clamped")
        defaults.set("invalid", forKey: ScrollSensitivity.verticalKey)
        try check(ConfigManager(defaults: defaults).scrollSensitivity.vertical == 1, "Invalid stored types fall back to 1x")

        var outputs: [CGEvent] = []
        let mapper = ButtonMapper(eventSink: { outputs.append($0) })
        var sensitivity = ScrollSensitivity(vertical: 2, horizontal: 0.1)
        mapper.scrollSensitivityProvider = { sensitivity }
        mapper.updateMapping([15: .mouse(action: .scrollLeft, description: nil),
            16: .mouse(action: .scrollRight, description: nil), 1: .mouse(action: .scrollUp, description: nil)])
        for _ in 0..<10 { mapper.handle(buttonIndex: 15) }
        try check(outputs.reduce(Int64(0)) { $0 + $1.getIntegerValueField(.scrollWheelEventDeltaAxis2) } == 3,
            "Assigned tilt scroll honors slow multipliers instead of forcing one line per press")
        try check(outputs.allSatisfy { $0.type == .scrollWheel && $0.getIntegerValueField(.eventSourceUserData) == ButtonMapper.syntheticMarker },
            "Scaled assigned scroll remains tagged to avoid scaling twice")
        outputs.removeAll()
        mapper.handle(buttonIndex: 1)
        try check(outputs.first?.getIntegerValueField(.scrollWheelEventDeltaAxis1) == 6,
            "Assigned vertical scrolling uses the vertical multiplier")
        outputs.removeAll()
        sensitivity = .init(vertical: 1, horizontal: 2)
        mapper.handle(buttonIndex: 16)
        try check(outputs.first?.getIntegerValueField(.scrollWheelEventDeltaAxis2) == -6,
            "Assigned right tilt uses the horizontal multiplier and preserves direction")
        return count
    }
}
