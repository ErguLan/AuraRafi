//! World-space transform evaluation for the scene graph.

use glam::{Mat4, Vec3};

use super::{SceneGraph, SceneNodeId};

impl SceneGraph {
    /// Compute the world (global) transform matrix for a node by walking
    /// up the parent chain. This is intentionally not cached so that the
    /// graph stays simple; for hot rendering paths the renderer should
    /// pre-compute a flat buffer.
    pub fn world_matrix(&self, id: SceneNodeId) -> Mat4 {
        let mut chain = Vec::with_capacity(8);
        if id.0 >= self.nodes.len() {
            return Mat4::IDENTITY;
        }
        let mut current = Some(id);
        while let Some(cid) = current {
            if chain.contains(&cid) {
                return Mat4::IDENTITY;
            }
            chain.push(cid);
            current = self
                .nodes
                .get(cid.0)
                .and_then(|node| node.parent)
                .filter(|parent| parent.0 < self.nodes.len());
        }
        let mut mat = Mat4::IDENTITY;
        for cid in chain.into_iter().rev() {
            if let Some(node) = self.nodes.get(cid.0) {
                mat = mat * node.local_matrix();
            }
        }
        mat
    }

    /// Reparent an existing node while keeping its world-space transform.
    ///
    /// The graph stores local transforms, so changing parents normally changes
    /// the rendered position. This variant derives the new local transform
    /// from the old world matrix before changing the hierarchy. It is the
    /// safe primitive used by authoring tools that reorganize an existing
    /// scene without visually moving its content.
    pub fn reparent_node_preserve_world_transform(
        &mut self,
        id: SceneNodeId,
        new_parent: Option<SceneNodeId>,
    ) -> bool {
        if !self.is_valid_node(id) {
            return false;
        }
        if let Some(parent_id) = new_parent {
            if !self.is_valid_node(parent_id)
                || parent_id == id
                || self.is_descendant(parent_id, id)
            {
                return false;
            }
        }

        let world_before = self.world_matrix(id);
        let parent_world = new_parent
            .map(|parent_id| self.world_matrix(parent_id))
            .unwrap_or(Mat4::IDENTITY);
        let local = parent_world.inverse() * world_before;
        if !local.is_finite() {
            return false;
        }

        let (scale, rotation, position) = local.to_scale_rotation_translation();
        if !scale.is_finite() || !rotation.is_finite() || !position.is_finite() {
            return false;
        }
        let (yaw, pitch, roll) = rotation.to_euler(glam::EulerRot::YXZ);
        let local_rotation = Vec3::new(pitch.to_degrees(), yaw.to_degrees(), roll.to_degrees());

        if !self.reparent_node(id, new_parent) {
            return false;
        }
        let Some(node) = self.get_mut(id) else {
            return false;
        };
        node.position = position;
        node.rotation = local_rotation;
        node.scale = scale;
        true
    }
}
