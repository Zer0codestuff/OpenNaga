import Cocoa

final class MappingWindowController: NSWindowController, NSWindowDelegate {
    static let shared = MappingWindowController()

    private init() {
        let vc = MappingViewController()
        let window = NSWindow(contentViewController: vc)
        window.styleMask = [.titled, .closable, .miniaturizable, .resizable]
        window.title = "OpenNaga"
        window.titleVisibility = .visible
        if #available(macOS 11.0, *) {
            window.toolbarStyle = .unified
        }
        window.isMovableByWindowBackground = false
        window.setContentSize(NSSize(width: 1180, height: 820))
        window.contentMinSize = NSSize(width: 980, height: 700)
        window.isReleasedWhenClosed = false
        if !CommandLine.arguments.contains("--snapshot") {
            window.setFrameAutosaveName("NagaWorkspace")
            if !window.setFrameUsingName("NagaWorkspace") { window.center() }
        } else { window.center() }

        super.init(window: window)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    func show() {
        guard let window else { return }

        // The menu bar app remains an accessory even while settings are open.
        NSApp.setActivationPolicy(.accessory)

        window.delegate = self
        showWindow(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    func windowWillClose(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
    }
}
