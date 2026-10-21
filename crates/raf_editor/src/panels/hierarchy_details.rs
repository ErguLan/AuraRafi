//! Cached presentation details derived from confirmed scene and asset state.
use raf_core::scene::{SceneGraph, SceneNodeId};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HierarchyBadge {
    pub key: &'static str,
    pub section: &'static str,
    pub count: usize,
    pub diagnostics: Vec<&'static str>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct HierarchyViewState {
    pub version: u32,
    pub collapsed: Vec<Uuid>,
    pub query: String,
    pub scroll: f32,
    pub show_hidden: bool,
    pub insertion_parent: Option<Uuid>,
}

pub fn badge_index(
    scene: &SceneGraph,
    assets: Option<&HashSet<String>>,
) -> HashMap<SceneNodeId, Vec<HierarchyBadge>> {
    let mut uuids = HashMap::new();
    let mut keys = HashMap::new();
    for (_, node) in scene.iter().filter(|(_, n)| !n.name.is_empty()) {
        *uuids.entry(node.uuid).or_insert(0usize) += 1;
        if let Some(key) = &node.stable_key {
            *keys.entry(key).or_insert(0usize) += 1;
        }
    }
    let known = |path: &str| {
        assets.map(|a| {
            a.contains(
                &path
                    .replace('\\', "/")
                    .trim_start_matches("assets/")
                    .to_string(),
            )
        })
    };
    scene
        .iter()
        .filter(|(_, n)| !n.name.is_empty())
        .map(|(id, node)| {
            let mut badges = Vec::new();
            let missing = node
                .scripts
                .iter()
                .filter(|s| known(s) == Some(false))
                .count()
                + node
                    .source_asset
                    .as_deref()
                    .filter(|s| !s.starts_with("builtin://") && known(s) == Some(false))
                    .is_some() as usize
                + node
                    .base_color_texture
                    .as_deref()
                    .filter(|s| known(s) == Some(false))
                    .is_some() as usize;
            let invalid =
                !node.position.is_finite() || !node.rotation.is_finite() || !node.scale.is_finite();
            let duplicate = uuids.get(&node.uuid).copied().unwrap_or_default() > 1
                || node
                    .stable_key
                    .as_ref()
                    .is_some_and(|key| keys.get(key).copied().unwrap_or_default() > 1);
            if missing > 0 || invalid || duplicate {
                badges.push(HierarchyBadge {
                    key: "hierarchy.badge.warning",
                    section: "metadata",
                    count: missing.max(1),
                    diagnostics: [
                        ("hierarchy.diagnostic.missing_reference", missing > 0),
                        ("hierarchy.diagnostic.invalid_transform", invalid),
                        ("hierarchy.diagnostic.duplicate_identity", duplicate),
                    ]
                    .into_iter()
                    .filter_map(|(key, active)| active.then_some(key))
                    .collect(),
                });
            }
            if !node.scripts.is_empty() {
                badges.push(HierarchyBadge {
                    key: "hierarchy.badge.scripts",
                    section: "behavior",
                    count: node.scripts.len(),
                    diagnostics: Vec::new(),
                });
            }
            if node.source_asset.is_some() || node.base_color_texture.is_some() {
                badges.push(HierarchyBadge {
                    key: "hierarchy.badge.source",
                    section: "appearance",
                    count: 1,
                    diagnostics: Vec::new(),
                });
            }
            if node.audio_source.enabled {
                badges.push(HierarchyBadge {
                    key: "hierarchy.badge.audio",
                    section: "audio",
                    count: 1,
                    diagnostics: Vec::new(),
                });
            }
            if node.rigid_body.enabled
                || node.collider.collider_type != raf_core::scene::ColliderType::None
            {
                badges.push(HierarchyBadge {
                    key: "hierarchy.badge.physics",
                    section: "physics",
                    count: 1,
                    diagnostics: Vec::new(),
                });
            }
            (id, badges)
        })
        .collect()
}
