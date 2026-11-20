//! Scene queries, entity reports and shared command helpers.

use super::*;

pub(super) fn json_command_arg(value: serde_json::Value) -> String {
    match value {
        serde_json::Value::String(value) => value,
        serde_json::Value::Null => String::new(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => value.to_string(),
    }
}

pub(super) fn structured_or_json_array(
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

pub(crate) fn text_argument(command: &ParsedCommand, name: &str) -> Option<String> {
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

pub(super) fn bool_argument(command: &ParsedCommand, name: &str, default: bool) -> Result<bool, String> {
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

pub(super) fn parent_target(
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

pub(super) fn parsed_command_from_params(name: &str, params: Value) -> ParsedCommand {
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

pub(super) fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

pub(super) fn describe_scene(ctx: &mut GameCommandContext<'_>) -> CommandOutput {
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

pub(super) fn resolve_target(
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

pub(super) fn resolve_text_target(target: &str, scene: &SceneGraph) -> Option<SceneNodeId> {
    let target = target.trim();
    if target.is_empty() {
        return None;
    }
    resolve_agent_target(scene, target)
}

pub(super) fn select_ids(ctx: &mut GameCommandContext<'_>, ids: Vec<SceneNodeId>) {
    ctx.selection.selected_node = ids.first().copied();
    ctx.selection.selected_nodes = ids.clone();
    ctx.viewport.set_selected_ids(ids);
}

pub(super) fn live_scene_entity_count(scene: &SceneGraph) -> usize {
    scene.all_live_ids().len()
}

pub(super) fn presentation_name(id: SceneNodeId, node: &raf_core::scene::graph::SceneNode) -> String {
    let cleaned = display_name(&node.name);
    if cleaned.is_empty() {
        format!("Entity_{}", id.0)
    } else {
        cleaned
    }
}

pub(super) fn node_detail_lines(
    id: SceneNodeId,
    node: &raf_core::scene::graph::SceneNode,
    scene: &SceneGraph,
) -> Vec<String> {
    let bounds = primitive_bounds(node.primitive);
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

pub(super) fn node_json(
    id: SceneNodeId,
    node: &raf_core::scene::graph::SceneNode,
    scene: &SceneGraph,
) -> serde_json::Value {
    let (bounds_min, bounds_max) = primitive_bounds(node.primitive);
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
            "game_camera": node.game_camera,
            "position": vec3_json(node.position),
            "rotation_deg": vec3_json(node.rotation),
            "scale": vec3_json(node.scale),
            "size": vec3_json(node.primitive.dimensions(node.scale)),
            "world_position": vec3_json(world_position),
            "color_rgba": [node.color.r, node.color.g, node.color.b, node.color.a],
            "visible": node.visible,
            "casts_shadows": node.casts_shadows,
            "receives_shadows": node.receives_shadows,
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

pub(super) fn primitive_bounds(primitive: Primitive) -> (Vec3, Vec3) {
    let half = primitive.descriptor().half_extents;
    (-half, half)
}

pub(super) fn mesh_vertex_count(primitive: Primitive) -> usize {
    match primitive {
        Primitive::Empty => 0,
        Primitive::Cube => 24,
        Primitive::Sphere => 425,
        Primitive::Plane => 4,
        Primitive::Capsule => 170,
        Primitive::Cylinder => 68,
    }
}

pub(super) fn mesh_index_count(primitive: Primitive) -> usize {
    match primitive {
        Primitive::Empty => 0,
        Primitive::Cube => 36,
        Primitive::Sphere => 2304,
        Primitive::Plane => 6,
        Primitive::Capsule => 768,
        Primitive::Cylinder => 192,
    }
}

pub(super) fn parse_f32(value: &str) -> Option<f32> {
    value.trim().parse::<f32>().ok()
}

pub(super) fn f32_arg(command: &ParsedCommand, name: &str, default: f32) -> f32 {
    if let Some(value) = command.structured_arg(name) {
        return parse_json_f32(value, name).unwrap_or(default);
    }
    command.arg(name).and_then(parse_f32).unwrap_or(default)
}

pub(super) fn finite_f32_argument(command: &ParsedCommand, name: &str, default: f32) -> Result<f32, String> {
    if let Some(value) = command.structured_arg(name) {
        return parse_json_f32(value, name);
    }
    command
        .arg(name)
        .map(|value| parse_finite_f32(value, name))
        .transpose()
        .map(|value| value.unwrap_or(default))
}

pub(super) fn usize_argument(
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

pub(super) fn axis_vector(axis: &str) -> Result<Vec3, String> {
    match axis.trim().to_ascii_lowercase().as_str() {
        "x" => Ok(Vec3::X),
        "y" => Ok(Vec3::Y),
        "z" => Ok(Vec3::Z),
        _ => Err("axis must be x, y, or z.".to_string()),
    }
}

pub(super) fn axis_index(axis: &str) -> Result<usize, String> {
    match axis.trim().to_ascii_lowercase().as_str() {
        "x" => Ok(0),
        "y" => Ok(1),
        "z" => Ok(2),
        _ => Err("axis must be x, y, or z.".to_string()),
    }
}

pub(super) fn component(value: Vec3, axis: usize) -> f32 {
    match axis {
        0 => value.x,
        1 => value.y,
        _ => value.z,
    }
}

pub(super) fn set_component(value: &mut Vec3, axis: usize, component: f32) {
    match axis {
        0 => value.x = component,
        1 => value.y = component,
        _ => value.z = component,
    }
}

pub(super) fn structured_or_json_value(
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

pub(super) fn parse_json_vec3_argument(raw: &str, label: &str) -> Result<[f32; 3], String> {
    parse_vec3_value(&parse_json_argument(raw, label)?, label)
}

pub(super) fn parse_json_argument(raw: &str, label: &str) -> Result<Value, String> {
    serde_json::from_str(raw).map_err(|error| format!("{label} must be valid JSON: {error}."))
}

pub(super) fn parse_vec3_value(value: &Value, label: &str) -> Result<[f32; 3], String> {
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

pub(super) fn json_shape(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

pub(super) fn optional_vec3_values(values: [f32; 3]) -> [Option<f32>; 3] {
    [Some(values[0]), Some(values[1]), Some(values[2])]
}

pub(super) fn parse_json_f32(value: &Value, label: &str) -> Result<f32, String> {
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

pub(super) fn parse_json_object_f32(
    object: &serde_json::Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<f32, String> {
    object
        .get(key)
        .ok_or_else(|| format!("{label} is required."))
        .and_then(|value| parse_json_f32(value, label))
}

pub(super) fn parse_finite_f32(raw: &str, label: &str) -> Result<f32, String> {
    let value = raw
        .trim()
        .parse::<f32>()
        .map_err(|_| format!("{label} must be a number."))?;
    value
        .is_finite()
        .then_some(value)
        .ok_or_else(|| format!("{label} must be finite."))
}

pub(super) fn parse_bool_literal(raw: &str, label: &str) -> Result<bool, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(format!("{label} must be true or false.")),
    }
}

pub(super) fn format_vec3(label: &str, value: Vec3) -> String {
    format!("{label}: [{:.3}, {:.3}, {:.3}]", value.x, value.y, value.z)
}

pub(super) fn vec3_json(value: Vec3) -> serde_json::Value {
    serde_json::json!([value.x, value.y, value.z])
}
