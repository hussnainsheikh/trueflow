use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use tokio::sync::Notify;

use super::{
    AudioChunk, ProviderVocabularyCapabilities, SessionConfig, SessionParts, TranscriptionError,
    TranscriptionEvent, TranscriptionEvents, TranscriptionInput, TranscriptionProvider,
};

#[derive(Clone, Debug)]
pub struct ScriptedEvent {
    pub delay: Duration,
    pub after_finish: bool,
    pub result: Result<TranscriptionEvent, TranscriptionError>,
}

impl ScriptedEvent {
    pub fn immediate(event: TranscriptionEvent) -> Self {
        Self {
            delay: Duration::ZERO,
            after_finish: false,
            result: Ok(event),
        }
    }

    pub fn after_finish(event: TranscriptionEvent) -> Self {
        Self {
            delay: Duration::ZERO,
            after_finish: true,
            result: Ok(event),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct MockSessionPlan {
    pub connection_delay: Duration,
    pub startup_error: Option<TranscriptionError>,
    pub events: Vec<ScriptedEvent>,
    pub send_error_after_chunks: Option<usize>,
    pub finish_error: Option<TranscriptionError>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MockSessionRecord {
    pub sample_rate: u32,
    pub vocabulary_terms: Vec<String>,
    pub sent_sequences: Vec<u64>,
    pub sent_samples: Vec<i16>,
    pub finish_count: usize,
    pub close_count: usize,
    pub cancel_count: usize,
}

#[derive(Clone, Default)]
pub struct MockTranscriptionProvider {
    plans: Arc<Mutex<VecDeque<MockSessionPlan>>>,
    records: Arc<Mutex<Vec<Arc<Mutex<MockSessionRecord>>>>>,
    starts: Arc<Mutex<usize>>,
    vocabulary_capabilities: ProviderVocabularyCapabilities,
}

impl MockTranscriptionProvider {
    pub fn new(plans: impl IntoIterator<Item = MockSessionPlan>) -> Self {
        Self {
            plans: Arc::new(Mutex::new(plans.into_iter().collect())),
            ..Self::default()
        }
    }

    pub fn records(&self) -> Vec<MockSessionRecord> {
        self.records
            .lock()
            .map(|records| {
                records
                    .iter()
                    .filter_map(|record| record.lock().ok().map(|record| record.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn with_vocabulary_capabilities(
        mut self,
        capabilities: ProviderVocabularyCapabilities,
    ) -> Self {
        self.vocabulary_capabilities = capabilities;
        self
    }

    pub fn starts(&self) -> usize {
        self.starts.lock().map(|starts| *starts).unwrap_or_default()
    }
}

#[async_trait]
impl TranscriptionProvider for MockTranscriptionProvider {
    fn name(&self) -> &'static str {
        "mock"
    }

    fn vocabulary_capabilities(&self) -> ProviderVocabularyCapabilities {
        self.vocabulary_capabilities
    }

    async fn start_session(
        &self,
        config: SessionConfig,
    ) -> Result<SessionParts, TranscriptionError> {
        if let Ok(mut starts) = self.starts.lock() {
            *starts = starts.saturating_add(1);
        }
        let plan = self
            .plans
            .lock()
            .ok()
            .and_then(|mut plans| plans.pop_front())
            .unwrap_or_default();
        if !plan.connection_delay.is_zero() {
            tokio::time::sleep(plan.connection_delay).await;
        }
        if let Some(error) = plan.startup_error {
            return Err(error);
        }

        let record = Arc::new(Mutex::new(MockSessionRecord {
            sample_rate: config.sample_rate,
            vocabulary_terms: config.vocabulary.terms,
            ..MockSessionRecord::default()
        }));
        if let Ok(mut records) = self.records.lock() {
            records.push(record.clone());
        }
        let lifecycle = Arc::new(MockLifecycle::default());
        Ok(SessionParts {
            input: Box::new(MockInput {
                record,
                lifecycle: lifecycle.clone(),
                send_error_after_chunks: plan.send_error_after_chunks,
                finish_error: plan.finish_error,
            }),
            events: Box::new(MockEvents {
                events: plan.events.into(),
                lifecycle,
            }),
        })
    }
}

#[derive(Default)]
struct MockLifecycle {
    finished: Mutex<bool>,
    cancelled: Mutex<bool>,
    changed: Notify,
}

struct MockInput {
    record: Arc<Mutex<MockSessionRecord>>,
    lifecycle: Arc<MockLifecycle>,
    send_error_after_chunks: Option<usize>,
    finish_error: Option<TranscriptionError>,
}

#[async_trait]
impl TranscriptionInput for MockInput {
    async fn send_audio(&mut self, chunk: AudioChunk) -> Result<(), TranscriptionError> {
        let mut record = self
            .record
            .lock()
            .map_err(|_| TranscriptionError::ProtocolError("mock record lock".to_owned()))?;
        if self
            .send_error_after_chunks
            .is_some_and(|limit| record.sent_sequences.len() >= limit)
        {
            return Err(TranscriptionError::ConnectionFailed(
                "scripted mid-stream failure".to_owned(),
            ));
        }
        record.sent_sequences.push(chunk.sequence);
        record.sent_samples.extend_from_slice(&chunk.samples);
        Ok(())
    }

    async fn finish(&mut self) -> Result<(), TranscriptionError> {
        if let Ok(mut record) = self.record.lock() {
            record.finish_count = record.finish_count.saturating_add(1);
        }
        if let Some(error) = self.finish_error.clone() {
            return Err(error);
        }
        if let Ok(mut finished) = self.lifecycle.finished.lock() {
            *finished = true;
        }
        self.lifecycle.changed.notify_waiters();
        Ok(())
    }

    async fn close(&mut self) -> Result<(), TranscriptionError> {
        if let Ok(mut record) = self.record.lock() {
            record.close_count = record.close_count.saturating_add(1);
        }
        Ok(())
    }

    async fn cancel(&mut self) -> Result<(), TranscriptionError> {
        if let Ok(mut record) = self.record.lock() {
            record.cancel_count = record.cancel_count.saturating_add(1);
        }
        if let Ok(mut cancelled) = self.lifecycle.cancelled.lock() {
            *cancelled = true;
        }
        self.lifecycle.changed.notify_waiters();
        Ok(())
    }
}

struct MockEvents {
    events: VecDeque<ScriptedEvent>,
    lifecycle: Arc<MockLifecycle>,
}

#[async_trait]
impl TranscriptionEvents for MockEvents {
    async fn next_event(&mut self) -> Result<TranscriptionEvent, TranscriptionError> {
        let after_finish = self
            .events
            .front()
            .ok_or(TranscriptionError::SessionClosedUnexpectedly)?
            .after_finish;
        if after_finish {
            loop {
                let changed = self.lifecycle.changed.notified();
                let finished = self
                    .lifecycle
                    .finished
                    .lock()
                    .map(|finished| *finished)
                    .unwrap_or(false);
                let cancelled = self
                    .lifecycle
                    .cancelled
                    .lock()
                    .map(|cancelled| *cancelled)
                    .unwrap_or(true);
                if cancelled {
                    return Err(TranscriptionError::Cancelled);
                }
                if finished {
                    break;
                }
                changed.await;
            }
        }
        let event = self
            .events
            .pop_front()
            .ok_or(TranscriptionError::SessionClosedUnexpectedly)?;
        if !event.delay.is_zero() {
            tokio::time::sleep(event.delay).await;
        }
        event.result
    }
}
