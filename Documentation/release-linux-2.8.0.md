# OpenNaga for Linux 2.8.0 (preview)

The first Linux release of OpenNaga: a native port of the macOS app in Rust with GTK 4 and libadwaita, with the same window, the same features and the same `profiles.json` format.

**Preview: not yet tested with the physical mouse on Linux.** The port builds, passes its automated tests (profile format, Razer protocol, onboard memory, mapper, macros and input frames) and its window was checked with rendered snapshots. Remapping through evdev and uinput, DPI, polling rate, driver mode and Save to Mouse over hidraw, Bluetooth and sleep and wake still need real-world confirmation. The protocol code is a port of the macOS implementation, which was verified on the mouse. Please report results in the issues, with `opennaga --diagnose` output where hardware is involved.

## What is included

- The settings window with Buttons, App Profiles, Sensitivity and Status, in light and dark appearance
- Every action type of the macOS app, the Hypershift layer, macros and text, profile switching
- Per-app profiles on Hyprland, Sway, i3 and X11
- DPI, polling rate, scroll sensitivity and driver mode for the DPI buttons, with journaled restore
- Save to Mouse and Restore Previous Assignments, with a backup of the original assignments
- Profile backups and recovery, Undo and Redo, import with Keep Both, export
- A tray icon, start at sign-in, and a udev rule installed from the Status section

## Download

`opennaga-2.8.0-x86_64-linux.tar.gz` is built on Ubuntu 24.04 by GitHub Actions and needs glibc 2.39, GTK 4.14 and libadwaita 1.5 or newer. Extract it and run `./install.sh --user`. The archive is not signed; its SHA-256 checksum is attached next to it. Arch users can also build `linux/packaging/arch/PKGBUILD`. See [linux/README.md](../linux/README.md) for device access, desktop support and limits.

The macOS app is unchanged by this release; the current macOS download remains 2.7.0.
