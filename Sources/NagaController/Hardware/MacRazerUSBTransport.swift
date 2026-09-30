import Foundation
import IOKit
import IOKit.hid

// Uses only the receiver's mouse collection advertising a 90-byte feature report.
// Parent enumeration confirmed UsagePage=1, Usage=2, MaxFeatureReportSize=90.
// IOHID translates feature report zero to HID class SET_REPORT/GET_REPORT.
// OpenRazer addresses interface zero. No interface probing, seizure, or undocumented
// writes are attempted. IOUSBHost is intentionally not an automatic fallback:
// acquiring its exclusive user client can conflict with the system HID driver.
// Apple API: https://developer.apple.com/documentation/iokit/iohiddevice_h
final class MacRazerUSBTransport: RazerTransport {
    private var device: IOHIDDevice?
    private var runLoop: CFRunLoop?
    private(set) var identity = "1532:00b4"
    private let timeout: TimeInterval = 1

    func open() throws {
        if device != nil { return }
        let manager = IOHIDManagerCreate(kCFAllocatorDefault, IOOptionBits(kIOHIDOptionsTypeNone))
        IOHIDManagerSetDeviceMatching(manager, [
            kIOHIDVendorIDKey: 0x1532,
            kIOHIDProductIDKey: 0x00b4,
            kIOHIDPrimaryUsagePageKey: 1,
            kIOHIDPrimaryUsageKey: 2
        ] as CFDictionary)
        // CopyDevices enumerates services without opening unrelated interfaces.
        guard let candidates = IOHIDManagerCopyDevices(manager) as? Set<IOHIDDevice> else {
            throw RazerHardwareError.disconnected
        }
        let supported = candidates.filter {
            (IOHIDDeviceGetProperty($0, kIOHIDTransportKey as CFString) as? String) == "USB"
            && (IOHIDDeviceGetProperty($0, kIOHIDMaxFeatureReportSizeKey as CFString) as? NSNumber)?.intValue == 90
        }
        guard supported.count == 1, let selected = supported.first else {
            throw RazerHardwareError.transport("USB interface missing or ambiguous. Connect a single Naga V2 HyperSpeed receiver.")
        }
        let result = IOHIDDeviceOpen(selected, IOOptionBits(kIOHIDOptionsTypeNone))
        guard result == kIOReturnSuccess else { throw ioError("USB open", result) }
        device = selected
        let loop = CFRunLoopGetCurrent()!
        runLoop = loop
        IOHIDDeviceScheduleWithRunLoop(selected, loop, CFRunLoopMode.defaultMode.rawValue)
        var registryID: UInt64 = 0
        IORegistryEntryGetRegistryEntryID(IOHIDDeviceGetService(selected), &registryID)
        // Location is stable across reopen, unlike registry entry ID. It also
        // prevents recovery records from being applied to another USB port.
        let location = (IOHIDDeviceGetProperty(selected, kIOHIDLocationIDKey as CFString) as? NSNumber)?.uint32Value
        identity = "1532:00b4:\(location.map(String.init) ?? String(registryID))"
    }

    func exchange(_ request: [UInt8]) throws -> [UInt8] {
        guard request.count == 90 else { throw RazerHardwareError.invalidValue("Invalid USB report.") }
        guard let device else { throw RazerHardwareError.disconnected }
        _ = try transfer(device: device, bytes: request, reading: false)
        // Receiver-specific response wait in OpenRazer is 31ms.
        Thread.sleep(forTimeInterval: 0.035)
        return try transfer(device: device, bytes: [UInt8](repeating: 0, count: 90), reading: true)
    }

    private final class Transfer {
        let buffer = UnsafeMutablePointer<UInt8>.allocate(capacity: 90)
        let lengthPointer = UnsafeMutablePointer<CFIndex>.allocate(capacity: 1)
        var result: IOReturn?
        var length = 90
        init(_ bytes: [UInt8]) {
            buffer.initialize(from: bytes, count: bytes.count)
            lengthPointer.initialize(to: 90)
        }
        deinit {
            buffer.deinitialize(count: 90); buffer.deallocate()
            lengthPointer.deinitialize(count: 1); lengthPointer.deallocate()
        }
    }
    private func transfer(device: IOHIDDevice, bytes: [UInt8], reading: Bool) throws -> [UInt8] {
        let transfer = Transfer(bytes)
        // Callback owns an extra retain. If macOS never calls back after an
        // aborted request, this small allocation is intentionally retained:
        // freeing it while IOKit may still reference the buffer is unsafe.
        let context = Unmanaged.passRetained(transfer).toOpaque()
        let callback: IOHIDReportCallback = { context, result, _, _, _, _, length in
            guard let context else { return }
            let operation = Unmanaged<Transfer>.fromOpaque(context).takeRetainedValue()
            operation.length = length
            operation.result = result
        }
        let result: IOReturn
        if reading {
            result = IOHIDDeviceGetReportWithCallback(device, kIOHIDReportTypeFeature, 0,
                transfer.buffer, transfer.lengthPointer, timeout, callback, context)
        } else {
            result = IOHIDDeviceSetReportWithCallback(device, kIOHIDReportTypeFeature, 0,
                transfer.buffer, 90, timeout, callback, context)
        }
        guard result == kIOReturnSuccess else {
            Unmanaged<Transfer>.fromOpaque(context).release()
            throw ioError(reading ? "USB read" : "USB write", result)
        }
        let deadline = Date().addingTimeInterval(timeout + 0.25)
        while transfer.result == nil, Date() < deadline {
            CFRunLoopRunInMode(.defaultMode, 0.025, true)
        }
        guard let completion = transfer.result else {
            close()
            throw RazerHardwareError.transport("USB timeout. Reconnect the receiver.")
        }
        guard completion == kIOReturnSuccess else { throw ioError("USB transfer", completion) }
        guard !reading || transfer.length == 90 else {
            throw RazerHardwareError.malformed("Incomplete USB report: \(transfer.length) bytes.")
        }
        return Array(UnsafeBufferPointer(start: transfer.buffer, count: 90))
    }
    func close() {
        if let device {
            IOHIDDeviceClose(device, IOOptionBits(kIOHIDOptionsTypeNone))
            if let runLoop { IOHIDDeviceUnscheduleFromRunLoop(device, runLoop, CFRunLoopMode.defaultMode.rawValue) }
        }
        device = nil
        runLoop = nil
    }
    private func ioError(_ operation: String, _ code: IOReturn) -> RazerHardwareError {
        .transport("\(operation): IOKit error \(String(format: "0x%08x", code)). Check USB access and the Input Monitoring permission.")
    }
}
