//! Transform interaction controller.
//!
//! Owns gizmo state plus drag lifecycle for translate/rotate/scale so the
//! editor panel does not mutate scene transforms directly.

use glam::{Mat4, Vec3};

use raf_core::scene::graph::{SceneGraph, SceneNodeId};

use crate::gizmo::{GizmoAxis, GizmoMode, GizmoState};
use crate::math::transform;
use crate::picking;

#[derive(Debug)]
pub struct ViewportTransformController {
    gizmo: GizmoState,
    drag_axis: GizmoAxis,
    drag_start_mouse: Option<[f32; 2]>,
    drag_start_pos: Option<Vec3>,
    drag_start_scale: Option<Vec3>,
    drag_start_rotation: Option<Vec3>,
}

impl Default for ViewportTransformController {
    fn default() -> Self {
        Self {
            gizmo: GizmoState::default(),
            drag_axis: GizmoAxis::None,
            drag_start_mouse: None,
            drag_start_pos: None,
            drag_start_scale: None,
            drag_start_rotation: None,
        }
    }
}

impl ViewportTransformController {
    pub fn gizmo(&self) -> &GizmoState {
        &self.gizmo
    }

    pub fn set_mode(&mut self, mode: GizmoMode) {
        self.gizmo.mode = mode;
    }

    pub fn drag_axis(&self) -> GizmoAxis {
        self.drag_axis
    }

    pub fn begin_drag(
        &mut self,
        scene: &SceneGraph,
        selected: Option<SceneNodeId>,
        view_proj: &Mat4,
        pointer_local: [f32; 2],
        vp_w: f32,
        vp_h: f32,
    ) {
        let Some(id) = selected else { return; };
        let Some(node) = scene.get(id) else { return; };
        let entity_pos = scene.world_matrix(id).col(3).truncate();

        let gizmo_hit = match self.gizmo.mode {
            GizmoMode::Rotate => picking::pick_gizmo_rotation_ring(
                pointer_local,
                entity_pos,
                view_proj,
                vp_w,
                vp_h,
            ),
            GizmoMode::Translate | GizmoMode::Scale => picking::pick_gizmo_arrow(
                pointer_local,
                entity_pos,
                view_proj,
                vp_w,
                vp_h,
            ),
        };

        if let Some((axis_idx, _)) = gizmo_hit {
            self.drag_axis = [GizmoAxis::X, GizmoAxis::Y, GizmoAxis::Z][axis_idx];
            self.drag_start_mouse = Some(pointer_local);
            self.drag_start_pos = Some(node.position);
            self.drag_start_scale = Some(node.scale);
            self.drag_start_rotation = Some(node.rotation);
        }
    }

    pub fn apply_drag(
        &mut self,
        scene: &mut SceneGraph,
        selected: Option<SceneNodeId>,
        view_proj: &Mat4,
        current_mouse: [f32; 2],
        orbit_distance: f32,
        vp_w: f32,
        vp_h: f32,
    ) -> bool {
        let Some(id) = selected else { return false; };
        let Some(start_mouse) = self.drag_start_mouse else { return false; };

        let axis_dir = match self.drag_axis {
            GizmoAxis::X => Vec3::X,
            GizmoAxis::Y => Vec3::Y,
            GizmoAxis::Z => Vec3::Z,
            GizmoAxis::None => return false,
        };

        let entity_pos = self.drag_start_pos.unwrap_or(Vec3::ZERO);
        let axis_end_world = entity_pos + axis_dir;
        let origin_screen = transform::project_point(entity_pos, view_proj, vp_w, vp_h);
        let axis_screen = transform::project_point(axis_end_world, view_proj, vp_w, vp_h);
        let (Some((o_s, _)), Some((a_s, _))) = (origin_screen, axis_screen) else { return false; };

        let axis_screen_dir = [a_s[0] - o_s[0], a_s[1] - o_s[1]];
        let axis_len = (axis_screen_dir[0] * axis_screen_dir[0] + axis_screen_dir[1] * axis_screen_dir[1]).sqrt();
        if axis_len < 1.0 {
            return false;
        }

        let mouse_delta = [current_mouse[0] - start_mouse[0], current_mouse[1] - start_mouse[1]];
        let projection = (mouse_delta[0] * axis_screen_dir[0] + mouse_delta[1] * axis_screen_dir[1]) / axis_len;
        let delta = projection * (orbit_distance / (vp_w.min(vp_h) * 0.5));

        match self.gizmo.mode {
            GizmoMode::Translate => {
                if let (Some(node), Some(start_pos)) = (scene.get_mut(id), self.drag_start_pos) {
                    node.position = start_pos + axis_dir * delta;
                }
            }
            GizmoMode::Scale => {
                if let (Some(node), Some(start_scale)) = (scene.get_mut(id), self.drag_start_scale) {
                    let factor = 1.0 + delta * 0.5;
                    let scale_delta = axis_dir * (factor - 1.0);
                    node.scale = (start_scale + scale_delta * start_scale).max(Vec3::splat(0.01));
                }
            }
            GizmoMode::Rotate => {
                if let (Some(node), Some(start_rotation)) = (scene.get_mut(id), self.drag_start_rotation) {
                    node.rotation = start_rotation + axis_dir * (delta * 45.0);
                }
            }
        }

        true
    }

    pub fn end_drag(&mut self) {
        self.drag_axis = GizmoAxis::None;
        self.drag_start_mouse = None;
        self.drag_start_pos = None;
        self.drag_start_scale = None;
        self.drag_start_rotation = None;
    }
}