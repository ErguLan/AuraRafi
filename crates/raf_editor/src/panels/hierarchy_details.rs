//! Cached presentation details derived from confirmed scene and asset state.
//!
//! This module is the single registry for row kinds. Icons, badges and scene
//! queries all resolve through [`HierarchyEntityKind`] so a new row type only
//! needs one classification rule instead of three parallel condition chains.
use raf_core::scene::{Primitive, SceneGraph, SceneNode, SceneNodeId};
use raf_render::api_graphic_basic::ui_surface::UiIconId;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use super::HierarchyEntityKind;

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

impl HierarchyEntityKind {
    /// Stable query/type slug shared by the `type:` filter and diagnostics.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Folder => "folder",
            Self::Camera => "camera",
            Self::Light => "light",
            Self::MeshAsset => "mesh",
            Self::Primitive => "primitive",
            Self::Empty => "empty",
            Self::Audio => "audio",
            Self::Prefab => "prefab",
            Self::Script => "script",
            Self::Assembly => "assembly",
            Self::Spawn => "spawn",
            Self::Trigger => "trigger",
        }
    }

    /// Primary row icon. Primitive rows keep their shape-specific icon.
    pub const fn icon(self, primitive: Primitive) -> UiIconId {
        match self {
            Self::Folder => UiIconId::Folder,
            Self::Camera => UiIconId::Camera,
            Self::Light => UiIconId::Sun,
            Self::MeshAsset => UiIconId::Assets,
            Self::Primitive => match primitive {
                Primitive::Cube => UiIconId::Cube,
                Primitive::Sphere | Primitive::Sphere2D => UiIconId::Sphere,
                Primitive::Plane => UiIconId::Plane,
                Primitive::Cylinder | Primitive::Prism => UiIconId::Cylinder,
                Primitive::Capsule => UiIconId::Capsule,
                Primitive::Wedge => UiIconId::Cube,
                Primitive::Torus => UiIconId::Sphere,
                Primitive::Empty => UiIconId::Node,
            },
            Self::Empty => UiIconId::Node,
            Self::Audio => UiIconId::Play,
            Self::Prefab | Self::Assembly => UiIconId::Entity,
            Self::Script => UiIconId::Script,
            Self::Spawn | Self::Trigger => UiIconId::Node,
        }
    }
}

/// Component/asset badge carried by one hierarchy row.
///
/// Badges are ordered by this enum; the surface only paints the first two and
/// folds the remainder into a `+N` affordance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HierarchyBadgeKind {
    Warning,
    Light,
    Camera,
    Mesh,
    Script,
    Source,
    Audio,
    Physics,
    Assembly,
}

impl HierarchyBadgeKind {
    pub const fn key(self) -> &'static str {
        match self {
            Self::Warning => "hierarchy.badge.warning",
            Self::Light => "hierarchy.badge.light",
            Self::Camera => "hierarchy.badge.camera",
            Self::Mesh => "hierarchy.badge.mesh",
            Self::Script => "hierarchy.badge.scripts",
            Self::Source => "hierarchy.badge.source",
            Self::Audio => "hierarchy.badge.audio",
            Self::Physics => "hierarchy.badge.physics",
            Self::Assembly => "hierarchy.badge.assembly",
        }
    }

    pub const fn section(self) -> &'static str {
        match self {
            Self::Warning => "metadata",
            // The inspector has no dedicated light/mesh/camera section yet;
            // opening Components keeps the badge actionable instead of
            // emitting an unknown section slug.
            Self::Light | Self::Camera | Self::Mesh => "components",
            Self::Script => "behavior",
            Self::Source => "appearance",
            Self::Audio => "audio",
            Self::Physics => "physics",
            Self::Assembly => "components",
        }
    }

    pub const fn icon(self) -> UiIconId {
        match self {
            Self::Warning => UiIconId::Warning,
            Self::Light => UiIconId::Sun,
            Self::Camera => UiIconId::Camera,
            Self::Mesh => UiIconId::Assets,
            Self::Script => UiIconId::Script,
            Self::Source => UiIconId::Assets,
            Self::Audio => UiIconId::Play,
            Self::Physics => UiIconId::Node,
            Self::Assembly => UiIconId::Entity,
        }
    }

    fn badge(self, count: usize) -> HierarchyBadge {
        HierarchyBadge {
            key: self.key(),
            section: self.section(),
            count,
            diagnostics: Vec::new(),
        }
    }
}

/// Badge mapping used by the retained surface. Keeping the match here means a
/// badge can never drift from its tooltip key.
pub fn badge_icon(badge: &HierarchyBadge) -> UiIconId {
    HierarchyBadgeKind::from_key(badge.key)
        .map(HierarchyBadgeKind::icon)
        .unwrap_or(UiIconId::Node)
}

impl HierarchyBadgeKind {
    pub fn from_key(key: &str) -> Option<Self> {
        [
            Self::Warning,
            Self::Light,
            Self::Camera,
            Self::Mesh,
            Self::Script,
            Self::Source,
            Self::Audio,
            Self::Physics,
            Self::Assembly,
        ]
        .into_iter()
        .find(|kind| kind.key() == key)
    }
}

fn normalize_asset_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim_start_matches("assets/")
        .to_string()
}

/// Number of scene references that are absent from the current project
/// catalogue.
///
/// `None` means the catalogue was not available yet. Callers must treat that
/// as "unknown" instead of reporting every reference as missing, which is also
/// why `missing:true` only matches once `set_assets` has produced a catalogue.
pub fn missing_reference_count(node: &SceneNode, assets: Option<&HashSet<String>>) -> usize {
    let Some(assets) = assets else {
        return 0;
    };
    let missing = |path: &str| !assets.contains(&normalize_asset_path(path));
    node.scripts.iter().filter(|path| missing(path)).count()
        + node
            .source_asset
            .as_deref()
            .filter(|path| !path.starts_with("builtin://") && missing(path))
            .is_some() as usize
        + node
            .base_color_texture
            .as_deref()
            .filter(|path| missing(path))
            .is_some() as usize
        + node
            .mesh_asset
            .as_ref()
            .filter(|mesh| missing(&mesh.path))
            .is_some() as usize
        + node
            .animation_asset
            .as_deref()
            .filter(|path| missing(path))
            .is_some() as usize
        + (!node.audio_source.clip.is_empty() && missing(&node.audio_source.clip)) as usize
        + node
            .linked_assembly
            .as_ref()
            .is_some_and(|assembly| missing(&assembly.source)) as usize
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
    scene
        .iter()
        .filter(|(_, n)| !n.name.is_empty())
        .map(|(id, node)| {
            let mut badges = Vec::new();
            let kind = super::entity_kind(node);
            let missing = missing_reference_count(node, assets);
            let invalid =
                !node.position.is_finite() || !node.rotation.is_finite() || !node.scale.is_finite();
            let duplicate = uuids.get(&node.uuid).copied().unwrap_or_default() > 1
                || node
                    .stable_key
                    .as_ref()
                    .is_some_and(|key| keys.get(key).copied().unwrap_or_default() > 1);
            if missing > 0 || invalid || duplicate {
                badges.push(HierarchyBadge {
                    key: HierarchyBadgeKind::Warning.key(),
                    section: HierarchyBadgeKind::Warning.section(),
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
            // Component badges follow warning so the actionable diagnostic is
            // never pushed behind decorative component badges.
            if kind == HierarchyEntityKind::Light || node.light.is_some() {
                badges.push(HierarchyBadgeKind::Light.badge(1));
            }
            if kind == HierarchyEntityKind::Camera || node.game_camera.is_some() {
                badges.push(HierarchyBadgeKind::Camera.badge(1));
            }
            if kind == HierarchyEntityKind::MeshAsset || node.mesh_asset.is_some() {
                badges.push(HierarchyBadgeKind::Mesh.badge(1));
            }
            if kind == HierarchyEntityKind::Script || !node.scripts.is_empty() {
                badges.push(HierarchyBadgeKind::Script.badge(node.scripts.len().max(1)));
            }
            if node.source_asset.is_some() || node.base_color_texture.is_some() {
                badges.push(HierarchyBadgeKind::Source.badge(1));
            }
            if node.audio_source.enabled {
                badges.push(HierarchyBadgeKind::Audio.badge(1));
            }
            if node.rigid_body.enabled
                || node.collider.collider_type != raf_core::scene::ColliderType::None
            {
                badges.push(HierarchyBadgeKind::Physics.badge(1));
            }
            if node.linked_assembly.is_some() {
                badges.push(HierarchyBadgeKind::Assembly.badge(1));
            }
            (id, badges)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_core::scene::{MeshAssetRef, SceneLight};

    #[test]
    fn light_and_mesh_nodes_carry_component_badges() {
        let mut scene = SceneGraph::new();
        let light = scene.add_root("Key light");
        scene.get_mut(light).unwrap().light = Some(SceneLight::default());
        let mesh = scene.add_root("Chair");
        scene.get_mut(mesh).unwrap().mesh_asset = Some(MeshAssetRef::new("models/chair.glb"));

        let badges = badge_index(&scene, None);
        assert!(badges[&light]
            .iter()
            .any(|badge| badge.key == "hierarchy.badge.light"));
        assert!(badges[&mesh]
            .iter()
            .any(|badge| badge.key == "hierarchy.badge.mesh"));
        assert_eq!(
            badge_icon(&badges[&light][0]),
            UiIconId::Sun,
            "the badge icon registry resolves light badges"
        );
    }
}
