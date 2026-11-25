//! Native Electronics authoring and interaction mechanics.
//!
//! Pointer gestures, placement, routing, selection editing, undo/redo and
//! persistence are kept outside the document controller's state/analysis
//! module. The child module intentionally operates on the live controller;
//! it does not create a parallel document or renderer bridge.

use super::*;

#[path = "electronics_controller_interaction/canvas.rs"]
mod canvas;
#[path = "electronics_controller_interaction/drag.rs"]
mod drag;
#[path = "electronics_controller_interaction/gestures.rs"]
mod gestures;
#[path = "electronics_controller_interaction/overlay.rs"]
mod overlay;
#[path = "electronics_controller_interaction/pointer.rs"]
mod pointer;
#[cfg(test)]
#[path = "electronics_controller_interaction/tests.rs"]
mod tests;

/// Scroll units that the platform adapter reports for one physical wheel notch.
///
/// `NativeUiInputBridge` converts `MouseScrollDelta::LineDelta(_, y)` into
/// `[x * 24.0, y * 24.0]`
/// (`raf_render/src/ApiGraphicBasic/ui_surface/native_input.rs`), so a single
/// mouse notch is 24.0 snapshot units and a trackpad pixel delta is a fraction
/// of it. Dividing by this value turns the accumulated per-frame delta into the
/// only device-independent unit available, which is what makes one notch feel the
/// same on every device.
const SCROLL_UNITS_PER_NOTCH: f32 = 24.0;

/// Zoom multiplier applied to one wheel notch, and the reference for the `+`/`-`
/// shortcuts. Expressed as a power so a partial notch (trackpad, high-resolution
/// wheel) interpolates smoothly instead of snapping to a fixed step.
const ZOOM_STEP_PER_NOTCH: f32 = 1.12;

/// Bounds for a single zoom step. One notch must never cross a meaningful part of
/// the 0.15..=12 range; the previous `1.0 + 24.0 * 0.1` factor saturated at 1.8
/// and crossed the whole range in five notches.
const ZOOM_STEP_MIN: f32 = 0.75;
const ZOOM_STEP_MAX: f32 = 1.35;

/// Maximum number of wheel notches honoured in a single frame. A fast flick can
/// deliver several notches at once; clamping keeps one flick from jumping the
/// view across the document.
const ZOOM_NOTCHES_PER_FRAME_MAX: f32 = 3.0;

/// Logical pixels the cursor must travel before the hover pick re-runs.
///
/// Picking walks every CAD object, so it is not free. One logical pixel is below
/// the smallest visible movement of a mouse, so the pick still follows the
/// cursor exactly while an idle or barely moving cursor costs nothing.
const HOVER_REPICK_MIN_DISTANCE_SCREEN: f32 = 1.0;

/// Logical-pixel radius that decides whether a press grabbed a polyline vertex
/// instead of only selecting the wire or the trace. It is wider than
/// `PICK_TOLERANCE_SCREEN` on purpose, so the endpoints of any object stay
/// reachable: a click that already hit the geometry therefore also finds the
/// nearer endpoint.
const VERTEX_GRAB_TOLERANCE_SCREEN: f32 = 14.0;

/// Screen distance that separates a click from a press-and-drag gesture.
const PRESS_DRAG_THRESHOLD: f32 = 4.0;

/// Shortest accepted wire segment, in world units. A click that lands back on the
/// armed start point is not a zero-length wire, it is a click that did nothing.
const WIRE_SEGMENT_MIN_LENGTH: f32 = 0.5;

/// Logical-pixel radius two clicks may differ by and still be one double click.
///
/// Narrower than `PICK_TOLERANCE_SCREEN` on purpose. Two clicks that both land
/// inside the pick radius of the same point are still two different intentions
/// whenever they are further apart than this, and routing a wire is built on
/// exactly two clicks in a row: click the source pin, click the target pin. The
/// radius therefore has to be small enough that two distinct pins on a dense
/// symbol cannot be mistaken for one double click at low zoom.
const DOUBLE_CLICK_MAX_DISTANCE_SCREEN: f32 = 8.0;

/// Rotation increment, in degrees, for one `R` press.
const ROTATE_STEP_DEGREES: f32 = 90.0;

/// Maximum number of grid cells a pin may capture the pointer from.
///
/// The pin snap radius is authored in screen pixels so a pin stays as easy to
/// grab as it looks, but at low zoom that radius spans many grid cells and a
/// route drawn far from the pin would suddenly jump onto it. Capping the world
/// radius at a fraction of the grid keeps the affordance and removes the jump.
const PIN_SNAP_MAX_GRID_STEPS: f32 = 0.75;

/// Canvas keyboard shortcuts, in one documented place.
///
/// The retained tooltips (`electronics.tooltip.*` in the locale catalogs) already
/// publish these bindings, so the table lives next to the handler that honours
/// them and the two cannot drift. They are intentionally NOT routed through
/// `editor_shortcuts`: every binding is only meaningful while the Electronics
/// canvas owns the keyboard, which is a canvas decision and not a global one.
mod shortcut {
    use raf_core::InputKey;

    /// Select tool.
    pub const SELECT_TOOL: InputKey = InputKey::V;
    /// Wire tool. Schematic only: it routes pins, which a PCB has no notion of.
    pub const WIRE_TOOL: InputKey = InputKey::W;
    /// Place tool. Mirrors the toolbar Place button in both surfaces.
    pub const PLACE_TOOL: InputKey = InputKey::P;
    /// Frame the whole design.
    pub const FIT_VIEW: InputKey = InputKey::F;
    /// Show or hide the grid.
    pub const TOGGLE_GRID: InputKey = InputKey::G;
    /// Rotate the selection. `Shift` inverts the direction.
    pub const ROTATE: InputKey = InputKey::R;

    /// Zoom in: `+`/`=` and the numpad plus.
    pub const ZOOM_IN: [InputKey; 2] = [InputKey::Equal, InputKey::NumpadAdd];
    /// Zoom out: `-`/`_` and the numpad minus.
    pub const ZOOM_OUT: [InputKey; 2] = [InputKey::Minus, InputKey::NumpadSubtract];
}

impl NativeElectronicsEditor {
    /// True when a pointer gesture, a preview or a confirmation is armed.
    pub(crate) fn has_pending_gesture(&self) -> bool {
        self.component_drag.is_some()
            || self.placement_drag_active
            || self.wire_start.is_some()
            || self.board_outline_start.is_some()
            || self.pcb_route_draft.is_some()
            || self.secondary_pointer.is_some()
            || self.pan_pointer.is_some()
            || self.minimap_drag
            || self.context_menu_position.is_some()
            || self.pending_delete
    }

    pub fn undo(&mut self) -> bool {
        if self.undo_route_point() {
            return true;
        }
        if !self.history.undo(&mut self.schematic, &mut self.pcb) {
            return false;
        }
        self.selection = None;
        self.wire_start = None;
        self.rebuild_scene();
        self.dirty = true;
        self.touch_ui();
        true
    }

    pub fn redo(&mut self) -> bool {
        if !self.history.redo(&mut self.schematic, &mut self.pcb) {
            return false;
        }
        self.selection = None;
        self.wire_start = None;
        self.rebuild_scene();
        self.dirty = true;
        self.touch_ui();
        true
    }

    pub fn save(&mut self, project: &Project) -> Result<(), String> {
        let registry = ProjectSessionRegistry::load_or_legacy(&project.path, project.project_type);
        self.save_to_session(project, registry.active_session)
    }

    pub fn save_to_session(
        &mut self,
        project: &Project,
        session_id: raf_core::session::SessionId,
    ) -> Result<(), String> {
        let registry = ProjectSessionRegistry::load_or_legacy(&project.path, project.project_type);
        let session = registry
            .sessions
            .iter()
            .find(|session| session.id == session_id)
            .ok_or_else(|| "project session was not found".to_string())?;
        session
            .ensure_storage(&project.path)
            .map_err(|error| format!("session storage: {error}"))?;
        documents::save(
            &session.path(&project.path, &session.schematic_file),
            &session.path(&project.path, &session.pcb_file),
            &self.schematic,
            &self.pcb,
        )?;
        self.dirty = false;
        // The retained workbench keys its status bar by ui_revision. Saving is
        // not a CAD edit, but it must invalidate the Modified/Saved indicator.
        self.touch_ui();
        Ok(())
    }

    pub(super) fn pin_endpoint_at(&self, point: Vec2) -> Option<(Uuid, Vec2, WireAnchor)> {
        // Grid snapping and electrical endpoint acquisition are independent.
        // Turning off the grid must not make a visible terminal unconnectable.
        // The radius stays in screen pixels so a pin is as grabbable as it looks,
        // but it is capped in grid cells so a low zoom cannot turn "near" into
        // "several cells away" and make a distant route jump onto the pin.
        let screen_radius = PIN_SNAP_TOLERANCE_SCREEN / self.camera.zoom.max(MIN_ZOOM);
        let tolerance = screen_radius.min(self.grid_step * PIN_SNAP_MAX_GRID_STEPS);
        self.schematic
            .components
            .iter()
            .flat_map(|component| {
                component.pins.iter().map(move |pin| {
                    let position = component_pin_world_position(component, pin);
                    (component.id, pin.id, position, position.distance(point))
                })
            })
            .filter(|(_, _, _, distance)| *distance <= tolerance)
            .min_by(|left, right| left.3.total_cmp(&right.3))
            .map(|(component_id, pin_id, position, _)| {
                (
                    component_id,
                    position,
                    WireAnchor::Pin {
                        component_id,
                        pin_id,
                    },
                )
            })
    }

    pub(super) fn place_component(&mut self, world: Vec2) -> bool {
        let before = self.snapshot();
        let position = self.snap_world(world);
        let Some(mut component) = self
            .placement_template
            .and_then(|index| self.library.components.get(index))
            .map(|template| template.instantiate())
        else {
            // No template is armed. The click is intentionally not treated as a
            // successful placement: saying so needs a localized notice the
            // locale catalogs do not carry yet, and writing English text here
            // would bypass the translation system. The canvas hint already tells
            // the user to pick a library item first.
            return false;
        };
        component.position = position;
        let id = self.schematic.add_component(component);
        if self.active_surface == CadSurfaceKind::Pcb {
            self.pcb.sync_from_schematic(&self.schematic);
            if let Some(placement) = self
                .pcb
                .components
                .iter_mut()
                .find(|placement| placement.component_id == id)
            {
                placement.position = position;
            }
            self.pcb.rebuild_airwires();
        }
        self.selection = Some(ElectronicsSelection {
            source_id: id,
            kind: ElectronicsSelectionKind::Component,
        });
        let current = self.snapshot();
        if self.history.record(before, &current) {
            self.dirty = true;
        }
        self.rebuild_scene();
        self.touch_ui();
        true
    }
}
