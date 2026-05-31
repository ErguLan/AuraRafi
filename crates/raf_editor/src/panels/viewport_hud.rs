use super::*;
use crate::theme;

const HUD_BUTTON_SIZE: f32 = 26.0;
const HUD_BUTTON_GAP: f32 = 3.0;
const HUD_PADDING: f32 = 8.0;
const HUD_TOGGLE_BUTTON_WIDTH: f32 = 28.0;
const HUD_TOGGLE_BUTTON_HEIGHT: f32 = 20.0;
const HUD_TOGGLE_BUTTON_GAP: f32 = 4.0;

#[derive(Clone, Copy)]
enum HudAction {
    SetGizmo(GizmoMode),
    Focus,
    ToggleGrid,
    ToggleLabels,
    SnapAxis(Vec3),
    ResetView,
}

#[derive(Clone, Copy)]
struct AxisGizmoEndpoint {
    end: Pos2,
    axis: Vec3,
    color: Color32,
    label: &'static str,
}

impl ViewportPanel {
    pub(super) fn draw_hud(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        is_dark: bool,
        icons: &UiIconAtlas,
    ) {
        self.draw_toolbar(painter, rect, is_dark, icons);
        self.draw_mode_toggle(painter, rect, is_dark);
        self.draw_info_overlay(painter, rect, is_dark);
        self.draw_visual_toggles(painter, rect, is_dark);

        if self.mode == ViewportMode::View3D {
            self.draw_axis_gizmo(painter, rect, is_dark);
        }
    }

    pub(super) fn handle_hud_click(
        &mut self,
        response: &egui::Response,
        rect: Rect,
        scene: &SceneGraph,
    ) -> bool {
        if !response.clicked() {
            return false;
        }

        let Some(pos) = response.interact_pointer_pos() else {
            return false;
        };

        if let Some(action) = self.toolbar_action_at(rect, pos) {
            match action {
                HudAction::SetGizmo(mode) => self.bridge.set_gizmo_mode(mode),
                HudAction::Focus => self
                    .bridge
                    .focus_selected(scene, self.selected.first().copied(), self.mode == ViewportMode::View2D),
                _ => {}
            }
            return true;
        }

        if let Some(mode) = self.mode_toggle_at(rect, pos) {
            self.mode = mode;
            return true;
        }

        if self.edit_mode_badge_rect(rect).contains(pos) {
            self.toggle_edit_mode(scene);
            return true;
        }

        if let Some(action) = self.toggle_action_at(rect, pos) {
            match action {
                HudAction::ToggleGrid => self.grid_visible = !self.grid_visible,
                HudAction::ToggleLabels => self.show_labels = !self.show_labels,
                _ => {}
            }
            return true;
        }

        if self.mode == ViewportMode::View3D {
            if let Some(action) = self.axis_gizmo_action_at(rect, pos) {
                match action {
                    HudAction::SnapAxis(axis) => self.bridge.snap_view_to_axis(axis),
                    HudAction::ResetView => self.bridge.reset_isometric_view(),
                    _ => {}
                }
                return true;
            }
        }

        false
    }

    pub(super) fn overlay_blocks_world_input(&self, rect: Rect, pos: Pos2) -> bool {
        self.toolbar_group_rect(rect).contains(pos)
            || self.mode_toggle_group_rect(rect).contains(pos)
            || self.edit_mode_badge_rect(rect).contains(pos)
            || self.info_rect(rect).contains(pos)
            || self.toggle_group_rect(rect).contains(pos)
            || (self.mode == ViewportMode::View3D && self.axis_gizmo_rect(rect).contains(pos))
    }

    fn draw_toolbar(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        is_dark: bool,
        icons: &UiIconAtlas,
    ) {
        let group_rect = self.toolbar_group_rect(rect);
        painter.rect_filled(group_rect, 6.0, hud_group_fill(is_dark));
        painter.rect_stroke(group_rect, 6.0, Stroke::new(0.5, hud_group_border(is_dark)));

        let buttons = [
            (HudAction::SetGizmo(GizmoMode::Translate), Some("move.png"), "G"),
            (HudAction::SetGizmo(GizmoMode::Rotate), Some("rotate.png"), "R"),
            (HudAction::SetGizmo(GizmoMode::Scale), None, "S"),
            (HudAction::Focus, Some("focus.png"), "F"),
        ];

        for (index, (action, icon_name, fallback)) in buttons.iter().enumerate() {
            let button_rect = self.toolbar_button_rect(rect, index);
            let is_active = matches!(action, HudAction::SetGizmo(mode) if *mode == self.bridge.gizmo().mode);

            if is_active {
                let active_bg = if is_dark {
                    Color32::from_rgba_premultiplied(55, 55, 60, 220)
                } else {
                    Color32::from_rgba_premultiplied(200, 200, 205, 220)
                };
                painter.rect_filled(button_rect, 4.0, active_bg);
                painter.circle_filled(
                    Pos2::new(button_rect.center().x, button_rect.bottom() - 1.5),
                    2.0,
                    theme::ACCENT,
                );
            }

            let icon_rect = Rect::from_center_size(button_rect.center(), egui::vec2(16.0, 16.0));
            let tint = if is_active {
                Color32::WHITE
            } else if is_dark {
                Color32::from_gray(190)
            } else {
                Color32::from_gray(70)
            };

            let painted = icon_name
                .map(|name| icons.paint(painter, name, icon_rect, tint))
                .unwrap_or(false);
            if !painted {
                painter.text(
                    button_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    *fallback,
                    egui::FontId::proportional(11.0),
                    tint,
                );
            }

            painter.text(
                Pos2::new(button_rect.right() - 2.0, button_rect.top() + 2.0),
                egui::Align2::RIGHT_TOP,
                *fallback,
                egui::FontId::proportional(7.0),
                if is_dark {
                    Color32::from_rgba_unmultiplied(130, 130, 138, 150)
                } else {
                    Color32::from_rgba_unmultiplied(120, 120, 128, 150)
                },
            );
        }

        let badge_rect = self.edit_mode_badge_rect(rect);
        let badge_fill = if self.edit_mode == EditMode::Vertex {
            Color32::from_rgba_unmultiplied(theme::ACCENT.r(), theme::ACCENT.g(), theme::ACCENT.b(), 60)
        } else if is_dark {
            Color32::from_rgba_unmultiplied(40, 40, 44, 180)
        } else {
            Color32::from_rgba_unmultiplied(228, 228, 232, 190)
        };
        painter.rect_filled(badge_rect, 4.0, badge_fill);

        let icon_name = match self.edit_mode {
            EditMode::Object => "object_mode.png",
            EditMode::Vertex => "vertex_mode.png",
        };
        let icon_rect = Rect::from_center_size(badge_rect.center(), egui::vec2(16.0, 16.0));
        let tint = if self.edit_mode == EditMode::Vertex {
            theme::ACCENT
        } else if is_dark {
            Color32::from_gray(170)
        } else {
            Color32::from_gray(80)
        };
        if !icons.paint(painter, icon_name, icon_rect, tint) {
            painter.text(
                badge_rect.center(),
                egui::Align2::CENTER_CENTER,
                match self.edit_mode {
                    EditMode::Object => "OBJ",
                    EditMode::Vertex => "VTX",
                },
                egui::FontId::proportional(8.0),
                tint,
            );
        }
    }

    fn draw_mode_toggle(&self, painter: &egui::Painter, rect: Rect, is_dark: bool) {
        let group_rect = self.mode_toggle_group_rect(rect);
        painter.rect_filled(group_rect, 5.0, hud_group_fill(is_dark));
        painter.rect_stroke(group_rect, 5.0, Stroke::new(0.5, hud_group_border(is_dark)));

        for (index, mode) in [ViewportMode::View2D, ViewportMode::View3D].iter().enumerate() {
            let button_rect = self.mode_toggle_button_rect(rect, index);
            let is_active = self.mode == *mode;
            if is_active {
                let active_bg = if is_dark {
                    Color32::from_rgba_premultiplied(55, 55, 60, 220)
                } else {
                    Color32::from_rgba_premultiplied(200, 200, 205, 220)
                };
                painter.rect_filled(button_rect, 3.0, active_bg);
                let accent_rect = Rect::from_min_size(
                    Pos2::new(button_rect.left() + 4.0, button_rect.bottom() - 2.0),
                    egui::vec2(button_rect.width() - 8.0, 2.0),
                );
                painter.rect_filled(accent_rect, 1.0, theme::ACCENT);
            }

            painter.text(
                button_rect.center(),
                egui::Align2::CENTER_CENTER,
                match mode {
                    ViewportMode::View2D => "2D",
                    ViewportMode::View3D => "3D",
                },
                egui::FontId::proportional(10.0),
                if is_active {
                    if is_dark { Color32::from_gray(235) } else { Color32::from_gray(35) }
                } else if is_dark {
                    Color32::from_gray(120)
                } else {
                    Color32::from_gray(125)
                },
            );
        }

        painter.text(
            Pos2::new(group_rect.center().x, group_rect.bottom() + 4.0),
            egui::Align2::CENTER_TOP,
            match self.mode {
                ViewportMode::View2D => "2D View",
                ViewportMode::View3D => "3D View",
            },
            egui::FontId::proportional(9.0),
            Color32::from_rgba_unmultiplied(110, 110, 118, 150),
        );
    }

    fn draw_info_overlay(&self, painter: &egui::Painter, rect: Rect, is_dark: bool) {
        let info = self.info_overlay_text();
        let info_rect = self.info_rect(rect);
        painter.rect_filled(info_rect, 5.0, hud_group_fill(is_dark));
        painter.rect_stroke(info_rect, 5.0, Stroke::new(0.5, hud_group_border(is_dark)));

        painter.text(
            Pos2::new(info_rect.right() - 8.0, info_rect.center().y),
            egui::Align2::RIGHT_CENTER,
            info,
            egui::FontId::proportional(9.0),
            if is_dark { Color32::from_gray(215) } else { Color32::from_gray(55) },
        );
    }

    fn draw_visual_toggles(&self, painter: &egui::Painter, rect: Rect, is_dark: bool) {
        let group_rect = self.toggle_group_rect(rect);
        painter.rect_filled(group_rect, 5.0, hud_group_fill(is_dark));
        painter.rect_stroke(group_rect, 5.0, Stroke::new(0.5, hud_group_border(is_dark)));

        for (index, is_active) in [self.grid_visible, self.show_labels].iter().enumerate() {
            let button_rect = self.toggle_button_rect(rect, index);
            let fill = if *is_active {
                Color32::from_rgba_unmultiplied(theme::ACCENT.r(), theme::ACCENT.g(), theme::ACCENT.b(), if is_dark { 70 } else { 58 })
            } else if is_dark {
                Color32::from_rgba_unmultiplied(38, 38, 42, 190)
            } else {
                Color32::from_rgba_unmultiplied(244, 244, 247, 230)
            };
            let stroke = if *is_active {
                Stroke::new(1.0, theme::ACCENT)
            } else {
                Stroke::new(0.75, hud_group_border(is_dark))
            };
            painter.rect_filled(button_rect, 4.0, fill);
            painter.rect_stroke(button_rect, 4.0, stroke);

            match index {
                0 => self.paint_grid_toggle_icon(painter, button_rect, *is_active, is_dark),
                1 => {
                    painter.text(
                        button_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Aa",
                        egui::FontId::proportional(9.0),
                        if *is_active {
                            if is_dark { Color32::WHITE } else { Color32::from_gray(30) }
                        } else if is_dark {
                            Color32::from_gray(180)
                        } else {
                            Color32::from_gray(90)
                        },
                    );
                }
                _ => {}
            }
        }
    }

    fn paint_grid_toggle_icon(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        is_active: bool,
        is_dark: bool,
    ) {
        let color = if is_active {
            if is_dark { Color32::WHITE } else { Color32::from_gray(30) }
        } else if is_dark {
            Color32::from_gray(180)
        } else {
            Color32::from_gray(90)
        };
        let left = rect.left() + 7.0;
        let right = rect.right() - 7.0;
        let top = rect.top() + 5.0;
        let bottom = rect.bottom() - 5.0;

        for factor in [0.0, 0.5, 1.0] {
            let x = egui::lerp(left..=right, factor);
            painter.line_segment([Pos2::new(x, top), Pos2::new(x, bottom)], Stroke::new(1.0, color));
        }
        for factor in [0.0, 0.5, 1.0] {
            let y = egui::lerp(top..=bottom, factor);
            painter.line_segment([Pos2::new(left, y), Pos2::new(right, y)], Stroke::new(1.0, color));
        }
    }

    fn draw_axis_gizmo(&self, painter: &egui::Painter, rect: Rect, is_dark: bool) {
        let gizmo_rect = self.axis_gizmo_rect(rect);
        let center = gizmo_rect.center();
        painter.rect_filled(gizmo_rect, 8.0, hud_group_fill(is_dark));
        painter.rect_stroke(gizmo_rect, 8.0, Stroke::new(0.5, hud_group_border(is_dark)));

        painter.circle_filled(
            center,
            8.0,
            if is_dark {
                Color32::from_rgba_unmultiplied(42, 42, 48, 220)
            } else {
                Color32::from_rgba_unmultiplied(246, 246, 250, 235)
            },
        );
        painter.circle_stroke(center, 8.0, Stroke::new(1.0, theme::ACCENT));
        painter.text(
            center,
            egui::Align2::CENTER_CENTER,
            "ISO",
            egui::FontId::proportional(7.0),
            if is_dark { Color32::from_gray(220) } else { Color32::from_gray(50) },
        );

        for endpoint in self.axis_gizmo_endpoints(rect) {
            painter.line_segment([center, endpoint.end], Stroke::new(2.0, endpoint.color));
            painter.circle_filled(endpoint.end, 4.5, endpoint.color);
            painter.text(
                endpoint.end + egui::vec2(0.0, -9.0),
                egui::Align2::CENTER_BOTTOM,
                endpoint.label,
                egui::FontId::proportional(9.0),
                endpoint.color,
            );
        }
    }

    fn axis_gizmo_endpoints(&self, rect: Rect) -> [AxisGizmoEndpoint; 3] {
        let gizmo_rect = self.axis_gizmo_rect(rect);
        let center = gizmo_rect.center();
        let len = 18.0;
        let view = self.bridge.camera().view_matrix();

        [
            (Vec3::X, Color32::from_rgb(220, 70, 70), "X"),
            (Vec3::Y, Color32::from_rgb(70, 220, 70), "Y"),
            (Vec3::Z, Color32::from_rgb(70, 100, 220), "Z"),
        ]
        .map(|(axis, color, label)| {
            let view_dir = view.transform_vector3(axis);
            AxisGizmoEndpoint {
                end: Pos2::new(center.x + view_dir.x * len, center.y - view_dir.y * len),
                axis,
                color,
                label,
            }
        })
    }

    fn info_overlay_text(&self) -> String {
        let stats = self.bridge.stats();
        let graphics_status = self.render_runtime.status_badge();
        match self.mode {
            ViewportMode::View2D => format!(
                "{} | {} | Zoom {:.2}x | Sel {} | R {:.1}ms | U {:.1}ms",
                match self.edit_mode {
                    EditMode::Object => "OBJ",
                    EditMode::Vertex => "VTX",
                },
                graphics_status,
                self.bridge.zoom_2d(),
                self.selected.len(),
                self.render_cpu_ms(),
                self.upload_cpu_ms(),
            ),
            ViewportMode::View3D => format!(
                "{} | {} | {} | E {}/{} | T {} | Sel {} | D {:.1} | Q {:.2}x | R {:.1}ms | U {:.1}ms",
                match self.edit_mode {
                    EditMode::Object => "OBJ",
                    EditMode::Vertex => "VTX",
                },
                graphics_status,
                match self.bridge.gizmo().mode {
                    GizmoMode::Translate => "Move",
                    GizmoMode::Rotate => "Rotate",
                    GizmoMode::Scale => "Scale",
                },
                stats.visible_entities,
                stats.total_entities,
                stats.triangles_rendered,
                self.selected.len(),
                self.bridge.orbit_distance(),
                self.effective_render_scale(),
                self.render_cpu_ms(),
                self.upload_cpu_ms(),
            ),
        }
    }

    fn toolbar_action_at(&self, rect: Rect, pos: Pos2) -> Option<HudAction> {
        let actions = [
            HudAction::SetGizmo(GizmoMode::Translate),
            HudAction::SetGizmo(GizmoMode::Rotate),
            HudAction::SetGizmo(GizmoMode::Scale),
            HudAction::Focus,
        ];

        actions.iter().enumerate().find_map(|(index, action)| {
            self.toolbar_button_rect(rect, index).contains(pos).then_some(*action)
        })
    }

    fn mode_toggle_at(&self, rect: Rect, pos: Pos2) -> Option<ViewportMode> {
        [ViewportMode::View2D, ViewportMode::View3D]
            .iter()
            .enumerate()
            .find_map(|(index, mode)| self.mode_toggle_button_rect(rect, index).contains(pos).then_some(*mode))
    }

    fn toggle_action_at(&self, rect: Rect, pos: Pos2) -> Option<HudAction> {
        [HudAction::ToggleGrid, HudAction::ToggleLabels]
            .iter()
            .enumerate()
            .find_map(|(index, action)| self.toggle_button_rect(rect, index).contains(pos).then_some(*action))
    }

    fn axis_gizmo_action_at(&self, rect: Rect, pos: Pos2) -> Option<HudAction> {
        let gizmo_rect = self.axis_gizmo_rect(rect);
        if !gizmo_rect.contains(pos) {
            return None;
        }

        let center = gizmo_rect.center();
        if center.distance(pos) <= 10.0 {
            return Some(HudAction::ResetView);
        }

        self.axis_gizmo_endpoints(rect)
            .iter()
            .find_map(|endpoint| (endpoint.end.distance(pos) <= 11.0).then_some(HudAction::SnapAxis(endpoint.axis)))
    }

    fn toolbar_group_rect(&self, rect: Rect) -> Rect {
        let width = (HUD_BUTTON_SIZE + HUD_BUTTON_GAP) * 4.0 - HUD_BUTTON_GAP;
        Rect::from_min_size(
            Pos2::new(rect.left() + HUD_PADDING - 3.0, rect.top() + HUD_PADDING - 3.0),
            egui::vec2(width + 6.0, HUD_BUTTON_SIZE + 6.0),
        )
    }

    fn toolbar_button_rect(&self, rect: Rect, index: usize) -> Rect {
        Rect::from_min_size(
            Pos2::new(
                rect.left() + HUD_PADDING + index as f32 * (HUD_BUTTON_SIZE + HUD_BUTTON_GAP),
                rect.top() + HUD_PADDING,
            ),
            egui::vec2(HUD_BUTTON_SIZE, HUD_BUTTON_SIZE),
        )
    }

    fn edit_mode_badge_rect(&self, rect: Rect) -> Rect {
        let group_rect = self.toolbar_group_rect(rect);
        Rect::from_min_size(
            Pos2::new(group_rect.right() + 8.0, rect.top() + HUD_PADDING + 3.0),
            egui::vec2(30.0, HUD_BUTTON_SIZE - 6.0),
        )
    }

    fn mode_toggle_group_rect(&self, rect: Rect) -> Rect {
        Rect::from_min_size(
            Pos2::new(rect.center().x - 35.0, rect.top() + HUD_PADDING - 1.0),
            egui::vec2(70.0, 24.0),
        )
    }

    fn mode_toggle_button_rect(&self, rect: Rect, index: usize) -> Rect {
        let group = self.mode_toggle_group_rect(rect);
        Rect::from_min_size(
            Pos2::new(group.left() + index as f32 * 34.0 + 1.0, group.top() + 1.0),
            egui::vec2(34.0, 22.0),
        )
    }

    fn info_rect(&self, rect: Rect) -> Rect {
        let text_len = self.info_overlay_text().chars().count() as f32;
        let min_left = rect.center().x + 48.0;
        let max_width = (rect.right() - min_left - 8.0).max(180.0);
        let width = ((text_len * 5.9) + 22.0).clamp(180.0, max_width);
        Rect::from_min_size(
            Pos2::new(rect.right() - width - 8.0, rect.top() + HUD_PADDING - 1.0),
            egui::vec2(width, 24.0),
        )
    }

    fn toggle_group_rect(&self, rect: Rect) -> Rect {
        let info_rect = self.info_rect(rect);
        let width = (HUD_TOGGLE_BUTTON_WIDTH * 2.0) + HUD_TOGGLE_BUTTON_GAP + 6.0;
        Rect::from_min_size(
            Pos2::new(info_rect.right() - width, info_rect.bottom() + 6.0),
            egui::vec2(width, HUD_TOGGLE_BUTTON_HEIGHT + 6.0),
        )
    }

    fn toggle_button_rect(&self, rect: Rect, index: usize) -> Rect {
        let group_rect = self.toggle_group_rect(rect);
        Rect::from_min_size(
            Pos2::new(
                group_rect.left() + 3.0 + index as f32 * (HUD_TOGGLE_BUTTON_WIDTH + HUD_TOGGLE_BUTTON_GAP),
                group_rect.top() + 3.0,
            ),
            egui::vec2(HUD_TOGGLE_BUTTON_WIDTH, HUD_TOGGLE_BUTTON_HEIGHT),
        )
    }

    fn axis_gizmo_rect(&self, rect: Rect) -> Rect {
        Rect::from_min_size(
            Pos2::new(rect.left() + 12.0, rect.bottom() - 72.0),
            egui::vec2(56.0, 56.0),
        )
    }
}

fn hud_group_fill(is_dark: bool) -> Color32 {
    if is_dark {
        Color32::from_rgba_premultiplied(20, 20, 22, 210)
    } else {
        Color32::from_rgba_premultiplied(235, 235, 238, 230)
    }
}

fn hud_group_border(is_dark: bool) -> Color32 {
    if is_dark {
        Color32::from_rgba_premultiplied(60, 60, 65, 100)
    } else {
        Color32::from_rgba_premultiplied(180, 180, 185, 100)
    }
}