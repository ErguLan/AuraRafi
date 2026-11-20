//! Document snapshots and undo availability.

use super::*;

impl NativeElectronicsEditor {
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    pub(super) fn snapshot(&self) -> ElectronicsDocumentSnapshot {
        ElectronicsDocumentSnapshot {
            schematic: self.schematic.clone(),
            pcb: self.pcb.clone(),
        }
    }
}
