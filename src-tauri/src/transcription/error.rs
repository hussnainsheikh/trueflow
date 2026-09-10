use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Retryability {
    Retryable,
    NotRetryable,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum TranscriptionError {
    #[error("streaming transcription is not configured")]
    ConfigurationMissing,
    #[error("transcription authentication failed")]
    AuthenticationFailed,
    #[error("transcription connection failed: {0}")]
    ConnectionFailed(String),
    #[error("transcription connection timed out")]
    ConnectionTimeout,
    #[error("transcription provider is unavailable: {0}")]
    ProviderUnavailable(String),
    #[error("transcription provider rate limit was reached")]
    RateLimited,
    #[error("transcription protocol failed: {0}")]
    ProtocolError(String),
    #[error("transcription provider returned an invalid response: {0}")]
    InvalidResponse(String),
    #[error("transcription provider rejected the audio: {0}")]
    AudioRejected(String),
    #[error("transcription audio backpressure exceeded the bounded queue")]
    BackpressureExceeded,
    #[error("transcription session closed unexpectedly")]
    SessionClosedUnexpectedly,
    #[error("transcription finalization timed out")]
    FinalizationTimeout,
    #[error("transcription session close timed out")]
    CloseTimeout,
    #[error("transcription was cancelled")]
    Cancelled,
    #[error("the network is unavailable: {0}")]
    NetworkUnavailable(String),
    #[error("the transcription worker stopped unexpectedly")]
    RuntimeFailed,
}

impl TranscriptionError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::ConfigurationMissing => "STT_CONFIGURATION_MISSING",
            Self::AuthenticationFailed => "STT_AUTHENTICATION_FAILED",
            Self::ConnectionFailed(_) => "STT_CONNECTION_FAILED",
            Self::ConnectionTimeout => "STT_CONNECTION_TIMEOUT",
            Self::ProviderUnavailable(_) => "STT_PROVIDER_UNAVAILABLE",
            Self::RateLimited => "STT_RATE_LIMITED",
            Self::ProtocolError(_) => "STT_PROTOCOL_ERROR",
            Self::InvalidResponse(_) => "STT_INVALID_RESPONSE",
            Self::AudioRejected(_) => "STT_AUDIO_REJECTED",
            Self::BackpressureExceeded => "STT_BACKPRESSURE_EXCEEDED",
            Self::SessionClosedUnexpectedly => "STT_SESSION_CLOSED_UNEXPECTEDLY",
            Self::FinalizationTimeout => "STT_FINALIZATION_TIMEOUT",
            Self::CloseTimeout => "STT_SESSION_CLOSE_TIMEOUT",
            Self::Cancelled => "STT_CANCELLED",
            Self::NetworkUnavailable(_) => "NETWORK_UNAVAILABLE",
            Self::RuntimeFailed => "STT_RUNTIME_FAILED",
        }
    }

    pub const fn user_message(&self) -> &'static str {
        match self {
            Self::ConfigurationMissing => {
                "Streaming transcription is not configured. Add a development provider key and restart Trueflow."
            }
            Self::AuthenticationFailed => {
                "The transcription credential was rejected. Check the local provider configuration."
            }
            Self::RateLimited => {
                "The transcription service is busy for this account. Wait briefly and try again."
            }
            Self::BackpressureExceeded => {
                "The transcription connection could not keep up with your audio. Please try again."
            }
            Self::FinalizationTimeout | Self::CloseTimeout => {
                "Trueflow kept the available transcript, but the transcription service did not finish in time."
            }
            Self::Cancelled => "Transcription was cancelled.",
            Self::NetworkUnavailable(_) => {
                "The network is unavailable. Check your connection and try again."
            }
            Self::RuntimeFailed => {
                "The transcription worker stopped unexpectedly. Please try again."
            }
            Self::ConnectionFailed(_)
            | Self::ConnectionTimeout
            | Self::ProviderUnavailable(_)
            | Self::ProtocolError(_)
            | Self::InvalidResponse(_)
            | Self::AudioRejected(_)
            | Self::SessionClosedUnexpectedly => {
                "Trueflow could not complete transcription. Your available transcript has been preserved."
            }
        }
    }

    pub const fn retryability(&self) -> Retryability {
        match self {
            Self::ConfigurationMissing
            | Self::AuthenticationFailed
            | Self::ProtocolError(_)
            | Self::InvalidResponse(_)
            | Self::AudioRejected(_)
            | Self::BackpressureExceeded
            | Self::Cancelled => Retryability::NotRetryable,
            Self::ConnectionFailed(_)
            | Self::ConnectionTimeout
            | Self::ProviderUnavailable(_)
            | Self::RateLimited
            | Self::SessionClosedUnexpectedly
            | Self::FinalizationTimeout
            | Self::CloseTimeout
            | Self::NetworkUnavailable(_) => Retryability::Retryable,
            Self::RuntimeFailed => Retryability::Retryable,
        }
    }
}
