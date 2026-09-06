//! Native Electronics authoring and interaction mechanics.
//!
//! Pointer gestures, placement, routing, selection editing, undo/redo and
//! persistence are kept outside the document controller's state/analysis
//! module. The child module intentionally operates on the live controller;
//! it does not create a parallel document or renderer bridge.

use super::*;

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

        let inside_canvas = input
            .pointer_position
            .and_then(|point| canvas.local_point(point));
        let owns_gesture =
            router.exclusive_pointer_owner() == Some(owner) || self.secondary_pointer.is_some();
        if input.key_pressed(InputKey::Escape) && (inside_canvas.is_some() || owns_gesture) {
            self.cancel_gesture(router, owner);
            return ElectronicsInputResult {
                changed: true,
                request_redraw: true,
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
        let minimap = crate::electronics_minimap::overlay_rect(size);
        let over_minimap = pointer[0] >= minimap.x
            && pointer[0] <= minimap.x + minimap.width
            && pointer[1] >= minimap.y
            && pointer[1] <= minimap.y + minimap.height;
        if over_minimap
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
            if let Some(center) =
                crate::electronics_minimap::world_at(&self.scene, Vec2::from(pointer), size)
            {
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
        if over_minimap && !owns_gesture {
            return ElectronicsInputResult::default();
        }
        let world = self.camera.world_from_screen(Vec2::from(pointer), size);
        let pointer_changed = self.pointer_world != Some(world);
        self.pointer_world = Some(world);

        let mut result = ElectronicsInputResult {
            changed: false,
            request_redraw: false,
        };

        // Secondary click is a click-or-pan gesture in CAD. A short click
        // opens the context menu on release; a drag captures the secondary
        // button and pans the camera. Opening the menu on press used to leave
        // a stale interaction line behind and made right-drag impossible.
        if inside_canvas.is_some() && input.button_pressed(PointerButton::Secondary) {
            // A secondary gesture always takes navigation priority. If a
            // wire/outline preview was active, discard it before panning so
            // the preview cannot look like a stray black line during a
            // context-menu click or right-drag.
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
            let moved = Vec2::from(pointer).distance(secondary.start) >= 4.0;
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

        if input.key_pressed(InputKey::Escape) {
            self.cancel_gesture(router, owner);
            result.changed = true;
            result.request_redraw = true;
        }

        if input.key_pressed(InputKey::Z) && input.modifiers.command_modifier() {
            if self.undo() {
                result.changed = true;
                result.request_redraw = true;
            }
        } else if input.key_pressed(InputKey::Y) && input.modifiers.command_modifier() {
            if self.redo() {
                result.changed = true;
                result.request_redraw = true;
            }
        }

        if input.key_pressed(InputKey::Delete) {
            if self.delete_selected() {
                result.changed = true;
                result.request_redraw = true;
            }
        }
        if input.key_pressed(InputKey::R) && self.selection.is_some() {
            if self.rotate_selected() {
                result.changed = true;
                result.request_redraw = true;
            }
        }

        if input.scroll_delta[1].abs() > f32::EPSILON && !router.has_pointer_capture() {
            let factor = (1.0 + input.scroll_delta[1] * 0.1).clamp(0.55, 1.8);
            self.camera.zoom_at(factor, Vec2::from(pointer), size);
            self.touch();
            result.changed = true;
            result.request_redraw = true;
        }

        let space_pan = input.key_down(InputKey::Space);
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
                    self.begin_selection_or_drag(world, input, router, owner, &mut result);
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
                if let Some(drag) = self.component_drag.as_ref() {
                    let component_id = drag.component_id;
                    let target = self.snap_world(world + drag.pointer_offset);
                    let changed = if self.active_surface == CadSurfaceKind::Schematic {
                        if let Some(component) = self
                            .schematic
                            .components
                            .iter_mut()
                            .find(|component| component.id == component_id)
                        {
                            component.position = target;
                            self.schematic.sync_wire_anchors();
                            true
                        } else {
                            false
                        }
                    } else if let Some(component) = self
                        .pcb
                        .components
                        .iter_mut()
                        .find(|component| component.component_id == component_id)
                    {
                        component.position = target;
                        self.pcb.rebuild_airwires();
                        true
                    } else {
                        false
                    };
                    if changed {
                        self.rebuild_scene();
                        self.touch();
                        result.changed = true;
                        result.request_redraw = true;
                    }
                }
            }
            if input.button_released(PointerButton::Primary)
                || !input.button_down(PointerButton::Primary)
            {
                router.release_pointer(PointerButton::Primary, owner);
                if let Some(drag) = self.component_drag.take() {
                    let current = self.snapshot();
                    if self.history.record(drag.before, &current) {
                        self.dirty = true;
                    }
                    self.touch_ui();
                    result.changed = true;
                    result.request_redraw = true;
                }
            }
        }

        if self.active_surface == CadSurfaceKind::Schematic
            && self.tool == ElectronicsTool::Wire
            && self.wire_start.is_some()
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
        if let Some(hit) = pick(&self.scene, world, tolerance) {
            let kind = match hit.selection.kind {
                CadObjectKind::Component => ElectronicsSelectionKind::Component,
                CadObjectKind::Pin | CadObjectKind::Pad => ElectronicsSelectionKind::Pin,
                CadObjectKind::Wire => ElectronicsSelectionKind::Wire,
                CadObjectKind::Trace => ElectronicsSelectionKind::Trace,
                _ => ElectronicsSelectionKind::Other,
            };
            let source_id = hit.selection.source_id.unwrap_or_else(Uuid::nil);
            self.selection = Some(ElectronicsSelection { source_id, kind });
            self.interaction.selected = Some(hit.selection.clone());
            self.interaction.hovered = Some(hit.selection);
        } else {
            self.selection = None;
            self.interaction.clear_selection();
        }
    }

    pub fn save(&mut self, project: &Project) -> Result<(), String> {
        let registry = ProjectSessionRegistry::load_or_legacy(&project.path, project.project_type);
        let session = registry
            .active()
            .ok_or_else(|| "project has no active session".to_string())?;
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
        input: &InputSnapshot,
        router: &mut InputRouter,
        owner: InputOwner,
        result: &mut ElectronicsInputResult,
    ) {
        let tolerance = PICK_TOLERANCE_SCREEN / self.camera.zoom.max(MIN_ZOOM);
        let hit = pick(&self.scene, world, tolerance);
        let Some(hit) = hit else {
            self.selection = None;
            self.interaction.clear_selection();
            router.release_pointer(PointerButton::Primary, owner);
            self.touch_ui();
            result.changed = true;
            result.request_redraw = true;
            return;
        };

        let kind = match hit.selection.kind {
            CadObjectKind::Component => ElectronicsSelectionKind::Component,
            CadObjectKind::Pin | CadObjectKind::Pad => ElectronicsSelectionKind::Pin,
            CadObjectKind::Wire => ElectronicsSelectionKind::Wire,
            CadObjectKind::Trace => ElectronicsSelectionKind::Trace,
            _ => ElectronicsSelectionKind::Other,
        };
        let source_id = hit.selection.source_id.unwrap_or_else(Uuid::nil);
        self.selection = Some(ElectronicsSelection { source_id, kind });
        self.interaction.selected = Some(hit.selection.clone());
        self.interaction.hovered = Some(hit.selection);
        self.touch_ui();
        result.changed = true;
        result.request_redraw = true;

        if matches!(
            kind,
            ElectronicsSelectionKind::Component | ElectronicsSelectionKind::Pin
        ) {
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
            if let Some((position, locked)) = position_and_locked {
                if !locked {
                    self.component_drag = Some(ComponentDrag {
                        component_id: source_id,
                        pointer_offset: position - world,
                        before: self.snapshot(),
                    });
                } else {
                    router.release_pointer(PointerButton::Primary, owner);
                }
            }
        } else {
            router.release_pointer(PointerButton::Primary, owner);
        }

        if input.modifiers.control {
            // Multi-selection is a later surface concern; preserving the
            // modifier here prevents accidental camera movement while the
            // document controller is still single-selection based.
        }
    }

    fn handle_wire_click(
        &mut self,
        world: Vec2,
        router: &mut InputRouter,
        owner: InputOwner,
        result: &mut ElectronicsInputResult,
    ) {
        let endpoint = self.pin_endpoint_at(world);
        if let Some(start) = self.wire_start.take() {
            let end = endpoint.map(|(_, position, anchor)| (position, anchor));
            let (end_world, end_anchor) = end.unwrap_or((
                self.snap_world(world),
                WireAnchor::Point(self.snap_world(world)),
            ));
            if start.world.distance(end_world) > 0.5 {
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
                self.rebuild_scene();
                self.touch_ui();
                result.changed = true;
                result.request_redraw = true;
            }
        } else {
            let (point, anchor) = endpoint
                .map(|(_, point, anchor)| (point, Some(anchor)))
                .unwrap_or((
                    self.snap_world(world),
                    Some(WireAnchor::Point(self.snap_world(world))),
                ));
            self.wire_start = Some(WireStart {
                world: point,
                anchor,
            });
            self.touch_ui();
            result.changed = true;
            result.request_redraw = true;
        }
        router.release_pointer(PointerButton::Primary, owner);
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
        let tolerance = PICK_TOLERANCE_SCREEN / self.camera.zoom.max(MIN_ZOOM);
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
        let minimap = crate::electronics_minimap::overlay_rect(size);
        let world = local
            .filter(|point| {
                !(point[0] >= minimap.x
                    && point[0] <= minimap.x + minimap.width
                    && point[1] >= minimap.y
                    && point[1] <= minimap.y + minimap.height)
            })
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
        let minimap = crate::electronics_minimap::overlay_rect(size);
        let world = local
            .filter(|point| {
                !(point[0] >= minimap.x
                    && point[0] <= minimap.x + minimap.width
                    && point[1] >= minimap.y
                    && point[1] <= minimap.y + minimap.height)
            })
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
            component.rotation = (component.rotation + 90.0) % 360.0;
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
        component.rotation = (component.rotation + 90.0) % 360.0;
        self.schematic.sync_wire_anchors();
        self.history.record(before, &self.snapshot());
        self.dirty = true;
        self.rebuild_scene();
        self.touch_ui();
        true
    }

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
        self.rebuild_scene_internal();
        router.cancel_owner(owner);
        self.touch_ui();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
