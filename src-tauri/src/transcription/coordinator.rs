use std::{panic::AssertUnwindSafe, sync::Arc, time::Duration};

use futures_util::FutureExt;
use tokio::{sync::mpsc, time::timeout};
use tracing::{info, warn};

use super::{
    BoundedAudioQueue, Retryability, SessionConfig, SessionParts, SessionTimeouts, SharedProvider,
    TranscriptAccumulator, TranscriptionError, TranscriptionEvent,
};

const MAX_CONNECTION_ATTEMPTS: u32 = 2;
const RETRY_BACKOFF: Duration = Duration::from_millis(150);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TranscriptionOutcome {
    pub text: String,
    pub is_final: bool,
    pub degraded: bool,
}

pub trait CoordinatorObserver: Send + Sync {
    fn connection_ready(&self, retry_count: u32);
    fn first_audio_sent(&self);
    fn provider_event(&self, event: &TranscriptionEvent, transcript: &TranscriptAccumulator);
    fn completed(&self, outcome: TranscriptionOutcome);
    fn failed(&self, error: TranscriptionError, outcome: TranscriptionOutcome);
}

enum CoordinatorCommand {
    Finish,
    Cancel,
}

pub struct TranscriptionControl {
    command: mpsc::Sender<CoordinatorCommand>,
    audio: Arc<BoundedAudioQueue>,
}

impl TranscriptionControl {
    pub fn finish(&self) {
        self.audio.close();
        let _ = self.command.try_send(CoordinatorCommand::Finish);
    }

    pub fn cancel(&self) {
        self.audio.clear();
        let _ = self.command.try_send(CoordinatorCommand::Cancel);
    }
}

pub fn spawn_transcription(
    provider: SharedProvider,
    config: SessionConfig,
    timeouts: SessionTimeouts,
    audio: Arc<BoundedAudioQueue>,
    observer: Arc<dyn CoordinatorObserver>,
) -> TranscriptionControl {
    let (command, command_rx) = mpsc::channel(2);
    let task_audio = audio.clone();
    let panic_audio = audio.clone();
    let panic_observer = observer.clone();
    tauri::async_runtime::spawn(async move {
        let result = AssertUnwindSafe(run_transcription(
            provider, config, timeouts, task_audio, command_rx, observer,
        ))
        .catch_unwind()
        .await;
        if result.is_err() {
            panic_audio.clear();
            panic_observer.failed(TranscriptionError::RuntimeFailed, empty_outcome());
        }
    });
    TranscriptionControl { command, audio }
}

async fn run_transcription(
    provider: SharedProvider,
    config: SessionConfig,
    timeouts: SessionTimeouts,
    audio: Arc<BoundedAudioQueue>,
    mut commands: mpsc::Receiver<CoordinatorCommand>,
    observer: Arc<dyn CoordinatorObserver>,
) {
    let operation_id = config.operation_id.clone();
    let (mut session, retry_count, finish_requested) =
        match connect_with_retry(&provider, config, &mut commands).await {
            Ok(connected) => connected,
            Err(error) => {
                audio.clear();
                observer.failed(error, empty_outcome());
                return;
            }
        };
    observer.connection_ready(retry_count);
    info!(
        operation_id,
        subsystem = "transcription",
        event = "provider_connection_ready",
        provider = provider.name(),
        retry_count,
        "streaming transcription connected"
    );

    let mut transcript = TranscriptAccumulator::default();
    let mut first_audio_sent = false;
    let mut finish_requested = finish_requested;
    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(CoordinatorCommand::Finish) => finish_requested = true,
                Some(CoordinatorCommand::Cancel) | None => {
                    let _ = timeout(timeouts.close, session.input.cancel()).await;
                    audio.clear();
                    observer.failed(TranscriptionError::Cancelled, outcome(&transcript, true));
                    return;
                }
            },
            chunk = audio.pop() => {
                match chunk {
                    Some(chunk) => {
                        let sent = timeout(timeouts.audio_send, session.input.send_audio(chunk)).await;
                        let result = match sent {
                            Ok(result) => result,
                            Err(_) => Err(TranscriptionError::ConnectionTimeout),
                        };
                        if let Err(error) = result {
                            fail_session(&mut session, &audio, &observer, &transcript, error, timeouts.close).await;
                            return;
                        }
                        if !first_audio_sent {
                            first_audio_sent = true;
                            observer.first_audio_sent();
                        }
                    }
                    None if finish_requested => break,
                    None => {}
                }
            },
            event = session.events.next_event() => {
                match handle_event(event, &mut transcript, observer.as_ref()) {
                    EventAction::Continue => {}
                    EventAction::Failed(error) => {
                        fail_session(&mut session, &audio, &observer, &transcript, error, timeouts.close).await;
                        return;
                    }
                    EventAction::Closed => {
                        fail_session(
                            &mut session,
                            &audio,
                            &observer,
                            &transcript,
                            TranscriptionError::SessionClosedUnexpectedly,
                            timeouts.close,
                        ).await;
                        return;
                    }
                }
            }
        }
    }

    if let Err(error) = session.input.finish().await {
        fail_session(
            &mut session,
            &audio,
            &observer,
            &transcript,
            error,
            timeouts.close,
        )
        .await;
        return;
    }

    let final_result = timeout(timeouts.finalization, async {
        loop {
            let event = session.events.next_event().await?;
            match handle_event(Ok(event), &mut transcript, observer.as_ref()) {
                EventAction::Continue if transcript.is_operation_final() => return Ok(()),
                EventAction::Continue => {}
                EventAction::Failed(error) => return Err(error),
                EventAction::Closed if transcript.is_operation_final() => return Ok(()),
                EventAction::Closed => {
                    return Err(TranscriptionError::SessionClosedUnexpectedly);
                }
            }
        }
    })
    .await;

    match final_result {
        Err(_) => {
            fail_session(
                &mut session,
                &audio,
                &observer,
                &transcript,
                TranscriptionError::FinalizationTimeout,
                timeouts.close,
            )
            .await;
        }
        Ok(Err(error)) => {
            fail_session(
                &mut session,
                &audio,
                &observer,
                &transcript,
                error,
                timeouts.close,
            )
            .await;
        }
        Ok(Ok(())) => {
            // The provider has already delivered its final transcript, so the
            // operation is complete regardless of how the socket winds down.
            // Reporting completion before the close handshake removes it from
            // the user-visible path; it was measured at 738 ms of post-release
            // latency, roughly a third of the release-to-insert time, spent
            // holding a finished transcript while waiting on socket teardown.
            observer.completed(outcome(&transcript, false));

            let close_result = timeout(timeouts.close, async {
                session.input.close().await?;
                loop {
                    // Events arriving during teardown cannot change an
                    // operation-final transcript that has already been
                    // reported, so they are drained without republishing.
                    if session.events.next_event().await? == TranscriptionEvent::SessionClosed {
                        return Ok::<(), TranscriptionError>(());
                    }
                }
            })
            .await;
            match close_result {
                Err(_) => warn!(
                    operation_id,
                    subsystem = "transcription",
                    event = "close_after_completion_failed",
                    error_code = TranscriptionError::CloseTimeout.code(),
                    "provider session did not close cleanly after the final transcript"
                ),
                Ok(Err(error)) => warn!(
                    operation_id,
                    subsystem = "transcription",
                    event = "close_after_completion_failed",
                    error_code = error.code(),
                    "provider session did not close cleanly after the final transcript"
                ),
                Ok(Ok(())) => {}
            }
        }
    }
}

async fn connect_with_retry(
    provider: &SharedProvider,
    config: SessionConfig,
    commands: &mut mpsc::Receiver<CoordinatorCommand>,
) -> Result<(SessionParts, u32, bool), TranscriptionError> {
    let mut finish_requested = false;
    for attempt in 1..=MAX_CONNECTION_ATTEMPTS {
        let connection = provider.start_session(config.clone());
        tokio::pin!(connection);
        let result = loop {
            tokio::select! {
                command = commands.recv() => match command {
                    Some(CoordinatorCommand::Cancel) | None => {
                        return Err(TranscriptionError::Cancelled);
                    }
                    Some(CoordinatorCommand::Finish) => {
                        finish_requested = true;
                    }
                },
                result = &mut connection => break result,
            }
        };
        match result {
            Ok(session) => return Ok((session, attempt - 1, finish_requested)),
            Err(error)
                if attempt < MAX_CONNECTION_ATTEMPTS
                    && error.retryability() == Retryability::Retryable =>
            {
                warn!(
                    operation_id = config.operation_id,
                    subsystem = "transcription",
                    event = "connection_retry",
                    provider = provider.name(),
                    retry_count = attempt,
                    error_code = error.code(),
                    "retrying transient transcription connection failure"
                );
                tokio::select! {
                    () = tokio::time::sleep(RETRY_BACKOFF) => {}
                    command = commands.recv() => match command {
                        Some(CoordinatorCommand::Cancel) | None => return Err(TranscriptionError::Cancelled),
                        Some(CoordinatorCommand::Finish) => finish_requested = true,
                    }
                }
            }
            Err(error) => return Err(error),
        }
    }
    Err(TranscriptionError::ConnectionFailed(
        "connection retry budget exhausted".to_owned(),
    ))
}

enum EventAction {
    Continue,
    Failed(TranscriptionError),
    Closed,
}

fn handle_event(
    event: Result<TranscriptionEvent, TranscriptionError>,
    transcript: &mut TranscriptAccumulator,
    observer: &dyn CoordinatorObserver,
) -> EventAction {
    let event = match event {
        Ok(event) => event,
        Err(error) => return EventAction::Failed(error),
    };
    match &event {
        TranscriptionEvent::PartialTranscript { text } => {
            transcript.replace_partial(text.clone());
        }
        TranscriptionEvent::FinalTranscript {
            text,
            operation_final,
        } => transcript.add_final_segment(text.clone(), *operation_final),
        TranscriptionEvent::ProviderError(error) => return EventAction::Failed(error.clone()),
        TranscriptionEvent::SessionClosed => return EventAction::Closed,
        TranscriptionEvent::SessionStarted { .. }
        | TranscriptionEvent::SpeechStarted
        | TranscriptionEvent::SpeechEnded
        | TranscriptionEvent::ProviderWarning { .. } => {}
    }
    observer.provider_event(&event, transcript);
    EventAction::Continue
}

async fn fail_session(
    session: &mut SessionParts,
    audio: &BoundedAudioQueue,
    observer: &Arc<dyn CoordinatorObserver>,
    transcript: &TranscriptAccumulator,
    error: TranscriptionError,
    close_timeout: Duration,
) {
    audio.clear();
    let _ = timeout(close_timeout, session.input.cancel()).await;
    observer.failed(error, outcome(transcript, true));
}

fn outcome(transcript: &TranscriptAccumulator, degraded: bool) -> TranscriptionOutcome {
    TranscriptionOutcome {
        text: transcript.best_known_text(),
        is_final: transcript.is_operation_final(),
        degraded,
    }
}

fn empty_outcome() -> TranscriptionOutcome {
    TranscriptionOutcome {
        text: String::new(),
        is_final: false,
        degraded: true,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    use async_trait::async_trait;
    use tokio::sync::mpsc;

    use super::{CoordinatorCommand, CoordinatorObserver, TranscriptionOutcome, run_transcription};
    use crate::transcription::{
        AudioEncoding, BoundedAudioQueue, SessionConfig, SessionParts, SessionTimeouts,
        TranscriptAccumulator, TranscriptionError, TranscriptionEvent, TranscriptionProvider,
        mock::{MockSessionPlan, MockTranscriptionProvider, ScriptedEvent},
    };

    #[derive(Default)]
    struct TestObserver {
        events: Mutex<Vec<TranscriptionEvent>>,
        completed: Mutex<Option<TranscriptionOutcome>>,
        failed: Mutex<Option<(TranscriptionError, TranscriptionOutcome)>>,
        connection_retries: Mutex<Vec<u32>>,
        first_audio_count: Mutex<usize>,
    }

    impl CoordinatorObserver for TestObserver {
        fn connection_ready(&self, retry_count: u32) {
            self.connection_retries
                .lock()
                .expect("retry lock")
                .push(retry_count);
        }

        fn first_audio_sent(&self) {
            let mut count = self.first_audio_count.lock().expect("audio count lock");
            *count = count.saturating_add(1);
        }

        fn provider_event(&self, event: &TranscriptionEvent, _transcript: &TranscriptAccumulator) {
            self.events.lock().expect("event lock").push(event.clone());
        }

        fn completed(&self, outcome: TranscriptionOutcome) {
            *self.completed.lock().expect("completed lock") = Some(outcome);
        }

        fn failed(&self, error: TranscriptionError, outcome: TranscriptionOutcome) {
            *self.failed.lock().expect("failed lock") = Some((error, outcome));
        }
    }

    fn config() -> SessionConfig {
        SessionConfig {
            operation_id: "operation-test".to_owned(),
            sample_rate: 16_000,
            channels: 1,
            encoding: AudioEncoding::Linear16,
            language: "en-US".to_owned(),
            model: "mock".to_owned(),
            vocabulary: Default::default(),
        }
    }

    fn timeouts() -> SessionTimeouts {
        SessionTimeouts {
            connection: Duration::from_millis(50),
            audio_send: Duration::from_millis(50),
            finalization: Duration::from_millis(60),
            close: Duration::from_millis(50),
        }
    }

    fn success_plan() -> MockSessionPlan {
        MockSessionPlan {
            events: vec![
                ScriptedEvent::immediate(TranscriptionEvent::SessionStarted {
                    provider_session_id: Some("mock".to_owned()),
                }),
                ScriptedEvent::immediate(TranscriptionEvent::PartialTranscript {
                    text: "I think".to_owned(),
                }),
                ScriptedEvent::immediate(TranscriptionEvent::PartialTranscript {
                    text: "I think we should".to_owned(),
                }),
                ScriptedEvent::after_finish(TranscriptionEvent::FinalTranscript {
                    text: "I think we should wait.".to_owned(),
                    operation_final: true,
                }),
                ScriptedEvent::after_finish(TranscriptionEvent::SessionClosed),
            ],
            ..MockSessionPlan::default()
        }
    }

    async fn run_plan(
        provider: Arc<MockTranscriptionProvider>,
        audio: Arc<BoundedAudioQueue>,
        observer: Arc<TestObserver>,
        command: CoordinatorCommand,
    ) {
        let (commands, command_rx) = mpsc::channel(2);
        let task = tokio::spawn(run_transcription(
            provider,
            config(),
            timeouts(),
            audio.clone(),
            command_rx,
            observer,
        ));
        tokio::task::yield_now().await;
        match command {
            CoordinatorCommand::Finish => audio.close(),
            CoordinatorCommand::Cancel => audio.clear(),
        }
        let _ = commands.send(command).await;
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("coordinator finishes")
            .expect("coordinator task succeeds");
    }

    #[tokio::test]
    async fn streams_audio_in_order_and_replaces_revised_partials() {
        let provider = Arc::new(MockTranscriptionProvider::new([success_plan()]));
        let audio = BoundedAudioQueue::new(16);
        audio.try_push(Arc::from([1_i16, 2])).expect("first audio");
        audio.try_push(Arc::from([3_i16])).expect("second audio");
        let observer = Arc::new(TestObserver::default());

        run_plan(
            provider.clone(),
            audio,
            observer.clone(),
            CoordinatorCommand::Finish,
        )
        .await;

        assert_eq!(provider.records()[0].sent_sequences, vec![0, 1]);
        assert_eq!(provider.records()[0].sent_samples, vec![1, 2, 3]);
        assert_eq!(
            observer.completed.lock().expect("completed").clone(),
            Some(TranscriptionOutcome {
                text: "I think we should wait.".to_owned(),
                is_final: true,
                degraded: false,
            })
        );
        assert_eq!(*observer.first_audio_count.lock().expect("audio count"), 1);
    }

    #[tokio::test]
    async fn retains_early_audio_during_delayed_connection() {
        let mut plan = success_plan();
        plan.connection_delay = Duration::from_millis(30);
        let provider = Arc::new(MockTranscriptionProvider::new([plan]));
        let audio = BoundedAudioQueue::new(8);
        audio
            .try_push(Arc::from([7_i16, 8, 9]))
            .expect("early audio buffered");

        run_plan(
            provider.clone(),
            audio,
            Arc::new(TestObserver::default()),
            CoordinatorCommand::Finish,
        )
        .await;

        assert_eq!(provider.records()[0].sent_samples, vec![7, 8, 9]);
    }

    #[tokio::test]
    async fn retries_one_transient_startup_failure_only() {
        let provider = Arc::new(MockTranscriptionProvider::new([
            MockSessionPlan {
                startup_error: Some(TranscriptionError::ConnectionFailed("temporary".to_owned())),
                ..MockSessionPlan::default()
            },
            success_plan(),
        ]));
        let observer = Arc::new(TestObserver::default());

        run_plan(
            provider.clone(),
            BoundedAudioQueue::new(8),
            observer.clone(),
            CoordinatorCommand::Finish,
        )
        .await;

        assert_eq!(provider.starts(), 2);
        assert_eq!(
            *observer.connection_retries.lock().expect("retries"),
            vec![1]
        );
    }

    #[tokio::test]
    async fn cancellation_closes_without_finalizing() {
        let provider = Arc::new(MockTranscriptionProvider::new([success_plan()]));
        let observer = Arc::new(TestObserver::default());

        run_plan(
            provider,
            BoundedAudioQueue::new(8),
            observer.clone(),
            CoordinatorCommand::Cancel,
        )
        .await;

        assert!(matches!(
            observer.failed.lock().expect("failure").as_ref(),
            Some((TranscriptionError::Cancelled, _))
        ));
    }

    #[tokio::test]
    async fn finalization_timeout_preserves_usable_partial() {
        let provider = Arc::new(MockTranscriptionProvider::new([MockSessionPlan {
            events: vec![
                ScriptedEvent::immediate(TranscriptionEvent::PartialTranscript {
                    text: "usable words".to_owned(),
                }),
                ScriptedEvent {
                    delay: Duration::from_secs(5),
                    after_finish: true,
                    result: Ok(TranscriptionEvent::FinalTranscript {
                        text: "late".to_owned(),
                        operation_final: true,
                    }),
                },
            ],
            ..MockSessionPlan::default()
        }]));
        let observer = Arc::new(TestObserver::default());

        run_plan(
            provider,
            BoundedAudioQueue::new(8),
            observer.clone(),
            CoordinatorCommand::Finish,
        )
        .await;

        let failure = observer.failed.lock().expect("failure").clone();
        assert_eq!(
            failure,
            Some((
                TranscriptionError::FinalizationTimeout,
                TranscriptionOutcome {
                    text: "usable words".to_owned(),
                    is_final: false,
                    degraded: true,
                },
            ))
        );
    }

    #[tokio::test]
    async fn rate_limit_is_bounded_and_normalized() {
        let rate_limited = || MockSessionPlan {
            startup_error: Some(TranscriptionError::RateLimited),
            ..MockSessionPlan::default()
        };
        let provider = Arc::new(MockTranscriptionProvider::new([
            rate_limited(),
            rate_limited(),
        ]));
        let observer = Arc::new(TestObserver::default());

        run_plan(
            provider.clone(),
            BoundedAudioQueue::new(8),
            observer.clone(),
            CoordinatorCommand::Finish,
        )
        .await;

        assert_eq!(provider.starts(), 2);
        assert!(matches!(
            observer.failed.lock().expect("failure").as_ref(),
            Some((TranscriptionError::RateLimited, _))
        ));
    }

    #[tokio::test]
    async fn provider_warning_does_not_corrupt_the_final_transcript() {
        let mut plan = success_plan();
        plan.events.insert(
            1,
            ScriptedEvent::immediate(TranscriptionEvent::ProviderWarning {
                code: "SCRIPTED_WARNING".to_owned(),
            }),
        );
        let observer = Arc::new(TestObserver::default());

        run_plan(
            Arc::new(MockTranscriptionProvider::new([plan])),
            BoundedAudioQueue::new(8),
            observer.clone(),
            CoordinatorCommand::Finish,
        )
        .await;

        assert_eq!(
            observer
                .completed
                .lock()
                .expect("completion")
                .as_ref()
                .map(|outcome| outcome.text.as_str()),
            Some("I think we should wait.")
        );
        assert!(observer.events.lock().expect("events").iter().any(|event| {
            matches!(event, TranscriptionEvent::ProviderWarning { code } if code == "SCRIPTED_WARNING")
        }));
    }

    #[tokio::test]
    async fn unexpected_provider_close_is_normalized() {
        let observer = Arc::new(TestObserver::default());
        run_plan(
            Arc::new(MockTranscriptionProvider::new([MockSessionPlan {
                events: vec![ScriptedEvent::immediate(TranscriptionEvent::SessionClosed)],
                ..MockSessionPlan::default()
            }])),
            BoundedAudioQueue::new(8),
            observer.clone(),
            CoordinatorCommand::Finish,
        )
        .await;

        assert!(matches!(
            observer.failed.lock().expect("failure").as_ref(),
            Some((TranscriptionError::SessionClosedUnexpectedly, _))
        ));
    }

    #[tokio::test]
    async fn midstream_send_failure_preserves_the_latest_partial() {
        let provider = Arc::new(MockTranscriptionProvider::new([MockSessionPlan {
            events: vec![
                ScriptedEvent::immediate(TranscriptionEvent::PartialTranscript {
                    text: "recoverable partial".to_owned(),
                }),
                ScriptedEvent::after_finish(TranscriptionEvent::FinalTranscript {
                    text: "should not arrive".to_owned(),
                    operation_final: true,
                }),
            ],
            send_error_after_chunks: Some(0),
            ..MockSessionPlan::default()
        }]));
        let audio = BoundedAudioQueue::new(8);
        let observer = Arc::new(TestObserver::default());
        let (commands, command_rx) = mpsc::channel(2);
        let task = tokio::spawn(run_transcription(
            provider,
            config(),
            timeouts(),
            audio.clone(),
            command_rx,
            observer.clone(),
        ));

        tokio::time::timeout(Duration::from_secs(1), async {
            while observer.events.lock().expect("events").is_empty() {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .expect("partial arrives");
        audio.try_push(Arc::from([1_i16, 2])).expect("audio queued");
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("coordinator exits")
            .expect("task succeeds");
        drop(commands);

        let failure = observer.failed.lock().expect("failure").clone();
        assert!(matches!(
            failure,
            Some((
                TranscriptionError::ConnectionFailed(_),
                TranscriptionOutcome { ref text, is_final: false, degraded: true }
            )) if text == "recoverable partial"
        ));
    }

    struct PanickingProvider;

    #[async_trait]
    impl TranscriptionProvider for PanickingProvider {
        fn name(&self) -> &'static str {
            "panicking-mock"
        }

        async fn start_session(
            &self,
            _config: SessionConfig,
        ) -> Result<SessionParts, TranscriptionError> {
            panic!("scripted provider panic");
        }
    }

    #[tokio::test]
    async fn provider_task_panic_becomes_a_recoverable_terminal_error() {
        let observer = Arc::new(TestObserver::default());
        let control = super::spawn_transcription(
            Arc::new(PanickingProvider),
            config(),
            timeouts(),
            BoundedAudioQueue::new(8),
            observer.clone(),
        );

        tokio::time::timeout(Duration::from_secs(1), async {
            while observer.failed.lock().expect("failure").is_none() {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .expect("panic is converted to a terminal error");
        assert!(matches!(
            observer.failed.lock().expect("failure").as_ref(),
            Some((TranscriptionError::RuntimeFailed, _))
        ));
        control.cancel();
    }
}
