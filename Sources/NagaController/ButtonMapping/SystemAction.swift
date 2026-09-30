import Cocoa
import Carbon.HIToolbox
import IOKit.hidsystem

enum SystemActionGroup: String, CaseIterable {
    case audio = "Audio"
    case playback = "Playback"
    case brightness = "Brightness"
    case screenshots = "Screenshots"
    case windows = "Windows and Spaces"
    case tools = "Tools"

    var actions: [SystemAction] { SystemAction.allCases.filter { $0.group == self } }
}

enum SystemAction: String, Codable, CaseIterable {
    case volumeUp, volumeDown, mute
    case playPause, previousTrack, nextTrack
    case brightnessUp, brightnessDown
    case screenshot, screenshotSelection, screenshotOptions
    case screenshotClipboard, screenshotSelectionClipboard
    case missionControl, applicationWindows, showDesktop, previousSpace, nextSpace
    case hideApplication, switchApplication
    case spotlight, finder, systemSettings, notificationCenter, doNotDisturb

    init(audio: AudioAction) {
        switch audio {
        case .volumeUp: self = .volumeUp
        case .volumeDown: self = .volumeDown
        case .mute: self = .mute
        }
    }

    var group: SystemActionGroup {
        switch self {
        case .volumeUp, .volumeDown, .mute: return .audio
        case .playPause, .previousTrack, .nextTrack: return .playback
        case .brightnessUp, .brightnessDown: return .brightness
        case .screenshot, .screenshotSelection, .screenshotOptions,
             .screenshotClipboard, .screenshotSelectionClipboard: return .screenshots
        case .missionControl, .applicationWindows, .showDesktop, .previousSpace, .nextSpace,
             .hideApplication, .switchApplication: return .windows
        case .spotlight, .finder, .systemSettings, .notificationCenter, .doNotDisturb: return .tools
        }
    }

    var title: String {
        switch self {
        case .volumeUp: return "Volume up"
        case .volumeDown: return "Volume down"
        case .mute: return "Mute/unmute"
        case .playPause: return "Play/pause"
        case .previousTrack: return "Previous track"
        case .nextTrack: return "Next track"
        case .brightnessUp: return "Brightness up"
        case .brightnessDown: return "Brightness down"
        case .screenshot: return "Capture entire screen"
        case .screenshotSelection: return "Capture selection"
        case .screenshotOptions: return "Screenshot and recording options"
        case .screenshotClipboard: return "Entire screen to clipboard"
        case .screenshotSelectionClipboard: return "Selection to clipboard"
        case .missionControl: return "Mission Control"
        case .applicationWindows: return "Show application windows"
        case .showDesktop: return "Show Desktop"
        case .previousSpace: return "Move left a space"
        case .nextSpace: return "Move right a space"
        case .hideApplication: return "Hide current app"
        case .switchApplication: return "Switch to last app"
        case .spotlight: return "Open Spotlight"
        case .finder: return "Open Finder"
        case .systemSettings: return "Open System Settings"
        case .notificationCenter: return "Show Notification Center"
        case .doNotDisturb: return "Toggle Do Not Disturb"
        }
    }

    var symbol: String {
        switch self {
        case .volumeUp: return "speaker.plus.fill"
        case .volumeDown: return "speaker.minus.fill"
        case .mute: return "speaker.slash.fill"
        case .playPause: return "playpause.fill"
        case .previousTrack: return "backward.end.fill"
        case .nextTrack: return "forward.end.fill"
        case .brightnessUp: return "sun.max"
        case .brightnessDown: return "sun.min"
        case .screenshot: return "camera"
        case .screenshotSelection: return "viewfinder"
        case .screenshotOptions: return "camera.viewfinder"
        case .screenshotClipboard, .screenshotSelectionClipboard: return "clipboard"
        case .missionControl: return "rectangle.3.group"
        case .applicationWindows: return "macwindow.on.rectangle"
        case .showDesktop: return "menubar.dock.rectangle"
        case .previousSpace: return "rectangle.lefthalf.inset.filled.arrow.left"
        case .nextSpace: return "rectangle.righthalf.inset.filled.arrow.right"
        case .hideApplication: return "eye.slash"
        case .switchApplication: return "arrow.left.arrow.right"
        case .spotlight: return "magnifyingglass"
        case .finder: return "folder"
        case .systemSettings: return "gearshape"
        case .notificationCenter: return "bell"
        case .doNotDisturb: return "moon"
        }
    }

    var help: String {
        switch self {
        case .brightnessUp, .brightnessDown:
            return "Works like the Mac brightness keys. External displays must support brightness control from macOS."
        case .playPause, .previousTrack, .nextTrack:
            return "Controls the active media playback, like the Mac media keys."
        case .screenshot, .screenshotSelection:
            return "Saves to the location chosen in the macOS Screenshot options."
        case .screenshotOptions:
            return "Opens the macOS tools to capture the screen or a window, or to record a video."
        case .screenshotClipboard, .screenshotSelectionClipboard:
            return "Copies the capture to the clipboard instead of saving it as a file."
        case .previousSpace, .nextSpace:
            return "Moves to the adjacent space, if there is one."
        case .switchApplication:
            return "Switches to the last used app, like a quick Command-Tab."
        case .hideApplication:
            return "Hides the frontmost app without quitting it."
        case .doNotDisturb:
            return "Uses the Do Not Disturb shortcut configured in macOS. Focus settings can sync to your other devices."
        default:
            return "One action per press. Holding the button does not repeat it."
        }
    }

    var mediaKey: Int? {
        switch self {
        case .volumeUp: return Int(NX_KEYTYPE_SOUND_UP)
        case .volumeDown: return Int(NX_KEYTYPE_SOUND_DOWN)
        case .mute: return Int(NX_KEYTYPE_MUTE)
        case .brightnessUp: return Int(NX_KEYTYPE_BRIGHTNESS_UP)
        case .brightnessDown: return Int(NX_KEYTYPE_BRIGHTNESS_DOWN)
        case .playPause: return Int(NX_KEYTYPE_PLAY)
        // A short press on the Mac's seek keys skips a track.
        case .nextTrack: return Int(NX_KEYTYPE_FAST)
        case .previousTrack: return Int(NX_KEYTYPE_REWIND)
        default: return nil
        }
    }

    func mediaEvent(down: Bool) -> CGEvent? {
        guard let mediaKey else { return nil }
        let state = down ? 0xA : 0xB
        return NSEvent.otherEvent(with: .systemDefined, location: .zero,
            modifierFlags: NSEvent.ModifierFlags(rawValue: UInt(state << 8)),
            timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: 0,
            context: nil, subtype: 8, data1: (mediaKey << 16) | (state << 8), data2: -1)?.cgEvent
    }

    var shortcut: MacSystemShortcut? {
        switch self {
        case .screenshot: return .init(id: 28, keyCode: kVK_ANSI_3, flags: [.maskCommand, .maskShift])
        case .screenshotClipboard: return .init(id: 29, keyCode: kVK_ANSI_3, flags: [.maskCommand, .maskShift, .maskControl])
        case .screenshotSelection: return .init(id: 30, keyCode: kVK_ANSI_4, flags: [.maskCommand, .maskShift])
        case .screenshotSelectionClipboard: return .init(id: 31, keyCode: kVK_ANSI_4, flags: [.maskCommand, .maskShift, .maskControl])
        case .screenshotOptions: return .init(id: 184, keyCode: kVK_ANSI_5, flags: [.maskCommand, .maskShift])
        case .missionControl: return .init(id: 32, keyCode: kVK_UpArrow, flags: .maskControl)
        case .applicationWindows: return .init(id: 33, keyCode: kVK_DownArrow, flags: .maskControl)
        case .showDesktop: return .init(id: 36, keyCode: kVK_F11)
        case .previousSpace: return .init(id: 79, keyCode: kVK_LeftArrow, flags: .maskControl)
        case .nextSpace: return .init(id: 81, keyCode: kVK_RightArrow, flags: .maskControl)
        case .switchApplication: return .init(keyCode: kVK_Tab, flags: .maskCommand)
        case .notificationCenter: return .init(keyCode: kVK_ANSI_N, flags: .maskSecondaryFn)
        case .doNotDisturb: return .init(id: 175)
        default: return nil
        }
    }

    var applicationBundleIdentifier: String? {
        switch self {
        case .spotlight: return "com.apple.Spotlight"
        case .finder: return "com.apple.finder"
        case .systemSettings: return "com.apple.systempreferences"
        default: return nil
        }
    }

    var needsAccessibility: Bool { mediaKey != nil || shortcut != nil }

    var shortcutSetupMessage: String {
        let section = group == .screenshots ? "Screenshots" : "Mission Control"
        return "Assign and enable the shortcut in Keyboard > Keyboard Shortcuts > \(section)."
    }
}
