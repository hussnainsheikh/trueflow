use std::{collections::BTreeMap, time::Instant};

use tracing::{error, info};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use crate::error::AppError;

pub fn initialize(log_filter: &str) -> Result<(), AppError> {
    tracing_subscriber::registry()
        .with(EnvFilter::new(log_filter))
        .with(
            tracing_subscriber::fmt::layer()
                .json()
                .with_current_span(true)
                .with_span_list(true),
        )
        .try_init()?;
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PipelineEvent {
    RewriteShortcutPressed,
    RewriteTargetResolved,
    RewriteSelectionCaptured,
    InstructionCaptureStarted,
    InstructionFirstAudioFrame,
    InstructionFinalTranscriptReceived,
    RewriteTransformRequested,
    RewriteTransformCompleted,
    RewriteRevalidationStarted,
    RewriteRevalidationSucceeded,
    RewriteRevalidationFailed,
    RewriteReplaceRequested,
    RewriteReplaceCompleted,
    RewriteOperationCompleted,
    ShortcutPressed,
    CaptureStartRequested,
    CaptureStarted,
    FirstAudioFrameReceived,
    SttConnectRequested,
    SttConnectionReady,
    FirstAudioSent,
    FirstPartialReceived,
    ShortcutReleased,
    CaptureStopRequested,
    CaptureStopped,
    SttFinalizeRequested,
    FinalTranscriptReceived,
    SttSessionClosed,
    TransformRequested,
    TransformStarted,
    TransformFirstResult,
    TransformCompleted,
    TransformFallback,
    InsertRequested,
    TargetResolved,
    DirectInsertStarted,
    DirectInsertCompleted,
    TypingStarted,
    TypingCompleted,
    PasteFallbackStarted,
    PasteDispatched,
    ClipboardRestoreStarted,
    ClipboardRestoreCompleted,
    ClipboardOnlyFallback,
    InsertCompleted,
    OperationCompleted,
}

impl PipelineEvent {
    const fn name(self) -> &'static str {
        match self {
            Self::RewriteShortcutPressed => "rewrite_shortcut_pressed",
            Self::RewriteTargetResolved => "rewrite_target_resolved",
            Self::RewriteSelectionCaptured => "rewrite_selection_captured",
            Self::InstructionCaptureStarted => "instruction_capture_started",
            Self::InstructionFirstAudioFrame => "instruction_first_audio_frame",
            Self::InstructionFinalTranscriptReceived => "instruction_final_transcript_received",
            Self::RewriteTransformRequested => "rewrite_transform_requested",
            Self::RewriteTransformCompleted => "rewrite_transform_completed",
            Self::RewriteRevalidationStarted => "rewrite_revalidation_started",
            Self::RewriteRevalidationSucceeded => "rewrite_revalidation_succeeded",
            Self::RewriteRevalidationFailed => "rewrite_revalidation_failed",
            Self::RewriteReplaceRequested => "rewrite_replace_requested",
            Self::RewriteReplaceCompleted => "rewrite_replace_completed",
            Self::RewriteOperationCompleted => "rewrite_operation_completed",
            Self::ShortcutPressed => "shortcut_pressed",
            Self::CaptureStartRequested => "capture_start_requested",
            Self::CaptureStarted => "capture_started",
            Self::FirstAudioFrameReceived => "first_audio_frame_received",
            Self::SttConnectRequested => "stt_connect_requested",
            Self::SttConnectionReady => "stt_connection_ready",
            Self::FirstAudioSent => "first_audio_sent",
            Self::FirstPartialReceived => "first_partial_received",
            Self::ShortcutReleased => "shortcut_released",
            Self::CaptureStopRequested => "capture_stop_requested",
            Self::CaptureStopped => "capture_stopped",
            Self::SttFinalizeRequested => "stt_finalize_requested",
            Self::FinalTranscriptReceived => "final_transcript_received",
            Self::SttSessionClosed => "stt_session_closed",
            Self::TransformRequested => "transform_requested",
            Self::TransformStarted => "transform_started",
            Self::TransformFirstResult => "transform_first_result",
            Self::TransformCompleted => "transform_completed",
            Self::TransformFallback => "transform_fallback",
            Self::InsertRequested => "insert_requested",
            Self::TargetResolved => "target_resolved",
            Self::DirectInsertStarted => "direct_insert_started",
            Self::DirectInsertCompleted => "direct_insert_completed",
            Self::TypingStarted => "typing_started",
            Self::TypingCompleted => "typing_completed",
            Self::PasteFallbackStarted => "paste_fallback_started",
            Self::PasteDispatched => "paste_dispatched",
            Self::ClipboardRestoreStarted => "clipboard_restore_started",
            Self::ClipboardRestoreCompleted => "clipboard_restore_completed",
            Self::ClipboardOnlyFallback => "clipboard_only_fallback",
            Self::InsertCompleted => "insert_completed",
            Self::OperationCompleted => "operation_completed",
        }
    }
}

pub struct OperationTelemetry {
    operation_id: String,
    started_at: Instant,
    events_us: BTreeMap<PipelineEvent, u64>,
}

impl OperationTelemetry {
    pub fn new(operation_id: String) -> Self {
        Self {
            operation_id,
            started_at: Instant::now(),
            events_us: BTreeMap::new(),
        }
    }

    pub fn mark(&mut self, event: PipelineEvent) {
        self.events_us
            .entry(event)
            .or_insert_with(|| elapsed_micros(self.started_at));
    }

    pub fn finish(mut self) {
        self.mark(PipelineEvent::OperationCompleted);
        for (event, elapsed_us) in self.events_us {
            info!(
                operation_id = %self.operation_id,
                subsystem = "dictation",
                event = event.name(),
                elapsed_us,
                "dictation performance event"
            );
        }
    }

    pub fn fail(mut self, error_code: &str) {
        self.mark(PipelineEvent::OperationCompleted);
        error!(
            operation_id = %self.operation_id,
            subsystem = "dictation",
            event = "operation_failed",
            error_code,
            elapsed_us = elapsed_micros(self.started_at),
            "dictation operation failed"
        );
        for (event, elapsed_us) in self.events_us {
            info!(
                operation_id = %self.operation_id,
                subsystem = "dictation",
                event = event.name(),
                elapsed_us,
                "dictation performance event"
            );
        }
    }
}

fn elapsed_micros(started_at: Instant) -> u64 {
    u64::try_from(started_at.elapsed().as_micros()).unwrap_or(u64::MAX)
}
