use thiserror::Error;

use crate::transcription::{Retryability, TranscriptionError};

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DictationError {
    #[error("microphone permission was denied")]
    MicrophonePermissionDenied,
    #[error("microphone access is restricted")]
    MicrophonePermissionRestricted,
    #[error("microphone permission status is unavailable")]
    MicrophonePermissionUnavailable,
    #[error("microphone permission request timed out")]
    MicrophonePermissionTimeout,
    #[error("no default microphone is available")]
    NoInputDevice,
    #[error("default microphone configuration is unavailable: {0}")]
    UnsupportedAudioConfiguration(String),
    #[error("microphone capture could not start: {0}")]
    CaptureStartFailed(String),
    #[error("microphone capture stopped unexpectedly: {0}")]
    AudioDeviceLost(String),
    #[error("microphone capture cleanup failed: {0}")]
    CaptureCleanupFailed(String),
    #[error("global shortcut registration failed: {0}")]
    ShortcutRegistrationFailed(String),
    #[error("dictation state is unavailable")]
    StateUnavailable,
    #[error("invalid dictation state transition")]
    InvalidTransition,
    #[error("active project context is unavailable")]
    ProjectContextUnavailable,
    #[error(transparent)]
    Transcription(#[from] TranscriptionError),
}

impl DictationError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::MicrophonePermissionDenied => "MICROPHONE_PERMISSION_DENIED",
            Self::MicrophonePermissionRestricted => "MICROPHONE_PERMISSION_RESTRICTED",
            Self::MicrophonePermissionUnavailable => "MICROPHONE_PERMISSION_UNAVAILABLE",
            Self::MicrophonePermissionTimeout => "MICROPHONE_PERMISSION_TIMEOUT",
            Self::NoInputDevice => "MICROPHONE_UNAVAILABLE",
            Self::UnsupportedAudioConfiguration(_) => "AUDIO_CONFIGURATION_UNSUPPORTED",
            Self::CaptureStartFailed(_) => "AUDIO_CAPTURE_START_FAILED",
            Self::AudioDeviceLost(_) => "AUDIO_DEVICE_LOST",
            Self::CaptureCleanupFailed(_) => "AUDIO_CAPTURE_CLEANUP_FAILED",
            Self::ShortcutRegistrationFailed(_) => "SHORTCUT_REGISTRATION_FAILED",
            Self::StateUnavailable => "DICTATION_STATE_UNAVAILABLE",
            Self::InvalidTransition => "DICTATION_INVALID_TRANSITION",
            Self::ProjectContextUnavailable => "ACTIVE_PROJECT_UNAVAILABLE",
            Self::Transcription(error) => error.code(),
        }
    }

    pub const fn user_message(&self) -> &'static str {
        match self {
            Self::MicrophonePermissionDenied => {
                "Microphone access is required. Enable Trueflow in System Settings → Privacy & Security → Microphone."
            }
            Self::MicrophonePermissionRestricted => {
                "Microphone access is restricted by this Mac's settings."
            }
            Self::MicrophonePermissionUnavailable | Self::MicrophonePermissionTimeout => {
                "Trueflow could not determine microphone access. Try again."
            }
            Self::NoInputDevice => "No microphone is available. Connect or select an input device.",
            Self::UnsupportedAudioConfiguration(_) => {
                "The default microphone uses an unsupported audio format."
            }
            Self::CaptureStartFailed(_) => "Trueflow could not start the microphone. Try again.",
            Self::AudioDeviceLost(_) => {
                "The microphone became unavailable. Reconnect it and try again."
            }
            Self::CaptureCleanupFailed(_) => {
                "Trueflow stopped recording but could not clean up normally."
            }
            Self::ShortcutRegistrationFailed(_) => {
                "Trueflow could not register Option + Space. Another application may be using it."
            }
            Self::StateUnavailable | Self::InvalidTransition => {
                "Trueflow's dictation state needs to be reset. Try again."
            }
            Self::ProjectContextUnavailable => {
                "Trueflow could not load the active project. Try again."
            }
            Self::Transcription(error) => error.user_message(),
        }
    }

    pub const fn is_fatal(&self) -> bool {
        matches!(self, Self::ShortcutRegistrationFailed(_))
    }

    pub const fn is_retryable(&self) -> bool {
        match self {
            Self::Transcription(error) => {
                matches!(error.retryability(), Retryability::Retryable)
            }
            _ => !self.is_fatal(),
        }
    }
}
