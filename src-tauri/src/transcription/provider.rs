use std::{sync::Arc, time::Duration};

use async_trait::async_trait;

use super::{AudioChunk, TranscriptionError, TranscriptionEvent};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionConfig {
    pub operation_id: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub encoding: AudioEncoding,
    pub language: String,
    pub model: String,
    pub vocabulary: ProviderVocabularyConfig,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProviderVocabularyCapabilities {
    pub supports_hints: bool,
    pub supports_weighted_terms: bool,
    pub max_hint_count: usize,
    pub max_total_tokens: usize,
    pub max_hint_length: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProviderVocabularyConfig {
    pub terms: Vec<String>,
    pub source_term_count: usize,
    pub truncated: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioEncoding {
    Linear16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionTimeouts {
    pub connection: Duration,
    pub audio_send: Duration,
    pub finalization: Duration,
    pub close: Duration,
}

impl Default for SessionTimeouts {
    fn default() -> Self {
        Self {
            connection: Duration::from_secs(5),
            audio_send: Duration::from_secs(2),
            finalization: Duration::from_secs(8),
            close: Duration::from_secs(2),
        }
    }
}

pub struct SessionParts {
    pub input: Box<dyn TranscriptionInput>,
    pub events: Box<dyn TranscriptionEvents>,
}

#[async_trait]
pub trait TranscriptionProvider: Send + Sync {
    fn name(&self) -> &'static str;

    fn vocabulary_capabilities(&self) -> ProviderVocabularyCapabilities {
        ProviderVocabularyCapabilities::default()
    }

    async fn start_session(
        &self,
        config: SessionConfig,
    ) -> Result<SessionParts, TranscriptionError>;
}

pub fn prepare_vocabulary_config(
    capabilities: ProviderVocabularyCapabilities,
    canonical_terms: impl IntoIterator<Item = String>,
    aliases: impl IntoIterator<Item = String>,
) -> ProviderVocabularyConfig {
    let candidates = canonical_terms.into_iter().chain(aliases);
    let mut selected = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut estimated_tokens = 0_usize;
    let mut source_term_count = 0_usize;
    let mut truncated = false;
    for candidate in candidates {
        source_term_count = source_term_count.saturating_add(1);
        let candidate = candidate.trim();
        let folded = candidate
            .chars()
            .flat_map(char::to_lowercase)
            .collect::<String>();
        let token_cost = conservative_token_estimate(candidate);
        if !capabilities.supports_hints
            || candidate.is_empty()
            || candidate.chars().count() > capabilities.max_hint_length
            || selected.len() >= capabilities.max_hint_count
            || estimated_tokens.saturating_add(token_cost) > capabilities.max_total_tokens
        {
            truncated |= !candidate.is_empty();
            continue;
        }
        if seen.insert(folded) {
            selected.push(candidate.to_owned());
            estimated_tokens = estimated_tokens.saturating_add(token_cost);
        }
    }
    ProviderVocabularyConfig {
        terms: selected,
        source_term_count,
        truncated,
    }
}

fn conservative_token_estimate(value: &str) -> usize {
    let characters = value.chars().count();
    let whitespace_words = value.split_whitespace().count();
    let non_ascii = value
        .chars()
        .filter(|character| !character.is_ascii())
        .count();
    whitespace_words
        .max(characters.saturating_add(3) / 4)
        .max(non_ascii)
        .max(1)
}

#[async_trait]
pub trait TranscriptionInput: Send {
    async fn send_audio(&mut self, chunk: AudioChunk) -> Result<(), TranscriptionError>;
    async fn finish(&mut self) -> Result<(), TranscriptionError>;
    async fn close(&mut self) -> Result<(), TranscriptionError>;
    async fn cancel(&mut self) -> Result<(), TranscriptionError>;
}

#[async_trait]
pub trait TranscriptionEvents: Send {
    async fn next_event(&mut self) -> Result<TranscriptionEvent, TranscriptionError>;
}

pub type SharedProvider = Arc<dyn TranscriptionProvider>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_provider_ignores_hints_without_blocking_sessions() {
        let config = prepare_vocabulary_config(
            ProviderVocabularyCapabilities::default(),
            ["Aeryndor".to_owned()],
            [],
        );
        assert!(config.terms.is_empty());
        assert!(config.truncated);
    }

    #[test]
    fn canonical_terms_precede_aliases_and_limits_are_deterministic() {
        let config = prepare_vocabulary_config(
            ProviderVocabularyCapabilities {
                supports_hints: true,
                max_hint_count: 2,
                max_total_tokens: 10,
                max_hint_length: 40,
                ..ProviderVocabularyCapabilities::default()
            },
            ["Aeryndor".to_owned(), "Vhal'Tor".to_owned()],
            ["air in door".to_owned()],
        );
        assert_eq!(config.terms, vec!["Aeryndor", "Vhal'Tor"]);
        assert!(config.truncated);
    }
}
