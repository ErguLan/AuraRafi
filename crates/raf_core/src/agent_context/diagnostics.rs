//! Revision fingerprints, semantic diffs and scene verification diagnostics.

use super::scene::{
    aabb_overlaps, display_path, is_ancestor, node_kind, presentation_name, resolve_target,
    scene_item_reference, scoped_ids, world_bounds,
};
use super::support::{bool_arg, string_arg, string_array_arg};
use super::AgentObservationResult;
use crate::scene::{Primitive, SceneGraph, SceneNode, SceneNodeId};
use crate::transaction::VerificationSummary;
use glam::Vec3;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};

/// Create a bounded semantic fingerprint for document-level change tracking.
/// It intentionally uses UUIDs instead of flat slot IDs, skips deleted slots,
/// and keeps only fields that matter to authoring, references and verification.
/// Renderer caches and mesh payloads never participate in this fingerprint.
pub fn scene_fingerprint(scene: &SceneGraph) -> BTreeMap<String, Value> {
    let mut fingerprint: BTreeMap<String, Value> = scene
        .iter()
        .filter(|(id, _)| scene.is_valid_node(*id))
        .map(|(id, node)| {
            (
                node.uuid.to_string(),
                json!({
                    "id": id.0,
                    "name": presentation_name(id, node),
                    "path": display_path(scene, id),
                    "position": [node.position.x, node.position.y, node.position.z],
                    "rotation": [node.rotation.x, node.rotation.y, node.rotation.z],
                    "scale": [node.scale.x, node.scale.y, node.scale.z],
                    "primitive": node.primitive.label(),
                    "color_rgba": [node.color.r, node.color.g, node.color.b, node.color.a],
                    "visible": node.visible,
                    "casts_shadows": node.casts_shadows,
                    "receives_shadows": node.receives_shadows,
                    "locked": node.locked,
                    "is_folder": node.is_folder,
                    "parent": node.parent.map(|parent| parent.0),
                    "source_asset": node.source_asset,
                    "animation_asset": node.animation_asset,
                    "source_schema_version": node.source_schema_version,
                    "scripts": node.scripts,
                    "variables": node.variables,
                    "behavior_parameters": node.behavior_parameters,
                    "game_camera": node.game_camera,
                    "semantic_role": node.semantic_role,
                    "stable_key": node.stable_key,
                    "tags": node.tags,
                    "agent_origin": node.agent_origin
                }),
            )
        })
        .collect();
    fingerprint.insert("$environment".into(), json!(scene.environment()));
    fingerprint
}

/// Compare two semantic scene fingerprints and return a compact document diff.
/// The arrays contain stable UUIDs, so callers can safely use them in undo,
/// telemetry and follow-up tool calls without exposing the full scene again.
pub fn scene_diff(
    before: &BTreeMap<String, Value>,
    after: &BTreeMap<String, Value>,
) -> Option<Value> {
    let environment_changed = before.get("$environment") != after.get("$environment");
    let created = after
        .keys()
        .filter(|key| key.as_str() != "$environment" && !before.contains_key(*key))
        .cloned()
        .collect::<Vec<_>>();
    let deleted = before
        .keys()
        .filter(|key| key.as_str() != "$environment" && !after.contains_key(*key))
        .cloned()
        .collect::<Vec<_>>();
    let updated = after
        .iter()
        .filter(|(key, value)| {
            key.as_str() != "$environment"
                && before.get(*key).is_some_and(|before| before != *value)
        })
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    (environment_changed || !created.is_empty() || !deleted.is_empty() || !updated.is_empty()).then(
        || {
            json!({
                "created": created,
                "deleted": deleted,
                "updated": updated,
                "environment_changed": environment_changed
            })
        },
    )
}

pub fn scene_verify(
    scene: &SceneGraph,
    args: &Value,
    selected: &[SceneNodeId],
) -> AgentObservationResult {
    let mut checks = Vec::new();
    let mut failures = Vec::new();
    let root_target = string_arg(args, "root").or_else(|| string_arg(args, "scope"));
    let root = root_target.and_then(|target| resolve_target(scene, target));
    if root_target.is_some() && root.is_none() {
        failures.push("The requested verification root was not found.".to_string());
    }
    let scope_ids = if root_target.is_some() && root.is_none() {
        Vec::new()
    } else {
        scoped_ids(scene, root)
    };
    let scope_set = scope_ids.iter().copied().collect::<HashSet<_>>();
    let live_count = scene.all_live_ids().len();
    let checked_count = scope_ids.len();
    checks.push(format!(
        "Checked {checked_count} live node(s){}.",
        root.map(|_| " in the requested scope").unwrap_or("")
    ));
    let targets = string_array_arg(args, "targets");
    let target = string_arg(args, "target").map(str::to_string);
    let mut targets = targets;
    if let Some(target) = target {
        targets.push(target);
    }
    if targets.is_empty() && !selected.is_empty() {
        checks.push(
            "No explicit targets were provided; the requested scope was verified. Selection is available for an explicit target check."
                .to_string(),
        );
    }
    let mut resolved_targets = Vec::new();
    for target in &targets {
        if let Some(id) = resolve_target(scene, target) {
            if root.is_some() && !scope_set.contains(&id) {
                failures.push(format!("Target '{target}' is outside the requested scope."));
                continue;
            }
            resolved_targets.push(id);
            checks.push(format!("Target '{target}' resolves to a live entity."));
        } else {
            failures.push(format!("Target '{target}' was not found."));
        }
    }
    let expected_scope_count = if targets.is_empty() {
        checked_count
    } else {
        resolved_targets.len()
    };
    if let Some(expected) = args.get("expected_count").and_then(Value::as_u64) {
        if expected as usize == expected_scope_count {
            checks.push(format!("Expected entity count {expected} matches."));
        } else {
            failures.push(format!(
                "Expected {expected} entities for the requested verification, found {expected_scope_count}."
            ));
        }
    }
    for id in &scope_ids {
        let Some(node) = scene.get(*id) else { continue };
        let finite = [
            node.position.x,
            node.position.y,
            node.position.z,
            node.rotation.x,
            node.rotation.y,
            node.rotation.z,
            node.scale.x,
            node.scale.y,
            node.scale.z,
        ]
        .into_iter()
        .all(|value| value.is_finite());
        if !finite {
            failures.push(format!(
                "Entity '{}' has a non-finite transform.",
                presentation_name(*id, node)
            ));
        }
        if node.scale.x.abs() <= f32::EPSILON
            || node.scale.y.abs() <= f32::EPSILON
            || node.scale.z.abs() <= f32::EPSILON
        {
            failures.push(format!(
                "Entity '{}' has a zero scale axis.",
                presentation_name(*id, node)
            ));
        }
        if let Some(parent) = node.parent {
            if !scene.is_valid_node(parent) {
                failures.push(format!(
                    "Entity '{}' points to a missing parent.",
                    presentation_name(*id, node)
                ));
            } else if !scene
                .get(parent)
                .is_some_and(|parent_node| parent_node.children.contains(id))
            {
                failures.push(format!(
                    "Entity '{}' is not linked from its parent.",
                    presentation_name(*id, node)
                ));
            }
        }
        for child in &node.children {
            if !scene
                .get(*child)
                .is_some_and(|child_node| child_node.parent == Some(*id))
            {
                failures.push(format!(
                    "Entity '{}' has an inconsistent child link.",
                    presentation_name(*id, node)
                ));
            }
        }
    }
    if bool_arg(args, "strict", false) {
        let mut names = BTreeMap::<String, usize>::new();
        for id in &scope_ids {
            if let Some(node) = scene.get(*id) {
                *names
                    .entry(presentation_name(*id, node).to_ascii_lowercase())
                    .or_default() += 1;
            }
        }
        let duplicate_count = names.values().filter(|count| **count > 1).count();
        if duplicate_count > 0 {
            failures.push(format!(
                "{duplicate_count} duplicate name group(s) in scope."
            ));
        } else {
            checks.push("Names are unique within the requested scope.".to_string());
        }
    }
    let expected_names = string_array_arg(args, "expected_names");
    for expected_name in &expected_names {
        let found = scope_ids.iter().any(|id| {
            scene.get(*id).is_some_and(|node| {
                presentation_name(*id, node).eq_ignore_ascii_case(expected_name)
            })
        });
        if found {
            checks.push(format!("Expected entity '{expected_name}' exists."));
        } else {
            failures.push(format!("Expected entity '{expected_name}' was not found."));
        }
    }
    if let Some(expected) = args.get("expected") {
        let expected_targets = if !resolved_targets.is_empty() {
            resolved_targets.clone()
        } else {
            selected
                .iter()
                .copied()
                .filter(|id| scope_set.contains(id) && scene.is_valid_node(*id))
                .collect::<Vec<_>>()
        };
        if expected_targets.is_empty() {
            failures.push(
                "An expected state requires at least one resolved target or selected entity."
                    .to_string(),
            );
        } else {
            for id in expected_targets {
                if let Some(node) = scene.get(id) {
                    failures.extend(verify_expected_node_state(scene, id, node, expected));
                }
            }
        }
    }
    let check_collisions = bool_arg(args, "check_collisions", false);
    let mut collisions_truncated = false;
    let collisions = if check_collisions {
        let renderable = scope_ids
            .iter()
            .copied()
            .filter(|id| {
                scene
                    .get(*id)
                    .is_some_and(|node| !node.is_folder && node.primitive != Primitive::Empty)
            })
            .take(512)
            .collect::<Vec<_>>();
        collisions_truncated = scope_ids
            .iter()
            .filter(|id| {
                scene
                    .get(**id)
                    .is_some_and(|node| !node.is_folder && node.primitive != Primitive::Empty)
            })
            .count()
            > renderable.len();
        let mut collisions = Vec::new();
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
                if aabb_overlaps(left_bounds, right_bounds) {
                    collisions.push(json!({
                        "a": scene_item_reference(scene, *left),
                        "b": scene_item_reference(scene, *right),
                    }));
                    if collisions.len() >= 512 {
                        collisions_truncated = true;
                        break;
                    }
                }
            }
            if collisions_truncated {
                break;
            }
        }
        if collisions.is_empty() {
            checks.push("No renderable AABB collisions were found.".to_string());
        } else {
            failures.push(format!(
                "{} renderable AABB collision(s) found.",
                collisions.len()
            ));
        }
        collisions
    } else {
        Vec::new()
    };
    let status = if failures.is_empty() {
        "passed"
    } else {
        "failed"
    };
    let mut result = AgentObservationResult::new(
        "Scene verification",
        if failures.is_empty() {
            format!("Scene verification passed with {} check(s).", checks.len())
        } else {
            format!(
                "Scene verification failed with {} issue(s).",
                failures.len()
            )
        },
        json!({
            "live_entity_count": live_count,
            "checked_entity_count": checked_count,
            "expected_scope_count": expected_scope_count,
            "scope": root.and_then(|id| scene_item_reference(scene, id)),
            "targets": targets,
            "resolved_targets": resolved_targets.iter().filter_map(|id| scene_item_reference(scene, *id)).collect::<Vec<_>>(),
            "expected_names": expected_names,
            "collisions": collisions,
            "collisions_truncated": collisions_truncated,
            "checks": checks,
            "failures": failures
        }),
    );
    result.verification = Some(VerificationSummary {
        status: status.to_string(),
        checks: checks.clone(),
        failures: failures.clone(),
    });
    if collisions_truncated {
        result
            .warnings
            .push("Collision verification was capped at 512 pairs.".to_string());
    }
    result
}

fn verify_expected_node_state(
    scene: &SceneGraph,
    id: SceneNodeId,
    node: &SceneNode,
    expected: &Value,
) -> Vec<String> {
    let Some(expected) = expected.as_object() else {
        return vec!["expected must be an object.".to_string()];
    };
    let name = presentation_name(id, node);
    let mut failures = Vec::new();
    if let Some(value) = expected.get("primitive") {
        let Some(kind) = value.as_str() else {
            failures.push("expected.primitive must be a string.".to_string());
            return failures;
        };
        let expected_kind = canonical_expected_kind(kind);
        if node_kind(node) != expected_kind.as_str() {
            failures.push(format!(
                "{name} has primitive {}, expected {expected_kind}.",
                node_kind(node)
            ));
        }
    }
    if let Some(transform) = expected.get("transform") {
        let Some(transform) = transform.as_object() else {
            failures.push("expected.transform must be an object.".to_string());
            return failures;
        };
        for (key, actual) in [
            ("position", node.position),
            ("rotation_deg", node.rotation),
            ("scale", node.scale),
        ] {
            if let Some(value) = transform.get(key) {
                match expected_vec3(value) {
                    Some(expected) if vec3_matches(actual, expected) => {}
                    Some(expected) => failures.push(format!(
                        "{name} {key} is [{:.3}, {:.3}, {:.3}], expected [{:.3}, {:.3}, {:.3}].",
                        actual.x, actual.y, actual.z, expected[0], expected[1], expected[2]
                    )),
                    None => failures.push(format!(
                        "expected.transform.{key} must contain 3 finite numbers."
                    )),
                }
            }
        }
    }
    if let Some(value) = expected.get("color_rgba") {
        let channels = value.as_array().and_then(|values| {
            (values.len() == 3 || values.len() == 4).then(|| {
                [
                    values.first().and_then(Value::as_u64),
                    values.get(1).and_then(Value::as_u64),
                    values.get(2).and_then(Value::as_u64),
                    if values.len() == 3 {
                        Some(255)
                    } else {
                        values.get(3).and_then(Value::as_u64)
                    },
                ]
            })
        });
        match channels {
            Some([Some(r), Some(g), Some(b), Some(a)])
                if [r, g, b, a]
                    == [
                        node.color.r as u64,
                        node.color.g as u64,
                        node.color.b as u64,
                        node.color.a as u64,
                    ] => {}
            Some(_) => failures.push(format!(
                "{name} color does not match expected RGBA channels."
            )),
            None => failures
                .push("expected.color_rgba must contain 3 or 4 integer channels.".to_string()),
        }
    }
    if let Some(value) = expected.get("parent") {
        let (expected_parent, unresolved) = match value {
            Value::Null => (None, false),
            Value::String(parent)
                if parent.trim().is_empty()
                    || matches!(
                        parent.trim().to_ascii_lowercase().as_str(),
                        "root" | "scene" | "none" | "null"
                    ) =>
            {
                (None, false)
            }
            Value::String(parent) => {
                let resolved = resolve_target(scene, parent);
                (resolved, resolved.is_none())
            }
            _ => (None, true),
        };
        if !matches!(value, Value::Null | Value::String(_)) {
            failures.push("expected.parent must be a target string or null.".to_string());
        } else if unresolved {
            failures.push("expected.parent target was not found.".to_string());
        } else if node.parent != expected_parent {
            failures.push(format!(
                "{name} parent is {:?}, expected {:?}.",
                node.parent, expected_parent
            ));
        }
    }
    failures
}

fn expected_vec3(value: &Value) -> Option<[f32; 3]> {
    let output = match value {
        Value::Array(values) if values.len() == 3 => [
            values.first()?.as_f64()? as f32,
            values.get(1)?.as_f64()? as f32,
            values.get(2)?.as_f64()? as f32,
        ],
        Value::Object(object) => [
            object.get("x")?.as_f64()? as f32,
            object.get("y")?.as_f64()? as f32,
            object.get("z")?.as_f64()? as f32,
        ],
        _ => return None,
    };
    output
        .iter()
        .all(|value| value.is_finite())
        .then_some(output)
}

fn vec3_matches(actual: Vec3, expected: [f32; 3]) -> bool {
    (actual.x - expected[0]).abs() <= 0.0001
        && (actual.y - expected[1]).abs() <= 0.0001
        && (actual.z - expected[2]).abs() <= 0.0001
}

fn canonical_expected_kind(kind: &str) -> String {
    match kind.trim().to_ascii_lowercase().as_str() {
        "box" | "block" => "cube".to_string(),
        "ball" | "circle" | "uv_sphere" => "sphere".to_string(),
        "floor" | "sprite" | "sprite2d" => "plane".to_string(),
        "group" | "folder" => "group".to_string(),
        "empty" => "empty".to_string(),
        _ => kind.trim().to_ascii_lowercase(),
    }
}
