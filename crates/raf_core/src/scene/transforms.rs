//! Frame-scoped world-transform snapshots for hot engine paths.

use glam::Mat4;

use super::graph::{SceneGraph, SceneNodeId};

/// Flat snapshot of active scene world transforms.
///
/// Build this once after a scene revision, then share it across rendering,
/// picking, physics, and any other per-frame traversal. It preserves the
/// existing editable scene graph while removing repeated parent-chain walks.
#[derive(Debug, Clone)]
pub struct WorldTransformCache {
    matrices: Vec<Mat4>,
    valid: Vec<bool>,
}

impl WorldTransformCache {
    /// Build a cache using the scene hierarchy in one top-down traversal.
    pub fn build(scene: &SceneGraph) -> Self {
        let mut matrices = vec![Mat4::IDENTITY; scene.len()];
        let mut valid = vec![false; scene.len()];
        let mut stack = Vec::with_capacity(scene.roots().len());

        for &root in scene.roots().iter().rev() {
            stack.push((root, Mat4::IDENTITY));
        }

        while let Some((id, parent_world)) = stack.pop() {
            if valid.get(id.0).copied().unwrap_or(false) {
                continue;
            }
            let Some(node) = scene.get(id) else {
                continue;
            };
            if node.name.is_empty() {
                continue;
            }

            let world = parent_world * node.local_matrix();
            matrices[id.0] = world;
            valid[id.0] = true;

            for &child in node.children.iter().rev() {
                stack.push((child, world));
            }
        }

        // Keep malformed or legacy orphaned nodes renderable without turning
        // the normal hierarchy traversal into repeated recursive walks.
        for (id, node) in scene.iter() {
            if !node.name.is_empty() && !valid[id.0] {
                matrices[id.0] = scene.world_matrix(id);
                valid[id.0] = true;
            }
        }

        Self { matrices, valid }
    }

    /// Returns the cached world transform for an active node.
    pub fn world_matrix(&self, id: SceneNodeId) -> Option<Mat4> {
        self.valid
            .get(id.0)
            .copied()
            .filter(|is_valid| *is_valid)
            .and_then(|_| self.matrices.get(id.0).copied())
    }

    /// Number of active transforms represented by this cache.
    pub fn active_count(&self) -> usize {
        self.valid.iter().filter(|is_valid| **is_valid).count()
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec3;

    use super::*;

    #[test]
    fn cache_matches_nested_world_matrix() {
        let mut scene = SceneGraph::new();
        let root = scene.add_root("Root");
        let child = scene.add_child(root, "Child");
        scene.get_mut(root).unwrap().position = Vec3::new(2.0, 0.0, 0.0);
        scene.get_mut(child).unwrap().position = Vec3::new(0.0, 3.0, 0.0);

        let cache = WorldTransformCache::build(&scene);
        let cached = cache.world_matrix(child).expect("cached child transform");

        assert_eq!(cached, scene.world_matrix(child));
        assert_eq!(cached.col(3).truncate(), Vec3::new(2.0, 3.0, 0.0));
        assert_eq!(cache.active_count(), 2);
    }
}
