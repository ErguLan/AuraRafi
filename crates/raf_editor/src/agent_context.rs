//! Compact, native project perception for the embedded Agent.
//!
//! These reads use the already-mounted scene and immutable worker-backed asset
//! catalog. They never crawl the workspace, mutate the document, or depend on
//! a renderer/window type.

use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use raf_ai::agent_runtime::AgentToolResult;
use raf_core::agent_context::{
    asset_inspect as core_asset_inspect, assets_catalog as core_assets_catalog,
    assets_recommend as core_assets_recommend, display_name, display_path,
    resolve_target as resolve_core_target, scene_check_overlaps as core_scene_check_overlaps,
    scripts_catalog as core_scripts_catalog, world_bounds, AgentObservationResult,
};
use raf_core::project::{Project, ProjectType};
use raf_core::scene::graph::{Primitive, SceneGraph, SceneNode, SceneNodeId};
use raf_core::{i18n::t, Language};
use serde::Serialize;
use serde_json::Value;

use crate::commands::game::SceneSelectionState;
use crate::script_support::scan_script_catalog;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AgentProjectSnapshot {
    pub project: Option<SnapshotProject>,
    pub active_session: String,
    pub revision: u64,
    pub scene: SnapshotScene,
    pub assets: SnapshotAssets,
    pub scripts: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SnapshotProject {
    pub id: String,
    pub name: String,
    pub project_type: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SnapshotScene {
    pub entities: usize,
    pub folders: usize,
    pub reachable: usize,
    pub orphaned: usize,
    pub hidden: usize,
    pub up_axis: &'static str,
    pub units: &'static str,
    pub roots: Vec<SnapshotReference>,
    pub selected: Vec<SnapshotReference>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SnapshotAssets {
    pub imported: usize,
    pub used: usize,
    pub unused: usize,
    pub catalog_pending: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SnapshotReference {
    pub id: usize,
    pub uuid: String,
    pub name: String,
    pub path: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stable_key: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

pub struct AgentObservationContext<'a> {
    pub scene: &'a SceneGraph,
    pub selection: &'a SceneSelectionState,
    pub project: Option<&'a Project>,
    pub assets: &'a [String],
    pub active_session: &'a str,
    pub revision: u64,
    pub catalog_pending: bool,
    pub catalog_error: Option<&'a str>,
}

impl AgentObservationContext<'_> {
    pub fn snapshot(&self) -> AgentProjectSnapshot {
        let used_assets = used_asset_keys(self.scene);
        let script_paths = script_paths(self.scene, self.assets);
        let live_ids = self.scene.all_live_ids();
        let reachable_ids = reachable_scene_ids(self.scene);
        let mut warnings = Vec::new();
        if self.catalog_pending {
            warnings.push("Asset catalog is refreshing; results may be incomplete.".to_string());
        }
        if let Some(error) = self.catalog_error {
            warnings.push(format!("Asset catalog: {error}"));
        }
        let orphaned_count = live_ids
            .iter()
            .filter(|id| !reachable_ids.contains(id))
            .count();
        if orphaned_count > 0 {
            warnings.push(format!(
                "{orphaned_count} live scene node(s) are not reachable from the scene roots."
            ));
        }
        AgentProjectSnapshot {
            project: self.project.map(|project| SnapshotProject {
                id: project.id.to_string(),
                name: project.name.clone(),
                project_type: project_type_name(project.project_type),
            }),
            active_session: self.active_session.to_string(),
            revision: self.revision,
            scene: SnapshotScene {
                entities: live_ids.len(),
                folders: live_ids
                    .iter()
                    .filter(|id| self.scene.get(**id).is_some_and(|node| node.is_folder))
                    .count(),
                reachable: reachable_ids.len(),
                orphaned: orphaned_count,
                hidden: live_ids
                    .iter()
                    .filter(|id| self.scene.get(**id).is_some_and(|node| !node.visible))
                    .count(),
                up_axis: "y",
                units: "engine_units",
                roots: self
                    .scene
                    .roots()
                    .iter()
                    .filter_map(|id| reference(self.scene, *id))
                    .take(24)
                    .collect(),
                selected: self
                    .selection
                    .selected_nodes
                    .iter()
                    .filter_map(|id| reference(self.scene, *id))
                    .take(16)
                    .collect(),
            },
            assets: SnapshotAssets {
                imported: self.assets.len(),
                used: self
                    .assets
                    .iter()
                    .filter(|asset| asset_is_used(asset, &used_assets))
                    .count(),
                unused: self
                    .assets
                    .iter()
                    .filter(|asset| !asset_is_used(asset, &used_assets))
                    .count(),
                catalog_pending: self.catalog_pending,
            },
            scripts: script_paths.len(),
            warnings,
        }
    }

    pub fn execute(&self, name: &str, arguments: &Value) -> Option<AgentToolResult> {
        match name {
            "project_summary" => Some(self.project_summary()),
            "scene_outline" => Some(self.scene_outline(arguments)),
            "scene_query" => Some(self.scene_query(arguments)),
            "scene_spatial_map" => Some(self.scene_spatial_map(arguments)),
            "scene_check_overlaps" => Some(self.scene_check_overlaps(arguments)),
            "scene_design_audit" => Some(self.scene_design_audit(arguments)),
            "scene_inspect" => Some(self.scene_inspect(arguments)),
            "selection_get" => Some(self.selection_get()),
            "assets_catalog" => Some(self.assets_catalog(arguments)),
            "assets_recommend" => Some(self.assets_recommend(arguments)),
            "asset_inspect" => Some(self.asset_inspect(arguments)),
            "scripts_catalog" => Some(self.scripts_catalog(arguments)),
            "project_health" => Some(self.project_health(arguments)),
            "game_completion_audit" => Some(self.game_completion_audit()),
            "scene_verify" => Some(self.scene_verify(arguments)),
            "game_validate_layout" => Some(self.validate_layout()),
            _ => None,
        }
    }

    fn project_summary(&self) -> AgentToolResult {
        let snapshot = self.snapshot();
        let summary = if let Some(project) = &snapshot.project {
            format!(
                "{} is open with {} scene entities, {} assets, and {} scripts.",
                project.name, snapshot.scene.entities, snapshot.assets.imported, snapshot.scripts
            )
        } else {
            "No project is currently open.".to_string()
        };
        AgentToolResult::success(
            summary,
            serde_json::to_value(snapshot).unwrap_or(Value::Null),
        )
    }

    fn scene_outline(&self, arguments: &Value) -> AgentToolResult {
        observation_to_tool_result(raf_core::agent_context::scene_outline(
            self.scene,
            arguments,
            &self.selection.selected_nodes,
        ))
    }

    fn scene_query(&self, arguments: &Value) -> AgentToolResult {
        // Keep native Agent and CLI/MCP query semantics identical. The old
        // editor-only implementation silently ignored root scoping and did
        // not expose `has_more`, which made large scenes hard to navigate.
        let mut normalized = arguments.clone();
        if let Some(object) = normalized.as_object_mut() {
            if !object.contains_key("selected_only") {
                if let Some(selected) = object.remove("selected") {
                    object.insert("selected_only".to_string(), selected);
                }
            }
        }
        observation_to_tool_result(raf_core::agent_context::scene_query(
            self.scene,
            &normalized,
            &self.selection.selected_nodes,
        ))
    }

    fn scene_spatial_map(&self, arguments: &Value) -> AgentToolResult {
        observation_to_tool_result(raf_core::agent_context::scene_spatial_map(
            self.scene,
            arguments,
            &self.selection.selected_nodes,
        ))
    }

    fn scene_check_overlaps(&self, arguments: &Value) -> AgentToolResult {
        observation_to_tool_result(core_scene_check_overlaps(
            self.scene,
            arguments,
            &self.selection.selected_nodes,
        ))
    }

    fn scene_design_audit(&self, arguments: &Value) -> AgentToolResult {
        observation_to_tool_result(raf_core::agent_context::scene_design_audit(
            self.scene,
            arguments,
            &self.selection.selected_nodes,
        ))
    }

    fn scene_inspect(&self, arguments: &Value) -> AgentToolResult {
        let targets = target_values(arguments);
        let ids: Vec<SceneNodeId> = if targets.is_empty() {
            self.selection
                .selected_nodes
                .iter()
                .copied()
                .filter(|id| self.scene.is_valid_node(*id))
                .collect()
        } else {
            targets
                .iter()
                .filter_map(|target| resolve_target(self.scene, self.selection, target))
                .collect()
        };
        let items = ids
            .iter()
            .copied()
            .take(32)
            .filter_map(|id| node_detail(self.scene, id))
            .collect::<Vec<_>>();
        let unresolved = targets.len().saturating_sub(ids.len());
        let mut result = AgentToolResult::success(
            if unresolved == 0 {
                format!("Inspected {} scene entities.", items.len())
            } else {
                format!(
                    "Inspected {} scene entities; {} target(s) were not found.",
                    items.len(),
                    unresolved
                )
            },
            serde_json::json!({"items": items, "unresolved": unresolved}),
        );
        if unresolved > 0 {
            result.ok = false;
            result.verification = Some(serde_json::json!({
                "status": "failed",
                "reason": "target_not_found",
                "unresolved": unresolved
            }));
        }
        result.references = ids
            .iter()
            .filter_map(|id| {
                self.scene
                    .get(*id)
                    .map(|node| format!("entity:{}", node.uuid))
            })
            .collect();
        result
    }

    fn selection_get(&self) -> AgentToolResult {
        observation_to_tool_result(raf_core::agent_context::selection_info(
            self.scene,
            &self.selection.selected_nodes,
        ))
    }

    fn assets_catalog(&self, arguments: &Value) -> AgentToolResult {
        observation_to_tool_result(core_assets_catalog(
            self.scene,
            self.assets,
            arguments,
            self.catalog_pending,
            self.catalog_error,
        ))
    }

    fn assets_recommend(&self, arguments: &Value) -> AgentToolResult {
        observation_to_tool_result(core_assets_recommend(
            self.scene,
            self.assets,
            arguments,
            self.catalog_pending,
            self.catalog_error,
        ))
    }

    fn asset_inspect(&self, arguments: &Value) -> AgentToolResult {
        observation_to_tool_result(core_asset_inspect(
            self.scene,
            self.assets,
            arguments,
            self.catalog_pending,
            self.catalog_error,
        ))
    }

    fn scripts_catalog(&self, arguments: &Value) -> AgentToolResult {
        observation_to_tool_result(core_scripts_catalog(self.scene, self.assets, arguments))
    }

    fn project_health(&self, arguments: &Value) -> AgentToolResult {
        observation_to_tool_result(raf_core::agent_context::project_health_scoped(
            self.scene,
            self.assets,
            arguments,
        ))
    }

    fn scene_verify(&self, arguments: &Value) -> AgentToolResult {
        let core_observation = raf_core::agent_context::scene_verify(
            self.scene,
            arguments,
            &self.selection.selected_nodes,
        );
        let mut result = observation_to_tool_result(core_observation);
        let targets = target_values(arguments);
        let found = if targets.is_empty() {
            self.selection
                .selected_nodes
                .iter()
                .copied()
                .filter(|id| self.scene.is_valid_node(*id))
                .collect::<Vec<_>>()
        } else {
            targets
                .iter()
                .filter_map(|target| resolve_target(self.scene, self.selection, target))
                .collect::<Vec<_>>()
        };
        if let Some(object) = result.data.as_object_mut() {
            object.insert(
                "target_details".to_string(),
                serde_json::json!(found
                    .iter()
                    .filter_map(|id| node_summary(self.scene, *id, true))
                    .collect::<Vec<_>>()),
            );
        }
        result.references = found
            .iter()
            .filter_map(|id| {
                self.scene
                    .get(*id)
                    .map(|node| format!("entity:{}", node.uuid))
            })
            .collect();
        if result.ok {
            result.summary = format!(
                "{} Target state matches the structural verification.",
                result.summary
            );
        }
        result
    }

    fn validate_layout(&self) -> AgentToolResult {
        let invalid = self
            .scene
            .iter()
            .filter(|(_, node)| {
                !node.position.is_finite()
                    || !node.rotation.is_finite()
                    || !node.scale.is_finite()
                    || node.scale.x.abs() < 0.0001
                    || node.scale.y.abs() < 0.0001
                    || node.scale.z.abs() < 0.0001
            })
            .filter_map(|(id, _)| node_summary(self.scene, id, false))
            .collect::<Vec<_>>();
        let passed = invalid.is_empty();
        let mut result = AgentToolResult::success(
            if passed {
                "Layout validation passed.".to_string()
            } else {
                format!(
                    "Layout validation found {} invalid transforms.",
                    invalid.len()
                )
            },
            serde_json::json!({"invalid_transforms": invalid}),
        );
        result.ok = passed;
        result.verification = Some(serde_json::json!({
            "status": if passed { "passed" } else { "failed" },
            "invalid_transforms": invalid.len()
        }));
        result
    }

    /// Evidence-oriented completion audit for game authoring.
    ///
    /// It does not claim that a game is fun or finished. It checks the
    /// objective foundation an Agent must not skip: visible geometry, a
    /// camera, authored lighting state, attached scripts, and gameplay
    /// signals in those scripts.
    pub(crate) fn game_completion_audit(&self) -> AgentToolResult {
        let live = self.scene.all_live_ids();
        let mut renderable = 0usize;
        let mut camera_nodes = 0usize;
        let mut attached_scripts = 0usize;
        let mut source_asset_entities = 0usize;
        for id in &live {
            let Some(node) = self.scene.get(*id) else {
                continue;
            };
            if !node.is_folder && node.visible && node.primitive != Primitive::Empty {
                renderable += 1;
            }
            if node.game_camera.is_some() {
                camera_nodes += 1;
            }
            if node.source_asset.is_some() {
                source_asset_entities += 1;
            }
            attached_scripts += node.scripts.len();
        }
        let startup_camera = self
            .project
            .and_then(|project| project.settings.runtime.active_camera)
            .and_then(|uuid| {
                live.iter().copied().find(|id| {
                    self.scene
                        .get(*id)
                        .is_some_and(|node| node.uuid == uuid && node.game_camera.is_some())
                })
            });
        let scripts = self
            .project
            .map(|project| {
                let assets = project.path.join("assets");
                let mut reports = Vec::new();
                for entry in scan_script_catalog(&assets).into_iter().take(64) {
                    let source = bounded_source(&entry.absolute_path, 64 * 1024);
                    // Templates contain commented examples. Only executable
                    // lines count as gameplay evidence.
                    let active = source
                        .lines()
                        .filter(|line| !line.trim_start().starts_with("//"))
                        .collect::<Vec<_>>()
                        .join("\n")
                        .to_ascii_lowercase();
                    let signals = GAMEPLAY_SIGNALS
                        .iter()
                        .copied()
                        .filter(|marker| active.contains(marker))
                        .collect::<Vec<_>>();
                    reports.push(serde_json::json!({
                        "path": entry.relative_path,
                        "language": entry.language.label(),
                        "has_on_start": entry.has_on_start,
                        "has_on_update": entry.has_on_update,
                        "gameplay_signals": signals,
                    }));
                }
                reports
            })
            .unwrap_or_default();
        let gameplay_scripts = scripts
            .iter()
            .filter(|script| {
                script
                    .get("gameplay_signals")
                    .and_then(Value::as_array)
                    .is_some_and(|signals| !signals.is_empty())
            })
            .count();
        let environment = self.scene.environment();
        let lighting = serde_json::json!({
            "mode": match environment.mode {
                raf_core::environment::EnvironmentMode::Neutral => "neutral",
                raf_core::environment::EnvironmentMode::Scene => "scene",
            },
            "sun_enabled": environment.sun.enabled,
            "sun_intensity": environment.sun.intensity,
            "shadows_enabled": environment.shadows.enabled,
            "ambient_intensity": environment.ambient_intensity,
        });
        let physics = self
            .project
            .map(|project| project.settings.enable_physics)
            .unwrap_or(false);
        let audio = self
            .project
            .map(|project| project.settings.enable_audio)
            .unwrap_or(false);
        let mut missing = Vec::new();
        if renderable == 0 {
            missing.push("renderable_geometry");
        }
        if startup_camera.is_none() && camera_nodes == 0 {
            missing.push("camera");
        }
        if attached_scripts == 0 {
            missing.push("attached_script");
        } else if gameplay_scripts == 0 {
            missing.push("gameplay_signal");
        }
        let verdict = if renderable == 0 {
            "empty_scene"
        } else if startup_camera.is_none() && camera_nodes == 0 {
            "needs_camera"
        } else if attached_scripts == 0 {
            "geometry_only"
        } else if gameplay_scripts == 0 {
            "scripted_without_gameplay_signals"
        } else {
            "playable_foundation"
        };
        let mut next_actions = Vec::new();
        if renderable == 0 {
            next_actions.push("Create the scene envelope and visible geometry first.");
        }
        if startup_camera.is_none() && camera_nodes == 0 {
            next_actions.push("Add a Camera entity and set it as the startup camera.");
        }
        if attached_scripts == 0 {
            next_actions.push("Write a Rhai gameplay script and attach it to the controller entity.");
        } else if gameplay_scripts == 0 {
            next_actions.push("Add input, movement, collision, scoring, or event logic to the attached script.");
        }
        if environment.mode == raf_core::environment::EnvironmentMode::Neutral {
            next_actions.push("If the game needs authored light, set lighting mode to Scene and tune Sun/Sky/Shadows.");
        }
        next_actions.push("Capture at least two angles with viewport_capture, inspect the images, and repair what the images reveal.");
        let passed = missing.is_empty();
        let summary = if passed {
            "Gameplay foundation detected: geometry, camera, and a script with gameplay signals are present. Visual review is still required.".to_string()
        } else {
            format!(
                "Gameplay foundation is incomplete ({}). This is evidence for the next pass, not a finished game.",
                missing.join(", ")
            )
        };
        let mut result = AgentToolResult::success(
            summary,
            serde_json::json!({
                "verdict": verdict,
                "counts": {
                    "live_entities": live.len(),
                    "renderable": renderable,
                    "camera_components": camera_nodes,
                    "attached_scripts": attached_scripts,
                    "scripts_with_gameplay_signals": gameplay_scripts,
                    "imported_assets": self.assets.len(),
                    "entities_with_source_asset": source_asset_entities,
                },
                "startup_camera": startup_camera.and_then(|id| node_summary(self.scene, id, false)).unwrap_or(Value::Null),
                "lighting": lighting,
                "systems": {"physics_enabled": physics, "audio_enabled": audio},
                "scripts": scripts,
                "missing": missing,
                "next_actions": next_actions,
            }),
        );
        result.ok = passed;
        result.verification = Some(serde_json::json!({
            "status": if passed { "foundation_present" } else { "incomplete" },
            "verdict": verdict,
        }));
        result
    }
}

const GAMEPLAY_SIGNALS: &[&str] = &[
    "is_key_pressed",
    "is_key_down",
    "move_by",
    "set_world_position",
    "velocity",
    "jump",
    "score",
    "points",
    "send_event",
    "collision",
    "physics",
    "spawn",
    "health",
    "lives",
    "get_axis",
    "input_axis",
    "set_velocity",
];

fn bounded_source(path: &Path, limit: u64) -> String {
    use std::io::Read;
    let Ok(file) = std::fs::File::open(path) else {
        return String::new();
    };
    let mut bytes = Vec::new();
    if file.take(limit + 1).read_to_end(&mut bytes).is_err() || bytes.len() as u64 > limit {
        return String::new();
    }
    String::from_utf8(bytes).unwrap_or_default()
}

fn observation_to_tool_result(observation: AgentObservationResult) -> AgentToolResult {
    let ok = observation.is_success();
    let mut result = AgentToolResult::success(observation.summary, observation.data);
    result.ok = ok;
    result.warnings = observation.warnings;
    result.verification = observation
        .verification
        .and_then(|verification| serde_json::to_value(verification).ok());
    result
}

pub fn build_agent_system_prompt(
    _snapshot: &AgentProjectSnapshot,
    mode: &str,
    language: Language,
) -> String {
    format!(
        "{} {} {}",
        t("app.agent_system_prompt", language),
        t("app.agent_design_rules", language),
        t("app.agent_asset_rules", language)
    )
    .replace("{mode}", mode)
}

fn reference(scene: &SceneGraph, id: SceneNodeId) -> Option<SnapshotReference> {
    if !scene.is_valid_node(id) {
        return None;
    }
    let node = scene.get(id)?;
    Some(SnapshotReference {
        id: id.0,
        uuid: node.uuid.to_string(),
        name: presentation_name(id, node),
        path: display_path(scene, id),
        kind: node_kind(node).to_string(),
        semantic_role: node.semantic_role.clone(),
        stable_key: node.stable_key.clone(),
        tags: node.tags.clone(),
    })
}

fn presentation_name(id: SceneNodeId, node: &SceneNode) -> String {
    let cleaned = display_name(&node.name);
    if cleaned.is_empty() {
        format!("Entity_{}", id.0)
    } else {
        cleaned
    }
}

fn node_summary(scene: &SceneGraph, id: SceneNodeId, include_transform: bool) -> Option<Value> {
    if !scene.is_valid_node(id) {
        return None;
    }
    let node = scene.get(id)?;
    let world_position = scene.world_matrix(id).col(3).truncate();
    let half = node.primitive.descriptor().half_extents;
    let mut value = serde_json::json!({
        "id": id.0,
        "uuid": node.uuid,
        "ref": format!("entity:{}", node.uuid),
        "name": presentation_name(id, node),
        "path": display_path(scene, id),
        "kind": node_kind(node),
        "primitive": node.primitive.label(),
        "is_folder": node.is_folder,
        "orphaned": !is_reachable(scene, id),
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
        "source_asset": node.source_asset,
        "scripts": node.scripts,
        "semantic_role": node.semantic_role,
        "stable_key": node.stable_key,
        "tags": node.tags,
        "agent_origin": node.agent_origin,
        "color_rgba": [node.color.r, node.color.g, node.color.b, node.color.a],
        "visible": node.visible,
        "locked": node.locked
    });
    value["local_bounds"] = serde_json::json!({
        "min": [-half.x, -half.y, -half.z],
        "max": [half.x, half.y, half.z]
    });
    if let Some((min, max)) = world_bounds(scene, id) {
        value["world_bounds"] = serde_json::json!({
            "min": [min.x, min.y, min.z],
            "max": [max.x, max.y, max.z],
            "size": [max.x - min.x, max.y - min.y, max.z - min.z]
        });
    }
    if include_transform {
        value["transform"] = serde_json::json!({
            "position": [node.position.x, node.position.y, node.position.z],
            "rotation_deg": [node.rotation.x, node.rotation.y, node.rotation.z],
            "scale": [node.scale.x, node.scale.y, node.scale.z],
            "size": node.primitive.dimensions(node.scale).to_array(),
            "world_position": [world_position.x, world_position.y, world_position.z]
        });
    }
    Some(value)
}

fn node_detail(scene: &SceneGraph, id: SceneNodeId) -> Option<Value> {
    let node = scene.get(id)?;
    let mut value = node_summary(scene, id, true)?;
    value["visible"] = Value::Bool(node.visible);
    value["locked"] = Value::Bool(node.locked);
    value["color_rgba"] =
        serde_json::json!([node.color.r, node.color.g, node.color.b, node.color.a]);
    value["variables"] = serde_json::to_value(&node.variables).unwrap_or(Value::Null);
    value["child_ids"] = serde_json::json!(node.children.iter().map(|id| id.0).collect::<Vec<_>>());
    Some(value)
}

fn descendants(scene: &SceneGraph, root: SceneNodeId, max_depth: usize) -> Vec<SceneNodeId> {
    let mut output = Vec::new();
    let mut seen = HashSet::new();
    let mut pending = vec![(root, 0usize)];
    while let Some((id, depth)) = pending.pop() {
        if !scene.is_valid_node(id) || !seen.insert(id) {
            continue;
        }
        output.push(id);
        if depth >= max_depth {
            continue;
        }
        if let Some(node) = scene.get(id) {
            for child in node.children.iter().rev() {
                pending.push((*child, depth + 1));
            }
        }
    }
    output
}

fn outline_ids(scene: &SceneGraph, max_depth: usize) -> Vec<SceneNodeId> {
    let mut output = Vec::new();
    let mut seen = HashSet::new();
    for root in scene.roots() {
        for id in descendants(scene, *root, max_depth) {
            if seen.insert(id) {
                output.push(id);
            }
        }
    }
    let reachable = reachable_scene_ids(scene);
    for id in scene.all_live_ids() {
        if !reachable.contains(&id) && seen.insert(id) {
            output.push(id);
        }
    }
    output
}

fn reachable_scene_ids(scene: &SceneGraph) -> HashSet<SceneNodeId> {
    scene
        .roots()
        .iter()
        .flat_map(|root| descendants(scene, *root, usize::MAX))
        .collect()
}

fn is_reachable(scene: &SceneGraph, id: SceneNodeId) -> bool {
    if !scene.is_valid_node(id) {
        return false;
    }
    let roots = scene.roots();
    let mut current = Some(id);
    let mut seen = HashSet::new();
    while let Some(current_id) = current {
        if !seen.insert(current_id) || !scene.is_valid_node(current_id) {
            return false;
        }
        let Some(node) = scene.get(current_id) else {
            return false;
        };
        if let Some(parent) = node.parent {
            current = Some(parent);
        } else {
            return roots.contains(&current_id);
        }
    }
    false
}

pub(crate) fn resolve_target(
    scene: &SceneGraph,
    selection: &SceneSelectionState,
    target: &str,
) -> Option<SceneNodeId> {
    let target = target.trim();
    if target.is_empty() {
        return None;
    }
    if target.eq_ignore_ascii_case("selected") {
        return selection
            .selected_node
            .filter(|id| scene.is_valid_node(*id));
    }
    resolve_core_target(scene, target).or_else(|| {
        let normalized = display_name(target).to_ascii_lowercase();
        scene.iter().find_map(|(id, node)| {
            (scene.is_valid_node(id)
                && (node
                    .semantic_role
                    .as_deref()
                    .is_some_and(|role| role.eq_ignore_ascii_case(&normalized))
                    || node
                        .stable_key
                        .as_deref()
                        .is_some_and(|key| key.eq_ignore_ascii_case(&normalized))))
            .then_some(id)
        })
    })
}

fn target_values(arguments: &Value) -> Vec<String> {
    if let Some(targets) = arguments.get("targets").and_then(Value::as_array) {
        return targets
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .take(64)
            .collect();
    }
    string_value(arguments, "target")
        .map(|target| vec![target.to_string()])
        .unwrap_or_default()
}

fn string_value<'a>(arguments: &'a Value, name: &str) -> Option<&'a str> {
    arguments.get(name).and_then(Value::as_str)
}

fn node_kind(node: &SceneNode) -> &'static str {
    if node.is_folder {
        "group"
    } else {
        match node.primitive {
            raf_core::scene::graph::Primitive::Empty => "empty",
            raf_core::scene::graph::Primitive::Cube => "cube",
            raf_core::scene::graph::Primitive::Sphere => "sphere",
            raf_core::scene::graph::Primitive::Plane => "plane",
            raf_core::scene::graph::Primitive::Cylinder => "cylinder",
            raf_core::scene::graph::Primitive::Capsule => "capsule",
        }
    }
}

fn kind_matches(node: &SceneNode, kind: &str) -> bool {
    if kind.is_empty() {
        return true;
    }
    let canonical = match kind {
        "box" | "block" => "cube",
        "ball" | "circle" | "uv_sphere" => "sphere",
        "floor" | "sprite" | "sprite2d" => "plane",
        "group" => "folder",
        other => other,
    };
    node_kind(node) == canonical || node.primitive.label().eq_ignore_ascii_case(canonical)
}

fn project_type_name(project_type: ProjectType) -> &'static str {
    match project_type {
        ProjectType::Game => "game",
        ProjectType::Electronics => "electronics",
    }
}

fn normalize_asset(asset: &str) -> String {
    asset
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_ascii_lowercase()
}

fn used_asset_keys(scene: &SceneGraph) -> BTreeSet<String> {
    scene
        .iter()
        .filter_map(|(_, node)| node.source_asset.as_deref())
        .map(normalize_asset)
        .collect()
}

fn asset_is_used(asset: &str, used: &BTreeSet<String>) -> bool {
    let asset = normalize_asset(asset);
    used.iter()
        .any(|source| source == &asset || source.ends_with(&asset) || asset.ends_with(source))
}

fn script_paths(scene: &SceneGraph, assets: &[String]) -> Vec<String> {
    let mut scripts = BTreeSet::new();
    for asset in assets.iter().filter(|asset| is_script_path(asset)) {
        scripts.insert(asset.clone());
    }
    for (_, node) in scene.iter() {
        scripts.extend(node.scripts.iter().cloned());
    }
    scripts.into_iter().collect()
}

fn is_script_path(path: &str) -> bool {
    matches!(
        path.rsplit('.')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "rhai" | "lua" | "js" | "ts" | "py"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_core::scene::graph::{NodeColor, Primitive};

    #[test]
    fn snapshot_exposes_names_paths_assets_and_scripts() {
        let mut scene = SceneGraph::new();
        let root = scene.add_root_folder("Store");
        let shelf = scene.add_child_with_primitive(root, "Shelf_Left", Primitive::Cube);
        scene.get_mut(shelf).unwrap().source_asset = Some("assets/shelf.glb".to_string());
        scene
            .get_mut(shelf)
            .unwrap()
            .scripts
            .push("assets/shelf.rhai".to_string());
        let assets = vec![
            "assets/shelf.glb".to_string(),
            "assets/unused.png".to_string(),
        ];
        let selection = SceneSelectionState {
            selected_node: Some(shelf),
            selected_nodes: vec![shelf],
        };
        let context = AgentObservationContext {
            scene: &scene,
            selection: &selection,
            project: None,
            assets: &assets,
            active_session: "Main",
            revision: 7,
            catalog_pending: false,
            catalog_error: None,
        };

        let snapshot = context.snapshot();
        assert_eq!(snapshot.scene.roots[0].name, "Store");
        assert_eq!(snapshot.scene.selected[0].path, "/Store/Shelf_Left");
        assert_eq!(snapshot.assets.used, 1);
        assert_eq!(snapshot.assets.unused, 1);
        assert_eq!(snapshot.scripts, 1);
    }

    #[test]
    fn scene_verify_reports_transform_and_color_mismatches() {
        let mut scene = SceneGraph::new();
        let id = scene.add_root_with_primitive("Shelf", Primitive::Cube);
        let node = scene.get_mut(id).expect("created node");
        node.position = glam::Vec3::new(1.0, 2.0, 3.0);
        node.scale = glam::Vec3::new(2.0, 1.0, 0.5);
        node.color = NodeColor::rgb(20, 30, 40);
        let selection = SceneSelectionState {
            selected_node: Some(id),
            selected_nodes: vec![id],
        };
        let context = AgentObservationContext {
            scene: &scene,
            selection: &selection,
            project: None,
            assets: &[],
            active_session: "Main",
            revision: 4,
            catalog_pending: false,
            catalog_error: None,
        };

        let result = context
            .execute(
                "scene_verify",
                &serde_json::json!({
                    "targets": [id.0.to_string()],
                    "expected_count": 1,
                    "expected": {
                        "primitive": "cube",
                        "transform": {
                            "position": [1.0, 2.0, 3.0],
                            "scale": [2.0, 1.0, 0.5]
                        },
                        "color_rgba": [20, 30, 40]
                    }
                }),
            )
            .expect("scene_verify tool");

        assert!(result.ok);
        assert_eq!(result.verification.unwrap()["status"], "passed");

        let mismatch = context
            .execute(
                "scene_verify",
                &serde_json::json!({
                    "targets": [id.0.to_string()],
                    "expected": {"transform": {"position": [9.0, 2.0, 3.0]}}
                }),
            )
            .expect("scene_verify tool");
        assert!(!mismatch.ok);
        assert_eq!(mismatch.verification.unwrap()["status"], "failed");
    }

    #[test]
    fn game_completion_audit_separates_geometry_from_playable_foundation() {
        let root = std::env::temp_dir().join(format!(
            "raf-game-audit-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let mut project = Project::create("Audit", ProjectType::Game, &root).unwrap();
        let scripts = project.path.join("assets/scripts");
        std::fs::create_dir_all(&scripts).unwrap();
        std::fs::write(
            scripts.join("player.rhai"),
            "fn on_update(dt) { if is_key_pressed(\"w\") { owner().move_by(0.0, 0.0, dt); } }",
        )
        .unwrap();
        let mut scene = SceneGraph::new();
        let player = scene.add_root_with_primitive("Player", Primitive::Cube);
        let camera = scene.add_root_with_primitive("Camera", Primitive::Empty);
        scene.get_mut(camera).unwrap().game_camera =
            Some(raf_core::runtime_config::GameCamera::default());
        project.settings.runtime.active_camera = Some(scene.get(camera).unwrap().uuid);
        scene
            .get_mut(player)
            .unwrap()
            .scripts
            .push("scripts/player.rhai".to_string());
        let selection = SceneSelectionState::default();
        let context = AgentObservationContext {
            scene: &scene,
            selection: &selection,
            project: Some(&project),
            assets: &["scripts/player.rhai".to_string()],
            active_session: "Main",
            revision: 2,
            catalog_pending: false,
            catalog_error: None,
        };

        let result = context
            .execute("game_completion_audit", &serde_json::json!({}))
            .expect("game completion audit");
        assert!(result.ok, "{}", result.summary);
        assert_eq!(result.data["verdict"], "playable_foundation");
        assert_eq!(result.data["counts"]["attached_scripts"], 1);
        assert_eq!(
            result.data["counts"]["scripts_with_gameplay_signals"],
            1
        );

        scene.get_mut(player).unwrap().scripts.clear();
        let context = AgentObservationContext {
            scene: &scene,
            selection: &selection,
            project: Some(&project),
            assets: &["scripts/player.rhai".to_string()],
            active_session: "Main",
            revision: 2,
            catalog_pending: false,
            catalog_error: None,
        };
        let incomplete = context
            .execute("game_completion_audit", &serde_json::json!({}))
            .expect("game completion audit");
        assert!(!incomplete.ok);
        assert_eq!(incomplete.data["verdict"], "geometry_only");

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn snapshot_excludes_soft_removed_scene_slots_from_counts() {
        let mut scene = SceneGraph::new();
        let removed = scene.add_root_with_primitive("Removed", Primitive::Cube);
        scene.add_root_with_primitive("Visible", Primitive::Sphere);
        assert!(scene.remove_node(removed));
        assert_eq!(scene.len(), 2);
        let selection = SceneSelectionState::default();

        let context = AgentObservationContext {
            scene: &scene,
            selection: &selection,
            project: None,
            assets: &[],
            active_session: "Main",
            revision: 1,
            catalog_pending: false,
            catalog_error: None,
        };

        let snapshot = context.snapshot();
        assert_eq!(snapshot.scene.entities, 1);
    }
}
