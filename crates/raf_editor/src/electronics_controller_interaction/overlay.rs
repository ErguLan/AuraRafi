//! Library drag placement and its canvas overlay preview.

use super::*;

use super::pointer::rect_contains;

impl NativeElectronicsEditor {
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
        let minimap_panel = self.minimap_rects(size).map(|rects| rects.panel);
        let world = local
            .filter(|point| minimap_panel.map_or(true, |panel| !rect_contains(panel, *point)))
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

    pub(super) fn process_library_drag_input(
        &mut self,
        input: &InputSnapshot,
        canvas: EditorRect,
    ) -> ElectronicsInputResult {
        let size = Vec2::new(canvas.width.max(1.0), canvas.height.max(1.0));
        let pointer = input.pointer_position;
        let local = pointer.and_then(|point| canvas.local_point(point));
        let minimap_panel = self.minimap_rects(size).map(|rects| rects.panel);
        let world = local
            .filter(|point| minimap_panel.map_or(true, |panel| !rect_contains(panel, *point)))
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
}
