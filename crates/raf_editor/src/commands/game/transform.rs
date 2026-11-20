//! Transform, bounds, size, snap and parent commands.

use super::query::*;
use super::*;

pub(super) fn reparent_entity(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
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

pub(super) fn snap_entity(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
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

pub(super) fn set_transform(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
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

pub(super) fn move_entity(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
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

pub(super) fn rotate_entity(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
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

pub(super) fn scale_entity(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
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

pub(super) fn arrange_grid(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
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

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct TransformPatch {
    position: [Option<f32>; 3],
    rotation: [Option<f32>; 3],
    scale: [Option<f32>; 3],
}

impl TransformPatch {
    pub(super) fn is_empty(self) -> bool {
        self.position.iter().all(Option::is_none)
            && self.rotation.iter().all(Option::is_none)
            && self.scale.iter().all(Option::is_none)
    }

    pub(super) fn position_or(self, default: Vec3) -> Vec3 {
        optional_vec3(self.position, default)
    }

    pub(super) fn rotation_or(self, default: Vec3) -> Vec3 {
        optional_vec3(self.rotation, default)
    }

    pub(super) fn scale_or(self, default: Vec3) -> Vec3 {
        optional_vec3(self.scale, default)
    }
}

pub(super) fn optional_vec3(values: [Option<f32>; 3], default: Vec3) -> Vec3 {
    Vec3::new(
        values[0].unwrap_or(default.x),
        values[1].unwrap_or(default.y),
        values[2].unwrap_or(default.z),
    )
}

pub(super) fn apply_transform_patch(node: &mut raf_core::scene::graph::SceneNode, patch: TransformPatch) {
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

pub(super) fn dimensions_patch(
    command: &ParsedCommand,
    primitive: Primitive,
    previous: Vec3,
    mut patch: TransformPatch,
) -> Result<TransformPatch, String> {
    if let Some(value) = structured_or_json_value(command, "size", "size")? {
        if patch.scale.iter().any(Option::is_some) {
            return Err("commands.error.dimensions_scale".into());
        }
        let size = Vec3::from_array(parse_vec3_value(&value, "size")?);
        patch.scale = optional_vec3_values(
            primitive
                .scale_for_dimensions(size, previous)
                .map_err(str::to_string)?
                .to_array(),
        );
    }
    Ok(patch)
}

pub(super) fn parse_transform_patch(command: &ParsedCommand) -> Result<TransformPatch, String> {
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

pub(super) fn parse_scalar_component(
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
