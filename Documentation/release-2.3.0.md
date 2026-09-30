# OpenNaga 2.3.0

Save a button profile directly to the Naga V2 HyperSpeed. It keeps working after you quit OpenNaga, over Bluetooth, and on computers without the app.

- **Save to Mouse**: writes the selected profile to the mouse's onboard memory over the USB receiver (mouse in 2.4 GHz mode). Only changed buttons are written, and every button is read back to confirm.
- **Restore Previous Assignments**: puts back the assignments the mouse had before the first save and turns software remapping back on.
- Stored in the mouse: single keys with modifiers, clicks, mouse buttons 4/5, scrolling and wheel tilt, DPI up/down, volume and media keys, and system functions that map to a keyboard shortcut. Sequences, macros, scripts and app launching still need the app; the window names any button that cannot be saved.
- New DPI up and DPI down actions for the top buttons.
- Command line: `--inspect-onboard` (read only), `--save-onboard-profile`, `--restore-onboard-profile`.

OpenNaga writes to the mouse only when you click Save or Restore, never at startup or when you edit a button.

Upgrading: quit OpenNaga (or NagaController), replace the app in Applications with the new `OpenNaga.app`. Profiles are kept.

Apple Silicon build, ad-hoc signed, not notarized. On first launch right-click the app and choose Open, or run `xattr -dr com.apple.quarantine /Applications/OpenNaga.app`.
