use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MY_VOICE_ID: &str = "10000000-0000-4000-8000-000000000001";
pub const CLEAN_DICTATION_MODE_ID: &str = "20000000-0000-4000-8000-000000000001";

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Voice {
    pub id: String,
    pub project_id: Option<String>,
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub examples: Vec<String>,
    pub avoidances: String,
    pub is_default: bool,
    pub is_builtin: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceInput {
    pub project_id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub instructions: String,
    #[serde(default)]
    pub examples: Vec<String>,
    #[serde(default)]
    pub avoidances: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ValidatedVoiceInput {
    pub project_id: Option<String>,
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub examples: Vec<String>,
    pub avoidances: String,
}

impl VoiceInput {
    pub(crate) fn validate(self) -> Result<ValidatedVoiceInput, WritingError> {
        let project_id = self
            .project_id
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        let name = required(self.name, "Voice name", 120)?;
        let instructions = required(self.instructions, "Voice instructions", 8_000)?;
        let description = limited(self.description, "Voice description", 2_000)?;
        let avoidances = limited(self.avoidances, "Voice avoidances", 4_000)?;
        let mut examples = BTreeMap::new();
        for example in self.examples {
            let example = required(example, "Voice example", 4_000)?;
            examples.entry(example.to_lowercase()).or_insert(example);
        }
        if examples.len() > 20 {
            return Err(WritingError::ValidationFailed(
                "A Voice can contain at most 20 examples.".to_owned(),
            ));
        }
        Ok(ValidatedVoiceInput {
            project_id,
            name,
            description,
            instructions,
            examples: examples.into_values().collect(),
            avoidances,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mode {
    pub id: String,
    pub name: String,
    pub description: String,
    pub system_instructions: String,
    pub requires_voice: bool,
    pub supports_selection: bool,
    pub is_builtin: bool,
    pub position: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Error)]
pub enum WritingError {
    #[error("writing configuration validation failed: {0}")]
    ValidationFailed(String),
    #[error("Voice was not found")]
    VoiceNotFound,
    #[error("Mode was not found")]
    ModeNotFound,
    #[error("Voice belongs to another project")]
    VoiceProjectMismatch,
    #[error("built-in Voice cannot be modified")]
    BuiltinVoiceProtected,
    #[error("selection mode is unavailable for dictation")]
    SelectionModeUnavailable,
}

impl WritingError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::ValidationFailed(_) => "WRITING_VALIDATION_FAILED",
            Self::VoiceNotFound => "VOICE_NOT_FOUND",
            Self::ModeNotFound => "MODE_NOT_FOUND",
            Self::VoiceProjectMismatch => "VOICE_PROJECT_MISMATCH",
            Self::BuiltinVoiceProtected => "VOICE_BUILTIN_PROTECTED",
            Self::SelectionModeUnavailable => "MODE_SELECTION_REWRITE_UNAVAILABLE",
        }
    }

    pub const fn user_message(&self) -> &'static str {
        match self {
            Self::ValidationFailed(_) => "Check the Voice fields and try again.",
            Self::VoiceNotFound => "That Voice no longer exists.",
            Self::ModeNotFound => "That Mode no longer exists.",
            Self::VoiceProjectMismatch => "That Voice is not available in this project.",
            Self::BuiltinVoiceProtected => "Built-in Voices cannot be edited or deleted.",
            Self::SelectionModeUnavailable => {
                "Use Option + Shift + Space on selected text for the dedicated rewrite workflow."
            }
        }
    }
}

fn required(value: String, label: &str, max: usize) -> Result<String, WritingError> {
    let value = value.trim().to_owned();
    if value.is_empty() {
        return Err(WritingError::ValidationFailed(format!(
            "{label} cannot be empty."
        )));
    }
    limited(value, label, max)
}

fn limited(value: String, label: &str, max: usize) -> Result<String, WritingError> {
    let value = value.trim().to_owned();
    if value.chars().count() > max {
        Err(WritingError::ValidationFailed(format!(
            "{label} cannot exceed {max} characters."
        )))
    } else {
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_deduplicates_voice_examples() {
        let input = VoiceInput {
            project_id: Some(" project ".to_owned()),
            name: " Kael ".to_owned(),
            description: " terse ".to_owned(),
            instructions: " Speak tersely. ".to_owned(),
            examples: vec!["Hold.".to_owned(), "hold.".to_owned()],
            avoidances: " No slang. ".to_owned(),
        }
        .validate()
        .expect("valid Voice");

        assert_eq!(input.project_id.as_deref(), Some("project"));
        assert_eq!(input.examples, vec!["Hold."]);
    }

    #[test]
    fn rejects_empty_voice_instructions() {
        let error = VoiceInput {
            project_id: None,
            name: "Voice".to_owned(),
            description: String::new(),
            instructions: " ".to_owned(),
            examples: Vec::new(),
            avoidances: String::new(),
        }
        .validate()
        .expect_err("instructions required");
        assert_eq!(error.code(), "WRITING_VALIDATION_FAILED");
    }
}
