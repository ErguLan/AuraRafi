//! Tool arming, placement state and snap behaviour.

use super::*;

use super::library::component_asset_key;

impl NativeElectronicsEditor {
    /// Asset used by the pointer-driven library placement ghost. Keeping this
    /// lookup in the controller means the canvas overlay does not need to know
    /// how component templates are classified.
    pub fn placement_preview_asset_key(&self) -> Option<&'static str> {
        self.placement_drag_active.then(|| {
            self.placement_template
                .and_then(|index| self.library.components.get(index))
                .map(|template| component_asset_key(template.template.kind_label()))
                .unwrap_or("electronics://symbols/generic.png")
        })
    }

    pub fn set_tool(&mut self, tool: ElectronicsTool) {
        if self.tool != tool {
            self.tool = tool;
            if tool != ElectronicsTool::Place {
                self.placement_template = None;
                self.placement_drag_active = false;
                self.placement_preview = None;
            }
            if tool != ElectronicsTool::BoardOutline {
                self.board_outline_start = None;
                self.board_outline_vertices.clear();
            }
            if tool != ElectronicsTool::Route {
                self.clear_route_error();
                if self.pcb_route_draft.is_some() {
                    self.highlight_net = None;
                }
                self.pcb_route_draft = None;
            }
            if tool != ElectronicsTool::Wire {
                self.wire_start = None;
                // Changing tool is not a document edit, so it must not promote a
                // stored DRC report back to "current".
                self.rebuild_scene();
            }
            self.touch_ui();
        }
    }

    /// Whether component placement snaps to the schematic grid.
    pub fn snap_enabled(&self) -> bool {
        self.snap_enabled
    }

    /// Flips the grid snap and rebuilds the scene so the change is visible at
    /// once.
    ///
    /// Snap is a view-level decision, not a document edit: it deliberately takes
    /// no undo entry and leaves the project clean.
    pub fn toggle_snap(&mut self) {
        self.snap_enabled = !self.snap_enabled;
        self.rebuild_scene();
        self.touch_ui();
    }
}
