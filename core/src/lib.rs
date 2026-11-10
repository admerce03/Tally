//! Tally core: domain model, grade math and typed Commands. Pure: no IO, clock or randomness.

pub mod command;
pub mod error;
pub mod grade;
pub mod ids;
pub mod model;

#[cfg(test)]
mod testutil;

pub use command::{Command, CommandOutput, execute};
pub use error::CoreError;
pub use ids::{AssignmentId, CategoryId, CourseId, ScenarioId, TermId};
pub use model::*;
