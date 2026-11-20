//! Read-only scene audits: static navigation reachability and real-world design checks.

use super::scene::{
    bounds_value, display_name, merge_bounds, resolve_target, scene_item_reference, scoped_ids,
    world_bounds,
};
use super::support::{string_arg, string_array_arg};
use super::AgentObservationResult;
use crate::scene::{Primitive, SceneGraph, SceneNode, SceneNodeId};
use crate::transaction::VerificationSummary;
use glam::Vec3;
use serde_json::{json, Value};
use std::collections::VecDeque;

/// Static walkability and reachability audit. It never starts Play or Runtime:
/// it samples the authored AABBs into a bounded occupancy grid, expands
/// obstacles by an agent radius and runs a BFS from spawn to goals.
pub fn scene_navigation_audit(
    scene: &SceneGraph,
    args: &Value,
    selected: &[SceneNodeId],
) -> AgentObservationResult {
    let root_target = string_arg(args, "root").or_else(|| string_arg(args, "scope"));
    let root = root_target.and_then(|target| resolve_target(scene, target));
    let scoped = if root_target.is_some() && root.is_none() {
        Vec::new()
    } else {
        scoped_ids(scene, root)
    };
    if root_target.is_some() && root.is_none() {
        let mut result = AgentObservationResult::new(
            "Scene navigation audit",
            "The requested navigation audit root was not found.",
            json!({"ok": false, "error": "root_not_found", "root": root_target}),
        );
        result.verification = Some(VerificationSummary {
            status: "failed".to_string(),
            checks: Vec::new(),
            failures: vec!["The requested navigation audit root was not found.".to_string()],
        });
        return result;
    }
    let resolution = args
        .get("resolution")
        .and_then(Value::as_f64)
        .unwrap_or(0.5)
        .clamp(0.1, 8.0) as f32;
    let agent_radius = args
        .get("agent_radius")
        .and_then(Value::as_f64)
        .unwrap_or(0.35)
        .clamp(0.0, 8.0) as f32;
    let mut aggregate_bounds: Option<(Vec3, Vec3)> = None;
    let mut walkable_nodes = Vec::new();
    let mut obstacle_nodes = Vec::new();
    for id in &scoped {
        let Some(node) = scene.get(*id) else {
            continue;
        };
        if node.is_folder || node.primitive == Primitive::Empty || !node.visible {
            continue;
        }
        let Some(bounds) = world_bounds(scene, *id) else {
            continue;
        };
        merge_bounds(&mut aggregate_bounds, bounds);
        let text = navigation_text(node);
        let walkable = is_walkable_surface(node, &text);
        let obstacle = !walkable
            && (node.collider.collider_type != crate::scene::ColliderType::None
                || text.contains("wall")
                || text.contains("obstacle")
                || text.contains("barrier")
                || text.contains("fence")
                || text.contains("shelf")
                || text.contains("counter")
                || text.contains("crate")
                || text.contains("building"));
        if walkable {
            walkable_nodes.push(bounds);
        } else if obstacle {
            obstacle_nodes.push(bounds);
        }
    }
    let Some((min, max)) = aggregate_bounds else {
        let mut result = AgentObservationResult::new(
            "Scene navigation audit",
            "The requested scope has no renderable nodes.",
            json!({"ok": false, "error": "empty_scope"}),
        );
        result.verification = Some(VerificationSummary {
            status: "failed".to_string(),
            checks: Vec::new(),
            failures: vec!["No renderable nodes were found in scope.".to_string()],
        });
        return result;
    };
    let columns = (((max.x - min.x) / resolution).ceil() as usize).saturating_add(1).clamp(2, 256);
    let rows = (((max.z - min.z) / resolution).ceil() as usize).saturating_add(1).clamp(2, 256);
    let cell_center = |column: usize, row: usize| {
        Vec3::new(
            min.x + column as f32 * resolution,
            min.y,
            min.z + row as f32 * resolution,
        )
    };
    let inside = |bounds: (Vec3, Vec3), point: Vec3, margin: f32| {
        point.x >= bounds.0.x - margin
            && point.x <= bounds.1.x + margin
            && point.z >= bounds.0.z - margin
            && point.z <= bounds.1.z + margin
    };
    let mut walkable = vec![true; columns * rows];
    let has_walkable_surfaces = !walkable_nodes.is_empty();
    for row in 0..rows {
        for column in 0..columns {
            let point = cell_center(column, row);
            let index = row * columns + column;
            if has_walkable_surfaces && !walkable_nodes.iter().any(|bounds| inside(*bounds, point, 0.01)) {
                walkable[index] = false;
            }
            if obstacle_nodes
                .iter()
                .any(|bounds| inside(*bounds, point, agent_radius))
            {
                walkable[index] = false;
            }
        }
    }
    let fallback_walkable = !walkable.iter().any(|value| *value);
    if fallback_walkable {
        for row in 0..rows {
            for column in 0..columns {
                let point = cell_center(column, row);
                let index = row * columns + column;
                walkable[index] = !obstacle_nodes
                    .iter()
                    .any(|bounds| inside(*bounds, point, agent_radius));
            }
        }
    }
    let cell_of = |point: Vec3| {
        let column = (((point.x - min.x) / resolution).round() as i64)
            .clamp(0, columns as i64 - 1) as usize;
        let row = (((point.z - min.z) / resolution).round() as i64)
            .clamp(0, rows as i64 - 1) as usize;
        (column, row)
    };
    let node_center = |id: SceneNodeId| {
        world_bounds(scene, id)
            .map(|bounds| (bounds.0 + bounds.1) * 0.5)
            .or_else(|| scene.get(id).map(|_| scene.world_matrix(id).col(3).truncate()))
    };
    let spawn_target = string_arg(args, "spawn");
    let spawn_id = spawn_target
        .and_then(|target| resolve_target(scene, target))
        .or_else(|| {
            scoped.iter().copied().find(|id| {
                scene
                    .get(*id)
                    .is_some_and(|node| navigation_text(node).contains("spawn"))
            })
        });
    let mut goal_ids = string_array_arg(args, "goals")
        .into_iter()
        .filter_map(|target| resolve_target(scene, &target))
        .collect::<Vec<_>>();
    if goal_ids.is_empty() {
        goal_ids = scoped
            .iter()
            .copied()
            .filter(|id| {
                scene.get(*id).is_some_and(|node| {
                    let text = navigation_text(node);
                    text.contains("goal") || text.contains("checkpoint") || text.contains("exit")
                })
            })
            .take(32)
            .collect();
    }
    let spawn_point = spawn_id
        .and_then(node_center)
        .unwrap_or_else(|| cell_center(columns / 2, rows / 2));
    let mut spawn_cell = cell_of(spawn_point);
    if !walkable[spawn_cell.1 * columns + spawn_cell.0] {
        if let Some(cell) = nearest_walkable(&walkable, columns, rows, spawn_cell, 6) {
            spawn_cell = cell;
        }
    }
    let mut distances = vec![usize::MAX; columns * rows];
    let mut queue = VecDeque::new();
    if walkable[spawn_cell.1 * columns + spawn_cell.0] {
        distances[spawn_cell.1 * columns + spawn_cell.0] = 0;
        queue.push_back(spawn_cell);
    }
    while let Some((column, row)) = queue.pop_front() {
        let distance = distances[row * columns + column];
        for (dc, dr) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
            let next_column = column as i64 + dc;
            let next_row = row as i64 + dr;
            if next_column < 0 || next_row < 0 || next_column >= columns as i64 || next_row >= rows as i64 {
                continue;
            }
            let next_column = next_column as usize;
            let next_row = next_row as usize;
            let index = next_row * columns + next_column;
            if !walkable[index] || distances[index] != usize::MAX {
                continue;
            }
            distances[index] = distance + 1;
            queue.push_back((next_column, next_row));
        }
    }
    let goals = goal_ids
        .iter()
        .filter_map(|id| {
            let point = node_center(*id)?;
            let (column, row) = cell_of(point);
            let index = row * columns + column;
            let distance = distances[index];
            Some(json!({
                "ref": scene_item_reference(scene, *id),
                "cell": [column, row],
                "reachable": distance != usize::MAX,
                "distance_m": if distance == usize::MAX { Value::Null } else { json!(distance as f32 * resolution) },
            }))
        })
        .collect::<Vec<_>>();
    let reachable_goals = goals
        .iter()
        .filter(|goal| goal.get("reachable").and_then(Value::as_bool) == Some(true))
        .count();
    let visited = distances.iter().filter(|distance| **distance != usize::MAX).count();
    let walkable_count = walkable.iter().filter(|value| **value).count();
    let narrow = distances
        .iter()
        .enumerate()
        .filter(|(index, distance)| {
            if **distance == usize::MAX {
                return false;
            }
            let column = index % columns;
            let row = index / columns;
            let free = [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .filter(|(dc, dr)| {
                    let c = column as i64 + dc;
                    let r = row as i64 + dr;
                    c >= 0
                        && r >= 0
                        && c < columns as i64
                        && r < rows as i64
                        && walkable[r as usize * columns + c as usize]
                })
                .count();
            free <= 1
        })
        .count();
    let goal_count = goals.len();
    let has_goals = goal_count > 0;
    let all_reachable = has_goals && reachable_goals == goal_count;
    let status = if !has_goals {
        "no_goals"
    } else if all_reachable {
        "passed"
    } else {
        "failed"
    };
    let mut result = AgentObservationResult::new(
        "Scene navigation audit",
        if !has_goals {
            format!(
                "Walkability sampled on {walkable_count} cell(s); no goals were declared."
            )
        } else {
            format!(
                "{reachable_goals}/{} goal(s) reachable from spawn on a {}x{} grid.",
                goals.len(),
                columns,
                rows
            )
        },
        json!({
            "ok": !has_goals || all_reachable,
            "status": status,
            "scope": root.and_then(|id| scene_item_reference(scene, id)),
            "spawn": spawn_id.and_then(|id| scene_item_reference(scene, id)),
            "spawn_cell": [spawn_cell.0, spawn_cell.1],
            "goals": goals,
            "reachable_goals": reachable_goals,
            "grid": {"columns": columns, "rows": rows, "resolution": resolution},
            "walkable_cells": walkable_count,
            "visited_cells": visited,
            "narrow_cells": narrow,
            "fallback_walkable": fallback_walkable,
            "bounds": bounds_value((min, max)),
            "selected": selected
                .iter()
                .filter(|id| scoped.contains(id))
                .filter_map(|id| scene_item_reference(scene, *id))
                .collect::<Vec<_>>(),
            "note": "Static AABB sampling; doors, slopes and runtime physics are not simulated.",
        }),
    );
    result.verification = Some(VerificationSummary {
        status: if !has_goals { "not_checked" } else if all_reachable { "passed" } else { "failed" }.to_string(),
        checks: vec![format!(
            "{walkable_count} walkable cell(s), {visited} visited, {narrow} narrow."
        )],
        failures: if has_goals && !all_reachable {
            vec![format!(
                "{} goal(s) are unreachable from spawn.",
                goal_count.saturating_sub(reachable_goals)
            )]
        } else {
            Vec::new()
        },
    });
    result
}

pub(super) fn navigation_text(node: &SceneNode) -> String {
    let mut text = display_name(&node.name).to_ascii_lowercase();
    if let Some(role) = node.semantic_role.as_deref() {
        text.push(' ');
        text.push_str(&role.to_ascii_lowercase());
    }
    for tag in &node.tags {
        text.push(' ');
        text.push_str(&tag.to_ascii_lowercase());
    }
    text
}

fn is_walkable_surface(node: &SceneNode, text: &str) -> bool {
    node.primitive != Primitive::Empty
        && (text.contains("floor")
            || text.contains("ground")
            || text.contains("aisle")
            || text.contains("path")
            || text.contains("walkway")
            || text.contains("road")
            || text.contains("pavement")
            || text.contains("sidewalk")
            || text.contains("terrain")
            || text.contains("platform"))
}

fn nearest_walkable(
    walkable: &[bool],
    columns: usize,
    rows: usize,
    origin: (usize, usize),
    radius: i64,
) -> Option<(usize, usize)> {
    for distance in 1..=radius {
        for dc in -distance..=distance {
            for dr in -distance..=distance {
                if dc.abs() != distance && dr.abs() != distance {
                    continue;
                }
                let column = origin.0 as i64 + dc;
                let row = origin.1 as i64 + dr;
                if column < 0 || row < 0 || column >= columns as i64 || row >= rows as i64 {
                    continue;
                }
                let index = row as usize * columns + column as usize;
                if walkable[index] {
                    return Some((column as usize, row as usize));
                }
            }
        }
    }
    None
}

/// Audit whether a bounded scene scope reads as an intentional real-world
/// place. This is deliberately read-only: it turns design expectations into
/// evidence the agent can act on instead of silently changing user content.
/// Callers may provide `required_features`; otherwise the generic real-world
/// profile checks for a floor, an enclosure, an entrance and circulation.
pub fn scene_design_audit(
    scene: &SceneGraph,
    args: &Value,
    selected: &[SceneNodeId],
) -> AgentObservationResult {
    let root_target = string_arg(args, "root").or_else(|| string_arg(args, "scope"));
    let root = root_target.and_then(|target| resolve_target(scene, target));
    let scope = if root_target.is_some() && root.is_none() {
        Vec::new()
    } else {
        scoped_ids(scene, root)
    };
    let design_profile = string_arg(args, "design_profile")
        .map(str::trim)
        .filter(|profile| !profile.is_empty())
        .map(|profile| profile.to_ascii_lowercase());
    let default_features = match design_profile.as_deref() {
        Some("store") | Some("supermarket") => vec![
            "floor".to_string(),
            "enclosure".to_string(),
            "entrance".to_string(),
            "circulation".to_string(),
            "primary_modules".to_string(),
        ],
        Some("parking") | Some("parking_lot") | Some("parking-lot") => vec![
            "floor".to_string(),
            "primary_modules".to_string(),
            "entrance".to_string(),
            "circulation".to_string(),
        ],
        Some("outdoor") | Some("environment") => {
            vec!["floor".to_string(), "circulation".to_string()]
        }
        Some("generic") | Some("real_world") | Some("real-world") | Some("building") | None => {
            vec![
                "floor".to_string(),
                "enclosure".to_string(),
                "entrance".to_string(),
                "circulation".to_string(),
            ]
        }
        Some(_) => Vec::new(),
    };
    let requested_features = string_array_arg(args, "required_features");
    let known_features = [
        "floor",
        "enclosure",
        "entrance",
        "circulation",
        "roof",
        "primary_modules",
        "details",
    ];
    let unknown_features = requested_features
        .iter()
        .filter(|feature| {
            !known_features
                .iter()
                .any(|known| known.eq_ignore_ascii_case(feature))
        })
        .cloned()
        .collect::<Vec<_>>();
    let required_features = if requested_features.is_empty() {
        default_features
    } else {
        requested_features
    };
    let feature_names = known_features;
    let mut feature_rows = Vec::new();
    let mut failures = Vec::new();
    let mut checks = Vec::new();
    if !unknown_features.is_empty() {
        failures.push(format!(
            "Unknown required design feature(s): {}.",
            unknown_features.join(", ")
        ));
    }
    let structural_enclosure = matches!(
        design_profile.as_deref(),
        Some("real_world")
            | Some("real-world")
            | Some("building")
            | Some("store")
            | Some("supermarket")
            | None
    );
    if design_profile.as_deref().is_some_and(|profile| {
        !matches!(
            profile,
            "generic"
                | "real_world"
                | "real-world"
                | "building"
                | "store"
                | "supermarket"
                | "parking"
                | "parking_lot"
                | "parking-lot"
                | "outdoor"
                | "environment"
        )
    }) {
        failures.push(format!(
            "Unknown design profile '{}'.",
            design_profile.as_deref().unwrap_or_default()
        ));
    }
    for feature in feature_names {
        let ids = scope
            .iter()
            .copied()
            .filter(|id| {
                scene
                    .get(*id)
                    .is_some_and(|node| feature_matches(node, feature))
            })
            .collect::<Vec<_>>();
        let renderable_ids = ids
            .iter()
            .copied()
            .filter(|id| {
                scene
                    .get(*id)
                    .is_some_and(|node| !node.is_folder && node.primitive != Primitive::Empty)
            })
            .collect::<Vec<_>>();
        let references = ids
            .iter()
            .take(12)
            .filter_map(|id| scene_item_reference(scene, *id))
            .collect::<Vec<_>>();
        let required = required_features
            .iter()
            .any(|requested| requested.eq_ignore_ascii_case(feature));
        let present = !ids.is_empty();
        let status = if present { "present" } else { "missing" };
        feature_rows.push(json!({
            "feature": feature,
            "required": required,
            "status": status,
            "count": ids.len(),
            "renderable_count": renderable_ids.len(),
            "entities": references,
        }));
        if required && !present {
            let message = format!("Required design feature '{feature}' was not found in scope.");
            failures.push(message.clone());
            checks.push(message);
        } else if required && renderable_ids.is_empty() {
            let message =
                format!("Required design feature '{feature}' has no renderable entity in scope.");
            failures.push(message.clone());
            checks.push(message);
        } else if required
            && feature == "enclosure"
            && structural_enclosure
            && renderable_ids.len() < 2
        {
            let message = format!(
                "Design feature 'enclosure' needs at least 2 structural candidates; found {}.",
                renderable_ids.len()
            );
            failures.push(message.clone());
            checks.push(message);
        } else if present {
            checks.push(format!(
                "Design feature '{feature}' has {} candidate(s).",
                ids.len()
            ));
        }
    }

    let renderable = scope
        .iter()
        .copied()
        .filter(|id| {
            scene
                .get(*id)
                .is_some_and(|node| !node.is_folder && node.primitive != Primitive::Empty)
        })
        .collect::<Vec<_>>();
    let mut aggregate_bounds = None;
    for id in &renderable {
        if let Some(bounds) = world_bounds(scene, *id) {
            merge_bounds(&mut aggregate_bounds, bounds);
        }
    }
    if renderable.is_empty() {
        failures.push("The requested design scope has no renderable entities.".to_string());
    }
    if let Some(bounds) = aggregate_bounds {
        let size = bounds.1 - bounds.0;
        if size.x <= 0.001 || size.y <= 0.001 || size.z <= 0.001 {
            failures
                .push("The design scope has no meaningful three-dimensional extent.".to_string());
        } else {
            checks.push(format!(
                "Design extents are {:.2} x {:.2} x {:.2} world units.",
                size.x, size.y, size.z
            ));
        }
    }
    if root_target.is_some() && root.is_none() {
        failures.push("The requested design audit root was not found.".to_string());
    }

    let status = if failures.is_empty() {
        "passed"
    } else {
        "failed"
    };
    let mut result = AgentObservationResult::new(
        "Scene design audit",
        if failures.is_empty() {
            format!(
                "Design audit passed for {} renderable node(s).",
                renderable.len()
            )
        } else {
            format!(
                "Design audit found {} issue(s) in {} renderable node(s).",
                failures.len(),
                renderable.len()
            )
        },
        json!({
            "scope": root.and_then(|id| scene_item_reference(scene, id)),
            "design_profile": design_profile,
            "required_features": required_features,
            "features": feature_rows,
            "renderable_count": renderable.len(),
            "bounds": aggregate_bounds.map(bounds_value),
            "selected": selected
                .iter()
                .filter(|id| scope.contains(id))
                .filter_map(|id| scene_item_reference(scene, *id))
                .collect::<Vec<_>>(),
            "identity": "feature presence is inferred from semantic_role, tags and display names; verify candidates before mutation",
        }),
    );
    if root_target.is_some() && root.is_none() {
        result
            .warnings
            .push("The requested design audit root was not found.".to_string());
    }
    result.verification = Some(VerificationSummary {
        status: status.to_string(),
        checks,
        failures,
    });
    result
}

fn feature_matches(node: &SceneNode, feature: &str) -> bool {
    let mut text = display_name(&node.name).to_ascii_lowercase();
    if let Some(role) = node.semantic_role.as_deref() {
        text.push(' ');
        text.push_str(&role.to_ascii_lowercase());
    }
    if let Some(key) = node.stable_key.as_deref() {
        text.push(' ');
        text.push_str(&key.to_ascii_lowercase());
    }
    for tag in &node.tags {
        text.push(' ');
        text.push_str(&tag.to_ascii_lowercase());
    }
    let tokens: &[&str] = match feature {
        "floor" => &[
            "floor",
            "ground",
            "suelo",
            "piso",
            "pavement",
            "groundplane",
        ],
        "enclosure" => &[
            "wall",
            "walls",
            "muro",
            "pared",
            "facade",
            "fence",
            "boundary",
            "enclosure",
        ],
        "entrance" => &[
            "entrance", "entry", "door", "doorway", "gate", "access", "entrada", "acceso",
        ],
        "circulation" => &[
            "aisle",
            "path",
            "road",
            "parking",
            "circulation",
            "sidewalk",
            "walkway",
            "pasillo",
            "calle",
            "estacionamiento",
            "circulacion",
            "banqueta",
        ],
        "roof" => &["roof", "ceiling", "techo", "cubierta"],
        "primary_modules" => &[
            "shelf", "checkout", "counter", "building", "store", "car", "vehicle", "module",
            "estante", "caja", "edificio", "tienda", "auto", "vehiculo",
        ],
        "details" => &[
            "light", "lamp", "sign", "product", "bollard", "marking", "detail", "luz", "letrero",
            "producto", "detalle",
        ],
        _ => &[],
    };
    tokens.iter().any(|token| text.contains(token))
}
