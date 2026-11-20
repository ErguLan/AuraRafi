//! Headless engine helper functions.

use crate::cli::CliResult;
use raf_core::agent_context;
use raf_core::capabilities::{CapabilityCatalog, CapabilityDefinition};
use raf_core::project::{Project, ProjectType};
use raf_core::scene::{SceneGraph, SceneNodeId};
use raf_core::session::ProjectSessionRegistry;
use serde_json::json;
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

pub(super) fn load_project(path: &Path) -> CliResult<Project> {
    let directory = if path.is_file() {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    Project::load(directory)
        .map_err(|error| format!("Unable to load project {}: {error}", directory.display()))
}

pub(super) fn valid_project_name(name: &str) -> bool {
    !name.trim().is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains(':')
}

pub(super) fn string_param<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params.as_object()?.get(key)?.as_str()
}

pub(super) fn capability_value(capability: &CapabilityDefinition) -> Value {
    json!({
        "name": capability.name,
        "aliases": capability.aliases,
        "domain": capability.domain,
        "category": capability.category,
        "description_key": capability.description_key,
        "parameters": capability.parameters,
        "examples": capability.examples,
        "risk": capability.risk(),
        "read_only": capability.is_read_only(),
    })
}

pub(super) fn count_workspace_entries(root: &Path) -> (usize, usize) {
    fn visit(path: &Path, files: &mut usize, directories: &mut usize, depth: u8) {
        if depth > 8 || *files + *directories >= 8_192 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if matches!(
                name.as_str(),
                ".git"
                    | ".aura_rafi"
                    | ".ai"
                    | ".codex"
                    | "target"
                    | "target_gnu"
                    | "node_modules"
                    | ".cache"
            ) || name == "agent_history.ron"
                || name == "agent_endpoint.json"
            {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                *directories += 1;
                visit(&entry.path(), files, directories, depth.saturating_add(1));
            } else if kind.is_file() {
                *files += 1;
            }
        }
    }
    let mut files = 0;
    let mut directories = 0;
    visit(root, &mut files, &mut directories, 0);
    (files, directories)
}

pub(super) fn headless_capability_values(catalog: &CapabilityCatalog) -> Vec<Value> {
    [
        "engine.status",
        "engine.context",
        "scene.outline",
        "scene.query",
        "scene.spatial_map",
        "scene.overlaps",
        "scene.diff",
        "scene.design_audit",
        "scene.inspect",
        "selection.get",
        "assets.catalog",
        "assets.recommend",
        "assets.inspect",
        "scripts.catalog",
        "project.health",
        "scene.verify",
        "engine.doctor",
        "capabilities.list",
        "capabilities.search",
        "project.create",
        "project.open",
        "project.info",
        "project.save",
        "session.list",
        "workspace.describe",
    ]
    .into_iter()
    .map(|name| {
        catalog.find(name).map(capability_value).unwrap_or_else(|| {
            json!({
                "name": name,
                "aliases": [],
                "domain": "shared",
                "category": "inspection",
                "description_key": "commands.headless_metadata.desc",
                "parameters": [],
                "examples": [],
                "risk": "read",
                "read_only": true,
            })
        })
    })
    .collect()
}

pub(super) fn project_type_label(project_type: ProjectType) -> &'static str {
    match project_type {
        ProjectType::Game => "game",
        ProjectType::Electronics => "electronics",
    }
}

pub(super) fn load_project_scene(project: &Project) -> SceneGraph {
    let registry = ProjectSessionRegistry::load_or_legacy(&project.path, project.project_type);
    let Some(session) = registry.active() else {
        return SceneGraph::new();
    };
    SceneGraph::load_ron(&session.path(&project.path, &session.scene_file))
}

pub(super) fn scene_document_fingerprint(scene: &SceneGraph) -> u64 {
    let semantic = agent_context::scene_fingerprint(scene);
    let encoded = serde_json::to_string(&semantic).unwrap_or_default();
    let mut hasher = DefaultHasher::new();
    encoded.hash(&mut hasher);
    hasher.finish()
}

pub(super) fn scene_context(scene: &SceneGraph, selected: &[SceneNodeId]) -> Value {
    agent_context::scene_context(scene, selected)
}

const MAX_PROJECT_ASSETS: usize = 2_048;

pub(super) fn project_asset_paths(root: &Path) -> Vec<String> {
    fn visit(root: &Path, current: &Path, output: &mut Vec<String>, depth: u8) {
        if depth > 24 || output.len() >= MAX_PROJECT_ASSETS {
            return;
        }
        let Ok(entries) = std::fs::read_dir(current) else {
            return;
        };
        for entry in entries.flatten() {
            if output.len() >= MAX_PROJECT_ASSETS {
                break;
            }
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if matches!(
                name.as_str(),
                ".git"
                    | ".aura_rafi"
                    | ".ai"
                    | ".codex"
                    | "target"
                    | "target_gnu"
                    | "node_modules"
                    | ".cache"
            ) {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_dir() {
                visit(root, &path, output, depth.saturating_add(1));
            } else if kind.is_file() {
                let relative = path
                    .strip_prefix(root)
                    .unwrap_or(path.as_path())
                    .to_string_lossy()
                    .replace('\\', "/");
                output.push(relative);
            }
        }
    }

    let assets_root = root.join("assets");
    let mut assets = Vec::new();
    if assets_root.is_dir() {
        visit(root, &assets_root, &mut assets, 0);
    }
    assets.sort_unstable();
    assets
}

pub(super) fn used_asset_count(scene: &SceneGraph, assets: &[String]) -> usize {
    let used = scene
        .iter()
        .filter_map(|(_, node)| node.source_asset.as_deref())
        .map(normalize_asset_path)
        .collect::<Vec<_>>();
    assets
        .iter()
        .filter(|asset| {
            let asset = normalize_asset_path(asset);
            used.iter().any(|source| {
                asset == *source || asset.ends_with(source) || source.ends_with(&asset)
            })
        })
        .count()
}

pub(super) fn normalize_asset_path(path: &str) -> String {
    path.trim()
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_ascii_lowercase()
}
