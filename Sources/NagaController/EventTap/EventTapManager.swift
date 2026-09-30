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

    private(set) var isListeningOnly: Bool = true
    var isRemappingEnabled: Bool {
        get { return !isListeningOnly }
        set {
            let newListenOnly = !newValue
            if newListenOnly != isListeningOnly {
                start(listenOnly: newListenOnly)
            }
        }
    }

    private init() {}

    func start(listenOnly: Bool) {
        stop()
        let listenOnly = listenOnly || OnboardProfileStore.isActive
        isListeningOnly = listenOnly

        let types: [CGEventType] = [.keyDown, .keyUp, .leftMouseDown, .leftMouseUp, .rightMouseDown, .rightMouseUp, .otherMouseDown, .otherMouseUp, .scrollWheel, .mouseMoved, .leftMouseDragged, .rightMouseDragged, .otherMouseDragged]
        let mask = types.reduce(CGEventMask(0)) { $0 | (CGEventMask(1) << $1.rawValue) }

        var options: CGEventTapOptions = listenOnly ? .listenOnly : .defaultTap

        var tap = CGEvent.tapCreate(
            tap: .cgSessionEventTap,
            place: .headInsertEventTap,
            options: options,
            eventsOfInterest: CGEventMask(mask),
            callback: EventTapManager.eventCallback,
            userInfo: UnsafeMutableRawPointer(Unmanaged.passUnretained(self).toOpaque())
        )
        if tap == nil && !listenOnly {
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

        }
        guard let tap = tap else {
            isListeningOnly = true
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
            NSLog("[EventTap] Started (listenOnly=\(listenOnly)).")
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
            return Unmanaged.passUnretained(event)
        }
        if type == .tapDisabledByTimeout || type == .tapDisabledByUserInput {
            manager.resetInputState()
            if let tap = manager.eventTap { CGEvent.tapEnable(tap: tap, enable: true) }
            return Unmanaged.passUnretained(event)
        }
        guard !manager.isListeningOnly else { return Unmanaged.passUnretained(event) }
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
                return Unmanaged.passUnretained(event)
            }
        case .leftMouseDown, .leftMouseUp: button = 18; down = type == .leftMouseDown
        case .rightMouseDown, .rightMouseUp: button = 19; down = type == .rightMouseDown
        case .otherMouseDown, .otherMouseUp:
            let number = event.getIntegerValueField(.mouseEventButtonNumber)
            button = number == 2 ? 17 : HIDListener.shared.usesRawTiltDecoding && (5...6).contains(number) ? Int(number) + 10 : nil
            down = type == .otherMouseDown
        case .scrollWheel:
            let horizontal = event.getIntegerValueField(.scrollWheelEventDeltaAxis2)
            button = horizontal == 0 ? nil : horizontal > 0 ? 15 : 16
            down = true
        default: return Unmanaged.passUnretained(event)
        }
        guard let button else { return Unmanaged.passUnretained(event) }
        guard HIDListener.shared.consume(buttonIndex: button, down: down, timestamp: Double(event.timestamp) / 1_000_000_000) else {
            if type == .keyDown || type == .keyUp { manager.repeatOwnership.invalidate(button: button) }
            return Unmanaged.passUnretained(event)
        }
        if down {
            guard ButtonMapper.shared.hasMapping(buttonIndex: button) else { return Unmanaged.passUnretained(event) }
            if HIDListener.shared.usesRawTiltDecoding && (15...16).contains(button) {
                if type != .scrollWheel { manager.activeDownButtons.insert(button) }
                return nil // Raw reports alone execute tilt actions in driver mode.
            }
            if type == .scrollWheel {
                if !HIDListener.shared.isRawControlHeld(button) { ButtonMapper.shared.handle(buttonIndex: button) }
            }
            else {
                manager.activeDownButtons.insert(button)
                if type == .keyDown { manager.repeatOwnership.claim(button: button, signature: signature) }
                ButtonMapper.shared.handlePress(buttonIndex: button)
            }
            return nil
        }
        guard manager.activeDownButtons.remove(button) != nil else { return Unmanaged.passUnretained(event) }
        manager.repeatOwnership.invalidate(button: button)
        ButtonMapper.shared.handleRelease(buttonIndex: button)
        return nil
    }
}
