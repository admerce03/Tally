//! Split the schema of `Command` (an internally tagged enum, tag `command`) into one MCP tool
//! per variant.

use std::collections::BTreeSet;

use anyhow::{Context, Result, anyhow};
use rmcp::model::Tool;
use serde_json::{Map, Value, json};

/// The tag property of `Command` (`#[serde(tag = "command")]`).
pub const TAG: &str = "command";

/// The JSON schema of the whole `Command` enum.
pub fn command_schema() -> Value {
    serde_json::to_value(schemars::schema_for!(tally_core::Command)).unwrap_or(Value::Null)
}

/// Collect every `#/$defs/<Name>` referenced (transitively) from `value`.
fn collect_refs(value: &Value, defs: &Map<String, Value>, out: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(r)) = map.get("$ref")
                && let Some(name) = r.strip_prefix("#/$defs/")
                && out.insert(name.to_string())
                && let Some(def) = defs.get(name)
            {
                collect_refs(def, defs, out);
            }
            map.values().for_each(|v| collect_refs(v, defs, out));
        }
        Value::Array(items) => items.iter().for_each(|v| collect_refs(v, defs, out)),
        _ => {}
    }
}

/// Turn one `oneOf` variant schema into `(tag, description, input schema)`.
fn split_variant(
    variant: &Value,
    defs: &Map<String, Value>,
) -> Result<(String, Option<String>, Map<String, Value>)> {
    let obj = variant
        .as_object()
        .ok_or_else(|| anyhow!("variant schema is not an object"))?;
    let mut props = obj
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .ok_or_else(|| anyhow!("variant schema has no properties"))?;
    let tag = props
        .remove(TAG)
        .and_then(|p| p.get("const").and_then(Value::as_str).map(str::to_string))
        .ok_or_else(|| anyhow!("variant schema has no `{TAG}` const"))?;
    let required: Vec<Value> = obj
        .get("required")
        .and_then(Value::as_array)
        .map(|r| r.iter().filter(|v| *v != TAG).cloned().collect())
        .unwrap_or_default();
    let description = obj
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_string);

    let mut schema = Map::new();
    schema.insert("type".into(), json!("object"));
    schema.insert("properties".into(), Value::Object(props));
    schema.insert("additionalProperties".into(), json!(false));
    if !required.is_empty() {
        schema.insert("required".into(), Value::Array(required));
    }
    // Keep only the definitions this tool needs, so each schema is self-contained.
    let mut needed = BTreeSet::new();
    collect_refs(&Value::Object(schema.clone()), defs, &mut needed);
    if !needed.is_empty() {
        let used: Map<String, Value> = needed
            .into_iter()
            .filter_map(|n| defs.get(&n).map(|d| (n, d.clone())))
            .collect();
        schema.insert("$defs".into(), Value::Object(used));
    }
    Ok((tag, description, schema))
}

/// Build one tool per variant of the given `Command` schema.
pub fn tools_from_schema(schema: &Value) -> Result<Vec<Tool>> {
    let defs = schema
        .get("$defs")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let variants = schema
        .get("oneOf")
        .and_then(Value::as_array)
        .context("Command schema has no oneOf")?;
    variants
        .iter()
        .map(|v| {
            let (tag, description, input) = split_variant(v, &defs)?;
            Ok(Tool::new(tag, description.unwrap_or_default(), input))
        })
        .collect()
}

/// All tools, one per `Command` variant.
pub fn build_tools() -> Result<Vec<Tool>> {
    tools_from_schema(&command_schema())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags_in_schema() -> Vec<String> {
        let schema = command_schema();
        schema["oneOf"]
            .as_array()
            .map(|v| v.as_slice())
            .unwrap_or_default()
            .iter()
            .filter_map(|v| v["properties"][TAG]["const"].as_str().map(str::to_string))
            .collect()
    }

    #[test]
    fn one_tool_per_command_variant() {
        let tools = build_tools().unwrap();
        let tags = tags_in_schema();
        assert!(tags.len() > 30, "schema should list every Command variant");
        assert_eq!(tools.len(), tags.len());
        let names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
        assert_eq!(names, tags);
        let unique: BTreeSet<&String> = names.iter().collect();
        assert_eq!(unique.len(), names.len(), "tool names must be unique");
        for expected in ["add_term", "list_terms", "solve_target", "resolve"] {
            assert!(names.iter().any(|n| n == expected), "{expected}");
        }
    }

    #[test]
    fn schemas_are_objects_without_the_tag_and_self_contained() {
        for tool in build_tools().unwrap() {
            let s = &tool.input_schema;
            assert_eq!(s["type"], "object", "{}", tool.name);
            assert!(s["properties"].is_object(), "{}", tool.name);
            assert_eq!(s["additionalProperties"], false, "{}", tool.name);
            assert!(s["properties"].get(TAG).is_none(), "{}", tool.name);
            let required = s.get("required").and_then(Value::as_array);
            assert!(
                required.is_none_or(|r| !r.iter().any(|v| v == TAG)),
                "{}",
                tool.name
            );
            assert!(
                tool.description.as_deref().is_some_and(|d| !d.is_empty()),
                "{} has no description",
                tool.name
            );
            // Every $ref resolves inside the tool's own $defs.
            let defs = s.get("$defs").and_then(Value::as_object).cloned();
            let mut refs = BTreeSet::new();
            let no_defs = Map::new();
            collect_refs(
                &Value::Object((**s).clone()),
                defs.as_ref().unwrap_or(&no_defs),
                &mut refs,
            );
            for r in refs {
                assert!(
                    defs.as_ref().is_some_and(|d| d.contains_key(&r)),
                    "{}: missing def {r}",
                    tool.name
                );
            }
        }
    }

    #[test]
    fn specific_schemas_look_right() {
        let tools = build_tools().unwrap();
        let find = |n: &str| tools.iter().find(|t| t.name == n).unwrap();
        let list = find("list_terms");
        assert!(list.input_schema.get("required").is_none());
        let solve = find("solve_target");
        assert_eq!(solve.input_schema["required"], json!(["course_id"]));
        assert!(solve.input_schema["$defs"].get("LockedScore").is_some());
        let resolve = find("resolve");
        assert_eq!(
            resolve.input_schema["required"],
            json!(["kind", "reference"])
        );
    }
}
