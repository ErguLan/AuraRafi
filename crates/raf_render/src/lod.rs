//! Level of Detail (LOD) system.
//!
//! Automatically switches mesh detail based on camera distance.
//! Zero overhead when not configured - just returns the original mesh.
//! Ultra lightweight: no mesh decimation algorithms, uses pre-built levels.

use serde::{Deserialize, Serialize};

/// A single LOD level with its transition distance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LodLevel {
    /// Maximum camera distance for this level (object switches to next level beyond this).
    pub max_distance: f32,
    /// Mesh detail parameter (e.g., sphere segments, cylinder segments).
    /// Lower = fewer polygons = faster.
    pub detail: u8,
}

/// LOD configuration for an entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LodConfig {
    /// LOD levels sorted by distance (closest first).
    pub levels: Vec<LodLevel>,
    /// Whether LOD is enabled for this entity.
    pub enabled: bool,
    /// Distance beyond the last level where the entity becomes invisible.
    pub cull_distance: f32,
}

impl Default for LodConfig {
    fn default() -> Self {
        Self {
            levels: vec![
                LodLevel { max_distance: 10.0, detail: 3 },  // High detail: close
                LodLevel { max_distance: 25.0, detail: 2 },  // Medium: middle
                LodLevel { max_distance: 50.0, detail: 1 },  // Low: far away
            ],
            enabled: false, // Off by default to keep things simple
            cull_distance: 100.0,
        }
    }
}

impl LodConfig {
    /// Get the detail level for a given camera distance.
    /// Returns None if beyond cull distance (should not render).
    pub fn detail_for_distance(&self, distance: f32) -> Option<u8> {
        if !self.enabled {
            return Some(3); // Max detail when LOD disabled
        }
        if distance > self.cull_distance {
            return None; // Too far, cull entirely
        }
        for level in &self.levels {
            if distance <= level.max_distance {
                return Some(level.detail);
            }
        }
        // Beyond all levels but within cull distance: use lowest detail
        self.levels.last().map(|l| l.detail).or(Some(1))
    }

    /// Convert detail level (1-3) to polygon counts for sphere/cylinder.
    pub fn segments_for_detail(detail: u8) -> usize {
        match detail {
            0 => 4,   // Extremely low poly
            1 => 6,   // Low poly
            2 => 8,   // Medium
            3 => 12,  // High for this engine
            _ => 16,  // Max
        }
    }

    /// Convert detail level to sphere stacks.
    pub fn stacks_for_detail(detail: u8) -> usize {
        match detail {
            0 => 2,
            1 => 3,
            2 => 4,
            _ => 6,
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
    fn lod_disabled_returns_max() {
        let lod = LodConfig::default();
        assert_eq!(lod.detail_for_distance(999.0), Some(3));
    }

    #[test]
    fn lod_enabled_selects_level() {
        let mut lod = LodConfig::default();
        lod.enabled = true;
        assert_eq!(lod.detail_for_distance(5.0), Some(3));  // Close
        assert_eq!(lod.detail_for_distance(15.0), Some(2)); // Mid
        assert_eq!(lod.detail_for_distance(40.0), Some(1)); // Far
    }

    #[test]
    fn lod_cull_beyond_distance() {
        let mut lod = LodConfig::default();
        lod.enabled = true;
        assert_eq!(lod.detail_for_distance(150.0), None);
    }
}
