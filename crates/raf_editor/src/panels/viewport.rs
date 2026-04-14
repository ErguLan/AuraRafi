//! Viewport panel - Hybrid 2D/3D scene view.
//!
//! Renders scene entities using egui painter with projection math.
//! No GPU pipeline - just matrix math and vector drawing. Runs on anything.
//! Ultra lightweight: zero GPU buffers, zero shaders, zero texture memory.
//!
//! Features:
//! - Depth-sorted rendering (painter's algorithm, no Z-fighting)
//! - Transform gizmo arrows (RGB X/Y/Z) with drag interaction
//! - Entity picking (click to select in 3D)
//! - Multi-select (Shift+Click)
//! - Edit mode (Tab toggle for vertex editing)

use egui::{Color32, Pos2, Rect, Stroke, Ui, Vec2};
use glam::{Mat4, Vec3};

use raf_core::config::Language;
use raf_core::scene::graph::{NodeColor, Primitive, SceneNodeId};
use raf_core::SceneGraph;
use raf_render::camera::Camera;
use raf_render::depth_sort::{self, DepthSorter};
use raf_render::mesh;
use raf_render::picking::{self, GIZMO_ARROWS, GIZMO_LINE_WIDTH};
use raf_render::projection;

use crate::theme;

// ---------------------------------------------------------------------------
// Viewport mode
// ---------------------------------------------------------------------------

/// Viewport rendering mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewportMode {
    /// Top-down 2D view for 2D games / sprites.
    View2D,
    /// Perspective 3D view with orbit camera.
    View3D,
}

impl Default for ViewportMode {
    fn default() -> Self {
        Self::View3D
    }
}

/// Currently active viewport tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewportTool {
    Select,
    Move,
    Rotate,
    Scale,
}

/// Edit mode (Object mode vs vertex/face editing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditMode {
    /// Normal object mode (select, move, rotate, scale whole entities).
    Object,
    /// Vertex editing mode (select and move individual vertices).
    Vertex,
}

impl Default for EditMode {
    fn default() -> Self {
        Self::Object
    }
}

/// Active gizmo drag state (when user is dragging an axis arrow).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GizmoDragAxis {
    None,
    X,
    Y,
    Z,
}

/// Rendering style for 3D entities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderStyle {
    /// Filled faces with flat shading + wireframe overlay.
    Solid,
    /// Wireframe only (lightest).
    Wireframe,
    /// Filled faces without wireframe.
    SolidOnly,
}

impl Default for RenderStyle {
    fn default() -> Self {
        Self::Solid
    }
}

// ---------------------------------------------------------------------------
// Panel state
// ---------------------------------------------------------------------------

/// State for the viewport panel.
pub struct ViewportPanel {
    /// Current rendering mode (2D or 3D).
    pub mode: ViewportMode,
    /// Camera for 3D projection.
    pub camera: Camera,
    /// Current tool mode.
    pub tool: ViewportTool,
    /// Selected entities (multi-select via Shift+Click).
    pub selected: Vec<SceneNodeId>,
    /// Show grid.
    pub grid_visible: bool,
    /// 3D rendering style.
    pub render_style: RenderStyle,
    /// Current edit mode.
    pub edit_mode: EditMode,
    /// Active gizmo drag axis.
    pub gizmo_drag: GizmoDragAxis,
    /// Gizmo drag start position (screen coords).
    gizmo_drag_start: [f32; 2],
    /// Gizmo drag entity position at start of drag.
    gizmo_drag_origin: Vec3,
    /// Depth sorter (reused across frames, avoids allocation).
    depth_sorter: DepthSorter,
    /// Rendered triangle count (for stats display).
    pub tri_count: usize,
    /// Orbit yaw angle (radians).
    orbit_yaw: f32,
    /// Orbit pitch angle (radians).
    orbit_pitch: f32,
    /// Orbit distance from target.
    orbit_distance: f32,
    /// 2D camera offset for panning.
    offset_2d: [f32; 2],
    /// 2D zoom level.
    zoom_2d: f32,
}

impl Default for ViewportPanel {
    fn default() -> Self {
        Self {
            mode: ViewportMode::View3D,
            camera: Camera::default(),
            tool: ViewportTool::Select,
            selected: Vec::new(),
            grid_visible: true,
            render_style: RenderStyle::Solid,
            edit_mode: EditMode::Object,
            gizmo_drag: GizmoDragAxis::None,
            gizmo_drag_start: [0.0; 2],
            gizmo_drag_origin: Vec3::ZERO,
            depth_sorter: DepthSorter::new(),
            tri_count: 0,
            orbit_yaw: std::f32::consts::FRAC_PI_4,
            orbit_pitch: 0.5,
            orbit_distance: 8.0,
            offset_2d: [0.0, 0.0],
            zoom_2d: 1.0,
        }
    }
}

impl ViewportPanel {
    /// Main draw entry point.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        scene: &mut SceneGraph,
        is_dark: bool,
        lang: Language,
    ) {
        let rect = ui.available_rect_before_wrap();
        let painter = ui.painter_at(rect);

        // Background
        let bg = if is_dark {
            Color32::from_rgb(18, 18, 18)
        } else {
            Color32::from_rgb(240, 240, 242)
        };
        painter.rect_filled(rect, 0.0, bg);

        match self.mode {
            ViewportMode::View2D => self.draw_2d(&painter, rect, scene, is_dark),
            ViewportMode::View3D => self.draw_3d(&painter, rect, scene, is_dark),
        }

        // Overlays (toolbar, info, mode toggle, render style toggle)
        self.draw_toolbar(&painter, rect, is_dark, lang);
        self.draw_mode_toggle(&painter, rect, is_dark, lang);
        self.draw_render_style_toggle(&painter, rect, is_dark, lang);
        self.draw_info_overlay(&painter, rect, is_dark);

        // Interaction (includes entity picking, gizmo drag, camera controls)
        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        self.handle_input(ui, &response, rect, scene);
    }

    // -------------------------------------------------------------------
    // 3D rendering (depth-sorted faces + wireframe + gizmo)
    // -------------------------------------------------------------------

    fn draw_3d(
        &mut self,
        painter: &egui::Painter,
        rect: Rect,
        scene: &SceneGraph,
        is_dark: bool,
    ) {
        let vp_w = rect.width();
        let vp_h = rect.height();
        let view_proj = self.camera.view_projection(vp_w, vp_h);

        // Draw ground grid.
        self.draw_3d_grid(painter, rect, &view_proj, is_dark);

        // Draw axis indicator in corner.
        self.draw_axis_gizmo(painter, rect, is_dark);

        let light_dir = Vec3::new(0.5, 0.8, 0.3).normalize();

        // -- Phase 1: Collect all faces into depth sorter --
        self.depth_sorter.clear();

        for (id, node) in scene.iter() {
            if !node.visible || node.primitive == Primitive::Empty {
                continue;
            }

            let model = scene.world_matrix(id);
            let is_selected = self.selected.contains(&id);

            if self.render_style != RenderStyle::Wireframe {
                // Get face data for this primitive.
                let faces: Vec<([Vec3; 4], Vec3)> = match node.primitive {
                    Primitive::Cube => mesh::cube_faces(),
                    Primitive::Sphere => mesh::sphere_faces(4, 6),
                    Primitive::Plane => mesh::plane_faces(),
                    Primitive::Cylinder => mesh::cylinder_faces(8),
                    Primitive::Sprite2D => mesh::plane_faces(),
                    Primitive::Empty => continue,
                };

                for (corners, normal) in &faces {
                    let brightness = depth_sort::face_brightness(*normal, light_dir, &model);

                    let color = if is_selected {
                        depth_sort::shade_color(
                            theme::ACCENT.r(), theme::ACCENT.g(), theme::ACCENT.b(),
                            160, brightness,
                        )
                    } else {
                        depth_sort::shade_color(
                            node.color.r, node.color.g, node.color.b,
                            180, brightness,
                        )
                    };

                    let wire = self.render_style == RenderStyle::Solid;
                    let wc = if is_selected {
                        [theme::ACCENT.r(), theme::ACCENT.g(), theme::ACCENT.b(), 255]
                    } else {
                        [node.color.r, node.color.g, node.color.b, 200]
                    };

                    self.depth_sorter.add_quad(
                        corners, *normal, &model, &view_proj,
                        vp_w, vp_h, color, wire, wc,
                        if is_selected { 2.0 } else { 1.0 },
                    );
                }
            }
        }

        // -- Phase 2: Sort by depth (farthest first) --
        self.depth_sorter.sort();
        self.tri_count = self.depth_sorter.triangle_count();

        // -- Phase 3: Draw sorted faces --
        for face in self.depth_sorter.faces() {
            let pts: Vec<Pos2> = face.screen_points.iter()
                .map(|p| Pos2::new(rect.left() + p[0], rect.top() + p[1]))
                .collect();

            if pts.len() >= 3 {
                let fc = Color32::from_rgba_premultiplied(
                    face.color[0], face.color[1], face.color[2], face.color[3],
                );

                let stroke = if face.wireframe {
                    let wc = Color32::from_rgba_premultiplied(
                        face.wire_color[0], face.wire_color[1],
                        face.wire_color[2], face.wire_color[3],
                    );
                    Stroke::new(face.wire_width, wc)
                } else {
                    Stroke::NONE
                };

                painter.add(egui::Shape::convex_polygon(pts, fc, stroke));
            }
        }

        // -- Phase 4: Wireframe-only mode edges --
        if self.render_style == RenderStyle::Wireframe {
            for (id, node) in scene.iter() {
                if !node.visible || node.primitive == Primitive::Empty {
                    continue;
                }
                let model = scene.world_matrix(id);
                let is_selected = self.selected.contains(&id);
                self.draw_primitive_wireframe(
                    painter, rect, &model, &view_proj, &node.color,
                    is_selected, node.primitive, vp_w, vp_h,
                );
            }
        }

        // -- Phase 5: Entity labels (always on top) --
        for (id, node) in scene.iter() {
            if !node.visible || node.primitive == Primitive::Empty {
                continue;
            }
            let is_selected = self.selected.contains(&id);
            if let Some(center_2d) = projection::project_point(
                node.position, &view_proj, vp_w, vp_h,
            ) {
                let lbl_pos = Pos2::new(
                    rect.left() + center_2d[0],
                    rect.top() + center_2d[1] - 20.0,
                );
                if rect.contains(lbl_pos) {
                    painter.text(
                        lbl_pos,
                        egui::Align2::CENTER_BOTTOM,
                        &node.name,
                        egui::FontId::proportional(10.0),
                        if is_selected { theme::ACCENT } else { theme::DARK_TEXT_DIM },
                    );
                }
            }
        }

        // -- Phase 6: Transform gizmo arrows (on top of everything) --
        if self.tool != ViewportTool::Select {
            if let Some(sel_id) = self.selected.first().copied() {
                if let Some(node) = scene.get(sel_id) {
                    self.draw_gizmo_arrows(painter, rect, node.position, &view_proj, vp_w, vp_h);
                }
            }
        }

        // -- Phase 7: Edit mode indicator --
        if self.edit_mode == EditMode::Vertex {
            let badge_pos = Pos2::new(rect.left() + 8.0, rect.bottom() - 20.0);
            painter.text(
                badge_pos,
                egui::Align2::LEFT_BOTTOM,
                "EDIT MODE",
                egui::FontId::proportional(10.0),
                theme::ACCENT,
            );
        }
    }

    /// Draw transform gizmo arrows at entity position.
    fn draw_gizmo_arrows(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        entity_pos: Vec3,
        view_proj: &Mat4,
        vp_w: f32,
        vp_h: f32,
    ) {
        for arrow in &GIZMO_ARROWS {
            if let Some(screen) = picking::project_gizmo_arrow(
                entity_pos, arrow, view_proj, vp_w, vp_h,
            ) {
                let color = Color32::from_rgba_premultiplied(
                    screen.color[0], screen.color[1], screen.color[2], screen.color[3],
                );

                // Check if this axis is being dragged (thicker line).
                let is_dragging = match (self.gizmo_drag, arrow.label) {
                    (GizmoDragAxis::X, "X") => true,
                    (GizmoDragAxis::Y, "Y") => true,
                    (GizmoDragAxis::Z, "Z") => true,
                    _ => false,
                };
                let line_w = if is_dragging { GIZMO_LINE_WIDTH * 1.5 } else { GIZMO_LINE_WIDTH };

                let start = Pos2::new(rect.left() + screen.start[0], rect.top() + screen.start[1]);
                let end = Pos2::new(rect.left() + screen.end[0], rect.top() + screen.end[1]);

                // Shaft.
                painter.line_segment([start, end], Stroke::new(line_w, color));

                // Arrowhead triangle.
                let head = vec![
                    Pos2::new(rect.left() + screen.head_tip[0], rect.top() + screen.head_tip[1]),
                    Pos2::new(rect.left() + screen.head_left[0], rect.top() + screen.head_left[1]),
                    Pos2::new(rect.left() + screen.head_right[0], rect.top() + screen.head_right[1]),
                ];
                painter.add(egui::Shape::convex_polygon(head, color, Stroke::NONE));

                // Axis label at tip.
                painter.text(
                    Pos2::new(end.x + 6.0, end.y - 6.0),
                    egui::Align2::LEFT_BOTTOM,
                    arrow.label,
                    egui::FontId::proportional(9.0),
                    color,
                );
            }
        }
    }

    /// Render wireframe edges for any primitive type.
    fn draw_primitive_wireframe(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        model: &Mat4,
        view_proj: &Mat4,
        color: &NodeColor,
        is_selected: bool,
        primitive: Primitive,
        vp_w: f32,
        vp_h: f32,
    ) {
        let edges = match primitive {
            Primitive::Cube => mesh::cube_edges(),
            Primitive::Sphere => mesh::sphere_edges(12),     // 3 circles x 12 = 36 edges
            Primitive::Plane => mesh::plane_edges(),          // 5 edges
            Primitive::Cylinder => mesh::cylinder_edges(12),  // ~28 edges
            Primitive::Sprite2D => mesh::plane_edges(),
            Primitive::Empty => return,
        };

        let wire_color = if is_selected {
            theme::ACCENT
        } else {
            let nc = color;
            Color32::from_rgba_premultiplied(nc.r, nc.g, nc.b, 200)
        };
        let wire_width = if is_selected { 2.0 } else { 1.0 };

        for edge in &edges {
            let transformed = [
                (*model * edge[0].extend(1.0)).truncate(),
                (*model * edge[1].extend(1.0)).truncate(),
            ];
            if let Some(screen_edge) = projection::project_edge(
                &[transformed[0], transformed[1]],
                view_proj,
                vp_w,
                vp_h,
            ) {
                let a = Pos2::new(rect.left() + screen_edge[0][0], rect.top() + screen_edge[0][1]);
                let b = Pos2::new(rect.left() + screen_edge[1][0], rect.top() + screen_edge[1][1]);
                if rect.contains(a) || rect.contains(b) {
                    painter.line_segment([a, b], Stroke::new(wire_width, wire_color));
                }
            }
        }
    }

    // -------------------------------------------------------------------
    // 2D rendering
    // -------------------------------------------------------------------

    fn draw_2d(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        scene: &SceneGraph,
        is_dark: bool,
    ) {
        // Draw dot grid
        self.draw_2d_grid(painter, rect, is_dark);

        // Draw each entity as a colored rectangle
        for (id, node) in scene.iter() {
            if !node.visible || node.primitive == Primitive::Empty {
                continue;
            }

            let is_selected = self.selected.contains(&id);
            let screen_pos = self.world_to_screen_2d(
                Pos2::new(node.position.x, node.position.z), // XZ plane for 2D
                rect,
            );

            let size = node.scale.x * 40.0 * self.zoom_2d;
            let entity_rect = Rect::from_center_size(screen_pos, Vec2::splat(size));

            if !rect.intersects(entity_rect) {
                continue;
            }

            // Fill
            let nc = &node.color;
            let fill_alpha = if is_selected { 200 } else { 140 };
            let fill = Color32::from_rgba_premultiplied(nc.r, nc.g, nc.b, fill_alpha);
            painter.rect_filled(entity_rect, 4.0, fill);

            // Border
            let border_color = if is_selected { theme::ACCENT } else {
                Color32::from_rgba_premultiplied(nc.r, nc.g, nc.b, 220)
            };
            let border_width = if is_selected { 2.0 } else { 1.0 };
            painter.rect_stroke(entity_rect, 4.0, Stroke::new(border_width, border_color));

            // Label
            painter.text(
                Pos2::new(screen_pos.x, entity_rect.top() - 4.0),
                egui::Align2::CENTER_BOTTOM,
                &node.name,
                egui::FontId::proportional(10.0),
                if is_selected { theme::ACCENT } else { theme::DARK_TEXT_DIM },
            );
        }

        // Origin cross
        let origin = self.world_to_screen_2d(Pos2::ZERO, rect);
        let axis_len = 30.0;
        painter.line_segment(
            [Pos2::new(origin.x - axis_len, origin.y), Pos2::new(origin.x + axis_len, origin.y)],
            Stroke::new(1.0, Color32::from_rgba_premultiplied(220, 60, 60, 100)),
        );
        painter.line_segment(
            [Pos2::new(origin.x, origin.y - axis_len), Pos2::new(origin.x, origin.y + axis_len)],
            Stroke::new(1.0, Color32::from_rgba_premultiplied(60, 220, 60, 100)),
        );
        painter.circle_filled(origin, 3.0, Color32::from_rgba_premultiplied(212, 119, 26, 120));
    }

    // -------------------------------------------------------------------
    // Grids
    // -------------------------------------------------------------------

    fn draw_3d_grid(
        &self,
        painter: &egui::Painter,
        rect: Rect,
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

        let extent = 10;
        for i in -extent..=extent {
            let fi = i as f32;
            let is_major = i % 5 == 0;
            let color = if is_major { grid_major } else { grid_color };
            let width = if is_major { 0.8 } else { 0.4 };

            // Lines along X
            let a = Vec3::new(fi, 0.0, -(extent as f32));
            let b = Vec3::new(fi, 0.0, extent as f32);
            if let Some(edge) = projection::project_edge(&[a, b], view_proj, vp_w, vp_h) {
                painter.line_segment(
                    [
                        Pos2::new(rect.left() + edge[0][0], rect.top() + edge[0][1]),
                        Pos2::new(rect.left() + edge[1][0], rect.top() + edge[1][1]),
                    ],
                    Stroke::new(width, color),
                );
            }

            // Lines along Z
            let a = Vec3::new(-(extent as f32), 0.0, fi);
            let b = Vec3::new(extent as f32, 0.0, fi);
            if let Some(edge) = projection::project_edge(&[a, b], view_proj, vp_w, vp_h) {
                painter.line_segment(
                    [
                        Pos2::new(rect.left() + edge[0][0], rect.top() + edge[0][1]),
                        Pos2::new(rect.left() + edge[1][0], rect.top() + edge[1][1]),
                    ],
                    Stroke::new(width, color),
                );
            }
        }
    }

    fn draw_2d_grid(&self, painter: &egui::Painter, rect: Rect, is_dark: bool) {
        if !self.grid_visible {
            return;
        }

        let dot_color = if is_dark {
            Color32::from_rgba_premultiplied(255, 255, 255, 25)
        } else {
            Color32::from_rgba_premultiplied(0, 0, 0, 25)
        };

        let spacing = 40.0 * self.zoom_2d;
        if spacing < 8.0 {
            return;
        }

        let mut x = rect.left() + (self.offset_2d[0] % spacing);
        while x < rect.right() {
            let mut y = rect.top() + (self.offset_2d[1] % spacing);
            while y < rect.bottom() {
                painter.circle_filled(Pos2::new(x, y), 1.0, dot_color);
                y += spacing;
            }
            x += spacing;
        }
    }

    // -------------------------------------------------------------------
    // Overlays
    // -------------------------------------------------------------------

    fn draw_axis_gizmo(&self, painter: &egui::Painter, rect: Rect, _is_dark: bool) {
        // Small 3D axis gizmo in bottom-left corner
        let center = Pos2::new(rect.left() + 40.0, rect.bottom() - 40.0);
        let len = 25.0;

        // Get camera-relative directions
        let view = self.camera.view_matrix();

        // Project unit axis vectors through view matrix (ignore translation)
        let axes = [
            (Vec3::X, Color32::from_rgb(220, 70, 70), "X"),
            (Vec3::Y, Color32::from_rgb(70, 220, 70), "Y"),
            (Vec3::Z, Color32::from_rgb(70, 100, 220), "Z"),
        ];

        for (axis, color, label) in axes {
            let view_dir = view.transform_vector3(axis);
            let end = Pos2::new(
                center.x + view_dir.x * len,
                center.y - view_dir.y * len, // Y flipped
            );
            painter.line_segment([center, end], Stroke::new(2.0, color));
            painter.text(
                end,
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(9.0),
                color,
            );
        }
    }

    fn draw_toolbar(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        is_dark: bool,
        _lang: Language,
    ) {
        let tools = [
            (ViewportTool::Select, "Sel", "Q"),
            (ViewportTool::Move, "Mov", "W"),
            (ViewportTool::Rotate, "Rot", "E"),
            (ViewportTool::Scale, "Scl", "R"),
        ];

        let x = rect.left() + 8.0;
        let mut y = rect.top() + 8.0;
        let btn_w = 52.0;
        let btn_h = 22.0;
        let gap = 2.0;

        // Background pill for the entire toolbar group.
        let group_rect = Rect::from_min_size(
            Pos2::new(x - 2.0, y - 2.0),
            Vec2::new(btn_w + 4.0, (btn_h + gap) * tools.len() as f32 + 2.0),
        );
        let group_bg = if is_dark {
            Color32::from_rgba_premultiplied(20, 20, 22, 200)
        } else {
            Color32::from_rgba_premultiplied(235, 235, 238, 220)
        };
        painter.rect_filled(group_rect, 6.0, group_bg);
        let group_border = if is_dark {
            Color32::from_rgba_premultiplied(60, 60, 65, 100)
        } else {
            Color32::from_rgba_premultiplied(180, 180, 185, 100)
        };
        painter.rect_stroke(group_rect, 6.0, Stroke::new(0.5, group_border));

        for (tool, label, key) in tools {
            let btn_rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(btn_w, btn_h));
            let is_active = self.tool == tool;

            // Active: subtle filled background. Inactive: transparent.
            if is_active {
                let active_bg = if is_dark {
                    Color32::from_rgba_premultiplied(55, 55, 60, 200)
                } else {
                    Color32::from_rgba_premultiplied(200, 200, 205, 200)
                };
                painter.rect_filled(btn_rect, 3.0, active_bg);
                // Left accent bar.
                let accent_rect = Rect::from_min_size(
                    Pos2::new(btn_rect.left(), btn_rect.top() + 3.0),
                    Vec2::new(2.0, btn_h - 6.0),
                );
                painter.rect_filled(accent_rect, 1.0, Color32::from_rgb(212, 119, 26));
            }

            // Label text.
            let text_color = if is_active {
                if is_dark { Color32::from_rgb(230, 230, 235) } else { Color32::from_rgb(30, 30, 35) }
            } else if is_dark {
                Color32::from_rgb(130, 130, 135)
            } else {
                Color32::from_rgb(90, 90, 95)
            };

            painter.text(
                Pos2::new(btn_rect.left() + 10.0, btn_rect.center().y),
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(10.0),
                text_color,
            );

            // Shortcut key - dimmed.
            let key_color = if is_dark {
                Color32::from_rgba_premultiplied(100, 100, 105, 150)
            } else {
                Color32::from_rgba_premultiplied(140, 140, 145, 150)
            };
            painter.text(
                Pos2::new(btn_rect.right() - 8.0, btn_rect.center().y),
                egui::Align2::RIGHT_CENTER,
                key,
                egui::FontId::proportional(9.0),
                key_color,
            );

            y += btn_h + gap;
        }
    }

    fn draw_mode_toggle(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        is_dark: bool,
        _lang: Language,
    ) {
        // Segmented control at top-center.
        let center_x = rect.center().x;
        let y = rect.top() + 8.0;

        let btn_w = 34.0;
        let btn_h = 22.0;
        let total_w = btn_w * 2.0;

        // Group background.
        let group_rect = Rect::from_min_size(
            Pos2::new(center_x - total_w / 2.0 - 1.0, y - 1.0),
            Vec2::new(total_w + 2.0, btn_h + 2.0),
        );
        let group_bg = if is_dark {
            Color32::from_rgba_premultiplied(20, 20, 22, 200)
        } else {
            Color32::from_rgba_premultiplied(235, 235, 238, 220)
        };
        painter.rect_filled(group_rect, 5.0, group_bg);
        let border_color = if is_dark {
            Color32::from_rgba_premultiplied(60, 60, 65, 100)
        } else {
            Color32::from_rgba_premultiplied(180, 180, 185, 100)
        };
        painter.rect_stroke(group_rect, 5.0, Stroke::new(0.5, border_color));

        let modes = [
            (ViewportMode::View2D, "2D"),
            (ViewportMode::View3D, "3D"),
        ];

        for (i, (mode, label)) in modes.iter().enumerate() {
            let bx = center_x - total_w / 2.0 + (i as f32 * btn_w);
            let btn_rect = Rect::from_min_size(Pos2::new(bx, y), Vec2::new(btn_w, btn_h));
            let is_active = self.mode == *mode;

            if is_active {
                let active_bg = if is_dark {
                    Color32::from_rgba_premultiplied(55, 55, 60, 200)
                } else {
                    Color32::from_rgba_premultiplied(200, 200, 205, 200)
                };
                painter.rect_filled(btn_rect, 3.0, active_bg);
                // Bottom accent line.
                let accent_rect = Rect::from_min_size(
                    Pos2::new(btn_rect.left() + 4.0, btn_rect.bottom() - 2.0),
                    Vec2::new(btn_w - 8.0, 2.0),
                );
                painter.rect_filled(accent_rect, 1.0, Color32::from_rgb(212, 119, 26));
            }

            let text_color = if is_active {
                if is_dark { Color32::from_rgb(230, 230, 235) } else { Color32::from_rgb(30, 30, 35) }
            } else if is_dark {
                Color32::from_rgb(110, 110, 115)
            } else {
                Color32::from_rgb(120, 120, 125)
            };

            painter.text(
                btn_rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(10.0),
                text_color,
            );
        }

        // Mode label below.
        let mode_label = match self.mode {
            ViewportMode::View2D => "2D View",
            ViewportMode::View3D => "3D View",
        };
        painter.text(
            Pos2::new(center_x, y + btn_h + 3.0),
            egui::Align2::CENTER_TOP,
            mode_label,
            egui::FontId::proportional(9.0),
            Color32::from_rgba_premultiplied(100, 100, 105, 140),
        );
    }

    /// Render style toggle (top-right area).
    fn draw_render_style_toggle(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        is_dark: bool,
        _lang: Language,
    ) {
        if self.mode != ViewportMode::View3D {
            return;
        }

        let styles = [
            (RenderStyle::Solid, "Solid"),
            (RenderStyle::Wireframe, "Wire"),
            (RenderStyle::SolidOnly, "Fill"),
        ];

        let btn_w = 48.0;
        let btn_h = 20.0;
        let gap = 2.0;
        let x = rect.right() - btn_w - 10.0;
        let mut y = rect.top() + 30.0;

        // Group background.
        let group_rect = Rect::from_min_size(
            Pos2::new(x - 2.0, y - 2.0),
            Vec2::new(btn_w + 4.0, (btn_h + gap) * styles.len() as f32 + 2.0),
        );
        let group_bg = if is_dark {
            Color32::from_rgba_premultiplied(20, 20, 22, 200)
        } else {
            Color32::from_rgba_premultiplied(235, 235, 238, 220)
        };
        painter.rect_filled(group_rect, 6.0, group_bg);
        let border_color = if is_dark {
            Color32::from_rgba_premultiplied(60, 60, 65, 100)
        } else {
            Color32::from_rgba_premultiplied(180, 180, 185, 100)
        };
        painter.rect_stroke(group_rect, 6.0, Stroke::new(0.5, border_color));

        for (style, label) in styles {
            let btn_rect = Rect::from_min_size(
                Pos2::new(x, y),
                Vec2::new(btn_w, btn_h),
            );
            let is_active = self.render_style == style;

            if is_active {
                let active_bg = if is_dark {
                    Color32::from_rgba_premultiplied(55, 55, 60, 200)
                } else {
                    Color32::from_rgba_premultiplied(200, 200, 205, 200)
                };
                painter.rect_filled(btn_rect, 3.0, active_bg);
                // Right accent bar.
                let accent_rect = Rect::from_min_size(
                    Pos2::new(btn_rect.right() - 2.0, btn_rect.top() + 3.0),
                    Vec2::new(2.0, btn_h - 6.0),
                );
                painter.rect_filled(accent_rect, 1.0, Color32::from_rgb(212, 119, 26));
            }

            let text_color = if is_active {
                if is_dark { Color32::from_rgb(230, 230, 235) } else { Color32::from_rgb(30, 30, 35) }
            } else if is_dark {
                Color32::from_rgb(130, 130, 135)
            } else {
                Color32::from_rgb(90, 90, 95)
            };

            painter.text(
                btn_rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(10.0),
                text_color,
            );

            y += btn_h + gap;
        }
    }

    fn draw_info_overlay(&self, painter: &egui::Painter, rect: Rect, _is_dark: bool) {
        let info = match self.mode {
            ViewportMode::View2D => {
                format!("Zoom: {:.1}x", self.zoom_2d)
            }
            ViewportMode::View3D => {
                let mode_tag = match self.edit_mode {
                    EditMode::Object => "OBJ",
                    EditMode::Vertex => "VTX",
                };
                let tool_tag = match self.tool {
                    ViewportTool::Select => "Sel",
                    ViewportTool::Move => "Mov",
                    ViewportTool::Rotate => "Rot",
                    ViewportTool::Scale => "Scl",
                };
                format!(
                    "{} | {} | Tris: {} | Sel: {} | D:{:.1}",
                    mode_tag,
                    tool_tag,
                    self.tri_count,
                    self.selected.len(),
                    self.orbit_distance,
                )
            }
        };
        painter.text(
            Pos2::new(rect.right() - 8.0, rect.top() + 10.0),
            egui::Align2::RIGHT_TOP,
            info,
            egui::FontId::proportional(9.0),
            theme::DARK_TEXT_DIM,
        );
    }

    // -------------------------------------------------------------------
    // Input handling
    // -------------------------------------------------------------------

    fn handle_input(&mut self, ui: &mut Ui, response: &egui::Response, rect: Rect, scene: &mut SceneGraph) {
        // Click on render style buttons (top-right).
        if self.mode == ViewportMode::View3D {
            if response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let btn_w = 48.0;
                    let btn_h = 20.0;
                    let gap = 2.0;
                    let x = rect.right() - btn_w - 10.0;
                    let styles = [RenderStyle::Solid, RenderStyle::Wireframe, RenderStyle::SolidOnly];
                    let mut y = rect.top() + 30.0;
                    for style in styles {
                        let btn_rect = Rect::from_min_size(
                            Pos2::new(x, y),
                            Vec2::new(btn_w, btn_h),
                        );
                        if btn_rect.contains(pos) {
                            self.render_style = style;
                        }
                        y += btn_h + gap;
                    }
                }
            }
        }

        // Click on mode toggle buttons (top-center).
        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let center_x = rect.center().x;
                let y = rect.top() + 8.0;
                let btn_w = 34.0;
                let btn_h = 22.0;
                let total_w = btn_w * 2.0;
                let rect_2d = Rect::from_min_size(
                    Pos2::new(center_x - total_w / 2.0, y),
                    Vec2::new(btn_w, btn_h),
                );
                let rect_3d = Rect::from_min_size(
                    Pos2::new(center_x - total_w / 2.0 + btn_w, y),
                    Vec2::new(btn_w, btn_h),
                );
                if rect_2d.contains(pos) {
                    self.mode = ViewportMode::View2D;
                } else if rect_3d.contains(pos) {
                    self.mode = ViewportMode::View3D;
                }
            }
        }

        // Click on tool buttons (top-left).
        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let x = rect.left() + 8.0;
                let btn_w = 52.0;
                let btn_h = 22.0;
                let gap = 2.0;
                let tools = [ViewportTool::Select, ViewportTool::Move, ViewportTool::Rotate, ViewportTool::Scale];
                let mut y = rect.top() + 8.0;
                for tool in tools {
                    let btn_rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(btn_w, btn_h));
                    if btn_rect.contains(pos) {
                        self.tool = tool;
                    }
                    y += btn_h + gap;
                }
            }
        }

        // --- Entity picking (click to select in 3D/2D viewport) ---
        if self.mode == ViewportMode::View3D && response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                // Only pick if clicking in the viewport area (not on overlay buttons).
                let toolbar_rect = Rect::from_min_size(
                    Pos2::new(rect.left(), rect.top()),
                    Vec2::new(70.0, 100.0),
                );
                let mode_rect = Rect::from_center_size(
                    Pos2::new(rect.center().x, rect.top() + 19.0),
                    Vec2::new(80.0, 30.0),
                );
                let style_rect = Rect::from_min_size(
                    Pos2::new(rect.right() - 60.0, rect.top() + 28.0),
                    Vec2::new(60.0, 80.0),
                );

                if !toolbar_rect.contains(pos) && !mode_rect.contains(pos) && !style_rect.contains(pos) {
                    let vp_w = rect.width();
                    let vp_h = rect.height();
                    let view_proj = self.camera.view_projection(vp_w, vp_h);

                    // First check: gizmo arrow click.
                    let mut gizmo_clicked = false;
                    if self.tool != ViewportTool::Select {
                        if let Some(sel_id) = self.selected.first().copied() {
                            if let Some(node) = scene.get(sel_id) {
                                let click_local = [pos.x - rect.left(), pos.y - rect.top()];
                                if let Some((axis_idx, _dist)) = picking::pick_gizmo_arrow(
                                    click_local, node.position, &view_proj, vp_w, vp_h,
                                ) {
                                    self.gizmo_drag = match axis_idx {
                                        0 => GizmoDragAxis::X,
                                        1 => GizmoDragAxis::Y,
                                        2 => GizmoDragAxis::Z,
                                        _ => GizmoDragAxis::None,
                                    };
                                    self.gizmo_drag_start = [pos.x, pos.y];
                                    self.gizmo_drag_origin = node.position;
                                    gizmo_clicked = true;
                                }
                            }
                        }
                    }

                    // Second check: entity picking.
                    if !gizmo_clicked {
                        let click_local = [pos.x - rect.left(), pos.y - rect.top()];
                        let entities: Vec<(Vec3, f32)> = scene.iter()
                            .filter(|(_, n)| n.visible && n.primitive != Primitive::Empty)
                            .map(|(_, n)| (n.position, n.scale.x.max(n.scale.y).max(n.scale.z)))
                            .collect();

                        let entity_ids: Vec<SceneNodeId> = scene.iter()
                            .filter(|(_, n)| n.visible && n.primitive != Primitive::Empty)
                            .map(|(id, _)| id)
                            .collect();

                        if let Some(pick) = picking::pick_entity(
                            click_local, &entities, &view_proj, vp_w, vp_h,
                        ) {
                            let picked_id = entity_ids[pick.entity_index];
                            let shift = ui.input(|i| i.modifiers.shift);

                            if shift {
                                // Multi-select: toggle entity in selection.
                                if let Some(idx) = self.selected.iter().position(|&id| id == picked_id) {
                                    self.selected.remove(idx);
                                } else {
                                    self.selected.push(picked_id);
                                }
                            } else {
                                // Single select: replace selection.
                                self.selected = vec![picked_id];
                            }
                        } else {
                            // Clicked on empty space: clear selection.
                            let shift = ui.input(|i| i.modifiers.shift);
                            if !shift {
                                self.selected.clear();
                            }
                        }
                    }
                }
            }
        }

        // --- Gizmo drag (move entity along axis) ---
        if self.gizmo_drag != GizmoDragAxis::None {
            if response.dragged_by(egui::PointerButton::Primary) {
                let delta = response.drag_delta();
                if let Some(sel_id) = self.selected.first().copied() {
                    let drag_speed = self.orbit_distance * 0.005;
                    let movement = match self.gizmo_drag {
                        GizmoDragAxis::X => Vec3::new(delta.x * drag_speed, 0.0, 0.0),
                        GizmoDragAxis::Y => Vec3::new(0.0, -delta.y * drag_speed, 0.0),
                        GizmoDragAxis::Z => Vec3::new(0.0, 0.0, delta.x * drag_speed),
                        GizmoDragAxis::None => Vec3::ZERO,
                    };

                    if self.tool == ViewportTool::Move {
                        if let Some(node) = scene.get_mut(sel_id) {
                            node.position += movement;
                        }
                    } else if self.tool == ViewportTool::Scale {
                        if let Some(node) = scene.get_mut(sel_id) {
                            let scale_delta = delta.x * 0.01;
                            match self.gizmo_drag {
                                GizmoDragAxis::X => node.scale.x = (node.scale.x + scale_delta).max(0.01),
                                GizmoDragAxis::Y => node.scale.y = (node.scale.y - delta.y * 0.01).max(0.01),
                                GizmoDragAxis::Z => node.scale.z = (node.scale.z + scale_delta).max(0.01),
                                GizmoDragAxis::None => {}
                            }
                        }
                    }
                }
            }

            // Release drag.
            if response.drag_stopped() || !response.dragged_by(egui::PointerButton::Primary) {
                self.gizmo_drag = GizmoDragAxis::None;
            }
        }

        // --- Camera controls (only when NOT dragging a gizmo) ---
        if self.gizmo_drag == GizmoDragAxis::None {
            match self.mode {
                ViewportMode::View3D => {
                    // Left drag: orbit camera
                    if response.dragged_by(egui::PointerButton::Primary) {
                        let delta = response.drag_delta();
                        self.orbit_yaw += delta.x * 0.008;
                        self.orbit_pitch = (self.orbit_pitch - delta.y * 0.008)
                            .clamp(-1.4, 1.4);
                    }
                    // Middle drag: pan
                    if response.dragged_by(egui::PointerButton::Middle) {
                        let delta = response.drag_delta();
                        let right = self.camera.view_matrix().row(0).truncate();
                        let up = self.camera.view_matrix().row(1).truncate();
                        let pan_speed = self.orbit_distance * 0.003;
                        self.camera.target += Vec3::new(
                            -right.x * delta.x * pan_speed + up.x * delta.y * pan_speed,
                            -right.y * delta.x * pan_speed + up.y * delta.y * pan_speed,
                            -right.z * delta.x * pan_speed + up.z * delta.y * pan_speed,
                        );
                    }
                    // Scroll: zoom
                    if ui.rect_contains_pointer(rect) {
                        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                        if scroll != 0.0 {
                            self.orbit_distance = (self.orbit_distance - scroll * 0.02)
                                .clamp(1.0, 50.0);
                        }
                    }
                    // Update camera position from orbit
                    self.update_orbit_camera();
                }
                ViewportMode::View2D => {
                    // Middle or right drag: pan
                    if response.dragged_by(egui::PointerButton::Middle)
                        || response.dragged_by(egui::PointerButton::Secondary)
                    {
                        self.offset_2d[0] += response.drag_delta().x;
                        self.offset_2d[1] += response.drag_delta().y;
                    }
                    // Scroll: zoom
                    if ui.rect_contains_pointer(rect) {
                        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                        if scroll != 0.0 {
                            self.zoom_2d = (self.zoom_2d + scroll * 0.003).clamp(0.1, 10.0);
                        }
                    }
                }
            }
        }

        // Keyboard shortcuts
        ui.input(|i| {
            if i.key_pressed(egui::Key::Q) { self.tool = ViewportTool::Select; }
            if i.key_pressed(egui::Key::W) { self.tool = ViewportTool::Move; }
            if i.key_pressed(egui::Key::E) { self.tool = ViewportTool::Rotate; }
            if i.key_pressed(egui::Key::R) { self.tool = ViewportTool::Scale; }
            // Cycle render style with Z key
            if i.key_pressed(egui::Key::Z) {
                self.render_style = match self.render_style {
                    RenderStyle::Solid => RenderStyle::Wireframe,
                    RenderStyle::Wireframe => RenderStyle::SolidOnly,
                    RenderStyle::SolidOnly => RenderStyle::Solid,
                };
            }
            // Tab: toggle edit mode
            if i.key_pressed(egui::Key::Tab) {
                self.edit_mode = match self.edit_mode {
                    EditMode::Object => EditMode::Vertex,
                    EditMode::Vertex => EditMode::Object,
                };
            }
        });

        // Double-click to reset view
        if response.double_clicked() {
            match self.mode {
                ViewportMode::View3D => {
                    self.orbit_yaw = std::f32::consts::FRAC_PI_4;
                    self.orbit_pitch = 0.5;
                    self.orbit_distance = 8.0;
                    self.camera.target = Vec3::ZERO;
                    self.update_orbit_camera();
                }
                ViewportMode::View2D => {
                    self.offset_2d = [0.0, 0.0];
                    self.zoom_2d = 1.0;
                }
            }
        }
    }

    /// Toggle between 2D and 3D mode.
    pub fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            ViewportMode::View2D => ViewportMode::View3D,
            ViewportMode::View3D => ViewportMode::View2D,
        };
    }

    // -------------------------------------------------------------------
    // Helpers
    // -------------------------------------------------------------------

    fn update_orbit_camera(&mut self) {
        let x = self.orbit_distance * self.orbit_pitch.cos() * self.orbit_yaw.sin();
        let y = self.orbit_distance * self.orbit_pitch.sin();
        let z = self.orbit_distance * self.orbit_pitch.cos() * self.orbit_yaw.cos();
        self.camera.position = self.camera.target + Vec3::new(x, y, z);
    }

    fn world_to_screen_2d(&self, world: Pos2, rect: Rect) -> Pos2 {
        Pos2::new(
            rect.center().x + world.x * 40.0 * self.zoom_2d + self.offset_2d[0],
            rect.center().y - world.y * 40.0 * self.zoom_2d + self.offset_2d[1],
        )
    }
}
