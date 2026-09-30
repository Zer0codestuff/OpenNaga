# Onboard profile verification, 2026-09-11

## Device and protocol

Receiver `1532:00b4`, normal mode 0, DPI 1600/1600, polling 500 Hz, battery 98%.

Read-only bring-up returned 21 controls through `02:84`, size 22. Binding reads through `02:8c` return size 10. The initial size-22 binding request returned a size-10 response, so the production implementation requests 10 without relaxing the existing report codec.

Side-button IDs are `40..4b` in printed-number order. Enumeration instead groups the grid by column. Stored profile 1 and active profile 0 expose matching function bytes. A stored-profile write uses `02:0c`, size 10, transaction `1f`.

Wire-format references, implemented independently without copying external code:

- [OpenRazer issue 2845](https://github.com/openrazer/openrazer/issues/2845), initial read/write family on another device.
- [USB profile specification](https://github.com/gh123man/OpenSnek/blob/main/docs/protocol/USB_PROFILE_CRUD_SPEC.md), stored versus active bank and descriptor structure.
- [USB protocol notes](https://github.com/gh123man/OpenSnek/blob/main/docs/protocol/USB_PROTOCOL.md), keyboard/modifier and mouse function encoding.
- [OpenRazer issue 2031](https://github.com/openrazer/openrazer/issues/2031), multimedia function type.

## Hardware results

1. Read all original descriptors and backed them up before writing.
2. Wrote Escape to side button 1 in bank 1 and verified both banks. The user quit the app (then named NagaController), power-cycled the mouse in 2.4 GHz mode, and confirmed Escape still worked.
3. Read bank 1 again after that test and confirmed Escape remained. Restored the pre-probe descriptor before creating the complete-profile backup.
4. Compiled the user's Test profile without unsupported actions. Saved all 16 configured assignments and verified all 21 stored and active descriptors, including unchanged primary clicks and vertical scrolling.
5. Saved the same profile successfully through the installed app's native UI. An identical save avoids flash writes.
6. Verified that the software-remapping switch is disabled in hardware mode and that the footer reports the saved Test profile.
7. Quit the installed GUI and ran a fresh read-only diagnostic. All 21 active and stored descriptors matched the saved profile; DPI, polling and mode were unchanged.

A busy response during one attempt and a sleeping/nonresponding mouse during another attempt stopped the operation before profile writes. After the user woke the mouse and confirmed 2.4 GHz mode, saving succeeded.

The user reported that the tested functions appear to work, but did not test wheel tilt. This is tentative feedback, not a strict per-action verification. Wheel tilt is assigned to horizontal scrolling. Bluetooth persistence has not been verified.

## Software and preservation

Release bundle built and passed signature verification with the existing development identity. The published release was not changed. The installed UI was visually inspected with CUA. Profile JSON remained byte-identical to its pre-installation backup.

799 dependency-free checks passed. Added checks cover captured inventory/descriptor validation, key/modifier encoding, standard button 4/5 preservation, unsupported-plan rejection before writes, changed-only writes, unchanged-save avoidance, restoration across profile replacement, lost-ACK rollback, wrong-receiver rejection and recovery after rollback failure.

The durable runtime backup is `~/Library/Application Support/NagaController/onboard-profile.json`.

## Follow-up, 2026-09-30

The DPI buttons still held keys J/K from an earlier test. Saving the Test profile rewrote only those two controls to DPI stage up/down; all 21 stored and active descriptors were read back and matched. The user then switched the mouse to Bluetooth with the app closed and reported that the saved assignments work. This is user feedback, not a per-button verification.

The backup identity used to include the receiver's USB location, so moving the dongle behind a different hub blocked saving. Since 2.3.0 the backup is matched by receiver model (`1532:00b4`).
