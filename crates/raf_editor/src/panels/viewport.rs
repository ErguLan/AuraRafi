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

use std::collections::HashMap;

#[path = "viewport_edit.rs"]
mod viewport_edit;
#[path = "viewport_grid.rs"]
mod viewport_grid;
#[path = "viewport_render_software.rs"]
mod viewport_render_software;

use egui::{Color32, Pos2, Rect, Stroke, Ui, Vec2};
use glam::{EulerRot, Mat4, Quat, Vec3};

use raf_core::config::Language;
use raf_core::scene::graph::{NodeColor, Primitive, SceneNode, SceneNodeId};
use raf_core::SceneGraph;
use raf_render::camera::Camera;
use raf_render::depth_sort::{self, DepthSorter};
use raf_render::editable::EditableMesh;
use raf_render::lighting::LightingEnv;
use raf_render::mesh;
use raf_render::picking::{
    self, GIZMO_ARROWS, GIZMO_LINE_WIDTH, GIZMO_ROTATION_RADIUS, GIZMO_ROTATION_SEGMENTS,
};
use raf_render::projection;
use raf_render::render_config::RenderConfig;
use raf_render::software_raster::{
    project_quad_for_raster, rasterize_quad, rasterize_selection_outline, SoftwareFramebuffer,
};

use crate::theme;
use crate::ui_icons::UiIconAtlas;

fn brighten_channel(channel: u8, amount: u8) -> u8 {
    channel.saturating_add(amount)
}

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
    NegX,
    NegY,
    NegZ,
}

/// Rendering style for 3D entities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderStyle {
    /// Fully opaque faces using their base color.
    Solid,
    /// Wireframe only (lightest).
    Wireframe,
    /// Shaded preview with edge overlay.
    Preview,
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
    /// World-space spacing for the editor grid.
    pub grid_spacing: f32,
    /// 3D rendering style.
    pub render_style: RenderStyle,
    /// Whether entity labels should be shown.
    pub show_labels: bool,
    /// Frame-time hint from the outer app, used for adaptive detail.
    pub frame_time_hint: f32,
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
    /// v0.7.0: Per-project render configuration (all features off by default).
    pub render_cfg: RenderConfig,
    /// v0.7.0: Scene lighting environment.
    pub lighting: LightingEnv,
    /// Invert mouse X orbit.
    pub invert_mouse_x: bool,
    /// Invert mouse Y orbit.
    pub invert_mouse_y: bool,
    /// Editor tuning values for gizmo responsiveness.
    pub move_sensitivity: f32,
    pub rotate_sensitivity: f32,
    pub scale_sensitivity: f32,
    pub uniform_scale_by_default: bool,
    gizmo_rotation_origin: Vec3,
    gizmo_scale_origin: Vec3,
    gizmo_last_pointer: [f32; 2],
    editable_meshes: HashMap<SceneNodeId, EditableMesh>,
    edit_drag_active: bool,
    edit_last_pointer: [f32; 2],
    /// Solid mode: show surface edge lines.
    pub solid_show_surface_edges: bool,
    /// Solid mode: x-ray see-through.
    pub solid_xray_mode: bool,
    /// Solid mode: face tonality (directional shading on/off).
    pub solid_face_tonality: bool,
    /// v0.8.0: Software framebuffer for Z-buffer rendering (opt-in).
    sw_framebuffer: Option<SoftwareFramebuffer>,
    /// v0.8.0: egui texture handle for displaying the software framebuffer.
    sw_texture: Option<egui::TextureHandle>,
}

impl Default for ViewportPanel {
    fn default() -> Self {
        Self {
            mode: ViewportMode::View3D,
            camera: Camera::default(),
            tool: ViewportTool::Select,
            selected: Vec::new(),
            grid_visible: true,
            grid_spacing: 1.0,
            render_style: RenderStyle::Solid,
            show_labels: true,
            frame_time_hint: 1.0 / 60.0,
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
            render_cfg: RenderConfig::default(),
            lighting: LightingEnv::default(),
            invert_mouse_x: false,
            invert_mouse_y: true,
            move_sensitivity: 3.5,
            rotate_sensitivity: 3.5,
            scale_sensitivity: 3.5,
            uniform_scale_by_default: false,
            gizmo_rotation_origin: Vec3::ZERO,
            gizmo_scale_origin: Vec3::ONE,
            gizmo_last_pointer: [0.0; 2],
            editable_meshes: HashMap::new(),
            edit_drag_active: false,
            edit_last_pointer: [0.0; 2],
            solid_show_surface_edges: false,
            solid_xray_mode: false,
            solid_face_tonality: true,
            // v0.8.0: Software Z-buffer (created lazily on first frame if depth_accurate=true).
            sw_framebuffer: None,
            sw_texture: None,
        }
    }
}

impl ViewportPanel {
    /// Main draw entry point.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        ui: &mut Ui,
        scene: &mut SceneGraph,
        is_dark: bool,
        lang: Language,
        icons: &UiIconAtlas,
    ) -> bool {
        let rect = ui.available_rect_before_wrap();
        let painter = ui.painter_at(rect);

        // Background: white sky (natural, both themes).
        let bg = Color32::from_rgb(240, 240, 242);
        painter.rect_filled(rect, 0.0, bg);

        match self.mode {
            ViewportMode::View2D => self.draw_2d(&painter, rect, scene, is_dark),
            ViewportMode::View3D => self.draw_3d(ctx, &painter, rect, scene, is_dark),
        }

        // Overlays (toolbar, info, mode toggle)
        self.draw_toolbar(&painter, rect, is_dark, lang, icons);
        self.draw_mode_toggle(&painter, rect, is_dark, lang);
        self.draw_info_overlay(&painter, rect, is_dark);

        // Interaction (includes entity picking, gizmo drag, camera controls)
        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        self.handle_input(ui, &response, rect, scene)
    }

    // -------------------------------------------------------------------
    // 3D rendering (depth-sorted faces + wireframe + gizmo)
    // -------------------------------------------------------------------

    fn draw_3d(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        rect: Rect,
        scene: &SceneGraph,
        is_dark: bool,
    ) {
        let vp_w = rect.width();
        let vp_h = rect.height();
        let view_proj = self.camera.view_projection(vp_w, vp_h);
        let edit_target = if self.edit_mode == EditMode::Vertex {
            self.selected.first().copied()
        } else {
            None
        };
        let use_software_raster = self.should_use_software_raster(edit_target);

        if edit_target.is_none() {
            self.draw_3d_grid(painter, rect, scene, &view_proj, is_dark);
            if !use_software_raster {
                self.draw_axis_gizmo(painter, rect, is_dark);
            }
        } else {
            painter.rect_filled(rect, 0.0, Color32::from_rgba_premultiplied(0, 0, 0, 156));
        }

        let light_dir = Vec3::new(0.5, 0.8, 0.3).normalize();
        if use_software_raster {
            self.draw_3d_software_scene(ctx, painter, rect, scene, &view_proj, light_dir);
            self.depth_sorter.clear();
            if edit_target.is_none() {
                self.draw_axis_gizmo(painter, rect, is_dark);
            }
        } else {
            // -- Phase 1: Collect all faces into depth sorter --
            self.depth_sorter.clear();

            for (id, node) in scene.iter() {
                if !node.visible || node.primitive == Primitive::Empty {
                    continue;
                }
                if let Some(target_id) = edit_target {
                    if id != target_id {
                        continue;
                    }
                }

                let model = scene.world_matrix(id);
                let is_selected = self.selected.contains(&id);
                let distance_to_camera = (self.camera.position - node.position).length();
                let (sphere_stacks, sphere_slices, cylinder_segments, edge_segments) =
                    self.lod_profile(distance_to_camera);
                let editable_mesh = self.ensure_edit_mesh_for_render(
                    id,
                    node,
                    sphere_stacks,
                    sphere_slices,
                    cylinder_segments,
                    edit_target == Some(id),
                );

                if self.render_style != RenderStyle::Wireframe || edit_target == Some(id) {
                    if let Some(edit_mesh) = editable_mesh.as_ref() {
                        for (triangle, normal) in edit_mesh.render_faces() {
                            let brightness = depth_sort::face_brightness(normal, light_dir, &model);
                            let (color, wire, wc, wire_width) = if edit_target == Some(id) {
                                (
                                    depth_sort::shade_color(240, 240, 240, 240, brightness.max(0.55)),
                                    false,
                                    [0, 0, 0, 0],
                                    0.0,
                                )
                            } else {
                                match self.render_style {
                                    RenderStyle::Solid => self.solid_face_draw_style(&node.color, is_selected, brightness),
                                    RenderStyle::Preview => {
                                        let shaded = if is_selected {
                                            depth_sort::shade_color(
                                                theme::ACCENT.r(), theme::ACCENT.g(), theme::ACCENT.b(),
                                                220, brightness,
                                            )
                                        } else {
                                            depth_sort::shade_color(
                                                node.color.r, node.color.g, node.color.b,
                                                210, brightness,
                                            )
                                        };
                                        let wire_color = if is_selected {
                                            [theme::ACCENT.r(), theme::ACCENT.g(), theme::ACCENT.b(), 255]
                                        } else {
                                            [node.color.r, node.color.g, node.color.b, 220]
                                        };
                                        (shaded, true, wire_color, if is_selected { 2.0 } else { 1.0 })
                                    }
                                    RenderStyle::Wireframe => continue,
                                }
                            };

                            self.depth_sorter.add_quad(
                                &[triangle[0], triangle[1], triangle[2], triangle[2]],
                                normal,
                                &model,
                                &view_proj,
                                vp_w,
                                vp_h,
                                color,
                                wire,
                                wc,
                                wire_width,
                            );
                        }
                    } else {
                        let faces: Vec<([Vec3; 4], Vec3)> = match node.primitive {
                            Primitive::Cube => mesh::cube_faces(),
                            Primitive::Sphere => mesh::sphere_faces(sphere_stacks, sphere_slices),
                            Primitive::Plane => mesh::plane_faces(),
                            Primitive::Cylinder => mesh::cylinder_faces(cylinder_segments),
                            Primitive::Sprite2D => mesh::plane_faces(),
                            Primitive::Empty => continue,
                        };

                        for (corners, normal) in &faces {
                            let brightness = depth_sort::face_brightness(*normal, light_dir, &model);

                            let (color, wire, wc, wire_width) = match self.render_style {
                                RenderStyle::Solid => self.solid_face_draw_style(&node.color, is_selected, brightness),
                                RenderStyle::Preview => {
                                    let shaded = if is_selected {
                                        depth_sort::shade_color(
                                            theme::ACCENT.r(), theme::ACCENT.g(), theme::ACCENT.b(),
                                            220, brightness,
                                        )
                                    } else {
                                        depth_sort::shade_color(
                                            node.color.r, node.color.g, node.color.b,
                                            210, brightness,
                                        )
                                    };
                                    let wire_color = if is_selected {
                                        [theme::ACCENT.r(), theme::ACCENT.g(), theme::ACCENT.b(), 255]
                                    } else {
                                        [node.color.r, node.color.g, node.color.b, 220]
                                    };
                                    (shaded, true, wire_color, if is_selected { 2.0 } else { 1.0 })
                                }
                                RenderStyle::Wireframe => continue,
                            };

                            self.depth_sorter.add_quad(
                                corners, *normal, &model, &view_proj,
                                vp_w, vp_h, color, wire, wc, wire_width,
                            );
                        }
                    }
                }

                if self.render_style == RenderStyle::Wireframe && edit_target.is_none() {
                    if let Some(edit_mesh) = editable_mesh.as_ref() {
                        self.draw_editable_wireframe(
                            painter,
                            rect,
                            &model,
                            &view_proj,
                            edit_mesh,
                            &node.color,
                            is_selected,
                            vp_w,
                            vp_h,
                        );
                    } else {
                        self.draw_primitive_wireframe(
                            painter, rect, &model, &view_proj, &node.color,
                            is_selected, node.primitive, edge_segments, vp_w, vp_h,
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
        }

        // -- Phase 5: Entity labels (always on top) --
        if self.show_labels && edit_target.is_none() {
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
        }

        if let Some(target_id) = edit_target {
            if let Some(node) = scene.get(target_id) {
                let distance_to_camera = (self.camera.position - node.position).length();
                let (sphere_stacks, sphere_slices, cylinder_segments, _) = self.lod_profile(distance_to_camera);
                if let Some(edit_mesh) = self.ensure_edit_mesh_for_render(
                    target_id,
                    node,
                    sphere_stacks,
                    sphere_slices,
                    cylinder_segments,
                    true,
                ) {
                    self.draw_edit_mode_overlay(
                        painter,
                        rect,
                        &scene.world_matrix(target_id),
                        &view_proj,
                        &edit_mesh,
                        vp_w,
                        vp_h,
                    );
                }
            }
        }

        // -- Phase 6: Transform gizmo (on top of everything) --
        if edit_target.is_none() && self.tool != ViewportTool::Select {
            if let Some(sel_id) = self.selected.first().copied() {
                if let Some(node) = scene.get(sel_id) {
                    if self.tool == ViewportTool::Rotate {
                        self.draw_gizmo_circles(painter, rect, node.position, &view_proj, vp_w, vp_h);
                    } else if self.tool == ViewportTool::Scale {
                        self.draw_scale_handles(painter, rect, node, &view_proj, vp_w, vp_h);
                    } else {
                        self.draw_gizmo_arrows(painter, rect, node.position, &view_proj, vp_w, vp_h);
                    }
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

    /// Draw rotation gizmo as three colored circles (one per axis).
    fn draw_gizmo_circles(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        entity_pos: Vec3,
        view_proj: &Mat4,
        vp_w: f32,
        vp_h: f32,
    ) {
        let radius = GIZMO_ROTATION_RADIUS;
        let segments = GIZMO_ROTATION_SEGMENTS;
        let axis_configs: [(Vec3, Vec3, Color32, &str, GizmoDragAxis); 3] = [
            (Vec3::Y, Vec3::Z, Color32::from_rgb(220, 70, 70), "X", GizmoDragAxis::X),    // YZ plane -> rotate X
            (Vec3::X, Vec3::Z, Color32::from_rgb(70, 200, 70), "Y", GizmoDragAxis::Y),    // XZ plane -> rotate Y
            (Vec3::X, Vec3::Y, Color32::from_rgb(70, 100, 220), "Z", GizmoDragAxis::Z),   // XY plane -> rotate Z
        ];

        for (axis_a, axis_b, color, label, drag_axis) in &axis_configs {
            let is_dragging = self.gizmo_drag == *drag_axis;
            let line_w = if is_dragging { 2.5 } else { 1.5 };
            let draw_color = if is_dragging {
                Color32::from_rgb(
                    color.r().saturating_add(40),
                    color.g().saturating_add(40),
                    color.b().saturating_add(40),
                )
            } else {
                *color
            };

            let mut points: Vec<Pos2> = Vec::with_capacity(segments + 1);
            for i in 0..=segments {
                let angle = (i as f32 / segments as f32) * std::f32::consts::TAU;
                let world_pt = entity_pos
                    + *axis_a * (angle.cos() * radius)
                    + *axis_b * (angle.sin() * radius);
                if let Some(screen) = projection::project_point(world_pt, view_proj, vp_w, vp_h) {
                    points.push(Pos2::new(rect.left() + screen[0], rect.top() + screen[1]));
                }
            }

            // Draw as polyline segments.
            if points.len() > 1 {
                for pair in points.windows(2) {
                    painter.line_segment(
                        [pair[0], pair[1]],
                        Stroke::new(line_w, draw_color),
                    );
                }
            }

            // Axis label near the top of the ring.
            let label_world = entity_pos + *axis_a * radius * 1.15;
            if let Some(lbl_screen) = projection::project_point(label_world, view_proj, vp_w, vp_h) {
                painter.text(
                    Pos2::new(rect.left() + lbl_screen[0], rect.top() + lbl_screen[1]),
                    egui::Align2::CENTER_CENTER,
                    *label,
                    egui::FontId::proportional(9.0),
                    draw_color,
                );
            }
        }
    }

    fn draw_scale_handles(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        node: &SceneNode,
        view_proj: &Mat4,
        vp_w: f32,
        vp_h: f32,
    ) {
        let quat = rotation_quat(node.rotation);
        for handle in Self::scale_handles() {
            let axis = handle.axis();
            let sign = handle.sign();
            let offset = Vec3::new(
                axis.x.abs() * node.scale.x * 0.5 * sign,
                axis.y.abs() * node.scale.y * 0.5 * sign,
                axis.z.abs() * node.scale.z * 0.5 * sign,
            );
            let world_pos = node.position + quat * offset;
            let Some(screen) = projection::project_point(world_pos, view_proj, vp_w, vp_h) else {
                continue;
            };

            let center = Pos2::new(rect.left() + screen[0], rect.top() + screen[1]);
            let is_active = self.gizmo_drag == handle;
            let fill = if is_active {
                Color32::from_rgb(255, 181, 74)
            } else {
                Color32::from_rgba_premultiplied(212, 119, 26, 220)
            };
            let stroke = if is_active {
                Stroke::new(2.0, Color32::WHITE)
            } else {
                Stroke::new(1.2, Color32::from_rgba_premultiplied(255, 244, 220, 220))
            };
            let radius = if is_active { 7.0 } else { 5.5 };

            painter.circle_filled(center, radius, fill);
            painter.circle_stroke(center, radius, stroke);
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
        edge_segments: usize,
        vp_w: f32,
        vp_h: f32,
    ) {
        let edges = match primitive {
            Primitive::Cube => mesh::cube_edges(),
            Primitive::Sphere => mesh::sphere_edges(edge_segments),
            Primitive::Plane => mesh::plane_edges(),          // 5 edges
            Primitive::Cylinder => mesh::cylinder_edges(edge_segments),
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
            if !node.visible || node.primitive != Primitive::Sprite2D {
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

            if self.show_labels {
                painter.text(
                    Pos2::new(screen_pos.x, entity_rect.top() - 4.0),
                    egui::Align2::CENTER_BOTTOM,
                    &node.name,
                    egui::FontId::proportional(10.0),
                    if is_selected { theme::ACCENT } else { theme::DARK_TEXT_DIM },
                );
            }
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

    fn lod_profile(&self, distance_to_camera: f32) -> (usize, usize, usize, usize) {
        let under_pressure = self.frame_time_hint > (1.0 / 40.0);

        if distance_to_camera > 40.0 || under_pressure {
            (3, 4, 6, 8)
        } else if distance_to_camera > 18.0 {
            (4, 6, 8, 10)
        } else if distance_to_camera > 8.0 {
            (5, 8, 10, 12)
        } else {
            (6, 10, 12, 14)
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
        icons: &UiIconAtlas,
    ) {
        // Horizontal toolbar at top-left (icon buttons with key hints).
        let tools: [(ViewportTool, &str, &str); 4] = [
            (ViewportTool::Select, "select.png", "Q"),
            (ViewportTool::Move, "move.png", "T"),
            (ViewportTool::Rotate, "rotate.png", "E"),
            (ViewportTool::Scale, "focus.png", "R"),
        ];

        let btn_size = 26.0;
        let gap = 2.0;
        let x_start = rect.left() + 8.0;
        let y = rect.top() + 8.0;
        let total_w = (btn_size + gap) * tools.len() as f32 - gap;

        // Group background pill.
        let group_rect = Rect::from_min_size(
            Pos2::new(x_start - 3.0, y - 3.0),
            Vec2::new(total_w + 6.0, btn_size + 6.0),
        );
        let group_bg = if is_dark {
            Color32::from_rgba_premultiplied(20, 20, 22, 210)
        } else {
            Color32::from_rgba_premultiplied(235, 235, 238, 230)
        };
        painter.rect_filled(group_rect, 6.0, group_bg);
        let group_border = if is_dark {
            Color32::from_rgba_premultiplied(60, 60, 65, 100)
        } else {
            Color32::from_rgba_premultiplied(180, 180, 185, 100)
        };
        painter.rect_stroke(group_rect, 6.0, Stroke::new(0.5, group_border));

        for (i, (tool, icon_name, key)) in tools.iter().enumerate() {
            let bx = x_start + (i as f32 * (btn_size + gap));
            let btn_rect = Rect::from_min_size(Pos2::new(bx, y), Vec2::splat(btn_size));
            let is_active = self.tool == *tool;

            if is_active {
                let active_bg = if is_dark {
                    Color32::from_rgba_premultiplied(55, 55, 60, 220)
                } else {
                    Color32::from_rgba_premultiplied(200, 200, 205, 220)
                };
                painter.rect_filled(btn_rect, 4.0, active_bg);
                // Bottom accent dot.
                painter.circle_filled(
                    Pos2::new(btn_rect.center().x, btn_rect.bottom() - 1.5),
                    2.0,
                    Color32::from_rgb(212, 119, 26),
                );
            }

            // Icon.
            let icon_alpha = if is_active { 255 } else if is_dark { 180 } else { 210 };
            let icon_rect = Rect::from_center_size(btn_rect.center(), Vec2::new(16.0, 16.0));
            if !icons.paint(
                painter,
                icon_name,
                icon_rect,
                Color32::from_white_alpha(icon_alpha),
            ) {
                let fallback = match tool {
                    ViewportTool::Select => "Q",
                    ViewportTool::Move => "T",
                    ViewportTool::Rotate => "E",
                    ViewportTool::Scale => "R",
                };
                painter.text(
                    Pos2::new(btn_rect.center().x, btn_rect.center().y - 1.0),
                    egui::Align2::CENTER_CENTER,
                    fallback,
                    egui::FontId::proportional(11.0),
                    Color32::from_white_alpha(icon_alpha),
                );
            }

            // Key hint (tiny, top-right corner).
            let key_color = if is_dark {
                Color32::from_rgba_premultiplied(90, 90, 95, 140)
            } else {
                Color32::from_rgba_premultiplied(140, 140, 145, 140)
            };
            painter.text(
                Pos2::new(btn_rect.right() - 2.0, btn_rect.top() + 2.0),
                egui::Align2::RIGHT_TOP,
                *key,
                egui::FontId::proportional(7.0),
                key_color,
            );
        }

        // Edit mode indicator (right of toolbar).
        let mode_x = x_start + total_w + 10.0;
        let mode_bg = if self.edit_mode == EditMode::Vertex {
            Color32::from_rgba_premultiplied(212, 119, 26, 60)
        } else {
            Color32::from_rgba_premultiplied(40, 40, 45, 160)
        };
        let mode_rect = Rect::from_min_size(
            Pos2::new(mode_x, y + 3.0),
            Vec2::new(30.0, btn_size - 6.0),
        );
        painter.rect_filled(mode_rect, 3.0, mode_bg);
        let mode_color = if self.edit_mode == EditMode::Vertex {
            Color32::from_rgb(212, 119, 26)
        } else if is_dark {
            Color32::from_rgb(130, 130, 135)
        } else {
            Color32::from_rgb(80, 80, 85)
        };
        let mode_icon = match self.edit_mode {
            EditMode::Object => "object_mode.png",
            EditMode::Vertex => "vertex_mode.png",
        };
        let icon_rect = Rect::from_center_size(mode_rect.center(), Vec2::new(16.0, 16.0));
        if !icons.paint(painter, mode_icon, icon_rect, mode_color) {
            let fallback = match self.edit_mode {
                EditMode::Object => "OBJ",
                EditMode::Vertex => "VTX",
            };
            painter.text(
                mode_rect.center(),
                egui::Align2::CENTER_CENTER,
                fallback,
                egui::FontId::proportional(8.0),
                mode_color,
            );
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

    fn handle_input(&mut self, ui: &mut Ui, response: &egui::Response, rect: Rect, scene: &mut SceneGraph) -> bool {
        let is_hovered = ui.rect_contains_pointer(rect);
        let mut scene_changed = false;
        let edit_mode_active = self.mode == ViewportMode::View3D && self.edit_mode == EditMode::Vertex;

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

        // Click on tool buttons (horizontal at top-left).
        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let btn_size = 26.0;
                let gap = 2.0;
                let x_start = rect.left() + 8.0;
                let y = rect.top() + 8.0;
                let tools = [ViewportTool::Select, ViewportTool::Move, ViewportTool::Rotate, ViewportTool::Scale];
                for (i, tool) in tools.iter().enumerate() {
                    let bx = x_start + (i as f32 * (btn_size + gap));
                    let btn_rect = Rect::from_min_size(Pos2::new(bx, y), Vec2::splat(btn_size));
                    if btn_rect.contains(pos) {
                        self.tool = *tool;
                    }
                }
            }
        }

        if edit_mode_active {
            scene_changed |= self.handle_edit_mode_input(ui, response, rect, scene);
        }

        // =====================================================================
        // GIZMO DRAG: detect on first frame of left-drag (NOT on clicked()).
        // In egui, clicked() and dragged() are mutually exclusive!
        // =====================================================================
        if !edit_mode_active && self.mode == ViewportMode::View3D && self.gizmo_drag == GizmoDragAxis::None {
            if response.dragged_by(egui::PointerButton::Primary)
                && !ui.input(|i| i.modifiers.alt)
            {
                if let Some(pos) = response.interact_pointer_pos() {
                    // Only start gizmo from viewport area (not overlays).
                    let overlay_margin = 40.0;
                    let in_viewport = pos.x > rect.left() + overlay_margin
                        && pos.y > rect.top() + overlay_margin;

                    if in_viewport && self.tool != ViewportTool::Select {
                        if let Some(sel_id) = self.selected.first().copied() {
                            if let Some(node) = scene.get(sel_id) {
                                let vp_w = rect.width();
                                let vp_h = rect.height();
                                let view_proj = self.camera.view_projection(vp_w, vp_h);
                                let click_local = [pos.x - rect.left(), pos.y - rect.top()];
                                let picked_axis = if self.tool == ViewportTool::Rotate {
                                    picking::pick_gizmo_rotation_ring(
                                        click_local,
                                        node.position,
                                        &view_proj,
                                        vp_w,
                                        vp_h,
                                    )
                                } else if self.tool == ViewportTool::Scale {
                                    self.pick_scale_handle(node, click_local, &view_proj, vp_w, vp_h)
                                } else {
                                    picking::pick_gizmo_arrow(
                                        click_local,
                                        node.position,
                                        &view_proj,
                                        vp_w,
                                        vp_h,
                                    )
                                };

                                if let Some((axis_idx, _dist)) = picked_axis {
                                    self.gizmo_drag = if self.tool == ViewportTool::Scale {
                                        // pick_scale_handle returns index into scale_handles():
                                        // [X, NegX, Y, NegY, Z, NegZ]
                                        Self::scale_handles()
                                            .get(axis_idx)
                                            .copied()
                                            .unwrap_or(GizmoDragAxis::None)
                                    } else {
                                        // pick_gizmo_arrow / pick_gizmo_rotation_ring return 0=X 1=Y 2=Z
                                        match axis_idx {
                                            0 => GizmoDragAxis::X,
                                            1 => GizmoDragAxis::Y,
                                            2 => GizmoDragAxis::Z,
                                            _ => GizmoDragAxis::None,
                                        }
                                    };
                                    self.gizmo_drag_start = [pos.x, pos.y];
                                    self.gizmo_last_pointer = [pos.x, pos.y];
                                    self.gizmo_drag_origin = node.position;
                                    self.gizmo_rotation_origin = node.rotation;
                                    self.gizmo_scale_origin = node.scale;
                                }
                            }
                        }
                    }
                }
            }
        }

        // =====================================================================
        // GIZMO DRAG: apply movement while dragging.
        // =====================================================================
        if !edit_mode_active && self.gizmo_drag != GizmoDragAxis::None {
            if response.dragged_by(egui::PointerButton::Primary) {
                if let Some(pointer) = response.interact_pointer_pos() {
                    let frame_delta = Vec2::new(
                        pointer.x - self.gizmo_last_pointer[0],
                        pointer.y - self.gizmo_last_pointer[1],
                    );
                    self.gizmo_last_pointer = [pointer.x, pointer.y];

                    if let Some(sel_id) = self.selected.first().copied() {
                        let drag_speed = self.orbit_distance * 0.0015 * self.move_sensitivity.max(0.1);

                        if self.tool == ViewportTool::Move {
                            if let Some(node) = scene.get_mut(sel_id) {
                                match self.gizmo_drag {
                                    GizmoDragAxis::X => node.position.x += frame_delta.x * drag_speed,
                                    GizmoDragAxis::Y => node.position.y += -frame_delta.y * drag_speed,
                                    GizmoDragAxis::Z => {
                                        node.position.z += (frame_delta.x - frame_delta.y) * 0.5 * drag_speed
                                    }
                                    GizmoDragAxis::None | GizmoDragAxis::NegX | GizmoDragAxis::NegY | GizmoDragAxis::NegZ => {}
                                }
                                scene_changed = true;
                            }
                        } else if self.tool == ViewportTool::Scale {
                            if let Some(node) = scene.get_mut(sel_id) {
                                if let Some((axis, sign)) = scale_drag_axis(self.gizmo_drag) {
                                    let quat = rotation_quat(node.rotation);
                                    let world_axis = (quat * axis).normalize_or_zero();
                                    let vp_w = rect.width();
                                    let vp_h = rect.height();
                                    let handle_screen = project_direction_to_screen(world_axis, &self.camera, vp_w, vp_h);
                                    let scale_speed = self.orbit_distance * 0.0015 * self.scale_sensitivity.max(0.1);
                                    let drag_amount = if handle_screen.length_sq() > 0.0001 {
                                        let dir = handle_screen.normalized();
                                        (frame_delta.x * dir.x + frame_delta.y * dir.y) * scale_speed
                                    } else {
                                        (frame_delta.x - frame_delta.y) * 0.5 * scale_speed
                                    };

                                    apply_face_scale(node, axis, sign, drag_amount);
                                    scene_changed = true;
                                }
                            }
                        } else if self.tool == ViewportTool::Rotate {
                            if let Some(node) = scene.get_mut(sel_id) {
                                let rot_speed = self.rotate_sensitivity.max(0.1);
                                // Project entity center to screen to get reference point.
                                let vp_w = rect.width();
                                let vp_h = rect.height();
                                let view_proj = self.camera.view_projection(vp_w, vp_h);
                                let prev_ptr = self.gizmo_last_pointer;

                                if let Some(center_screen) = projection::project_point(
                                    self.gizmo_drag_origin, &view_proj, vp_w, vp_h,
                                ) {
                                    // Angle from entity center to previous cursor position.
                                    let prev_angle = (prev_ptr[1] - center_screen[1])
                                        .atan2(prev_ptr[0] - center_screen[0]);
                                    let curr_ptr = [pointer.x - rect.left(), pointer.y - rect.top()];
                                    // Angle from entity center to current cursor position.
                                    let curr_angle = (curr_ptr[1] - center_screen[1])
                                        .atan2(curr_ptr[0] - center_screen[0]);
                                    // Angular delta with wrap-around handling.
                                    let mut delta_angle = curr_angle - prev_angle;
                                    if delta_angle > std::f32::consts::PI {
                                        delta_angle -= std::f32::consts::TAU;
                                    } else if delta_angle < -std::f32::consts::PI {
                                        delta_angle += std::f32::consts::TAU;
                                    }
                                    let delta_angle = delta_angle * rot_speed;
                                    match self.gizmo_drag {
                                        GizmoDragAxis::X => node.rotation.x += delta_angle,
                                        GizmoDragAxis::Y => node.rotation.y -= delta_angle,
                                        GizmoDragAxis::Z => node.rotation.z += delta_angle,
                                        GizmoDragAxis::None | GizmoDragAxis::NegX | GizmoDragAxis::NegY | GizmoDragAxis::NegZ => {}
                                    }
                                } else {
                                    // Fallback when entity center is off-screen.
                                    let rot_speed_fallback = 0.025 * rot_speed;
                                    match self.gizmo_drag {
                                        GizmoDragAxis::X => node.rotation.x -= frame_delta.y * rot_speed_fallback,
                                        GizmoDragAxis::Y => node.rotation.y += frame_delta.x * rot_speed_fallback,
                                        GizmoDragAxis::Z => node.rotation.z += frame_delta.x * rot_speed_fallback,
                                        GizmoDragAxis::None | GizmoDragAxis::NegX | GizmoDragAxis::NegY | GizmoDragAxis::NegZ => {}
                                    }
                                }
                                scene_changed = true;
                            }
                        }
                    }
                }
            }

            // Release drag.
            if response.drag_stopped() || !response.dragged_by(egui::PointerButton::Primary) {
                self.gizmo_drag = GizmoDragAxis::None;
                self.gizmo_last_pointer = [0.0; 2];
            }
        }

        // =====================================================================
        // Entity picking (click OR small drag-stop to select).
        // `response.clicked()` fails if mouse moves 2+ px between press/release.
        // Also accept drag_stopped() with total delta < 8px as a valid click.
        // =====================================================================
        let is_pick_event = !edit_mode_active
            && self.mode == ViewportMode::View3D
            && self.gizmo_drag == GizmoDragAxis::None
            && (response.clicked()
                || (response.drag_stopped()
                    && {
                        let d = response.drag_delta();
                        (d.x * d.x + d.y * d.y).sqrt() < 8.0
                    }));

        if is_pick_event {
            if let Some(pos) = response.interact_pointer_pos() {
                // Exclude overlay areas from picking.
                let toolbar_rect = Rect::from_min_size(
                    Pos2::new(rect.left(), rect.top()),
                    Vec2::new(160.0, 40.0), // Horizontal toolbar area
                );
                let mode_rect = Rect::from_center_size(
                    Pos2::new(rect.center().x, rect.top() + 19.0),
                    Vec2::new(80.0, 30.0),
                );

                if !toolbar_rect.contains(pos) && !mode_rect.contains(pos) {
                    let vp_w = rect.width();
                    let vp_h = rect.height();
                    let view_proj = self.camera.view_projection(vp_w, vp_h);
                    let click_local = [pos.x - rect.left(), pos.y - rect.top()];

                    let entities: Vec<(Vec3, f32)> = scene.iter()
                        .filter(|(_, n)| n.visible && n.primitive != Primitive::Empty)
                        .map(|(_, n)| {
                            // Bounding sphere: use actual diagonal of AABB (vertices at ±0.5
                            // * scale), gives tighter fit than max(axis).
                            let r = (n.scale.x * n.scale.x
                                + n.scale.y * n.scale.y
                                + n.scale.z * n.scale.z)
                                .sqrt()
                                * 0.5 * 1.2; // 20% margin
                            (n.position, r.max(0.6))
                        })
                        .collect();

                    let entity_ids: Vec<SceneNodeId> = scene.iter()
                        .filter(|(_, n)| n.visible && n.primitive != Primitive::Empty)
                        .map(|(id, _)| id)
                        .collect();

                    let screen_pick = picking::pick_entity(
                        click_local, &entities, &view_proj, vp_w, vp_h,
                    );
                    let ray_pick = picking::pick_entity_ray(
                        click_local, &entities, &view_proj, vp_w, vp_h,
                    );

                    let picked_index = ray_pick
                        .map(|pick| pick.entity_index)
                        .or_else(|| screen_pick.map(|pick| pick.entity_index));

                    if let Some(entity_index) = picked_index {
                        let picked_id = entity_ids[entity_index];
                        let shift = ui.input(|i| i.modifiers.shift);

                        if shift {
                            if let Some(idx) = self.selected.iter().position(|&id| id == picked_id) {
                                self.selected.remove(idx);
                            } else {
                                self.selected.push(picked_id);
                            }
                        } else {
                            self.selected = vec![picked_id];
                        }
                    } else {
                        let shift = ui.input(|i| i.modifiers.shift);
                        if !shift {
                            self.selected.clear();
                        }
                    }
                }
            }
        }

        // =====================================================================
        // Camera controls (only when NOT dragging a gizmo).
        // =====================================================================
        if self.gizmo_drag == GizmoDragAxis::None {
            match self.mode {
                ViewportMode::View3D => {
                    // Orbit camera with RMB drag or Alt+LMB drag.
                    let alt_orbit_drag = ui.input(|i| i.modifiers.alt)
                        && response.dragged_by(egui::PointerButton::Primary);
                    if response.dragged_by(egui::PointerButton::Secondary) || alt_orbit_drag {
                        let delta = response.drag_delta();
                        let mx = if self.invert_mouse_x { 1.0 } else { -1.0 };
                        let my = if self.invert_mouse_y { 1.0 } else { -1.0 };
                        self.orbit_yaw += delta.x * 0.008 * mx;
                        self.orbit_pitch = (self.orbit_pitch + delta.y * 0.008 * my)
                            .clamp(-1.4, 1.4);
                    }
                    // Middle drag: pan disabled while editing mesh inline.
                    if !edit_mode_active && response.dragged_by(egui::PointerButton::Middle) {
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
                    if is_hovered {
                        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                        if scroll != 0.0 {
                            self.orbit_distance = (self.orbit_distance - scroll * 0.02)
                                .clamp(0.5, 100.0);
                        }
                    }

                    // WASD fly movement only while RMB navigation is active and not in inline mesh edit mode.
                    let fly_navigation_active = ui.input(|i| i.pointer.button_down(egui::PointerButton::Secondary));
                    if !edit_mode_active && is_hovered && fly_navigation_active {
                        let speed = (self.orbit_distance * 0.04).max(0.06);
                        let forward = (self.camera.target - self.camera.position).normalize();
                        let right = forward.cross(Vec3::Y).normalize();
                        ui.input(|i| {
                            if i.key_down(egui::Key::A) {
                                self.camera.target -= right * speed;
                            }
                            if i.key_down(egui::Key::D) {
                                self.camera.target += right * speed;
                            }
                            if i.key_down(egui::Key::S) {
                                self.camera.target -= forward * speed;
                            }
                            if i.key_down(egui::Key::W) {
                                self.camera.target += forward * speed;
                            }
                            if i.key_down(egui::Key::Space) {
                                self.camera.target.y += speed;
                            }
                            if i.key_down(egui::Key::C) {
                                self.camera.target.y -= speed;
                            }
                        });
                    }

                    if edit_mode_active {
                        if let Some(sel_id) = self.selected.first().copied() {
                            if let Some(node) = scene.get(sel_id) {
                                self.camera.target = node.position;
                            }
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
                    if is_hovered {
                        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                        if scroll != 0.0 {
                            self.zoom_2d = (self.zoom_2d + scroll * 0.003).clamp(0.1, 10.0);
                        }
                    }
                }
            }
        }

        // Keyboard shortcuts (only on key_pressed, not key_down).
        let mut toggle_edit_mode = false;
        if is_hovered {
            ui.input(|i| {
                if i.key_pressed(egui::Key::Q) { self.tool = ViewportTool::Select; }
                if i.key_pressed(egui::Key::T) { self.tool = ViewportTool::Move; }
                if i.key_pressed(egui::Key::E) { self.tool = ViewportTool::Rotate; }
                if i.key_pressed(egui::Key::R) { self.tool = ViewportTool::Scale; }
                if i.key_pressed(egui::Key::Tab) {
                    toggle_edit_mode = true;
                }
                // F key: focus camera on selected entity.
                if i.key_pressed(egui::Key::F) {
                    if let Some(sel_id) = self.selected.first().copied() {
                        if let Some(node) = scene.get(sel_id) {
                            self.camera.target = node.position;
                            let max_dim = node.scale.x.max(node.scale.y).max(node.scale.z);
                            self.orbit_distance = (max_dim * 3.0).clamp(2.0, 30.0);
                        }
                    }
                }
            });
        }

        if toggle_edit_mode {
            self.toggle_edit_mode(scene);
        }

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

        scene_changed
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

fn rotation_quat(rotation: Vec3) -> Quat {
    Quat::from_euler(
        EulerRot::YXZ,
        rotation.y.to_radians(),
        rotation.x.to_radians(),
        rotation.z.to_radians(),
    )
}

fn project_direction_to_screen(world_axis: Vec3, camera: &Camera, vp_w: f32, vp_h: f32) -> Vec2 {
    let origin = camera.target;
    let Some(screen_origin) = projection::project_point(origin, &camera.view_projection(vp_w, vp_h), vp_w, vp_h) else {
        return Vec2::ZERO;
    };
    let Some(screen_axis) = projection::project_point(origin + world_axis, &camera.view_projection(vp_w, vp_h), vp_w, vp_h) else {
        return Vec2::ZERO;
    };
    Vec2::new(screen_axis[0] - screen_origin[0], screen_axis[1] - screen_origin[1])
}

fn apply_face_scale(node: &mut SceneNode, axis: Vec3, sign: f32, delta: f32) {
    let old_scale = node.scale;
    let mut new_scale = old_scale;
    if axis.x.abs() > 0.5 {
        new_scale.x = (new_scale.x + delta).max(0.01);
    }
    if axis.y.abs() > 0.5 {
        new_scale.y = (new_scale.y + delta).max(0.01);
    }
    if axis.z.abs() > 0.5 {
        new_scale.z = (new_scale.z + delta).max(0.01);
    }

    let applied_delta = Vec3::new(
        (new_scale.x - old_scale.x) * axis.x.abs(),
        (new_scale.y - old_scale.y) * axis.y.abs(),
        (new_scale.z - old_scale.z) * axis.z.abs(),
    );

    node.scale = new_scale;

    let quat = rotation_quat(node.rotation);
    let shift_local = Vec3::new(
        applied_delta.x * 0.5 * sign * axis.x.signum(),
        applied_delta.y * 0.5 * sign * axis.y.signum(),
        applied_delta.z * 0.5 * sign * axis.z.signum(),
    );
    node.position += quat * shift_local;
}

fn scale_drag_axis(handle: GizmoDragAxis) -> Option<(Vec3, f32)> {
    match handle {
        GizmoDragAxis::X => Some((Vec3::X, 1.0)),
        GizmoDragAxis::Y => Some((Vec3::Y, 1.0)),
        GizmoDragAxis::Z => Some((Vec3::Z, 1.0)),
        GizmoDragAxis::NegX => Some((Vec3::X, -1.0)),
        GizmoDragAxis::NegY => Some((Vec3::Y, -1.0)),
        GizmoDragAxis::NegZ => Some((Vec3::Z, -1.0)),
        GizmoDragAxis::None => None,
    }
}

impl ViewportPanel {
    fn scale_handles() -> [GizmoDragAxis; 6] {
        [
            GizmoDragAxis::X,
            GizmoDragAxis::NegX,
            GizmoDragAxis::Y,
            GizmoDragAxis::NegY,
            GizmoDragAxis::Z,
            GizmoDragAxis::NegZ,
        ]
    }

    fn pick_scale_handle(
        &self,
        node: &SceneNode,
        click_local: [f32; 2],
        view_proj: &Mat4,
        vp_w: f32,
        vp_h: f32,
    ) -> Option<(usize, f32)> {
        let quat = rotation_quat(node.rotation);
        let mut best: Option<(usize, f32)> = None;

        for (index, handle) in Self::scale_handles().iter().enumerate() {
            let axis = handle.axis();
            let sign = handle.sign();
            let offset = Vec3::new(
                axis.x.abs() * node.scale.x * 0.5 * sign,
                axis.y.abs() * node.scale.y * 0.5 * sign,
                axis.z.abs() * node.scale.z * 0.5 * sign,
            );
            let world_pos = node.position + quat * offset;
            let Some(screen) = projection::project_point(world_pos, view_proj, vp_w, vp_h) else {
                continue;
            };
            let dx = screen[0] - click_local[0];
            let dy = screen[1] - click_local[1];
            let distance = (dx * dx + dy * dy).sqrt();
            if distance > 12.0 {
                continue;
            }

            let is_better = match best {
                None => true,
                Some((_, best_distance)) => distance < best_distance,
            };
            if is_better {
                best = Some((index, distance));
            }
        }

        best
    }
}

impl GizmoDragAxis {
    fn axis(self) -> Vec3 {
        match self {
            GizmoDragAxis::X | GizmoDragAxis::NegX => Vec3::X,
            GizmoDragAxis::Y | GizmoDragAxis::NegY => Vec3::Y,
            GizmoDragAxis::Z | GizmoDragAxis::NegZ => Vec3::Z,
            GizmoDragAxis::None => Vec3::ZERO,
        }
    }

    fn sign(self) -> f32 {
        match self {
            GizmoDragAxis::NegX | GizmoDragAxis::NegY | GizmoDragAxis::NegZ => -1.0,
            GizmoDragAxis::X | GizmoDragAxis::Y | GizmoDragAxis::Z => 1.0,
            GizmoDragAxis::None => 0.0,
        }
    }
}
