//! Scene graph hierarchy mutation: creation, reparenting, removal, and duplication.

use glam::Vec3;
use uuid::Uuid;

use super::{Primitive, SceneGraph, SceneNode, SceneNodeId};

impl SceneGraph {
    /// Add a root node and return its id.
    pub fn add_root(&mut self, name: &str) -> SceneNodeId {
        self.invalidate();
        let id = SceneNodeId(self.nodes.len());
        self.nodes.push(SceneNode::new(name));
        self.roots.push(id);
        id
    }

    /// Add a child node under the given parent. Returns the child's id.
    pub fn add_child(&mut self, parent: SceneNodeId, name: &str) -> SceneNodeId {
        self.invalidate();
        let child_id = SceneNodeId(self.nodes.len());
        let mut child = SceneNode::new(name);
        child.parent = Some(parent);
        self.nodes.push(child);
        self.nodes[parent.0].children.push(child_id);
        child_id
    }

    /// Add a visible primitive under the given parent and return its id.
    pub fn add_child_with_primitive(
        &mut self,
        parent: SceneNodeId,
        name: &str,
        primitive: Primitive,
    ) -> SceneNodeId {
        self.invalidate();
        let child_id = SceneNodeId(self.nodes.len());
        let mut child = SceneNode::with_primitive(name, primitive);
        child.parent = Some(parent);
        self.nodes.push(child);
        self.nodes[parent.0].children.push(child_id);
        child_id
    }

    /// Add a root folder node.
    pub fn add_root_folder(&mut self, name: &str) -> SceneNodeId {
        self.invalidate();
        let id = SceneNodeId(self.nodes.len());
        self.nodes.push(SceneNode::folder(name));
        self.roots.push(id);
        id
    }

    /// Add a folder node under the given parent.
    pub fn add_child_folder(&mut self, parent: SceneNodeId, name: &str) -> SceneNodeId {
        self.invalidate();
        let child_id = SceneNodeId(self.nodes.len());
        let mut child = SceneNode::folder(name);
        child.parent = Some(parent);
        self.nodes.push(child);
        self.nodes[parent.0].children.push(child_id);
        child_id
    }

    /// Get a mutable reference to a node.
    pub fn get_mut(&mut self, id: SceneNodeId) -> Option<&mut SceneNode> {
        self.invalidate();
        self.nodes.get_mut(id.0)
    }

    /// Add a root node with a specific primitive and return its id.
    pub fn add_root_with_primitive(&mut self, name: &str, primitive: Primitive) -> SceneNodeId {
        self.invalidate();
        let id = SceneNodeId(self.nodes.len());
        self.nodes.push(SceneNode::with_primitive(name, primitive));
        self.roots.push(id);
        id
    }

    /// Reparent an existing node, optionally before a specific sibling.
    /// If `before` is Some, the node is inserted right before that sibling
    /// (must share the same parent as `new_parent`). If None, it appends at end.
    pub fn reparent_node_before(
        &mut self,
        id: SceneNodeId,
        new_parent: Option<SceneNodeId>,
        before: Option<SceneNodeId>,
    ) -> bool {
        self.reparent_nodes_before(&[id], new_parent, before)
    }

    /// Reparent several sibling-independent nodes as one hierarchy operation.
    ///
    /// The source list is detached first and inserted in the same order. This
    /// keeps multi-selection drag/drop undoable as one editor mutation and
    /// avoids the reverse-order bug caused by inserting each source before the
    /// same sibling independently.
    pub fn reparent_nodes_before(
        &mut self,
        ids: &[SceneNodeId],
        new_parent: Option<SceneNodeId>,
        before: Option<SceneNodeId>,
    ) -> bool {
        let mut sources = Vec::with_capacity(ids.len());
        for &id in ids {
            if self.is_valid_node(id) && !sources.contains(&id) {
                sources.push(id);
            }
        }
        if sources.is_empty() {
            return false;
        }

        if let Some(parent_id) = new_parent {
            if !self.is_valid_node(parent_id)
                || sources.contains(&parent_id)
                || sources
                    .iter()
                    .any(|source| self.is_descendant(parent_id, *source))
            {
                return false;
            }
        }

        // A subtree may only be moved once. Selecting both an ancestor and a
        // descendant would otherwise flatten the descendant into the target.
        if sources.iter().enumerate().any(|(index, source)| {
            sources.iter().enumerate().any(|(other_index, other)| {
                index != other_index && self.is_descendant(*other, *source)
            })
        }) {
            return false;
        }

        if let Some(before_id) = before {
            let Some(before_node) = self.nodes.get(before_id.0) else {
                return false;
            };
            if before_node.parent != new_parent || sources.contains(&before_id) {
                return false;
            }
        }

        self.invalidate();
        for &id in &sources {
            if let Some(old_parent) = self.nodes[id.0].parent {
                self.nodes[old_parent.0]
                    .children
                    .retain(|child| *child != id);
            } else {
                self.roots.retain(|root_id| *root_id != id);
            }
        }

        for &id in &sources {
            self.nodes[id.0].parent = new_parent;
        }

        let target = if let Some(parent_id) = new_parent {
            &mut self.nodes[parent_id.0].children
        } else {
            &mut self.roots
        };
        let insert_at = before
            .and_then(|before_id| target.iter().position(|child| *child == before_id))
            .unwrap_or(target.len());
        target.splice(insert_at..insert_at, sources.iter().copied());
        true
    }

    /// Reparent an existing node under a new parent or back to root.
    pub fn reparent_node(&mut self, id: SceneNodeId, new_parent: Option<SceneNodeId>) -> bool {
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

        self.invalidate();
        if let Some(old_parent) = self.nodes[id.0].parent {
            self.nodes[old_parent.0]
                .children
                .retain(|child| *child != id);
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

        self.invalidate();
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
        self.duplicate_with_references(id, parent)
    }

    /// Duplicate a node and its subtree directly under a requested parent.
    /// This is used by the editor's hierarchy paste operation.
    pub fn duplicate_node_into(
        &mut self,
        id: SceneNodeId,
        parent: Option<SceneNodeId>,
    ) -> Option<SceneNodeId> {
        if !self.is_valid_node(id) {
            return None;
        }
        if let Some(parent_id) = parent {
            if !self.is_valid_node(parent_id)
                || parent_id == id
                || self.is_descendant(parent_id, id)
            {
                return None;
            }
        }
        self.duplicate_with_references(id, parent)
    }

    /// Ungroup a folder by moving its children to its parent (or root).
    pub fn ungroup_node(&mut self, id: SceneNodeId) -> bool {
        if !self.is_valid_node(id) || !self.nodes[id.0].is_folder {
            return false;
        }

        let parent = self.nodes[id.0].parent;
        let children = self.nodes[id.0].children.clone();
        self.invalidate();

        for child_id in &children {
            self.nodes[child_id.0].parent = parent;
        }

        if let Some(parent_id) = parent {
            let insert_at = self.nodes[parent_id.0]
                .children
                .iter()
                .position(|child_id| *child_id == id)
                .unwrap_or(self.nodes[parent_id.0].children.len());
            self.nodes[parent_id.0]
                .children
                .retain(|child_id| *child_id != id);

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

    fn duplicate_with_references(
        &mut self,
        source: SceneNodeId,
        parent: Option<SceneNodeId>,
    ) -> Option<SceneNodeId> {
        let start = self.nodes.len();
        let mut identities = std::collections::HashMap::new();
        let root = self.duplicate_subtree_internal(source, parent, true, &mut identities)?;
        for node in &mut self.nodes[start..] {
            crate::scene::prefab::remap_entity_references(node, &identities);
        }
        Some(root)
    }

    fn duplicate_subtree_internal(
        &mut self,
        source_id: SceneNodeId,
        parent: Option<SceneNodeId>,
        offset_root: bool,
        identities: &mut std::collections::HashMap<Uuid, Uuid>,
    ) -> Option<SceneNodeId> {
        let source = self.nodes.get(source_id.0)?.clone();
        self.invalidate();
        let new_id = SceneNodeId(self.nodes.len());
        let mut copy = source.clone();
        copy.uuid = Uuid::new_v4();
        identities.insert(source.uuid, copy.uuid);
        copy.entity_index = None;
        copy.name = format!("{} (copy)", source.name);
        // Stable reconciliation keys must remain unique. The semantic role
        // and tags describe what the copy is, but a duplicate receives a new
        // key only when an authoring tool explicitly assigns one.
        copy.stable_key = None;
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
            let _ = self.duplicate_subtree_internal(child_id, Some(new_id), false, identities);
        }

        Some(new_id)
    }
}
