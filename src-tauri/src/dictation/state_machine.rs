use serde::Serialize;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DictationState {
    Idle,
    Starting,
    Listening,
    Finalizing,
    Transforming,
    Inserting,
    Completed,
    RecoverableError,
    FatalError,
}

#[derive(Debug, Eq, Error, PartialEq)]
#[error("invalid dictation transition from {from:?} to {to:?}")]
pub struct TransitionError {
    pub from: DictationState,
    pub to: DictationState,
}

#[derive(Debug)]
pub struct StateMachine {
    current: DictationState,
}

impl Default for StateMachine {
    fn default() -> Self {
        Self {
            current: DictationState::Idle,
        }
    }
}

impl StateMachine {
    pub const fn current(&self) -> DictationState {
        self.current
    }

    pub fn transition(&mut self, next: DictationState) -> Result<(), TransitionError> {
        if !is_allowed(self.current, next) {
            return Err(TransitionError {
                from: self.current,
                to: next,
            });
        }
        self.current = next;
        Ok(())
    }
}

const fn is_allowed(from: DictationState, to: DictationState) -> bool {
    matches!(
        (from, to),
        (DictationState::Idle, DictationState::Starting)
            | (DictationState::Idle, DictationState::FatalError)
            | (DictationState::Starting, DictationState::Listening)
            | (DictationState::Starting, DictationState::Finalizing)
            | (DictationState::Starting, DictationState::RecoverableError)
            | (DictationState::Starting, DictationState::FatalError)
            | (DictationState::Listening, DictationState::Finalizing)
            | (DictationState::Listening, DictationState::RecoverableError)
            | (DictationState::Listening, DictationState::FatalError)
            | (DictationState::Finalizing, DictationState::Completed)
            | (DictationState::Finalizing, DictationState::Inserting)
            | (DictationState::Finalizing, DictationState::Transforming)
            | (DictationState::Finalizing, DictationState::RecoverableError)
            | (DictationState::Finalizing, DictationState::FatalError)
            | (DictationState::Transforming, DictationState::Inserting)
            | (
                DictationState::Transforming,
                DictationState::RecoverableError
            )
            | (DictationState::Transforming, DictationState::FatalError)
            | (DictationState::Inserting, DictationState::Completed)
            | (DictationState::Inserting, DictationState::RecoverableError)
            | (DictationState::Inserting, DictationState::FatalError)
            | (DictationState::Completed, DictationState::Idle)
            | (DictationState::RecoverableError, DictationState::Idle)
    )
}

#[cfg(test)]
mod tests {
    use super::{DictationState, StateMachine, TransitionError};

    #[test]
    fn supports_local_recording_lifecycle() {
        let mut machine = StateMachine::default();
        for state in [
            DictationState::Starting,
            DictationState::Listening,
            DictationState::Finalizing,
            DictationState::Transforming,
            DictationState::Inserting,
            DictationState::Completed,
            DictationState::Idle,
        ] {
            machine
                .transition(state)
                .expect("transition should be valid");
        }
    }

    #[test]
    fn rejects_invalid_transition() {
        let mut machine = StateMachine::default();
        let error = machine
            .transition(DictationState::Listening)
            .expect_err("idle cannot transition directly to listening");

        assert_eq!(
            error,
            TransitionError {
                from: DictationState::Idle,
                to: DictationState::Listening,
            }
        );
        assert_eq!(machine.current(), DictationState::Idle);
    }

    #[test]
    fn recoverable_error_returns_to_idle() {
        let mut machine = StateMachine::default();
        machine
            .transition(DictationState::Starting)
            .expect("start should be valid");
        machine
            .transition(DictationState::RecoverableError)
            .expect("startup may fail recoverably");
        machine
            .transition(DictationState::Idle)
            .expect("recoverable errors should reset");

        assert_eq!(machine.current(), DictationState::Idle);
    }

    #[test]
    fn fatal_error_cannot_recover_implicitly() {
        let mut machine = StateMachine::default();
        machine
            .transition(DictationState::Starting)
            .expect("start should be valid");
        machine
            .transition(DictationState::FatalError)
            .expect("startup may fail fatally");

        assert!(machine.transition(DictationState::Idle).is_err());
    }

    #[test]
    fn supports_raw_transcript_insertion_without_transformation() {
        let mut machine = StateMachine::default();
        for state in [
            DictationState::Starting,
            DictationState::Listening,
            DictationState::Finalizing,
            DictationState::Inserting,
            DictationState::Completed,
        ] {
            machine
                .transition(state)
                .expect("valid insertion transition");
        }
    }
}
