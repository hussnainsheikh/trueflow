# Architectural Decisions

## ADR-001: One modular Tauri application

- **Status:** Accepted
- **Date:** 2026-08-14

Trueflow starts as one Tauri 2 application with a React UI and a modular Rust
core. It does not split early modules into separate crates or services. Rust owns
future latency-sensitive and sensitive desktop behavior; React remains outside
the voice pipeline. Platform code has a dedicated `platform/macos` boundary.

This matches the MVP architecture while keeping module extraction possible once
real boundaries emerge.

## ADR-002: Rust-owned SQLite with embedded migrations

- **Status:** Accepted
- **Date:** 2026-08-14

Local persistence uses `rusqlite` with bundled SQLite. Ordered SQL migrations
are compiled into the Rust application and applied transactionally. The React
webview receives no filesystem or SQL capability.

Bundling SQLite provides a reproducible database version across supported Macs.
Owning migration execution in Rust preserves the architecture boundary and
avoids exposing local data access to frontend JavaScript.

## ADR-003: Structured logging uses `tracing`

- **Status:** Accepted
- **Date:** 2026-08-14

The Rust core emits JSON through `tracing` and `tracing-subscriber`, filtered by
`RUST_LOG`. Logs carry structured metadata and must not contain dictated text,
manuscript text, secrets, tokens, or clipboard contents.

This establishes one instrumentation model that can later carry operation IDs
and latency events without tying the core to a hosted telemetry vendor.

## ADR-004: Least-privilege Milestone 0 Tauri capability

- **Status:** Accepted
- **Date:** 2026-08-14

The only capability is scoped to the main window, macOS, and Tauri core defaults.
There are no shell, opener, filesystem, network plugin, microphone, global
shortcut, or Accessibility permissions. Future milestones must add narrowly
scoped permissions only when their behavior is implemented and tested.

## ADR-005: Rust-owned global shortcut lifecycle

- **Status:** Accepted
- **Date:** 2026-08-15

The official Tauri 2 global-shortcut plugin registers `Option + Space` in Rust
and delivers distinct pressed/released events directly to the Rust dictation
service. React never owns shortcut timing. Duplicate events are ignored by the
operation-scoped state machine, and registration is not exposed as a frontend
capability.

This preserves cross-application behavior and keeps the latency-sensitive
interaction outside the webview while using Tauri's supported lifecycle model.

## ADR-006: Native-rate CPAL capture with bounded mono buffering

- **Status:** Accepted
- **Date:** 2026-08-15

CPAL opens the current default input device for each recording. Supported native
sample formats are converted in the callback to mono `i16` while retaining the
device's native sample rate. Audio is held only in memory in a 30-second,
chunk-aware bounded buffer. When full, the oldest complete chunks are evicted and
dropped samples are counted. No permanent or temporary audio file is created.

Keeping native rate avoids premature resampling. Milestone 2 will frame and, only
when required, resample at the transcription-provider boundary. The bounded
chunk design can feed a streaming consumer without replacing capture.

## ADR-007: AVFoundation owns macOS microphone authorization

- **Status:** Accepted
- **Date:** 2026-08-15

The macOS platform adapter reads and requests microphone authorization through
AVFoundation using `objc2-av-foundation`. This preserves explicit
not-determined, granted, denied, restricted, and unavailable states and rechecks
authorization on every recording attempt. The small Objective-C FFI boundary is
isolated in `platform/macos.rs`, and each unsafe call has a documented safety
invariant.

The application bundle declares `NSMicrophoneUsageDescription`; the macOS audio
input entitlement is configured for signed builds. No provider credential or
microphone access is exposed to frontend JavaScript.

## ADR-008: State snapshots and scalar meter events cross the UI bridge

- **Status:** Accepted
- **Date:** 2026-08-15

Rust emits serializable dictation-state snapshots and operation-tagged scalar
audio levels to React. Audio-level publication is throttled to approximately
30 Hz by a joined worker, while the audio callback performs only normalization,
bounded-buffer insertion, atomic signaling, and a simple RMS calculation. Raw
audio never crosses the Tauri bridge.

The overlay is a separate, initially hidden, non-focusable Tauri window. Rust
controls its visibility from lifecycle state; React only validates event payloads
and renders them.

## ADR-009: Native auxiliary overlay across Spaces and full screen

- **Status:** Accepted
- **Date:** 2026-08-15

Tauri's `visibleOnAllWorkspaces` only supplies AppKit's `CanJoinAllSpaces`, which
does not by itself make an `NSWindow` eligible to appear alongside another
application's native full-screen window. During setup, the macOS adapter obtains
the overlay's native `NSWindow` from Tauri and adds `FullScreenAuxiliary`. On
macOS 15 and newer it also adds `CanJoinAllApplications`, which Apple documents
for floating windows and system overlays. Conflicting Space/full-screen flags
are removed while unrelated collection behavior is preserved.

After the collection-only change was tested, Chrome native full screen still
excluded the overlay on the minimum supported macOS version. Trueflow therefore
declares `LSUIElement = true`, establishing AppKit accessory/agent activation
policy when the bundle is opened normally. Setup also explicitly applies
accessory policy and then creates the overlay programmatically, instead of
declaring that window in `tauri.conf.json`. This preserves the required ordering
for development and direct-executable launches that bypass LaunchServices:
policy first, overlay window second. This removes the Dock/menu-bar presence
associated with a regular app, which is consistent with Trueflow's planned
menu-bar utility model; the existing main window can still be shown and used.

The overlay remains at `NSFloatingWindowLevel`, the standard level for floating
palettes and the same level requested by Tauri's `alwaysOnTop`. More invasive
status, menu, or screen-saver levels are unnecessary. Showing uses AppKit's
`orderFrontRegardless`, which orders the window above content even while
Trueflow is inactive without making the overlay key or main. The Tauri window is
still constructed with `focus: false` and `focusable: false`.

Immediately before ordering front, Rust uses `NSScreen.mainScreen` to select the
screen containing the currently focused application's window and places the
overlay at bottom center within that screen's visible frame. Screen data is not
cached. This keeps the overlay from remaining on the screen used at Tauri window
creation when focus or monitor context changes.

This native code remains isolated in `platform/macos.rs`. The direct `NSWindow`
pointer use is a narrow unsafe boundary: Tauri supplies a live native handle,
configuration occurs synchronously during main-thread setup, and visibility
ordering is dispatched to the AppKit main thread.

The programmatically created overlay retains its transparent webview through
Tauri's macOS private API feature. The MVP distribution plan is outside the Mac
App Store, where this feature would otherwise require reevaluation.

## ADR-010: Deepgram Nova-3 is the provisional first streaming provider

- **Status:** Accepted
- **Date:** 2026-08-15

Milestone 2 implements Deepgram's `/v1/listen` WebSocket API with Nova-3,
interim results, smart formatting, VAD events, and 300 ms endpointing. Each
dictation opens one provider session after the microphone format is known.
Shortcut release stops capture, drains queued audio in order, sends `Finalize`,
waits for the provider's `from_finalize` result, sends `CloseStream`, and waits
for the WebSocket to close.

Deepgram was selected as the provisional first adapter because its current API
supports true streaming, explicit push-to-talk finalization, raw mono linear16
at the device's declared sample rate, structured WebSocket failures, and Nova-3
keyterm prompting for the future Vocabulary milestone. No real-provider corpus
benchmark was available or executed, so this is not a benchmark winner or a
permanent lock-in. The privacy-safe 50-sample harness added in Milestone 2 must
be used to measure Deepgram and future candidates before a longer-term decision.

The desktop temporarily reads `TRUEFLOW_DEEPGRAM_API_KEY` from the Rust process
environment for local development only. It is never sent to React or logged.
The provider abstraction accepts configuration at construction so a later
backend-issued temporary credential can replace the development key without
changing dictation domain logic.

The rustls-backed WebSocket transport explicitly enables and installs rustls's
`ring` crypto provider during application setup. The transport dependency does
not select a crypto implementation itself, and relying on rustls auto-selection
caused the first live connection attempt to panic. Explicit process-level setup
makes the TLS backend deterministic and fails application initialization cleanly
if secure transport cannot be configured.

## ADR-011: Sample-bounded audio handoff and provider-neutral coordinator

- **Status:** Accepted
- **Date:** 2026-08-15

CPAL continues converting native input to mono `i16` without resampling.
Deepgram is told the actual native sample rate, avoiding an unnecessary format
conversion. Audio chunks use reference-counted sample storage so the existing
30-second recovery buffer and streaming handoff do not require a second sample
copy. The provider adapter performs only little-endian byte framing.

A sample-bounded 30-second queue separates the realtime audio callback from the
async network task. The callback uses non-blocking enqueue only. The coordinator
drains chunks in sequence and enforces explicit send, finalization, and close
timeouts. Queue overflow rejects new audio and fails the operation as
`STT_BACKPRESSURE_EXCEEDED`; it never evicts arbitrary unsent speech and claims a
valid transcript.

The coordinator owns one provider session per operation and normalizes provider
events before they reach dictation state. Interim text replaces the current
interim region, finalized segments accumulate separately, and operation finality
is explicit. It retries at most once, and only while establishing a session
before audio has been acknowledged. Mid-session disconnects are not replayed
because Deepgram does not expose a safe acknowledged-audio boundary; best-known
text is preserved as incomplete and the next operation starts with a fresh
session. Cancellation closes queues/tasks and operation IDs reject stale events.
The task boundary also catches an unexpected provider-worker panic and converts
it to `STT_RUNTIME_FAILED`, allowing dictation to stop the microphone and return
to a recoverable state instead of remaining indefinitely in `Finalizing`.

## ADR-012: Cursor-safe macOS insertion with an explicit fallback chain

- **Status:** Accepted
- **Date:** 2026-08-16

Dictation orchestration depends on a platform-neutral `TextInserter`; a separate
macOS backend owns ApplicationServices Accessibility, AppKit pasteboard, active
application discovery, and CoreGraphics paste dispatch. The target is resolved
at insertion time, not shortcut press time, and its process is revalidated
immediately before direct insertion or paste. Trueflow's own bundle is excluded.
Focused-element lookup first uses the system-wide Accessibility object. Some
Electron, Office, and custom editors return no element through that route, so the
backend retries `AXFocusedUIElement` through the foreground application's
PID-scoped Accessibility object and requests Electron's lazy tree through
`AXManualAccessibility`. If the app still exposes no focused element, it is
classified as an opaque/unknown target: direct Accessibility mutation remains
disabled, but one PID-revalidated paste is allowed. This is necessary for custom
canvas editors that accept ordinary paste while publishing no usable AX focus.
Secure fields that expose their role remain excluded; an app that exposes no AX
focus cannot be generically classified as secure, which is a documented native
API limitation of this compatibility fallback.

Direct insertion is attempted only when the focused element reports
`AXSelectedText` as settable. Setting that attribute delegates cursor, selection,
multiline, and UTF-16/Unicode behavior to the native control without reading or
reconstructing the full field value. Trueflow never sets `AXValue` to the raw
transcript.

A settable `AXSelectedText` that returns `kAXErrorSuccess` is not evidence that
text was inserted. Editors with their own text engines—Monaco's hidden textarea,
Sublime Text, and Office—publish a settable facade that accepts the write and
discards it. Live logs confirmed VS Code resolving as an editable `AXTextArea`
and completing as `InsertedDirectly` in 3 ms with no visible text. Direct
insertion therefore samples `AXSelectedTextRange` before and after the write and
falls back to paste when the write is proven discarded.

The verification is deliberately asymmetric. It requires positive proof of
failure—both reads succeeding and reporting an identical caret—because a false
negative would route an already-inserted transcript into the paste fallback and
duplicate the user's text. Targets that publish no caret position, and targets
whose caret moves at all, are treated as inserted. This preserves the existing
behavior for unverifiable controls rather than trading a silent no-op for a
silent duplication, consistent with data safety outranking feature breadth. Known non-editable elements fall back, while detectable secure text
fields reject both automatic insertion and clipboard copying; the transcript
remains visible in Trueflow.

Target editability is intentionally tri-state. Standard text roles or settable
`AXSelectedText` are editable; an explicit conservative set of controls such as
buttons, menus, images, and static text is non-editable; unrecognized and
container roles are unknown. Electron, web, Office, and custom native editors
often focus `AXGroup`, `AXWebArea`, `AXScrollArea`, or vendor-specific roles even
when their caret accepts normal paste. Unknown therefore remains eligible for
the bounded paste strategy rather than being misclassified as non-editable.
Secure fields remain excluded regardless of role.

The fallback chain is direct Accessibility insertion, then one temporary
pasteboard write plus one synthetic `Cmd+V`, then clipboard-only recovery. Paste
is attempted only while Accessibility is trusted and the target is not known to
be non-editable. The target process is rechecked before dispatch to reduce focus
races. After process-directed posting was observed to report dispatch without
reaching VS Code or Sublime Text's focused responder, the key events were moved
to the macOS HID event tap so the window server performs normal focused-window
routing. The foreground-process check still occurs immediately before dispatch.

Command travels as a flag on the V key pair. Synthetic Command `FlagsChanged`
events were tried first, on the belief that a Command flag on V alone was
dispatched but ignored by custom editor responders. That belief was unfounded:
the applications used to reach it were resolving as editable Accessibility
targets and completing through direct insertion, so `dispatch_paste` never
executed in them. Every conclusion drawn about paste transport before the
direct-insertion verification landed came from a code path that did not run.

Flagging the key pair alone was then tried, on the theory that synthetic
`FlagsChanged` events corrupt global modifier state across dispatches. Live
testing refuted it: Microsoft Word regressed from intermittent success to no
insertion at all, which establishes that Word requires the explicit modifier
events. The chord was restored and the requirement recorded at the call site so
it is not removed again on theoretical grounds.

Transport selection is no longer decided by inference. `examples/insertion_probe.rs`
deposits a labelled marker through each candidate transport into a focused
application, so the set a given application accepts is read directly rather than
deduced from whether a dictation appeared.
A dispatched paste is the strongest generic confirmation available for editors
that do not expose readable Accessibility text; application-specific behavior
therefore remains a manual validation requirement.

Synthetic paste additionally waits for physically held modifiers to clear. The
global shortcut reports release as soon as Space rises, so Option is routinely
still down when insertion runs a few hundred milliseconds later. A Command+V
chord posted into that state is resolved by the window server as
Command+Option+V, which is unbound in editors that perform their own key
handling. This is the common factor behind every reported paste failure: the
applications that worked all satisfied direct `AXSelectedText` insertion and
never reached this code path, so the paste strategy had no confirmed success in
any application. The backend now polls `CGEventSourceFlagsState` for a bounded
600 ms before dispatch and, if a modifier is still held at the deadline, posts
an honest chord that preserves the residual flags and logs
`paste_modifiers_held` rather than declaring an empty modifier set the user is
not actually in. Caps lock, fn, numeric-pad, and non-coalesced bits do not
change chord resolution and never delay dispatch.

Clipboard restoration exists to protect the user's clipboard, so it must first
establish that the clipboard is the user's. Trueflow writes exactly one
`public.utf8-plain-text` item, which is indistinguishable by shape from a
user-copied plain string, so the transcript left behind by one dictation was
captured as restorable user content by the next. That coupled consecutive
dictations: a run following a rich clipboard skipped restoration and left its
transcript available, while the run after it spent 1.2 seconds and then
overwrote its own transcript with the previous one, which Microsoft Word could
still be waiting to read. The backend now records the change count of each
transcript it writes and treats a pasteboard still holding that value as its
own, with nothing to restore. Restoration remains available for genuine
user-owned empty or plain-text clipboards.

Before paste, Trueflow captures only an empty or plain-text-only clipboard as
safely restorable. It waits 1.2 seconds after dispatch, then restores only if the
pasteboard change count is unchanged. This avoids immediately racing an
asynchronous paste—including editors that defer pasteboard reads—or overwriting
a clipboard change made by the user/target.
Rich or non-text pasteboards are not destructively approximated; dictated text
remains on the clipboard and `clipboard_restored` stays false. Restoration is
never claimed unless the native write or clear actually succeeded.

Accessibility is checked independently from microphone permission and is not a
prerequisite for recording or transcription. When it is denied, direct and
synthetic-paste strategies are skipped and the finalized transcript is copied
to the clipboard with an explicit recovery outcome. React receives normalized
permission/outcome metadata only and has no Accessibility, event-posting, or
clipboard capability.

## ADR-013: Synthetic typing precedes the pasteboard fallback

- **Status:** Accepted
- **Date:** 2026-08-21

Automatic insertion for targets without a settable `AXSelectedText` now attempts
synthetic keyboard input before the pasteboard. `CGEventKeyboardSetUnicodeString`
posts the transcript as typed text, using neither the general pasteboard nor a
Command+V chord.

The pasteboard strategy was pursued through five corrections and never became
reliable in Microsoft Word. The decisive evidence was two consecutive Word
dispatches whose structured logs were identical in every recorded field —
same resolved target, `held_modifiers: 0`, `flags: 1048576`, same pasteboard
change count and ownership — with one inserting text and the other not. The
variable is therefore outside anything Trueflow observes or controls, and no
further tuning of the chord can address it.

That is a property of the strategy rather than of Word. Paste depends on two
pieces of global mutable state that Trueflow neither owns nor can read back: the
window server's modifier state, and a pasteboard the target reads on its own
schedule. Trueflow cannot confirm the chord was routed to the focused responder,
cannot confirm the target read the pasteboard, and cannot restore the user's
clipboard without racing that read. Typing removes all of it: one event stream,
no shared state, and no clipboard to preserve or corrupt.

Typing is ordered ahead of paste and behind direct Accessibility insertion.
Direct insertion remains first because it is atomic and preserves selection
semantics natively. Paste is retained beneath typing because a target may
refuse synthetic typing while accepting a paste, and clipboard-only recovery
remains the final tier.

Transcripts are chunked at 20 UTF-16 units, never splitting a surrogate pair,
because some targets truncate a long single event. A chunk is allowed to exceed
the limit by exactly one unit when that is required to keep a pair together;
delivering half a surrogate would corrupt the user's text. The outcome is
reported as its own `InsertedByTyping` kind with the `SyntheticTyping` strategy
rather than being folded into an existing outcome, preserving the rule that
every insertion outcome states what actually happened.

## ADR-014: Completion is reported before provider session teardown

- **Status:** Accepted
- **Date:** 2026-08-21

The transcription coordinator reports `completed` as soon as the provider
delivers its operation-final transcript, then performs the WebSocket close
handshake without blocking the observer.

Measured on a live Word dictation, the close handshake cost 738 ms of the
2,139 ms between shortcut release and completed insertion — roughly a third of
post-release latency, spent holding a finished transcript while waiting for a
socket to wind down. The transcript is provider-final before the close begins,
so the wait contributed nothing to correctness.

A close that subsequently fails or times out no longer downgrades the operation.
It is logged as `close_after_completion_failed` and the transcript stands,
because the socket's disposition cannot retroactively make a delivered
provider-final transcript incomplete. Events arriving during teardown are
drained without republishing, since they cannot change an operation-final
transcript that has already been reported.

## ADR-015: Project context is an immutable per-dictation snapshot

- **Status:** Accepted
- **Date:** 2026-08-29

Rust keeps the current active project and its complete vocabulary in an
immutable in-memory snapshot. A dictation captures one `Arc` to that snapshot
when the shortcut is pressed. Project selection and vocabulary mutations are
serialized with snapshot replacement, so a UI change affects the next press but
cannot change recognition hints or normalization midway through an operation.

SQLite remains authoritative. The cache exists only to make the hot-path read
bounded and database-free; successful writes refresh it transactionally. There
is no external cache and React neither chooses nor copies dictation context.

## ADR-016: Vocabulary hinting is advisory; normalization is authoritative

- **Status:** Accepted
- **Date:** 2026-08-29

The transcription-provider interface declares vocabulary capabilities and
receives a deterministic, bounded list containing canonical terms first,
followed by aliases and pronunciation hints. Deepgram Nova-3 maps this list to
repeated unweighted `keyterm` query values within a conservative local budget.
Providers without hint support continue normally.

After final or recoverable STT output, Rust applies one project-scoped
normalization pass against the original transcript. Explicit forbidden variants
outrank aliases and canonical capitalization; longer equal-strength matches win;
ties use stable keys. Matching is Unicode-safe, boundary-aware, supports
reasonable multi-word whitespace variation, and does not recursively match
replacement output. Pronunciation hints are never literal replacement rules.

## ADR-017: Voice and Mode are independent persisted transformation inputs

- **Status:** Accepted
- **Date:** 2026-08-29

SQLite schema version 3 stores Voices separately from Modes. Built-in Voices
are global and deterministically seeded; user Voices are project-scoped so
character or manuscript style cannot leak into another project. Each project
stores one selected Voice and one selected Mode. Modes remain global built-ins
for the MVP because the plan defers custom Modes; `Rewrite Selection` is seeded
but cannot be selected by normal dictation until Milestone 6 supplies selected
text semantics.

Voice examples are ordered child rows rather than encoded JSON. This preserves
deterministic prompt order and allows future example metadata without replacing
the Voice record format. React uses narrow typed commands and never reads the
database or duplicates selection rules.

The active project cache now includes its selected Voice and Mode. The entire
project/vocabulary/Voice/Mode snapshot is captured on shortcut press, so edits
during speech affect only a later operation.

## ADR-018: Transformation is provider-neutral and fails open to normalized text

- **Status:** Accepted
- **Date:** 2026-08-29

Rust owns one deterministic prompt builder and a provider-neutral asynchronous
transformation trait. The first adapter uses OpenAI's Responses API from the
Rust configuration boundary with `store: false`; React receives neither the key
nor the prompt. Development may use a local environment key, while later
authentication can replace credential delivery without changing the domain or
dictation coordinator.

Transformation runs after vocabulary normalization and before the existing
insertion pipeline. It has a 12-second outer timeout, operation cancellation,
and at most one retry for rate limits or temporary provider availability. The
operation ID is checked again when the provider returns, preventing a late
response from mutating or inserting into another session.

Transformation is an enhancement, never a gate. Every provider/configuration,
timeout, cancellation, or response-validation failure retains the normalized
transcript, sends that text through normal insertion, records fallback telemetry,
and leaves a recoverable warning. This deliberately prefers reliable speech
preservation over stylistic output.

## ADR-019: Selection rewrite is a distinct fail-closed operation

- **Status:** Accepted
- **Date:** 2026-08-30

Selection rewrite uses its own `Option + Shift + Space` hold shortcut and an
explicit `SelectionRewrite` operation kind. It shares microphone capture, STT,
the provider-neutral transformation boundary, project snapshots, and the base
dictation state machine, but carries rewrite-specific stages and outcomes. It
does not overload `Option + Space`, and the two shortcut releases cannot finish
one another's operation.

At shortcut press, the macOS backend captures one non-empty contiguous selection
from a non-secure editable Accessibility element. The immutable snapshot holds
the foreground PID and bundle ID, a Core Foundation element fingerprint plus an
Accessibility identifier when exposed, role/subrole, selected text, and the
selection's UTF-16 range. Rust also captures the Project, Vocabulary, and Voice
at that boundary. The selected source is never emitted to React or logs.

The rewrite prompt is a dedicated task: the spoken transcript is the instruction
and the selected passage remains delimited untrusted source input. Project,
Vocabulary, and Voice apply, while the project's normal dictation Mode does not.
Unlike normal dictation's reliability-oriented transformation fallback, rewrite
fails closed: missing instruction, STT/provider failure, timeout, cancellation,
or invalid output never inserts the instruction or normalized transcript.

Immediately before document mutation, macOS re-resolves and compares the same
foreground process, focused element fingerprint/identifier, role, UTF-16 range,
and selected text. A mismatch aborts without searching for similar text.
Direct `AXSelectedText` replacement is accepted only when the post-write caret
proves the replacement; a proven no-op may use exactly one paste fallback after
a second full revalidation. Paste is disabled when the existing clipboard
cannot be represented and restored by the current plain-text preservation
mechanism. Synthetic typing is intentionally excluded because chunked typing
would destroy the selected range before the complete replacement could be
verified and could create partial or duplicate mutations.

## ADR-020: Product state and History remain Rust-owned SQLite data

- **Status:** Accepted
- **Date:** 2026-08-30

Schema version 4 adds one singleton product-settings row, local account
metadata, and append-only dictation History rows. Onboarding state and step,
History/analytics preferences, overlay visibility, shortcut labels, pause state,
and an anonymous installation identifier survive restart without webview
storage. History stores text-only final/useful recoverable results and context
labels; it excludes audio, selected source text, prompts, clipboard contents,
and credentials.

Terminal operation observation is deduplicated by operation ID and dispatched
after the Rust pipeline publishes its core outcome. History and analytics run as
best-effort background work, so database/reporting failure cannot retry,
duplicate, downgrade, or delay insertion/replacement.

## ADR-021: First-run readiness requires Rust-observed product proof

- **Status:** Accepted
- **Date:** 2026-08-30

Onboarding is a durable state machine (`first_run`, `in_progress`, `ready`) with
resumable named steps. React presents the flow, but Rust alone advances the
first-dictation gate after a real completed normal-dictation operation. The
ready transition additionally rechecks microphone and Accessibility state.
Re-running onboarding changes only its markers and never resets local data.

## ADR-022: Browser account contract with macOS Keychain session storage

- **Status:** Accepted
- **Date:** 2026-08-30

No authentication provider or backend was selected in prior decisions. Rather
than embed a second frontend auth stack, the desktop depends on a narrow
provider-neutral first-party account contract under `TRUEFLOW_AUTH_BASE_URL`:
bounded `/desktop-auth/start`, `/desktop-auth/poll`, and
`/desktop-auth/session` operations. The eventual managed provider remains behind
that server boundary. An unconfigured build reports account service unavailable
and never fabricates authentication.

Browser verification URLs and user codes may reach React; pending request IDs
stay in Rust memory and session tokens are stored only in macOS Keychain through
`security-framework`. Restoration is bounded and explicit about offline/expired
state. Sign-out clears Keychain/session metadata without deleting local data.
Authentication is never consulted from the audio callback or per-chunk pipeline.

## ADR-023: Opt-in analytics and privacy-filtered crash intake

- **Status:** Accepted
- **Date:** 2026-08-30

No hosted analytics or crash vendor was previously selected. Milestone 7 uses
small Rust-owned HTTP boundaries rather than desktop SDKs with automatic
breadcrumbs. Optional analytics configuration uses a PostHog-compatible capture
shape and anonymous local installation ID; delivery is disabled until explicit
local opt-in. Event properties are allowlisted metadata and never contain
authored text or email.

Optional crash intake receives only fixed categories and platform metadata.
Rust panic and React error boundaries omit panic messages, component stacks,
breadcrumbs, content, request bodies, and secrets. Both paths have three-second
timeouts, are asynchronous, ignore delivery failure, and cannot affect product
operations.

## ADR-024: Native menu lifecycle and transactional shortcut changes

- **Status:** Accepted
- **Date:** 2026-08-30

Trueflow uses Tauri 2's Rust tray/menu API and existing accessory policy. Closing
the main window hides it; the process, menu, and global shortcuts continue until
explicit Quit. Menu actions open and focus Home, Settings, or History without
changing the overlay's non-focusable policy.

Settings offers a conservative shortcut allowlist. Rust registers the complete
new pair before persistence. Conflicts unregister a partial pair and restore the
previous working pair. React has no global-shortcut capability and cannot
register arbitrary chords.

## ADR-025: Account-issued provider credentials and bounded History retention

- **Status:** Accepted
- **Date:** 2026-08-30

Production onboarding must not claim dictation readiness merely because an
account session exists. The first-party account poll/session response may issue
`transcriptionToken` and `transformationToken`. They are held only in a shared
Rust memory credential store, refreshed through bounded account restoration,
and cleared on sign-out. Provider adapters read the current token when starting
a request/session; React and SQLite never receive it. Explicit environment keys
remain a development fallback. Account transport requires HTTPS except for
localhost debug services.

History retains the newest 1,000 entries. Insertion and retention cleanup share
one SQLite transaction, while any History failure remains outside the core
operation result. List DTOs carry no more than 280 Unicode characters of result
preview; the complete result is loaded only by the narrow copy command. This
keeps React payloads bounded and gives local retention an explicit, predictable
policy.
