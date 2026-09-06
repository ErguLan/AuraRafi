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
    drag_start_world_pos: Option<Vec3>,
    drag_start_world_scale: Vec3,
    drag_world_axes: [Vec3; 3],
    drag_parent_inverse: Mat4,
    drag_presentation_scale: f32,
    /// Accumulated rotation delta (radians) per axis during the current drag.
    ///
    /// Each frame contributes the shortest signed delta between two angular
    /// parameters on the projected rotation ring. This can grow beyond one
    /// complete turn without wrapping the authored transform.
    accumulated_rotation: Vec3,
    /// Last angular parameter on the active ring, in radians.
    last_rotation_parameter: Option<f32>,
}

/// One frame of gizmo drag math computed against an arbitrary gizmo origin.
///
/// Multi-selection drags use this instead of [`Self::apply_drag`] because the
/// editor controller owns every member transform; the renderer-side controller
/// only resolves pointer motion into axis-space quantities.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AxisDragOutcome {
    /// Signed world-units advance along the dragged axis (translate/scale).
    pub axis_delta: f32,
    /// Signed accumulated rotation in radians around the dragged axis.
    pub rotation_radians: f32,
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
            drag_start_world_pos: None,
            drag_start_world_scale: Vec3::ONE,
            drag_world_axes: [Vec3::X, Vec3::Y, Vec3::Z],
            drag_parent_inverse: Mat4::IDENTITY,
            drag_presentation_scale: 1.0,
            accumulated_rotation: Vec3::ZERO,
            last_rotation_parameter: None,
        }
    }
}

impl ViewportTransformController {
    pub fn gizmo(&self) -> &GizmoState {
        &self.gizmo
    }

    pub fn gizmo_mut(&mut self) -> &mut GizmoState {
        &mut self.gizmo
    }

    pub fn set_mode(&mut self, mode: GizmoMode) {
        // A mode switch during a drag must not leave the previous controller
        // owning the pointer. The next gesture should start from a clean
        // axis/start-transform state.
        if self.drag_axis != GizmoAxis::None {
            self.end_drag();
        }
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
        self.update_hover_scaled(scene, selected, view_proj, pointer_local, vp_w, vp_h, 1.0);
    }

    pub fn update_hover_scaled(
        &mut self,
        scene: &SceneGraph,
        selected: Option<SceneNodeId>,
        view_proj: &Mat4,
        pointer_local: [f32; 2],
        vp_w: f32,
        vp_h: f32,
        presentation_scale: f32,
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
        let Some(_) = scene.get(id) else {
            self.gizmo.active_axis = GizmoAxis::None;
            self.hover_scale_sign = 0.0;
            return;
        };
        let world = scene.world_matrix(id);
        let entity_pos = world.col(3).truncate();
        let (entity_axes, entity_scale) = picking::gizmo_scale_basis(world);

        let gizmo_hit = match self.gizmo.mode {
            GizmoMode::Rotate => picking::pick_gizmo_rotation_ring_scaled(
                pointer_local,
                entity_pos,
                presentation_scale,
                view_proj,
                vp_w,
                vp_h,
            ),
            GizmoMode::Translate => picking::pick_gizmo_arrow_scaled(
                pointer_local,
                entity_pos,
                presentation_scale,
                view_proj,
                vp_w,
                vp_h,
            ),
            GizmoMode::Scale => {
                let hit = picking::pick_gizmo_scale_handle_scaled_oriented(
                    pointer_local,
                    entity_pos,
                    entity_scale,
                    entity_axes,
                    presentation_scale,
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
        self.begin_drag_scaled(scene, selected, view_proj, pointer_local, vp_w, vp_h, 1.0);
    }

    pub fn begin_drag_scaled(
        &mut self,
        scene: &SceneGraph,
        selected: Option<SceneNodeId>,
        view_proj: &Mat4,
        pointer_local: [f32; 2],
        vp_w: f32,
        vp_h: f32,
        presentation_scale: f32,
    ) {
        let Some(id) = selected else {
            return;
        };
        let Some(node) = scene.get(id) else {
            return;
        };
        let world = scene.world_matrix(id);
        let entity_pos = world.col(3).truncate();
        let (entity_axes, entity_scale) = picking::gizmo_scale_basis(world);
        let parent_inverse = node
            .parent
            .map(|parent| stable_inverse(scene.world_matrix(parent)))
            .unwrap_or(Mat4::IDENTITY);

        let gizmo_hit = match self.gizmo.mode {
            GizmoMode::Rotate => picking::pick_gizmo_rotation_ring_scaled(
                pointer_local,
                entity_pos,
                presentation_scale,
                view_proj,
                vp_w,
                vp_h,
            ),
            GizmoMode::Translate => picking::pick_gizmo_arrow_scaled(
                pointer_local,
                entity_pos,
                presentation_scale,
                view_proj,
                vp_w,
                vp_h,
            ),
            GizmoMode::Scale => {
                let picked = picking::pick_gizmo_scale_handle_scaled_oriented(
                    pointer_local,
                    entity_pos,
                    entity_scale,
                    entity_axes,
                    presentation_scale,
                    view_proj,
                    vp_w,
                    vp_h,
                );
                // Hover is resolved immediately before pointer dispatch in the
                // editor. Prefer that axis when it is available so a handle
                // that is visibly lit cannot lose the press to entity drag
                // because the pointer moved a fraction of a pixel between
                // frames.
                let hover_sign = if self.hover_scale_sign == 0.0 {
                    1.0
                } else {
                    self.hover_scale_sign.signum()
                };
                let hovered = match self.gizmo.active_axis {
                    GizmoAxis::X => Some((0, 0.0, hover_sign)),
                    GizmoAxis::Y => Some((1, 0.0, hover_sign)),
                    GizmoAxis::Z => Some((2, 0.0, hover_sign)),
                    GizmoAxis::None => None,
                };
                let hit = hovered.or(picked);
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
            self.drag_start_world_pos = Some(entity_pos);
            self.drag_start_world_scale = entity_scale;
            self.drag_world_axes = entity_axes;
            self.drag_parent_inverse = parent_inverse;
            self.drag_presentation_scale = presentation_scale.max(0.1);
            self.accumulated_rotation = Vec3::ZERO;
            self.last_rotation_parameter = if self.gizmo.mode == GizmoMode::Rotate {
                picking::rotation_ring_parameter_scaled(
                    pointer_local,
                    entity_pos,
                    axis_idx,
                    self.drag_presentation_scale,
                    view_proj,
                    vp_w,
                    vp_h,
                    None,
                )
            } else {
                None
            };
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
        let Some(axis_index) = gizmo_axis_index(self.drag_axis) else {
            return false;
        };
        let axis_dir = axis_vector(axis_index);

        if self.gizmo.mode == GizmoMode::Rotate {
            let Some(rotation_radians) =
                self.advance_rotation_drag(view_proj, current_mouse, vp_w, vp_h)
            else {
                return false;
            };
            if let (Some(node), Some(start_rotation)) =
                (scene.get_mut(id), self.drag_start_rotation)
            {
                // SceneNode stores editor Euler values in degrees. The ring
                // math remains radians and is converted only at this boundary.
                let mut delta_degrees = rotation_radians.to_degrees();
                if snap_to_ctrl {
                    const SNAP_DEGREES: f32 = 15.0;
                    delta_degrees = (delta_degrees / SNAP_DEGREES).round() * SNAP_DEGREES;
                }
                let mut final_rotation = start_rotation;
                set_axis_component(
                    &mut final_rotation,
                    axis_index,
                    axis_component(start_rotation, axis_index) + delta_degrees,
                );
                node.rotation = final_rotation;
            }
            return true;
        }

        let face_sign = if self.gizmo.mode == GizmoMode::Scale {
            normalized_sign(self.drag_scale_sign)
        } else {
            1.0
        };
        let world_axis = if self.gizmo.mode == GizmoMode::Scale {
            normalized_axis_or(self.drag_world_axes[axis_index], axis_dir)
        } else {
            axis_dir
        };
        let face_dir = world_axis * face_sign;

        let entity_pos = self
            .drag_start_world_pos
            .or(self.drag_start_pos)
            .unwrap_or(Vec3::ZERO);
        let handle_origin_world = if self.gizmo.mode == GizmoMode::Scale {
            picking::gizmo_scale_handle_world_position(
                entity_pos,
                self.drag_start_world_scale,
                self.drag_world_axes,
                axis_index,
                face_sign,
            )
        } else {
            entity_pos
        };
        let Some((axis_screen_dir, axis_len)) =
            screen_axis_basis(handle_origin_world, face_dir, view_proj, vp_w, vp_h)
        else {
            return false;
        };
        if axis_len < 1.0 {
            return false;
        }

        let delta = pointer_axis_projection(start_mouse, current_mouse, &axis_screen_dir, axis_len)
            * (orbit_distance / (vp_w.min(vp_h) * 0.5));

        match self.gizmo.mode {
            GizmoMode::Translate => {
                if let (Some(node), Some(start_pos)) = (scene.get_mut(id), self.drag_start_pos) {
                    let snap_step = if snap_to_ctrl { 1.0 } else { 0.0 };
                    let snapped_delta = if snap_step > 0.0 {
                        (delta / snap_step).round() * snap_step
                    } else {
                        delta
                    };
                    node.position = start_pos + axis_dir * snapped_delta;
                }
            }
            GizmoMode::Scale => {
                if let (Some(node), Some(start_scale), Some(start_pos)) = (
                    scene.get_mut(id),
                    self.drag_start_scale,
                    self.drag_start_pos,
                ) {
                    let start_axis_value = axis_component(start_scale, axis_index);
                    let start_axis_scale = start_axis_value.abs().max(0.01);
                    let start_world_axis_scale =
                        axis_component(self.drag_start_world_scale, axis_index)
                            .abs()
                            .max(0.01);
                    let local_delta = delta * (start_axis_scale / start_world_axis_scale);
                    let new_axis_scale = (start_axis_scale + local_delta).max(0.01);
                    let snapped = if snap_to_ctrl {
                        let step = 0.5;
                        ((new_axis_scale / step).round() * step).max(0.01)
                    } else {
                        new_axis_scale
                    };
                    if uniform_scale {
                        let factor = (snapped / start_axis_scale).max(0.05);
                        node.scale = scale_with_min_magnitude(start_scale * factor, 0.01);
                    } else {
                        let local_axis_delta = snapped - start_axis_scale;
                        let mut next_scale = start_scale;
                        let authored_sign = if start_axis_value < 0.0 { -1.0 } else { 1.0 };
                        set_axis_component(&mut next_scale, axis_index, snapped * authored_sign);
                        node.scale = scale_with_min_magnitude(next_scale, 0.01);

                        let world_axis_delta =
                            local_axis_delta * (start_world_axis_scale / start_axis_scale);
                        let world_offset = world_axis * (world_axis_delta * 0.5 * face_sign);
                        let local_offset = self.drag_parent_inverse.transform_vector3(world_offset);
                        node.position = start_pos + local_offset;
                    }
                }
            }
            GizmoMode::Rotate => unreachable!("rotation returns before linear drag math"),
        }

        true
    }

    /// Hover hit-testing against an explicit gizmo origin, used when the
    /// gizmo represents a multi-selection group anchored at its centroid.
    pub fn update_hover_world(
        &mut self,
        origin: Vec3,
        entity_scale: Vec3,
        view_proj: &Mat4,
        pointer_local: [f32; 2],
        vp_w: f32,
        vp_h: f32,
        presentation_scale: f32,
    ) {
        if self.drag_axis != GizmoAxis::None {
            self.gizmo.active_axis = self.drag_axis;
            return;
        }
        let gizmo_hit = match self.gizmo.mode {
            GizmoMode::Rotate => picking::pick_gizmo_rotation_ring_scaled(
                pointer_local,
                origin,
                presentation_scale,
                view_proj,
                vp_w,
                vp_h,
            ),
            GizmoMode::Translate => picking::pick_gizmo_arrow_scaled(
                pointer_local,
                origin,
                presentation_scale,
                view_proj,
                vp_w,
                vp_h,
            ),
            GizmoMode::Scale => {
                let hit = picking::pick_gizmo_scale_handle(
                    pointer_local,
                    origin,
                    entity_scale,
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

    /// Begins a drag against an explicit gizmo origin without reading member
    /// transforms from the scene. Member start states are owned by the
    /// editor-side controller.
    pub fn begin_drag_world(
        &mut self,
        origin: Vec3,
        entity_scale: Vec3,
        view_proj: &Mat4,
        pointer_local: [f32; 2],
        vp_w: f32,
        vp_h: f32,
        presentation_scale: f32,
    ) {
        let gizmo_hit = match self.gizmo.mode {
            GizmoMode::Rotate => picking::pick_gizmo_rotation_ring_scaled(
                pointer_local,
                origin,
                presentation_scale,
                view_proj,
                vp_w,
                vp_h,
            ),
            GizmoMode::Translate => picking::pick_gizmo_arrow_scaled(
                pointer_local,
                origin,
                presentation_scale,
                view_proj,
                vp_w,
                vp_h,
            ),
            GizmoMode::Scale => {
                let hit = picking::pick_gizmo_scale_handle_scaled(
                    pointer_local,
                    origin,
                    entity_scale,
                    presentation_scale,
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
            self.drag_start_pos = Some(origin);
            self.drag_start_scale = Some(entity_scale);
            self.drag_start_rotation = Some(Vec3::ZERO);
            self.drag_start_world_pos = Some(origin);
            self.drag_start_world_scale = entity_scale.abs().max(Vec3::splat(0.01));
            self.drag_world_axes = [Vec3::X, Vec3::Y, Vec3::Z];
            self.drag_parent_inverse = Mat4::IDENTITY;
            self.drag_presentation_scale = presentation_scale.max(0.1);
            self.accumulated_rotation = Vec3::ZERO;
            self.last_rotation_parameter = if self.gizmo.mode == GizmoMode::Rotate {
                picking::rotation_ring_parameter_scaled(
                    pointer_local,
                    origin,
                    axis_idx,
                    self.drag_presentation_scale,
                    view_proj,
                    vp_w,
                    vp_h,
                    None,
                )
            } else {
                None
            };
        }
    }

    /// Resolves the current pointer into axis-space drag quantities without
    /// mutating any scene node. Returns `None` when no drag is active or the
    /// axis projects degenerately this frame.
    ///
    /// Rotation accumulates internally exactly like [`Self::apply_drag`], so
    /// alternating between both entry points mid-gesture stays consistent.
    pub fn compute_axis_drag(
        &mut self,
        view_proj: &Mat4,
        current_mouse: [f32; 2],
        orbit_distance: f32,
        vp_w: f32,
        vp_h: f32,
    ) -> Option<AxisDragOutcome> {
        let axis_index = gizmo_axis_index(self.drag_axis)?;
        let start_mouse = self.drag_start_mouse?;
        if self.gizmo.mode == GizmoMode::Rotate {
            return self
                .advance_rotation_drag(view_proj, current_mouse, vp_w, vp_h)
                .map(|rotation_radians| AxisDragOutcome {
                    axis_delta: 0.0,
                    rotation_radians,
                });
        }

        let axis_dir = axis_vector(axis_index);
        let face_sign = if self.gizmo.mode == GizmoMode::Scale {
            normalized_sign(self.drag_scale_sign)
        } else {
            1.0
        };
        let world_axis = if self.gizmo.mode == GizmoMode::Scale {
            normalized_axis_or(self.drag_world_axes[axis_index], axis_dir)
        } else {
            axis_dir
        };
        let face_dir = world_axis * face_sign;

        let entity_pos = self
            .drag_start_world_pos
            .or(self.drag_start_pos)
            .unwrap_or(Vec3::ZERO);
        let handle_origin_world = if self.gizmo.mode == GizmoMode::Scale {
            picking::gizmo_scale_handle_world_position(
                entity_pos,
                self.drag_start_world_scale,
                self.drag_world_axes,
                axis_index,
                face_sign,
            )
        } else {
            entity_pos
        };

        let (axis_screen_dir, axis_len) =
            screen_axis_basis(handle_origin_world, face_dir, view_proj, vp_w, vp_h)?;
        if axis_len < 1.0 {
            return None;
        }

        let delta = pointer_axis_projection(start_mouse, current_mouse, &axis_screen_dir, axis_len)
            * (orbit_distance / (vp_w.min(vp_h) * 0.5));

        Some(AxisDragOutcome {
            axis_delta: delta,
            rotation_radians: 0.0,
        })
    }

    fn advance_rotation_drag(
        &mut self,
        view_proj: &Mat4,
        current_mouse: [f32; 2],
        vp_w: f32,
        vp_h: f32,
    ) -> Option<f32> {
        let axis_index = gizmo_axis_index(self.drag_axis)?;
        let origin = self.drag_start_world_pos.or(self.drag_start_pos)?;
        let current_parameter = picking::rotation_ring_parameter_scaled(
            current_mouse,
            origin,
            axis_index,
            self.drag_presentation_scale,
            view_proj,
            vp_w,
            vp_h,
            self.last_rotation_parameter,
        );

        if let Some(current_parameter) = current_parameter {
            if let Some(previous_parameter) = self.last_rotation_parameter {
                let increment = wrapped_rotation_delta(previous_parameter, current_parameter);
                self.accumulated_rotation += axis_vector(axis_index) * increment;
            }
            self.last_rotation_parameter = Some(current_parameter);
        }

        Some(axis_component(self.accumulated_rotation, axis_index))
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
        self.drag_start_world_pos = None;
        self.drag_start_world_scale = Vec3::ONE;
        self.drag_world_axes = [Vec3::X, Vec3::Y, Vec3::Z];
        self.drag_parent_inverse = Mat4::IDENTITY;
        self.drag_presentation_scale = 1.0;
        self.accumulated_rotation = Vec3::ZERO;
        self.last_rotation_parameter = None;
    }
}

/// Projects a world-space handle origin plus one world-unit face direction
/// into screen space, returning the screen axis direction and its length.
fn screen_axis_basis(
    handle_origin_world: Vec3,
    face_dir: Vec3,
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
) -> Option<([f32; 2], f32)> {
    let origin_screen = transform::project_point(handle_origin_world, view_proj, vp_w, vp_h);
    let axis_screen =
        transform::project_point(handle_origin_world + face_dir, view_proj, vp_w, vp_h);
    let (Some((o_s, _)), Some((a_s, _))) = (origin_screen, axis_screen) else {
        return None;
    };
    let dir = [a_s[0] - o_s[0], a_s[1] - o_s[1]];
    let len = (dir[0] * dir[0] + dir[1] * dir[1]).sqrt();
    Some((dir, len))
}

/// Signed projection of a pointer displacement onto a screen-space axis.
fn pointer_axis_projection(from: [f32; 2], to: [f32; 2], dir: &[f32; 2], len: f32) -> f32 {
    let delta = [to[0] - from[0], to[1] - from[1]];
    (delta[0] * dir[0] + delta[1] * dir[1]) / len
}

fn gizmo_axis_index(axis: GizmoAxis) -> Option<usize> {
    match axis {
        GizmoAxis::X => Some(0),
        GizmoAxis::Y => Some(1),
        GizmoAxis::Z => Some(2),
        GizmoAxis::None => None,
    }
}

fn axis_vector(axis_index: usize) -> Vec3 {
    [Vec3::X, Vec3::Y, Vec3::Z][axis_index.min(2)]
}

fn axis_component(vector: Vec3, axis_index: usize) -> f32 {
    match axis_index {
        0 => vector.x,
        1 => vector.y,
        _ => vector.z,
    }
}

fn set_axis_component(vector: &mut Vec3, axis_index: usize, value: f32) {
    match axis_index {
        0 => vector.x = value,
        1 => vector.y = value,
        _ => vector.z = value,
    }
}

fn wrapped_rotation_delta(previous: f32, current: f32) -> f32 {
    (current - previous + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI
}

fn normalized_axis_or(axis: Vec3, fallback: Vec3) -> Vec3 {
    let length = axis.length();
    if length.is_finite() && length > 1e-5 {
        axis / length
    } else {
        fallback
    }
}

fn normalized_sign(value: f32) -> f32 {
    if value < 0.0 {
        -1.0
    } else {
        1.0
    }
}

fn stable_inverse(matrix: Mat4) -> Mat4 {
    let determinant = matrix.determinant();
    if determinant.is_finite() && determinant.abs() > 1e-8 {
        matrix.inverse()
    } else {
        Mat4::IDENTITY
    }
}

fn scale_with_min_magnitude(scale: Vec3, minimum: f32) -> Vec3 {
    Vec3::new(
        signed_min_magnitude(scale.x, minimum),
        signed_min_magnitude(scale.y, minimum),
        signed_min_magnitude(scale.z, minimum),
    )
}

fn signed_min_magnitude(value: f32, minimum: f32) -> f32 {
    if value < 0.0 {
        -value.abs().max(minimum)
    } else {
        value.abs().max(minimum)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_core::scene::graph::Primitive;

    #[test]
    fn rotation_drag_accumulates_forward_across_the_pi_boundary() {
        let mut scene = SceneGraph::default();
        let id = scene.add_root_with_primitive("Rotating", Primitive::Cube);
        let mut controller = ViewportTransformController::default();
        controller.set_mode(GizmoMode::Rotate);
        controller.drag_axis = GizmoAxis::Z;
        controller.drag_start_mouse = Some(rotation_ring_screen_point(179.0f32.to_radians()));
        controller.drag_start_pos = Some(Vec3::ZERO);
        controller.drag_start_world_pos = Some(Vec3::ZERO);
        controller.drag_start_rotation = Some(Vec3::ZERO);
        controller.drag_presentation_scale = 1.0;
        controller.last_rotation_parameter = Some(179.0f32.to_radians());

        assert!(controller.apply_drag(
            &mut scene,
            Some(id),
            &Mat4::IDENTITY,
            rotation_ring_screen_point(-179.0f32.to_radians()),
            5.0,
            false,
            false,
            800.0,
            600.0,
        ));

        let rotation = scene.get(id).unwrap().rotation.z;
        assert!(
            (rotation - 2.0).abs() < 1e-3,
            "expected +2 degrees, got {rotation}"
        );
    }

    #[test]
    fn scale_drag_uses_the_rotated_face_axis() {
        let mut scene = SceneGraph::default();
        let id = scene.add_root_with_primitive("Scaled", Primitive::Cube);
        scene.get_mut(id).unwrap().rotation.z = 45.0;
        let world = scene.world_matrix(id);
        let origin = world.col(3).truncate();
        let (axes, world_scale) = picking::gizmo_scale_basis(world);
        let handle = picking::gizmo_scale_handle_world_position(origin, world_scale, axes, 0, 1.0);
        let start = transform::project_point(handle, &Mat4::IDENTITY, 800.0, 600.0)
            .unwrap()
            .0;
        let (screen_axis, screen_axis_len) =
            screen_axis_basis(handle, axes[0], &Mat4::IDENTITY, 800.0, 600.0).unwrap();
        let current = [
            start[0] + screen_axis[0] / screen_axis_len * 30.0,
            start[1] + screen_axis[1] / screen_axis_len * 30.0,
        ];

        let mut controller = ViewportTransformController::default();
        controller.set_mode(GizmoMode::Scale);
        controller.begin_drag_scaled(&scene, Some(id), &Mat4::IDENTITY, start, 800.0, 600.0, 1.0);
        assert_eq!(controller.drag_axis(), GizmoAxis::X);
        assert!(controller.apply_drag(
            &mut scene,
            Some(id),
            &Mat4::IDENTITY,
            current,
            5.0,
            false,
            false,
            800.0,
            600.0,
        ));

        let node = scene.get(id).unwrap();
        assert!((node.scale.x - 1.5).abs() < 1e-3);
        assert!((node.scale.y - 1.0).abs() < 1e-3);
        assert!((node.position - axes[0] * 0.25).length() < 1e-3);
    }

    fn rotation_ring_screen_point(angle: f32) -> [f32; 2] {
        let world = Vec3::new(
            angle.cos() * picking::GIZMO_ROTATION_RADIUS,
            angle.sin() * picking::GIZMO_ROTATION_RADIUS,
            0.0,
        );
        transform::project_point(world, &Mat4::IDENTITY, 800.0, 600.0)
            .unwrap()
            .0
    }
}
