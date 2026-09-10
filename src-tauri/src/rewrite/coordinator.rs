use std::{panic::AssertUnwindSafe, sync::Arc, time::Instant};

use async_trait::async_trait;
use futures_util::FutureExt;

use super::{
    DirectRewriteResult, RewriteBackend, RewriteControl, RewriteError, RewriteErrorKind,
    RewriteObserver, RewriteOutcome, RewriteOutcomeKind, RewritePipelineEvent, SelectionRewriter,
    SelectionSnapshot,
};

pub struct RewriteCoordinator {
    backend: Arc<dyn RewriteBackend>,
}

impl RewriteCoordinator {
    pub fn new(backend: Arc<dyn RewriteBackend>) -> Self {
        Self { backend }
    }
}

#[async_trait]
impl SelectionRewriter for RewriteCoordinator {
    fn capture_selection(&self) -> Result<SelectionSnapshot, RewriteError> {
        self.backend.capture_selection()
    }

    async fn replace(
        &self,
        snapshot: SelectionSnapshot,
        replacement: String,
        control: RewriteControl,
        observer: Arc<dyn RewriteObserver>,
    ) -> RewriteOutcome {
        let backend = self.backend.clone();
        let target_name = snapshot.target_name();
        let result = AssertUnwindSafe(tokio::task::spawn_blocking(move || {
            run_replacement(
                backend.as_ref(),
                &snapshot,
                &replacement,
                &control,
                observer.as_ref(),
            )
        }))
        .catch_unwind()
        .await;

        match result {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(join_error)) => failed(
                target_name,
                RewriteError::new(
                    RewriteErrorKind::RuntimeFailed,
                    format!("rewrite worker could not be joined: {join_error}"),
                ),
                0,
            ),
            Err(_) => failed(
                target_name,
                RewriteError::new(RewriteErrorKind::RuntimeFailed, "rewrite worker panicked"),
                0,
            ),
        }
    }
}

fn run_replacement(
    backend: &dyn RewriteBackend,
    snapshot: &SelectionSnapshot,
    replacement: &str,
    control: &RewriteControl,
    observer: &dyn RewriteObserver,
) -> RewriteOutcome {
    let started = Instant::now();
    let target_name = snapshot.target_name();
    if control.is_cancelled() {
        return failed(
            target_name,
            RewriteError::new(RewriteErrorKind::Cancelled, "cancelled before revalidation"),
            elapsed_ms(started),
        );
    }
    observer.event(RewritePipelineEvent::RevalidationStarted);
    if let Err(error) = backend.revalidate_selection(snapshot) {
        observer.event(RewritePipelineEvent::RevalidationFailed);
        return failed(target_name, error, elapsed_ms(started));
    }
    observer.event(RewritePipelineEvent::RevalidationSucceeded);
    if control.is_cancelled() {
        return failed(
            target_name,
            RewriteError::new(RewriteErrorKind::Cancelled, "cancelled before replacement"),
            elapsed_ms(started),
        );
    }
    observer.event(RewritePipelineEvent::ReplaceRequested);

    match backend.replace_directly(snapshot, replacement) {
        Ok(DirectRewriteResult::Applied) => {
            observer.event(RewritePipelineEvent::ReplaceCompleted);
            return RewriteOutcome {
                kind: RewriteOutcomeKind::RewrittenDirectly,
                target_application: target_name,
                duration_ms: elapsed_ms(started),
                clipboard_restored: false,
                fallback_used: false,
                error: None,
            };
        }
        Err(error) => return failed(target_name, error, elapsed_ms(started)),
        Ok(DirectRewriteResult::Unsupported | DirectRewriteResult::ProvenNoMutation) => {}
    }

    // Paste is permitted only while the same exact selection is still active.
    // A second validation follows a proven direct no-op and precedes touching
    // the shared pasteboard.
    observer.event(RewritePipelineEvent::RevalidationStarted);
    if let Err(error) = backend.revalidate_selection(snapshot) {
        observer.event(RewritePipelineEvent::RevalidationFailed);
        return failed(target_name, error, elapsed_ms(started));
    }
    observer.event(RewritePipelineEvent::RevalidationSucceeded);
    if control.is_cancelled() {
        return failed(
            target_name,
            RewriteError::new(
                RewriteErrorKind::Cancelled,
                "cancelled before paste fallback",
            ),
            elapsed_ms(started),
        );
    }

    let prior_clipboard = match backend.capture_clipboard() {
        Ok(snapshot) if snapshot.safely_restorable => snapshot,
        Ok(_) => {
            return failed(
                target_name,
                RewriteError::new(
                    RewriteErrorKind::ClipboardUnavailable,
                    "the current clipboard contains data this plain-text fallback cannot restore",
                ),
                elapsed_ms(started),
            );
        }
        Err(error) => return failed(target_name, error, elapsed_ms(started)),
    };
    let change_count = match backend.write_clipboard(replacement) {
        Ok(change_count) => change_count,
        Err(error) => return failed(target_name, error, elapsed_ms(started)),
    };

    if let Err(error) = backend.dispatch_paste(snapshot) {
        if matches!(
            error.kind,
            RewriteErrorKind::TargetApplicationChanged
                | RewriteErrorKind::TargetElementChanged
                | RewriteErrorKind::SelectionRangeChanged
                | RewriteErrorKind::SelectionTextChanged
                | RewriteErrorKind::SelectionMissing
        ) {
            observer.event(RewritePipelineEvent::RevalidationFailed);
        }
        let _ = backend.restore_clipboard(&prior_clipboard, change_count);
        return failed(target_name, error, elapsed_ms(started));
    }

    std::thread::sleep(backend.restoration_delay());
    let (clipboard_restored, restoration_error) =
        match backend.restore_clipboard(&prior_clipboard, change_count) {
            Ok(restored) => (restored, None),
            Err(error) => (false, Some(error)),
        };
    observer.event(RewritePipelineEvent::ReplaceCompleted);
    RewriteOutcome {
        kind: RewriteOutcomeKind::RewrittenViaPaste,
        target_application: target_name,
        duration_ms: elapsed_ms(started),
        clipboard_restored,
        fallback_used: true,
        error: restoration_error,
    }
}

fn failed(
    target_application: Option<String>,
    error: RewriteError,
    duration_ms: u64,
) -> RewriteOutcome {
    let kind = if error.kind == RewriteErrorKind::Cancelled {
        RewriteOutcomeKind::RewriteCancelled
    } else {
        RewriteOutcomeKind::RewriteFailed
    };
    RewriteOutcome {
        kind,
        target_application,
        duration_ms,
        clipboard_restored: false,
        fallback_used: false,
        error: Some(error),
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, sync::Mutex, time::Duration};

    use super::*;
    use crate::insertion::ClipboardSnapshot;

    #[derive(Clone)]
    struct Plan {
        validations: VecDeque<Result<(), RewriteError>>,
        direct: Result<DirectRewriteResult, RewriteError>,
        clipboard: Result<ClipboardSnapshot, RewriteError>,
        write: Result<u64, RewriteError>,
        paste: Result<(), RewriteError>,
        restore: Result<bool, RewriteError>,
    }

    impl Default for Plan {
        fn default() -> Self {
            Self {
                validations: VecDeque::from([Ok(()), Ok(())]),
                direct: Ok(DirectRewriteResult::Applied),
                clipboard: Ok(ClipboardSnapshot {
                    plain_text: Some("prior".to_owned()),
                    was_empty: false,
                    safely_restorable: true,
                }),
                write: Ok(4),
                paste: Ok(()),
                restore: Ok(true),
            }
        }
    }

    struct MockBackend {
        plan: Mutex<Plan>,
        calls: Mutex<Vec<String>>,
    }

    impl MockBackend {
        fn new(plan: Plan) -> Self {
            Self {
                plan: Mutex::new(plan),
                calls: Mutex::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<String> {
            self.calls.lock().expect("calls").clone()
        }

        fn record(&self, call: &str) {
            self.calls.lock().expect("calls").push(call.to_owned());
        }
    }

    impl RewriteBackend for MockBackend {
        fn capture_selection(&self) -> Result<SelectionSnapshot, RewriteError> {
            Ok(selection())
        }

        fn revalidate_selection(&self, _: &SelectionSnapshot) -> Result<(), RewriteError> {
            self.record("validate");
            self.plan
                .lock()
                .expect("plan")
                .validations
                .pop_front()
                .unwrap_or(Ok(()))
        }

        fn replace_directly(
            &self,
            _: &SelectionSnapshot,
            replacement: &str,
        ) -> Result<DirectRewriteResult, RewriteError> {
            self.record(&format!("direct:{replacement}"));
            self.plan.lock().expect("plan").direct.clone()
        }

        fn capture_clipboard(&self) -> Result<ClipboardSnapshot, RewriteError> {
            self.record("capture_clipboard");
            self.plan.lock().expect("plan").clipboard.clone()
        }

        fn write_clipboard(&self, text: &str) -> Result<u64, RewriteError> {
            self.record(&format!("write:{text}"));
            self.plan.lock().expect("plan").write.clone()
        }

        fn dispatch_paste(&self, _: &SelectionSnapshot) -> Result<(), RewriteError> {
            self.record("paste");
            self.plan.lock().expect("plan").paste.clone()
        }

        fn restore_clipboard(&self, _: &ClipboardSnapshot, _: u64) -> Result<bool, RewriteError> {
            self.record("restore");
            self.plan.lock().expect("plan").restore.clone()
        }

        fn restoration_delay(&self) -> Duration {
            Duration::ZERO
        }
    }

    struct NoopObserver;
    impl RewriteObserver for NoopObserver {
        fn event(&self, _: RewritePipelineEvent) {}
    }

    fn selection() -> SelectionSnapshot {
        SelectionSnapshot {
            process_id: 7,
            application_name: Some("Writer".to_owned()),
            bundle_id: Some("com.example.writer".to_owned()),
            element_fingerprint: 9,
            element_identifier: Some("editor".to_owned()),
            role: Some("AXTextArea".to_owned()),
            subrole: None,
            range: super::super::Utf16Range {
                location: 2,
                length: 5,
            },
            selected_text: "Kael 👋".to_owned(),
        }
    }

    async fn run(backend: Arc<MockBackend>) -> RewriteOutcome {
        RewriteCoordinator::new(backend)
            .replace(
                selection(),
                "Rewritten".to_owned(),
                RewriteControl::default(),
                Arc::new(NoopObserver),
            )
            .await
    }

    #[tokio::test]
    async fn direct_success_mutates_once_and_never_touches_clipboard() {
        let backend = Arc::new(MockBackend::new(Plan::default()));
        let outcome = run(backend.clone()).await;
        assert_eq!(outcome.kind, RewriteOutcomeKind::RewrittenDirectly);
        assert_eq!(backend.calls(), ["validate", "direct:Rewritten"]);
    }

    #[tokio::test]
    async fn proven_direct_noop_uses_one_guarded_paste_and_restores_clipboard() {
        let backend = Arc::new(MockBackend::new(Plan {
            direct: Ok(DirectRewriteResult::ProvenNoMutation),
            ..Plan::default()
        }));
        let outcome = run(backend.clone()).await;
        assert_eq!(outcome.kind, RewriteOutcomeKind::RewrittenViaPaste);
        assert!(outcome.clipboard_restored);
        assert_eq!(
            backend.calls(),
            [
                "validate",
                "direct:Rewritten",
                "validate",
                "capture_clipboard",
                "write:Rewritten",
                "paste",
                "restore",
            ]
        );
    }

    #[tokio::test]
    async fn changed_target_never_mutates_document_or_clipboard() {
        let backend = Arc::new(MockBackend::new(Plan {
            validations: VecDeque::from([Err(RewriteError::new(
                RewriteErrorKind::SelectionTextChanged,
                "changed",
            ))]),
            ..Plan::default()
        }));
        let outcome = run(backend.clone()).await;
        assert_eq!(outcome.kind, RewriteOutcomeKind::RewriteFailed);
        assert_eq!(backend.calls(), ["validate"]);
    }

    #[tokio::test]
    async fn change_after_direct_noop_stops_before_clipboard() {
        let backend = Arc::new(MockBackend::new(Plan {
            validations: VecDeque::from([
                Ok(()),
                Err(RewriteError::new(
                    RewriteErrorKind::SelectionRangeChanged,
                    "changed",
                )),
            ]),
            direct: Ok(DirectRewriteResult::ProvenNoMutation),
            ..Plan::default()
        }));
        let outcome = run(backend.clone()).await;
        assert_eq!(outcome.kind, RewriteOutcomeKind::RewriteFailed);
        assert_eq!(
            backend.calls(),
            ["validate", "direct:Rewritten", "validate"]
        );
    }

    #[tokio::test]
    async fn unpreservable_clipboard_disables_paste_fallback() {
        let backend = Arc::new(MockBackend::new(Plan {
            direct: Ok(DirectRewriteResult::Unsupported),
            clipboard: Ok(ClipboardSnapshot {
                plain_text: None,
                was_empty: false,
                safely_restorable: false,
            }),
            ..Plan::default()
        }));
        let outcome = run(backend.clone()).await;
        assert_eq!(outcome.kind, RewriteOutcomeKind::RewriteFailed);
        assert_eq!(
            backend.calls(),
            [
                "validate",
                "direct:Rewritten",
                "validate",
                "capture_clipboard"
            ]
        );
    }

    #[tokio::test]
    async fn paste_revalidation_failure_restores_clipboard_and_never_retries() {
        let backend = Arc::new(MockBackend::new(Plan {
            direct: Ok(DirectRewriteResult::Unsupported),
            paste: Err(RewriteError::new(
                RewriteErrorKind::SelectionTextChanged,
                "changed immediately before paste",
            )),
            ..Plan::default()
        }));
        let outcome = run(backend.clone()).await;
        assert_eq!(outcome.kind, RewriteOutcomeKind::RewriteFailed);
        assert_eq!(
            backend
                .calls()
                .iter()
                .filter(|call| *call == "paste")
                .count(),
            1
        );
        assert_eq!(backend.calls().last().map(String::as_str), Some("restore"));
    }

    #[tokio::test]
    async fn pre_cancelled_rewrite_never_validates_or_mutates() {
        let backend = Arc::new(MockBackend::new(Plan::default()));
        let control = RewriteControl::default();
        control.cancel();
        let outcome = RewriteCoordinator::new(backend.clone())
            .replace(
                selection(),
                "Rewritten".to_owned(),
                control,
                Arc::new(NoopObserver),
            )
            .await;
        assert_eq!(outcome.kind, RewriteOutcomeKind::RewriteCancelled);
        assert!(backend.calls().is_empty());
    }
}
