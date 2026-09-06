//! Scene graph with parent-child transform hierarchy.
//!
//! Uses a flat Vec with indices for O(1) lookup and cache-friendly iteration.
//! Each node stores its local transform; world transforms are computed lazily
//! when needed for rendering.

use glam::{Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::fmt;
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
    #[serde(alias = "Sprite2D", alias = "sprite2d")]
    Plane,
    /// Cylinder along Y axis.
    Cylinder,
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
        }
    }
}

/// RGBA color for an entity (0-255 per channel).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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
            Primitive::Cube => Self::rgb(224, 116, 24),
            Primitive::Sphere => Self::rgb(236, 236, 236),
            Primitive::Plane => Self::rgb(150, 150, 150),
            Primitive::Cylinder => Self::rgb(58, 58, 58),
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
    /// Prevents viewport transforms while keeping the node selectable.
    #[serde(default)]
    pub locked: bool,
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
    /// Optional asset/manifest identifier used to create this node.
    #[serde(default)]
    pub source_asset: Option<String>,
    /// Schema version of the source manifest, when this node came from one.
    #[serde(default)]
    pub source_schema_version: Option<u32>,
    /// Optional semantic role assigned by an authoring tool (for example
    /// `store.shelf`). It is metadata only and never changes rendering.
    #[serde(default)]
    pub semantic_role: Option<String>,
    /// Stable authoring key used by reconciliation workflows.
    #[serde(default)]
    pub stable_key: Option<String>,
    /// Searchable semantic tags for Agent perception and batch operations.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Task or tool that created the node, when known.
    #[serde(default)]
    pub agent_origin: Option<String>,
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
            locked: false,
            entity_index: None,
            scripts: Vec::new(),
            variables: Vec::new(),
            audio_source: AudioSource::default(),
            collider: Collider::default(),
            rigid_body: RigidBody::default(),
            is_folder: false,
            source_asset: None,
            source_schema_version: None,
            semantic_role: None,
            stable_key: None,
            tags: Vec::new(),
            agent_origin: None,
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
            locked: false,
            entity_index: None,
            scripts: Vec::new(),
            variables: Vec::new(),
            audio_source: AudioSource::default(),
            collider: Collider::default(),
            rigid_body: RigidBody::default(),
            is_folder: false,
            source_asset: None,
            source_schema_version: None,
            semantic_role: None,
            stable_key: None,
            tags: Vec::new(),
            agent_origin: None,
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
        if let Some(variable) = self
            .variables
            .iter_mut()
            .find(|variable| variable.name == name)
        {
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
#[derive(Clone, Serialize, Deserialize)]
pub struct SceneGraph {
    nodes: Vec<SceneNode>,
    /// Indices of root-level nodes (no parent).
    roots: Vec<SceneNodeId>,
    /// Cached render key. The cache is editor/runtime-local and is never
    /// serialized; every graph mutator invalidates it before changing nodes.
    #[serde(skip)]
    render_cache: Cell<Option<u64>>,
}

impl SceneGraph {
    /// Create an empty scene graph.
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            roots: Vec::new(),
            render_cache: Cell::new(None),
        }
    }

    /// Add a root node and return its id.
    pub fn add_root(&mut self, name: &str) -> SceneNodeId {
        self.render_cache.set(None);
        let id = SceneNodeId(self.nodes.len());
        self.nodes.push(SceneNode::new(name));
        self.roots.push(id);
        id
    }

    /// Add a child node under the given parent. Returns the child's id.
    pub fn add_child(&mut self, parent: SceneNodeId, name: &str) -> SceneNodeId {
        self.render_cache.set(None);
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
        self.render_cache.set(None);
        let child_id = SceneNodeId(self.nodes.len());
        let mut child = SceneNode::with_primitive(name, primitive);
        child.parent = Some(parent);
        self.nodes.push(child);
        self.nodes[parent.0].children.push(child_id);
        child_id
    }

    /// Add a root folder node.
    pub fn add_root_folder(&mut self, name: &str) -> SceneNodeId {
        self.render_cache.set(None);
        let id = SceneNodeId(self.nodes.len());
        self.nodes.push(SceneNode::folder(name));
        self.roots.push(id);
        id
    }

    /// Add a folder node under the given parent.
    pub fn add_child_folder(&mut self, parent: SceneNodeId, name: &str) -> SceneNodeId {
        self.render_cache.set(None);
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
        self.render_cache.set(None);
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

    /// Add a root node with a specific primitive and return its id.
    pub fn add_root_with_primitive(&mut self, name: &str, primitive: Primitive) -> SceneNodeId {
        self.render_cache.set(None);
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

        self.render_cache.set(None);
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

        self.render_cache.set(None);
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

    /// Soft-remove a node: hides it, clears its primitive, and detaches from
    /// parent/root list. We keep the slot to avoid invalidating indices.
    pub fn remove_node(&mut self, id: SceneNodeId) -> bool {
        if id.0 >= self.nodes.len() {
            return false;
        }

        self.render_cache.set(None);
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
        self.duplicate_subtree_internal(id, parent, true)
    }

    /// Ungroup a folder by moving its children to its parent (or root).
    pub fn ungroup_node(&mut self, id: SceneNodeId) -> bool {
        if !self.is_valid_node(id) || !self.nodes[id.0].is_folder {
            return false;
        }

        let parent = self.nodes[id.0].parent;
        let children = self.nodes[id.0].children.clone();
        self.render_cache.set(None);

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
        let mut seen = Vec::new();
        while let Some(node_id) = current {
            if node_id == ancestor {
                return true;
            }
            if seen.contains(&node_id) {
                return false;
            }
            seen.push(node_id);
            current = self
                .nodes
                .get(node_id.0)
                .and_then(|node| node.parent)
                .filter(|parent| parent.0 < self.nodes.len());
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
        self.render_cache.set(None);
        let new_id = SceneNodeId(self.nodes.len());
        let mut copy = source.clone();
        copy.uuid = Uuid::new_v4();
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
            let _ = self.duplicate_subtree_internal(child_id, Some(new_id), false);
        }

        Some(new_id)
    }
}

impl fmt::Debug for SceneGraph {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SceneGraph")
            .field("nodes", &self.nodes)
            .field("roots", &self.roots)
            .finish()
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

    #[test]
    fn reparent_preserves_world_transform() {
        let mut graph = SceneGraph::new();
        let source = graph.add_root("Source");
        graph.get_mut(source).unwrap().position = Vec3::new(10.0, 2.0, -4.0);
        let target = graph.add_root_folder("Target");
        graph.get_mut(target).unwrap().position = Vec3::new(-3.0, 5.0, 7.0);
        let child = graph.add_child_with_primitive(source, "Child", Primitive::Cube);
        graph.get_mut(child).unwrap().position = Vec3::new(2.0, 1.0, 3.0);

        let before = graph.world_matrix(child).to_cols_array();
        assert!(graph.reparent_node_preserve_world_transform(child, Some(target)));
        let after = graph.world_matrix(child).to_cols_array();

        assert_eq!(graph.get(child).unwrap().parent, Some(target));
        for (before, after) in before.iter().zip(after) {
            assert!((*before - after).abs() < 0.001);
        }
    }

    #[test]
    fn render_fingerprint_tracks_visual_changes_only() {
        let mut graph = SceneGraph::new();
        let node = graph.add_root_with_primitive("Cube", Primitive::Cube);
        let initial = graph.render_fingerprint();

        graph
            .get_mut(node)
            .unwrap()
            .scripts
            .push("logic.rhai".to_string());
        assert_eq!(graph.render_fingerprint(), initial);

        graph.get_mut(node).unwrap().position.x = 4.0;
        assert_ne!(graph.render_fingerprint(), initial);
    }

    #[test]
    fn legacy_sprite_primitive_deserializes_as_plane() {
        let primitive: Primitive = ron::from_str("Sprite2D").expect("legacy primitive alias");
        assert_eq!(primitive, Primitive::Plane);
    }

    #[test]
    fn reparent_before_preserves_sibling_order_and_rejects_cycles() {
        let mut graph = SceneGraph::new();
        let root = graph.add_root("Root");
        let first = graph.add_child(root, "First");
        let second = graph.add_child(root, "Second");
        let third = graph.add_child(root, "Third");

        assert!(graph.reparent_node_before(third, Some(root), Some(first)));
        assert_eq!(
            graph.get(root).unwrap().children,
            vec![third, first, second]
        );
        assert!(!graph.reparent_node_before(root, Some(third), None));
    }

    #[test]
    fn multi_reparent_preserves_source_order() {
        let mut graph = SceneGraph::new();
        let root = graph.add_root("Root");
        let first = graph.add_child(root, "First");
        let second = graph.add_child(root, "Second");
        let third = graph.add_child(root, "Third");
        let target = graph.add_root("Target");
        let before = graph.add_child(target, "Before");

        assert!(graph.reparent_nodes_before(&[first, second], Some(target), Some(before)));
        assert_eq!(
            graph.get(target).unwrap().children,
            vec![first, second, before]
        );
        assert_eq!(graph.get(root).unwrap().children, vec![third]);
    }

    #[test]
    fn multi_reparent_rejects_ancestor_and_descendant_selection() {
        let mut graph = SceneGraph::new();
        let root = graph.add_root("Root");
        let child = graph.add_child(root, "Child");
        let target = graph.add_root("Target");

        assert!(!graph.reparent_nodes_before(&[root, child], Some(target), None));
        assert_eq!(graph.roots(), &[root, target]);
        assert_eq!(graph.get(root).unwrap().children, vec![child]);
    }

    #[test]
    fn duplicate_into_preserves_subtree_under_requested_parent() {
        let mut graph = SceneGraph::new();
        let source = graph.add_root_folder("Source");
        graph.add_child(source, "Nested");
        let target = graph.add_root_folder("Target");

        let duplicate = graph.duplicate_node_into(source, Some(target)).unwrap();

        assert_eq!(graph.get(duplicate).unwrap().parent, Some(target));
        assert_eq!(graph.get(target).unwrap().children, vec![duplicate]);
        assert_eq!(graph.get(duplicate).unwrap().children.len(), 1);
        assert_ne!(
            graph.get(source).unwrap().uuid,
            graph.get(duplicate).unwrap().uuid
        );
    }

    #[test]
    fn render_fingerprint_tracks_child_order() {
        let mut graph = SceneGraph::new();
        let root = graph.add_root("Root");
        let first = graph.add_child(root, "First");
        let second = graph.add_child(root, "Second");
        let before = graph.render_fingerprint();

        assert!(graph.reparent_node_before(second, Some(root), Some(first)));
        assert_ne!(graph.render_fingerprint(), before);
    }

    #[test]
    fn all_live_ids_excludes_soft_removed_slots_but_keeps_hidden_nodes() {
        let mut graph = SceneGraph::new();
        let hidden = graph.add_root("Hidden");
        graph.get_mut(hidden).unwrap().visible = false;
        let removed = graph.add_root("Removed");
        assert!(graph.remove_node(removed));

        assert_eq!(graph.all_live_ids(), vec![hidden]);
        assert_eq!(graph.all_valid_ids(), Vec::<SceneNodeId>::new());
    }
}
