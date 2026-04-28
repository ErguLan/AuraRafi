//! Scene graph with parent-child transform hierarchy.
//!
//! Uses a flat Vec with indices for O(1) lookup and cache-friendly iteration.
//! Each node stores its local transform; world transforms are computed lazily
//! when needed for rendering.

use glam::{Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::scene::{AudioSource, Collider, RigidBody, SceneVariable, VariableValue};

/// Unique identifier for a scene node (index into the graph's node vec).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SceneNodeId(pub usize);

/// Primitive shape for rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Primitive {
    /// No visible geometry (group/empty node).
    Empty,
    /// Unit cube centered at origin.
    Cube,
    /// Unit sphere.
    Sphere,
    /// Flat plane on XZ.
    Plane,
    /// Cylinder along Y axis.
    Cylinder,
    /// 2D sprite (for 2D mode).
    Sprite2D,
}

impl Default for Primitive {
    fn default() -> Self {
        Self::Empty
    }
}

impl Primitive {
    /// Display name for the UI.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Empty => "Empty",
            Self::Cube => "Cube",
            Self::Sphere => "Sphere",
            Self::Plane => "Plane",
            Self::Cylinder => "Cylinder",
            Self::Sprite2D => "Sprite2D",
        }
    }

    /// Display name in Spanish.
    pub fn label_es(&self) -> &'static str {
        match self {
            Self::Empty => "Vac\u{00ed}o",
            Self::Cube => "Cubo",
            Self::Sphere => "Esfera",
            Self::Plane => "Plano",
            Self::Cylinder => "Cilindro",
            Self::Sprite2D => "Sprite2D",
        }
    }
}

/// RGBA color for an entity (0-255 per channel).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct NodeColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl NodeColor {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Default entity colors by primitive type.
    pub fn for_primitive(prim: Primitive) -> Self {
        match prim {
            Primitive::Empty => Self::rgb(120, 120, 120),
            Primitive::Cube => Self::rgb(90, 160, 220),
            Primitive::Sphere => Self::rgb(220, 130, 50),
            Primitive::Plane => Self::rgb(100, 180, 100),
            Primitive::Cylinder => Self::rgb(180, 100, 180),
            Primitive::Sprite2D => Self::rgb(220, 200, 80),
        }
    }
}

impl Default for NodeColor {
    fn default() -> Self {
        Self::rgb(180, 180, 180)
    }
}

/// A single node in the scene hierarchy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneNode {
    /// Stable UUID for serialization.
    pub uuid: Uuid,
    /// Human-readable name.
    pub name: String,
    /// Local position relative to parent.
    pub position: Vec3,
    /// Local rotation (Euler degrees for editor display).
    pub rotation: Vec3,
    /// Local scale.
    pub scale: Vec3,
    /// Visual primitive shape.
    pub primitive: Primitive,
    /// Display color.
    pub color: NodeColor,
    /// Parent index, `None` for root nodes.
    pub parent: Option<SceneNodeId>,
    /// Child indices.
    pub children: Vec<SceneNodeId>,
    /// Whether this node is visible.
    pub visible: bool,
    /// Associated ECS entity handle (optional, for linking with hecs).
    pub entity_index: Option<u32>,
    /// External script files attached to this entity (e.g. VS Code edited logic).
    pub scripts: Vec<String>,
    /// Script-facing custom variables for this entity.
    #[serde(default)]
    pub variables: Vec<SceneVariable>,
    /// Runtime audio source settings.
    #[serde(default)]
    pub audio_source: AudioSource,
    /// Collider configuration used by the runtime.
    #[serde(default)]
    pub collider: Collider,
    /// Physics body state for runtime simulation.
    #[serde(default)]
    pub rigid_body: RigidBody,
    /// Organizational folder/group node inside the hierarchy.
    #[serde(default)]
    pub is_folder: bool,
}

impl SceneNode {
    /// Create a new node with default transform.
    pub fn new(name: &str) -> Self {
        Self {
            uuid: Uuid::new_v4(),
            name: name.to_string(),
            position: Vec3::ZERO,
            rotation: Vec3::ZERO,
            scale: Vec3::ONE,
            primitive: Primitive::Empty,
            color: NodeColor::default(),
            parent: None,
            children: Vec::new(),
            visible: true,
            entity_index: None,
            scripts: Vec::new(),
            variables: Vec::new(),
            audio_source: AudioSource::default(),
            collider: Collider::default(),
            rigid_body: RigidBody::default(),
            is_folder: false,
        }
    }

    /// Create a node with a specific primitive.
    pub fn with_primitive(name: &str, primitive: Primitive) -> Self {
        Self {
            uuid: Uuid::new_v4(),
            name: name.to_string(),
            position: Vec3::ZERO,
            rotation: Vec3::ZERO,
            scale: Vec3::ONE,
            primitive,
            color: NodeColor::for_primitive(primitive),
            parent: None,
            children: Vec::new(),
            visible: true,
            entity_index: None,
            scripts: Vec::new(),
            variables: Vec::new(),
            audio_source: AudioSource::default(),
            collider: Collider::default(),
            rigid_body: RigidBody::default(),
            is_folder: false,
        }
    }

    /// Create a folder/group node. Uses Empty primitive but is semantically distinct.
    pub fn folder(name: &str) -> Self {
        let mut node = Self::new(name);
        node.is_folder = true;
        node
    }

    /// Compute the local-to-parent transformation matrix.
    pub fn local_matrix(&self) -> Mat4 {
        let rotation_quat = Quat::from_euler(
            glam::EulerRot::YXZ,
            self.rotation.y.to_radians(),
            self.rotation.x.to_radians(),
            self.rotation.z.to_radians(),
        );
        Mat4::from_scale_rotation_translation(self.scale, rotation_quat, self.position)
    }

    pub fn get_variable(&self, name: &str) -> Option<&VariableValue> {
        self.variables
            .iter()
            .find(|variable| variable.name == name)
            .map(|variable| &variable.value)
    }

    pub fn set_variable(&mut self, name: &str, value: VariableValue) {
        if let Some(variable) = self.variables.iter_mut().find(|variable| variable.name == name) {
            variable.value = value;
            return;
        }

        self.variables.push(SceneVariable {
            name: name.to_string(),
            value,
        });
    }
}

/// Flat-array scene graph. All nodes live in a contiguous `Vec` for
/// cache-friendly iteration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneGraph {
    nodes: Vec<SceneNode>,
    /// Indices of root-level nodes (no parent).
    roots: Vec<SceneNodeId>,
}

impl SceneGraph {
    /// Create an empty scene graph.
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            roots: Vec::new(),
        }
    }

    /// Add a root node and return its id.
    pub fn add_root(&mut self, name: &str) -> SceneNodeId {
        let id = SceneNodeId(self.nodes.len());
        self.nodes.push(SceneNode::new(name));
        self.roots.push(id);
        id
    }

    /// Add a child node under the given parent. Returns the child's id.
    pub fn add_child(&mut self, parent: SceneNodeId, name: &str) -> SceneNodeId {
        let child_id = SceneNodeId(self.nodes.len());
        let mut child = SceneNode::new(name);
        child.parent = Some(parent);
        self.nodes.push(child);
        self.nodes[parent.0].children.push(child_id);
        child_id
    }

    /// Add a root folder node.
    pub fn add_root_folder(&mut self, name: &str) -> SceneNodeId {
        let id = SceneNodeId(self.nodes.len());
        self.nodes.push(SceneNode::folder(name));
        self.roots.push(id);
        id
    }

    /// Add a folder node under the given parent.
    pub fn add_child_folder(&mut self, parent: SceneNodeId, name: &str) -> SceneNodeId {
        let child_id = SceneNodeId(self.nodes.len());
        let mut child = SceneNode::folder(name);
        child.parent = Some(parent);
        self.nodes.push(child);
        self.nodes[parent.0].children.push(child_id);
        child_id
    }

    /// Get a reference to a node.
    pub fn get(&self, id: SceneNodeId) -> Option<&SceneNode> {
        self.nodes.get(id.0)
    }

    /// Get a mutable reference to a node.
    pub fn get_mut(&mut self, id: SceneNodeId) -> Option<&mut SceneNode> {
        self.nodes.get_mut(id.0)
    }

    /// Check if an id points to an active node.
    pub fn is_valid_node(&self, id: SceneNodeId) -> bool {
        self.nodes
            .get(id.0)
            .map(|node| !node.name.is_empty())
            .unwrap_or(false)
    }

    /// Find first active node by exact name.
    pub fn find_node_by_name(&self, name: &str) -> Option<SceneNodeId> {
        self.nodes
            .iter()
            .enumerate()
            .find(|(_, node)| !node.name.is_empty() && node.name == name)
            .map(|(index, _)| SceneNodeId(index))
    }

    pub fn node_path(&self, id: SceneNodeId) -> Option<String> {
        if !self.is_valid_node(id) {
            return None;
        }

        let mut segments = Vec::new();
        let mut current = Some(id);
        while let Some(node_id) = current {
            let node = self.nodes.get(node_id.0)?;
            if node.name.is_empty() {
                return None;
            }
            segments.push(node.name.clone());
            current = node.parent;
        }
        segments.reverse();
        Some(format!("/{}", segments.join("/")))
    }

    pub fn find_node_by_path(&self, path: &str) -> Option<SceneNodeId> {
        let normalized = path.trim().trim_matches('/');
        if normalized.is_empty() {
            return None;
        }

        self.iter().find_map(|(id, _)| {
            self.node_path(id)
                .filter(|candidate| candidate.trim_matches('/') == normalized)
                .map(|_| id)
        })
    }

    /// Compute the world (global) transform matrix for a node by walking
    /// up the parent chain. This is intentionally not cached so that the
    /// graph stays simple; for hot rendering paths the renderer should
    /// pre-compute a flat buffer.
    pub fn world_matrix(&self, id: SceneNodeId) -> Mat4 {
        let mut chain = Vec::with_capacity(8);
        let mut current = Some(id);
        while let Some(cid) = current {
            chain.push(cid);
            current = self.nodes[cid.0].parent;
        }
        let mut mat = Mat4::IDENTITY;
        for cid in chain.into_iter().rev() {
            mat = mat * self.nodes[cid.0].local_matrix();
        }
        mat
    }

    /// All root node ids.
    pub fn roots(&self) -> &[SceneNodeId] {
        &self.roots
    }

    /// Total node count.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Check if the graph is empty.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Iterate all nodes (flat).
    pub fn iter(&self) -> impl Iterator<Item = (SceneNodeId, &SceneNode)> {
        self.nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (SceneNodeId(i), n))
    }

    /// Add a root node with a specific primitive and return its id.
    pub fn add_root_with_primitive(&mut self, name: &str, primitive: Primitive) -> SceneNodeId {
        let id = SceneNodeId(self.nodes.len());
        self.nodes.push(SceneNode::with_primitive(name, primitive));
        self.roots.push(id);
        id
    }

    /// Reparent an existing node under a new parent or back to root.
    pub fn reparent_node(&mut self, id: SceneNodeId, new_parent: Option<SceneNodeId>) -> bool {
        if !self.is_valid_node(id) {
            return false;
        }

        if let Some(parent_id) = new_parent {
            if !self.is_valid_node(parent_id) || parent_id == id || self.is_descendant(parent_id, id) {
                return false;
            }
        }

        if let Some(old_parent) = self.nodes[id.0].parent {
            self.nodes[old_parent.0].children.retain(|child| *child != id);
        } else {
            self.roots.retain(|root_id| *root_id != id);
        }

        self.nodes[id.0].parent = new_parent;

        if let Some(parent_id) = new_parent {
            if !self.nodes[parent_id.0].children.contains(&id) {
                self.nodes[parent_id.0].children.push(id);
            }
        } else if !self.roots.contains(&id) {
            self.roots.push(id);
        }

        true
    }

    /// Soft-remove a node: hides it, clears its primitive, and detaches from
    /// parent/root list. We keep the slot to avoid invalidating indices.
    pub fn remove_node(&mut self, id: SceneNodeId) -> bool {
        if id.0 >= self.nodes.len() {
            return false;
        }

        // Remove from parent's children list.
        if let Some(parent_id) = self.nodes[id.0].parent {
            if parent_id.0 < self.nodes.len() {
                self.nodes[parent_id.0].children.retain(|c| *c != id);
            }
        }

        // Remove from roots list if it is a root.
        self.roots.retain(|r| *r != id);

        // Also remove children recursively (soft).
        let children: Vec<SceneNodeId> = self.nodes[id.0].children.clone();
        for child_id in children {
            self.remove_node(child_id);
        }

        // Clear the node.
        self.nodes[id.0].visible = false;
        self.nodes[id.0].primitive = Primitive::Empty;
        self.nodes[id.0].children.clear();
        self.nodes[id.0].parent = None;
        self.nodes[id.0].name = String::new();
        self.nodes[id.0].is_folder = false;
        true
    }

    /// Duplicate a node and its subtree. The duplicate is created next to the
    /// original, preserving folder/group structure and child relationships.
    pub fn duplicate_node(&mut self, id: SceneNodeId) -> Option<SceneNodeId> {
        if !self.is_valid_node(id) {
            return None;
        }

        let parent = self.nodes[id.0].parent;
        self.duplicate_subtree_internal(id, parent, true)
    }

    /// Ungroup a folder by moving its children to its parent (or root).
    pub fn ungroup_node(&mut self, id: SceneNodeId) -> bool {
        if !self.is_valid_node(id) || !self.nodes[id.0].is_folder {
            return false;
        }

        let parent = self.nodes[id.0].parent;
        let children = self.nodes[id.0].children.clone();

        for child_id in &children {
            self.nodes[child_id.0].parent = parent;
        }

        if let Some(parent_id) = parent {
            let insert_at = self.nodes[parent_id.0]
                .children
                .iter()
                .position(|child_id| *child_id == id)
                .unwrap_or(self.nodes[parent_id.0].children.len());
            self.nodes[parent_id.0].children.retain(|child_id| *child_id != id);

            for (offset, child_id) in children.iter().enumerate() {
                self.nodes[parent_id.0]
                    .children
                    .insert(insert_at + offset, *child_id);
            }
        } else {
            self.roots.retain(|root_id| *root_id != id);
            for child_id in children {
                if !self.roots.contains(&child_id) {
                    self.roots.push(child_id);
                }
            }
        }

        self.nodes[id.0].children.clear();
        self.remove_node(id)
    }

    /// Collect all valid (visible, non-empty name) node ids.
    pub fn all_valid_ids(&self) -> Vec<SceneNodeId> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| !n.name.is_empty() && n.visible)
            .map(|(i, _)| SceneNodeId(i))
            .collect()
    }

    /// Save the scene graph to a RON file.
    pub fn save_ron(&self, path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
        let pretty = ron::ser::PrettyConfig::default();
        let data = ron::ser::to_string_pretty(self, pretty)?;
        std::fs::write(path, data)?;
        Ok(())
    }

    /// Load a scene graph from a RON file. Returns default if file missing.
    pub fn load_ron(path: &std::path::Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(data) => ron::from_str(&data).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    fn is_descendant(&self, candidate: SceneNodeId, ancestor: SceneNodeId) -> bool {
        let mut current = Some(candidate);
        while let Some(node_id) = current {
            if node_id == ancestor {
                return true;
            }
            current = self.nodes[node_id.0].parent;
        }
        false
    }

    fn duplicate_subtree_internal(
        &mut self,
        source_id: SceneNodeId,
        parent: Option<SceneNodeId>,
        offset_root: bool,
    ) -> Option<SceneNodeId> {
        let source = self.nodes.get(source_id.0)?.clone();
        let new_id = SceneNodeId(self.nodes.len());
        let mut copy = source.clone();
        copy.uuid = Uuid::new_v4();
        copy.name = format!("{} (copy)", source.name);
        copy.parent = parent;
        copy.children.clear();
        if offset_root {
            copy.position += Vec3::new(1.0, 0.0, 0.0);
        }
        self.nodes.push(copy);

        if let Some(parent_id) = parent {
            self.nodes[parent_id.0].children.push(new_id);
        } else {
            self.roots.push(new_id);
        }

        for child_id in source.children {
            let _ = self.duplicate_subtree_internal(child_id, Some(new_id), false);
        }

        Some(new_id)
    }
}

impl Default for SceneGraph {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hierarchy_basics() {
        let mut graph = SceneGraph::new();
        let root = graph.add_root("Root");
        let child = graph.add_child(root, "Child");

        assert_eq!(graph.len(), 2);
        assert_eq!(graph.roots().len(), 1);

        let root_node = graph.get(root).unwrap();
        assert_eq!(root_node.children.len(), 1);

        let child_node = graph.get(child).unwrap();
        assert_eq!(child_node.parent, Some(root));
    }

    #[test]
    fn world_matrix_propagation() {
        let mut graph = SceneGraph::new();
        let root = graph.add_root("Root");
        graph.get_mut(root).unwrap().position = Vec3::new(10.0, 0.0, 0.0);

        let child = graph.add_child(root, "Child");
        graph.get_mut(child).unwrap().position = Vec3::new(0.0, 5.0, 0.0);

        let world = graph.world_matrix(child);
        let translation = world.col(3);
        assert!((translation.x - 10.0).abs() < 0.001);
        assert!((translation.y - 5.0).abs() < 0.001);
    }
}
