//! Transactional world plan execution and commit.

use super::super::build::{build_scene, order_build_groups, reconcile_scene};
use super::super::entity::update_entity;
use super::super::query::{parsed_command_from_params, text_argument};
use super::plan::{load_planned_world, plan_cache, update_persisted_plan, PlannedWorld};
use super::*;

/// Plans are applied through the same validated semantic commands as a manual
/// build, but the plan may contain thousands of entities. The previous commit
/// path forwarded the whole plan to `game.build`, whose 32-group and
/// 128-entity contract is correct for one interactive build but wrong for a
/// compiled world. Plan chunks keep every individual command inside that
/// contract while the plan itself is one command, one history entry and one
/// undo token at the adapter boundary.
const PLAN_GROUP_CHUNK: usize = 32;
const PLAN_ENTITY_CHUNK: usize = 128;

fn plan_chunk_command(plan: &PlannedWorld, key: &str, chunk: &[Value]) -> ParsedCommand {
    let mut params = serde_json::json!({ key: chunk });
    if let Some(parent) = &plan.parent {
        params["parent"] = parent.clone();
    }
    parsed_command_from_params(
        if plan.mode == "reconcile" {
            "game.reconcile"
        } else {
            "game.build"
        },
        params,
    )
}

fn run_plan_chunk(
    plan: &PlannedWorld,
    scene: &mut SceneGraph,
    selection: &mut SceneSelectionState,
    viewport: &mut dyn GameViewportPort,
    key: &str,
    chunk: &[Value],
) -> Result<CommandOutput, String> {
    let command = plan_chunk_command(plan, key, chunk);
    let mut context = GameCommandContext {
        scene,
        selection,
        viewport,
    };
    let output = if plan.mode == "reconcile" {
        reconcile_scene(&command, &mut context)
    } else {
        build_scene(&command, &mut context)
    };
    if matches!(output.level, crate::commands::output::CommandLevel::Error) {
        return Err(output.lines.join(" "));
    }
    Ok(output)
}

/// Apply a compiled plan in bounded chunks and roll the whole document back if
/// any chunk fails. The caller owns the single transaction/undo scope; this
/// function only guarantees all-or-nothing scene state inside that scope.
fn resolve_plan_target(scene: &SceneGraph, reference: &str) -> Option<SceneNodeId> {
    if let Some(key) = reference.strip_prefix("key:") {
        return scene
            .iter()
            .find(|(_, node)| {
                node.stable_key
                    .as_deref()
                    .is_some_and(|stable| stable.eq_ignore_ascii_case(key))
            })
            .map(|(id, _)| id);
    }
    if let Ok(id) = crate::commands::behavior::resolve(scene, reference) {
        return Some(id);
    }
    scene
        .iter()
        .find(|(_, node)| {
            node.stable_key
                .as_deref()
                .is_some_and(|stable| stable.eq_ignore_ascii_case(reference))
        })
        .map(|(id, _)| id)
}

pub(crate) fn apply_plan_to_scene(
    plan: &PlannedWorld,
    scene: &mut SceneGraph,
    selection: &mut SceneSelectionState,
    viewport: &mut dyn GameViewportPort,
) -> Result<CommandOutput, String> {
    if plan.groups.is_empty()
        && plan.entities.is_empty()
        && plan.updates.is_empty()
        && plan.deletions.is_empty()
    {
        return Err(
            "The world plan contains no groups, entities, updates or deletions.".to_string(),
        );
    }
    let original_scene = scene.clone();
    let original_selection = selection.clone();
    let original_viewport_selection = viewport.selected_ids();
    let ordered_groups = match order_build_groups(plan.groups.clone(), scene, plan.parent.as_ref())
    {
        Ok(groups) => groups,
        Err(error) => return Err(error),
    };
    let mut chunks: Vec<(&'static str, Vec<Value>)> = Vec::new();
    for chunk in ordered_groups.chunks(PLAN_GROUP_CHUNK) {
        chunks.push(("groups", chunk.to_vec()));
    }
    for chunk in plan.entities.chunks(PLAN_ENTITY_CHUNK) {
        chunks.push(("entities", chunk.to_vec()));
    }
    let mut outputs = Vec::with_capacity(chunks.len());
    for (key, chunk) in &chunks {
        match run_plan_chunk(plan, scene, selection, viewport, key, chunk) {
            Ok(output) => outputs.push(output),
            Err(error) => {
                *scene = original_scene;
                *selection = original_selection;
                viewport.set_selected_ids(original_viewport_selection);
                return Err(error);
            }
        }
    }
    let mut changed = false;
    let mut updated = 0usize;
    let mut deleted = 0usize;
    let post_result = (|| -> Result<(), String> {
        let mut context = GameCommandContext {
            scene,
            selection,
            viewport,
        };
        for update in &plan.updates {
            let target = update
                .get("target")
                .and_then(Value::as_str)
                .ok_or_else(|| "Plan update requires target.".to_string())?;
            let Some(id) = resolve_plan_target(context.scene, target) else {
                return Err(format!("Plan update target '{target}' was not found."));
            };
            let uuid = context
                .scene
                .get(id)
                .ok_or_else(|| format!("Plan update target '{target}' disappeared."))?
                .uuid;
            let mut params = update.clone();
            if let Some(object) = params.as_object_mut() {
                object.insert("target".to_string(), Value::String(uuid.to_string()));
            }
            let command = parsed_command_from_params("game.update", params);
            let output = update_entity(&command, &mut context);
            if matches!(output.level, crate::commands::output::CommandLevel::Error) {
                return Err(format!(
                    "Plan update '{target}' failed: {}",
                    output.lines.join(" ")
                ));
            }
            updated += 1;
        }
        for deletion in &plan.deletions {
            let target = deletion
                .get("target")
                .and_then(Value::as_str)
                .ok_or_else(|| "Plan deletion requires target.".to_string())?;
            let optional = deletion
                .get("optional")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            match resolve_plan_target(context.scene, target) {
                Some(id) => {
                    if !context.scene.remove_node(id) {
                        return Err(format!(
                            "Plan deletion target '{target}' could not be removed."
                        ));
                    }
                    deleted += 1;
                }
                None if optional => {}
                None => return Err(format!("Plan deletion target '{target}' was not found.")),
            }
        }
        Ok(())
    })();
    if let Err(error) = post_result {
        *scene = original_scene;
        *selection = original_selection;
        viewport.set_selected_ids(original_viewport_selection);
        return Err(error);
    }
    let mut created = 0usize;
    let mut created_ids = Vec::new();
    let mut lines = Vec::new();
    for output in &outputs {
        changed |= output.changed;
        if let Some(ids) = output.json.get("created_ids").and_then(Value::as_array) {
            created += ids.len();
            created_ids.extend(ids.iter().take(64).cloned());
        }
        if let Some(reconcile) = output.json.get("reconcile") {
            created += reconcile
                .get("created")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;
            updated += reconcile
                .get("updated")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;
        }
        lines.extend(output.lines.iter().take(2).cloned());
    }
    created_ids.truncate(64);
    let mut result = CommandOutput::info(
        "World plan applied",
        vec![
            format!("mode: {}", plan.mode),
            format!("chunks: {}", chunks.len()),
            format!("groups: {}", plan.groups.len()),
            format!("entities: {}", plan.entities.len()),
            format!("updates: {}", plan.updates.len()),
            format!("deletions: {}", plan.deletions.len()),
            format!("created: {created}"),
            format!("updated: {updated}"),
            format!("deleted: {deleted}"),
        ],
        serde_json::json!({
            "ok": true,
            "atomic": true,
            "mode": plan.mode,
            "chunks": chunks.len(),
            "group_chunk_size": PLAN_GROUP_CHUNK,
            "entity_chunk_size": PLAN_ENTITY_CHUNK,
            "groups": plan.groups.len(),
            "entities": plan.entities.len(),
            "updates": plan.updates.len(),
            "deletions": plan.deletions.len(),
            "created": created,
            "updated": updated,
            "deleted": deleted,
            "created_ids": created_ids,
            "chunk_lines": lines,
            "undo_scope": "plan",
        }),
    );
    result.changed = changed || updated > 0 || deleted > 0;
    Ok(result)
}

pub(crate) fn execute_commit(
    request: &ParsedCommand,
    ctx: &mut GameCommandContext<'_>,
) -> CommandOutput {
    let result = (|| -> Result<CommandOutput, String> {
        let plan_id = text_argument(request, "plan_id")
            .or_else(|| text_argument(request, "id"))
            .ok_or_else(|| "game.commit requires plan_id.".to_string())?;
        if !plan_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
        {
            return Err("plan_id contains unsupported characters.".to_string());
        }
        let mut plan = load_planned_world(&plan_id)?;
        if plan.status == "cancelled" {
            return Err(format!(
                "Plan '{plan_id}' was cancelled. Compile a fresh plan before committing."
            ));
        }
        if let (Some(plan_root), Some(current_root)) =
            (plan.project_root.as_deref(), active_project_root())
        {
            let current_root = current_root.to_string_lossy().replace('\\', "/");
            if plan_root != current_root {
                return Err(format!(
                    "Plan '{plan_id}' belongs to a different project. Compile a new plan in the active project."
                ));
            }
        }
        if plan.status == "committed" {
            return Ok(CommandOutput::info(
                "World plan already committed",
                vec![format!("plan: {plan_id}")],
                serde_json::json!({
                    "ok": true,
                    "plan_id": plan_id,
                    "plan_hash": plan.hash,
                    "replayed": true,
                    "changed": false,
                }),
            ));
        }
        let current = raf_core::agent_context::scene_fingerprint(ctx.scene);
        if plan.fingerprint != current {
            return Err(format!(
                "The scene changed after plan '{plan_id}' was created. Create a fresh plan against revision evidence before committing."
            ));
        }
        let mut output = apply_plan_to_scene(&plan, ctx.scene, ctx.selection, ctx.viewport)?;
        plan.status = "committed".to_string();
        if let Ok(mut cache) = plan_cache().lock() {
            cache.insert(plan.id.clone(), plan.clone());
        }
        update_persisted_plan(&plan);
        output.title = "World plan committed".to_string();
        output.lines.insert(0, format!("plan: {plan_id}"));
        if let Some(object) = output.json.as_object_mut() {
            object.insert("plan_id".to_string(), Value::String(plan_id.clone()));
            object.insert(
                "world_plan".to_string(),
                serde_json::json!({
                    "id": plan.id,
                    "hash": plan.hash,
                    "status": plan.status,
                    "mode": plan.mode,
                    "groups": plan.groups.len(),
                    "entities": plan.entities.len(),
                    "persisted_path": plan.persisted_path,
                    "undo_scope": "plan",
                }),
            );
        }
        Ok(output)
    })();
    result.unwrap_or_else(|error| CommandOutput::error("World commit", error))
}
