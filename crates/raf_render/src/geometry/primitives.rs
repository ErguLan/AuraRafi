//! Built-in primitive geometry constructors.
//!
//! All primitives produce indexed triangle meshes (MeshData) centered at origin.
//! Vertices are duplicated at hard edges for correct normals.
//!
//! Winding: CCW when viewed from outside (front face = normal direction).
//! This matches the right-handed convention used by glam and the camera system.
//!
//! Coordinate system: Y-up, right-handed.
//! - Cube: axis-aligned, 1x1x1
//! - Cylinder: Y-axis, radius 0.5, height 1.0
//! - Sphere: center origin, radius 0.5
//! - Plane: XZ plane at Y=0, 1x1

use glam::Vec3;

use super::mesh_data::MeshData;

// ---------------------------------------------------------------------------
// Cube
// ---------------------------------------------------------------------------

/// Unit cube centered at origin (1x1x1).
/// 24 vertices (4 per face) for correct flat normals.
/// 12 triangles (2 per face).
pub fn cube(segments: usize) -> MeshData {
    let _ = segments; // Reserved for future subdivision
    let h = 0.5;
    let mut mesh = MeshData::with_capacity(24, 36);

    // Face definitions: (4 corners CCW from outside, face normal)
    let faces: [([Vec3; 4], Vec3); 6] = [
        // +Z front
        (
            [
                Vec3::new(-h, -h, h),
                Vec3::new(h, -h, h),
                Vec3::new(h, h, h),
                Vec3::new(-h, h, h),
            ],
            Vec3::Z,
        ),
        // -Z back
        (
            [
                Vec3::new(h, -h, -h),
                Vec3::new(-h, -h, -h),
                Vec3::new(-h, h, -h),
                Vec3::new(h, h, -h),
            ],
            Vec3::NEG_Z,
        ),
        // -X left
        (
            [
                Vec3::new(-h, -h, -h),
                Vec3::new(-h, -h, h),
                Vec3::new(-h, h, h),
                Vec3::new(-h, h, -h),
            ],
            Vec3::NEG_X,
        ),
        // +X right
        (
            [
                Vec3::new(h, -h, h),
                Vec3::new(h, -h, -h),
                Vec3::new(h, h, -h),
                Vec3::new(h, h, h),
            ],
            Vec3::X,
        ),
        // +Y top
        (
            [
                Vec3::new(-h, h, h),
                Vec3::new(h, h, h),
                Vec3::new(h, h, -h),
                Vec3::new(-h, h, -h),
            ],
            Vec3::Y,
        ),
        // -Y bottom
        (
            [
                Vec3::new(-h, -h, -h),
                Vec3::new(h, -h, -h),
                Vec3::new(h, -h, h),
                Vec3::new(-h, -h, h),
            ],
            Vec3::NEG_Y,
        ),
    ];

    // Contract: v = 0 is the top of the texture. Corner order is
    // bottom-left, bottom-right, top-right, top-left, so the top corners
    // (indices 2 and 3) receive v = 0.
    const FACE_UVS: [[f32; 2]; 4] = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
    for (corners, normal) in &faces {
        let base = mesh.vertex_count() as u32;
        for (index, corner) in corners.iter().enumerate() {
            mesh.push_vertex_uv(*corner, *normal, FACE_UVS[index]);
        }
        mesh.push_quad(base, base + 1, base + 2, base + 3);
    }

    mesh
}

// ---------------------------------------------------------------------------
// Cylinder
// ---------------------------------------------------------------------------

/// Cylinder along Y axis, radius 0.5, height 1.0, centered at origin.
///
/// Vertex layout:
/// - Side quads: 2 * segments vertices (top ring + bottom ring), each with outward normals
/// - Top cap: segments + 1 vertices (ring + center), all with +Y normal
/// - Bottom cap: segments + 1 vertices (ring + center), all with -Y normal
///
/// Vertices are duplicated between sides and caps because side normals point
/// outward while cap normals point up/down. Without duplication, shading
/// would bleed across the hard edge at the rim.
pub fn cylinder(segments: usize) -> MeshData {
    let seg = segments.max(6);
    let r = 0.5f32;
    let h = 0.5f32;

    // Side: 2*seg verts, 2*seg tris
    // Top cap: seg+1 verts, seg tris
    // Bottom cap: seg+1 verts, seg tris
    let vert_count = 2 * seg + (seg + 1) + (seg + 1);
    let tri_count = 2 * seg + seg + seg;
    let mut mesh = MeshData::with_capacity(vert_count, tri_count * 3);

    let tau = std::f32::consts::TAU;

    // --- Side quads ---
    // Two rings of vertices with outward-facing normals.
    let side_base = mesh.vertex_count() as u32;
    for i in 0..=seg {
        // Wrap: vertex at index `seg` equals vertex at index 0 in position
        // but we still need it for the last quad's indices.
        let wrapped = i % seg;
        let angle = tau * (wrapped as f32 / seg as f32);
        let cos_a = angle.cos();
        let sin_a = angle.sin();
        let normal = Vec3::new(cos_a, 0.0, sin_a);

        let u = i as f32 / seg as f32;
        // Top ring vertex
        mesh.push_vertex_uv(Vec3::new(r * cos_a, h, r * sin_a), normal, [u, 0.0]);
        // Bottom ring vertex
        mesh.push_vertex_uv(Vec3::new(r * cos_a, -h, r * sin_a), normal, [u, 1.0]);
    }

    for i in 0..seg {
        let top_i = side_base + (i as u32) * 2;
        let bot_i = top_i + 1;
        let top_next = side_base + (i as u32 + 1) * 2;
        let bot_next = top_next + 1;
        // CCW from outside: bot[i], top[i], top[i+1], bot[i+1].
        mesh.push_quad(bot_i, top_i, top_next, bot_next);
    }

    // --- Top cap (fan from center, normal = +Y) ---
    let top_center = mesh.push_vertex_uv(Vec3::new(0.0, h, 0.0), Vec3::Y, [0.5, 0.5]);
    let top_ring_base = mesh.vertex_count() as u32;
    for i in 0..seg {
        let angle = tau * (i as f32 / seg as f32);
        mesh.push_vertex_uv(
            Vec3::new(r * angle.cos(), h, r * angle.sin()),
            Vec3::Y,
            [0.5 + 0.5 * angle.cos(), 0.5 + 0.5 * angle.sin()],
        );
    }
    for i in 0..seg {
        let next = (i + 1) % seg;
        // The ring runs clockwise when seen from +Y, so reverse it for the
        // top cap to keep CCW winding from outside.
        mesh.push_triangle(
            top_center,
            top_ring_base + next as u32,
            top_ring_base + i as u32,
        );
    }

    // --- Bottom cap (fan from center, normal = -Y) ---
    let bot_center = mesh.push_vertex_uv(Vec3::new(0.0, -h, 0.0), Vec3::NEG_Y, [0.5, 0.5]);
    let bot_ring_base = mesh.vertex_count() as u32;
    for i in 0..seg {
        let angle = tau * (i as f32 / seg as f32);
        mesh.push_vertex_uv(
            Vec3::new(r * angle.cos(), -h, r * angle.sin()),
            Vec3::NEG_Y,
            [0.5 + 0.5 * angle.cos(), 0.5 + 0.5 * angle.sin()],
        );
    }
    for i in 0..seg {
        let next = (i + 1) % seg;
        // CCW from below (-Y): center, current, next.
        mesh.push_triangle(
            bot_center,
            bot_ring_base + i as u32,
            bot_ring_base + next as u32,
        );
    }

    mesh
}

// ---------------------------------------------------------------------------
// Sphere
// ---------------------------------------------------------------------------

/// UV sphere centered at origin, radius 0.5.
///
/// Uses latitude-longitude subdivision. Vertices at poles are duplicated
/// per-slice for correct UV mapping (future) and smooth normals.
pub fn sphere(stacks: usize, slices: usize) -> MeshData {
    let st = stacks.max(3);
    let sl = slices.max(4);
    let r = 0.5f32;

    let vert_count = (st + 1) * (sl + 1);
    let tri_count = st * sl * 2;
    let mut mesh = MeshData::with_capacity(vert_count, tri_count * 3);

    let pi = std::f32::consts::PI;
    let tau = std::f32::consts::TAU;

    // Generate vertices in a grid: rows = stacks+1, cols = slices+1
    for i in 0..=st {
        let phi = pi * (i as f32 / st as f32); // 0 (top) to PI (bottom)
        let sin_phi = phi.sin();
        let cos_phi = phi.cos();

        for j in 0..=sl {
            let theta = tau * (j as f32 / sl as f32); // 0 to TAU
            let x = sin_phi * theta.cos();
            let y = cos_phi;
            let z = sin_phi * theta.sin();

            let normal = Vec3::new(x, y, z);
            let position = normal * r;
            mesh.push_vertex_uv(
                position,
                normal,
                [j as f32 / sl as f32, i as f32 / st as f32],
            );
        }
    }

    // Generate triangles. Rows advance from the north pole to the south pole,
    // so the outward (CCW) order of a quad is a-d-c-b. The triangles that
    // touch a pole edge collapse to zero area and are skipped: the pole is
    // duplicated per slice for UVs, not for geometry.
    let cols = (sl + 1) as u32;
    for i in 0..st {
        for j in 0..sl {
            let a = (i as u32) * cols + (j as u32);
            let b = a + cols;
            let c = b + 1;
            let d = a + 1;

            if i > 0 {
                mesh.push_triangle(a, d, c);
            }
            if i + 1 < st {
                mesh.push_triangle(a, c, b);
            }
        }
    }

    mesh
}

// ---------------------------------------------------------------------------
// Plane
// ---------------------------------------------------------------------------

/// Flat plane on the XZ plane at Y=0, 1x1.
/// 4 vertices, 2 triangles, normal = +Y.
pub fn plane(segments: usize) -> MeshData {
    let _ = segments; // Reserved for future subdivision
    let h = 0.5;
    let mut mesh = MeshData::with_capacity(4, 6);

    let a = mesh.push_vertex_uv(Vec3::new(-h, 0.0, -h), Vec3::Y, [0.0, 0.0]);
    let b = mesh.push_vertex_uv(Vec3::new(h, 0.0, -h), Vec3::Y, [1.0, 0.0]);
    let c = mesh.push_vertex_uv(Vec3::new(h, 0.0, h), Vec3::Y, [1.0, 1.0]);
    let d = mesh.push_vertex_uv(Vec3::new(-h, 0.0, h), Vec3::Y, [0.0, 1.0]);

    // The XZ vertex order a-b-c-d winds toward -Y, opposite to the authored
    // normals. Reverse it so the front-face winding agrees with +Y.
    mesh.push_quad(a, d, c, b);
    mesh
}

// ---------------------------------------------------------------------------
// Wedge
// ---------------------------------------------------------------------------

/// Right triangular prism ("ramp") centered at origin, 1x1x1.
///
/// The vertical face is at -Z, the slope rises from the front bottom edge
/// (z = +0.5, y = -0.5) to the back top edge (z = -0.5, y = +0.5).
/// `segments` is reserved for future subdivision, matching cube/plane.
pub fn wedge(segments: usize) -> MeshData {
    let _ = segments;
    let h = 0.5;
    let mut mesh = MeshData::with_capacity(18, 24);

    let b0 = Vec3::new(-h, -h, -h);
    let b1 = Vec3::new(h, -h, -h);
    let b2 = Vec3::new(h, -h, h);
    let b3 = Vec3::new(-h, -h, h);
    let t0 = Vec3::new(-h, h, -h);
    let t1 = Vec3::new(h, h, -h);
    let slope_normal = Vec3::new(0.0, 1.0, 1.0).normalize();

    // Bottom (-Y).
    let base = mesh.vertex_count() as u32;
    for (corner, uv) in [
        (b0, [0.0, 0.0]),
        (b1, [1.0, 0.0]),
        (b2, [1.0, 1.0]),
        (b3, [0.0, 1.0]),
    ] {
        mesh.push_vertex_uv(corner, Vec3::NEG_Y, uv);
    }
    mesh.push_quad(base, base + 1, base + 2, base + 3);

    // Back vertical face (-Z).
    let base = mesh.vertex_count() as u32;
    for (corner, uv) in [
        (b0, [0.0, 1.0]),
        (t0, [0.0, 0.0]),
        (t1, [1.0, 0.0]),
        (b1, [1.0, 1.0]),
    ] {
        mesh.push_vertex_uv(corner, Vec3::NEG_Z, uv);
    }
    mesh.push_quad(base, base + 1, base + 2, base + 3);

    // Sloped face (+Y/+Z).
    let base = mesh.vertex_count() as u32;
    for (corner, uv) in [
        (b3, [0.0, 1.0]),
        (b2, [1.0, 1.0]),
        (t1, [1.0, 0.0]),
        (t0, [0.0, 0.0]),
    ] {
        mesh.push_vertex_uv(corner, slope_normal, uv);
    }
    mesh.push_quad(base, base + 1, base + 2, base + 3);

    // Left side triangle (-X).
    let base = mesh.vertex_count() as u32;
    for (corner, uv) in [(b0, [0.0, 1.0]), (b3, [1.0, 1.0]), (t0, [0.0, 0.0])] {
        mesh.push_vertex_uv(corner, Vec3::NEG_X, uv);
    }
    mesh.push_triangle(base, base + 1, base + 2);

    // Right side triangle (+X).
    let base = mesh.vertex_count() as u32;
    for (corner, uv) in [(b1, [0.0, 1.0]), (t1, [0.0, 0.0]), (b2, [1.0, 1.0])] {
        mesh.push_vertex_uv(corner, Vec3::X, uv);
    }
    mesh.push_triangle(base, base + 1, base + 2);

    mesh
}

// ---------------------------------------------------------------------------
// Torus
// ---------------------------------------------------------------------------

/// Torus on the XZ plane, unit outer diameter, tube thickness 0.4.
///
/// Major radius 0.3 plus tube radius 0.2 gives an outer diameter of 1.0 and a
/// vertical extent of +/-0.2, matching the authored shape descriptor.
pub fn torus(major_segments: usize, minor_segments: usize) -> MeshData {
    let major = major_segments.max(3);
    let minor = minor_segments.max(3);
    let tau = std::f32::consts::TAU;
    let major_radius = 0.3f32;
    let tube_radius = 0.2f32;

    let vert_count = (major + 1) * (minor + 1);
    let mut mesh = MeshData::with_capacity(vert_count, major * minor * 6);
    let cols = (minor + 1) as u32;

    for i in 0..=major {
        let u = tau * (i as f32 / major as f32);
        let (sin_u, cos_u) = u.sin_cos();
        for j in 0..=minor {
            let v = tau * (j as f32 / minor as f32);
            let (sin_v, cos_v) = v.sin_cos();
            let ring = major_radius + tube_radius * cos_v;
            let position = Vec3::new(ring * cos_u, tube_radius * sin_v, ring * sin_u);
            let normal = Vec3::new(cos_v * cos_u, sin_v, cos_v * sin_u);
            mesh.push_vertex_uv(
                position,
                normal,
                [i as f32 / major as f32, j as f32 / minor as f32],
            );
        }
    }

    for i in 0..major {
        for j in 0..minor {
            let a = (i as u32) * cols + j as u32;
            let b = a + cols;
            let c = b + 1;
            let d = a + 1;
            // CCW from outside: a-d-c-b.
            mesh.push_quad(a, d, c, b);
        }
    }

    mesh
}

// ---------------------------------------------------------------------------
// Prism
// ---------------------------------------------------------------------------

/// Regular n-sided prism along the Y axis, circumradius 0.5, height 1.0.
///
/// Unlike the smooth cylinder, each side uses four duplicated vertices with a
/// flat outward normal, producing faceted shading.
pub fn prism(sides: usize) -> MeshData {
    let sides = sides.clamp(3, 64);
    let r = 0.5f32;
    let h = 0.5f32;
    let tau = std::f32::consts::TAU;

    // Sides: 4 verts / 2 tris each. Caps: center + n ring verts, n tris each.
    let vert_count = sides * 4 + (sides + 1) * 2;
    let tri_count = sides * 2 + sides + sides;
    let mut mesh = MeshData::with_capacity(vert_count, tri_count * 3);

    for i in 0..sides {
        let angle0 = tau * (i as f32 / sides as f32);
        let angle1 = tau * ((i + 1) as f32 / sides as f32);
        let mid = (angle0 + angle1) * 0.5;
        let normal = Vec3::new(mid.cos(), 0.0, mid.sin());
        let u0 = i as f32 / sides as f32;
        let u1 = (i + 1) as f32 / sides as f32;
        let (p0, p1) = (
            Vec3::new(r * angle0.cos(), 0.0, r * angle0.sin()),
            Vec3::new(r * angle1.cos(), 0.0, r * angle1.sin()),
        );
        let base = mesh.vertex_count() as u32;
        mesh.push_vertex_uv(p0 + Vec3::new(0.0, -h, 0.0), normal, [u0, 1.0]);
        mesh.push_vertex_uv(p0 + Vec3::new(0.0, h, 0.0), normal, [u0, 0.0]);
        mesh.push_vertex_uv(p1 + Vec3::new(0.0, h, 0.0), normal, [u1, 0.0]);
        mesh.push_vertex_uv(p1 + Vec3::new(0.0, -h, 0.0), normal, [u1, 1.0]);
        mesh.push_quad(base, base + 1, base + 2, base + 3);
    }

    // Top cap (+Y). The ring advances clockwise seen from above, so the fan
    // is reversed to keep the exterior face CCW.
    let top_center = mesh.push_vertex_uv(Vec3::new(0.0, h, 0.0), Vec3::Y, [0.5, 0.5]);
    let top_ring_base = mesh.vertex_count() as u32;
    for i in 0..sides {
        let angle = tau * (i as f32 / sides as f32);
        mesh.push_vertex_uv(
            Vec3::new(r * angle.cos(), h, r * angle.sin()),
            Vec3::Y,
            [0.5 + 0.5 * angle.cos(), 0.5 + 0.5 * angle.sin()],
        );
    }
    for i in 0..sides {
        let next = (i + 1) % sides;
        mesh.push_triangle(
            top_center,
            top_ring_base + next as u32,
            top_ring_base + i as u32,
        );
    }

    // Bottom cap (-Y).
    let bot_center = mesh.push_vertex_uv(Vec3::new(0.0, -h, 0.0), Vec3::NEG_Y, [0.5, 0.5]);
    let bot_ring_base = mesh.vertex_count() as u32;
    for i in 0..sides {
        let angle = tau * (i as f32 / sides as f32);
        mesh.push_vertex_uv(
            Vec3::new(r * angle.cos(), -h, r * angle.sin()),
            Vec3::NEG_Y,
            [0.5 + 0.5 * angle.cos(), 0.5 + 0.5 * angle.sin()],
        );
    }
    for i in 0..sides {
        let next = (i + 1) % sides;
        mesh.push_triangle(
            bot_center,
            bot_ring_base + i as u32,
            bot_ring_base + next as u32,
        );
    }

    mesh
}

// ---------------------------------------------------------------------------
// Sphere 2D (billboard disc)
// ---------------------------------------------------------------------------

/// Flat disc on the XY plane with sphere-like normals, radius 0.5.
///
/// The geometry has no depth (all positions use z = 0), but the normals follow
/// a unit hemisphere: `n = (2x, 2y, sqrt(1 - 4(x^2 + y^2)))`. Billboarded
/// toward the camera and rendered two-sided, the disc reads as a shaded sphere.
pub fn sphere2d(segments: usize, rings: usize) -> MeshData {
    let segments = segments.max(3);
    let rings = rings.max(1);
    let r = 0.5f32;
    let tau = std::f32::consts::TAU;

    let vert_count = 1 + segments * rings;
    let tri_count = segments * (2 * rings - 1);
    let mut mesh = MeshData::with_capacity(vert_count, tri_count * 3);

    let normal_at = |position: Vec3| {
        let nz = (1.0 - 4.0 * (position.x * position.x + position.y * position.y)).max(0.0);
        Vec3::new(2.0 * position.x, 2.0 * position.y, nz.sqrt()).normalize()
    };
    let center = mesh.push_vertex_uv(Vec3::ZERO, Vec3::Z, [0.5, 0.5]);
    for ring in 1..=rings {
        let radius = r * ring as f32 / rings as f32;
        for i in 0..segments {
            let angle = tau * (i as f32 / segments as f32);
            let position = Vec3::new(radius * angle.cos(), radius * angle.sin(), 0.0);
            mesh.push_vertex_uv(
                position,
                normal_at(position),
                [0.5 + position.x, 0.5 - position.y],
            );
        }
    }

    // Center fan to the first ring (CCW seen from +Z).
    let first_ring = 1u32;
    for i in 0..segments {
        let next = (i + 1) % segments;
        mesh.push_triangle(center, first_ring + i as u32, first_ring + next as u32);
    }

    // Quad bands between rings. Winding: inner-i, outer-i, outer-next, inner-next.
    for ring in 1..rings {
        let inner = 1 + (ring - 1) * segments;
        let outer = 1 + ring * segments;
        for i in 0..segments {
            let next = (i + 1) % segments;
            mesh.push_quad(
                (inner + i) as u32,
                (outer + i) as u32,
                (outer + next) as u32,
                (inner + next) as u32,
            );
        }
    }

    mesh
}

// ---------------------------------------------------------------------------
// Wireframe edge extraction
// ---------------------------------------------------------------------------

/// Extract wireframe edges from a MeshData.
/// Returns unique edges as pairs of positions (no duplicates).
/// Rounded Y-axis solid with diameter 1 m and total height 2 m.
pub fn capsule(segments: usize, hemisphere_rings: usize) -> MeshData {
    let segments = segments.clamp(8, 48);
    let rings = hemisphere_rings.clamp(2, 12);
    let mut mesh = MeshData::new();
    for hemisphere in 0..2 {
        for ring in 0..=rings {
            let angle =
                (hemisphere as f32 + ring as f32 / rings as f32) * std::f32::consts::FRAC_PI_2;
            let offset = if hemisphere == 0 { 0.5 } else { -0.5 };
            for side in 0..=segments {
                let azimuth = std::f32::consts::TAU * (side % segments) as f32 / segments as f32;
                let normal = Vec3::new(
                    angle.sin() * azimuth.cos(),
                    angle.cos(),
                    angle.sin() * azimuth.sin(),
                );
                let position = normal * 0.5 + Vec3::Y * offset;
                mesh.push_vertex_uv(
                    position,
                    normal,
                    [side as f32 / segments as f32, (1.0 - position.y) * 0.5],
                );
            }
        }
    }
    let row = segments + 1;
    for ring in 0..(2 * (rings + 1) - 1) {
        for side in 0..segments {
            let a = (ring * row + side) as u32;
            let b = a + 1;
            let c = a + row as u32;
            let d = c + 1;
            for [x, y, z] in [[a, b, c], [b, d, c]] {
                let pa = mesh.positions[x as usize];
                let pb = mesh.positions[y as usize];
                let pc = mesh.positions[z as usize];
                if (pb - pa).cross(pc - pa).length_squared() > 1e-12 {
                    mesh.push_triangle(x, y, z);
                }
            }
        }
    }
    mesh
}

pub fn extract_edges(mesh: &MeshData) -> Vec<[Vec3; 2]> {
    use std::collections::HashSet;

    let mut edge_set: HashSet<(u32, u32)> = HashSet::new();
    let mut edges = Vec::new();

    for tri in mesh.indices.chunks_exact(3) {
        let pairs = [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])];
        for (a, b) in pairs {
            let key = if a < b { (a, b) } else { (b, a) };
            if edge_set.insert(key) {
                edges.push([mesh.positions[a as usize], mesh.positions[b as usize]]);
            }
        }
    }

    edges
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_vertex_count() {
        let mesh = cube(1);
        assert_eq!(mesh.vertex_count(), 24); // 4 per face * 6 faces
        assert_eq!(mesh.triangle_count(), 12); // 2 per face * 6 faces
        assert!(mesh.validate().is_ok());
    }

    #[test]
    fn cube_normals_correct() {
        let mesh = cube(1);
        // First 4 vertices are +Z face, all should have Z normal
        for i in 0..4 {
            assert!(
                (mesh.normals[i] - Vec3::Z).length() < 0.001,
                "vertex {} normal should be +Z, got {:?}",
                i,
                mesh.normals[i]
            );
        }
    }

    #[test]
    fn cylinder_structure() {
        let mesh = cylinder(8);
        assert!(mesh.validate().is_ok());
        // Side: 2*8 tris + top: 8 tris + bottom: 8 tris = 32
        assert_eq!(mesh.triangle_count(), 32);
    }

    #[test]
    fn sphere_structure() {
        let mesh = sphere(4, 6);
        assert!(mesh.validate().is_ok());
        // Two triangles per quad except one degenerate triangle per pole row.
        assert_eq!(mesh.triangle_count(), (4 - 1) * 6 * 2);
    }

    #[test]
    fn plane_structure() {
        let mesh = plane(1);
        assert_eq!(mesh.vertex_count(), 4);
        assert_eq!(mesh.triangle_count(), 2);
        assert!(mesh.validate().is_ok());
    }

    #[test]
    fn plane_winding_matches_its_authored_normals() {
        let mesh = plane(1);
        for triangle in mesh.indices.chunks_exact(3) {
            let a = mesh.positions[triangle[0] as usize];
            let b = mesh.positions[triangle[1] as usize];
            let c = mesh.positions[triangle[2] as usize];
            let geometric_normal = (b - a).cross(c - a).normalize();
            let authored_normal = mesh.normals[triangle[0] as usize];

            assert!(geometric_normal.dot(authored_normal) > 0.99);
        }
    }

    #[test]
    fn built_in_meshes_have_finite_uvs_and_curved_seams_are_closed() {
        let meshes = [
            cube(1),
            cylinder(8),
            sphere(4, 6),
            plane(1),
            wedge(1),
            torus(12, 6),
            prism(6),
            sphere2d(16, 4),
        ];
        for mesh in &meshes {
            assert_eq!(mesh.uvs.len(), mesh.positions.len());
            assert!(mesh.uvs.iter().flatten().all(|value| value.is_finite()));
            assert!(mesh
                .uvs
                .iter()
                .flatten()
                .all(|value| (0.0..=1.0).contains(value)));
            assert!(mesh.validate().is_ok());
        }

        let cylinder = cylinder(8);
        assert_eq!(cylinder.uvs[0][0], 0.0);
        assert_eq!(cylinder.uvs[16][0], 1.0);

        let sphere = sphere(4, 6);
        assert_eq!(sphere.uvs[0][0], 0.0);
        assert_eq!(sphere.uvs[6][0], 1.0);
    }

    #[test]
    fn edge_extraction() {
        let mesh = cube(1);
        let edges = extract_edges(&mesh);
        // Cube with 24 vertices (duplicated per face) produces more edge keys
        // because index pairs are unique per-face. Exact count depends on
        // triangle fan layout. Just verify it is reasonable and non-empty.
        assert!(
            edges.len() >= 12,
            "expected at least 12 edges, got {}",
            edges.len()
        );
    }

    #[test]
    fn cylinder_bounding_radius() {
        let mesh = cylinder(12);
        let radius = mesh.bounding_radius();
        // Max distance from origin: corner of cap = sqrt(0.5^2 + 0.5^2) ~ 0.707
        assert!(
            radius > 0.7 && radius < 0.72,
            "expected ~0.707, got {}",
            radius
        );
    }

    #[test]
    fn authoring_capsule_geometry_has_outward_winding_normals_and_metric_bounds() {
        let mesh = capsule(16, 4);
        let (min, max) = mesh.aabb();
        assert!((min - Vec3::new(-0.5, -1.0, -0.5)).length() < 1e-5);
        assert!((max - Vec3::new(0.5, 1.0, 0.5)).length() < 1e-5);
        assert_eq!(mesh.indices.len(), 768);
        for tri in mesh.indices.chunks_exact(3) {
            let a = tri[0] as usize;
            let b = tri[1] as usize;
            let c = tri[2] as usize;
            let cross = (mesh.positions[b] - mesh.positions[a])
                .cross(mesh.positions[c] - mesh.positions[a]);
            assert!(cross.dot(mesh.normals[a] + mesh.normals[b] + mesh.normals[c]) > 0.0);
        }
        assert!(mesh
            .normals
            .iter()
            .all(|normal| (normal.length() - 1.0).abs() < 1e-5));
    }

    #[test]
    fn cube_aabb() {
        let mesh = cube(1);
        let (min, max) = mesh.aabb();
        assert!((min - Vec3::splat(-0.5)).length() < 0.001);
        assert!((max - Vec3::splat(0.5)).length() < 0.001);
    }

    fn assert_outward_winding(mesh: &MeshData) {
        for (triangle_index, triangle) in mesh.indices.chunks_exact(3).enumerate() {
            let a = triangle[0] as usize;
            let b = triangle[1] as usize;
            let c = triangle[2] as usize;
            let cross = (mesh.positions[b] - mesh.positions[a])
                .cross(mesh.positions[c] - mesh.positions[a]);
            assert!(
                cross.length_squared() > 1e-12,
                "triangle {triangle_index} is degenerate"
            );
            let authored = mesh.normals[a] + mesh.normals[b] + mesh.normals[c];
            assert!(
                cross.dot(authored) > 0.0,
                "triangle {triangle_index} winds inward"
            );
        }
        assert!(mesh
            .normals
            .iter()
            .all(|normal| (normal.length() - 1.0).abs() < 1e-5));
    }

    #[test]
    fn closed_primitives_have_exterior_winding() {
        for mesh in [
            cylinder(16),
            sphere(8, 12),
            wedge(1),
            torus(16, 8),
            prism(6),
        ] {
            assert_outward_winding(&mesh);
        }
    }

    #[test]
    fn sphere2d_is_a_flat_front_facing_disc() {
        let mesh = sphere2d(16, 4);
        assert!(mesh.validate().is_ok());
        assert!(mesh.positions.iter().all(|position| position.z == 0.0));
        assert!(mesh
            .normals
            .iter()
            .all(|normal| (normal.length() - 1.0).abs() < 1e-5));
        for triangle in mesh.indices.chunks_exact(3) {
            let a = mesh.positions[triangle[0] as usize];
            let b = mesh.positions[triangle[1] as usize];
            let c = mesh.positions[triangle[2] as usize];
            let cross = (b - a).cross(c - a);
            assert!(cross.length_squared() > 1e-12, "degenerate triangle");
            assert!(cross.z > 0.0, "disc triangle does not face +Z");
        }
        // Rim normals are horizontal, center normal is +Z: spherical shading.
        assert!((mesh.normals[0] - Vec3::Z).length() < 1e-5);
        let rim_start = 1 + (4 - 1) * 16;
        let rim = &mesh.normals[rim_start..rim_start + 16];
        assert!(rim.iter().all(|normal| normal.z.abs() < 1e-3));
    }

    #[test]
    fn sphere_has_no_degenerate_pole_triangles() {
        let mesh = sphere(8, 12);
        for triangle in mesh.indices.chunks_exact(3) {
            let a = mesh.positions[triangle[0] as usize];
            let b = mesh.positions[triangle[1] as usize];
            let c = mesh.positions[triangle[2] as usize];
            assert!((b - a).cross(c - a).length_squared() > 1e-12);
        }
    }

    #[test]
    fn cube_face_uvs_place_v_zero_at_the_top() {
        let mesh = cube(1);
        // First face (+Z) corner order: bottom-left, bottom-right,
        // top-right, top-left.
        assert_eq!(mesh.uvs[0], [0.0, 1.0]);
        assert_eq!(mesh.uvs[1], [1.0, 1.0]);
        assert_eq!(mesh.uvs[2], [1.0, 0.0]);
        assert_eq!(mesh.uvs[3], [0.0, 0.0]);
        for face in 0..6 {
            let base = face * 4;
            assert_eq!(mesh.uvs[base + 2][1], 0.0);
            assert_eq!(mesh.uvs[base + 3][1], 0.0);
        }
    }

    #[test]
    fn new_primitives_have_expected_aabbs() {
        let assert_bounds = |mesh: &MeshData, expected_min: Vec3, expected_max: Vec3| {
            let (min, max) = mesh.aabb();
            assert!(
                (min - expected_min).length() < 1e-5,
                "min {:?} != {:?}",
                min,
                expected_min
            );
            assert!(
                (max - expected_max).length() < 1e-5,
                "max {:?} != {:?}",
                max,
                expected_max
            );
        };

        assert_bounds(&wedge(1), Vec3::splat(-0.5), Vec3::splat(0.5));
        assert_bounds(
            &torus(32, 16),
            Vec3::new(-0.5, -0.2, -0.5),
            Vec3::new(0.5, 0.2, 0.5),
        );
        assert_bounds(&prism(4), Vec3::splat(-0.5), Vec3::splat(0.5));
        assert_bounds(
            &sphere2d(16, 4),
            Vec3::new(-0.5, -0.5, 0.0),
            Vec3::new(0.5, 0.5, 0.0),
        );
    }
}
