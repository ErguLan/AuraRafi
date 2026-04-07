//! AI mesh streaming provider interface.
//!
//! Defines how AI-generated mesh data flows into the engine:
//! - MeshChunk: a piece of mesh data (can be streamed incrementally)
//! - MeshProviderConfig: how the provider behaves
//!
//! **Status**: Interface prepared. No provider implemented yet.
//! Future providers could include:
//! - Local model (Point-E, Shap-E running on user's GPU)
//! - Cloud API (external service returns mesh data)
//! - Procedural (algorithmic generation, no AI needed)
//!
//! See ROADMAP.md for the AI integration milestone.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Mesh chunk (streaming unit)
// ---------------------------------------------------------------------------

/// A chunk of mesh data that can be streamed incrementally.
/// Instead of loading an entire AI-generated world at once,
/// the engine receives chunks and renders them as they arrive.
/// This keeps memory usage low even with large generated worlds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshChunk {
    /// Chunk position in world space (grid coordinates).
    pub grid_x: i32,
    pub grid_z: i32,
    /// Vertex positions [x, y, z, ...] in local chunk space.
    pub positions: Vec<f32>,
    /// Triangle indices.
    pub indices: Vec<u32>,
    /// Per-vertex colors [r, g, b, ...] (0.0-1.0). Optional.
    pub colors: Vec<f32>,
    /// Whether this chunk is fully loaded.
    pub complete: bool,
    /// LOD level of this chunk (0 = highest detail).
    pub lod_level: u8,
}

impl MeshChunk {
    /// Vertex count.
    pub fn vertex_count(&self) -> usize {
        self.positions.len() / 3
    }

    /// Triangle count.
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Estimated memory in bytes.
    pub fn mem_bytes(&self) -> usize {
        (self.positions.len() + self.colors.len()) * 4 + self.indices.len() * 4
    }

    /// Create an empty chunk at a grid position.
    pub fn empty(grid_x: i32, grid_z: i32) -> Self {
        Self {
            grid_x,
            grid_z,
            positions: Vec::new(),
            indices: Vec::new(),
            colors: Vec::new(),
            complete: false,
            lod_level: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Provider config
// ---------------------------------------------------------------------------

/// Source of mesh data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MeshProviderType {
    /// No provider (static scene, no streaming).
    None,
    /// Procedural generation (algorithmic, no AI).
    Procedural,
    /// AI model running locally.
    LocalModel,
    /// AI model via cloud API.
    CloudApi,
}

impl Default for MeshProviderType {
    fn default() -> Self {
        Self::None
    }
}

/// Configuration for the mesh streaming provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshProviderConfig {
    /// Type of provider.
    pub provider_type: MeshProviderType,
    /// Chunk size in world units.
    pub chunk_size: f32,
    /// How many chunks to keep loaded around the camera.
    pub load_radius_chunks: u32,
    /// Maximum chunks in memory at once.
    pub max_loaded_chunks: usize,
    /// Maximum total vertices across all loaded chunks.
    pub max_total_vertices: usize,
    /// Whether to stream chunks as camera moves.
    pub stream_on_move: bool,
}

impl Default for MeshProviderConfig {
    fn default() -> Self {
        Self {
            provider_type: MeshProviderType::None,
            chunk_size: 16.0,
            load_radius_chunks: 3,
            max_loaded_chunks: 49, // 7x7 grid around camera
            max_total_vertices: 50_000, // Keep it lightweight
            stream_on_move: false,
        }
    }
}

/// Runtime state for the mesh provider.
#[derive(Debug, Clone, Default)]
pub struct MeshProviderState {
    /// Currently loaded chunks.
    pub loaded_chunks: Vec<MeshChunk>,
    /// Chunks requested but not yet received.
    pub pending_count: usize,
    /// Total vertices currently loaded.
    pub total_vertices: usize,
    /// Last camera grid position (for detecting movement).
    pub last_camera_grid: (i32, i32),
}

impl MeshProviderState {
    /// Check if camera has moved to a new grid cell.
    pub fn camera_moved(&self, camera_x: f32, camera_z: f32, chunk_size: f32) -> bool {
        let gx = (camera_x / chunk_size).floor() as i32;
        let gz = (camera_z / chunk_size).floor() as i32;
        (gx, gz) != self.last_camera_grid
    }

    /// Update camera grid position.
    pub fn update_camera_grid(&mut self, camera_x: f32, camera_z: f32, chunk_size: f32) {
        self.last_camera_grid = (
            (camera_x / chunk_size).floor() as i32,
            (camera_z / chunk_size).floor() as i32,
        );
    }

    /// Add a received chunk. Evicts farthest chunks if over limit.
    pub fn add_chunk(&mut self, chunk: MeshChunk, config: &MeshProviderConfig) {
        self.total_vertices += chunk.vertex_count();
        self.loaded_chunks.push(chunk);

        // Evict chunks if over budget.
        while self.loaded_chunks.len() > config.max_loaded_chunks
            || self.total_vertices > config.max_total_vertices
        {
            if let Some(removed) = self.evict_farthest_chunk() {
                self.total_vertices = self.total_vertices.saturating_sub(removed.vertex_count());
            } else {
                break;
            }
        }
    }

    /// Remove the chunk farthest from camera.
    fn evict_farthest_chunk(&mut self) -> Option<MeshChunk> {
        if self.loaded_chunks.is_empty() {
            return None;
        }
        let cam = self.last_camera_grid;
        let farthest_idx = self.loaded_chunks.iter().enumerate()
            .max_by_key(|(_, c)| {
                let dx = (c.grid_x - cam.0).abs();
                let dz = (c.grid_z - cam.1).abs();
                dx * dx + dz * dz
            })
            .map(|(i, _)| i)?;
        Some(self.loaded_chunks.remove(farthest_idx))
    }

    /// Number of loaded chunks.
    pub fn chunk_count(&self) -> usize {
        self.loaded_chunks.len()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_mem_estimation() {
        let chunk = MeshChunk {
            grid_x: 0,
            grid_z: 0,
            positions: vec![0.0; 9],    // 3 vertices
            indices: vec![0, 1, 2],     // 1 triangle
            colors: vec![],
            complete: true,
            lod_level: 0,
        };
        assert_eq!(chunk.vertex_count(), 3);
        assert_eq!(chunk.triangle_count(), 1);
        assert!(chunk.mem_bytes() > 0);
    }

    #[test]
    fn camera_movement_detection() {
        let state = MeshProviderState::default();
        assert!(state.camera_moved(20.0, 20.0, 16.0)); // (1,1) != (0,0)
        assert!(!state.camera_moved(5.0, 5.0, 16.0)); // (0,0) == (0,0)
    }

    #[test]
    fn eviction_on_budget() {
        let config = MeshProviderConfig {
            max_loaded_chunks: 2,
            max_total_vertices: 100,
            ..Default::default()
        };
        let mut state = MeshProviderState::default();

        for i in 0..5 {
            let chunk = MeshChunk {
                grid_x: i,
                grid_z: 0,
                positions: vec![0.0; 9],
                indices: vec![0, 1, 2],
                colors: vec![],
                complete: true,
                lod_level: 0,
            };
            state.add_chunk(chunk, &config);
        }
        // Should have evicted down to max.
        assert!(state.chunk_count() <= config.max_loaded_chunks);
    }
}
