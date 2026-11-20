//! Read-only scene graph queries, node lookup, and render fingerprints.

use super::{Primitive, SceneGraph, SceneNode, SceneNodeId};

impl SceneGraph {
    /// Get a reference to a node.
    pub fn get(&self, id: SceneNodeId) -> Option<&SceneNode> {
        self.nodes.get(id.0)
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
        let mut seen = Vec::new();
        let mut current = Some(id);
        while let Some(node_id) = current {
            if seen.contains(&node_id) {
                return None;
            }
            seen.push(node_id);
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

    /// Hashes only the scene fields that can change the rendered frame.
    ///
    /// This is deliberately not a persistence hash. It excludes editor-only
    /// metadata such as scripts and variables so an idle viewport can reuse
    /// its retained frame without treating unrelated authoring changes as
    /// renderer invalidations.
    pub fn render_fingerprint(&self) -> u64 {
        // Geometry caching is independent of the small, frequently changing environment.
        self.geometry_render_fingerprint() ^ self.environment.render_fingerprint().rotate_left(17)
    }

    fn geometry_render_fingerprint(&self) -> u64 {
        if let Some(cached) = self.render_cache.get() {
            return cached;
        }
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        mix_render_fingerprint(&mut hash, self.nodes.len() as u64);
        mix_render_fingerprint(&mut hash, self.roots.len() as u64);
        for root in &self.roots {
            mix_render_fingerprint(&mut hash, root.0 as u64);
        }

        for (index, node) in self.nodes.iter().enumerate() {
            mix_render_fingerprint(&mut hash, index as u64);
            mix_render_fingerprint(&mut hash, (!node.name.is_empty()) as u64);
            mix_render_fingerprint(&mut hash, node.visible as u64);
            mix_render_fingerprint(&mut hash, node.casts_shadows as u64);
            mix_render_fingerprint(&mut hash, node.receives_shadows as u64);
            mix_render_fingerprint(&mut hash, node.is_folder as u64);
            mix_render_fingerprint(&mut hash, primitive_fingerprint(node.primitive));
            mix_render_fingerprint(
                &mut hash,
                node.parent.map(|id| id.0 as u64).unwrap_or(u64::MAX),
            );
            mix_render_fingerprint(&mut hash, node.children.len() as u64);
            for child in &node.children {
                mix_render_fingerprint(&mut hash, child.0 as u64);
            }
            for value in [
                node.position.x,
                node.position.y,
                node.position.z,
                node.rotation.x,
                node.rotation.y,
                node.rotation.z,
                node.scale.x,
                node.scale.y,
                node.scale.z,
            ] {
                mix_render_fingerprint(&mut hash, value.to_bits() as u64);
            }
            for channel in [node.color.r, node.color.g, node.color.b, node.color.a] {
                mix_render_fingerprint(&mut hash, channel as u64);
            }
            if let Some(texture) = node.base_color_texture.as_deref() {
                mix_render_fingerprint(&mut hash, texture.len() as u64);
                for byte in texture.as_bytes() {
                    mix_render_fingerprint(&mut hash, u64::from(*byte));
                }
            } else {
                mix_render_fingerprint(&mut hash, u64::MAX);
            }
        }

        self.render_cache.set(Some(hash));
        hash
    }

    /// Iterate all nodes (flat).
    pub fn iter(&self) -> impl Iterator<Item = (SceneNodeId, &SceneNode)> {
        self.nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (SceneNodeId(i), n))
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

    /// Collect all live node ids, including nodes hidden in the editor.
    ///
    /// A removed node keeps its slot for stable ids but clears its name. This
    /// helper is the shared semantic count used by authoring, Agent context,
    /// CLI and MCP; callers that specifically need renderable nodes should use
    /// all_valid_ids instead.
    pub fn all_live_ids(&self) -> Vec<SceneNodeId> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| !node.name.is_empty())
            .map(|(index, _)| SceneNodeId(index))
            .collect()
    }
}

fn mix_render_fingerprint(hash: &mut u64, value: u64) {
    *hash ^= value;
    *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
}

fn primitive_fingerprint(primitive: Primitive) -> u64 {
    match primitive {
        Primitive::Empty => 0,
        Primitive::Cube => 1,
        Primitive::Sphere => 2,
        Primitive::Plane => 3,
        Primitive::Cylinder => 4,
        Primitive::Capsule => 5,
    }
}
