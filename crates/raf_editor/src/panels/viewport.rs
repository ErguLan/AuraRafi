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

use eframe::egui_wgpu;
use egui::{Color32, Pos2, Rect, Stroke};
use glam::{Mat4, Quat, Vec3};
use std::time::{Duration, Instant};

use raf_core::config::Language;
use raf_core::i18n::t;
use raf_core::scene::graph::{Primitive, SceneGraph, SceneNodeId};
use raf_core::scene::WorldTransformCache;
use raf_render::bridge::{
    EditorCameraBlock, EditorCameraMode, PickingPolicy, RenderRuntime, RenderRuntimeSnapshot,
    ViewportBridge, ViewportNavigationConfig, ViewportPointerInput,
};
use raf_render::gizmo::{GizmoAxis, GizmoMode};
use raf_render::render_config::RenderConfig;
use raf_render::scene_renderer::RenderOptions as SceneRenderOptions;
use raf_render::WorldStreamConfig;

use crate::panels::viewport_surface_host::{
    ViewportFrameKey, ViewportPresentationBackend, ViewportSurfaceHost,
};
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

#[derive(Debug, Clone)]
struct GroupTransformMemberState {
    id: SceneNodeId,
    position: Vec3,
    rotation: Vec3,
    scale: Vec3,
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
    /// Persisted large-world visibility policy. The renderer uses it before
    /// frustum culling, so small scenes retain their current unlimited path.
    pub world_stream_config: WorldStreamConfig,
    pub invert_mouse_x: bool,
    pub invert_mouse_y: bool,
    pub move_sensitivity: f32,
    pub rotate_sensitivity: f32,
    pub scale_sensitivity: f32,
    pub uniform_scale_by_default: bool,
    pub invert_ws: bool,
    pub wasd_speed: f32,
    pub wasd_speed_boost: f32,
    pub focus_lock_enabled: bool,
    pub focus_locked: bool,
    pub focus_base_distance: f32,
    pub focus_base_target: Vec3,
    pub focus_strafe_offset: Vec3,
    pub select_mode: bool,
    pub gizmo_growth_scale: f32,
    pub solid_show_surface_edges: bool,
    pub solid_xray_mode: bool,
    pub solid_face_tonality: bool,

    drag_ongoing: bool,
    free_drag_active: bool,
    free_drag_start_pos: Vec3,
    free_drag_start_mouse: [f32; 2],
    group_free_drag_starts: Vec<(SceneNodeId, Vec3)>,
    group_hover_axis: GizmoAxis,
    group_drag_axis: GizmoAxis,
    group_hover_scale_sign: f32,
    group_drag_scale_sign: f32,
    group_drag_start_mouse: Option<[f32; 2]>,
    group_drag_origin: Option<Vec3>,
    group_drag_scale: Option<Vec3>,
    group_drag_members: Vec<GroupTransformMemberState>,
    /// Accumulated rotation delta (radians) for group rotate drag.
    group_accumulated_rotation: f32,
    /// Last pointer position during a group drag, for incremental delta.
    group_last_drag_mouse: Option<[f32; 2]>,

    // 2D mode state
    bridge: ViewportBridge,
    render_runtime: RenderRuntimeSnapshot,
    surface_host: ViewportSurfaceHost,
    render_cpu_ms: f32,
    upload_cpu_ms: f32,
    adaptive_render_scale: f32,
    interaction_linger_s: f32,
    cached_grid_scene_fingerprint: Option<u64>,
    cached_grid_y: f32,
    viewport_frame_epoch: u64,
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
            world_stream_config: WorldStreamConfig::default(),
            invert_mouse_x: false,
            invert_mouse_y: true,
            move_sensitivity: 3.5,
            rotate_sensitivity: 3.5,
            scale_sensitivity: 3.5,
            uniform_scale_by_default: false,
            invert_ws: false,
            wasd_speed: 1.0,
            wasd_speed_boost: 1.0,
            focus_lock_enabled: true,
            focus_locked: false,
            focus_base_distance: 5.0,
            focus_base_target: Vec3::ZERO,
            focus_strafe_offset: Vec3::ZERO,
            select_mode: false,
            gizmo_growth_scale: 0.0,
            solid_show_surface_edges: false,
            solid_xray_mode: false,
            solid_face_tonality: true,

            drag_ongoing: false,
            free_drag_active: false,
            free_drag_start_pos: Vec3::ZERO,
            free_drag_start_mouse: [0.0, 0.0],
            group_free_drag_starts: Vec::new(),
            group_hover_axis: GizmoAxis::None,
            group_drag_axis: GizmoAxis::None,
            group_hover_scale_sign: 0.0,
            group_drag_scale_sign: 1.0,
            group_drag_start_mouse: None,
            group_drag_origin: None,
            group_drag_scale: None,
            group_drag_members: Vec::new(),
            group_accumulated_rotation: 0.0,
            group_last_drag_mouse: None,

            bridge: ViewportBridge::default(),
            render_runtime: RenderRuntimeSnapshot::default(),
            surface_host: ViewportSurfaceHost::default(),
            render_cpu_ms: 0.0,
            upload_cpu_ms: 0.0,
            adaptive_render_scale: 1.0,
            interaction_linger_s: 0.0,
            cached_grid_scene_fingerprint: None,
            cached_grid_y: -0.02,
            viewport_frame_epoch: 0,
        }
    }
}

impl ViewportPanel {
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        wgpu_render_state: Option<&egui_wgpu::RenderState>,
        render_runtime: &mut RenderRuntime,
        scene: &mut SceneGraph,
        is_dark: bool,
        lang: Language,
        icons: &UiIconAtlas,
    ) -> bool {
        self.bridge
            .set_picking_policy(PickingPolicy::for_render_config(&self.render_cfg));
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
        let movement_input = ctx.input(|i| {
            let ctrl = i.modifiers.ctrl || i.modifiers.mac_cmd;
            if ctrl {
                return (0.0, 0.0, 0.0);
            }
            let invert_fwd = if self.invert_ws { 1.0 } else { -1.0 };
            let forward = ((i.key_down(egui::Key::W) as i8 - i.key_down(egui::Key::S) as i8)
                as f32)
                * invert_fwd;
            let right = (i.key_down(egui::Key::D) as i8 - i.key_down(egui::Key::A) as i8) as f32;
            let up = (i.key_down(egui::Key::E) as i8 - i.key_down(egui::Key::Q) as i8) as f32;
            (forward, right, up)
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
        // Ctrl+Scroll adjusts WASD speed at runtime.
        if response.hovered() && scroll_delta_y.abs() > 0.01 {
            let ctrl = ctx.input(|i| i.modifiers.ctrl || i.modifiers.mac_cmd);
            if ctrl {
                self.wasd_speed_boost =
                    (self.wasd_speed_boost - scroll_delta_y * 0.002).clamp(0.5, 5.0);
            }
        }

        let camera_interacting = response.dragged_by(egui::PointerButton::Secondary)
            || response.dragged_by(egui::PointerButton::Middle)
            || (response.hovered() && scroll_delta_y.abs() > 0.01);
        let viewport_interacting = camera_interacting || response.dragged();

        // Focus lock: WASD offsets the editor camera relative to the focused
        // target. The offset only returns after every navigation key is
        // released, so the return animation never fights held input.
        let (move_fwd, move_rgt, move_up) = if self.focus_locked
            && self.mode != ViewportMode::View2D
        {
            let dt = self.frame_time_hint.clamp(1.0 / 240.0, 1.0 / 15.0);
            let speed = 3.0 * dt;
            let zoom_speed = 4.0 * dt;
            let navigation_active =
                movement_input.0 != 0.0 || movement_input.1 != 0.0 || movement_input.2 != 0.0;

            if navigation_active {
                // Strafe is camera-relative but leaves the object's own axes
                // untouched. It is a temporary inspection angle, not movement
                // of the focused object.
                let yaw = self.bridge.orbit_yaw();
                let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
                let up = Vec3::Y;
                self.focus_strafe_offset += right * (-movement_input.1 * speed * 3.0);
                self.focus_strafe_offset += up * (movement_input.2 * speed * 3.0);
                let new_dist = (self.bridge.orbit_distance() - movement_input.0 * zoom_speed * 3.0)
                    .clamp(0.5, 200.0);
                self.bridge.set_orbit_distance(new_dist);
            } else {
                let return_blend = 1.0 - (-10.0 * dt).exp();
                self.focus_strafe_offset = self.focus_strafe_offset.lerp(Vec3::ZERO, return_blend);
                let cur_dist = self.bridge.orbit_distance();
                let diff = self.focus_base_distance - cur_dist;
                self.bridge
                    .set_orbit_distance(cur_dist + diff * return_blend);
            }
            self.bridge
                .set_camera_target(self.focus_base_target + self.focus_strafe_offset);
            (0.0, 0.0, 0.0)
        } else {
            movement_input
        };

        self.bridge.handle_camera_input(
            ViewportPointerInput {
                pointer_delta: [pointer_delta.x, pointer_delta.y],
                scroll_delta_y: scroll_delta_y,
                drag_secondary: response.dragged_by(egui::PointerButton::Secondary),
                drag_middle: response.dragged_by(egui::PointerButton::Middle),
                hovered: response.hovered(),
                move_forward: move_fwd,
                move_right: move_rgt,
                move_up: move_up,
                frame_time_s: self.frame_time_hint.max(1.0 / 240.0),
            },
            self.mode == ViewportMode::View2D,
            ViewportNavigationConfig {
                invert_mouse_x: self.invert_mouse_x,
                invert_mouse_y: self.invert_mouse_y,
                move_sensitivity: self.move_sensitivity * self.wasd_speed * self.wasd_speed_boost,
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

        let is_dragging = self.bridge.active_drag_axis() != raf_render::gizmo::GizmoAxis::None;
        let scene_fingerprint = scene.render_fingerprint();
        let (grid_y, grid_no_depth_test) =
            self.grid_height_for_scene(scene, scene_fingerprint, is_dragging);

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
            secondary_selection_outline_color: [255, 120, 20, 180],
            primary_selected: self.selected.first().map(|id| id.0 as u64),
            grid_y,
            grid_no_depth_test,
            triangle_budget: self.render_cfg.max_triangles,
            world_streaming_enabled: self.world_stream_config.enabled,
            world_stream_region_size: self.world_stream_config.region_size,
            world_stream_load_radius: self.world_stream_config.load_radius,
        };
        let surface_plan = self.surface_host.configure_from_render_config(
            &self.render_cfg,
            [vp_w.round().max(1.0) as u32, vp_h.round().max(1.0) as u32],
        );
        let requested_scale = if self.mode == ViewportMode::View3D {
            self.render_cfg
                .depth_resolution_scale
                .min(surface_plan.preferred_surface_scale)
                .clamp(0.35, 1.0)
        } else {
            1.0
        };
        let render_scale = if self.mode == ViewportMode::View3D {
            self.update_adaptive_render_scale(
                requested_scale,
                viewport_interacting,
                is_dragging,
                self.selected.len(),
            )
        } else {
            requested_scale
        };
        let render_w = (vp_w * render_scale).round().max(1.0);
        let render_h = (vp_h * render_scale).round().max(1.0);

        let w = render_w as u32;
        let h = render_h as u32;
        let vertex_edit_enabled = self.edit_mode == EditMode::Vertex;
        if vertex_edit_enabled {
            self.viewport_frame_epoch = self.viewport_frame_epoch.wrapping_add(1);
        }
        let frame_key = ViewportFrameKey::new(
            scene_fingerprint,
            self.bridge.camera(),
            [w, h],
            &self.selected,
            bg,
            light_dir,
            render_options,
            vertex_edit_enabled,
            self.viewport_frame_epoch,
        );
        let render_start = Instant::now();
        let min_frame_interval = (self.fps_limit != 0)
            .then(|| Duration::from_secs_f32(1.0 / self.fps_limit.max(15) as f32));
        let presentation = {
            let bridge = &mut self.bridge;
            self.surface_host.present_frame(
                ctx,
                wgpu_render_state,
                render_runtime,
                min_frame_interval,
                frame_key,
                || {
                    bridge.build_scene_frame(
                        scene,
                        render_w,
                        render_h,
                        &self.selected,
                        bg,
                        light_dir,
                        render_options,
                        vertex_edit_enabled,
                    )
                },
            )
        };
        let surface_cpu_ms = render_start.elapsed().as_secs_f32() * 1000.0;
        self.render_runtime = render_runtime.snapshot();
        self.render_cpu_ms = smooth_metric_ms(
            self.render_cpu_ms,
            if presentation.cache.last_frame_reused {
                0.0
            } else {
                surface_cpu_ms
            },
        );
        self.upload_cpu_ms = match presentation.backend {
            ViewportPresentationBackend::CpuPixels => smooth_metric_ms(
                self.upload_cpu_ms,
                if presentation.cache.last_frame_reused {
                    0.0
                } else {
                    surface_cpu_ms
                },
            ),
            ViewportPresentationBackend::GpuTexture => smooth_metric_ms(self.upload_cpu_ms, 0.0),
        };

        // --- Draw the rendered image ---
        self.surface_host.paint(&painter, rect);

        // --- Grid overlay (drawn via egui painter, reuses existing grid math) ---
        let view_proj = self.bridge.view_projection(vp_w, vp_h);
        match self.mode {
            ViewportMode::View2D => self.draw_2d_grid(&painter, rect, is_dark),
            ViewportMode::View3D => {}
        }

        // --- Labels for entities ---
        if self.show_labels && self.should_draw_labels(viewport_interacting, is_dragging) {
            self.draw_entity_labels(&painter, rect, scene, &view_proj, vp_w, vp_h, is_dark);
        }

        if self.edit_mode != EditMode::Vertex {
            if let Some(pointer) = ctx.input(|i| i.pointer.hover_pos()) {
                if response.hovered() && !self.overlay_blocks_world_input(rect, pointer) {
                    let local = [pointer.x - rect.left(), pointer.y - rect.top()];
                    if self.selected.len() > 1 {
                        self.bridge.update_transform_hover(
                            scene,
                            None,
                            &view_proj,
                            [0.0, 0.0],
                            vp_w,
                            vp_h,
                        );
                        self.update_group_transform_hover(scene, &view_proj, local, vp_w, vp_h);
                    } else {
                        self.clear_group_transform_state();
                        self.bridge.update_transform_hover_scaled(
                            scene,
                            self.selected.first().copied(),
                            &view_proj,
                            local,
                            vp_w,
                            vp_h,
                            self.gizmo_presentation_scale(),
                        );
                    }
                } else {
                    self.clear_group_transform_state();
                    self.bridge.update_transform_hover(
                        scene,
                        None,
                        &view_proj,
                        [0.0, 0.0],
                        vp_w,
                        vp_h,
                    );
                }
            } else {
                self.clear_group_transform_state();
                self.bridge
                    .update_transform_hover(scene, None, &view_proj, [0.0, 0.0], vp_w, vp_h);
            }
        }

        if self.edit_mode == EditMode::Vertex {
            self.draw_edit_overlay(&painter, rect, scene, &view_proj, vp_w, vp_h);
        }

        // --- Gizmo overlay for selected entity ---
        let gizmo_transform = if self.selected.len() > 1 {
            self.multi_selection_transform(scene)
        } else {
            self.selected.first().and_then(|&id| {
                scene
                    .get(id)
                    .map(|node| (scene.world_matrix(id).col(3).truncate(), node.scale))
            })
        };
        if let Some((entity_pos, entity_scale)) = gizmo_transform {
            if self.edit_mode != EditMode::Vertex {
                self.draw_gizmo_overlay(
                    &painter,
                    rect,
                    entity_pos,
                    entity_scale,
                    &view_proj,
                    vp_w,
                    vp_h,
                );
            }
        }

        self.draw_hud(&painter, rect, is_dark, icons, lang);

        // Advance smooth camera focus animation (Lerp towards target).
        if self.bridge.update_smooth_focus() {
            self.schedule_viewport_repaint(ctx);
        }

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

        // Keep the selected object's center as the stable focus base. The
        // transient WASD offset is applied after it, avoiding the old rebound
        // loop where the follow code reset the target every frame.
        if self.focus_locked {
            if let Some(id) = self.selected.first().copied() {
                if scene.get(id).is_some() {
                    let world = scene.world_matrix(id);
                    self.focus_base_target = world.col(3).truncate();
                    self.bridge
                        .set_camera_target(self.focus_base_target + self.focus_strafe_offset);
                }
            }
        }

        // Request repaint for smooth camera interaction
        if response.dragged() {
            self.schedule_viewport_repaint(ctx);
        }

        changed
    }

    fn schedule_viewport_repaint(&self, ctx: &egui::Context) {
        if self.fps_limit == 0 {
            // Unrestricted means no artificial frame cap. It still needs an
            // asynchronous request while a camera animation is in flight.
            ctx.request_repaint_after(Duration::from_millis(1));
            return;
        }

        let fps = self.fps_limit.max(15) as f32;
        ctx.request_repaint_after(Duration::from_secs_f32(1.0 / fps));
    }

    fn grid_height_for_scene(
        &mut self,
        scene: &SceneGraph,
        scene_fingerprint: u64,
        is_dragging: bool,
    ) -> (f32, bool) {
        const GRID_CLEARANCE: f32 = 0.02;

        if is_dragging {
            let Some(selected_id) = self.selected.first().copied() else {
                return (self.cached_grid_y, false);
            };
            let Some(node) = scene.get(selected_id) else {
                return (self.cached_grid_y, false);
            };
            let model = scene.world_matrix(selected_id);
            let world_pos = model.col(3).truncate();
            let half_height = 0.5 * node.scale.y.abs();
            return (world_pos.y - half_height - GRID_CLEARANCE, true);
        }

        if self.cached_grid_scene_fingerprint == Some(scene_fingerprint) {
            return (self.cached_grid_y, false);
        }

        let transforms = WorldTransformCache::build(scene);
        let mut min_y = 0.0_f32;
        let mut has_blocks = false;
        for (id, node) in scene.iter() {
            if !node.visible
                || node.name.is_empty()
                || matches!(node.primitive, Primitive::Empty | Primitive::Sprite2D)
            {
                continue;
            }

            let model = transforms
                .world_matrix(id)
                .unwrap_or_else(|| node.local_matrix());
            let world_pos = model.col(3).truncate();
            let bottom_y = world_pos.y - 0.5 * node.scale.y.abs();
            min_y = if has_blocks {
                min_y.min(bottom_y)
            } else {
                bottom_y
            };
            has_blocks = true;
        }

        self.cached_grid_y = if has_blocks {
            min_y - GRID_CLEARANCE
        } else {
            -GRID_CLEARANCE
        };
        self.cached_grid_scene_fingerprint = Some(scene_fingerprint);
        (self.cached_grid_y, false)
    }

    pub fn render_cpu_ms(&self) -> f32 {
        self.render_cpu_ms
    }

    /// Returns the current camera bookmark snapshot.
    pub fn camera_bookmark_snapshot(&self) -> (Vec3, f32, f32, f32) {
        (
            self.bridge.camera_target(),
            self.bridge.orbit_yaw(),
            self.bridge.orbit_pitch(),
            self.bridge.orbit_distance(),
        )
    }

    /// Restores the camera from a bookmark snapshot.
    pub fn restore_camera_bookmark(&mut self, target: Vec3, yaw: f32, pitch: f32, dist: f32) {
        self.bridge.set_camera_target(target);
        self.bridge.set_orbit_angles(yaw, pitch);
        self.bridge.set_orbit_distance(dist);
    }

    pub fn set_render_runtime(&mut self, snapshot: RenderRuntimeSnapshot) {
        self.render_runtime = snapshot;
    }

    pub fn is_gizmo_active(&self) -> bool {
        self.bridge.active_drag_axis() != GizmoAxis::None
    }

    pub fn focus_selected_entity(&mut self, scene: &SceneGraph, selected: Option<SceneNodeId>) {
        self.focus_locked = false;
        self.focus_strafe_offset = Vec3::ZERO;
        self.bridge
            .focus_selected(scene, selected, self.mode == ViewportMode::View2D);
    }

    /// Snapshot the editor-only camera block. This is persisted with the
    /// active session and never materialized as a SceneGraph node.
    pub fn editor_camera_block(&self) -> EditorCameraBlock {
        self.bridge.editor_camera_block()
    }

    pub fn apply_editor_camera_block(&mut self, block: &EditorCameraBlock) {
        self.mode = match block.mode {
            EditorCameraMode::Orthographic2D => ViewportMode::View2D,
            EditorCameraMode::Orbit | EditorCameraMode::Fly => ViewportMode::View3D,
        };
        self.bridge.apply_editor_camera_block(block);
        self.focus_locked = false;
        self.focus_strafe_offset = Vec3::ZERO;
    }

    pub(super) fn set_focus_lock(&mut self, scene: &SceneGraph, enabled: bool) {
        if !enabled {
            self.focus_locked = false;
            self.focus_strafe_offset = Vec3::ZERO;
            return;
        }

        let Some(id) = self.selected.first().copied() else {
            self.focus_locked = false;
            return;
        };
        let Some(_) = scene.get(id) else {
            self.focus_locked = false;
            return;
        };

        self.focus_base_distance = self.bridge.orbit_distance();
        self.focus_base_target = scene.world_matrix(id).col(3).truncate();
        self.focus_strafe_offset = Vec3::ZERO;
        self.bridge.set_camera_target(self.focus_base_target);
        self.focus_locked = true;
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

    pub fn is_drag_ongoing(&self) -> bool {
        self.drag_ongoing
    }

    fn clear_group_transform_state(&mut self) {
        self.group_hover_axis = GizmoAxis::None;
        self.group_drag_axis = GizmoAxis::None;
        self.group_hover_scale_sign = 0.0;
        self.group_drag_scale_sign = 1.0;
        self.group_drag_start_mouse = None;
        self.group_drag_origin = None;
        self.group_drag_scale = None;
        self.group_drag_members.clear();
        self.group_accumulated_rotation = 0.0;
        self.group_last_drag_mouse = None;
    }

    fn multi_selection_transform(&self, scene: &SceneGraph) -> Option<(Vec3, Vec3)> {
        if self.selected.len() < 2 {
            return None;
        }

        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);

        for id in &self.selected {
            let node = scene.get(*id)?;
            let half = node.scale.abs().max(Vec3::splat(0.05)) * 0.5;
            min = min.min(node.position - half);
            max = max.max(node.position + half);
        }

        Some(((min + max) * 0.5, (max - min).max(Vec3::splat(0.1))))
    }

    fn multi_selection_highlighted_axis(&self) -> GizmoAxis {
        if self.group_drag_axis != GizmoAxis::None {
            self.group_drag_axis
        } else {
            self.group_hover_axis
        }
    }

    fn multi_selection_highlighted_scale_sign(&self) -> f32 {
        if self.group_drag_axis != GizmoAxis::None && self.bridge.gizmo().mode == GizmoMode::Scale {
            self.group_drag_scale_sign
        } else {
            self.group_hover_scale_sign
        }
    }

    fn update_group_transform_hover(
        &mut self,
        scene: &SceneGraph,
        view_proj: &Mat4,
        pointer_local: [f32; 2],
        vp_w: f32,
        vp_h: f32,
    ) {
        if self.group_drag_axis != GizmoAxis::None {
            self.group_hover_axis = self.group_drag_axis;
            return;
        }

        let Some((origin, size)) = self.multi_selection_transform(scene) else {
            self.group_hover_axis = GizmoAxis::None;
            self.group_hover_scale_sign = 0.0;
            return;
        };

        let gizmo_hit = match self.bridge.gizmo().mode {
            GizmoMode::Rotate => raf_render::picking::pick_gizmo_rotation_ring_scaled(
                pointer_local,
                origin,
                self.gizmo_presentation_scale(),
                view_proj,
                vp_w,
                vp_h,
            ),
            GizmoMode::Translate => raf_render::picking::pick_gizmo_arrow_scaled(
                pointer_local,
                origin,
                self.gizmo_presentation_scale(),
                view_proj,
                vp_w,
                vp_h,
            ),
            GizmoMode::Scale => {
                let hit = raf_render::picking::pick_gizmo_scale_handle(
                    pointer_local,
                    origin,
                    size,
                    view_proj,
                    vp_w,
                    vp_h,
                );
                self.group_hover_scale_sign = hit.map(|(_, _, sign)| sign).unwrap_or(0.0);
                hit.map(|(axis_idx, distance, _)| (axis_idx, distance))
            }
        };

        if self.bridge.gizmo().mode != GizmoMode::Scale {
            self.group_hover_scale_sign = 0.0;
        }

        self.group_hover_axis = gizmo_hit
            .map(|(axis_idx, _)| [GizmoAxis::X, GizmoAxis::Y, GizmoAxis::Z][axis_idx])
            .unwrap_or(GizmoAxis::None);
    }

    fn begin_group_transform_drag(
        &mut self,
        scene: &SceneGraph,
        view_proj: &Mat4,
        pointer_local: [f32; 2],
        vp_w: f32,
        vp_h: f32,
    ) {
        let Some((origin, size)) = self.multi_selection_transform(scene) else {
            return;
        };

        let gizmo_hit = match self.bridge.gizmo().mode {
            GizmoMode::Rotate => raf_render::picking::pick_gizmo_rotation_ring_scaled(
                pointer_local,
                origin,
                self.gizmo_presentation_scale(),
                view_proj,
                vp_w,
                vp_h,
            ),
            GizmoMode::Translate => raf_render::picking::pick_gizmo_arrow_scaled(
                pointer_local,
                origin,
                self.gizmo_presentation_scale(),
                view_proj,
                vp_w,
                vp_h,
            ),
            GizmoMode::Scale => {
                let hit = raf_render::picking::pick_gizmo_scale_handle(
                    pointer_local,
                    origin,
                    size,
                    view_proj,
                    vp_w,
                    vp_h,
                );
                self.group_drag_scale_sign = hit.map(|(_, _, sign)| sign).unwrap_or(1.0);
                hit.map(|(axis_idx, distance, _)| (axis_idx, distance))
            }
        };

        let Some((axis_idx, _)) = gizmo_hit else {
            return;
        };

        self.group_drag_axis = [GizmoAxis::X, GizmoAxis::Y, GizmoAxis::Z][axis_idx];
        self.group_hover_axis = self.group_drag_axis;
        self.group_drag_start_mouse = Some(pointer_local);
        self.group_drag_origin = Some(origin);
        self.group_drag_scale = Some(size);
        self.group_accumulated_rotation = 0.0;
        self.group_last_drag_mouse = Some(pointer_local);
        self.group_drag_members = self
            .selected
            .iter()
            .filter_map(|id| {
                scene.get(*id).map(|node| GroupTransformMemberState {
                    id: *id,
                    position: node.position,
                    rotation: node.rotation,
                    scale: node.scale,
                })
            })
            .collect();
    }

    fn apply_group_transform_drag(
        &mut self,
        scene: &mut SceneGraph,
        view_proj: &Mat4,
        current_mouse: [f32; 2],
        uniform_scale: bool,
        snap_to_ctrl: bool,
        vp_w: f32,
        vp_h: f32,
    ) -> bool {
        let Some(start_mouse) = self.group_drag_start_mouse else {
            return false;
        };
        let Some(origin) = self.group_drag_origin else {
            return false;
        };
        let Some(size) = self.group_drag_scale else {
            return false;
        };

        let axis_dir = match self.group_drag_axis {
            GizmoAxis::X => Vec3::X,
            GizmoAxis::Y => Vec3::Y,
            GizmoAxis::Z => Vec3::Z,
            GizmoAxis::None => return false,
        };
        let face_sign = if self.bridge.gizmo().mode == GizmoMode::Scale {
            self.group_drag_scale_sign.signum().max(-1.0)
        } else {
            1.0
        };
        let face_dir = axis_dir * if face_sign == 0.0 { 1.0 } else { face_sign };
        let handle_origin_world = if self.bridge.gizmo().mode == GizmoMode::Scale {
            let handle_offset = Vec3::new(
                if self.group_drag_axis == GizmoAxis::X {
                    size.x * 0.5 * face_dir.x.signum()
                } else {
                    0.0
                },
                if self.group_drag_axis == GizmoAxis::Y {
                    size.y * 0.5 * face_dir.y.signum()
                } else {
                    0.0
                },
                if self.group_drag_axis == GizmoAxis::Z {
                    size.z * 0.5 * face_dir.z.signum()
                } else {
                    0.0
                },
            );
            origin + handle_offset
        } else {
            origin
        };
        let axis_end_world = handle_origin_world + face_dir;
        let origin_screen =
            raf_render::math::transform::project_point(handle_origin_world, view_proj, vp_w, vp_h);
        let axis_screen =
            raf_render::math::transform::project_point(axis_end_world, view_proj, vp_w, vp_h);
        let (Some((o_s, _)), Some((a_s, _))) = (origin_screen, axis_screen) else {
            return false;
        };

        let axis_screen_dir = [a_s[0] - o_s[0], a_s[1] - o_s[1]];
        let axis_len = (axis_screen_dir[0] * axis_screen_dir[0]
            + axis_screen_dir[1] * axis_screen_dir[1])
            .sqrt();
        if axis_len < 1.0 {
            return false;
        }

        let mouse_delta = [
            current_mouse[0] - start_mouse[0],
            current_mouse[1] - start_mouse[1],
        ];
        let projection =
            (mouse_delta[0] * axis_screen_dir[0] + mouse_delta[1] * axis_screen_dir[1]) / axis_len;
        let delta = projection * (self.bridge.orbit_distance() / (vp_w.min(vp_h) * 0.5));

        let mut changed = false;
        match self.bridge.gizmo().mode {
            GizmoMode::Translate => {
                for member in &self.group_drag_members {
                    if let Some(node) = scene.get_mut(member.id) {
                        let next = member.position + axis_dir * delta;
                        if node.position != next {
                            node.position = next;
                            changed = true;
                        }
                    }
                }
            }
            GizmoMode::Rotate => {
                // Accumulate incremental rotation delta (same fix as single
                // entity: avoids the "rotate backwards after 180 deg" bug).
                let last = self.group_last_drag_mouse.unwrap_or(start_mouse);
                let inc_mouse = [current_mouse[0] - last[0], current_mouse[1] - last[1]];
                let inc_projection = (inc_mouse[0] * axis_screen_dir[0]
                    + inc_mouse[1] * axis_screen_dir[1])
                    / axis_len;
                let inc_delta =
                    inc_projection * (self.bridge.orbit_distance() / (vp_w.min(vp_h) * 0.5));
                let inc_radians = inc_delta * std::f32::consts::FRAC_PI_4;
                self.group_accumulated_rotation += inc_radians;
                self.group_last_drag_mouse = Some(current_mouse);

                let mut angle_rad = self.group_accumulated_rotation;
                if snap_to_ctrl {
                    const SNAP_STEP: f32 = std::f32::consts::PI / 12.0;
                    angle_rad = (angle_rad / SNAP_STEP).round() * SNAP_STEP;
                }
                let rotation = Quat::from_axis_angle(axis_dir, angle_rad);
                let angle_deg = angle_rad.to_degrees();
                for member in &self.group_drag_members {
                    if let Some(node) = scene.get_mut(member.id) {
                        let rotated_offset = rotation * (member.position - origin);
                        let next_position = origin + rotated_offset;
                        let next_rotation = member.rotation + axis_dir * angle_deg;
                        if node.position != next_position || node.rotation != next_rotation {
                            node.position = next_position;
                            node.rotation = next_rotation;
                            changed = true;
                        }
                    }
                }
            }
            GizmoMode::Scale => {
                let start_axis_scale = match self.group_drag_axis {
                    GizmoAxis::X => size.x.abs().max(0.01),
                    GizmoAxis::Y => size.y.abs().max(0.01),
                    GizmoAxis::Z => size.z.abs().max(0.01),
                    GizmoAxis::None => return false,
                };

                if uniform_scale {
                    let factor = (1.0 + (delta * 2.0) / start_axis_scale).max(0.05);
                    for member in &self.group_drag_members {
                        if let Some(node) = scene.get_mut(member.id) {
                            let next_scale = (member.scale * factor).max(Vec3::splat(0.01));
                            let next_position = origin + (member.position - origin) * factor;
                            if node.scale != next_scale || node.position != next_position {
                                node.scale = next_scale;
                                node.position = next_position;
                                changed = true;
                            }
                        }
                    }
                } else {
                    let factor = ((start_axis_scale + delta).max(0.01)) / start_axis_scale;
                    for member in &self.group_drag_members {
                        if let Some(node) = scene.get_mut(member.id) {
                            let mut next_scale = member.scale;
                            let mut next_position = member.position;
                            match self.group_drag_axis {
                                GizmoAxis::X => {
                                    next_scale.x = (member.scale.x * factor).abs().max(0.01)
                                        * member.scale.x.signum().max(1.0);
                                    next_position.x =
                                        origin.x + (member.position.x - origin.x) * factor;
                                }
                                GizmoAxis::Y => {
                                    next_scale.y = (member.scale.y * factor).abs().max(0.01)
                                        * member.scale.y.signum().max(1.0);
                                    next_position.y =
                                        origin.y + (member.position.y - origin.y) * factor;
                                }
                                GizmoAxis::Z => {
                                    next_scale.z = (member.scale.z * factor).abs().max(0.01)
                                        * member.scale.z.signum().max(1.0);
                                    next_position.z =
                                        origin.z + (member.position.z - origin.z) * factor;
                                }
                                GizmoAxis::None => {}
                            }
                            if node.scale != next_scale || node.position != next_position {
                                node.scale = next_scale;
                                node.position = next_position;
                                changed = true;
                            }
                        }
                    }
                }
            }
        }

        changed
    }

    fn should_draw_labels(&self, interacting: bool, is_dragging: bool) -> bool {
        if self.mode != ViewportMode::View3D {
            return true;
        }

        if is_dragging {
            return false;
        }

        !(interacting && self.selected.len() > 1)
    }

    fn update_adaptive_render_scale(
        &mut self,
        requested_scale: f32,
        interacting: bool,
        is_dragging: bool,
        selection_count: usize,
    ) -> f32 {
        let dt = self.frame_time_hint.clamp(1.0 / 240.0, 0.25);
        if interacting {
            self.interaction_linger_s = 0.20;
        } else {
            self.interaction_linger_s = (self.interaction_linger_s - dt).max(0.0);
        }

        let interaction_active = interacting || self.interaction_linger_s > 0.0;
        let budget_ms = self.render_cfg.frame_budget_ms.max(8.0);
        let previous_ms = self.measured_frame_ms().max(1.0);
        let preset_bias = adaptive_interaction_quality_bias(budget_ms);
        let drag_bias = if is_dragging { 0.86 } else { 1.0 };
        let multi_select_bias = if selection_count > 4 {
            (1.0 - ((selection_count - 4) as f32 * 0.025)).clamp(0.72, 1.0)
        } else {
            1.0
        };
        let budget_ratio = if interaction_active {
            ((budget_ms / previous_ms).sqrt() * preset_bias * drag_bias * multi_select_bias)
                .clamp(0.35, 1.0)
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

fn adaptive_interaction_quality_bias(frame_budget_ms: f32) -> f32 {
    let reference_budget_ms = 16.6;
    (reference_budget_ms / frame_budget_ms.max(reference_budget_ms)).clamp(0.45, 1.0)
}

#[cfg(test)]
mod tests {
    use super::adaptive_interaction_quality_bias;

    #[test]
    fn potato_bias_is_more_aggressive_than_medium() {
        let potato_bias = adaptive_interaction_quality_bias(50.0);
        let medium_bias = adaptive_interaction_quality_bias(16.6);

        assert!(potato_bias < medium_bias);
        assert_eq!(medium_bias, 1.0);
    }

    #[test]
    fn lower_presets_drop_scale_more_for_same_frame_time() {
        let requested_scale = 0.6_f32;
        let previous_ms = 25.0_f32;
        let potato_scale = (requested_scale
            * ((50.0 / previous_ms).sqrt() * adaptive_interaction_quality_bias(50.0))
                .clamp(0.35, 1.0))
        .clamp(0.35, requested_scale);
        let medium_scale = (requested_scale
            * ((16.6 / previous_ms).sqrt() * adaptive_interaction_quality_bias(16.6))
                .clamp(0.35, 1.0))
        .clamp(0.35, requested_scale);

        assert!(potato_scale < medium_scale);
    }
}
