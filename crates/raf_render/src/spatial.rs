//! Spatial partitioning for efficient rendering and physics queries.
//!
//! Determines WHAT to render based on camera position and frustum.
//! Without spatial partitioning, the engine checks every mesh every frame.
//! With it, only nearby/visible meshes are tested.
//!
//! Uses a grid-based approach (simplest, cheapest memory, good for open worlds).
//! Future: upgrade to octree or BVH for denser scenes.
//!
//! Zero cost: the grid is only built when scene changes. Queries are O(1) per cell.

use glam::Vec3;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Spatial cell
// ---------------------------------------------------------------------------

/// A cell in the spatial grid. Contains indices of objects in this cell.
#[derive(Debug, Clone, Default)]
pub struct SpatialCell {
    /// Indices of meshes/entities in this cell.
    pub entries: Vec<usize>,
}

// ---------------------------------------------------------------------------
// Spatial grid
// ---------------------------------------------------------------------------

/// A uniform grid that partitions 3D space into cells.
/// Objects are assigned to cells based on their position.
/// Frustum culling queries only check cells that overlap the view frustum.
#[derive(Debug, Clone)]
pub struct SpatialGrid {
    /// Cell size in world units.
    pub cell_size: f32,
    /// Grid dimensions (cells per axis).
    pub grid_size: usize,
    /// Grid origin (world position of cell 0,0,0).
    pub origin: Vec3,
    /// Flat array of cells [x + y*grid_size + z*grid_size*grid_size].
    cells: Vec<SpatialCell>,
    /// Total entries across all cells.
    pub total_entries: usize,
}

impl SpatialGrid {
    /// Create a new spatial grid centered at origin.
    /// `world_size` = total extent of the grid in world units.
    /// `cell_size` = size of each cell.
    pub fn new(world_size: f32, cell_size: f32) -> Self {
        let grid_size = (world_size / cell_size).ceil() as usize;
        let total_cells = grid_size * grid_size * grid_size;
        let half = world_size / 2.0;
        Self {
            cell_size,
            grid_size,
            origin: Vec3::new(-half, -half, -half),
            cells: vec![SpatialCell::default(); total_cells],
            total_entries: 0,
        }
    }

    /// Default grid for a medium scene (256 units, 16-unit cells = 16^3 = 4096 cells).
    pub fn medium() -> Self {
        Self::new(256.0, 16.0)
    }

    /// Small grid for editor/small scenes (64 units, 8-unit cells = 8^3 = 512 cells).
    pub fn small() -> Self {
        Self::new(64.0, 8.0)
    }

    /// Large grid for open worlds (1024 units, 32-unit cells = 32^3 = 32768 cells).
    pub fn large() -> Self {
        Self::new(1024.0, 32.0)
    }

    /// Convert world position to grid cell coordinates.
    fn world_to_cell(&self, pos: Vec3) -> Option<(usize, usize, usize)> {
        let local = pos - self.origin;
        let cx = (local.x / self.cell_size).floor() as isize;
        let cy = (local.y / self.cell_size).floor() as isize;
        let cz = (local.z / self.cell_size).floor() as isize;

        let gs = self.grid_size as isize;
        if cx >= 0 && cx < gs && cy >= 0 && cy < gs && cz >= 0 && cz < gs {
            Some((cx as usize, cy as usize, cz as usize))
        } else {
            None
        }
    }

    /// Flat index from 3D cell coords.
    fn cell_index(&self, x: usize, y: usize, z: usize) -> usize {
        x + y * self.grid_size + z * self.grid_size * self.grid_size
    }

    /// Insert an object into the grid at a world position.
    pub fn insert(&mut self, entry_index: usize, position: Vec3) {
        if let Some((cx, cy, cz)) = self.world_to_cell(position) {
            let idx = self.cell_index(cx, cy, cz);
            if idx < self.cells.len() {
                self.cells[idx].entries.push(entry_index);
                self.total_entries += 1;
            }
        }
    }

    /// Clear all entries (call before rebuilding).
    pub fn clear(&mut self) {
        for cell in &mut self.cells {
            cell.entries.clear();
        }
        self.total_entries = 0;
    }

    /// Query: get all entries within a radius of a position.
    /// Returns indices of objects that are in nearby cells.
    pub fn query_radius(&self, center: Vec3, radius: f32) -> Vec<usize> {
        let mut results = Vec::new();
        let cells_radius = (radius / self.cell_size).ceil() as isize;

        if let Some((cx, cy, cz)) = self.world_to_cell(center) {
            let cx = cx as isize;
            let cy = cy as isize;
            let cz = cz as isize;
            let gs = self.grid_size as isize;

            for dz in -cells_radius..=cells_radius {
                for dy in -cells_radius..=cells_radius {
                    for dx in -cells_radius..=cells_radius {
                        let nx = cx + dx;
                        let ny = cy + dy;
                        let nz = cz + dz;
                        if nx >= 0 && nx < gs && ny >= 0 && ny < gs && nz >= 0 && nz < gs {
                            let idx = self.cell_index(nx as usize, ny as usize, nz as usize);
                            results.extend_from_slice(&self.cells[idx].entries);
                        }
                    }
                }
            }
        }

        results
    }

    /// Query: get all entries in the given axis-aligned bounding box.
    pub fn query_aabb(&self, min: Vec3, max: Vec3) -> Vec<usize> {
        let mut results = Vec::new();
        let gs = self.grid_size as isize;

        let min_cell = self.world_to_cell(min).unwrap_or((0, 0, 0));
        let max_cell = self.world_to_cell(max).unwrap_or((
            self.grid_size.saturating_sub(1),
            self.grid_size.saturating_sub(1),
            self.grid_size.saturating_sub(1),
        ));

        for z in min_cell.2..=max_cell.2 {
            for y in min_cell.1..=max_cell.1 {
                for x in min_cell.0..=max_cell.0 {
                    if (x as isize) < gs && (y as isize) < gs && (z as isize) < gs {
                        let idx = self.cell_index(x, y, z);
                        results.extend_from_slice(&self.cells[idx].entries);
                    }
                }
            }
        }

        results
    }

    /// Number of non-empty cells.
    pub fn occupied_cells(&self) -> usize {
        self.cells.iter().filter(|c| !c.entries.is_empty()).count()
    }

    /// Memory usage estimate in bytes.
    pub fn mem_bytes(&self) -> usize {
        let cell_overhead = self.cells.len() * std::mem::size_of::<SpatialCell>();
        let entry_data: usize = self.cells.iter().map(|c| c.entries.len() * 8).sum();
        cell_overhead + entry_data
    }
}

// ---------------------------------------------------------------------------
// Frustum (for view frustum culling)
// ---------------------------------------------------------------------------

/// A view frustum defined by 6 planes. Used for culling.
/// Objects outside the frustum are not submitted to the render backend.
#[derive(Debug, Clone)]
pub struct Frustum {
    /// 6 frustum planes: [near, far, left, right, top, bottom].
    /// Each plane is (normal.x, normal.y, normal.z, distance).
    pub planes: [FrustumPlane; 6],
}

/// A single frustum plane (Ax + By + Cz + D = 0).
#[derive(Debug, Clone, Copy, Default)]
pub struct FrustumPlane {
    pub normal: Vec3,
    pub distance: f32,
}

impl FrustumPlane {
    /// Signed distance from a point to this plane.
    /// Positive = in front (inside frustum side).
    /// Negative = behind (outside frustum side).
    pub fn distance_to_point(&self, point: Vec3) -> f32 {
        self.normal.dot(point) + self.distance
    }
}

impl Frustum {
    /// Check if a point is inside the frustum.
    pub fn contains_point(&self, point: Vec3) -> bool {
        self.planes.iter().all(|p| p.distance_to_point(point) >= 0.0)
    }

    /// Check if a sphere intersects the frustum.
    /// Used for bounding-sphere culling (fast, conservative).
    pub fn intersects_sphere(&self, center: Vec3, radius: f32) -> bool {
        self.planes.iter().all(|p| p.distance_to_point(center) >= -radius)
    }

    /// Placeholder: build frustum from view-projection matrix.
    /// Full implementation requires extracting planes from VP matrix.
    pub fn from_view_projection(_vp: glam::Mat4) -> Self {
        // NOTE: full Gribb-Hartmann plane extraction goes here.
        // For now returns a permissive frustum that accepts everything.
        Self {
            planes: [FrustumPlane {
                normal: Vec3::ZERO,
                distance: f32::MAX,
            }; 6],
        }
    }
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Configuration for spatial partitioning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpatialConfig {
    /// Whether spatial partitioning is enabled.
    pub enabled: bool,
    /// Cell size in world units.
    pub cell_size: f32,
    /// World extent (total grid size).
    pub world_size: f32,
    /// Whether frustum culling is enabled.
    pub frustum_culling: bool,
    /// Whether distance culling is enabled (cull beyond max distance).
    pub distance_culling: bool,
    /// Max render distance.
    pub max_render_distance: f32,
}

impl Default for SpatialConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cell_size: 16.0,
            world_size: 256.0,
            frustum_culling: true,
            distance_culling: true,
            max_render_distance: 500.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_insert_query() {
        let mut grid = SpatialGrid::new(64.0, 8.0);
        grid.insert(0, Vec3::new(1.0, 1.0, 1.0));
        grid.insert(1, Vec3::new(2.0, 2.0, 2.0));

        let nearby = grid.query_radius(Vec3::ZERO, 16.0);
        assert!(nearby.contains(&0));
        assert!(nearby.contains(&1));
    }

    #[test]
    fn grid_out_of_bounds() {
        let mut grid = SpatialGrid::new(10.0, 5.0);
        grid.insert(99, Vec3::new(999.0, 999.0, 999.0)); // Out of bounds
        assert_eq!(grid.total_entries, 0); // Should not be inserted
    }

    #[test]
    fn frustum_sphere_test() {
        let frustum = Frustum::from_view_projection(glam::Mat4::IDENTITY);
        // Permissive frustum accepts everything.
        assert!(frustum.intersects_sphere(Vec3::ZERO, 1.0));
    }
}
