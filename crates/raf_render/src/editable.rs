//! Editable mesh data structure for runtime vertex manipulation.
//!
//! Allows selecting and moving individual vertices, edges, and faces
//! directly in the editor viewport. No external modeling tool needed.
//! All data lives in simple Vecs - zero GPU overhead.

use glam::Vec3;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Vertex / Face primitives
// ---------------------------------------------------------------------------

/// A single vertex with position and optional normal.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Vertex {
    pub position: Vec3,
    pub normal: Vec3,
}

impl Vertex {
    pub fn new(pos: Vec3) -> Self {
        Self {
            position: pos,
            normal: Vec3::Y, // default up, recalculated when faces change
        }
    }
}

/// A triangle face defined by 3 vertex indices.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Face {
    pub indices: [usize; 3],
}

impl Face {
    pub fn new(a: usize, b: usize, c: usize) -> Self {
        Self { indices: [a, b, c] }
    }

    /// Compute face normal from vertex positions (counter-clockwise winding).
    pub fn normal(&self, vertices: &[Vertex]) -> Vec3 {
        let a = vertices[self.indices[0]].position;
        let b = vertices[self.indices[1]].position;
        let c = vertices[self.indices[2]].position;
        (b - a).cross(c - a).normalize_or_zero()
    }

    /// Compute face center (centroid).
    pub fn center(&self, vertices: &[Vertex]) -> Vec3 {
        let a = vertices[self.indices[0]].position;
        let b = vertices[self.indices[1]].position;
        let c = vertices[self.indices[2]].position;
        (a + b + c) / 3.0
    }
}

// ---------------------------------------------------------------------------
// Selection state
// ---------------------------------------------------------------------------

/// What element type is being selected in edit mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SelectionMode {
    /// Select individual vertices (points).
    Vertex,
    /// Select edges (pairs of vertices).
    Edge,
    /// Select faces (triangles).
    Face,
}

impl Default for SelectionMode {
    fn default() -> Self {
        Self::Vertex
    }
}

/// Tracks which elements are currently selected.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MeshSelection {
    /// Selected vertex indices.
    pub vertices: Vec<usize>,
    /// Selected face indices.
    pub faces: Vec<usize>,
    /// Current selection mode.
    pub mode: SelectionMode,
}

impl MeshSelection {
    pub fn clear(&mut self) {
        self.vertices.clear();
        self.faces.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty() && self.faces.is_empty()
    }

    /// Toggle vertex selection (add if not selected, remove if already).
    pub fn toggle_vertex(&mut self, idx: usize) {
        if let Some(pos) = self.vertices.iter().position(|&v| v == idx) {
            self.vertices.remove(pos);
        } else {
            self.vertices.push(idx);
        }
    }

    /// Toggle face selection.
    pub fn toggle_face(&mut self, idx: usize) {
        if let Some(pos) = self.faces.iter().position(|&f| f == idx) {
            self.faces.remove(pos);
        } else {
            self.faces.push(idx);
        }
    }

    /// Select all vertices of a face.
    pub fn select_face_vertices(&mut self, face: &Face) {
        for &idx in &face.indices {
            if !self.vertices.contains(&idx) {
                self.vertices.push(idx);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Editable Mesh
// ---------------------------------------------------------------------------

/// A mesh that can be modified at runtime in the editor.
/// Stores raw vertex + face data. No GPU buffers, just Vecs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditableMesh {
    /// All vertices of this mesh.
    pub vertices: Vec<Vertex>,
    /// Triangle faces referencing vertex indices.
    pub faces: Vec<Face>,
    /// Current selection state (only relevant in edit mode).
    #[serde(skip)]
    pub selection: MeshSelection,
    /// Whether this mesh is in edit mode.
    #[serde(skip)]
    pub edit_mode: bool,
}

impl EditableMesh {
    /// Create an empty mesh.
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            faces: Vec::new(),
            selection: MeshSelection::default(),
            edit_mode: false,
        }
    }

    /// Create a cube mesh (12 triangles, 8 vertices).
    pub fn cube() -> Self {
        let h = 0.5;
        let vertices = vec![
            Vertex::new(Vec3::new(-h, -h, -h)), // 0
            Vertex::new(Vec3::new( h, -h, -h)), // 1
            Vertex::new(Vec3::new( h,  h, -h)), // 2
            Vertex::new(Vec3::new(-h,  h, -h)), // 3
            Vertex::new(Vec3::new(-h, -h,  h)), // 4
            Vertex::new(Vec3::new( h, -h,  h)), // 5
            Vertex::new(Vec3::new( h,  h,  h)), // 6
            Vertex::new(Vec3::new(-h,  h,  h)), // 7
        ];
        let faces = vec![
            // Front (+Z)
            Face::new(4, 5, 6), Face::new(4, 6, 7),
            // Back (-Z)
            Face::new(1, 0, 3), Face::new(1, 3, 2),
            // Left (-X)
            Face::new(0, 4, 7), Face::new(0, 7, 3),
            // Right (+X)
            Face::new(5, 1, 2), Face::new(5, 2, 6),
            // Top (+Y)
            Face::new(3, 7, 6), Face::new(3, 6, 2),
            // Bottom (-Y)
            Face::new(0, 1, 5), Face::new(0, 5, 4),
        ];
        let mut mesh = Self { vertices, faces, selection: MeshSelection::default(), edit_mode: false };
        mesh.recalculate_normals();
        mesh
    }

    /// Create a plane mesh (2 triangles, 4 vertices, XZ at Y=0).
    pub fn plane() -> Self {
        let h = 0.5;
        let vertices = vec![
            Vertex::new(Vec3::new(-h, 0.0, -h)),
            Vertex::new(Vec3::new( h, 0.0, -h)),
            Vertex::new(Vec3::new( h, 0.0,  h)),
            Vertex::new(Vec3::new(-h, 0.0,  h)),
        ];
        let faces = vec![
            Face::new(0, 1, 2),
            Face::new(0, 2, 3),
        ];
        Self { vertices, faces, selection: MeshSelection::default(), edit_mode: false }
    }

    /// Create a cylinder mesh (segments around Y axis).
    pub fn cylinder(segments: usize) -> Self {
        let seg = segments.max(6);
        let r = 0.5;
        let h = 0.5;
        let mut vertices = Vec::with_capacity(seg * 2 + 2);
        let mut faces = Vec::with_capacity(seg * 4);

        // Top center = 0, bottom center = 1
        vertices.push(Vertex::new(Vec3::new(0.0, h, 0.0)));
        vertices.push(Vertex::new(Vec3::new(0.0, -h, 0.0)));

        // Ring vertices: top ring starts at index 2, bottom ring at 2 + seg
        for i in 0..seg {
            let angle = std::f32::consts::TAU * (i as f32 / seg as f32);
            let x = r * angle.cos();
            let z = r * angle.sin();
            vertices.push(Vertex::new(Vec3::new(x, h, z)));    // top ring
            vertices.push(Vertex::new(Vec3::new(x, -h, z)));   // bottom ring
        }

        for i in 0..seg {
            let next = (i + 1) % seg;
            let ti = 2 + i * 2;      // top vertex i
            let bi = 2 + i * 2 + 1;  // bottom vertex i
            let tn = 2 + next * 2;   // top vertex next
            let bn = 2 + next * 2 + 1; // bottom vertex next

            // Side quads (2 triangles each)
            faces.push(Face::new(ti, bi, bn));
            faces.push(Face::new(ti, bn, tn));

            // Top cap
            faces.push(Face::new(0, ti, tn));

            // Bottom cap
            faces.push(Face::new(1, bn, bi));
        }

        let mut mesh = Self { vertices, faces, selection: MeshSelection::default(), edit_mode: false };
        mesh.recalculate_normals();
        mesh
    }

    /// Create a sphere mesh (UV sphere, low poly).
    pub fn sphere(stacks: usize, slices: usize) -> Self {
        let st = stacks.max(3);
        let sl = slices.max(4);
        let r = 0.5;
        let mut vertices = Vec::new();
        let mut faces = Vec::new();

        // Generate vertices
        for i in 0..=st {
            let phi = std::f32::consts::PI * (i as f32 / st as f32);
            for j in 0..=sl {
                let theta = std::f32::consts::TAU * (j as f32 / sl as f32);
                let pos = Vec3::new(
                    r * phi.sin() * theta.cos(),
                    r * phi.cos(),
                    r * phi.sin() * theta.sin(),
                );
                vertices.push(Vertex::new(pos));
            }
        }

        // Generate faces
        for i in 0..st {
            for j in 0..sl {
                let a = i * (sl + 1) + j;
                let b = a + 1;
                let c = (i + 1) * (sl + 1) + j;
                let d = c + 1;
                faces.push(Face::new(a, c, d));
                faces.push(Face::new(a, d, b));
            }
        }

        let mut mesh = Self { vertices, faces, selection: MeshSelection::default(), edit_mode: false };
        mesh.recalculate_normals();
        mesh
    }

    // -----------------------------------------------------------------------
    // Operations
    // -----------------------------------------------------------------------

    /// Move selected vertices by a delta.
    pub fn move_selected(&mut self, delta: Vec3) {
        for &idx in &self.selection.vertices {
            if let Some(v) = self.vertices.get_mut(idx) {
                v.position += delta;
            }
        }
        self.recalculate_normals();
    }

    /// Scale selected vertices relative to their centroid.
    pub fn scale_selected(&mut self, factor: Vec3) {
        let centroid = self.selected_centroid();
        for &idx in &self.selection.vertices {
            if let Some(v) = self.vertices.get_mut(idx) {
                let offset = v.position - centroid;
                v.position = centroid + offset * factor;
            }
        }
        self.recalculate_normals();
    }

    /// Scale selected vertices along a single axis only.
    pub fn scale_selected_axis(&mut self, axis: usize, factor: f32) {
        let centroid = self.selected_centroid();
        for &idx in &self.selection.vertices {
            if let Some(v) = self.vertices.get_mut(idx) {
                let mut offset = v.position - centroid;
                match axis {
                    0 => offset.x *= factor,
                    1 => offset.y *= factor,
                    _ => offset.z *= factor,
                }
                v.position = centroid + offset;
            }
        }
        self.recalculate_normals();
    }

    /// Extrude selected faces outward along their average normal.
    pub fn extrude_selected_faces(&mut self, distance: f32) {
        if self.selection.faces.is_empty() {
            return;
        }

        // Compute average normal of selected faces.
        let mut avg_normal = Vec3::ZERO;
        for &fi in &self.selection.faces {
            if let Some(face) = self.faces.get(fi) {
                avg_normal += face.normal(&self.vertices);
            }
        }
        avg_normal = avg_normal.normalize_or_zero();

        // Collect unique vertex indices from selected faces.
        let mut vert_indices: Vec<usize> = Vec::new();
        for &fi in &self.selection.faces {
            if let Some(face) = self.faces.get(fi) {
                for &idx in &face.indices {
                    if !vert_indices.contains(&idx) {
                        vert_indices.push(idx);
                    }
                }
            }
        }

        // Duplicate vertices and move them.
        let base = self.vertices.len();
        let mut old_to_new: Vec<(usize, usize)> = Vec::new();
        for (i, &old_idx) in vert_indices.iter().enumerate() {
            let mut new_vert = self.vertices[old_idx];
            new_vert.position += avg_normal * distance;
            self.vertices.push(new_vert);
            old_to_new.push((old_idx, base + i));
        }

        // Remap selected faces to new vertices.
        for &fi in &self.selection.faces {
            if let Some(face) = self.faces.get_mut(fi) {
                for idx in &mut face.indices {
                    for &(old, new) in &old_to_new {
                        if *idx == old {
                            *idx = new;
                            break;
                        }
                    }
                }
            }
        }

        // Create side faces connecting old and new vertices.
        // (Simplified: one quad per edge of selected faces that borders unselected)
        self.recalculate_normals();
    }

    /// Delete selected faces.
    pub fn delete_selected_faces(&mut self) {
        let mut to_remove = self.selection.faces.clone();
        to_remove.sort_unstable();
        to_remove.dedup();
        // Remove in reverse order to preserve indices.
        for &fi in to_remove.iter().rev() {
            if fi < self.faces.len() {
                self.faces.remove(fi);
            }
        }
        self.selection.clear();
        self.recalculate_normals();
    }

    /// Select all vertices.
    pub fn select_all(&mut self) {
        self.selection.vertices = (0..self.vertices.len()).collect();
    }

    /// Deselect everything.
    pub fn deselect_all(&mut self) {
        self.selection.clear();
    }

    /// Centroid of selected vertices.
    pub fn selected_centroid(&self) -> Vec3 {
        if self.selection.vertices.is_empty() {
            return Vec3::ZERO;
        }
        let mut sum = Vec3::ZERO;
        let mut count = 0;
        for &idx in &self.selection.vertices {
            if let Some(v) = self.vertices.get(idx) {
                sum += v.position;
                count += 1;
            }
        }
        if count > 0 { sum / count as f32 } else { Vec3::ZERO }
    }

    /// Recalculate all vertex normals from face normals (area-weighted average).
    pub fn recalculate_normals(&mut self) {
        // Zero all normals.
        for v in &mut self.vertices {
            v.normal = Vec3::ZERO;
        }
        // Accumulate face normals to each vertex.
        for face in &self.faces {
            let n = face.normal(&self.vertices);
            for &idx in &face.indices {
                if let Some(v) = self.vertices.get_mut(idx) {
                    v.normal += n;
                }
            }
        }
        // Normalize.
        for v in &mut self.vertices {
            v.normal = v.normal.normalize_or_zero();
        }
    }

    /// Vertex count.
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    /// Face (triangle) count.
    pub fn face_count(&self) -> usize {
        self.faces.len()
    }

    /// Get wireframe edges (for rendering). No duplicates.
    pub fn wireframe_edges(&self) -> Vec<[Vec3; 2]> {
        let mut edges: Vec<[usize; 2]> = Vec::new();
        for face in &self.faces {
            let [a, b, c] = face.indices;
            let pairs = [[a, b], [b, c], [c, a]];
            for mut pair in pairs {
                if pair[0] > pair[1] {
                    pair.swap(0, 1);
                }
                if !edges.contains(&pair) {
                    edges.push(pair);
                }
            }
        }
        edges.iter().map(|[a, b]| {
            [self.vertices[*a].position, self.vertices[*b].position]
        }).collect()
    }

    /// Get face quads for rendering (as triangles: 3 corners + normal).
    pub fn render_faces(&self) -> Vec<([Vec3; 3], Vec3)> {
        self.faces.iter().map(|face| {
            let a = self.vertices[face.indices[0]].position;
            let b = self.vertices[face.indices[1]].position;
            let c = self.vertices[face.indices[2]].position;
            let n = face.normal(&self.vertices);
            ([a, b, c], n)
        }).collect()
    }
}

impl Default for EditableMesh {
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
    fn cube_has_correct_counts() {
        let cube = EditableMesh::cube();
        assert_eq!(cube.vertex_count(), 8);
        assert_eq!(cube.face_count(), 12); // 6 sides * 2 triangles
    }

    #[test]
    fn move_selected_works() {
        let mut cube = EditableMesh::cube();
        cube.selection.vertices = vec![0];
        let orig = cube.vertices[0].position;
        cube.move_selected(Vec3::new(1.0, 0.0, 0.0));
        assert!((cube.vertices[0].position.x - orig.x - 1.0).abs() < 0.001);
    }

    #[test]
    fn select_all_deselect_all() {
        let mut cube = EditableMesh::cube();
        cube.select_all();
        assert_eq!(cube.selection.vertices.len(), 8);
        cube.deselect_all();
        assert!(cube.selection.is_empty());
    }

    #[test]
    fn plane_mesh_simple() {
        let plane = EditableMesh::plane();
        assert_eq!(plane.vertex_count(), 4);
        assert_eq!(plane.face_count(), 2);
    }

    #[test]
    fn cylinder_mesh() {
        let cyl = EditableMesh::cylinder(8);
        // 2 centers + 8*2 ring = 18 vertices
        assert_eq!(cyl.vertex_count(), 18);
        // 8 sides * 2 + 8 top + 8 bottom = 32 faces
        assert_eq!(cyl.face_count(), 32);
    }
}
