# OpenNaga

Free, open-source macOS app for the Razer Naga V2 HyperSpeed, a replacement for Razer Synapse, which does not support this mouse on the Mac. It remaps the 12 side buttons and the extra mouse controls, can save a button profile into the mouse itself so it works without the app, and reads or changes DPI and polling rate over the USB receiver.

OpenNaga is an independent project, not affiliated with or endorsed by Razer. It started from [DParent10/NagaController](https://github.com/DParent10/NagaController) and has since been rewritten with a new interface, hardware control and input engine.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="Documentation/demo-dark.gif">
  <img alt="OpenNaga stepping through the 12 side buttons and the top controls with their assignments" src="Documentation/demo-light.gif">
</picture>

## Features

- Adaptive macOS settings window with a restrained green accent and four sections: Buttons, App Profiles, Sensitivity, Status
- Interactive mouse photographs with side-button highlights, a top view and a matching assignment grid for all 19 logical controls
- Actions per button: keyboard keys selected by category or recorded, with optional modifiers, multi-step key sequence, mouse action (browser back/forward, real mouse buttons 4/5, middle/left/right click, scroll), launch application, shell command, macro, profile switch, disabled, or original passthrough
- Hypershift layer: assign Hypershift key to a control, then give any other control a second action that applies only while you hold it. Controls without a Hypershift action keep their standard one. Save to Mouse stores both layers, so Hypershift also works with the app closed. See [Hypershift layer](#hypershift-layer)
- App profiles: bind a profile to any app, and OpenNaga loads it whenever that app comes to the front. Done in software, so it works over Bluetooth. See [App profiles](#app-profiles)
- Held keyboard modifiers combine with the mouse's keys: Command plus a side button that sends C produces Command-C. This applies to remapped buttons, unmapped buttons and profiles saved to mouse memory, including keypad keys, key sequences and shortcuts. macOS does not apply modifiers held on one keyboard to keys from another device, so OpenNaga re-sends such mouse keys with the held modifiers while it is running. With a profile saved to or imported from the mouse this is always on; otherwise it follows the Remapping switch. Without the app, the mouse's keys arrive without keyboard modifiers.
- System editor with 25 actions for audio, playback, brightness, screenshots, windows, spaces and macOS tools, with a preview button
- Existing text snippets remain saved and can be replaced through the Keys editor
- Save a profile to the mouse's onboard memory: it keeps working after you quit OpenNaga, on another computer and over Bluetooth
- Profiles with auto-save on the Mac, import/export as JSON, rename/duplicate/delete
- Import the mouse's current assignments into an editable local profile for explicit Save to Mouse
- DPI (100 to 30000 per axis) and polling rate (125/500/1000 Hz) read and written through the Razer USB protocol, with read-back verification
- Separate vertical-wheel and horizontal/tilt sensitivity sliders, from 0.1× to 5×, applied in software while remapping is enabled
- Optional "driver mode" for the top DPI buttons, with journaled restore of the original mode at quit
- Menu bar popover with profile selection and remapping toggle; closing the settings window keeps the service running
- Settings appear in the Dock and Command-Tab while the window is open
- Remapping activity prevents App Nap while allowing normal system sleep

Release 2.6.0 adds the Hypershift layer, stored in mouse memory as well, and applies held keyboard modifiers to the keys of a profile saved in the mouse. See the [release notes](Documentation/release-2.6.0.md) for changes and verified behavior.

## Requirements

- macOS 13.0 or later. The downloadable DMG contains an Apple Silicon build; Intel Macs must build from source.
- Razer Naga V2 HyperSpeed connected through its HyperSpeed USB receiver (`1532:00b4`) for DPI, polling rate and driver mode. Saving a profile to the mouse also needs the receiver. The app also recognizes the Bluetooth identity `068e:00b5`, shown by macOS as "Naga V2 HS".
- Xcode Command Line Tools with Swift 5.9+ to build from source

## Install and first run

1. Download `OpenNaga-v2.6.0.dmg` from the [latest release](https://github.com/Zer0codestuff/OpenNaga/releases/latest), or build the app bundle (see below).
2. Open the DMG and drag `OpenNaga.app` to Applications. Permissions are tied to the app location, so do not move it afterwards.
3. The release is not notarized. On first launch right-click the app and choose Open, or run `xattr -dr com.apple.quarantine /Applications/OpenNaga.app`.
4. Launch it. macOS prompts for two permissions; both are required:
   - Accessibility (System Settings > Privacy & Security > Accessibility)
   - Input Monitoring (System Settings > Privacy & Security > Input Monitoring)
5. Open the settings window from the menu bar icon, turn on "Remapping", and assign actions.

Upgrading from 2.3.0 or earlier, including the builds named NagaController 2.x: quit the old app and replace it in Applications with `OpenNaga.app`. On first launch OpenNaga copies your profiles and mouse backup from `~/Library/Application Support/NagaController` into `~/Library/Application Support/OpenNaga` and leaves the old folder as it is. macOS may ask for the two permissions again.

The Status section shows the current permission state, the detected device, and the last input seen. If a permission was granted after launch, macOS may require restarting the app.

Button assignments are saved on the Mac and applied by OpenNaga while it runs in the menu bar. Closing the settings window is fine; quitting the app stops software remapping. To use a profile without the app, save it to the mouse.

## Hypershift layer

Hypershift doubles the number of actions without another profile, as in Razer Synapse. Each profile has a Standard layer and a Hypershift layer.

1. In Buttons, keep the Standard layer selected, choose the control you want to hold (for example side button 12 or the middle click) and set its action to Hypershift key.
2. Switch the layer selector above the button list to Hypershift. The list shows each control's Hypershift action, or its standard action when it has none.
3. Select a control and assign its Hypershift action like any other action. Choose Same as standard to remove it.

While the Hypershift key is held, controls with a Hypershift action perform it and every other control keeps its standard action. The key itself sends nothing. An output that starts while the key is held, such as a held keyboard key, still ends with the same key when you release the button, even if you let go of the Hypershift key first. You can assign Hypershift key to more than one control; the layer stays active while any of them is held.

Use a control that reports when it is held and that you can hold while pressing the others, for example a side button under your thumb combined with wheel clicks and tilts. Wheel tilt without driver mode arrives as scroll events with no hold, so it cannot act as a Hypershift key in the app. A left or right click assigned to Hypershift key no longer clicks.

In the app, OpenNaga applies the layer while remapping is on. **Save to Mouse** writes both layers to the mouse, so the Hypershift key keeps working with the app closed, after a power cycle and on other computers. In mouse memory the Hypershift layer can also hold DPI up and down. The left click cannot be the Hypershift key there, so the mouse always keeps a working click. Profiles written before this feature load unchanged; the layer is stored as an optional `hypershift` map next to `buttons` in `profiles.json` and in exported files.

## App profiles

App Profiles gives an app its own profile, so the same side button can do different things in Safari, Notes or a game.

1. Open App Profiles and turn on Switch profiles by app.
2. Choose Add Application, pick an installed or running app (or Choose Other for one elsewhere), and pick the profile it should use.
3. Come back to any other app and the profile you selected in the menu bar or in Buttons applies again.

OpenNaga watches which app is in front and swaps the live assignments; nothing is written to the mouse, so this works over Bluetooth. Only apps that appear in the Dock count, so Spotlight and menu bar helpers do not flip the profile. A button held while you switch apps still ends with the key it started. Choosing a profile by hand applies at once and stays until the next app change. While OpenNaga's own window is in front, the profile you selected is the live one, so what you edit is what you test. Switching pauses while a profile saved in the mouse or imported from it is active, because those never remap in software. Deleting or renaming a profile updates its app bindings. Bindings are stored in `profiles.json` under `settings.appProfiles`, a map from bundle identifier to profile name; older files load unchanged.

## Save a profile to the mouse

The Naga V2 HyperSpeed stores one button profile in its own memory. OpenNaga can write the selected profile there:

1. Connect the USB receiver and set the mouse to 2.4 GHz mode.
2. Select the profile in the settings window and click **Save to Mouse…** next to the profile menu.
3. Click **Save to Mouse**. If a button uses an action the mouse cannot store, the window names it and saving stays disabled.

After saving, the mouse sends the assigned keys by itself, so the profile keeps working after you quit OpenNaga, over Bluetooth, and on computers without the app. Software remapping pauses while a saved profile is active, so the new buttons are not remapped twice. While OpenNaga runs, it still adds modifiers held on your keyboard to the keys the mouse sends, so Command plus a button saved as C gives Command-C. Without the app, macOS delivers the mouse's keys without those modifiers. Editing assignments or switching profiles does not change the mouse until you save again.

What the mouse can store:

- A single key, with or without Command, Control, Shift and Option
- Left, right and middle click, mouse buttons 4 and 5, scrolling and wheel tilt
- DPI up and DPI down on the top buttons
- A Hypershift key and the Hypershift layer, written with the same actions as above
- Volume and media controls
- System functions that map to a keyboard shortcut, such as screenshots
- One-character text snippets that match a key in the current layout

Key sequences, macros, shell commands, application launching, profile switching and Fn shortcuts need the app. If the profile contains one of them, OpenNaga lists the affected buttons and does not save. Key repeat and long presses follow the mouse firmware.

Each save keeps a backup of the mouse's previous assignments, including the Hypershift layer when a save changes it, and reads every button back to confirm the write. **Restore Previous Assignments** in the same window puts the backup back and re-enables software remapping. OpenNaga never writes to the mouse at startup, on refresh or at quit, only when you click Save or Restore.

**Import from Mouse** in the Mouse Memory window reads the active assignments into a new **From Mouse** profile. Import performs no hardware writes. Supported keys, modifiers, mouse actions, DPI, media controls and a Hypershift key with its layer remain editable; unsupported assignments are listed. Imported profiles use **Save to Mouse** to apply edits. Their software remapping stays paused because custom firmware assignments can produce identical events from different physical controls. This restriction survives export, duplication and restart.

The scroll sensitivity sliders are in **Sensitivity**. Vertical wheel and horizontal scrolling, including wheel tilt and assigned scroll actions, have separate multipliers. **Reset** returns both to 1×. These settings stay on the Mac and require OpenNaga running with remapping enabled; they are not written to mouse memory. Trackpad and momentum scrolling are left unchanged.

## Assign system controls

Select a mouse button, choose **System** in the **Action** menu, then pick a category and function. The selection saves immediately. **Test** runs the selected function without pressing the mouse button.

| Category | Functions |
| --- | --- |
| Audio | Volume up, volume down, mute toggle |
| Playback | Play/pause, previous track, next track |
| Brightness | Display brightness up/down |
| Screenshots | Full screen or selection to a file or clipboard, screenshot and recording tools |
| Windows and Spaces | Mission Control, app windows, desktop, previous/next space, hide app, switch to the last app |
| Tools | Spotlight, Finder, System Settings, Notification Center, Do Not Disturb |

Each physical press runs the action once, with no hold repeat. Existing audio mappings appear under System without being rewritten. Shell commands remain a separate action type.

Screenshots, Mission Control and space navigation use the keyboard shortcuts configured in macOS. Disabled or unassigned shortcuts show setup instructions instead of sending a different key combination. Do Not Disturb requires an enabled shortcut in System Settings > Keyboard > Keyboard Shortcuts > Mission Control. OpenNaga does not change these preferences. Spotlight opens directly even when its keyboard shortcut is disabled.

Brightness uses the Mac's brightness keys, so an external monitor must support brightness control through macOS. Media controls target the active playback app. Focus settings may sync Do Not Disturb to other Apple devices.

## How it compares

Other ways to configure a Naga V2 HyperSpeed on a Mac:

| | OpenNaga | NagaController | SteerMouse |
| --- | --- | --- | --- |
| Price | Free | Free | US$19.99, 30-day trial |
| Open source | Yes, MIT | Yes, MIT | No |
| Naga V2 HyperSpeed | Yes | Yes | Since 5.7.2, USB receiver only |
| 12 side buttons | Remapped | Remapped | Remapped |
| Profile saved in the mouse | Yes | Not listed | Not listed |
| DPI and polling rate | Yes, over the USB receiver | Not listed | Cursor speed in software |
| Hypershift layer | Yes, also in mouse memory | Yes | Not listed |

Razer Synapse for Mac does not support this mouse; owners are still asking for it on [Razer Insider](https://insider.razer.com/razer-synapse-for-mac-preview-61/naga-v2-hyperspeed-support-in-synapse-preview-78820). The usual workaround is to configure the mouse in Synapse on a Windows PC and carry the onboard profile over.

[NagaController](https://github.com/DParent10/NagaController) is the project OpenNaga started from and is still maintained. [SteerMouse](https://plentycom.jp/en/steermouse/) works with mice from many brands and has per-application profiles, which OpenNaga does not have.

Details for the other apps come from their own pages and the [SteerMouse release notes](https://plentycom.jp/en/steermouse/download.php), checked in October 2026.

## Build from source

```bash
bash Scripts/build_app.sh        # release bundle, signed with the dev identity if available
open OpenNaga.app
```

Without a signing identity the bundle is ad-hoc signed and macOS forgets its permissions on every rebuild. For development, create a local self-signed identity once:

```bash
bash Scripts/make_dev_certificate.sh   # creates "NagaController Dev" in the login keychain
```

`build_app.sh` picks it up automatically. Set `SIGNING_IDENTITY="Developer ID Application: ..."` to use a real identity instead. Builds are not notarized.

To produce the distributable disk image (ad-hoc signed, with an Applications shortcut):

```bash
bash Scripts/make_dmg.sh              # writes OpenNaga-v<version>.dmg
```

## Tests and diagnostics

```bash
bash Scripts/test.sh             # dependency-free checks, no XCTest needed
```

Read-only hardware diagnostics (requires Input Monitoring for the launching process):

```bash
open OpenNaga.app --args --diagnose-file /tmp/naga.json
cat /tmp/naga.json
```

Add `--verify-hardware` to also write back the current DPI and polling values and confirm the read-back. Values are not changed.

Capture the UI without starting the input or hardware services:

```bash
open -n OpenNaga.app --args --snapshot /tmp/ui.png --snapshot-appearance dark --snapshot-size 980x700 --snapshot-button 8
```

Appearance, size and selected button are optional snapshot controls.

Modifier trace, for reports about held modifiers not combining with mouse buttons:

```bash
defaults write com.zer0codestuff.NagaController ModifierTrace -bool true   # then quit and reopen OpenNaga
cat ~/Library/Application\ Support/OpenNaga/modifier-trace.log
defaults delete com.zer0codestuff.NagaController ModifierTrace            # turn it off again
```

The trace records modifier key changes, the flags of keys sent by the mouse and whether OpenNaga re-sent them. It does not record ordinary keyboard typing. The file is capped at about 1 MB.

Onboard memory from the command line (quit the app first; each command writes a JSON report and exits):

```bash
open -n OpenNaga.app --args --inspect-onboard --diagnose-file /tmp/naga-onboard.json        # read only
open -n OpenNaga.app --args --save-onboard-profile --diagnose-file /tmp/naga-save.json      # saves the selected profile
open -n OpenNaga.app --args --restore-onboard-profile --diagnose-file /tmp/naga-restore.json
```
See [the latest verification record](Documentation/verification-2026-10-12.md) for the issue audit, tested flows and remaining device checks. The [original verification record](Documentation/verification-2026-09-08.md) covers the earlier release.

## How input handling works

- `HIDListener` observes vendor `1532` devices whose product ID is `00b4` or whose name contains "naga", plus the exact Bluetooth identity `068e:00b5`, on a background run loop. It does not enumerate other products under the Bluetooth vendor ID.
- Each physical press/release is recorded with its timestamp. `EventTapManager` consumes a matching system event only if it arrives within 25 ms of a recorded HID edge, so regular keyboards are never blocked.
- Synthetic events are tagged through `eventSourceUserData` and ignored by the tap.
- macOS keeps modifier state per keyboard device, so keys from the mouse's keyboard interface never carry modifiers held on another keyboard. Every key usage from that interface is recorded, and when a matching key event lacks a held modifier, the tap drops it and re-sends the key through the synthetic source with the modifiers merged. Its auto-repeats and key-up follow the same path. While a profile saved to or imported from the mouse is selected, the tap stays blocking for this alone and never remaps keys.
- Held buttons are released on stop, disconnect, profile change, or when the tap is disabled by the system.
- Buttons 13 to 16 (DPI, wheel tilt) rely on documented driver-mode reports and are only reported when a matching report is observed.

## Project structure

The Swift package, target and source folder keep the internal name `NagaController`, as does the bundle identifier, so existing permissions carry over.

Profiles, the mouse backup and the driver mode journal live in `~/Library/Application Support/OpenNaga`. Up to 2.3.0 OpenNaga shared the `NagaController` folder with [NagaController](https://github.com/DParent10/NagaController). Since 2.4.0 it reads that folder once to import older data and never writes to it, so the two apps no longer overwrite each other's profiles.

- `Sources/NagaController/AppProfiles/` frontmost-app observer, profile resolution and installed-app catalog
- `Sources/NagaController/ButtonMapping/` action model, event synthesis, layout-aware browser shortcuts
- `Sources/NagaController/EventTap/` CGEvent tap and correlation with HID input
- `Sources/NagaController/HID/` IOHID listener and pure report decoding
- `Sources/NagaController/Hardware/` Razer USB protocol codec, IOHID feature-report transport, device controller, onboard profile encoding and backup
- `Sources/NagaController/UI/` SwiftUI settings window and menu bar popover
- `Sources/NagaController/Utils/` profiles storage, permissions, battery monitor
- `Tests/` dependency-free test sources run by `Scripts/test.sh`
- `Resources/` Info.plist, bundled default profiles and transparent mouse images
- `Resources/Mouse/README.md` image sources, generation prompts and transparency verification

## Credits and license

OpenNaga started as a fork of [NagaController](https://github.com/DParent10/NagaController) by DParent10, also MIT licensed. Protocol facts come from the published OpenRazer driver sources, pull request 2850 and issues 2845 and 2031, and from the OpenSnek protocol notes. The Swift code was written from those wire formats and checked on a real receiver; no OpenRazer or other GPL code is included. Each file in `Sources/NagaController/Hardware/` lists its references. Code is licensed under MIT, see [LICENSE](LICENSE). Mouse image provenance is documented in [Resources/Mouse/README.md](Resources/Mouse/README.md).
