use super::*;
use raf_render::api_graphic_basic::grid::{build_2d_grid_points, build_3d_grid, GridLineKind};

impl ViewportPanel {
    pub(super) fn draw_3d_grid(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        scene: &SceneGraph,
        view_proj: &Mat4,
        is_dark: bool,
    ) {
        if !self.grid_visible {
            return;
        }

        let vp_w = rect.width();
        let vp_h = rect.height();
        let grid_color = if is_dark {
            Color32::from_rgba_premultiplied(255, 255, 255, 18)
        } else {
            Color32::from_rgba_premultiplied(0, 0, 0, 18)
        };
        let grid_major = if is_dark {
            Color32::from_rgba_premultiplied(255, 255, 255, 35)
        } else {
            Color32::from_rgba_premultiplied(0, 0, 0, 35)
        };

        let axis_color = if is_dark {
            Color32::from_rgba_premultiplied(theme::ACCENT.r(), theme::ACCENT.g(), theme::ACCENT.b(), 90)
        } else {
            Color32::from_rgba_premultiplied(theme::ACCENT.r(), theme::ACCENT.g(), theme::ACCENT.b(), 70)
        };

        let (bounds_min, bounds_max) = scene_grid_bounds(scene);
        for line in build_3d_grid(bounds_min, bounds_max, self.grid_spacing) {
            let color = match line.kind {
                GridLineKind::Axis => axis_color,
                GridLineKind::Major => grid_major,
                GridLineKind::Minor => grid_color,
            };

            if let Some(edge) = projection::project_edge(&[line.start, line.end], view_proj, vp_w, vp_h) {
                painter.line_segment(
                    [
                        Pos2::new(rect.left() + edge[0][0], rect.top() + edge[0][1]),
                        Pos2::new(rect.left() + edge[1][0], rect.top() + edge[1][1]),
                    ],
                    Stroke::new(line.width, color),
                );
            }
        }
    }

    pub(super) fn draw_2d_grid(&self, painter: &egui::Painter, rect: Rect, is_dark: bool) {
        if !self.grid_visible {
            return;
        }

        let dot_color = if is_dark {
            Color32::from_rgba_premultiplied(255, 255, 255, 25)
        } else {
            Color32::from_rgba_premultiplied(0, 0, 0, 25)
        };

        for point in build_2d_grid_points(
            rect.width(),
            rect.height(),
            self.offset_2d,
            self.zoom_2d,
            self.grid_spacing,
        ) {
            painter.circle_filled(
                Pos2::new(rect.left() + point[0], rect.top() + point[1]),
                1.0,
                dot_color,
            );
        }
    }
}

fn scene_grid_bounds(scene: &SceneGraph) -> (Vec3, Vec3) {
    let mut min = Vec3::splat(-1.0);
    let mut max = Vec3::splat(1.0);
    let mut has_visible = false;

    for (_, node) in scene.iter() {
        if !node.visible || node.primitive == Primitive::Empty {
            continue;
        }
        has_visible = true;
        let half = Vec3::new(
            node.scale.x.abs().max(0.5) * 0.5,
            node.scale.y.abs().max(0.5) * 0.5,
            node.scale.z.abs().max(0.5) * 0.5,
        );
        let node_min = node.position - half;
        let node_max = node.position + half;
        min = min.min(node_min);
        max = max.max(node_max);
    }

    if has_visible {
        (min, max)
    } else {
        (Vec3::new(-2.0, 0.0, -2.0), Vec3::new(2.0, 0.0, 2.0))
    }
}