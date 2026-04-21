//! 3D to 2D projection utilities for the viewport painter.
//!
//! Projects 3D world coordinates to 2D screen coordinates using view-projection
//! matrices. This is what makes "3D" rendering possible without any GPU pipeline.

use glam::{Mat4, Vec3, Vec4};

const CLIP_EPSILON: f32 = 0.001;

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
    if clip.w <= CLIP_EPSILON {
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
    let mut a_clip = *view_proj * Vec4::new(edge[0].x, edge[0].y, edge[0].z, 1.0);
    let mut b_clip = *view_proj * Vec4::new(edge[1].x, edge[1].y, edge[1].z, 1.0);

    if a_clip.w <= CLIP_EPSILON && b_clip.w <= CLIP_EPSILON {
        return None;
    }

    if a_clip.w <= CLIP_EPSILON {
        a_clip = clip_line_to_near(a_clip, b_clip)?;
    }
    if b_clip.w <= CLIP_EPSILON {
        b_clip = clip_line_to_near(b_clip, a_clip)?;
    }

    let a = clip_to_screen(a_clip, w, h)?;
    let b = clip_to_screen(b_clip, w, h)?;
    Some([a, b])
}

fn clip_line_to_near(behind: Vec4, front: Vec4) -> Option<Vec4> {
    let denom = front.w - behind.w;
    if denom.abs() <= f32::EPSILON {
        return None;
    }

    let t = ((CLIP_EPSILON - behind.w) / denom).clamp(0.0, 1.0);
    Some(behind + (front - behind) * t)
}

fn clip_to_screen(clip: Vec4, viewport_width: f32, viewport_height: f32) -> Option<[f32; 2]> {
    if clip.w <= CLIP_EPSILON {
        return None;
    }

    let ndc_x = clip.x / clip.w;
    let ndc_y = clip.y / clip.w;
    Some([
        (ndc_x + 1.0) * 0.5 * viewport_width,
        (1.0 - ndc_y) * 0.5 * viewport_height,
    ])
}

/// Calculate basic directional light shading (dot product).
/// Returns a brightness factor from 0.3 to 1.0.
pub fn face_brightness(face_normal: Vec3, light_dir: Vec3, model_rotation: &Mat4) -> f32 {
    let world_normal = (*model_rotation * Vec4::from((face_normal, 0.0))).truncate().normalize();
    let dot = world_normal.dot(light_dir.normalize());
    // Remap from [-1, 1] to [0.3, 1.0] for ambient + diffuse
    0.3 + 0.7 * dot.max(0.0)
}
