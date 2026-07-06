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
    drag_scale_sign: f32,
    hover_scale_sign: f32,
    drag_start_mouse: Option<[f32; 2]>,
    drag_start_pos: Option<Vec3>,
    drag_start_scale: Option<Vec3>,
    drag_start_rotation: Option<Vec3>,
    /// Accumulated rotation delta (radians) per axis during the current drag.
    ///
    /// The previous implementation computed rotation from the absolute mouse
    /// delta since drag start (`start_rotation + delta * 45`). When the mouse
    /// crossed the screen-space origin of the axis, the projection flipped
    /// sign and the object "went backwards". By accumulating the incremental
    /// delta every frame instead, the rotation keeps going in the same
    /// direction no matter how far the mouse travels.
    accumulated_rotation: Vec3,
    /// Last pointer position recorded during a drag, used to compute the
    /// incremental delta for rotation.
    last_drag_mouse: Option<[f32; 2]>,
}

impl Default for ViewportTransformController {
    fn default() -> Self {
        Self {
            gizmo: GizmoState::default(),
            drag_axis: GizmoAxis::None,
            drag_scale_sign: 1.0,
            hover_scale_sign: 0.0,
            drag_start_mouse: None,
            drag_start_pos: None,
            drag_start_scale: None,
            drag_start_rotation: None,
            accumulated_rotation: Vec3::ZERO,
            last_drag_mouse: None,
        }
    }
}

impl ViewportTransformController {
    pub fn gizmo(&self) -> &GizmoState {
        &self.gizmo
    }

    pub fn set_mode(&mut self, mode: GizmoMode) {
        self.gizmo.mode = mode;
        self.gizmo.active_axis = GizmoAxis::None;
        self.hover_scale_sign = 0.0;
        self.drag_scale_sign = 1.0;
    }

    pub fn drag_axis(&self) -> GizmoAxis {
        self.drag_axis
    }

    pub fn highlighted_axis(&self) -> GizmoAxis {
        if self.drag_axis != GizmoAxis::None {
            self.drag_axis
        } else {
            self.gizmo.active_axis
        }
    }

    pub fn highlighted_scale_sign(&self) -> f32 {
        if self.drag_axis != GizmoAxis::None && self.gizmo.mode == GizmoMode::Scale {
            self.drag_scale_sign
        } else {
            self.hover_scale_sign
        }
    }

    pub fn update_hover(
        &mut self,
        scene: &SceneGraph,
        selected: Option<SceneNodeId>,
        view_proj: &Mat4,
        pointer_local: [f32; 2],
        vp_w: f32,
        vp_h: f32,
    ) {
        if self.drag_axis != GizmoAxis::None {
            self.gizmo.active_axis = self.drag_axis;
            return;
        }

        let Some(id) = selected else {
            self.gizmo.active_axis = GizmoAxis::None;
            self.hover_scale_sign = 0.0;
            return;
        };
        let Some(_node) = scene.get(id) else {
            self.gizmo.active_axis = GizmoAxis::None;
            self.hover_scale_sign = 0.0;
            return;
        };
        let node = scene.get(id).expect("checked above");
        let entity_pos = scene.world_matrix(id).col(3).truncate();

        let gizmo_hit = match self.gizmo.mode {
            GizmoMode::Rotate => {
                picking::pick_gizmo_rotation_ring(pointer_local, entity_pos, view_proj, vp_w, vp_h)
            }
            GizmoMode::Translate => {
                picking::pick_gizmo_arrow(pointer_local, entity_pos, view_proj, vp_w, vp_h)
            }
            GizmoMode::Scale => {
                let hit = picking::pick_gizmo_scale_handle(
                    pointer_local,
                    entity_pos,
                    node.scale,
                    view_proj,
                    vp_w,
                    vp_h,
                );
                self.hover_scale_sign = hit.map(|(_, _, sign)| sign).unwrap_or(0.0);
                hit.map(|(axis_idx, distance, _)| (axis_idx, distance))
            }
        };

        if self.gizmo.mode != GizmoMode::Scale {
            self.hover_scale_sign = 0.0;
        }

        self.gizmo.active_axis = gizmo_hit
            .map(|(axis_idx, _)| [GizmoAxis::X, GizmoAxis::Y, GizmoAxis::Z][axis_idx])
            .unwrap_or(GizmoAxis::None);
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
        let Some(id) = selected else {
            return;
        };
        let Some(node) = scene.get(id) else {
            return;
        };
        let entity_pos = scene.world_matrix(id).col(3).truncate();

        let gizmo_hit = match self.gizmo.mode {
            GizmoMode::Rotate => {
                picking::pick_gizmo_rotation_ring(pointer_local, entity_pos, view_proj, vp_w, vp_h)
            }
            GizmoMode::Translate => {
                picking::pick_gizmo_arrow(pointer_local, entity_pos, view_proj, vp_w, vp_h)
            }
            GizmoMode::Scale => {
                let hit = picking::pick_gizmo_scale_handle(
                    pointer_local,
                    entity_pos,
                    node.scale,
                    view_proj,
                    vp_w,
                    vp_h,
                );
                self.drag_scale_sign = hit.map(|(_, _, sign)| sign).unwrap_or(1.0);
                hit.map(|(axis_idx, distance, _)| (axis_idx, distance))
            }
        };

        if let Some((axis_idx, _)) = gizmo_hit {
            self.drag_axis = [GizmoAxis::X, GizmoAxis::Y, GizmoAxis::Z][axis_idx];
            self.gizmo.active_axis = self.drag_axis;
            self.drag_start_mouse = Some(pointer_local);
            self.drag_start_pos = Some(node.position);
            self.drag_start_scale = Some(node.scale);
            self.drag_start_rotation = Some(node.rotation);
            self.accumulated_rotation = Vec3::ZERO;
            self.last_drag_mouse = Some(pointer_local);
        }
    }

    pub fn apply_drag(
        &mut self,
        scene: &mut SceneGraph,
        selected: Option<SceneNodeId>,
        view_proj: &Mat4,
        current_mouse: [f32; 2],
        orbit_distance: f32,
        uniform_scale: bool,
        snap_to_ctrl: bool,
        vp_w: f32,
        vp_h: f32,
    ) -> bool {
        let Some(id) = selected else {
            return false;
        };
        let Some(start_mouse) = self.drag_start_mouse else {
            return false;
        };

        let axis_dir = match self.drag_axis {
            GizmoAxis::X => Vec3::X,
            GizmoAxis::Y => Vec3::Y,
            GizmoAxis::Z => Vec3::Z,
            GizmoAxis::None => return false,
        };
        let face_sign = if self.gizmo.mode == GizmoMode::Scale {
            self.drag_scale_sign.signum().max(-1.0)
        } else {
            1.0
        };
        let face_dir = axis_dir * if face_sign == 0.0 { 1.0 } else { face_sign };

        let entity_pos = self.drag_start_pos.unwrap_or(Vec3::ZERO);
        let handle_origin_world = if self.gizmo.mode == GizmoMode::Scale {
            let start_scale = self
                .drag_start_scale
                .unwrap_or(Vec3::ONE)
                .abs()
                .max(Vec3::splat(0.05));
            let handle_offset = Vec3::new(
                if self.drag_axis == GizmoAxis::X {
                    start_scale.x * 0.5 * face_dir.x.signum()
                } else {
                    0.0
                },
                if self.drag_axis == GizmoAxis::Y {
                    start_scale.y * 0.5 * face_dir.y.signum()
                } else {
                    0.0
                },
                if self.drag_axis == GizmoAxis::Z {
                    start_scale.z * 0.5 * face_dir.z.signum()
                } else {
                    0.0
                },
            );
            entity_pos + handle_offset
        } else {
            entity_pos
        };
        let axis_end_world = handle_origin_world + face_dir;
        let origin_screen = transform::project_point(handle_origin_world, view_proj, vp_w, vp_h);
        let axis_screen = transform::project_point(axis_end_world, view_proj, vp_w, vp_h);
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
        let delta = projection * (orbit_distance / (vp_w.min(vp_h) * 0.5));

        match self.gizmo.mode {
            GizmoMode::Translate => {
                if let (Some(node), Some(start_pos)) = (scene.get_mut(id), self.drag_start_pos) {
                    node.position = start_pos + axis_dir * delta;
                }
            }
            GizmoMode::Scale => {
                if let (Some(node), Some(start_scale), Some(start_pos)) = (
                    scene.get_mut(id),
                    self.drag_start_scale,
                    self.drag_start_pos,
                ) {
                    let start_axis_scale = match self.drag_axis {
                        GizmoAxis::X => start_scale.x.abs().max(0.01),
                        GizmoAxis::Y => start_scale.y.abs().max(0.01),
                        GizmoAxis::Z => start_scale.z.abs().max(0.01),
                        GizmoAxis::None => return false,
                    };

                    if uniform_scale {
                        let factor = (1.0 + (delta * 2.0) / start_axis_scale).max(0.05);
                        node.scale = (start_scale * factor).max(Vec3::splat(0.01));
                    } else {
                        let new_axis_scale = (start_axis_scale + delta).max(0.01);
                        let axis_delta = new_axis_scale - start_axis_scale;
                        node.scale = (start_scale + axis_dir * axis_delta).max(Vec3::splat(0.01));
                        node.position = start_pos + face_dir * (axis_delta * 0.5);
                    }
                }
            }
            GizmoMode::Rotate => {
                if let (Some(node), Some(start_rotation)) =
                    (scene.get_mut(id), self.drag_start_rotation)
                {
                    // Accumulate incremental rotation instead of computing from
                    // the absolute mouse delta. The previous code used
                    // `start_rotation + delta * 45` where `delta` was the
                    // projection of (current - start). When the mouse passed
                    // the screen-space axis origin, the projection flipped and
                    // the object rotated backwards. By accumulating the
                    // per-frame delta we keep rotating in the same direction.
                    let last = self.last_drag_mouse.unwrap_or(start_mouse);
                    let inc_mouse = [
                        current_mouse[0] - last[0],
                        current_mouse[1] - last[1],
                    ];
                    let inc_projection = (inc_mouse[0] * axis_screen_dir[0]
                        + inc_mouse[1] * axis_screen_dir[1])
                        / axis_len;
                    let inc_delta = inc_projection * (orbit_distance / (vp_w.min(vp_h) * 0.5));
                    let inc_radians = inc_delta * std::f32::consts::FRAC_PI_4; // 45 deg base
                    self.accumulated_rotation += axis_dir * inc_radians;

                    let mut final_rotation = start_rotation + self.accumulated_rotation;

                    // Snap to 15 degrees when Ctrl is held (Blender/Unity style).
                    if snap_to_ctrl {
                        const SNAP_STEP: f32 = std::f32::consts::PI / 12.0; // 15 deg
                        if self.drag_axis == GizmoAxis::X {
                            final_rotation.x = (final_rotation.x / SNAP_STEP).round() * SNAP_STEP;
                        } else if self.drag_axis == GizmoAxis::Y {
                            final_rotation.y = (final_rotation.y / SNAP_STEP).round() * SNAP_STEP;
                        } else if self.drag_axis == GizmoAxis::Z {
                            final_rotation.z = (final_rotation.z / SNAP_STEP).round() * SNAP_STEP;
                        }
                    }

                    node.rotation = final_rotation;
                    self.last_drag_mouse = Some(current_mouse);
                }
            }
        }

        true
    }

    pub fn end_drag(&mut self) {
        self.drag_axis = GizmoAxis::None;
        self.gizmo.active_axis = GizmoAxis::None;
        self.drag_scale_sign = 1.0;
        self.hover_scale_sign = 0.0;
        self.drag_start_mouse = None;
        self.drag_start_pos = None;
        self.drag_start_scale = None;
        self.drag_start_rotation = None;
        self.accumulated_rotation = Vec3::ZERO;
        self.last_drag_mouse = None;
    }
}
