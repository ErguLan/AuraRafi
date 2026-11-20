//! Spatial projections: world-space extents and conservative AABB overlap evidence.

use super::scene::{
    aabb_overlaps, bounds_value, is_ancestor, merge_bounds, resolve_target, scene_item,
    scene_item_reference, scoped_ids, world_bounds,
};
use super::support::{bool_arg, page, paged_data, string_arg};
use super::AgentObservationResult;
use crate::scene::{Primitive, SceneGraph, SceneNodeId};
use crate::transaction::VerificationSummary;
use glam::Vec3;
use serde_json::{json, Value};

/// Describe the spatial layout of a bounded scene scope without exposing
/// renderer mesh payloads. This is the perception primitive an agent needs
/// before arranging a real place: semantic entities, world bounds, overall
/// extents and conservative AABB overlaps.
pub fn scene_spatial_map(
    scene: &SceneGraph,
    args: &Value,
    selected: &[SceneNodeId],
) -> AgentObservationResult {
    const MAX_SPATIAL_NODES: usize = 256;
    const MAX_OVERLAPS: usize = 128;

    let (offset, limit) = page(args);
    let root_target = string_arg(args, "root").or_else(|| string_arg(args, "scope"));
    let root = root_target.and_then(|target| resolve_target(scene, target));
    let scoped = if root_target.is_some() && root.is_none() {
        Vec::new()
    } else {
        scoped_ids(scene, root)
    };
    let include_hidden = bool_arg(args, "include_hidden", false);
    let check_collisions = bool_arg(args, "check_collisions", true);
    let renderable = scoped
        .iter()
        .copied()
        .filter(|id| {
            scene.get(*id).is_some_and(|node| {
                !node.is_folder
                    && node.primitive != Primitive::Empty
                    && (include_hidden || node.visible)
            })
        })
        .collect::<Vec<_>>();
    let total = renderable.len();
    let items = renderable
        .iter()
        .skip(offset)
        .take(limit)
        .filter_map(|id| {
            let mut item = scene_item(scene, *id, false)?;
            if let Some((min, max)) = world_bounds(scene, *id) {
                if let Some(object) = item.as_object_mut() {
                    object.insert(
                        "world_bounds".to_string(),
                        json!({
                            "min": [min.x, min.y, min.z],
                            "max": [max.x, max.y, max.z],
                            "size": [max.x - min.x, max.y - min.y, max.z - min.z],
                        }),
                    );
                    let world = scene.world_matrix(*id).col(3).truncate();
                    object.insert(
                        "world_position".to_string(),
                        json!([world.x, world.y, world.z]),
                    );
                }
            }
            Some(item)
        })
        .collect::<Vec<_>>();

    let inspected = renderable
        .iter()
        .copied()
        .take(MAX_SPATIAL_NODES)
        .collect::<Vec<_>>();
    let mut aggregate_bounds: Option<(Vec3, Vec3)> = None;
    for id in &inspected {
        if let Some(bounds) = world_bounds(scene, *id) {
            merge_bounds(&mut aggregate_bounds, bounds);
        }
    }

    let mut overlaps = Vec::new();
    let mut overlaps_truncated = false;
    if check_collisions {
        for (index, left) in inspected.iter().enumerate() {
            let Some(left_bounds) = world_bounds(scene, *left) else {
                continue;
            };
            for right in inspected.iter().skip(index + 1) {
                if is_ancestor(scene, *left, *right) || is_ancestor(scene, *right, *left) {
                    continue;
                }
                let Some(right_bounds) = world_bounds(scene, *right) else {
                    continue;
                };
                if aabb_overlaps(left_bounds, right_bounds) {
                    overlaps.push(json!({
                        "a": scene_item_reference(scene, *left),
                        "b": scene_item_reference(scene, *right),
                    }));
                    if overlaps.len() >= MAX_OVERLAPS {
                        overlaps_truncated = true;
                        break;
                    }
                }
            }
            if overlaps.len() >= MAX_OVERLAPS {
                overlaps_truncated = true;
                break;
            }
        }
    }

    let mut result = AgentObservationResult::new(
        "Scene spatial map",
        format!(
            "{} renderable node(s) in the requested scope; {} returned, {} overlap(s) detected.",
            total,
            items.len(),
            overlaps.len()
        ),
        paged_data(
            items,
            total,
            offset,
            limit,
            json!({
                "scope": root.and_then(|id| scene_item_reference(scene, id)),
                "selected": selected
                    .iter()
                    .filter(|id| scoped.contains(id))
                    .filter_map(|id| scene_item_reference(scene, *id))
                    .collect::<Vec<_>>(),
                "bounds": aggregate_bounds.map(bounds_value),
                "overlaps": overlaps,
                "inspected": inspected.len(),
                "spatial_limit": MAX_SPATIAL_NODES,
                "overlaps_truncated": overlaps_truncated
                    || (check_collisions && inspected.len() < renderable.len()),
                "collision_check": check_collisions,
                "include_hidden": include_hidden,
                "identity": "world-space AABBs are conservative; UUID/ref remains canonical",
            }),
        ),
    );
    if root_target.is_some() && root.is_none() {
        result
            .warnings
            .push("The requested spatial map root was not found.".to_string());
        result.verification = Some(VerificationSummary {
            status: "failed".to_string(),
            checks: Vec::new(),
            failures: vec!["The requested spatial map root was not found.".to_string()],
        });
    }
    if renderable.len() > MAX_SPATIAL_NODES {
        result.warnings.push(format!(
            "Spatial overlap checks were limited to {MAX_SPATIAL_NODES} renderable nodes."
        ));
    }
    if overlaps_truncated {
        result.warnings.push(format!(
            "Spatial overlap results were capped at {MAX_OVERLAPS} pairs."
        ));
    }
    result
}

/// Return only actionable overlap evidence for a bounded scope. Each pair
/// includes penetration depth and a deterministic world-space correction for
/// the second entity, allowing an agent to repair layout without guessing
/// from a large spatial dump.
pub fn scene_check_overlaps(
    scene: &SceneGraph,
    args: &Value,
    _selected: &[SceneNodeId],
) -> AgentObservationResult {
    let root_target = string_arg(args, "root").or_else(|| string_arg(args, "scope"));
    let root = root_target.and_then(|target| resolve_target(scene, target));
    let include_hidden = bool_arg(args, "include_hidden", false);
    let margin = args
        .get("margin")
        .and_then(Value::as_f64)
        .unwrap_or(0.02)
        .clamp(0.0, 100.0) as f32;
    let max_pairs = args
        .get("max_pairs")
        .and_then(Value::as_u64)
        .unwrap_or(64)
        .clamp(1, 256) as usize;
    let scoped = if root_target.is_some() && root.is_none() {
        Vec::new()
    } else {
        scoped_ids(scene, root)
    };
    let renderable = scoped
        .into_iter()
        .filter(|id| {
            scene.get(*id).is_some_and(|node| {
                !node.is_folder
                    && node.primitive != Primitive::Empty
                    && (include_hidden || node.visible)
            })
        })
        .take(256)
        .collect::<Vec<_>>();
    let mut pairs = Vec::new();
    let mut total_detected = 0usize;
    for (index, left) in renderable.iter().enumerate() {
        let Some(left_bounds) = world_bounds(scene, *left) else {
            continue;
        };
        for right in renderable.iter().skip(index + 1) {
            if is_ancestor(scene, *left, *right) || is_ancestor(scene, *right, *left) {
                continue;
            }
            let Some(right_bounds) = world_bounds(scene, *right) else {
                continue;
            };
            if !aabb_overlaps(left_bounds, right_bounds) {
                continue;
            }
            total_detected = total_detected.saturating_add(1);
            if pairs.len() >= max_pairs {
                continue;
            }
            let penetration = Vec3::new(
                left_bounds.1.x.min(right_bounds.1.x) - left_bounds.0.x.max(right_bounds.0.x),
                left_bounds.1.y.min(right_bounds.1.y) - left_bounds.0.y.max(right_bounds.0.y),
                left_bounds.1.z.min(right_bounds.1.z) - left_bounds.0.z.max(right_bounds.0.z),
            );
            let (axis, depth) = if penetration.x <= penetration.y && penetration.x <= penetration.z
            {
                ("x", penetration.x)
            } else if penetration.y <= penetration.z {
                ("y", penetration.y)
            } else {
                ("z", penetration.z)
            };
            let left_center = (left_bounds.0 + left_bounds.1) * 0.5;
            let right_center = (right_bounds.0 + right_bounds.1) * 0.5;
            let direction = match axis {
                "x" if right_center.x < left_center.x => -1.0,
                "y" if right_center.y < left_center.y => -1.0,
                "z" if right_center.z < left_center.z => -1.0,
                _ => 1.0,
            };
            let mut offset = Vec3::ZERO;
            match axis {
                "x" => offset.x = direction * (depth + margin),
                "y" => offset.y = direction * (depth + margin),
                _ => offset.z = direction * (depth + margin),
            }
            pairs.push(json!({
                "a": scene_item_reference(scene, *left),
                "b": scene_item_reference(scene, *right),
                "a_bounds": bounds_value(left_bounds),
                "b_bounds": bounds_value(right_bounds),
                "penetration": [penetration.x, penetration.y, penetration.z],
                "smallest_axis": axis,
                "suggested_world_offset_for_b": [offset.x, offset.y, offset.z],
                "suggested_snap": {
                    "target": scene.get(*right).map(|node| format!("entity:{}", node.uuid)),
                    "snap_to": scene.get(*left).map(|node| format!("entity:{}", node.uuid)),
                    "axis": axis,
                    "placement": if direction >= 0.0 { "after" } else { "before" },
                    "gap": margin
                }
            }));
        }
    }
    let truncated = total_detected > pairs.len();
    let mut result = AgentObservationResult::new(
        "Scene overlap check",
        format!(
            "Checked {} renderable node(s); found {} overlap pair(s).",
            renderable.len(),
            total_detected
        ),
        json!({
            "scope": root.and_then(|id| scene_item_reference(scene, id)),
            "checked": renderable.len(),
            "overlap_count": total_detected,
            "pairs": pairs,
            "truncated": truncated,
            "max_pairs": max_pairs,
            "margin": margin,
            "suggestion": if total_detected > 0 {
                "Use each pair's suggested_snap with scene_snap, then run this check again."
            } else {
                "No overlap repair is needed in this scope."
            }
        }),
    );
    if root_target.is_some() && root.is_none() {
        result
            .warnings
            .push("The requested overlap scope was not found.".to_string());
        result.verification = Some(VerificationSummary {
            status: "failed".to_string(),
            checks: Vec::new(),
            failures: vec!["The requested overlap scope was not found.".to_string()],
        });
    } else {
        result.verification = Some(VerificationSummary {
            status: if total_detected == 0 {
                "passed"
            } else {
                "failed"
            }
            .to_string(),
            checks: vec![format!("{} renderable node(s) checked.", renderable.len())],
            failures: (total_detected > 0)
                .then(|| format!("{total_detected} overlap pair(s) require review."))
                .into_iter()
                .collect(),
        });
    }
    result
}
