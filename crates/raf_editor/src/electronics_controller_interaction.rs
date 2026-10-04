//! Native Electronics authoring and interaction mechanics.
//!
//! Pointer gestures, placement, routing, selection editing, undo/redo and
//! persistence are kept outside the document controller's state/analysis
//! module. The child module intentionally operates on the live controller;
//! it does not create a parallel document or renderer bridge.

use super::*;

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

/// Outcome of one frame of a component or polyline-vertex drag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragOutcome {
    /// The dragged object is no longer in the document. The gesture is orphaned
    /// and must not be presented as a normal edit.
    Missing,
    /// The object exists but the gesture resolved to no new geometry, for
    /// example a locked part or a snap that landed on the same grid point.
    Unchanged,
    /// The document moved.
    Moved,
}

fn rect_contains(rect: raf_ui::UiRect, point: [f32; 2]) -> bool {
    point[0] >= rect.x
        && point[0] <= rect.x + rect.width
        && point[1] >= rect.y
        && point[1] <= rect.y + rect.height
}

fn pan_button_for_press(
    tool: ElectronicsTool,
    space_pan: bool,
    primary_pressed: bool,
    middle_pressed: bool,
) -> Option<PointerButton> {
    if (tool == ElectronicsTool::Pan || space_pan) && primary_pressed {
        Some(PointerButton::Primary)
    } else if middle_pressed {
        Some(PointerButton::Middle)
    } else {
        None
    }
}

/// Maps a CAD object kind onto the editor-owned selection kind.
fn selection_kind(kind: CadObjectKind) -> ElectronicsSelectionKind {
    match kind {
        CadObjectKind::Component => ElectronicsSelectionKind::Component,
        CadObjectKind::Pin | CadObjectKind::Pad => ElectronicsSelectionKind::Pin,
        CadObjectKind::Wire => ElectronicsSelectionKind::Wire,
        CadObjectKind::Trace => ElectronicsSelectionKind::Trace,
        _ => ElectronicsSelectionKind::Other,
    }
}

/// True while the active tool keeps an in-progress run the user can finish or
/// abandon: a schematic wire or a board outline.
///
/// These are the only two tools that hold pending geometry between clicks, so
/// they are also the only two where a double click has something to end. The
/// surface is part of the test because each tool only exists on one of them.
fn chaining_tool(tool: ElectronicsTool, surface: CadSurfaceKind) -> bool {
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
fn wire_preview_follows_pointer(tool: ElectronicsTool, wire_armed: bool) -> bool {
    wire_armed && matches!(tool, ElectronicsTool::Wire | ElectronicsTool::Select)
}

/// True when a primary release has to resolve the armed wire.
///
/// Wire is the click-click route and the pin drag-drop; Select is the contextual
/// pin drag. Both end on a release, but only Wire chains to the next segment.
fn wire_gesture_owns_release(tool: ElectronicsTool, surface: CadSurfaceKind) -> bool {
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
fn press_starts_pin_wire(
    tool: ElectronicsTool,
    surface: CadSurfaceKind,
    picked: Option<CadObjectKind>,
) -> bool {
    matches!(
        (tool, surface),
        (ElectronicsTool::Select, CadSurfaceKind::Schematic)
    ) && matches!(picked, Some(CadObjectKind::Pin))
}

/// Every pointer button the router can arbitrate.
const POINTER_BUTTONS: [PointerButton; 5] = [
    PointerButton::Primary,
    PointerButton::Secondary,
    PointerButton::Middle,
    PointerButton::Back,
    PointerButton::Forward,
];

/// True when this canvas itself holds a pointer button.
///
/// Replaces the previous global `!router.has_pointer_capture()` test: a capture
/// held by an unrelated surface (a panel slider, a dock drag, a modal) no longer
/// freezes the viewport, while the canvas still refuses to zoom the world out
/// from under its own in-flight gesture.
fn canvas_pointer_busy(router: &InputRouter, owner: InputOwner) -> bool {
    POINTER_BUTTONS
        .into_iter()
        .any(|button| router.pointer_owner(button) == Some(owner))
}

/// True when a different surface owns a pointer button. A passive hover must
/// never fight the surface that is actually being dragged.
fn foreign_pointer_capture(router: &InputRouter, owner: InputOwner) -> bool {
    POINTER_BUTTONS.into_iter().any(|button| {
        router
            .pointer_owner(button)
            .is_some_and(|held| held != owner)
    })
}

/// True when the hover pick has to run again for this pointer sample.
fn hover_pick_required(previous_world: Option<Vec2>, world: Vec2, zoom: f32) -> bool {
    match previous_world {
        // Re-entering the canvas has no previous sample to compare against, and
        // the frame before it had no hover target to keep.
        None => true,
        Some(previous) => {
            previous.distance(world) * zoom.max(MIN_ZOOM) >= HOVER_REPICK_MIN_DISTANCE_SCREEN
        }
    }
}

/// True once the pointer moved far enough from the press origin to be a drag.
fn pointer_left_press_origin(
    router: &InputRouter,
    owner: InputOwner,
    canvas: EditorRect,
    local: Vec2,
) -> bool {
    router
        .pointer_capture(PointerButton::Primary)
        .filter(|capture| capture.owner == owner)
        .and_then(|capture| canvas.local_point(capture.origin))
        .map(|origin| Vec2::from(origin).distance(local) >= PRESS_DRAG_THRESHOLD)
        .unwrap_or(false)
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
fn grabbed_vertex_index(points: &[Vec2], offset: Vec2, press_world: Vec2) -> Option<usize> {
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
fn drag_polyline_before(
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

/// Zoom multiplier for one frame of wheel input, expressed in notches.
fn wheel_zoom_factor(scroll_y: f32) -> f32 {
    let notches = (scroll_y / SCROLL_UNITS_PER_NOTCH)
        .clamp(-ZOOM_NOTCHES_PER_FRAME_MAX, ZOOM_NOTCHES_PER_FRAME_MAX);
    ZOOM_STEP_PER_NOTCH
        .powf(notches)
        .clamp(ZOOM_STEP_MIN, ZOOM_STEP_MAX)
}

impl NativeElectronicsEditor {
    /// Routes one native frame after RafUI has had first refusal over its own
    /// controls. The canvas only captures on an actual press; hover and scroll
    /// never steal ownership from the rest of the editor.
    pub fn process_input(
        &mut self,
        input: &InputSnapshot,
        router: &mut InputRouter,
        canvas: EditorRect,
    ) -> ElectronicsInputResult {
        let owner = InputOwner::ElectronicsCanvas;
        if self.placement_drag_active {
            return self.process_library_drag_input(input, canvas);
        }
        if router.has_exclusive_pointer_capture() && router.exclusive_pointer_owner() != Some(owner)
        {
            return ElectronicsInputResult::default();
        }
        // While a destructive confirmation is armed, the canvas stops reacting to
        // the keys that would act on the document (Delete, R, Space pan). Escape
        // is still handled below, because the canvas may be the only surface that
        // receives it when the pointer is not over the dialog.
        let modal_captures_input = self.pending_delete;

        let inside_canvas = input
            .pointer_position
            .and_then(|point| canvas.local_point(point));
        let owns_gesture =
            router.exclusive_pointer_owner() == Some(owner) || self.secondary_pointer.is_some();

        // Escape resolves the pending gesture wherever the pointer is.
        //
        // `wire_start` and `board_outline_start` deliberately never capture the
        // pointer, so `owns_gesture` alone could never see them: the user would
        // start a wire, move to a panel, press Escape, and the orange preview
        // stayed frozen. `has_pending_gesture` also covers the context menu and
        // the armed confirmation, which are pending canvas-owned states.
        if input.key_pressed(InputKey::Escape) && (owns_gesture || self.has_pending_gesture()) {
            self.cancel_gesture(router, owner);
            return ElectronicsInputResult {
                changed: true,
                request_redraw: true,
            };
        }

        // A drag that lost its pointer capture without a release event is a real
        // edit that already reached the document, not a cancelled gesture. It is
        // committed here, before anything can roll it back, so it can never
        // survive outside the undo stack and outside the dirty flag.
        if self.component_drag.is_some()
            && !router.is_pointer_owned_by(PointerButton::Primary, owner)
        {
            let committed = self.commit_lost_drag(router, owner);
            return ElectronicsInputResult {
                changed: committed,
                request_redraw: true,
            };
        }
        // Camera state only, so a pan whose capture disappeared is simply retired.
        if self
            .pan_pointer
            .is_some_and(|button| !router.is_pointer_owned_by(button, owner))
        {
            self.pan_pointer = None;
        }

        // Pointer outside the canvas with nothing in flight: the hover target and
        // the preview anchor must not survive it, otherwise the highlight sticks
        // to an object the cursor is no longer over and re-entry would reuse a
        // stale repick decision.
        if inside_canvas.is_none() && !owns_gesture {
            let had_hover = self.hovered.is_some();
            self.set_hovered(None);
            self.pointer_world = None;
            return ElectronicsInputResult {
                changed: had_hover,
                request_redraw: had_hover,
            };
        }

        let Some(pointer) = inside_canvas.or_else(|| {
            owns_gesture
                .then(|| {
                    input
                        .pointer_position
                        .map(|point| [point[0] - canvas.x, point[1] - canvas.y])
                })
                .flatten()
        }) else {
            // The pointer left the window while this canvas owned a gesture.
            // There is no position left to route it with, so it is rolled back
            // instead of staying armed against a capture nothing would release.
            if owns_gesture {
                self.cancel_gesture(router, owner);
                return ElectronicsInputResult {
                    changed: true,
                    request_redraw: true,
                };
            }
            return ElectronicsInputResult::default();
        };
        let size = Vec2::new(canvas.width.max(1.0), canvas.height.max(1.0));
        self.ensure_camera(canvas);
        let panel = crate::electronics_minimap::overlay_rect(size);
        let image = crate::electronics_minimap::image_rect(size);
        let over_panel = rect_contains(panel, pointer);
        let over_image = rect_contains(image, pointer);

        // Sampled before the assignment below so the hover repick can tell a real
        // cursor movement from a repeated frame.
        let previous_pointer_world = self.pointer_world;
        let world = self.camera.world_from_screen(Vec2::from(pointer), size);
        let pointer_changed = self.pointer_world != Some(world);
        self.pointer_world = Some(world);

        let mut result = ElectronicsInputResult {
            changed: false,
            request_redraw: false,
        };

        // Hover is interaction work, not render-loop work, but it must stay
        // bounded: the pick is skipped while a gesture owns the pointer, while
        // another surface captured it, while the cursor is over the minimap, and
        // whenever the cursor has not travelled a whole logical pixel. The canvas
        // therefore pays at most one scene walk per frame that actually moved.
        let editing = self.component_drag.is_some()
            || self.pan_pointer.is_some()
            || self.secondary_pointer.is_some()
            || self.minimap_drag
            || self.board_outline_start.is_some()
            || (self.tool == ElectronicsTool::Wire && self.wire_start.is_some())
            || input.button_down(PointerButton::Primary)
            || input.button_down(PointerButton::Secondary)
            || input.button_down(PointerButton::Middle);
        if self.update_pointer_hover(
            router,
            owner,
            world,
            previous_pointer_world,
            over_panel,
            editing,
        ) {
            result.request_redraw = true;
        }

        if over_image
            && input.button_pressed(PointerButton::Primary)
            && !router.has_pointer_capture()
        {
            self.minimap_drag = router.try_capture_pointer(
                PointerButton::Primary,
                owner,
                CaptureMode::Exclusive,
                input.pointer_position.unwrap_or_default(),
                input.time_seconds,
            );
        }
        if self.minimap_drag {
            if let Some(center) = crate::electronics_minimap::world_at(
                &self.scene,
                self.camera,
                size,
                Vec2::from(pointer),
            ) {
                self.camera.center = center;
                self.touch();
            }
            if input.button_released(PointerButton::Primary)
                || !input.button_down(PointerButton::Primary)
            {
                self.minimap_drag = false;
                router.release_pointer(PointerButton::Primary, owner);
            }
            return ElectronicsInputResult {
                changed: true,
                request_redraw: true,
            };
        }
        if over_panel && !owns_gesture {
            return result;
        }

        // Secondary click is a click-or-pan gesture in CAD. A short click
        // opens the context menu on release; a drag captures the secondary
        // button and pans the camera. Opening the menu on press used to leave
        // a stale interaction line behind and made right-drag impossible.
        if inside_canvas.is_some() && input.button_pressed(PointerButton::Secondary) {
            // A secondary gesture always takes navigation priority. If a
            // wire/outline route was in progress, end it before panning so
            // the preview cannot look like a stray black line during a
            // context-menu click or right-drag. Ending a chained route loses
            // nothing: the last confirmed point is already a committed
            // segment endpoint, not pending state.
            if self.wire_start.take().is_some() || self.board_outline_start.take().is_some() {
                self.rebuild_scene_internal();
                self.touch_ui();
                result.request_redraw = true;
            }
            self.secondary_pointer = Some(SecondaryPointerState {
                start: Vec2::from(pointer),
                dragging: false,
            });
            self.context_menu_position = None;
            result.request_redraw = true;
        }

        if let Some(mut secondary) = self.secondary_pointer.take() {
            let moved = Vec2::from(pointer).distance(secondary.start) >= PRESS_DRAG_THRESHOLD;
            if !secondary.dragging && input.button_down(PointerButton::Secondary) && moved {
                secondary.dragging = router.try_capture_pointer(
                    PointerButton::Secondary,
                    owner,
                    CaptureMode::Exclusive,
                    input.pointer_position.unwrap_or([canvas.x, canvas.y]),
                    input.time_seconds,
                );
                if secondary.dragging {
                    self.context_menu_position = None;
                    self.touch_ui();
                }
            }

            if secondary.dragging && router.is_pointer_owned_by(PointerButton::Secondary, owner) {
                if input.pointer_delta != [0.0, 0.0] {
                    self.camera.center -= Vec2::from(input.pointer_delta) / self.camera.zoom;
                    self.touch();
                    result.changed = true;
                    result.request_redraw = true;
                }
                if input.button_released(PointerButton::Secondary)
                    || !input.button_down(PointerButton::Secondary)
                {
                    router.release_pointer(PointerButton::Secondary, owner);
                    result.request_redraw = true;
                } else {
                    self.secondary_pointer = Some(secondary);
                }
            } else if input.button_released(PointerButton::Secondary)
                || !input.button_down(PointerButton::Secondary)
            {
                if inside_canvas.is_some() && !secondary.dragging && !moved {
                    self.select_secondary_target(world);
                    self.context_menu_position = input.pointer_position;
                    self.touch_ui();
                    result.changed = true;
                    result.request_redraw = true;
                }
            } else {
                self.secondary_pointer = Some(secondary);
            }
        }

        if self.apply_shortcuts(input, modal_captures_input) {
            result.changed = true;
            result.request_redraw = true;
        }

        // Wheel zoom, normalized to notches so one mouse notch is a smooth and
        // reproducible step on any device, and a trackpad reports a fraction of
        // one. Only this canvas holding the pointer blocks it; an unrelated
        // capture no longer freezes the view. `scroll_delta[0]` is ignored on
        // purpose: Windows does not deliver a meaningful horizontal wheel delta,
        // and treating it as zoom would fight the dock and panel scroll that do
        // consume it.
        if input.scroll_delta[1].abs() > f32::EPSILON && !canvas_pointer_busy(router, owner) {
            let factor = wheel_zoom_factor(input.scroll_delta[1]);
            if (factor - 1.0).abs() > f32::EPSILON {
                self.camera.zoom_at(factor, Vec2::from(pointer), size);
                self.touch();
                result.changed = true;
                result.request_redraw = true;
            }
        }

        let space_pan = input.key_down(InputKey::Space) && !modal_captures_input;
        if let Some(pan_button) = pan_button_for_press(
            self.tool,
            space_pan,
            input.button_pressed(PointerButton::Primary),
            input.button_pressed(PointerButton::Middle),
        ) {
            if router.try_capture_pointer(
                pan_button,
                owner,
                CaptureMode::Exclusive,
                input.pointer_position.unwrap_or([canvas.x, canvas.y]),
                input.time_seconds,
            ) {
                self.pan_pointer = Some(pan_button);
                result.request_redraw = true;
            }
        }

        let pan_button = self
            .pan_pointer
            .filter(|button| router.is_pointer_owned_by(*button, owner));
        if let Some(pan_button) = pan_button {
            if input.pointer_delta != [0.0, 0.0] {
                self.camera.center -= Vec2::from(input.pointer_delta) / self.camera.zoom;
                self.touch();
                result.changed = true;
                result.request_redraw = true;
            }
            if input.button_released(pan_button) || !input.button_down(pan_button) {
                router.release_pointer(pan_button, owner);
                self.pan_pointer = None;
            }
        }

        if inside_canvas.is_some()
            && input.button_pressed(PointerButton::Primary)
            && self.tool != ElectronicsTool::Pan
            && !router.has_pointer_capture()
            && router.try_capture_pointer(
                PointerButton::Primary,
                owner,
                CaptureMode::Exclusive,
                input.pointer_position.unwrap_or([canvas.x, canvas.y]),
                input.time_seconds,
            )
        {
            if self.context_menu_position.take().is_some() {
                self.touch_ui();
            }
            match self.tool {
                ElectronicsTool::Select => {
                    self.begin_selection_or_drag(world, router, owner, &mut result);
                }
                ElectronicsTool::Wire => {
                    self.handle_wire_click(world, router, owner, &mut result);
                }
                ElectronicsTool::Route => {
                    self.handle_route_click(world, router, owner, &mut result);
                }
                ElectronicsTool::Place => {
                    let changed = self.place_component(world);
                    router.release_pointer(PointerButton::Primary, owner);
                    result.changed |= changed;
                    result.request_redraw |= changed;
                }
                ElectronicsTool::BoardOutline => {
                    self.handle_board_outline_click(world, router, owner, &mut result);
                }
                ElectronicsTool::Pan => {}
            }
        }

        if self.component_drag.is_some()
            && router.is_pointer_owned_by(PointerButton::Primary, owner)
        {
            if input.button_down(PointerButton::Primary) && input.pointer_delta != [0.0, 0.0] {
                let drag_kind = self.selection.map(|selection| selection.kind);
                let drag_state = self.component_drag.as_ref().map(|drag| {
                    (
                        drag.component_id,
                        drag.pointer_offset,
                        drag_polyline_before(drag, drag_kind),
                    )
                });
                if let Some((source_id, offset, before_points)) = drag_state {
                    let target = self.snap_world(world + offset);
                    let press = self.press_world(router, owner, canvas, size);
                    // `component_id` carries the moved component, or the wire or
                    // trace whose vertex is being pulled. The committed selection
                    // kind tells the three apart, so vertex editing reuses this
                    // one gesture instead of adding a second parallel drag state.
                    let vertex = match (drag_kind, before_points) {
                        (Some(ElectronicsSelectionKind::Wire), Some(points))
                        | (Some(ElectronicsSelectionKind::Trace), Some(points)) => {
                            press.and_then(|at| grabbed_vertex_index(&points, offset, at))
                        }
                        _ => None,
                    };
                    let outcome = match (drag_kind, vertex) {
                        (Some(ElectronicsSelectionKind::Wire), Some(index)) => {
                            self.move_wire_vertex(source_id, index, target)
                        }
                        (Some(ElectronicsSelectionKind::Trace), Some(index)) => {
                            self.move_trace_vertex(source_id, index, target)
                        }
                        (Some(ElectronicsSelectionKind::Wire), None)
                        | (Some(ElectronicsSelectionKind::Trace), None) => DragOutcome::Missing,
                        _ => self.move_component_to(source_id, target),
                    };
                    match outcome {
                        DragOutcome::Missing => {
                            if let Some(drag) = self.component_drag.as_mut() {
                                drag.orphaned = true;
                            }
                        }
                        DragOutcome::Unchanged => {}
                        DragOutcome::Moved => {
                            self.rebuild_scene();
                            self.touch();
                            result.changed = true;
                            result.request_redraw = true;
                        }
                    }
                }
            }
            if input.button_released(PointerButton::Primary)
                || !input.button_down(PointerButton::Primary)
            {
                router.release_pointer(PointerButton::Primary, owner);
                if let Some(drag) = self.component_drag.take() {
                    let orphaned = drag.orphaned;
                    if self.history.record(drag.before, &self.snapshot()) {
                        self.dirty = true;
                    }
                    if orphaned {
                        // The object the gesture worked on is gone from the
                        // document, so the selection that pointed at it would let
                        // Delete and Rotate act on a stale identity.
                        self.selection = None;
                        self.interaction.clear_selection();
                    }
                    // Force the next frame to repick: the hover was suppressed for
                    // the whole gesture and must come back when it ends.
                    self.pointer_world = None;
                    self.touch_ui();
                    result.changed = true;
                    result.request_redraw = true;
                }
            }
        }

        // Double click leaves a chaining tool.
        //
        // The click pair is reconstructed from the release events this canvas
        // already owns. `InputSnapshot` has no click counter and no double click
        // flag, and the CAD canvas is not a `RafUI` surface, so the
        // `UiEventKind::DoubleClick` that the asset and hierarchy surfaces
        // dispatch never reaches it. The pair is judged on the release itself, at
        // the same point where the click-click route would otherwise commit a
        // segment, which is what makes the two impossible to confuse: a release
        // only pairs with the release right before it when both landed within
        // `DOUBLE_CLICK_MAX_DISTANCE_SCREEN` of each other. Clicking pin A and
        // then pin B is the ordinary route and lands far outside that radius.
        //
        // Nothing is confirmed before this runs, and the run is dropped instead of
        // committed, so a double click can never produce a zero-length segment.
        if input.button_released(PointerButton::Primary)
            && inside_canvas.is_some()
            && chaining_tool(self.tool, self.active_surface)
        {
            let tolerance = DOUBLE_CLICK_MAX_DISTANCE_SCREEN / self.camera.zoom.max(MIN_ZOOM);
            if self
                .interaction
                .consume_double_click(world, input.time_seconds, tolerance)
            {
                self.leave_chaining_tool(&mut result, router, owner);
                return result;
            }
        }

        // Wire tool pointer lifetime.
        //
        // The press keeps the capture on purpose: that is what makes the natural
        // `pin -> drag -> drop` gesture possible, because the in-flight preview
        // needs the pointer. A release that never moved is the existing
        // click-click route and leaves the armed start untouched; a release over
        // another pin commits the segment and the chain continues from there.
        // Under Select the same block resolves the contextual pin drag, which
        // never chains and never commits a click: see `begin_selection_or_drag`.
        if wire_gesture_owns_release(self.tool, self.active_surface)
            && self.pan_pointer.is_none()
            && self.wire_start.is_some()
            && router.is_pointer_owned_by(PointerButton::Primary, owner)
            && (input.button_released(PointerButton::Primary)
                || !input.button_down(PointerButton::Primary))
        {
            let release_world = self.camera.world_from_screen(Vec2::from(pointer), size);
            let dragged = pointer_left_press_origin(router, owner, canvas, Vec2::from(pointer));
            let contextual = self.tool == ElectronicsTool::Select;
            if dragged && self.pin_endpoint_at(release_world).is_some() {
                if self.commit_wire_segment(release_world) {
                    result.changed = true;
                    result.request_redraw = true;
                }
            }
            if contextual {
                // The contextual drag is one gesture per press: it either landed
                // on a pin and committed, or it is dropped here. Releasing the
                // wire back to Select is what guarantees the gesture never leaves
                // a wire hanging in mid air, and it writes nothing to the
                // document, so there is no edit for the history to record.
                self.wire_start = None;
                self.pointer_world = None;
                self.rebuild_scene();
                self.touch_ui();
                result.changed = true;
                result.request_redraw = true;
            }
            router.release_pointer(PointerButton::Primary, owner);
        }

        if self.active_surface == CadSurfaceKind::Schematic
            && wire_preview_follows_pointer(self.tool, self.wire_start.is_some())
            && pointer_changed
        {
            self.rebuild_scene();
            self.touch();
            result.request_redraw = true;
        }
        if self.active_surface == CadSurfaceKind::Pcb
            && self.tool == ElectronicsTool::BoardOutline
            && self.board_outline_start.is_some()
            && pointer_changed
        {
            self.rebuild_scene();
            self.touch();
            result.request_redraw = true;
        }

        result
    }

    /// World position of the press that started the current primary gesture.
    ///
    /// The pointer capture keeps its origin for the whole gesture, so this is the
    /// stable reference that freezes a vertex grab. It is resolved against the
    /// live camera, which cannot move while the canvas holds the primary button:
    /// the wheel is blocked for this owner and no pan can be armed at the same
    /// time.
    fn press_world(
        &self,
        router: &InputRouter,
        owner: InputOwner,
        canvas: EditorRect,
        size: Vec2,
    ) -> Option<Vec2> {
        router
            .pointer_capture(PointerButton::Primary)
            .filter(|capture| capture.owner == owner)
            .and_then(|capture| canvas.local_point(capture.origin))
            .map(|origin| self.camera.world_from_screen(Vec2::from(origin), size))
    }

    /// Refreshes the pointer hover target for one frame.
    ///
    /// Returns `true` when the hover target actually changed, which is the only
    /// reason a redraw is requested: an unchanged hover must never invalidate the
    /// retained chrome.
    fn update_pointer_hover(
        &mut self,
        router: &InputRouter,
        owner: InputOwner,
        world: Vec2,
        previous_world: Option<Vec2>,
        over_panel: bool,
        editing: bool,
    ) -> bool {
        // While a gesture is in flight the hover is cleared instead of repainted:
        // the preview and the drag own the cursor, and a highlight that followed
        // the pointer would fight both.
        let target = if editing || over_panel || foreign_pointer_capture(router, owner) {
            None
        } else if hover_pick_required(previous_world, world, self.camera.zoom) {
            let tolerance = PICK_TOLERANCE_SCREEN / self.camera.zoom.max(MIN_ZOOM);
            self.interaction
                .update_hover_editable(&self.scene, world, tolerance)
                .map(|hit| ElectronicsSelection {
                    source_id: hit.selection.source_id.unwrap_or_else(Uuid::nil),
                    kind: selection_kind(hit.selection.kind),
                })
        } else {
            return false;
        };
        let changed = self.hovered != target;
        self.set_hovered(target);
        changed
    }

    /// True when a pointer gesture, a preview or a confirmation is armed.
    fn has_pending_gesture(&self) -> bool {
        self.component_drag.is_some()
            || self.wire_start.is_some()
            || self.board_outline_start.is_some()
            || self.secondary_pointer.is_some()
            || self.pan_pointer.is_some()
            || self.minimap_drag
            || self.context_menu_position.is_some()
            || self.pending_delete
    }

    /// Commits a drag whose pointer capture disappeared without a release event.
    ///
    /// The document already moved and the user can see it on screen, so dropping
    /// the gesture would leave a persistent change that is in neither the undo
    /// stack nor the dirty flag. The move is committed through the normal
    /// transactional path instead, and the drag is flagged as orphaned so the
    /// record says the gesture lost its grip. `cancel_gesture` must never run for
    /// this case: it would roll the document back and silently undo a movement
    /// the user already performed.
    fn commit_lost_drag(&mut self, router: &mut InputRouter, owner: InputOwner) -> bool {
        let Some(mut drag) = self.component_drag.take() else {
            return false;
        };
        drag.orphaned = true;
        let recorded = self.history.record(drag.before, &self.snapshot());
        if recorded {
            self.dirty = true;
        }
        router.release_pointer(PointerButton::Primary, owner);
        self.pointer_world = None;
        self.rebuild_scene();
        self.touch_ui();
        recorded
    }

    /// Applies the canvas keyboard shortcuts for one frame.
    ///
    /// Returns `true` when the document or the view changed. The bindings are
    /// declared in `shortcut` so the retained tooltips and this handler cannot
    /// drift apart.
    ///
    /// Only the single-key canvas gestures live here. `Ctrl+Z`, `Ctrl+Y` and
    /// `Delete` are deliberately NOT handled again: the application boundary
    /// already routes them through `editor_shortcuts` into this same document
    /// model (`electronics_shortcut_command` in `native_application.rs`), for the
    /// same reason it refuses to reuse the Game dispatcher there. Handling them
    /// in both places made one keypress undo two steps or arm the confirmation
    /// twice.
    ///
    /// The application boundary also refuses to call `process_input` while a
    /// RafUI text control owns the keyboard (`text_input_owned`), and the
    /// pointer-resolution gate in `process_input` refuses it when the cursor is
    /// outside the canvas, so neither a focused field nor another surface is
    /// reachable from here. The modifier test below is the second, local guard: a
    /// bare letter or a zoom key must never fire behind Ctrl/Cmd/Alt, where
    /// Ctrl+V is paste and Alt+F belongs to a menu.
    fn apply_shortcuts(&mut self, input: &InputSnapshot, modal_captures_input: bool) -> bool {
        if modal_captures_input {
            // The armed confirmation owns the keyboard. Canvas keys that would
            // act on the document stay out of the way until it is resolved.
            return false;
        }
        if !input.ime_preedit.is_empty() {
            // An IME composition in progress means a text control owns the
            // keyboard, whatever the application boundary decided. This is the
            // only local signal that is unambiguous: `text_input` is populated
            // for every printable key press, focused field or not, so it cannot
            // be used to detect focus.
            return false;
        }
        if input.modifiers.command_modifier() || input.modifiers.alt {
            return false;
        }
        if input.key_pressed(shortcut::ROTATE) {
            // `R` turns clockwise and `Shift+R` counter-clockwise. Without the
            // inversion a part could only be brought back by three more presses,
            // which reads as the shortcut being stuck.
            let turn = if input.modifiers.shift {
                -ROTATE_STEP_DEGREES
            } else {
                ROTATE_STEP_DEGREES
            };
            return self.rotate_selection_by(turn);
        }
        if input.key_pressed(shortcut::FIT_VIEW) {
            self.fit_view();
            return true;
        }
        if input.key_pressed(shortcut::TOGGLE_GRID) {
            self.toggle_grid();
            return true;
        }
        if shortcut::ZOOM_IN.iter().any(|key| input.key_pressed(*key)) {
            self.zoom_in();
            return true;
        }
        if shortcut::ZOOM_OUT.iter().any(|key| input.key_pressed(*key)) {
            self.zoom_out();
            return true;
        }
        if input.key_pressed(shortcut::SELECT_TOOL) {
            self.set_tool(ElectronicsTool::Select);
            return true;
        }
        if input.key_pressed(shortcut::PLACE_TOOL) {
            // Same behaviour as the toolbar Place button in both surfaces: the
            // tool is armed and the canvas hint already tells the user to pick a
            // library item first, so shortcut and button cannot disagree about
            // what `P` does. With no template armed the click places nothing,
            // which is reported as a missing localized notice rather than
            // invented here.
            self.set_tool(ElectronicsTool::Place);
            return true;
        }
        // `Wire` only routes schematic pins, so the shortcut respects the same
        // surface boundary the toolbar does instead of arming a tool that cannot
        // do anything in the current document. `Route` and `BoardOutline` are
        // PCB-only and are not part of the published shortcut set.
        if self.active_surface == CadSurfaceKind::Schematic
            && input.key_pressed(shortcut::WIRE_TOOL)
        {
            self.set_tool(ElectronicsTool::Wire);
            return true;
        }
        false
    }

    pub fn undo(&mut self) -> bool {
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

    fn select_secondary_target(&mut self, world: Vec2) {
        // A context menu belongs to the item under the pointer. Select it
        // first so Delete/Duplicate/Properties operate on the same object
        // the user invoked the menu for.
        let tolerance = PICK_TOLERANCE_SCREEN / self.camera.zoom.max(MIN_ZOOM);
        // Editable picking: a painted net label, airwire or DRC marker sitting on
        // top of the real geometry must not become the menu target.
        if let Some(hit) = self
            .interaction
            .select_editable_at(&self.scene, world, tolerance)
        {
            let kind = selection_kind(hit.selection.kind);
            let source_id = hit.selection.source_id.unwrap_or_else(Uuid::nil);
            self.selection = Some(ElectronicsSelection { source_id, kind });
            self.set_hovered(Some(ElectronicsSelection { source_id, kind }));
        } else {
            self.selection = None;
            self.interaction.clear_selection();
        }
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
        save_schematic_document(
            &session.path(&project.path, &session.schematic_file),
            &self.schematic,
        )
        .map_err(|error| format!("schematic save: {error}"))?;
        save_pcb_document(&session.path(&project.path, &session.pcb_file), &self.pcb)
            .map_err(|error| format!("pcb save: {error}"))?;
        self.dirty = false;
        Ok(())
    }

    fn begin_selection_or_drag(
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
        // When the pin cannot be resolved the press falls through to the ordinary
        // drag below. That happens when pin snapping is off, and it is the right
        // degradation: with snapping disabled the document has said that pins do
        // not capture pointers, so a press on one keeps meaning "move this part"
        // instead of arming a gesture that could never anchor.
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
    fn move_component_to(&mut self, component_id: Uuid, target: Vec2) -> DragOutcome {
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
    fn move_wire_vertex(&mut self, wire_id: Uuid, index: usize, target: Vec2) -> DragOutcome {
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
    fn move_trace_vertex(&mut self, trace_id: Uuid, index: usize, target: Vec2) -> DragOutcome {
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

    fn handle_wire_click(
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
    fn commit_wire_segment(&mut self, world: Vec2) -> bool {
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

    fn handle_route_click(
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
        let tolerance = PICK_TOLERANCE_SCREEN / self.camera.zoom.max(MIN_ZOOM);
        // The route tool keeps the legacy full pick on purpose: an airwire is a
        // painted overlay and `pick_editable` rejects it by design so it can never
        // steal a click from the copper underneath. Here the airwire IS the
        // target, so this is the one place that must be picked explicitly.
        if let Some(hit) = pick(&self.scene, world, tolerance) {
            if hit.selection.kind == CadObjectKind::Airwire {
                self.interaction.selected = Some(hit.selection);
                if self.route_selected_airwire() {
                    result.changed = true;
                    result.request_redraw = true;
                }
            }
        }
        router.release_pointer(PointerButton::Primary, owner);
    }

    fn handle_board_outline_click(
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
        if let Some(start) = self.board_outline_start.take() {
            let min = start.min(point);
            let max = start.max(point);
            if (max.x - min.x).abs() >= 20.0 && (max.y - min.y).abs() >= 20.0 {
                let before = self.snapshot();
                self.pcb.board_outline.points = vec![
                    Vec2::new(min.x, min.y),
                    Vec2::new(max.x, min.y),
                    Vec2::new(max.x, max.y),
                    Vec2::new(min.x, max.y),
                    Vec2::new(min.x, min.y),
                ];
                self.pcb.rebuild_airwires();
                if self.history.record(before, &self.snapshot()) {
                    self.dirty = true;
                }
                self.set_tool(ElectronicsTool::Select);
                self.rebuild_scene();
                result.changed = true;
                result.request_redraw = true;
            } else {
                self.board_outline_start = Some(start);
                self.touch_ui();
            }
        } else {
            self.board_outline_start = Some(point);
            self.touch_ui();
            result.changed = true;
            result.request_redraw = true;
        }
        router.release_pointer(PointerButton::Primary, owner);
    }

    pub(super) fn pin_endpoint_at(&self, point: Vec2) -> Option<(Uuid, Vec2, WireAnchor)> {
        if !self.snap_enabled {
            // Pin snapping used to be impossible to turn off, so a project that
            // disabled grid snapping still got wires jumping onto pins.
            return None;
        }
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

    /// Starts a pointer-driven placement originating in the component
    /// library. The retained library card owns the pointer capture; the CAD
    /// controller only tracks the semantic drag and renders its preview.
    pub(crate) fn begin_library_drag(&mut self, index: usize) -> bool {
        if index >= self.library.components.len() {
            return false;
        }
        self.placement_template = Some(index);
        self.placement_drag_active = true;
        self.placement_preview = None;
        self.set_tool(ElectronicsTool::Place);
        self.touch_ui();
        true
    }

    /// Completes a library drag using window-space coordinates. Placement is
    /// accepted only inside the Electronics canvas and never on the minimap.
    /// Returning `true` means a real component was committed.
    pub(crate) fn finish_library_drag_at(
        &mut self,
        pointer: Option<[f32; 2]>,
        canvas: EditorRect,
    ) -> bool {
        if !self.placement_drag_active {
            return false;
        }
        let size = Vec2::new(canvas.width.max(1.0), canvas.height.max(1.0));
        self.ensure_camera(canvas);
        let local = pointer.and_then(|point| canvas.local_point(point));
        let panel = crate::electronics_minimap::overlay_rect(size);
        let world = local
            .filter(|point| !rect_contains(panel, *point))
            .map(|point| self.camera.world_from_screen(Vec2::from(point), size));
        let changed = world.is_some_and(|world| self.place_component(world));
        self.placement_drag_active = false;
        self.placement_preview = None;
        self.pointer_world = None;
        self.rebuild_scene_internal();
        if !changed {
            self.touch_ui();
        }
        changed
    }

    fn process_library_drag_input(
        &mut self,
        input: &InputSnapshot,
        canvas: EditorRect,
    ) -> ElectronicsInputResult {
        let size = Vec2::new(canvas.width.max(1.0), canvas.height.max(1.0));
        let pointer = input.pointer_position;
        let local = pointer.and_then(|point| canvas.local_point(point));
        let panel = crate::electronics_minimap::overlay_rect(size);
        let world = local
            .filter(|point| !rect_contains(panel, *point))
            .map(|point| {
                self.ensure_camera(canvas);
                self.snap_world(self.camera.world_from_screen(Vec2::from(point), size))
            });

        if input.key_pressed(InputKey::Escape)
            || input.button_released(PointerButton::Primary)
            || (!input.button_down(PointerButton::Primary) && self.placement_preview.is_some())
        {
            let changed = if input.key_pressed(InputKey::Escape) {
                false
            } else {
                self.finish_library_drag_at(pointer, canvas)
            };
            if self.placement_drag_active {
                self.placement_drag_active = false;
                self.placement_preview = None;
                self.rebuild_scene_internal();
                self.touch_ui();
            }
            return ElectronicsInputResult {
                changed,
                request_redraw: true,
            };
        }

        if self.placement_preview != world {
            self.placement_preview = world;
            self.rebuild_scene_internal();
            self.touch_ui();
            return ElectronicsInputResult {
                changed: true,
                request_redraw: true,
            };
        }
        ElectronicsInputResult::default()
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

    pub(super) fn delete_selected(&mut self) -> bool {
        let Some(selection) = self.selection else {
            return false;
        };
        let before = self.snapshot();
        let mut changed = false;
        if self.active_surface == CadSurfaceKind::Pcb
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

    pub(super) fn route_selected_airwire(&mut self) -> bool {
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
        if !self.pcb.route_airwire(index) {
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
            layer: CadLayerKind::PcbTopCopper,
        });
        self.history.record(before, &self.snapshot());
        self.dirty = true;
        self.rebuild_scene();
        self.touch_ui();
        true
    }

    pub(super) fn rotate_selected(&mut self) -> bool {
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
    fn leave_chaining_tool(
        &mut self,
        result: &mut ElectronicsInputResult,
        router: &mut InputRouter,
        owner: InputOwner,
    ) {
        self.wire_start = None;
        self.board_outline_start = None;
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
    fn cancel_gesture(&mut self, router: &mut InputRouter, owner: InputOwner) {
        if let Some(drag) = self.component_drag.take() {
            self.schematic = drag.before.schematic;
            self.pcb = drag.before.pcb;
            self.rebuild_scene();
        }
        self.wire_start = None;
        self.board_outline_start = None;
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

#[cfg(test)]
mod tests {
    use super::*;
    use raf_core::PointerButtonSet;

    /// Canvas used by the pointer tests.
    ///
    /// The world points below are kept clear of the minimap panel, which owns the
    /// bottom-right corner and swallows presses of its own.
    fn test_canvas() -> EditorRect {
        EditorRect::new(0.0, 0.0, 640.0, 480.0)
    }

    /// Window position of a world point under a camera centred on the origin at
    /// zoom 1 over `test_canvas`.
    fn window_of(world: Vec2) -> [f32; 2] {
        [world.x + 320.0, world.y + 240.0]
    }

    fn buttons(list: &[PointerButton]) -> PointerButtonSet {
        let mut set = PointerButtonSet::default();
        for button in list {
            set.insert(*button);
        }
        set
    }

    fn frame(world: Vec2, time_seconds: f64) -> InputSnapshot {
        InputSnapshot {
            pointer_position: Some(window_of(world)),
            time_seconds,
            window_focused: true,
            ..InputSnapshot::default()
        }
    }

    fn press_frame(world: Vec2, time_seconds: f64) -> InputSnapshot {
        InputSnapshot {
            pointer_buttons_down: buttons(&[PointerButton::Primary]),
            pointer_pressed_buttons: buttons(&[PointerButton::Primary]),
            ..frame(world, time_seconds)
        }
    }

    fn hold_frame(world: Vec2, time_seconds: f64) -> InputSnapshot {
        InputSnapshot {
            pointer_buttons_down: buttons(&[PointerButton::Primary]),
            ..frame(world, time_seconds)
        }
    }

    fn release_frame(world: Vec2, time_seconds: f64) -> InputSnapshot {
        InputSnapshot {
            pointer_released_buttons: buttons(&[PointerButton::Primary]),
            ..frame(world, time_seconds)
        }
    }

    fn pin_position(editor: &NativeElectronicsEditor, component: usize, pin: usize) -> Vec2 {
        let placed = &editor.schematic.components[component];
        component_pin_world_position(placed, &placed.pins[pin])
    }

    /// Two resistors far enough apart that a drag between their pins can never be
    /// mistaken for a click, and placed in the upper half of the canvas.
    fn editor_with_two_resistors() -> NativeElectronicsEditor {
        let mut editor = NativeElectronicsEditor::empty("Pins");
        editor
            .schematic
            .add_component(raf_electronics::component::ElectronicComponent::resistor(
                "10k",
            ));
        editor
            .schematic
            .add_component(raf_electronics::component::ElectronicComponent::resistor(
                "4k7",
            ));
        for (index, component) in editor.schematic.components.iter_mut().enumerate() {
            component.position = if index == 0 {
                Vec2::new(-200.0, -100.0)
            } else {
                Vec2::new(80.0, -100.0)
            };
        }
        editor.rebuild_scene();
        editor.camera.center = Vec2::ZERO;
        editor.camera.zoom = 1.0;
        editor.camera_initialized = true;
        editor
    }

    /// A world point far from every pin in `editor_with_two_resistors`.
    fn empty_world() -> Vec2 {
        Vec2::new(-40.0, 60.0)
    }

    #[test]
    fn only_the_chaining_tools_can_be_left_with_a_double_click() {
        assert!(chaining_tool(
            ElectronicsTool::Wire,
            CadSurfaceKind::Schematic
        ));
        assert!(chaining_tool(
            ElectronicsTool::BoardOutline,
            CadSurfaceKind::Pcb
        ));
        // Route has no pending run to abandon, and neither tool means anything on
        // the surface it does not belong to.
        assert!(!chaining_tool(ElectronicsTool::Route, CadSurfaceKind::Pcb));
        assert!(!chaining_tool(ElectronicsTool::Wire, CadSurfaceKind::Pcb));
        assert!(!chaining_tool(
            ElectronicsTool::Select,
            CadSurfaceKind::Schematic
        ));
    }

    #[test]
    fn the_armed_wire_preview_follows_the_pointer_in_select_too() {
        assert!(wire_preview_follows_pointer(ElectronicsTool::Wire, true));
        assert!(wire_preview_follows_pointer(ElectronicsTool::Select, true));
        assert!(!wire_preview_follows_pointer(
            ElectronicsTool::Select,
            false
        ));
        assert!(!wire_preview_follows_pointer(ElectronicsTool::Route, true));
    }

    #[test]
    fn a_press_on_a_pin_starts_a_wire_only_in_schematic_select() {
        assert!(press_starts_pin_wire(
            ElectronicsTool::Select,
            CadSurfaceKind::Schematic,
            Some(CadObjectKind::Pin)
        ));
        // The Wire tool has its own press path, and a pin press there must not run
        // the contextual gesture on top of it.
        assert!(!press_starts_pin_wire(
            ElectronicsTool::Wire,
            CadSurfaceKind::Schematic,
            Some(CadObjectKind::Pin)
        ));
        // A pad is not a pin, and the board has no pins to route.
        assert!(!press_starts_pin_wire(
            ElectronicsTool::Select,
            CadSurfaceKind::Pcb,
            Some(CadObjectKind::Pad)
        ));
        assert!(!press_starts_pin_wire(
            ElectronicsTool::Select,
            CadSurfaceKind::Pcb,
            Some(CadObjectKind::Pin)
        ));
    }

    #[test]
    fn a_press_on_anything_but_a_pin_keeps_selecting_and_dragging() {
        for picked in [
            Some(CadObjectKind::Component),
            Some(CadObjectKind::Wire),
            Some(CadObjectKind::Trace),
            Some(CadObjectKind::NetLabel),
            None,
        ] {
            assert!(
                !press_starts_pin_wire(ElectronicsTool::Select, CadSurfaceKind::Schematic, picked),
                "selecting or dragging {picked:?} must not be taken over by the wire gesture"
            );
        }
    }

    #[test]
    fn dragging_from_a_pin_to_another_pin_wires_them_without_the_wire_tool() {
        let mut editor = editor_with_two_resistors();
        let mut router = InputRouter::default();
        let canvas = test_canvas();
        let from = pin_position(&editor, 0, 0);
        let to = pin_position(&editor, 1, 1);
        let before = editor.schematic.components[0].position;

        editor.process_input(&press_frame(from, 1.0), &mut router, canvas);

        assert!(
            editor.wire_start.is_some(),
            "a press on a pin has to arm the wire preview"
        );
        assert!(
            editor.component_drag.is_none(),
            "the gesture must not move the part it started from"
        );

        editor.process_input(&hold_frame(from.lerp(to, 0.5), 1.1), &mut router, canvas);
        editor.process_input(&release_frame(to, 1.2), &mut router, canvas);

        assert_eq!(editor.schematic.wires.len(), 1);
        assert!(editor.can_undo(), "the wire is an undoable edit");
        assert!(editor.is_dirty());
        assert!(
            editor.wire_start.is_none(),
            "the contextual drag never chains to a second segment"
        );
        assert_eq!(editor.tool(), ElectronicsTool::Select);
        assert_eq!(
            editor.schematic.components[0].position, before,
            "neither part may move while a wire is drawn between them"
        );
    }

    #[test]
    fn dropping_the_contextual_wire_on_empty_space_creates_nothing() {
        let mut editor = editor_with_two_resistors();
        let mut router = InputRouter::default();
        let canvas = test_canvas();
        let from = pin_position(&editor, 0, 0);

        editor.process_input(&press_frame(from, 1.0), &mut router, canvas);
        editor.process_input(
            &hold_frame(from.lerp(empty_world(), 0.5), 1.1),
            &mut router,
            canvas,
        );
        editor.process_input(&release_frame(empty_world(), 1.2), &mut router, canvas);

        assert!(editor.schematic.wires.is_empty());
        assert!(
            !editor.can_undo(),
            "a dropped wire is a dismissal, not an edit"
        );
        assert!(!editor.is_dirty());
        assert!(editor.wire_start.is_none());
    }

    #[test]
    fn a_double_click_leaves_the_wire_tool_and_draws_nothing() {
        let mut editor = editor_with_two_resistors();
        let mut router = InputRouter::default();
        let canvas = test_canvas();
        editor.set_tool(ElectronicsTool::Wire);
        let at = empty_world();

        editor.process_input(&press_frame(at, 1.0), &mut router, canvas);
        editor.process_input(&release_frame(at, 1.05), &mut router, canvas);
        assert_eq!(editor.tool(), ElectronicsTool::Wire);

        editor.process_input(&press_frame(at, 1.15), &mut router, canvas);
        editor.process_input(&release_frame(at, 1.2), &mut router, canvas);

        assert_eq!(editor.tool(), ElectronicsTool::Select);
        assert!(editor.wire_start.is_none());
        assert!(
            editor.schematic.wires.is_empty(),
            "the second click of the pair must not confirm a zero-length segment"
        );
        assert!(!editor.is_dirty());
    }

    #[test]
    fn a_double_click_abandons_a_half_drawn_board_outline() {
        let mut editor = NativeElectronicsEditor::empty("Outline");
        let mut router = InputRouter::default();
        let canvas = test_canvas();
        editor.set_surface(CadSurfaceKind::Pcb);
        editor.set_tool(ElectronicsTool::BoardOutline);
        editor.camera.center = Vec2::ZERO;
        editor.camera.zoom = 1.0;
        editor.camera_initialized = true;
        let at = empty_world();

        editor.process_input(&press_frame(at, 1.0), &mut router, canvas);
        assert!(editor.board_outline_start.is_some());
        editor.process_input(&release_frame(at, 1.05), &mut router, canvas);

        editor.process_input(&press_frame(at, 1.15), &mut router, canvas);
        editor.process_input(&release_frame(at, 1.2), &mut router, canvas);

        assert_eq!(editor.tool(), ElectronicsTool::Select);
        assert!(editor.board_outline_start.is_none());
        // A new PCB already owns a default outline, so the invariant is that the
        // abandoned rectangle left it untouched rather than replacing it.
        assert_eq!(
            editor.pcb.board_outline.points,
            raf_electronics::pcb::layout::BoardOutline::default_rect(420.0, 280.0).points,
            "the abandoned rectangle must not reach the document"
        );
    }

    #[test]
    fn clicking_two_pins_in_a_row_is_a_wire_and_stays_in_the_tool() {
        let mut editor = editor_with_two_resistors();
        let mut router = InputRouter::default();
        let canvas = test_canvas();
        editor.set_tool(ElectronicsTool::Wire);
        let from = pin_position(&editor, 0, 0);
        let to = pin_position(&editor, 1, 1);

        editor.process_input(&press_frame(from, 1.0), &mut router, canvas);
        editor.process_input(&release_frame(from, 1.05), &mut router, canvas);
        editor.process_input(&press_frame(to, 1.1), &mut router, canvas);
        editor.process_input(&release_frame(to, 1.15), &mut router, canvas);

        assert_eq!(editor.schematic.wires.len(), 1);
        assert_eq!(
            editor.tool(),
            ElectronicsTool::Wire,
            "two clicks on two pins are the route, not a double click"
        );
        assert!(
            editor.wire_start.is_some(),
            "and the run continues from the confirmed endpoint"
        );
    }

    #[test]
    fn a_press_on_a_component_body_still_moves_the_component() {
        let mut editor = editor_with_two_resistors();
        let mut router = InputRouter::default();
        let canvas = test_canvas();
        let body = editor.schematic.components[0].position;
        let moved = body + Vec2::new(40.0, 0.0);

        editor.process_input(&press_frame(body, 1.0), &mut router, canvas);
        assert!(editor.component_drag.is_some());
        assert!(
            editor.wire_start.is_none(),
            "a body press must not arm the wire gesture"
        );
        editor.process_input(
            &InputSnapshot {
                pointer_delta: [
                    window_of(moved)[0] - window_of(body)[0],
                    window_of(moved)[1] - window_of(body)[1],
                ],
                ..hold_frame(moved, 1.1)
            },
            &mut router,
            canvas,
        );
        editor.process_input(&release_frame(moved, 1.2), &mut router, canvas);

        assert_eq!(editor.schematic.components[0].position, moved);
        assert!(editor.schematic.wires.is_empty());
        assert!(editor.component_drag.is_none());
    }

    #[test]
    fn middle_drag_remains_pan_in_select_and_pan_modes() {
        assert_eq!(
            pan_button_for_press(ElectronicsTool::Select, false, false, true),
            Some(PointerButton::Middle)
        );
        assert_eq!(
            pan_button_for_press(ElectronicsTool::Pan, false, false, true),
            Some(PointerButton::Middle)
        );
    }

    #[test]
    fn space_pan_uses_primary_without_stealing_middle_drag() {
        assert_eq!(
            pan_button_for_press(ElectronicsTool::Select, true, true, false),
            Some(PointerButton::Primary)
        );
        assert_eq!(
            pan_button_for_press(ElectronicsTool::Select, true, false, true),
            Some(PointerButton::Middle)
        );
    }

    #[test]
    fn one_wheel_notch_is_a_reproducible_zoom_step() {
        // The platform adapter reports a whole mouse notch as 24.0 units, so
        // after normalization a notch is the same step everywhere instead of the
        // saturated 1.8 the raw 24.0 produced.
        assert!((wheel_zoom_factor(SCROLL_UNITS_PER_NOTCH) - ZOOM_STEP_PER_NOTCH).abs() < 1e-5);
        assert!(
            (wheel_zoom_factor(-SCROLL_UNITS_PER_NOTCH) - 1.0 / ZOOM_STEP_PER_NOTCH).abs() < 1e-5
        );
    }

    #[test]
    fn a_trackpad_reports_a_fraction_of_a_notch() {
        let fraction = wheel_zoom_factor(SCROLL_UNITS_PER_NOTCH / 4.0);
        assert!(fraction > 1.0 && fraction < ZOOM_STEP_PER_NOTCH);
    }

    #[test]
    fn a_fast_wheel_flick_cannot_jump_across_the_zoom_range() {
        let factor = wheel_zoom_factor(SCROLL_UNITS_PER_NOTCH * 20.0);
        assert!(factor <= ZOOM_STEP_MAX);
        assert!(
            factor < 1.8,
            "one flick must stay far below the old saturation"
        );
    }

    #[test]
    fn hover_pick_is_skipped_until_the_cursor_travels_a_pixel() {
        let world = Vec2::new(12.0, 8.0);
        // No previous sample: re-entering the canvas always repicks.
        assert!(hover_pick_required(None, world, 1.0));
        assert!(!hover_pick_required(Some(world), world, 1.0));
        assert!(!hover_pick_required(
            Some(Vec2::new(12.0, 8.0)),
            Vec2::new(12.0, 8.4),
            1.0
        ));
        assert!(hover_pick_required(
            Some(Vec2::new(12.0, 8.0)),
            Vec2::new(12.0, 9.5),
            1.0
        ));
    }

    #[test]
    fn the_vertex_grab_is_frozen_by_the_press_and_not_by_the_drag() {
        let points = [
            Vec2::new(0.0, 0.0),
            Vec2::new(0.0, 80.0),
            Vec2::new(80.0, 80.0),
        ];
        let press = Vec2::new(-2.0, 76.0);
        let offset = points[1] - press;
        assert_eq!(grabbed_vertex_index(&points, offset, press), Some(1));

        // Resolving against the live pointer would hand the gesture to the
        // neighbouring vertex as soon as the drag travelled far enough, and the
        // run would start following the wrong point.
        let dragged = press + Vec2::new(120.0, 0.0);
        assert_ne!(grabbed_vertex_index(&points, Vec2::ZERO, dragged), Some(1));
        // The original press keeps resolving to the same vertex.
        assert_eq!(grabbed_vertex_index(&points, offset, press), Some(1));
    }

    #[test]
    fn a_pin_inside_the_grid_ceiling_snaps_and_one_outside_it_does_not() {
        let mut editor = NativeElectronicsEditor::empty("Snap");
        editor.snap_enabled = true;
        editor.grid_step = 20.0;
        editor.camera.zoom = 0.2;
        editor
            .schematic
            .add_component(raf_electronics::component::ElectronicComponent::resistor(
                "10k",
            ));
        let pin = component_pin_world_position(
            &editor.schematic.components[0],
            &editor.schematic.components[0].pins[0],
        );
        // At this zoom the pure screen radius spans many grid cells, so only the
        // grid ceiling keeps a distant route from jumping onto the pin.
        assert!(editor.pin_endpoint_at(pin + Vec2::new(10.0, 0.0)).is_some());
        assert!(editor.pin_endpoint_at(pin + Vec2::new(18.0, 0.0)).is_none());
    }

    #[test]
    fn disabling_grid_snap_also_releases_the_pin_snap() {
        let mut editor = NativeElectronicsEditor::empty("Snap off");
        editor.snap_enabled = false;
        editor
            .schematic
            .add_component(raf_electronics::component::ElectronicComponent::resistor(
                "10k",
            ));
        let pin = component_pin_world_position(
            &editor.schematic.components[0],
            &editor.schematic.components[0].pins[0],
        );
        assert!(editor.pin_endpoint_at(pin).is_none());
    }
}
