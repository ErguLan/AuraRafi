//! Primitive prefab instantiation commands.

use super::query::*;
use super::transform::parse_transform_patch;
use super::*;

pub(super) fn generate_prefab(
    command: &ParsedCommand,
    ctx: &mut GameCommandContext<'_>,
) -> CommandOutput {
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
            node.source_asset = Some(format!("builtin://model/{kind}"));
            node.source_schema_version = Some(manifest.schema_version);
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
