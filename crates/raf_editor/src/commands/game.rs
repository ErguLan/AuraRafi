use glam::Vec3;
use raf_assets::{builtin_primitive_model_kinds, PrimitiveModelManifest};
use raf_core::scene::graph::{NodeColor, Primitive, SceneGraph, SceneNodeId};

use crate::commands::output::CommandOutput;
use crate::commands::parser::ParsedCommand;
use crate::panels::hierarchy::HierarchyPanel;
use crate::panels::viewport::ViewportPanel;

pub struct GameCommandContext<'a> {
    pub scene: &'a mut SceneGraph,
    pub hierarchy: &'a mut HierarchyPanel,
    pub viewport: &'a mut ViewportPanel,
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
        "game.move" => move_entity(command, ctx),
        "game.rotate" => rotate_entity(command, ctx),
        "game.scale" => scale_entity(command, ctx),
        "game.color" => color_entity(command, ctx),
        "game.arrange_grid" => arrange_grid(command, ctx),
        "game.generate_prefab" => generate_prefab(command, ctx),
        "game.describe_scene" => describe_scene(ctx),
        "game.focus" => focus_entity(command, ctx),
        _ => CommandOutput::error(
            "Game command",
            format!("Unknown game command: {command_name}"),
        ),
    }
}

fn add_entity(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let primitive = primitive_arg(command).unwrap_or(Primitive::Cube);
    let name = command
        .arg("name")
        .map(str::to_string)
        .unwrap_or_else(|| format!("{} {}", primitive.label(), ctx.scene.len() + 1));
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
        node.position = Vec3::new(
            f32_arg(command, "x", 0.0),
            f32_arg(command, "y", 0.0),
            f32_arg(command, "z", 0.0),
        );
        node.rotation = Vec3::new(
            f32_arg(command, "rx", 0.0),
            f32_arg(command, "ry", 0.0),
            f32_arg(command, "rz", 0.0),
        );
        node.scale = Vec3::new(
            f32_arg(command, "sx", 1.0),
            f32_arg(command, "sy", 1.0),
            f32_arg(command, "sz", 1.0),
        );
        if let Some(color) = command.arg("color").and_then(parse_color) {
            node.color = color;
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
    let Some(id) = resolve_target(command, ctx.scene, ctx.hierarchy) else {
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
    let Some(id) = resolve_target(command, ctx.scene, ctx.hierarchy) else {
        return CommandOutput::error("Rename entity", "Target not found.");
    };
    let Some(name) = command.arg("name") else {
        return CommandOutput::error("Rename entity", "Missing name=<new name>.");
    };
    if let Some(node) = ctx.scene.get_mut(id) {
        node.name = name.to_string();
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
    let Some(id) = resolve_target(command, ctx.scene, ctx.hierarchy) else {
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
    let Some(id) = resolve_target(command, ctx.scene, ctx.hierarchy) else {
        return CommandOutput::error("Duplicate entity", "Target not found.");
    };
    let Some(new_id) = ctx.scene.duplicate_node(id) else {
        return CommandOutput::error("Duplicate entity", "Could not duplicate target.");
    };
    select_ids(ctx, vec![new_id]);
    let node = ctx.scene.get(new_id).expect("duplicate exists");
    CommandOutput::changed(
        format!("Duplicated {}", node.name),
        node_detail_lines(new_id, node, ctx.scene),
        node_json(new_id, node, ctx.scene),
    )
}

fn set_transform(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let Some(id) = resolve_target(command, ctx.scene, ctx.hierarchy) else {
        return CommandOutput::error("Set transform", "Target not found.");
    };
    if let Some(node) = ctx.scene.get_mut(id) {
        if let Some(x) = command.arg("x").and_then(parse_f32) {
            node.position.x = x;
        }
        if let Some(y) = command.arg("y").and_then(parse_f32) {
            node.position.y = y;
        }
        if let Some(z) = command.arg("z").and_then(parse_f32) {
            node.position.z = z;
        }
        if let Some(rx) = command.arg("rx").and_then(parse_f32) {
            node.rotation.x = rx;
        }
        if let Some(ry) = command.arg("ry").and_then(parse_f32) {
            node.rotation.y = ry;
        }
        if let Some(rz) = command.arg("rz").and_then(parse_f32) {
            node.rotation.z = rz;
        }
        if let Some(sx) = command.arg("sx").and_then(parse_f32) {
            node.scale.x = sx;
        }
        if let Some(sy) = command.arg("sy").and_then(parse_f32) {
            node.scale.y = sy;
        }
        if let Some(sz) = command.arg("sz").and_then(parse_f32) {
            node.scale.z = sz;
        }
    }
    select_ids(ctx, vec![id]);
    let node = ctx.scene.get(id).expect("valid node");
    CommandOutput::changed(
        format!("Transform updated {}", node.name),
        node_detail_lines(id, node, ctx.scene),
        node_json(id, node, ctx.scene),
    )
}

fn move_entity(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let Some(id) = resolve_target(command, ctx.scene, ctx.hierarchy) else {
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
    let Some(id) = resolve_target(command, ctx.scene, ctx.hierarchy) else {
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
    let Some(id) = resolve_target(command, ctx.scene, ctx.hierarchy) else {
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
    let Some(id) = resolve_target(command, ctx.scene, ctx.hierarchy) else {
        return CommandOutput::error("Color entity", "Target not found.");
    };
    let color = if let Some(color) = command.arg("color").and_then(parse_color) {
        color
    } else {
        NodeColor::rgba(
            u8_arg(command, "r", 255),
            u8_arg(command, "g", 255),
            u8_arg(command, "b", 255),
            u8_arg(command, "a", 255),
        )
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
    let ids = if ctx.viewport.selected.is_empty() {
        ctx.scene.all_valid_ids()
    } else {
        ctx.viewport.selected.clone()
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
    let created = manifest.instantiate_into_scene_with_name(ctx.scene, Some(&group_name));
    let Some(root) = created.first().copied() else {
        return CommandOutput::error(
            "Generate prefab",
            "Primitive manifest produced no root node.",
        );
    };
    let part_count = created.len().saturating_sub(1);

    select_ids(ctx, vec![root]);
    CommandOutput::changed(
        format!("Imported {group_name}"),
        vec![
            format!("kind: {kind}"),
            format!("manifest: {}", manifest.name),
            format!("schema_version: {}", manifest.schema_version),
            format!("root_id: {}", root.0),
            format!("created_nodes: {}", created.len()),
            format!("parts: {part_count}"),
            "source: embedded_json_manifest".to_string(),
        ],
        serde_json::json!({
            "ok": true,
            "kind": kind,
            "manifest": manifest.name,
            "schema_version": manifest.schema_version,
            "source": "embedded_json_manifest",
            "root_id": root.0,
            "part_count": part_count,
            "created_ids": created.iter().map(|id| id.0).collect::<Vec<_>>()
        }),
    )
}

fn describe_scene(ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let ids = ctx.scene.all_valid_ids();
    let mut primitive_counts = std::collections::BTreeMap::<&'static str, usize>::new();
    for id in &ids {
        if let Some(node) = ctx.scene.get(*id) {
            *primitive_counts.entry(node.primitive.label()).or_default() += 1;
        }
    }

    let mut lines = vec![format!("entities: {}", ids.len())];
    for (primitive, count) in &primitive_counts {
        lines.push(format!("{primitive}: {count}"));
    }
    if let Some(id) = ctx.hierarchy.selected_node {
        if let Some(node) = ctx.scene.get(id) {
            lines.push(format!("selected: {} ({})", node.name, id.0));
        }
    }

    CommandOutput::info(
        "Scene description",
        lines,
        serde_json::json!({
            "ok": true,
            "entities": ids.len(),
            "selected": ctx.hierarchy.selected_node.map(|id| id.0)
        }),
    )
}

fn focus_entity(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let Some(id) = resolve_target(command, ctx.scene, ctx.hierarchy) else {
        return CommandOutput::error("Focus entity", "Target not found.");
    };
    select_ids(ctx, vec![id]);
    ctx.viewport.focus_selected_entity(ctx.scene, Some(id));
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
    hierarchy: &HierarchyPanel,
) -> Option<SceneNodeId> {
    let target = command.arg("target").map(str::to_string).or_else(|| {
        if command.positional.is_empty() {
            None
        } else {
            Some(command.positional.join(" "))
        }
    });

    let Some(target) = target else {
        return hierarchy.selected_node;
    };

    if target.eq_ignore_ascii_case("selected") {
        return hierarchy.selected_node;
    }
    if let Ok(index) = target.parse::<usize>() {
        let id = SceneNodeId(index);
        if scene.is_valid_node(id) {
            return Some(id);
        }
    }
    scene
        .find_node_by_path(&target)
        .or_else(|| scene.find_node_by_name(&target))
        .or_else(|| {
            let lower = target.to_ascii_lowercase();
            scene
                .iter()
                .find(|(_, node)| node.name.to_ascii_lowercase().contains(&lower))
                .map(|(id, _)| id)
        })
}

fn select_ids(ctx: &mut GameCommandContext<'_>, ids: Vec<SceneNodeId>) {
    ctx.hierarchy.selected_node = ids.first().copied();
    ctx.hierarchy.selected_nodes = ids.clone();
    ctx.viewport.selected = ids;
}

fn primitive_arg(command: &ParsedCommand) -> Option<Primitive> {
    let raw = command
        .arg("primitive")
        .or_else(|| command.arg("type"))
        .or_else(|| command.arg("kind"))
        .or_else(|| command.first_positional())?;
    match raw.to_ascii_lowercase().as_str() {
        "empty" => Some(Primitive::Empty),
        "cube" | "box" | "block" => Some(Primitive::Cube),
        "sphere" | "ball" => Some(Primitive::Sphere),
        "plane" | "floor" => Some(Primitive::Plane),
        "cylinder" => Some(Primitive::Cylinder),
        // Keep command compatibility while using the orthographic 3D plane.
        "sprite" | "sprite2d" => Some(Primitive::Plane),
        _ => None,
    }
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
        format!("name: {}", node.name),
        format!("primitive: {}", node.primitive.label()),
        format_vec3("position", node.position),
        format_vec3("rotation_deg", node.rotation),
        format_vec3("scale", node.scale),
        format!(
            "color_rgba: [{}, {}, {}, {}]",
            node.color.r, node.color.g, node.color.b, node.color.a
        ),
        format!("world_path: {}", scene.node_path(id).unwrap_or_default()),
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
    lines.extend(local_vertex_lines(node.primitive, node.scale));
    lines
}

fn node_json(
    id: SceneNodeId,
    node: &raf_core::scene::graph::SceneNode,
    scene: &SceneGraph,
) -> serde_json::Value {
    let (bounds_min, bounds_max) = primitive_bounds(node.scale);
    serde_json::json!({
        "ok": true,
        "entity": {
            "id": id.0,
            "uuid": node.uuid.to_string(),
            "name": node.name,
            "path": scene.node_path(id),
            "primitive": node.primitive.label(),
            "position": vec3_json(node.position),
            "rotation_deg": vec3_json(node.rotation),
            "scale": vec3_json(node.scale),
            "color_rgba": [node.color.r, node.color.g, node.color.b, node.color.a],
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

fn local_vertex_lines(primitive: Primitive, scale: Vec3) -> Vec<String> {
    match primitive {
        Primitive::Cube => {
            let sx = scale.x * 0.5;
            let sy = scale.y * 0.5;
            let sz = scale.z * 0.5;
            [
                Vec3::new(-sx, -sy, -sz),
                Vec3::new(sx, -sy, -sz),
                Vec3::new(sx, sy, -sz),
                Vec3::new(-sx, sy, -sz),
                Vec3::new(-sx, -sy, sz),
                Vec3::new(sx, -sy, sz),
                Vec3::new(sx, sy, sz),
                Vec3::new(-sx, sy, sz),
            ]
            .iter()
            .enumerate()
            .map(|(index, vertex)| format_vec3(&format!("local_vertex_{index}"), *vertex))
            .collect()
        }
        Primitive::Plane => {
            let sx = scale.x * 0.5;
            let sz = scale.z * 0.5;
            [
                Vec3::new(-sx, 0.0, -sz),
                Vec3::new(sx, 0.0, -sz),
                Vec3::new(sx, 0.0, sz),
                Vec3::new(-sx, 0.0, sz),
            ]
            .iter()
            .enumerate()
            .map(|(index, vertex)| format_vec3(&format!("local_vertex_{index}"), *vertex))
            .collect()
        }
        Primitive::Cylinder => {
            vec!["local_vertex_note: cylinder uses builtin primitive asset bounds.".to_string()]
        }
        Primitive::Sphere => {
            vec!["local_vertex_note: sphere uses builtin primitive asset bounds.".to_string()]
        }
        Primitive::Empty => Vec::new(),
    }
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
    command.arg(name).and_then(parse_f32).unwrap_or(default)
}

fn u8_arg(command: &ParsedCommand, name: &str, default: u8) -> u8 {
    command
        .arg(name)
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(default)
}

fn parse_color(raw: &str) -> Option<NodeColor> {
    let raw = raw.trim();
    if let Some(hex) = raw.strip_prefix('#') {
        if hex.len() == 6 || hex.len() == 8 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = if hex.len() == 8 {
                u8::from_str_radix(&hex[6..8], 16).ok()?
            } else {
                255
            };
            return Some(NodeColor::rgba(r, g, b, a));
        }
    }

    let parts = raw
        .split(',')
        .filter_map(|part| part.trim().parse::<u8>().ok())
        .collect::<Vec<_>>();
    match parts.as_slice() {
        [r, g, b] => Some(NodeColor::rgba(*r, *g, *b, 255)),
        [r, g, b, a] => Some(NodeColor::rgba(*r, *g, *b, *a)),
        _ => None,
    }
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
        let mut hierarchy = HierarchyPanel::default();
        let mut viewport = ViewportPanel::default();
        let ParsedInput::Command(command) =
            parse_console_input("/game.add primitive=cube name=Block x=1 y=2 z=3").unwrap()
        else {
            panic!("expected command");
        };
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            hierarchy: &mut hierarchy,
            viewport: &mut viewport,
        };

        let output = execute("game.add", &command, &mut ctx);

        assert_eq!(output.level, CommandLevel::Info);
        assert!(output.changed);
        let selected = ctx.hierarchy.selected_node.expect("selected node");
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
}
