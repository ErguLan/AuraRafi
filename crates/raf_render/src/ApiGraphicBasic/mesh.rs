use glam::Vec3;

/// A standard vertex definition for the basic graphics API.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BasicVertex {
    /// 3D position of the vertex.
    pub position: Vec3,
    /// Normal vector for lighting calculations.
    pub normal: Vec3,
    /// Texture mapping coordinates.
    pub uv: [f32; 2],
}

/// A unified geometry container that holds vertex and index data.
/// Can be drawn on both CPU and GPU backends.
#[derive(Debug, Clone)]
pub struct BasicMesh {
    /// List of vertices.
    pub vertices: Vec<BasicVertex>,
    /// List of indices for triangle indexing.
    pub indices: Vec<u32>,
}

impl BasicMesh {
    /// Create a new basic mesh.
    pub fn new(vertices: Vec<BasicVertex>, indices: Vec<u32>) -> Self {
        Self { vertices, indices }
    }

    /// Helper to construct a mesh from raw positions and indices (generating flat normals).
    pub fn from_positions(positions: &[Vec3], indices: &[u32]) -> Self {
        let mut vertices = Vec::with_capacity(positions.len());
        for &pos in positions {
            vertices.push(BasicVertex {
                position: pos,
                normal: Vec3::ZERO,
                uv: [0.0, 0.0],
            });
        }

        // Recompute flat normals
        for chunk in indices.chunks_exact(3) {
            let i0 = chunk[0] as usize;
            let i1 = chunk[1] as usize;
            let i2 = chunk[2] as usize;

            if i0 < vertices.len() && i1 < vertices.len() && i2 < vertices.len() {
                let p0 = vertices[i0].position;
                let p1 = vertices[i1].position;
                let p2 = vertices[i2].position;

                let u = p1 - p0;
                let v = p2 - p0;
                let normal = u.cross(v).normalize_or_zero();

                vertices[i0].normal += normal;
                vertices[i1].normal += normal;
                vertices[i2].normal += normal;
            }
        }

        for v in &mut vertices {
            v.normal = v.normal.normalize_or_zero();
        }

        Self {
            vertices,
            indices: indices.to_vec(),
        }
    }
}
