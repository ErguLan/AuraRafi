//! Receipts, diffs, result compaction, and postcondition verification.

use std::collections::HashSet;

use raf_ai::agent_runtime::{AgentToolKind, AgentToolResult, ToolExecutionMode};
use raf_core::scene::SceneGraph;
use raf_core::EngineCommandResponse;
use serde_json::Value;

use crate::commands::output::CommandOutput;

use super::AgentToolExecutor;

impl AgentToolExecutor<'_> {
    pub(super) fn execute_scene_diff(&self, arguments: &Value) -> AgentToolResult {
        let from_revision = arguments
            .get("from_revision")
            .and_then(Value::as_u64)
            .unwrap_or_else(|| self.ledger.revision().saturating_sub(1));
        match self.ledger.diff_since(from_revision) {
            Ok(data) => {
                let mut result = AgentToolResult::success(
                    format!(
                        "Scene changes from revision {from_revision} to {}.",
                        self.ledger.revision()
                    ),
                    data,
                );
                result.revision = Some(self.ledger.revision());
                result
            }
            Err(error) => AgentToolResult {
                ok: false,
                summary: error.clone(),
                data: serde_json::json!({
                    "error": {"code": "revision_not_available", "message": error},
                    "current_revision": self.ledger.revision()
                }),
                references: Vec::new(),
                changed: false,
                revision: Some(self.ledger.revision()),
                diff: None,
                verification: None,
                warnings: Vec::new(),
                details: Vec::new(),
                preview: false,
                partial: false,
                artifacts: Vec::new(),
            },
        }
    }
}
pub(super) fn command_output_to_tool_result(output: CommandOutput) -> AgentToolResult {
    let ok = !matches!(output.level, crate::commands::output::CommandLevel::Error);
    let mut result = AgentToolResult::success(output.title.clone(), output.json.clone());
    result.ok = ok;
    result.changed = output.changed;
    result.details = compact_response_lines(&output.lines);
    if !ok {
        result.warnings = output.lines.iter().skip(1).cloned().collect();
    }
    result
}

pub(super) fn response_to_tool_result(
    response: EngineCommandResponse,
    preview: bool,
    scene_diff: Option<Value>,
) -> AgentToolResult {
    let receipt = serde_json::json!({"transaction_id": response.transaction_id, "undo_token": response.undo_token,
        "undo_available": response.undo_available, "artifacts": response.artifacts.clone(), "metrics": response.metrics,
        "revision": response.revision});
    let references = collect_references(&response.data);
    let details = compact_response_lines(&response.lines);
    let summary = if details.is_empty() {
        response.title.clone()
    } else {
        format!("{}: {}", response.title, details[0])
    };
    let mut result = AgentToolResult {
        ok: response.ok,
        summary,
        data: compact_command_data(response.data),
        references,
        changed: response.changed,
        revision: Some(response.revision),
        diff: scene_diff.or(response.diff),
        verification: response
            .verification
            .and_then(|verification| serde_json::to_value(verification).ok()),
        warnings: response.warnings,
        details,
        preview,
        partial: false,
        artifacts: response
            .artifacts
            .iter()
            .filter_map(|artifact| serde_json::to_value(artifact).ok())
            .collect(),
    };
    if !result.data.is_object() {
        result.data = serde_json::json!({"value": result.data});
    }
    result.data["receipt"] = receipt;
    result
}

fn compact_response_lines(lines: &[String]) -> Vec<String> {
    raf_core::agent_context::compact_result_lines(lines)
}

pub(super) fn compact_command_data(data: Value) -> Value {
    let Some(object) = data.as_object() else {
        return raf_core::agent_context::compact_result_data(&data);
    };
    // `game.mesh` returns indexed topology at the top level when the caller asks
    // for `topology=true`. The generic compaction drops keys named `indices` and
    // any key containing `vertex`, which would make the Agent-facing inspect
    // result unusable. Keep this bounded per-entity payload intact; `model_content`
    // still applies its own array/string budget before the model sees it.
    if object.contains_key("has_editable_mesh") && object.contains_key("editable_mesh") {
        return data;
    }
    if let Some(entity) = object.get("entity").and_then(Value::as_object) {
        return serde_json::json!({
            "entity": {
                "id": entity.get("id"),
                "uuid": entity.get("uuid"),
                "name": entity.get("name"),
                "path": entity.get("path"),
                "parent_id": entity.get("parent_id"),
                "parent_path": entity.get("parent_path"),
                "children": entity.get("children"),
                "child_names": entity.get("child_names"),
                "ref": entity.get("ref"),
                "kind": entity.get("kind"),
                "is_folder": entity.get("is_folder"),
                "primitive": entity.get("primitive"),
                "position": entity.get("position"),
                "rotation_deg": entity.get("rotation_deg"),
                "scale": entity.get("scale"),
                "world_position": entity.get("world_position"),
                "world_bounds": entity.get("world_bounds"),
                "color_rgba": entity.get("color_rgba"),
                "visible": entity.get("visible"),
                "locked": entity.get("locked"),
                "source_asset": entity.get("source_asset"),
                "scripts": entity.get("scripts"),
                "semantic_role": entity.get("semantic_role"),
                "stable_key": entity.get("stable_key"),
                "tags": entity.get("tags"),
                "agent_origin": entity.get("agent_origin")
            }
        });
    }
    if object.get("operations").and_then(Value::as_array).is_some() {
        let mut compact = serde_json::Map::new();
        for key in [
            "ok",
            "created_ids",
            "entity_count",
            "build",
            "reconcile",
            "repair",
        ] {
            if let Some(value) = object.get(key) {
                compact.insert(key.to_string(), value.clone());
            }
        }
        if let Some(operations) = object.get("operations").and_then(Value::as_array) {
            compact.insert(
                "operations".to_string(),
                Value::Array(operations.iter().map(compact_operation_result).collect()),
            );
        }
        if let Some(entities) = object.get("entities").and_then(Value::as_array) {
            compact.insert(
                "entities".to_string(),
                Value::Array(entities.iter().map(compact_entity_data).collect()),
            );
        }
        return Value::Object(compact);
    }
    raf_core::agent_context::compact_result_data(&data)
}

fn compact_operation_result(operation: &Value) -> Value {
    let Some(operation) = operation.as_object() else {
        return operation.clone();
    };
    let mut compact = serde_json::Map::new();
    for key in ["name", "ok", "summary", "changed"] {
        if let Some(value) = operation.get(key) {
            compact.insert(key.to_string(), value.clone());
        }
    }
    if let Some(entity) = operation.get("entity") {
        compact.insert("entity".to_string(), compact_entity_data(entity));
    }
    Value::Object(compact)
}

fn compact_entity_data(entity: &Value) -> Value {
    let Some(entity) = entity.as_object() else {
        return entity.clone();
    };
    let mut compact = serde_json::Map::new();
    for key in [
        "id",
        "uuid",
        "ref",
        "name",
        "path",
        "parent_id",
        "parent_path",
        "children",
        "child_names",
        "kind",
        "is_folder",
        "primitive",
        "position",
        "rotation_deg",
        "scale",
        "size",
        "world_position",
        "color_rgba",
        "visible",
        "locked",
        "source_asset",
        "source_schema_version",
        "scripts",
        "semantic_role",
        "stable_key",
        "tags",
        "agent_origin",
    ] {
        if let Some(value) = entity.get(key) {
            compact.insert(key.to_string(), value.clone());
        }
    }
    if let Some(mesh) = entity.get("mesh").and_then(Value::as_object) {
        let mut mesh_summary = serde_json::Map::new();
        for key in ["vertex_count", "index_count"] {
            if let Some(value) = mesh.get(key) {
                mesh_summary.insert(key.to_string(), value.clone());
            }
        }
        if !mesh_summary.is_empty() {
            compact.insert("mesh".to_string(), Value::Object(mesh_summary));
        }
    }
    Value::Object(compact)
}

fn collect_references(value: &Value) -> Vec<String> {
    let mut references = Vec::new();
    collect_reference_field(value, "uuid", &mut references);
    collect_reference_field(value, "path", &mut references);
    references.sort();
    references.dedup();
    references.truncate(64);
    references
}

fn collect_reference_field(value: &Value, key: &str, output: &mut Vec<String>) {
    match value {
        Value::Array(values) => {
            for value in values {
                collect_reference_field(value, key, output);
            }
        }
        Value::Object(values) => {
            if let Some(value) = values.get(key).and_then(Value::as_str) {
                output.push(value.to_string());
            }
            for value in values.values() {
                collect_reference_field(value, key, output);
            }
        }
        _ => {}
    }
}

pub(crate) fn verify_postconditions(
    command_name: &str,
    params: &Value,
    data: &Value,
    scene: &SceneGraph,
) -> Vec<String> {
    match command_name {
        "game.add" | "game.create_group" | "game.update" | "game.reparent" | "game.snap" => data
            .get("entity")
            .map(|entity| verify_entity_postcondition(params, entity, scene))
            .unwrap_or_else(|| vec!["The mutation result did not include an entity.".to_string()]),
        "game.duplicate" | "game.generate_prefab" => {
            let expected = params.get("count").and_then(Value::as_u64).unwrap_or(1);
            let actual = data
                .get("count")
                .or_else(|| data.get("instances"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            (actual != expected)
                .then(|| format!("Created {actual} instance(s), expected {expected}."))
                .into_iter()
                .collect()
        }
        "game.batch" | "game.repair" => {
            let operations = params
                .get("operations")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            verify_batch_postconditions(&operations, data, scene)
        }
        "game.build" => {
            let operations = build_postcondition_operations(params);
            verify_batch_postconditions(&operations, data, scene)
        }
        "game.reconcile" => verify_reconcile_postconditions(params, scene),
        _ => Vec::new(),
    }
}

fn verify_reconcile_postconditions(params: &Value, scene: &SceneGraph) -> Vec<String> {
    let mut failures = Vec::new();
    let Some(object) = params.as_object() else {
        return vec!["reconcile params must be an object.".to_string()];
    };
    let default_parent = object.get("parent");
    for key in ["groups", "entities"] {
        let Some(items) = object.get(key).and_then(Value::as_array) else {
            continue;
        };
        for (index, item) in items.iter().enumerate() {
            let Some(stable_key) = item.get("stable_key").and_then(Value::as_str) else {
                failures.push(format!("{key}[{index}] has no stable_key."));
                continue;
            };
            let Some(id) = resolve_scene_target(scene, &format!("key:{stable_key}")) else {
                failures.push(format!(
                    "stable_key '{stable_key}' was not found after reconcile."
                ));
                continue;
            };
            let Some(node) = scene.get(id) else {
                failures.push(format!(
                    "stable_key '{stable_key}' resolved to an invalid entity."
                ));
                continue;
            };
            if key == "groups" && !node.is_folder {
                failures.push(format!("stable_key '{stable_key}' is not a group."));
            }
            if key == "entities" && node.is_folder {
                failures.push(format!(
                    "stable_key '{stable_key}' is a group, not an entity."
                ));
            }
            let Some(mut expected) = item.as_object().cloned() else {
                failures.push(format!("{key}[{index}] is not an object."));
                continue;
            };
            let parent_value = item.get("parent").or(default_parent);
            if let Some(parent) = parent_value.and_then(Value::as_str) {
                let expected_parent = if parent.trim().is_empty()
                    || matches!(
                        parent.trim().to_ascii_lowercase().as_str(),
                        "root" | "scene" | "none" | "null"
                    ) {
                    None
                } else {
                    resolve_scene_target(scene, parent)
                };
                if parent.trim() != ""
                    && !matches!(
                        parent.trim().to_ascii_lowercase().as_str(),
                        "root" | "scene" | "none" | "null"
                    )
                    && expected_parent.is_none()
                {
                    failures.push(format!(
                        "parent '{parent}' was not found for '{stable_key}'."
                    ));
                } else if node.parent != expected_parent {
                    failures.push(format!("'{stable_key}' has an unexpected parent."));
                }
            }
            if !expected.contains_key("parent") {
                if let Some(parent) = default_parent {
                    expected.insert("parent".to_string(), parent.clone());
                }
            }
            let inspection = raf_core::agent_context::scene_inspect(
                scene,
                &serde_json::json!({"target": format!("entity:{}", node.uuid)}),
                &[],
            );
            if let Some(entity) = inspection.data.get("entity") {
                failures.extend(
                    verify_entity_postcondition(&Value::Object(expected), entity, scene)
                        .into_iter()
                        .map(|failure| format!("'{stable_key}': {failure}")),
                );
            } else {
                failures.push(format!(
                    "stable_key '{stable_key}' could not be inspected after reconcile."
                ));
            }
        }
    }
    failures
}

fn build_postcondition_operations(params: &Value) -> Vec<Value> {
    let Some(object) = params.as_object() else {
        return Vec::new();
    };
    let default_parent = object.get("parent").cloned();
    let mut operations = Vec::new();
    for key in ["groups", "entities"] {
        let operation_name = if key == "groups" {
            "game.create_group"
        } else {
            "game.add"
        };
        let Some(items) = object.get(key).and_then(Value::as_array) else {
            continue;
        };
        for item in items {
            let Some(mut item) = item.as_object().cloned() else {
                continue;
            };
            if !item.contains_key("parent") {
                if let Some(parent) = &default_parent {
                    item.insert("parent".to_string(), parent.clone());
                }
            }
            operations.push(serde_json::json!({
                "name": operation_name,
                "params": Value::Object(item)
            }));
        }
    }
    operations
}

fn verify_batch_postconditions(
    operations: &[Value],
    data: &Value,
    scene: &SceneGraph,
) -> Vec<String> {
    let entities = data
        .get("entities")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let result_operations = data
        .get("operations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut used_results = HashSet::new();
    let mut failures = Vec::new();
    for (index, operation) in operations.iter().enumerate() {
        let Some(name) = operation.get("name").and_then(Value::as_str) else {
            failures.push(format!("Operation {} has no command name.", index + 1));
            continue;
        };
        let Some(params) = operation.get("params") else {
            failures.push(format!("Operation {} has no params.", index + 1));
            continue;
        };
        if !matches!(
            name,
            "game.add"
                | "scene_create"
                | "game.create_group"
                | "scene_create_group"
                | "game.update"
                | "scene_update"
                | "game.reparent"
                | "scene_reparent"
                | "game.snap"
                | "scene_snap"
                | "scene.snap"
        ) {
            continue;
        }
        let expected_name = canonical_operation_name(name);
        let result_match = result_operations
            .iter()
            .enumerate()
            .filter(|(result_index, result)| {
                !used_results.contains(result_index)
                    && result
                        .get("name")
                        .and_then(Value::as_str)
                        .is_some_and(|result_name| {
                            canonical_operation_name(result_name) == expected_name
                        })
                    && result
                        .get("entity")
                        .filter(|entity| entity.is_object())
                        .is_some_and(|entity| {
                            entity_matches_operation(entity, params, scene)
                                || (params.get("name").is_none()
                                    && params.get("target").is_none()
                                    && matches!(
                                        expected_name.as_str(),
                                        "game.add" | "game.create_group"
                                    ))
                        })
            })
            .min_by_key(|(result_index, _)| (*result_index).abs_diff(index));
        let actual = if let Some((result_index, result)) = result_match {
            used_results.insert(result_index);
            result.get("entity")
        } else {
            entities
                .iter()
                .find(|entity| entity_matches_operation(entity, params, scene))
        };
        match actual {
            Some(entity) => failures.extend(
                verify_entity_postcondition(params, entity, scene)
                    .into_iter()
                    .map(|failure| format!("operation {}: {failure}", index + 1)),
            ),
            None => failures.push(format!(
                "operation {} ({name}) did not return an affected entity.",
                index + 1
            )),
        }
    }
    failures
}

fn canonical_operation_name(name: &str) -> String {
    match name.trim_start_matches('/').to_ascii_lowercase().as_str() {
        "scene_create" | "game.add" => "game.add".to_string(),
        "scene_create_group" | "game.create_group" => "game.create_group".to_string(),
        "scene_update" | "game.update" => "game.update".to_string(),
        "scene_reparent" | "game.reparent" => "game.reparent".to_string(),
        "scene_snap" | "scene.snap" | "game.snap" => "game.snap".to_string(),
        "scene_delete" | "game.delete" => "game.delete".to_string(),
        "scene_duplicate" | "game.duplicate" => "game.duplicate".to_string(),
        "scene_arrange" | "game.arrange_grid" => "game.arrange_grid".to_string(),
        "scene_instantiate_prefab" | "scene_instantiate_template" | "game.generate_prefab" => {
            "game.generate_prefab".to_string()
        }
        _ => name.to_string(),
    }
}

fn entity_matches_operation(entity: &Value, params: &Value, scene: &SceneGraph) -> bool {
    let Some(entity_object) = entity.as_object() else {
        return false;
    };
    if let Some(name) = params.get("name").and_then(Value::as_str) {
        let expected_name = raf_core::agent_context::display_name(name);
        return entity_object
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(|actual| actual == expected_name);
    }
    let Some(target) = params.get("target").and_then(Value::as_str) else {
        return false;
    };
    if let Some(id) = entity_object.get("id").and_then(Value::as_u64) {
        if target.parse::<usize>().ok() == Some(id as usize) {
            return true;
        }
    }
    if entity_object
        .get("uuid")
        .and_then(Value::as_str)
        .is_some_and(|uuid| uuid == target)
    {
        return true;
    }
    if entity_object
        .get("ref")
        .and_then(Value::as_str)
        .is_some_and(|entity_ref| entity_ref.eq_ignore_ascii_case(target))
    {
        return true;
    }
    if entity_object
        .get("stable_key")
        .and_then(Value::as_str)
        .is_some_and(|stable_key| {
            stable_key.eq_ignore_ascii_case(target)
                || format!("key:{stable_key}").eq_ignore_ascii_case(target)
        })
    {
        return true;
    }
    if entity_object
        .get("path")
        .and_then(Value::as_str)
        .is_some_and(|path| {
            path.trim_matches('/')
                .eq_ignore_ascii_case(target.trim_matches('/'))
        })
    {
        return true;
    }
    resolve_scene_target(scene, target)
        .and_then(|id| {
            entity_object
                .get("id")
                .and_then(Value::as_u64)
                .map(|actual| (id, actual))
        })
        .is_some_and(|(id, actual)| id.0 == actual as usize)
}

pub(super) fn verify_entity_postcondition(
    params: &Value,
    entity: &Value,
    scene: &SceneGraph,
) -> Vec<String> {
    let Some(params) = params.as_object() else {
        return vec!["Mutation params must be an object.".to_string()];
    };
    let Some(entity) = entity.as_object() else {
        return vec!["Mutation result entity must be an object.".to_string()];
    };
    let mut failures = Vec::new();
    if let Some(expected_name) = params.get("name").and_then(Value::as_str) {
        let expected_name = raf_core::agent_context::display_name(expected_name);
        if entity.get("name").and_then(Value::as_str) != Some(expected_name.as_str()) {
            failures.push(format!(
                "name is {:?}, expected {:?}.",
                entity.get("name").and_then(Value::as_str),
                expected_name
            ));
        }
    }
    if let Some(expected) = params
        .get("primitive")
        .or_else(|| params.get("kind"))
        .or_else(|| params.get("shape"))
        .or_else(|| params.get("type"))
        .and_then(Value::as_str)
    {
        let expected = canonical_primitive(expected);
        let actual = entity
            .get("primitive")
            .and_then(Value::as_str)
            .map(canonical_primitive);
        if actual.as_deref() != Some(expected.as_str()) {
            failures.push(format!("primitive is {:?}, expected {}.", actual, expected));
        }
    }
    if let Some(transform) = params.get("transform") {
        let Some(transform) = transform.as_object() else {
            failures.push("transform must be an object.".to_string());
            return failures;
        };
        for (key, actual_key) in [
            ("position", "position"),
            ("rotation_deg", "rotation_deg"),
            ("rotation", "rotation_deg"),
            ("scale", "scale"),
        ] {
            if let Some(expected) = transform.get(key) {
                compare_postcondition_vec3(
                    &mut failures,
                    &format!("transform.{key}"),
                    expected,
                    entity.get(actual_key),
                );
            }
        }
    }
    for (key, actual_key) in [
        ("position", "position"),
        ("rotation_deg", "rotation_deg"),
        ("rotation", "rotation_deg"),
        ("scale", "scale"),
    ] {
        if let Some(expected) = params.get(key) {
            compare_postcondition_vec3(&mut failures, key, expected, entity.get(actual_key));
        }
    }
    if let Some(expected) = params.get("color").or_else(|| params.get("color_rgba")) {
        compare_postcondition_color(&mut failures, expected, entity.get("color_rgba"));
    }
    for key in ["visible", "locked"] {
        if let Some(expected) = params.get(key).and_then(Value::as_bool) {
            if entity.get(key).and_then(Value::as_bool) != Some(expected) {
                failures.push(format!(
                    "{key} is {:?}, expected {expected}.",
                    entity.get(key).and_then(Value::as_bool)
                ));
            }
        }
    }
    if let Some(expected_parent) = params.get("parent") {
        let expected_id = match expected_parent {
            Value::Null => None,
            Value::String(parent)
                if parent.trim().is_empty()
                    || matches!(
                        parent.trim().to_ascii_lowercase().as_str(),
                        "root" | "scene" | "none" | "null"
                    ) =>
            {
                None
            }
            Value::String(parent) => resolve_scene_target(scene, parent).map(|id| id.0),
            Value::Number(parent) => parent.as_u64().map(|id| id as usize),
            _ => {
                failures.push("parent must be a target string or null.".to_string());
                None
            }
        };
        // Command receipts use parent_id; semantic inspections use parent.ref.
        // Neither presentation shape is authoritative over the live document.
        let actual_id = entity
            .get("id")
            .and_then(Value::as_u64)
            .and_then(|id| scene.get(raf_core::scene::SceneNodeId(id as usize)))
            .map(|node| node.parent.map(|id| id.0))
            .unwrap_or_else(|| {
                entity
                    .get("parent_id")
                    .and_then(Value::as_u64)
                    .or_else(|| {
                        entity
                            .get("parent")
                            .and_then(|p| p.get("id"))
                            .and_then(Value::as_u64)
                    })
                    .map(|id| id as usize)
            });
        if expected_id != actual_id {
            failures.push(format!(
                "parent_id is {:?}, expected {:?}.",
                actual_id, expected_id
            ));
        }
    }
    for key in ["semantic_role", "stable_key", "agent_origin"] {
        if let Some(expected) = params.get(key) {
            let expected = match expected {
                Value::Null => None,
                Value::String(value) => {
                    let value = raf_core::agent_context::display_name(value);
                    (!value.is_empty()).then_some(value)
                }
                _ => {
                    failures.push(format!("{key} must be a string or null."));
                    continue;
                }
            };
            let actual = entity.get(key).and_then(Value::as_str).map(str::to_string);
            if actual != expected {
                failures.push(format!("{key} is {:?}, expected {:?}.", actual, expected));
            }
        }
    }
    if let Some(expected) = params.get("tags") {
        let expected = normalized_tags(expected);
        let actual = entity.get("tags").and_then(normalized_tags);
        match (expected, actual) {
            (Some(expected), Some(actual)) if expected == actual => {}
            (Some(expected), Some(actual)) => {
                failures.push(format!("tags are {:?}, expected {:?}.", actual, expected))
            }
            _ => failures.push("tags must be an array of strings or null.".to_string()),
        }
    }
    failures
}

fn normalized_tags(value: &Value) -> Option<Vec<String>> {
    match value {
        Value::Null => Some(Vec::new()),
        Value::Array(values) => {
            let mut tags = values
                .iter()
                .map(|value| value.as_str().map(raf_core::agent_context::display_name))
                .collect::<Option<Vec<_>>>()?;
            tags.retain(|tag| !tag.is_empty());
            tags.sort_unstable();
            tags.dedup();
            Some(tags)
        }
        _ => None,
    }
}

fn compare_postcondition_vec3(
    failures: &mut Vec<String>,
    label: &str,
    expected: &Value,
    actual: Option<&Value>,
) {
    let Some(expected) = postcondition_vec3(expected) else {
        failures.push(format!("{label} must contain 3 finite numbers."));
        return;
    };
    let Some(actual) = actual.and_then(postcondition_vec3) else {
        failures.push(format!("{label} was not returned by the engine."));
        return;
    };
    if !expected
        .iter()
        .zip(actual.iter())
        .all(|(expected, actual)| (expected - actual).abs() <= 0.0001)
    {
        failures.push(format!(
            "{label} is [{:.4}, {:.4}, {:.4}], expected [{:.4}, {:.4}, {:.4}].",
            actual[0], actual[1], actual[2], expected[0], expected[1], expected[2]
        ));
    }
}

fn compare_postcondition_color(
    failures: &mut Vec<String>,
    expected: &Value,
    actual: Option<&Value>,
) {
    let Some(expected) = postcondition_color(expected) else {
        failures.push("color must contain 3 or 4 channels.".to_string());
        return;
    };
    let Some(actual) = actual.and_then(postcondition_color) else {
        failures.push("color_rgba was not returned by the engine.".to_string());
        return;
    };
    if expected != actual {
        failures.push(format!(
            "color_rgba is [{}, {}, {}, {}], expected [{}, {}, {}, {}].",
            actual[0],
            actual[1],
            actual[2],
            actual[3],
            expected[0],
            expected[1],
            expected[2],
            expected[3]
        ));
    }
}

fn postcondition_vec3(value: &Value) -> Option<[f64; 3]> {
    match value {
        Value::Array(values) if values.len() == 3 => Some([
            values[0].as_f64()?,
            values[1].as_f64()?,
            values[2].as_f64()?,
        ]),
        Value::Object(object) => Some([
            object.get("x").and_then(Value::as_f64)?,
            object.get("y").and_then(Value::as_f64)?,
            object.get("z").and_then(Value::as_f64)?,
        ]),
        Value::String(raw) => serde_json::from_str(raw)
            .ok()
            .and_then(|value| postcondition_vec3(&value)),
        _ => None,
    }
}

fn postcondition_color(value: &Value) -> Option<[u64; 4]> {
    match value {
        Value::Array(values) if values.len() == 3 || values.len() == 4 => Some([
            values[0].as_u64()?,
            values[1].as_u64()?,
            values[2].as_u64()?,
            values.get(3).and_then(Value::as_u64).unwrap_or(255),
        ]),
        Value::Object(object) => Some([
            object.get("r").and_then(Value::as_u64)?,
            object.get("g").and_then(Value::as_u64)?,
            object.get("b").and_then(Value::as_u64)?,
            object.get("a").and_then(Value::as_u64).unwrap_or(255),
        ]),
        Value::String(raw) if raw.trim_start().starts_with('#') => {
            let raw = raw.trim().trim_start_matches('#');
            if raw.len() != 6 && raw.len() != 8 {
                return None;
            }
            Some([
                u64::from_str_radix(&raw[0..2], 16).ok()?,
                u64::from_str_radix(&raw[2..4], 16).ok()?,
                u64::from_str_radix(&raw[4..6], 16).ok()?,
                if raw.len() == 8 {
                    u64::from_str_radix(&raw[6..8], 16).ok()?
                } else {
                    255
                },
            ])
        }
        Value::String(raw) => serde_json::from_str(raw)
            .ok()
            .and_then(|value| postcondition_color(&value)),
        _ => None,
    }
}

fn canonical_primitive(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "box" | "block" => "cube".to_string(),
        "ball" | "circle" | "uv_sphere" => "sphere".to_string(),
        "floor" | "sprite" | "sprite2d" => "plane".to_string(),
        "sphere 2d" | "sphere_2d" | "billboard_sphere" => "sphere2d".to_string(),
        "ramp" => "wedge".to_string(),
        "group" | "folder" => "empty".to_string(),
        value => value.to_string(),
    }
}

fn resolve_scene_target(
    scene: &SceneGraph,
    target: &str,
) -> Option<raf_core::scene::graph::SceneNodeId> {
    raf_core::agent_context::resolve_target(scene, target)
}

pub(super) fn request_preview(mode: ToolExecutionMode, kind: AgentToolKind) -> bool {
    mode == ToolExecutionMode::Preview && kind == AgentToolKind::Mutation
}

pub(super) fn expects_observable_scene_change(command_name: &str) -> bool {
    // `game.mesh` is intentionally absent: one command surface mixes read-only
    // actions (inspect, apply) and legitimate no-ops (revert with no editable
    // mesh) with real topology edits. Requiring a scene diff per call would
    // fail those valid invocations. Its mutating actions are still verified by
    // the command's own `changed` flag and the host transaction ledger.
    matches!(
        command_name,
        "game.add"
            | "game.create_group"
            | "game.delete"
            | "game.duplicate"
            | "game.update"
            | "game.reparent"
            | "game.snap"
            | "game.arrange_grid"
            | "game.generate_prefab"
            | "game.light"
            | "game.audio"
            | "game.mesh_asset"
            | "game.build"
            | "game.reconcile"
            | "game.repair"
            | "game.batch"
            | "game.commit"
    )
}

pub(super) fn live_scene_entity_count(scene: &SceneGraph) -> usize {
    scene.all_live_ids().len()
}
