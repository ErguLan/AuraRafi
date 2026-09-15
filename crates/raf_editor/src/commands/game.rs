use glam::Vec3;
use raf_assets::{builtin_primitive_model_kinds, PrimitiveModelManifest};
use raf_core::agent_context::{
    display_name, display_path, resolve_target as resolve_agent_target, world_bounds,
};
use raf_core::scene::graph::{NodeColor, Primitive, SceneGraph, SceneNodeId};
use serde_json::Value;
use std::collections::HashSet;

use crate::commands::output::CommandOutput;
use crate::commands::parser::ParsedCommand;

#[derive(Debug, Clone, Default)]
pub struct SceneSelectionState {
    pub selected_node: Option<SceneNodeId>,
    pub selected_nodes: Vec<SceneNodeId>,
}

/// Narrow presentation port for game commands. The command kernel can run
/// headless with a no-op implementation; a viewport is only one consumer.
pub trait GameViewportPort {
    fn selected_ids(&self) -> Vec<SceneNodeId>;
    fn set_selected_ids(&mut self, ids: Vec<SceneNodeId>);
    fn focus_entity(&mut self, scene: &SceneGraph, id: Option<SceneNodeId>);
}

#[derive(Debug, Default)]
pub struct HeadlessGameViewportPort {
    selected: Vec<SceneNodeId>,
    pub focused: Option<SceneNodeId>,
}

impl GameViewportPort for HeadlessGameViewportPort {
    fn selected_ids(&self) -> Vec<SceneNodeId> {
        self.selected.clone()
    }

    fn set_selected_ids(&mut self, ids: Vec<SceneNodeId>) {
        self.selected = ids;
    }

    fn focus_entity(&mut self, _scene: &SceneGraph, id: Option<SceneNodeId>) {
        self.focused = id;
    }
}

pub struct GameCommandContext<'a> {
    pub scene: &'a mut SceneGraph,
    pub selection: &'a mut SceneSelectionState,
    pub viewport: &'a mut dyn GameViewportPort,
}

const KNOWN_GAME_COMMANDS: &[&str] = &[
    "game.add",
    "game.select",
    "game.rename",
    "game.delete",
    "game.duplicate",
    "game.set_transform",
    "game.update",
    "game.move",
    "game.rotate",
    "game.scale",
    "game.color",
    "game.arrange_grid",
    "game.generate_prefab",
    "game.describe_scene",
    "game.focus",
    "game.batch",
    "game.create_group",
    "game.reparent",
    "game.snap",
    "game.build",
    "game.reconcile",
    "game.repair",
];

/// Unknown-command errors teach the caller the real surface instead of
/// leaving agents guessing.
fn unknown_game_command(command_name: &str) -> CommandOutput {
    let query = command_name
        .trim_start_matches("game.")
        .to_ascii_lowercase();
    let mut suggestions: Vec<String> = KNOWN_GAME_COMMANDS
        .iter()
        .filter(|known| {
            known
                .trim_start_matches("game.")
                .to_ascii_lowercase()
                .contains(&query)
                || query.is_empty()
        })
        .map(|known| (*known).to_string())
        .collect();
    if suggestions.is_empty() {
        suggestions = KNOWN_GAME_COMMANDS
            .iter()
            .map(|known| known.to_string())
            .collect();
    }
    CommandOutput::error(
        "Game command",
        format!(
            "Unknown game command: {command_name}. Available commands: {}",
            suggestions.join(", ")
        ),
    )
}

pub fn execute(
    command_name: &str,
    command: &ParsedCommand,
    ctx: &mut GameCommandContext<'_>,
) -> CommandOutput {
    match command_name {
        "game.add" => add_entity(command, ctx),
        "game.select" => select_entity(command, ctx),
        "game.rename" => rename_entity(command, ctx),
        "game.delete" => delete_entity(command, ctx),
        "game.duplicate" => duplicate_entity(command, ctx),
        "game.set_transform" => set_transform(command, ctx),
        "game.update" => update_entity(command, ctx),
        "game.move" => move_entity(command, ctx),
        "game.rotate" => rotate_entity(command, ctx),
        "game.scale" => scale_entity(command, ctx),
        "game.color" => color_entity(command, ctx),
        "game.arrange_grid" => arrange_grid(command, ctx),
        "game.generate_prefab" => generate_prefab(command, ctx),
        "game.describe_scene" => describe_scene(ctx),
        "game.focus" => focus_entity(command, ctx),
        "game.batch" => batch(command, ctx),
        "game.create_group" | "scene.group" | "scene.folder" => create_group(command, ctx),
        "game.reparent" | "scene.reparent" => reparent_entity(command, ctx),
        "game.snap" | "scene.snap" => snap_entity(command, ctx),
        "game.build" | "scene.build" => build_scene(command, ctx),
        "game.reconcile" | "scene.reconcile" => reconcile_scene(command, ctx),
        "game.repair" | "scene.repair" => repair_scene(command, ctx),
        other => unknown_game_command(other),
    }
}

fn batch(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let operations: Vec<serde_json::Value> =
        match structured_or_json_array(command, "operations", "operations") {
            Ok(Some(operations)) => operations,
            Ok(None) => {
                return CommandOutput::error(
                    "Game batch",
                    "Missing operations=[{name, params}, ...].",
                )
            }
            Err(error) => return CommandOutput::error("Game batch", error),
        };
    if operations.is_empty() || operations.len() > 512 {
        return CommandOutput::error(
            "Game batch",
            "A batch requires between 1 and 512 ordered operations.",
        );
    }

    let mut staged_scene = ctx.scene.clone();
    let mut staged_selection = ctx.selection.clone();
    let mut staged_viewport = HeadlessGameViewportPort::default();
    staged_viewport.set_selected_ids(staged_selection.selected_nodes.clone());
    let before_ids = staged_scene.all_live_ids();
    let mut results = Vec::with_capacity(operations.len());
    let mut affected_ids = Vec::new();

    for (index, operation) in operations.into_iter().enumerate() {
        let Some(name) = operation.get("name").and_then(Value::as_str) else {
            return CommandOutput::error(
                "Game batch",
                format!("Operation {} is missing name.", index + 1),
            );
        };
        let normalized_name = name.trim_start_matches('/').to_ascii_lowercase();
        let parsed_name = match normalized_name.as_str() {
            "scene_create" | "game.add" => "game.add".to_string(),
            "scene_update" | "game.update" => "game.update".to_string(),
            "scene_delete" | "game.delete" => "game.delete".to_string(),
            "scene_duplicate" | "game.duplicate" => "game.duplicate".to_string(),
            "scene_arrange" | "game.arrange_grid" => "game.arrange_grid".to_string(),
            "scene_instantiate_prefab"
            | "scene_instantiate_template"
            | "game.generate_prefab" => {
                "game.generate_prefab".to_string()
            }
            "scene_create_group" | "game.create_group" => "game.create_group".to_string(),
            "scene_reparent" | "game.reparent" => "game.reparent".to_string(),
            "scene_snap" | "game.snap" => "game.snap".to_string(),
            "scene_build" | "game.build" => {
                return CommandOutput::error(
                    "Game batch",
                    "Nested scene_build operations are not supported; make the build the outer command.",
                )
            }
            other => other.to_string(),
        };
        if parsed_name == "game.batch" {
            return CommandOutput::error("Game batch", "Nested batches are not supported.");
        }
        let Some(params) = operation.get("params").and_then(Value::as_object).cloned() else {
            return CommandOutput::error(
                "Game batch",
                format!(
                    "Operation {} ({}) requires an object params value.",
                    index + 1,
                    parsed_name
                ),
            );
        };
        let structured_params = Value::Object(params.clone());
        let parsed = ParsedCommand {
            raw: format!("/{parsed_name}"),
            name: parsed_name,
            args: params
                .into_iter()
                .map(|(key, value)| (key, json_command_arg(value)))
                .collect(),
            positional: Vec::new(),
            structured_args: Some(structured_params),
        };
        let mut staged_context = GameCommandContext {
            scene: &mut staged_scene,
            selection: &mut staged_selection,
            viewport: &mut staged_viewport,
        };
        let output = execute(&parsed.name, &parsed, &mut staged_context);
        if matches!(output.level, crate::commands::output::CommandLevel::Error) {
            return CommandOutput::error(
                "Game batch",
                format!(
                    "Operation {} ({}) failed; no scene changes were applied: {}",
                    index + 1,
                    parsed.name,
                    output.lines.join(" ")
                ),
            );
        }
        if let Some(id) = output
            .json
            .get("entity")
            .and_then(|entity| entity.get("id"))
            .and_then(Value::as_u64)
            .map(|id| SceneNodeId(id as usize))
        {
            affected_ids.push(id);
        }
        let operation_entity = output.json.get("entity").cloned();
        results.push(serde_json::json!({
            "name": parsed.name,
            "ok": true,
            "summary": output.title,
            "changed": output.changed,
            "entity": operation_entity
        }));
    }

    let after_ids = staged_scene.all_live_ids();
    let created_ids = after_ids
        .iter()
        .filter(|id| !before_ids.contains(id))
        .map(|id| id.0)
        .collect::<Vec<_>>();
    affected_ids.sort_by_key(|id| id.0);
    affected_ids.dedup();
    let observed_entities = affected_ids
        .iter()
        .filter(|id| staged_scene.is_valid_node(**id))
        .filter_map(|id| {
            staged_scene.get(*id).map(|node| {
                let detail = node_json(*id, node, &staged_scene);
                detail.get("entity").cloned().unwrap_or(detail)
            })
        })
        .collect::<Vec<_>>();
    *ctx.scene = staged_scene;
    *ctx.selection = staged_selection;
    ctx.viewport
        .set_selected_ids(ctx.selection.selected_nodes.clone());

    CommandOutput::changed(
        format!("Applied {} scene operations", results.len()),
        vec![
            format!("operations: {}", results.len()),
            format!("created: {}", created_ids.len()),
            format!("entities: {}", live_scene_entity_count(ctx.scene)),
        ],
        serde_json::json!({
            "ok": true,
            "operations": results,
            "created_ids": created_ids,
            "entities": observed_entities,
            "entity_count": live_scene_entity_count(ctx.scene)
        }),
    )
}

fn json_command_arg(value: serde_json::Value) -> String {
    match value {
        serde_json::Value::String(value) => value,
        serde_json::Value::Null => String::new(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => value.to_string(),
    }
}

fn structured_or_json_array(
    command: &ParsedCommand,
    name: &str,
    label: &str,
) -> Result<Option<Vec<Value>>, String> {
    if let Some(value) = command.structured_arg(name) {
        return match value {
            Value::Array(values) => Ok(Some(values.clone())),
            Value::String(raw) => serde_json::from_str(raw)
                .map(Some)
                .map_err(|error| format!("{label} must be a JSON array: {error}")),
            Value::Null => Ok(None),
            _ => Err(format!("{label} must be a JSON array.")),
        };
    }
    command
        .arg(name)
        .map(|raw| {
            serde_json::from_str(raw)
                .map_err(|error| format!("{label} must be a JSON array: {error}"))
        })
        .transpose()
}

fn text_argument(command: &ParsedCommand, name: &str) -> Option<String> {
    if let Some(value) = command.structured_arg(name) {
        match value {
            Value::String(value) => return Some(value.clone()),
            Value::Number(value) => return Some(value.to_string()),
            Value::Null => return None,
            _ => {}
        }
    }
    command.arg(name).map(str::to_string)
}

fn bool_argument(command: &ParsedCommand, name: &str, default: bool) -> Result<bool, String> {
    if let Some(value) = command.structured_arg(name) {
        return match value {
            Value::Bool(value) => Ok(*value),
            Value::String(value) => parse_bool_literal(value, name),
            Value::Null => Ok(default),
            _ => Err(format!("{name} must be true or false.")),
        };
    }
    command
        .arg(name)
        .map(|value| parse_bool_literal(value, name))
        .transpose()
        .map(|value| value.unwrap_or(default))
}

#[derive(Debug, Clone, Default)]
struct SemanticMetadataPatch {
    semantic_role: Option<Option<String>>,
    stable_key: Option<Option<String>>,
    tags: Option<Vec<String>>,
    agent_origin: Option<Option<String>>,
}

fn parse_semantic_metadata(command: &ParsedCommand) -> Result<SemanticMetadataPatch, String> {
    Ok(SemanticMetadataPatch {
        semantic_role: optional_metadata_text(command, "semantic_role", Some("role"))?,
        stable_key: optional_metadata_text(command, "stable_key", None)?,
        tags: optional_tags(command)?,
        agent_origin: optional_metadata_text(command, "agent_origin", None)?,
    })
}

fn optional_metadata_text(
    command: &ParsedCommand,
    name: &str,
    alias: Option<&str>,
) -> Result<Option<Option<String>>, String> {
    let structured = command
        .structured_arg(name)
        .or_else(|| alias.and_then(|alias| command.structured_arg(alias)));
    if let Some(value) = structured {
        return match value {
            Value::Null => Ok(Some(None)),
            Value::String(value) => {
                let value = display_name(value);
                Ok(Some((!value.is_empty()).then_some(value)))
            }
            _ => Err(format!("{name} must be a string or null.")),
        };
    }
    Ok(command
        .arg(name)
        .or_else(|| alias.and_then(|alias| command.arg(alias)))
        .map(display_name)
        .map(|value| (!value.is_empty()).then_some(value)))
}

fn optional_tags(command: &ParsedCommand) -> Result<Option<Vec<String>>, String> {
    let values = if let Some(value) = command.structured_arg("tags") {
        match value {
            Value::Null => return Ok(Some(Vec::new())),
            Value::Array(values) => values
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(display_name)
                        .ok_or_else(|| "tags must contain only strings.".to_string())
                })
                .collect::<Result<Vec<_>, _>>()?,
            Value::String(value) => value.split(',').map(display_name).collect(),
            _ => return Err("tags must be an array of strings or null.".to_string()),
        }
    } else if let Some(raw) = command.arg("tags") {
        serde_json::from_str::<Vec<String>>(raw)
            .unwrap_or_else(|_| raw.split(',').map(str::to_string).collect())
            .into_iter()
            .map(|value| display_name(&value))
            .collect()
    } else {
        return Ok(None);
    };
    let mut tags = values
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    tags.sort_unstable();
    tags.dedup();
    if tags.len() > 32 || tags.iter().any(|tag| tag.chars().count() > 64) {
        return Err("tags supports at most 32 values of 64 characters each.".to_string());
    }
    Ok(Some(tags))
}

fn apply_semantic_metadata(
    node: &mut raf_core::scene::graph::SceneNode,
    patch: &SemanticMetadataPatch,
) {
    if let Some(value) = &patch.semantic_role {
        node.semantic_role = value.clone();
    }
    if let Some(value) = &patch.stable_key {
        node.stable_key = value.clone();
    }
    if let Some(value) = &patch.tags {
        node.tags = value.clone();
    }
    if let Some(value) = &patch.agent_origin {
        node.agent_origin = value.clone();
    }
}

fn validate_semantic_metadata(
    scene: &SceneGraph,
    patch: &SemanticMetadataPatch,
    current: Option<SceneNodeId>,
) -> Result<(), String> {
    for (field, value) in [
        ("semantic_role", &patch.semantic_role),
        ("stable_key", &patch.stable_key),
        ("agent_origin", &patch.agent_origin),
    ] {
        if let Some(Some(value)) = value {
            if value.chars().count() > 128 {
                return Err(format!("{field} supports at most 128 characters."));
            }
        }
    }
    let Some(Some(stable_key)) = &patch.stable_key else {
        return Ok(());
    };
    if scene.iter().any(|(id, node)| {
        Some(id) != current
            && !node.name.is_empty()
            && node
                .stable_key
                .as_deref()
                .is_some_and(|key| key.eq_ignore_ascii_case(stable_key))
    }) {
        return Err(format!(
            "stable_key '{stable_key}' is already used by another entity."
        ));
    }
    Ok(())
}

fn parent_target(
    command: &ParsedCommand,
    scene: &SceneGraph,
) -> Result<Option<SceneNodeId>, String> {
    let Some(raw) = text_argument(command, "parent") else {
        return Ok(None);
    };
    let normalized = raw.trim();
    if normalized.is_empty()
        || matches!(
            normalized.to_ascii_lowercase().as_str(),
            "root" | "scene" | "none" | "null"
        )
    {
        return Ok(None);
    }
    resolve_text_target(normalized, scene)
        .map(Some)
        .ok_or_else(|| format!("Parent target '{normalized}' was not found."))
}

fn parsed_command_from_params(name: &str, params: Value) -> ParsedCommand {
    let args = params
        .as_object()
        .map(|object| {
            object
                .iter()
                .map(|(key, value)| (key.clone(), json_command_arg(value.clone())))
                .collect()
        })
        .unwrap_or_default();
    ParsedCommand {
        raw: format!("/{name}"),
        name: name.to_string(),
        args,
        positional: Vec::new(),
        structured_args: Some(params),
    }
}

fn create_group(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let Some(name) = text_argument(command, "name") else {
        return CommandOutput::error("Create scene group", "Missing name for the group.");
    };
    let name = display_name(&name);
    if name.is_empty() {
        return CommandOutput::error("Create scene group", "The group name cannot be empty.");
    }
    let metadata = match parse_semantic_metadata(command) {
        Ok(metadata) => metadata,
        Err(error) => return CommandOutput::error("Create scene group", error),
    };
    if let Err(error) = validate_semantic_metadata(ctx.scene, &metadata, None) {
        return CommandOutput::error("Create scene group", error);
    }
    let parent = match parent_target(command, ctx.scene) {
        Ok(parent) => parent,
        Err(error) => return CommandOutput::error("Create scene group", error),
    };
    if let Some(parent_id) = parent {
        if !ctx.scene.get(parent_id).is_some_and(|node| node.is_folder) {
            return CommandOutput::error(
                "Create scene group",
                "Group parent must be another folder or group.",
            );
        }
    }
    let transform = match parse_transform_patch(command) {
        Ok(transform) => transform,
        Err(error) => return CommandOutput::error("Create scene group", error),
    };
    let id = parent
        .map(|parent_id| ctx.scene.add_child_folder(parent_id, &name))
        .unwrap_or_else(|| ctx.scene.add_root_folder(&name));
    if let Some(node) = ctx.scene.get_mut(id) {
        apply_transform_patch(node, transform);
        apply_semantic_metadata(node, &metadata);
    }
    select_ids(ctx, vec![id]);
    let node = ctx.scene.get(id).expect("new group exists");
    CommandOutput::changed(
        format!("Created group {}", node.name),
        node_detail_lines(id, node, ctx.scene),
        node_json(id, node, ctx.scene),
    )
}

fn reparent_entity(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let Some(id) = resolve_target(command, ctx.scene, ctx.selection) else {
        return CommandOutput::error("Reparent entity", "Target not found.");
    };
    let parent = match parent_target(command, ctx.scene) {
        Ok(parent) => parent,
        Err(error) => return CommandOutput::error("Reparent entity", error),
    };
    let preserve_world = match bool_argument(command, "preserve_world", true) {
        Ok(value) => value,
        Err(error) => return CommandOutput::error("Reparent entity", error),
    };
    let moved = if preserve_world {
        ctx.scene.reparent_node_preserve_world_transform(id, parent)
    } else {
        ctx.scene.reparent_node(id, parent)
    };
    if !moved {
        return CommandOutput::error(
            "Reparent entity",
            "The requested parent is invalid or would create a hierarchy cycle.",
        );
    }
    select_ids(ctx, vec![id]);
    let node = ctx.scene.get(id).expect("reparented node exists");
    CommandOutput::changed(
        format!("Reparented {}", node.name),
        vec![
            format!(
                "parent: {}",
                node.parent
                    .map(|id| id.0.to_string())
                    .unwrap_or_else(|| "root".to_string())
            ),
            format!("preserve_world: {preserve_world}"),
        ],
        node_json(id, node, ctx.scene),
    )
}

/// Reconcile a semantic desired state without duplicating entities that carry
/// the same stable key. Existing nodes are updated in place; missing nodes are
/// created; unmentioned nodes are intentionally preserved unless a future
/// explicit cleanup operation is requested.
fn reconcile_scene(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let groups = match structured_or_json_array(command, "groups", "groups") {
        Ok(Some(groups)) => groups,
        Ok(None) => Vec::new(),
        Err(error) => return CommandOutput::error("Reconcile scene", error),
    };
    let entities = match structured_or_json_array(command, "entities", "entities") {
        Ok(Some(entities)) => entities,
        Ok(None) => Vec::new(),
        Err(error) => return CommandOutput::error("Reconcile scene", error),
    };
    if groups.is_empty() && entities.is_empty() {
        return CommandOutput::error(
            "Reconcile scene",
            "Provide at least one keyed group or entity to reconcile.",
        );
    }
    if groups.len() > 32 || entities.len() > 128 {
        return CommandOutput::error(
            "Reconcile scene",
            "A reconcile accepts at most 32 groups and 128 entities.",
        );
    }
    if let Err(error) = validate_design_profile(command, &groups, &entities, Some(ctx.scene)) {
        return CommandOutput::error("Reconcile scene", error);
    }

    let mut desired_keys = HashSet::new();
    let mut operations = Vec::with_capacity(groups.len() + entities.len() * 2);
    let mut created = 0usize;
    let mut updated = 0usize;
    let mut reparented = 0usize;

    let default_parent = command.structured_arg("parent").cloned().or_else(|| {
        command
            .arg("parent")
            .map(|value| Value::String(value.to_string()))
    });
    let ordered_groups = match order_build_groups(groups, ctx.scene, default_parent.as_ref()) {
        Ok(groups) => groups,
        Err(error) => return CommandOutput::error("Reconcile scene", error),
    };
    for (index, group) in ordered_groups.into_iter().enumerate() {
        let Some(mut params) = group.as_object().cloned() else {
            return CommandOutput::error(
                "Reconcile scene",
                format!("groups[{index}] must be an object."),
            );
        };
        let Some(key) = stable_key_from_params(&params).map(str::to_string) else {
            return CommandOutput::error(
                "Reconcile scene",
                format!("groups[{index}] requires a non-empty stable_key."),
            );
        };
        if !desired_keys.insert(key.to_ascii_lowercase()) {
            return CommandOutput::error(
                "Reconcile scene",
                format!("stable_key '{key}' is duplicated in the desired state."),
            );
        }
        let target = format!("key:{key}");
        if !params.contains_key("parent") {
            if let Some(parent) = &default_parent {
                params.insert("parent".to_string(), parent.clone());
            }
        }
        let parent = params.get("parent").cloned();
        if let Some(id) = resolve_text_target(&target, ctx.scene) {
            if !ctx.scene.get(id).is_some_and(|node| node.is_folder) {
                return CommandOutput::error(
                    "Reconcile scene",
                    format!("stable_key '{key}' belongs to an entity, not a group."),
                );
            }
            params.insert("target".to_string(), Value::String(target.clone()));
            params.remove("parent");
            operations.push(serde_json::json!({
                "name": "game.update",
                "params": Value::Object(params)
            }));
            updated += 1;
            if parent
                .as_ref()
                .is_some_and(|parent| parent_requires_reparent(ctx.scene, id, parent))
            {
                operations.push(serde_json::json!({
                    "name": "game.reparent",
                    "params": {"target": target.clone(), "parent": parent.clone().unwrap_or(Value::Null), "preserve_world": true}
                }));
                reparented += 1;
            }
        } else {
            operations.push(serde_json::json!({
                "name": "game.create_group",
                "params": Value::Object(params)
            }));
            created += 1;
        }
    }

    for (index, entity) in entities.into_iter().enumerate() {
        let Some(mut params) = entity.as_object().cloned() else {
            return CommandOutput::error(
                "Reconcile scene",
                format!("entities[{index}] must be an object."),
            );
        };
        let Some(key) = stable_key_from_params(&params).map(str::to_string) else {
            return CommandOutput::error(
                "Reconcile scene",
                format!("entities[{index}] requires a non-empty stable_key."),
            );
        };
        if !desired_keys.insert(key.to_ascii_lowercase()) {
            return CommandOutput::error(
                "Reconcile scene",
                format!("stable_key '{key}' is duplicated in the desired state."),
            );
        }
        let target = format!("key:{key}");
        if let Some(id) = resolve_text_target(&target, ctx.scene) {
            if ctx.scene.get(id).is_some_and(|node| node.is_folder) {
                return CommandOutput::error(
                    "Reconcile scene",
                    format!("stable_key '{key}' belongs to a group, not an entity."),
                );
            }
            if let Some(desired) = primitive_arg(&parsed_command_from_params(
                "game.add",
                Value::Object(params.clone()),
            )) {
                if ctx
                    .scene
                    .get(id)
                    .is_some_and(|node| node.primitive != desired)
                {
                    return CommandOutput::error(
                        "Reconcile scene",
                        format!("stable_key '{key}' changes primitive type; delete and recreate it explicitly."),
                    );
                }
            }
            params.insert("target".to_string(), Value::String(target.clone()));
            params.remove("kind");
            params.remove("primitive");
            let parent = params.remove("parent").or_else(|| default_parent.clone());
            operations.push(serde_json::json!({
                "name": "game.update",
                "params": Value::Object(params)
            }));
            updated += 1;
            if parent
                .as_ref()
                .is_some_and(|parent| parent_requires_reparent(ctx.scene, id, parent))
            {
                operations.push(serde_json::json!({
                    "name": "game.reparent",
                    "params": {"target": target.clone(), "parent": parent.clone().unwrap_or(Value::Null), "preserve_world": true}
                }));
                reparented += 1;
            }
        } else {
            let has_kind = params
                .get("kind")
                .or_else(|| params.get("primitive"))
                .and_then(Value::as_str)
                .is_some_and(|kind| !kind.trim().is_empty());
            if !has_kind {
                return CommandOutput::error(
                    "Reconcile scene",
                    format!("entities[{index}] requires kind or primitive when it is new."),
                );
            }
            if !params.contains_key("parent") {
                if let Some(parent) = &default_parent {
                    params.insert("parent".to_string(), parent.clone());
                }
            }
            operations.push(serde_json::json!({
                "name": "game.add",
                "params": Value::Object(params)
            }));
            created += 1;
        }
    }

    let batch_command =
        parsed_command_from_params("game.batch", serde_json::json!({"operations": operations}));
    let mut output = batch(&batch_command, ctx);
    if matches!(output.level, crate::commands::output::CommandLevel::Error) {
        return output;
    }
    output.title = "Reconciled scene".to_string();
    output.lines.insert(
        0,
        format!("created: {created}, updated: {updated}, reparented: {reparented}"),
    );
    if let Some(object) = output.json.as_object_mut() {
        object.insert(
            "reconcile".to_string(),
            serde_json::json!({
                "created": created,
                "updated": updated,
                "reparented": reparented,
                "preserved_unmentioned": true,
                "stable_key_identity": true
            }),
        );
    }
    output
}

/// Apply an explicit, audit-driven repair as one atomic batch. The engine does
/// not guess geometry from a prose audit: the Agent supplies the concrete
/// operations after inspecting the reported targets and the repair remains
/// reversible through the normal command transaction.
fn repair_scene(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let operations = match structured_or_json_array(command, "operations", "operations") {
        Ok(Some(operations)) => operations,
        Ok(None) => {
            return CommandOutput::error(
                "Repair scene",
                "Provide explicit operations=[{name, params}, ...] from the audit result.",
            )
        }
        Err(error) => return CommandOutput::error("Repair scene", error),
    };
    if operations.is_empty() || operations.len() > 512 {
        return CommandOutput::error(
            "Repair scene",
            "A repair requires between 1 and 512 ordered operations.",
        );
    }
    let root = text_argument(command, "root");
    let root_id = root.as_deref().and_then(|root| {
        (!matches!(
            root.trim().to_ascii_lowercase().as_str(),
            "" | "root" | "scene" | "none" | "null"
        ))
        .then(|| resolve_repair_target(ctx.scene, root))
        .flatten()
    });
    if root.as_deref().is_some_and(|root| {
        !matches!(
            root.trim().to_ascii_lowercase().as_str(),
            "" | "root" | "scene" | "none" | "null"
        )
    }) && root_id.is_none()
    {
        return CommandOutput::error(
            "Repair scene",
            format!(
                "Repair root '{}' was not found; no changes were applied.",
                root.as_deref().unwrap_or_default()
            ),
        );
    }
    if let Err(error) = validate_repair_operations(ctx.scene, &operations, root_id) {
        return CommandOutput::error("Repair scene", error);
    }
    let batch_command =
        parsed_command_from_params("game.batch", serde_json::json!({"operations": operations}));
    let mut output = batch(&batch_command, ctx);
    if matches!(output.level, crate::commands::output::CommandLevel::Error) {
        return output;
    }
    output.title = "Repaired scene".to_string();
    output.lines.insert(
        0,
        format!(
            "repair operations: {}, scope: {}",
            output
                .json
                .get("operations")
                .and_then(Value::as_array)
                .map_or(0, Vec::len),
            root.as_deref().unwrap_or("scene")
        ),
    );
    if let Some(object) = output.json.as_object_mut() {
        object.insert(
            "repair".to_string(),
            serde_json::json!({
                "atomic": true,
                "scope": root,
                "explicit_operations": true,
                "inferred_geometry": false
            }),
        );
    }
    output
}

fn validate_repair_operations(
    scene: &SceneGraph,
    operations: &[Value],
    root: Option<SceneNodeId>,
) -> Result<(), String> {
    let mut staged_groups = Vec::<String>::new();
    for (index, operation) in operations.iter().enumerate() {
        let object = operation
            .as_object()
            .ok_or_else(|| format!("Repair operation {} must be an object.", index + 1))?;
        let raw_name = object
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("Repair operation {} is missing name.", index + 1))?;
        let name = canonical_repair_operation_name(raw_name);
        let params = object
            .get("params")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                format!(
                    "Repair operation {} ({name}) requires an object params value.",
                    index + 1
                )
            })?;

        match name.as_str() {
            "game.add" | "game.create_group" => {
                if let Some(root_id) = root {
                    let parent = params.get("parent").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "Repair operation {} ({name}) must parent new nodes inside the audited root.",
                            index + 1
                        )
                    })?;
                    validate_repair_parent_or_staged(
                        scene,
                        parent,
                        root_id,
                        index,
                        &name,
                        &staged_groups,
                    )?;
                }
                if name == "game.create_group" {
                    let group_name = params
                        .get("name")
                        .and_then(Value::as_str)
                        .map(display_name)
                        .filter(|name| !name.is_empty())
                        .ok_or_else(|| {
                            format!(
                                "Repair operation {} (game.create_group) requires a non-empty name.",
                                index + 1
                            )
                        })?;
                    staged_groups.push(group_name.to_ascii_lowercase());
                    if let Some(stable_key) = params
                        .get("stable_key")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|key| !key.is_empty())
                    {
                        staged_groups.push(stable_key.to_ascii_lowercase());
                        staged_groups.push(format!("key:{}", stable_key.to_ascii_lowercase()));
                    }
                }
            }
            "game.update" | "game.delete" | "game.duplicate" | "game.reparent" | "game.snap" => {
                let target = params
                    .get("target")
                    .and_then(Value::as_str)
                    .filter(|target| !target.trim().is_empty())
                    .ok_or_else(|| {
                        format!(
                            "Repair operation {} ({name}) requires an explicit target; selection fallback is disabled.",
                            index + 1
                        )
                    })?;
                let target_id = resolve_repair_target(scene, target).ok_or_else(|| {
                    format!(
                        "Repair operation {} ({name}) target '{target}' was not found.",
                        index + 1
                    )
                })?;
                if let Some(root_id) = root {
                    if !is_in_repair_scope(scene, target_id, root_id) {
                        return Err(format!(
                            "Repair operation {} ({name}) target '{target}' is outside the audited root.",
                            index + 1
                        ));
                    }
                    if name == "game.reparent" {
                        let parent = params
                            .get("parent")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                format!(
                                    "Repair operation {} (game.reparent) must specify a parent inside the audited root.",
                                    index + 1
                                )
                            })?;
                        validate_repair_parent_or_staged(
                            scene,
                            parent,
                            root_id,
                            index,
                            &name,
                            &staged_groups,
                        )?;
                    }
                    if name == "game.snap" {
                        if let Some(surface) = params
                            .get("snap_to")
                            .and_then(Value::as_str)
                            .filter(|value| !value.trim().is_empty())
                        {
                            let surface_id = resolve_repair_target(scene, surface).ok_or_else(|| {
                                format!(
                                    "Repair operation {} (game.snap) surface '{surface}' was not found.",
                                    index + 1
                                )
                            })?;
                            if !is_in_repair_scope(scene, surface_id, root_id) {
                                return Err(format!(
                                    "Repair operation {} (game.snap) surface '{surface}' is outside the audited root.",
                                    index + 1
                                ));
                            }
                        }
                    }
                }
            }
            "game.arrange_grid" | "game.generate_prefab" if root.is_some() => {
                return Err(format!(
                    "Repair operation {} ({name}) cannot be scoped safely; use explicit scene_update or scene_create operations instead.",
                    index + 1
                ));
            }
            "game.arrange_grid" | "game.generate_prefab" => {}
            _ => {
                return Err(format!(
                    "Repair operation {} uses unsupported command '{}'.",
                    index + 1,
                    raw_name
                ));
            }
        }
    }
    Ok(())
}

fn canonical_repair_operation_name(raw_name: &str) -> String {
    match raw_name
        .trim_start_matches('/')
        .to_ascii_lowercase()
        .as_str()
    {
        "scene_create" | "game.add" => "game.add".to_string(),
        "scene_create_group" | "game.create_group" => "game.create_group".to_string(),
        "scene_update" | "game.update" => "game.update".to_string(),
        "scene_delete" | "game.delete" => "game.delete".to_string(),
        "scene_duplicate" | "game.duplicate" => "game.duplicate".to_string(),
        "scene_reparent" | "game.reparent" => "game.reparent".to_string(),
        "scene_snap" | "scene.snap" | "game.snap" => "game.snap".to_string(),
        "scene_arrange" | "game.arrange_grid" => "game.arrange_grid".to_string(),
        "scene_instantiate_prefab" | "game.generate_prefab" => "game.generate_prefab".to_string(),
        other => other.to_string(),
    }
}

fn resolve_repair_target(scene: &SceneGraph, target: &str) -> Option<SceneNodeId> {
    let command =
        parsed_command_from_params("repair_target", serde_json::json!({"target": target}));
    resolve_target(&command, scene, &SceneSelectionState::default())
}

fn validate_repair_parent(
    scene: &SceneGraph,
    parent: &str,
    root: SceneNodeId,
    index: usize,
    operation: &str,
) -> Result<(), String> {
    let parent = parent.trim();
    if parent.is_empty()
        || matches!(
            parent.to_ascii_lowercase().as_str(),
            "root" | "scene" | "none" | "null"
        )
    {
        return Err(format!(
            "Repair operation {} ({operation}) must parent the new or moved node inside the audited root.",
            index + 1
        ));
    }
    let parent_id = resolve_repair_target(scene, parent).ok_or_else(|| {
        format!(
            "Repair operation {} ({operation}) parent '{parent}' was not found.",
            index + 1
        )
    })?;
    if !is_in_repair_scope(scene, parent_id, root) {
        return Err(format!(
            "Repair operation {} ({operation}) parent '{parent}' is outside the audited root.",
            index + 1
        ));
    }
    Ok(())
}

fn validate_repair_parent_or_staged(
    scene: &SceneGraph,
    parent: &str,
    root: SceneNodeId,
    index: usize,
    operation: &str,
    staged_groups: &[String],
) -> Result<(), String> {
    let normalized = display_name(parent).to_ascii_lowercase();
    let leaf = normalized.rsplit('/').next().unwrap_or(&normalized);
    if staged_groups
        .iter()
        .any(|known| known == &normalized || known == leaf)
    {
        return Ok(());
    }
    validate_repair_parent(scene, parent, root, index, operation)
}

fn is_in_repair_scope(scene: &SceneGraph, node: SceneNodeId, root: SceneNodeId) -> bool {
    let mut current = Some(node);
    for _ in 0..=scene.len() {
        if current == Some(root) {
            return true;
        }
        current = current.and_then(|id| scene.get(id).and_then(|item| item.parent));
    }
    false
}

fn stable_key_from_params(params: &serde_json::Map<String, Value>) -> Option<&str> {
    params
        .get("stable_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
}

fn parent_requires_reparent(scene: &SceneGraph, id: SceneNodeId, parent: &Value) -> bool {
    let current_parent = scene.get(id).and_then(|node| node.parent);
    let desired_parent = match parent {
        Value::Null => None,
        Value::String(raw) => {
            let raw = raw.trim();
            if raw.is_empty()
                || matches!(
                    raw.to_ascii_lowercase().as_str(),
                    "root" | "scene" | "none" | "null"
                )
            {
                None
            } else {
                resolve_text_target(raw, scene)
            }
        }
        _ => return true,
    };
    desired_parent != current_parent
}

fn build_scene(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let groups = match structured_or_json_array(command, "groups", "groups") {
        Ok(Some(groups)) => groups,
        Ok(None) => Vec::new(),
        Err(error) => return CommandOutput::error("Build scene", error),
    };
    let entities = match structured_or_json_array(command, "entities", "entities") {
        Ok(Some(entities)) => entities,
        Ok(None) => Vec::new(),
        Err(error) => return CommandOutput::error("Build scene", error),
    };
    if groups.is_empty() && entities.is_empty() {
        return CommandOutput::error(
            "Build scene",
            "Provide at least one group or entity to build.",
        );
    }
    if groups.len() > 32 || entities.len() > 128 {
        return CommandOutput::error(
            "Build scene",
            "A build accepts at most 32 groups and 128 entities.",
        );
    }
    if let Err(error) = validate_design_profile(command, &groups, &entities, None) {
        return CommandOutput::error("Build scene", error);
    }
    let group_count = groups.len();
    let entity_count = entities.len();
    let default_parent = command.structured_arg("parent").cloned().or_else(|| {
        command
            .arg("parent")
            .map(|value| Value::String(value.to_string()))
    });
    let groups = match order_build_groups(groups, ctx.scene, default_parent.as_ref()) {
        Ok(groups) => groups,
        Err(error) => return CommandOutput::error("Build scene", error),
    };
    let mut operations = Vec::with_capacity(groups.len() + entities.len());
    for (index, group) in groups.into_iter().enumerate() {
        let Some(mut params) = group.as_object().cloned() else {
            return CommandOutput::error(
                "Build scene",
                format!("groups[{index}] must be an object."),
            );
        };
        let valid_name = params
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(|name| !name.trim().is_empty());
        if !valid_name {
            return CommandOutput::error(
                "Build scene",
                format!("groups[{index}].name is required."),
            );
        }
        if !params.contains_key("parent") {
            if let Some(parent) = &default_parent {
                params.insert("parent".to_string(), parent.clone());
            }
        }
        operations.push(serde_json::json!({
            "name": "game.create_group",
            "params": Value::Object(params)
        }));
    }
    for (index, entity) in entities.into_iter().enumerate() {
        let Some(mut params) = entity.as_object().cloned() else {
            return CommandOutput::error(
                "Build scene",
                format!("entities[{index}] must be an object."),
            );
        };
        let valid_name = params
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(|name| !name.trim().is_empty());
        if !valid_name {
            return CommandOutput::error(
                "Build scene",
                format!("entities[{index}].name is required."),
            );
        }
        if !params.contains_key("parent") {
            if let Some(parent) = &default_parent {
                params.insert("parent".to_string(), parent.clone());
            }
        }
        operations.push(serde_json::json!({
            "name": "game.add",
            "params": Value::Object(params)
        }));
    }

    let batch_params = serde_json::json!({"operations": operations});
    let batch_command = parsed_command_from_params("game.batch", batch_params);
    let mut output = batch(&batch_command, ctx);
    if matches!(output.level, crate::commands::output::CommandLevel::Error) {
        return output;
    }
    output.title = "Built scene structure".to_string();
    output.lines.insert(
        0,
        format!("groups: {group_count}, entities: {entity_count}"),
    );
    if let Some(object) = output.json.as_object_mut() {
        object.insert(
            "build".to_string(),
            serde_json::json!({
                "groups": group_count,
                "entities": entity_count,
                "atomic": true
            }),
        );
    }
    output
}

/// Keep scene_build forgiving about the order in which a model lists groups.
/// A generated hierarchy often mentions a parent before or after its child;
/// the final operation order must still create the parent first. Existing
/// scene targets are resolved immediately, while unresolved references to
/// another group in this payload are deferred until that group is created.
fn order_build_groups(
    groups: Vec<Value>,
    scene: &SceneGraph,
    default_parent: Option<&Value>,
) -> Result<Vec<Value>, String> {
    let mut pending = groups.into_iter().enumerate().collect::<Vec<_>>();
    let mut ordered = Vec::with_capacity(pending.len());

    while !pending.is_empty() {
        let ready = pending.iter().position(|(_, group)| {
            let parent = group
                .get("parent")
                .cloned()
                .or_else(|| default_parent.cloned());
            !build_parent_waits_for_pending_group(parent.as_ref(), &pending, scene)
        });
        let Some(index) = ready else {
            let names = pending
                .iter()
                .filter_map(|(_, group)| group.get("name").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(format!(
                "Group hierarchy contains an unresolved or circular parent reference: {names}."
            ));
        };
        let (_, group) = pending.remove(index);
        ordered.push(group);
    }

    Ok(ordered)
}

/// Validate an optional design profile before a semantic build reaches the
/// mutation batch. This is intentionally declarative: the engine does not
/// invent walls or furniture, but it does stop a real-world build that forgot
/// the parts that make it read as a place instead of disconnected primitives.
fn validate_design_profile(
    command: &ParsedCommand,
    groups: &[Value],
    entities: &[Value],
    existing_scene: Option<&SceneGraph>,
) -> Result<(), String> {
    let Some(profile) = text_argument(command, "design_profile") else {
        return Ok(());
    };
    let profile = profile.trim().to_ascii_lowercase();
    let required: &[(&str, &[&str])] = match profile.as_str() {
        "generic" => &[],
        "real_world" | "real-world" | "building" => &[
            (
                "floor",
                &["floor", "ground", "suelo", "piso", "groundplane"],
            ),
            (
                "enclosure",
                &["wall", "walls", "muro", "pared", "facade", "enclosure"],
            ),
            (
                "entrance",
                &["entrance", "entry", "door", "doorway", "access", "entrada", "acceso"],
            ),
            (
                "circulation",
                &["aisle", "path", "road", "circulation", "sidewalk", "walkway", "pasillo", "circulacion", "banqueta"],
            ),
        ],
        "store" | "supermarket" => &[
            (
                "floor",
                &["floor", "ground", "suelo", "piso", "groundplane"],
            ),
            (
                "enclosure",
                &["wall", "walls", "muro", "pared", "facade", "enclosure"],
            ),
            (
                "entrance",
                &["entrance", "entry", "door", "doorway", "access", "entrada", "acceso"],
            ),
            (
                "circulation",
                &["aisle", "path", "road", "circulation", "sidewalk", "walkway", "pasillo", "circulacion", "banqueta"],
            ),
            (
                "primary_modules",
                &["shelf", "shelves", "checkout", "counter", "caja", "estante", "gondola"],
            ),
        ],
        "parking" | "parking_lot" | "parking-lot" => &[
            (
                "ground",
                &["ground", "floor", "pavement", "asphalt", "suelo", "piso", "pavimento"],
            ),
            (
                "parking",
                &["parking", "stall", "space", "vehicle", "car", "estacionamiento", "cajon", "auto"],
            ),
            (
                "entrance",
                &["entrance", "entry", "road", "gate", "access", "entrada", "acceso"],
            ),
            (
                "circulation",
                &["lane", "drive", "road", "aisle", "circulation", "carril", "calle", "pasillo", "circulacion"],
            ),
        ],
        "outdoor" | "environment" => &[
            (
                "ground",
                &["ground", "floor", "terrain", "land", "suelo", "piso", "terreno"],
            ),
            (
                "circulation",
                &["path", "road", "walkway", "circulation", "camino", "sendero", "circulacion"],
            ),
        ],
        other => {
            return Err(format!(
                "Unknown design_profile '{other}'. Use generic, real_world, building, supermarket, parking, or outdoor."
            ))
        }
    };
    if required.is_empty() {
        return Ok(());
    }

    let mut design_values = groups
        .iter()
        .chain(entities.iter())
        .flat_map(|item| design_text_values(item).into_iter())
        .collect::<Vec<_>>();
    if let Some(scene) = existing_scene {
        design_values.extend(scene_design_text_values(scene));
    }
    let payload_text = design_values.join(" ").to_ascii_lowercase();
    let mut missing = required
        .iter()
        .filter(|(_, tokens)| !tokens.iter().any(|token| payload_text.contains(token)))
        .map(|(feature, _)| (*feature).to_string())
        .collect::<Vec<_>>();
    let has_parent = text_argument(command, "parent").is_some_and(|parent| {
        let parent = parent.trim();
        !parent.is_empty()
            && !matches!(
                parent.to_ascii_lowercase().as_str(),
                "root" | "scene" | "none" | "null"
            )
    });
    let has_existing_root_group = existing_scene.is_some_and(|scene| {
        scene.roots().iter().any(|id| {
            scene
                .get(*id)
                .is_some_and(|node| node.is_folder && node.visible)
        })
    });
    if groups.is_empty() && !has_parent && !has_existing_root_group {
        missing.push("root_group".to_string());
    }
    for (feature, tokens) in required {
        let renderable_count = renderable_feature_item_count(entities, tokens)
            + existing_scene
                .map(|scene| scene_renderable_feature_item_count(scene, tokens))
                .unwrap_or_default();
        if renderable_count == 0 {
            missing.push(format!("{feature} (renderable entity)"));
        }
    }
    if matches!(
        profile.as_str(),
        "real_world" | "real-world" | "building" | "store" | "supermarket"
    ) && (renderable_feature_item_count(
        entities,
        &["wall", "walls", "muro", "pared", "facade", "enclosure"],
    ) + existing_scene
        .map(|scene| {
            scene_renderable_feature_item_count(
                scene,
                &["wall", "walls", "muro", "pared", "facade", "enclosure"],
            )
        })
        .unwrap_or_default())
        < 2
    {
        missing.push("enclosure (at least 2 structural parts)".to_string());
    }
    missing.sort_unstable();
    missing.dedup();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "design_profile '{profile}' is incomplete. Missing: {}. Add named or tagged groups/entities for these features; no changes were applied.",
            missing.join(", ")
        ))
    }
}

fn renderable_feature_item_count(entities: &[Value], tokens: &[&str]) -> usize {
    entities
        .iter()
        .filter(|item| payload_is_renderable(item))
        .filter(|item| {
            design_text_values(item).iter().any(|value| {
                tokens
                    .iter()
                    .any(|token| value.to_ascii_lowercase().contains(token))
            })
        })
        .count()
}

fn scene_renderable_feature_item_count(scene: &SceneGraph, tokens: &[&str]) -> usize {
    scene
        .iter()
        .filter(|(_, node)| !node.is_folder && node.primitive != Primitive::Empty)
        .filter(|(_, node)| {
            scene_node_design_text_values(node)
                .iter()
                .any(|value| tokens.iter().any(|token| value.contains(token)))
        })
        .count()
}

fn scene_design_text_values(scene: &SceneGraph) -> Vec<String> {
    scene
        .iter()
        .flat_map(|(_, node)| scene_node_design_text_values(node))
        .collect()
}

fn scene_node_design_text_values(node: &raf_core::scene::graph::SceneNode) -> Vec<String> {
    let mut values = vec![node.name.clone(), node.primitive.label().to_string()];
    if let Some(role) = &node.semantic_role {
        values.push(role.clone());
    }
    if let Some(stable_key) = &node.stable_key {
        values.push(stable_key.clone());
    }
    values.push(node.tags.join(" "));
    values
        .into_iter()
        .map(|value| value.to_ascii_lowercase())
        .collect()
}

fn payload_is_renderable(item: &Value) -> bool {
    let Some(object) = item.as_object() else {
        return false;
    };
    let primitive = object
        .get("kind")
        .or_else(|| object.get("primitive"))
        .or_else(|| object.get("shape"))
        .or_else(|| object.get("type"))
        .and_then(Value::as_str)
        .map(|value| value.trim().to_ascii_lowercase());
    !matches!(primitive.as_deref(), Some("empty" | "group" | "folder"))
}

fn design_text_values(item: &Value) -> Vec<String> {
    let Some(object) = item.as_object() else {
        return Vec::new();
    };
    let mut values = Vec::new();
    for key in [
        "name",
        "semantic_role",
        "stable_key",
        "role",
        "kind",
        "primitive",
    ] {
        if let Some(value) = object.get(key).and_then(Value::as_str) {
            values.push(value.to_string());
        }
    }
    for key in ["tags", "features"] {
        if let Some(items) = object.get(key).and_then(Value::as_array) {
            values.extend(items.iter().filter_map(Value::as_str).map(str::to_string));
        }
    }
    values
}

fn build_parent_waits_for_pending_group(
    parent: Option<&Value>,
    pending: &[(usize, Value)],
    scene: &SceneGraph,
) -> bool {
    let Some(Value::String(parent)) = parent else {
        return false;
    };
    let parent = parent.trim();
    if parent.is_empty()
        || matches!(
            parent.to_ascii_lowercase().as_str(),
            "root" | "scene" | "none" | "null"
        )
        || resolve_text_target(parent, scene).is_some()
    {
        return false;
    }

    let normalized = parent.trim_matches('/');
    let leaf = normalized.rsplit('/').next().unwrap_or(normalized);
    pending.iter().any(|(_, group)| {
        let name_match = group
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(|name| {
                display_name(name).eq_ignore_ascii_case(normalized)
                    || display_name(name).eq_ignore_ascii_case(leaf)
            });
        let key_match = group
            .get("stable_key")
            .and_then(Value::as_str)
            .is_some_and(|key| {
                key.eq_ignore_ascii_case(parent)
                    || format!("key:{key}").eq_ignore_ascii_case(parent)
            });
        name_match || key_match
    })
}

fn add_entity(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let primitive = match primitive_arg(command) {
        Some(primitive) => primitive,
        None if primitive_argument(command).is_none() => Primitive::Cube,
        None => {
            return CommandOutput::error(
                "Create entity",
                "Unsupported primitive. Use empty, cube, sphere, plane, or cylinder.",
            )
        }
    };
    let transform = match parse_transform_patch(command) {
        Ok(transform) => transform,
        Err(error) => return CommandOutput::error("Create entity", error),
    };
    let requested_color = match color_argument(command, NodeColor::for_primitive(primitive)) {
        Ok(color) => color,
        Err(error) => return CommandOutput::error("Create entity", error),
    };
    let parent = match parent_target(command, ctx.scene) {
        Ok(parent) => parent,
        Err(error) => return CommandOutput::error("Create entity", error),
    };
    let metadata = match parse_semantic_metadata(command) {
        Ok(metadata) => metadata,
        Err(error) => return CommandOutput::error("Create entity", error),
    };
    if let Err(error) = validate_semantic_metadata(ctx.scene, &metadata, None) {
        return CommandOutput::error("Create entity", error);
    }
    let name = command
        .arg("name")
        .map(display_name)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| default_entity_name(ctx.scene, primitive));
    let id = match PrimitiveModelManifest::builtin_for_primitive(primitive) {
        Ok(Some(manifest)) => match manifest.instantiate_single_root_into_scene(
            ctx.scene,
            Some(&name),
            primitive_asset_source(primitive),
        ) {
            Ok(id) => id,
            Err(error) => {
                return CommandOutput::error(
                    "Create entity",
                    format!("Could not import primitive asset: {error}"),
                );
            }
        },
        Ok(None) => ctx.scene.add_root_with_primitive(&name, primitive),
        Err(error) => {
            return CommandOutput::error(
                "Create entity",
                format!("Could not load primitive asset manifest: {error}"),
            );
        }
    };

    if let Some(node) = ctx.scene.get_mut(id) {
        node.position = transform.position_or(Vec3::ZERO);
        node.rotation = transform.rotation_or(Vec3::ZERO);
        node.scale = transform.scale_or(Vec3::ONE);
        if let Some(color) = requested_color {
            node.color = color;
        }
        apply_semantic_metadata(node, &metadata);
    }

    if let Some(parent) = parent {
        if !ctx.scene.reparent_node(id, Some(parent)) {
            ctx.scene.remove_node(id);
            return CommandOutput::error(
                "Create entity",
                "The requested parent is invalid or would create a hierarchy cycle.",
            );
        }
    }

    select_ids(ctx, vec![id]);
    let node = ctx.scene.get(id).expect("new node exists");
    CommandOutput::changed(
        format!("Created {}", node.name),
        node_detail_lines(id, node, ctx.scene),
        node_json(id, node, ctx.scene),
    )
}

fn primitive_asset_source(primitive: Primitive) -> Option<&'static str> {
    match primitive {
        Primitive::Cube => Some("builtin://primitive/cube"),
        Primitive::Sphere => Some("builtin://primitive/sphere"),
        Primitive::Cylinder => Some("builtin://primitive/cylinder"),
        Primitive::Plane => Some("builtin://primitive/plane"),
        Primitive::Empty => None,
    }
}

fn select_entity(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let Some(id) = resolve_target(command, ctx.scene, ctx.selection) else {
        return CommandOutput::error("Select entity", "Target not found.");
    };
    select_ids(ctx, vec![id]);
    let node = ctx.scene.get(id).expect("valid node");
    CommandOutput::info(
        format!("Selected {}", node.name),
        node_detail_lines(id, node, ctx.scene),
        node_json(id, node, ctx.scene),
    )
}

fn rename_entity(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let Some(id) = resolve_target(command, ctx.scene, ctx.selection) else {
        return CommandOutput::error("Rename entity", "Target not found.");
    };
    let Some(name) = text_argument(command, "name") else {
        return CommandOutput::error("Rename entity", "Missing name=<new name>.");
    };
    let name = display_name(&name);
    if name.is_empty() {
        return CommandOutput::error("Rename entity", "The entity name cannot be empty.");
    }
    if let Some(node) = ctx.scene.get_mut(id) {
        node.name = name;
    }
    select_ids(ctx, vec![id]);
    let node = ctx.scene.get(id).expect("valid node");
    CommandOutput::changed(
        format!("Renamed {}", node.name),
        node_detail_lines(id, node, ctx.scene),
        node_json(id, node, ctx.scene),
    )
}

fn delete_entity(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let Some(id) = resolve_target(command, ctx.scene, ctx.selection) else {
        return CommandOutput::error("Delete entity", "Target not found.");
    };
    let name = ctx
        .scene
        .get(id)
        .map(|node| node.name.clone())
        .unwrap_or_default();
    if !ctx.scene.remove_node(id) {
        return CommandOutput::error("Delete entity", "Could not remove target.");
    }
    select_ids(ctx, Vec::new());
    CommandOutput::changed(
        format!("Deleted {name}"),
        vec![
            format!("removed_id: {}", id.0),
            format!("removed_name: {name}"),
        ],
        serde_json::json!({
            "ok": true,
            "removed": {"id": id.0, "name": name}
        }),
    )
}

fn duplicate_entity(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let Some(id) = resolve_target(command, ctx.scene, ctx.selection) else {
        return CommandOutput::error("Duplicate entity", "Target not found.");
    };
    let source = ctx.scene.get(id).expect("resolved source exists").clone();
    let parent_specified = text_argument(command, "parent").is_some();
    let destination_parent = if parent_specified {
        match parent_target(command, ctx.scene) {
            Ok(parent) => parent,
            Err(error) => return CommandOutput::error("Duplicate entity", error),
        }
    } else {
        source.parent
    };
    if let Some(parent) = destination_parent {
        if !ctx.scene.get(parent).is_some_and(|node| node.is_folder) {
            return CommandOutput::error(
                "Duplicate entity",
                "parent must reference a live folder or group.",
            );
        }
    }
    let count = match usize_argument(command, "count", 1, 1, 128) {
        Ok(count) => count,
        Err(error) => return CommandOutput::error("Duplicate entity", error),
    };
    let offset = match structured_or_json_value(command, "offset", "offset") {
        Ok(Some(value)) => match parse_vec3_value(&value, "offset") {
            Ok([x, y, z]) => Vec3::new(x, y, z),
            Err(error) => return CommandOutput::error("Duplicate entity", error),
        },
        Ok(None) => Vec3::new(1.0, 0.0, 0.0),
        Err(error) => return CommandOutput::error("Duplicate entity", error),
    };
    let axis = match axis_vector(command.arg("axis").unwrap_or("x")) {
        Ok(axis) => axis,
        Err(error) => return CommandOutput::error("Duplicate entity", error),
    };
    let spacing = match finite_f32_argument(command, "spacing", 1.0) {
        Ok(value) => value,
        Err(error) => return CommandOutput::error("Duplicate entity", error),
    };
    let preserve_world = match bool_argument(command, "preserve_world", true) {
        Ok(value) => value,
        Err(error) => return CommandOutput::error("Duplicate entity", error),
    };
    let requested_name = text_argument(command, "name").map(|name| display_name(&name));
    let mut created = Vec::with_capacity(count);
    let mut entities = Vec::with_capacity(count);
    for index in 0..count {
        let Some(new_id) = ctx.scene.duplicate_node(id) else {
            return CommandOutput::error("Duplicate entity", "Could not duplicate target.");
        };
        if let Some(node) = ctx.scene.get_mut(new_id) {
            node.position = source.position;
        }
        if destination_parent != source.parent {
            let moved = if preserve_world {
                ctx.scene
                    .reparent_node_preserve_world_transform(new_id, destination_parent)
            } else {
                ctx.scene.reparent_node(new_id, destination_parent)
            };
            if !moved {
                return CommandOutput::error(
                    "Duplicate entity",
                    "The requested parent is invalid or would create a hierarchy cycle.",
                );
            }
        }
        if let Some(node) = ctx.scene.get_mut(new_id) {
            node.position += offset + axis * spacing * index as f32;
            if let Some(base) = requested_name.as_deref() {
                node.name = if count == 1 {
                    base.to_string()
                } else {
                    format!("{base} {}", index + 1)
                };
            }
        }
        let node = ctx.scene.get(new_id).expect("duplicate exists");
        entities.push(node_json(new_id, node, ctx.scene)["entity"].clone());
        created.push(new_id);
    }
    select_ids(ctx, created.clone());
    CommandOutput::changed(
        format!("Duplicated {} time(s)", created.len()),
        vec![
            format!("source: {}", presentation_name(id, &source)),
            format!("copies: {}", created.len()),
            format!(
                "parent: {}",
                destination_parent
                    .map(|id| id.0.to_string())
                    .unwrap_or_else(|| "root".to_string())
            ),
            format!(
                "offset: [{:.3}, {:.3}, {:.3}]",
                offset.x, offset.y, offset.z
            ),
            format!("spacing: {spacing:.3}"),
        ],
        serde_json::json!({
            "ok": true,
            "source": format!("entity:{}", source.uuid),
            "count": created.len(),
            "created_ids": created.iter().map(|id| id.0).collect::<Vec<_>>(),
            "entities": entities
        }),
    )
}

fn snap_entity(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let Some(id) = resolve_target(command, ctx.scene, ctx.selection) else {
        return CommandOutput::error("Snap entity", "Target not found.");
    };
    let mode = command
        .arg("mode")
        .unwrap_or("grid")
        .trim()
        .to_ascii_lowercase();
    let gap = match finite_f32_argument(command, "gap", 0.0) {
        Ok(value) if value >= 0.0 => value,
        Ok(_) => return CommandOutput::error("Snap entity", "gap must be zero or positive."),
        Err(error) => return CommandOutput::error("Snap entity", error),
    };
    if mode == "grid" {
        let grid = match finite_f32_argument(command, "grid", 1.0) {
            Ok(value) if value > 0.0 => value,
            Ok(_) => return CommandOutput::error("Snap entity", "grid must be greater than zero."),
            Err(error) => return CommandOutput::error("Snap entity", error),
        };
        if let Some(node) = ctx.scene.get_mut(id) {
            node.position = (node.position / grid).round() * grid;
        }
    } else if matches!(mode.as_str(), "floor" | "surface") {
        let axis_name = command.arg("axis").unwrap_or("y");
        let axis = match axis_index(axis_name) {
            Ok(axis) => axis,
            Err(error) => return CommandOutput::error("Snap entity", error),
        };
        let placement = command
            .arg("placement")
            .unwrap_or(if mode == "floor" { "after" } else { "after" })
            .trim()
            .to_ascii_lowercase();
        if !matches!(placement.as_str(), "before" | "after" | "center") {
            return CommandOutput::error(
                "Snap entity",
                "placement must be before, after, or center.",
            );
        }
        let Some(source_bounds) = world_bounds(ctx.scene, id) else {
            return CommandOutput::error(
                "Snap entity",
                "The target has no renderable world bounds to snap.",
            );
        };
        let snap_target =
            text_argument(command, "snap_to").or_else(|| text_argument(command, "surface"));
        let surface_bounds = match snap_target.as_deref() {
            Some(target) => {
                let Some(surface_id) = resolve_text_target(target, ctx.scene) else {
                    return CommandOutput::error(
                        "Snap entity",
                        format!("Snap surface target '{target}' was not found."),
                    );
                };
                if surface_id == id {
                    return CommandOutput::error(
                        "Snap entity",
                        "target and snap_to must reference different entities.",
                    );
                }
                let Some(bounds) = world_bounds(ctx.scene, surface_id) else {
                    return CommandOutput::error(
                        "Snap entity",
                        "The snap surface has no renderable world bounds.",
                    );
                };
                bounds
            }
            None if mode == "floor" => (Vec3::ZERO, Vec3::ZERO),
            None => {
                return CommandOutput::error(
                    "Snap entity",
                    "snap_to is required when mode=surface.",
                )
            }
        };
        let source_min = component(source_bounds.0, axis);
        let source_max = component(source_bounds.1, axis);
        let source_center = (source_min + source_max) * 0.5;
        let surface_min = component(surface_bounds.0, axis);
        let surface_max = component(surface_bounds.1, axis);
        let surface_center = (surface_min + surface_max) * 0.5;
        let delta = match placement.as_str() {
            "before" => surface_min - gap - source_max,
            "center" => surface_center - source_center,
            _ => surface_max + gap - source_min,
        };
        let mut world_delta = Vec3::ZERO;
        set_component(&mut world_delta, axis, delta);
        let parent = ctx.scene.get(id).and_then(|node| node.parent);
        let local_delta = parent
            .map(|parent| {
                ctx.scene
                    .world_matrix(parent)
                    .inverse()
                    .transform_vector3(world_delta)
            })
            .unwrap_or(world_delta);
        if let Some(node) = ctx.scene.get_mut(id) {
            node.position += local_delta;
        }
    } else {
        return CommandOutput::error("Snap entity", "mode must be grid, floor, or surface.");
    }
    select_ids(ctx, vec![id]);
    let node = ctx.scene.get(id).expect("snapped node exists");
    CommandOutput::changed(
        format!("Snapped {}", node.name),
        vec![format!("mode: {mode}"), format!("gap: {gap:.3}")],
        node_json(id, node, ctx.scene),
    )
}

fn set_transform(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let Some(id) = resolve_target(command, ctx.scene, ctx.selection) else {
        return CommandOutput::error("Set transform", "Target not found.");
    };
    let transform = match parse_transform_patch(command) {
        Ok(transform) => transform,
        Err(error) => return CommandOutput::error("Set transform", error),
    };
    if transform.is_empty() {
        return CommandOutput::error(
            "Set transform",
            "At least one position, rotation, or scale value is required.",
        );
    }
    if let Some(node) = ctx.scene.get_mut(id) {
        apply_transform_patch(node, transform);
    }
    select_ids(ctx, vec![id]);
    let node = ctx.scene.get(id).expect("valid node");
    CommandOutput::changed(
        format!("Transform updated {}", node.name),
        node_detail_lines(id, node, ctx.scene),
        node_json(id, node, ctx.scene),
    )
}

fn update_entity(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let Some(id) = resolve_target(command, ctx.scene, ctx.selection) else {
        return CommandOutput::error("Update entity", "Target not found.");
    };
    let Some(current_node) = ctx.scene.get(id) else {
        return CommandOutput::error("Update entity", "Target not found.");
    };
    let transform = match parse_transform_patch(command) {
        Ok(transform) => transform,
        Err(error) => return CommandOutput::error("Update entity", error),
    };
    let requested_color = match color_argument(command, current_node.color) {
        Ok(color) => color,
        Err(error) => return CommandOutput::error("Update entity", error),
    };
    let visible = match command
        .arg("visible")
        .map(|value| parse_bool_literal(value, "visible"))
        .transpose()
    {
        Ok(value) => value,
        Err(error) => return CommandOutput::error("Update entity", error),
    };
    let locked = match command
        .arg("locked")
        .map(|value| parse_bool_literal(value, "locked"))
        .transpose()
    {
        Ok(value) => value,
        Err(error) => return CommandOutput::error("Update entity", error),
    };
    let metadata = match parse_semantic_metadata(command) {
        Ok(metadata) => metadata,
        Err(error) => return CommandOutput::error("Update entity", error),
    };
    if let Err(error) = validate_semantic_metadata(ctx.scene, &metadata, Some(id)) {
        return CommandOutput::error("Update entity", error);
    }
    let Some(node) = ctx.scene.get_mut(id) else {
        return CommandOutput::error("Update entity", "Target not found.");
    };
    if let Some(name) = text_argument(command, "name") {
        let name = display_name(&name);
        if name.is_empty() {
            return CommandOutput::error("Update entity", "The entity name cannot be empty.");
        }
        node.name = name;
    }
    apply_transform_patch(node, transform);
    if let Some(color) = requested_color {
        node.color = color;
    }
    if let Some(visible) = visible {
        node.visible = visible;
    }
    if let Some(locked) = locked {
        node.locked = locked;
    }
    apply_semantic_metadata(node, &metadata);
    select_ids(ctx, vec![id]);
    let node = ctx.scene.get(id).expect("updated node exists");
    CommandOutput::changed(
        format!("Updated {}", node.name),
        node_detail_lines(id, node, ctx.scene),
        node_json(id, node, ctx.scene),
    )
}

fn move_entity(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let Some(id) = resolve_target(command, ctx.scene, ctx.selection) else {
        return CommandOutput::error("Move entity", "Target not found.");
    };
    if let Some(node) = ctx.scene.get_mut(id) {
        node.position += Vec3::new(
            f32_arg(command, "dx", f32_arg(command, "x", 0.0)),
            f32_arg(command, "dy", f32_arg(command, "y", 0.0)),
            f32_arg(command, "dz", f32_arg(command, "z", 0.0)),
        );
    }
    select_ids(ctx, vec![id]);
    let node = ctx.scene.get(id).expect("valid node");
    CommandOutput::changed(
        format!("Moved {}", node.name),
        node_detail_lines(id, node, ctx.scene),
        node_json(id, node, ctx.scene),
    )
}

fn rotate_entity(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let Some(id) = resolve_target(command, ctx.scene, ctx.selection) else {
        return CommandOutput::error("Rotate entity", "Target not found.");
    };
    if let Some(node) = ctx.scene.get_mut(id) {
        node.rotation += Vec3::new(
            f32_arg(command, "rx", f32_arg(command, "x", 0.0)),
            f32_arg(command, "ry", f32_arg(command, "y", 0.0)),
            f32_arg(command, "rz", f32_arg(command, "z", 0.0)),
        );
    }
    select_ids(ctx, vec![id]);
    let node = ctx.scene.get(id).expect("valid node");
    CommandOutput::changed(
        format!("Rotated {}", node.name),
        node_detail_lines(id, node, ctx.scene),
        node_json(id, node, ctx.scene),
    )
}

fn scale_entity(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let Some(id) = resolve_target(command, ctx.scene, ctx.selection) else {
        return CommandOutput::error("Scale entity", "Target not found.");
    };
    if let Some(node) = ctx.scene.get_mut(id) {
        if let Some(factor) = command.arg("factor").and_then(parse_f32) {
            node.scale *= factor;
        } else {
            node.scale.x *= f32_arg(command, "sx", f32_arg(command, "x", 1.0));
            node.scale.y *= f32_arg(command, "sy", f32_arg(command, "y", 1.0));
            node.scale.z *= f32_arg(command, "sz", f32_arg(command, "z", 1.0));
        }
    }
    select_ids(ctx, vec![id]);
    let node = ctx.scene.get(id).expect("valid node");
    CommandOutput::changed(
        format!("Scaled {}", node.name),
        node_detail_lines(id, node, ctx.scene),
        node_json(id, node, ctx.scene),
    )
}

fn color_entity(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let Some(id) = resolve_target(command, ctx.scene, ctx.selection) else {
        return CommandOutput::error("Color entity", "Target not found.");
    };
    let color = match color_argument(command, NodeColor::rgb(255, 255, 255)) {
        Ok(Some(color)) => color,
        Ok(None) => {
            return CommandOutput::error(
                "Color entity",
                "A color value is required (for example color=#44AAFF).",
            )
        }
        Err(error) => return CommandOutput::error("Color entity", error),
    };
    if let Some(node) = ctx.scene.get_mut(id) {
        node.color = color;
    }
    select_ids(ctx, vec![id]);
    let node = ctx.scene.get(id).expect("valid node");
    CommandOutput::changed(
        format!("Color updated {}", node.name),
        node_detail_lines(id, node, ctx.scene),
        node_json(id, node, ctx.scene),
    )
}

fn arrange_grid(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let spacing = f32_arg(command, "spacing", 2.0).max(0.1);
    let selected = ctx.viewport.selected_ids();
    let ids = if selected.is_empty() {
        ctx.scene.all_valid_ids()
    } else {
        selected
    };

    if ids.is_empty() {
        return CommandOutput::warning(
            "Arrange grid",
            vec!["No game entities available.".to_string()],
            serde_json::json!({"ok": false, "reason": "empty_scene"}),
        );
    }

    let columns = (ids.len() as f32).sqrt().ceil() as usize;
    for (index, id) in ids.iter().copied().enumerate() {
        if let Some(node) = ctx.scene.get_mut(id) {
            let col = index % columns;
            let row = index / columns;
            node.position.x = col as f32 * spacing;
            node.position.z = row as f32 * spacing;
        }
    }
    select_ids(ctx, ids.clone());
    CommandOutput::changed(
        "Arranged game grid",
        vec![
            format!("entities: {}", ids.len()),
            format!("spacing: {spacing:.3}"),
            format!("columns: {columns}"),
        ],
        serde_json::json!({
            "ok": true,
            "count": ids.len(),
            "spacing": spacing,
            "columns": columns
        }),
    )
}

fn generate_prefab(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let kind = command
        .arg("kind")
        .unwrap_or("platform")
        .to_ascii_lowercase();
    let manifest = match PrimitiveModelManifest::builtin(&kind) {
        Ok(Some(manifest)) => manifest,
        Ok(None) => {
            return CommandOutput::warning(
                "Generate prefab",
                vec![
                    format!("Unknown prefab kind: {kind}"),
                    format!("available: {}", builtin_primitive_model_kinds().join(", ")),
                ],
                serde_json::json!({
                    "ok": false,
                    "reason": "unknown_manifest",
                    "kind": kind,
                    "available": builtin_primitive_model_kinds()
                }),
            );
        }
        Err(error) => {
            return CommandOutput::error(
                "Generate prefab",
                format!("Could not load primitive manifest for {kind}: {error}"),
            );
        }
    };
    let group_name = command
        .arg("name")
        .map(str::to_string)
        .unwrap_or_else(|| manifest.name.clone());
    let parent = match parent_target(command, ctx.scene) {
        Ok(parent) => parent,
        Err(error) => return CommandOutput::error("Generate prefab", error),
    };
    if let Some(parent) = parent {
        if !ctx.scene.get(parent).is_some_and(|node| node.is_folder) {
            return CommandOutput::error(
                "Generate prefab",
                "parent must reference a live folder or group.",
            );
        }
    }
    let transform = match parse_transform_patch(command) {
        Ok(transform) => transform,
        Err(error) => return CommandOutput::error("Generate prefab", error),
    };
    let count = match usize_argument(command, "count", 1, 1, 64) {
        Ok(count) => count,
        Err(error) => return CommandOutput::error("Generate prefab", error),
    };
    let axis = match axis_vector(command.arg("axis").unwrap_or("x")) {
        Ok(axis) => axis,
        Err(error) => return CommandOutput::error("Generate prefab", error),
    };
    let spacing = match finite_f32_argument(command, "spacing", 2.0) {
        Ok(value) => value,
        Err(error) => return CommandOutput::error("Generate prefab", error),
    };
    let base_position = transform.position_or(Vec3::ZERO);
    let mut roots = Vec::with_capacity(count);
    let mut created_ids = Vec::new();
    for index in 0..count {
        let instance_name = if count == 1 {
            group_name.clone()
        } else {
            format!("{group_name} {}", index + 1)
        };
        let created = manifest.instantiate_into_scene_with_name(ctx.scene, Some(&instance_name));
        let Some(root) = created.first().copied() else {
            return CommandOutput::error(
                "Generate prefab",
                "Primitive manifest produced no root node.",
            );
        };
        if !ctx.scene.reparent_node(root, parent) && parent.is_some() {
            return CommandOutput::error(
                "Generate prefab",
                "Could not attach the template to the requested parent.",
            );
        }
        if let Some(node) = ctx.scene.get_mut(root) {
            node.position = base_position + axis * spacing * index as f32;
            node.rotation = transform.rotation_or(node.rotation);
            node.scale = transform.scale_or(node.scale);
        }
        roots.push(root);
        created_ids.extend(created);
    }
    let part_count = created_ids.len().saturating_sub(roots.len());
    select_ids(ctx, roots.clone());
    CommandOutput::changed(
        format!("Instantiated {group_name}"),
        vec![
            format!("kind: {kind}"),
            format!("manifest: {}", manifest.name),
            format!("schema_version: {}", manifest.schema_version),
            format!("instances: {}", roots.len()),
            format!("created_nodes: {}", created_ids.len()),
            format!("parts: {part_count}"),
            "source: embedded_json_manifest".to_string(),
        ],
        serde_json::json!({
            "ok": true,
            "kind": kind,
            "manifest": manifest.name,
            "schema_version": manifest.schema_version,
            "source": "embedded_json_manifest",
            "root_ids": roots.iter().map(|id| id.0).collect::<Vec<_>>(),
            "instances": roots.len(),
            "part_count": part_count,
            "created_ids": created_ids.iter().map(|id| id.0).collect::<Vec<_>>()
        }),
    )
}

fn describe_scene(ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let ids = ctx.scene.all_live_ids();
    let mut primitive_counts = std::collections::BTreeMap::<&'static str, usize>::new();
    for id in &ids {
        if let Some(node) = ctx.scene.get(*id) {
            *primitive_counts.entry(node.primitive.label()).or_default() += 1;
        }
    }

    // Keep the legacy command useful for humans and older clients while using
    // the same bounded, UUID-first representation as Agent/CLI/MCP. The old
    // implementation duplicated a large mesh-oriented payload and silently
    // rendered control-only names as blank strings.
    let mut data = raf_core::agent_context::scene_context(ctx.scene, &ctx.selection.selected_nodes);
    if let Some(object) = data.as_object_mut() {
        object.insert("ok".to_string(), serde_json::json!(true));
        object.insert("entities".to_string(), serde_json::json!(ids.len()));
        object.insert(
            "primitive_counts".to_string(),
            serde_json::to_value(&primitive_counts).unwrap_or(serde_json::Value::Null),
        );
        object.insert(
            "selected_id".to_string(),
            ctx.selection
                .selected_node
                .map(|id| serde_json::json!(id.0))
                .unwrap_or(serde_json::Value::Null),
        );
    }

    let mut lines = vec![
        format!("entities: {}", ids.len()),
        format!("roots: {}", ctx.scene.roots().len()),
    ];
    for (primitive, count) in &primitive_counts {
        lines.push(format!("{primitive}: {count}"));
    }
    if let Some(id) = ctx.selection.selected_node {
        if ctx.scene.is_valid_node(id) {
            if ctx.scene.get(id).is_some() {
                let name = raf_core::agent_context::display_path(ctx.scene, id);
                lines.push(format!("selected: {name} ({})", id.0));
            }
        }
    }

    CommandOutput::info("Scene description", lines, data)
}

fn focus_entity(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let Some(id) = resolve_target(command, ctx.scene, ctx.selection) else {
        return CommandOutput::error("Focus entity", "Target not found.");
    };
    select_ids(ctx, vec![id]);
    ctx.viewport.focus_entity(ctx.scene, Some(id));
    let node = ctx.scene.get(id).expect("valid node");
    CommandOutput::info(
        format!("Focused {}", node.name),
        node_detail_lines(id, node, ctx.scene),
        node_json(id, node, ctx.scene),
    )
}

fn resolve_target(
    command: &ParsedCommand,
    scene: &SceneGraph,
    selection: &SceneSelectionState,
) -> Option<SceneNodeId> {
    let target = command.arg("target").map(str::to_string).or_else(|| {
        if command.positional.is_empty() {
            None
        } else {
            Some(command.positional.join(" "))
        }
    });

    let Some(target) = target else {
        return selection
            .selected_node
            .filter(|id| scene.is_valid_node(*id));
    };
    let target = target.trim();
    if target.is_empty() {
        return None;
    }

    if target.eq_ignore_ascii_case("selected") {
        return selection
            .selected_node
            .filter(|id| scene.is_valid_node(*id));
    }
    resolve_agent_target(scene, &target).or_else(|| {
        let lower = target.to_ascii_lowercase();
        scene
            .iter()
            .find(|(id, node)| {
                presentation_name(*id, node)
                    .to_ascii_lowercase()
                    .contains(&lower)
                    || node
                        .semantic_role
                        .as_deref()
                        .unwrap_or("")
                        .to_ascii_lowercase()
                        .contains(&lower)
                    || node
                        .stable_key
                        .as_deref()
                        .unwrap_or("")
                        .to_ascii_lowercase()
                        .contains(&lower)
            })
            .map(|(id, _)| id)
    })
}

fn resolve_text_target(target: &str, scene: &SceneGraph) -> Option<SceneNodeId> {
    let target = target.trim();
    if target.is_empty() {
        return None;
    }
    resolve_agent_target(scene, target)
}

fn select_ids(ctx: &mut GameCommandContext<'_>, ids: Vec<SceneNodeId>) {
    ctx.selection.selected_node = ids.first().copied();
    ctx.selection.selected_nodes = ids.clone();
    ctx.viewport.set_selected_ids(ids);
}

fn live_scene_entity_count(scene: &SceneGraph) -> usize {
    scene.all_live_ids().len()
}

fn presentation_name(id: SceneNodeId, node: &raf_core::scene::graph::SceneNode) -> String {
    let cleaned = display_name(&node.name);
    if cleaned.is_empty() {
        format!("Entity_{}", id.0)
    } else {
        cleaned
    }
}

fn default_entity_name(scene: &SceneGraph, primitive: Primitive) -> String {
    let mut index = scene.all_live_ids().len().saturating_add(1);
    loop {
        let candidate = format!("{} {}", primitive.label(), index);
        if scene.find_node_by_name(&candidate).is_none() {
            return candidate;
        }
        index = index.saturating_add(1);
    }
}

fn primitive_arg(command: &ParsedCommand) -> Option<Primitive> {
    let raw = primitive_argument(command)?;
    match raw.to_ascii_lowercase().as_str() {
        "empty" | "group" | "folder" => Some(Primitive::Empty),
        "cube" | "box" | "block" => Some(Primitive::Cube),
        "sphere" | "ball" | "circle" | "uv_sphere" => Some(Primitive::Sphere),
        "plane" | "floor" => Some(Primitive::Plane),
        "cylinder" => Some(Primitive::Cylinder),
        // Keep command compatibility while using the orthographic 3D plane.
        "sprite" | "sprite2d" => Some(Primitive::Plane),
        _ => None,
    }
}

fn primitive_argument(command: &ParsedCommand) -> Option<&str> {
    command
        .arg("primitive")
        .or_else(|| command.arg("type"))
        .or_else(|| command.arg("kind"))
        .or_else(|| command.arg("shape"))
        .or_else(|| command.first_positional())
}

fn node_detail_lines(
    id: SceneNodeId,
    node: &raf_core::scene::graph::SceneNode,
    scene: &SceneGraph,
) -> Vec<String> {
    let bounds = primitive_bounds(node.scale);
    let mut lines = vec![
        format!("id: {}", id.0),
        format!("uuid: {}", node.uuid),
        format!("name: {}", presentation_name(id, node)),
        format!("primitive: {}", node.primitive.label()),
        format_vec3("position", node.position),
        format_vec3("rotation_deg", node.rotation),
        format_vec3("scale", node.scale),
        format_vec3("world_position", scene.world_matrix(id).col(3).truncate()),
        format!(
            "color_rgba: [{}, {}, {}, {}]",
            node.color.r, node.color.g, node.color.b, node.color.a
        ),
        format!("world_path: {}", display_path(scene, id)),
        format!(
            "parent_id: {}",
            node.parent
                .map(|parent| parent.0.to_string())
                .unwrap_or_else(|| "root".to_string())
        ),
        format!("mesh_vertices: {}", mesh_vertex_count(node.primitive)),
        format!("mesh_indices: {}", mesh_index_count(node.primitive)),
        format_vec3("bounds_min", bounds.0),
        format_vec3("bounds_max", bounds.1),
    ];
    if let Some(source_asset) = &node.source_asset {
        lines.push(format!("source_asset: {source_asset}"));
    }
    if let Some(schema_version) = node.source_schema_version {
        lines.push(format!("source_schema_version: {schema_version}"));
    }
    lines
}

fn node_json(
    id: SceneNodeId,
    node: &raf_core::scene::graph::SceneNode,
    scene: &SceneGraph,
) -> serde_json::Value {
    let (bounds_min, bounds_max) = primitive_bounds(node.scale);
    let world_position = scene.world_matrix(id).col(3).truncate();
    serde_json::json!({
        "ok": true,
        "entity": {
            "id": id.0,
            "uuid": node.uuid.to_string(),
            "ref": format!("entity:{}", node.uuid),
            "name": presentation_name(id, node),
            "path": display_path(scene, id),
            "parent_id": node.parent.map(|parent| parent.0),
            "parent_path": node.parent.map(|parent| display_path(scene, parent)),
            "children": node.children.len(),
            "child_names": node
                .children
                .iter()
                .filter_map(|child| {
                    scene
                        .get(*child)
                        .map(|node| presentation_name(*child, node))
                })
                .collect::<Vec<_>>(),
            "kind": if node.is_folder {
                "folder".to_string()
            } else {
                node.primitive.label().to_ascii_lowercase()
            },
            "is_folder": node.is_folder,
            "primitive": node.primitive.label(),
            "position": vec3_json(node.position),
            "rotation_deg": vec3_json(node.rotation),
            "scale": vec3_json(node.scale),
            "world_position": vec3_json(world_position),
            "color_rgba": [node.color.r, node.color.g, node.color.b, node.color.a],
            "visible": node.visible,
            "locked": node.locked,
            "semantic_role": node.semantic_role,
            "stable_key": node.stable_key,
            "tags": node.tags,
            "agent_origin": node.agent_origin,
            "mesh": {
                "vertex_count": mesh_vertex_count(node.primitive),
                "index_count": mesh_index_count(node.primitive),
                "bounds_min": vec3_json(bounds_min),
                "bounds_max": vec3_json(bounds_max)
            },
            "source_asset": node.source_asset,
            "source_schema_version": node.source_schema_version
        }
    })
}

fn primitive_bounds(scale: Vec3) -> (Vec3, Vec3) {
    let half = scale * 0.5;
    (-half, half)
}

fn mesh_vertex_count(primitive: Primitive) -> usize {
    match primitive {
        Primitive::Empty => 0,
        Primitive::Cube => 24,
        Primitive::Sphere => 425,
        Primitive::Plane => 4,
        Primitive::Cylinder => 68,
    }
}

fn mesh_index_count(primitive: Primitive) -> usize {
    match primitive {
        Primitive::Empty => 0,
        Primitive::Cube => 36,
        Primitive::Sphere => 2304,
        Primitive::Plane => 6,
        Primitive::Cylinder => 192,
    }
}

fn parse_f32(value: &str) -> Option<f32> {
    value.trim().parse::<f32>().ok()
}

fn f32_arg(command: &ParsedCommand, name: &str, default: f32) -> f32 {
    if let Some(value) = command.structured_arg(name) {
        return parse_json_f32(value, name).unwrap_or(default);
    }
    command.arg(name).and_then(parse_f32).unwrap_or(default)
}

fn finite_f32_argument(command: &ParsedCommand, name: &str, default: f32) -> Result<f32, String> {
    if let Some(value) = command.structured_arg(name) {
        return parse_json_f32(value, name);
    }
    command
        .arg(name)
        .map(|value| parse_finite_f32(value, name))
        .transpose()
        .map(|value| value.unwrap_or(default))
}

fn usize_argument(
    command: &ParsedCommand,
    name: &str,
    default: usize,
    minimum: usize,
    maximum: usize,
) -> Result<usize, String> {
    let value = if let Some(value) = command.structured_arg(name) {
        value
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| format!("{name} must be a positive integer."))?
    } else if let Some(value) = command.arg(name) {
        value
            .parse::<usize>()
            .map_err(|_| format!("{name} must be a positive integer."))?
    } else {
        default
    };
    if !(minimum..=maximum).contains(&value) {
        return Err(format!(
            "{name} must be between {minimum} and {maximum}; received {value}."
        ));
    }
    Ok(value)
}

fn axis_vector(axis: &str) -> Result<Vec3, String> {
    match axis.trim().to_ascii_lowercase().as_str() {
        "x" => Ok(Vec3::X),
        "y" => Ok(Vec3::Y),
        "z" => Ok(Vec3::Z),
        _ => Err("axis must be x, y, or z.".to_string()),
    }
}

fn axis_index(axis: &str) -> Result<usize, String> {
    match axis.trim().to_ascii_lowercase().as_str() {
        "x" => Ok(0),
        "y" => Ok(1),
        "z" => Ok(2),
        _ => Err("axis must be x, y, or z.".to_string()),
    }
}

fn component(value: Vec3, axis: usize) -> f32 {
    match axis {
        0 => value.x,
        1 => value.y,
        _ => value.z,
    }
}

fn set_component(value: &mut Vec3, axis: usize, component: f32) {
    match axis {
        0 => value.x = component,
        1 => value.y = component,
        _ => value.z = component,
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct TransformPatch {
    position: [Option<f32>; 3],
    rotation: [Option<f32>; 3],
    scale: [Option<f32>; 3],
}

impl TransformPatch {
    fn is_empty(self) -> bool {
        self.position.iter().all(Option::is_none)
            && self.rotation.iter().all(Option::is_none)
            && self.scale.iter().all(Option::is_none)
    }

    fn position_or(self, default: Vec3) -> Vec3 {
        optional_vec3(self.position, default)
    }

    fn rotation_or(self, default: Vec3) -> Vec3 {
        optional_vec3(self.rotation, default)
    }

    fn scale_or(self, default: Vec3) -> Vec3 {
        optional_vec3(self.scale, default)
    }
}

fn optional_vec3(values: [Option<f32>; 3], default: Vec3) -> Vec3 {
    Vec3::new(
        values[0].unwrap_or(default.x),
        values[1].unwrap_or(default.y),
        values[2].unwrap_or(default.z),
    )
}

fn apply_transform_patch(node: &mut raf_core::scene::graph::SceneNode, patch: TransformPatch) {
    if !patch.position.iter().all(Option::is_none) {
        node.position = patch.position_or(node.position);
    }
    if !patch.rotation.iter().all(Option::is_none) {
        node.rotation = patch.rotation_or(node.rotation);
    }
    if !patch.scale.iter().all(Option::is_none) {
        node.scale = patch.scale_or(node.scale);
    }
}

fn parse_transform_patch(command: &ParsedCommand) -> Result<TransformPatch, String> {
    let mut patch = TransformPatch::default();

    if let Some(value) = structured_or_json_value(command, "transform", "transform")? {
        let object = value
            .as_object()
            .ok_or_else(|| "transform must be an object.".to_string())?;
        if let Some(unknown) = object.keys().find(|key| {
            !matches!(
                key.as_str(),
                "position" | "rotation_deg" | "rotation" | "scale"
            )
        }) {
            return Err(format!("transform contains unsupported field '{unknown}'."));
        }
        if let Some(value) = object.get("position") {
            patch.position = optional_vec3_values(parse_vec3_value(value, "transform.position")?);
        }
        if let Some(value) = object
            .get("rotation_deg")
            .or_else(|| object.get("rotation"))
        {
            patch.rotation =
                optional_vec3_values(parse_vec3_value(value, "transform.rotation_deg")?);
        }
        if let Some(value) = object.get("scale") {
            patch.scale = optional_vec3_values(parse_vec3_value(value, "transform.scale")?);
        }
    }

    if let Some(value) = structured_or_json_value(command, "position", "position")? {
        patch.position = optional_vec3_values(parse_vec3_value(&value, "position")?);
    }
    let rotation_value = match structured_or_json_value(command, "rotation_deg", "rotation_deg")? {
        Some(value) => Some(value),
        None => structured_or_json_value(command, "rotation", "rotation_deg")?,
    };
    if let Some(value) = rotation_value {
        patch.rotation = optional_vec3_values(parse_vec3_value(&value, "rotation_deg")?);
    }
    if let Some(value) = structured_or_json_value(command, "scale", "scale")? {
        patch.scale = optional_vec3_values(parse_vec3_value(&value, "scale")?);
    }

    parse_scalar_component(command, "x", "position.x", &mut patch.position[0])?;
    parse_scalar_component(command, "y", "position.y", &mut patch.position[1])?;
    parse_scalar_component(command, "z", "position.z", &mut patch.position[2])?;
    parse_scalar_component(command, "rx", "rotation.x", &mut patch.rotation[0])?;
    parse_scalar_component(command, "ry", "rotation.y", &mut patch.rotation[1])?;
    parse_scalar_component(command, "rz", "rotation.z", &mut patch.rotation[2])?;
    parse_scalar_component(command, "sx", "scale.x", &mut patch.scale[0])?;
    parse_scalar_component(command, "sy", "scale.y", &mut patch.scale[1])?;
    parse_scalar_component(command, "sz", "scale.z", &mut patch.scale[2])?;

    Ok(patch)
}

fn parse_scalar_component(
    command: &ParsedCommand,
    key: &str,
    label: &str,
    destination: &mut Option<f32>,
) -> Result<(), String> {
    if let Some(value) = command.structured_arg(key) {
        *destination = Some(parse_json_f32(value, label)?);
    } else if let Some(raw) = command.arg(key) {
        *destination = Some(parse_finite_f32(raw, label)?);
    }
    Ok(())
}

fn structured_or_json_value(
    command: &ParsedCommand,
    name: &str,
    label: &str,
) -> Result<Option<Value>, String> {
    if let Some(value) = command.structured_arg(name) {
        return Ok((!value.is_null()).then_some(value.clone()));
    }
    command
        .arg(name)
        .map(|raw| parse_json_argument(raw, label))
        .transpose()
}

fn parse_json_vec3_argument(raw: &str, label: &str) -> Result<[f32; 3], String> {
    parse_vec3_value(&parse_json_argument(raw, label)?, label)
}

fn parse_json_argument(raw: &str, label: &str) -> Result<Value, String> {
    serde_json::from_str(raw).map_err(|error| format!("{label} must be valid JSON: {error}."))
}

fn parse_vec3_value(value: &Value, label: &str) -> Result<[f32; 3], String> {
    match value {
        Value::Array(values) if values.len() == 3 => Ok([
            parse_json_f32(&values[0], &format!("{label}[0]"))?,
            parse_json_f32(&values[1], &format!("{label}[1]"))?,
            parse_json_f32(&values[2], &format!("{label}[2]"))?,
        ]),
        Value::Array(values) if values.len() == 1 && values[0].is_array() => Err(format!(
            "{label} expected [x, y, z], received a nested array [[x, y, z]]. Remove the extra array layer and retry."
        )),
        Value::Array(values) => Err(format!(
            "{label} expected [x, y, z] with exactly 3 finite numbers; received an array with {} values.",
            values.len()
        )),
        Value::Object(object) => {
            if let Some(values) = object
                .get("values")
                .or_else(|| object.get("items"))
                .or_else(|| object.get("value"))
            {
                return parse_vec3_value(values, label);
            }
            let missing = ["x", "y", "z"]
                .iter()
                .find(|key| !object.contains_key(**key))
                .copied();
            if let Some(missing) = missing {
                return Err(format!(
                    "{label} must contain x, y, and z; missing {label}.{missing}."
                ));
            }
            Ok([
                parse_json_object_f32(object, "x", &format!("{label}.x"))?,
                parse_json_object_f32(object, "y", &format!("{label}.y"))?,
                parse_json_object_f32(object, "z", &format!("{label}.z"))?,
            ])
        }
        Value::String(raw) => parse_json_vec3_argument(raw, label),
        _ => Err(format!(
            "{label} expected [x, y, z] or {{x, y, z}} with finite numbers; received {}.",
            json_shape(value)
        )),
    }
}

fn json_shape(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

fn optional_vec3_values(values: [f32; 3]) -> [Option<f32>; 3] {
    [Some(values[0]), Some(values[1]), Some(values[2])]
}

fn parse_json_f32(value: &Value, label: &str) -> Result<f32, String> {
    match value {
        Value::Number(number) => number
            .as_f64()
            .map(|value| value as f32)
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("{label} must be a finite number.")),
        Value::String(raw) => parse_finite_f32(raw, label),
        _ => Err(format!("{label} must be a number.")),
    }
}

fn parse_json_object_f32(
    object: &serde_json::Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<f32, String> {
    object
        .get(key)
        .ok_or_else(|| format!("{label} is required."))
        .and_then(|value| parse_json_f32(value, label))
}

fn parse_finite_f32(raw: &str, label: &str) -> Result<f32, String> {
    let value = raw
        .trim()
        .parse::<f32>()
        .map_err(|_| format!("{label} must be a number."))?;
    value
        .is_finite()
        .then_some(value)
        .ok_or_else(|| format!("{label} must be finite."))
}

fn parse_bool_literal(raw: &str, label: &str) -> Result<bool, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(format!("{label} must be true or false.")),
    }
}

fn color_argument(
    command: &ParsedCommand,
    default: NodeColor,
) -> Result<Option<NodeColor>, String> {
    if let Some(value) = command
        .structured_arg("color")
        .or_else(|| command.structured_arg("color_rgba"))
    {
        return parse_color_value(value, "color").map(Some);
    }
    if let Some(raw) = command.arg("color").or_else(|| command.arg("color_rgba")) {
        return parse_color(raw).map(Some);
    }

    let has_channels = ["r", "g", "b", "a"]
        .iter()
        .any(|key| command.arg(key).is_some());
    if !has_channels {
        return Ok(None);
    }

    Ok(Some(NodeColor::rgba(
        parse_u8_component(command, "r", default.r)?,
        parse_u8_component(command, "g", default.g)?,
        parse_u8_component(command, "b", default.b)?,
        parse_u8_component(command, "a", default.a)?,
    )))
}

fn parse_u8_component(command: &ParsedCommand, key: &str, default: u8) -> Result<u8, String> {
    command
        .arg(key)
        .map(|raw| parse_u8_literal(raw, key))
        .unwrap_or(Ok(default))
}

fn parse_color(raw: &str) -> Result<NodeColor, String> {
    let raw = raw.trim();
    if let Some(hex) = raw.strip_prefix('#') {
        if hex.len() != 6 && hex.len() != 8 {
            return Err("color hex must use #RRGGBB or #RRGGBBAA.".to_string());
        }
        let r = u8::from_str_radix(&hex[0..2], 16)
            .map_err(|_| "color contains an invalid red channel.".to_string())?;
        let g = u8::from_str_radix(&hex[2..4], 16)
            .map_err(|_| "color contains an invalid green channel.".to_string())?;
        let b = u8::from_str_radix(&hex[4..6], 16)
            .map_err(|_| "color contains an invalid blue channel.".to_string())?;
        let a = if hex.len() == 8 {
            u8::from_str_radix(&hex[6..8], 16)
                .map_err(|_| "color contains an invalid alpha channel.".to_string())?
        } else {
            255
        };
        return Ok(NodeColor::rgba(r, g, b, a));
    }

    if raw.starts_with('[') || raw.starts_with('{') || raw.starts_with('"') {
        let value = parse_json_argument(raw, "color")?;
        return parse_color_value(&value, "color");
    }

    let parts = raw.split(',').map(str::trim).collect::<Vec<_>>();
    if parts.len() != 3 && parts.len() != 4 {
        return Err(
            "color must be #RRGGBB, #RRGGBBAA, or 3/4 comma-separated channels.".to_string(),
        );
    }
    let r = parse_u8_literal(parts[0], "color.r")?;
    let g = parse_u8_literal(parts[1], "color.g")?;
    let b = parse_u8_literal(parts[2], "color.b")?;
    let a = if parts.len() == 4 {
        parse_u8_literal(parts[3], "color.a")?
    } else {
        255
    };
    Ok(NodeColor::rgba(r, g, b, a))
}

fn parse_color_value(value: &Value, label: &str) -> Result<NodeColor, String> {
    match value {
        Value::Array(values) if values.len() == 3 || values.len() == 4 => {
            let r = parse_json_u8(&values[0], &format!("{label}[0]"))?;
            let g = parse_json_u8(&values[1], &format!("{label}[1]"))?;
            let b = parse_json_u8(&values[2], &format!("{label}[2]"))?;
            let a = values
                .get(3)
                .map(|value| parse_json_u8(value, &format!("{label}[3]")))
                .transpose()?
                .unwrap_or(255);
            Ok(NodeColor::rgba(r, g, b, a))
        }
        Value::Array(_) => Err(format!("{label} must contain 3 or 4 channels.")),
        Value::Object(object) => {
            if let Some(values) = object
                .get("values")
                .or_else(|| object.get("channels"))
                .or_else(|| object.get("items"))
            {
                return parse_color_value(values, label);
            }
            Ok(NodeColor::rgba(
                parse_json_object_u8(object, "r", &format!("{label}.r"))?,
                parse_json_object_u8(object, "g", &format!("{label}.g"))?,
                parse_json_object_u8(object, "b", &format!("{label}.b"))?,
                object
                    .get("a")
                    .map(|value| parse_json_u8(value, &format!("{label}.a")))
                    .transpose()?
                    .unwrap_or(255),
            ))
        }
        Value::String(raw) => parse_color(raw),
        _ => Err(format!(
            "{label} must be a color array, object, or literal."
        )),
    }
}

fn parse_json_u8(value: &Value, label: &str) -> Result<u8, String> {
    match value {
        Value::Number(number) => number
            .as_u64()
            .and_then(|value| u8::try_from(value).ok())
            .ok_or_else(|| format!("{label} must be an integer from 0 to 255.")),
        Value::String(raw) => parse_u8_literal(raw, label),
        _ => Err(format!("{label} must be an integer from 0 to 255.")),
    }
}

fn parse_json_object_u8(
    object: &serde_json::Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<u8, String> {
    object
        .get(key)
        .ok_or_else(|| format!("{label} is required."))
        .and_then(|value| parse_json_u8(value, label))
}

fn parse_u8_literal(raw: &str, label: &str) -> Result<u8, String> {
    raw.trim()
        .parse::<u8>()
        .map_err(|_| format!("{label} must be an integer from 0 to 255."))
}

fn format_vec3(label: &str, value: Vec3) -> String {
    format!("{label}: [{:.3}, {:.3}, {:.3}]", value.x, value.y, value.z)
}

fn vec3_json(value: Vec3) -> serde_json::Value {
    serde_json::json!([value.x, value.y, value.z])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::output::CommandLevel;
    use crate::commands::parser::{parse_console_input, ParsedInput};

    #[test]
    fn add_primitive_uses_builtin_asset_manifest() {
        let mut scene = SceneGraph::new();
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let ParsedInput::Command(command) =
            parse_console_input("/game.add primitive=cube name=Block x=1 y=2 z=3").unwrap()
        else {
            panic!("expected command");
        };
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };

        let output = execute("game.add", &command, &mut ctx);

        assert_eq!(output.level, CommandLevel::Info);
        assert!(output.changed);
        let selected = ctx.selection.selected_node.expect("selected node");
        let node = ctx.scene.get(selected).expect("created node");
        assert_eq!(node.name, "Block");
        assert_eq!(node.primitive, Primitive::Cube);
        assert_eq!(node.position, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(
            node.source_asset.as_deref(),
            Some("builtin://primitive/cube")
        );
        assert_eq!(
            output.json["entity"]["source_asset"],
            "builtin://primitive/cube"
        );
    }

    #[test]
    fn semantic_add_preserves_primitive_transform_and_color() {
        let mut scene = SceneGraph::new();
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let command = parsed_command(
            "game.add",
            serde_json::json!({
                "kind": "sphere",
                "name": "Round_Product",
                "transform": {
                    "position": [2.5, 1.25, -3.0],
                    "rotation_deg": [0.0, 45.0, 10.0],
                    "scale": [0.5, 2.0, 1.5]
                },
                "color_rgba": [12, 34, 56, 200]
            }),
        );
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };

        let output = execute("game.add", &command, &mut ctx);
        let id = ctx.selection.selected_node.expect("created entity");
        let node = ctx.scene.get(id).expect("created node");

        assert_eq!(output.level, CommandLevel::Info);
        assert_eq!(node.primitive, Primitive::Sphere);
        assert_eq!(node.position, Vec3::new(2.5, 1.25, -3.0));
        assert_eq!(node.rotation, Vec3::new(0.0, 45.0, 10.0));
        assert_eq!(node.scale, Vec3::new(0.5, 2.0, 1.5));
        assert_eq!(node.color, NodeColor::rgba(12, 34, 56, 200));
        assert_eq!(
            output.json["entity"]["color_rgba"],
            serde_json::json!([12, 34, 56, 200])
        );
    }

    #[test]
    fn structured_update_accepts_object_vectors_and_color() {
        let mut scene = SceneGraph::new();
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let add = parsed_command(
            "game.add",
            serde_json::json!({"primitive": "cube", "name": "Shelf"}),
        );
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };
        execute("game.add", &add, &mut ctx);
        let id = ctx.selection.selected_node.expect("created entity");
        let uuid = ctx.scene.get(id).expect("created node").uuid;

        let update = parsed_command(
            "game.update",
            serde_json::json!({
                "target": uuid.to_string(),
                "transform": {
                    "position": {"x": -1.0, "y": 2.0, "z": 3.0},
                    "scale": {"x": 2.0, "y": 0.5, "z": 4.0}
                },
                "color_rgba": [220, 180, 40]
            }),
        );
        let output = execute("game.update", &update, &mut ctx);
        let node = ctx.scene.get(id).expect("updated node");

        assert_eq!(output.level, CommandLevel::Info);
        assert_eq!(node.position, Vec3::new(-1.0, 2.0, 3.0));
        assert_eq!(node.scale, Vec3::new(2.0, 0.5, 4.0));
        assert_eq!(node.color, NodeColor::rgb(220, 180, 40));
    }

    #[test]
    fn semantic_batch_keeps_different_shapes_separate() {
        let mut scene = SceneGraph::new();
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let command = parsed_command(
            "game.batch",
            serde_json::json!({
                "operations": [
                    {"name": "scene_create", "params": {"kind": "cube", "name": "Body", "transform": {"position": [0, 0, 0], "scale": [4, 1, 2]}, "color_rgba": [90, 90, 90, 255]}},
                    {"name": "scene_create", "params": {"kind": "sphere", "name": "Display", "transform": {"position": [0, 1, 0], "scale": [1, 1, 1]}, "color_rgba": [30, 140, 220, 255]}},
                    {"name": "scene_create", "params": {"kind": "cylinder", "name": "Post", "transform": {"position": [1, 2, 0], "scale": [0.2, 2, 0.2]}, "color_rgba": [200, 50, 30, 255]}}
                ]
            }),
        );
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };

        let output = execute("game.batch", &command, &mut ctx);

        assert_eq!(output.level, CommandLevel::Info);
        assert_eq!(ctx.scene.len(), 3);
        let nodes = ctx.scene.iter().map(|(_, node)| node).collect::<Vec<_>>();
        assert_eq!(nodes[0].primitive, Primitive::Cube);
        assert_eq!(nodes[1].primitive, Primitive::Sphere);
        assert_eq!(nodes[2].primitive, Primitive::Cylinder);
        assert_eq!(nodes[0].scale, Vec3::new(4.0, 1.0, 2.0));
        assert_eq!(nodes[1].position, Vec3::new(0.0, 1.0, 0.0));
        assert_eq!(nodes[2].color, NodeColor::rgb(200, 50, 30));
    }

    #[test]
    fn structured_build_creates_modular_groups_and_effective_values() {
        let mut scene = SceneGraph::new();
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let command = parsed_command(
            "game.build",
            serde_json::json!({
                "groups": [
                    {"name": "Structure"},
                    {"name": "Products", "parent": "Structure"}
                ],
                "entities": [
                    {
                        "kind": "cube",
                        "name": "Floor",
                        "parent": "Structure",
                        "transform": {
                            "position": [0.0, 0.5, -1.0],
                            "scale": [4.0, 0.2, 2.0]
                        },
                        "color_rgba": [60, 150, 90, 255]
                    }
                ]
            }),
        );
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };

        let output = execute("game.build", &command, &mut ctx);

        assert_eq!(output.level, CommandLevel::Info);
        assert_eq!(output.json["build"]["groups"], 2);
        assert_eq!(output.json["build"]["entities"], 1);
        let structure = ctx.scene.find_node_by_name("Structure").expect("structure");
        let products = ctx.scene.find_node_by_name("Products").expect("products");
        let floor = ctx.scene.find_node_by_name("Floor").expect("floor");
        assert!(ctx.scene.get(structure).unwrap().is_folder);
        assert_eq!(ctx.scene.get(products).unwrap().parent, Some(structure));
        assert_eq!(ctx.scene.get(floor).unwrap().parent, Some(structure));
        assert_eq!(
            ctx.scene.get(floor).unwrap().position,
            Vec3::new(0.0, 0.5, -1.0)
        );
        assert_eq!(
            ctx.scene.get(floor).unwrap().scale,
            Vec3::new(4.0, 0.2, 2.0)
        );
        assert_eq!(
            ctx.scene.get(floor).unwrap().color,
            NodeColor::rgba(60, 150, 90, 255)
        );
    }

    #[test]
    fn structured_build_orders_child_groups_after_their_parent() {
        let mut scene = SceneGraph::new();
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let command = parsed_command(
            "game.build",
            serde_json::json!({
                "groups": [
                    {"name": "Products", "parent": "Structure"},
                    {"name": "Structure"}
                ],
                "entities": []
            }),
        );
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };

        let output = execute("game.build", &command, &mut ctx);

        assert_eq!(output.level, CommandLevel::Info);
        let structure = ctx.scene.find_node_by_name("Structure").expect("structure");
        let products = ctx.scene.find_node_by_name("Products").expect("products");
        assert_eq!(ctx.scene.get(products).unwrap().parent, Some(structure));
    }

    #[test]
    fn real_world_design_profile_rejects_incomplete_structure_before_mutation() {
        let mut scene = SceneGraph::new();
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let command = parsed_command(
            "game.build",
            serde_json::json!({
                "design_profile": "supermarket",
                "groups": [{"name": "Store"}],
                "entities": [
                    {"kind": "cube", "name": "Floor", "parent": "Store"},
                    {"kind": "cube", "name": "Entrance", "parent": "Store"},
                    {"kind": "cube", "name": "Aisle_Main", "parent": "Store"},
                    {"kind": "cube", "name": "Shelf_Main", "parent": "Store"}
                ]
            }),
        );
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };

        let output = execute("game.build", &command, &mut ctx);

        assert_eq!(output.level, CommandLevel::Error);
        assert!(output.lines.join(" ").contains("enclosure"));
        assert_eq!(ctx.scene.len(), 0);
    }

    #[test]
    fn repair_is_explicit_scoped_and_atomic() {
        let mut scene = SceneGraph::new();
        let store = scene.add_root_folder("Store");
        let shelf = scene.add_child_with_primitive(store, "Shelf", Primitive::Cube);
        let outside = scene.add_root_folder("Outside");
        scene.get_mut(shelf).unwrap().position = Vec3::ZERO;
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let successful = parsed_command(
            "game.repair",
            serde_json::json!({
                "root": "Store",
                "operations": [{
                    "name": "scene_update",
                    "params": {
                        "target": "Shelf",
                        "transform": {"position": [2.0, 0.0, 0.0]}
                    }
                }]
            }),
        );
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };

        let output = execute("game.repair", &successful, &mut ctx);

        assert_eq!(output.level, CommandLevel::Info);
        assert_eq!(ctx.scene.get(shelf).unwrap().position.x, 2.0);

        let outside_repair = parsed_command(
            "game.repair",
            serde_json::json!({
                "root": "Store",
                "operations": [{
                    "name": "scene_update",
                    "params": {
                        "target": "Outside",
                        "transform": {"position": [9.0, 0.0, 0.0]}
                    }
                }]
            }),
        );
        let output = execute("game.repair", &outside_repair, &mut ctx);

        assert_eq!(output.level, CommandLevel::Error);
        assert_eq!(ctx.scene.get(outside).unwrap().position.x, 0.0);

        let atomic_failure = parsed_command(
            "game.repair",
            serde_json::json!({
                "operations": [
                    {
                        "name": "scene_update",
                        "params": {
                            "target": "Shelf",
                            "transform": {"position": [4.0, 0.0, 0.0]}
                        }
                    },
                    {
                        "name": "scene_reparent",
                        "params": {"target": "Shelf", "parent": "Missing"}
                    }
                ]
            }),
        );
        let output = execute("game.repair", &atomic_failure, &mut ctx);

        assert_eq!(output.level, CommandLevel::Error);
        assert_eq!(ctx.scene.get(shelf).unwrap().position.x, 2.0);
    }

    #[test]
    fn reconcile_reuses_stable_keys_instead_of_duplicating_scene_nodes() {
        let mut scene = SceneGraph::new();
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let first = parsed_command(
            "game.reconcile",
            serde_json::json!({
                "groups": [{"name": "Store", "stable_key": "store"}],
                "entities": [{
                    "kind": "cube",
                    "name": "Shelf_Left",
                    "stable_key": "store.shelf.left",
                    "parent": "key:store",
                    "transform": {"position": [0.0, 1.0, 0.0]}
                }]
            }),
        );
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };

        let output = execute("game.reconcile", &first, &mut ctx);
        assert_eq!(output.level, CommandLevel::Info);
        assert_eq!(ctx.scene.len(), 2);
        let store = ctx
            .scene
            .iter()
            .find(|(_, node)| node.stable_key.as_deref() == Some("store"))
            .map(|(id, _)| id)
            .expect("store group");
        let shelf = ctx
            .scene
            .iter()
            .find(|(_, node)| node.stable_key.as_deref() == Some("store.shelf.left"))
            .map(|(id, _)| id)
            .expect("shelf entity");

        let second = parsed_command(
            "game.reconcile",
            serde_json::json!({
                "groups": [{"name": "Storefront", "stable_key": "store"}],
                "entities": [{
                    "kind": "cube",
                    "name": "Shelf_Left_Wide",
                    "stable_key": "store.shelf.left",
                    "parent": "key:store",
                    "transform": {"position": [2.0, 1.0, 0.0]}
                }]
            }),
        );
        let output = execute("game.reconcile", &second, &mut ctx);
        assert_eq!(output.level, CommandLevel::Info);
        assert_eq!(ctx.scene.len(), 2);
        assert_eq!(
            ctx.scene
                .iter()
                .find(|(_, node)| node.stable_key.as_deref() == Some("store"))
                .map(|(id, _)| id),
            Some(store)
        );
        let updated_shelf = ctx
            .scene
            .iter()
            .find(|(_, node)| node.stable_key.as_deref() == Some("store.shelf.left"))
            .map(|(id, node)| (id, node));
        assert_eq!(updated_shelf.map(|(id, _)| id), Some(shelf));
        assert_eq!(updated_shelf.unwrap().1.name, "Shelf_Left_Wide");
        assert_eq!(updated_shelf.unwrap().1.position.x, 2.0);
    }

    #[test]
    fn reconcile_profile_uses_existing_envelope_for_partial_deltas() {
        let mut scene = SceneGraph::new();
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let initial = parsed_command(
            "game.build",
            serde_json::json!({
                "design_profile": "supermarket",
                "groups": [{"name": "Store", "stable_key": "store"}],
                "entities": [
                    {"kind": "cube", "name": "Floor", "stable_key": "store.floor"},
                    {"kind": "cube", "name": "Wall_North", "stable_key": "store.wall.north"},
                    {"kind": "cube", "name": "Wall_South", "stable_key": "store.wall.south"},
                    {"kind": "cube", "name": "Entrance", "stable_key": "store.entrance"},
                    {"kind": "cube", "name": "Aisle_Main", "stable_key": "store.aisle.main"},
                    {"kind": "cube", "name": "Shelf_Main", "stable_key": "store.shelf.main"}
                ]
            }),
        );
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };

        let output = execute("game.build", &initial, &mut ctx);
        assert_eq!(output.level, CommandLevel::Info);
        let before_count = ctx.scene.len();
        let shelf = ctx
            .scene
            .iter()
            .find(|(_, node)| node.stable_key.as_deref() == Some("store.shelf.main"))
            .map(|(id, _)| id)
            .expect("shelf entity");

        let partial = parsed_command(
            "game.reconcile",
            serde_json::json!({
                "design_profile": "supermarket",
                "entities": [{
                    "kind": "cube",
                    "name": "Shelf_Main_Wide",
                    "stable_key": "store.shelf.main",
                    "transform": {"scale": [2.0, 1.0, 1.0]}
                }]
            }),
        );
        let output = execute("game.reconcile", &partial, &mut ctx);

        assert_eq!(output.level, CommandLevel::Info);
        assert_eq!(ctx.scene.len(), before_count);
        assert_eq!(
            ctx.scene
                .iter()
                .find(|(_, node)| node.stable_key.as_deref() == Some("store.shelf.main"))
                .map(|(id, node)| (id, node.name.as_str())),
            Some((shelf, "Shelf_Main_Wide"))
        );
    }

    #[test]
    fn reparent_command_preserves_world_transform_by_default() {
        let mut scene = SceneGraph::new();
        let source = scene.add_root_folder("Source");
        let target = scene.add_root_folder("Target");
        let child = scene.add_child_with_primitive(source, "Shelf", Primitive::Cube);
        scene.get_mut(source).unwrap().position = Vec3::new(4.0, 1.0, 0.0);
        scene.get_mut(target).unwrap().position = Vec3::new(-2.0, 0.0, 3.0);
        scene.get_mut(child).unwrap().position = Vec3::new(1.0, 2.0, 3.0);
        let before = scene.world_matrix(child).to_cols_array();
        let command = parsed_command(
            "game.reparent",
            serde_json::json!({"target": "Shelf", "parent": "Target"}),
        );
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };

        let output = execute("game.reparent", &command, &mut ctx);

        assert_eq!(output.level, CommandLevel::Info);
        assert_eq!(ctx.scene.get(child).unwrap().parent, Some(target));
        let after = ctx.scene.world_matrix(child).to_cols_array();
        for (before, after) in before.iter().zip(after) {
            assert!((*before - after).abs() < 0.001);
        }
    }

    #[test]
    fn duplicate_supports_parent_count_and_spacing() {
        let mut scene = SceneGraph::new();
        let source_parent = scene.add_root_folder("Source");
        let target_parent = scene.add_root_folder("Products");
        scene.add_child_with_primitive(source_parent, "Can", Primitive::Cylinder);
        let command = parsed_command(
            "game.duplicate",
            serde_json::json!({
                "target": "Can",
                "parent": "Products",
                "name": "Can Copy",
                "offset": [0.0, 0.0, 0.0],
                "count": 3,
                "axis": "x",
                "spacing": 0.5,
                "preserve_world": false
            }),
        );
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };

        let output = execute("game.duplicate", &command, &mut ctx);

        assert_eq!(output.level, CommandLevel::Info);
        assert_eq!(output.json["count"], 3);
        assert_eq!(ctx.selection.selected_nodes.len(), 3);
        for (index, id) in ctx.selection.selected_nodes.iter().enumerate() {
            let node = ctx.scene.get(*id).unwrap();
            assert_eq!(node.parent, Some(target_parent));
            assert_eq!(node.position.x, index as f32 * 0.5);
        }
    }

    #[test]
    fn snap_places_entity_on_top_of_surface_bounds() {
        let mut scene = SceneGraph::new();
        let shelf = scene.add_root_with_primitive("Shelf", Primitive::Cube);
        let product = scene.add_root_with_primitive("Product", Primitive::Cube);
        scene.get_mut(shelf).unwrap().scale = Vec3::new(4.0, 0.2, 2.0);
        scene.get_mut(product).unwrap().scale = Vec3::new(0.5, 1.0, 0.5);
        let command = parsed_command(
            "game.snap",
            serde_json::json!({
                "target": "Product",
                "mode": "surface",
                "snap_to": "Shelf",
                "axis": "y",
                "placement": "after",
                "gap": 0.05
            }),
        );
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };

        let output = execute("game.snap", &command, &mut ctx);

        assert_eq!(output.level, CommandLevel::Info);
        let shelf_max = world_bounds(ctx.scene, shelf).unwrap().1.y;
        let product_min = world_bounds(ctx.scene, product).unwrap().0.y;
        assert!((product_min - shelf_max - 0.05).abs() < 0.001);
    }

    #[test]
    fn malformed_structured_transform_does_not_create_default_entity() {
        let mut scene = SceneGraph::new();
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let command = parsed_command(
            "game.add",
            serde_json::json!({
                "kind": "cube",
                "name": "Invalid",
                "transform": {"position": [1, 2]}
            }),
        );
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };

        let output = execute("game.add", &command, &mut ctx);

        assert_eq!(output.level, CommandLevel::Error);
        assert_eq!(ctx.scene.len(), 0);
        assert_eq!(output.json["error"]["code"], "invalid_vector_shape");
    }

    #[test]
    fn batch_is_atomic_when_a_later_operation_fails() {
        let mut scene = SceneGraph::new();
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let command = ParsedCommand {
            raw: "/game.batch".to_string(),
            name: "game.batch".to_string(),
            args: std::collections::BTreeMap::from([(
                "operations".to_string(),
                serde_json::to_string(&serde_json::json!([
                    {"name": "game.add", "params": {"primitive": "cube", "name": "Shelf"}},
                    {"name": "game.delete", "params": {"target": "does-not-exist"}}
                ]))
                .unwrap(),
            )]),
            positional: Vec::new(),
            structured_args: None,
        };
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };

        let output = execute("game.batch", &command, &mut ctx);

        assert_eq!(output.level, CommandLevel::Error);
        assert_eq!(ctx.scene.len(), 0);
    }

    #[test]
    fn semantic_parent_and_update_preserve_stable_entity_identity() {
        let mut scene = SceneGraph::new();
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let parent = scene.add_root_folder("Store");
        let add = ParsedCommand {
            raw: "/game.add".to_string(),
            name: "game.add".to_string(),
            args: std::collections::BTreeMap::from([
                ("primitive".to_string(), "cube".to_string()),
                ("name".to_string(), "Shelf".to_string()),
                ("parent".to_string(), parent.0.to_string()),
            ]),
            positional: Vec::new(),
            structured_args: None,
        };
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };
        execute("game.add", &add, &mut ctx);
        let id = ctx.selection.selected_node.expect("created node");
        let uuid = ctx.scene.get(id).unwrap().uuid;
        let update = ParsedCommand {
            raw: "/game.update".to_string(),
            name: "game.update".to_string(),
            args: std::collections::BTreeMap::from([
                ("target".to_string(), uuid.to_string()),
                ("name".to_string(), "Shelf_Left".to_string()),
                ("x".to_string(), "2".to_string()),
            ]),
            positional: Vec::new(),
            structured_args: None,
        };
        execute("game.update", &update, &mut ctx);

        assert_eq!(
            ctx.scene.node_path(id).as_deref(),
            Some("/Store/Shelf_Left")
        );
        assert_eq!(ctx.scene.get(id).unwrap().uuid, uuid);
        assert_eq!(ctx.scene.get(id).unwrap().position.x, 2.0);
    }

    fn parsed_command(name: &str, params: Value) -> ParsedCommand {
        ParsedCommand {
            raw: format!("/{name}"),
            name: name.to_string(),
            args: params
                .as_object()
                .expect("test params object")
                .iter()
                .map(|(key, value)| (key.clone(), json_command_arg(value.clone())))
                .collect(),
            positional: Vec::new(),
            structured_args: Some(params),
        }
    }
}
