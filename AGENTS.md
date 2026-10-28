# AGENTS.md

## Purpose

OpenNaga: macOS menu bar app (Swift, AppKit + SwiftUI, no Xcode project) that remaps the buttons of the Razer Naga V2 HyperSpeed and controls DPI / polling rate over the USB receiver `1532:00b4`. Standalone project that started as a fork of DParent10/NagaController (credited in README and LICENSE); remote `origin` is `Zer0codestuff/OpenNaga`. Not affiliated with Razer.

Naming: the shipped app is `OpenNaga.app` (executable `OpenNaga`, display name OpenNaga). The SwiftPM package/product/target, `Sources/NagaController`, the bundle id `com.zer0codestuff.NagaController` and the "NagaController Dev" signing identity intentionally keep the old name so TCC grants carry over. Data lives in `~/Library/Application Support/OpenNaga` since 2.4.0; the older `NagaController` folder belongs to the upstream app and is only read once for import.

## Architecture

- `Sources/NagaController/main.swift`: CLI switches (`--diagnose`, `--diagnose-file <path>`, `--verify-hardware`, `--inspect-onboard`, `--save-onboard-profile`, `--restore-onboard-profile`, `--snapshot <png>`), otherwise starts `AppDelegate`.
- `AppDelegate`: starts `HIDListener`, `RazerDeviceController.refresh()`, the event tap (only when Accessibility and Input Monitoring are granted), a 2 s permission poll, menu bar item and popover; defers quit until `restoreOriginalMode` completes.
- `ButtonMapping/`: `ActionType` (system, legacy audio, mouse, disabled, keySequence, application, systemCommand, textSnippet, macro, profileSwitch), `ButtonMapper` (press/hold/release semantics, synthetic marker `eventSourceUserData`), `KeyboardLayoutShortcut` (browser back/forward per layout). `SystemAction` groups native system controls; `MacSystemShortcut` reads configured macOS shortcuts without changing preferences.
- `EventTap/EventTapManager`: CGEvent tap; consumes an event only if `HIDListener.consume` matches a HID edge within 25 ms. Logical indices: 1..12 side grid, 13 DPI up, 14 DPI down, 15 wheel left, 16 wheel right, 17 middle, 18 left, 19 right.
- `HID/HIDListener` + `HID/InputModel`: IOHIDManager on a dedicated thread, pure decoders (`NagaInput`, `InputEdgeMatcher`, `DriverButtonState`). Enumeration includes vendor `1532` and exact BLE identity `068e:00b5`; callbacks additionally validate Naga identity.
- `Hardware/`: `RazerProtocol` (90-byte report codec, CRC, transaction IDs), `MacRazerUSBTransport` (IOHID feature reports on the mouse collection with 90-byte feature size), `RazerDeviceController` (`@MainActor` facade, serial worker queue, driver-mode recovery journal `driver-mode-recovery.json` in the data folder), a nonblocking `hardware.lock` so GUI and CLI sessions never interleave. Onboard memory: `RazerOnboardBindings` (21-control inventory and 10-byte descriptors, class 2 commands 0x84/0x8c), `OnboardProfilePlan` (actions to USB HID function blocks; unsupported actions block the whole save), `OnboardProfileStore` (writes only changed bindings to stored bank 1 via 0x0c, verifies bank 1 and active bank 0, keeps the original assignments in `onboard-profile.json`, rolls back on failure; the backup follows the receiver model `1532:00b4`, not the USB port).
- `UI/`: `MappingViewController` (NSHostingController with `NagaWorkspace`), `ActionInspector`, `SettingsPanes` (Sensitivity, Status, ProfileManager), `OnboardProfilePane` (Mouse Memory sheet), `MainViewController` (popover), `MappingWindowController`.
- `Utils/ConfigManager`: profiles JSON, auto-save, `didChangeNotification`, `lastError`, scroll wheel sensitivity (`Utils/ScrollSensitivity`).
- `Utils/ScrollSensitivity`: scroll wheel multipliers (vertical and tilt, 0.1 to 5) with a residual `ScrollScaler`. Applied by the event tap to correlated Naga wheel events and by `ButtonMapper` to synthetic scroll; never written to the hardware.
- `Utils/DataFolder`: the only place that names the data folder. `importLegacyFilesIfNeeded()` runs first in `main.swift`: it copies `profiles.json`, `onboard-profile.json`, `driver-mode-recovery.json` and `onboard-profile.*.bak.json` from the `NagaController` folder when they are missing in `OpenNaga`, then writes `.legacy-import-done`. It never overwrites, writes to or deletes the older folder; a failed copy leaves the marker out so the next launch retries.

## Build, run, test

```bash
swift build                      # debug
bash Scripts/make_dev_certificate.sh   # once: self-signed "NagaController Dev" identity, keeps TCC grants across rebuilds
bash Scripts/build_app.sh        # release bundle ./OpenNaga.app, signed with the dev identity if present, else ad-hoc
bash Scripts/test.sh             # 856 dependency-free checks (XCTest is not available with CLI tools only)
bash Scripts/make_dmg.sh         # ad-hoc signed release DMG (OpenNaga-v<version>.dmg, git-ignored)
gh release create vX.Y.Z OpenNaga-vX.Y.Z.dmg --title "OpenNaga X.Y.Z" --notes-file <file>   # publish
open OpenNaga.app --args --diagnose-file /tmp/naga.json   # read-only hardware probe
open -n OpenNaga.app --args --inspect-onboard --diagnose-file /tmp/o.json   # read-only onboard dump (quit the GUI first)
./OpenNaga.app/Contents/MacOS/OpenNaga --snapshot /tmp/ui.png  # UI render without hardware
```

`Package.swift` also declares a `TapTester` executable and an XCTest target (`Tests/NagaControllerTests`) that cannot run without Xcode.

## Current status (2026-10-04)

- Single branch: everything lives on `main`. Feature branches and extra worktrees were removed after 2.3.0; do not create long-lived branches.
- Version 2.3.0, build 7: onboard memory (Save to Mouse / Restore Previous Assignments) ported from the Italian 2.1.1-based work onto 2.2.0, translated, renamed to OpenNaga. Published as GitHub release v2.3.0 with `OpenNaga-v2.3.0.dmg` (Apple Silicon, ad-hoc signed, not notarized).
- Version 2.4.0, build 8: own data folder with one-time import, released as v2.4.0. Verified with the release bundle in an isolated home (`CFFIXED_USER_HOME`) holding copies of the user's real `profiles.json` and mouse backup: files imported byte for byte, the Test profile and "Mouse memory: Test" shown, no re-import on the second launch, older folder unchanged. Not checked on hardware, since only file locations changed.
- 856 dependency-free checks pass; release build has no warnings or errors. README screenshots re-rendered in light and dark with the synthetic `Everyday` profile (`.build/showcase-home`, `CFFIXED_USER_HOME`).
- Hardware evidence: on 2026-09-11 the Test profile was written and read back (21 descriptors), Escape on side button 1 survived a power cycle. On 2026-09-30 the DPI buttons were rewritten to DPI up/down, all 21 stored and active descriptors verified, and the user tested the saved profile over Bluetooth with the app closed and reported it works (tentative, not per-button).
- The user's journal `onboard-profile.json` was rebound from USB location `18026496` to `18022400` by hand before the model-based check existed; the original is `onboard-profile.port-0x01131000.bak.json` in Application Support.
- `--diagnose` run from a terminal fails with `0xe00002e2` (kIOReturnNotPermitted): the launching process needs Input Monitoring. Use `open -n OpenNaga.app --args --diagnose-file <path>` instead.

## Recent changes

- 2026-10-10: scroll wheel sensitivity (issue 7). `Utils/ScrollSensitivity` holds the multipliers (vertical and tilt, 0.1 to 5, UserDefaults through `ConfigManager.scrollSensitivity`) and a residual `ScrollScaler`. `HIDListener` records vertical wheel ticks (Generic Desktop Wheel, page 1 usage 0x38) so `EventTapManager` can scale only the Naga's own scroll events; unmapped tilt passthrough is scaled through its controls 15/16, and synthetic scroll from assigned actions is scaled in `ButtonMapper`. Two sliders live in the Sensitivity pane. Trackpads and other mice are untouched.
- 2026-10-07: README hero is an animated demo, `Documentation/demo-dark.gif` and `demo-light.gif`. Built from `--snapshot` renders of controls 1 to 19 with the synthetic `Everyday` profile in `.build/showcase-home`, cropped to the mouse, list and inspector (the sidebar and footer show "Mouse disconnected" without hardware), crossfaded, then `ffmpeg` palette and `gifsicle -O3`. `screenshot-light.png` and `screenshot-dark.png` stay because external list entries link to them.
- 2.4.0 (2026-10-04): DParent10, the NagaController author, reported that both apps wrote `~/Library/Application Support/NagaController/` with diverged profile formats, so each could rewrite the other's `profiles.json`. OpenNaga moved to its own folder (see `Utils/DataFolder`). The three releases titled "NagaController 2.x" were retitled, issues and private vulnerability reporting were enabled, and the README credits now list every protocol source (OpenRazer driver sources, PR 2850, issues 2845 and 2031, OpenSnek notes). He also asked to take `Hardware/` upstream; the owner's answer is not recorded here.
- 2026-10-04: README gained a `How it compares` table (OpenNaga, NagaController, SteerMouse) and a note on Razer Synapse for Mac, built from each project's own pages. The "5.7.2, USB connection only" note that search engines attribute to Synapse is from the SteerMouse release notes; Synapse for Mac still does not list this mouse. Recheck those pages before editing the table. Comments linking the repo were posted on Reddit and on `1kc/razer-macos` issues 739 and 926.
- 2.3.0: onboard memory. `Save to Mouse…` button next to the profile picker and `Mouse Memory…` menu item open `OnboardProfilePane`. Saving is explicit only (never on edit, profile switch, startup, refresh or quit). While a saved profile is active, software interception and driver mode are disabled; restoring the backup re-enables them. DPI up/down mouse actions exist only as hardware functions (software mapper ignores them). Onboard buttons 4/5 stay standard mouse buttons (user choice, for games). Backup identity now compares vendor:product only, because moving the dongle to another hub/port blocked save and restore.
- 2.2.0: rename to OpenNaga and English UI. Persisted identifiers unchanged; the default profile `Navigazione` became `Navigation`.
- 2.1.x: Bluetooth identity `068e:00b5` detection, System actions catalog (25 functions), native UI with mouse photo targets, keyboard key picker, background and hotplug fixes.

## Installed copy

The user still runs `/Applications/NagaController.app`, a local onboard build from 2026-09-11 (bundle id `com.zer0codestuff.NagaController`). It predates the `dpiUp`/`dpiDown` actions, cannot decode the user's `profiles.json` and silently falls back to the bundled Default profile. Replace it with 2.4.0: quit it, delete it, copy `./OpenNaga.app` to `/Applications`. Permissions carry over (same bundle id and signing identity) and the first launch imports profiles and the mouse backup into the `OpenNaga` folder. As long as the old build is used it keeps writing the older folder, and those later changes are not imported.

## Preferences and constraints

- Everything in English: user-facing strings, code, comments, docs, commit messages. Do not reintroduce Italian strings.
- Native macOS light/dark appearance with restrained Razer green accents. Preserve transparent PNG alpha and the interactive mouse photo.
- No em dashes anywhere.
- MIT only: protocol knowledge from OpenRazer docs / PR 2850 and public protocol notes, no GPL code copied; OpenMouse is unlicensed, do not copy.
- Never change hardware settings on init or refresh; driver mode only through the explicit toggle, always journaled and restored. Onboard writes only on explicit Save or Restore, always with a backup.
- Local profiles are saved on the Mac; only an explicit Save to Mouse writes one profile to the mouse. Never describe edits or profile selection as synchronized to the mouse. Sequences, macros, scripts, app launch, profile switch and Fn shortcuts need the app.
- Do not claim signing or notarization that does not exist.
- The project is being promoted in Razer communities: keep README accurate, honest about what is verified, and free of filler.

## Known issues / next steps

- Not notarized: users see a Gatekeeper warning. A Developer ID and notarization would remove it.
- Pending physical checks: per-button onboard verification (wheel tilt in particular), software remapping over Bluetooth, driver mode toggle and restore, USB unplug/replug.
- System actions: Do Not Disturb needs a user-assigned shortcut; brightness depends on display support; media keys depend on the playback app.
- `onChange(of:perform:)` deprecation is tolerated because the deployment target is macOS 13.

## Do not

- Do not commit or push without an explicit request.
- Do not create feature branches or extra worktrees; work on `main`.
- Do not rename the bundle id or the dev signing identity; that would drop permissions. Do not move the data folder again without an import step.
- Do not write to, rename or delete `~/Library/Application Support/NagaController`; it belongs to the upstream app. Do not rerun the import once the marker exists.
- Do not reintroduce Italian user-facing strings or the NagaController name in the UI.
- Do not open the receiver with IOUSBHost exclusive access (conflicts with the system HID driver).
- Do not reintroduce the legacy neon card UI or global keyboard interception outside explicit shortcut recording.
- Do not add permanent stub models for UI compilation.
- Do not change macOS keyboard shortcuts automatically. Lock, sleep, shutdown and restart are outside the agreed System catalog.
- Do not write mouse flash on edits, startup, refresh, profile selection or quit. Do not delete `onboard-profile.json` to bypass recovery.
- Do not translate onboard mouse buttons 4/5 to browser shortcuts.
