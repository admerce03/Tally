//! Tally desktop app backend: one generic `execute` command over core `Command`s.
//!
//! Adapter only: it parses JSON into a core `Command`, runs it in `Store::transact` (same path
//! as the MCP server) and returns the `CommandOutput` as JSON. No grade logic lives here.

use std::sync::{Arc, Mutex};

use anyhow::Context;
use serde_json::Value;
use tally_core::json::command_from_value;
use tally_core::{CommandOutput, CoreError};
use tally_store::{Store, StoreError, now_ms, resolve_db_path};
use tauri::State;

/// Shared, lock-protected database handle held in Tauri managed state.
pub struct AppState {
    store: Arc<Mutex<Store>>,
}

impl AppState {
    pub fn new(store: Store) -> Self {
        Self {
            store: Arc::new(Mutex::new(store)),
        }
    }
}

enum RunError {
    Core(CoreError),
    Store(StoreError),
}

impl From<StoreError> for RunError {
    fn from(e: StoreError) -> Self {
        RunError::Store(e)
    }
}

impl From<CoreError> for RunError {
    fn from(e: CoreError) -> Self {
        RunError::Core(e)
    }
}

impl RunError {
    fn message(&self) -> String {
        match self {
            RunError::Core(e) => e.to_string(),
            RunError::Store(e) => e.to_string(),
        }
    }
}

/// Parse and run one command against the store. Blocking.
pub fn run_command(store: &Mutex<Store>, command: Value) -> Result<CommandOutput, String> {
    let command = command_from_value(command)?;
    let mut store = store.lock().unwrap_or_else(|p| p.into_inner());
    store
        .transact(now_ms(), |tx| Ok(tx.execute(command)?))
        .map_err(|e: RunError| e.message())
}

/// The single Tauri command the UI calls: JSON `Command` in, JSON `CommandOutput` out.
#[tauri::command]
async fn execute(state: State<'_, AppState>, command: Value) -> Result<Value, String> {
    let store = Arc::clone(&state.store);
    let out = tauri::async_runtime::spawn_blocking(move || run_command(&store, command))
        .await
        .map_err(|e| format!("command task failed: {e}"))??;
    serde_json::to_value(out).map_err(|e| format!("could not encode result: {e}"))
}

pub fn run() -> anyhow::Result<()> {
    let path = resolve_db_path(None).context("resolving database path")?;
    let store = Store::open(&path).with_context(|| format!("opening {}", path.display()))?;
    tauri::Builder::default()
        .manage(AppState::new(store))
        .invoke_handler(tauri::generate_handler![execute])
        .run(tauri::generate_context!())
        .context("running Tauri application")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn store() -> Mutex<Store> {
        Mutex::new(Store::open_in_memory().unwrap())
    }

    #[test]
    fn mutation_persists_and_query_reads_it() {
        let s = store();
        let id = "11111111-1111-4111-8111-111111111111";
        let out = run_command(&s, json!({"command": "add_term", "id": id, "name": "Fall"}));
        assert!(matches!(out, Ok(CommandOutput::Created { .. })));
        let out = run_command(&s, json!({"command": "list_terms"})).unwrap();
        let v = serde_json::to_value(out).unwrap();
        assert_eq!(v["terms"][0]["name"], "Fall");
    }

    #[test]
    fn bad_input_and_core_errors_are_strings() {
        let s = store();
        let e = run_command(&s, json!({"command": "list_terms", "bogus": 1})).unwrap_err();
        assert!(e.contains("bogus"), "{e}");
        let e = run_command(
            &s,
            json!({"command": "rename_term", "id": "22222222-2222-4222-8222-222222222222", "name": "x"}),
        )
        .unwrap_err();
        assert!(e.contains("unknown term"), "{e}");
    }

    #[test]
    fn failed_command_does_not_persist() {
        let s = store();
        let _ = run_command(&s, json!({"command": "nope"}));
        let out = run_command(&s, json!({"command": "list_terms"})).unwrap();
        assert_eq!(serde_json::to_value(out).unwrap()["terms"], json!([]));
    }

    /// The committed schemas the TS types are generated from must match core.
    #[test]
    fn committed_schemas_are_fresh() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("ui")
            .join("src")
            .join("lib")
            .join("generated");
        for (file, current) in [
            (
                "command.schema.json",
                tally_core::json::command_schema_json(),
            ),
            ("output.schema.json", tally_core::json::output_schema_json()),
        ] {
            let committed = std::fs::read_to_string(dir.join(file))
                .unwrap_or_else(|e| panic!("{file}: {e}. Run `npm run gen:types` in app/ui"))
                .replace("\r\n", "\n");
            assert_eq!(
                committed, current,
                "{file} is stale. Run `npm run gen:types` in app/ui and commit the result"
            );
        }
    }
}
