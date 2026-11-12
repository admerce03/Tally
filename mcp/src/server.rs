//! The MCP server handler: lists the generated tools and runs calls through `Store::transact`.

use std::fmt;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
        ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    },
    service::RequestContext,
};
use serde_json::{Map, Value};
use tally_core::json::command_from_value;
use tally_core::{Command, CommandOutput, CoreError};
use tally_store::{Store, StoreError, now_ms};

use crate::tools::{TAG, build_tools};

const INSTRUCTIONS: &str = "\
Tally is a grades tracker: terms contain courses, courses contain weighted categories, \
categories contain assignments. Every tool is one Tally command.\n\
IDs: tools take UUIDs. New entities (add_term, add_course, add_category, add_assignment, \
create_scenario) need a fresh UUID v4 that you generate and pass as `id`; each add returns \
`{result: created, id}` so you can chain calls. To discover existing ids call list_terms, then \
show_course (categories and assignments with ids) and list_scenarios. To turn a name, course \
code or UUID prefix into an id call the `resolve` tool (kind: term, course, category, \
assignment or scenario; category needs `course`).\n\
Units: all percentages and grades are numbers from 0 to 100 (not 0 to 1). Scores and points \
possible are raw points. Dates are YYYY-MM-DD. Curves and what-if scenarios are assumptions \
and never change stored scores. Errors come back as tool errors with a message.";

/// A tool call failed: either the command (core) or the database (store).
#[derive(Debug)]
enum CallError {
    Core(CoreError),
    Store(StoreError),
}

impl From<StoreError> for CallError {
    fn from(e: StoreError) -> Self {
        CallError::Store(e)
    }
}

impl From<CoreError> for CallError {
    fn from(e: CoreError) -> Self {
        CallError::Core(e)
    }
}

impl fmt::Display for CallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CallError::Core(e) => e.fmt(f),
            CallError::Store(e) => e.fmt(f),
        }
    }
}

/// Called under the store lock after a successful mutating command (used by the sync server
/// to register the new local ops).
pub type AfterMutation = Arc<dyn Fn(&mut Store) + Send + Sync>;

#[derive(Clone)]
pub struct TallyServer {
    store: Arc<Mutex<Store>>,
    tools: Arc<Vec<Tool>>,
    after_mutation: Option<AfterMutation>,
}

fn error_result(message: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(message.into())])
}

impl TallyServer {
    pub fn new(store: Store) -> Result<Self> {
        Self::with_shared(Arc::new(Mutex::new(store)), None)
    }

    /// Serve an already shared store. `after_mutation` runs under the store lock after every
    /// successful mutating command.
    pub fn with_shared(
        store: Arc<Mutex<Store>>,
        after_mutation: Option<AfterMutation>,
    ) -> Result<Self> {
        Ok(Self {
            store,
            tools: Arc::new(build_tools()?),
            after_mutation,
        })
    }

    pub fn tools(&self) -> &[Tool] {
        &self.tools
    }

    /// Run one tool call. Bad arguments, unknown tools and command failures become tool
    /// errors (`isError`), never protocol errors. Blocking: wraps one `Store::transact`.
    pub fn run_tool(&self, name: &str, arguments: Option<Map<String, Value>>) -> CallToolResult {
        if !self.tools.iter().any(|t| t.name == name) {
            return error_result(format!("unknown tool '{name}'"));
        }
        let mut args = arguments.unwrap_or_default();
        // The tag is the tool name; callers may not smuggle in a different command.
        if args.contains_key(TAG) {
            return error_result(format!(
                "unknown argument(s) for '{name}': {TAG}. The command is the tool name"
            ));
        }
        args.insert(TAG.to_string(), Value::String(name.to_string()));
        let command: Command = match command_from_value(Value::Object(args)) {
            Ok(c) => c,
            Err(e) => return error_result(e),
        };
        let result: Result<CommandOutput, CallError> = {
            let mut store = self.store.lock().unwrap_or_else(|p| p.into_inner());
            let mutation = command.is_mutation();
            let out = store.transact(now_ms(), |tx| Ok(tx.execute(command)?));
            if let (true, Ok(_), Some(hook)) = (mutation, &out, &self.after_mutation) {
                hook(&mut store);
            }
            out
        };
        match result {
            Ok(out) => match serde_json::to_value(&out) {
                Ok(v) => CallToolResult::structured(v),
                Err(e) => error_result(format!("could not encode result: {e}")),
            },
            Err(e) => error_result(e.to_string()),
        }
    }
}

impl ServerHandler for TallyServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("tally", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(self.tools.to_vec()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let server = self.clone();
        let name = request.name.to_string();
        let result =
            tokio::task::spawn_blocking(move || server.run_tool(&name, request.arguments)).await;
        Ok(join_to_result(result).into())
    }
}

/// A panicking or cancelled tool task becomes a tool error, not a protocol error.
fn join_to_result(result: Result<CallToolResult, tokio::task::JoinError>) -> CallToolResult {
    result.unwrap_or_else(|e| error_result(format!("tool task failed: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn server() -> TallyServer {
        TallyServer::new(Store::open_in_memory().unwrap()).unwrap()
    }

    fn args(v: Value) -> Option<Map<String, Value>> {
        v.as_object().cloned()
    }

    fn text(r: &CallToolResult) -> String {
        serde_json::to_string(&r.content).unwrap()
    }

    #[test]
    fn add_term_then_list_terms_round_trip() {
        let s = server();
        let id = "11111111-1111-4111-8111-111111111111";
        let r = s.run_tool("add_term", args(json!({"id": id, "name": "Fall"})));
        assert_eq!(r.is_error, Some(false), "{}", text(&r));
        let out = r.structured_content.unwrap();
        assert_eq!(out["result"], "created");
        assert_eq!(out["kind"], "term");
        assert_eq!(out["id"], id);

        let r = s.run_tool("list_terms", None);
        assert_eq!(r.is_error, Some(false));
        assert!(text(&r).contains("Fall"), "text content mirrors structured");
        let out = r.structured_content.unwrap();
        assert_eq!(out["terms"][0]["name"], "Fall");

        let r = s.run_tool(
            "resolve",
            args(json!({"kind": "term", "reference": "fall"})),
        );
        let out = r.structured_content.unwrap();
        assert_eq!(out["result"], "resolved");
        assert_eq!(out["id"], id);
        assert_eq!(out["name"], "Fall");
    }

    #[test]
    fn failures_are_tool_errors() {
        let s = server();
        // Bad arguments: missing required field, wrong type, unknown tool.
        for (name, a) in [
            ("add_term", json!({"name": "x"})),
            ("add_term", json!({"id": "not-a-uuid", "name": "x"})),
            ("nope", json!({})),
        ] {
            let r = s.run_tool(name, args(a));
            assert_eq!(r.is_error, Some(true), "{name}");
            assert!(!text(&r).is_empty());
        }
        // Core error: unknown id.
        let r = s.run_tool(
            "rename_term",
            args(json!({"id": "22222222-2222-4222-8222-222222222222", "name": "x"})),
        );
        assert_eq!(r.is_error, Some(true));
        assert!(text(&r).contains("unknown term"), "{}", text(&r));
        // A failed call leaves the database untouched.
        let r = s.run_tool("list_terms", None);
        assert_eq!(r.structured_content.unwrap()["terms"], json!([]));
    }

    #[test]
    fn unknown_arguments_are_rejected_with_allowed_list() {
        let s = server();
        let r = s.run_tool(
            "add_term",
            args(json!({"id": "x", "name": "n", "bogus": 1})),
        );
        assert_eq!(r.is_error, Some(true));
        let t = text(&r);
        assert!(
            t.contains("bogus") && t.contains("Allowed") && t.contains("name"),
            "{t}"
        );
        // The tag key may not be smuggled in to run a different command.
        let r = s.run_tool("list_terms", args(json!({"command": "remove_term"})));
        assert_eq!(r.is_error, Some(true));
        assert!(text(&r).contains("command"));
    }

    #[tokio::test]
    async fn panicking_task_becomes_tool_error() {
        let joined = tokio::task::spawn_blocking(|| -> CallToolResult { panic!("boom") }).await;
        assert!(joined.is_err());
        let r = join_to_result(joined);
        assert_eq!(r.is_error, Some(true));
        assert!(text(&r).contains("tool task failed"));
    }

    #[test]
    fn mutations_persist_across_calls() {
        let s = server();
        let id = "33333333-3333-4333-8333-333333333333";
        s.run_tool("add_term", args(json!({"id": id, "name": "A"})));
        s.run_tool("rename_term", args(json!({"id": id, "name": "B"})));
        let r = s.run_tool("list_terms", None);
        assert_eq!(r.structured_content.unwrap()["terms"][0]["name"], "B");
    }

    #[test]
    fn server_info_mentions_ids_and_resolve() {
        let info = server().get_info();
        assert_eq!(info.server_info.name, "tally");
        let ins = info.instructions.unwrap();
        assert!(ins.contains("resolve") && ins.contains("0 to 100"));
        assert!(info.capabilities.tools.is_some());
    }
}
