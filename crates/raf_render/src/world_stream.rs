//! World streaming - seamless open world without loading screens.
//!
//! Manages which parts of the world are loaded in memory based on
//! the camera/player position. Like BlackSpace Engine: the world is
//! divided into regions, and regions are loaded/unloaded as you move.
//!
//! No loading screens. No pop-in (with proper LOD). Zero cost when the
//! world is small enough to fit entirely in memory (editor mode).
//!
//! This works with any render backend (CPU painter or wgpu).

use glam::Vec3;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// World region
// ---------------------------------------------------------------------------

/// A region (chunk) of the world that can be loaded/unloaded independently.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldRegion {
    /// Region grid coordinates.
    pub grid_x: i32,
    pub grid_z: i32,
    /// Region size in world units (square: size x size).
    pub size: f32,
    /// Load state.
    pub state: RegionState,
    /// Biome type (for terrain generation / asset selection).
    pub biome: BiomeType,
    /// Number of entities in this region.
    pub entity_count: usize,
    /// Number of triangles in this region (for budget tracking).
    pub triangle_count: usize,
    /// Memory usage in bytes.
    pub mem_bytes: usize,
    /// LOD level currently loaded (0 = full detail, higher = less detail).
    pub loaded_lod: u8,
    /// Path to region data file on disk (for async loading).
    pub data_path: Option<String>,
}

impl WorldRegion {
    /// World-space center of this region.
    pub fn center(&self) -> Vec3 {
        Vec3::new(
            self.grid_x as f32 * self.size + self.size / 2.0,
            0.0,
            self.grid_z as f32 * self.size + self.size / 2.0,
        )
    }

    /// World-space AABB min.
    pub fn min(&self) -> Vec3 {
        Vec3::new(
            self.grid_x as f32 * self.size,
            -100.0,
            self.grid_z as f32 * self.size,
        )
    }

    /// World-space AABB max.
    pub fn max(&self) -> Vec3 {
        Vec3::new(
            (self.grid_x + 1) as f32 * self.size,
            500.0,
            (self.grid_z + 1) as f32 * self.size,
        )
    }
}

/// Load state of a region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegionState {
    /// Not loaded, not requested.
    Unloaded,
    /// Loading requested, waiting for data.
    Loading,
    /// Loaded at reduced LOD (streaming in higher detail).
    PartiallyLoaded,
    /// Fully loaded at target LOD.
    Loaded,
    /// Scheduled for unloading (camera moved away).
    Unloading,
}

impl Default for RegionState {
    fn default() -> Self {
        Self::Unloaded
    }
}

/// Biome type for a region. Affects terrain generation and asset placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BiomeType {
    Plains,
    Forest,
    Desert,
    Mountain,
    Tundra,
    Ocean,
    City,
    Custom(u32),
}

impl Default for BiomeType {
    fn default() -> Self {
        Self::Plains
    }
}

impl BiomeType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Plains => "Plains",
            Self::Forest => "Forest",
            Self::Desert => "Desert",
            Self::Mountain => "Mountain",
            Self::Tundra => "Tundra",
            Self::Ocean => "Ocean",
            Self::City => "City",
            Self::Custom(_) => "Custom",
        }
    }

    pub fn label_es(&self) -> &'static str {
        match self {
            Self::Plains => "Llanura",
            Self::Forest => "Bosque",
            Self::Desert => "Desierto",
            Self::Mountain => "Montana",
            Self::Tundra => "Tundra",
            Self::Ocean => "Oceano",
            Self::City => "Ciudad",
            Self::Custom(_) => "Personalizado",
        }
    }
}

// ---------------------------------------------------------------------------
// World streamer config
// ---------------------------------------------------------------------------

/// Configuration for world streaming.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldStreamConfig {
    /// Whether world streaming is enabled.
    /// When disabled, the entire scene is loaded at once (editor mode).
    pub enabled: bool,
    /// Region size in world units.
    pub region_size: f32,
    /// Load radius in regions around the camera.
    pub load_radius: u32,
    /// Unload distance in regions (regions beyond this are unloaded).
    pub unload_radius: u32,
    /// Maximum loaded regions at once.
    pub max_loaded_regions: usize,
    /// Maximum total triangles across all loaded regions.
    pub max_total_triangles: usize,
    /// Maximum memory budget for loaded regions (bytes).
    pub max_memory_bytes: usize,
    /// Whether to stream LOD levels incrementally
    /// (load low LOD first, then upgrade to high LOD).
    pub incremental_lod: bool,
    /// LOD bias: 0 = use optimal LOD, positive = force lower LOD (cheaper).
    pub lod_bias: i8,
}

impl Default for WorldStreamConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Off by default - only for open world games
            region_size: 128.0,
            load_radius: 3,      // 7x7 = 49 regions max loaded
            unload_radius: 5,
            max_loaded_regions: 49,
            max_total_triangles: 200_000,
            max_memory_bytes: 128 * 1024 * 1024, // 128 MB
            incremental_lod: true,
            lod_bias: 0,
        }
    }
}

impl WorldStreamConfig {
    /// Ultra-lightweight preset for potato PCs.
    pub fn potato() -> Self {
        Self {
            enabled: true,
            region_size: 128.0,
            load_radius: 1,     // 3x3 = 9 regions
            unload_radius: 2,
            max_loaded_regions: 9,
            max_total_triangles: 50_000,
            max_memory_bytes: 32 * 1024 * 1024, // 32 MB
            incremental_lod: true,
            lod_bias: 2, // Force low LOD
        }
    }

    /// High quality preset for powerful hardware.
    pub fn high() -> Self {
        Self {
            enabled: true,
            region_size: 256.0,
            load_radius: 5,     // 11x11 = 121 regions
            unload_radius: 7,
            max_loaded_regions: 121,
            max_total_triangles: 2_000_000,
            max_memory_bytes: 512 * 1024 * 1024, // 512 MB
            incremental_lod: true,
            lod_bias: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// World streamer state
// ---------------------------------------------------------------------------

/// Runtime state for the world streaming system.
#[derive(Debug, Clone, Default)]
pub struct WorldStreamState {
    /// All known regions.
    pub regions: Vec<WorldRegion>,
    /// Currently loaded region count.
    pub loaded_count: usize,
    /// Regions currently loading (async).
    pub loading_count: usize,
    /// Total triangles across loaded regions.
    pub total_triangles: usize,
    /// Total memory of loaded regions.
    pub total_memory: usize,
    /// Last camera grid position (for movement detection).
    pub last_camera_grid: (i32, i32),
    /// Whether the camera has moved to a new region since last update.
    pub camera_region_changed: bool,
}

impl WorldStreamState {
    /// Update camera position and detect region change.
    pub fn update_camera(&mut self, camera_pos: Vec3, region_size: f32) {
        let gx = (camera_pos.x / region_size).floor() as i32;
        let gz = (camera_pos.z / region_size).floor() as i32;
        self.camera_region_changed = (gx, gz) != self.last_camera_grid;
        self.last_camera_grid = (gx, gz);
    }

    /// Get regions that should be loaded based on camera position.
    pub fn regions_to_load(&self, config: &WorldStreamConfig) -> Vec<(i32, i32)> {
        let mut to_load = Vec::new();
        let r = config.load_radius as i32;
        let (cx, cz) = self.last_camera_grid;

        for dz in -r..=r {
            for dx in -r..=r {
                let gx = cx + dx;
                let gz = cz + dz;
                let is_loaded = self.regions.iter().any(|reg| {
                    reg.grid_x == gx && reg.grid_z == gz && reg.state == RegionState::Loaded
                });
                if !is_loaded {
                    to_load.push((gx, gz));
                }
            }
        }
        to_load
    }

    /// Get regions that should be unloaded (too far from camera).
    pub fn regions_to_unload(&self, config: &WorldStreamConfig) -> Vec<usize> {
        let mut to_unload = Vec::new();
        let ur = config.unload_radius as i32;
        let (cx, cz) = self.last_camera_grid;

        for (i, region) in self.regions.iter().enumerate() {
            if region.state == RegionState::Loaded {
                let dx = (region.grid_x - cx).abs();
                let dz = (region.grid_z - cz).abs();
                if dx > ur || dz > ur {
                    to_unload.push(i);
                }
            }
        }
        to_unload
    }

    /// Recompute stats from region data.
    pub fn update_stats(&mut self) {
        self.loaded_count = self.regions.iter()
            .filter(|r| r.state == RegionState::Loaded || r.state == RegionState::PartiallyLoaded)
            .count();
        self.loading_count = self.regions.iter()
            .filter(|r| r.state == RegionState::Loading)
            .count();
        self.total_triangles = self.regions.iter()
            .filter(|r| r.state == RegionState::Loaded || r.state == RegionState::PartiallyLoaded)
            .map(|r| r.triangle_count)
            .sum();
        self.total_memory = self.regions.iter()
            .filter(|r| r.state == RegionState::Loaded || r.state == RegionState::PartiallyLoaded)
            .map(|r| r.mem_bytes)
            .sum();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_disabled() {
        let config = WorldStreamConfig::default();
        assert!(!config.enabled);
    }

    #[test]
    fn potato_preset_small() {
        let config = WorldStreamConfig::potato();
        assert!(config.enabled);
        assert_eq!(config.max_loaded_regions, 9);
        assert_eq!(config.lod_bias, 2);
    }

    #[test]
    fn camera_region_detection() {
        let mut state = WorldStreamState::default();
        state.update_camera(Vec3::new(200.0, 0.0, 200.0), 128.0);
        assert!(state.camera_region_changed);
        state.update_camera(Vec3::new(201.0, 0.0, 201.0), 128.0);
        assert!(!state.camera_region_changed); // Same region
    }

    #[test]
    fn regions_to_load() {
        let state = WorldStreamState::default();
        let config = WorldStreamConfig {
            enabled: true,
            load_radius: 1,
            ..Default::default()
        };
        let to_load = state.regions_to_load(&config);
        assert_eq!(to_load.len(), 9); // 3x3 grid
    }
}
