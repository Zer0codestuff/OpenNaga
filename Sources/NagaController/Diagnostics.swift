import Foundation
import ApplicationServices

enum NagaDiagnostics {
    static func run(verifyWrites: Bool = false, inspectOnboard: Bool = false, saveOnboard: Bool = false, restoreOnboard: Bool = false, output: URL? = nil) -> Int32 {
        var report: [String: Any] = [
            "device": "Razer Naga V2 HyperSpeed",
            "receiver": "1532:00b4",
            "accessibility": AXIsProcessTrusted(),
            "inputMonitoring": CGPreflightListenEventAccess(),
            "writesRequested": verifyWrites || saveOnboard || restoreOnboard
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
            guard !(saveOnboard && restoreOnboard) else { throw RazerHardwareError.invalidValue("Choose either save or restore.") }
            if saveOnboard {
                ConfigManager.shared.load()
                let plan = OnboardProfilePlan(name: ConfigManager.shared.currentProfileName, mapping: ConfigManager.shared.mappingForCurrentProfile())
                report["profile"] = plan.name
                report["planIssues"] = plan.issues
                try OnboardProfileStore.save(plan, session: session, identity: transport.identity)
                report["onboardSaveVerified"] = true
            }
            if restoreOnboard {
                try OnboardProfileStore.restore(session: session, identity: transport.identity)
                report["onboardRestoreVerified"] = true
            }
            if inspectOnboard || saveOnboard || restoreOnboard {
                report["onboardActive"] = try RazerOnboardBindings.read(session: session, profile: 0).map(\.bytes)
                report["onboardStored"] = try RazerOnboardBindings.read(session: session, profile: 1).map(\.bytes)
                report["onboardReadVerified"] = true
            }
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
