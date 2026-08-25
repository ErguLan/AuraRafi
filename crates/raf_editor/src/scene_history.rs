//! Snapshot history for editor-owned scene mutations.
//!
//! The core command bus records command metadata, but the editor still needs
//! a state transition that can restore the actual `SceneGraph`. This small
//! bounded snapshot stack is intentionally local to the editor boundary and
//! can later be replaced by granular commands without changing its callers.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use raf_core::scene::SceneGraph;

const DEFAULT_MAX_HISTORY: usize = 128;

#[derive(Debug, Clone)]
pub struct SceneHistory {
    undo: Vec<SceneGraph>,
    redo: Vec<SceneGraph>,
    max_history: usize,
}

impl Default for SceneHistory {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_MAX_HISTORY)
    }
}

impl SceneHistory {
    pub fn with_capacity(max_history: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            max_history: max_history.max(1),
        }
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Stores the state before a successful mutation and starts a new branch.
    pub fn record_before(&mut self, scene: SceneGraph) {
        self.undo.push(scene);
        self.redo.clear();
        if self.undo.len() > self.max_history {
            let overflow = self.undo.len() - self.max_history;
            self.undo.drain(0..overflow);
        }
    }

    /// Records a snapshot only when the editor-visible graph actually changed.
    pub fn record_if_changed(&mut self, before: SceneGraph, after: &SceneGraph) -> bool {
        self.record_if_changed_grouped(before, after, false)
    }

    /// Same as `record_if_changed`, but lets a continuous gesture keep one
    /// undo entry (for example a transform slider dragged across many frames).
    pub fn record_if_changed_grouped(
        &mut self,
        before: SceneGraph,
        after: &SceneGraph,
        coalesce: bool,
    ) -> bool {
        if fingerprint(&before) == fingerprint(after) {
            return false;
        }
        if !coalesce {
            self.record_before(before);
        }
        true
    }

    pub fn undo(&mut self, current: &mut SceneGraph) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(current.clone());
        *current = previous;
        true
    }

    pub fn redo(&mut self, current: &mut SceneGraph) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(current.clone());
        if self.undo.len() > self.max_history {
            self.undo.remove(0);
        }
        *current = next;
        true
    }
}

pub(crate) fn scene_fingerprint(scene: &SceneGraph) -> u64 {
    // SceneGraph deliberately does not derive Hash because glam values are
    // not hashable. Debug is stable for the in-memory Rust model and this
    // function is used after mutations and at save/exit boundaries, never in
    // the normal render loop.
    let mut hasher = DefaultHasher::new();
    format!("{scene:?}").hash(&mut hasher);
    hasher.finish()
}

fn fingerprint(scene: &SceneGraph) -> u64 {
    scene_fingerprint(scene)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_and_redo_restore_scene_state() {
        let mut history = SceneHistory::default();
        let mut scene = SceneGraph::new();
        scene.add_root("Root");
        let before = scene.clone();
        scene.add_root("Second");
        assert!(history.record_if_changed(before, &scene));
        assert!(history.undo(&mut scene));
        assert_eq!(scene.len(), 1);
        assert!(history.redo(&mut scene));
        assert_eq!(scene.len(), 2);
    }

    #[test]
    fn unchanged_snapshot_does_not_create_history_entry() {
        let mut history = SceneHistory::default();
        let scene = SceneGraph::new();
        assert!(!history.record_if_changed(scene.clone(), &scene));
        assert!(!history.can_undo());
    }

    #[test]
    fn grouped_gesture_keeps_one_entry_and_next_gesture_stays_undoable() {
        let mut history = SceneHistory::default();
        let mut scene = SceneGraph::new();
        let first = scene.clone();
        scene.add_root("First");
        assert!(history.record_if_changed_grouped(first, &scene, false));

        let second = scene.clone();
        scene.add_root("Second");
        assert!(history.record_if_changed_grouped(second, &scene, true));
        assert!(history.undo(&mut scene));
        assert_eq!(scene.len(), 0);

        let third = scene.clone();
        scene.add_root("Third");
        assert!(history.record_if_changed_grouped(third, &scene, false));
        assert!(history.undo(&mut scene));
        assert_eq!(scene.len(), 0);
    }
}
