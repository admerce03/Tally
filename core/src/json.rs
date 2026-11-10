//! JSON -> `Command` with clear errors, shared by the MCP and desktop adapters.
//!
//! Plain serde ignores unknown fields; agents and UIs get much better feedback when a typo
//! is rejected with the list of allowed fields. The allowed fields come from the `Command`
//! schema, so adding a variant needs no change here.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde_json::Value;

use crate::{Command, CommandOutput};

/// The tag property of `Command` (`#[serde(tag = "command")]`).
pub const TAG: &str = "command";

/// Allowed field names (excluding the tag) per command name, in schema order.
fn allowed_fields() -> &'static BTreeMap<String, Vec<String>> {
    static CACHE: OnceLock<BTreeMap<String, Vec<String>>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let schema = serde_json::to_value(schemars::schema_for!(Command)).unwrap_or(Value::Null);
        let mut out = BTreeMap::new();
        let variants = schema
            .get("oneOf")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for variant in variants {
            let Some(props) = variant.get("properties").and_then(Value::as_object) else {
                continue;
            };
            let Some(tag) = props
                .get(TAG)
                .and_then(|p| p.get("const"))
                .and_then(Value::as_str)
            else {
                continue;
            };
            let fields = props.keys().filter(|k| *k != TAG).cloned().collect();
            out.insert(tag.to_string(), fields);
        }
        out
    })
}

/// Pretty-printed JSON schema of [`Command`] (with trailing newline), for TS type generation.
pub fn command_schema_json() -> String {
    pretty(serde_json::to_value(schemars::schema_for!(Command)))
}

/// Pretty-printed JSON schema of [`CommandOutput`] (with trailing newline).
pub fn output_schema_json() -> String {
    pretty(serde_json::to_value(schemars::schema_for!(CommandOutput)))
}

fn pretty(value: Result<Value, serde_json::Error>) -> String {
    let v = value.unwrap_or(Value::Null);
    let mut s = serde_json::to_string_pretty(&v).unwrap_or_default();
    s.push('\n');
    s
}

/// Parse a JSON object (tagged with `command`) into a [`Command`].
///
/// Errors are human-readable strings: missing/unknown command, unknown fields (with the
/// allowed list) or the serde error for wrong/missing field values.
pub fn command_from_value(value: Value) -> Result<Command, String> {
    let obj = value
        .as_object()
        .ok_or_else(|| "command must be a JSON object".to_string())?;
    let name = obj
        .get(TAG)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing string field `{TAG}` naming the command"))?
        .to_string();
    let allowed = allowed_fields()
        .get(&name)
        .ok_or_else(|| format!("unknown command '{name}'"))?;
    let unknown: Vec<&str> = obj
        .keys()
        .map(String::as_str)
        .filter(|k| *k != TAG && !allowed.iter().any(|a| a == k))
        .collect();
    if !unknown.is_empty() {
        return Err(format!(
            "unknown argument(s) for '{name}': {}. Allowed: {}",
            unknown.join(", "),
            if allowed.is_empty() {
                "(none)".to_string()
            } else {
                allowed.join(", ")
            }
        ));
    }
    serde_json::from_value(value).map_err(|e| format!("invalid arguments for '{name}': {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_valid_commands() {
        let c = command_from_value(json!({"command": "list_terms"})).unwrap();
        assert!(matches!(c, Command::ListTerms));
        let c = command_from_value(json!({
            "command": "add_term",
            "id": "11111111-1111-4111-8111-111111111111",
            "name": "Fall"
        }))
        .unwrap();
        assert!(matches!(c, Command::AddTerm { .. }));
    }

    #[test]
    fn rejects_bad_input_with_clear_messages() {
        let e = command_from_value(json!([])).unwrap_err();
        assert!(e.contains("object"));
        let e = command_from_value(json!({})).unwrap_err();
        assert!(e.contains("command"));
        let e = command_from_value(json!({"command": "nope"})).unwrap_err();
        assert!(e.contains("unknown command 'nope'"));
        let e = command_from_value(json!({"command": "add_term", "name": "x"})).unwrap_err();
        assert!(e.contains("invalid arguments for 'add_term'"), "{e}");
        let e = command_from_value(json!({
            "command": "add_term",
            "id": "11111111-1111-4111-8111-111111111111",
            "name": "x",
            "bogus": 1
        }))
        .unwrap_err();
        assert!(
            e.contains("bogus") && e.contains("Allowed") && e.contains("name"),
            "{e}"
        );
    }
}
