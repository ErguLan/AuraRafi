//! Bounded, provider-neutral project perception for Agent, CLI and MCP.
//!
//! The model should not discover a scene by walking arbitrary project files.
//! This module exposes the small semantic view that every adapter can share:
//! stable entity references, hierarchy, bounded queries, asset usage and
//! verification. It deliberately has no editor, renderer or transport
//! dependency.

use crate::scene::{Primitive, SceneGraph, SceneNode, SceneNodeId};
use crate::transaction::VerificationSummary;
use glam::Vec3;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashSet};

pub const DEFAULT_PAGE_SIZE: usize = 48;
pub const MAX_PAGE_SIZE: usize = 128;

/// Remove transport-noisy geometry payloads before a command result is sent
/// back into an LLM context. The full response remains available to the
/// editor/CLI JSON consumers; this view keeps bounds, vertices and duplicate
/// serialized JSON from consuming the next tool-call window.
pub fn compact_result_data(value: &Value) -> Value {
    compact_value(value, 0)
}

/// Keep human/model-facing command lines useful without repeating renderer
/// dumps that already live in structured data. The full response remains
/// available to machine callers that explicitly request raw details.
pub fn compact_result_lines(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter(|line| {
            let normalized = line.trim().to_ascii_lowercase();
            !normalized.is_empty()
                && normalized != "command executed:"
                && !normalized.starts_with("json: {")
                && !normalized.starts_with("local_vertex")
                && !normalized.starts_with("vertex_")
                && !normalized.starts_with("mesh_indices")
                && !normalized.starts_with("mesh_vertices")
                && !normalized.starts_with("index_")
                && !normalized.starts_with("bounds_min")
                && !normalized.starts_with("bounds_max")
        })
        .take(16)
        .map(|line| display_name(line))
        .collect()
}

fn compact_value(value: &Value, depth: usize) -> Value {
    if depth > 10 {
        return json!("[truncated]");
    }
    match value {
        Value::Object(object) => {
            let mut compact = Map::new();
            for (key, value) in object {
                let normalized = key.to_ascii_lowercase();
                if normalized == "json"
                    || normalized == "mesh"
                    || normalized.contains("vertex")
                    || normalized == "bounds_min"
                    || normalized == "bounds_max"
                    || normalized == "mesh_bounds"
                    || normalized == "geometry_bounds"
                    || normalized == "indices"
                {
                    continue;
                }
                compact.insert(key.clone(), compact_value(value, depth + 1));
            }
            Value::Object(compact)
        }
        Value::Array(values) => {
            if values.len() <= 128 {
                Value::Array(
                    values
                        .iter()
                        .map(|value| compact_value(value, depth + 1))
                        .collect(),
                )
            } else {
                Value::Array(
                    values
                        .iter()
                        .take(128)
                        .map(|value| compact_value(value, depth + 1))
                        .collect(),
                )
            }
        }
        Value::String(value) if value.len() > 4_096 => {
            let end = value
                .char_indices()
                .take_while(|(index, _)| *index <= 4_096)
                .map(|(index, _)| index)
                .last()
                .unwrap_or(0);
            Value::String(format!("{}... [truncated]", &value[..end]))
        }
        _ => value.clone(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentObservationResult {
    pub title: String,
    pub summary: String,
    pub data: Value,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub verification: Option<VerificationSummary>,
}

impl AgentObservationResult {
    fn new(title: impl Into<String>, summary: impl Into<String>, data: Value) -> Self {
        Self {
            title: title.into(),
            summary: summary.into(),
            data,
            warnings: Vec::new(),
            verification: None,
        }
    }

    /// Return the machine-level outcome of an observation. Human-readable
    /// warnings may be non-fatal, but a failed verification or an explicit
    /// `found: false` must never be reported as a successful tool call.
    pub fn is_success(&self) -> bool {
        let found = self
            .data
            .get("found")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let verification_failed = self
            .verification
            .as_ref()
            .is_some_and(|verification| verification.status == "failed");
        found && !verification_failed
    }
}

/// The canonical compact scene representation used by engine.context and
/// project.summary. Every caller gets the same count and the same entity
/// identity rules.
pub fn scene_context(scene: &SceneGraph, selected: &[SceneNodeId]) -> Value {
    let live_nodes = scene.all_live_ids();
    let outline: Vec<Value> = live_nodes
        .iter()
        .take(MAX_PAGE_SIZE)
        .filter_map(|id| scene_item(scene, *id, false))
        .collect();
    let selected = selected
        .iter()
        .filter_map(|id| scene_item_reference(scene, *id))
        .collect::<Vec<_>>();
    let root_count = scene
        .roots()
        .iter()
        .filter(|id| scene.is_valid_node(**id))
        .count();

    json!({
        "entity_count": live_nodes.len(),
        "outline_count": outline.len(),
        "root_count": root_count,
        "selected": selected,
        "outline": outline,
        "truncated": live_nodes.len() > MAX_PAGE_SIZE,
        "identity": "uuid-first; name and path are display hints",
    })
}

/// Create a bounded semantic fingerprint for document-level change tracking.
/// It intentionally uses UUIDs instead of flat slot IDs, skips deleted slots,
/// and keeps only fields that matter to authoring, references and verification.
/// Renderer caches and mesh payloads never participate in this fingerprint.
pub fn scene_fingerprint(scene: &SceneGraph) -> BTreeMap<String, Value> {
    scene
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
                    "locked": node.locked,
                    "is_folder": node.is_folder,
                    "parent": node.parent.map(|parent| parent.0),
                    "source_asset": node.source_asset,
                    "source_schema_version": node.source_schema_version,
                    "scripts": node.scripts,
                    "semantic_role": node.semantic_role,
                    "stable_key": node.stable_key,
                    "tags": node.tags,
                    "agent_origin": node.agent_origin
                }),
            )
        })
        .collect()
}

/// Compare two semantic scene fingerprints and return a compact document diff.
/// The arrays contain stable UUIDs, so callers can safely use them in undo,
/// telemetry and follow-up tool calls without exposing the full scene again.
pub fn scene_diff(
    before: &BTreeMap<String, Value>,
    after: &BTreeMap<String, Value>,
) -> Option<Value> {
    let created = after
        .keys()
        .filter(|key| !before.contains_key(*key))
        .cloned()
        .collect::<Vec<_>>();
    let deleted = before
        .keys()
        .filter(|key| !after.contains_key(*key))
        .cloned()
        .collect::<Vec<_>>();
    let updated = after
        .iter()
        .filter(|(key, value)| before.get(*key).is_some_and(|before| before != *value))
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    (!created.is_empty() || !deleted.is_empty() || !updated.is_empty()).then(|| {
        json!({
            "created": created,
            "deleted": deleted,
            "updated": updated
        })
    })
}

pub fn scene_outline(
    scene: &SceneGraph,
    args: &Value,
    selected: &[SceneNodeId],
) -> AgentObservationResult {
    let (offset, limit) = page(args);
    let root_target = string_arg(args, "root").or_else(|| string_arg(args, "scope"));
    let root = root_target.and_then(|target| resolve_target(scene, target));
    let max_depth = args
        .get("depth")
        .and_then(Value::as_u64)
        .map(|depth| depth.min(64) as usize);
    let live_nodes = if root_target.is_some() && root.is_none() {
        Vec::new()
    } else {
        scoped_ids_to_depth(scene, root, max_depth)
    };
    let total = live_nodes.len();
    let items: Vec<Value> = live_nodes
        .iter()
        .skip(offset)
        .take(limit)
        .filter_map(|id| scene_item(scene, *id, false))
        .collect();
    let outline_roots = if root_target.is_some() {
        root.into_iter().collect::<Vec<_>>()
    } else {
        scene.roots().to_vec()
    };
    let outline_root_refs = outline_roots
        .iter()
        .filter_map(|id| scene_item_reference(scene, *id))
        .collect::<Vec<_>>();
    let mut result = AgentObservationResult::new(
        "Scene outline",
        format!("{} scene node(s), {} returned.", total, items.len()),
        paged_data(
            items,
            total,
            offset,
            limit,
            json!({
                "scope": root.and_then(|id| scene_item_reference(scene, id)),
                "roots": outline_root_refs,
                "max_depth": max_depth,
                "selected": selected.iter().filter_map(|id| scene_item_reference(scene, *id)).collect::<Vec<_>>(),
                "identity": "uuid-first; use ref or uuid for mutations",
            }),
        ),
    );
    if root_target.is_some() && root.is_none() {
        result
            .warnings
            .push("The requested scene outline root was not found.".to_string());
        result.verification = Some(VerificationSummary {
            status: "failed".to_string(),
            checks: Vec::new(),
            failures: vec!["The requested scene outline root was not found.".to_string()],
        });
    }
    result
}

pub fn scene_query(
    scene: &SceneGraph,
    args: &Value,
    selected: &[SceneNodeId],
) -> AgentObservationResult {
    let query = string_arg(args, "query")
        .or_else(|| string_arg(args, "name"))
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let primitive = string_arg(args, "primitive")
        .or_else(|| string_arg(args, "kind"))
        .map(|value| value.trim().to_ascii_lowercase());
    let semantic_role = string_arg(args, "semantic_role")
        .or_else(|| string_arg(args, "role"))
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());
    let requested_tags = string_array_arg(args, "tags")
        .into_iter()
        .map(|tag| tag.trim().to_ascii_lowercase())
        .filter(|tag| !tag.is_empty())
        .collect::<Vec<_>>();
    let match_all_tags = bool_arg(args, "match_all_tags", true);
    let selected_only = bool_arg(args, "selected_only", false);
    let include_hidden = bool_arg(args, "include_hidden", true);
    let selected_ids: HashSet<SceneNodeId> = selected.iter().copied().collect();
    let root_target = string_arg(args, "root").or_else(|| string_arg(args, "scope"));
    let root = root_target.and_then(|target| resolve_target(scene, target));
    let scope = if root_target.is_some() && root.is_none() {
        HashSet::new()
    } else {
        scoped_ids(scene, root).into_iter().collect::<HashSet<_>>()
    };
    let matches = |id: SceneNodeId, node: &SceneNode| {
        if !scope.contains(&id) {
            return false;
        }
        if !include_hidden && !node.visible {
            return false;
        }
        if selected_only && !selected_ids.contains(&id) {
            return false;
        }
        if let Some(primitive) = &primitive {
            let primitive_name = node.primitive.label().to_ascii_lowercase();
            let kind = if node.is_folder {
                "group"
            } else {
                primitive_name.as_str()
            };
            if kind != primitive.as_str() && primitive_name != primitive.as_str() {
                return false;
            }
        }
        if let Some(requested_role) = &semantic_role {
            let role = node
                .semantic_role
                .as_deref()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            if role != *requested_role && !role.starts_with(&format!("{requested_role}.")) {
                return false;
            }
        }
        if !requested_tags.is_empty() {
            let tags = node
                .tags
                .iter()
                .map(|tag| tag.trim().to_ascii_lowercase())
                .collect::<HashSet<_>>();
            let tags_match = if match_all_tags {
                requested_tags.iter().all(|tag| tags.contains(tag))
            } else {
                requested_tags.iter().any(|tag| tags.contains(tag))
            };
            if !tags_match {
                return false;
            }
        }
        query.is_empty()
            || display_name(&node.name)
                .to_ascii_lowercase()
                .contains(&query)
            || display_path(scene, id)
                .to_ascii_lowercase()
                .contains(&query)
            || node
                .semantic_role
                .as_deref()
                .unwrap_or("")
                .to_ascii_lowercase()
                .contains(&query)
            || node
                .stable_key
                .as_deref()
                .unwrap_or("")
                .to_ascii_lowercase()
                .contains(&query)
            || node
                .tags
                .iter()
                .any(|tag| tag.to_ascii_lowercase().contains(&query))
            || node
                .source_asset
                .as_deref()
                .unwrap_or("")
                .to_ascii_lowercase()
                .contains(&query)
    };
    let matching_ids: Vec<SceneNodeId> = scene
        .iter()
        .filter(|(id, node)| matches(*id, node))
        .map(|(id, _)| id)
        .collect();
    let (offset, limit) = page(args);
    let items = matching_ids
        .iter()
        .skip(offset)
        .take(limit)
        .filter_map(|id| scene_item(scene, *id, false))
        .collect::<Vec<_>>();
    let mut result = AgentObservationResult::new(
        "Scene query",
        format!(
            "{} node(s) matched; {} returned.",
            matching_ids.len(),
            items.len()
        ),
        paged_data(
            items,
            matching_ids.len(),
            offset,
            limit,
            json!({
                "query": query,
                "primitive": primitive,
                "semantic_role": semantic_role,
                "tags": requested_tags,
                "match_all_tags": match_all_tags,
                "selected_only": selected_only,
                "scope": root.and_then(|id| scene_item_reference(scene, id))
            }),
        ),
    );
    if root_target.is_some() && root.is_none() {
        result
            .warnings
            .push("The requested scene query root was not found.".to_string());
        result.verification = Some(VerificationSummary {
            status: "failed".to_string(),
            checks: Vec::new(),
            failures: vec!["The requested scene query root was not found.".to_string()],
        });
    }
    result
}

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
        .filter_map(|id| scene_item(scene, *id, false))
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

pub fn scene_inspect(
    scene: &SceneGraph,
    args: &Value,
    selected: &[SceneNodeId],
) -> AgentObservationResult {
    let target = string_arg(args, "target")
        .or_else(|| string_arg(args, "entity"))
        .or_else(|| string_arg(args, "uuid"));
    let id = target
        .as_deref()
        .and_then(|target| resolve_target(scene, target))
        .or_else(|| selected.first().copied());
    let Some(id) = id else {
        let mut result = AgentObservationResult::new(
            "Scene inspect",
            "No entity target was provided.",
            json!({"found": false, "target": target}),
        );
        result.warnings.push(
            "Provide target as ref, uuid, path or exact name; a selected entity is used as fallback."
                .to_string(),
        );
        return result;
    };
    let Some(item) = scene_item(scene, id, true) else {
        return AgentObservationResult::new(
            "Scene inspect",
            "The requested entity is no longer live.",
            json!({"found": false, "target": target}),
        );
    };
    AgentObservationResult::new(
        "Scene inspect",
        format!("Inspected {}.", item["name"].as_str().unwrap_or("entity")),
        json!({"found": true, "entity": item}),
    )
}

pub fn selection_info(scene: &SceneGraph, selected: &[SceneNodeId]) -> AgentObservationResult {
    let items = selected
        .iter()
        .copied()
        .take(MAX_PAGE_SIZE)
        .filter_map(|id| scene_item(scene, id, true))
        .collect::<Vec<_>>();
    AgentObservationResult::new(
        "Scene selection",
        format!("{} selected scene node(s).", items.len()),
        json!({
            "count": items.len(),
            "items": items,
            "identity": "uuid-first; use ref for mutations",
        }),
    )
}

pub fn asset_inspect(
    scene: &SceneGraph,
    assets: &[String],
    args: &Value,
    catalog_pending: bool,
    catalog_error: Option<&str>,
) -> AgentObservationResult {
    let target = string_arg(args, "target")
        .or_else(|| string_arg(args, "path"))
        .or_else(|| string_arg(args, "asset"));
    let Some(target) = target.map(str::trim).filter(|target| !target.is_empty()) else {
        let mut result = AgentObservationResult::new(
            "Asset inspect",
            "No asset target was provided.",
            json!({"found": false}),
        );
        result
            .warnings
            .push("Provide target as an asset path.".to_string());
        return result;
    };
    let target = normalize_asset_path(target);
    let Some(path) = assets.iter().find(|path| {
        let path = normalize_asset_path(path);
        path == target || path.ends_with(&target) || target.ends_with(&path)
    }) else {
        return AgentObservationResult::new(
            "Asset inspect",
            format!("Asset '{target}' was not found in the project catalog."),
            json!({"found": false, "target": target}),
        );
    };
    let used = used_assets(scene);
    let normalized = normalize_asset_path(path);
    let is_used = used.contains(&normalized)
        || used
            .iter()
            .any(|source| normalized.ends_with(source) || source.ends_with(&normalized));
    let mut result = AgentObservationResult::new(
        "Asset inspect",
        format!("Inspected asset '{}'.", path),
        json!({
            "found": true,
            "path": path,
            "name": path.rsplit(['/', '\\']).next().unwrap_or(path),
            "kind": asset_kind(path),
            "used": is_used,
            "references": scene_asset_references(scene, path),
            "catalog_pending": catalog_pending,
        }),
    );
    if let Some(error) = catalog_error {
        result
            .warnings
            .push(format!("Asset catalog warning: {error}"));
    } else if catalog_pending {
        result
            .warnings
            .push("The asset catalog is still indexing; results may be incomplete.".to_string());
    }
    result
}

pub fn assets_catalog(
    scene: &SceneGraph,
    assets: &[String],
    args: &Value,
    catalog_pending: bool,
    catalog_error: Option<&str>,
) -> AgentObservationResult {
    let query = string_arg(args, "query")
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let usage = string_arg(args, "usage")
        .unwrap_or("all")
        .trim()
        .to_ascii_lowercase();
    let kind = string_arg(args, "kind")
        .map(str::trim)
        .filter(|kind| !kind.is_empty())
        .map(str::to_ascii_lowercase);
    let used = used_assets(scene);
    let mut entries = assets
        .iter()
        .filter(|path| {
            let normalized = normalize_asset_path(path);
            let is_used = used.contains(&normalized)
                || used
                    .iter()
                    .any(|source| normalized.ends_with(source) || source.ends_with(&normalized));
            let usage_matches = match usage.as_str() {
                "used" => is_used,
                "unused" => !is_used,
                _ => true,
            };
            let kind_matches = kind
                .as_deref()
                .map(|requested| asset_kind_matches(path, requested))
                .unwrap_or(true);
            usage_matches && kind_matches && (query.is_empty() || normalized.contains(&query))
        })
        .map(|path| {
            let normalized = normalize_asset_path(path);
            let is_used = used.contains(&normalized)
                || used
                    .iter()
                    .any(|source| normalized.ends_with(source) || source.ends_with(&normalized));
            json!({
            "path": path,
            "name": path.rsplit(['/', '\\']).next().unwrap_or(path),
            "kind": asset_kind(path),
                "used": is_used,
                "references": scene_asset_references(scene, path),
            })
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left["path"].to_string().cmp(&right["path"].to_string()));
    let total = entries.len();
    let (offset, limit) = page(args);
    let items = entries
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    let mut result = AgentObservationResult::new(
        "Assets catalog",
        format!("{} asset(s) matched; {} returned.", total, items.len()),
        paged_data(
            items,
            total,
            offset,
            limit,
            json!({"query": query, "usage": usage, "kind": kind, "catalog_pending": catalog_pending}),
        ),
    );
    if let Some(error) = catalog_error {
        result
            .warnings
            .push(format!("Asset catalog warning: {error}"));
    } else if catalog_pending {
        result
            .warnings
            .push("The asset catalog is still indexing; results may be incomplete.".to_string());
    }
    result
}

/// Rank imported assets for an authoring intent without invoking a model or
/// crawling workspace internals. This remains a transparent filename/type
/// heuristic and reports its reasons so the Agent can decide whether to use a
/// real asset or fall back to procedural primitives.
pub fn assets_recommend(
    scene: &SceneGraph,
    assets: &[String],
    args: &Value,
    catalog_pending: bool,
    catalog_error: Option<&str>,
) -> AgentObservationResult {
    let intent = string_arg(args, "intent").unwrap_or("").trim();
    let preferred_kind = string_arg(args, "kind")
        .map(str::trim)
        .filter(|kind| !kind.is_empty())
        .map(str::to_ascii_lowercase);
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(8)
        .clamp(1, 24) as usize;
    let mut tokens = semantic_tokens(intent);
    for (source, related) in [
        ("shelf", &["rack", "cabinet", "display", "estante"][..]),
        ("estante", &["shelf", "rack", "gondola"][..]),
        ("store", &["shop", "market", "retail", "tienda"][..]),
        ("tienda", &["store", "shop", "market", "retail"][..]),
        ("wall", &["pared", "panel"][..]),
        ("pared", &["wall", "panel"][..]),
    ] {
        if tokens.iter().any(|token| token == source) {
            tokens.extend(related.iter().map(|value| (*value).to_string()));
        }
    }
    tokens.sort();
    tokens.dedup();
    let used = used_assets(scene);
    let mut ranked = assets
        .iter()
        .filter_map(|path| {
            let normalized = normalize_asset_path(path);
            let kind = asset_kind(path);
            let is_used = used.contains(&normalized)
                || used
                    .iter()
                    .any(|source| normalized.ends_with(source) || source.ends_with(&normalized));
            let mut score = 0i32;
            let mut reasons = Vec::new();
            for token in &tokens {
                if normalized.contains(token) {
                    score += 10;
                    reasons.push(format!("name/path matches '{token}'"));
                }
            }
            if preferred_kind
                .as_deref()
                .is_some_and(|preferred| preferred == kind)
            {
                score += 8;
                reasons.push(format!("preferred kind '{kind}'"));
            }
            if !is_used {
                score += 1;
                reasons.push("currently unused".to_string());
            }
            (score > 0 || tokens.is_empty()).then(|| {
                json!({
                    "path": path,
                    "name": path.rsplit(['/', '\\']).next().unwrap_or(path),
                    "kind": kind,
                    "used": is_used,
                    "score": score,
                    "reasons": reasons,
                    "references": scene_asset_references(scene, path)
                })
            })
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right["score"]
            .as_i64()
            .cmp(&left["score"].as_i64())
            .then_with(|| left["path"].as_str().cmp(&right["path"].as_str()))
    });
    ranked.truncate(limit);
    let mut result = AgentObservationResult::new(
        "Asset recommendations",
        format!(
            "Recommended {} imported asset(s) for '{}'.",
            ranked.len(),
            intent
        ),
        json!({
            "intent": intent,
            "kind": preferred_kind,
            "items": ranked,
            "catalog_pending": catalog_pending,
            "strategy": "transparent filename, semantic-token and asset-kind ranking"
        }),
    );
    if let Some(error) = catalog_error {
        result
            .warnings
            .push(format!("Asset catalog warning: {error}"));
    } else if catalog_pending {
        result.warnings.push(
            "The asset catalog is still indexing; recommendations may be incomplete.".to_string(),
        );
    }
    result
}

fn semantic_tokens(value: &str) -> Vec<String> {
    value
        .split(|character: char| !character.is_alphanumeric())
        .map(str::trim)
        .filter(|token| token.chars().count() >= 3)
        .map(str::to_ascii_lowercase)
        .collect()
}

pub fn scripts_catalog(
    scene: &SceneGraph,
    assets: &[String],
    args: &Value,
) -> AgentObservationResult {
    let query = string_arg(args, "query")
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let mut references: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for asset in assets.iter().filter(|asset| is_script_path(asset)) {
        references.entry(asset.clone()).or_default();
    }
    for (id, node) in scene.iter() {
        for script in &node.scripts {
            references
                .entry(script.clone())
                .or_default()
                .push(scene_item_reference(scene, id).unwrap_or(Value::Null));
        }
    }
    let entries = references
        .into_iter()
        .filter(|(path, _)| query.is_empty() || path.to_ascii_lowercase().contains(&query))
        .map(|(path, entities)| {
            json!({"path": path, "name": path.rsplit(['/', '\\']).next().unwrap_or(&path), "attached_to": entities})
        })
        .collect::<Vec<_>>();
    let total = entries.len();
    let (offset, limit) = page(args);
    let items = entries
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    AgentObservationResult::new(
        "Scripts catalog",
        format!("{} script(s) matched; {} returned.", total, items.len()),
        paged_data(items, total, offset, limit, json!({"query": query})),
    )
}

pub fn project_health(scene: &SceneGraph, assets: &[String]) -> AgentObservationResult {
    project_health_scoped(scene, assets, &Value::Null)
}

/// Check only the requested scene scope. Agents should use `root` when a
/// project contains legacy or archived scene roots; this keeps unrelated
/// diagnostics out of the current authoring task.
pub fn project_health_scoped(
    scene: &SceneGraph,
    assets: &[String],
    args: &Value,
) -> AgentObservationResult {
    let root_target = string_arg(args, "root").or_else(|| string_arg(args, "scope"));
    let strict = bool_arg(args, "strict", false);
    let root = root_target.and_then(|target| resolve_target(scene, target));
    let mut scope_warning = None;
    if root_target.is_some() && root.is_none() {
        scope_warning = Some("The requested health scope was not found.".to_string());
    }
    let live = if root_target.is_some() && root.is_none() {
        Vec::new()
    } else {
        scoped_ids(scene, root)
    };
    let live_set = live.iter().copied().collect::<HashSet<_>>();
    let mut duplicate_names = BTreeMap::<String, Vec<Value>>::new();
    let mut missing_assets = Vec::new();
    let mut missing_scripts = Vec::new();
    let mut sanitized_names = Vec::new();
    let asset_paths = assets
        .iter()
        .map(|path| normalize_asset_path(path))
        .collect::<Vec<_>>();
    for id in &live {
        let Some(node) = scene.get(*id) else { continue };
        let presentation = presentation_name(*id, node);
        if presentation != node.name {
            sanitized_names.push(scene_item_reference(scene, *id).unwrap_or(Value::Null));
        }
        duplicate_names
            .entry(presentation.to_ascii_lowercase())
            .or_default()
            .push(scene_item_reference(scene, *id).unwrap_or(Value::Null));
        if let Some(source) = &node.source_asset {
            let source_lower = normalize_asset_path(source);
            if !source_lower.starts_with("builtin://")
                && !asset_paths.iter().any(|asset| {
                    asset == &source_lower
                        || asset.ends_with(&source_lower)
                        || source_lower.ends_with(asset)
                })
            {
                missing_assets
                    .push(json!({"entity": scene_item_reference(scene, *id), "asset": source}));
            }
        }
        for script in &node.scripts {
            let script_lower = normalize_asset_path(script);
            if !asset_paths.iter().any(|asset| {
                asset == &script_lower
                    || asset.ends_with(&script_lower)
                    || script_lower.ends_with(asset)
            }) {
                missing_scripts
                    .push(json!({"entity": scene_item_reference(scene, *id), "script": script}));
            }
        }
    }
    let duplicate_names = duplicate_names
        .into_iter()
        .filter(|(_, entities)| entities.len() > 1)
        .map(|(name, entities)| json!({"name": name, "entities": entities}))
        .collect::<Vec<_>>();
    let orphaned = if root_target.is_some() {
        orphaned_entities_in_scope(scene, &live_set)
    } else {
        orphaned_entities(scene)
    };
    let mut failures = Vec::new();
    if let Some(warning) = &scope_warning {
        failures.push(warning.clone());
    }
    if !duplicate_names.is_empty() {
        failures.push(format!(
            "{} duplicate name group(s).",
            duplicate_names.len()
        ));
    }
    if !missing_assets.is_empty() {
        failures.push(format!(
            "{} missing asset reference(s).",
            missing_assets.len()
        ));
    }
    if !missing_scripts.is_empty() {
        failures.push(format!(
            "{} missing script reference(s).",
            missing_scripts.len()
        ));
    }
    if !orphaned.is_empty() {
        failures.push(format!("{} orphaned scene node(s).", orphaned.len()));
    }
    let sanitized_message = (!sanitized_names.is_empty()).then(|| {
        format!(
            "{} scene name(s) require a sanitized presentation label.",
            sanitized_names.len()
        )
    });
    if strict {
        if let Some(message) = &sanitized_message {
            failures.push(message.clone());
        }
    }
    let status = if failures.is_empty() {
        "passed"
    } else {
        "failed"
    };
    let mut result = AgentObservationResult::new(
        "Project health",
        if failures.is_empty() {
            format!("Project health passed for {} scene node(s).", live.len())
        } else {
            format!("Project health found {} issue group(s).", failures.len())
        },
        json!({
            "scope": root.and_then(|id| scene_item_reference(scene, id)),
            "strict": strict,
            "scene_entities": live.len(),
            "duplicate_names": duplicate_names,
            "missing_assets": missing_assets,
            "missing_scripts": missing_scripts,
            "sanitized_names": sanitized_names,
            "orphaned_entities": orphaned,
            "checked": ["scene identity", "hierarchy reachability", "asset references", "script references", "display names"],
        }),
    );
    if let Some(warning) = scope_warning {
        result.warnings.push(warning);
    }
    if !strict {
        if let Some(message) = sanitized_message {
            result.warnings.push(message);
        }
    }
    result.verification = Some(VerificationSummary {
        status: status.to_string(),
        checks: vec![
            format!(
                "{} live scene node(s) counted from the shared graph.",
                live.len()
            ),
            "Entity references use UUID and path fallback.".to_string(),
            format!("Health scope contains {} live node(s).", live.len()),
        ],
        failures,
    });
    result
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

fn page(args: &Value) -> (usize, usize) {
    let offset = args
        .get("cursor")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .min(usize::MAX as u64) as usize;
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_PAGE_SIZE as u64)
        .clamp(1, MAX_PAGE_SIZE as u64) as usize;
    (offset, limit)
}

fn paged_data(
    items: Vec<Value>,
    total: usize,
    offset: usize,
    limit: usize,
    mut extra: Value,
) -> Value {
    let returned = items.len();
    let next_cursor = (offset + returned < total).then_some(offset + returned);
    let object = extra.as_object_mut().unwrap_or_else(|| unreachable!());
    object.insert("items".to_string(), Value::Array(items));
    object.insert("total".to_string(), json!(total));
    object.insert("returned".to_string(), json!(returned));
    object.insert("cursor".to_string(), json!(offset));
    object.insert("next_cursor".to_string(), json!(next_cursor));
    object.insert("has_more".to_string(), json!(next_cursor.is_some()));
    object.insert("page_size".to_string(), json!(limit));
    extra
}

fn string_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

fn string_array_arg(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn bool_arg(args: &Value, key: &str, default: bool) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn scene_item(scene: &SceneGraph, id: SceneNodeId, include_details: bool) -> Option<Value> {
    if !scene.is_valid_node(id) {
        return None;
    }
    let node = scene.get(id)?;
    let world_position = scene.world_matrix(id).col(3).truncate();
    let depth = hierarchy_depth(scene, id);
    let mut item = Map::new();
    item.insert("id".to_string(), json!(id.0));
    item.insert("uuid".to_string(), json!(node.uuid));
    item.insert("ref".to_string(), json!(format!("entity:{}", node.uuid)));
    item.insert("name".to_string(), json!(presentation_name(id, node)));
    item.insert("path".to_string(), json!(display_path(scene, id)));
    item.insert("depth".to_string(), json!(depth));
    item.insert(
        "parent".to_string(),
        node.parent
            .and_then(|parent| scene_item_reference(scene, parent))
            .unwrap_or(Value::Null),
    );
    let children = node
        .children
        .iter()
        .take(MAX_PAGE_SIZE)
        .filter_map(|child| scene_item_reference(scene, *child))
        .collect::<Vec<_>>();
    item.insert("children".to_string(), json!(children));
    item.insert("children_count".to_string(), json!(node.children.len()));
    item.insert(
        "children_truncated".to_string(),
        json!(node.children.len() > MAX_PAGE_SIZE),
    );
    item.insert("kind".to_string(), json!(node_kind(node)));
    item.insert("primitive".to_string(), json!(node.primitive.label()));
    item.insert("is_folder".to_string(), json!(node.is_folder));
    item.insert("visible".to_string(), json!(node.visible));
    item.insert("locked".to_string(), json!(node.locked));
    item.insert("semantic_role".to_string(), json!(node.semantic_role));
    item.insert("stable_key".to_string(), json!(node.stable_key));
    item.insert("tags".to_string(), json!(node.tags));
    item.insert("agent_origin".to_string(), json!(node.agent_origin));
    item.insert(
        "position".to_string(),
        json!([node.position.x, node.position.y, node.position.z]),
    );
    item.insert(
        "rotation_deg".to_string(),
        json!([node.rotation.x, node.rotation.y, node.rotation.z]),
    );
    item.insert(
        "scale".to_string(),
        json!([node.scale.x, node.scale.y, node.scale.z]),
    );
    if include_details {
        item.insert(
            "world_position".to_string(),
            json!([world_position.x, world_position.y, world_position.z]),
        );
        if let Some((min, max)) = world_bounds(scene, id) {
            item.insert(
                "world_bounds".to_string(),
                json!({
                    "min": [min.x, min.y, min.z],
                    "max": [max.x, max.y, max.z],
                    "size": [max.x - min.x, max.y - min.y, max.z - min.z]
                }),
            );
        }
        item.insert(
            "color_rgba".to_string(),
            json!([node.color.r, node.color.g, node.color.b, node.color.a]),
        );
        item.insert("source_asset".to_string(), json!(node.source_asset));
        item.insert("scripts".to_string(), json!(node.scripts));
        item.insert(
            "name_sanitized".to_string(),
            json!(presentation_name(id, node) != node.name),
        );
    }
    Some(Value::Object(item))
}

fn scene_item_reference(scene: &SceneGraph, id: SceneNodeId) -> Option<Value> {
    if !scene.is_valid_node(id) {
        return None;
    }
    let node = scene.get(id)?;
    Some(json!({
        "id": id.0,
        "uuid": node.uuid,
        "ref": format!("entity:{}", node.uuid),
        "name": presentation_name(id, node),
        "path": display_path(scene, id),
        "semantic_role": node.semantic_role,
        "stable_key": node.stable_key,
        "tags": node.tags,
    }))
}

/// Resolve the stable entity references accepted by Agent-facing tools.
/// Names and paths are presentation fallbacks; UUID/ref remains canonical.
pub fn resolve_target(scene: &SceneGraph, target: &str) -> Option<SceneNodeId> {
    let target = target.trim();
    let stable_key = target.strip_prefix("key:").unwrap_or(target);
    if let Some((id, _)) = scene.iter().find(|(id, node)| {
        scene.is_valid_node(*id)
            && node
                .stable_key
                .as_deref()
                .is_some_and(|key| key.eq_ignore_ascii_case(stable_key))
    }) {
        return Some(id);
    }
    if let Some(raw_id) = target.strip_prefix("entity:") {
        if let Ok(uuid) = uuid::Uuid::parse_str(raw_id) {
            return scene
                .iter()
                .find(|(id, node)| scene.is_valid_node(*id) && node.uuid == uuid)
                .map(|(id, _)| id);
        }
    }
    if let Ok(id) = target.parse::<usize>() {
        let id = SceneNodeId(id);
        if scene.is_valid_node(id) {
            return Some(id);
        }
    }
    if let Ok(uuid) = uuid::Uuid::parse_str(target) {
        if let Some((id, _)) = scene
            .iter()
            .find(|(id, node)| scene.is_valid_node(*id) && node.uuid == uuid)
        {
            return Some(id);
        }
    }
    let normalized_path = target.trim_matches('/').to_ascii_lowercase();
    if let Some((id, _)) = scene.iter().find(|(id, _)| {
        scene.is_valid_node(*id)
            && display_path(scene, *id)
                .trim_matches('/')
                .to_ascii_lowercase()
                == normalized_path
    }) {
        return Some(id);
    }
    let normalized_name = display_name(target).to_ascii_lowercase();
    scene
        .iter()
        .find(|(id, node)| {
            scene.is_valid_node(*id)
                && presentation_name(*id, node).to_ascii_lowercase() == normalized_name
        })
        .map(|(id, _)| id)
}

fn hierarchy_depth(scene: &SceneGraph, id: SceneNodeId) -> usize {
    let mut depth = 0;
    let mut current = scene.get(id).and_then(|node| node.parent);
    let mut seen = HashSet::new();
    while let Some(parent) = current {
        if !seen.insert(parent) {
            break;
        }
        depth += 1;
        current = scene.get(parent).and_then(|node| node.parent);
    }
    depth
}

fn is_ancestor(scene: &SceneGraph, ancestor: SceneNodeId, node: SceneNodeId) -> bool {
    let mut current = scene.get(node).and_then(|item| item.parent);
    let mut visited = HashSet::new();
    while let Some(parent) = current {
        if !visited.insert(parent) {
            return false;
        }
        if parent == ancestor {
            return true;
        }
        current = scene.get(parent).and_then(|item| item.parent);
    }
    false
}

fn scoped_ids(scene: &SceneGraph, root: Option<SceneNodeId>) -> Vec<SceneNodeId> {
    match root {
        Some(root) => descendants_including(scene, root),
        None => scene.all_live_ids(),
    }
}

fn scoped_ids_to_depth(
    scene: &SceneGraph,
    root: Option<SceneNodeId>,
    max_depth: Option<usize>,
) -> Vec<SceneNodeId> {
    let Some(max_depth) = max_depth else {
        return scoped_ids(scene, root);
    };
    match root {
        Some(root) => descendants_to_depth(scene, root, max_depth),
        None => scene
            .all_live_ids()
            .into_iter()
            .filter(|id| hierarchy_depth(scene, *id) <= max_depth)
            .collect(),
    }
}

fn descendants_including(scene: &SceneGraph, root: SceneNodeId) -> Vec<SceneNodeId> {
    let mut result = Vec::new();
    let mut stack = vec![root];
    let mut visited = HashSet::new();
    while let Some(id) = stack.pop() {
        if !visited.insert(id) || !scene.is_valid_node(id) {
            continue;
        }
        result.push(id);
        if let Some(node) = scene.get(id) {
            stack.extend(node.children.iter().rev().copied());
        }
    }
    result
}

fn descendants_to_depth(
    scene: &SceneGraph,
    root: SceneNodeId,
    max_depth: usize,
) -> Vec<SceneNodeId> {
    let mut result = Vec::new();
    let mut stack = vec![(root, 0usize)];
    let mut visited = HashSet::new();
    while let Some((id, depth)) = stack.pop() {
        if !visited.insert(id) || !scene.is_valid_node(id) {
            continue;
        }
        result.push(id);
        if depth >= max_depth {
            continue;
        }
        if let Some(node) = scene.get(id) {
            stack.extend(
                node.children
                    .iter()
                    .rev()
                    .copied()
                    .map(|child| (child, depth + 1)),
            );
        }
    }
    result
}

/// Compute a conservative world-space AABB for a node and its descendants.
/// Primitive meshes are unit-sized in the semantic graph, so this remains
/// useful to an agent without loading renderer mesh data.
pub fn world_bounds(scene: &SceneGraph, id: SceneNodeId) -> Option<(Vec3, Vec3)> {
    fn visit(
        scene: &SceneGraph,
        id: SceneNodeId,
        visited: &mut HashSet<SceneNodeId>,
        bounds: &mut Option<(Vec3, Vec3)>,
    ) {
        if !visited.insert(id) {
            return;
        }
        let Some(node) = scene.get(id) else { return };
        if !node.is_folder && node.primitive != Primitive::Empty {
            let half_extents = match node.primitive {
                Primitive::Plane => Vec3::new(0.5, 0.01, 0.5),
                _ => Vec3::splat(0.5),
            };
            let world = scene.world_matrix(id);
            for x in [-half_extents.x, half_extents.x] {
                for y in [-half_extents.y, half_extents.y] {
                    for z in [-half_extents.z, half_extents.z] {
                        let point = world.transform_point3(Vec3::new(x, y, z));
                        if !point.is_finite() {
                            continue;
                        }
                        if let Some((min, max)) = bounds.as_mut() {
                            *min = min.min(point);
                            *max = max.max(point);
                        } else {
                            *bounds = Some((point, point));
                        }
                    }
                }
            }
        }
        for child in &node.children {
            visit(scene, *child, visited, bounds);
        }
    }

    let mut visited = HashSet::new();
    let mut bounds = None;
    visit(scene, id, &mut visited, &mut bounds);
    bounds
}

fn aabb_overlaps(left: (Vec3, Vec3), right: (Vec3, Vec3)) -> bool {
    const EPSILON: f32 = 0.0001;
    (left.0.x < right.1.x - EPSILON && left.1.x > right.0.x + EPSILON)
        && (left.0.y < right.1.y - EPSILON && left.1.y > right.0.y + EPSILON)
        && (left.0.z < right.1.z - EPSILON && left.1.z > right.0.z + EPSILON)
}

fn merge_bounds(target: &mut Option<(Vec3, Vec3)>, bounds: (Vec3, Vec3)) {
    if let Some((min, max)) = target.as_mut() {
        *min = min.min(bounds.0);
        *max = max.max(bounds.1);
    } else {
        *target = Some(bounds);
    }
}

fn bounds_value(bounds: (Vec3, Vec3)) -> Value {
    json!({
        "min": [bounds.0.x, bounds.0.y, bounds.0.z],
        "max": [bounds.1.x, bounds.1.y, bounds.1.z],
        "size": [
            bounds.1.x - bounds.0.x,
            bounds.1.y - bounds.0.y,
            bounds.1.z - bounds.0.z
        ]
    })
}

/// Return a safe, human-readable path with control characters removed.
pub fn display_path(scene: &SceneGraph, id: SceneNodeId) -> String {
    let mut segments = Vec::new();
    let mut current = Some(id);
    let mut seen = HashSet::new();
    while let Some(current_id) = current {
        if !seen.insert(current_id) {
            break;
        }
        let Some(node) = scene.get(current_id) else {
            break;
        };
        segments.push(presentation_name(current_id, node));
        current = node.parent;
    }
    segments.reverse();
    if segments.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", segments.join("/"))
    }
}

/// Return a stable display label without mutating the scene. Control-only or
/// blank legacy names remain addressable by UUID/ref, while the fallback keeps
/// counts, paths, queries and diagnostics internally consistent.
fn presentation_name(id: SceneNodeId, node: &SceneNode) -> String {
    let cleaned = display_name(&node.name);
    if cleaned.is_empty() {
        format!("Entity_{}", id.0)
    } else {
        cleaned
    }
}

/// Remove control characters from names before they cross an Agent/CLI/MCP
/// boundary. Existing scene data is not mutated by this presentation helper.
pub fn display_name(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>()
        .trim()
        .to_string()
}

fn node_kind(node: &SceneNode) -> &'static str {
    if node.is_folder {
        "group"
    } else {
        match node.primitive {
            Primitive::Empty => "empty",
            Primitive::Cube => "cube",
            Primitive::Sphere => "sphere",
            Primitive::Plane => "plane",
            Primitive::Cylinder => "cylinder",
        }
    }
}

fn asset_kind(path: &str) -> &'static str {
    if is_script_path(path) {
        return "script";
    }
    match path
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") | Some("jpg") | Some("jpeg") | Some("webp") | Some("gif") => "image",
        Some("glb") | Some("gltf") | Some("obj") | Some("fbx") => "model",
        Some("wav") | Some("mp3") | Some("ogg") => "audio",
        Some("ron") | Some("json") | Some("toml") => "data",
        _ => "file",
    }
}

fn asset_kind_matches(path: &str, requested: &str) -> bool {
    let requested = match requested {
        "texture" => "image",
        "other" => "file",
        value => value,
    };
    asset_kind(path) == requested
}

fn is_script_path(path: &str) -> bool {
    matches!(
        path.rsplit_once('.')
            .map(|(_, extension)| extension.to_ascii_lowercase())
            .as_deref(),
        Some("rhai") | Some("rs") | Some("lua") | Some("js") | Some("ts") | Some("cpp")
    )
}

fn normalize_asset_path(path: &str) -> String {
    path.trim()
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_ascii_lowercase()
}

fn used_assets(scene: &SceneGraph) -> BTreeSet<String> {
    scene
        .iter()
        .filter_map(|(_, node)| node.source_asset.as_ref())
        .map(|asset| normalize_asset_path(asset))
        .collect()
}

fn scene_asset_references(scene: &SceneGraph, asset: &str) -> Vec<Value> {
    let asset = normalize_asset_path(asset);
    scene
        .iter()
        .filter(|(_, node)| {
            node.source_asset.as_deref().is_some_and(|source| {
                let source = normalize_asset_path(source);
                source == asset || source.ends_with(&asset) || asset.ends_with(&source)
            })
        })
        .filter_map(|(id, _)| scene_item_reference(scene, id))
        .collect()
}

fn orphaned_entities(scene: &SceneGraph) -> Vec<Value> {
    let live = scene.all_live_ids().into_iter().collect::<HashSet<_>>();
    orphaned_entities_in_scope(scene, &live)
}

fn orphaned_entities_in_scope(scene: &SceneGraph, scope: &HashSet<SceneNodeId>) -> Vec<Value> {
    let mut reachable = HashSet::new();
    let mut stack = scene
        .roots()
        .iter()
        .copied()
        .filter(|id| scene.get(*id).is_some())
        .collect::<Vec<_>>();
    while let Some(id) = stack.pop() {
        if !reachable.insert(id) {
            continue;
        }
        if let Some(node) = scene.get(id) {
            stack.extend(
                node.children
                    .iter()
                    .copied()
                    .filter(|child| scene.get(*child).is_some()),
            );
        }
    }
    scene
        .all_live_ids()
        .into_iter()
        .filter(|id| scope.contains(id))
        .filter(|id| !reachable.contains(id))
        .filter_map(|id| scene_item_reference(scene, id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Primitive;

    #[test]
    fn scene_context_uses_one_live_count_and_sanitizes_display_names() {
        let mut scene = SceneGraph::new();
        let root = scene.add_root_with_primitive("Store", Primitive::Empty);
        let child = scene.add_child_with_primitive(root, "Shelf\u{0008}", Primitive::Cube);
        let context = scene_context(&scene, &[child]);
        assert_eq!(context["entity_count"], 2);
        assert_eq!(context["outline_count"], 2);
        assert_eq!(context["selected"][0]["name"], "Shelf");
        assert_eq!(context["outline"][1]["path"], "/Store/Shelf");
    }

    #[test]
    fn broken_names_remain_visible_and_addressable_without_mutating_scene() {
        let mut scene = SceneGraph::new();
        let root = scene.add_root_folder("Store");
        let unnamed = scene.add_child_with_primitive(root, "\u{0008}", Primitive::Cube);
        let child = scene.add_child_with_primitive(unnamed, "Shelf", Primitive::Cube);

        let context = scene_context(&scene, &[]);
        assert_eq!(context["entity_count"], 3);
        assert_eq!(context["outline_count"], 3);
        assert_eq!(
            context["outline"][1]["name"],
            json!(format!("Entity_{}", unnamed.0))
        );
        assert_eq!(
            context["outline"][2]["path"],
            json!(format!("/Store/Entity_{}/Shelf", unnamed.0))
        );
        assert_eq!(
            resolve_target(
                &scene,
                &format!("entity:{}", scene.get(unnamed).unwrap().uuid)
            ),
            Some(unnamed)
        );
        assert!(world_bounds(&scene, unnamed).is_some());
        assert_eq!(scene.get(unnamed).unwrap().name, "\u{0008}");
        assert_eq!(scene.get(child).unwrap().name, "Shelf");
    }

    #[test]
    fn scene_query_is_bounded_and_resolves_clean_names() {
        let mut scene = SceneGraph::new();
        scene.add_root_with_primitive("Shelf_Left", Primitive::Cube);
        scene.add_root_with_primitive("Shelf_Right", Primitive::Cube);
        let result = scene_query(&scene, &json!({"query": "shelf", "limit": 1}), &[]);
        assert_eq!(result.data["total"], 2);
        assert_eq!(result.data["returned"], 1);
        assert_eq!(result.data["has_more"], true);
    }

    #[test]
    fn scene_query_filters_semantic_roles_and_exact_tags() {
        let mut scene = SceneGraph::new();
        let shelf = scene.add_root_with_primitive("Shelf", Primitive::Cube);
        let product = scene.add_root_with_primitive("Product", Primitive::Cube);
        scene.get_mut(shelf).unwrap().semantic_role = Some("store.shelf".to_string());
        scene.get_mut(shelf).unwrap().tags = vec!["bodega".to_string(), "fixture".to_string()];
        scene.get_mut(product).unwrap().semantic_role = Some("store.product".to_string());
        scene.get_mut(product).unwrap().tags = vec!["bodega".to_string()];

        let result = scene_query(
            &scene,
            &json!({"semantic_role":"store.shelf", "tags":["bodega", "fixture"]}),
            &[],
        );

        assert_eq!(result.data["total"], 1);
        assert_eq!(result.data["items"][0]["name"], "Shelf");
    }

    #[test]
    fn scene_outline_honors_requested_hierarchy_depth() {
        let mut scene = SceneGraph::new();
        let root = scene.add_root_folder("Store");
        let aisle = scene.add_child_folder(root, "Aisles");
        scene.add_child_with_primitive(aisle, "Shelf", Primitive::Cube);

        let result = scene_outline(&scene, &json!({"root": "Store", "depth": 1}), &[]);

        assert_eq!(result.data["total"], 2);
        assert_eq!(result.data["items"][0]["name"], "Store");
        assert_eq!(result.data["items"][1]["name"], "Aisles");
    }

    #[test]
    fn spatial_map_reports_extents_and_renderable_overlaps() {
        let mut scene = SceneGraph::new();
        let root = scene.add_root_folder("Store");
        let floor = scene.add_child_with_primitive(root, "Floor", Primitive::Cube);
        let shelf = scene.add_child_with_primitive(root, "Shelf", Primitive::Cube);
        scene.get_mut(floor).unwrap().scale = Vec3::new(4.0, 0.2, 4.0);
        scene.get_mut(shelf).unwrap().position = Vec3::new(0.25, 0.0, 0.0);
        let result = scene_spatial_map(
            &scene,
            &json!({"root": "Store", "check_collisions": true}),
            &[],
        );
        assert!(result.is_success());
        assert_eq!(result.data["total"], 2);
        assert!(result.data["bounds"]["size"][0].as_f64().unwrap() > 0.0);
        assert_eq!(result.data["overlaps"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn spatial_map_does_not_report_parent_child_bounds_as_collisions() {
        let mut scene = SceneGraph::new();
        let root = scene.add_root_folder("Store");
        let container = scene.add_child_with_primitive(root, "ShelfAssembly", Primitive::Cube);
        scene.add_child_with_primitive(container, "ShelfBoard", Primitive::Cube);

        let result = scene_spatial_map(
            &scene,
            &json!({"root": "Store", "check_collisions": true}),
            &[],
        );

        assert!(result.is_success());
        assert_eq!(result.data["overlaps"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn overlap_check_returns_a_snap_repair_suggestion() {
        let mut scene = SceneGraph::new();
        scene.add_root_with_primitive("Shelf A", Primitive::Cube);
        let second = scene.add_root_with_primitive("Shelf B", Primitive::Cube);
        scene.get_mut(second).unwrap().position.x = 0.25;

        let result = scene_check_overlaps(&scene, &json!({"margin":0.1}), &[]);

        assert_eq!(result.data["overlap_count"], 1);
        assert_eq!(result.data["pairs"][0]["smallest_axis"], "x");
        assert_eq!(result.data["pairs"][0]["suggested_snap"]["axis"], "x");
    }

    #[test]
    fn design_audit_reports_missing_envelope_features() {
        let mut scene = SceneGraph::new();
        let root = scene.add_root_folder("Store");
        scene.add_child_with_primitive(root, "Floor", Primitive::Cube);
        let result = scene_design_audit(&scene, &json!({"root": "Store"}), &[]);
        assert!(!result.is_success());
        assert_eq!(result.verification.unwrap().status, "failed");
        assert!(result.data["features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|feature| feature["feature"] == "enclosure"));
    }

    #[test]
    fn verification_count_uses_explicit_targets_when_present() {
        let mut scene = SceneGraph::new();
        scene.add_root_with_primitive("Floor", Primitive::Cube);
        scene.add_root_with_primitive("Shelf", Primitive::Cube);
        let result = scene_verify(
            &scene,
            &json!({"targets": ["Shelf"], "expected_count": 1}),
            &[],
        );
        assert!(result.is_success());
        assert_eq!(result.data["expected_scope_count"], 1);
    }

    #[test]
    fn verification_compares_expected_state_in_the_shared_core() {
        let mut scene = SceneGraph::new();
        let root = scene.add_root_folder("Store");
        let shelf = scene.add_child_with_primitive(root, "Shelf", Primitive::Cube);
        scene.get_mut(shelf).unwrap().position = Vec3::new(1.0, 2.0, 3.0);
        scene.get_mut(shelf).unwrap().color = crate::scene::NodeColor::rgba(20, 40, 60, 255);

        let expected = json!({
            "targets": ["Shelf"],
            "expected": {
                "primitive": "cube",
                "transform": {"position": [1.0, 2.0, 3.0]},
                "color_rgba": [20, 40, 60, 255],
                "parent": "Store"
            }
        });
        let result = scene_verify(&scene, &expected, &[]);
        assert!(result.is_success());

        let mismatch = scene_verify(
            &scene,
            &json!({
                "targets": ["Shelf"],
                "expected": {"transform": {"position": [9.0, 2.0, 3.0]}}
            }),
            &[],
        );
        assert!(!mismatch.is_success());
        assert_eq!(mismatch.verification.unwrap().status, "failed");
    }

    #[test]
    fn scene_verification_reports_missing_targets() {
        let scene = SceneGraph::new();
        let result = scene_verify(&scene, &json!({"targets": ["Missing"]}), &[]);
        assert_eq!(result.verification.unwrap().status, "failed");
    }

    #[test]
    fn asset_catalog_and_selection_are_bounded_observations() {
        let mut scene = SceneGraph::new();
        let id = scene.add_root_with_primitive("Shelf", Primitive::Cube);
        let selection = selection_info(&scene, &[id]);
        assert_eq!(selection.data["count"], 1);
        assert_eq!(selection.data["items"][0]["name"], "Shelf");

        let assets = assets_catalog(
            &scene,
            &[
                "models/Shelf.glb".to_string(),
                "scripts/store.rhai".to_string(),
            ],
            &json!({"usage":"unused", "kind":"model"}),
            false,
            None,
        );
        assert_eq!(assets.data["items"][0]["kind"], "model");
        assert_eq!(assets.data["items"][0]["used"], false);
        assert_eq!(assets.data["total"], 1);

        let scripts = scripts_catalog(&scene, &["scripts/store.rhai".to_string()], &json!({}));
        assert_eq!(scripts.data["items"][0]["path"], "scripts/store.rhai");
    }

    #[test]
    fn asset_recommendations_explain_semantic_filename_matches() {
        let scene = SceneGraph::new();
        let result = assets_recommend(
            &scene,
            &[
                "models/supermarket_shelf.glb".to_string(),
                "audio/checkout.wav".to_string(),
            ],
            &json!({"intent":"estante de supermercado", "kind":"model"}),
            false,
            None,
        );

        assert_eq!(
            result.data["items"][0]["path"],
            "models/supermarket_shelf.glb"
        );
        assert!(result.data["items"][0]["reasons"]
            .as_array()
            .is_some_and(|reasons| !reasons.is_empty()));
    }

    #[test]
    fn compact_result_lines_removes_renderer_dump_lines() {
        let lines = vec![
            "Command executed:".to_string(),
            "title: Created shelf".to_string(),
            "mesh_indices: 36".to_string(),
            "local_vertex_0: [0, 0, 0]".to_string(),
            "changed: true".to_string(),
        ];

        assert_eq!(
            compact_result_lines(&lines),
            vec!["title: Created shelf", "changed: true"]
        );
    }
}
