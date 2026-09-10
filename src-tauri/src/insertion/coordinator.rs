use std::{panic::AssertUnwindSafe, sync::Arc, time::Instant};

use async_trait::async_trait;
use futures_util::FutureExt;

use super::{
    AccessibilityPermissionState, ClipboardSnapshot, InsertionBackend, InsertionContext,
    InsertionError, InsertionErrorKind, InsertionObserver, InsertionOutcome, InsertionOutcomeKind,
    InsertionPipelineEvent, InsertionStrategy, TargetContext, TargetEditability, TextInserter,
};

pub struct InsertionCoordinator {
    backend: Arc<dyn InsertionBackend>,
}

impl InsertionCoordinator {
    pub fn new(backend: Arc<dyn InsertionBackend>) -> Self {
        Self { backend }
    }
}

#[async_trait]
impl TextInserter for InsertionCoordinator {
    fn permission_status(&self) -> AccessibilityPermissionState {
        self.backend.permission_status()
    }

    fn open_accessibility_settings(&self) -> Result<(), InsertionError> {
        self.backend.open_accessibility_settings()
    }

    async fn insert(
        &self,
        context: InsertionContext,
        text: String,
        observer: Arc<dyn InsertionObserver>,
    ) -> InsertionOutcome {
        let backend = self.backend.clone();
        let degraded = context.degraded_transcript;
        let result = AssertUnwindSafe(tokio::task::spawn_blocking(move || {
            run_strategy_chain(backend.as_ref(), &text, degraded, observer.as_ref())
        }))
        .catch_unwind()
        .await;

        match result {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(join_error)) => failed_outcome(
                degraded,
                InsertionError::new(
                    InsertionErrorKind::RuntimeFailed,
                    format!("insertion worker could not be joined: {join_error}"),
                ),
                0,
            ),
            Err(_) => failed_outcome(
                degraded,
                InsertionError::new(
                    InsertionErrorKind::RuntimeFailed,
                    "insertion worker panicked",
                ),
                0,
            ),
        }
    }
}

fn run_strategy_chain(
    backend: &dyn InsertionBackend,
    text: &str,
    degraded: bool,
    observer: &dyn InsertionObserver,
) -> InsertionOutcome {
    let started = Instant::now();
    let permission = backend.permission_status();
    let target = match backend.resolve_target() {
        Ok(target) => {
            observer.event(InsertionPipelineEvent::TargetResolved);
            Some(target)
        }
        Err(error) => {
            return clipboard_only(backend, None, text, degraded, error, observer, started);
        }
    };
    let Some(target_ref) = target.as_ref() else {
        return failed_outcome(
            degraded,
            InsertionError::new(
                InsertionErrorKind::RuntimeFailed,
                "resolved insertion target was unexpectedly absent",
            ),
            elapsed_ms(started),
        );
    };
    let target_name = target_ref.display_name();

    if target_ref.trueflow {
        return clipboard_only(
            backend,
            target_name,
            text,
            degraded,
            InsertionError::new(
                InsertionErrorKind::TrueflowTargetExcluded,
                "frontmost application is Trueflow",
            ),
            observer,
            started,
        );
    }
    if target_ref.secure {
        return failed_outcome_with_target(
            target_name,
            degraded,
            InsertionError::new(
                InsertionErrorKind::SecureField,
                "focused accessibility element is a secure field",
            ),
            elapsed_ms(started),
        );
    }
    if permission != AccessibilityPermissionState::Granted {
        let kind = if permission == AccessibilityPermissionState::Denied {
            InsertionErrorKind::AccessibilityPermissionDenied
        } else {
            InsertionErrorKind::AccessibilityUnavailable
        };
        return clipboard_only(
            backend,
            target_name,
            text,
            degraded,
            InsertionError::new(kind, format!("Accessibility status is {permission:?}")),
            observer,
            started,
        );
    }
    if target_ref.editability == TargetEditability::NotEditable {
        return clipboard_only(
            backend,
            target_name,
            text,
            degraded,
            InsertionError::new(
                InsertionErrorKind::TargetNotEditable,
                format!("focused role {:?} is not editable", target_ref.role),
            ),
            observer,
            started,
        );
    }

    if target_ref.direct_insertion_supported {
        observer.event(InsertionPipelineEvent::DirectInsertStarted);
        if backend.insert_directly(target_ref, text).is_ok() {
            observer.event(InsertionPipelineEvent::DirectInsertCompleted);
            return InsertionOutcome {
                kind: InsertionOutcomeKind::InsertedDirectly,
                strategy: InsertionStrategy::DirectAccessibility,
                target_application: target_name,
                duration_ms: elapsed_ms(started),
                clipboard_restored: false,
                fallback_used: false,
                degraded_transcript: degraded,
                error: None,
            };
        }
    }

    // Synthetic typing is attempted before the pasteboard because it touches no
    // shared global state. The Command+V chord depends on the window server's
    // modifier state and on the target reading a pasteboard we must then leave
    // or restore; in Microsoft Word two byte-identical dispatches were observed
    // producing different results, which that path cannot distinguish or
    // recover from. Typing has no such shared state to get wrong.
    observer.event(InsertionPipelineEvent::TypingStarted);
    if backend.type_text(target_ref, text).is_ok() {
        observer.event(InsertionPipelineEvent::TypingCompleted);
        return InsertionOutcome {
            kind: InsertionOutcomeKind::InsertedByTyping,
            strategy: InsertionStrategy::SyntheticTyping,
            target_application: target_name,
            duration_ms: elapsed_ms(started),
            clipboard_restored: false,
            fallback_used: true,
            degraded_transcript: degraded,
            error: None,
        };
    }

    paste_or_clipboard(
        backend,
        target_ref,
        target_name,
        text,
        degraded,
        observer,
        started,
    )
}

#[allow(clippy::too_many_arguments)]
fn paste_or_clipboard(
    backend: &dyn InsertionBackend,
    target: &TargetContext,
    target_name: Option<String>,
    text: &str,
    degraded: bool,
    observer: &dyn InsertionObserver,
    started: Instant,
) -> InsertionOutcome {
    observer.event(InsertionPipelineEvent::PasteFallbackStarted);
    let prior_clipboard = backend.capture_clipboard().ok();
    let change_count = match backend.write_clipboard(text) {
        Ok(change_count) => change_count,
        Err(error) => {
            return failed_outcome_with_target(target_name, degraded, error, elapsed_ms(started));
        }
    };

    if let Err(error) = backend.dispatch_paste(target.process_id) {
        observer.event(InsertionPipelineEvent::ClipboardOnlyFallback);
        return InsertionOutcome {
            kind: InsertionOutcomeKind::CopiedToClipboard,
            strategy: InsertionStrategy::ClipboardOnly,
            target_application: target_name,
            duration_ms: elapsed_ms(started),
            clipboard_restored: false,
            fallback_used: true,
            degraded_transcript: degraded,
            error: Some(error),
        };
    }
    observer.event(InsertionPipelineEvent::PasteDispatched);

    let (clipboard_restored, restoration_error) =
        restore_after_paste(backend, prior_clipboard.as_ref(), change_count, observer);
    InsertionOutcome {
        kind: InsertionOutcomeKind::InsertedViaPaste,
        strategy: InsertionStrategy::Paste,
        target_application: target_name,
        duration_ms: elapsed_ms(started),
        clipboard_restored,
        fallback_used: true,
        degraded_transcript: degraded,
        error: restoration_error,
    }
}

fn restore_after_paste(
    backend: &dyn InsertionBackend,
    snapshot: Option<&ClipboardSnapshot>,
    change_count: u64,
    observer: &dyn InsertionObserver,
) -> (bool, Option<InsertionError>) {
    let Some(snapshot) = snapshot.filter(|snapshot| snapshot.safely_restorable) else {
        return (false, None);
    };
    std::thread::sleep(backend.restoration_delay());
    observer.event(InsertionPipelineEvent::ClipboardRestoreStarted);
    match backend.restore_clipboard(snapshot, change_count) {
        Ok(true) => {
            observer.event(InsertionPipelineEvent::ClipboardRestoreCompleted);
            (true, None)
        }
        Ok(false) => (false, None),
        Err(error) => (false, Some(error)),
    }
}

fn clipboard_only(
    backend: &dyn InsertionBackend,
    target_application: Option<String>,
    text: &str,
    degraded: bool,
    reason: InsertionError,
    observer: &dyn InsertionObserver,
    started: Instant,
) -> InsertionOutcome {
    observer.event(InsertionPipelineEvent::ClipboardOnlyFallback);
    match backend.write_clipboard(text) {
        Ok(_) => InsertionOutcome {
            kind: InsertionOutcomeKind::CopiedToClipboard,
            strategy: InsertionStrategy::ClipboardOnly,
            target_application,
            duration_ms: elapsed_ms(started),
            clipboard_restored: false,
            fallback_used: true,
            degraded_transcript: degraded,
            error: Some(reason),
        },
        Err(error) => {
            failed_outcome_with_target(target_application, degraded, error, elapsed_ms(started))
        }
    }
}

fn failed_outcome(degraded: bool, error: InsertionError, duration_ms: u64) -> InsertionOutcome {
    failed_outcome_with_target(None, degraded, error, duration_ms)
}

fn failed_outcome_with_target(
    target_application: Option<String>,
    degraded: bool,
    error: InsertionError,
    duration_ms: u64,
) -> InsertionOutcome {
    InsertionOutcome {
        kind: InsertionOutcomeKind::Failed,
        strategy: InsertionStrategy::None,
        target_application,
        duration_ms,
        clipboard_restored: false,
        fallback_used: true,
        degraded_transcript: degraded,
        error: Some(error),
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    use super::*;

    /// The typing attempt that now precedes every pasteboard fallback.
    const TYPE_CALL: &str = "type:héllo 👋\nsecond line";

    #[derive(Clone)]
    struct Plan {
        permission: AccessibilityPermissionState,
        target: Result<TargetContext, InsertionError>,
        direct: Result<(), InsertionError>,
        typing: Result<(), InsertionError>,
        clipboard_capture: Result<ClipboardSnapshot, InsertionError>,
        clipboard_write: Result<u64, InsertionError>,
        paste: Result<(), InsertionError>,
        restore: Result<bool, InsertionError>,
    }

    impl Default for Plan {
        fn default() -> Self {
            Self {
                permission: AccessibilityPermissionState::Granted,
                target: Ok(target(true)),
                direct: Ok(()),
                // Existing coverage exercises the pasteboard chain, so typing
                // is unsupported by default and enabled per test.
                typing: Err(InsertionError::new(
                    InsertionErrorKind::TypingDispatchFailed,
                    "typing is unsupported in this plan",
                )),
                clipboard_capture: Ok(ClipboardSnapshot {
                    plain_text: Some("previous".to_owned()),
                    was_empty: false,
                    safely_restorable: true,
                }),
                clipboard_write: Ok(2),
                paste: Ok(()),
                restore: Ok(true),
            }
        }
    }

    struct MockBackend {
        plan: Plan,
        calls: Mutex<Vec<String>>,
    }

    impl MockBackend {
        fn new(plan: Plan) -> Self {
            Self {
                plan,
                calls: Mutex::new(Vec::new()),
            }
        }

        fn record(&self, call: impl Into<String>) {
            self.calls.lock().expect("calls lock").push(call.into());
        }

        fn calls(&self) -> Vec<String> {
            self.calls.lock().expect("calls lock").clone()
        }
    }

    impl InsertionBackend for MockBackend {
        fn permission_status(&self) -> AccessibilityPermissionState {
            self.plan.permission
        }

        fn open_accessibility_settings(&self) -> Result<(), InsertionError> {
            Ok(())
        }

        fn resolve_target(&self) -> Result<TargetContext, InsertionError> {
            self.record("resolve");
            self.plan.target.clone()
        }

        fn insert_directly(
            &self,
            _target: &TargetContext,
            text: &str,
        ) -> Result<(), InsertionError> {
            self.record(format!("direct:{text}"));
            self.plan.direct.clone()
        }

        fn type_text(&self, _target: &TargetContext, text: &str) -> Result<(), InsertionError> {
            self.record(format!("type:{text}"));
            self.plan.typing.clone()
        }

        fn capture_clipboard(&self) -> Result<ClipboardSnapshot, InsertionError> {
            self.record("capture");
            self.plan.clipboard_capture.clone()
        }

        fn write_clipboard(&self, text: &str) -> Result<u64, InsertionError> {
            self.record(format!("write:{text}"));
            self.plan.clipboard_write.clone()
        }

        fn dispatch_paste(&self, _expected_process_id: i32) -> Result<(), InsertionError> {
            self.record("paste");
            self.plan.paste.clone()
        }

        fn restore_clipboard(
            &self,
            _snapshot: &ClipboardSnapshot,
            _expected_change_count: u64,
        ) -> Result<bool, InsertionError> {
            self.record("restore");
            self.plan.restore.clone()
        }

        fn restoration_delay(&self) -> Duration {
            Duration::ZERO
        }
    }

    #[derive(Default)]
    struct Events(Mutex<Vec<InsertionPipelineEvent>>);

    impl InsertionObserver for Events {
        fn event(&self, event: InsertionPipelineEvent) {
            self.0.lock().expect("events lock").push(event);
        }
    }

    fn target(direct: bool) -> TargetContext {
        TargetContext {
            process_id: 42,
            application_name: Some("Writer".to_owned()),
            bundle_id: Some("com.example.writer".to_owned()),
            role: Some("AXTextArea".to_owned()),
            subrole: None,
            editability: TargetEditability::Editable,
            direct_insertion_supported: direct,
            secure: false,
            trueflow: false,
        }
    }

    fn error(kind: InsertionErrorKind) -> InsertionError {
        InsertionError::new(kind, "test failure")
    }

    async fn run(backend: Arc<MockBackend>) -> InsertionOutcome {
        InsertionCoordinator::new(backend)
            .insert(
                InsertionContext {
                    operation_id: "operation".to_owned(),
                    degraded_transcript: false,
                },
                "héllo 👋\nsecond line".to_owned(),
                Arc::new(Events::default()),
            )
            .await
    }

    #[tokio::test]
    async fn direct_success_stops_the_fallback_chain() {
        let backend = Arc::new(MockBackend::new(Plan::default()));
        let outcome = run(backend.clone()).await;

        assert_eq!(outcome.kind, InsertionOutcomeKind::InsertedDirectly);
        assert_eq!(
            backend.calls(),
            vec!["resolve", "direct:héllo 👋\nsecond line"]
        );
    }

    #[tokio::test]
    async fn typing_inserts_without_touching_the_pasteboard() {
        let backend = Arc::new(MockBackend::new(Plan {
            target: Ok(target(false)),
            typing: Ok(()),
            ..Plan::default()
        }));
        let outcome = run(backend.clone()).await;

        assert_eq!(outcome.kind, InsertionOutcomeKind::InsertedByTyping);
        assert_eq!(outcome.strategy, InsertionStrategy::SyntheticTyping);
        assert!(outcome.error.is_none());
        assert!(!outcome.clipboard_restored);
        // The pasteboard must never be read or written when typing succeeds, so
        // the user's clipboard survives the dictation untouched.
        assert_eq!(backend.calls(), vec!["resolve", TYPE_CALL]);
    }

    #[tokio::test]
    async fn typing_failure_still_falls_back_to_paste() {
        let backend = Arc::new(MockBackend::new(Plan {
            target: Ok(target(false)),
            typing: Err(error(InsertionErrorKind::TypingDispatchFailed)),
            ..Plan::default()
        }));
        let outcome = run(backend.clone()).await;

        assert_eq!(outcome.kind, InsertionOutcomeKind::InsertedViaPaste);
        assert!(backend.calls().contains(&"paste".to_owned()));
    }

    #[tokio::test]
    async fn direct_unsupported_uses_one_paste_and_restores_plain_clipboard() {
        let backend = Arc::new(MockBackend::new(Plan {
            target: Ok(target(false)),
            ..Plan::default()
        }));
        let outcome = run(backend.clone()).await;

        assert_eq!(outcome.kind, InsertionOutcomeKind::InsertedViaPaste);
        assert!(outcome.clipboard_restored);
        assert_eq!(
            backend.calls(),
            vec![
                "resolve",
                TYPE_CALL,
                "capture",
                "write:héllo 👋\nsecond line",
                "paste",
                "restore"
            ]
        );
    }

    #[tokio::test]
    async fn unknown_custom_editor_uses_paste_fallback() {
        let mut custom_editor = target(false);
        custom_editor.role = Some("AXGroup".to_owned());
        custom_editor.editability = TargetEditability::Unknown;
        let backend = Arc::new(MockBackend::new(Plan {
            target: Ok(custom_editor),
            ..Plan::default()
        }));

        let outcome = run(backend.clone()).await;

        assert_eq!(outcome.kind, InsertionOutcomeKind::InsertedViaPaste);
        assert_eq!(outcome.strategy, InsertionStrategy::Paste);
        assert_eq!(
            backend.calls(),
            vec![
                "resolve",
                TYPE_CALL,
                "capture",
                "write:héllo 👋\nsecond line",
                "paste",
                "restore"
            ]
        );
    }

    #[tokio::test]
    async fn known_noneditable_target_skips_paste_and_copies_once() {
        let mut button = target(false);
        button.role = Some("AXButton".to_owned());
        button.editability = TargetEditability::NotEditable;
        let backend = Arc::new(MockBackend::new(Plan {
            target: Ok(button),
            ..Plan::default()
        }));

        let outcome = run(backend.clone()).await;

        assert_eq!(outcome.kind, InsertionOutcomeKind::CopiedToClipboard);
        assert_eq!(outcome.strategy, InsertionStrategy::ClipboardOnly);
        assert_eq!(
            outcome.error.as_ref().map(|error| error.kind),
            Some(InsertionErrorKind::TargetNotEditable)
        );
        assert_eq!(
            backend.calls(),
            vec!["resolve", "write:héllo 👋\nsecond line"]
        );
    }

    #[tokio::test]
    async fn direct_and_paste_failures_leave_one_clipboard_copy() {
        let backend = Arc::new(MockBackend::new(Plan {
            direct: Err(error(InsertionErrorKind::DirectInsertionFailed)),
            paste: Err(error(InsertionErrorKind::PasteDispatchFailed)),
            ..Plan::default()
        }));
        let outcome = run(backend.clone()).await;

        assert_eq!(outcome.kind, InsertionOutcomeKind::CopiedToClipboard);
        assert_eq!(outcome.strategy, InsertionStrategy::ClipboardOnly);
        assert_eq!(
            backend.calls(),
            vec![
                "resolve",
                "direct:héllo 👋\nsecond line",
                TYPE_CALL,
                "capture",
                "write:héllo 👋\nsecond line",
                "paste"
            ]
        );
    }

    #[tokio::test]
    async fn clipboard_failure_preserves_a_failed_recoverable_outcome() {
        let backend = Arc::new(MockBackend::new(Plan {
            target: Ok(target(false)),
            clipboard_write: Err(error(InsertionErrorKind::ClipboardWriteFailed)),
            ..Plan::default()
        }));
        let outcome = run(backend).await;

        assert_eq!(outcome.kind, InsertionOutcomeKind::Failed);
        assert_eq!(
            outcome.error.as_ref().map(|error| error.kind),
            Some(InsertionErrorKind::ClipboardWriteFailed)
        );
    }

    #[tokio::test]
    async fn denied_permission_skips_automatic_strategies_and_copies_once() {
        let backend = Arc::new(MockBackend::new(Plan {
            permission: AccessibilityPermissionState::Denied,
            ..Plan::default()
        }));
        let outcome = run(backend.clone()).await;

        assert_eq!(outcome.kind, InsertionOutcomeKind::CopiedToClipboard);
        assert_eq!(
            backend.calls(),
            vec!["resolve", "write:héllo 👋\nsecond line"]
        );
    }

    #[tokio::test]
    async fn secure_field_never_inserts_or_touches_the_clipboard() {
        let mut secure = target(true);
        secure.secure = true;
        let backend = Arc::new(MockBackend::new(Plan {
            target: Ok(secure),
            ..Plan::default()
        }));
        let outcome = run(backend.clone()).await;

        assert_eq!(outcome.kind, InsertionOutcomeKind::Failed);
        assert_eq!(backend.calls(), vec!["resolve"]);
        assert_eq!(
            outcome.error.as_ref().map(|error| error.kind),
            Some(InsertionErrorKind::SecureField)
        );
    }

    #[tokio::test]
    async fn restoration_failure_is_not_reported_as_restored() {
        let backend = Arc::new(MockBackend::new(Plan {
            target: Ok(target(false)),
            restore: Err(error(InsertionErrorKind::ClipboardRestoreFailed)),
            ..Plan::default()
        }));
        let outcome = run(backend).await;

        assert_eq!(outcome.kind, InsertionOutcomeKind::InsertedViaPaste);
        assert!(!outcome.clipboard_restored);
        assert_eq!(
            outcome.error.as_ref().map(|error| error.kind),
            Some(InsertionErrorKind::ClipboardRestoreFailed)
        );
    }
}
