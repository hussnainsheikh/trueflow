# AI Voice Writing Desktop App — MVP Plan

## 1. Product Definition

Build a macOS-first desktop application that allows users to dictate polished text into virtually any text-capable desktop application.

The core interaction is:

**Hold global shortcut → speak → release → processed text appears at the user's existing cursor.**

The initial target market is:

- authors;
- fiction writers;
- long-form writers;
- creators who care about maintaining a consistent writing voice.

The product must also remain useful for normal system-wide dictation.

---

# 2. Product Thesis

Basic speech recognition is becoming a commodity.

Our differentiation is not merely:

> speech → transcript

It is:

> speech → intent → vocabulary → Voice → Mode → useful writing

Three layers of "voice" must remain conceptually separate:

### Physical voice

What the microphone captured.

Handled by speech recognition.

### Writing Voice

How the user or persona writes.

Examples:

- My Voice
- Narrator
- Kael
- Elara

### Mode

What the user is trying to do.

Examples:

- Clean Dictation
- Draft
- Dialogue
- Brainstorm
- Rewrite
- Outline

Voice and Mode must be independent.

Example:

Voice:
`Kael`

Mode:
`Dialogue`

is different from:

Voice:
`Kael`

Mode:
`Brainstorm`

---

# 3. MVP Success Condition

The MVP succeeds if beta users voluntarily use it instead of:

- typing;
- macOS Dictation;
- copying speech into ChatGPT;
- basic dictation tools.

The primary subjective reaction sought is:

> "I don't want to go back to typing everything."

---

# 4. Supported Platform

MVP:

- macOS only.

Target initially:

- Apple Silicon.

Intel Mac support is optional if Tauri produces it without material additional work.

Do not implement:

- Windows;
- Linux;
- iOS;
- Android.

The architecture should allow future platform implementations without contaminating the Rust core with unnecessary macOS dependencies.

---

# 5. Technology Stack

## Desktop shell

Tauri 2.

## Core

Rust.

## Frontend

- React
- TypeScript
- Vite

Use a lightweight frontend.

Recommended:

- React Router only if routing becomes necessary;
- Zustand for modest UI-only state if needed;
- TanStack Query only if remote server state becomes sufficiently complex.

Do not add libraries simply because they are fashionable.

## Styling

Use a simple component architecture.

Tailwind CSS is acceptable.

Avoid adopting a massive UI framework for the MVP.

## Local persistence

SQLite.

Persist:

- settings;
- projects;
- Voices;
- Modes;
- vocabulary;
- dictation history metadata;
- finalized transcript text when history is enabled;
- performance events;
- migration version.

## Cloud backend

Use a small TypeScript service.

Recommended:

- Node.js
- TypeScript
- Hono or Fastify
- PostgreSQL

Responsibilities:

- authentication integration;
- user account;
- entitlement/subscription state;
- server-side provider credentials;
- AI transformation endpoint;
- provider-token brokering where supported;
- sync later;
- usage accounting;
- abuse/rate limiting.

Avoid microservices.

One deployable backend is enough.

## Authentication

Use a managed provider.

Acceptable MVP choices:

- Clerk;
- Supabase Auth.

Select one and record the decision in `docs/DECISIONS.md`.

## Billing

Stripe.

Billing may be added after core voice workflow is validated but must be included before public paid beta.

## Analytics

PostHog or equivalent.

Analytics events must not include manuscript text by default.

## Error reporting

Sentry or equivalent.

Redact user text from error payloads.

---

# 6. High-Level Architecture

```text
┌─────────────────────────────────────────────┐
│                 Tauri App                   │
│                                             │
│  React UI                                   │
│  ├─ onboarding                              │
│  ├─ settings                                │
│  ├─ history                                 │
│  ├─ projects                                │
│  ├─ voices                                  │
│  ├─ modes                                   │
│  └─ vocabulary                              │
│                 │                           │
│                 │ commands/events           │
│                 ▼                           │
│  ┌───────────────────────────────────────┐  │
│  │              Rust Core                │  │
│  │                                       │  │
│  │ hotkey                                │  │
│  │ audio                                 │  │
│  │ state machine                         │  │
│  │ transcription                         │  │
│  │ transformation                        │  │
│  │ vocabulary                            │  │
│  │ local storage                         │  │
│  │ context                               │  │
│  │ insertion                             │  │
│  │ telemetry                             │  │
│  │ fallback/recovery                     │  │
│  └───────────────────┬───────────────────┘  │
│                      │                      │
└──────────────────────┼──────────────────────┘
                       │
                       ▼
              Cloud/API services
              ├─ STT provider
              ├─ LLM provider
              ├─ backend
              ├─ auth
              └─ billing
```

---

# 7. Repository Structure

Start with:

```text
/
├── AGENTS.md
├── README.md
├── package.json
├── pnpm-lock.yaml
├── docs/
│   ├── MVP_PLAN.md
│   ├── PROGRESS.md
│   ├── DECISIONS.md
│   └── TEST_MATRIX.md
│
├── src/
│   ├── app/
│   ├── components/
│   ├── features/
│   │   ├── onboarding/
│   │   ├── settings/
│   │   ├── projects/
│   │   ├── voices/
│   │   ├── modes/
│   │   ├── vocabulary/
│   │   └── history/
│   ├── hooks/
│   ├── lib/
│   ├── types/
│   └── main.tsx
│
└── src-tauri/
    ├── Cargo.toml
    ├── capabilities/
    ├── migrations/
    └── src/
        ├── lib.rs
        ├── error.rs
        ├── state.rs
        ├── commands/
        ├── audio/
        ├── hotkey/
        ├── dictation/
        ├── transcription/
        │   ├── mod.rs
        │   ├── provider.rs
        │   └── mock.rs
        ├── transformation/
        ├── vocabulary/
        ├── context/
        ├── insertion/
        ├── storage/
        ├── telemetry/
        └── platform/
            └── macos/
```

Do not create separate Cargo crates for every module on day one.

Begin as a modular Rust application.

Extract crates only when module boundaries prove valuable.

---

# 8. Dictation State Machine

Represent the pipeline explicitly.

```text
Idle
 │
 │ shortcut pressed
 ▼
Starting
 │
 │ microphone active
 ▼
Listening
 │
 │ shortcut released
 ▼
Finalizing
 │
 │ final transcript
 ▼
Transforming
 │
 │ output ready
 ▼
Inserting
 │
 │ success
 ▼
Completed
 │
 ▼
Idle
```

Recoverable failures may transition to:

```text
RecoverableError
```

then return to Idle after user-visible handling.

Examples:

- transformation provider unavailable;
- insertion unavailable but clipboard works;
- temporary network failure.

Fatal configuration errors may use:

```text
FatalError
```

Examples:

- required microphone permission permanently unavailable;
- unrecoverable database initialization failure.

---

# 9. Global Shortcut

MVP requires hold-to-talk.

Default candidate:

`Option + Space`

or another configurable shortcut that does not conflict excessively with macOS.

Requirements:

- Press starts dictation.
- Release stops dictation.
- Shortcut works while another application is focused.
- App main window does not need focus.
- Shortcut configuration stored locally.
- Detect registration failure.
- User can change shortcut in Settings.

Do not rely on React keyboard events.

Shortcut handling belongs in Rust.

---

# 10. Overlay

Create a small always-on-top floating overlay.

Requirements:

- does not steal focus;
- appears quickly after shortcut press;
- disappears after completion/error timeout;
- draggable is optional;
- no Dock-like large UI;
- visually minimal.

States:

### Starting

Small activity indicator.

### Listening

- microphone/listening indicator;
- current project;
- current Voice;
- current Mode;
- optional partial transcript.

### Processing

Show concise processing state.

### Success

Very brief confirmation only if needed.

### Error

Provide actionable short message.

Example:

`Couldn't insert text — copied to clipboard.`

Overlay must never require clicking during normal dictation.

---

# 11. Audio Capture

Audio capture belongs in Rust.

Requirements:

- enumerate/select default microphone;
- capture PCM suitable for STT;
- low-latency buffering;
- bounded memory;
- graceful device loss handling;
- no audio file written to disk during normal operation;
- stream chunks while speaking;
- retain enough temporary buffered audio to improve resilience.

Recommended internal format where supported:

- mono;
- 16 kHz or provider-preferred rate;
- 16-bit PCM where provider expects it.

Do not resample unnecessarily.

Provider requirements may override format.

Audio capture must support cancellation.

---

# 12. Speech-to-Text Provider Abstraction

Do not couple the domain pipeline to one provider.

Define a conceptual interface similar to:

```rust
trait TranscriptionProvider {
    async fn start_session(...) -> Result<Session>;
}
```

Session behavior should support:

- send audio chunk;
- receive partial transcript;
- receive final transcript;
- finalize;
- cancel;
- connection state;
- provider errors.

Define normalized internal events:

```text
SessionStarted
PartialTranscript
FinalTranscript
SpeechStarted
SpeechEnded
ProviderWarning
ProviderError
SessionClosed
```

First production implementation:

Use a high-quality streaming STT provider selected by benchmark.

Candidates should include at least:

- Deepgram;
- OpenAI realtime/transcription;
- AssemblyAI.

Benchmark rather than blindly choosing based on marketing.

Mock provider required for automated tests.

---

# 13. STT Benchmark Harness

Before locking the provider, create a small benchmark/evaluation tool.

Dataset:

At least 50 internally recorded samples representing:

- short dictation;
- long dictation;
- names;
- unusual fantasy vocabulary;
- corrections;
- punctuation;
- quiet speech;
- fast speech;
- different microphone quality;
- filler words;
- self-corrections.

Measure:

- time to connection;
- time to first partial;
- finalization latency;
- word error/correction burden;
- proper noun accuracy;
- stability of partial transcripts;
- connection failures;
- cost.

Store benchmark results outside production user analytics.

Select the initial provider based on measured performance.

Record result in `docs/DECISIONS.md`.

---

# 14. Vocabulary

Vocabulary is a core MVP feature.

## Vocabulary entry

Fields:

```text
id
project_id
term
pronunciation_hint
type
description
aliases
forbidden_variants
case_sensitive
created_at
updated_at
```

Types:

- Person
- Place
- Organization
- Object
- Technical
- Other

Example:

```text
Term:
Aeryndor

Pronunciation:
air-in-door

Type:
Person

Description:
Former commander of the Northern Guard

Aliases:
Commander Aeryndor
Aery

Forbidden variants:
Erin Door
Arindor
```

Vocabulary must influence both:

1. transcription where provider features allow;
2. post-transcription normalization/transformation.

Do not depend solely on STT provider vocabulary features.

Implement deterministic replacements carefully.

Avoid replacing substrings inside unrelated words.

---

# 15. Projects

Project fields:

```text
id
name
description
default_voice_id
default_mode_id
created_at
updated_at
archived_at
```

Examples:

- The Last Kingdom
- Newsletter
- Personal Writing

The active project is global application state.

Changing project changes available/default contextual vocabulary and Voice choices.

MVP requires:

- create;
- rename;
- select;
- archive;
- delete with confirmation.

Provide a default project automatically.

---

# 16. Voices

A Voice represents how text should sound.

Fields:

```text
id
project_id nullable
name
description
instructions
examples
avoidances
is_default
created_at
updated_at
```

Examples:

- My Voice
- Narrator
- Kael
- Elara

Voice should support:

### Description

Human-readable description.

### Instructions

Structured prose describing:

- tone;
- sentence rhythm;
- POV;
- tense;
- vocabulary;
- dialogue tendencies;
- stylistic rules.

### Examples

Multiple input/output or representative writing examples.

### Avoidances

Examples:

- no corporate language;
- avoid excessive adjectives;
- avoid em dashes;
- never use modern slang.

MVP does not need automatic style extraction from uploaded manuscripts.

Manual creation is sufficient.

---

# 17. Modes

Mode represents what transformation should happen.

Built-in MVP modes:

### Clean Dictation

Preserve meaning closely.

Only:

- punctuation;
- capitalization;
- filler removal;
- false-start cleanup;
- obvious grammar correction.

Do not creatively rewrite.

### Draft

Turn spoken thought into readable prose while respecting selected Voice.

### Dialogue

Produce dialogue appropriate for selected character Voice.

### Brainstorm

Preserve ideas but structure them clearly.

### Rewrite Selection

Transforms selected text based on spoken instruction.

Custom Modes may come after basic built-ins work.

Mode fields:

```text
id
name
description
system_instructions
requires_voice
supports_selection
is_builtin
created_at
updated_at
```

---

# 18. Transformation Engine

Inputs:

```text
raw transcript
normalized transcript
active project
relevant vocabulary
selected Voice
selected Mode
selected text if applicable
active application metadata
user preference settings
```

Output:

```text
final text
transformation type
provider metadata
latency metadata
fallback status
```

Prompt construction must be deterministic and testable.

Do not construct prompts ad hoc across UI components.

Implement one transformation/prompt builder module.

Important instruction:

Never let normal cleanup turn the user's prose into generic verbose AI text.

For Clean Dictation:

- preserve wording;
- preserve meaning;
- minimize stylistic intervention.

For author modes:

- obey Voice;
- obey vocabulary;
- obey Mode;
- do not invent plot facts unless mode explicitly allows creativity.

---

# 19. Fast Path

Not every dictation needs expensive transformation.

Implement the architecture so we can later support:

```text
Transcript
  │
  ├─ no transformation necessary
  │      → insert transcript
  │
  ├─ light cleanup
  │      → fast transformation
  │
  └─ Voice/Mode transformation
         → full transformation
```

MVP may initially transform all non-raw modes.

Do not hard-code the architecture in a way that prevents a fast path.

---

# 20. AI Provider Abstraction

Do not couple transformation logic to one LLM vendor.

Internal provider interface should support:

- request;
- streaming where useful;
- cancellation;
- timeout;
- normalized error;
- usage metadata.

Prompt builder must remain provider independent.

Provider credentials must never be embedded in frontend JavaScript.

---

# 21. Backend API

Minimum endpoints conceptually:

```text
POST /v1/auth/session
GET  /v1/me

POST /v1/transcription/token
POST /v1/transform

GET  /v1/entitlements
POST /v1/usage

POST /v1/billing/checkout
POST /v1/billing/portal
POST /v1/webhooks/stripe
```

Exact paths can evolve.

Use versioned API routes.

Use structured error bodies.

Example:

```json
{
  "error": {
    "code": "TRANSFORMATION_TIMEOUT",
    "message": "Transformation timed out",
    "retryable": true
  }
}
```

Do not leak provider internals directly to the client.

---

# 22. Authentication

MVP flow:

1. Launch app.
2. User can test onboarding if desired.
3. Sign in through browser/system auth flow.
4. Desktop receives authenticated session through secure mechanism/deep link.
5. Session stored securely.
6. Refresh handled automatically.

Never put auth tokens in plain localStorage where avoidable.

---

# 23. Text Insertion

This is critical.

Primary goal:

Insert text at the focused text element without stealing focus.

Implementation order:

### Strategy A

macOS Accessibility APIs where reliable.

### Strategy B

Clipboard + simulated paste where appropriate and permitted.

### Strategy C

Clipboard-only fallback with user notification.

The insertion module exposes one interface regardless of strategy.

Return structured result:

```text
InsertedDirectly
InsertedViaPaste
CopiedToClipboard
Failed
```

Do not report success unless insertion/copy actually succeeded.

Preserve the user's prior clipboard content where practical if using temporary paste-based insertion.

Test extensively.

---

# 24. Selection-Based Rewrite

Secondary shortcut:

User highlights text in another application.

User triggers rewrite action.

App:

1. obtains selected text;
2. captures spoken instruction;
3. passes:

   - selected text;
   - instruction;
   - Voice;
   - project vocabulary;

4. transforms;
5. replaces selected text.

Example:

Selected:

`Kael walked across the room. He was angry.`

Instruction:

`Make this more tense but don't explicitly say he's angry.`

Result replaces selection.

If selected text cannot be retrieved:

- show clear error;
- do not destroy clipboard/selection.

---

# 25. Application Context

MVP context:

- active application bundle/name;
- focused element information when safely accessible;
- selected text for rewrite;
- project/Voice/Mode.

Do NOT capture the entire screen.

Do NOT automatically scrape complete documents.

Do NOT upload unrelated application content.

Future contextual features must remain explicitly scoped.

---

# 26. Local Database

Use migrations from day one.

Tables conceptually:

```text
settings
projects
voices
voice_examples
modes
vocabulary_entries
dictation_history
performance_events
schema_metadata
```

History row:

```text
id
project_id
voice_id
mode_id
source_app
raw_transcript
final_text
status
started_at
completed_at
latency_ms
created_at
```

Allow history to be disabled.

Future migration compatibility matters.

Never modify schema manually without migration.

---

# 27. History

MVP history screen:

- timestamp;
- application;
- project;
- Voice;
- Mode;
- final text preview;
- copy button;
- optional raw transcript view;
- delete item;
- clear history.

History must not become a writing editor.

Default retention should be configurable later.

Provide immediate clear-all functionality.

---

# 28. Onboarding

Onboarding should be short.

Required flow:

### Screen 1

Product explanation.

`Hold shortcut. Speak. Release.`

### Screen 2

Microphone permission.

Explain why required.

### Screen 3

Accessibility permission.

Explain why required.

Provide button to open relevant System Settings location where possible.

### Screen 4

Shortcut test.

User presses shortcut and records a test phrase.

### Screen 5

Result test.

Insert or display the first transformed result.

### Screen 6

Choose default behavior:

- Clean Dictation;
- Draft.

Then enter app.

Onboarding cannot be considered complete until permissions/status are rechecked.

---

# 29. Settings

MVP settings:

## General

- launch at login optional;
- show overlay;
- history enabled.

## Shortcut

- push-to-talk shortcut;
- rewrite shortcut.

## Audio

- microphone selection;
- input level test.

## Writing

- active project;
- default Voice;
- default Mode.

## Privacy

- history enabled;
- clear history;
- analytics preference where legally/product-appropriate.

## Account

- email;
- subscription;
- sign out.

## About

- version;
- check update;
- diagnostic information.

---

# 30. Menu Bar

The app should live comfortably as a menu-bar utility.

Menu items:

```text
Status: Ready

Project: <current>
Voice: <current>
Mode: <current>

Open App
Settings
History

Pause Dictation
Quit
```

Do not require the Dock icon to be the primary interaction.

Whether the Dock icon is hidden permanently or configurable can be decided during implementation.

---

# 31. Permissions

Required macOS permissions:

- microphone;
- Accessibility.

Potential permission requirements must be documented before adding them.

Do not request permissions that are not required.

App must detect permission state and provide actionable recovery when permission is denied.

---

# 32. Telemetry

Every dictation gets a unique operation ID.

Record timestamps:

```text
shortcut_pressed
capture_started
stt_connection_ready
first_audio_sent
first_partial_received
shortcut_released
final_transcript_received
transform_started
transform_first_result
transform_completed
insert_started
insert_completed
```

Derived metrics:

```text
mic startup latency
STT first partial latency
STT finalization latency
transformation latency
insertion latency
total post-release latency
end-to-end latency
```

Do not send transcript content in performance telemetry.

Targets for beta:

### Desired

Release → useful text starts/appears:

- p50 < 500 ms where feasible
- p95 < 1.2 seconds where feasible

Voice transformations may initially exceed this.

Measure before optimizing.

Also measure:

- failed dictations;
- fallback rate;
- insertion failure rate;
- provider errors.

---

# 33. Product Analytics

Events:

```text
app_installed
onboarding_started
onboarding_completed

dictation_started
dictation_completed
dictation_failed

rewrite_started
rewrite_completed
rewrite_failed

project_created
voice_created
mode_selected
vocabulary_created

clipboard_fallback_used
permission_denied
subscription_started
```

Properties may include:

- duration;
- word count;
- mode;
- source application category;
- latency;
- error code.

Do not send actual dictated text to product analytics.

---

# 34. Error Model

Create a domain error taxonomy.

Examples:

```text
MicrophonePermissionDenied
MicrophoneUnavailable
AudioDeviceLost

ShortcutRegistrationFailed

STTConnectionFailed
STTTimeout
STTProviderUnavailable
STTInvalidResponse

TransformTimeout
TransformProviderUnavailable
TransformInvalidResponse

AccessibilityPermissionDenied
SelectionUnavailable
InsertionFailed
ClipboardFailed

DatabaseUnavailable
MigrationFailed

AuthenticationExpired
NetworkUnavailable
RateLimited
```

Each error declares:

- user-safe message;
- internal diagnostic details;
- retryability;
- fallback action.

---

# 35. Reliability/Fallback Matrix

## Microphone unavailable

Do not start dictation.

Show actionable error.

## STT connection unavailable before recording

Retain/buffer audio where feasible.

Attempt bounded reconnect.

If impossible, fail visibly.

## STT disconnects mid-session

Attempt controlled reconnect if provider/session supports it.

Never pretend transcript is complete.

## Transcript succeeds, transformation fails

Insert finalized transcript.

Mark operation as fallback.

## Direct insertion fails

Attempt safe paste strategy.

## Paste strategy fails

Copy final text to clipboard.

Show:

`Couldn't insert automatically. Text copied to clipboard.`

## Backend unavailable

If transcription can operate independently, continue dictation and skip advanced transformation.

Otherwise show recoverable offline error.

---

# 36. Security

Never expose permanent third-party AI keys in desktop/frontend code.

Use HTTPS/WSS.

Validate backend inputs.

Set server-side request size limits.

Rate-limit expensive endpoints.

Sanitize logs.

Use least-privilege Tauri capabilities.

Keep Tauri frontend command permissions explicitly scoped.

Do not enable broad shell execution.

Do not enable arbitrary filesystem access.

Do not expose unrestricted network/client capabilities to UI without reason.

---

# 37. Automatic Updates

Before public beta:

- configure signed builds;
- configure updater;
- test update from N to N+1;
- test failed update behavior;
- keep database migrations backward-safe where practical.

Do not postpone updater testing until many external users already have builds.

---

# 38. Build and Distribution

MVP distribution:

- signed macOS app;
- notarized;
- DMG distribution outside the Mac App Store initially unless product constraints suggest otherwise.

Production build must not use development API endpoints.

Separate:

- local;
- staging;
- production configuration.

---

# 39. Development Environments

Create:

```text
.env.example
```

Never commit:

```text
.env
provider API secrets
Stripe secrets
signing secrets
auth secrets
```

Define environment modes:

```text
development
test
staging
production
```

---

# 40. Logging

Use structured logs.

Include:

- operation ID;
- subsystem;
- event;
- duration;
- error code.

Avoid including raw dictated text.

Development mode may allow explicitly enabled verbose diagnostics, but production defaults must protect content.

---

# 41. Testing Strategy

## Rust unit tests

Test:

- state machine;
- normalization;
- vocabulary matcher;
- Voice/Mode prompt composition;
- error mapping;
- fallback decisions;
- provider abstraction;
- metrics calculations.

## Rust integration tests

Use mock:

- STT provider;
- transformation provider;
- insertion provider.

Test complete pipeline:

```text
press
→ audio
→ transcript
→ transformation
→ insertion
```

including failure cases.

## Frontend tests

Test:

- onboarding state;
- settings;
- project CRUD;
- Voice CRUD;
- vocabulary CRUD;
- history.

## Manual macOS integration tests

Test at minimum:

- Notes;
- TextEdit;
- Safari;
- Chrome;
- Google Docs;
- Slack;
- VS Code;
- Microsoft Word if available;
- Scrivener if available;
- Obsidian if available.

For every application test:

- normal insertion;
- multiline insertion;
- punctuation;
- clipboard preservation;
- Unicode;
- long text;
- rapid repeated dictations;
- focus preservation.

---

# 42. Test Corpus

Maintain a privacy-safe test corpus.

Examples:

### Simple

`hello john i'll send that tomorrow`

Expected cleanup:

`Hello John, I'll send that tomorrow.`

### Messy thought

`okay so I think we should um probably move the second scene actually no keep it but move the reveal until later`

Expected:

Meaning preserved while removing filler and false start.

### Proper noun

Vocabulary includes:

`Aeryndor`

Speech approximates pronunciation.

Expected:

Correct project spelling.

### Character Voice

Voice:

`Kael`

Mode:

`Dialogue`

Input:

`tell elara we can't stay because the king's men will find us`

Expected:

Valid dialogue consistent with defined Voice without inventing unrelated plot.

---

# 43. Performance Tests

Measure:

- app startup;
- shortcut response;
- microphone startup;
- memory during idle;
- memory during long dictation;
- CPU idle;
- CPU while recording;
- STT partial latency;
- finalization latency;
- transformation latency;
- insertion latency.

Test recordings:

- 5 seconds;
- 30 seconds;
- 2 minutes;
- 5 minutes.

MVP may cap single dictation duration.

If capped, surface limit clearly.

---

# 44. Accessibility

The app UI should support:

- keyboard navigation;
- sensible labels;
- adequate contrast;
- system text sizing where practical.

Do not break macOS Accessibility users while relying on Accessibility APIs ourselves.

---

# 45. MVP UX Screens

Build only these major surfaces:

1. Onboarding
2. Home/status
3. Projects
4. Voices
5. Vocabulary
6. Modes/default behavior
7. History
8. Settings
9. Account/billing

Do not build a document editor.

---

# 46. Home Screen

Show:

```text
Ready to dictate

Shortcut:
⌥ Space

Project:
The Last Kingdom

Voice:
Narrator

Mode:
Draft

Today:
42 dictations
6,840 words
```

Provide quick selectors.

Avoid dashboard clutter.

---

# 47. Keyboard UX

Primary:

Hold shortcut → dictation.

Secondary:

Rewrite selected text.

Potential future shortcuts:

- cycle Voice;
- cycle Mode;
- raw dictation.

Do not overload MVP with shortcuts.

---

# 48. Offline Behavior

MVP does not promise fully offline transcription.

However:

- app UI must start offline;
- projects/Voices/vocabulary stored locally;
- existing settings must work;
- network failure must not corrupt state;
- unavailable cloud capabilities clearly identified.

Offline/local STT is future scope.

---

# 49. Privacy UX

Explain clearly:

- microphone is active only during dictation unless product behavior changes;
- what is sent to transcription provider;
- what is sent for AI transformation;
- what history is stored;
- how history can be deleted.

Do not use misleading "local" wording for cloud processing.

---

# 50. MVP Non-Goals

Explicitly excluded:

- custom manuscript editor;
- complete manuscript ingestion;
- RAG;
- continuity detection;
- automatic character extraction;
- automatic world model;
- team collaboration;
- enterprise administration;
- mobile;
- Windows;
- local AI models;
- marketplace;
- browser extension;
- autonomous computer actions;
- meeting transcription;
- voice commands controlling arbitrary applications.

These can be evaluated after product signal.

---

# 51. Milestone 0 — Repository Foundation

Deliver:

- Tauri 2 app;
- React/TypeScript/Vite frontend;
- Rust formatting/lint setup;
- TypeScript strict mode;
- test framework;
- CI;
- environment config;
- docs;
- SQLite migration infrastructure;
- structured logging.

Acceptance:

- fresh clone documented;
- one command starts development environment;
- frontend tests run;
- Rust tests run;
- typecheck passes;
- lint passes;
- production desktop build succeeds.

Do not implement product features yet.

---

# 52. Milestone 1 — Local Hold-to-Talk

Deliver:

- global shortcut;
- press event;
- release event;
- microphone capture;
- overlay states;
- audio meter;
- no cloud dependency.

Use a temporary local/mock transcript response if needed solely to test state transitions.

Acceptance:

- shortcut works from another focused app;
- press starts recording;
- release stops recording;
- overlay never steals focus;
- repeated 50 times without failure;
- permission-denied behavior works;
- no crashes.

---

# 53. Milestone 2 — Streaming STT

Deliver:

- provider abstraction;
- first production STT provider;
- partial transcript;
- final transcript;
- connection lifecycle;
- timeout/cancellation;
- telemetry.

Acceptance:

- 100 consecutive test dictations with no application crash;
- partials displayed;
- final transcript obtained;
- provider failure surfaced;
- audio not silently lost;
- benchmark metrics recorded.

At completion, manually compare with macOS Dictation using test corpus.

---

# 54. Milestone 3 — System-Wide Insertion

Deliver:

- macOS Accessibility permission flow;
- active application/focus tracking;
- direct insertion strategy;
- paste strategy;
- clipboard fallback.

Acceptance:

Successful basic insertion tested in:

- Notes;
- TextEdit;
- Chrome;
- Safari;
- VS Code;
- Google Docs.

Overlay does not steal focus.

Clipboard fallback works.

At this milestone the product's basic magic loop must exist:

```text
hold
→ speak
→ release
→ text
```

---

# 55. Milestone 4 — Projects + Vocabulary

Deliver:

- SQLite project schema;
- project CRUD;
- active project;
- vocabulary CRUD;
- pronunciation/aliases;
- normalization;
- STT vocabulary integration where available.

Acceptance:

- vocabulary influences output;
- project switching changes vocabulary;
- no cross-project leakage;
- migrations tested;
- Unicode names supported.

---

# 56. Milestone 5 — Voice + Mode Transformation

Deliver:

- Voice schema/UI;
- Mode schema;
- built-in modes;
- provider-independent prompt builder;
- transformation endpoint;
- fallback to transcript;
- transformation latency telemetry.

Acceptance:

Test corpus proves distinct output for:

```text
Narrator + Draft
Kael + Dialogue
My Voice + Clean Dictation
```

Transformation failure inserts transcript rather than losing text.

---

# 57. Milestone 6 — Selection Rewrite

Deliver:

- retrieve selected text;
- rewrite shortcut;
- spoken instruction;
- transform selected text;
- replace selection;
- safe failure behavior.

Acceptance:

Works in at least:

- Notes;
- Chrome/Google Docs;
- VS Code;
- TextEdit.

Failed selection retrieval never destroys user data.

---

# 58. Milestone 7 — Productization

Deliver:

- full onboarding;
- Settings;
- History;
- menu bar;
- account/auth;
- diagnostics;
- analytics;
- crash reporting;
- polished permission recovery.

Acceptance:

A new user can install the app without developer assistance and complete first successful dictation.

---

# 59. Milestone 8 — Paid Beta Infrastructure

Deliver:

- billing;
- entitlement checks;
- basic usage accounting;
- staging/production separation;
- signed/notarized macOS build;
- updater;
- release process.

Acceptance:

- signup works;
- payment works in production/test flow as appropriate;
- paid entitlement updates correctly;
- logout/login works;
- update N → N+1 tested;
- clean-machine installation tested.

---

# 60. Beta Gate

Do not call the MVP beta-ready until:

- crash-free basic workflow is stable;
- permissions onboarding works;
- 100 consecutive internal dictations complete without app crash;
- text insertion works acceptably across test matrix;
- transformation failures gracefully fall back;
- no API keys exist in frontend bundle;
- production build is signed/notarized;
- updater works;
- telemetry can identify pipeline latency;
- history deletion works;
- privacy behavior is documented.

---

# 61. Beta User Target

Initial beta:

10 writers.

Then:

25 users.

Then:

50 users.

Do not open broadly merely because signup works.

Observe actual usage.

---

# 62. Metrics

Primary:

### Dictations per active user per day

### Words dictated per active user per day

### D1 / D7 / D30 retention

### Successful dictation rate

### Successful automatic insertion rate

### p50/p95 post-release latency

Secondary:

- Voice usage;
- Mode usage;
- vocabulary usage;
- rewrite usage;
- corrections/undo where measurable without invasive tracking;
- fallback rate;
- provider error rate.

---

# 63. Early Product Questions

Use beta data to answer:

1. Do users care about Voices?
2. Do authors create character Voices?
3. Does vocabulary materially reduce corrections?
4. Is Draft mode preferred over Clean Dictation?
5. Is selection rewrite habit-forming?
6. What latency becomes irritating?
7. Which writing applications matter most?
8. Are users willing to pay?
9. Do writers use it daily?
10. Does output actually preserve their style?

Do not build manuscript intelligence until we answer these.

---

# 64. Future V2

Only after positive MVP signal:

- manuscript import;
- style learning from samples;
- automatic vocabulary extraction;
- character extraction;
- project memory;
- manuscript RAG;
- timeline;
- continuity warnings;
- advanced Voice learning;
- Windows;
- team/creator/professional verticals.

---

# 65. Performance Principle

Rust does not eliminate network or model latency.

Optimize the pipeline, not benchmarks unrelated to UX.

Important strategies:

- initialize microphone efficiently;
- maintain warm connections where appropriate;
- stream audio immediately;
- small audio buffers;
- process partial transcripts;
- finalize quickly on key release;
- begin downstream preparation early;
- use fast transformation models;
- avoid unnecessary transformations;
- avoid frontend round trips in the hot path;
- cancel obsolete requests;
- cache static Voice/project context;
- keep local configuration readily available.

---

# 66. Reliability Principle

The most important invariant:

> A successful recording should almost never produce zero recoverable text.

Second invariant:

> Never claim text was inserted when it was not.

Third invariant:

> User content is more important than fancy AI output.

---

# 67. Product Principle

Never become "another AI editor."

The product should disappear into the user's existing workflow.

The defining experience remains:

> Hold. Speak. Release. Continue writing.
