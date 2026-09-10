mod audio_queue;
mod coordinator;
mod deepgram;
mod error;
pub mod mock;
mod provider;
mod transcript;

use std::sync::Arc;

pub use audio_queue::{AudioQueueError, BoundedAudioQueue};
pub use coordinator::{
    CoordinatorObserver, TranscriptionControl, TranscriptionOutcome, spawn_transcription,
};
pub use deepgram::DeepgramProvider;
pub use error::{Retryability, TranscriptionError};
pub use provider::{
    AudioEncoding, ProviderVocabularyCapabilities, ProviderVocabularyConfig, SessionConfig,
    SessionParts, SessionTimeouts, SharedProvider, TranscriptionEvents, TranscriptionInput,
    TranscriptionProvider, prepare_vocabulary_config,
};
pub use transcript::TranscriptAccumulator;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioChunk {
    pub sequence: u64,
    pub samples: Arc<[i16]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TranscriptionEvent {
    SessionStarted { provider_session_id: Option<String> },
    SpeechStarted,
    PartialTranscript { text: String },
    FinalTranscript { text: String, operation_final: bool },
    SpeechEnded,
    ProviderWarning { code: String },
    ProviderError(TranscriptionError),
    SessionClosed,
}

pub fn initialize_tls_crypto_provider() -> Result<(), crate::error::AppError> {
    if rustls::crypto::CryptoProvider::get_default().is_some() {
        return Ok(());
    }

    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|_| crate::error::AppError::TlsCryptoProvider)
}

#[cfg(test)]
mod tests {
    use super::initialize_tls_crypto_provider;

    #[test]
    fn tls_crypto_provider_initialization_is_explicit_and_idempotent() {
        initialize_tls_crypto_provider().expect("first initialization succeeds");
        initialize_tls_crypto_provider().expect("second initialization is a no-op");
        assert!(rustls::crypto::CryptoProvider::get_default().is_some());
    }
}
