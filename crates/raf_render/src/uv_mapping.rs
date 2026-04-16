//! UV coordinate generation and mapping.
//!
//! Generates UV coordinates for standard primitives (cube, sphere, plane, cylinder).
//! Used by the texture system when textures_enabled = true.
//! Zero cost when textures are disabled.

use glam::Vec3;

/// UV generation mode for mesh faces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UvProjection {
    /// Planar projection from the given axis.
    Planar,
    /// Box/cube projection (6-face, auto-select by normal).
    Box,
    /// Spherical projection.
    Spherical,
    /// Cylindrical projection.
    Cylindrical,
}

/// Generate UV coordinates for a vertex given its position and normal.
///
/// Uses box projection by default: picks the dominant axis from the normal
/// and projects the position onto the other two axes.
pub fn generate_uv_box(position: Vec3, normal: Vec3, scale: f32) -> [f32; 2] {
    let abs_n = normal.abs();
    let inv_scale = if scale > 0.001 { 1.0 / scale } else { 1.0 };

    if abs_n.x >= abs_n.y && abs_n.x >= abs_n.z {
        // X-dominant face: project onto YZ.
        [position.z * inv_scale * 0.5 + 0.5, position.y * inv_scale * 0.5 + 0.5]
    } else if abs_n.y >= abs_n.x && abs_n.y >= abs_n.z {
        // Y-dominant face: project onto XZ.
        [position.x * inv_scale * 0.5 + 0.5, position.z * inv_scale * 0.5 + 0.5]
    } else {
        // Z-dominant face: project onto XY.
        [position.x * inv_scale * 0.5 + 0.5, position.y * inv_scale * 0.5 + 0.5]
    }
}

/// Generate spherical UV coordinates from a position on a unit sphere.
pub fn generate_uv_spherical(position: Vec3) -> [f32; 2] {
    let n = position.normalize();
    let u = 0.5 + n.z.atan2(n.x) / (2.0 * std::f32::consts::PI);
    let v = 0.5 - n.y.asin() / std::f32::consts::PI;
    [u, v]
}

/// Generate cylindrical UV coordinates.
/// Wraps U around the Y axis, V maps to height.
pub fn generate_uv_cylindrical(position: Vec3, height: f32) -> [f32; 2] {
    let u = 0.5 + position.z.atan2(position.x) / (2.0 * std::f32::consts::PI);
    let v = if height > 0.001 {
        (position.y / height) * 0.5 + 0.5
    } else {
        0.5
    };
    [u, v]
}

/// Generate planar UVs from a given axis.
pub fn generate_uv_planar(position: Vec3, axis: Vec3, scale: f32) -> [f32; 2] {
    let abs_axis = axis.abs();
    let inv_scale = if scale > 0.001 { 1.0 / scale } else { 1.0 };

    if abs_axis.y > abs_axis.x && abs_axis.y > abs_axis.z {
        [position.x * inv_scale * 0.5 + 0.5, position.z * inv_scale * 0.5 + 0.5]
    } else if abs_axis.x > abs_axis.z {
        [position.z * inv_scale * 0.5 + 0.5, position.y * inv_scale * 0.5 + 0.5]
    } else {
        [position.x * inv_scale * 0.5 + 0.5, position.y * inv_scale * 0.5 + 0.5]
    }
}

/// A quad with pre-computed UV coordinates.
#[derive(Debug, Clone)]
pub struct UvQuad {
    /// Four vertex positions.
    pub positions: [Vec3; 4],
    /// Four UV coordinates.
    pub uvs: [[f32; 2]; 4],
    /// Face normal.
    pub normal: Vec3,
}

/// Generate UV quads for a unit cube (6 faces, 4 verts each).
/// Returns 6 UvQuads with proper UVs for texture mapping.
pub fn cube_uv_quads() -> Vec<UvQuad> {
    let faces: &[([Vec3; 4], Vec3)] = &[
        // Front (+Z).
        ([Vec3::new(-1.0, -1.0,  1.0), Vec3::new( 1.0, -1.0,  1.0),
          Vec3::new( 1.0,  1.0,  1.0), Vec3::new(-1.0,  1.0,  1.0)], Vec3::Z),
        // Back (-Z).
        ([Vec3::new( 1.0, -1.0, -1.0), Vec3::new(-1.0, -1.0, -1.0),
          Vec3::new(-1.0,  1.0, -1.0), Vec3::new( 1.0,  1.0, -1.0)], Vec3::NEG_Z),
        // Right (+X).
        ([Vec3::new( 1.0, -1.0,  1.0), Vec3::new( 1.0, -1.0, -1.0),
          Vec3::new( 1.0,  1.0, -1.0), Vec3::new( 1.0,  1.0,  1.0)], Vec3::X),
        // Left (-X).
        ([Vec3::new(-1.0, -1.0, -1.0), Vec3::new(-1.0, -1.0,  1.0),
          Vec3::new(-1.0,  1.0,  1.0), Vec3::new(-1.0,  1.0, -1.0)], Vec3::NEG_X),
        // Top (+Y).
        ([Vec3::new(-1.0,  1.0,  1.0), Vec3::new( 1.0,  1.0,  1.0),
          Vec3::new( 1.0,  1.0, -1.0), Vec3::new(-1.0,  1.0, -1.0)], Vec3::Y),
        // Bottom (-Y).
        ([Vec3::new(-1.0, -1.0, -1.0), Vec3::new( 1.0, -1.0, -1.0),
          Vec3::new( 1.0, -1.0,  1.0), Vec3::new(-1.0, -1.0,  1.0)], Vec3::NEG_Y),
    ];

    faces.iter().map(|(verts, normal)| {
        UvQuad {
            positions: *verts,
            uvs: [
                [0.0, 1.0],
                [1.0, 1.0],
                [1.0, 0.0],
                [0.0, 0.0],
            ],
            normal: *normal,
        }
    }).collect()
}
