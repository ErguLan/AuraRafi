//! Entity creation, hierarchy, metadata and component commands.

use super::material::*;
use super::query::*;
use super::transform::*;
use super::*;

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

pub(super) fn create_group(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
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

/// Semantic component editing shared by `game.add`, `game.update`, semantic
/// batches and the World Program compiler. The Inspector already exposes these
/// properties; the Agent must be able to author the same object, not only its
/// primitive and color.
fn apply_scene_components(
    command: &ParsedCommand,
    node: &mut SceneNode,
    label: &str,
) -> Result<(), String> {
    if let Some(texture) =
        component_text(command, "texture").or_else(|| component_text(command, "base_color_texture"))
    {
        node.base_color_texture = validate_component_texture(&texture, label)?;
    }

    if let Some(value) =
        structured_or_json_value(command, "collider", &format!("{label}.collider"))?
    {
        apply_collider_component(&value, node, label)?;
    }
    if let Some(mut value) =
        structured_or_json_value(command, "rigid_body", &format!("{label}.rigid_body"))?
    {
        if let Some(object) = value.as_object_mut() {
            let body_type = object
                .get("body_type")
                .and_then(Value::as_str)
                .map(str::to_string);
            if let Some(body_type) = body_type {
                let normalized = match body_type.trim().to_ascii_lowercase().as_str() {
                    "static" => "Static",
                    "dynamic" => "Dynamic",
                    "kinematic" => "Kinematic",
                    _ => body_type.as_str(),
                };
                object.insert(
                    "body_type".to_string(),
                    Value::String(normalized.to_string()),
                );
            }
        }
        let mut body: RigidBody = serde_json::from_value(value)
            .map_err(|error| format!("{label}.rigid_body is invalid: {error}"))?;
        body.mass = finite_component_f32(
            body.mass,
            &format!("{label}.rigid_body.mass"),
            0.0001,
            1_000_000.0,
        )?;
        body.damping = finite_component_f32(
            body.damping,
            &format!("{label}.rigid_body.damping"),
            0.0,
            100.0,
        )?;
        body.velocity =
            finite_component_vec3(body.velocity, &format!("{label}.rigid_body.velocity"))?;
        node.rigid_body = body;
    }
    if let Some(value) = structured_or_json_value(command, "audio", &format!("{label}.audio"))? {
        let mut audio: AudioSource = serde_json::from_value(value)
            .map_err(|error| format!("{label}.audio is invalid: {error}"))?;
        audio.volume =
            finite_component_f32(audio.volume, &format!("{label}.audio.volume"), 0.0, 1.0)?;
        if !audio.clip.trim().is_empty() {
            audio.clip =
                validate_component_asset_path(&audio.clip, label, &["mp3", "ogg", "wav", "flac"])?;
        }
        node.audio_source = audio;
    }
    if let Some(value) = structured_or_json_value(command, "camera", &format!("{label}.camera"))? {
        let camera: GameCamera = serde_json::from_value(value)
            .map_err(|error| format!("{label}.camera is invalid: {error}"))?;
        camera
            .validate()
            .map_err(|error| format!("{label}.camera is invalid: {error}"))?;
        node.game_camera = Some(camera);
    }
    if let Some(value) = structured_or_json_value(command, "scripts", &format!("{label}.scripts"))?
    {
        node.scripts = parse_component_scripts(&value, label)?;
    }
    if let Some(value) =
        structured_or_json_value(command, "variables", &format!("{label}.variables"))?
    {
        let variables: Vec<raf_core::scene::SceneVariable> = serde_json::from_value(value)
            .map_err(|error| format!("{label}.variables is invalid: {error}"))?;
        if variables.len() > 64 {
            return Err(format!("{label}.variables accepts at most 64 entries."));
        }
        node.variables = variables;
    }
    Ok(())
}

fn component_text(command: &ParsedCommand, name: &str) -> Option<String> {
    text_argument(command, name).filter(|value| !value.trim().is_empty())
}

fn apply_collider_component(
    value: &Value,
    node: &mut SceneNode,
    label: &str,
) -> Result<(), String> {
    let (kind, offset, visible_in_editor) = match value {
        Value::String(kind) => (kind.as_str(), Vec3::ZERO, true),
        Value::Object(object) => {
            let kind = object
                .get("type")
                .or_else(|| object.get("collider_type"))
                .and_then(Value::as_str)
                .unwrap_or("aabb");
            let offset = object
                .get("offset")
                .map(|value| parse_vec3_value(value, &format!("{label}.collider.offset")))
                .transpose()?
                .unwrap_or([0.0, 0.0, 0.0]);
            let visible = object
                .get("visible_in_editor")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            (kind, Vec3::from_array(offset), visible)
        }
        Value::Null => return Ok(()),
        other => {
            return Err(format!(
                "{label}.collider must be a type string or an object, received {}.",
                json_kind(other)
            ))
        }
    };
    let collider_type = match kind.trim().to_ascii_lowercase().as_str() {
        "none" => ColliderType::None,
        "aabb" => ColliderType::Aabb,
        "convex_hull" | "convexhull" | "convex hull" => ColliderType::ConvexHull,
        "mesh" | "mesh_collider" | "mesh collider" => ColliderType::MeshCollider,
        other => {
            return Err(format!(
                "{label}.collider.type is invalid ('{other}'). Use none, aabb, convex_hull or mesh."
            ))
        }
    };
    let size = node.primitive.dimensions(Vec3::ONE).abs() * 0.5;
    let aabb = Aabb {
        min: -size,
        max: size,
    };
    let convex_hull = (collider_type == ColliderType::ConvexHull).then(|| ConvexHull {
        points: aabb.corners().to_vec(),
    });
    node.collider = raf_core::scene::Collider {
        collider_type,
        aabb,
        convex_hull,
        mesh_collider: None,
        visible_in_editor,
        offset,
        ..raf_core::scene::Collider::default()
    };
    Ok(())
}

fn parse_component_scripts(value: &Value, label: &str) -> Result<Vec<String>, String> {
    let values = value.as_array().ok_or_else(|| {
        format!("{label}.scripts must be an array of project-relative .rhai paths.")
    })?;
    if values.len() > 32 {
        return Err(format!("{label}.scripts accepts at most 32 entries."));
    }
    values
        .iter()
        .map(|value| {
            let raw = value
                .as_str()
                .ok_or_else(|| format!("{label}.scripts entries must be strings."))?;
            let path = validate_component_asset_path(raw, label, &["rhai"])?;
            Ok(if path.starts_with("assets/") {
                path
            } else {
                format!("assets/{path}")
            })
        })
        .collect()
}

fn finite_component_f32(value: f32, label: &str, min: f32, max: f32) -> Result<f32, String> {
    if !value.is_finite() || value < min || value > max {
        return Err(format!(
            "{label} must be finite and between {min} and {max}."
        ));
    }
    Ok(value)
}

fn finite_component_vec3(value: Vec3, label: &str) -> Result<Vec3, String> {
    if !value.is_finite() {
        return Err(format!("{label} requires finite X/Y/Z values."));
    }
    Ok(value)
}

pub(super) fn add_entity(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let primitive = match primitive_arg(command) {
        Some(primitive) => primitive,
        None if primitive_argument(command).is_none() => Primitive::Cube,
        None => {
            return CommandOutput::error(
                "Create entity",
                "Unsupported shape. Use empty, cube, sphere, plane, cylinder, or capsule.",
            )
        }
    };
    let transform = match parse_transform_patch(command)
        .and_then(|patch| dimensions_patch(command, primitive, Vec3::ONE, patch))
    {
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
        if let Err(error) = apply_scene_components(command, node, "game.add") {
            ctx.scene.remove_node(id);
            return CommandOutput::error("Create entity", error);
        }
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
        Primitive::Capsule => Some("builtin://primitive/capsule"),
        Primitive::Cylinder => Some("builtin://primitive/cylinder"),
        Primitive::Plane => Some("builtin://primitive/plane"),
        Primitive::Empty => None,
    }
}

pub(super) fn select_entity(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
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

pub(super) fn rename_entity(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
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

pub(super) fn delete_entity(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
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

pub(super) fn duplicate_entity(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
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

pub(super) fn update_entity(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let Some(id) = resolve_target(command, ctx.scene, ctx.selection) else {
        return CommandOutput::error("Update entity", "Target not found.");
    };
    let Some(current_node) = ctx.scene.get(id) else {
        return CommandOutput::error("Update entity", "Target not found.");
    };
    let transform = match parse_transform_patch(command).and_then(|patch| {
        dimensions_patch(command, current_node.primitive, current_node.scale, patch)
    }) {
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
    let shadow_flags = ["casts_shadows", "receives_shadows"].map(|key| {
        command
            .arg(key)
            .map(|value| parse_bool_literal(value, key))
            .transpose()
    });
    let [casts_shadows, receives_shadows] = match shadow_flags {
        [Ok(casts), Ok(receives)] => [casts, receives],
        [Err(error), _] | [_, Err(error)] => return CommandOutput::error("Update entity", error),
    };
    let metadata = match parse_semantic_metadata(command) {
        Ok(metadata) => metadata,
        Err(error) => return CommandOutput::error("Update entity", error),
    };
    if let Err(error) = validate_semantic_metadata(ctx.scene, &metadata, Some(id)) {
        return CommandOutput::error("Update entity", error);
    }
    // Validate and stage components before touching the live node so a bad
    // collider/camera path cannot leave a half-updated entity behind.
    let mut staged_components = current_node.clone();
    if let Err(error) = apply_scene_components(command, &mut staged_components, "game.update") {
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
    if let Some(value) = casts_shadows {
        node.casts_shadows = value;
    }
    if let Some(value) = receives_shadows {
        node.receives_shadows = value;
    }
    apply_semantic_metadata(node, &metadata);
    node.base_color_texture = staged_components.base_color_texture;
    node.collider = staged_components.collider;
    node.rigid_body = staged_components.rigid_body;
    node.audio_source = staged_components.audio_source;
    node.game_camera = staged_components.game_camera;
    node.scripts = staged_components.scripts;
    node.variables = staged_components.variables;
    select_ids(ctx, vec![id]);
    let node = ctx.scene.get(id).expect("updated node exists");
    CommandOutput::changed(
        format!("Updated {}", node.name),
        node_detail_lines(id, node, ctx.scene),
        node_json(id, node, ctx.scene),
    )
}

pub(super) fn focus_entity(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
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

pub(super) fn primitive_arg(command: &ParsedCommand) -> Option<Primitive> {
    let raw = primitive_argument(command)?;
    Primitive::from_slug(&raw)
}

fn primitive_argument(command: &ParsedCommand) -> Option<&str> {
    command
        .arg("primitive")
        .or_else(|| command.arg("type"))
        .or_else(|| command.arg("kind"))
        .or_else(|| command.arg("shape"))
        .or_else(|| command.first_positional())
}
