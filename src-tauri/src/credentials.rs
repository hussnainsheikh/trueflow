use std::sync::{Arc, RwLock};

#[derive(Clone, Default)]
pub struct ProviderCredentials {
    inner: Arc<RwLock<CredentialState>>,
}

#[derive(Default)]
struct CredentialState {
    development_transcription: Option<String>,
    development_transformation: Option<String>,
    session_transcription: Option<String>,
    session_transformation: Option<String>,
}

impl ProviderCredentials {
    pub fn new(
        development_transcription: Option<String>,
        development_transformation: Option<String>,
    ) -> Self {
        Self {
            inner: Arc::new(RwLock::new(CredentialState {
                development_transcription,
                development_transformation,
                ..CredentialState::default()
            })),
        }
    }

    pub fn transcription(&self) -> Option<String> {
        self.inner.read().ok().and_then(|state| {
            state
                .session_transcription
                .clone()
                .or_else(|| state.development_transcription.clone())
        })
    }

    pub fn transformation(&self) -> Option<String> {
        self.inner.read().ok().and_then(|state| {
            state
                .session_transformation
                .clone()
                .or_else(|| state.development_transformation.clone())
        })
    }

    pub fn cloud_ready(&self) -> bool {
        self.transcription().is_some()
    }

    pub fn apply_session(&self, transcription: Option<String>, transformation: Option<String>) {
        if let Ok(mut state) = self.inner.write() {
            state.session_transcription = nonempty(transcription);
            state.session_transformation = nonempty(transformation);
        }
    }

    pub fn clear_session(&self) {
        self.apply_session(None, None);
    }
}

fn nonempty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_credentials_override_and_clear_back_to_development_values() {
        let credentials = ProviderCredentials::new(
            Some("development-stt".to_owned()),
            Some("development-ai".to_owned()),
        );
        credentials.apply_session(
            Some("session-stt".to_owned()),
            Some("session-ai".to_owned()),
        );
        assert_eq!(credentials.transcription().as_deref(), Some("session-stt"));
        assert_eq!(credentials.transformation().as_deref(), Some("session-ai"));
        credentials.clear_session();
        assert_eq!(
            credentials.transcription().as_deref(),
            Some("development-stt")
        );
    }
}
