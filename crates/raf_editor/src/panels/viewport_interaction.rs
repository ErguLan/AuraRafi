use super::*;

impl ViewportPanel {
    pub(super) fn apply_object_shortcuts(&mut self, ctx: &egui::Context, scene: &SceneGraph) {
        if self.edit_mode == EditMode::Vertex {
            return;
        }

        // Block shortcuts when user is typing in a text field.
        if ctx.wants_keyboard_input() {
            return;
        }

        let input = ctx.input(|i| {
            let ctrl = i.modifiers.ctrl || i.modifiers.mac_cmd;
            (
                ctrl,
                i.key_pressed(egui::Key::G),
                i.key_pressed(egui::Key::R),
                i.key_pressed(egui::Key::T),
                i.key_pressed(egui::Key::F),
                i.key_pressed(egui::Key::C),
            )
        });

        if input.0 {
            return; // Ctrl held: block all shortcuts
        }
        if input.1 || input.2 || input.3 {
            self.select_mode = false;
            self.bridge.gizmo_mut().visible = true;
        }
        if input.1 {
            self.bridge.set_gizmo_mode(GizmoMode::Translate);
        }
        if input.2 {
            self.bridge.set_gizmo_mode(GizmoMode::Rotate);
        }
        if input.3 {
            self.bridge.set_gizmo_mode(GizmoMode::Scale);
        }
        if input.4 {
            if self.focus_lock_enabled {
                self.set_focus_lock(scene, !self.focus_locked);
            } else {
                // A disabled lock must never turn into a hidden second press.
                // F only frames the selected object and immediately returns to
                // normal camera navigation.
                self.focus_selected_entity(scene, self.selected.first().copied());
            }
        }
        if input.5 {
            self.select_mode = !self.select_mode;
            self.bridge.gizmo_mut().visible = !self.select_mode;
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
                    } else {
                        // Free-drag fallback for multi-select: store all start positions.
                        self.group_free_drag_starts.clear();
                        for &gid in &self.selected {
                            if scene.get(gid).is_some() {
                                let gworld = scene.world_matrix(gid);
                                let gcenter = gworld.col(3).truncate();
                                self.group_free_drag_starts.push((gid, gcenter));
                            }
                        }
                        if !self.group_free_drag_starts.is_empty() {
                            self.drag_ongoing = true;
                            changed = true;
                        }
                    }
                }
                return changed;
            }

            if let Some(pos) = response.interact_pointer_pos() {
                if self.overlay_blocks_world_input(rect, pos) {
                    return false;
                }
                let local = [pos.x - rect.left(), pos.y - rect.top()];
                let presentation_scale = self.gizmo_presentation_scale();
                self.bridge.begin_transform_drag_scaled(
                    scene,
                    self.selected.first().copied(),
                    view_proj,
                    local,
                    vp_w,
                    vp_h,
                    presentation_scale,
                );

                // If gizmo was not hit, try free drag on the entity.
                if self.bridge.active_drag_axis() == GizmoAxis::None {
                    let picked = self
                        .bridge
                        .pick_entity(scene, view_proj, local[0], local[1], vp_w, vp_h);
                    if let Some(id) = picked {
                        if scene.get(id).is_some() {
                            let world = scene.world_matrix(id);
                            let center = world.col(3).truncate();
                            self.free_drag_active = true;
                            self.free_drag_start_pos = center;
                            self.free_drag_start_mouse = local;
                            if !self.selected.contains(&id) {
                                self.selected = vec![id];
                            }
                        }
                    }
                }
            }
            self.drag_ongoing = true;
            changed = true;
        }

        if multi_selection && response.dragged_by(egui::PointerButton::Primary) {
            if self.group_drag_axis == GizmoAxis::None && !self.group_free_drag_starts.is_empty() {
                // Free-drag for multi-select: compute delta in world horizontal plane.
                if let Some(pos) = response.interact_pointer_pos() {
                    let local = [pos.x - rect.left(), pos.y - rect.top()];
                    let vp_inv = view_proj.inverse();
                    if let Some((ray_orig, ray_dir)) =
                        raf_render::math::transform::screen_to_world_ray(
                            self.free_drag_start_mouse[0],
                            self.free_drag_start_mouse[1],
                            vp_w,
                            vp_h,
                            &vp_inv,
                        )
                    {
                        if let Some((cur_orig, cur_dir)) =
                            raf_render::math::transform::screen_to_world_ray(
                                local[0], local[1], vp_w, vp_h, &vp_inv,
                            )
                        {
                            let t1 = raf_render::math::ray::ray_plane(
                                &raf_render::math::ray::Ray::new(ray_orig, ray_dir),
                                self.free_drag_start_pos,
                                Vec3::Y,
                            )
                            .unwrap_or(0.0);
                            let t2 = raf_render::math::ray::ray_plane(
                                &raf_render::math::ray::Ray::new(cur_orig, cur_dir),
                                self.free_drag_start_pos,
                                Vec3::Y,
                            )
                            .unwrap_or(0.0);
                            let hit1 = ray_orig + ray_dir * t1;
                            let hit2 = cur_orig + cur_dir * t2;
                            let delta = hit2 - hit1;
                            for (gid, start_gpos) in &self.group_free_drag_starts {
                                if let Some(gnode) = scene.get_mut(*gid) {
                                    gnode.position = *start_gpos + delta;
                                }
                            }
                            changed = true;
                        }
                    }
                }
            } else if self.group_drag_axis != GizmoAxis::None {
                if let Some(pos) = response.interact_pointer_pos() {
                    let local = [pos.x - rect.left(), pos.y - rect.top()];
                    let snap = response
                        .ctx
                        .input(|i| i.modifiers.ctrl || i.modifiers.mac_cmd);
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
        } else if self.free_drag_active {
            if let Some(id) = self.selected.first().copied() {
                if scene.get(id).is_some() {
                    if let (Some(pos), Some(_world)) =
                        (response.interact_pointer_pos(), selected_world_pos)
                    {
                        let local = [pos.x - rect.left(), pos.y - rect.top()];
                        let vp_inv = view_proj.inverse();
                        if let Some((ray_orig, ray_dir)) =
                            raf_render::math::transform::screen_to_world_ray(
                                self.free_drag_start_mouse[0],
                                self.free_drag_start_mouse[1],
                                vp_w,
                                vp_h,
                                &vp_inv,
                            )
                        {
                            let t = raf_render::math::ray::ray_plane(
                                &raf_render::math::ray::Ray::new(ray_orig, ray_dir),
                                self.free_drag_start_pos,
                                Vec3::Y,
                            )
                            .unwrap_or(0.0);
                            let hit_world = ray_orig + ray_dir * t;
                            if let Some((cur_orig, cur_dir)) =
                                raf_render::math::transform::screen_to_world_ray(
                                    local[0], local[1], vp_w, vp_h, &vp_inv,
                                )
                            {
                                let cur_t = raf_render::math::ray::ray_plane(
                                    &raf_render::math::ray::Ray::new(cur_orig, cur_dir),
                                    self.free_drag_start_pos,
                                    Vec3::Y,
                                )
                                .unwrap_or(0.0);
                                let cur_hit = cur_orig + cur_dir * cur_t;
                                let delta = cur_hit - hit_world;
                                if let Some(node) = scene.get_mut(id) {
                                    node.position = self.free_drag_start_pos + delta;
                                    changed = true;
                                }
                            }
                        }
                    }
                }
            }
        } else if response.dragged_by(egui::PointerButton::Primary)
            && self.bridge.active_drag_axis() != raf_render::gizmo::GizmoAxis::None
        {
            if let Some(pos) = response.interact_pointer_pos() {
                let local = [pos.x - rect.left(), pos.y - rect.top()];
                let snap = response
                    .ctx
                    .input(|i| i.modifiers.ctrl || i.modifiers.mac_cmd);
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
            self.free_drag_active = false;
            self.group_free_drag_starts.clear();
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
