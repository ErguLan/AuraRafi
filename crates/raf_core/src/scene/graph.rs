//! Scene graph with parent-child transform hierarchy.
//!
//! Uses a flat Vec with indices for O(1) lookup and cache-friendly iteration.
//! Each node stores its local transform; world transforms are computed lazily
//! when needed for rendering.

use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

mod hierarchy;
mod nodes;
mod queries;
mod serde_support;
mod transforms;

#[cfg(test)]
mod tests;

pub use nodes::{NodeColor, Primitive, SceneNode, SceneNodeId};

/// Flat-array scene graph. All nodes live in a contiguous `Vec` for
/// cache-friendly iteration.
#[derive(Clone, Serialize, Deserialize)]
pub struct SceneGraph {
    #[serde(default)]
    environment: crate::environment::EnvironmentSettings,
    #[serde(skip, default = "initial_document_revision")]
    environment_revision: Cell<u64>,
    nodes: Vec<SceneNode>,
    /// Indices of root-level nodes (no parent).
    roots: Vec<SceneNodeId>,
    /// Cached render key. The cache is editor/runtime-local and is never
    /// serialized; every graph mutator invalidates it before changing nodes.
    #[serde(skip)]
    render_cache: Cell<Option<u64>>,
    /// Cached hierarchy/authoring key. Unlike `render_cache`, this ignores
    /// animation-only fields such as position, rotation and color so a moving
    /// script object does not rebuild the editor hierarchy every tick.
    #[serde(skip, default)]
    hierarchy_cache: Cell<(u64, u64)>,
    /// Monotonic authoring revision used by editor surfaces to invalidate
    /// retained projections without serializing or re-hashing the full graph.
    /// It is runtime-local on purpose: persisted identity is still handled by
    /// the document serializer at save/attach boundaries.
    #[serde(skip, default = "initial_document_revision")]
    document_revision: Cell<u64>,
}

static NEXT_DOCUMENT_REVISION: AtomicU64 = AtomicU64::new(1);

fn next_document_revision() -> u64 {
    NEXT_DOCUMENT_REVISION
        .fetch_add(1, Ordering::Relaxed)
        .max(1)
}

fn initial_document_revision() -> Cell<u64> {
    Cell::new(next_document_revision())
}

impl SceneGraph {
    /// Create an empty scene graph.
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            environment: Default::default(),
            environment_revision: Cell::new(next_document_revision()),
            roots: Vec::new(),
            render_cache: Cell::new(None),
            hierarchy_cache: Cell::new((0, 0)),
            document_revision: Cell::new(next_document_revision()),
        }
    }

    #[inline]
    fn invalidate(&self) {
        self.render_cache.set(None);
        self.document_revision.set(next_document_revision());
    }

    /// Cheap change token for retained editor projections.
    ///
    /// Every graph mutator advances this value before exposing mutable state,
    /// so consumers can compare revisions in O(1) instead of walking the
    /// hierarchy every frame.
    pub fn document_revision(&self) -> u64 {
        self.document_revision
            .get()
            .max(self.environment_revision.get())
    }

    pub fn geometry_revision(&self) -> u64 {
        self.document_revision.get()
    }
    pub fn environment(&self) -> &crate::environment::EnvironmentSettings {
        &self.environment
    }
    pub fn set_environment(
        &mut self,
        environment: crate::environment::EnvironmentSettings,
    ) -> bool {
        if self.environment == environment {
            return false;
        }
        self.environment = environment;
        self.environment_revision.set(next_document_revision());
        true
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
}
