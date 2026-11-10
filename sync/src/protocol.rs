//! Wire types for the sync HTTP API. Used by both the client and the server.

use serde::{Deserialize, Serialize};
use tally_core::Op;

/// Most ops accepted in one push.
pub const MAX_PUSH_BATCH: usize = 500;
/// Page size used when `limit` is omitted.
pub const DEFAULT_PULL_LIMIT: usize = 500;
/// Largest page the server returns.
pub const MAX_PULL_LIMIT: usize = 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushRequest {
    pub ops: Vec<Op>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushResponse {
    /// Ops the server had not seen before.
    pub accepted: usize,
    /// Ops the server already had.
    pub duplicates: usize,
    /// Server sequence head after the push.
    pub head: i64,
}

/// Query string of `GET /api/sync/pull`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullQuery {
    #[serde(default)]
    pub since: i64,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PulledOp {
    pub seq: i64,
    pub op: Op,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullResponse {
    pub ops: Vec<PulledOp>,
    /// Cursor to use as `since` next time.
    pub next: i64,
    pub more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Health {
    pub status: String,
    pub version: String,
}
