use super::*;
use raf_render::api_graphic_basic::grid::build_2d_grid_points;

impl ViewportPanel {
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
            self.bridge.offset_2d(),
            self.bridge.zoom_2d(),
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
