import Cocoa
import ApplicationServices
import Darwin

final class EventTapManager {
    static let shared = EventTapManager()

    private var eventTap: CFMachPort?
    private var runLoopSource: CFRunLoopSource?

    static let didUpdateNotification = Notification.Name("NagaEventTapDidUpdate")
    private(set) var isRunning = false
    private var activeDownButtons: Set<Int> = []
    private var repeatOwnership = InputRepeatOwnership()
    private let scrollScaler = ScrollEventScaler()
    private var nativeKeys = NativeKeyRouting()
    private var traceEnabled = false
    /// The tap can re-send native mouse keys with held keyboard modifiers.
    /// This also applies while a mouse-memory profile pauses remapping.
    private(set) var mergesNativeModifiers = false

    private(set) var isListeningOnly: Bool = true
    var isRemappingEnabled: Bool {
        get { return !isListeningOnly && !ConfigManager.shared.currentProfileIsOnboardOnly }
        set {
            let newListenOnly = !newValue
            if newListenOnly != isListeningOnly {
                start(listenOnly: newListenOnly)
            }
        }
    }

    private init() {}

    private static var onboardActive: Bool {
        OnboardProfileStore.isActive || ConfigManager.shared.currentProfileIsOnboardOnly
    }

    /// Mouse-memory and imported profiles never remap, and their Remapping
    /// switch is unavailable, so native mouse keys always receive held keyboard
    /// modifiers there. Otherwise both follow the Remapping switch.
    static func policy(listenOnly: Bool, onboard: Bool) -> (remaps: Bool, mergesNativeModifiers: Bool) {
        let remaps = !listenOnly && !onboard
        return (remaps, remaps || onboard)
    }

    /// True when a running tap already matches the requested state.
    func matches(listenOnly: Bool) -> Bool {
        let policy = Self.policy(listenOnly: listenOnly, onboard: Self.onboardActive)
        return isListeningOnly == !policy.remaps && mergesNativeModifiers == policy.mergesNativeModifiers
    }

    func start(listenOnly: Bool) {
        stop()
        ButtonMapper.shared.resetPhysicalModifierObservation()
        let onboard = Self.onboardActive
        let policy = Self.policy(listenOnly: listenOnly, onboard: onboard)
        isListeningOnly = !policy.remaps
        mergesNativeModifiers = policy.mergesNativeModifiers
        traceEnabled = ModifierTrace.isEnabled
        // Only a tap that may modify events is created as a blocking tap.
        let tapListenOnly = !mergesNativeModifiers

        let types: [CGEventType] = [.keyDown, .keyUp, .flagsChanged, .leftMouseDown, .leftMouseUp, .rightMouseDown, .rightMouseUp, .otherMouseDown, .otherMouseUp, .scrollWheel, .mouseMoved, .leftMouseDragged, .rightMouseDragged, .otherMouseDragged]
        let mask = types.reduce(CGEventMask(0)) { $0 | (CGEventMask(1) << $1.rawValue) }

        var options: CGEventTapOptions = tapListenOnly ? .listenOnly : .defaultTap

        var tap = CGEvent.tapCreate(
            tap: .cgSessionEventTap,
            place: .headInsertEventTap,
            options: options,
            eventsOfInterest: CGEventMask(mask),
            callback: EventTapManager.eventCallback,
            userInfo: UnsafeMutableRawPointer(Unmanaged.passUnretained(self).toOpaque())
        )
        if tap == nil && !tapListenOnly {
            NSLog("[EventTap] Failed to create blocking event tap; falling back to listen-only. Enable Input Monitoring in System Settings.")
            options = .listenOnly
            tap = CGEvent.tapCreate(
                tap: .cgSessionEventTap,
                place: .headInsertEventTap,
                options: options,
                eventsOfInterest: CGEventMask(mask),
                callback: EventTapManager.eventCallback,
                userInfo: UnsafeMutableRawPointer(Unmanaged.passUnretained(self).toOpaque())
            )
            isListeningOnly = true
            mergesNativeModifiers = false
        }
        guard let tap = tap else {
            isListeningOnly = true
            mergesNativeModifiers = false
            notify()
            NSLog("[EventTap] Failed to create event tap. Check permissions.")
            return
        }

        eventTap = tap
        runLoopSource = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, tap, 0)

        if let source = runLoopSource {
            CFRunLoopAddSource(CFRunLoopGetCurrent(), source, .commonModes)
            CGEvent.tapEnable(tap: tap, enable: true)
            isRunning = true
            notify()
            NSLog("[EventTap] Started (listenOnly=\(options == .listenOnly), remapping=\(!isListeningOnly), nativeModifiers=\(mergesNativeModifiers)).")
            if traceEnabled { ModifierTrace.log("tap started remapping=\(!isListeningOnly) nativeModifiers=\(mergesNativeModifiers) onboard=\(onboard)") }
        }
    }

    func stop() {
        resetInputState()
        isRunning = false
        if let tap = eventTap {
            CGEvent.tapEnable(tap: tap, enable: false)
        }
        if let source = runLoopSource {
            CFRunLoopRemoveSource(CFRunLoopGetCurrent(), source, .commonModes)
        }
        eventTap = nil
        runLoopSource = nil
        notify()
    }

    func resetInputState() {
        activeDownButtons.removeAll()
        repeatOwnership.reset()
        scrollScaler.reset()
        for code in nativeKeys.reset() { ButtonMapper.shared.postNativeKey(code, flags: [], down: false) }
        ButtonMapper.shared.releaseAll()
        HIDListener.shared.resetCorrelation()
    }

    private func notify() {
        DispatchQueue.main.async { NotificationCenter.default.post(name: Self.didUpdateNotification, object: self) }
    }

    private static let eventCallback: CGEventTapCallBack = { (proxy, type, event, refcon) in
        guard let refcon = refcon else { return Unmanaged.passUnretained(event) }
        let manager = Unmanaged<EventTapManager>.fromOpaque(refcon).takeUnretainedValue()

        if event.getIntegerValueField(.eventSourceUserData) == ButtonMapper.syntheticMarker {
            ButtonMapper.shared.observeSyntheticModifier(event)
            return Unmanaged.passUnretained(event)
        }
        if type == .flagsChanged {
            ButtonMapper.shared.observePhysicalModifier(event)
            if manager.traceEnabled {
                ModifierTrace.log("flagsChanged code=\(event.getIntegerValueField(.keyboardEventKeycode)) flags=\(ModifierTrace.hex(event.flags)) observed=\(ModifierTrace.hex(ButtonMapper.shared.observedModifierFlags))")
            }
            return Unmanaged.passUnretained(event)
        }
        if type == .tapDisabledByTimeout || type == .tapDisabledByUserInput {
            manager.resetInputState()
            if let tap = manager.eventTap { CGEvent.tapEnable(tap: tap, enable: true) }
            return Unmanaged.passUnretained(event)
        }
        guard manager.isRemappingEnabled else { return manager.passKey(event, type) }
        if type == .scrollWheel { return manager.handleScroll(event) }
        if [.mouseMoved, .leftMouseDragged, .rightMouseDragged, .otherMouseDragged].contains(type),
           let drag = ButtonMapper.shared.dragEvent(for: event) {
            return Unmanaged.passRetained(drag)
        }
        let signature = InputRepeatOwnership.Signature(keyboardType: event.getIntegerValueField(.keyboardEventKeyboardType), sourcePID: event.getIntegerValueField(.eventSourceUnixProcessID), sourceState: event.getIntegerValueField(.eventSourceStateID))
        let down: Bool
        let button: Int?
        switch type {
        case .keyDown, .keyUp:
            button = KeyCodeMapper.buttonIndex(for: CGKeyCode(event.getIntegerValueField(.keyboardEventKeycode)))
            down = type == .keyDown
            if event.getIntegerValueField(.keyboardEventAutorepeat) != 0 {
                if let button, manager.repeatOwnership.matches(button: button, signature: signature, physicallyHeld: HIDListener.shared.isPhysicallyHeld(button)) { return nil }
                return manager.passKey(event, type)
            }
        case .leftMouseDown, .leftMouseUp: button = 18; down = type == .leftMouseDown
        case .rightMouseDown, .rightMouseUp: button = 19; down = type == .rightMouseDown
        case .otherMouseDown, .otherMouseUp:
            let number = event.getIntegerValueField(.mouseEventButtonNumber)
            button = number == 2 ? 17 : HIDListener.shared.usesRawTiltDecoding && (5...6).contains(number) ? Int(number) + 10 : nil
            down = type == .otherMouseDown
        default: return Unmanaged.passUnretained(event)
        }
        guard let button else { return manager.passKey(event, type) }
        guard HIDListener.shared.consume(buttonIndex: button, down: down, timestamp: Double(event.timestamp) / 1_000_000_000) else {
            if type == .keyDown || type == .keyUp { manager.repeatOwnership.invalidate(button: button) }
            return manager.passKey(event, type)
        }
        if down {
            guard ButtonMapper.shared.hasMapping(buttonIndex: button) else { return manager.passKey(event, type) }
            if HIDListener.shared.usesRawTiltDecoding && (15...16).contains(button) {
                manager.activeDownButtons.insert(button)
                return nil // Raw reports alone execute tilt actions in driver mode.
            }
            manager.activeDownButtons.insert(button)
            if type == .keyDown { manager.repeatOwnership.claim(button: button, signature: signature) }
            // The trigger's flags hold the modifiers the user is pressing now.
            if manager.traceEnabled, type == .keyDown {
                ModifierTrace.log("mapped button=\(button) down flags=\(ModifierTrace.hex(event.flags)) observed=\(ModifierTrace.hex(ButtonMapper.shared.observedModifierFlags))")
            }
            ButtonMapper.shared.handlePress(buttonIndex: button, inputFlags: event.flags)
            return nil
        }
        guard manager.activeDownButtons.remove(button) != nil else { return manager.passKey(event, type) }
        manager.repeatOwnership.invalidate(button: button)
        ButtonMapper.shared.handleRelease(buttonIndex: button, inputFlags: event.flags)
        return nil
    }

    /// Passes a key event that no mapping consumed. A key the mouse sent
    /// natively is re-sent when keyboard modifiers it lacks are held.
    private func passKey(_ event: CGEvent, _ type: CGEventType) -> Unmanaged<CGEvent>? {
        guard mergesNativeModifiers, type == .keyDown || type == .keyUp else { return Unmanaged.passUnretained(event) }
        let code = CGKeyCode(event.getIntegerValueField(.keyboardEventKeycode))
        let down = type == .keyDown
        let autorepeat = event.getIntegerValueField(.keyboardEventAutorepeat) != 0
        let fromMouse = !autorepeat && HIDListener.shared.consumeNativeKey(keyCode: code, down: down, timestamp: Double(event.timestamp) / 1_000_000_000)
        let missing = fromMouse || autorepeat ? ButtonMapper.shared.heldModifiersMissing(from: event.flags) : []
        let decision = nativeKeys.decide(keyCode: code, down: down, autorepeat: autorepeat, fromMouse: fromMouse,
                                         mouseHolds: HIDListener.shared.isNativeKeyHeld(keyCode: code), missingModifiers: missing)
        if traceEnabled, fromMouse {
            ModifierTrace.log("native key=\(code) \(down ? "down" : "up") flags=\(ModifierTrace.hex(event.flags)) observed=\(ModifierTrace.hex(ButtonMapper.shared.observedModifierFlags)) missing=\(ModifierTrace.hex(missing)) decision=\(decision)")
        }
        guard decision == .reroute else { return Unmanaged.passUnretained(event) }
        let deviceIndependent = CGEventFlags(rawValue: 0xffff0000)
        ButtonMapper.shared.postNativeKey(code, flags: event.flags.intersection(deviceIndependent).union(missing), down: down, autorepeat: autorepeat)
        return nil
    }

    private func handleScroll(_ event: CGEvent) -> Unmanaged<CGEvent>? {
        guard ScrollEventScaler.isDiscreteHardwareEvent(event) else { return Unmanaged.passUnretained(event) }
        let match = HIDListener.shared.consumeScroll(
            vertical: ScrollEventScaler.hasMotion(event, axis: .vertical),
            horizontal: ScrollEventScaler.hasMotion(event, axis: .horizontal),
            shifted: event.flags.contains(.maskShift),
            timestamp: Double(event.timestamp) / 1_000_000_000
        )
        guard !match.isEmpty else { return Unmanaged.passUnretained(event) }
        var suppressHorizontal = false
        if let button = match.horizontalButton, ButtonMapper.shared.hasMapping(buttonIndex: button) {
            suppressHorizontal = true
            if !HIDListener.shared.usesRawTiltDecoding && !HIDListener.shared.isRawControlHeld(button) {
                ButtonMapper.shared.handle(buttonIndex: button, inputFlags: event.flags)
            }
        }
        scrollScaler.apply(to: event, sensitivity: ConfigManager.shared.scrollSensitivity,
            vertical: match.vertical, horizontal: match.horizontalButton != nil || match.shiftedWheel,
            suppressHorizontal: suppressHorizontal)
        // A combined event can still carry vertical motion after consuming tilt.
        if !ScrollEventScaler.hasMotion(event, axis: .vertical)
            && !ScrollEventScaler.hasMotion(event, axis: .horizontal) { return nil }
        return Unmanaged.passUnretained(event)
    }
}
