//! HTTP API tests driving the router in-process.

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tally_server::{AppState, router};
use tally_store::Store;
use tower::ServiceExt;

const TOKEN: &str = "secret-token";

fn app() -> Router {
    router(AppState::new(
        Store::open_in_memory().unwrap(),
        TOKEN.into(),
        vec![],
        vec![],
    ))
}

async fn call(app: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn authed(method: &str, uri: &str, body: Option<Value>) -> Request<Body> {
    let b = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {TOKEN}"));
    match body {
        Some(v) => b
            .header("content-type", "application/json")
            .body(Body::from(v.to_string()))
            .unwrap(),
        None => b.body(Body::empty()).unwrap(),
    }
}

fn add_term(n: u32) -> Value {
    json!({"command":"add_term","id":format!("10000000-0000-4000-8000-{n:012}"),"name":format!("T{n}")})
}

#[tokio::test]
async fn health_needs_no_auth() {
    let (s, v) = call(&app(), Request::get("/health").body(Body::empty()).unwrap()).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["status"], "ok");
}

#[tokio::test]
async fn auth_is_required() {
    let a = app();
    let (s, v) = call(
        &a,
        Request::get("/api/sync/pull").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert!(v["error"].is_string());
    let bad = Request::get("/api/sync/pull")
        .header("authorization", "Bearer wrong")
        .body(Body::empty())
        .unwrap();
    assert_eq!(call(&a, bad).await.0, StatusCode::UNAUTHORIZED);
    let (s, _) = call(&a, authed("GET", "/api/sync/pull?since=0", None)).await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test]
async fn preflight_needs_no_auth() {
    let req = Request::builder()
        .method("OPTIONS")
        .uri("/api/execute")
        .header("origin", "tauri://localhost")
        .header("access-control-request-method", "POST")
        .header(
            "access-control-request-headers",
            "authorization,content-type",
        )
        .body(Body::empty())
        .unwrap();
    let resp = app().oneshot(req).await.unwrap();
    assert!(resp.status().is_success(), "{}", resp.status());
    assert_eq!(
        resp.headers().get("access-control-allow-origin").unwrap(),
        "tauri://localhost"
    );
}

#[tokio::test]
async fn execute_runs_commands_and_reports_errors() {
    let a = app();
    let (s, v) = call(&a, authed("POST", "/api/execute", Some(add_term(1)))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["result"], "created");
    let (s, v) = call(
        &a,
        authed(
            "POST",
            "/api/execute",
            Some(json!({"command":"list_terms"})),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["terms"][0]["name"], "T1");
    let (s, v) = call(
        &a,
        authed("POST", "/api/execute", Some(json!({"command":"nope"}))),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(v["error"].is_string());
    let rename =
        json!({"command":"rename_term","id":"99999999-0000-4000-8000-000000000000","name":"x"});
    let (s, _) = call(&a, authed("POST", "/api/execute", Some(rename))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn execute_ops_appear_in_pull() {
    let a = app();
    call(&a, authed("POST", "/api/execute", Some(add_term(1)))).await;
    call(
        &a,
        authed(
            "POST",
            "/api/execute",
            Some(json!({"command":"list_terms"})),
        ),
    )
    .await;
    let (_, v) = call(&a, authed("GET", "/api/sync/pull?since=0", None)).await;
    assert_eq!(
        v["ops"].as_array().unwrap().len(),
        1,
        "queries append no op"
    );
    assert_eq!(v["ops"][0]["op"]["command"]["command"], "add_term");
    assert_eq!(v["more"], false);
}

#[tokio::test]
async fn push_is_idempotent_and_pull_pages() {
    // Produce real ops on a scratch server, then push them to the app under test.
    let src = app();
    for n in 1..=5 {
        call(&src, authed("POST", "/api/execute", Some(add_term(n)))).await;
    }
    let (_, pulled) = call(&src, authed("GET", "/api/sync/pull?since=0", None)).await;
    let ops: Vec<Value> = pulled["ops"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["op"].clone())
        .collect();

    let a = app();
    let body = json!({ "ops": ops });
    let (s, v) = call(&a, authed("POST", "/api/sync/push", Some(body.clone()))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(
        (
            v["accepted"].clone(),
            v["duplicates"].clone(),
            v["head"].clone()
        ),
        (json!(5), json!(0), json!(5))
    );
    let (_, v) = call(&a, authed("POST", "/api/sync/push", Some(body))).await;
    assert_eq!(
        (
            v["accepted"].clone(),
            v["duplicates"].clone(),
            v["head"].clone()
        ),
        (json!(0), json!(5), json!(5))
    );

    let (_, v) = call(&a, authed("GET", "/api/sync/pull?since=0&limit=2", None)).await;
    assert_eq!(v["ops"].as_array().unwrap().len(), 2);
    assert_eq!(
        (v["next"].clone(), v["more"].clone()),
        (json!(2), json!(true))
    );
    let (_, v) = call(&a, authed("GET", "/api/sync/pull?since=2&limit=10", None)).await;
    assert_eq!(v["ops"].as_array().unwrap().len(), 3);
    assert_eq!(
        (v["next"].clone(), v["more"].clone()),
        (json!(5), json!(false))
    );
    assert_eq!(v["ops"][0]["seq"], 3);
}

#[tokio::test]
async fn oversized_push_is_rejected() {
    let a = app();
    let (_, pulled) = {
        call(&a, authed("POST", "/api/execute", Some(add_term(1)))).await;
        call(&a, authed("GET", "/api/sync/pull?since=0", None)).await
    };
    let op = pulled["ops"][0]["op"].clone();
    let ops: Vec<Value> = std::iter::repeat_n(op, 501).collect();
    let (s, _) = call(
        &a,
        authed("POST", "/api/sync/push", Some(json!({"ops": ops}))),
    )
    .await;
    assert_eq!(s, StatusCode::PAYLOAD_TOO_LARGE);
}

fn mcp_init(token: Option<&str>) -> Request<Body> {
    let mut b = Request::post("/mcp")
        .header("host", "localhost")
        .header("accept", "application/json, text/event-stream")
        .header("content-type", "application/json");
    if let Some(t) = token {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    let body = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion":"2025-03-26","capabilities":{},
        "clientInfo":{"name":"test","version":"0"}}});
    b.body(Body::from(body.to_string())).unwrap()
}

#[tokio::test]
async fn mcp_requires_token_and_initializes() {
    let a = app();
    assert_eq!(call(&a, mcp_init(None)).await.0, StatusCode::UNAUTHORIZED);
    let resp = a.clone().oneshot(mcp_init(Some(TOKEN))).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
