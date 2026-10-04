use raf_core::SceneGraph;
use std::collections::HashSet;
pub fn validate_scene(scene: &SceneGraph) -> Result<(), String> {
    let mut identities = HashSet::new();
    for (id, node) in scene.iter().filter(|(id, _)| scene.is_valid_node(*id)) {
        if !identities.insert(node.uuid) {
            return Err("scene contains duplicate entity UUIDs".into());
        }
        if !node.position.is_finite() || !node.rotation.is_finite() || !node.scale.is_finite() {
            return Err(format!("{}: non-finite transform", node.name));
        }
        let mut cursor = Some(id);
        let mut ancestry = HashSet::new();
        while let Some(parent) = cursor {
            if ancestry.len() >= 256 || !ancestry.insert(parent) {
                return Err(format!(
                    "{}: cyclic or excessively deep hierarchy",
                    node.name
                ));
            }
            if !scene.is_valid_node(parent) {
                return Err(format!("{}: missing parent", node.name));
            }
            cursor = scene.get(parent).and_then(|n| n.parent);
        }
        let mut children = HashSet::new();
        for child in &node.children {
            if !children.insert(*child)
                || !scene.is_valid_node(*child)
                || scene.get(*child).and_then(|n| n.parent) != Some(id)
            {
                return Err(format!("{}: inconsistent child links", node.name));
            }
        }
    }
    Ok(())
}
