//! Bounded, conjunctive scene queries. Quoted values may contain spaces.
use raf_core::scene::{Primitive, SceneGraph, SceneNodeId};

pub fn matches(scene: &SceneGraph, id: SceneNodeId, query: &str) -> bool {
    let Some(node) = scene.get(id) else {
        return false;
    };
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
                "camera" => node.game_camera.is_some(),
                "folder" | "group" => node.is_folder,
                "shape" => node.primitive != Primitive::Empty && !node.is_folder,
                "audio" => node.audio_source.enabled,
                "prefab" => node
                    .source_asset
                    .as_deref()
                    .is_some_and(|s| s.ends_with(".prefab.ron")),
                _ => node.primitive.descriptor().slug == value,
            },
            "script" => node.scripts.iter().any(|s| contains(s)),
            "asset" => {
                node.source_asset.as_deref().is_some_and(contains)
                    || node.base_color_texture.as_deref().is_some_and(contains)
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
}
