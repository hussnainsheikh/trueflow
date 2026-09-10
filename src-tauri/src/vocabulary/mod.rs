use std::{collections::BTreeMap, time::Instant};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VocabularyKind {
    Person,
    Place,
    Organization,
    Object,
    Technical,
    Other,
}

impl VocabularyKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Place => "place",
            Self::Organization => "organization",
            Self::Object => "object",
            Self::Technical => "technical",
            Self::Other => "other",
        }
    }

    pub fn parse(value: &str) -> Result<Self, VocabularyError> {
        match value {
            "person" => Ok(Self::Person),
            "place" => Ok(Self::Place),
            "organization" => Ok(Self::Organization),
            "object" => Ok(Self::Object),
            "technical" => Ok(Self::Technical),
            "other" => Ok(Self::Other),
            _ => Err(VocabularyError::DatabaseReadFailed),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VocabularyEntry {
    pub id: String,
    pub project_id: String,
    pub term: String,
    pub pronunciation_hint: Option<String>,
    pub kind: VocabularyKind,
    pub description: Option<String>,
    pub aliases: Vec<String>,
    pub forbidden_variants: Vec<String>,
    pub case_sensitive: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VocabularyInput {
    pub term: String,
    pub pronunciation_hint: Option<String>,
    pub kind: VocabularyKind,
    pub description: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub forbidden_variants: Vec<String>,
    #[serde(default)]
    pub case_sensitive: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedVocabularyInput {
    pub term: String,
    pub pronunciation_hint: Option<String>,
    pub kind: VocabularyKind,
    pub description: Option<String>,
    pub aliases: Vec<String>,
    pub forbidden_variants: Vec<String>,
    pub case_sensitive: bool,
}

impl VocabularyInput {
    pub fn validate(self) -> Result<ValidatedVocabularyInput, VocabularyError> {
        let term = required(self.term, "term")?;
        let pronunciation_hint = optional(self.pronunciation_hint);
        let description = optional(self.description);
        let aliases = related_values(self.aliases, &term, "alias")?;
        let forbidden_variants =
            related_values(self.forbidden_variants, &term, "forbidden variant")?;
        if aliases.iter().any(|alias| {
            forbidden_variants
                .iter()
                .any(|variant| equivalent(alias, variant))
        }) {
            return Err(VocabularyError::ValidationFailed(
                "An alias cannot also be a forbidden variant.".to_owned(),
            ));
        }
        Ok(ValidatedVocabularyInput {
            term,
            pronunciation_hint,
            kind: self.kind,
            description,
            aliases,
            forbidden_variants,
            case_sensitive: self.case_sensitive,
        })
    }
}

fn required(value: String, field: &str) -> Result<String, VocabularyError> {
    let value = value.trim().to_owned();
    if value.is_empty() {
        Err(VocabularyError::ValidationFailed(format!(
            "Vocabulary {field} cannot be empty."
        )))
    } else {
        Ok(value)
    }
}

fn optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn related_values(
    values: Vec<String>,
    term: &str,
    label: &str,
) -> Result<Vec<String>, VocabularyError> {
    let mut unique = BTreeMap::<String, String>::new();
    for value in values {
        let trimmed = value.trim().to_owned();
        if trimmed.is_empty() {
            return Err(VocabularyError::ValidationFailed(format!(
                "A vocabulary {label} cannot be empty."
            )));
        }
        if equivalent(&trimmed, term) {
            return Err(VocabularyError::ValidationFailed(format!(
                "The canonical term cannot also be its own {label}."
            )));
        }
        unique.entry(fold(&trimmed)).or_insert(trimmed);
    }
    Ok(unique.into_values().collect())
}

fn equivalent(left: &str, right: &str) -> bool {
    fold(left) == fold(right)
}

fn fold(value: &str) -> String {
    value.chars().flat_map(char::to_lowercase).collect()
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum VocabularyError {
    #[error("vocabulary validation failed: {0}")]
    ValidationFailed(String),
    #[error("vocabulary entry was not found")]
    EntryNotFound,
    #[error("the vocabulary entry belongs to another project")]
    ProjectMismatch,
    #[error("a vocabulary entry with that canonical term already exists")]
    DuplicateEntry,
    #[error("vocabulary database read failed")]
    DatabaseReadFailed,
}

impl VocabularyError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::ValidationFailed(_) => "VOCABULARY_VALIDATION_FAILED",
            Self::EntryNotFound => "VOCABULARY_ENTRY_NOT_FOUND",
            Self::ProjectMismatch => "VOCABULARY_PROJECT_MISMATCH",
            Self::DuplicateEntry => "VOCABULARY_DUPLICATE_ENTRY",
            Self::DatabaseReadFailed => "VOCABULARY_DATABASE_READ_FAILED",
        }
    }

    pub const fn user_message(&self) -> &'static str {
        match self {
            Self::ValidationFailed(_) => "Check the vocabulary fields and try again.",
            Self::EntryNotFound => "That vocabulary entry no longer exists.",
            Self::ProjectMismatch => "That vocabulary entry belongs to another project.",
            Self::DuplicateEntry => "That project already contains this vocabulary term.",
            Self::DatabaseReadFailed => "Trueflow could not update project vocabulary.",
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NormalizationResult {
    pub text: String,
    pub match_count: usize,
    pub duration_us: u64,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum RuleStrength {
    Canonical,
    Alias,
    Forbidden,
}

#[derive(Clone, Debug)]
struct Rule {
    source: String,
    folded_source: String,
    replacement: String,
    case_sensitive: bool,
    strength: RuleStrength,
    stable_key: String,
}

#[derive(Clone, Debug)]
struct Candidate {
    start: usize,
    end: usize,
    replacement: String,
    strength: RuleStrength,
    source_chars: usize,
    stable_key: String,
}

pub fn normalize(text: &str, entries: &[VocabularyEntry]) -> NormalizationResult {
    let started = Instant::now();
    if text.is_empty() || entries.is_empty() {
        return NormalizationResult {
            text: text.to_owned(),
            match_count: 0,
            duration_us: elapsed_us(started),
        };
    }
    let rules = build_rules(entries);
    let boundaries = char_boundaries(text);
    let mut candidates = Vec::new();
    for (start_index, &start) in boundaries.iter().enumerate().take(boundaries.len() - 1) {
        for rule in &rules {
            let source_chars = rule.source.chars().count();
            let max_end_index =
                (start_index + source_chars.saturating_mul(3) + 3).min(boundaries.len() - 1);
            for &end in &boundaries[start_index + 1..=max_end_index] {
                let candidate = &text[start..end];
                let matches = if rule.case_sensitive {
                    normalize_spacing(candidate) == normalize_spacing(&rule.source)
                } else {
                    comparison_key(candidate) == rule.folded_source
                };
                if matches && has_boundaries(text, start, end, &rule.source) {
                    candidates.push(Candidate {
                        start,
                        end,
                        replacement: rule.replacement.clone(),
                        strength: rule.strength,
                        source_chars,
                        stable_key: rule.stable_key.clone(),
                    });
                    break;
                }
            }
        }
    }

    candidates.sort_by(|left, right| {
        right
            .strength
            .cmp(&left.strength)
            .then_with(|| right.source_chars.cmp(&left.source_chars))
            .then_with(|| left.start.cmp(&right.start))
            .then_with(|| left.stable_key.cmp(&right.stable_key))
    });
    let mut selected = Vec::<Candidate>::new();
    for candidate in candidates {
        if selected
            .iter()
            .all(|other| candidate.end <= other.start || candidate.start >= other.end)
        {
            selected.push(candidate);
        }
    }
    selected.sort_by_key(|candidate| candidate.start);

    let mut output = String::with_capacity(text.len());
    let mut cursor = 0;
    for candidate in &selected {
        output.push_str(&text[cursor..candidate.start]);
        output.push_str(&candidate.replacement);
        cursor = candidate.end;
    }
    output.push_str(&text[cursor..]);
    NormalizationResult {
        text: output,
        match_count: selected.len(),
        duration_us: elapsed_us(started),
    }
}

fn build_rules(entries: &[VocabularyEntry]) -> Vec<Rule> {
    let mut rules = Vec::new();
    for entry in entries {
        let mut push = |source: &str, strength| {
            rules.push(Rule {
                source: source.to_owned(),
                folded_source: comparison_key(source),
                replacement: entry.term.clone(),
                case_sensitive: entry.case_sensitive,
                strength,
                stable_key: format!("{}:{source}", entry.id),
            });
        };
        push(&entry.term, RuleStrength::Canonical);
        for alias in &entry.aliases {
            push(alias, RuleStrength::Alias);
        }
        for variant in &entry.forbidden_variants {
            push(variant, RuleStrength::Forbidden);
        }
    }
    rules
}

fn comparison_key(value: &str) -> String {
    fold(&normalize_spacing(value))
}

fn normalize_spacing(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn char_boundaries(text: &str) -> Vec<usize> {
    text.char_indices()
        .map(|(index, _)| index)
        .chain(std::iter::once(text.len()))
        .collect()
}

fn has_boundaries(text: &str, start: usize, end: usize, source: &str) -> bool {
    let first = source.chars().next();
    let last = source.chars().next_back();
    let previous = text[..start].chars().next_back();
    let next = text[end..].chars().next();
    (!first.is_some_and(is_word) || !previous.is_some_and(is_word))
        && (!last.is_some_and(is_word) || !next.is_some_and(is_word))
}

fn is_word(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn elapsed_us(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(
        id: &str,
        term: &str,
        aliases: &[&str],
        variants: &[&str],
        case_sensitive: bool,
    ) -> VocabularyEntry {
        VocabularyEntry {
            id: id.to_owned(),
            project_id: "project".to_owned(),
            term: term.to_owned(),
            pronunciation_hint: None,
            kind: VocabularyKind::Other,
            description: None,
            aliases: aliases.iter().map(|value| (*value).to_owned()).collect(),
            forbidden_variants: variants.iter().map(|value| (*value).to_owned()).collect(),
            case_sensitive,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn normalizes_capitalization_for_case_insensitive_terms() {
        let result = normalize(
            "aeryndor crossed the ashen coast",
            &[
                entry("1", "Aeryndor", &[], &[], false),
                entry("2", "Ashen Coast", &[], &[], false),
            ],
        );
        assert_eq!(result.text, "Aeryndor crossed the Ashen Coast");
        assert_eq!(result.match_count, 2);
    }

    #[test]
    fn forbidden_variants_and_aliases_map_to_the_canonical_term() {
        let result = normalize(
            "Commander Aery met Erin Door near Vhal Tor.",
            &[
                entry("1", "Aeryndor", &["Commander Aery"], &["Erin Door"], false),
                entry("2", "Vhal'Tor", &[], &["Vhal Tor"], false),
            ],
        );
        assert_eq!(result.text, "Aeryndor met Aeryndor near Vhal'Tor.");
    }

    #[test]
    fn does_not_corrupt_substrings() {
        assert_eq!(
            normalize(
                "Kai watched the Kaiser and fashion.",
                &[entry("1", "Kai", &[], &[], false)]
            )
            .text,
            "Kai watched the Kaiser and fashion."
        );
    }

    #[test]
    fn forbidden_rules_precede_weaker_rules_without_replacement_cascades() {
        let entries = [
            entry("1", "York", &[], &["New"], false),
            entry("2", "New York", &[], &[], false),
            entry("3", "Something Else", &["Aeryndor"], &[], false),
            entry("4", "Aeryndor", &[], &["Erin Door"], false),
        ];
        assert_eq!(
            normalize("New York and Erin Door", &entries).text,
            "York York and Aeryndor"
        );
    }

    #[test]
    fn longer_phrases_win_over_shorter_rules_of_equal_strength() {
        let entries = [
            entry("1", "York", &[], &[], false),
            entry("2", "New York", &[], &[], false),
        ];
        assert_eq!(normalize("new york", &entries).text, "New York");
    }

    #[test]
    fn multiword_terms_accept_reasonable_whitespace_variation() {
        let entries = [entry("1", "Ashen Coast", &[], &[], false)];
        assert_eq!(
            normalize("the ashen   coast", &entries).text,
            "the Ashen Coast"
        );
    }

    #[test]
    fn handles_unicode_and_emoji_adjacent_text_without_byte_corruption() {
        let entries = [
            entry("1", "Élara", &[], &["élara"], false),
            entry("2", "王明", &[], &[], false),
        ];
        assert_eq!(
            normalize("✨élara met 王明✨", &entries).text,
            "✨Élara met 王明✨"
        );
    }

    #[test]
    fn case_sensitive_entries_match_only_exact_configured_sources() {
        let entries = [entry("1", "iOS", &["IOS"], &["ios"], true)];
        assert_eq!(
            normalize("iOS IOS ios IoS", &entries).text,
            "iOS iOS iOS IoS"
        );
    }

    #[test]
    fn empty_vocabulary_leaves_text_unchanged() {
        assert_eq!(normalize("Untouched ✨", &[]).text, "Untouched ✨");
    }

    #[test]
    fn validation_trims_and_deduplicates_related_values() {
        let validated = VocabularyInput {
            term: " Aeryndor ".to_owned(),
            pronunciation_hint: Some(" air-in-door ".to_owned()),
            kind: VocabularyKind::Person,
            description: Some(" Commander ".to_owned()),
            aliases: vec!["Aery".to_owned(), " aery ".to_owned()],
            forbidden_variants: vec!["Erin Door".to_owned()],
            case_sensitive: false,
        }
        .validate()
        .expect("valid input");
        assert_eq!(validated.term, "Aeryndor");
        assert_eq!(validated.aliases, vec!["Aery"]);
    }
}
