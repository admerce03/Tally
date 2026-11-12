//! Blocking HTTP client for the sync server.

use std::time::Duration;

use tally_core::Op;

use crate::protocol::{ErrorBody, Health, PullResponse, PushRequest, PushResponse};
use crate::{SyncConfig, SyncError, Transport};

pub struct SyncClient {
    agent: ureq::Agent,
    base: String,
    auth: String,
}

impl SyncClient {
    pub fn new(config: &SyncConfig) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .build()
            .into();
        Self {
            agent,
            base: config.server.trim_end_matches('/').to_string(),
            auth: format!("Bearer {}", config.token),
        }
    }

    /// `GET /health` (no auth needed, but sent anyway).
    pub fn health(&self) -> Result<Health, SyncError> {
        let resp = self
            .agent
            .get(format!("{}/health", self.base))
            .header("Authorization", &self.auth)
            .call()
            .map_err(transport)?;
        decode(resp)
    }
}

fn transport(e: ureq::Error) -> SyncError {
    SyncError::Transport(e.to_string())
}

/// Map non-2xx to errors and decode a JSON body otherwise.
fn decode<T: serde::de::DeserializeOwned>(
    mut resp: ureq::http::Response<ureq::Body>,
) -> Result<T, SyncError> {
    let status = resp.status().as_u16();
    if (200..300).contains(&status) {
        return resp
            .body_mut()
            .read_json::<T>()
            .map_err(|e| SyncError::Transport(format!("invalid response: {e}")));
    }
    if status == 401 {
        return Err(SyncError::Unauthorized);
    }
    let text = resp.body_mut().read_to_string().unwrap_or_default();
    let message = serde_json::from_str::<ErrorBody>(&text)
        .map(|b| b.error)
        .unwrap_or(text);
    Err(SyncError::Http { status, message })
}

impl Transport for SyncClient {
    fn push(&self, ops: &[Op]) -> Result<PushResponse, SyncError> {
        let body = PushRequest { ops: ops.to_vec() };
        let resp = self
            .agent
            .post(format!("{}/api/sync/push", self.base))
            .header("Authorization", &self.auth)
            .send_json(&body)
            .map_err(transport)?;
        decode(resp)
    }

    fn pull(&self, since: i64, limit: usize) -> Result<PullResponse, SyncError> {
        let resp = self
            .agent
            .get(format!("{}/api/sync/pull", self.base))
            .query("since", since.to_string())
            .query("limit", limit.to_string())
            .header("Authorization", &self.auth)
            .call()
            .map_err(transport)?;
        decode(resp)
    }
}
