# Trueflow

Trueflow is a macOS-first, system-wide AI voice writing application. The current
repository state is Milestone 7 code-complete pending physical macOS/provider validation:
put the cursor in another application, hold `Option + Space`, speak, and release
to transcribe with Deepgram, normalize project vocabulary, transform with the
selected Voice and Mode, and insert the result. Or select one contiguous passage,
hold `Option + Shift + Space`, speak a rewrite instruction, and release to replace
that exact selection after safety revalidation. Milestone 7 adds durable
first-run onboarding, Settings, local History, menu-bar operation, a
browser-account boundary, privacy-safe diagnostics, opt-in product analytics,
and crash reporting. Billing and release infrastructure are not implemented.

## Prerequisites

- macOS 12 or newer
- Xcode Command Line Tools (`xcode-select --install`)
- current stable Rust with `rustfmt` and `clippy`
- Node.js 22.12 or newer
- pnpm 10.34.5 (the exact package-manager version is declared in
  `package.json`)

Enable Corepack and install the declared pnpm version if needed:

```bash
corepack enable
corepack install
```

If the Corepack bundled with an older Node installation reports a package-signing
key error, update Corepack first with `npm install --global corepack@latest`, then
repeat the two commands above.

## Fresh-clone setup

```bash
git clone <repository-url> trueflow
cd trueflow
pnpm install --frozen-lockfile
pnpm dev
```

`pnpm dev` is the single command for the complete Tauri development
environment. It starts Vite through Tauri and opens the desktop app.

On a fresh profile, Trueflow resumes a short SQLite-backed onboarding flow:
welcome, microphone, Accessibility, account/credential readiness, shortcuts, one real
external-app dictation, and a Clean Dictation/Draft choice. Setup is not marked
ready until both permissions are rechecked and the Rust pipeline reports a
successful real dictation.

The app and automated test suite start without credentials. Local development
may use a Deepgram API key and OpenAI API key directly. Copy `.env.example` to `.env`, set
`TRUEFLOW_DEEPGRAM_API_KEY` and `TRUEFLOW_OPENAI_API_KEY`, export the Rust-side
values in your shell, and never commit `.env`. `TRUEFLOW_OPENAI_MODEL` may
override the documented default.

Optional deployment integrations are configured only in Rust:

- `TRUEFLOW_AUTH_BASE_URL` enables the bounded browser/device account contract
  at `/desktop-auth/start`, `/desktop-auth/poll`, and `/desktop-auth/session`.
  Session tokens are stored in macOS Keychain. Successful poll/session responses
  may include `transcriptionToken` and `transformationToken`; these remain only
  in Rust memory and override development credentials. Production onboarding
  cannot proceed to first dictation until a transcription credential exists.
- `TRUEFLOW_ANALYTICS_ENDPOINT` and `TRUEFLOW_ANALYTICS_KEY` enable a
  PostHog-compatible capture endpoint. Delivery remains off until the user opts
  in under Settings.
- `TRUEFLOW_CRASH_REPORT_ENDPOINT` enables a privacy-filtered crash intake
  gateway. Only allowlisted category/platform metadata is submitted.

An unconfigured deployment reports these integrations as unavailable instead
of fabricating success. Local projects, settings, History, and application
startup remain available offline.

```bash
set -a
source .env
set +a
pnpm dev
```

Supported application environments are `development`, `test`, `staging`, and
`production`. Release builds default to `production`; debug builds default to
`development`. Development keys are read only by Rust and never sent to the
React webview. Local third-party keys are temporary development configuration;
a later authenticated backend can replace them with short-lived credentials
without changing either provider-neutral domain.

## Microphone and Accessibility permissions

Start the app, then hold `Option + Space` while any normal macOS application has
focus. On first use, grant Trueflow microphone access. Speak while holding the
shortcut to see interim text, then release to stop the microphone and request an
explicit final transcript. The overlay is configured not to accept focus. After
finalization, Rust resolves the currently focused application and element and
attempts insertion at its native cursor or selection.

Selection rewrite uses a separate shortcut and never changes normal dictation:
select one non-empty contiguous passage in an editable, non-secure field, hold
`Option + Shift + Space`, speak an instruction such as “make this more concise,”
then release. Rust captures the application, focused Accessibility element,
UTF-16 selection range, selected text, Project, Vocabulary, and Voice before the
microphone starts. It revalidates all target identity fields immediately before
replacement. If the application, element, range, or source text changes, or any
provider step fails, the original passage is left untouched. Selected source
text remains Rust-side and is never sent to React or diagnostic logs.

If access was denied or later revoked, open **System Settings → Privacy &
Security → Microphone**, enable Trueflow, and try the shortcut again. Permission
is rechecked on each attempt. Development and production app identities can
appear as separate entries in macOS privacy settings.

Automatic insertion also requires **System Settings → Privacy & Security →
Accessibility**. Open the Trueflow main window and choose **Open Accessibility
settings**, enable the currently running Trueflow development executable or app,
return to Trueflow, and retry. Accessibility is rechecked while the app is
running. Microphone and Accessibility are independent: recording,
transcription, and final transcript display continue if Accessibility is denied.

When direct cursor-aware Accessibility insertion is unsupported, Trueflow writes
the finalized transcript to the general pasteboard and dispatches normal `⌘V`
without activating Trueflow. It waits 1.2 seconds before restoring an earlier
plain-text or empty clipboard and restores only if the pasteboard change count
has not changed. Rich or non-text clipboard contents are not reconstructed; in
that case dictated text remains on the clipboard. If automatic insertion cannot
run, the result is explicitly reported as **Copied to clipboard**, not as
inserted. If clipboard writing also fails, the transcript remains visible in
Trueflow rather than being discarded.

Trueflow does not automatically insert or copy dictated text while a detectable
secure/password field is focused. It also excludes its own overlay/application
as an insertion target. No target-field text, clipboard contents, or transcript
content is written to structured diagnostic logs.

Trueflow retains at most 30 seconds of normalized mono `i16` audio in memory and
uses a separate sample-bounded queue to stream small chunks directly from Rust
to Deepgram. Audio is not written to disk and never passes through React. When
the network queue cannot preserve ordering within its bound, the operation fails
visibly instead of silently dropping speech and claiming success.

If the key is missing or rejected, the overlay shows a recoverable configuration
or authentication error. If provider finalization fails after partial text was
received, Trueflow normalizes and inserts the recovered text when possible while
retaining a recoverable warning rather than claiming it is provider-final.

## Projects and vocabulary

The main window always has one active, non-archived project; a fresh database
starts with **General**. Create, rename, select, archive, unarchive, and delete
projects from **Projects & vocabulary**. Deleting a project also deletes its
vocabulary, and Trueflow prevents removal of the last usable project.

Vocabulary entries contain a canonical term, optional pronunciation hint, type,
description, aliases, explicit forbidden variants, and a case-sensitivity rule.
Canonical terms, aliases, and pronunciation hints are sent as bounded advisory
recognition hints when the configured provider supports them. Final and
recoverable transcripts are then normalized deterministically in Rust before
the existing insertion pipeline runs. Forbidden variants are strongest,
aliases are next, canonical capitalization is weakest, and overlapping rules
use longer-match and stable tie-breaking. Matching is boundary-aware and does
not rewrite substrings inside unrelated words.

Project context is captured when the shortcut is pressed. Changing the active
project while speaking intentionally affects only the next dictation. Provider
hints may improve recognition but are not authoritative; deterministic local
normalization is the correctness layer.

## Voices, Modes, and transformation

The main window's **Voices & Modes** section lists the built-in **My Voice** and
**Narrator** Voices and all five MVP Modes. Create and edit project-specific
Voices there, and select one Voice and Mode per project. **Rewrite Selection** is
persisted as a built-in task marker but remains unavailable as a normal dictation
Mode; use the dedicated `Option + Shift + Space` workflow instead.

Rust captures project, vocabulary, Voice, and Mode in one immutable snapshot at
shortcut press. After STT and deterministic normalization, a provider-neutral
prompt builder composes that snapshot and a Rust-only OpenAI Responses adapter
performs one bounded transformation. The request has a 12-second timeout, at
most one retry for transient availability/rate-limit failures, operation-scoped
cancellation, and `store: false`. Prompt and transcript content are not logged.

If transformation is unconfigured, rejected, timed out, disconnected, or
returns no usable text, Trueflow keeps and inserts the normalized transcript
through the same system-wide insertion chain. The UI reports a recoverable
warning instead of losing speech or pretending transformation succeeded.

Selection rewrite deliberately fails closed rather than using that normal
dictation fallback. The spoken transcript is an instruction, never replacement
prose. A rewrite transformation failure, timeout, cancellation, or unusable
response leaves the original selection unchanged and does not insert the spoken
instruction. The rewrite prompt uses the captured Project, Vocabulary, and Voice
with dedicated rewrite task rules; it does not leak the project's unrelated
normal dictation Mode into the request.

## Product shell, History, and account

The main window provides Home, Projects, Voices & Modes, History, Settings, and
Account sections. Closing it hides the window; Trueflow stays in the menu bar
with global shortcuts active. The menu opens Home, Settings, or History, pauses
or resumes new shortcut operations, and quits explicitly. Opening a user-facing
window may take focus; the dictation overlay remains a separate non-focusable
window. Opening the menu refreshes its Ready/Listening/Finalizing/Transforming/
Inserting/permission/account status, active Project/Voice/Mode, and both
shortcut reminders.

Settings are stored in Rust-owned SQLite, including onboarding progress,
History/analytics preferences, overlay visibility, shortcut choices, and pause
state. Shortcut changes are registered with macOS before persistence; conflicts
roll back to the last working pair. Settings → Shortcuts & audio offers a
microphone picker. The default "System default" follows macOS. A pinned device
is remembered by its stable CoreAudio identifier and applies from the next
dictation; if it is disconnected, dictation records from the system default and
Settings says so. Denied microphone
and Accessibility states provide direct System Settings actions and explicit
rechecks.

History is local, optional, and text-only. It retains the newest 1,000 entries
and transactionally removes older entries after a successful write. List pages
contain at most 50 rows and 280 Unicode characters per preview; copy retrieves
the complete entry only when requested. History stores completed or useful
recoverable operation results with timestamp, operation
kind, project/Voice/Mode labels, target app label, status, and safe error code.
It never stores audio, clipboard contents, selected source text, prompts,
provider keys, or account tokens. Delete-one and confirmed clear-all are
immediate. Disabling History stops future writes without deleting existing rows.

Configured account sign-in starts a browser/device flow through the first-party
account-service contract. Pending request identifiers stay in Rust memory; the
returned session token is stored in macOS Keychain and validated through a
bounded restoration request during startup without blocking local UI. Provider
tokens returned by the account service stay only in Rust memory. An expired or
offline session is explicit. Signing
out clears secure session/account metadata but preserves all local projects,
Voices, vocabulary, settings, and History. No billing UI is present.

Product analytics is opt-in and sends only allowlisted event metadata with an
anonymous installation identifier. Crash/error reporting sends only sanitized
categories and platform context. Neither path receives user-authored content,
raw provider payloads, email addresses, tokens, or secrets. Both are
non-blocking and cannot change a dictation/rewrite outcome.

## STT evaluation harness

The privacy-safe 50-sample recording plan and raw PCM benchmark instructions are
in `benchmarks/stt/`. Raw recordings are gitignored. The harness streams a local
PCM16 sample in real time and reports connection, first-partial, and
release-to-final timing:

```bash
cargo run --manifest-path src-tauri/Cargo.toml --example stt_benchmark -- \
  --sample-id short-01 \
  --path benchmarks/stt/audio/short-01.pcm \
  --sample-rate 16000 \
  --expected "Hello, this is a test of TrueFlow."
```

The corpus has not been recorded or executed in this workspace; it is evaluation
infrastructure, not a latency or accuracy claim.

## Insertion transport probe

When an application refuses automatic insertion, measure which synthetic
transport it accepts instead of inferring it from whether dictation appeared:

```bash
cargo run --manifest-path src-tauri/Cargo.toml --example insertion_probe
```

Focus the target application and place the cursor in a document during the
countdown, then leave the keyboard alone. The probe deposits one uniquely
labelled marker per candidate transport — flag-only and `FlagsChanged` chords
across the HID, Session, and AnnotatedSession taps, a private event source, and
clipboard-free synthetic typing. Whichever markers appear identify the
transports that application accepts.

To test whether an application accepts _repeated_ synthetic input, which is the
question when insertion succeeds only on some attempts:

```bash
cargo run --manifest-path src-tauri/Cargo.toml --example insertion_probe -- --repeat
```

This performs the same synthetic typing Trueflow performs, six times, with none
of Trueflow's overlay, state machine, or window ordering around it. If every
marker lands, the application accepts repeated synthetic input and the fault is
in Trueflow's surrounding behaviour. If the markers alternate, the application
itself rejects alternate synthetic events and no insertion strategy will fix it.

The probe posts synthetic events, so the process running it must be trusted for
Accessibility. Launched from a terminal, that trust belongs to the terminal
application rather than to Trueflow; the probe reports this before doing
anything else.

## Manual Milestone 3 run

1. Export `TRUEFLOW_DEEPGRAM_API_KEY` as shown above and run `pnpm dev`.
2. Grant Microphone and Accessibility permission to that Trueflow identity.
3. Put the cursor in Notes or TextEdit; do not activate the Trueflow overlay.
4. Hold `Option + Space`, speak, and release.
5. Confirm the overlay reaches **Inserting** and then reports **Text inserted**,
   **text copied to clipboard**, or a specific recoverable error.
6. Confirm existing text and native selection behavior are preserved and the
   target application remains focused.

Use the full per-application checklist in `docs/TEST_MATRIX.md` before treating
Milestone 3 as fully validated. Development and production bundles can require
separate Accessibility grants because macOS tracks app identity/path/signature.

## Manual Milestone 4 run

1. Start Trueflow with a live Deepgram key and create two projects in the main
   window.
2. In Project A, add canonical `Aeryndor`, pronunciation `air-in-door`, alias
   `Erindor`, and forbidden variant `Erin Door`; leave Project B without it.
3. Select Project A, focus a known-working target such as Notes or TextEdit,
   dictate “erin door,” and confirm `Aeryndor` inserts.
4. Select Project B and repeat. Confirm Project A's correction does not leak.
5. Restart Trueflow and confirm both projects, vocabulary, and active selection
   persist.
6. During one longer dictation, switch projects in the main window. Confirm that
   operation keeps its original project and the next dictation uses the new one.

Record the full project, vocabulary, provider, insertion, persistence, and
performance results in `docs/TEST_MATRIX.md` before marking Milestone 4 complete.

## Manual Milestone 5 run

1. Export both provider keys and run `pnpm dev`.
2. Test **Narrator + Draft**, a manually created **Kael + Dialogue**, and **My
   Voice + Clean Dictation**; confirm visibly different appropriate output.
3. Restart Trueflow and confirm Voice content and project selections persist
   without duplicate built-ins.
4. Remove or invalidate `TRUEFLOW_OPENAI_API_KEY`, dictate again, and confirm the
   normalized transcript still inserts with a recoverable warning.
5. Exercise network interruption, a deliberately slow response, rapid
   consecutive dictations, and cancellation; confirm no stale result inserts.
6. Repeat focus, clipboard, full-screen, and multi-app insertion checks from
   `docs/TEST_MATRIX.md` while recording content-free transformation latency.

## Manual Milestone 6 run

1. Export both provider keys, run `pnpm dev`, and grant the running identity
   Microphone and Accessibility access.
2. In TextEdit or Notes, select one passage with surrounding text. Hold
   `Option + Shift + Space`, say “make this more concise,” release, and confirm
   only the original selection is replaced while focus and surrounding text stay
   intact.
3. Repeat with Unicode/emoji and a multiline selection, then test Chrome
   textarea/contenteditable, Safari, VS Code, and Google Docs. Record the actual
   direct or paste replacement strategy from structured logs.
4. During separate attempts, switch applications, move the selection, edit the
   selected text, or remove the selection while transformation is running.
   Confirm every attempt aborts without changing the document or clipboard.
5. Test no selection, read-only content, a secure/password field, an unsupported
   control, empty spoken instruction, invalid provider credentials, timeout,
   cancellation, and network failure. Confirm the original text is always
   preserved and the spoken instruction is never inserted.
6. Run 20 consecutive successful rewrites and check for duplicate replacements,
   stale output, focus loss, clipboard corruption, or a stuck overlay.

Record every result in `docs/TEST_MATRIX.md`; automated mocks do not establish
physical Accessibility compatibility or live provider behavior.

## Manual Milestone 7 run

Use a fresh macOS user/profile or move the existing app database aside only if
you intentionally want a clean onboarding test. Launch the built `.app`, follow
Welcome → Microphone → Accessibility → Account/skip → Shortcuts, then focus
TextEdit and complete a real live-provider dictation. Confirm onboarding advances
only after the text actually inserts and both permissions are granted. Choose a
default behavior, close the main window, and verify the menu-bar utility and
global shortcuts remain active.

Then reopen Settings and History from the menu bar. Revoke and restore each
permission, change both shortcuts (including a conflict), toggle overlay,
History, and analytics preferences, restart, and verify persistence. Exercise
copy/delete-one/clear-all History actions. Where deployment endpoints are
configured, test browser sign-in, restart restoration, offline/expired session,
retry, and sign-out while verifying local data remains. Use the full checklist
in `docs/TEST_MATRIX.md`; none of these physical tests is implied by automated
coverage.

## Validation

```bash
pnpm format:check
pnpm lint
pnpm typecheck
pnpm test
pnpm build
pnpm rust:fmt
pnpm rust:clippy
pnpm rust:test
pnpm tauri build
```

To apply formatting, run `pnpm format`. The desktop build is written under
`src-tauri/target/release/bundle/macos/`.

## Architecture

- React, TypeScript, and Vite own non-critical desktop UI only.
- A Rust productization service owns onboarding/settings state, History,
  anonymous analytics identity, diagnostics, and account/session boundaries.
- Account session tokens use macOS Keychain and never cross the Tauri bridge.
- Menu-bar lifecycle and shortcut re-registration stay Rust-side; closing the
  main window hides it without terminating background dictation.
- Tauri commands form the typed UI-to-core boundary.
- Rust owns the global shortcut, dictation state machine, microphone permission,
  capture, bounded audio buffer/stream queue, provider connection, transcript
  assembly, normalization, immutable writing-context snapshots, deterministic
  normal and rewrite prompt construction, AI transformation/fallback, immutable
  native selection snapshots and revalidation, target discovery, Accessibility
  permission, direct cursor/selection insertion, guarded rewrite replacement,
  paste/clipboard recovery, lifecycle cleanup, and telemetry.
- React receives state snapshots, scalar audio levels, and transcript/status
  strings only; raw audio, provider credentials, Accessibility handles, and
  clipboard operations remain in Rust.
- SQLite migrations in `src-tauri/migrations/` are append-only and are applied
  transactionally at startup.
- macOS-specific integrations live under `src-tauri/src/platform/`.

See [docs/MVP_PLAN.md](docs/MVP_PLAN.md) for the product source of truth and
[docs/PROGRESS.md](docs/PROGRESS.md) for the current milestone state.

## Privacy and secrets

Do not put provider keys, authentication tokens, signing credentials, dictated
text, manuscripts, or clipboard content in frontend code, environment examples,
logs, tests, or commits. Structured logs should contain metadata such as
subsystem, event, operation ID, duration, and safe error code only.
