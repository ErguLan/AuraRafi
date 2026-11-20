//! Project-level identity and reference health projections.

use super::assets::normalize_asset_path;
use super::scene::{presentation_name, resolve_target, scene_item_reference, scoped_ids};
use super::support::{bool_arg, string_arg};
use super::AgentObservationResult;
use crate::scene::{SceneGraph, SceneNodeId};
use crate::transaction::VerificationSummary;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};

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
    // Duplicates only matter within the same parent: unique full paths still
    // resolve unambiguously, matching the behavior of Unity/Unreal/Godot.
    let mut duplicate_names =
        BTreeMap::<(Option<usize>, String), (Option<usize>, Vec<Value>)>::new();
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
            .entry((
                node.parent.map(|parent_id| parent_id.0),
                presentation.to_ascii_lowercase(),
            ))
            .or_insert_with(|| (node.parent.map(|parent_id| parent_id.0), Vec::new()))
            .1
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
        .filter(|(_, (_, entities))| entities.len() > 1)
        .map(|((_, name), (parent, entities))| {
            json!({
                "name": name,
                "parent": parent
                    .and_then(|parent_id| scene_item_reference(scene, SceneNodeId(parent_id)))
                    .unwrap_or(Value::Null),
                "entities": entities
            })
        })
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
