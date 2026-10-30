# PR #10 input correlation review

Reviewed on 2026-10-30 against `Zer0codestuff/OpenNaga` main `600a754` and PR #10 head `d5d959c`. The local candidate includes additional regression checks and documentation corrections. This record distinguishes code checks from physical input evidence.

## Code and security review

- Reviewed all six changed PR files and their event-tap, mapper, device identity and reset call paths. No new network access, dependencies, shell execution, permission requests, persisted settings or hardware writes are introduced.
- The original code drops a press when an HID release more than 25 ms later arrives before the tap handles the press. A comparison compiled from the base and candidate reproduces the failure on the base and preserves both edges on the candidate.
- The candidate retains at most 128 edges in each of two matchers, with a one-second timestamp retention interval. Matching still requires the same control, direction and a timestamp difference of at most 25 ms. Consumption removes the edge once. The longer retention does not expand the matching window or enumerate additional devices.
- Each correlation lock encloses only bounded in-memory matcher operations. There are no nested correlation locks, condition waits or retry loops. The HID held-state lock is released before recording an edge. The removed late-edge wait from PR #9 is absent.
- The new system-disabled-tap message contains the disable reason, without key content. Existing synthetic event markers, supported-device checks and held-output cleanup remain in place.
- No new security defect was identified in the reviewed change. Timestamp correlation remains a heuristic: nearly simultaneous identical keys from different devices can be ambiguous. An event delivered to the tap before its HID callback still passes through. Higher HID QoS does not establish callback ordering. README now states these limits instead of guaranteeing that keyboards can never be affected.

## Automated verification

- `swift build -c release --product NagaController`: passed.
- Original PR suite, `bash Scripts/test.sh`: 1368 checks passed.
- Expanded suite: 1399 checks passed. Added 31 checks covering delayed batches across all 12 side controls, the 128-edge capacity, the unchanged matching window, single consumption and 8,000 concurrently recorded/consumed edges.
- An isolated comparison/stress executable using the actual matcher source processed 80,000 contended edges exactly once, then exercised concurrent resets, recording and consumption. The optimized run completed the ownership phase in approximately 47 ms. A separately compiled Thread Sanitizer run passed without a reported race. These are matcher measurements, not end-to-end game latency measurements.
- `git diff --check`: passed. Existing macOS 14 `onChange` deprecation warnings remain.

Temporary logs and audit helpers are under `.build/pr-10-audit`, outside version control. The historical macro-cancellation test uses short wall-clock sleeps; its reported sensitivity under scheduler load is not evidence of a product regression.

## Physical Bluetooth verification

The connected device enumerates as `Naga V2 HS`, vendor/product `068e:00b5`, transport `Bluetooth Low Energy`, and is not a virtual HID device. The temporary signed test host runs the production `HIDListener`, `EventTapManager` and `ButtonMapper` unchanged. Accessibility and Input Monitoring are granted. It does not load or save user profiles and never opens a hardware settings transport.

The owner supplied physical mouse and keyboard input; the maintainer inspected and asserted the captured event counts:

| Phase | Physical/control evidence | Events after the production tap |
| --- | --- | --- |
| Baseline | 5 side-button 6 presses and 5 releases | 5 native `6` presses and 5 releases |
| Remapping | 5 side-button 6 presses and 5 releases | 5 F20 presses and 5 releases; no native `6` edges |
| Load | 22 side-button 6 presses and 22 releases | 22 F20 presses and 22 releases; no native `6` edges |
| Regular keyboard | 5 keyboard `6` presses and 5 releases, no mouse-button 6 evidence | 5 native `6` presses and 5 releases; no F20 edges |

The load phase also captured a modified F20 press/release pair, 325 movement events, 3 left clicks, 1 right click and 7 scroll events. The owner was instructed to include a two-second hold and Shift with the button and reported completing the sequence. The capture counts establish balanced mapped key edges; they do not independently measure the hold duration.

Load consisted of background CPU work plus bounded 60 ms main-run-loop work on a 100 ms timer. The largest observed event timestamp-to-tail-observer delay in that phase was approximately 381 ms, while all 22 tested remaps remained correlated. This deliberately induced delay is not a benchmark of normal app latency. There were no observer-disable events in the captured counts. All 27 mapped presses had matching releases and no original-key leakage. The test host stopped and removed its taps afterward. No USB receiver was required.

The installed app is 2.6.0 and is not replaced by this review. The user's imported `From Mouse` profile remains protected; it is not enabled for software remapping. The temporary host maps only the baseline-verified side control in its own in-memory configuration. The SHA-256 of the user's `profiles.json` is identical before and after testing. No mouse flash, DPI, polling rate or driver-mode writes are part of this review.
