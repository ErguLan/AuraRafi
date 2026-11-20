//! Component and polyline-vertex drag mechanics.

use super::*;

use super::gestures::press_starts_pin_wire;

use super::pointer::selection_kind;

/// Outcome of one frame of a component or polyline-vertex drag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DragOutcome {
    /// The dragged object is no longer in the document. The gesture is orphaned
    /// and must not be presented as a normal edit.
    Missing,
    /// The object exists but the gesture resolved to no new geometry, for
    /// example a locked part or a snap that landed on the same grid point.
    Unchanged,
    /// The document moved.
    Moved,
}

/// Which vertex of a polyline the press grabbed.
///
/// `offset` is the grab offset recorded at press time. Projecting every candidate
/// back through it recovers the press point that would have selected that
/// candidate, so the one landing closest to `press_world` is the vertex the
/// gesture started on. Resolving against the pre-drag geometry and the original
/// press, instead of against the live pointer, is what keeps the choice frozen:
/// a drag that travels past a neighbouring vertex would otherwise silently hand
/// the gesture over to it and start moving the wrong point.
pub(super) fn grabbed_vertex_index(points: &[Vec2], offset: Vec2, press_world: Vec2) -> Option<usize> {
    points
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| {
            (press_world - (*left - offset))
                .length_squared()
                .total_cmp(&(press_world - (*right - offset)).length_squared())
        })
        .map(|(index, _)| index)
}

/// Polyline of the dragged object as it was before the gesture started.
pub(super) fn drag_polyline_before(
    drag: &ComponentDrag,
    kind: Option<ElectronicsSelectionKind>,
) -> Option<Vec<Vec2>> {
    match kind? {
        ElectronicsSelectionKind::Wire => drag
            .before
            .schematic
            .wires
            .iter()
            .find(|wire| wire.id == drag.component_id)
            .map(|wire| vec![wire.start, wire.end]),
        ElectronicsSelectionKind::Trace => drag
            .before
            .pcb
            .traces
            .iter()
            .find(|trace| trace.id == drag.component_id)
            .map(|trace| trace.points.clone()),
        _ => None,
    }
}

impl NativeElectronicsEditor {
    pub(super) fn begin_selection_or_drag(
        &mut self,
        world: Vec2,
        router: &mut InputRouter,
        owner: InputOwner,
        result: &mut ElectronicsInputResult,
    ) {
        let tolerance = PICK_TOLERANCE_SCREEN / self.camera.zoom.max(MIN_ZOOM);
        // Editable picking, so the click lands on the geometry the user aimed at
        // instead of on a net label, an airwire or a DRC marker painted above it.
        let Some(hit) = self
            .interaction
            .select_editable_at(&self.scene, world, tolerance)
        else {
            self.selection = None;
            self.interaction.clear_selection();
            self.set_hovered(None);
            router.release_pointer(PointerButton::Primary, owner);
            self.touch_ui();
            result.changed = true;
            result.request_redraw = true;
            return;
        };

        let kind = selection_kind(hit.selection.kind);
        let source_id = hit.selection.source_id.unwrap_or_else(Uuid::nil);
        let selection = ElectronicsSelection { source_id, kind };
        self.selection = Some(selection);
        self.set_hovered(Some(selection));
        self.touch_ui();
        result.changed = true;
        result.request_redraw = true;

        // A press on a pin arms the wire preview instead of moving the part, so
        // connecting two points needs no tool switch. The selection above is kept:
        // it is the same press the user would have made to select, and the
        // inspector should not blank out because the drag began a wire.
        //
        // Grid snapping does not disable acquiring electrical terminals.
        if press_starts_pin_wire(self.tool, self.active_surface, Some(hit.selection.kind)) {
            if let Some((_, point, anchor)) = self.pin_endpoint_at(world) {
                self.wire_start = Some(WireStart {
                    world: point,
                    anchor: Some(anchor),
                });
                self.rebuild_scene();
                // The capture is deliberately kept, for the same reason the Wire
                // tool keeps it: the in-flight preview needs the pointer, and the
                // release is what decides whether the drag lands on a pin.
                return;
            }
        }

        match kind {
            ElectronicsSelectionKind::Component | ElectronicsSelectionKind::Pin => {
                let position_and_locked = if self.active_surface == CadSurfaceKind::Schematic {
                    self.schematic
                        .components
                        .iter()
                        .find(|component| component.id == source_id)
                        .map(|component| (component.position, component.locked))
                } else {
                    self.pcb
                        .components
                        .iter()
                        .find(|component| component.component_id == source_id)
                        .map(|component| (component.position, component.locked))
                        .or_else(|| self.pcb.vias.iter().find(|via| via.id == source_id).map(|via| (via.position, false)))
                };
                match position_and_locked {
                    Some((position, false)) => {
                        self.component_drag = Some(ComponentDrag {
                            component_id: source_id,
                            pointer_offset: position - world,
                            before: self.snapshot(),
                            orphaned: false,
                        });
                    }
                    // Locked, or already gone from this document: selection only.
                    _ => {
                        router.release_pointer(PointerButton::Primary, owner);
                    }
                }
            }
            ElectronicsSelectionKind::Wire | ElectronicsSelectionKind::Trace => {
                // A mis-routed run used to be a delete-and-redraw. Grabbing the
                // nearest vertex turns it into an edit, and it goes through the
                // same transactional history as every other gesture.
                match self.vertex_grab_offset(kind, source_id, world) {
                    Some(pointer_offset) => {
                        self.component_drag = Some(ComponentDrag {
                            component_id: source_id,
                            pointer_offset,
                            before: self.snapshot(),
                            orphaned: false,
                        });
                    }
                    None => {
                        router.release_pointer(PointerButton::Primary, owner);
                    }
                }
            }
            _ => {
                router.release_pointer(PointerButton::Primary, owner);
            }
        }
    }

    /// Grab offset of the nearest editable vertex of a wire or a trace.
    ///
    /// Whole-vertex editing is out of scope here, but one movable point per
    /// object is what makes an existing route repairable. The nearest vertex is
    /// the one the user visually grabbed, and the radius is wider than the pick
    /// tolerance so both endpoints of a wire stay reachable.
    fn vertex_grab_offset(
        &self,
        kind: ElectronicsSelectionKind,
        source_id: Uuid,
        world: Vec2,
    ) -> Option<Vec2> {
        let points = match (kind, self.active_surface) {
            (ElectronicsSelectionKind::Wire, CadSurfaceKind::Schematic) => {
                let wire = self
                    .schematic
                    .wires
                    .iter()
                    .find(|wire| wire.id == source_id)?;
                vec![wire.start, wire.end]
            }
            (ElectronicsSelectionKind::Trace, CadSurfaceKind::Pcb) => self
                .pcb
                .traces
                .iter()
                .find(|trace| trace.id == source_id)?
                .points
                .clone(),
            _ => return None,
        };
        let tolerance = VERTEX_GRAB_TOLERANCE_SCREEN / self.camera.zoom.max(MIN_ZOOM);
        points
            .iter()
            .copied()
            .min_by(|left, right| left.distance(world).total_cmp(&right.distance(world)))
            .filter(|point| point.distance(world) <= tolerance)
            .map(|point| point - world)
    }

    /// Moves one dragged component to a resolved world position.
    pub(super) fn move_component_to(&mut self, component_id: Uuid, target: Vec2) -> DragOutcome {
        if self.active_surface == CadSurfaceKind::Schematic {
            let Some(component) = self
                .schematic
                .components
                .iter_mut()
                .find(|component| component.id == component_id)
            else {
                return DragOutcome::Missing;
            };
            if component.locked {
                return DragOutcome::Unchanged;
            }
            if component.position.distance(target) <= f32::EPSILON {
                return DragOutcome::Unchanged;
            }
            component.position = target;
            self.schematic.sync_wire_anchors();
            DragOutcome::Moved
        } else {
            if let Some(via) = self.pcb.vias.iter_mut().find(|via| via.id == component_id) {
                let previous = via.position;
                if previous.distance(target) <= f32::EPSILON { return DragOutcome::Unchanged; }
                via.position = target;
                for trace in self.pcb.traces.iter_mut().filter(|trace| trace.net == via.net) {
                    for point in &mut trace.points {
                        if point.distance(previous) <= 0.0001 { *point = target; }
                    }
                }
                self.pcb.rebuild_airwires();
                return DragOutcome::Moved;
            }
            let Some(component) = self
                .pcb
                .components
                .iter_mut()
                .find(|component| component.component_id == component_id)
            else {
                return DragOutcome::Missing;
            };
            if component.locked {
                return DragOutcome::Unchanged;
            }
            if component.position.distance(target) <= f32::EPSILON {
                return DragOutcome::Unchanged;
            }
            component.position = target;
            self.pcb.rebuild_airwires();
            DragOutcome::Moved
        }
    }

    /// Moves one endpoint of a schematic wire segment.
    ///
    /// Dragging an endpoint detaches it: an anchored endpoint is owned by the
    /// component and `sync_wire_anchors` would immediately put it back.
    /// Clearing the anchor is the same gesture every CAD tool performs when a wire
    /// end is pulled off a pin, and the netlist and DRC report the lost
    /// connection instead of this layer inventing a validation of its own.
    pub(super) fn move_wire_vertex(&mut self, wire_id: Uuid, index: usize, target: Vec2) -> DragOutcome {
        let Some((start, end)) = self
            .schematic
            .wires
            .iter()
            .find(|wire| wire.id == wire_id)
            .map(|wire| (wire.start, wire.end))
        else {
            return DragOutcome::Missing;
        };
        let before = if index == 0 { start } else { end };
        if before.distance(target) <= f32::EPSILON {
            return DragOutcome::Unchanged;
        }
        let Some(wire) = self
            .schematic
            .wires
            .iter_mut()
            .find(|wire| wire.id == wire_id)
        else {
            return DragOutcome::Missing;
        };
        if index == 0 {
            wire.start = target;
            wire.start_anchor = None;
        } else {
            wire.end = target;
            wire.end_anchor = None;
        }
        self.schematic.sync_wire_anchors();
        DragOutcome::Moved
    }

    /// Moves one vertex of a PCB trace.
    pub(super) fn move_trace_vertex(&mut self, trace_id: Uuid, index: usize, target: Vec2) -> DragOutcome {
        let Some(before) = self
            .pcb
            .traces
            .iter()
            .find(|trace| trace.id == trace_id)
            .and_then(|trace| trace.points.get(index).copied())
        else {
            return DragOutcome::Missing;
        };
        if before.distance(target) <= f32::EPSILON {
            return DragOutcome::Unchanged;
        }
        let Some(trace) = self
            .pcb
            .traces
            .iter_mut()
            .find(|trace| trace.id == trace_id)
        else {
            return DragOutcome::Missing;
        };
        if let Some(point) = trace.points.get_mut(index) {
            *point = target;
        }
        self.pcb.rebuild_airwires();
        DragOutcome::Moved
    }
}
