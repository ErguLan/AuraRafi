//! Scene graph with parent-child transform hierarchy.
//!
//! Uses a flat Vec with indices for O(1) lookup and cache-friendly iteration.
//! Each node stores its local transform; world transforms are computed lazily
//! when needed for rendering.

use glam::{Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a scene node (index into the graph's node vec).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SceneNodeId(pub usize);

/// A single node in the scene hierarchy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneNode {
    /// Stable UUID for serialization.
    pub uuid: Uuid,
    /// Human-readable name.
    pub name: String,
    /// Local position relative to parent.
    pub position: Vec3,
    /// Local rotation (Euler degrees for editor display, stored as quat internally).
    pub rotation: Vec3,
    /// Local scale.
    pub scale: Vec3,
    /// Parent index, `None` for root nodes.
    pub parent: Option<SceneNodeId>,
    /// Child indices.
    pub children: Vec<SceneNodeId>,
    /// Whether this node is visible.
    pub visible: bool,
    /// Associated ECS entity handle (optional, for linking with hecs).
    pub entity_index: Option<u32>,
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
            parent: None,
            children: Vec::new(),
            visible: true,
            entity_index: None,
        }
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

    /// Get a reference to a node.
    pub fn get(&self, id: SceneNodeId) -> Option<&SceneNode> {
        self.nodes.get(id.0)
    }

    /// Get a mutable reference to a node.
    pub fn get_mut(&mut self, id: SceneNodeId) -> Option<&mut SceneNode> {
        self.nodes.get_mut(id.0)
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
