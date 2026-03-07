//! Viewport panel - 3D/2D scene view with grid, entity visualization, and tools.
//!
//! Renders a visual representation of scene entities on a 2D canvas with
//! grid overlay, origin marker, zoom/pan controls, and entity gizmos.

use egui::{Color32, Pos2, Rect, Stroke, Ui, Vec2};

use crate::theme;

/// State for the viewport panel.
pub struct ViewportPanel {
    /// Whether to show the grid.
    pub grid_visible: bool,
    /// Grid cell size in pixels.
    pub grid_size: f32,
    /// Camera zoom level for 2D pan/zoom.
    pub zoom: f32,
    /// Camera offset for panning.
    pub offset: [f32; 2],
    /// Current tool mode.
    pub tool: ViewportTool,
}

/// Currently active viewport tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewportTool {
    Select,
    Move,
    Rotate,
    Scale,
}

impl Default for ViewportPanel {
    fn default() -> Self {
        Self {
            grid_visible: true,
            grid_size: 40.0,
            zoom: 1.0,
            offset: [0.0, 0.0],
            tool: ViewportTool::Select,
        }
    }
}

impl ViewportPanel {
    /// Draw the viewport panel content.
    pub fn show(&mut self, ui: &mut Ui, is_dark: bool) {
        let available = ui.available_rect_before_wrap();
        let painter = ui.painter_at(available);

        // Background fill with subtle gradient feel.
        let bg = if is_dark {
            Color32::from_rgb(16, 16, 22)
        } else {
            Color32::from_rgb(238, 238, 242)
        };
        painter.rect_filled(available, 0.0, bg);

        // Draw grid if enabled.
        if self.grid_visible {
            self.draw_grid(&painter, available, is_dark);
        }

        // Draw axis lines through origin.
        let origin = self.world_to_screen(Pos2::ZERO, available);
        if available.contains(origin) || true {
            // X axis (subtle red).
            let x_color = Color32::from_rgba_premultiplied(200, 60, 60, 50);
            painter.line_segment(
                [
                    Pos2::new(available.left(), origin.y),
                    Pos2::new(available.right(), origin.y),
                ],
                Stroke::new(1.0, x_color),
            );

            // Y axis (subtle green).
            let y_color = Color32::from_rgba_premultiplied(60, 200, 60, 50);
            painter.line_segment(
                [
                    Pos2::new(origin.x, available.top()),
                    Pos2::new(origin.x, available.bottom()),
                ],
                Stroke::new(1.0, y_color),
            );

            // Origin crosshair.
            let cross_color = if is_dark {
                Color32::from_rgba_premultiplied(212, 119, 26, 100)
            } else {
                Color32::from_rgba_premultiplied(212, 119, 26, 140)
            };
            let cross_size = 14.0;
            painter.line_segment(
                [
                    Pos2::new(origin.x - cross_size, origin.y),
                    Pos2::new(origin.x + cross_size, origin.y),
                ],
                Stroke::new(2.0, cross_color),
            );
            painter.line_segment(
                [
                    Pos2::new(origin.x, origin.y - cross_size),
                    Pos2::new(origin.x, origin.y + cross_size),
                ],
                Stroke::new(2.0, cross_color),
            );

            // Origin dot.
            painter.circle_filled(origin, 3.0, cross_color);
        }

        // Toolbar overlay (top-left).
        self.draw_toolbar(ui, &painter, available, is_dark);

        // Info overlay (top-right).
        let info_color = if is_dark {
            theme::DARK_TEXT_DIM
        } else {
            theme::LIGHT_TEXT_DIM
        };
        let info_text = format!(
            "Zoom: {:.1}x | Offset: ({:.0}, {:.0})",
            self.zoom, self.offset[0], self.offset[1]
        );
        painter.text(
            Pos2::new(available.right() - 10.0, available.top() + 10.0),
            egui::Align2::RIGHT_TOP,
            info_text,
            egui::FontId::proportional(10.0),
            info_color,
        );

        // Viewport label.
        painter.text(
            Pos2::new(available.left() + 10.0, available.bottom() - 10.0),
            egui::Align2::LEFT_BOTTOM,
            "Scene Viewport",
            egui::FontId::proportional(10.0),
            Color32::from_rgba_premultiplied(120, 120, 130, 80),
        );

        // Handle mouse interaction for panning.
        let response = ui.allocate_rect(available, egui::Sense::click_and_drag());
        if response.dragged_by(egui::PointerButton::Middle) {
            self.offset[0] += response.drag_delta().x;
            self.offset[1] += response.drag_delta().y;
        }
        // Right-drag also pans.
        if response.dragged_by(egui::PointerButton::Secondary) {
            self.offset[0] += response.drag_delta().x;
            self.offset[1] += response.drag_delta().y;
        }

        // Scroll to zoom.
        if ui.rect_contains_pointer(available) {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.zoom = (self.zoom + scroll * 0.002).clamp(0.1, 10.0);
            }
        }

        // Keyboard shortcuts for tools.
        ui.input(|i| {
            if i.key_pressed(egui::Key::Q) {
                self.tool = ViewportTool::Select;
            }
            if i.key_pressed(egui::Key::W) {
                self.tool = ViewportTool::Move;
            }
            if i.key_pressed(egui::Key::E) {
                self.tool = ViewportTool::Rotate;
            }
            if i.key_pressed(egui::Key::R) {
                self.tool = ViewportTool::Scale;
            }
        });

        // Double-click to reset view.
        if response.double_clicked() {
            self.zoom = 1.0;
            self.offset = [0.0, 0.0];
        }
    }

    fn draw_toolbar(
        &mut self,
        ui: &mut Ui,
        painter: &egui::Painter,
        rect: Rect,
        is_dark: bool,
    ) {
        let tools = [
            (ViewportTool::Select, "Sel (Q)"),
            (ViewportTool::Move, "Mov (W)"),
            (ViewportTool::Rotate, "Rot (E)"),
            (ViewportTool::Scale, "Scl (R)"),
        ];

        let toolbar_x = rect.left() + 10.0;
        let mut toolbar_y = rect.top() + 10.0;

        for (tool, label) in tools {
            let btn_rect = Rect::from_min_size(
                Pos2::new(toolbar_x, toolbar_y),
                Vec2::new(64.0, 22.0),
            );

            let resp = ui.allocate_rect(btn_rect, egui::Sense::click());
            let is_active = self.tool == tool;

            let bg = if is_active {
                Color32::from_rgba_premultiplied(212, 119, 26, 180)
            } else if resp.hovered() {
                if is_dark {
                    Color32::from_rgba_premultiplied(60, 60, 68, 200)
                } else {
                    Color32::from_rgba_premultiplied(180, 180, 190, 200)
                }
            } else if is_dark {
                Color32::from_rgba_premultiplied(30, 30, 38, 180)
            } else {
                Color32::from_rgba_premultiplied(220, 220, 228, 180)
            };

            painter.rect_filled(btn_rect, 4.0, bg);

            let text_color = if is_active {
                Color32::WHITE
            } else if is_dark {
                Color32::from_rgb(180, 180, 190)
            } else {
                Color32::from_rgb(60, 60, 70)
            };

            painter.text(
                btn_rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(10.0),
                text_color,
            );

            if resp.clicked() {
                self.tool = tool;
            }

            toolbar_y += 26.0;
        }
    }

    fn draw_grid(&self, painter: &egui::Painter, rect: Rect, is_dark: bool) {
        let grid_color = if is_dark {
            Color32::from_rgba_premultiplied(255, 255, 255, 10)
        } else {
            Color32::from_rgba_premultiplied(0, 0, 0, 12)
        };
        let grid_major = if is_dark {
            Color32::from_rgba_premultiplied(255, 255, 255, 22)
        } else {
            Color32::from_rgba_premultiplied(0, 0, 0, 25)
        };
        let step = self.grid_size * self.zoom;
        if step < 4.0 {
            return; // Too zoomed out.
        }

        let mut x = rect.left() + (self.offset[0] % step);
        let mut ix = 0u32;
        while x < rect.right() {
            let c = if ix % 5 == 0 { grid_major } else { grid_color };
            painter.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(0.5, c),
            );
            x += step;
            ix += 1;
        }

        let mut y = rect.top() + (self.offset[1] % step);
        let mut iy = 0u32;
        while y < rect.bottom() {
            let c = if iy % 5 == 0 { grid_major } else { grid_color };
            painter.line_segment(
                [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                Stroke::new(0.5, c),
            );
            y += step;
            iy += 1;
        }
    }

    /// Convert world coordinates to screen coordinates.
    fn world_to_screen(&self, world: Pos2, rect: Rect) -> Pos2 {
        Pos2::new(
            rect.center().x + world.x * self.grid_size * self.zoom + self.offset[0],
            rect.center().y - world.y * self.grid_size * self.zoom + self.offset[1],
        )
    }
}
