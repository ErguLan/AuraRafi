//! Renderer-side viewport bridge.
//!
//! Owns camera/navigation state, render orchestration, and edit-session state
//! so the editor host can stay focused on retained layout and painting.

use glam::{Mat4, Vec3};

use raf_core::agent_context::world_bounds;
use raf_core::scene::graph::{SceneGraph, SceneNodeId};

use crate::api_graphic_basic::device::SceneFrameOutput;
use crate::bridge::editor_camera::{EditorCameraBlock, EditorCameraMode};
use crate::bridge::input_handler::{ProjectedEditOverlay, ViewportEditSession};
use crate::bridge::picking_policy::PickingPolicy;
use crate::bridge::render_runtime::RenderRuntime;
use crate::bridge::transform_controller::{AxisDragOutcome, ViewportTransformController};
use crate::camera::{Camera, CameraMode};
use crate::gizmo::{GizmoAxis, GizmoMode, GizmoState};
use crate::scene_renderer::{FrameStats, RenderOptions, SceneRenderFrame, SceneRenderer};

const CAMERA_SNAP_DURATION_S: f32 = 0.22;

fn shortest_angle_delta(from: f32, to: f32) -> f32 {
    (to - from + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

fn normalize_angle(angle: f32) -> f32 {
    (angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

#[derive(Debug, Clone, Copy)]
struct CameraSnapTransition {
    from_yaw: f32,
    to_yaw: f32,
    from_pitch: f32,
    to_pitch: f32,
    elapsed_s: f32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ViewportPointerInput {
    pub pointer_delta: [f32; 2],
    pub scroll_delta_y: f32,
    pub drag_secondary: bool,
    pub drag_middle: bool,
    pub hovered: bool,
    pub move_forward: f32,
    pub move_right: f32,
    pub move_up: f32,
    pub frame_time_s: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct ViewportNavigationConfig {
    pub invert_mouse_x: bool,
    pub invert_mouse_y: bool,
    pub move_sensitivity: f32,
    pub wasd_speed: f32,
    pub rotate_sensitivity: f32,
    pub scale_sensitivity: f32,
}

impl Default for ViewportNavigationConfig {
    fn default() -> Self {
        Self {
            invert_mouse_x: false,
            invert_mouse_y: true,
            move_sensitivity: 3.5,
            wasd_speed: 1.0,
            rotate_sensitivity: 3.5,
            scale_sensitivity: 3.5,
        }
    }
}

pub struct ViewportBridge {
    camera: Camera,
    renderer: SceneRenderer,
    edit_session: ViewportEditSession,
    picking_policy: PickingPolicy,
    transform_controller: ViewportTransformController,
    offset_2d: [f32; 2],
    zoom_2d: f32,
    orbit_yaw: f32,
    orbit_pitch: f32,
    orbit_distance: f32,
    /// Pending smooth-focus target: (target_position, target_distance).
    /// When set, the camera lerps towards this each frame and clears it
    /// once it is close enough. Replaces the old instant `focus_selected`.
    pending_focus: Option<(Vec3, f32)>,
    /// Short, interruptible orientation transition started by the viewport
    /// compass or an equivalent view-snap command.
    pending_camera_snap: Option<CameraSnapTransition>,
}

impl Default for ViewportBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl ViewportBridge {
    pub fn new() -> Self {
        Self {
            camera: Camera::default(),
            renderer: SceneRenderer::new(1, 1),
            edit_session: ViewportEditSession::default(),
            picking_policy: PickingPolicy::default(),
            transform_controller: ViewportTransformController::default(),
            offset_2d: [0.0, 0.0],
            zoom_2d: 1.0,
            orbit_yaw: std::f32::consts::FRAC_PI_4,
            orbit_pitch: 0.5,
            orbit_distance: 8.0,
            pending_focus: None,
            pending_camera_snap: None,
        }
    }

    pub fn camera(&self) -> &Camera {
        &self.camera
    }

    pub fn editor_camera_block(&self) -> EditorCameraBlock {
        EditorCameraBlock {
            mode: match self.camera.mode {
                CameraMode::Orthographic => EditorCameraMode::Orthographic2D,
                CameraMode::Perspective => EditorCameraMode::Orbit,
            },
            target: self.camera.target,
            yaw: self.orbit_yaw,
            pitch: self.orbit_pitch,
            distance: self.orbit_distance,
            offset_2d: self.offset_2d,
            zoom_2d: self.zoom_2d,
            fov_degrees: self.camera.fov,
            near_clip: self.camera.near,
            far_clip: self.camera.far,
            ..EditorCameraBlock::default()
        }
        .sanitized()
    }

    pub fn apply_editor_camera_block(&mut self, block: &EditorCameraBlock) {
        let block = block.clone().sanitized();
        self.camera.target = block.target;
        self.camera.fov = block.fov_degrees;
        self.camera.near = block.near_clip;
        self.camera.far = block.far_clip;
        self.orbit_yaw = block.yaw;
        self.orbit_pitch = block.pitch;
        self.orbit_distance = block.distance;
        self.offset_2d = block.offset_2d;
        self.zoom_2d = block.zoom_2d;
        self.pending_focus = None;
        self.pending_camera_snap = None;
        self.update_camera(block.mode == EditorCameraMode::Orthographic2D);
    }

    pub fn view_projection(&self, width: f32, height: f32) -> Mat4 {
        self.camera.view_projection(width, height)
    }

    pub fn stats(&self) -> &FrameStats {
        &self.renderer.stats
    }

    pub fn picking_policy(&self) -> PickingPolicy {
        self.picking_policy
    }

    pub fn set_picking_policy(&mut self, policy: PickingPolicy) {
        self.picking_policy = policy;
    }

    pub fn orbit_distance(&self) -> f32 {
        self.orbit_distance
    }

    pub fn orbit_yaw(&self) -> f32 {
        self.orbit_yaw
    }

    pub fn orbit_pitch(&self) -> f32 {
        self.orbit_pitch
    }

    pub fn camera_target(&self) -> Vec3 {
        self.camera.target
    }

    pub fn set_camera_target(&mut self, target: Vec3) {
        self.camera.target = target;
        self.pending_focus = None;
        self.pending_camera_snap = None;
    }

    pub fn set_orbit_angles(&mut self, yaw: f32, pitch: f32) {
        self.orbit_yaw = yaw;
        self.orbit_pitch = pitch.clamp(-1.4, 1.4);
        self.pending_camera_snap = None;
    }

    pub fn set_orbit_distance(&mut self, distance: f32) {
        self.orbit_distance = distance.clamp(0.5, 200.0);
        self.pending_camera_snap = None;
    }

    pub fn offset_2d(&self) -> [f32; 2] {
        self.offset_2d
    }

    pub fn zoom_2d(&self) -> f32 {
        self.zoom_2d
    }

    pub fn gizmo(&self) -> &GizmoState {
        self.transform_controller.gizmo()
    }

    pub fn gizmo_mut(&mut self) -> &mut GizmoState {
        self.transform_controller.gizmo_mut()
    }

    pub fn set_gizmo_mode(&mut self, mode: GizmoMode) {
        self.transform_controller.set_mode(mode);
    }

    pub fn active_drag_axis(&self) -> GizmoAxis {
        self.transform_controller.drag_axis()
    }

    pub fn highlighted_gizmo_axis(&self) -> GizmoAxis {
        self.transform_controller.highlighted_axis()
    }

    pub fn highlighted_gizmo_scale_sign(&self) -> f32 {
        self.transform_controller.highlighted_scale_sign()
    }

    pub fn snap_view_to_axis(&mut self, axis: Vec3) {
        let axis = axis.normalize_or_zero();
        if axis.length_squared() <= f32::EPSILON {
            return;
        }

        let (yaw, pitch) = if axis.y.abs() > 0.99 {
            (0.0, 1.35 * axis.y.signum())
        } else {
            (axis.x.atan2(axis.z), axis.y.clamp(-0.97, 0.97).asin())
        };
        self.begin_camera_snap(yaw, pitch);
    }

    pub fn reset_isometric_view(&mut self) {
        self.begin_camera_snap(std::f32::consts::FRAC_PI_4, 0.5);
    }

    fn begin_camera_snap(&mut self, yaw: f32, pitch: f32) {
        self.pending_focus = None;

        let pitch = pitch.clamp(-1.4, 1.4);
        let to_yaw = self.orbit_yaw + shortest_angle_delta(self.orbit_yaw, yaw);
        let yaw_delta = to_yaw - self.orbit_yaw;
        if yaw_delta.abs() <= f32::EPSILON && (pitch - self.orbit_pitch).abs() <= f32::EPSILON {
            self.orbit_yaw = normalize_angle(yaw);
            self.orbit_pitch = pitch;
            self.pending_camera_snap = None;
            return;
        }

        self.pending_camera_snap = Some(CameraSnapTransition {
            from_yaw: self.orbit_yaw,
            to_yaw,
            from_pitch: self.orbit_pitch,
            to_pitch: pitch,
            elapsed_s: 0.0,
        });
    }

    /// Returns whether a focus or orientation transition still owns a camera
    /// update. The controller uses this before advancing a frame so the final
    /// transition frame is rendered even when the animation completes there.
    pub fn has_camera_transition(&self) -> bool {
        self.pending_focus.is_some() || self.pending_camera_snap.is_some()
    }

    /// Advances the camera transitions that are driven by the editor frame
    /// loop. Compass snaps use a short smoothstep curve and can be interrupted
    /// by any direct camera input without forcing the user through the rest of
    /// the animation.
    pub fn update_camera_transitions(&mut self, delta_seconds: f32) -> bool {
        let focus_active = self.update_smooth_focus();
        let snap_active = self.update_camera_snap(delta_seconds);
        focus_active || snap_active
    }

    fn update_camera_snap(&mut self, delta_seconds: f32) -> bool {
        let Some(mut transition) = self.pending_camera_snap.take() else {
            return false;
        };

        transition.elapsed_s =
            (transition.elapsed_s + delta_seconds.max(0.0).min(0.25)).min(CAMERA_SNAP_DURATION_S);
        let t = (transition.elapsed_s / CAMERA_SNAP_DURATION_S).clamp(0.0, 1.0);
        let eased = t * t * (3.0 - 2.0 * t);
        self.orbit_yaw = transition.from_yaw + (transition.to_yaw - transition.from_yaw) * eased;
        self.orbit_pitch =
            transition.from_pitch + (transition.to_pitch - transition.from_pitch) * eased;

        if t >= 1.0 {
            self.orbit_yaw = normalize_angle(transition.to_yaw);
            self.orbit_pitch = transition.to_pitch;
            false
        } else {
            self.pending_camera_snap = Some(transition);
            true
        }
    }

    pub fn edit_session(&self) -> &ViewportEditSession {
        &self.edit_session
    }

    pub fn edit_session_mut(&mut self) -> &mut ViewportEditSession {
        &mut self.edit_session
    }

    pub fn clear_edit_drag_state(&mut self) {
        self.edit_session.clear_drag_state();
    }

    pub fn prepare_selected_edit_mesh(
        &mut self,
        scene: &SceneGraph,
        selected: Option<SceneNodeId>,
    ) {
        self.edit_session.prepare_selected_mesh(scene, selected);
    }

    pub fn handle_edit_selection_click(
        &mut self,
        scene: &SceneGraph,
        selected: Option<SceneNodeId>,
        view_proj: &Mat4,
        vp_w: f32,
        vp_h: f32,
        click_local: [f32; 2],
        shift: bool,
    ) -> bool {
        self.edit_session.handle_selection_click(
            scene,
            selected,
            view_proj,
            vp_w,
            vp_h,
            click_local,
            shift,
        )
    }

    pub fn begin_edit_drag(
        &mut self,
        scene: &SceneGraph,
        selected: Option<SceneNodeId>,
        view_proj: &Mat4,
        vp_w: f32,
        vp_h: f32,
        pointer_local: [f32; 2],
    ) {
        self.edit_session
            .begin_drag(scene, selected, view_proj, vp_w, vp_h, pointer_local);
    }

    pub fn drag_selected_vertices(
        &mut self,
        scene: &SceneGraph,
        selected: Option<SceneNodeId>,
        move_sensitivity: f32,
        current_pointer: [f32; 2],
    ) -> bool {
        self.edit_session.drag_selected_vertices(
            scene,
            selected,
            &self.camera,
            self.orbit_distance,
            move_sensitivity,
            current_pointer,
        )
    }

    pub fn project_edit_overlay(
        &self,
        scene: &SceneGraph,
        selected: Option<SceneNodeId>,
        view_proj: &Mat4,
        vp_w: f32,
        vp_h: f32,
    ) -> Option<ProjectedEditOverlay> {
        self.edit_session
            .project_overlay(scene, selected, view_proj, vp_w, vp_h)
    }

    pub fn pick_entity(
        &self,
        scene: &SceneGraph,
        view_proj: &Mat4,
        screen_x: f32,
        screen_y: f32,
        vp_w: f32,
        vp_h: f32,
    ) -> Option<SceneNodeId> {
        self.edit_session.pick_entity_with_policy(
            scene,
            view_proj,
            screen_x,
            screen_y,
            vp_w,
            vp_h,
            self.picking_policy,
        )
    }

    pub fn begin_transform_drag(
        &mut self,
        scene: &SceneGraph,
        selected: Option<SceneNodeId>,
        view_proj: &Mat4,
        pointer_local: [f32; 2],
        vp_w: f32,
        vp_h: f32,
    ) {
        self.transform_controller
            .begin_drag(scene, selected, view_proj, pointer_local, vp_w, vp_h);
    }

    pub fn begin_transform_drag_scaled(
        &mut self,
        scene: &SceneGraph,
        selected: Option<SceneNodeId>,
        view_proj: &Mat4,
        pointer_local: [f32; 2],
        vp_w: f32,
        vp_h: f32,
        presentation_scale: f32,
    ) {
        self.transform_controller.begin_drag_scaled(
            scene,
            selected,
            view_proj,
            pointer_local,
            vp_w,
            vp_h,
            presentation_scale,
        );
    }

    pub fn update_transform_hover(
        &mut self,
        scene: &SceneGraph,
        selected: Option<SceneNodeId>,
        view_proj: &Mat4,
        pointer_local: [f32; 2],
        vp_w: f32,
        vp_h: f32,
    ) {
        self.transform_controller.update_hover(
            scene,
            selected,
            view_proj,
            pointer_local,
            vp_w,
            vp_h,
        );
    }

    pub fn update_transform_hover_scaled(
        &mut self,
        scene: &SceneGraph,
        selected: Option<SceneNodeId>,
        view_proj: &Mat4,
        pointer_local: [f32; 2],
        vp_w: f32,
        vp_h: f32,
        presentation_scale: f32,
    ) {
        self.transform_controller.update_hover_scaled(
            scene,
            selected,
            view_proj,
            pointer_local,
            vp_w,
            vp_h,
            presentation_scale,
        );
    }

    pub fn update_transform_hover_world(
        &mut self,
        origin: Vec3,
        entity_scale: Vec3,
        view_proj: &Mat4,
        pointer_local: [f32; 2],
        vp_w: f32,
        vp_h: f32,
        presentation_scale: f32,
    ) {
        self.transform_controller.update_hover_world(
            origin,
            entity_scale,
            view_proj,
            pointer_local,
            vp_w,
            vp_h,
            presentation_scale,
        );
    }

    pub fn begin_transform_drag_world(
        &mut self,
        origin: Vec3,
        entity_scale: Vec3,
        view_proj: &Mat4,
        pointer_local: [f32; 2],
        vp_w: f32,
        vp_h: f32,
        presentation_scale: f32,
    ) {
        self.transform_controller.begin_drag_world(
            origin,
            entity_scale,
            view_proj,
            pointer_local,
            vp_w,
            vp_h,
            presentation_scale,
        );
    }

    /// Resolves the active drag into axis-space quantities without touching
    /// scene nodes. Used by multi-selection drags where the editor owns every
    /// member transform.
    pub fn compute_axis_drag(
        &mut self,
        view_proj: &Mat4,
        current_mouse: [f32; 2],
        vp_w: f32,
        vp_h: f32,
    ) -> Option<AxisDragOutcome> {
        self.transform_controller.compute_axis_drag(
            view_proj,
            current_mouse,
            self.orbit_distance,
            vp_w,
            vp_h,
        )
    }

    pub fn apply_transform_drag(
        &mut self,
        scene: &mut SceneGraph,
        selected: Option<SceneNodeId>,
        view_proj: &Mat4,
        current_mouse: [f32; 2],
        uniform_scale: bool,
        snap_to_ctrl: bool,
        vp_w: f32,
        vp_h: f32,
    ) -> bool {
        self.transform_controller.apply_drag(
            scene,
            selected,
            view_proj,
            current_mouse,
            self.orbit_distance,
            uniform_scale,
            snap_to_ctrl,
            vp_w,
            vp_h,
        )
    }

    pub fn end_transform_drag(&mut self) {
        self.transform_controller.end_drag();
    }

    pub fn handle_camera_input(
        &mut self,
        input: ViewportPointerInput,
        is_2d: bool,
        config: ViewportNavigationConfig,
    ) {
        let pointer_delta = Vec3::new(input.pointer_delta[0], input.pointer_delta[1], 0.0);
        let direct_camera_input = input.drag_secondary
            || input.drag_middle
            || (input.hovered
                && (input.move_forward.abs() > f32::EPSILON
                    || input.move_right.abs() > f32::EPSILON
                    || input.move_up.abs() > f32::EPSILON
                    || input.scroll_delta_y.abs() > 0.01));
        if direct_camera_input {
            self.pending_camera_snap = None;
            self.pending_focus = None;
        }

        if is_2d {
            if input.drag_secondary || input.drag_middle {
                let pan_scale = 0.01 * (self.camera.ortho_scale / self.zoom_2d.max(0.1));
                self.offset_2d[0] -= pointer_delta.x * pan_scale;
                self.offset_2d[1] += pointer_delta.y * pan_scale;
            }

            if input.hovered && input.scroll_delta_y.abs() > 0.01 {
                // Positive wheel motion means wheel-up and zooms toward the
                // scene.
                self.zoom_2d *= 1.0 + input.scroll_delta_y * 0.0015 * config.scale_sensitivity;
                self.zoom_2d = self.zoom_2d.clamp(0.1, 50.0);
            }

            return;
        }

        if input.drag_secondary {
            let x_factor = if config.invert_mouse_x { 1.0 } else { -1.0 };
            let y_factor = if config.invert_mouse_y { -1.0 } else { 1.0 };

            self.orbit_yaw += pointer_delta.x * 0.005 * config.rotate_sensitivity * x_factor;
            let pitch_delta = pointer_delta.y * 0.005 * config.rotate_sensitivity * y_factor;
            // Soft-clamp pitch near limits to avoid bounce.
            if (self.orbit_pitch > 1.3 && pitch_delta > 0.0)
                || (self.orbit_pitch < -1.3 && pitch_delta < 0.0)
            {
                let remaining = if pitch_delta > 0.0 {
                    1.4 - self.orbit_pitch
                } else {
                    -1.4 - self.orbit_pitch
                };
                self.orbit_pitch += remaining.signum() * remaining.abs().min(pitch_delta.abs());
            } else {
                self.orbit_pitch += pitch_delta;
            }
            self.orbit_pitch = self.orbit_pitch.clamp(-1.4, 1.4);
        }

        if input.drag_middle {
            let right = Vec3::new(self.orbit_yaw.cos(), 0.0, -self.orbit_yaw.sin());
            let up = Vec3::Y;
            let pan_speed = self.orbit_distance * 0.002 * config.move_sensitivity;
            self.camera.target -= right * pointer_delta.x * pan_speed;
            self.camera.target += up * pointer_delta.y * pan_speed;
        }

        if input.hovered {
            let dt = input.frame_time_s.max(1.0 / 240.0).min(1.0 / 15.0);
            let forward =
                Vec3::new(self.orbit_yaw.sin(), 0.0, self.orbit_yaw.cos()).normalize_or_zero();
            let right =
                Vec3::new(self.orbit_yaw.cos(), 0.0, -self.orbit_yaw.sin()).normalize_or_zero();
            let up = Vec3::Y;
            // `forward` is the target-to-camera vector in orbit space. Moving
            // the camera forward therefore moves the orbit target in the
            // opposite direction. Scale fly speed with orbit distance so 1x
            // covers a useful fraction of the current view each second. The
            // previous sqrt(distance) * 0.05 scale reduced the initial 8-unit
            // view to roughly 0.14 units/second and made normal 0.5x settings
            // look completely frozen in the real editor.
            let move_speed = self.orbit_distance.clamp(2.0, 200.0)
                * 0.85
                * config.wasd_speed.clamp(0.05, 5.0)
                * dt;
            self.camera.target -= forward * input.move_forward * move_speed;
            self.camera.target += right * input.move_right * move_speed;
            self.camera.target += up * input.move_up * move_speed;
        }

        if input.hovered && input.scroll_delta_y.abs() > 0.01 {
            // Positive wheel motion means wheel-up and reduces the orbit
            // distance to zoom toward the scene.
            self.orbit_distance *= 1.0 - input.scroll_delta_y * 0.001 * config.scale_sensitivity;
            self.orbit_distance = self.orbit_distance.clamp(0.5, 200.0);
        }
    }

    pub fn update_camera(&mut self, is_2d: bool) {
        if is_2d {
            self.camera.mode = CameraMode::Orthographic;
            self.camera.up = Vec3::Y;
            self.camera.target = Vec3::new(self.offset_2d[0], self.offset_2d[1], 0.0);
            self.camera.position = self.camera.target + Vec3::new(0.0, 0.0, 10.0);
            self.camera.ortho_scale = (10.0 / self.zoom_2d.max(0.1)).clamp(0.2, 200.0);
            return;
        }

        self.camera.mode = CameraMode::Perspective;
        let x = self.orbit_distance * self.orbit_pitch.cos() * self.orbit_yaw.sin();
        let y = self.orbit_distance * self.orbit_pitch.sin();
        let z = self.orbit_distance * self.orbit_pitch.cos() * self.orbit_yaw.cos();
        self.camera.position = self.camera.target + Vec3::new(x, y, z);
    }

    pub fn focus_selected(
        &mut self,
        scene: &SceneGraph,
        selected: Option<SceneNodeId>,
        is_2d: bool,
    ) {
        let Some(id) = selected else {
            return;
        };
        let Some(_node) = scene.get(id) else {
            return;
        };

        let world = scene.world_matrix(id);
        let fallback_center = world.col(3).truncate();
        let (center, max_extent) = world_bounds(scene, id)
            .map(|(min, max)| {
                let size = max - min;
                (
                    (min + max) * 0.5,
                    size.x.max(size.y).max(size.z).max(0.0001),
                )
            })
            .unwrap_or_else(|| {
                let extent = world
                    .x_axis
                    .truncate()
                    .length()
                    .max(world.y_axis.truncate().length())
                    .max(world.z_axis.truncate().length())
                    .max(0.0001);
                (fallback_center, extent)
            });

        if is_2d {
            self.pending_camera_snap = None;
            self.offset_2d = [center.x, center.y];
            self.zoom_2d = (4.0 / max_extent.max(0.25)).clamp(0.2, 25.0);
        } else {
            let target_distance = (max_extent * 3.0).clamp(1.5, 40.0);
            // Queue a smooth focus instead of snapping instantly.
            self.pending_camera_snap = None;
            self.pending_focus = Some((center, target_distance));
        }
    }

    /// Advance the smooth-focus animation. Call this once per frame before
    /// rendering. Returns true if the camera is still animating.
    pub fn update_smooth_focus(&mut self) -> bool {
        let Some((target_pos, target_dist)) = self.pending_focus else {
            return false;
        };

        const LERP_FACTOR: f32 = 0.15;
        const SNAP_EPS: f32 = 0.01;

        self.camera.target = self.camera.target.lerp(target_pos, LERP_FACTOR);
        self.orbit_distance =
            self.orbit_distance + (target_dist - self.orbit_distance) * LERP_FACTOR;

        let pos_close = self.camera.target.distance(target_pos) < SNAP_EPS;
        let dist_close = (self.orbit_distance - target_dist).abs() < SNAP_EPS * 10.0;
        if pos_close && dist_close {
            self.camera.target = target_pos;
            self.orbit_distance = target_dist;
            self.pending_focus = None;
            false
        } else {
            true
        }
    }

    pub fn render(
        &mut self,
        render_runtime: &mut RenderRuntime,
        scene: &SceneGraph,
        vp_w: f32,
        vp_h: f32,
        selected: &[SceneNodeId],
        bg_color: [u8; 4],
        light_dir: Vec3,
        options: RenderOptions,
        vertex_edit_enabled: bool,
    ) -> SceneFrameOutput {
        let frame = self.build_scene_frame(
            scene,
            vp_w,
            vp_h,
            selected,
            bg_color,
            light_dir,
            options,
            vertex_edit_enabled,
        );
        render_runtime.render_scene_frame(&frame)
    }

    /// Builds a renderer-neutral frame for a viewport surface.
    ///
    /// Keeping this separate from presentation lets a retained surface reuse
    /// the command frame while the scene, camera, and render options remain
    /// unchanged. The existing `render` method remains as the convenient
    /// immediate path for callers that do not host a retained surface.
    pub fn build_scene_frame(
        &mut self,
        scene: &SceneGraph,
        vp_w: f32,
        vp_h: f32,
        selected: &[SceneNodeId],
        bg_color: [u8; 4],
        light_dir: Vec3,
        options: RenderOptions,
        vertex_edit_enabled: bool,
    ) -> SceneRenderFrame {
        let mesh_override = if vertex_edit_enabled {
            self.edit_session
                .mesh_override(scene, selected.first().copied())
        } else {
            None
        };

        self.renderer.build_frame(
            scene,
            &self.camera,
            vp_w,
            vp_h,
            selected,
            bg_color,
            light_dir,
            options,
            mesh_override.as_ref().map(|(id, mesh)| (*id, mesh)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn angle_error(actual: f32, expected: f32) -> f32 {
        shortest_angle_delta(expected, actual).abs()
    }

    #[test]
    fn compass_snap_interpolates_before_settling_on_axis() {
        let mut bridge = ViewportBridge::new();
        bridge.set_orbit_angles(0.0, 0.0);

        bridge.snap_view_to_axis(Vec3::X);

        assert!(bridge.has_camera_transition());
        assert_eq!(bridge.orbit_yaw(), 0.0);
        assert_eq!(bridge.orbit_pitch(), 0.0);

        assert!(bridge.update_camera_transitions(CAMERA_SNAP_DURATION_S * 0.5));
        assert!(bridge.orbit_yaw() > 0.0);
        assert!(bridge.orbit_yaw() < std::f32::consts::FRAC_PI_2);

        assert!(!bridge.update_camera_transitions(CAMERA_SNAP_DURATION_S));
        assert!(!bridge.has_camera_transition());
        assert!(angle_error(bridge.orbit_yaw(), std::f32::consts::FRAC_PI_2) < 0.0001);
        assert!(angle_error(bridge.orbit_pitch(), 0.0) < 0.0001);
    }

    #[test]
    fn compass_snap_uses_the_shortest_yaw_path() {
        let mut bridge = ViewportBridge::new();
        bridge.set_orbit_angles(std::f32::consts::PI - 0.1, 0.0);

        bridge.snap_view_to_axis(Vec3::new(-1.0, 0.0, -0.1));
        assert!(bridge.has_camera_transition());

        let starting_yaw = bridge.orbit_yaw();
        bridge.update_camera_transitions(CAMERA_SNAP_DURATION_S * 0.5);
        assert!(bridge.orbit_yaw() > starting_yaw);

        while bridge.has_camera_transition() {
            bridge.update_camera_transitions(CAMERA_SNAP_DURATION_S);
        }
        assert!(angle_error(bridge.orbit_yaw(), (-1.0f32).atan2(-0.1)) < 0.0001);
    }

    #[test]
    fn direct_camera_input_interrupts_compass_snap() {
        let mut bridge = ViewportBridge::new();
        bridge.snap_view_to_axis(Vec3::X);
        assert!(bridge.has_camera_transition());

        bridge.handle_camera_input(
            ViewportPointerInput {
                hovered: true,
                move_forward: 1.0,
                frame_time_s: 1.0 / 60.0,
                ..ViewportPointerInput::default()
            },
            false,
            ViewportNavigationConfig::default(),
        );

        assert!(!bridge.has_camera_transition());
    }
}
