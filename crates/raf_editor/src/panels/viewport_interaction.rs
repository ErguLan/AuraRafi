use super::*;

impl ViewportPanel {
    pub(super) fn apply_object_shortcuts(&mut self, ctx: &egui::Context, scene: &SceneGraph) {
        if self.edit_mode == EditMode::Vertex {
            return;
        }

        let input = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::G),
                i.key_pressed(egui::Key::R),
                i.key_pressed(egui::Key::S),
                i.key_pressed(egui::Key::F),
            )
        });

        if input.0 {
            self.bridge.set_gizmo_mode(GizmoMode::Translate);
        }
        if input.1 {
            self.bridge.set_gizmo_mode(GizmoMode::Rotate);
        }
        if input.2 {
            self.bridge.set_gizmo_mode(GizmoMode::Scale);
        }
        if input.3 {
            self.bridge.focus_selected(
                scene,
                self.selected.first().copied(),
                self.mode == ViewportMode::View2D,
            );
        }
    }

    pub(super) fn handle_object_mode_input(
        &mut self,
        response: &egui::Response,
        scene: &mut SceneGraph,
        view_proj: &Mat4,
        rect: Rect,
        vp_w: f32,
        vp_h: f32,
    ) -> bool {
        let multi_selection = self.selected.len() > 1;
        let selected_world_pos = self.selected.first().and_then(|&id| {
            scene
                .get(id)
                .map(|_| scene.world_matrix(id).col(3).truncate())
        });

        let mut changed = false;

        if response.drag_started_by(egui::PointerButton::Primary) {
            if multi_selection {
                if let Some(pos) = response.interact_pointer_pos() {
                    if self.overlay_blocks_world_input(rect, pos) {
                        return false;
                    }
                    let local = [pos.x - rect.left(), pos.y - rect.top()];
                    self.begin_group_transform_drag(scene, view_proj, local, vp_w, vp_h);
                    if self.group_drag_axis != GizmoAxis::None {
                        self.drag_ongoing = true;
                        changed = true;
                    }
                }
                return changed;
            }

            if let (Some(pos), Some(_entity_pos)) =
                (response.interact_pointer_pos(), selected_world_pos)
            {
                if self.overlay_blocks_world_input(rect, pos) {
                    return false;
                }
                let local = [pos.x - rect.left(), pos.y - rect.top()];
                self.bridge.begin_transform_drag(
                    scene,
                    self.selected.first().copied(),
                    view_proj,
                    local,
                    vp_w,
                    vp_h,
                );
            }
            self.drag_ongoing = true;
            changed = true;
        }

        if multi_selection && response.dragged_by(egui::PointerButton::Primary) {
            if self.group_drag_axis != GizmoAxis::None {
                if let Some(pos) = response.interact_pointer_pos() {
                    let local = [pos.x - rect.left(), pos.y - rect.top()];
                    let snap = response.ctx.input(|i| i.modifiers.ctrl || i.modifiers.mac_cmd);
                    self.apply_group_transform_drag(
                        scene,
                        view_proj,
                        local,
                        self.uniform_scale_by_default,
                        snap,
                        vp_w,
                        vp_h,
                    );
                }
            }
        } else if response.dragged_by(egui::PointerButton::Primary)
            && self.bridge.active_drag_axis() != raf_render::gizmo::GizmoAxis::None
        {
            if let Some(pos) = response.interact_pointer_pos() {
                let local = [pos.x - rect.left(), pos.y - rect.top()];
                let snap = response.ctx.input(|i| i.modifiers.ctrl || i.modifiers.mac_cmd);
                if self.drag_ongoing {
                    self.bridge.apply_transform_drag(
                        scene,
                        self.selected.first().copied(),
                        view_proj,
                        local,
                        self.uniform_scale_by_default,
                        snap,
                        vp_w,
                        vp_h,
                    );
                } else {
                    changed |= self.bridge.apply_transform_drag(
                        scene,
                        self.selected.first().copied(),
                        view_proj,
                        local,
                        self.uniform_scale_by_default,
                        snap,
                        vp_w,
                        vp_h,
                    );
                }
            }
        }

        if response.drag_stopped_by(egui::PointerButton::Primary) {
            if multi_selection && self.group_drag_axis != GizmoAxis::None {
                self.clear_group_transform_state();
                self.drag_ongoing = false;
            } else {
                self.bridge.end_transform_drag();
                self.drag_ongoing = false;
            }
        }

        if response.clicked()
            && self.bridge.active_drag_axis() == raf_render::gizmo::GizmoAxis::None
        {
            if let Some(pos) = response.interact_pointer_pos() {
                if self.overlay_blocks_world_input(rect, pos) {
                    return false;
                }
                let local_x = pos.x - rect.left();
                let local_y = pos.y - rect.top();
                let picked = self
                    .bridge
                    .pick_entity(scene, view_proj, local_x, local_y, vp_w, vp_h);
                let add_to_selection = response.ctx.input(|i| i.modifiers.shift);

                if let Some(id) = picked {
                    if add_to_selection {
                        if let Some(existing) =
                            self.selected.iter().position(|selected| *selected == id)
                        {
                            self.selected.remove(existing);
                        } else {
                            self.selected.push(id);
                        }
                    } else {
                        self.selected = vec![id];
                    }
                } else if !add_to_selection {
                    self.selected.clear();
                }

                if self.selected.len() < 2 {
                    self.clear_group_transform_state();
                }

                changed = true;
            }
        }

        changed
    }

    pub(super) fn toggle_edit_mode(&mut self, scene: &SceneGraph) {
        self.edit_mode = match self.edit_mode {
            EditMode::Object => EditMode::Vertex,
            EditMode::Vertex => EditMode::Object,
        };

        self.bridge.clear_edit_drag_state();

        if self.edit_mode == EditMode::Vertex {
            self.bridge
                .prepare_selected_edit_mesh(scene, self.selected.first().copied());
        }
    }

    pub(super) fn handle_edit_mode_input(
        &mut self,
        response: &egui::Response,
        scene: &mut SceneGraph,
        view_proj: &Mat4,
        rect: Rect,
        vp_w: f32,
        vp_h: f32,
    ) -> bool {
        let mut changed = false;
        let selected = self.selected.first().copied();

        if response.clicked_by(egui::PointerButton::Primary) {
            if let Some(pointer) = response.interact_pointer_pos() {
                if self.overlay_blocks_world_input(rect, pointer) {
                    return false;
                }
                let local = [pointer.x - rect.left(), pointer.y - rect.top()];
                let shift = response.ctx.input(|i| i.modifiers.shift);
                changed |= self.bridge.handle_edit_selection_click(
                    scene, selected, view_proj, vp_w, vp_h, local, shift,
                );
            }
        }

        if response.drag_started_by(egui::PointerButton::Primary) {
            if let Some(pointer) = response.interact_pointer_pos() {
                if self.overlay_blocks_world_input(rect, pointer) {
                    return changed;
                }
                let local = [pointer.x - rect.left(), pointer.y - rect.top()];
                self.bridge
                    .begin_edit_drag(scene, selected, view_proj, vp_w, vp_h, local);
            }
        }

        if response.dragged_by(egui::PointerButton::Primary) {
            if let Some(pointer) = response.interact_pointer_pos() {
                let current = [pointer.x - rect.left(), pointer.y - rect.top()];
                changed |= self.bridge.drag_selected_vertices(
                    scene,
                    selected,
                    self.move_sensitivity,
                    current,
                );
            }
        }

        if response.drag_stopped_by(egui::PointerButton::Primary) {
            self.bridge.clear_edit_drag_state();
        }

        changed
    }
}
