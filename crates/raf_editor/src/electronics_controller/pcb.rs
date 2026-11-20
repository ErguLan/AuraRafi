//! PCB presentation filters and schematic-to-board synchronization.

use super::*;

/// PCB presentation filters surfaced by the navigator's Layers tab.
///
/// They only prune objects when the presentation scene is built, so no
/// document, memory or export ever observes a "hidden" element; re-showing a
/// layer is one scene rebuild. This is a board view option in the same class
/// as the grid and labels toggles: it never takes an undo entry and never
/// marks the document dirty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PcbLayerVisibility {
    /// Component profile bodies and their pads.
    pub components: bool,
    /// Top copper routing.
    pub top_traces: bool,
    /// Bottom copper routing.
    pub bottom_traces: bool,
    /// Board outline and silkscreen frame.
    pub silkscreen: bool,
    /// Rubber bands of unrouted pads.
    pub airwires: bool,
}

impl Default for PcbLayerVisibility {
    fn default() -> Self {
        Self {
            components: true,
            top_traces: true,
            bottom_traces: true,
            silkscreen: true,
            airwires: true,
        }
    }
}

impl PcbLayerVisibility {
    /// True when no filter prunes anything, the common steady state, so the
    /// scene rebuild stays a no-op for it.
    pub(super) fn is_all_visible(self) -> bool {
        self.components && self.top_traces && self.bottom_traces && self.silkscreen && self.airwires
    }

    /// Flips one layer. `None` for an unknown key so the command bus reports a
    /// rejected command instead of a silent success.
    fn apply_toggle(&mut self, key: &str) -> Option<()> {
        match key {
            "components" => self.components = !self.components,
            "top" => self.top_traces = !self.top_traces,
            "bottom" => self.bottom_traces = !self.bottom_traces,
            "silk" => self.silkscreen = !self.silkscreen,
            "airwires" => self.airwires = !self.airwires,
            _ => return None,
        }
        Some(())
    }
}

/// Stable command suffixes of one visibility row.
pub const PCB_LAYER_KEYS: [&str; 5] = ["components", "top", "bottom", "silk", "airwires"];

impl NativeElectronicsEditor {
    /// Current PCB presentation filters for the Layers rail tab.
    pub fn pcb_layer_visibility(&self) -> PcbLayerVisibility {
        self.pcb_layer_visibility
    }

    /// Flips one presentation filter and rebuilds the scene so the change is
    /// visible at once.
    ///
    /// Like the grid and snap toggles, hiding a board layer is a view decision:
    /// it deliberately takes no undo entry and leaves the project clean.
    pub fn toggle_pcb_layer(&mut self, key: &str) -> bool {
        if self.pcb_layer_visibility.apply_toggle(key).is_none() {
            return false;
        }
        self.rebuild_scene();
        self.touch_ui();
        true
    }

    /// Routing preserved for explicit review after a net was split or removed.
    pub fn removed_trace_count(&self) -> usize {
        self.pcb.removed_traces.len()
    }

    /// Explicit user-triggered synchronization. The command is an edit, so it
    /// gets an undo entry and marks the document dirty.
    pub fn sync_pcb_from_schematic(&mut self) -> raf_electronics::PcbSyncSummary {
        let before = self.pcb.clone();
        let summary = self.pcb.sync_from_schematic(&self.schematic);
        if self.pcb != before {
            self.history.record(
                ElectronicsDocumentSnapshot {
                    schematic: self.schematic.clone(),
                    pcb: before,
                },
                &self.snapshot(),
            );
            self.dirty = true;
            self.rebuild_scene();
        }
        self.sync_stale = !self.pcb_matches_schematic();
        self.touch_ui();
        summary
    }

    /// Materializes the PCB projection without pretending it was an edit.
    ///
    /// Entering the PCB tab used to record a history entry and mark the project
    /// dirty, so after twenty minutes of work the next Ctrl+Z undid the
    /// synchronization instead of the user's edit. The PCB is still brought up to
    /// date; it simply stops competing with the undo stack.
    pub(crate) fn sync_pcb_from_schematic_quiet(&mut self) -> raf_electronics::PcbSyncSummary {
        let summary = self.pcb.sync_from_schematic(&self.schematic);
        self.sync_stale = !self.pcb_matches_schematic();
        self.rebuild_scene();
        summary
    }

    /// True when the PCB still describes the same design as the schematic: one
    /// placement per schematic component and the same pad-to-net assignment.
    pub(super) fn pcb_matches_schematic(&self) -> bool {
        self.pcb.matches_schematic(&self.schematic)
    }
}
