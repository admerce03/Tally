//! Drives the built `tally-mcp` binary over stdio with raw JSON-RPC.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

const TIMEOUT: Duration = Duration::from_secs(30);

struct Client {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    next_id: u64,
    db: PathBuf,
}

impl Client {
    fn spawn() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let db =
            std::env::temp_dir().join(format!("tally-mcp-test-{}-{nanos}.db", std::process::id()));
        let mut child = Command::new(env!("CARGO_BIN_EXE_tally-mcp"))
            .arg("--db")
            .arg(&db)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn tally-mcp");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        let (tx, lines) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Client {
            child,
            stdin,
            lines,
            next_id: 1,
            db,
        }
    }

    fn send(&mut self, msg: &Value) {
        writeln!(self.stdin, "{msg}").expect("write request");
        self.stdin.flush().expect("flush");
    }

    fn notify(&mut self, method: &str) {
        self.send(&json!({"jsonrpc": "2.0", "method": method}));
    }

    /// Send a request and return the response with the matching id (skipping notifications).
    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        loop {
            let line = self
                .lines
                .recv_timeout(TIMEOUT)
                .expect("timed out waiting for the server");
            let msg: Value = serde_json::from_str(&line).expect("server wrote non-JSON to stdout");
            if msg.get("id") == Some(&json!(id)) {
                return msg;
            }
        }
    }

    fn call(&mut self, name: &str, arguments: Value) -> Value {
        let resp = self.request("tools/call", json!({"name": name, "arguments": arguments}));
        assert!(resp.get("error").is_none(), "protocol error: {resp}");
        resp["result"].clone()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.db);
    }
}

fn initialized() -> Client {
    let mut c = Client::spawn();
    let resp = c.request(
        "initialize",
        json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "0"}
        }),
    );
    let result = &resp["result"];
    assert_eq!(result["serverInfo"]["name"], "tally", "{resp}");
    assert!(result["capabilities"]["tools"].is_object(), "{resp}");
    assert!(
        result["instructions"]
            .as_str()
            .is_some_and(|s| s.contains("resolve")),
        "{resp}"
    );
    c.notify("notifications/initialized");
    c
}

fn structured(result: &Value) -> &Value {
    assert_ne!(result["isError"], json!(true), "{result}");
    &result["structuredContent"]
}

#[test]
fn lists_tools_and_runs_calls() {
    let mut c = initialized();

    let resp = c.request("tools/list", json!({}));
    let tools = resp["result"]["tools"].as_array().expect("tools array");
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    for expected in [
        "add_term",
        "list_terms",
        "resolve",
        "solve_target",
        "cumulative_gpa",
    ] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    for t in tools {
        assert_eq!(t["inputSchema"]["type"], "object", "{t}");
        assert!(
            t["inputSchema"]["properties"].get("command").is_none(),
            "{t}"
        );
        assert!(
            t["description"].as_str().is_some_and(|d| !d.is_empty()),
            "{t}"
        );
    }

    let id = "44444444-4444-4444-8444-444444444444";
    let added = c.call("add_term", json!({"id": id, "name": "Fall 2026"}));
    assert_eq!(structured(&added)["id"], id);
    assert!(
        added["content"][0]["text"]
            .as_str()
            .is_some_and(|t| t.contains(id)),
        "{added}"
    );

    let listed = c.call("list_terms", json!({}));
    assert_eq!(structured(&listed)["terms"][0]["name"], "Fall 2026");

    let resolved = c.call("resolve", json!({"kind": "term", "reference": "fall 2026"}));
    assert_eq!(structured(&resolved)["id"], id);
}

#[test]
fn failing_calls_return_is_error() {
    let mut c = initialized();

    // Core error (unknown id).
    let r = c.call(
        "remove_term",
        json!({"id": "55555555-5555-4555-8555-555555555555"}),
    );
    assert_eq!(r["isError"], json!(true), "{r}");
    assert!(
        r["content"][0]["text"]
            .as_str()
            .is_some_and(|t| t.contains("unknown term")),
        "{r}"
    );

    // Bad arguments are a tool error, not a protocol error.
    let r = c.call("add_term", json!({"name": "no id"}));
    assert_eq!(r["isError"], json!(true), "{r}");

    // The server is still alive afterwards.
    let listed = c.call("list_terms", json!({}));
    assert_eq!(structured(&listed)["terms"], json!([]));
}
