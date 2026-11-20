//! Command result compaction shared by Agent, CLI and MCP.

use super::scene::display_name;
use serde_json::{json, Map, Value};

/// Remove transport-noisy geometry payloads before a command result is sent
/// back into an LLM context. The full response remains available to the
/// editor/CLI JSON consumers; this view keeps bounds, vertices and duplicate
/// serialized JSON from consuming the next tool-call window.
pub fn compact_result_data(value: &Value) -> Value {
    compact_value(value, 0)
}

/// Keep human/model-facing command lines useful without repeating renderer
/// dumps that already live in structured data. The full response remains
/// available to machine callers that explicitly request raw details.
pub fn compact_result_lines(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter(|line| {
            let normalized = line.trim().to_ascii_lowercase();
            !normalized.is_empty()
                && normalized != "command executed:"
                && !normalized.starts_with("json: {")
                && !normalized.starts_with("local_vertex")
                && !normalized.starts_with("vertex_")
                && !normalized.starts_with("mesh_indices")
                && !normalized.starts_with("mesh_vertices")
                && !normalized.starts_with("index_")
                && !normalized.starts_with("bounds_min")
                && !normalized.starts_with("bounds_max")
        })
        .take(16)
        .map(|line| display_name(line))
        .collect()
}

fn compact_value(value: &Value, depth: usize) -> Value {
    if depth > 10 {
        return json!("[truncated]");
    }
    match value {
        Value::Object(object) => {
            let mut compact = Map::new();
            for (key, value) in object {
                let normalized = key.to_ascii_lowercase();
                if normalized == "json"
                    || normalized == "mesh"
                    || normalized.contains("vertex")
                    || normalized == "bounds_min"
                    || normalized == "bounds_max"
                    || normalized == "mesh_bounds"
                    || normalized == "geometry_bounds"
                    || normalized == "indices"
                {
                    continue;
                }
                compact.insert(key.clone(), compact_value(value, depth + 1));
            }
            Value::Object(compact)
        }
        Value::Array(values) => {
            if values.len() <= 128 {
                Value::Array(
                    values
                        .iter()
                        .map(|value| compact_value(value, depth + 1))
                        .collect(),
                )
            } else {
                Value::Array(
                    values
                        .iter()
                        .take(128)
                        .map(|value| compact_value(value, depth + 1))
                        .collect(),
                )
            }
        }
        Value::String(value) if value.len() > 4_096 => {
            let end = value
                .char_indices()
                .take_while(|(index, _)| *index <= 4_096)
                .map(|(index, _)| index)
                .last()
                .unwrap_or(0);
            Value::String(format!("{}... [truncated]", &value[..end]))
        }
        _ => value.clone(),
    }
}
