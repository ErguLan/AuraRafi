//! 3D to 2D projection utilities for the viewport painter.
//!
//! Projects 3D world coordinates to 2D screen coordinates using view-projection
//! matrices. This is what makes "3D" rendering possible without any GPU pipeline.

use glam::{Mat4, Vec3, Vec4};

/// Project a 3D world point to 2D screen coordinates.
/// Returns None if the point is behind the camera.
pub fn project_point(
    point: Vec3,
    view_proj: &Mat4,
    viewport_width: f32,
    viewport_height: f32,
) -> Option<[f32; 2]> {
    let clip = *view_proj * Vec4::new(point.x, point.y, point.z, 1.0);

    // Behind camera check
    if clip.w <= 0.001 {
        return None;
    }

    // Perspective divide -> NDC (-1 to 1)
    let ndc_x = clip.x / clip.w;
    let ndc_y = clip.y / clip.w;

    // NDC to screen coordinates
    let screen_x = (ndc_x + 1.0) * 0.5 * viewport_width;
    let screen_y = (1.0 - ndc_y) * 0.5 * viewport_height; // Y is flipped

    Some([screen_x, screen_y])
}

/// Project a 3D edge to 2D. Returns None if either point is behind camera.
pub fn project_edge(
    edge: &[Vec3; 2],
    view_proj: &Mat4,
    w: f32,
    h: f32,
) -> Option<[[f32; 2]; 2]> {
    let a = project_point(edge[0], view_proj, w, h)?;
    let b = project_point(edge[1], view_proj, w, h)?;
    Some([a, b])
}

/// Calculate basic directional light shading (dot product).
/// Returns a brightness factor from 0.3 to 1.0.
pub fn face_brightness(face_normal: Vec3, light_dir: Vec3, model_rotation: &Mat4) -> f32 {
    let world_normal = (*model_rotation * Vec4::from((face_normal, 0.0))).truncate().normalize();
    let dot = world_normal.dot(light_dir.normalize());
    // Remap from [-1, 1] to [0.3, 1.0] for ambient + diffuse
    0.3 + 0.7 * dot.max(0.0)
}
