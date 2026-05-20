//! Core mesh data structure: indexed triangle mesh.
//!
//! MeshData is the universal geometry format for the renderer.
//! All primitives, imported models, and procedural geometry produce MeshData.
//!
//! Layout:
//! - positions: one Vec3 per vertex (object-space)
//! - normals: one Vec3 per vertex (object-space, unit length)
//! - indices: groups of 3 forming triangles (CCW winding = front face)
//!
//! Winding convention: counter-clockwise (CCW) when viewed from the
//! front face (the side the normal points toward). This is consistent
//! with glam's right-handed coordinate system (look_at_rh, perspective_rh).
//!
//! Vertex duplication: vertices are duplicated at hard edges so each
//! vertex carries a single normal. A cube has 24 vertices (4 per face),
//! not 8, because each face needs its own normal direction.

use glam::Vec3;

/// Indexed triangle mesh. The fundamental geometry primitive for the renderer.
///
/// All geometry in the engine -- built-in primitives, imported models,
/// procedural meshes -- is represented as MeshData before entering the
/// render pipeline.
///
/// Invariants:
/// - `normals.len() == positions.len()` (one normal per vertex)
/// - `indices.len() % 3 == 0` (complete triangles only)
/// - All indices are `< positions.len()`
/// - Triangle winding is CCW when viewed from the front face
#[derive(Debug, Clone)]
pub struct MeshData {
    /// Vertex positions in object space.
    pub positions: Vec<Vec3>,
    /// Vertex normals in object space (unit length).
    pub normals: Vec<Vec3>,
    /// Triangle indices (groups of 3, CCW winding).
    pub indices: Vec<u32>,
}

impl MeshData {
    /// Create an empty mesh.
    pub fn new() -> Self {
        Self {
            positions: Vec::new(),
            normals: Vec::new(),
            indices: Vec::new(),
        }
    }

    /// Create a mesh with pre-allocated capacity.
    pub fn with_capacity(vertex_count: usize, index_count: usize) -> Self {
        Self {
            positions: Vec::with_capacity(vertex_count),
            normals: Vec::with_capacity(vertex_count),
            indices: Vec::with_capacity(index_count),
        }
    }

    /// Number of vertices.
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    /// Number of triangles.
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Push a vertex (position + normal). Returns the vertex index.
    pub fn push_vertex(&mut self, position: Vec3, normal: Vec3) -> u32 {
        let index = self.positions.len() as u32;
        self.positions.push(position);
        self.normals.push(normal);
        index
    }

    /// Push a triangle (3 vertex indices, CCW winding).
    pub fn push_triangle(&mut self, a: u32, b: u32, c: u32) {
        self.indices.push(a);
        self.indices.push(b);
        self.indices.push(c);
    }

    /// Push a quad as two triangles (CCW winding).
    /// Vertices must be in CCW order: a-b-c-d forms triangles (a,b,c) and (a,c,d).
    pub fn push_quad(&mut self, a: u32, b: u32, c: u32, d: u32) {
        self.push_triangle(a, b, c);
        self.push_triangle(a, c, d);
    }

    /// Compute the axis-aligned bounding box.
    /// Returns (min, max) corners. Returns zero vectors if empty.
    pub fn aabb(&self) -> (Vec3, Vec3) {
        if self.positions.is_empty() {
            return (Vec3::ZERO, Vec3::ZERO);
        }
        let mut min = self.positions[0];
        let mut max = self.positions[0];
        for p in &self.positions[1..] {
            min = min.min(*p);
            max = max.max(*p);
        }
        (min, max)
    }

    /// Compute the bounding sphere radius (centered at origin).
    pub fn bounding_radius(&self) -> f32 {
        let mut max_sq = 0.0f32;
        for p in &self.positions {
            let d = p.length_squared();
            if d > max_sq {
                max_sq = d;
            }
        }
        max_sq.sqrt()
    }

    /// Validate mesh invariants. Returns Ok(()) if valid.
    #[cfg(debug_assertions)]
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.normals.len() != self.positions.len() {
            return Err("normals.len() != positions.len()");
        }
        if self.indices.len() % 3 != 0 {
            return Err("indices.len() is not a multiple of 3");
        }
        let vertex_count = self.positions.len() as u32;
        for idx in &self.indices {
            if *idx >= vertex_count {
                return Err("index out of bounds");
            }
        }
        Ok(())
    }
}

impl Default for MeshData {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_mesh() {
        let mesh = MeshData::new();
        assert_eq!(mesh.vertex_count(), 0);
        assert_eq!(mesh.triangle_count(), 0);
    }

    #[test]
    fn push_single_triangle() {
        let mut mesh = MeshData::new();
        let a = mesh.push_vertex(Vec3::ZERO, Vec3::Y);
        let b = mesh.push_vertex(Vec3::X, Vec3::Y);
        let c = mesh.push_vertex(Vec3::Z, Vec3::Y);
        mesh.push_triangle(a, b, c);

        assert_eq!(mesh.vertex_count(), 3);
        assert_eq!(mesh.triangle_count(), 1);
        assert!(mesh.validate().is_ok());
    }

    #[test]
    fn quad_produces_two_triangles() {
        let mut mesh = MeshData::new();
        let a = mesh.push_vertex(Vec3::new(-1.0, 0.0, -1.0), Vec3::Y);
        let b = mesh.push_vertex(Vec3::new(1.0, 0.0, -1.0), Vec3::Y);
        let c = mesh.push_vertex(Vec3::new(1.0, 0.0, 1.0), Vec3::Y);
        let d = mesh.push_vertex(Vec3::new(-1.0, 0.0, 1.0), Vec3::Y);
        mesh.push_quad(a, b, c, d);

        assert_eq!(mesh.triangle_count(), 2);
        assert!(mesh.validate().is_ok());
    }

    #[test]
    fn aabb_single_point() {
        let mut mesh = MeshData::new();
        mesh.push_vertex(Vec3::new(3.0, 5.0, 7.0), Vec3::Y);
        let (min, max) = mesh.aabb();
        assert_eq!(min, Vec3::new(3.0, 5.0, 7.0));
        assert_eq!(max, Vec3::new(3.0, 5.0, 7.0));
    }
}
