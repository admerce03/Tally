//! `tally-server`: the home-server sync hub. Routes: `/health`, `/api/execute`,
//! `/api/sync/push`, `/api/sync/pull` and MCP over streamable HTTP at `/mcp`. All logic is in
//! core Commands and the store; this crate is HTTP plumbing and auth.

use std::io;
use std::sync::{Arc, Mutex, MutexGuard};

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Query, Request, State};
use axum::http::{HeaderName, HeaderValue, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use tally_core::json::command_from_value;
use tally_core::{CommandOutput, CoreError};
use tally_mcp::{AfterMutation, TallyServer};
use tally_store::{Store, StoreError, now_ms};
use tally_sync::protocol::{
    DEFAULT_PULL_LIMIT, ErrorBody, Health, MAX_PULL_LIMIT, MAX_PUSH_BATCH, PullQuery, PullResponse,
    PulledOp, PushRequest, PushResponse,
};
use tower_http::cors::{AllowOrigin, CorsLayer};

/// Request body limit for the JSON API.
const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

/// Origins of the desktop app (Tauri on each platform, and the Vite dev server).
pub const DEFAULT_CORS_ORIGINS: [&str; 4] = [
    "tauri://localhost",
    "http://tauri.localhost",
    "https://tauri.localhost",
    "http://localhost:5173",
];

#[derive(Clone)]
pub struct AppState {
    store: Arc<Mutex<Store>>,
    token: Arc<str>,
    cors_origins: Vec<String>,
    mcp_allowed_hosts: Vec<String>,
}

impl AppState {
    /// `cors_origins` and `mcp_allowed_hosts` are added to the built-in defaults.
    pub fn new(
        store: Store,
        token: String,
        cors_origins: Vec<String>,
        mcp_allowed_hosts: Vec<String>,
    ) -> Self {
        Self {
            store: Arc::new(Mutex::new(store)),
            token: Arc::from(token),
            cors_origins,
            mcp_allowed_hosts,
        }
    }

    /// The shared store (also used by `/mcp`).
    pub fn store(&self) -> Arc<Mutex<Store>> {
        self.store.clone()
    }
}

fn lock(store: &Mutex<Store>) -> MutexGuard<'_, Store> {
    store.lock().unwrap_or_else(|p| p.into_inner())
}

/// Register ops created locally (by a Command) so devices can pull them.
fn register_local(store: &mut Store) {
    if let Err(e) = store.server_log_register_local() {
        eprintln!("tally-server: could not register local ops: {e}");
    }
}

// ---- errors ----

struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(ErrorBody { error: self.1 })).into_response()
    }
}

impl From<StoreError> for ApiError {
    fn from(e: StoreError) -> Self {
        ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    }
}

enum ExecError {
    Core(CoreError),
    Store(StoreError),
}

impl From<CoreError> for ExecError {
    fn from(e: CoreError) -> Self {
        ExecError::Core(e)
    }
}

impl From<StoreError> for ExecError {
    fn from(e: StoreError) -> Self {
        ExecError::Store(e)
    }
}

impl From<ExecError> for ApiError {
    fn from(e: ExecError) -> Self {
        match e {
            ExecError::Core(e) => ApiError(StatusCode::BAD_REQUEST, e.to_string()),
            ExecError::Store(e) => e.into(),
        }
    }
}

/// Run blocking store work off the async runtime, under the shared lock.
async fn with_store<T: Send + 'static>(
    state: &AppState,
    f: impl FnOnce(&mut Store) -> Result<T, ApiError> + Send + 'static,
) -> Result<T, ApiError> {
    let store = state.store.clone();
    tokio::task::spawn_blocking(move || f(&mut lock(&store)))
        .await
        .map_err(|e| {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("task failed: {e}"),
            )
        })?
}

// ---- handlers ----

async fn health() -> Json<Health> {
    Json(Health {
        status: "ok".into(),
        version: env!("CARGO_PKG_VERSION").into(),
    })
}

async fn execute(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<Json<CommandOutput>, ApiError> {
    let value: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("invalid JSON: {e}")))?;
    let command = command_from_value(value).map_err(|e| ApiError(StatusCode::BAD_REQUEST, e))?;
    let out = with_store(&state, move |store| {
        let mutation = command.is_mutation();
        let out = store
            .transact(now_ms(), |tx| Ok::<_, ExecError>(tx.execute(command)?))
            .map_err(ApiError::from)?;
        if mutation {
            register_local(store);
        }
        Ok(out)
    })
    .await?;
    Ok(Json(out))
}

async fn push(State(state): State<AppState>, body: Bytes) -> Result<Json<PushResponse>, ApiError> {
    let req: PushRequest = serde_json::from_slice(&body)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("invalid push body: {e}")))?;
    if req.ops.len() > MAX_PUSH_BATCH {
        return Err(ApiError(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("at most {MAX_PUSH_BATCH} ops per push"),
        ));
    }
    let resp = with_store(&state, move |store| {
        let result = store.server_push(req.ops, now_ms())?;
        // Merging may bootstrap pre-log server data into a local op; publish it too.
        register_local(store);
        let accepted = result.accepted.iter().filter(|a| **a).count();
        Ok(PushResponse {
            accepted,
            duplicates: result.accepted.len() - accepted,
            head: store.server_log_head()?,
        })
    })
    .await?;
    Ok(Json(resp))
}

async fn pull(
    State(state): State<AppState>,
    Query(q): Query<PullQuery>,
) -> Result<Json<PullResponse>, ApiError> {
    let limit = q
        .limit
        .unwrap_or(DEFAULT_PULL_LIMIT)
        .clamp(1, MAX_PULL_LIMIT);
    let page = with_store(&state, move |store| {
        Ok(store.server_log_pull(q.since, limit)?)
    })
    .await?;
    Ok(Json(PullResponse {
        ops: page
            .ops
            .into_iter()
            .map(|(seq, op)| PulledOp { seq, op })
            .collect(),
        next: page.next,
        more: page.more,
    }))
}

// ---- auth ----

/// Constant-time equality (no early exit on the first differing byte).
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        diff |= usize::from(x ^ y);
    }
    diff == 0
}

async fn require_token(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let ok = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|t| constant_time_eq(t.as_bytes(), state.token.as_bytes()));
    if ok {
        next.run(req).await
    } else {
        ApiError(
            StatusCode::UNAUTHORIZED,
            "missing or invalid bearer token".into(),
        )
        .into_response()
    }
}

// ---- router ----

fn cors_layer(extra: &[String]) -> CorsLayer {
    let origins: Vec<HeaderValue> = DEFAULT_CORS_ORIGINS
        .iter()
        .map(|s| s.to_string())
        .chain(extra.iter().cloned())
        .filter_map(|o| HeaderValue::from_str(o.trim_end_matches('/')).ok())
        .collect();
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::ACCEPT,
            HeaderName::from_static("mcp-session-id"),
            HeaderName::from_static("mcp-protocol-version"),
        ])
        .expose_headers([HeaderName::from_static("mcp-session-id")])
}

fn mcp_service(state: &AppState) -> StreamableHttpService<TallyServer, LocalSessionManager> {
    let mut hosts: Vec<String> = ["localhost", "127.0.0.1", "::1"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    hosts.extend(state.mcp_allowed_hosts.iter().cloned());
    let config = StreamableHttpServerConfig::default().with_allowed_hosts(hosts);
    let store = state.store.clone();
    let hook: AfterMutation = Arc::new(register_local);
    StreamableHttpService::new(
        move || {
            TallyServer::with_shared(store.clone(), Some(hook.clone()))
                .map_err(|e| io::Error::other(e.to_string()))
        },
        Arc::new(LocalSessionManager::default()),
        config,
    )
}

pub fn router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/api/execute", post(execute))
        .route("/api/sync/push", post(push))
        .route("/api/sync/pull", get(pull))
        .nest_service("/mcp", mcp_service(&state))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(middleware::from_fn_with_state(state.clone(), require_token));
    // CORS is outermost so preflight requests never need a token.
    Router::new()
        .route("/health", get(health))
        .merge(protected)
        .with_state(state.clone())
        .layer(cors_layer(&state.cors_origins))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_eq_works() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(!constant_time_eq(b"", b"a"));
        assert!(constant_time_eq(b"", b""));
    }
}
