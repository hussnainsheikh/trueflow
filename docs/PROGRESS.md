# Progress

## Current milestone

**Milestone 7 — Productization: CODE COMPLETE — PENDING HUMAN VALIDATION**

Milestone 7 is implemented and passes the automated repository checks. It is
not marked fully complete because the fresh-install onboarding, physical macOS
permission recovery, menu-bar lifecycle, live account service, configured
analytics/crash endpoints, and real-app regression matrix require human testing.

## Completed items

### Milestone 7 gap closure (2026-10-12)

- Added microphone selection (MVP plan §11/§29). Migration 5 adds a nullable
  device preference; `NULL` keeps following the macOS system default. Rust
  enumerates inputs on a blocking worker, saves only devices that are currently
  connected, and reads the preference from memory at each recording start. A
  disconnected pinned device falls back to the system default, so speech is
  still captured, and Settings shows it as disconnected. See ADR-026.
- Renamed `docs/mvp_plan.md` to `docs/MVP_PLAN.md` to match every reference in
  `AGENTS.md`, `README.md`, and this file. Case-sensitive checkouts such as Linux
  CI previously could not resolve those links.

### Milestone 7 initial implementation

- Added durable first-run onboarding with explicit Welcome, Microphone,
  Accessibility, account/credential readiness, Shortcuts, first real Dictation, and Behavior
  steps. The first-dictation gate advances only after a genuine completed normal
  dictation; there is no simulated success path.
- Added a product shell with Home, History, Settings, Permissions, Diagnostics,
  and Account surfaces. Settings persist locally and cover supported dictation
  and rewrite shortcuts, history, privacy-safe analytics, overlay visibility,
  pause/resume, and onboarding restart.
- Added SQLite migration 4 for singleton product settings, bounded local
  dictation/rewrite history, and non-secret account metadata. History supports
  paging, detail retrieval, copy, per-entry deletion, clear-all, restart
  persistence, operation-ID deduplication, Unicode, and large entries.
  Retention is capped transactionally at the newest 1,000 entries; list DTOs
  contain 280-character previews while copy loads the full result on demand.
- Added a cached Rust product-settings snapshot so the voice/rewrite pipeline
  does not perform synchronous database reads. Terminal persistence and
  analytics run after core operation/UI completion and cannot change the outcome.
- Added settings-owned, allowlisted global shortcuts with conflict rollback and
  pause behavior. Unsupported shortcut strings fail closed.
- Added menu-bar status/context plus Open, Settings, History, Pause/Resume, and
  Quit actions. Closing the main window hides it while the menu-bar process and
  shortcuts remain active. Opening the menu refreshes pipeline/permission/account
  status, active Project/Voice/Mode, and both shortcut reminders.
- Added browser/device authentication abstractions and a concrete first-party
  HTTP contract for start, poll, and session refresh. Session secrets use the
  macOS Keychain; SQLite and React receive only non-secret account state.
  Pending, authenticated, signed-out, expired, and offline/retryable states are
  represented explicitly, and sign-out preserves local projects and history.
  Account poll/restoration may deliver provider credentials into Rust memory;
  they override development keys, never enter React/SQLite, and clear on sign-out.
- Added opt-in, PostHog-compatible metadata-only analytics with anonymous local
  installation ID, operation deduplication, timeout/error isolation, and no text,
  audio, prompts, selections, clipboard data, or credentials.
- Added category-only crash and frontend error intake hooks. Panic text, stacks,
  breadcrumbs, user content, and credentials are excluded.
- Added permission recheck/open-settings recovery, safe diagnostics, React error
  recovery, strict runtime validation for frontend product DTOs, and focused
  automated coverage for the new product behaviors.
- Documented setup, external integration contracts, product behavior,
  architectural decisions, and the physical macOS/manual validation matrix.

## Validation results

Re-validated automatically on 2026-10-12 after the gap closure:

- `cargo fmt … --check`, `cargo clippy … -D warnings`: VERIFIED
- `cargo test --all-features`: VERIFIED — 163 Rust unit tests and 2 streaming
  integration tests
- `pnpm format:check`, `pnpm lint`, `pnpm typecheck`: VERIFIED
- `pnpm test`: VERIFIED — 5 files and 26 frontend tests
- `pnpm build`: VERIFIED
- `pnpm tauri build`: VERIFIED — `Trueflow.app` bundle produced

Previously validated automatically on 2026-08-30:

- `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check`: VERIFIED
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings`: VERIFIED
- `cargo test --manifest-path src-tauri/Cargo.toml --all-features`: VERIFIED —
  152 Rust unit tests and 2 streaming integration tests
- `pnpm format:check`: VERIFIED
- `pnpm lint`: VERIFIED with zero warnings
- `pnpm typecheck`: VERIFIED in strict mode
- `pnpm test`: VERIFIED — 5 files and 22 frontend tests
- `pnpm build`: VERIFIED
- `pnpm tauri build`: VERIFIED — optimized binary and
  `src-tauri/target/release/bundle/macos/Trueflow.app` produced

## Manual validation

No Milestone 7 physical macOS or live-service validation was performed in this
implementation run. Automated mocks validate the account and external-reporting
state machines, but do not establish that a deployed service contract or
provider credentials work.

## Unresolved issues

- Run the documented clean/fresh-profile onboarding flow and verify microphone
  and Accessibility denial, Settings recovery, app restart, first real
  dictation, and onboarding resume behavior.
- Exercise current status/context/shortcut display, Open, Settings, History,
  Pause/Resume, window close/reopen, restart, and Quit from the menu bar.
- Validate history against real dictation and rewrite operations, including
  copy, paging, individual deletion, clear-all, restart persistence, long text,
  Unicode, and failure records.
- Deploy/configure the first-party auth contract and test browser sign-in,
  Keychain restore, Rust-only provider credentials, first dictation without
  shell keys, expiry, offline startup/recovery, and sign-out on macOS. With no
  auth base URL or development STT key, onboarding truthfully blocks at account
  readiness rather than claiming dictation is configured.
- Configure test analytics and crash endpoints and inspect received payloads to
  confirm opt-in/opt-out and metadata-only behavior. These integrations are off
  or unconfigured by default.
- Repeat the physical macOS insertion, selection-rewrite, focus, clipboard,
  Spaces/full-screen, provider-failure, cancellation, and latency regression
  matrices from prior milestones.
- Microphone selection is implemented but not physically validated. Run the
  microphone rows of the Milestone 7 Settings matrix in `docs/TEST_MATRIX.md`
  (USB/Bluetooth listing, unplug fallback, replug, restart, start latency).
- The Settings "input level test" from MVP plan §29 remains the live overlay
  meter during a real dictation; there is no standalone level test.
- Billing, entitlements, usage accounting, updater behavior, signing,
  notarization, and release automation are intentionally not implemented.

## Next milestone

**Milestone 8 — Paid Beta / Release Infrastructure**, only after Milestone 7
manual acceptance is completed.

## Next recommended Codex prompt

> Implement **Milestone 8 — Paid Beta / Release Infrastructure only**, following
> `AGENTS.md` and `docs/MVP_PLAN.md`. First read the required project documents
> completely, inspect the completed Milestones 0–7 implementation and recorded
> automated/manual validation, and produce a concise plan before editing. Add
> billing, entitlement checks, basic usage accounting, strict staging/production
> separation, signed and notarized macOS distribution, a tested Tauri updater,
> and a reproducible release process. Preserve the Rust-owned dictation and
> selection-rewrite pipelines, operation safety, local data/privacy guarantees,
> authenticated credential boundaries, bounded network behavior, least-privilege
> Tauri capabilities, and all productization recovery paths. Do not add later
> product features, Windows/Linux/mobile support, manuscript-wide RAG,
> collaboration, or unrelated scope. Use test/sandbox billing flows where
> appropriate; add automated tests and a documented clean-machine/payment/
> entitlement/update N→N+1 manual matrix; update `README.md`,
> `docs/PROGRESS.md`, `docs/DECISIONS.md`, and `docs/TEST_MATRIX.md`; run all
> repository validation including production Tauri builds; and distinguish
> automated verification from actual signing/notarization/payment/update/manual
> validation without claiming unperformed tests.
