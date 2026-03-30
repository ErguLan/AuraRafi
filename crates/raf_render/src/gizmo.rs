//! Transform gizmo data for per-axis manipulation handles.
//!
//! Provides visual axis arrows (X/Y/Z) that the user can drag
//! to move/scale/rotate entities along a single axis.
//! Drawn with the same CPU painter - no GPU resources.

use glam::Vec3;
use serde::{Deserialize, Serialize};

/// Which axis (if any) the user is currently dragging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GizmoAxis {
    None,
    X,
    Y,
    Z,
}

/// Gizmo operation mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GizmoMode {
    /// Move along axis.
    Translate,
    /// Scale along axis.
    Scale,
    /// Rotate around axis.
    Rotate,
}

/// State for the transform gizmo.
#[derive(Debug, Clone)]
pub struct GizmoState {
    /// Which axis is being hovered / dragged.
    pub active_axis: GizmoAxis,
    /// Current operation mode.
    pub mode: GizmoMode,
    /// Whether the gizmo is visible.
    pub visible: bool,
    /// Handle length in screen pixels.
    pub handle_length: f32,
    /// Hit threshold in screen pixels (how close to click on an axis).
    pub hit_threshold: f32,
}

impl Default for GizmoState {
    fn default() -> Self {
        Self {
            active_axis: GizmoAxis::None,
            mode: GizmoMode::Translate,
            visible: true,
            handle_length: 60.0,
            hit_threshold: 8.0,
        }
    }
}

/// Axis direction vectors and colors for rendering.
pub struct GizmoAxisInfo {
    pub direction: Vec3,
    pub color_r: u8,
    pub color_g: u8,
    pub color_b: u8,
    pub label: &'static str,
}

/// Get the 3 axis definitions.
pub fn axes() -> [GizmoAxisInfo; 3] {
    [
        GizmoAxisInfo {
            direction: Vec3::X,
            color_r: 220, color_g: 70, color_b: 70,
            label: "X",
        },
        GizmoAxisInfo {
            direction: Vec3::Y,
            color_r: 70, color_g: 220, color_b: 70,
            label: "Y",
        },
        GizmoAxisInfo {
            direction: Vec3::Z,
            color_r: 70, color_g: 100, color_b: 220,
            label: "Z",
        },
    ]
}

impl GizmoState {
    /// Test if a 2D screen point is near enough to a gizmo axis handle.
    /// Returns which axis was hit (or None).
    /// `origin_2d`: screen position of the entity center.
    /// `axis_ends_2d`: screen positions of the 3 axis arrow tips [X, Y, Z].
    pub fn hit_test(
        &self,
        click: [f32; 2],
        origin_2d: [f32; 2],
        axis_ends_2d: [[f32; 2]; 3],
    ) -> GizmoAxis {
        let threshold_sq = self.hit_threshold * self.hit_threshold;

        let axes = [GizmoAxis::X, GizmoAxis::Y, GizmoAxis::Z];
        for (i, axis) in axes.iter().enumerate() {
            let dist_sq = point_to_segment_dist_sq(
                click,
                origin_2d,
                axis_ends_2d[i],
            );
            if dist_sq < threshold_sq {
                return *axis;
            }
        }
        GizmoAxis::None
    }

    /// Convert active axis to a world-space direction vector.
    pub fn axis_direction(&self) -> Vec3 {
        match self.active_axis {
            GizmoAxis::X => Vec3::X,
            GizmoAxis::Y => Vec3::Y,
            GizmoAxis::Z => Vec3::Z,
            GizmoAxis::None => Vec3::ZERO,
        }
    }
}

/// Squared distance from a point to a line segment (2D).
fn point_to_segment_dist_sq(
    p: [f32; 2],
    a: [f32; 2],
    b: [f32; 2],
) -> f32 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len_sq = dx * dx + dy * dy;

    if len_sq < 0.0001 {
        let ex = p[0] - a[0];
        let ey = p[1] - a[1];
        return ex * ex + ey * ey;
    }

    let t = ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len_sq;
    let t = t.clamp(0.0, 1.0);

    let proj_x = a[0] + t * dx;
    let proj_y = a[1] + t * dy;

    let ex = p[0] - proj_x;
    let ey = p[1] - proj_y;
    ex * ex + ey * ey
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_test_on_x_axis() {
        let gizmo = GizmoState::default();
        let origin = [100.0, 100.0];
        let ends = [
            [160.0, 100.0], // X axis
            [100.0, 40.0],  // Y axis
            [130.0, 130.0], // Z axis
        ];
        // Click right on the X axis line
        let result = gizmo.hit_test([130.0, 100.0], origin, ends);
        assert_eq!(result, GizmoAxis::X);
    }

    #[test]
    fn hit_test_misses() {
        let gizmo = GizmoState::default();
        let origin = [100.0, 100.0];
        let ends = [
            [160.0, 100.0],
            [100.0, 40.0],
            [130.0, 130.0],
        ];
        // Click far away from any axis
        let result = gizmo.hit_test([200.0, 200.0], origin, ends);
        assert_eq!(result, GizmoAxis::None);
    }
}
