use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::sync::Notify;
use trueflow_lib::transcription::{
    AudioEncoding, BoundedAudioQueue, CoordinatorObserver, SessionConfig, SessionTimeouts,
    TranscriptAccumulator, TranscriptionError, TranscriptionEvent, TranscriptionOutcome,
    mock::{MockSessionPlan, MockTranscriptionProvider, ScriptedEvent},
    spawn_transcription,
};

#[derive(Default)]
struct Observer {
    partials: Mutex<Vec<String>>,
    terminal: Mutex<Option<Result<TranscriptionOutcome, TranscriptionError>>>,
    changed: Notify,
}

impl CoordinatorObserver for Observer {
    fn connection_ready(&self, _retry_count: u32) {}

    fn first_audio_sent(&self) {}

    fn provider_event(&self, event: &TranscriptionEvent, _transcript: &TranscriptAccumulator) {
        if let TranscriptionEvent::PartialTranscript { text } = event {
            self.partials
                .lock()
                .expect("partial lock")
                .push(text.clone());
        }
    }

    fn completed(&self, outcome: TranscriptionOutcome) {
        *self.terminal.lock().expect("terminal lock") = Some(Ok(outcome));
        self.changed.notify_waiters();
    }

    fn failed(&self, error: TranscriptionError, _outcome: TranscriptionOutcome) {
        *self.terminal.lock().expect("terminal lock") = Some(Err(error));
        self.changed.notify_waiters();
    }
}

fn config(operation_id: &str) -> SessionConfig {
    SessionConfig {
        operation_id: operation_id.to_owned(),
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
        connection: Duration::from_millis(100),
        audio_send: Duration::from_millis(100),
        finalization: Duration::from_millis(200),
        close: Duration::from_millis(100),
    }
}

fn success_plan(text: &str) -> MockSessionPlan {
    MockSessionPlan {
        events: vec![
            ScriptedEvent::immediate(TranscriptionEvent::PartialTranscript {
                text: "revisable partial".to_owned(),
            }),
            ScriptedEvent::after_finish(TranscriptionEvent::FinalTranscript {
                text: text.to_owned(),
                operation_final: true,
            }),
            ScriptedEvent::after_finish(TranscriptionEvent::SessionClosed),
        ],
        ..MockSessionPlan::default()
    }
}

async fn wait_terminal(observer: &Observer) -> Result<TranscriptionOutcome, TranscriptionError> {
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let changed = observer.changed.notified();
            if let Some(result) = observer.terminal.lock().expect("terminal lock").clone() {
                return result;
            }
            changed.await;
        }
    })
    .await
    .expect("pipeline reached a terminal result")
}

#[tokio::test]
async fn press_audio_partial_release_finalize_flow_is_operation_scoped() {
    let provider = Arc::new(MockTranscriptionProvider::new([success_plan(
        "reliable final transcript",
    )]));
    let audio = BoundedAudioQueue::new(32);
    let observer = Arc::new(Observer::default());
    let control = spawn_transcription(
        provider.clone(),
        config("operation-one"),
        timeouts(),
        audio.clone(),
        observer.clone(),
    );
    audio
        .try_push(Arc::from([10_i16, 20, 30]))
        .expect("audio queued");
    control.finish();

    let outcome = wait_terminal(&observer).await.expect("successful final");
    assert_eq!(outcome.text, "reliable final transcript");
    assert!(outcome.is_final);
    assert_eq!(provider.records()[0].sent_samples, vec![10, 20, 30]);
    assert_eq!(
        *observer.partials.lock().expect("partials"),
        vec!["revisable partial"]
    );
}

#[tokio::test]
async fn failed_operation_cleans_up_and_subsequent_operation_succeeds() {
    let failure = || MockSessionPlan {
        startup_error: Some(TranscriptionError::ConnectionFailed("offline".to_owned())),
        ..MockSessionPlan::default()
    };
    let provider = Arc::new(MockTranscriptionProvider::new([
        failure(),
        failure(),
        success_plan("second operation works"),
    ]));

    let first_observer = Arc::new(Observer::default());
    let first = spawn_transcription(
        provider.clone(),
        config("failed-operation"),
        timeouts(),
        BoundedAudioQueue::new(16),
        first_observer.clone(),
    );
    first.finish();
    assert!(matches!(
        wait_terminal(&first_observer).await,
        Err(TranscriptionError::ConnectionFailed(_))
    ));

    let second_observer = Arc::new(Observer::default());
    let second = spawn_transcription(
        provider,
        config("next-operation"),
        timeouts(),
        BoundedAudioQueue::new(16),
        second_observer.clone(),
    );
    second.finish();
    let outcome = wait_terminal(&second_observer)
        .await
        .expect("subsequent operation succeeds");
    assert_eq!(outcome.text, "second operation works");
}
