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
        ([
            Vec3::new(-h, -h, h),
            Vec3::new(h, -h, h),
            Vec3::new(h, h, h),
            Vec3::new(-h, h, h),
        ], Vec3::Z),
        // -Z back
        ([
            Vec3::new(h, -h, -h),
            Vec3::new(-h, -h, -h),
            Vec3::new(-h, h, -h),
            Vec3::new(h, h, -h),
        ], Vec3::NEG_Z),
        // -X left
        ([
            Vec3::new(-h, -h, -h),
            Vec3::new(-h, -h, h),
            Vec3::new(-h, h, h),
            Vec3::new(-h, h, -h),
        ], Vec3::NEG_X),
        // +X right
        ([
            Vec3::new(h, -h, h),
            Vec3::new(h, -h, -h),
            Vec3::new(h, h, -h),
            Vec3::new(h, h, h),
        ], Vec3::X),
        // +Y top
        ([
            Vec3::new(-h, h, h),
            Vec3::new(h, h, h),
            Vec3::new(h, h, -h),
            Vec3::new(-h, h, -h),
        ], Vec3::Y),
        // -Y bottom
        ([
            Vec3::new(-h, -h, -h),
            Vec3::new(h, -h, -h),
            Vec3::new(h, -h, h),
            Vec3::new(-h, -h, h),
        ], Vec3::NEG_Y),
    ];

    for (corners, normal) in &faces {
        let base = mesh.vertex_count() as u32;
        for corner in corners {
            mesh.push_vertex(*corner, *normal);
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

        // Top ring vertex
        mesh.push_vertex(Vec3::new(r * cos_a, h, r * sin_a), normal);
        // Bottom ring vertex
        mesh.push_vertex(Vec3::new(r * cos_a, -h, r * sin_a), normal);
    }

    for i in 0..seg {
        let i0 = side_base + (i as u32) * 2;     // top[i]
        let i1 = i0 + 1;                          // bot[i]
        let i2 = side_base + (i as u32 + 1) * 2;  // top[i+1]
        let i3 = i2 + 1;                          // bot[i+1]
        // CCW from outside: bot[i], bot[i+1], top[i+1], top[i]
        mesh.push_quad(i1, i3, i2, i0);
    }

    // --- Top cap (fan from center, normal = +Y) ---
    let top_center = mesh.push_vertex(Vec3::new(0.0, h, 0.0), Vec3::Y);
    let top_ring_base = mesh.vertex_count() as u32;
    for i in 0..seg {
        let angle = tau * (i as f32 / seg as f32);
        mesh.push_vertex(Vec3::new(r * angle.cos(), h, r * angle.sin()), Vec3::Y);
    }
    for i in 0..seg {
        let next = (i + 1) % seg;
        // CCW from above (+Y): center, current, next
        mesh.push_triangle(top_center, top_ring_base + i as u32, top_ring_base + next as u32);
    }

    // --- Bottom cap (fan from center, normal = -Y) ---
    let bot_center = mesh.push_vertex(Vec3::new(0.0, -h, 0.0), Vec3::NEG_Y);
    let bot_ring_base = mesh.vertex_count() as u32;
    for i in 0..seg {
        let angle = tau * (i as f32 / seg as f32);
        mesh.push_vertex(Vec3::new(r * angle.cos(), -h, r * angle.sin()), Vec3::NEG_Y);
    }
    for i in 0..seg {
        let next = (i + 1) % seg;
        // CCW from below (-Y): center, next, current (reversed because viewing from -Y)
        mesh.push_triangle(bot_center, bot_ring_base + next as u32, bot_ring_base + i as u32);
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
            mesh.push_vertex(position, normal);
        }
    }

    // Generate triangles
    let cols = (sl + 1) as u32;
    for i in 0..st {
        for j in 0..sl {
            let a = (i as u32) * cols + (j as u32);
            let b = a + cols;
            let c = b + 1;
            let d = a + 1;

            // Two triangles per quad (CCW)
            mesh.push_triangle(a, b, c);
            mesh.push_triangle(a, c, d);
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

    let a = mesh.push_vertex(Vec3::new(-h, 0.0, -h), Vec3::Y);
    let b = mesh.push_vertex(Vec3::new(h, 0.0, -h), Vec3::Y);
    let c = mesh.push_vertex(Vec3::new(h, 0.0, h), Vec3::Y);
    let d = mesh.push_vertex(Vec3::new(-h, 0.0, h), Vec3::Y);

    mesh.push_quad(a, b, c, d);
    mesh
}

// ---------------------------------------------------------------------------
// Wireframe edge extraction
// ---------------------------------------------------------------------------

/// Extract wireframe edges from a MeshData.
/// Returns unique edges as pairs of positions (no duplicates).
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
            assert!((mesh.normals[i] - Vec3::Z).length() < 0.001,
                "vertex {} normal should be +Z, got {:?}", i, mesh.normals[i]);
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
        // (stacks * slices * 2) triangles = 4 * 6 * 2 = 48
        assert_eq!(mesh.triangle_count(), 48);
    }

    #[test]
    fn plane_structure() {
        let mesh = plane(1);
        assert_eq!(mesh.vertex_count(), 4);
        assert_eq!(mesh.triangle_count(), 2);
        assert!(mesh.validate().is_ok());
    }

    #[test]
    fn edge_extraction() {
        let mesh = cube(1);
        let edges = extract_edges(&mesh);
        // Cube with 24 vertices (duplicated per face) produces more edge keys
        // because index pairs are unique per-face. Exact count depends on
        // triangle fan layout. Just verify it is reasonable and non-empty.
        assert!(edges.len() >= 12, "expected at least 12 edges, got {}", edges.len());
    }

    #[test]
    fn cylinder_bounding_radius() {
        let mesh = cylinder(12);
        let radius = mesh.bounding_radius();
        // Max distance from origin: corner of cap = sqrt(0.5^2 + 0.5^2) ~ 0.707
        assert!(radius > 0.7 && radius < 0.72,
            "expected ~0.707, got {}", radius);
    }

    #[test]
    fn cube_aabb() {
        let mesh = cube(1);
        let (min, max) = mesh.aabb();
        assert!((min - Vec3::splat(-0.5)).length() < 0.001);
        assert!((max - Vec3::splat(0.5)).length() < 0.001);
    }
}
