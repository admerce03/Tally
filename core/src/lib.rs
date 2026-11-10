//! Tally core: domain model, grade math and typed Commands. Pure: no IO, clock or randomness.

pub mod analysis;
pub mod command;
pub mod error;
pub mod gpa;
pub mod grade;
pub mod ids;
#[cfg(feature = "schema")]
pub mod json;
pub mod model;
pub mod refs;
pub mod scenario;
pub mod solver;

#[cfg(test)]
mod testutil;

pub use command::{Command, CommandOutput, CourseView, EntityKind, execute};
pub use error::CoreError;
pub use ids::{AssignmentId, CategoryId, CourseId, ScenarioId, TermId};
pub use model::*;
pub use refs::{
    resolve_assignment, resolve_category, resolve_course, resolve_scenario, resolve_term,
};
