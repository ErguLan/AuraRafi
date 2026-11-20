//! Scene projections: context, digest, outline, queries, inspection and identity helpers.

use super::spatial::scene_check_overlaps;
use super::support::{bool_arg, page, paged_data, string_arg, string_array_arg};
use super::{AgentObservationResult, MAX_PAGE_SIZE};
use crate::scene::{Primitive, SceneGraph, SceneNode, SceneNodeId};
use crate::transaction::VerificationSummary;
use glam::Vec3;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashSet};

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

/// One-call semantic snapshot for authoring Agents. It combines the counts,
/// hierarchy, world bounds, component coverage and optional overlap evidence
/// that otherwise force a model to chain several observation calls.
pub fn scene_digest(
    scene: &SceneGraph,
    assets: &[String],
    args: &Value,
    selected: &[SceneNodeId],
) -> AgentObservationResult {
    let (offset, limit) = page(args);
    let root_target = string_arg(args, "root").or_else(|| string_arg(args, "scope"));
    let root = root_target.and_then(|target| resolve_target(scene, target));
    let scoped = if root_target.is_some() && root.is_none() {
        Vec::new()
    } else {
        scoped_ids(scene, root)
    };
    let mut renderable = 0usize;
    let mut folders = 0usize;
    let mut hidden = 0usize;
    let mut cameras = Vec::new();
    let mut attached_scripts = 0usize;
    let mut source_assets = 0usize;
    let mut textured = 0usize;
    let mut colliders = 0usize;
    let mut rigid_bodies = 0usize;
    let mut audio_sources = 0usize;
    let mut roles: BTreeMap<String, usize> = BTreeMap::new();
    let mut tags: BTreeMap<String, usize> = BTreeMap::new();
    let mut aggregate_bounds: Option<(Vec3, Vec3)> = None;
    for id in &scoped {
        let Some(node) = scene.get(*id) else {
            continue;
        };
        if node.is_folder {
            folders += 1;
        } else if node.primitive != Primitive::Empty && node.visible {
            renderable += 1;
            if let Some(bounds) = world_bounds(scene, *id) {
                merge_bounds(&mut aggregate_bounds, bounds);
            }
        }
        if !node.visible {
            hidden += 1;
        }
        if node.game_camera.is_some() {
            cameras.push(scene_item_reference(scene, *id));
        }
        attached_scripts += node.scripts.len();
        if node.source_asset.is_some() {
            source_assets += 1;
        }
        if let Some(role) = node.semantic_role.as_deref().filter(|role| !role.is_empty()) {
            *roles.entry(role.to_string()).or_default() += 1;
        }
        for tag in &node.tags {
            *tags.entry(tag.clone()).or_default() += 1;
        }
        if node.base_color_texture.is_some() {
            textured += 1;
        }
        if node.collider.collider_type != crate::scene::ColliderType::None {
            colliders += 1;
        }
        if node.rigid_body.enabled {
            rigid_bodies += 1;
        }
        if node.audio_source.enabled {
            audio_sources += 1;
        }
    }
    let components = json!({
        "textured": textured,
        "collider": colliders,
        "rigid_body": rigid_bodies,
        "audio": audio_sources,
    });
    let total = scoped.len();
    let items = scoped
        .iter()
        .skip(offset)
        .take(limit)
        .filter_map(|id| scene_item(scene, *id, false))
        .collect::<Vec<_>>();
    let mut top_roles = roles.into_iter().collect::<Vec<_>>();
    top_roles.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
    top_roles.truncate(16);
    let mut top_tags = tags.into_iter().collect::<Vec<_>>();
    top_tags.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
    top_tags.truncate(16);
    let check_collisions = bool_arg(args, "check_collisions", false);
    let overlaps = check_collisions
        .then(|| {
            scene_check_overlaps(
                scene,
                &json!({"root": root_target, "max_pairs": 16}),
                selected,
            )
        })
        .map(|observation| {
            json!({
                "count": observation.data.get("overlap_count"),
                "pairs": observation.data.get("pairs"),
                "truncated": observation.data.get("truncated"),
            })
        });
    let mut result = AgentObservationResult::new(
        "Scene digest",
        format!(
            "{} live node(s), {} renderable, {} folder(s), {} camera(s), {} attached script(s).",
            total,
            renderable,
            folders,
            cameras.len(),
            attached_scripts
        ),
        paged_data(
            items,
            total,
            offset,
            limit,
            json!({
                "scope": root.and_then(|id| scene_item_reference(scene, id)),
                "roots": scene.roots().iter().filter_map(|id| scene_item_reference(scene, *id)).take(24).collect::<Vec<_>>(),
                "selected": selected.iter().filter_map(|id| scene_item_reference(scene, *id)).take(16).collect::<Vec<_>>(),
                "counts": {
                    "live": total,
                    "renderable": renderable,
                    "folders": folders,
                    "hidden": hidden,
                    "cameras": cameras.len(),
                    "attached_scripts": attached_scripts,
                    "entities_with_source_asset": source_assets,
                    "imported_assets": assets.len(),
                },
                "bounds": aggregate_bounds.map(bounds_value),
                "cameras": cameras,
                "components": components,
                "top_semantic_roles": top_roles.into_iter().map(|(role, count)| json!({"role": role, "count": count})).collect::<Vec<_>>(),
                "top_tags": top_tags.into_iter().map(|(tag, count)| json!({"tag": tag, "count": count})).collect::<Vec<_>>(),
                "overlaps": overlaps,
                "identity": "uuid-first; use ref or uuid for mutations",
                "hint": "Use scene_program for large worlds, then scene_commit and scene_capture.",
            }),
        ),
    );
    if root_target.is_some() && root.is_none() {
        result
            .warnings
            .push("The requested scene digest root was not found.".to_string());
        result.verification = Some(VerificationSummary {
            status: "failed".to_string(),
            checks: Vec::new(),
            failures: vec!["The requested scene digest root was not found.".to_string()],
        });
    }
    result
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

pub fn scene_inspect(
    scene: &SceneGraph,
    args: &Value,
    selected: &[SceneNodeId],
) -> AgentObservationResult {
    let mut targets = Vec::new();
    if let Some(target) = string_arg(args, "target")
        .or_else(|| string_arg(args, "entity"))
        .or_else(|| string_arg(args, "uuid"))
    {
        targets.push(target.to_string());
    }
    targets.extend(string_array_arg(args, "targets"));
    targets.retain(|target| !target.trim().is_empty());
    if targets.is_empty() {
        if let Some(id) = selected.first().copied() {
            return match scene_item(scene, id, true) {
                Some(item) => AgentObservationResult::new(
                    "Scene inspect",
                    format!("Inspected {}.", item["name"].as_str().unwrap_or("entity")),
                    json!({
                        "found": true,
                        "status": "ok",
                        "entity": item,
                        "items": [item],
                        "partial": false,
                        "unresolved_targets": Vec::<String>::new(),
                    }),
                ),
                None => AgentObservationResult::new(
                    "Scene inspect",
                    "The selected entity is no longer live.",
                    json!({"found": false, "status": "failed"}),
                ),
            };
        }
        let mut result = AgentObservationResult::new(
            "Scene inspect",
            "No entity target was provided.",
            json!({"found": false, "status": "failed", "targets": targets}),
        );
        result.warnings.push(
            "Provide target/targets as ref, uuid, path or exact name; a selected entity is used as fallback."
                .to_string(),
        );
        return result;
    }
    let mut items = Vec::new();
    let mut unresolved = Vec::new();
    for target in &targets {
        match resolve_target(scene, target) {
            Some(id) => {
                if let Some(item) = scene_item(scene, id, true) {
                    items.push(item);
                }
            }
            None => unresolved.push(target.clone()),
        }
    }
    let partial = !items.is_empty() && !unresolved.is_empty();
    let found = !items.is_empty();
    let first = items.first().cloned();
    let item_count = items.len();
    let unresolved_count = unresolved.len();
    let unresolved_text = unresolved.join(", ");
    let mut result = AgentObservationResult::new(
        "Scene inspect",
        if partial {
            format!(
                "Inspected {} entity(ies); {} target(s) were not found.",
                item_count,
                unresolved_count
            )
        } else if found {
            format!("Inspected {item_count} entity(ies).")
        } else {
            format!("None of the {unresolved_count} requested target(s) were found.")
        },
        json!({
            "found": found,
            "status": if partial { "partial" } else if found { "ok" } else { "failed" },
            "partial": partial,
            "items": items,
            "entity": first,
            "unresolved_targets": unresolved,
            "targets": targets,
        }),
    );
    if partial {
        result
            .warnings
            .push(format!("Some targets were not resolved: {unresolved_text}."));
    }
    result
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

pub(super) fn scene_item(scene: &SceneGraph, id: SceneNodeId, include_details: bool) -> Option<Value> {
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
    item.insert("has_camera".to_string(), json!(node.game_camera.is_some()));
    item.insert("is_folder".to_string(), json!(node.is_folder));
    item.insert("visible".to_string(), json!(node.visible));
    item.insert("casts_shadows".to_string(), json!(node.casts_shadows));
    item.insert("receives_shadows".to_string(), json!(node.receives_shadows));
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
    item.insert("size".to_string(), json!(node.primitive.dimensions(node.scale).to_array()));
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
        item.insert("game_camera".to_string(), json!(node.game_camera));
        item.insert(
            "name_sanitized".to_string(),
            json!(presentation_name(id, node) != node.name),
        );
    }
    Some(Value::Object(item))
}

pub(super) fn scene_item_reference(scene: &SceneGraph, id: SceneNodeId) -> Option<Value> {
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

pub(super) fn is_ancestor(scene: &SceneGraph, ancestor: SceneNodeId, node: SceneNodeId) -> bool {
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

pub(super) fn scoped_ids(scene: &SceneGraph, root: Option<SceneNodeId>) -> Vec<SceneNodeId> {
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
    crate::scene::geometry::subtree_geometry_bounds(scene, id)
}

pub(super) fn aabb_overlaps(left: (Vec3, Vec3), right: (Vec3, Vec3)) -> bool {
    const EPSILON: f32 = 0.0001;
    (left.0.x < right.1.x - EPSILON && left.1.x > right.0.x + EPSILON)
        && (left.0.y < right.1.y - EPSILON && left.1.y > right.0.y + EPSILON)
        && (left.0.z < right.1.z - EPSILON && left.1.z > right.0.z + EPSILON)
}

pub(super) fn merge_bounds(target: &mut Option<(Vec3, Vec3)>, bounds: (Vec3, Vec3)) {
    if let Some((min, max)) = target.as_mut() {
        *min = min.min(bounds.0);
        *max = max.max(bounds.1);
    } else {
        *target = Some(bounds);
    }
}

pub(super) fn bounds_value(bounds: (Vec3, Vec3)) -> Value {
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
pub(super) fn presentation_name(id: SceneNodeId, node: &SceneNode) -> String {
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

pub(super) fn node_kind(node: &SceneNode) -> &'static str {
    if node.is_folder {
        "group"
    } else {
        match node.primitive {
            Primitive::Empty => "empty",
            Primitive::Cube => "cube",
            Primitive::Sphere => "sphere",
            Primitive::Plane => "plane",
            Primitive::Cylinder => "cylinder",
            Primitive::Capsule => "capsule",
        }
    }
}
