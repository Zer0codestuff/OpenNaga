import Foundation
import ApplicationServices

enum NagaDiagnostics {
    static func run(verifyWrites: Bool = false, output: URL? = nil) -> Int32 {
        var report: [String: Any] = [
            "device": "Razer Naga V2 HyperSpeed",
            "receiver": "1532:00b4",
            "accessibility": AXIsProcessTrusted(),
            "inputMonitoring": CGPreflightListenEventAccess(),
            "writesRequested": verifyWrites
        ]
        let transport = MacRazerUSBTransport()
        var status: Int32 = 0
        do {
            try transport.open()
            defer { transport.close() }
            let session = RazerHardwareSession(transport: transport)
            let snapshot = try session.readSnapshot()
            report["dpiX"] = snapshot.dpiX
            report["dpiY"] = snapshot.dpiY
            report["pollingRate"] = snapshot.pollingRate
            report["batteryPercent"] = snapshot.batteryLevel
            report["mode"] = snapshot.mode
            report["readVerified"] = true
            report["warnings"] = snapshot.warnings
            if verifyWrites {
                guard let x = snapshot.dpiX, let y = snapshot.dpiY, let rate = snapshot.pollingRate else {
                    throw RazerHardwareError.invalidValue("Read DPI and polling rate before verifying writes.")
                }
                // Exercise both setters without changing the user's sensitivity.
                try session.setDPI(x: x, y: y)
                report["dpiWriteReadbackVerified"] = true
                try session.setPolling(rate)
                report["pollingWriteReadbackVerified"] = true
            }
        } catch {
            report["error"] = error.localizedDescription
            status = 1
        }
        do {
            let data = try JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys])
            if let output { try data.write(to: output, options: .atomic) }
            else {
                FileHandle.standardOutput.write(data)
                FileHandle.standardOutput.write(Data("\n".utf8))
            }
        } catch {
            fputs("Diagnostic output failed: \(error.localizedDescription)\n", stderr)
            return 1
        }
        return status
    }
}
