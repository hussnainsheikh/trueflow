# Test Matrix

This file distinguishes automated coverage from manual macOS integration checks.
Do not mark a manual result as passed without performing it on the stated build.

Status vocabulary: `AUTOMATED VERIFIED`, `MANUALLY VERIFIED`,
`PENDING HUMAN VALIDATION`, and `FAILED`.

## Repository automated checks

| Area     | Check                  | Command                                                    | Milestone 1 result             |
| -------- | ---------------------- | ---------------------------------------------------------- | ------------------------------ |
| Frontend | Formatting             | `pnpm format:check`                                        | AUTOMATED VERIFIED             |
| Frontend | Lint                   | `pnpm lint`                                                | AUTOMATED VERIFIED             |
| Frontend | Strict types           | `pnpm typecheck`                                           | AUTOMATED VERIFIED             |
| Frontend | Component/bridge tests | `pnpm test`                                                | AUTOMATED VERIFIED — 7 passed  |
| Frontend | Production bundle      | `pnpm build`                                               | AUTOMATED VERIFIED             |
| Rust     | Formatting             | `cargo fmt --all --check`                                  | AUTOMATED VERIFIED             |
| Rust     | Lint                   | `cargo clippy --all-targets --all-features -- -D warnings` | AUTOMATED VERIFIED             |
| Rust     | Unit/lifecycle tests   | `cargo test --all-features`                                | AUTOMATED VERIFIED — 25 passed |
| Desktop  | Production app         | `pnpm tauri build`                                         | AUTOMATED VERIFIED             |
| Desktop  | Bundle metadata        | Inspect microphone purpose and `LSUIElement` with `plutil` | AUTOMATED VERIFIED             |
| Desktop  | Native overlay startup | Verify accessory policy/flags/level in structured logs     | AUTOMATED VERIFIED             |

Rust tests use permission, audio-input, session, and UI test doubles. They cover
valid/invalid/recovery state transitions; press/release and duplicate events;
early release; permission, startup, no-device, and capture errors; cleanup;
bounded-buffer eviction; scalar meter events; and 50 repeated lifecycle cycles.
Native tests also verify pre-macOS-15 and macOS-15+ overlay collection behavior
and active-screen placement across normal, negative-origin, and constrained
screen geometries. They do not prove physical shortcut, microphone, focus,
full-screen rendering, or permission behavior.

## Milestone 2 automated checks

| Area          | Check                             | Command                                                    | Milestone 2 result                    |
| ------------- | --------------------------------- | ---------------------------------------------------------- | ------------------------------------- |
| Frontend      | Formatting                        | `pnpm format:check`                                        | AUTOMATED VERIFIED                    |
| Frontend      | Lint                              | `pnpm lint`                                                | AUTOMATED VERIFIED                    |
| Frontend      | Strict types                      | `pnpm typecheck`                                           | AUTOMATED VERIFIED                    |
| Frontend      | Transcript/status UI tests        | `pnpm test`                                                | AUTOMATED VERIFIED — 8 passed         |
| Frontend      | Production bundle                 | `pnpm build`                                               | AUTOMATED VERIFIED                    |
| Rust          | Formatting                        | `cargo fmt --all -- --check`                               | AUTOMATED VERIFIED                    |
| Rust          | Lint                              | `cargo clippy --all-targets --all-features -- -D warnings` | AUTOMATED VERIFIED                    |
| Rust          | Unit tests                        | `cargo test --all-features`                                | AUTOMATED VERIFIED — 46 passed        |
| Rust          | Streaming integration tests       | `cargo test --all-features`                                | AUTOMATED VERIFIED — 2 passed         |
| Benchmark     | Example builds with all targets   | Rust all-target compilation/clippy                         | AUTOMATED VERIFIED                    |
| Desktop       | Production app                    | `pnpm tauri build`                                         | AUTOMATED VERIFIED                    |
| Desktop       | Bundle metadata                   | Inspect built application `Info.plist`                     | AUTOMATED VERIFIED                    |
| Security      | Provider secret/frontend boundary | Scan `dist/` for key variable, endpoint, and auth header   | AUTOMATED VERIFIED                    |
| Live provider | Deepgram WebSocket/accuracy       | Interactive credentialed run                               | PENDING HUMAN VALIDATION              |
| Performance   | Corpus latency/accuracy benchmark | `stt_benchmark` with recorded corpus                       | PENDING HUMAN VALIDATION — 0/50 audio |

Automated Rust coverage verifies sample-bounded queue behavior, ordered audio,
early audio during delayed connection, exactly one bounded startup retry,
revised interim replacement, final-segment assembly, explicit finalization,
graceful close, cancellation, rate limiting, malformed provider responses,
finalization timeout with partial recovery, end-to-end mock success,
failure cleanup, and a successful subsequent operation. Mock tests never contact
Deepgram and are not evidence of live accuracy or latency.

## Milestone 2 manual provider and macOS acceptance

Record build identity, macOS version, hardware, microphone, network, Deepgram
model/configuration, date, and tester. Record actual measured values; do not
infer latency from compilation or mock tests.

### Provider configuration and basic transcription

| Scenario                                 | Expected result                                             | Status                                                                   |
| ---------------------------------------- | ----------------------------------------------------------- | ------------------------------------------------------------------------ |
| Missing development key                  | Recoverable configuration error; app remains usable         | PENDING HUMAN VALIDATION                                                 |
| Invalid/revoked key                      | Safe authentication error; no secret shown or logged        | PENDING HUMAN VALIDATION                                                 |
| Basic phrase                             | Live partial and provider-final shown, no application crash | MANUALLY VERIFIED 2026-08-16 (user report; phrase/accuracy not recorded) |
| Messy natural phrase                     | Spoken content retained without AI cleanup expectations     | PENDING HUMAN VALIDATION                                                 |
| Very short one/two-word dictation        | Release finalizes cleanly                                   | PENDING HUMAN VALIDATION                                                 |
| Immediate release / no useful speech     | Clean completion/recovery; no stale transcript              | PENDING HUMAN VALIDATION                                                 |
| 30–60 second continuous phrase           | Partials continue; beginning/end retained; no growing lag   | PENDING HUMAN VALIDATION                                                 |
| Previous operation followed by a new one | No transcript or provider-session leakage                   | PENDING HUMAN VALIDATION                                                 |

Required spoken samples:

- `Hello, this is a test of TrueFlow.`
- `Okay so I think we should um probably move the second scene actually no keep the second scene but move the reveal later.`

### Latency and transcript observations

| Metric                       | Observed result | Status                   |
| ---------------------------- | --------------- | ------------------------ |
| Provider connection latency  | Not measured    | PENDING HUMAN VALIDATION |
| Time to first partial        | Not measured    | PENDING HUMAN VALIDATION |
| Release-to-final latency p50 | Not measured    | PENDING HUMAN VALIDATION |
| Release-to-final latency p95 | Not measured    | PENDING HUMAN VALIDATION |
| Finalization timeout rate    | Not measured    | PENDING HUMAN VALIDATION |
| Proper-noun behavior         | Not measured    | PENDING HUMAN VALIDATION |
| Provider failure rate        | Not measured    | PENDING HUMAN VALIDATION |
| Cost metadata                | Not recorded    | PENDING HUMAN VALIDATION |
| macOS Dictation comparison   | Not performed   | PENDING HUMAN VALIDATION |

### Reliability, cancellation, and failure recovery

| Scenario                              | Expected result                                             | Status                                     |
| ------------------------------------- | ----------------------------------------------------------- | ------------------------------------------ |
| 20 rapid consecutive sessions         | No overlap, leaks, stale text, or orphaned provider session | PENDING HUMAN VALIDATION                   |
| 100 consecutive acceptance dictations | No application crash; every failure visible/recoverable     | PENDING HUMAN VALIDATION                   |
| Network unavailable before speech     | Bounded retry; early audio bounded; recoverable error       | PENDING HUMAN VALIDATION                   |
| Network disabled during speech        | Mic stops; best-known text incomplete; next attempt works   | PENDING HUMAN VALIDATION                   |
| Provider finalization timeout         | Available text preserved and explicitly incomplete          | PENDING HUMAN VALIDATION                   |
| TLS crypto provider regression        | Connection starts without rustls panic or stuck Finalizing  | MANUALLY VERIFIED 2026-08-16 (user report) |
| Quit while listening/finalizing       | Audio, socket, queue, and tasks close                       | PENDING HUMAN VALIDATION                   |
| Long provider slowdown                | Memory stays bounded; no silent speech loss                 | PENDING HUMAN VALIDATION                   |

### Author baseline and provider comparison

| Spoken sample                                             | Trueflow result | macOS Dictation result | Status                   |
| --------------------------------------------------------- | --------------- | ---------------------- | ------------------------ |
| `Aeryndor crossed the Ashen Coast before sunrise.`        | Not tested      | Not tested             | PENDING HUMAN VALIDATION |
| `Kael told Elara that Vhal'Tor would fall before winter.` | Not tested      | Not tested             | PENDING HUMAN VALIDATION |

### Milestone 1 regression checks during live STT

| Scenario                              | Expected result                                          | Status                   |
| ------------------------------------- | -------------------------------------------------------- | ------------------------ |
| Notes/Chrome/VS Code normal windows   | Shortcut, overlay, meter, focus, capture remain reliable | PENDING HUMAN VALIDATION |
| Chrome native full screen             | Partial/final overlay visible without focus stealing     | PENDING HUMAN VALIDATION |
| Safari/VS Code native full screen     | Overlay remains in the active full-screen Space          | PENDING HUMAN VALIDATION |
| Microphone permission denial/recovery | Existing actionable behavior does not regress            | PENDING HUMAN VALIDATION |
| Device loss during streaming          | Capture/provider clean up; later dictation works         | PENDING HUMAN VALIDATION |

## Milestone 1 manual macOS acceptance

Use the production `.app` where practical. Record the tested macOS version,
hardware/input device, build identity, date, and tester with the results.

### Shortcut behavior

| Scenario                         | Expected result                          | Status                   |
| -------------------------------- | ---------------------------------------- | ------------------------ |
| Shortcut while Notes has focus   | `Option + Space` begins/ends one capture | PENDING HUMAN VALIDATION |
| Shortcut while Chrome has focus  | Main window focus is unnecessary         | PENDING HUMAN VALIDATION |
| Shortcut while VS Code has focus | Main window focus is unnecessary         | PENDING HUMAN VALIDATION |
| Shortcut in another normal app   | System-wide behavior remains reliable    | PENDING HUMAN VALIDATION |
| Press                            | Enters Starting, then Listening          | PENDING HUMAN VALIDATION |
| Sustained hold                   | Exactly one capture stays active         | PENDING HUMAN VALIDATION |
| Release                          | Enters Finalizing and stops promptly     | PENDING HUMAN VALIDATION |
| Rapid press/release              | No stuck or orphaned recording           | PENDING HUMAN VALIDATION |
| Duplicate/accidental events      | State and session remain valid           | PENDING HUMAN VALIDATION |
| Shortcut registration conflict   | Actionable fatal error, no false success | PENDING HUMAN VALIDATION |

### Focus and overlay

| Scenario                      | Expected result                                | Status                   |
| ----------------------------- | ---------------------------------------------- | ------------------------ |
| Overlay appears               | Original application remains focused           | PENDING HUMAN VALIDATION |
| Type while overlay is visible | Typing remains in original application         | PENDING HUMAN VALIDATION |
| Overlay disappears            | Focus does not change                          | PENDING HUMAN VALIDATION |
| Starting/Listening/Finalizing | Overlay follows Rust state                     | PENDING HUMAN VALIDATION |
| Recoverable error             | Concise actionable message then safe dismissal | PENDING HUMAN VALIDATION |

### Full-screen applications and Spaces

For every row, verify that the overlay and meter are visible above application
content, the original application remains focused throughout press/hold/release,
and the overlay disappears without changing focus.

| Scenario                                    | Expected result                                   | Status                   |
| ------------------------------------------- | ------------------------------------------------- | ------------------------ |
| Normal Chrome window                        | Overlay appears above Chrome                      | PENDING HUMAN VALIDATION |
| Chrome native full screen                   | Overlay joins Chrome's full-screen Space          | PENDING HUMAN VALIDATION |
| Safari native full screen                   | Overlay joins Safari's full-screen Space          | PENDING HUMAN VALIDATION |
| VS Code native full screen                  | Overlay joins VS Code's full-screen Space         | PENDING HUMAN VALIDATION |
| Another native full-screen app              | Overlay joins the active full-screen Space        | PENDING HUMAN VALIDATION |
| Switch between desktop Spaces               | Overlay follows the active user context           | PENDING HUMAN VALIDATION |
| App window on another Space                 | Overlay appears on the currently active Space     | PENDING HUMAN VALIDATION |
| Enter full screen after Trueflow launch     | Overlay remains available in full screen          | PENDING HUMAN VALIDATION |
| Launch Trueflow while Chrome is full screen | First dictation overlay appears in Chrome's Space | PENDING HUMAN VALIDATION |
| Leave full screen                           | Overlay remains available on the desktop Space    | PENDING HUMAN VALIDATION |
| Repeated desktop/full-screen transitions    | Overlay is never stranded on an old Space         | PENDING HUMAN VALIDATION |
| Repeated hold-to-talk in full screen        | Every cycle shows, meters, and hides correctly    | PENDING HUMAN VALIDATION |

### Microphone and devices

| Scenario                                   | Expected result                              | Status                   |
| ------------------------------------------ | -------------------------------------------- | ------------------------ |
| Normal default microphone                  | Capture starts and stops cleanly             | PENDING HUMAN VALIDATION |
| Normal speech                              | Scalar meter responds visibly                | PENDING HUMAN VALIDATION |
| Silence                                    | Meter settles near zero                      | PENDING HUMAN VALIDATION |
| No input device                            | Recoverable error; no crash or stuck state   | PENDING HUMAN VALIDATION |
| Device unavailable                         | Recoverable error and subsequent retry works | PENDING HUMAN VALIDATION |
| Disconnect during capture, where practical | Recoverable error and resources release      | PENDING HUMAN VALIDATION |
| Default device changed between sessions    | Next session uses new default                | PENDING HUMAN VALIDATION |

### Microphone permissions

| Scenario                       | Expected result                              | Status                   |
| ------------------------------ | -------------------------------------------- | ------------------------ |
| First request                  | macOS prompt includes the documented purpose | PENDING HUMAN VALIDATION |
| Permission granted             | Current/retried capture reaches Listening    | PENDING HUMAN VALIDATION |
| Permission denied              | No crash; actionable settings message        | PENDING HUMAN VALIDATION |
| Relaunch after denial          | Denied state remains safe and visible        | PENDING HUMAN VALIDATION |
| Later grant in System Settings | A new attempt succeeds without app reset     | PENDING HUMAN VALIDATION |
| Revoke after previous grant    | New attempt detects revocation               | PENDING HUMAN VALIDATION |
| Restore after revocation       | App recovers on a later attempt              | PENDING HUMAN VALIDATION |

### Repeated-use acceptance run

Perform 50 consecutive physical hold-to-talk cycles. For every cycle: press the
shortcut, verify Listening, speak briefly, verify meter response, release,
verify capture stops, and verify the app is usable for the next cycle.

| Field                             | Result                   |
| --------------------------------- | ------------------------ |
| Status                            | PENDING HUMAN VALIDATION |
| Attempted                         | 0 / 50                   |
| Successful                        | 0 / 50                   |
| Failures                          | Not tested               |
| Resource/state observations       | Not tested               |
| Idle CPU/memory observations      | Not tested               |
| Recording CPU/memory observations | Not tested               |

## Earlier Milestone 0 manual checks

| Scenario                 | Expected result                             | Status                       |
| ------------------------ | ------------------------------------------- | ---------------------------- |
| Fresh-clone setup        | Documented commands install and launch      | PENDING HUMAN VALIDATION     |
| `pnpm dev`               | One command opens the Tauri app             | MANUALLY VERIFIED 2026-08-14 |
| First launch             | App data directory and database are created | MANUALLY VERIFIED 2026-08-14 |
| Second launch            | Migration 1 remains applied once            | PENDING HUMAN VALIDATION     |
| Offline launch           | Foundation starts without network           | PENDING HUMAN VALIDATION     |
| Invalid `TRUEFLOW_ENV`   | Startup fails without false ready state     | PENDING HUMAN VALIDATION     |
| Production `.app` launch | Built bundle opens normally                 | PENDING HUMAN VALIDATION     |

## Milestone 3 automated checks

The latest 2026-08-17 paste-transport correction passed Rust formatting, clippy
with warnings denied, 63 unit tests, 2 integration tests, frontend formatting/lint/
strict typechecking, 10 frontend tests, the production frontend build, and the
native production application build.

| Area     | Check                            | Command                                                    | Milestone 3 result                                                                |
| -------- | -------------------------------- | ---------------------------------------------------------- | --------------------------------------------------------------------------------- |
| Frontend | Formatting                       | `pnpm format:check`                                        | AUTOMATED VERIFIED                                                                |
| Frontend | Lint                             | `pnpm lint`                                                | AUTOMATED VERIFIED                                                                |
| Frontend | Strict types                     | `pnpm typecheck`                                           | AUTOMATED VERIFIED                                                                |
| Frontend | Permission/outcome UI tests      | `pnpm test`                                                | AUTOMATED VERIFIED — 10 passed                                                    |
| Frontend | Production bundle                | `pnpm build`                                               | AUTOMATED VERIFIED                                                                |
| Rust     | Formatting                       | `cargo fmt --all -- --check`                               | AUTOMATED VERIFIED                                                                |
| Rust     | Lint                             | `cargo clippy --all-targets --all-features -- -D warnings` | AUTOMATED VERIFIED                                                                |
| Rust     | Unit tests                       | `cargo test --all-features`                                | AUTOMATED VERIFIED — 63 passed                                                    |
| Rust     | Streaming integration regression | `cargo test --all-features`                                | AUTOMATED VERIFIED — 2 passed                                                     |
| Desktop  | Production app                   | `pnpm tauri build`                                         | AUTOMATED VERIFIED                                                                |
| Security | Tauri capabilities               | Inspect `src-tauri/capabilities/default.json`              | AUTOMATED VERIFIED — core only; no frontend clipboard/shell/filesystem permission |

Deterministic insertion tests use no live Accessibility permission. They verify
direct success stops the chain; direct unsupported/failure dispatches exactly
one paste; paste failure leaves exactly one clipboard copy; clipboard failure
returns `Failed`; permission denial skips automatic strategies; secure fields
do not insert or touch the clipboard; plain-text restoration is attempted after
paste; restoration failure is never reported as restored; Unicode and multiline
text remain intact; only finalized text reaches insertion; stale operation IDs
cannot mutate the active operation; and transcript state survives recovery.

## Milestone 3 manual macOS acceptance

Record build identity, macOS version, hardware, app version, Deepgram model,
date, tester, actual insertion strategy from structured logs, and observed
latency. No row below was performed in this implementation run.

### User-reported basic compatibility (2026-08-17)

These results cover only whether one basic dictation appeared in the target app.
They do not validate strategy, cursor/selection behavior, focus, multiline text,
clipboard restoration, full screen, or repeated use. All results below came from
the development builds before the latest window-server-routed paste correction.

| App                   | Pre-fix basic result | Corrected build              |
| --------------------- | -------------------- | ---------------------------- |
| TextEdit              | MANUALLY VERIFIED    | RETEST PENDING               |
| Messages              | MANUALLY VERIFIED    | RETEST PENDING               |
| Terminal              | MANUALLY VERIFIED    | RETEST PENDING               |
| Notes                 | MANUALLY VERIFIED    | RETEST PENDING               |
| Chrome                | MANUALLY VERIFIED    | RETEST PENDING               |
| WhatsApp              | MANUALLY VERIFIED    | RETEST PENDING               |
| VS Code               | FAILED               | MANUALLY VERIFIED 2026-08-18 |
| Sublime Text          | FAILED               | RETEST PENDING               |
| Microsoft Word/Office | FAILED               | ALTERNATING — RETEST PENDING |
| Notion                | FAILED               | RETEST PENDING               |
| Slack                 | FAILED               | RETEST PENDING               |

Microsoft Word succeeded on alternating attempts after the direct-insertion
verification landed. Its log showed the paste strategy reached normally but
completing in 61 ms with no `clipboard_restore_started`, which exposed clipboard
state coupling between consecutive dictations: Trueflow's own leftover
transcript was classified as restorable user content by the next run, which then
restored it over its own transcript before Word had read the pasteboard. Word
retesting must therefore run at least four consecutive dictations, and record
`clipboard_captured` `owned_by_trueflow` and `safely_restorable` for each.

Logs captured on 2026-08-18 show VS Code resolving as an editable `AXTextArea`
with `direct_insertion_supported: true` and completing as `InsertedDirectly` in
3 ms with `error_code: none` while inserting nothing. The paste strategy is
never reached in the failing applications; `AXSelectedText` accepts the write and
discards it. Direct insertion now verifies the caret moved before reporting
success. When retesting, record whether `direct_insert_discarded` appears and
which strategy the outcome names; the failing applications should now report
`InsertedViaPaste`, which is itself still an unconfirmed path.

The first failure was traced to unknown/container Accessibility roles being
rejected before paste, but permitting those roles and improving paste dispatch
was insufficient in live use. The later `No editable field was focused` result
proved the affected applications stopped during focused-element lookup. Supplied
logs then confirmed that app-scoped lookup was also insufficient and that paste
was never attempted. The latest build requests Electron's manual Accessibility
tree and, if the editor remains opaque, classifies the known foreground app as
unknown so it reaches one PID-revalidated paste attempt. Direct mutation remains
disabled without a resolved element. A later live test reached
`InsertedViaPaste` in VS Code and Sublime Text but inserted no content, proving
process-directed events did not reach the focused responder. The latest build
posts the validated chord through the HID event tap for normal window-server
routing. When manual `Cmd+V` worked but the HID event still did not, clipboard
handling was ruled out and the synthetic chord was changed from flag-only V to
explicit Command down, V down/up, and Command up events. Actual application
behavior requires the latest-build retest.

| App                     | Control/type               | Direct attempted? | Actual strategy | Insert correct? | Existing text preserved? | Selection handled? | Multiline? | Focus preserved? | Full-screen? | Clipboard restored? | Status                   | Notes                                  |
| ----------------------- | -------------------------- | ----------------- | --------------- | --------------- | ------------------------ | ------------------ | ---------- | ---------------- | ------------ | ------------------- | ------------------------ | -------------------------------------- |
| Notes                   | Empty note                 | Not tested        | Not tested      | Not tested      | Not tested               | N/A                | Not tested | Not tested       | N/A          | Not tested          | PENDING HUMAN VALIDATION | —                                      |
| Notes                   | Cursor at end/middle       | Not tested        | Not tested      | Not tested      | Not tested               | N/A                | Not tested | Not tested       | N/A          | Not tested          | PENDING HUMAN VALIDATION | —                                      |
| Notes                   | Selected text              | Not tested        | Not tested      | Not tested      | Not tested               | Not tested         | Not tested | Not tested       | N/A          | Not tested          | PENDING HUMAN VALIDATION | —                                      |
| TextEdit                | Empty/end/middle/selection | Not tested        | Not tested      | Not tested      | Not tested               | Not tested         | Not tested | Not tested       | N/A          | Not tested          | PENDING HUMAN VALIDATION | —                                      |
| Chrome                  | Standard `<input>`         | Not tested        | Not tested      | Not tested      | Not tested               | Not tested         | N/A        | Not tested       | Not tested   | Not tested          | PENDING HUMAN VALIDATION | Include native full screen             |
| Chrome                  | `<textarea>`               | Not tested        | Not tested      | Not tested      | Not tested               | Not tested         | Not tested | Not tested       | Not tested   | Not tested          | PENDING HUMAN VALIDATION | —                                      |
| Chrome                  | `contenteditable`          | Not tested        | Not tested      | Not tested      | Not tested               | Not tested         | Not tested | Not tested       | Not tested   | Not tested          | PENDING HUMAN VALIDATION | —                                      |
| Safari                  | Standard input/textarea    | Not tested        | Not tested      | Not tested      | Not tested               | Not tested         | Not tested | Not tested       | Not tested   | Not tested          | PENDING HUMAN VALIDATION | Include native full screen             |
| VS Code                 | Editor                     | Not tested        | Not tested      | Not tested      | Not tested               | Not tested         | Not tested | Not tested       | Not tested   | Not tested          | PENDING HUMAN VALIDATION | Multiline, repeated, full screen       |
| Google Docs             | Editing canvas             | Not tested        | Not tested      | Not tested      | Not tested               | Not tested         | Not tested | Not tested       | Not tested   | Not tested          | PENDING HUMAN VALIDATION | Cursor, selection, multiline, repeated |
| Slack, if available     | Composer                   | Not tested        | Not tested      | Not tested      | Not tested               | Not tested         | Not tested | Not tested       | Not tested   | Not tested          | PENDING HUMAN VALIDATION | Optional                               |
| Word, if available      | Document                   | Not tested        | Not tested      | Not tested      | Not tested               | Not tested         | Not tested | Not tested       | Not tested   | Not tested          | PENDING HUMAN VALIDATION | Optional                               |
| Scrivener, if available | Editor                     | Not tested        | Not tested      | Not tested      | Not tested               | Not tested         | Not tested | Not tested       | Not tested   | Not tested          | PENDING HUMAN VALIDATION | Optional                               |
| Obsidian, if available  | Editor                     | Not tested        | Not tested      | Not tested      | Not tested               | Not tested         | Not tested | Not tested       | Not tested   | Not tested          | PENDING HUMAN VALIDATION | Optional                               |

### Safety, permission, fallback, and reliability

| Scenario                              | Expected result                                                                     | Status                          |
| ------------------------------------- | ----------------------------------------------------------------------------------- | ------------------------------- |
| Accessibility granted                 | Permission becomes granted without conflating microphone state                      | PENDING HUMAN VALIDATION        |
| Permission revoked while running      | Transcription completes; direct/paste skipped; text copied with actionable guidance | PENDING HUMAN VALIDATION        |
| Permission restored while running     | Recheck shows granted; next operation inserts automatically                         | PENDING HUMAN VALIDATION        |
| Existing text, cursor in middle       | Existing field content remains intact; transcript inserted at native selection      | PENDING HUMAN VALIDATION        |
| Existing selection                    | Normal native replacement semantics; no rewrite behavior                            | PENDING HUMAN VALIDATION        |
| Unicode, emoji, smart punctuation     | Exact finalized text inserted without range corruption                              | PENDING HUMAN VALIDATION        |
| Read-only/button/page content         | No destructive direct mutation; clipboard-only recovery                             | PENDING HUMAN VALIDATION        |
| Detectable secure/password field      | No automatic insertion or clipboard write; transcript retained visibly              | PENDING HUMAN VALIDATION        |
| Prior plain-text clipboard            | Paste succeeds and prior plain text restores after bounded delay                    | PENDING HUMAN VALIDATION        |
| Prior rich/non-text clipboard         | No false restoration claim; dictated text intentionally remains on clipboard        | PENDING HUMAN VALIDATION        |
| Target changes before insertion       | No stale-target mutation; transcript copied or retained                             | PENDING HUMAN VALIDATION        |
| Trueflow/overlay target               | Automatic insertion excluded                                                        | PENDING HUMAN VALIDATION        |
| Paste dispatch failure                | `CopiedToClipboard`, not automatic success                                          | PENDING HUMAN VALIDATION        |
| Clipboard write failure               | `Failed`; finalized transcript remains in visible state                             | PENDING HUMAN VALIDATION        |
| 20 consecutive inserted dictations    | No duplicate/stale insertion, focus loss, or clipboard corruption                   | PENDING HUMAN VALIDATION — 0/20 |
| 30–60 second dictation then insertion | Beginning/end and multiline preserved; next short operation works                   | PENDING HUMAN VALIDATION        |

### Full-screen regression

| Scenario                   | Expected result                                        | Status                   |
| -------------------------- | ------------------------------------------------------ | ------------------------ |
| Chrome native full screen  | Overlay visible, text inserts, Chrome remains focused  | PENDING HUMAN VALIDATION |
| Safari native full screen  | Overlay visible, text inserts, Safari remains focused  | PENDING HUMAN VALIDATION |
| VS Code native full screen | Overlay visible, text inserts, VS Code remains focused | PENDING HUMAN VALIDATION |

### Milestone 3 latency observations

| Metric                              | Observed result | Status                   |
| ----------------------------------- | --------------- | ------------------------ |
| Release → final transcript          | Not measured    | PENDING HUMAN VALIDATION |
| Final transcript → insert requested | Not measured    | PENDING HUMAN VALIDATION |
| Insert requested → insert completed | Not measured    | PENDING HUMAN VALIDATION |
| Release → text appears              | Not measured    | PENDING HUMAN VALIDATION |

## Milestone 4 automated checks

| Area          | Scenario                                                                                  | Result             |
| ------------- | ----------------------------------------------------------------------------------------- | ------------------ |
| Migration     | Fresh schema reaches v2 and seeds one active General project                              | AUTOMATED VERIFIED |
| Migration     | Existing schema v1 upgrades without losing metadata                                       | AUTOMATED VERIFIED |
| Migration     | Repeated startup is idempotent                                                            | AUTOMATED VERIFIED |
| Migration     | Failed migration rolls back schema and version                                            | AUTOMATED VERIFIED |
| Projects      | CRUD, select, archive/unarchive, delete, active-project invariant                         | AUTOMATED VERIFIED |
| Vocabulary    | CRUD, validation, duplicate policy, cascading delete                                      | AUTOMATED VERIFIED |
| Isolation     | Vocabulary cannot be read/changed through another project                                 | AUTOMATED VERIFIED |
| Snapshot      | Mid-operation project switch stays on captured context; next press changes                | AUTOMATED VERIFIED |
| Provider      | Unsupported capability, deterministic limits, canonical/alias/pronunciation ordering      | AUTOMATED VERIFIED |
| Deepgram      | Repeated keyterm query values and Unicode URL encoding                                    | AUTOMATED VERIFIED |
| Normalization | Canonical capitalization, aliases, forbidden variants, precedence, no cascade             | AUTOMATED VERIFIED |
| Safety        | Boundaries, phrases/whitespace, apostrophes, case policy, Unicode/emoji, empty vocabulary | AUTOMATED VERIFIED |
| Recovery      | Recovered provider transcript is normalized, inserted, and retains warning                | AUTOMATED VERIFIED |
| Frontend      | Runtime DTO validation and project/vocabulary load/create flows                           | AUTOMATED VERIFIED |
| Regression    | Existing state, audio, STT, insertion, and frontend suites                                | AUTOMATED VERIFIED |

## Milestone 4 manual acceptance

No row in this section was performed during the implementation run.

### Persistence and project lifecycle

| Scenario                 | Expected result                                                     | Status                   |
| ------------------------ | ------------------------------------------------------------------- | ------------------------ |
| Fresh profile launch     | Exactly one active General project appears                          | PENDING HUMAN VALIDATION |
| Restart after edits      | Projects, active selection, and vocabulary persist                  | PENDING HUMAN VALIDATION |
| Create/rename project    | UI and next dictation use updated project                           | PENDING HUMAN VALIDATION |
| Archive active project   | Another usable project becomes active                               | PENDING HUMAN VALIDATION |
| Unarchive/select project | Project becomes selectable and active                               | PENDING HUMAN VALIDATION |
| Delete project           | Project vocabulary disappears; another valid active project remains | PENDING HUMAN VALIDATION |

### Vocabulary correctness and isolation

Create Project A with `Aeryndor`, alias `Erindor`, forbidden variant
`Erin Door`, pronunciation `air-in-door`; create Project B without those rules.

| Scenario                      | Expected result                                               | Status                   |
| ----------------------------- | ------------------------------------------------------------- | ------------------------ |
| Project A: “erin door”        | Inserts `Aeryndor`                                            | PENDING HUMAN VALIDATION |
| Project A: “aeryndor”         | Inserts canonical `Aeryndor` capitalization                   | PENDING HUMAN VALIDATION |
| Project B: same speech        | Project A correction does not leak                            | PENDING HUMAN VALIDATION |
| Multi-word term               | Extra normal word spacing safely normalizes                   | PENDING HUMAN VALIDATION |
| Apostrophe variant            | Explicit `Vhal Tor` variant becomes `Vhal'Tor`                | PENDING HUMAN VALIDATION |
| Unicode term                  | Accent/non-Latin/emoji-adjacent text remains intact           | PENDING HUMAN VALIDATION |
| Substring safety              | `Kai` does not alter `Kaiser`; `Ash` does not alter `fashion` | PENDING HUMAN VALIDATION |
| Switch project while speaking | Current operation uses old project; next uses new project     | PENDING HUMAN VALIDATION |

### Provider hints and insertion regression

| Scenario                        | Expected result                                              | Status                   |
| ------------------------------- | ------------------------------------------------------------ | ------------------------ |
| Live Deepgram with vocabulary   | Session connects; logs show safe hint counts, no terms       | PENDING HUMAN VALIDATION |
| More terms than provider budget | Dictation still works; logs report truncation                | PENDING HUMAN VALIDATION |
| Notes/TextEdit insertion        | Normalized text inserts at cursor                            | PENDING HUMAN VALIDATION |
| VS Code insertion               | Existing verified compatibility remains intact               | PENDING HUMAN VALIDATION |
| Word four consecutive attempts  | Record each actual strategy and whether text appears         | PENDING HUMAN VALIDATION |
| STT disconnect after partial    | Recovered normalized text inserts/copies and warning remains | PENDING HUMAN VALIDATION |

### Milestone 4 performance observations

| Metric                      | Observed result                                                              | Status                   |
| --------------------------- | ---------------------------------------------------------------------------- | ------------------------ |
| Active snapshot preparation | Instrumented as `duration_us`; not measured live                             | PENDING HUMAN VALIDATION |
| Provider hint preparation   | Count/truncation instrumented; not measured live                             | PENDING HUMAN VALIDATION |
| Normalization duration      | Unit operations expose `duration_us`; no representative live corpus measured | PENDING HUMAN VALIDATION |
| Regression release → text   | Not measured after Milestone 4                                               | PENDING HUMAN VALIDATION |

## Milestone 5 automated checks

| Area         | Scenario                                                                                | Result             |
| ------------ | --------------------------------------------------------------------------------------- | ------------------ |
| Migration    | Schema v2 upgrades to v3 without losing project/vocabulary data                         | AUTOMATED VERIFIED |
| Built-ins    | Two Voices and five Modes seed deterministically without restart duplicates             | AUTOMATED VERIFIED |
| Voice CRUD   | Create/read/list/update/delete, examples, built-in protection, selected-Voice fallback  | AUTOMATED VERIFIED |
| Isolation    | Project-scoped Voices cannot be listed, read, or selected through another project       | AUTOMATED VERIFIED |
| Modes        | Stable built-in ordering/selection; Rewrite Selection rejected for normal dictation     | AUTOMATED VERIFIED |
| Prompt       | Narrator + Draft, Kael + Dialogue, and My Voice + Clean have distinct instructions      | AUTOMATED VERIFIED |
| Snapshot     | Project, vocabulary, Voice, and Mode remain operation-scoped without cross-session leak | AUTOMATED VERIFIED |
| Provider     | Responses request fields, `store: false`, HTTP mapping, and missing-key handling        | AUTOMATED VERIFIED |
| Bounds       | Explicit timeout, bounded transient retry, and cancellation                             | AUTOMATED VERIFIED |
| Fallback     | Provider failure inserts normalized transcript once and surfaces recoverable warning    | AUTOMATED VERIFIED |
| Stale result | A late transformation result cannot mutate the current operation                        | AUTOMATED VERIFIED |
| Frontend     | Runtime DTO validation plus Voice CRUD and Voice/Mode command interactions              | AUTOMATED VERIFIED |
| Regression   | Existing state, audio, STT, vocabulary, insertion, migration, and UI suites             | AUTOMATED VERIFIED |

## Milestone 5 manual acceptance

No row in this section was performed during the implementation run.

### Voice, Mode, and persistence

| Scenario                   | Expected result                                                        | Status                   |
| -------------------------- | ---------------------------------------------------------------------- | ------------------------ |
| Fresh schema               | My Voice, Narrator, and exactly five ordered Modes appear              | PENDING HUMAN VALIDATION |
| Restart after edits        | User Voice and project selections persist; no duplicate built-ins      | PENDING HUMAN VALIDATION |
| Narrator + Draft           | Readable narrator prose respects facts and project vocabulary          | PENDING HUMAN VALIDATION |
| Kael + Dialogue            | Dialogue follows the manually configured Kael Voice                    | PENDING HUMAN VALIDATION |
| My Voice + Clean Dictation | Wording/meaning stay close; only light cleanup occurs                  | PENDING HUMAN VALIDATION |
| Project A custom Voice     | Voice is absent from Project B and never affects its dictation         | PENDING HUMAN VALIDATION |
| Edit while speaking        | Current operation keeps captured context; next operation uses the edit | PENDING HUMAN VALIDATION |
| Rewrite Selection Mode     | Disabled for normal dictation with dedicated rewrite-shortcut guidance | PENDING HUMAN VALIDATION |

### Provider, fallback, and session safety

| Scenario                         | Expected result                                                                | Status                   |
| -------------------------------- | ------------------------------------------------------------------------------ | ------------------------ |
| Valid OpenAI key                 | One bounded Responses request transforms and inserts result                    | PENDING HUMAN VALIDATION |
| Missing/invalid key              | Normalized transcript inserts once; recoverable transformation warning appears | PENDING HUMAN VALIDATION |
| Provider timeout                 | Request ends within configured bound; normalized transcript inserts            | PENDING HUMAN VALIDATION |
| Rate limit/temporary outage      | At most one retry; fallback inserts once if still unavailable                  | PENDING HUMAN VALIDATION |
| Network interruption             | Recorded speech/final transcript is retained; fallback is visible              | PENDING HUMAN VALIDATION |
| Cancellation                     | Provider work is cancelled; no late transformed result inserts                 | PENDING HUMAN VALIDATION |
| Rapid consecutive dictations     | Each operation uses its own context and inserts exactly once                   | PENDING HUMAN VALIDATION |
| Empty/unusable provider response | Normalized transcript inserts; invalid-response warning appears                | PENDING HUMAN VALIDATION |

### macOS insertion and performance regression

| Scenario                             | Expected result                                                          | Status                   |
| ------------------------------------ | ------------------------------------------------------------------------ | ------------------------ |
| Notes and TextEdit                   | Transformed/fallback text inserts at cursor; focus remains in target     | PENDING HUMAN VALIDATION |
| Chrome/Google Docs                   | Correct text inserts; focus and clipboard remain safe                    | PENDING HUMAN VALIDATION |
| VS Code/Sublime/Word/Notion/Slack    | Record actual strategy and verify repeated insertion                     | PENDING HUMAN VALIDATION |
| Unicode and multiline transformation | Result remains intact through transformation and insertion               | PENDING HUMAN VALIDATION |
| Prior plain/rich clipboard           | Existing Milestone 3 safety guarantees remain intact                     | PENDING HUMAN VALIDATION |
| Full-screen target                   | Overlay stays non-focusable and insertion returns to correct application | PENDING HUMAN VALIDATION |
| Transformation latency               | Start/first-result/completion/fallback timestamps recorded without text  | PENDING HUMAN VALIDATION |
| Release-to-insertion latency         | Measure p50/p95-capable events with representative corpus                | PENDING HUMAN VALIDATION |

## Milestone 6 automated checks

| Area                | Scenario                                                                                                 | Result             |
| ------------------- | -------------------------------------------------------------------------------------------------------- | ------------------ |
| Operation isolation | Dedicated rewrite kind/shortcut; normal and rewrite release events cannot finish each other's session    | AUTOMATED VERIFIED |
| Selection gate      | Invalid/no selection stops before audio, transformation, replacement, and insertion                      | AUTOMATED VERIFIED |
| Prompt              | Spoken instruction and selected source are separately delimited untrusted input                          | AUTOMATED VERIFIED |
| Context             | Captured Project, Vocabulary, and Voice remain stable; next rewrite receives new context                 | AUTOMATED VERIFIED |
| Mode isolation      | Dedicated rewrite task excludes unrelated normal dictation Mode instructions                             | AUTOMATED VERIFIED |
| Source safety       | Injection-like text remains selected source and never becomes system instruction                         | AUTOMATED VERIFIED |
| Revalidation        | Changed app/element/range/text errors stop before document or clipboard mutation                         | AUTOMATED VERIFIED |
| Direct replacement  | Verified direct success stops; a proven no-op may continue to one guarded paste                          | AUTOMATED VERIFIED |
| Paste safety        | A second revalidation precedes clipboard use; dispatch failure restores the prior clipboard once         | AUTOMATED VERIFIED |
| Clipboard safety    | Unrepresentable rich clipboard disables paste instead of approximating or destroying it                  | AUTOMATED VERIFIED |
| Duplicate safety    | Direct success never pastes; paste is dispatched at most once; unverifiable direct mutation never pastes | AUTOMATED VERIFIED |
| Failure behavior    | Transformation failure preserves original selection and never inserts spoken instruction                 | AUTOMATED VERIFIED |
| Cancellation        | Pre-cancelled replacement performs no validation, document mutation, or clipboard operation              | AUTOMATED VERIFIED |
| Stale result        | A result with another operation ID cannot replace the active selection                                   | AUTOMATED VERIFIED |
| Unicode             | macOS ranges use UTF-16 units and preserve emoji/surrogate-pair lengths                                  | AUTOMATED VERIFIED |
| Frontend boundary   | Runtime DTO validation and rewrite-specific overlay states contain no selected source field              | AUTOMATED VERIFIED |
| Regression          | Existing dictation, STT, transformation, vocabulary, insertion, storage, and UI suites                   | AUTOMATED VERIFIED |

Automated native tests use test doubles and pure range/strategy logic. They do
not prove that a particular third-party application's Accessibility tree exposes
stable element identity, selection text/ranges, direct replacement, paste
routing, focus preservation, or rich-text behavior.

## Milestone 6 manual macOS/provider acceptance

No row in this section was performed during the implementation run. Record the
build identity, macOS version, target app/version, editor/control type, provider
configuration, actual rewrite strategy from structured logs, surrounding-text
and clipboard observations, focus result, and measured latency.

### Core rewrite compatibility

Use a passage with unique text before and after the selection. For each row,
verify the source can be captured, only the selected passage changes, surrounding
text is byte-for-byte preserved where the app exposes plain text, the target
keeps focus, and the overlay does not display the selected source.

| App                      | Field/editor                   | Unicode/multiline | Direct/paste strategy | Surrounding text | Focus      | Status                   |
| ------------------------ | ------------------------------ | ----------------- | --------------------- | ---------------- | ---------- | ------------------------ |
| TextEdit                 | Plain-text document            | Not tested        | Not tested            | Not tested       | Not tested | PENDING HUMAN VALIDATION |
| Notes                    | Note body                      | Not tested        | Not tested            | Not tested       | Not tested | PENDING HUMAN VALIDATION |
| Chrome                   | `<textarea>`                   | Not tested        | Not tested            | Not tested       | Not tested | PENDING HUMAN VALIDATION |
| Chrome                   | `contenteditable`              | Not tested        | Not tested            | Not tested       | Not tested | PENDING HUMAN VALIDATION |
| Safari                   | Input/textarea/contenteditable | Not tested        | Not tested            | Not tested       | Not tested | PENDING HUMAN VALIDATION |
| VS Code                  | Monaco editor                  | Not tested        | Not tested            | Not tested       | Not tested | PENDING HUMAN VALIDATION |
| Google Docs              | Editing canvas                 | Not tested        | Not tested            | Not tested       | Not tested | PENDING HUMAN VALIDATION |
| Sublime Text, optional   | Editor                         | Not tested        | Not tested            | Not tested       | Not tested | PENDING HUMAN VALIDATION |
| Microsoft Word, optional | Document                       | Not tested        | Not tested            | Not tested       | Not tested | PENDING HUMAN VALIDATION |
| Notion/Slack, optional   | Editor/composer                | Not tested        | Not tested            | Not tested       | Not tested | PENDING HUMAN VALIDATION |

### Safety and failure matrix

| Scenario                                      | Expected result                                                                          | Status                   |
| --------------------------------------------- | ---------------------------------------------------------------------------------------- | ------------------------ |
| No selection                                  | Actionable “Select text first”; no microphone/provider/document/clipboard mutation       | PENDING HUMAN VALIDATION |
| Empty or unusable spoken instruction          | No transform request; original selection unchanged                                       | PENDING HUMAN VALIDATION |
| Switch application during transformation      | Revalidation aborts; original text and clipboard unchanged                               | PENDING HUMAN VALIDATION |
| Focus another element in the same application | Element identity mismatch aborts                                                         | PENDING HUMAN VALIDATION |
| Move/resize/remove selection                  | Range mismatch aborts; no nearby/same-text search                                        | PENDING HUMAN VALIDATION |
| Edit selected text                            | Source mismatch aborts                                                                   | PENDING HUMAN VALIDATION |
| Duplicate identical passage elsewhere         | Only captured element/range is eligible; no text search                                  | PENDING HUMAN VALIDATION |
| Password/secure field                         | Selected contents are not retrieved or sent; no clipboard write                          | PENDING HUMAN VALIDATION |
| Read-only/noneditable content                 | Rejected before provider call and mutation                                               | PENDING HUMAN VALIDATION |
| Multiple/discontiguous selection              | Rejected with explicit unsupported message                                               | PENDING HUMAN VALIDATION |
| Unsupported Accessibility selection           | Recoverable error; original and clipboard unchanged                                      | PENDING HUMAN VALIDATION |
| Missing/invalid OpenAI key                    | Original unchanged; spoken instruction is not inserted                                   | PENDING HUMAN VALIDATION |
| Provider timeout/rate limit/network failure   | Bounded completion; original unchanged                                                   | PENDING HUMAN VALIDATION |
| Cancel/quit before replacement                | No late provider result or native mutation                                               | PENDING HUMAN VALIDATION |
| Direct AX write proven discarded              | One revalidation then at most one paste                                                  | PENDING HUMAN VALIDATION |
| Rich/non-text prior clipboard                 | Paste disabled if state cannot be safely preserved; original unchanged                   | PENDING HUMAN VALIDATION |
| Plain-text/empty prior clipboard              | Paste may run once and prior clipboard restores only if change count still matches       | PENDING HUMAN VALIDATION |
| Full-screen/Spaces                            | Overlay visible without focus theft; same target revalidated                             | PENDING HUMAN VALIDATION |
| 20 consecutive rewrites                       | No duplicate, stale output, partial mutation, focus loss, or clipboard corruption — 0/20 | PENDING HUMAN VALIDATION |

### Rewrite latency observations

| Metric                                    | Observed result | Status                   |
| ----------------------------------------- | --------------- | ------------------------ |
| Shortcut → selection captured             | Not measured    | PENDING HUMAN VALIDATION |
| Shortcut → first instruction audio frame  | Not measured    | PENDING HUMAN VALIDATION |
| Release → final instruction transcript    | Not measured    | PENDING HUMAN VALIDATION |
| Transform requested → completed           | Not measured    | PENDING HUMAN VALIDATION |
| Revalidation requested → succeeded/failed | Not measured    | PENDING HUMAN VALIDATION |
| Replacement requested → completed         | Not measured    | PENDING HUMAN VALIDATION |
| Release → rewritten text appears p50/p95  | Not measured    | PENDING HUMAN VALIDATION |

## Milestone 7 automated checks

| Area              | Scenario                                                                                                                | Result             |
| ----------------- | ----------------------------------------------------------------------------------------------------------------------- | ------------------ |
| Migration         | Fresh schema and v3 → v4 upgrade; existing writing data retained                                                        | AUTOMATED VERIFIED |
| Onboarding        | Fresh/partial persistence, non-destructive re-entry, real completed-operation gate, duplicate observation               | AUTOMATED VERIFIED |
| Settings          | Rust persistence/restart, backend allowlist, registration-failure rollback, async frontend guards                       | AUTOMATED VERIFIED |
| History           | Create/list, 50-row pages, 280-character previews, 1,000-row retention, copy/delete/clear/restart/Unicode/large content | AUTOMATED VERIFIED |
| Isolation         | History write failure cannot change completion; clear/sign-out preserve all other local tables/data                     | AUTOMATED VERIFIED |
| Authentication    | Signed-out, pending, success, memory credentials, restoration, expiry, invalid response, sign-out, network retry        | AUTOMATED VERIFIED |
| Analytics         | Opt-out, safe event metadata, duplicate completion suppression, delivery failure isolation                              | AUTOMATED VERIFIED |
| Diagnostics/crash | Safe-token allowlist and representative secret/content exclusion                                                        | AUTOMATED VERIFIED |
| Frontend          | Onboarding persistence/microphone recovery, History actions, settings, account readiness, UI regression                 | AUTOMATED VERIFIED |
| Security          | Core-only Tauri capability remains unchanged; no frontend shell/filesystem/network/clipboard permission                 | AUTOMATED VERIFIED |

Automated tests use SQLite temporary databases, auth/session/analytics test
doubles, and the existing native/provider mocks. They do not contact a live
account, analytics, crash, STT, or transformation provider and do not prove
physical macOS permission, Keychain, tray, focus, or insertion behavior.

## Milestone 7 manual macOS acceptance

No row in this section was performed during the implementation run. Record app
build identity, macOS version/hardware, clean-state method, account/provider
configuration, network condition, target application/version, and tester/date.

### Fresh onboarding and permissions

| Scenario                          | Expected result                                                                                        | Status                   |
| --------------------------------- | ------------------------------------------------------------------------------------------------------ | ------------------------ |
| Fresh application data            | Welcome appears once; SQLite starts at `first_run`                                                     | PENDING HUMAN VALIDATION |
| Quit on every onboarding step     | Relaunch resumes the exact step without data reset                                                     | PENDING HUMAN VALIDATION |
| Microphone not determined → grant | Native prompt, recheck, then progression                                                               | PENDING HUMAN VALIDATION |
| Microphone deny/revoke/restore    | Actionable state; no false progression; retry recovers                                                 | PENDING HUMAN VALIDATION |
| Accessibility deny/revoke/restore | Correct Settings pane, explicit recheck, retry recovers                                                | PENDING HUMAN VALIDATION |
| Account service unavailable       | Honest unconfigured/offline state; setup blocks only when no development/service STT credential exists | PENDING HUMAN VALIDATION |
| Shortcut instruction              | Current persisted dictation/rewrite chords shown accurately                                            | PENDING HUMAN VALIDATION |
| Fake/no dictation                 | First-dictation step does not advance                                                                  | PENDING HUMAN VALIDATION |
| Real TextEdit dictation           | Real STT/transform/insertion completes and advances to behavior                                        | PENDING HUMAN VALIDATION |
| Ready gate                        | Both permissions rechecked plus real dictation required                                                | PENDING HUMAN VALIDATION |
| Run onboarding again              | Re-entry is safe and leaves all local user data intact                                                 | PENDING HUMAN VALIDATION |

### Settings, History, and menu bar

| Scenario                                    | Expected result                                                          | Status                   |
| ------------------------------------------- | ------------------------------------------------------------------------ | ------------------------ |
| History/analytics/overlay toggles + restart | Values persist and take effect                                           | PENDING HUMAN VALIDATION |
| Working shortcut change + restart           | New pair registers, displays, and operates                               | PENDING HUMAN VALIDATION |
| Conflicting shortcut                        | Actionable error; prior pair remains registered/persisted                | PENDING HUMAN VALIDATION |
| Permission recovery from Settings           | Status/actions/recheck are accurate                                      | PENDING HUMAN VALIDATION |
| Close main window                           | Window hides; tray and both shortcuts continue                           | PENDING HUMAN VALIDATION |
| Tray Open/Settings/History                  | Correct surface opens and user-facing window activates                   | PENDING HUMAN VALIDATION |
| Tray status/context/shortcuts               | Values refresh when opened and reflect current pipeline/context/settings | PENDING HUMAN VALIDATION |
| Tray Pause/Resume                           | New operations stop/resume; current data remains intact                  | PENDING HUMAN VALIDATION |
| Tray Quit                                   | Audio/provider/rewrite tasks and shortcuts clean up                      | PENDING HUMAN VALIDATION |
| Completed/recoverable History rows          | Correct local metadata/result; no audio/selection source                 | PENDING HUMAN VALIDATION |
| History disabled                            | Future operations are not saved; existing rows remain                    | PENDING HUMAN VALIDATION |
| Copy result                                 | Exact result reaches clipboard through narrow Rust command               | PENDING HUMAN VALIDATION |
| Delete one                                  | Only selected row is removed                                             | PENDING HUMAN VALIDATION |
| Confirmed clear all                         | All History removed; projects/Voices/vocabulary unaffected               | PENDING HUMAN VALIDATION |
| Unicode/multiline/large result              | Preview/detail/copy remain correct and responsive                        | PENDING HUMAN VALIDATION |

### Account, reporting, offline, and regressions

| Scenario                                         | Expected result                                                            | Status                          |
| ------------------------------------------------ | -------------------------------------------------------------------------- | ------------------------------- |
| Configured browser sign-in/create                | HTTPS browser opens; pending/code/success states are accurate              | PENDING HUMAN VALIDATION        |
| Restart signed in                                | Keychain session restores without frontend token exposure                  | PENDING HUMAN VALIDATION        |
| Service-issued provider credentials              | Poll/restore tokens stay Rust-only and permit dictation without shell keys | PENDING HUMAN VALIDATION        |
| Incorrect/expired session                        | Explicit retry/sign-in state; no crash or local-data deletion              | PENDING HUMAN VALIDATION        |
| Offline startup/restoration                      | UI starts; cached/local features work; account says offline                | PENDING HUMAN VALIDATION        |
| Sign out                                         | Keychain session cleared; projects/settings/History preserved              | PENDING HUMAN VALIDATION        |
| Analytics opt-out                                | No optional capture requests                                               | PENDING HUMAN VALIDATION        |
| Analytics opt-in                                 | Only documented metadata/anonymous ID; no authored content/email/token     | PENDING HUMAN VALIDATION        |
| Reporting endpoints unavailable                  | Dictation/rewrite/onboarding continue with no user-facing reporting error  | PENDING HUMAN VALIDATION        |
| Approved test crash                              | Sanitized category/platform only; recovery on next launch                  | PENDING HUMAN VALIDATION        |
| 20 normal dictations                             | No History/reporting duplicate, insertion regression, or stuck overlay     | PENDING HUMAN VALIDATION — 0/20 |
| 20 selection rewrites                            | Exact safety invariants and one History result per operation               | PENDING HUMAN VALIDATION — 0/20 |
| Notes/TextEdit/Chrome/Safari/VS Code/Google Docs | Existing insertion/rewrite compatibility does not regress                  | PENDING HUMAN VALIDATION        |
| Startup/core latency                             | Compare p50/p95 and idle resources with Milestone 6 baseline               | PENDING HUMAN VALIDATION        |
