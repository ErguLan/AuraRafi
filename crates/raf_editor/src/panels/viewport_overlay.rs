use super::*;

impl ViewportPanel {
    pub(super) fn draw_entity_labels(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        scene: &SceneGraph,
        view_proj: &Mat4,
        vp_w: f32,
        vp_h: f32,
        is_dark: bool,
    ) {
        let label_color = if is_dark {
            Color32::from_rgba_unmultiplied(220, 220, 230, 180)
        } else {
            Color32::from_rgba_unmultiplied(40, 40, 50, 180)
        };

        for (id, node) in scene.iter() {
            if !node.visible || node.name.is_empty() {
                continue;
            }

            let world = scene.world_matrix(id);
            let pos = world.col(3).truncate() + Vec3::new(0.0, 0.6, 0.0);
            if let Some((screen, _)) = raf_render::math::transform::project_point(pos, view_proj, vp_w, vp_h) {
                let sx = rect.left() + screen[0];
                let sy = rect.top() + screen[1];
                if rect.contains(Pos2::new(sx, sy)) {
                    let color = if self.selected.contains(&id) {
                        crate::theme::ACCENT
                    } else {
                        label_color
                    };

                    painter.text(
                        Pos2::new(sx, sy),
                        egui::Align2::CENTER_BOTTOM,
                        &node.name,
                        egui::FontId::proportional(10.0),
                        color,
                    );
                }
            }
        }
    }

    pub(super) fn draw_gizmo_overlay(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        entity_pos: Vec3,
        view_proj: &Mat4,
        vp_w: f32,
        vp_h: f32,
    ) {
        let gizmo = self.bridge.gizmo();
        if !gizmo.visible {
            return;
        }

        for arrow in &raf_render::picking::GIZMO_ARROWS {
            if let Some(screen_arrow) = raf_render::picking::project_gizmo_arrow(entity_pos, arrow, view_proj, vp_w, vp_h) {
                let offset = egui::vec2(rect.left(), rect.top());
                let start = Pos2::new(screen_arrow.start[0], screen_arrow.start[1]) + offset;
                let end = Pos2::new(screen_arrow.end[0], screen_arrow.end[1]) + offset;
                let [r, g, b, a] = screen_arrow.color;

                let is_active = matches!((&self.bridge.active_drag_axis(), screen_arrow.label),
                    (raf_render::gizmo::GizmoAxis::X, "X")
                    | (raf_render::gizmo::GizmoAxis::Y, "Y")
                    | (raf_render::gizmo::GizmoAxis::Z, "Z"));
                let color = Color32::from_rgba_unmultiplied(r, g, b, if is_active { 255 } else { a });
                painter.line_segment([start, end], Stroke::new(if is_active { 3.5 } else { 2.0 }, color));

                if matches!(gizmo.mode, GizmoMode::Translate) {
                    let tip = Pos2::new(screen_arrow.head_tip[0], screen_arrow.head_tip[1]) + offset;
                    let left = Pos2::new(screen_arrow.head_left[0], screen_arrow.head_left[1]) + offset;
                    let right = Pos2::new(screen_arrow.head_right[0], screen_arrow.head_right[1]) + offset;
                    painter.add(egui::Shape::convex_polygon(vec![tip, left, right], color, Stroke::NONE));
                }

                if matches!(gizmo.mode, GizmoMode::Scale) {
                    painter.rect_filled(
                        Rect::from_center_size(end, egui::vec2(8.0, 8.0)),
                        0.0,
                        color,
                    );
                }

                painter.text(
                    end + egui::vec2(6.0, -6.0),
                    egui::Align2::LEFT_BOTTOM,
                    screen_arrow.label,
                    egui::FontId::monospace(10.0),
                    color,
                );
            }
        }

        if matches!(gizmo.mode, GizmoMode::Rotate) {
            self.draw_rotation_rings(painter, rect, entity_pos, view_proj, vp_w, vp_h);
        }

        let mode_label = match gizmo.mode {
            GizmoMode::Translate => "Move (G)",
            GizmoMode::Rotate => "Rotate (R)",
            GizmoMode::Scale => "Scale (S)",
        };
        painter.text(
            Pos2::new(rect.right() - 8.0, rect.top() + 8.0),
            egui::Align2::RIGHT_TOP,
            mode_label,
            egui::FontId::proportional(10.0),
            Color32::from_rgba_unmultiplied(180, 180, 190, 160),
        );
    }

    fn draw_rotation_rings(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        entity_pos: Vec3,
        view_proj: &Mat4,
        vp_w: f32,
        vp_h: f32,
    ) {
        let offset = egui::vec2(rect.left(), rect.top());
        let colors = [
            Color32::from_rgb(220, 70, 70),
            Color32::from_rgb(70, 200, 70),
            Color32::from_rgb(70, 100, 220),
        ];
        let axis_planes = [(Vec3::Y, Vec3::Z), (Vec3::X, Vec3::Z), (Vec3::X, Vec3::Y)];
        let radius = raf_render::picking::GIZMO_ROTATION_RADIUS;

        for (axis_idx, (axis_a, axis_b)) in axis_planes.iter().enumerate() {
            let mut previous = None;
            for step in 0..=48 {
                let angle = (step as f32 / 48.0) * std::f32::consts::TAU;
                let world_pt = entity_pos
                    + *axis_a * (angle.cos() * radius)
                    + *axis_b * (angle.sin() * radius);

                if let Some((screen, _)) = raf_render::math::transform::project_point(world_pt, view_proj, vp_w, vp_h) {
                    let current = Pos2::new(screen[0], screen[1]) + offset;
                    if let Some(prev) = previous {
                        painter.line_segment([prev, current], Stroke::new(1.5, colors[axis_idx]));
                    }
                    previous = Some(current);
                }
            }
        }
    }

    pub(super) fn draw_edit_overlay(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        scene: &SceneGraph,
        view_proj: &Mat4,
        vp_w: f32,
        vp_h: f32,
    ) {
        let Some(overlay) = self.bridge.project_edit_overlay(
            scene,
            self.selected.first().copied(),
            view_proj,
            vp_w,
            vp_h,
        ) else { return; };

        for edge in overlay.edges {
            painter.line_segment(
                [
                    Pos2::new(rect.left() + edge.start[0], rect.top() + edge.start[1]),
                    Pos2::new(rect.left() + edge.end[0], rect.top() + edge.end[1]),
                ],
                Stroke::new(1.4, Color32::from_rgba_unmultiplied(255, 180, 90, 220)),
            );
        }

        for vertex in overlay.vertices {
            let pos = Pos2::new(rect.left() + vertex.position[0], rect.top() + vertex.position[1]);
            painter.circle_filled(
                pos,
                if vertex.selected { 5.0 } else { 3.5 },
                if vertex.selected { Color32::WHITE } else { Color32::from_rgb(255, 160, 40) },
            );
            painter.circle_stroke(
                pos,
                if vertex.selected { 5.0 } else { 3.5 },
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(20, 20, 24, 220)),
            );
        }

        painter.text(
            Pos2::new(rect.right() - 8.0, rect.top() + 24.0),
            egui::Align2::RIGHT_TOP,
            "Vertex Edit (Tab)",
            egui::FontId::proportional(10.0),
            Color32::from_rgba_unmultiplied(255, 180, 90, 180),
        );
    }
}