//! Matrix construction and coordinate space transformations.
//!
//! Coordinate spaces and their transforms:
//!
//! ```text
//! Object Space --[Model]--> World Space --[View]--> View Space --[Proj]--> Clip Space
//!                                                                            |
//!                                                               perspective divide
//!                                                                            |
//!                                                                        NDC Space
//!                                                                            |
//!                                                                  viewport transform
//!                                                                            |
//!                                                                       Screen Space
//! ```
//!
//! - Model = T * R * S (translate * rotate * scale)
//! - View = (T_cam * R_cam)^-1 = look_at_rh
//! - Projection = perspective_rh or orthographic_rh
//! - MVP = Projection * View * Model
//!
//! Screen space: origin at top-left, X right, Y down, in pixels.
//! NDC: origin at center, X right, Y up, range [-1, 1].

use glam::{Mat4, Quat, Vec3, Vec4};

/// Build a model matrix from position, rotation (euler degrees YXZ), and scale.
///
/// This matches SceneNode::local_matrix() in raf_core.
pub fn model_matrix(position: Vec3, rotation_degrees: Vec3, scale: Vec3) -> Mat4 {
    let rotation = Quat::from_euler(
        glam::EulerRot::YXZ,
        rotation_degrees.y.to_radians(),
        rotation_degrees.x.to_radians(),
        rotation_degrees.z.to_radians(),
    );
    Mat4::from_scale_rotation_translation(scale, rotation, position)
}

/// Build the view matrix from camera parameters.
///
/// Uses right-handed look-at: camera at `eye`, looking toward `target`,
/// with `up` defining the vertical direction.
///
/// This is the inverse of the camera's world transform: V = (T*R)^-1
pub fn view_matrix(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
    Mat4::look_at_rh(eye, target, up)
}

/// Build a perspective projection matrix.
///
/// - `fov_radians`: vertical field of view in radians
/// - `aspect`: width / height
/// - `near`, `far`: clipping plane distances (must be positive, near < far)
pub fn perspective_matrix(fov_radians: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    Mat4::perspective_rh(fov_radians, aspect, near, far)
}

/// Build an orthographic projection matrix.
///
/// Used for 2D mode (Unity-style: same 3D pipeline, orthographic camera).
pub fn orthographic_matrix(
    left: f32, right: f32,
    bottom: f32, top: f32,
    near: f32, far: f32,
) -> Mat4 {
    Mat4::orthographic_rh(left, right, bottom, top, near, far)
}

/// Compute the combined Model-View-Projection matrix.
pub fn mvp(model: &Mat4, view: &Mat4, proj: &Mat4) -> Mat4 {
    *proj * *view * *model
}

/// Compute the normal matrix for transforming normals correctly.
///
/// The normal matrix is the transpose of the inverse of the upper-left 3x3
/// of the model matrix. This is necessary because non-uniform scaling
/// distorts normals when using the model matrix directly.
///
/// For uniform scale, this is equivalent to the model matrix rotation.
pub fn normal_matrix(model: &Mat4) -> Mat4 {
    model.inverse().transpose()
}

/// Transform a world-space normal by the normal matrix and normalize it.
pub fn transform_normal(normal: Vec3, normal_mat: &Mat4) -> Vec3 {
    (*normal_mat * Vec4::from((normal, 0.0)))
        .truncate()
        .normalize_or_zero()
}

/// Project a 3D world point to 2D screen coordinates.
///
/// Returns `None` if the point is behind the camera (clip.w <= epsilon).
///
/// Screen coordinates: (0,0) at top-left, (vp_w, vp_h) at bottom-right.
pub fn project_point(
    point: Vec3,
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
) -> Option<([f32; 2], f32)> {
    let clip = *view_proj * Vec4::new(point.x, point.y, point.z, 1.0);
    if clip.w <= 0.001 {
        return None;
    }

    let ndc_x = clip.x / clip.w;
    let ndc_y = clip.y / clip.w;
    let depth = clip.z / clip.w;

    let screen = [
        (ndc_x + 1.0) * 0.5 * vp_w,
        (1.0 - ndc_y) * 0.5 * vp_h,
    ];

    Some((screen, depth))
}

/// Unproject a screen point to a world-space ray.
///
/// Takes pixel coordinates (origin top-left) and viewport dimensions.
/// Returns `(ray_origin, ray_direction)` in world space.
///
/// The ray starts at the near plane and points toward the far plane.
/// For perspective: origin is near the camera, direction is into the scene.
/// For orthographic: origin varies across the viewport, direction is constant.
pub fn screen_to_world_ray(
    screen_x: f32,
    screen_y: f32,
    vp_w: f32,
    vp_h: f32,
    view_proj_inverse: &Mat4,
) -> Option<(Vec3, Vec3)> {
    // Screen to NDC
    let ndc_x = (screen_x / vp_w) * 2.0 - 1.0;
    let ndc_y = 1.0 - (screen_y / vp_h) * 2.0;

    // Unproject near and far points
    let near_clip = *view_proj_inverse * Vec4::new(ndc_x, ndc_y, -1.0, 1.0);
    let far_clip = *view_proj_inverse * Vec4::new(ndc_x, ndc_y, 1.0, 1.0);

    if near_clip.w.abs() <= 0.0001 || far_clip.w.abs() <= 0.0001 {
        return None;
    }

    let near_world = near_clip.truncate() / near_clip.w;
    let far_world = far_clip.truncate() / far_clip.w;
    let direction = (far_world - near_world).normalize_or_zero();

    if direction.length_squared() <= 0.0001 {
        return None;
    }

    Some((near_world, direction))
}

/// Project a 3D edge to screen space.
/// Clips against the near plane if one vertex is behind the camera.
/// Returns None if both vertices are behind the camera.
pub fn project_edge(
    edge: &[Vec3; 2],
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
) -> Option<[[f32; 2]; 2]> {
    let mut a_clip = *view_proj * Vec4::new(edge[0].x, edge[0].y, edge[0].z, 1.0);
    let mut b_clip = *view_proj * Vec4::new(edge[1].x, edge[1].y, edge[1].z, 1.0);

    const EPS: f32 = 0.001;

    if a_clip.w <= EPS && b_clip.w <= EPS {
        return None;
    }

    if a_clip.w <= EPS {
        a_clip = clip_line_to_near(a_clip, b_clip)?;
    }
    if b_clip.w <= EPS {
        b_clip = clip_line_to_near(b_clip, a_clip)?;
    }

    let a = clip_to_screen(a_clip, vp_w, vp_h)?;
    let b = clip_to_screen(b_clip, vp_w, vp_h)?;
    Some([a, b])
}

fn clip_line_to_near(behind: Vec4, front: Vec4) -> Option<Vec4> {
    const EPS: f32 = 0.001;
    let denom = front.w - behind.w;
    if denom.abs() <= f32::EPSILON {
        return None;
    }
    let t = ((EPS - behind.w) / denom).clamp(0.0, 1.0);
    Some(behind + (front - behind) * t)
}

fn clip_to_screen(clip: Vec4, vp_w: f32, vp_h: f32) -> Option<[f32; 2]> {
    if clip.w <= 0.001 {
        return None;
    }
    let ndc_x = clip.x / clip.w;
    let ndc_y = clip.y / clip.w;
    Some([
        (ndc_x + 1.0) * 0.5 * vp_w,
        (1.0 - ndc_y) * 0.5 * vp_h,
    ])
}

/// Compute flat shading brightness for a face.
///
/// Transforms the face normal to world space, dots with the light direction,
/// and returns a brightness factor in [0.3, 1.0] (ambient + diffuse).
pub fn face_brightness(face_normal: Vec3, light_dir: Vec3, model: &Mat4) -> f32 {
    let world_normal = transform_normal(face_normal, &normal_matrix(model));
    let dot = world_normal.dot(light_dir.normalize()).max(0.0);
    0.3 + 0.7 * dot
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_model_matrix() {
        let m = model_matrix(Vec3::ZERO, Vec3::ZERO, Vec3::ONE);
        let diff = (m - Mat4::IDENTITY).abs_diff_eq(Mat4::ZERO, 0.001);
        assert!(diff, "identity model matrix should equal Mat4::IDENTITY");
    }

    #[test]
    fn projection_of_origin() {
        let view = view_matrix(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
        let proj = perspective_matrix(60.0f32.to_radians(), 1.0, 0.1, 100.0);
        let vp = proj * view;

        if let Some((screen, _depth)) = project_point(Vec3::ZERO, &vp, 800.0, 600.0) {
            // Origin should project to center of viewport
            assert!((screen[0] - 400.0).abs() < 1.0, "x={}", screen[0]);
            assert!((screen[1] - 300.0).abs() < 1.0, "y={}", screen[1]);
        } else {
            panic!("Origin should be visible from z=5");
        }
    }

    #[test]
    fn unproject_roundtrip() {
        let view = view_matrix(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
        let proj = perspective_matrix(60.0f32.to_radians(), 1.0, 0.1, 100.0);
        let vp = proj * view;
        let vp_inv = vp.inverse();

        // Project origin, then unproject the screen point
        let (screen, _) = project_point(Vec3::ZERO, &vp, 800.0, 600.0).unwrap();
        let (ray_origin, ray_dir) = screen_to_world_ray(
            screen[0], screen[1], 800.0, 600.0, &vp_inv,
        ).unwrap();

        // Ray should pass through or near the origin
        // Project ray_origin onto ray toward origin
        let to_origin = Vec3::ZERO - ray_origin;
        let t = to_origin.dot(ray_dir);
        let closest = ray_origin + ray_dir * t;
        assert!(closest.length() < 0.1,
            "ray should pass near origin, closest point: {:?}", closest);
    }

    #[test]
    fn normal_matrix_uniform_scale() {
        let model = model_matrix(Vec3::ZERO, Vec3::ZERO, Vec3::splat(2.0));
        let n = transform_normal(Vec3::Y, &normal_matrix(&model));
        assert!((n - Vec3::Y).length() < 0.001,
            "uniform scale should not change normal direction");
    }
}
