//! Mesh merging and grouping operations.
//!
//! - Group: multiple entities treated as one unit (preserves individuals).
//! - Merge: combine multiple meshes into a single mesh (fewer draw calls).
//! - Unmerge: split a merged mesh back into components (undo support).

use glam::Vec3;
use serde::{Deserialize, Serialize};

/// Identifier for a mesh group.
pub type GroupId = u32;

/// Tracks which group (if any) an entity belongs to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshGroup {
    /// Group identifier. Entities with the same group_id move together.
    pub group_id: GroupId,
    /// Human-readable group name.
    pub name: String,
    /// Whether this entity is the group root (the "main" entity).
    pub is_root: bool,
}

/// Result of merging multiple meshes into one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergedMesh {
    /// Combined vertices (positions from all source meshes, transformed).
    pub vertices: Vec<Vec3>,
    /// Combined face indices (offset to match merged vertex array).
    pub faces: Vec<[usize; 3]>,
    /// Which original entity each vertex range came from (for potential unmerge).
    pub source_ranges: Vec<MergeSourceRange>,
}

/// Records where in the merged mesh a particular source mesh lives.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeSourceRange {
    /// Original entity name (for reference).
    pub source_name: String,
    /// Start index in the merged vertex array.
    pub vertex_start: usize,
    /// Number of vertices from this source.
    pub vertex_count: usize,
    /// Start index in the merged face array.
    pub face_start: usize,
    /// Number of faces from this source.
    pub face_count: usize,
}

/// A set of vertices + faces ready to merge (one source mesh).
pub struct MergeInput {
    /// Vertices already transformed to world space.
    pub vertices: Vec<Vec3>,
    /// Face indices (local to this mesh's vertex array).
    pub faces: Vec<[usize; 3]>,
    /// Name for unmerge tracking.
    pub name: String,
}

/// Merge multiple mesh inputs into a single combined mesh.
/// All vertex positions should already be in world space.
pub fn merge_meshes(inputs: &[MergeInput]) -> MergedMesh {
    let total_verts: usize = inputs.iter().map(|m| m.vertices.len()).sum();
    let total_faces: usize = inputs.iter().map(|m| m.faces.len()).sum();

    let mut vertices = Vec::with_capacity(total_verts);
    let mut faces = Vec::with_capacity(total_faces);
    let mut source_ranges = Vec::with_capacity(inputs.len());

    for input in inputs {
        let v_offset = vertices.len();
        let f_offset = faces.len();

        // Copy vertices.
        vertices.extend_from_slice(&input.vertices);

        // Copy faces with index offset.
        for face in &input.faces {
            faces.push([
                face[0] + v_offset,
                face[1] + v_offset,
                face[2] + v_offset,
            ]);
        }

        source_ranges.push(MergeSourceRange {
            source_name: input.name.clone(),
            vertex_start: v_offset,
            vertex_count: input.vertices.len(),
            face_start: f_offset,
            face_count: input.faces.len(),
        });
    }

    MergedMesh {
        vertices,
        faces,
        source_ranges,
    }
}

/// Remove duplicate vertices that are very close together (weld).
/// Returns new vertex array and remapped face indices.
/// `threshold`: maximum distance to consider vertices as duplicates.
pub fn weld_vertices(
    vertices: &[Vec3],
    faces: &[[usize; 3]],
    threshold: f32,
) -> (Vec<Vec3>, Vec<[usize; 3]>) {
    let threshold_sq = threshold * threshold;
    let mut new_verts: Vec<Vec3> = Vec::new();
    let mut remap: Vec<usize> = Vec::with_capacity(vertices.len());

    for v in vertices {
        // Check if there's already a close enough vertex.
        let mut found = None;
        for (i, nv) in new_verts.iter().enumerate() {
            if (*v - *nv).length_squared() < threshold_sq {
                found = Some(i);
                break;
            }
        }
        match found {
            Some(idx) => remap.push(idx),
            None => {
                remap.push(new_verts.len());
                new_verts.push(*v);
            }
        }
    }

    // Remap face indices.
    let new_faces: Vec<[usize; 3]> = faces.iter().map(|f| {
        [remap[f[0]], remap[f[1]], remap[f[2]]]
    }).collect();

    (new_verts, new_faces)
}

/// Count stats for UI display.
pub struct MergeStats {
    pub source_count: usize,
    pub total_vertices: usize,
    pub total_faces: usize,
    pub welded_vertices: usize,
}

/// Compute stats without actually merging (preview).
pub fn merge_preview_stats(inputs: &[MergeInput]) -> MergeStats {
    MergeStats {
        source_count: inputs.len(),
        total_vertices: inputs.iter().map(|m| m.vertices.len()).sum(),
        total_faces: inputs.iter().map(|m| m.faces.len()).sum(),
        welded_vertices: 0, // Would need actual weld to compute
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_two_meshes() {
        let a = MergeInput {
            vertices: vec![Vec3::ZERO, Vec3::X, Vec3::Y],
            faces: vec![[0, 1, 2]],
            name: "A".into(),
        };
        let b = MergeInput {
            vertices: vec![Vec3::Z, Vec3::ONE, Vec3::NEG_ONE],
            faces: vec![[0, 1, 2]],
            name: "B".into(),
        };

        let merged = merge_meshes(&[a, b]);
        assert_eq!(merged.vertices.len(), 6);
        assert_eq!(merged.faces.len(), 2);
        // Second mesh faces should be offset by 3.
        assert_eq!(merged.faces[1], [3, 4, 5]);
        assert_eq!(merged.source_ranges.len(), 2);
    }

    #[test]
    fn weld_removes_duplicates() {
        let verts = vec![
            Vec3::ZERO,
            Vec3::X,
            Vec3::new(0.001, 0.0, 0.0), // Almost same as ZERO
        ];
        let faces = vec![[0, 1, 2]];
        let (welded_v, welded_f) = weld_vertices(&verts, &faces, 0.01);
        assert_eq!(welded_v.len(), 2); // 3rd vertex merged with 1st
        assert_eq!(welded_f[0][2], 0); // Remapped to vertex 0
    }
}
