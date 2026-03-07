//! Camera system supporting orthographic (2D) and perspective (3D) modes.

use glam::{Mat4, Vec3};
use serde::{Deserialize, Serialize};

/// Camera projection mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CameraMode {
    /// 2D orthographic projection.
    Orthographic,
    /// 3D perspective projection.
    Perspective,
}

impl Default for CameraMode {
    fn default() -> Self {
        Self::Perspective
    }
}

/// Scene camera with configurable projection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Camera {
    pub position: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub mode: CameraMode,
    /// Field of view in degrees (perspective only).
    pub fov: f32,
    /// Near clipping plane.
    pub near: f32,
    /// Far clipping plane.
    pub far: f32,
    /// Orthographic zoom level (ortho only).
    pub ortho_scale: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            position: Vec3::new(5.0, 5.0, 5.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            mode: CameraMode::Perspective,
            fov: 60.0,
            near: 0.1,
            far: 1000.0,
            ortho_scale: 10.0,
        }
    }
}

impl Camera {
    /// Compute the view matrix (world -> camera space).
    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.position, self.target, self.up)
    }

    /// Compute the projection matrix for the given viewport dimensions.
    pub fn projection_matrix(&self, width: f32, height: f32) -> Mat4 {
        let aspect = width / height;
        match self.mode {
            CameraMode::Perspective => {
                Mat4::perspective_rh(self.fov.to_radians(), aspect, self.near, self.far)
            }
            CameraMode::Orthographic => {
                let half_w = self.ortho_scale * aspect * 0.5;
                let half_h = self.ortho_scale * 0.5;
                Mat4::orthographic_rh(-half_w, half_w, -half_h, half_h, self.near, self.far)
            }
        }
    }

    /// Combined view-projection matrix.
    pub fn view_projection(&self, width: f32, height: f32) -> Mat4 {
        self.projection_matrix(width, height) * self.view_matrix()
    }
}
