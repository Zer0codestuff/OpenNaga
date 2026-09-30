import Cocoa

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    private var statusItem: NSStatusItem!
    private let popover = NSPopover()
    private var observers: [NSObjectProtocol] = []
    private var permissionTimer: Timer?
    private var hadInputPermission = false
    private var hadAccessibilityPermission = false
    private var isTerminating = false
    private var remappingActivity: NSObjectProtocol?
    private var hardwareRefreshTimer: Timer?
    private var observedDeviceName: String?
    private var observedTransport: String?

    private var snapshotPath: String? {
        let args = CommandLine.arguments
        guard let index = args.firstIndex(of: "--snapshot"), args.indices.contains(index + 1) else { return nil }
        return args[index + 1]
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        if snapshotPath != nil, let appearance = argument(after: "--snapshot-appearance") {
            NSApp.appearance = NSAppearance(named: appearance == "dark" ? .darkAqua : .aqua)
        }
        ConfigManager.shared.load()
        installApplicationMenu()

        if snapshotPath == nil {
            observers.append(NotificationCenter.default.addObserver(forName: HIDListener.didUpdateNotification, object: nil, queue: .main) { [weak self] _ in
                Task { @MainActor in self?.checkHardwareConnection() }
            })
            PermissionManager.shared.requestMissingPermissions()
            HIDListener.shared.start()
            RazerDeviceController.shared.refresh()
            hadInputPermission = PermissionManager.shared.hasInputMonitoringPermission()
            hadAccessibilityPermission = PermissionManager.shared.hasAccessibilityPermission()
            startRemappingIfPermitted()
            permissionTimer = Timer.scheduledTimer(withTimeInterval: 2, repeats: true) { [weak self] _ in
                Task { @MainActor in self?.checkPermissions() }
            }
        }

        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        statusItem.button?.image = NSImage(systemSymbolName: "computermouse", accessibilityDescription: "OpenNaga")
        statusItem.button?.image?.isTemplate = true
        statusItem.button?.target = self
        statusItem.button?.action = #selector(togglePopover)
        popover.behavior = .transient
        popover.contentViewController = MainViewController()
        for name in [RazerDeviceController.didUpdateNotification, EventTapManager.didUpdateNotification] {
            observers.append(NotificationCenter.default.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
                Task { @MainActor in
                    if name == RazerDeviceController.didUpdateNotification {
                        HIDListener.shared.setDriverModeEnabled(RazerDeviceController.shared.driverModeEnabled)
                    }
                    self?.updateStatusItem()
                }
            })
        }
        updateStatusItem()
        MappingWindowController.shared.show()

        if let path = snapshotPath {
            if let value = argument(after: "--snapshot-size") {
                let parts = value.split(separator: "x").compactMap { Double($0) }
                if parts.count == 2, parts[0] >= 980, parts[1] >= 700 {
                    MappingWindowController.shared.window?.setContentSize(NSSize(width: parts[0], height: parts[1]))
                }
            }
            DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
                self.captureWindow(to: path)
            }
        }
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        MappingWindowController.shared.show()
        return true
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { false }

    func applicationWillTerminate(_ notification: Notification) {
        permissionTimer?.invalidate()
        hardwareRefreshTimer?.invalidate()
        if let remappingActivity { ProcessInfo.processInfo.endActivity(remappingActivity) }
        remappingActivity = nil
        EventTapManager.shared.stop()
        HIDListener.shared.stop()
        observers.forEach(NotificationCenter.default.removeObserver)
    }

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        guard snapshotPath == nil else { return .terminateNow }
        guard !isTerminating else { return .terminateLater }
        isTerminating = true
        hardwareRefreshTimer?.invalidate()
        EventTapManager.shared.stop()
        DispatchQueue.main.async {
            RazerDeviceController.shared.restoreOriginalMode {
                sender.reply(toApplicationShouldTerminate: true)
            }
        }
        return .terminateLater
    }

    private func startRemappingIfPermitted() {
        let permission = PermissionManager.shared
        guard permission.hasAccessibilityPermission(), permission.hasInputMonitoringPermission() else { return }
        EventTapManager.shared.start(listenOnly: !ConfigManager.shared.getRemappingEnabled())
    }

    private func checkHardwareConnection() {
        let listener = HIDListener.shared
        guard observedDeviceName != listener.connectedDeviceName || observedTransport != listener.transport else { return }
        observedDeviceName = listener.connectedDeviceName
        observedTransport = listener.transport
        scheduleHardwareRefresh()
    }

    private func scheduleHardwareRefresh() {
        hardwareRefreshTimer?.invalidate()
        guard !isTerminating else { return }
        // Coalesce the receiver's HID collections and wait for any active hardware operation.
        hardwareRefreshTimer = Timer.scheduledTimer(withTimeInterval: 0.4, repeats: false) { [weak self] _ in
            Task { @MainActor in
                guard let self, !self.isTerminating else { return }
                if RazerDeviceController.shared.isBusy {
                    self.scheduleHardwareRefresh()
                } else {
                    RazerDeviceController.shared.refresh()
                }
            }
        }
    }

    private func checkPermissions() {
        let permission = PermissionManager.shared
        let input = permission.hasInputMonitoringPermission()
        if input && !hadInputPermission {
            HIDListener.shared.stop()
            HIDListener.shared.start()
            RazerDeviceController.shared.refresh()
        }
        let accessibility = permission.hasAccessibilityPermission()
        if hadInputPermission != input || hadAccessibilityPermission != accessibility {
            NotificationCenter.default.post(name: PermissionManager.didUpdateNotification, object: nil)
        }
        hadInputPermission = input
        hadAccessibilityPermission = accessibility
        if !permission.hasAccessibilityPermission() || !input {
            if EventTapManager.shared.isRunning { EventTapManager.shared.stop() }
        } else if !EventTapManager.shared.isRunning {
            startRemappingIfPermitted()
        }
        updateStatusItem()
    }

    private func updateStatusItem() {
        let hardware = RazerDeviceController.shared
        statusItem?.button?.title = hardware.batteryLevel.map { " \($0)%" } ?? ""
        let active = EventTapManager.shared.isRunning && EventTapManager.shared.isRemappingEnabled
        // Keep remapping responsive under App Nap while allowing normal system sleep.
        if active && remappingActivity == nil {
            remappingActivity = ProcessInfo.processInfo.beginActivity(options: .userInitiatedAllowingIdleSystemSleep, reason: "Mouse button remapping")
        } else if !active, let activity = remappingActivity {
            ProcessInfo.processInfo.endActivity(activity)
            remappingActivity = nil
        }
        statusItem?.button?.toolTip = active ? "OpenNaga · Remapping active" : "OpenNaga · Remapping paused"
    }

    @objc private func togglePopover(_ sender: Any?) {
        guard let button = statusItem.button else { return }
        if popover.isShown {
            popover.performClose(sender)
        } else {
            (popover.contentViewController as? MainViewController)?.refreshPermissionStatuses()
            popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
        }
    }

    @objc private func showSettings() {
        popover.performClose(nil)
        MappingWindowController.shared.show()
    }

    private func installApplicationMenu() {
        let menu = NSMenu()
        let appItem = NSMenuItem()
        let appMenu = NSMenu()
        let settings = NSMenuItem(title: "Settings…", action: #selector(showSettings), keyEquivalent: ",")
        settings.target = self
        appMenu.addItem(settings)
        let close = NSMenuItem(title: "Close Window", action: #selector(NSWindow.performClose(_:)), keyEquivalent: "w")
        appMenu.addItem(close)
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: "Quit OpenNaga", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        appItem.submenu = appMenu
        menu.addItem(appItem)
        let editItem = NSMenuItem()
        editItem.title = "Edit"
        let edit = NSMenu(title: "Edit")
        edit.addItem(withTitle: "Undo", action: Selector(("undo:")), keyEquivalent: "z")
        edit.addItem(withTitle: "Cut", action: #selector(NSText.cut(_:)), keyEquivalent: "x")
        edit.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        edit.addItem(withTitle: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
        edit.addItem(withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
        editItem.submenu = edit
        menu.addItem(editItem)
        NSApp.mainMenu = menu
    }

    private func argument(after option: String) -> String? {
        let args = CommandLine.arguments
        guard let index = args.firstIndex(of: option), args.indices.contains(index + 1) else { return nil }
        return args[index + 1]
    }

    private func captureWindow(to path: String) {
        guard let view = MappingWindowController.shared.window?.contentView,
              let bitmap = view.bitmapImageRepForCachingDisplay(in: view.bounds) else {
            fputs("Unable to capture the settings window.\n", stderr)
            NSApp.terminate(nil)
            return
        }
        view.layoutSubtreeIfNeeded()
        view.cacheDisplay(in: view.bounds, to: bitmap)
        do {
            // cacheDisplay excludes the native window backing. Composite it explicitly.
            let size = view.bounds.size
            let rendered = NSImage(size: size)
            rendered.lockFocus()
            NSColor.windowBackgroundColor.setFill()
            NSRect(origin: .zero, size: size).fill()
            bitmap.draw(in: NSRect(origin: .zero, size: size))
            rendered.unlockFocus()
            guard let tiff = rendered.tiffRepresentation,
                  let flattened = NSBitmapImageRep(data: tiff),
                  let png = flattened.representation(using: .png, properties: [:]) else { throw CocoaError(.fileWriteUnknown) }
            try png.write(to: URL(fileURLWithPath: path), options: .atomic)
            print("UI snapshot: \(path)")
        } catch {
            fputs("Snapshot failed: \(error.localizedDescription)\n", stderr)
        }
        NSApp.terminate(nil)
    }
}
