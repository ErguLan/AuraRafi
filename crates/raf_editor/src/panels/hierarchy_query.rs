//! Bounded, conjunctive scene queries. Quoted values may contain spaces.
//!
//! Every structured filter is an AND term. `type:` resolves through the same
//! [`HierarchyEntityKind`] registry used by row icons/badges, so a node can
//! never present one kind and answer a different `type:` slug.
use raf_core::scene::{Primitive, SceneGraph, SceneNodeId};
use std::collections::HashSet;

use super::details::missing_reference_count;
use super::{entity_kind, HierarchyEntityKind};

/// Convenience wrapper for callers that do not have a project catalogue.
#[cfg_attr(not(test), allow(dead_code))]
pub fn matches(scene: &SceneGraph, id: SceneNodeId, query: &str) -> bool {
    matches_with_assets(scene, id, query, None)
}

/// Catalogue-aware variant used by the hierarchy projection. Passing the
/// project asset set enables `missing:true`; without it the filter stays inert
/// instead of reporting every reference as missing before the catalogue loads.
pub fn matches_with_assets(
    scene: &SceneGraph,
    id: SceneNodeId,
    query: &str,
    assets: Option<&HashSet<String>>,
) -> bool {
    let Some(node) = scene.get(id) else {
        return false;
    };
    let kind = entity_kind(node);
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut quoted = false;
    for c in query.chars().take(256) {
        if c == '"' {
            quoted = !quoted;
        } else if c.is_whitespace() && !quoted {
            if !token.is_empty() {
                tokens.push(std::mem::take(&mut token));
            }
        } else {
            token.push(c);
        }
    }
    if !token.is_empty() {
        tokens.push(token);
    }
    tokens.iter().all(|token| {
        let (key, value) = token.split_once(':').unwrap_or(("", token));
        let value = value.to_lowercase();
        let contains = |text: &str| text.to_lowercase().contains(&value);
        match key {
            "" => contains(&node.name) || node.scripts.iter().any(|s| contains(s)),
            "type" => match value.as_str() {
                "camera" => kind == HierarchyEntityKind::Camera || node.game_camera.is_some(),
                "light" => node.light.is_some(),
                "mesh" | "meshasset" | "mesh-asset" => node.mesh_asset.is_some(),
                "folder" | "group" => node.is_folder,
                "shape" => node.primitive != Primitive::Empty && !node.is_folder,
                "audio" => node.audio_source.enabled,
                "script" => !node.scripts.is_empty(),
                "empty" => node.primitive == Primitive::Empty && !node.is_folder,
                "prefab" => node
                    .source_asset
                    .as_deref()
                    .is_some_and(|s| s.ends_with(".prefab.ron")),
                "assembly" => node.linked_assembly.is_some(),
                "spawn" | "spawn-marker" | "spawnmarker" => node.spawn_marker.is_some(),
                "trigger" | "trigger-volume" | "volume" => {
                    node.rigid_body.enabled && node.rigid_body.is_trigger
                }
                _ => kind.slug() == value || node.primitive.descriptor().slug == value,
            },
            "has" => match value.as_str() {
                "light" => node.light.is_some(),
                "mesh" => node.mesh_asset.is_some(),
                "camera" => node.game_camera.is_some(),
                "script" => !node.scripts.is_empty(),
                "audio" => node.audio_source.enabled,
                _ => false,
            },
            "missing" => {
                let Some(assets) = assets else {
                    return false;
                };
                let missing = missing_reference_count(node, Some(assets)) > 0;
                match value.as_str() {
                    "true" => missing,
                    "false" => !missing,
                    _ => false,
                }
            }
            "script" => node.scripts.iter().any(|s| contains(s)),
            "asset" => {
                node.source_asset.as_deref().is_some_and(contains)
                    || node.animation_asset.as_deref().is_some_and(contains)
                    || node.base_color_texture.as_deref().is_some_and(contains)
                    || node
                        .mesh_asset
                        .as_ref()
                        .is_some_and(|mesh| contains(&mesh.path))
                    || node.scripts.iter().any(|script| contains(script))
                    || (!node.audio_source.clip.is_empty() && contains(&node.audio_source.clip))
            }
            "tag" => node.tags.iter().any(|tag| tag.to_lowercase() == value),
            "role" => node.semantic_role.as_deref().is_some_and(contains),
            "hidden" => match value.as_str() {
                "true" => !node.visible,
                "false" => node.visible,
                _ => false,
            },
            "locked" => match value.as_str() {
                "true" => node.locked,
                "false" => !node.locked,
                _ => false,
            },
            "under" => {
                let mut parent = node.parent;
                let mut depth = 0;
                let mut found = false;
                while let Some(p) = parent {
                    let Some(ancestor) = scene.get(p) else {
                        break;
                    };
                    if ancestor.name.to_lowercase() == value
                        || ancestor.uuid.to_string() == value
                        || scene
                            .node_path(p)
                            .is_some_and(|path| path.to_lowercase() == value)
                    {
                        found = true;
                        break;
                    }
                    parent = ancestor.parent;
                    depth += 1;
                    if depth > scene.len() {
                        break;
                    }
                }
                found
            }
            _ => false,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_core::scene::{MeshAssetRef, SceneLight};

    #[test]
    fn hierarchy_query_combines_fields_and_keeps_quoted_values() {
        let mut scene = SceneGraph::new();
        let root = scene.add_root("Retail Hall");
        let id = scene.add_child_with_primitive(root, "Shelf", Primitive::Cube);
        let node = scene.get_mut(id).unwrap();
        node.tags.push("retail".into());
        node.locked = true;
        assert!(matches(
            &scene,
            id,
            "type:cube tag:retail under:\"Retail Hall\" locked:true"
        ));
        assert!(!matches(&scene, id, "tag:retail hidden:true"));
    }

    #[test]
    fn light_and_mesh_types_and_components_are_queryable() {
        let mut scene = SceneGraph::new();
        let light = scene.add_root("Key light");
        scene.get_mut(light).unwrap().light = Some(SceneLight::default());
        let mesh = scene.add_root("Chair");
        scene.get_mut(mesh).unwrap().mesh_asset = Some(MeshAssetRef::new("models/chair.glb"));
        let plain = scene.add_root("Plain");

        assert!(matches(&scene, light, "type:light has:light"));
        assert!(!matches(&scene, light, "type:mesh"));
        assert!(!matches(&scene, mesh, "type:light"));
        assert!(matches(&scene, mesh, "type:mesh has:mesh"));
        assert!(!matches(&scene, plain, "has:light"));
        assert!(!matches(&scene, plain, "has:mesh"));
        // Existing field filters keep their AND semantics next to the new keys.
        assert!(matches(&scene, light, "type:light hidden:false"));
        assert!(!matches(&scene, light, "type:light hidden:true"));
    }

    #[test]
    fn missing_filter_requires_a_loaded_catalogue() {
        let mut scene = SceneGraph::new();
        let id = scene.add_root("Chair");
        scene.get_mut(id).unwrap().mesh_asset = Some(MeshAssetRef::new("models/chair.glb"));

        // Catalogue unavailable: the hook stays inert rather than guessing.
        assert!(!matches(&scene, id, "missing:true"));

        let empty = HashSet::new();
        assert!(matches_with_assets(
            &scene,
            id,
            "missing:true",
            Some(&empty)
        ));
        let loaded = HashSet::from(["models/chair.glb".to_string()]);
        assert!(!matches_with_assets(
            &scene,
            id,
            "missing:true",
            Some(&loaded)
        ));
    }
}
