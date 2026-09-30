import Foundation
import IOKit.hid
import Darwin

final class HIDListener {
    static let shared = HIDListener()
    static let didUpdateNotification = Notification.Name("NagaHIDDidUpdate")
    static let deviceMatchingCriteria: [[String: Int]] = [
        [kIOHIDVendorIDKey: 0x1532],
        [kIOHIDVendorIDKey: NagaInput.bluetoothIdentity.vendor,
         kIOHIDProductIDKey: NagaInput.bluetoothIdentity.product]
    ]
    private(set) var connectedDeviceName: String?
    private(set) var transport: String?
    private(set) var lastInputDescription = "No input detected. The DPI buttons require a recognized driver report."
    private let lock = NSLock()
    private var matcher = InputEdgeMatcher()
    private var runLoop: CFRunLoop?
    private var thread: Thread?
    private var manager: IOHIDManager?
    private var stopRequested = false
    private var devices: [String: (String, String?)] = [:]
    private var rawState = DriverButtonState()
    private var driverModeEnabled = false
    private var heldButtons = Set<Int>()
    private var desiredRunning = false
    private var valueStates: [String: Bool] = [:]
    private init() {}

    // Explicit lifecycle keeps pure tests and model access free of hardware I/O.
    func start() {
        desiredRunning = true
        guard thread == nil else { return }
        lock.lock(); stopRequested = false; lock.unlock()
        let worker = Thread { [self] in
            let loop = CFRunLoopGetCurrent()!
            lock.lock(); runLoop = loop; lock.unlock()
            let manager = IOHIDManagerCreate(kCFAllocatorDefault, 0)
            self.manager = manager
            // BLE uses a different vendor ID. Do not enumerate that entire vendor.
            IOHIDManagerSetDeviceMatchingMultiple(manager, Self.deviceMatchingCriteria as CFArray)
            let context = Unmanaged.passUnretained(self).toOpaque()
            IOHIDManagerRegisterDeviceMatchingCallback(manager, { context, _, _, device in
                guard let context else { return }
                Unmanaged<HIDListener>.fromOpaque(context).takeUnretainedValue().connected(device)
            }, context)
            IOHIDManagerRegisterDeviceRemovalCallback(manager, { context, _, _, device in
                guard let context else { return }
                Unmanaged<HIDListener>.fromOpaque(context).takeUnretainedValue().disconnected(device)
            }, context)
            IOHIDManagerRegisterInputValueCallback(manager, { context, _, _, value in
                guard let context else { return }
                Unmanaged<HIDListener>.fromOpaque(context).takeUnretainedValue().handle(value)
            }, context)
            IOHIDManagerRegisterInputReportCallback(manager, { context, _, sender, _, _, bytes, count in
                guard let context, let sender else { return }
                let device = Unmanaged<IOHIDDevice>.fromOpaque(sender).takeUnretainedValue()
                Unmanaged<HIDListener>.fromOpaque(context).takeUnretainedValue().handleReport(device, bytes: Array(UnsafeBufferPointer(start: bytes, count: count)))
            }, context)
            IOHIDManagerScheduleWithRunLoop(manager, loop, CFRunLoopMode.defaultMode.rawValue)
            let result = IOHIDManagerOpen(manager, 0)
            lock.lock(); let shouldRun = !stopRequested; lock.unlock()
            if result == kIOReturnSuccess && shouldRun { CFRunLoopRun() }
            else if result != kIOReturnSuccess {
                DispatchQueue.main.async { self.desiredRunning = false }
                publish { self.lastInputDescription = "HID access unavailable: \(result)" }
            }
            IOHIDManagerUnscheduleFromRunLoop(manager, loop, CFRunLoopMode.defaultMode.rawValue)
            IOHIDManagerClose(manager, 0)
            devices.removeAll(); valueStates.removeAll()
            lock.lock(); runLoop = nil; rawState.reset(); heldButtons.removeAll(); lock.unlock()
            DispatchQueue.main.async {
                self.thread = nil
                self.manager = nil
                if self.desiredRunning { self.start() }
            }
        }
        worker.name = "Naga HID input"
        thread = worker
        worker.start()
    }

    var usesRawTiltDecoding: Bool {
        lock.lock(); defer { lock.unlock() }
        return driverModeEnabled
    }

    func stop() {
        desiredRunning = false
        lock.lock(); stopRequested = true; let loop = runLoop; lock.unlock()
        if let loop { CFRunLoopStop(loop) }
        // Do not join or wait on an event callback.
        resetCorrelation()
        EventTapManager.shared.resetInputState()
        publish { self.connectedDeviceName = nil; self.transport = nil }
    }

    func resetCorrelation() {
        lock.lock(); matcher.reset(); lock.unlock()
    }

    func consume(buttonIndex: Int, down: Bool, timestamp: TimeInterval) -> Bool {
        lock.lock(); defer { lock.unlock() }
        return matcher.consume(button: buttonIndex, down: down, timestamp: timestamp)
    }

    private func identity(_ device: IOHIDDevice) -> String { String(describing: Unmanaged.passUnretained(device).toOpaque()) }
    private func supported(_ device: IOHIDDevice) -> Bool {
        NagaInput.isSupported(vendor: IOHIDDeviceGetProperty(device, kIOHIDVendorIDKey as CFString) as? Int ?? 0,
                              product: IOHIDDeviceGetProperty(device, kIOHIDProductIDKey as CFString) as? Int ?? 0,
                              name: IOHIDDeviceGetProperty(device, kIOHIDProductKey as CFString) as? String)
    }

    private func connected(_ device: IOHIDDevice) {
        guard supported(device) else { return }
        let name = IOHIDDeviceGetProperty(device, kIOHIDProductKey as CFString) as? String ?? "Razer Naga"
        let transport = IOHIDDeviceGetProperty(device, kIOHIDTransportKey as CFString) as? String
        devices[identity(device)] = (name, transport)
        NSLog("[HID] Naga connected via %@", transport ?? "unknown transport")
        publish { self.connectedDeviceName = name; self.transport = transport }
    }

    private func disconnected(_ device: IOHIDDevice) {
        guard devices.removeValue(forKey: identity(device)) != nil else { return }
        lock.lock(); rawState.reset(); heldButtons.removeAll(); lock.unlock()
        valueStates.removeAll(); resetCorrelation()
        let remaining = devices.values.first
        publish {
            self.connectedDeviceName = remaining?.0; self.transport = remaining?.1
            EventTapManager.shared.resetInputState()
        }
    }

    private func handle(_ value: IOHIDValue) {
        let element = IOHIDValueGetElement(value)
        let device = IOHIDElementGetDevice(element)
        guard supported(device) else { return }
        let page = IOHIDElementGetUsagePage(element), usage = IOHIDElementGetUsage(element)
        let integer = IOHIDValueGetIntegerValue(value)
        let rawTilt = usesRawTiltDecoding &&
            (IOHIDDeviceGetProperty(device, kIOHIDProductIDKey as CFString) as? Int) == 0x00b4 &&
            page == 9 && (usage == 6 || usage == 7)
        guard let button = rawTilt ? (usage == 6 ? 15 : 16) : NagaInput.button(page: page, usage: usage, value: integer) else { return }
        let down = integer != 0
        let pan = page == 0x0c && usage == 0x238
        let key = "\(identity(device))-\(page)-\(usage)"
        if !pan {
            guard valueStates[key, default: false] != down else { return }
            valueStates[key] = down
        }
        var info = mach_timebase_info_data_t()
        mach_timebase_info(&info)
        let timestamp = Double(IOHIDValueGetTimeStamp(value)) * Double(info.numer) / Double(info.denom) / 1_000_000_000
        lock.lock()
        matcher.record(button: button, down: down, timestamp: timestamp)
        if !pan { if down { heldButtons.insert(button) } else { heldButtons.remove(button) } }
        lock.unlock()
        publishInput {
            self.lastInputDescription = "Button \(button): \(down ? "pressed" : "released")"
            if down { NotificationCenter.default.post(name: Notification.Name("NagaButtonActivity"), object: self, userInfo: ["buttonIndex": button]) }
            // Release outputs even if the system key-up was not correlated in time.
            if !down && !(self.usesRawTiltDecoding && (15...16).contains(button)) {
                ButtonMapper.shared.handleRelease(buttonIndex: button)
            }
        }
    }

    func setDriverModeEnabled(_ enabled: Bool) {
        lock.lock()
        let changed = driverModeEnabled != enabled
        driverModeEnabled = enabled
        if changed { rawState.reset(); matcher.reset() }
        lock.unlock()
        if changed { EventTapManager.shared.resetInputState() }
    }

    func isPhysicallyHeld(_ button: Int) -> Bool {
        lock.lock(); defer { lock.unlock() }
        return heldButtons.contains(button)
    }

    func isRawControlHeld(_ button: Int) -> Bool {
        lock.lock(); defer { lock.unlock() }
        return rawState.held.contains(button)
    }

    private func handleReport(_ device: IOHIDDevice, bytes: [UInt8]) {
        guard supported(device),
              (IOHIDDeviceGetProperty(device, kIOHIDProductIDKey as CFString) as? Int) == 0x00b4 else { return }
        let usage = IOHIDDeviceGetProperty(device, kIOHIDPrimaryUsageKey as CFString) as? Int
        lock.lock(); let mode = driverModeEnabled; lock.unlock()
        let decoded: Set<Int>?
        if usage == 6 {
            decoded = NagaInput.driverButtons(report: bytes).map { mode ? $0 : $0.intersection([13, 14]) }
        }
        else if usage == 2 && mode { decoded = NagaInput.driverMouseButtons(report: bytes) }
        else { decoded = nil }
        guard let buttons = decoded else { return }
        lock.lock()
        let changes = rawState.update(source: identity(device), buttons: buttons)
        lock.unlock()
        guard !changes.released.isEmpty || !changes.pressed.isEmpty else { return }
        publishInput {
            for button in changes.released { ButtonMapper.shared.handleRelease(buttonIndex: button) }
            for button in changes.pressed {
                self.lastInputDescription = "Button \(button): recognized driver report"
                NotificationCenter.default.post(name: Notification.Name("NagaButtonActivity"), object: self, userInfo: ["buttonIndex": button])
                if EventTapManager.shared.isRunning && EventTapManager.shared.isRemappingEnabled {
                    ButtonMapper.shared.handlePress(buttonIndex: button)
                }
            }
        }
    }

    private func publishInput(_ body: @escaping () -> Void) {
        DispatchQueue.main.async {
            self.lock.lock(); let stopped = self.stopRequested; self.lock.unlock()
            guard !stopped else { return }
            body()
            NotificationCenter.default.post(name: Self.didUpdateNotification, object: self)
        }
    }

    private func publish(_ body: @escaping () -> Void) {
        DispatchQueue.main.async { body(); NotificationCenter.default.post(name: Self.didUpdateNotification, object: self) }
    }
}
