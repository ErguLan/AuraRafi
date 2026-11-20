//! Canvas frame routing, wheel zoom and keyboard shortcuts.

use super::*;

use super::drag::{drag_polyline_before, grabbed_vertex_index, DragOutcome};
use super::gestures::{
    chaining_tool, wire_gesture_owns_release, wire_preview_follows_pointer,
};
use super::pointer::{
    canvas_pointer_busy, pan_button_for_press, pointer_left_press_origin, rect_contains,
};

/// Zoom multiplier for one frame of wheel input, expressed in notches.
pub(super) fn wheel_zoom_factor(scroll_y: f32) -> f32 {
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
        let minimap = self.minimap_rects(size);
        let over_panel = minimap.is_some_and(|rects| rect_contains(rects.panel, pointer));
        let over_image = !self.minimap_collapsed && minimap.is_some_and(|rects| rect_contains(rects.image, pointer));

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
            if (input.button_down(PointerButton::Primary) && input.pointer_delta != [0.0, 0.0])
                || input.button_released(PointerButton::Primary)
            {
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

        if self.active_surface == CadSurfaceKind::Pcb && self.tool == ElectronicsTool::Route && pointer_changed {
            let target_changed = previous_pointer_world.and_then(|p| self.route_hover_identity(p)) != self.route_hover_identity(world);
            if self.pcb_route_draft.is_some() || target_changed {
                self.rebuild_scene();
                if target_changed { self.touch_ui(); } else { self.touch(); }
                result.request_redraw = true;
            }
        }
        result
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
        if input.key_pressed(InputKey::Enter) && self.tool == ElectronicsTool::Route && self.active_surface == CadSurfaceKind::Pcb {
            return self.apply_pcb_tool_command("electronics.pcb.route.finish");
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
            if input.modifiers.shift && self.active_surface == CadSurfaceKind::Pcb {
                return self.apply_pcb_tool_command("electronics.pcb.via");
            }
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
}
