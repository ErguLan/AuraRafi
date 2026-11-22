use glam::Vec3;
use raf_assets::{builtin_primitive_model_kinds, PrimitiveModelManifest};
use raf_core::agent_context::{
    display_name, display_path, resolve_target as resolve_agent_target, world_bounds,
};
use raf_core::runtime_config::GameCamera;
use raf_core::scene::collider::{Aabb, ColliderType, ConvexHull};
use raf_core::scene::graph::{NodeColor, Primitive, SceneGraph, SceneNode, SceneNodeId};
use raf_core::scene::runtime::{AudioSource, RigidBody};
use serde_json::{Map, Value};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;

use crate::commands::output::CommandOutput;
use crate::commands::parser::ParsedCommand;

mod build;
pub(crate) mod animation;
mod camera;
mod entity;
mod material;
mod prefab;
mod query;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
mod transform;

mod visual;
mod world;

pub use world::{bind_project_root, clear_project_root, set_project_root, ProjectRootGuard};
use build::{build_scene, reconcile_scene, repair_scene};
use camera::camera_entity;
use entity::{
    add_entity, create_group, delete_entity, duplicate_entity, focus_entity, rename_entity,
    select_entity, update_entity,
};
use material::color_entity;
use prefab::generate_prefab;
use query::{
    describe_scene, json_command_arg, live_scene_entity_count, node_json,
    parsed_command_from_params, structured_or_json_array, text_argument,
};
use transform::{
    arrange_grid, move_entity, reparent_entity, rotate_entity, scale_entity, set_transform,
    snap_entity,
};
use world::{
    execute_cancel, execute_commit, execute_plan, execute_world_brief, execute_world_recipe,
    execute_world_script,
};

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
    "game.animation.sample",
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
    "game.camera",
    "game.snap",
    "game.build",
    "game.reconcile",
    "game.repair",
    "game.plan",
    "game.commit",
    "world.brief",
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
    if command_name.starts_with("runtime.") {
        return CommandOutput::error("Runtime", "Runtime controls require manual native UI or local-console interaction; attached Agent/CLI/MCP activation is disabled.");
    }
    match command_name {
        "game.add" => add_entity(command, ctx),
        "game.select" => select_entity(command, ctx),
        "game.rename" => rename_entity(command, ctx),
        "game.delete" => delete_entity(command, ctx),
        "game.duplicate" => duplicate_entity(command, ctx),
        "game.set_transform" => set_transform(command, ctx),
        "game.update" => update_entity(command, ctx),
        "game.animation.sample" => animation::sample(command, ctx),
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
        "game.camera" => camera_entity(command, ctx),
        "game.snap" | "scene.snap" => snap_entity(command, ctx),
        "game.build" | "scene.build" => build_scene(command, ctx),
        "game.reconcile" | "scene.reconcile" => reconcile_scene(command, ctx),
        "game.repair" | "scene.repair" => repair_scene(command, ctx),
        "game.plan" | "scene.plan" => execute_plan(command, ctx),
        "game.commit" | "scene.commit" => execute_commit(command, ctx),
        "game.cancel" | "scene.cancel" => execute_cancel(command),
        "world.brief" | "world.brief.read" | "world.brief.write" => execute_world_brief(command),
        "world.recipe" | "scene.recipe" => execute_world_recipe(command, ctx),
        "world.script" | "scene.script" => execute_world_script(command, ctx),
        "scene.visual_diff" | "game.visual_diff" => visual::execute_visual_diff(command),
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
