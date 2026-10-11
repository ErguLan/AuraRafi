//! Versioned, editable subtree snapshots. No live inheritance or gameplay defaults.
use super::{SceneGraph, SceneNode, SceneNodeId, VariableValue};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

pub const MAX_PREFAB_NODES: usize = 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenePrefab {
    pub version: u32,
    /// Parent-before-child order; local parent/child indices are confined to this array.
    pub nodes: Vec<SceneNode>,
}

impl ScenePrefab {
    pub fn capture(scene: &SceneGraph, root: SceneNodeId) -> Result<Self, String> {
        let mut pending = vec![root];
        let mut ids = Vec::new();
        let mut seen = HashSet::new();
        while let Some(id) = pending.pop() {
            if !scene.is_valid_node(id) || !seen.insert(id) {
                return Err("Prefab hierarchy contains a missing node or cycle".into());
            }
            if ids.len() >= MAX_PREFAB_NODES {
                return Err("Prefab exceeds 1024 nodes".into());
            }
            let node = scene.get(id).expect("validated node");
            pending.extend(node.children.iter().rev().copied());
            ids.push(id);
        }
        let remap: HashMap<_, _> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, SceneNodeId(i)))
            .collect();
        let nodes = ids
            .iter()
            .enumerate()
            .map(|(i, id)| {
                let mut node = scene.get(*id).expect("validated node").clone();
                node.parent = if i == 0 {
                    None
                } else {
                    node.parent.and_then(|id| remap.get(&id).copied())
                };
                node.children = node
                    .children
                    .iter()
                    .filter_map(|id| remap.get(id).copied())
                    .collect();
                node.entity_index = None;
                node
            })
            .collect();
        let prefab = Self { version: 1, nodes };
        prefab.validate()?;
        Ok(prefab)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.nodes.is_empty() || self.nodes.len() > MAX_PREFAB_NODES {
            return Err("Unsupported prefab version or node count (1..1024)".into());
        }
        let mut uuids = HashSet::new();
        let mut incoming = vec![0usize; self.nodes.len()];
        for (i, node) in self.nodes.iter().enumerate() {
            if node.name.trim().is_empty()
                || node.name.len() > 256
                || !uuids.insert(node.uuid)
                || node.uuid.is_nil()
                || !node.position.is_finite()
                || !node.rotation.is_finite()
                || !node.scale.is_finite()
            {
                return Err("Prefab contains invalid identity or transform".into());
            }
            if (i == 0 && node.parent.is_some())
                || (i > 0 && !node.parent.is_some_and(|parent| parent.0 < i))
            {
                return Err("Prefab parents must precede children and have one root".into());
            }
            for child in &node.children {
                if child.0 <= i
                    || child.0 >= self.nodes.len()
                    || self.nodes[child.0].parent != Some(SceneNodeId(i))
                {
                    return Err("Prefab child/parent links disagree".into());
                }
                incoming[child.0] += 1;
            }
            if node.variables.len() > 256
                || node.scripts.len() > 64
                || node.behavior_parameters.len() > 64
                || node
                    .behavior_parameters
                    .values()
                    .any(|values| values.len() > 256)
            {
                return Err("Prefab component budget exceeded".into());
            }
            for values in std::iter::once(&node.variables).chain(node.behavior_parameters.values())
            {
                let mut names = HashSet::new();
                for variable in values {
                    if variable.name.trim().is_empty()
                        || variable.name.len() > 128
                        || !names.insert(variable.name.as_str())
                        || matches!(&variable.value, VariableValue::Number(value) if !value.is_finite())
                        || matches!(&variable.value, VariableValue::Text(value) if value.len() > 4096)
                    {
                        return Err("Prefab contains invalid or duplicate parameters".into());
                    }
                }
            }
        }
        if incoming[0] != 0 || incoming.iter().skip(1).any(|count| *count != 1) {
            return Err("Prefab contains disconnected or duplicated children".into());
        }
        Ok(())
    }

    pub fn instantiate(
        &self,
        scene: &mut SceneGraph,
        parent: Option<SceneNodeId>,
    ) -> Result<SceneNodeId, String> {
        self.validate()?;
        if parent.is_some_and(|id| !scene.is_valid_node(id))
            || scene.len().saturating_add(self.nodes.len()) > 10_000
        {
            return Err("Invalid prefab parent or 10000-slot scene budget exceeded".into());
        }
        let identities: HashMap<_, _> = self
            .nodes
            .iter()
            .map(|n| (n.uuid, Uuid::new_v4()))
            .collect();
        let mut created: Vec<SceneNodeId> = Vec::with_capacity(self.nodes.len());
        for source in &self.nodes {
            let target_parent = source.parent.map(|p| created[p.0]).or(parent);
            let id = match target_parent {
                Some(parent) => scene.add_child(parent, &source.name),
                None => scene.add_root(&source.name),
            };
            let mut node = source.clone();
            node.uuid = identities[&source.uuid];
            node.parent = target_parent;
            node.children.clear();
            node.entity_index = None;
            // Reconciliation keys belong to the source; copies must not alias them.
            node.stable_key = None;
            remap_entity_references(&mut node, &identities);
            *scene.get_mut(id).expect("newly allocated node") = node;
            created.push(id);
        }
        Ok(created[0])
    }
}

/// Internal references follow copies; external UUIDs remain explicit external dependencies.
pub fn remap_entity_references(node: &mut SceneNode, identities: &HashMap<Uuid, Uuid>) {
    for variable in node
        .variables
        .iter_mut()
        .chain(node.behavior_parameters.values_mut().flatten())
    {
        if let VariableValue::Entity(Some(id)) = &mut variable.value {
            if let Some(replacement) = identities.get(id) {
                *id = *replacement;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn copies_remap_internal_parameters_keep_external_references_and_reject_bad_links() {
        let mut scene = SceneGraph::new();
        let root = scene.add_root("Rig");
        let camera = scene.add_child(root, "Camera");
        let external = Uuid::new_v4();
        let camera_uuid = scene.get(camera).unwrap().uuid;
        scene
            .get_mut(root)
            .unwrap()
            .set_variable("camera", VariableValue::Entity(Some(camera_uuid)));
        scene
            .get_mut(root)
            .unwrap()
            .set_variable("target", VariableValue::Entity(Some(external)));
        let prefab = ScenePrefab::capture(&scene, root).unwrap();
        let encoded = ron::to_string(&prefab).unwrap();
        let mut prefab: ScenePrefab = ron::from_str(&encoded).unwrap();
        let copy = prefab.instantiate(&mut scene, None).unwrap();
        let copied_camera = scene.get(copy).unwrap().children[0];
        assert_ne!(scene.get(copied_camera).unwrap().uuid, camera_uuid);
        assert_eq!(
            scene.get(copy).unwrap().get_variable("camera"),
            Some(&VariableValue::Entity(Some(
                scene.get(copied_camera).unwrap().uuid
            )))
        );
        assert_eq!(
            scene.get(copy).unwrap().get_variable("target"),
            Some(&VariableValue::Entity(Some(external)))
        );
        prefab.nodes[0].children.push(SceneNodeId(0));
        let count = scene.len();
        assert!(prefab.instantiate(&mut scene, None).is_err());
        assert_eq!(scene.len(), count);
    }
}
