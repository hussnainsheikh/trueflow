#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TranscriptAccumulator {
    finalized_segments: Vec<String>,
    interim: String,
    operation_final: bool,
}

impl TranscriptAccumulator {
    pub fn replace_partial(&mut self, text: String) {
        self.interim = text.trim().to_owned();
    }

    pub fn add_final_segment(&mut self, text: String, operation_final: bool) {
        let text = text.trim();
        if !text.is_empty()
            && self
                .finalized_segments
                .last()
                .is_none_or(|existing| existing != text)
        {
            self.finalized_segments.push(text.to_owned());
        }
        self.interim.clear();
        self.operation_final |= operation_final;
    }

    pub fn display_text(&self) -> String {
        join_nonempty(
            self.finalized_segments
                .iter()
                .map(String::as_str)
                .chain([self.interim.as_str()]),
        )
    }

    pub fn best_known_text(&self) -> String {
        self.display_text()
    }

    pub const fn is_operation_final(&self) -> bool {
        self.operation_final
    }
}

fn join_nonempty<'a>(parts: impl Iterator<Item = &'a str>) -> String {
    parts
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::TranscriptAccumulator;

    #[test]
    fn revised_partials_replace_instead_of_append() {
        let mut transcript = TranscriptAccumulator::default();
        transcript.replace_partial("I think".to_owned());
        transcript.replace_partial("I think we should".to_owned());

        assert_eq!(transcript.display_text(), "I think we should");
    }

    #[test]
    fn final_segments_and_partial_do_not_overlap() {
        let mut transcript = TranscriptAccumulator::default();
        transcript.replace_partial("Hello there".to_owned());
        transcript.add_final_segment("Hello there".to_owned(), false);
        transcript.replace_partial("general Kenobi".to_owned());

        assert_eq!(transcript.display_text(), "Hello there general Kenobi");
    }

    #[test]
    fn duplicate_final_segment_is_ignored() {
        let mut transcript = TranscriptAccumulator::default();
        transcript.add_final_segment("Same words".to_owned(), false);
        transcript.add_final_segment("Same words".to_owned(), true);

        assert_eq!(transcript.display_text(), "Same words");
        assert!(transcript.is_operation_final());
    }
}
