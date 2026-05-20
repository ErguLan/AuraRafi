//! 3D Viewport panel.
//!
//! Bridges the editor UI (egui) with the raf_render scene renderer.
//! Responsibilities:
//! - Orbit camera input (mouse drag, scroll zoom)
//! - Upload pixel buffer from SceneRenderer to egui texture
//! - 2D editor grid overlay
//! - Entity picking (click-to-select)
//!
//! All rendering logic lives in raf_render. This module only handles
//! input and egui integration.

#[path = "viewport_grid.rs"]
mod viewport_grid;
#[path = "viewport_hud.rs"]
mod viewport_hud;
#[path = "viewport_interaction.rs"]
mod viewport_interaction;
#[path = "viewport_overlay.rs"]
mod viewport_overlay;

use egui::{Color32, Pos2, Rect, Stroke};
use glam::{Mat4, Vec3};
use std::time::{Duration, Instant};

use raf_core::config::Language;
use raf_core::scene::graph::{SceneGraph, SceneNodeId, Primitive};
use raf_render::bridge::{ViewportBridge, ViewportNavigationConfig, ViewportPointerInput};
use raf_render::gizmo::GizmoMode;
use raf_render::render_config::RenderConfig;
use raf_render::scene_renderer::RenderOptions as SceneRenderOptions;

use crate::ui_icons::UiIconAtlas;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewportMode {
    View2D,
    View3D,
}

impl Default for ViewportMode {
    fn default() -> Self {
        Self::View3D
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditMode {
    Object,
    Vertex,
}

impl Default for EditMode {
    fn default() -> Self {
        Self::Object
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderStyle {
    Solid,
    Wireframe,
    Preview,
}

impl Default for RenderStyle {
    fn default() -> Self {
        Self::Solid
    }
}

pub struct ViewportPanel {
    pub mode: ViewportMode,
    pub selected: Vec<SceneNodeId>,
    pub grid_visible: bool,
    pub grid_spacing: f32,
    pub grid_load_distance: f32,
    pub render_style: RenderStyle,
    pub show_labels: bool,
    pub frame_time_hint: f32,
    pub fps_limit: u32,
    pub edit_mode: EditMode,
    pub render_cfg: RenderConfig,
    pub invert_mouse_x: bool,
    pub invert_mouse_y: bool,
    pub move_sensitivity: f32,
    pub rotate_sensitivity: f32,
    pub scale_sensitivity: f32,
    pub uniform_scale_by_default: bool,
    pub solid_show_surface_edges: bool,
    pub solid_xray_mode: bool,
    pub solid_face_tonality: bool,

    // 2D mode state
    bridge: ViewportBridge,
    texture: Option<egui::TextureHandle>,
    last_size: [u32; 2],
    render_cpu_ms: f32,
    upload_cpu_ms: f32,
    adaptive_render_scale: f32,
    interaction_linger_s: f32,
}

impl Default for ViewportPanel {
    fn default() -> Self {
        Self {
            mode: ViewportMode::View3D,
            selected: Vec::new(),
            grid_visible: true,
            grid_spacing: 1.0,
            grid_load_distance: 15.0,
            render_style: RenderStyle::Solid,
            show_labels: true,
            frame_time_hint: 1.0 / 60.0,
            fps_limit: 60,
            edit_mode: EditMode::Object,
            render_cfg: RenderConfig::default(),
            invert_mouse_x: false,
            invert_mouse_y: true,
            move_sensitivity: 3.5,
            rotate_sensitivity: 3.5,
            scale_sensitivity: 3.5,
            uniform_scale_by_default: false,
            solid_show_surface_edges: false,
            solid_xray_mode: false,
            solid_face_tonality: true,

            bridge: ViewportBridge::default(),
            texture: None,
            last_size: [1, 1],
            render_cpu_ms: 0.0,
            upload_cpu_ms: 0.0,
            adaptive_render_scale: 1.0,
            interaction_linger_s: 0.0,
        }
    }
}

impl ViewportPanel {
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        scene: &mut SceneGraph,
        is_dark: bool,
        _lang: Language,
        icons: &UiIconAtlas,
    ) -> bool {
        let rect = ui.available_rect_before_wrap();
        let vp_w = rect.width().max(1.0);
        let vp_h = rect.height().max(1.0);

        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        let viewport_keys = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Tab),
                i.key_pressed(egui::Key::Num2),
                i.key_pressed(egui::Key::Num3),
            )
        });

        if response.hovered() {
            if viewport_keys.0 {
                self.toggle_edit_mode(scene);
            }
            if viewport_keys.1 {
                self.mode = ViewportMode::View2D;
            }
            if viewport_keys.2 {
                self.mode = ViewportMode::View3D;
            }
        }

        let pointer_delta = ctx.input(|i| i.pointer.delta());
        let scroll_delta_y = ctx.input(|i| i.smooth_scroll_delta.y);
        let camera_interacting = response.dragged_by(egui::PointerButton::Secondary)
            || response.dragged_by(egui::PointerButton::Middle)
            || (response.hovered() && scroll_delta_y.abs() > 0.01);
        let viewport_interacting = camera_interacting || response.dragged();
        self.bridge.handle_camera_input(
            ViewportPointerInput {
                pointer_delta: [pointer_delta.x, pointer_delta.y],
                scroll_delta_y: scroll_delta_y,
                drag_secondary: response.dragged_by(egui::PointerButton::Secondary),
                drag_middle: response.dragged_by(egui::PointerButton::Middle),
                hovered: response.hovered(),
            },
            self.mode == ViewportMode::View2D,
            ViewportNavigationConfig {
                invert_mouse_x: self.invert_mouse_x,
                invert_mouse_y: self.invert_mouse_y,
                move_sensitivity: self.move_sensitivity,
                rotate_sensitivity: self.rotate_sensitivity,
                scale_sensitivity: self.scale_sensitivity,
            },
        );
        self.bridge.update_camera(self.mode == ViewportMode::View2D);

        // --- Render scene ---
        let bg = [240, 240, 242, 255];

        let light_dir = Vec3::new(0.4, 0.8, 0.6).normalize();

        let render_mode = match self.render_style {
            RenderStyle::Solid => raf_render::scene_renderer::RenderMode::Solid,
            RenderStyle::Wireframe => raf_render::scene_renderer::RenderMode::Wireframe,
            RenderStyle::Preview => raf_render::scene_renderer::RenderMode::Preview,
        };

        // Determine dynamic grid Y position and whether to bypass depth testing
        let mut grid_y = -0.02;
        let mut grid_no_depth_test = false;
        let is_dragging = self.bridge.active_drag_axis() != raf_render::gizmo::GizmoAxis::None;

        if is_dragging {
            // Dragging: align grid to the bottom of the active/selected node and draw on top of everything
            if let Some(&selected_id) = self.selected.first() {
                if let Some(node) = scene.get(selected_id) {
                    let model = scene.world_matrix(selected_id);
                    let world_pos = model.col(3).truncate();
                    let half_height = 0.5 * node.scale.y.abs();
                    grid_y = world_pos.y - half_height - 0.02;
                    grid_no_depth_test = true;
                }
            }
        } else {
            // Idle: align grid to the bottom of the lowest block in the scene
            let mut min_y = 0.0_f32;
            let mut has_blocks = false;
            for (id, node) in scene.iter() {
                if !node.visible || node.name.is_empty() || matches!(node.primitive, Primitive::Empty | Primitive::Sprite2D) {
                    continue;
                }
                let model = scene.world_matrix(id);
                let world_pos = model.col(3).truncate();
                let half_height = 0.5 * node.scale.y.abs();
                let bottom_y = world_pos.y - half_height;
                if !has_blocks {
                    min_y = bottom_y;
                    has_blocks = true;
                } else {
                    min_y = min_y.min(bottom_y);
                }
            }
            if has_blocks {
                grid_y = min_y - 0.02;
            }
        }

        let render_options = SceneRenderOptions {
            mode: render_mode,
            show_grid_3d: self.grid_visible && self.mode == ViewportMode::View3D,
            grid_spacing: self.grid_spacing,
            grid_load_distance: self.grid_load_distance.max(0.0),
            solid_show_surface_edges: self.solid_show_surface_edges,
            solid_xray_mode: self.solid_xray_mode,
            solid_face_tonality: self.solid_face_tonality,
            selection_outline: self.render_cfg.selection_outline,
            selection_outline_color: self.render_cfg.selection_outline_color,
            grid_y,
            grid_no_depth_test,
        };
        let requested_scale = if self.mode == ViewportMode::View3D {
            self.render_cfg.depth_resolution_scale.clamp(0.35, 1.0)
        } else {
            1.0
        };
        let render_scale = if self.mode == ViewportMode::View3D {
            self.update_adaptive_render_scale(requested_scale, viewport_interacting)
        } else {
            requested_scale
        };
        let render_w = (vp_w * render_scale).round().max(1.0);
        let render_h = (vp_h * render_scale).round().max(1.0);

        let render_start = Instant::now();
        let pixels = self.bridge.render(
            scene,
            render_w,
            render_h,
            &self.selected,
            bg,
            light_dir,
            render_options,
            self.edit_mode == EditMode::Vertex,
        );
        self.render_cpu_ms = smooth_metric_ms(self.render_cpu_ms, render_start.elapsed().as_secs_f32() * 1000.0);

        // Create the image while the pixel borrow is still active
        let upload_start = Instant::now();
        let w = render_w as u32;
        let h = render_h as u32;
        let size = [w as usize, h as usize];

        let image = egui::ColorImage::from_rgba_premultiplied(size, pixels);

        // Now the borrow on self.renderer is released, safe to call upload
        self.upload_image(ctx, image, w, h);
        self.upload_cpu_ms = smooth_metric_ms(self.upload_cpu_ms, upload_start.elapsed().as_secs_f32() * 1000.0);

        // --- Draw the rendered image ---
        if let Some(tex) = &self.texture {
            painter.image(
                tex.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }

        // --- Grid overlay (drawn via egui painter, reuses existing grid math) ---
        let view_proj = self.bridge.view_projection(vp_w, vp_h);
        match self.mode {
            ViewportMode::View2D => self.draw_2d_grid(&painter, rect, is_dark),
            ViewportMode::View3D => {}
        }

        // --- Labels for entities ---
        if self.show_labels {
            self.draw_entity_labels(&painter, rect, scene, &view_proj, vp_w, vp_h, is_dark);
        }

        if self.edit_mode == EditMode::Vertex {
            self.draw_edit_overlay(&painter, rect, scene, &view_proj, vp_w, vp_h);
        }

        // --- Gizmo overlay for selected entity ---
        let selected_world_pos = self.selected.first().and_then(|&id| {
            scene.get(id).map(|_| scene.world_matrix(id).col(3).truncate())
        });
        if let Some(entity_pos) = selected_world_pos {
            if self.edit_mode != EditMode::Vertex {
                self.draw_gizmo_overlay(&painter, rect, entity_pos, &view_proj, vp_w, vp_h);
            }
        }

        self.draw_hud(&painter, rect, is_dark, icons);

        let mut changed = self.handle_hud_click(&response, rect, scene);

        self.apply_object_shortcuts(ctx, scene);

        if self.edit_mode == EditMode::Vertex {
            changed |= self.handle_edit_mode_input(&response, scene, &view_proj, rect, vp_w, vp_h);

            if response.dragged() {
                self.schedule_viewport_repaint(ctx);
            }

            return changed;
        }

        changed |= self.handle_object_mode_input(&response, scene, &view_proj, rect, vp_w, vp_h);

        // Request repaint for smooth camera interaction
        if response.dragged() {
            self.schedule_viewport_repaint(ctx);
        }

        changed
    }

    fn schedule_viewport_repaint(&self, ctx: &egui::Context) {
        let fps = self.fps_limit.max(15) as f32;
        ctx.request_repaint_after(Duration::from_secs_f32(1.0 / fps));
    }

    pub fn render_cpu_ms(&self) -> f32 {
        self.render_cpu_ms
    }

    pub fn upload_cpu_ms(&self) -> f32 {
        self.upload_cpu_ms
    }

    pub fn measured_frame_ms(&self) -> f32 {
        let viewport_ms = self.render_cpu_ms + self.upload_cpu_ms;
        if viewport_ms > 0.0 {
            viewport_ms
        } else {
            self.frame_time_hint.max(0.0) * 1000.0
        }
    }

    pub fn measured_fps(&self) -> u32 {
        let frame_ms = self.measured_frame_ms();
        if frame_ms > 0.0 {
            (1000.0 / frame_ms).round() as u32
        } else {
            0
        }
    }

    pub fn effective_render_scale(&self) -> f32 {
        self.adaptive_render_scale.max(0.35)
    }

    /// Upload pixel image to egui texture (reuses allocation when size matches).
    fn upload_image(&mut self, ctx: &egui::Context, image: egui::ColorImage, w: u32, h: u32) {
        if let Some(tex) = &mut self.texture {
            if self.last_size == [w, h] {
                tex.set(image, egui::TextureOptions::LINEAR);
            } else {
                *tex = ctx.load_texture("viewport_render", image, egui::TextureOptions::LINEAR);
            }
        } else {
            self.texture = Some(ctx.load_texture("viewport_render", image, egui::TextureOptions::LINEAR));
        }
        self.last_size = [w, h];
    }

    fn update_adaptive_render_scale(&mut self, requested_scale: f32, interacting: bool) -> f32 {
        let dt = self.frame_time_hint.clamp(1.0 / 240.0, 0.25);
        if interacting {
            self.interaction_linger_s = 0.20;
        } else {
            self.interaction_linger_s = (self.interaction_linger_s - dt).max(0.0);
        }

        let interaction_active = interacting || self.interaction_linger_s > 0.0;
        let budget_ms = self.render_cfg.frame_budget_ms.max(8.0);
        let previous_ms = self.measured_frame_ms().max(1.0);
        let budget_ratio = if interaction_active {
            (budget_ms / previous_ms).sqrt().clamp(0.35, 1.0)
        } else {
            1.0
        };

        let target_scale = if interaction_active {
            (requested_scale * budget_ratio).clamp(0.35, requested_scale)
        } else {
            requested_scale
        };
        let smoothing = if interaction_active { 0.45 } else { 0.18 };

        self.adaptive_render_scale = if self.adaptive_render_scale <= 0.0 {
            target_scale
        } else {
            self.adaptive_render_scale + (target_scale - self.adaptive_render_scale) * smoothing
        };

        self.adaptive_render_scale.clamp(0.35, requested_scale)
    }
}

fn smooth_metric_ms(current: f32, sample: f32) -> f32 {
    if current <= 0.0 {
        sample
    } else {
        current * 0.85 + sample * 0.15
    }
}


