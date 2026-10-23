# OpenNaga 2.7.0

OpenNaga 2.7.0 adds automatic per-app profile switching. Thanks to @LoessCode for contributing [PR #8](https://github.com/Zer0codestuff/OpenNaga/pull/8).

- Bind profiles to applications in the new App Profiles section. OpenNaga applies the matching profile when an app comes to the front and returns to your selected profile for other apps.
- Switching happens in software, including over Bluetooth, and never writes to mouse memory. OpenNaga must be running with remapping enabled.
- Held buttons release the output they started, even after an automatic profile change. Manual profile selection applies until the next app change.
- Pick installed or running apps, including game apps in the default Steam library, or use Choose Other. Apps need a bundle identifier to be matched.
- Switching pauses for profiles saved to or imported from mouse memory. Renaming or deleting a profile updates its app bindings.

Validation: **1359 automated checks passed**, the release build compiled, and the App Profiles screen was inspected in light and dark appearance at 980x700. The contributor reported a physical Bluetooth test with app focus changes. The maintainer review did not repeat that hardware test; packaging adds no new physical-input, power-cycle or Bluetooth coverage.

Upgrade: quit OpenNaga and replace the app in Applications. Profiles and mouse backups remain in `~/Library/Application Support/OpenNaga`. Public builds are ad-hoc signed and not notarized; macOS may request Accessibility and Input Monitoring again after replacement. The downloadable build is Apple Silicon for macOS 13 or later. `SHA256SUMS.txt` contains the DMG checksum.
