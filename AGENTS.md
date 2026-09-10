# AGENTS.md

## Project Mission

We are building a macOS-first, system-wide AI voice writing application.

The primary user presses and holds a global shortcut, speaks naturally, releases the shortcut, and receives polished text inserted into the application they were already using.

The initial wedge is writers and authors.

The key product differentiators are:

1. Extremely fast voice-to-text interaction.
2. High reliability.
3. Project-specific vocabulary.
4. Persistent writing Voices.
5. Writing Modes.
6. Preservation of the user's intended writing voice instead of generic AI prose.
7. Works inside existing writing applications rather than requiring users to adopt a proprietary editor.

The desktop application uses Tauri 2 with a Rust core and a React/TypeScript frontend.

The MVP targets macOS only.

---

# Codex Operating Rules

Before making significant changes:

1. Read this file.
2. Read `docs/MVP_PLAN.md`.
3. Read `docs/PROGRESS.md`.
4. Read `docs/DECISIONS.md` if it exists.
5. Inspect the existing implementation before proposing new architecture.
6. Do not assume a feature is missing until the codebase has been searched.

`docs/MVP_PLAN.md` is the product and architecture source of truth.

If implementation conflicts with the plan:

* do not silently redesign the product;
* document the issue;
* choose the smallest solution consistent with the product goals;
* record meaningful architectural deviations in `docs/DECISIONS.md`.

---

# Execution Rules

Work on one milestone or clearly scoped task at a time.

For every task:

1. inspect relevant existing code;
2. describe the implementation approach briefly;
3. implement it;
4. add or update tests;
5. run relevant formatting, linting, tests and builds;
6. fix regressions introduced by the task;
7. update `docs/PROGRESS.md`;
8. summarize what changed and what remains.

Do not declare a milestone complete until its acceptance criteria pass.

Never fake working functionality with placeholder success responses.

Mocks are acceptable only in tests or where the specification explicitly allows them.

---

# Engineering Priorities

In priority order:

1. Reliability
2. User-perceived latency
3. Correctness
4. Data safety
5. Privacy/security
6. Maintainability
7. Developer velocity
8. Visual polish
9. Feature breadth

Never sacrifice reliability merely to produce a more impressive demo.

A successful recording should almost never result in lost speech.

If transformation fails but transcription succeeds, preserve and surface the transcript.

---

# Architecture Boundaries

The performance-sensitive voice pipeline belongs in Rust.

Keep the React frontend outside the critical audio/transcription pipeline.

React is responsible primarily for:

* settings;
* onboarding;
* history;
* project management;
* Voice management;
* Mode management;
* vocabulary management;
* account UI;
* non-critical overlay presentation.

Rust owns:

* application state machine;
* global shortcut handling;
* microphone capture;
* audio buffering;
* streaming transcription;
* transcription provider abstraction;
* transformation orchestration;
* retry/fallback logic;
* local persistence;
* text insertion;
* clipboard fallback;
* macOS Accessibility integration;
* active application context;
* latency telemetry;
* sensitive desktop-side state.

Do not move latency-sensitive operations into React unless there is a compelling documented reason.

---

# Cross-Platform Design

The MVP targets macOS, but reusable core logic must not unnecessarily depend on macOS.

Separate:

* cross-platform core;
* macOS-specific implementation;
* UI.

Platform-specific APIs should live behind traits/interfaces.

Do not build Windows support during the MVP.

Do not compromise the macOS MVP in order to simulate premature cross-platform compatibility.

---

# State Machine

The dictation interaction must use an explicit state machine.

At minimum:

* Idle
* Starting
* Listening
* Finalizing
* Transforming
* Inserting
* Completed
* RecoverableError
* FatalError

Illegal state transitions should be prevented where practical.

Do not spread dictation state across unrelated boolean flags.

---

# Performance

Instrument the voice pipeline.

Capture timestamps for at least:

* shortcut pressed;
* audio capture started;
* first audio packet;
* STT connection ready;
* first partial transcript;
* shortcut released;
* final transcript received;
* transformation started;
* first transformation result/token when applicable;
* transformation completed;
* insertion started;
* insertion completed.

Expose p50/p95-capable event data to the local telemetry system.

Optimization work should be based on measurements rather than assumptions.

---

# Reliability Rules

Never discard recorded speech merely because a downstream service failed.

Required fallbacks:

STT succeeds, AI transformation fails:

* use the clean/final transcript.

Insertion fails:

* copy result to clipboard;
* notify the user clearly.

Network disconnects during recording:

* retain buffered audio;
* attempt bounded recovery when safe;
* surface a recoverable error;
* never pretend insertion succeeded.

Application restart after an interrupted operation:

* preserve completed recent transcripts where possible.

All network calls must have:

* explicit timeouts;
* bounded retries where appropriate;
* cancellation;
* structured error handling.

No infinite retries.

---

# Privacy and Security

Never commit secrets.

Never expose provider API keys in frontend JavaScript.

Secrets belong server-side or in secure local storage when appropriate.

Do not log:

* full audio;
* full manuscripts;
* authentication tokens;
* provider API keys;
* sensitive clipboard contents.

Diagnostic logs should favor metadata over user content.

The user must be able to delete local dictation history.

Permissions requested from macOS must be limited to what the feature requires.

---

# Rust Standards

Use current stable Rust.

Prefer:

* explicit error types;
* `thiserror` for library/domain errors where appropriate;
* `anyhow` only at application boundaries where appropriate;
* structured modules;
* async cancellation;
* RAII;
* bounded channels;
* ownership rather than unnecessary cloning.

Avoid:

* `unwrap()` in production paths;
* `expect()` except for truly impossible startup/config invariants;
* uncontrolled `spawn`;
* global mutable state;
* blocking the async runtime;
* giant modules;
* premature unsafe code.

Unsafe Rust requires a documented justification.

Use `cargo fmt`.

Use `cargo clippy`.

Add unit tests for domain logic.

---

# TypeScript Standards

Use TypeScript strict mode.

Avoid `any`.

Use runtime validation for untrusted external data.

Keep UI state separate from core domain state.

Prefer small components.

Do not duplicate Rust business rules in React.

Use generated/shared DTO types where practical.

Run:

* formatting;
* lint;
* typecheck;
* frontend tests;
* production build.

---

# Testing Expectations

Every important domain feature needs automated tests where practical.

Particularly test:

* state transitions;
* vocabulary normalization;
* prompt construction;
* fallback behavior;
* provider error mapping;
* transcript handling;
* database operations;
* settings migration;
* Voice + Mode composition;
* text transformation decisions.

Platform integrations requiring macOS must additionally have a documented manual test matrix.

---

# User Experience Rules

The core workflow must not require opening the main window.

Primary interaction:

hold shortcut → speak → release → text appears.

The overlay must not steal keyboard focus from the application the user is writing in.

Do not make the user manually copy normal successful dictations.

Do not force authors to write inside our application.

The application complements:

* Scrivener;
* Word;
* Google Docs;
* Notes;
* browsers;
* Obsidian;
* editors;
* communication apps.

---

# Scope Discipline

Do not add these during MVP unless explicitly requested:

* Windows;
* Linux;
* iOS;
* Android;
* proprietary manuscript editor;
* collaboration;
* manuscript-wide RAG;
* continuity checking;
* autonomous agents;
* plugin marketplace;
* AI image generation;
* team administration;
* dozens of languages;
* offline/local speech model;
* elaborate world-building database;
* mobile companion application.

Do not turn future ideas into MVP dependencies.

---

# Dependency Policy

Before adding a major dependency:

1. check whether the standard library or existing dependency solves the problem;
2. verify the library is actively maintained;
3. prefer mature and well-scoped dependencies;
4. avoid overlapping libraries for the same responsibility;
5. document architectural dependencies in `docs/DECISIONS.md`.

Pin reproducible dependency versions via the normal lockfiles.

---

# Documentation Rules

Maintain:

`README.md`

* developer setup;
* environment variables;
* build/run commands.

`docs/MVP_PLAN.md`

* product/technical source of truth.

`docs/PROGRESS.md`

* milestone status;
* completed work;
* unresolved blockers;
* next recommended task.

`docs/DECISIONS.md`

* meaningful architecture decisions.

Do not fill `AGENTS.md` with transient progress notes.

---

# Completion Standard

A feature is complete only when:

* implementation works;
* error states exist;
* automated tests pass where applicable;
* manual platform behavior has been tested where required;
* formatting/lint/typecheck pass;
* production build succeeds;
* relevant documentation is updated.

When uncertain, prefer the smaller, more reliable implementation.
