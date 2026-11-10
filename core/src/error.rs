use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum CoreError {
    #[error("command '{tag}' is not a mutation and cannot be applied as an op")]
    NotAMutation { tag: &'static str },
    #[error("unknown term {0}")]
    UnknownTerm(Uuid),
    #[error("unknown course {0}")]
    UnknownCourse(Uuid),
    #[error("unknown category {0}")]
    UnknownCategory(Uuid),
    #[error("unknown assignment {0}")]
    UnknownAssignment(Uuid),
    #[error("duplicate {kind} id {id}")]
    DuplicateId { kind: &'static str, id: Uuid },
    #[error("{field} must not be negative")]
    NegativePoints { field: &'static str },
    #[error("category weight must not be negative")]
    NegativeWeight,
    #[error("credits must not be negative")]
    NegativeCredits,
    #[error("{field} must be a finite number")]
    NonFinite { field: &'static str },
    #[error("{field} must be between 0 and 100")]
    OutOfRange { field: &'static str },
    #[error("year must be between 1000 and 9999")]
    InvalidYear,
    #[error("invalid curve: {reason}")]
    InvalidCurve { reason: &'static str },
    #[error("unknown scenario {0}")]
    UnknownScenario(Uuid),
    #[error("override index {index} out of range (scenario has {len})")]
    OverrideIndex { index: usize, len: usize },
    #[error("no target set for this course and none given")]
    NoTarget,
    #[error("assignment {0} is excused and cannot be locked")]
    LockedExcused(Uuid),
    #[error("course has no cutoff for letter {0}")]
    NoCutoff(&'static str),
    #[error("invalid letter '{0}' (expected one of A+ A A- B+ B B- C+ C C- D+ D F)")]
    InvalidLetter(String),
    #[error("invalid target '{0}' (expected a percent like 90, 90.5, 90% or a letter like A-)")]
    InvalidTarget(String),
    #[error("resolving a {kind} requires {needs}")]
    MissingContext {
        kind: &'static str,
        needs: &'static str,
    },
    #[error("unknown {kind} '{reference}'")]
    UnknownRef {
        kind: &'static str,
        reference: String,
    },
    #[error("ambiguous {kind} '{reference}', matches: {}", .candidates.join(", "))]
    AmbiguousRef {
        kind: &'static str,
        reference: String,
        candidates: Vec<String>,
    },
}
