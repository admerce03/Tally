use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum CoreError {
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
    #[error("invalid curve: {reason}")]
    InvalidCurve { reason: &'static str },
}
