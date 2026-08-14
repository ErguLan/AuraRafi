//! Transactional history for the Electronics documents.
//!
//! Scene history is intentionally scoped to `SceneGraph`. Schematic and PCB
//! edits need an independent stack so Edit/Undo works consistently regardless
//! of the active CAD mode. Snapshots are captured only when an action commits;
//! no document is serialized or cloned from the frame loop.

use raf_electronics::{PcbLayout, Schematic};

#[derive(Debug, Clone)]
pub struct ElectronicsDocumentSnapshot {
    pub schematic: Schematic,
    pub pcb: PcbLayout,
}

#[derive(Debug)]
pub struct ElectronicsHistory {
    undo: Vec<ElectronicsDocumentSnapshot>,
    redo: Vec<ElectronicsDocumentSnapshot>,
    limit: usize,
}

impl Default for ElectronicsHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl ElectronicsHistory {
    pub fn new() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit: 50,
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

    pub fn record(
        &mut self,
        before: ElectronicsDocumentSnapshot,
        current: &ElectronicsDocumentSnapshot,
    ) -> bool {
        if !documents_differ(&before, current) {
            return false;
        }
        self.undo.push(before);
        self.redo.clear();
        if self.undo.len() > self.limit {
            let overflow = self.undo.len() - self.limit;
            self.undo.drain(0..overflow);
        }
        true
    }

    pub fn undo(&mut self, schematic: &mut Schematic, pcb: &mut PcbLayout) -> bool {
        let Some(snapshot) = self.undo.pop() else {
            return false;
        };
        self.redo.push(ElectronicsDocumentSnapshot {
            schematic: schematic.clone(),
            pcb: pcb.clone(),
        });
        *schematic = snapshot.schematic;
        *pcb = snapshot.pcb;
        true
    }

    pub fn redo(&mut self, schematic: &mut Schematic, pcb: &mut PcbLayout) -> bool {
        let Some(snapshot) = self.redo.pop() else {
            return false;
        };
        self.undo.push(ElectronicsDocumentSnapshot {
            schematic: schematic.clone(),
            pcb: pcb.clone(),
        });
        *schematic = snapshot.schematic;
        *pcb = snapshot.pcb;
        true
    }
}

fn documents_differ(
    before: &ElectronicsDocumentSnapshot,
    current: &ElectronicsDocumentSnapshot,
) -> bool {
    ron::ser::to_string(&before.schematic).ok() != ron::ser::to_string(&current.schematic).ok()
        || ron::ser::to_string(&before.pcb).ok() != ron::ser::to_string(&current.pcb).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_documents_do_not_create_history_entries() {
        let before = ElectronicsDocumentSnapshot {
            schematic: Schematic::new("test"),
            pcb: PcbLayout::new("test"),
        };
        let current = before.clone();
        let mut history = ElectronicsHistory::new();

        assert!(!history.record(before, &current));
        assert!(!history.can_undo());
    }

    #[test]
    fn undo_and_redo_restore_both_electronics_documents() {
        let before = ElectronicsDocumentSnapshot {
            schematic: Schematic::new("before"),
            pcb: PcbLayout::new("before"),
        };
        let mut current = before.clone();
        current.schematic.name = "after".to_string();
        current.pcb.name = "after".to_string();
        let mut history = ElectronicsHistory::new();

        assert!(history.record(before, &current));
        let mut schematic = current.schematic.clone();
        let mut pcb = current.pcb.clone();
        assert!(history.undo(&mut schematic, &mut pcb));
        assert_eq!(schematic.name, "before");
        assert_eq!(pcb.name, "before");
        assert!(history.redo(&mut schematic, &mut pcb));
        assert_eq!(schematic.name, "after");
        assert_eq!(pcb.name, "after");
    }
}
