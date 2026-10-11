//! Shared, unambiguous scene lookup used by the Host API and static tooling.
use raf_core::scene::{SceneGraph, SceneNodeId};
use std::collections::HashMap;
use uuid::Uuid;

/// Rebuilt only when authoring changes. Static tooling resolves many references
/// without repeating a full scene scan for every script and literal.
pub struct SceneLookup<'a> {
    scene: &'a SceneGraph,
    names: HashMap<&'a str, Option<SceneNodeId>>,
    uuids: HashMap<Uuid, Option<SceneNodeId>>,
    roots: HashMap<&'a str, Option<SceneNodeId>>,
    children: HashMap<(SceneNodeId, &'a str), Option<SceneNodeId>>,
}
impl<'a> SceneLookup<'a> {
    pub fn new(scene: &'a SceneGraph) -> Self {
        let mut lookup = Self {
            scene,
            names: HashMap::new(),
            uuids: HashMap::new(),
            roots: HashMap::new(),
            children: HashMap::new(),
        };
        let roots: std::collections::HashSet<_> = scene.roots().iter().copied().collect();
        for (id, node) in scene.iter().filter(|(id, _)| scene.is_valid_node(*id)) {
            lookup
                .names
                .entry(&node.name)
                .and_modify(|v| *v = None)
                .or_insert(Some(id));
            lookup
                .uuids
                .entry(node.uuid)
                .and_modify(|v| *v = None)
                .or_insert(Some(id));
            if roots.contains(&id) {
                lookup
                    .roots
                    .entry(&node.name)
                    .and_modify(|v| *v = None)
                    .or_insert(Some(id));
            }
            for child in &node.children {
                if scene.is_valid_node(*child) {
                    if let Some(child_node) = scene.get(*child) {
                        lookup
                            .children
                            .entry((id, &child_node.name))
                            .and_modify(|v| *v = None)
                            .or_insert(Some(*child));
                    }
                }
            }
        }
        lookup
    }
    pub fn root(&self, name: &str) -> Option<SceneNodeId> {
        self.roots.get(name).copied().flatten()
    }
    pub fn child(&self, id: SceneNodeId, name: &str) -> Option<SceneNodeId> {
        self.children.get(&(id, name)).copied().flatten()
    }
    pub fn parent(&self, id: SceneNodeId) -> Option<SceneNodeId> {
        self.scene
            .get(id)?
            .parent
            .filter(|id| self.scene.is_valid_node(*id))
    }
    pub fn valid(&self, id: SceneNodeId) -> bool {
        self.scene.is_valid_node(id)
    }
    pub fn parameter(
        &self,
        owner: SceneNodeId,
        source: Option<&str>,
        name: &str,
    ) -> Option<SceneNodeId> {
        let node = self.scene.get(owner)?;
        let value = source
            .and_then(|source| node.behavior_parameters.get(source))
            .and_then(|values| values.iter().find(|p| p.name == name))
            .map(|p| &p.value)
            .or_else(|| node.get_variable(name))?;
        match value {
            raf_core::scene::VariableValue::Entity(Some(id)) => {
                self.uuids.get(id).copied().flatten()
            }
            _ => None,
        }
    }
    pub fn entity(&self, reference: &str) -> Option<SceneNodeId> {
        if reference.is_empty() || reference.len() > 1024 {
            return None;
        }
        if let Ok(uuid) = Uuid::parse_str(reference) {
            return self.uuids.get(&uuid).copied().flatten();
        }
        if !reference.contains('/') {
            return self.names.get(reference).copied().flatten();
        }
        let mut current = None;
        for name in reference.trim_start_matches('/').split('/') {
            if matches!(name, "" | "." | "..") {
                return None;
            }
            current = Some(match current {
                Some(id) => self.child(id, name)?,
                None => self.root(name)?,
            });
        }
        current
    }
}

fn unique(mut ids: impl Iterator<Item = SceneNodeId>) -> Option<SceneNodeId> {
    let id = ids.next()?;
    ids.next().is_none().then_some(id)
}

pub fn child(scene: &SceneGraph, parent: SceneNodeId, name: &str) -> Option<SceneNodeId> {
    unique(scene.get(parent)?.children.iter().copied().filter(|id| {
        scene.is_valid_node(*id) && scene.get(*id).is_some_and(|node| node.name == name)
    }))
}

pub fn root_child(scene: &SceneGraph, name: &str) -> Option<SceneNodeId> {
    unique(scene.roots().iter().copied().filter(|id| {
        scene.is_valid_node(*id) && scene.get(*id).is_some_and(|node| node.name == name)
    }))
}

pub fn entity(scene: &SceneGraph, reference: &str) -> Option<SceneNodeId> {
    if reference.is_empty() || reference.len() > 1024 {
        return None;
    }
    if let Ok(uuid) = Uuid::parse_str(reference) {
        return unique(scene.iter().filter_map(|(id, node)| {
            (scene.is_valid_node(id) && node.uuid == uuid).then_some(id)
        }));
    }
    if !reference.contains('/') {
        return unique(scene.iter().filter_map(|(id, node)| {
            (scene.is_valid_node(id) && node.name == reference).then_some(id)
        }));
    }
    let mut current = None;
    for name in reference.trim_start_matches('/').split('/') {
        if matches!(name, "" | "." | "..") {
            return None;
        }
        current = Some(match current {
            Some(parent) => child(scene, parent, name)?,
            None => root_child(scene, name)?,
        });
    }
    current
}
