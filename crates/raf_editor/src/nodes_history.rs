//! Bounded authoring history for the Nodes document.
//!
//! This is deliberately separate from scene history. A node graph is another
//! persisted document, but it is still authoring-only in this pass: undo and
//! redo never execute a graph or enter Play mode.

use raf_nodes::NodeGraph;

const DEFAULT_CAPACITY: usize = 64;

#[derive(Debug, Clone)]
pub(crate) struct NodeGraphHistory {
    undo: Vec<NodeGraph>,
    redo: Vec<NodeGraph>,
    capacity: usize,
}

impl Default for NodeGraphHistory {
    fn default() -> Self {
        Self::new(DEFAULT_CAPACITY)
    }
}

impl NodeGraphHistory {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            capacity: capacity.max(1),
        }
    }

    pub(crate) fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    pub(crate) fn checkpoint(&mut self, before: &NodeGraph, after: &NodeGraph) {
        if before == after {
            return;
        }
        self.undo.push(before.clone());
        if self.undo.len() > self.capacity {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub(crate) fn undo(&mut self, current: &NodeGraph) -> Option<NodeGraph> {
        let previous = self.undo.pop()?;
        self.redo.push(current.clone());
        Some(previous)
    }

    pub(crate) fn redo(&mut self, current: &NodeGraph) -> Option<NodeGraph> {
        let next = self.redo.pop()?;
        self.undo.push(current.clone());
        Some(next)
    }

    pub(crate) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub(crate) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_nodes::Node;

    #[test]
    fn undo_and_redo_restore_the_graph_document() {
        let before = NodeGraph::new("Main");
        let mut after = before.clone();
        after.add_node(Node::on_start());

        let mut history = NodeGraphHistory::new(2);
        history.checkpoint(&before, &after);
        assert!(history.can_undo());
        assert_eq!(history.undo(&after), Some(before.clone()));
        assert!(history.can_redo());
        assert_eq!(history.redo(&before), Some(after));
    }
}
