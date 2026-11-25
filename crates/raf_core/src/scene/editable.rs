//! Persistent editable mesh payload stored on a scene node.
//!
//! The renderer owns the interactive editing algorithms; this type is the
//! serializable, backend-neutral geometry that survives save/load and can be
//! authored by the Agent or CLI.

use std::collections::{HashMap, HashSet};

use glam::Vec3;
use serde::{Deserialize, Serialize};

pub const MAX_EDITABLE_VERTICES: usize = 32_768;
pub const MAX_EDITABLE_TRIANGLES: usize = 65_536;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EditableMeshData {
    pub vertices: Vec<[f32; 3]>,
    /// Triangle indices. Every three entries form one face.
    pub indices: Vec<u32>,
    /// Monotonic authoring revision used to invalidate renderer caches.
    #[serde(default)]
    pub revision: u64,
}

impl EditableMeshData {
    pub fn new(vertices: Vec<[f32; 3]>, indices: Vec<u32>) -> Result<Self, String> {
        let mesh = Self {
            vertices,
            indices,
            revision: 1,
        };
        mesh.validate()?;
        Ok(mesh)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.vertices.is_empty() || self.indices.is_empty() {
            return Err("Editable mesh requires vertices and faces.".into());
        }
        if self.vertices.len() > MAX_EDITABLE_VERTICES
            || self.indices.len() / 3 > MAX_EDITABLE_TRIANGLES
            || self.indices.len() % 3 != 0
        {
            return Err("Editable mesh exceeds the bounded topology budget.".into());
        }
        if self
            .vertices
            .iter()
            .flatten()
            .any(|value| !value.is_finite())
            || self
                .indices
                .iter()
                .any(|index| *index as usize >= self.vertices.len())
        {
            return Err("Editable mesh contains invalid vertices or indices.".into());
        }
        Ok(())
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for vertex in &self.vertices {
            let point = Vec3::from_array(*vertex);
            if !point.is_finite() {
                return None;
            }
            min = min.min(point);
            max = max.max(point);
        }
        (min.is_finite() && max.is_finite()).then_some((min, max))
    }

    pub fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1).max(1);
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty() || self.indices.is_empty()
    }

    /// Triangle positions in local space. Used by the renderer and picking.
    pub fn triangles(&self) -> impl Iterator<Item = [Vec3; 3]> + '_ {
        self.indices.chunks_exact(3).map(|triangle| {
            [
                Vec3::from_array(self.vertices[triangle[0] as usize]),
                Vec3::from_array(self.vertices[triangle[1] as usize]),
                Vec3::from_array(self.vertices[triangle[2] as usize]),
            ]
        })
    }

    /// Area-weighted smooth normals derived from the current topology.
    pub fn vertex_normals(&self) -> Vec<Vec3> {
        let mut normals = vec![Vec3::ZERO; self.vertices.len()];
        for triangle in self.indices.chunks_exact(3) {
            let a = Vec3::from_array(self.vertices[triangle[0] as usize]);
            let b = Vec3::from_array(self.vertices[triangle[1] as usize]);
            let c = Vec3::from_array(self.vertices[triangle[2] as usize]);
            let normal = (b - a).cross(c - a);
            for &index in triangle {
                normals[index as usize] += normal;
            }
        }
        for normal in &mut normals {
            *normal = normal.normalize_or_zero();
        }
        normals
    }

    pub fn vertex_position(&self, index: u32) -> Option<Vec3> {
        self.vertices
            .get(index as usize)
            .copied()
            .map(Vec3::from_array)
    }

    pub fn face_normal(&self, face: u32) -> Option<Vec3> {
        let offset = face as usize * 3;
        let triangle = self.indices.get(offset..offset + 3)?;
        let a = self.vertex_position(triangle[0])?;
        let b = self.vertex_position(triangle[1])?;
        let c = self.vertex_position(triangle[2])?;
        Some((b - a).cross(c - a).normalize_or_zero())
    }

    pub fn face_center(&self, face: u32) -> Option<Vec3> {
        let offset = face as usize * 3;
        let triangle = self.indices.get(offset..offset + 3)?;
        Some(
            (self.vertex_position(triangle[0])?
                + self.vertex_position(triangle[1])?
                + self.vertex_position(triangle[2])?)
                / 3.0,
        )
    }

    pub fn face_count(&self) -> usize {
        self.indices.len() / 3
    }

    fn require_vertex(&self, index: u32) -> Result<usize, String> {
        let index = index as usize;
        if index < self.vertices.len() {
            Ok(index)
        } else {
            Err(format!("Vertex index {index} is outside the mesh."))
        }
    }

    fn require_face(&self, face: u32) -> Result<usize, String> {
        if (face as usize) < self.face_count() {
            Ok(face as usize)
        } else {
            Err(format!("Face index {face} is outside the mesh."))
        }
    }

    /// Move the given vertices by a local-space delta.
    pub fn move_vertices(&mut self, indices: &[u32], delta: [f32; 3]) -> Result<usize, String> {
        let delta = Vec3::from_array(delta);
        if !delta.is_finite() {
            return Err("Vertex delta must be finite.".into());
        }
        let mut touched = 0;
        for &index in indices {
            let index = self.require_vertex(index)?;
            self.vertices[index] = (Vec3::from_array(self.vertices[index]) + delta).to_array();
            touched += 1;
        }
        if touched > 0 {
            self.touch();
        }
        Ok(touched)
    }

    /// Scale vertices around a local-space center. Axis factors may be negative.
    pub fn scale_vertices(
        &mut self,
        indices: &[u32],
        center: [f32; 3],
        factor: [f32; 3],
    ) -> Result<usize, String> {
        let center = Vec3::from_array(center);
        let factor = Vec3::from_array(factor);
        if !center.is_finite() || !factor.is_finite() {
            return Err("Scale center and factors must be finite.".into());
        }
        let mut touched = 0;
        for &index in indices {
            let index = self.require_vertex(index)?;
            let point = Vec3::from_array(self.vertices[index]);
            self.vertices[index] = (center + (point - center) * factor).to_array();
            touched += 1;
        }
        if touched > 0 {
            self.touch();
        }
        Ok(touched)
    }

    /// Extrude one or more triangles along their average normal, creating real
    /// side walls on region borders. Returns the duplicated vertices and the
    /// newly created triangle indices.
    pub fn extrude_faces(
        &mut self,
        faces: &[u32],
        distance: f32,
    ) -> Result<MeshEditOutcome, String> {
        if faces.is_empty() || !distance.is_finite() {
            return Err("Extrude requires faces and a finite distance.".into());
        }
        let selected: HashSet<usize> = faces
            .iter()
            .map(|face| self.require_face(*face))
            .collect::<Result<_, _>>()?;

        let mut normal = Vec3::ZERO;
        for &face in &selected {
            normal += self.face_normal(face as u32).unwrap_or(Vec3::ZERO);
        }
        normal = normal.normalize_or_zero();
        if normal == Vec3::ZERO {
            return Err("Selected faces have no stable normal.".into());
        }
        let offset = normal * distance;

        let mut duplicates: HashMap<u32, u32> = HashMap::new();
        let mut created_vertices = Vec::new();
        for &face in &selected {
            let offset_index = face * 3;
            for cursor in offset_index..offset_index + 3 {
                let old = self.indices[cursor];
                let new = *duplicates.entry(old).or_insert_with(|| {
                    let position =
                        (Vec3::from_array(self.vertices[old as usize]) + offset).to_array();
                    self.vertices.push(position);
                    (self.vertices.len() - 1) as u32
                });
                let _ = new;
            }
        }
        for &old in duplicates.keys() {
            created_vertices.push(duplicates[&old]);
        }

        // Count how many selected triangles use each undirected edge. An edge
        // used once is a region border and needs a side wall.
        let mut edge_use: HashMap<(u32, u32), usize> = HashMap::new();
        for &face in &selected {
            let offset_index = face * 3;
            let triangle = [
                self.indices[offset_index],
                self.indices[offset_index + 1],
                self.indices[offset_index + 2],
            ];
            for edge in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                let key = if edge.0 < edge.1 {
                    edge
                } else {
                    (edge.1, edge.0)
                };
                *edge_use.entry(key).or_insert(0) += 1;
            }
        }

        let mut side_triangles = Vec::new();
        for &face in &selected {
            let offset_index = face * 3;
            let triangle = [
                self.indices[offset_index],
                self.indices[offset_index + 1],
                self.indices[offset_index + 2],
            ];
            for (a, b) in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                let key = if a < b { (a, b) } else { (b, a) };
                if edge_use.get(&key).copied().unwrap_or(0) != 1 {
                    continue;
                }
                let a_new = duplicates[&a];
                let b_new = duplicates[&b];
                side_triangles.push([a, b, b_new]);
                side_triangles.push([a, b_new, a_new]);
            }
        }

        for &face in &selected {
            let offset_index = face * 3;
            for cursor in offset_index..offset_index + 3 {
                let old = self.indices[cursor];
                self.indices[cursor] = duplicates[&old];
            }
        }

        let first_new_face = self.face_count() as u32;
        for triangle in &side_triangles {
            self.indices.extend_from_slice(triangle);
        }
        let created_faces = (first_new_face..self.face_count() as u32).collect::<Vec<_>>();
        self.validate_budget()?;
        self.touch();
        Ok(MeshEditOutcome {
            created_vertices,
            created_faces,
        })
    }

    /// Inset each selected triangle towards its own center and build the
    /// surrounding ring. Adjacent selected triangles are inset independently.
    pub fn inset_faces(&mut self, faces: &[u32], amount: f32) -> Result<MeshEditOutcome, String> {
        if faces.is_empty() || !amount.is_finite() || !(0.0..1.0).contains(&amount) {
            return Err("Inset amount must be within 0.0..1.0.".into());
        }
        let selected: Vec<usize> = faces
            .iter()
            .map(|face| self.require_face(*face))
            .collect::<Result<_, _>>()?;

        let mut created_vertices = Vec::new();
        let mut created_faces = Vec::new();
        for &face in &selected {
            let offset_index = face * 3;
            let corners = [
                self.indices[offset_index],
                self.indices[offset_index + 1],
                self.indices[offset_index + 2],
            ];
            let points = corners.map(|index| Vec3::from_array(self.vertices[index as usize]));
            let center = (points[0] + points[1] + points[2]) / 3.0;
            let mut inner = [0u32; 3];
            for (slot, point) in points.iter().enumerate() {
                let position = center + (*point - center) * (1.0 - amount);
                self.vertices.push(position.to_array());
                inner[slot] = (self.vertices.len() - 1) as u32;
                created_vertices.push(inner[slot]);
            }
            self.indices[offset_index] = inner[0];
            self.indices[offset_index + 1] = inner[1];
            self.indices[offset_index + 2] = inner[2];

            for edge in 0..3 {
                let next = (edge + 1) % 3;
                let a = corners[edge];
                let b = corners[next];
                let a_inner = inner[edge];
                let b_inner = inner[next];
                created_faces.push(self.face_count() as u32);
                self.indices.extend_from_slice(&[a, b, b_inner]);
                created_faces.push(self.face_count() as u32);
                self.indices.extend_from_slice(&[a, b_inner, a_inner]);
            }
        }
        self.validate_budget()?;
        self.touch();
        Ok(MeshEditOutcome {
            created_vertices,
            created_faces,
        })
    }

    /// Subdivide selected triangles into four, sharing edge midpoints with
    /// neighbouring selected triangles.
    pub fn subdivide_faces(&mut self, faces: &[u32]) -> Result<MeshEditOutcome, String> {
        if faces.is_empty() {
            return Err("Subdivide requires at least one face.".into());
        }
        let selected: Vec<usize> = faces
            .iter()
            .map(|face| self.require_face(*face))
            .collect::<Result<_, _>>()?;
        let mut created_vertices = Vec::new();
        let mut midpoints: HashMap<(u32, u32), u32> = HashMap::new();
        let mut extra_faces: Vec<[u32; 3]> = Vec::new();

        for &face in &selected {
            let offset_index = face * 3;
            let corners = [
                self.indices[offset_index],
                self.indices[offset_index + 1],
                self.indices[offset_index + 2],
            ];
            let mut mid = [0u32; 3];
            for edge in 0..3 {
                let a = corners[edge];
                let b = corners[(edge + 1) % 3];
                let key = if a < b { (a, b) } else { (b, a) };
                mid[edge] = *midpoints.entry(key).or_insert_with(|| {
                    let a = Vec3::from_array(self.vertices[a as usize]);
                    let b = Vec3::from_array(self.vertices[b as usize]);
                    self.vertices.push(((a + b) * 0.5).to_array());
                    let index = (self.vertices.len() - 1) as u32;
                    created_vertices.push(index);
                    index
                });
            }
            self.indices[offset_index] = corners[0];
            self.indices[offset_index + 1] = mid[0];
            self.indices[offset_index + 2] = mid[2];
            extra_faces.push([mid[0], corners[1], mid[1]]);
            extra_faces.push([mid[2], mid[1], corners[2]]);
            extra_faces.push([mid[0], mid[1], mid[2]]);
        }

        let first_new_face = self.face_count() as u32;
        for triangle in &extra_faces {
            self.indices.extend_from_slice(triangle);
        }
        let created_faces = (first_new_face..self.face_count() as u32).collect();
        self.validate_budget()?;
        self.touch();
        Ok(MeshEditOutcome {
            created_vertices,
            created_faces,
        })
    }

    /// Insert a vertex on a triangle and split it into three, keeping the
    /// original winding. This is the topology behind the Tab Mode right-click
    /// "add point" gesture.
    pub fn poke_face(&mut self, face: u32, point: [f32; 3]) -> Result<u32, String> {
        if point.iter().any(|value| !value.is_finite()) {
            return Err("Poke point must be finite.".into());
        }
        let face = self.require_face(face)?;
        let offset = face * 3;
        let corners = [
            self.indices[offset],
            self.indices[offset + 1],
            self.indices[offset + 2],
        ];
        if self.vertices.len() >= MAX_EDITABLE_VERTICES
            || self.face_count() + 2 > MAX_EDITABLE_TRIANGLES
        {
            return Err("Editable mesh exceeded its topology budget.".into());
        }
        self.vertices.push(point);
        let new_vertex = (self.vertices.len() - 1) as u32;
        self.indices[offset] = corners[0];
        self.indices[offset + 1] = corners[1];
        self.indices[offset + 2] = new_vertex;
        self.indices
            .extend_from_slice(&[corners[1], corners[2], new_vertex]);
        self.indices
            .extend_from_slice(&[corners[2], corners[0], new_vertex]);
        self.touch();
        Ok(new_vertex)
    }

    /// Delete triangles and compact vertices that are no longer referenced.
    pub fn delete_faces(&mut self, faces: &[u32]) -> Result<usize, String> {
        if faces.is_empty() {
            return Err("Delete requires at least one face.".into());
        }
        let mut selected: Vec<usize> = faces
            .iter()
            .map(|face| self.require_face(*face))
            .collect::<Result<_, _>>()?;
        selected.sort_unstable();
        selected.dedup();
        for &face in selected.iter().rev() {
            let start = face * 3;
            self.indices.drain(start..start + 3);
        }
        self.compact_vertices();
        self.touch();
        Ok(selected.len())
    }

    /// Weld vertices that occupy the same position (within tolerance) so
    /// dragging one corner moves every face that shares it. Render meshes often
    /// duplicate vertices per UV/normal; authored topology must not.
    pub fn weld_vertices(&mut self, tolerance: f32) -> usize {
        let tolerance = tolerance.max(1e-6);
        let inverse = 1.0 / tolerance;
        let mut buckets: HashMap<(i64, i64, i64), u32> = HashMap::new();
        let mut remap = vec![0u32; self.vertices.len()];
        let mut welded = Vec::with_capacity(self.vertices.len());
        for (index, vertex) in self.vertices.iter().enumerate() {
            let key = (
                (vertex[0] * inverse).round() as i64,
                (vertex[1] * inverse).round() as i64,
                (vertex[2] * inverse).round() as i64,
            );
            let target = *buckets.entry(key).or_insert_with(|| {
                welded.push(*vertex);
                (welded.len() - 1) as u32
            });
            remap[index] = target;
        }
        if welded.len() == self.vertices.len() {
            return 0;
        }
        let removed = self.vertices.len() - welded.len();
        self.vertices = welded;
        for index in &mut self.indices {
            *index = remap[*index as usize];
        }
        self.touch();
        removed
    }

    /// Remove unreferenced vertices and remap triangle indices in place.
    pub fn compact_vertices(&mut self) {
        let mut remap = vec![u32::MAX; self.vertices.len()];
        let mut kept = Vec::with_capacity(self.vertices.len());
        for triangle in self.indices.chunks_exact(3) {
            for &index in triangle {
                let slot = &mut remap[index as usize];
                if *slot == u32::MAX {
                    *slot = kept.len() as u32;
                    kept.push(self.vertices[index as usize]);
                }
            }
        }
        if kept.len() == self.vertices.len() {
            return;
        }
        for index in &mut self.indices {
            *index = remap[*index as usize];
        }
        self.vertices = kept;
    }

    /// Append a vertex and return its index.
    pub fn add_vertex(&mut self, position: [f32; 3]) -> Result<u32, String> {
        if position.iter().any(|value| !value.is_finite()) {
            return Err("Vertex position must be finite.".into());
        }
        if self.vertices.len() >= MAX_EDITABLE_VERTICES {
            return Err("Editable mesh exceeded its vertex budget.".into());
        }
        self.vertices.push(position);
        self.touch();
        Ok((self.vertices.len() - 1) as u32)
    }

    /// Append one triangle and return its face index.
    pub fn add_triangle(&mut self, triangle: [u32; 3]) -> Result<u32, String> {
        for index in triangle {
            self.require_vertex(index)?;
        }
        if self.face_count() >= MAX_EDITABLE_TRIANGLES {
            return Err("Editable mesh exceeded its triangle budget.".into());
        }
        let face = self.face_count() as u32;
        self.indices.extend_from_slice(&triangle);
        self.touch();
        Ok(face)
    }

    /// Axis-aligned box topology, useful for Agent-authored meshes.
    pub fn box_mesh(min: Vec3, max: Vec3) -> Result<Self, String> {
        let min = min.min(max);
        let max = min.max(max);
        let (min, max) = (min, max);
        let vertices = vec![
            [min.x, min.y, min.z],
            [max.x, min.y, min.z],
            [max.x, max.y, min.z],
            [min.x, max.y, min.z],
            [min.x, min.y, max.z],
            [max.x, min.y, max.z],
            [max.x, max.y, max.z],
            [min.x, max.y, max.z],
        ];
        let indices = vec![
            4, 5, 6, 4, 6, 7, // +Z
            1, 0, 3, 1, 3, 2, // -Z
            0, 4, 7, 0, 7, 3, // -X
            5, 1, 2, 5, 2, 6, // +X
            3, 7, 6, 3, 6, 2, // +Y
            0, 1, 5, 0, 5, 4, // -Y
        ];
        Self::new(vertices, indices)
    }

    fn validate_budget(&self) -> Result<(), String> {
        if self.vertices.len() > MAX_EDITABLE_VERTICES || self.face_count() > MAX_EDITABLE_TRIANGLES
        {
            return Err("Editable mesh exceeded its topology budget.".into());
        }
        Ok(())
    }
}

/// Result of a topology-changing edit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MeshEditOutcome {
    pub created_vertices: Vec<u32>,
    pub created_faces: Vec<u32>,
}

/// Local pivot placement for an editable or parametric shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PivotMode {
    #[default]
    Center,
    Base,
    Corner,
}

impl PivotMode {
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Center => "center",
            Self::Base => "base",
            Self::Corner => "corner",
        }
    }

    pub fn from_slug(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "center" | "centre" => Some(Self::Center),
            "base" | "bottom" => Some(Self::Base),
            "corner" => Some(Self::Corner),
            _ => None,
        }
    }

    /// Offset applied to the authored local mesh so the chosen point becomes
    /// the transform origin.
    pub fn offset(self, min: Vec3, max: Vec3) -> Vec3 {
        match self {
            Self::Center => (min + max) * 0.5,
            Self::Base => Vec3::new((min.x + max.x) * 0.5, min.y, (min.z + max.z) * 0.5),
            Self::Corner => Vec3::new(min.x, min.y, min.z),
        }
    }
}

/// Minimal surface policy that the renderer can honour without a PBR stack.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SurfaceStyle {
    pub texture_tiling: [f32; 2],
    pub unlit: bool,
    pub double_sided: bool,
}

impl Default for SurfaceStyle {
    fn default() -> Self {
        Self {
            texture_tiling: [1.0, 1.0],
            unlit: false,
            double_sided: false,
        }
    }
}

impl SurfaceStyle {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self
            .texture_tiling
            .iter()
            .any(|value| !value.is_finite() || *value <= 0.0 || *value > 128.0)
        {
            return Err("Surface tiling must be finite and within 0..128.");
        }
        Ok(())
    }
}

/// A spawn marker is an entity component, not a second scene type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SpawnMarker {
    pub enabled: bool,
    pub tag: String,
    pub radius: f32,
}

impl Default for SpawnMarker {
    fn default() -> Self {
        Self {
            enabled: true,
            tag: "spawn.default".to_string(),
            radius: 0.5,
        }
    }
}

impl SpawnMarker {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.tag.len() > 128 || self.tag.chars().any(char::is_control) {
            return Err("Spawn tag must be bounded text.");
        }
        if !self.radius.is_finite() || !(0.01..=100.0).contains(&self.radius) {
            return Err("Spawn radius must be within 0.01..100.");
        }
        Ok(())
    }
}

/// Linked Assembly identity. Instances keep their own scene nodes; the source
/// is a reusable `.prefab.ron` subtree and the version is its schema version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinkedAssembly {
    /// Project-relative source below `assets/`.
    pub source: String,
    #[serde(default = "default_assembly_version")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_hash: Option<String>,
}

const fn default_assembly_version() -> u32 {
    1
}

impl LinkedAssembly {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.source.is_empty()
            || self.source.starts_with('/')
            || self.source.contains('\\')
            || self.source.contains(':')
            || self
                .source
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err("Assembly source must be project-relative.");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editable_mesh_roundtrip_and_bounds() {
        let mesh = EditableMeshData::new(
            vec![[-0.5, -0.5, 0.0], [0.5, -0.5, 0.0], [0.0, 0.5, 0.0]],
            vec![0, 1, 2],
        )
        .unwrap();
        assert_eq!(mesh.triangle_count(), 1);
        assert_eq!(
            mesh.bounds(),
            Some((Vec3::new(-0.5, -0.5, 0.0), Vec3::new(0.5, 0.5, 0.0)))
        );
    }

    #[test]
    fn extrude_builds_side_walls_and_moves_the_cap() {
        let mut mesh =
            EditableMeshData::box_mesh(Vec3::splat(-0.5), Vec3::splat(0.5)).expect("box mesh");
        let top_faces = vec![8, 9];
        let before_vertices = mesh.vertices.len();
        let outcome = mesh.extrude_faces(&top_faces, 1.0).expect("extrude");
        assert_eq!(outcome.created_vertices.len(), 4);
        assert_eq!(outcome.created_faces.len(), 8);
        assert_eq!(mesh.vertices.len(), before_vertices + 4);
        assert!(mesh
            .vertex_position(outcome.created_vertices[0])
            .is_some_and(|point| (point.y - 1.5).abs() < 1e-5));
        mesh.validate().expect("valid extruded mesh");
    }

    #[test]
    fn inset_and_subdivide_preserve_valid_topology() {
        let mut mesh =
            EditableMeshData::box_mesh(Vec3::splat(-0.5), Vec3::splat(0.5)).expect("box mesh");
        mesh.inset_faces(&[0], 0.2).expect("inset");
        mesh.validate().expect("valid inset mesh");
        let faces = (0..mesh.face_count() as u32).collect::<Vec<_>>();
        mesh.subdivide_faces(&faces).expect("subdivide");
        mesh.validate().expect("valid subdivided mesh");
        assert_eq!(mesh.face_count(), 72);
    }

    #[test]
    fn poke_face_splits_one_triangle_into_three_and_keeps_winding() {
        let mut mesh =
            EditableMeshData::box_mesh(Vec3::splat(-0.5), Vec3::splat(0.5)).expect("box mesh");
        let before_normal = mesh.face_normal(0).expect("face normal");
        let vertex = mesh.poke_face(0, [0.0, 0.0, 0.6]).expect("poke");
        assert_eq!(vertex, 8);
        assert_eq!(mesh.vertices.len(), 9);
        assert_eq!(mesh.face_count(), 14);
        let after_normal = mesh.face_normal(0).expect("face normal");
        assert!(before_normal.dot(after_normal) > 0.9);
        mesh.validate().expect("valid poked mesh");
    }

    #[test]
    fn welding_shared_positions_keeps_one_editable_corner() {
        let mut mesh = EditableMeshData::new(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
            ],
            vec![0, 1, 2, 3, 4, 5],
        )
        .expect("mesh");
        assert_eq!(mesh.weld_vertices(1e-4), 2);
        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.indices, vec![0, 1, 2, 0, 1, 3]);
    }

    #[test]
    fn delete_faces_compacts_unused_vertices() {
        let mut mesh =
            EditableMeshData::box_mesh(Vec3::splat(-0.5), Vec3::splat(0.5)).expect("box mesh");
        mesh.delete_faces(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9])
            .expect("delete");
        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.face_count(), 2);
        mesh.validate().expect("valid remaining mesh");
    }

    #[test]
    fn pivot_offsets_are_stable() {
        let min = Vec3::new(-1.0, 0.0, -2.0);
        let max = Vec3::new(1.0, 4.0, 2.0);
        assert_eq!(PivotMode::Center.offset(min, max), Vec3::new(0.0, 2.0, 0.0));
        assert_eq!(PivotMode::Base.offset(min, max), Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(PivotMode::Corner.offset(min, max), min);
    }
}
