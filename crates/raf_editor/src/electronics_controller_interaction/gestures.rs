//! Wire, route, outline, rotation and cancellation gestures.

use super::*;

use super::super::wiring::anchor_component_id;

/// True while the active tool keeps an in-progress run the user can finish or
/// abandon: a schematic wire or a board outline.
///
/// These are the only two tools that hold pending geometry between clicks, so
/// they are also the only two where a double click has something to end. The
/// surface is part of the test because each tool only exists on one of them.
pub(super) fn chaining_tool(tool: ElectronicsTool, surface: CadSurfaceKind) -> bool {
    matches!(
        (tool, surface),
        (ElectronicsTool::Wire, CadSurfaceKind::Schematic)
            | (ElectronicsTool::BoardOutline, CadSurfaceKind::Pcb)
    )
}

/// True when the armed wire preview has to follow the pointer.
///
/// Select is in the list because of the contextual pin drag: it arms the very
/// same preview as the Wire tool, so the preview has to track the pointer in both
/// tools or the rubber band would freeze at the pin it started from. No other
/// tool can hold an armed wire, which is why the armed flag still has to be part
/// of the test.
pub(super) fn wire_preview_follows_pointer(tool: ElectronicsTool, wire_armed: bool) -> bool {
    wire_armed && matches!(tool, ElectronicsTool::Wire | ElectronicsTool::Select)
}

/// True when a primary release has to resolve the armed wire.
///
/// Wire is the click-click route and the pin drag-drop; Select is the contextual
/// pin drag. Both end on a release, but only Wire chains to the next segment.
pub(super) fn wire_gesture_owns_release(tool: ElectronicsTool, surface: CadSurfaceKind) -> bool {
    surface == CadSurfaceKind::Schematic
        && matches!(tool, ElectronicsTool::Wire | ElectronicsTool::Select)
}

/// Decides whether a primary press starts a wire without the Wire tool being
/// armed, which is the whole point of the gesture: connecting two points is
/// something the user does, not a mode they have to switch into first.
///
/// The gesture is for pins only, and that restriction is the design rather than
/// an omission. A press on a symbol body must keep meaning "move this part",
/// because dragging a body is how a part is positioned and how a group of parts
/// is laid out. If a body press could also mean "start a wire", both gestures
/// would compete for the same press and nothing on screen could tell the user
/// which one the drag would perform before they let go. A pin has nothing to
/// move: it is a terminal, so the only meaningful drag that starts on one is the
/// wire leaving it.
///
/// A PCB is excluded on purpose. A pad is not a schematic pin and the board has
/// no pins to route; dropping there means "land on copper", which is a different
/// confirmation rule and a different gesture, not this one.
pub(super) fn press_starts_pin_wire(
    tool: ElectronicsTool,
    surface: CadSurfaceKind,
    picked: Option<CadObjectKind>,
) -> bool {
    matches!(
        (tool, surface),
        (ElectronicsTool::Select, CadSurfaceKind::Schematic)
    ) && matches!(picked, Some(CadObjectKind::Pin))
}

impl NativeElectronicsEditor {
    pub(super) fn handle_wire_click(
        &mut self,
        world: Vec2,
        router: &mut InputRouter,
        owner: InputOwner,
        result: &mut ElectronicsInputResult,
    ) {
        if self.active_surface != CadSurfaceKind::Schematic {
            // Wires are a schematic concept. The tool can still be armed from a
            // command while the PCB tab is active, and placing a schematic wire
            // from the PCB view would be an invisible cross-document edit.
            router.release_pointer(PointerButton::Primary, owner);
            return;
        }
        if self.wire_start.is_some() {
            if self.commit_wire_segment(world) {
                result.changed = true;
                result.request_redraw = true;
            }
            return;
        }
        let (point, anchor) = self.pin_endpoint_at(world).map_or_else(
            || {
                let point = self.snap_world(world);
                (point, Some(WireAnchor::Point(point)))
            },
            |(_, point, anchor)| (point, Some(anchor)),
        );
        self.wire_start = Some(WireStart {
            world: point,
            anchor,
        });
        self.touch_ui();
        result.changed = true;
        result.request_redraw = true;
        // The capture is intentionally kept: the same press then doubles as the
        // `pin -> drag -> drop` wire gesture, and a release without movement
        // resolves it as the ordinary click-click route.
    }

    /// Commits the armed wire endpoint and re-arms the point the run finished at.
    ///
    /// This is what makes a route one continuous gesture. Before, confirming a
    /// segment destroyed the start point, so every extra corner cost two more
    /// clicks from scratch. Chaining ends when the tool changes (`set_tool`), on
    /// Escape (`cancel_gesture`), on a secondary click, or when the wire tool
    /// leaves the schematic.
    pub(super) fn commit_wire_segment(&mut self, world: Vec2) -> bool {
        let Some(start) = self.wire_start.take() else {
            return false;
        };
        let (end_world, end_anchor) = self.pin_endpoint_at(world).map_or_else(
            || {
                let point = self.snap_world(world);
                (point, WireAnchor::Point(point))
            },
            |(_, position, anchor)| (position, anchor),
        );
        if start.world.distance(end_world) <= WIRE_SEGMENT_MIN_LENGTH {
            // A click that did not move is not a zero-length wire: the armed
            // start is restored so the route continues from it.
            self.wire_start = Some(start);
            self.touch_ui();
            return false;
        }
        let before = self.snapshot();
        let points = orthogonal_wire_points(start.world, end_world);
        self.schematic.add_wire_path_anchored(
            &points,
            &self.next_net_name(),
            start.anchor,
            Some(end_anchor),
        );
        let current = self.snapshot();
        if self.history.record(before, &current) {
            self.dirty = true;
        }
        // The confirmed endpoint becomes the next start point, keeping its pin
        // anchor so the committed segment stays glued to that pin.
        self.wire_start = Some(WireStart {
            world: end_world,
            anchor: Some(end_anchor),
        });
        self.rebuild_scene();
        self.touch_ui();
        true
    }

    pub(super) fn handle_route_click(
        &mut self,
        world: Vec2,
        router: &mut InputRouter,
        owner: InputOwner,
        result: &mut ElectronicsInputResult,
    ) {
        if self.active_surface != CadSurfaceKind::Pcb {
            router.release_pointer(PointerButton::Primary, owner);
            return;
        }
        if self.route_physical_click(world) {
            router.release_pointer(PointerButton::Primary, owner);
            result.changed = true;
            result.request_redraw = true;
            return;
        }
        let tolerance = PICK_TOLERANCE_SCREEN / self.camera.zoom.max(MIN_ZOOM);
        // Clicking a guide arms its nearest endpoint. Quick routing remains an
        // explicit context command; a missed pad must never author a whole trace.
        if let Some(hit) = pick(&self.scene, world, tolerance) {
            if hit.selection.kind == CadObjectKind::Airwire {
                if let Some(object) = self
                    .scene
                    .objects
                    .iter()
                    .find(|o| o.id == hit.selection.object_id)
                {
                    if let Some(endpoint) = object.points.iter().copied().min_by(|a, b| {
                        a.distance_squared(world)
                            .total_cmp(&b.distance_squared(world))
                    }) {
                        self.route_physical_click(endpoint);
                        result.request_redraw = true;
                        result.changed = true;
                    }
                }
            }
        }
        router.release_pointer(PointerButton::Primary, owner);
    }

    pub(super) fn handle_board_outline_click(
        &mut self,
        world: Vec2,
        router: &mut InputRouter,
        owner: InputOwner,
        result: &mut ElectronicsInputResult,
    ) {
        if self.active_surface != CadSurfaceKind::Pcb {
            router.release_pointer(PointerButton::Primary, owner);
            return;
        }
        let point = self.snap_world(world);
        if self.board_outline_start.is_none() {
            self.board_outline_vertices.clear();
        }
        // Closing is a screen-space target, independent of the manufacturing
        // grid. At a 0.25 mm grid the old cap made it smaller than one pixel.
        let close_radius = 8.0 / self.camera.zoom.max(MIN_ZOOM);
        if self.board_outline_vertices.len() >= 3
            && point.distance(self.board_outline_vertices[0]) <= close_radius
        {
            let mut points = self.board_outline_vertices.clone();
            points.push(points[0]);
            if raf_electronics::pcb::geometry_outline_is_simple(&points) {
                let before = self.snapshot();
                self.pcb.board_outline.points = points;
                self.pcb.rebuild_airwires();
                if self.history.record(before, &self.snapshot()) {
                    self.dirty = true;
                }
                self.set_tool(ElectronicsTool::Select);
            } else {
                self.push_surface_error_key("electronics.error.outline_invalid");
            }
        } else if self.board_outline_vertices.len() < 512
            && self
                .board_outline_vertices
                .last()
                .map_or(true, |last| last.distance(point) > 0.0001)
        {
            self.board_outline_vertices.push(point);
            self.board_outline_start = self.board_outline_vertices.first().copied();
        }
        self.rebuild_scene();
        self.touch_ui();
        result.changed = true;
        result.request_redraw = true;
        router.release_pointer(PointerButton::Primary, owner);
    }

    pub(crate) fn delete_selected(&mut self) -> bool {
        let Some(selection) = self.selection else {
            return false;
        };
        let before = self.snapshot();
        let mut changed = false;
        if self.active_surface == CadSurfaceKind::Pcb
            && self
                .pcb
                .vias
                .iter()
                .any(|via| via.id == selection.source_id)
        {
            self.pcb.vias.retain(|via| via.id != selection.source_id);
            self.pcb.rebuild_airwires();
            changed = true;
        } else if self.active_surface == CadSurfaceKind::Pcb
            && matches!(
                selection.kind,
                ElectronicsSelectionKind::Component | ElectronicsSelectionKind::Pin
            )
        {
            let before_len = self.pcb.components.len();
            self.pcb
                .components
                .retain(|component| component.component_id != selection.source_id);
            changed = before_len != self.pcb.components.len();
            if changed {
                self.pcb.rebuild_airwires();
                // The schematic still owns the part, so the two documents now disagree.
                // Silently resyncing would make the component reappear on the next tab
                // switch; the user is told instead, and undo still reverses this delete.
                self.sync_stale = true;
            }
        } else if matches!(
            selection.kind,
            ElectronicsSelectionKind::Component | ElectronicsSelectionKind::Pin
        ) {
            let before_len = self.schematic.components.len();
            self.schematic
                .components
                .retain(|component| component.id != selection.source_id);
            changed = before_len != self.schematic.components.len();
            if changed {
                self.schematic.wires.retain(|wire| {
                    !wire
                        .start_anchor
                        .is_some_and(|anchor| anchor_component_id(anchor) == selection.source_id)
                        && !wire.end_anchor.is_some_and(|anchor| {
                            anchor_component_id(anchor) == selection.source_id
                        })
                });
                self.schematic.sync_wire_anchors();
            }
        } else if selection.kind == ElectronicsSelectionKind::Wire {
            let before_len = self.schematic.wires.len();
            self.schematic
                .wires
                .retain(|wire| wire.id != selection.source_id);
            changed = before_len != self.schematic.wires.len();
        } else if self.active_surface == CadSurfaceKind::Pcb
            && selection.kind == ElectronicsSelectionKind::Trace
        {
            let before_len = self.pcb.traces.len();
            self.pcb
                .traces
                .retain(|trace| trace.id != selection.source_id);
            changed = before_len != self.pcb.traces.len();
            if changed {
                self.pcb.rebuild_airwires();
            }
        }
        if !changed {
            return false;
        }
        self.history.record(before, &self.snapshot());
        self.dirty = true;
        self.selection = None;
        self.interaction.clear_selection();
        self.rebuild_scene();
        self.touch_ui();
        true
    }

    pub(crate) fn route_selected_airwire(&mut self) -> bool {
        if self.active_surface != CadSurfaceKind::Pcb {
            return false;
        }
        let Some(index) = self
            .interaction
            .selected
            .as_ref()
            .and_then(|selected| selected.object_id.strip_prefix("airwire:"))
            .and_then(|index| index.parse::<usize>().ok())
        else {
            return false;
        };
        let before = self.snapshot();
        if let Err(error) = self.pcb.try_route_airwire_with_options(
            index,
            self.pcb_route_layer,
            self.pcb_route_width,
        ) {
            self.push_surface_error(error);
            self.touch_ui();
            return false;
        }
        let Some(trace) = self.pcb.traces.last() else {
            return false;
        };
        self.selection = Some(ElectronicsSelection {
            source_id: trace.id,
            kind: ElectronicsSelectionKind::Trace,
        });
        self.interaction.selected = Some(raf_electronics::cad_interaction::CadSelection {
            object_id: format!("trace:{}", trace.id),
            source_id: Some(trace.id),
            kind: CadObjectKind::Trace,
            layer: match trace.layer {
                raf_electronics::PcbLayer::TopCopper => CadLayerKind::PcbTopCopper,
                raf_electronics::PcbLayer::BottomCopper => CadLayerKind::PcbBottomCopper,
            },
        });
        self.history.record(before, &self.snapshot());
        self.dirty = true;
        self.rebuild_scene();
        self.touch_ui();
        true
    }

    pub(crate) fn rotate_selected(&mut self) -> bool {
        self.rotate_selection_by(ROTATE_STEP_DEGREES)
    }

    /// Rotates the selection by a signed multiple of 90 degrees.
    ///
    /// The direction travels with the call so `R` and `Shift+R` are the same
    /// operation in opposite directions instead of an undocumented asymmetry.
    /// Public so a surface can offer the inverted rotation from a button without
    /// duplicating the rotation rules of this module.
    pub fn rotate_selection_by(&mut self, degrees: f32) -> bool {
        let Some(selection) = self.selection else {
            return false;
        };
        if !matches!(
            selection.kind,
            ElectronicsSelectionKind::Component | ElectronicsSelectionKind::Pin
        ) {
            return false;
        }
        if self.active_surface == CadSurfaceKind::Pcb {
            let before = self.snapshot();
            let Some(component) = self
                .pcb
                .components
                .iter_mut()
                .find(|component| component.component_id == selection.source_id)
            else {
                return false;
            };
            if component.locked {
                return false;
            }
            component.rotation = (component.rotation + degrees).rem_euclid(360.0);
            self.pcb.rebuild_airwires();
            self.history.record(before, &self.snapshot());
            self.dirty = true;
            self.rebuild_scene();
            self.touch_ui();
            return true;
        }
        if !self
            .schematic
            .components
            .iter()
            .any(|component| component.id == selection.source_id)
        {
            return false;
        }
        if self
            .schematic
            .components
            .iter()
            .find(|component| component.id == selection.source_id)
            .is_some_and(|component| component.locked)
        {
            return false;
        }
        let before = self.snapshot();
        let Some(component) = self
            .schematic
            .components
            .iter_mut()
            .find(|component| component.id == selection.source_id)
        else {
            return false;
        };
        component.rotation = (component.rotation + degrees).rem_euclid(360.0);
        self.schematic.sync_wire_anchors();
        self.history.record(before, &self.snapshot());
        self.dirty = true;
        self.rebuild_scene();
        self.touch_ui();
        true
    }

    /// Ends a chaining tool from a double click and returns to Select.
    ///
    /// The armed run is dropped rather than committed, so the double click is a
    /// dismissal and never an edit: nothing reaches the document, nothing is
    /// written to the history and the project does not become dirty. `set_tool`
    /// clears the state that does not belong to Select and repaints the scene
    /// that carried the preview; the tool the user came from is left intact, so
    /// `W` or the toolbar brings the same tool back armed.
    pub(super) fn leave_chaining_tool(
        &mut self,
        result: &mut ElectronicsInputResult,
        router: &mut InputRouter,
        owner: InputOwner,
    ) {
        self.wire_start = None;
        self.board_outline_start = None;
        self.board_outline_vertices.clear();
        self.set_tool(ElectronicsTool::Select);
        router.release_pointer(PointerButton::Primary, owner);
        // The hover was suppressed for the whole run and has to come back with the
        // next pointer sample, exactly as it does after a component drag.
        self.pointer_world = None;
        result.changed = true;
        result.request_redraw = true;
    }

    /// Rolls the pending gesture back and releases every canvas-owned capture.
    ///
    /// This is a true rollback: the pre-gesture document is restored and nothing
    /// is written to the history, which is exactly what a cancelled gesture must
    /// do. It must therefore never be reached from the capture-lost path, where
    /// the movement already reached the document and has to be committed.
    pub(super) fn cancel_gesture(&mut self, router: &mut InputRouter, owner: InputOwner) {
        self.clear_route_error();
        if self.pcb_route_draft.is_some() {
            self.highlight_net = None;
        }
        self.pcb_route_draft = None;
        if let Some(drag) = self.component_drag.take() {
            self.schematic = drag.before.schematic;
            self.pcb = drag.before.pcb;
            self.rebuild_scene();
        }
        self.wire_start = None;
        self.board_outline_start = None;
        self.board_outline_vertices.clear();
        self.secondary_pointer = None;
        self.pan_pointer = None;
        self.minimap_drag = false;
        self.placement_drag_active = false;
        self.placement_preview = None;
        // Escape also dismisses the canvas context menu and an armed destructive
        // confirmation. Both are pending canvas-owned states, not document edits,
        // so resolving them here is a dismissal and not a rollback.
        self.context_menu_position = None;
        // A click that is still waiting for its partner is pending input state of
        // the same kind: after the dismissal the next click has to stand on its
        // own instead of pairing with a click from before the cancel.
        self.interaction.clear_double_click();
        self.cancel_pending_action();
        self.rebuild_scene_internal();
        router.cancel_owner(owner);
        self.touch_ui();
    }
}
