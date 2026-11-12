//! `tally-mcp`: MCP (stdio) adapter over tally-core Commands.
//!
//! One tool per `Command` variant, generated from the schemars schema of `Command` so a new
//! Command is automatically a new tool. No grade math lives here.

pub mod server;
pub mod tools;

pub use server::{AfterMutation, TallyServer};
