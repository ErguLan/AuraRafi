//! Semantic scene build, reconcile and repair commands.

use super::entity::primitive_arg;
use super::query::*;
use super::*;

/// Reconcile a semantic desired state without duplicating entities that carry
/// the same stable key. Existing nodes are updated in place; missing nodes are
/// created; unmentioned nodes are intentionally preserved unless a future
/// explicit cleanup operation is requested.
pub(super) fn reconcile_scene(
    command: &ParsedCommand,
    ctx: &mut GameCommandContext<'_>,
) -> CommandOutput {
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
pub(super) fn repair_scene(
    command: &ParsedCommand,
    ctx: &mut GameCommandContext<'_>,
) -> CommandOutput {
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

pub(super) fn build_scene(
    command: &ParsedCommand,
    ctx: &mut GameCommandContext<'_>,
) -> CommandOutput {
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
pub(super) fn order_build_groups(
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
