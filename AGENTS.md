# AGENTS.md

## Purpose

OpenNaga: macOS menu bar app (Swift, AppKit + SwiftUI, no Xcode project) that remaps the buttons of the Razer Naga V2 HyperSpeed and controls DPI / polling rate over the USB receiver `1532:00b4`. Standalone project that started as a fork of DParent10/NagaController (credited in README and LICENSE); remote `origin` is `Zer0codestuff/OpenNaga`. Not affiliated with Razer.

Naming: the shipped app is `OpenNaga.app` (executable `OpenNaga`, display name OpenNaga). The SwiftPM package/product/target, `Sources/NagaController`, the bundle id `com.zer0codestuff.NagaController`, the `~/Library/Application Support/NagaController` folder and the "NagaController Dev" signing identity intentionally keep the old name so TCC grants and profiles carry over.

## Architecture

- `Sources/NagaController/main.swift`: CLI switches (`--diagnose`, `--diagnose-file <path>`, `--verify-hardware`, `--snapshot <png>`), otherwise starts `AppDelegate`.
- `AppDelegate`: starts `HIDListener`, `RazerDeviceController.refresh()`, the event tap (only when Accessibility and Input Monitoring are granted), a 2 s permission poll, menu bar item and popover; defers quit until `restoreOriginalMode` completes.
- `ButtonMapping/`: `ActionType` (system, legacy audio, mouse, disabled, keySequence, application, systemCommand, textSnippet, macro, profileSwitch), `ButtonMapper` (press/hold/release semantics, synthetic marker `eventSourceUserData`), `KeyboardLayoutShortcut` (browser back/forward per layout). `SystemAction` groups native system controls; `MacSystemShortcut` reads configured macOS shortcuts without changing preferences.
- `EventTap/EventTapManager`: CGEvent tap; consumes an event only if `HIDListener.consume` matches a HID edge within 25 ms. Logical indices: 1..12 side grid, 13 DPI up, 14 DPI down, 15 wheel left, 16 wheel right, 17 middle, 18 left, 19 right.
- `HID/HIDListener` + `HID/InputModel`: IOHIDManager on a dedicated thread, pure decoders (`NagaInput`, `InputEdgeMatcher`, `DriverButtonState`). Enumeration includes vendor `1532` and exact BLE identity `068e:00b5`; callbacks additionally validate Naga identity.
- `Hardware/`: `RazerProtocol` (90-byte report codec, CRC, transaction IDs), `MacRazerUSBTransport` (IOHID feature reports on the mouse collection with 90-byte feature size), `RazerDeviceController` (`@MainActor` facade, serial worker queue, driver-mode recovery journal in `~/Library/Application Support/NagaController/driver-mode-recovery.json`).
- `UI/`: `MappingViewController` (NSHostingController with `NagaWorkspace`), `ActionInspector`, `SettingsPanes` (Sensitivity, Status, ProfileManager), `MainViewController` (popover), `MappingWindowController`.
- `Utils/ConfigManager`: profiles JSON, auto-save, `didChangeNotification`, `lastError`.

## Build, run, test

```bash
swift build                      # debug
bash Scripts/make_dev_certificate.sh   # once: self-signed "NagaController Dev" identity, keeps TCC grants across rebuilds
bash Scripts/build_app.sh        # release bundle ./OpenNaga.app, signed with the dev identity if present, else ad-hoc
bash Scripts/test.sh             # 745 dependency-free checks (XCTest is not available with CLI tools only)
bash Scripts/make_dmg.sh         # ad-hoc signed release DMG (OpenNaga-v<version>.dmg, git-ignored)
gh release create vX.Y.Z OpenNaga-vX.Y.Z.dmg --title "OpenNaga X.Y.Z" --notes-file <file>   # publish
open OpenNaga.app --args --diagnose-file /tmp/naga.json   # read-only hardware probe
./OpenNaga.app/Contents/MacOS/OpenNaga --snapshot /tmp/ui.png  # UI render without hardware
```

`Package.swift` also declares a `TapTester` executable and an XCTest target (`Tests/NagaControllerTests`) that cannot run without Xcode.

## Current status (2026-09-30)

- Version 2.2.0, build 6: rename to OpenNaga plus English UI, released from branch `english-ui` (worktree `../NagaController-english`). The main checkout `../RazerNagaV2` still holds uncommitted onboard-profile work based on 2.1.1, with Italian strings; it must be rebased onto 2.2.0, translated and renamed before it can ship.
- Version 2.1.1, build 5, is published as GitHub release v2.1.1 with `NagaController-v2.1.1.dmg` (Apple Silicon, ad-hoc signed, not notarized). It adds Bluetooth detection on top of 2.1.0 (Sistema, native UI, keyboard picker, background/hotplug fixes).
- The Bluetooth fix is installed in `/Applications/NagaController.app` with the existing development signing identity. Release build and 745 dependency-free checks pass. Startup logs confirm `Naga connected via Bluetooth Low Energy` and an active blocking event tap. The profile JSON stayed byte-identical across installation and relaunch; physical Bluetooth button actions and switching transports still need user testing.
- Sistema checked in isolated light/dark snapshots at 980 x 700, plus the missing-shortcut state at 1180 x 780. Live UI automation was blocked by the terminal's Accessibility permission. The installed executable matches the release build, and the user profile JSON remained byte-identical after snapshots and relaunch.
- Earlier UI checks covered native light/dark appearance, background process survival and the active event tap. On September 9, USB receiver read and same-value write/readback passed: 1600 DPI on both axes, 500 Hz, battery 100%, normal mode 0, no warnings. Physical remapping and driver-mode restore still require user testing.
- `--diagnose` run from a terminal fails with `0xe00002e2` (kIOReturnNotPermitted): the launching process needs Input Monitoring. Use `open -n OpenNaga.app --args --diagnose-file <path>` instead.
- Worktrees `../naga-worktree-{input,ui,hardware}` on branches `work/*` hold the sub-agent originals; they are fully merged and can be removed with `git worktree remove`.

## Recent changes

- 2.2.0 rename: user-visible name OpenNaga (window, sidebar, popover, menus, tooltips, export file `OpenNaga-profiles.json`, Info.plist `CFBundleName`/`CFBundleDisplayName`/`CFBundleExecutable`), build and DMG scripts produce `OpenNaga.app` / `OpenNaga-v<version>.dmg`. README rewritten with a Razer non-affiliation note and upgrade steps; CONTRIBUTING and SECURITY point to `Zer0codestuff/OpenNaga`. Removed the upstream author's `SETUP-INSTRUCTIONS.md`, `dmg-assets/setup-app-icon-and-dmg-simple.sh` and `dmg-background.png` (Developer ID flow never used here). Em dashes removed from BatteryMonitor logs.
- English UI (branch `english-ui`, based on v2.1.1, worktree `../NagaController-english`): every user-facing string, menu, error and the Info.plist usage text translated; the default profile `Navigazione` is now `Navigation`. Persisted identifiers are unchanged, so existing profiles load as before. Display-only enum raw values (`EditorKind`, `WorkspaceSection`, `SystemActionGroup`, `KeyboardKeyGroup`) changed. README shows `Documentation/screenshot-{light,dark}.png`, rendered with `--snapshot` and a synthetic `Everyday` profile under `.build/showcase-home` (`CFFIXED_USER_HOME`). The uncommitted onboard-profile work in the main checkout still has Italian strings and needs translating when it is merged.

- Fixed Bluetooth enumeration and callback filtering for the observed `Naga V2 HS` identity `068e:00b5`. Previously both gates required vendor `1532`, so macOS saw the mouse but NagaController excluded it. `IOHIDManagerSetDeviceMatchingMultiple` now includes the exact BLE pair without opening unrelated products under vendor `068e`. The live BLE descriptor includes a standard keyboard collection, report ID 6. Existing input decoding, profile storage and USB hardware controls are unchanged.
- Added 15 identity/enumeration regression checks, covering BLE with the short or missing name, unrelated devices, existing Razer name matching and USB receiver detection. The pre-fix installed app and scoped runtime log are in `.build/bluetooth-install.uqz61mjj/`.

- Sistema replaces the Audio editor with 25 functions in six categories: audio, playback, brightness, screenshots, windows/spaces, and tools. Tools include Spotlight, Finder, System Settings, Notification Center and Do Not Disturb. Selection saves immediately; Prova runs the selected function. Shell commands remain separate.
- System actions run once per physical press. Media events use tagged down/up pairs; keyboard actions honor configured macOS shortcuts, and app switching explicitly releases Command. Disabled/unassigned shortcuts show setup guidance. Spotlight opens directly because its keyboard shortcut may be disabled.
- Legacy `audio` JSON remains readable and is not migrated on load. Its earlier volume and mute implementation was tested live before this change. `Tests/SystemActionTests.swift` adds 305 checks for the catalog, event encoding, press/release behavior, shortcut overrides, legacy preservation and mixed-profile import/export.
- Sistema snapshots use `CFFIXED_USER_HOME` with synthetic profiles under `.build/system-ui-home`, without starting hardware/input services or changing real profiles. The pre-update installed bundle is preserved at `.build/system-install.TDzXAb/NagaController.app`.

- Fixed side-photo hover always highlighting button 12. One continuous pointer tracker resolves the cursor against the actual button polygons, accounting for image centering and scaling. Live checks covered buttons 3 and 5 and the empty mouse body; 26 regression checks cover all 12 targets at two sizes.

- `Tasti` replaces the text editor with `KeyboardKeyCatalog` and `KeyboardKeySelector`, physical key codes, current-layout labels, modifiers and local shortcut recording. Existing text actions remain unchanged until replaced by the user. Multi-step sequences remain editable.
- `WorkspaceModel`, `MouseWorkspace` and adaptive `UIStyle` provide a native sidebar, a mouse photo with 12 polygon targets, top controls, and a separate assignment inspector. PNG assets with real alpha and provenance are in `Resources/Mouse/`.
- Closing the window retains the menu-bar service. A scoped ProcessInfo activity prevents App Nap while remapping is active and still permits system sleep. Permission and active-service states are shown separately.
- A previous-session recovery journal was cleared through the explicit restore command on September 9. The receiver was already in normal mode 0, so a driver-to-normal transition remains unverified.
- Hardware state refreshes after HID connection name or transport changes. Refreshes are debounced and wait for active hardware operations; button events do not trigger USB reads. Physical unplug/replug validation is pending.
- Verification details and light/dark screenshots are in `Documentation/verification-2026-09-08.md`. Profiles were restored after temporary UI tests.

- Mouse actions `button4`/`button5`: when the frontmost app is a browser (`MouseAction.browserBundlePrefixes`) they are converted to `browserBack`/`browserForward`, because Safari and Chrome on macOS ignore mouse buttons 4/5. Real clicks are still sent elsewhere. `ButtonMapper.frontmostBundleIdentifier` is injectable for tests.
- `KeyboardLayoutShortcut.browserStroke`: brackets are used only when reachable without Option; otherwise ⌘← / ⌘→ (the user's layout is "Italian - Pro", where `[` needs Option).
- Version bumped to 2.0.0 (`CFBundleVersion` 3). GitHub release v2.0.0 published with `NagaController-v2.0.0.dmg` built by `Scripts/make_dmg.sh`. The stale tracked `NagaController-v0.1.0.dmg` was removed from git; root DMGs are now ignored.

- Deleted emptied legacy UI files (`ActionEditorViewController`, `GlassyBatteryView`, `MouseMappingView`).
- `PermissionManager`: added `ensureInputMonitoringPermission()` and `requestMissingPermissions()`; the app now requests both permissions at launch and before opening the corresponding System Settings pane, so it appears in the Privacy lists.
- `RazerHardwareSession.readMode()`: retries the mode query with transaction 0x1f when the OpenRazer 0xff variant returns status 4 (timeout), which is what this receiver did during on-device probes.
- `Scripts/make_dev_certificate.sh` added; `build_app.sh` auto-selects the "NagaController Dev" identity. Ad-hoc builds lost Accessibility and Input Monitoring after every rebuild (designated requirement was the cdhash).
- On-device verification with `--diagnose --verify-hardware`: DPI 1600/1600 and polling 500 Hz read, written back and re-read successfully; battery 100%.
- README rewritten for 2.0.0.

## Installed copy

The user runs `/Applications/NagaController.app` (local onboard-enabled build, not 2.2.0, bundle id `com.zer0codestuff.NagaController`). The old upstream 0.1.0 (`com.example.NagaController`) was removed from `/Applications` together with its Accessibility and Input Monitoring grants and its UserDefaults. To install 2.2.0 or later, quit and delete `/Applications/NagaController.app`, then copy `./OpenNaga.app` to `/Applications`.

## Preferences and constraints

- Everything in English: user-facing strings, code, comments, docs, commit messages. The UI was translated from Italian on 2026-09-29 (branch `english-ui`); do not reintroduce Italian strings.
- Native macOS light/dark appearance with restrained Razer green accents. Preserve transparent PNG alpha and the interactive mouse photo.
- No em dashes anywhere.
- MIT only: protocol knowledge from OpenRazer docs / PR 2850, no GPL code copied; OpenMouse is unlicensed, do not copy.
- Never change hardware settings on init or refresh; driver mode only through the explicit toggle, always journaled and restored.
- Button profiles are stored on the Mac and require the app to run. Do not describe them as written to the mouse's onboard memory.
- Do not claim signing or notarization that does not exist.

## Known issues / next steps

- Pending on-device verification: side button remap and release timing, browser back/forward, button 4/5, wheel tilt, driver mode toggle and restore, USB unplug/replug. Bluetooth connection detection and event-tap startup are verified; physical Bluetooth remapping and Bluetooth/dongle transitions are not.
- New system actions still need live testing with the mouse. Do Not Disturb has no assigned shortcut on this Mac; the inspector links to keyboard settings for setup. Brightness depends on display support for macOS brightness keys, and media keys depend on the playback app.
- `onChange(of:perform:)` deprecation warnings remain because the deployment target is macOS 13.

## Do not

- Do not commit or push without an explicit request.
- Do not rename the bundle id, the Application Support folder or the dev signing identity; that would drop permissions and profiles.
- Do not reintroduce Italian user-facing strings or the NagaController name in the UI.
- Do not open the receiver with IOUSBHost exclusive access (conflicts with the system HID driver).
- Do not reintroduce the legacy neon card UI or global keyboard interception outside explicit shortcut recording.
- Do not add permanent stub models for UI compilation.
- Do not change macOS keyboard shortcuts automatically. Lock, sleep, shutdown and restart are outside the agreed Sistema catalog.
