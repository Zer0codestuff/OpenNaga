# Reliability and feature audit, 2026-10-30

Unreleased source changes based on `4278e5f`, following the PR #10 merge. The owner approved direct commit and push to main on 2026-11-02. The scope covers input delivery, profile persistence/imports, macro execution, recovery and the existing native interface. The USB protocol and firmware-writing algorithms are unchanged. No new dependency or network service is introduced.

## Findings and implemented changes

| Priority | Finding | Result |
| --- | --- | --- |
| High | After a failed profile load, an edit could replace the unreadable file with defaults. | Read errors enter protection mode, retain usable in-memory profiles, pause software remapping and block edits and mouse saves. Explicit reload or backup restore is required. |
| High | Import silently overwrote same-name profiles and could activate the imported selection. | Preview lists collisions and shell-command presence. Keep Both is the default, with unique names and rewritten switches/bindings. Selection stays unchanged. Replacement needs a second confirmation and cannot remove mouse-import protection. |
| High | Save errors left unsaved in-memory assignments active and profile operations could report success. | Profile edits now commit to disk before changing the live mapping or defaults. Failure restores the previous document, selection and history. Mouse import reports local save failures. |
| High | Two running copies could overwrite each other's newer profile document. | Saves compare the exact loaded revision and use a nonblocking advisory lock. A stale instance must reload before editing. This protects cooperating new builds; older binaries do not implement the new lock. |
| High | A newer file format could be decoded partially and rewritten without recognizing its version. | New files identify format version 1. Missing versions remain compatible. Unsupported versions enter protection mode and retain the original bytes. |
| Medium | Direct automatic-profile application did not itself reject imported firmware profiles. | The API and runtime mapping both preserve the imported-profile gate. No automatic path enables their software remapping. |
| Medium | Repeated macro presses could overlap delayed executions from the same button. | A new press restarts only that button's macro. Other buttons stay independent. Cancelled delay closures hold weak execution contexts. |
| Medium | A long text step sent all characters synchronously on the main queue. | Unicode text is sent in small chunks, yielding between them. Cancellation interrupts the remaining text, without changing the clipboard or splitting surrogate pairs. |
| Medium | Macro validation differed between the editor and profile imports. | One validator covers supported actions, keys, modifiers, macro steps, delays and resource limits before persistence or execution. |
| Medium | Deleting a profile left button actions pointing to a missing target. | Both layers' dangling switches become Disabled and app bindings are removed in the same transaction. Undo recovers the whole change. |
| Medium | Stale UserDefaults could override a valid saved profile selection. | The saved document's selection is authoritative. Defaults remain a fallback for older files. |
| Medium | HID held-key observations survived a stopped listener. | Stop and worker cleanup clear native holds and correlation state. Sleep stops input; wake reconnects it. Permission loss also stops observation. Quit blocks permission polling from restarting the service. |
| Low | Failed shell launches logged the complete command. | Error logs omit command text. |
| Low | Legacy modifier aliases could appear disabled in key editors. | Editors normalize command/control/option aliases and avoid duplicate spellings. |
| Low | README still said OpenNaga lacked per-app profiles. | Corrected the statement using the existing implementation. Competitor pricing and support claims were not refreshed. |

The first profile regression run failed on three assertions: preservation of unreadable bytes, preservation of a same-name original and creation of a separately named incoming profile. The corrected implementation passes those regressions.

## User-facing improvements

- Native visual macro editing: add key, text or delay steps; record shortcuts; select, reorder and remove steps; apply the draft explicitly. Advanced JSON remains available.
- Direct editing of existing text snippets, rather than showing them through the key editor.
- Stop Macros and running-macro count in Status.
- Undo/Redo for up to 12 session document changes, including edits, imports, renames, deletion and app bindings. Option-Command-Z and Shift-Option-Command-Z are native Edit-menu shortcuts. Ordinary text-field Command-Z remains separate.
- A profile recovery group with Reload Profiles, Show Folder and Restore Backup.
- A read-only protection state that explains why editing and remapping are paused.
- Restart Input Service, which resets input state without writing mouse settings.

The visual review caught two additional defects before handoff: Undo could leave the selected macro-step index outside the shorter array, and shortcuts attached only to a SwiftUI menu were unreliable. The index now clamps when the step count changes, and profile shortcuts live in the native Edit menu. Rechecking the flow showed the expected step selection after Undo and a successful Redo.

## Persistence contract

`ProfileFileStore` owns file reading, validation, locking, previous-save backup and atomic publication. Temporary files are created with `mkstemp`, written completely, flushed and renamed in the same directory. Profile and backup files have mode 0600. Newly created storage directories have mode 0700. Primary and lock paths must not be symbolic links.

`profiles.backup.json` contains the previous successful document, not a full version archive. Explicit restore preserves the current file as `profiles.before-restore.<id>.json` before publishing the validated backup. Restore does not touch `onboard-profile.json`, hardware assignments or the upstream app's folder.

Limits are 2 MiB per document, 128 profiles, 19 controls per layer, 64 keys per key sequence, 256 macro steps and 16 KiB of macro text. Delays are bounded to 600,000 ms per step and one hour in total. Invalid data fails before it changes disk or the runtime mapping. Existing user files without a version field remain supported. Hardware DPI functions in the Hypershift layer remain supported for explicit Save to Mouse.

## Verification

- Release executable and locally signed app bundle compile; all 1473 dependency-free checks pass, including 63 profile-protection checks and 11 deterministic macro-runner checks.
- The dependency-free suite exercises damaged-file recovery, stale writers, lock contention, backup-write failure, linked-path rejection, owner-only permissions, bounded history, rename/delete/import references, protection metadata, unsupported versions and existing-file compatibility.
- Deterministic macro tests use an injected scheduler instead of timing sleeps to check restart, independence, cancellation, bounded Unicode chunks and complete output ordering. The previous timing-sensitive cancellation check now cancels before yielding; delayed cancellation itself is covered by the deterministic scheduler tests.
- Real macOS event delivery passed in a temporary native host using the production mapper: 116 Unicode characters including accents, emoji and newlines arrived exactly; a restarted macro emitted two first strokes and one final stroke; the other button's macro completed independently; Stop Macros prevented its delayed final stroke. Every captured key press had a release, and no macros remained active.
- Native UI flows passed in an isolated data folder; dark macro editing and light Status were also inspected at the minimum 980x700 content size without horizontal clipping: draft macro edits did not save until Apply, Apply persisted, Undo/Redo worked, Unicode text edits persisted, Keep Both preserved the original and rewrote incoming references, a damaged file entered protection mode, explicit restore recovered valid profiles and retained the damaged bytes exactly, and Restart Input Service left interception running.
- The actual user's three profiles passed the new validator in a read-only helper. No migration was needed.
- A construction-only microbenchmark with 8,192 ASCII characters created 16,384 CGEvents using the previous per-character algorithm and 1,024 with the chunked runner. The previous loop occupied about 68.9 ms; the largest new scheduled text chunk took about 0.047 ms in that run. Events were not posted to macOS in this benchmark. These figures do not measure game latency or physical Bluetooth performance.

Logs, fixtures, benchmark output and temporary hosts are under `.build/improvement-audit`. Real profile data and the installed app are kept separate from UI fixtures. Preference changes made by UI testing are restored before handoff.

## Remaining work, in priority order

1. **Physical lifecycle and macro regression pass.** Repeat the new macro restart/stop checks on the real Bluetooth mouse, then test sleep/wake while holding a mapped control, permission loss/regrant and USB unplug/replug. The mouse disconnected during this work; the earlier PR #10 Bluetooth capture remains separate evidence. Do not claim these new hardware paths are verified from the native synthetic-event host.
2. **Onboard Hypershift over Bluetooth.** Existing USB readbacks do not establish per-button execution or persistence after a power cycle. Use an explicit, backed-up hardware test with feasible thumb-plus-wheel combinations. No firmware writes were performed in this audit.
3. **Input attribution under extreme scheduling delay.** The 25 ms timestamp window still cannot prove ownership of simultaneous identical keys from separate devices. An event reaching the tap before its HID callback still passes through. A future diagnostic should distinguish genuine mouse evidence from ordinary keyboard events before reporting missed remaps; widening the match window or waiting in the tap is not an acceptable fix.
4. **Bluetooth battery reporting.** Current UI battery data comes from the USB controller. The unused `BatteryMonitor` is not wired into the app and can select an arbitrary connected Battery Service peripheral. Do not enable that fallback. A useful implementation needs a verified association with this mouse, bounded retries and explicit unavailable/stale states.
5. **Safer advanced gaming macros.** Press/release/hold steps and mouse steps would add useful combinations, but need balanced cancellation across stop, profile changes, disconnect and sleep before release. Keep them software-only and show that they cannot be saved to mouse memory.
6. **Profile editing at scale.** Assignment copy/paste across controls and layers, retained unapplied drafts with navigation guards, profile search and conflict-aware presets are useful next additions. They should use the same validated transactions and undo history instead of bypassing persistence rules.
7. **Command job visibility.** Shell commands still have no completion status, cancellation UI or user-selected timeout. Add these as explicit job controls without logging command content or silently killing long-running user jobs.
8. **Distribution trust.** Public releases remain ad-hoc signed and not notarized. Developer ID signing and notarization need the owner's Apple credentials and a separately validated release workflow. Local development signing does not supply notarization.

The source changes remain unreleased. The owner approved direct commit and push to main on 2026-11-02 and then asked to stop further hardware testing. Bluetooth enumeration was observed again; physical macro and sleep/wake checks remain pending. No version bump, release asset or installed-app replacement is included.
