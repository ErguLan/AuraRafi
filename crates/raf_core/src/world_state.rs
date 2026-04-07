//! World state for AI observation.
//!
//! A lightweight, serializable snapshot of the game world that any AI system
//! (Director, mesh generator, behavior agent) can read to make decisions.
//!
//! **Status**: Structure prepared. Not yet connected to the game loop.
//! Integration requires the game loop to update this state each frame.
//! See ROADMAP.md for the AI integration milestone.

use glam::Vec3;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Time & Weather
// ---------------------------------------------------------------------------

/// In-game time of day.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct WorldTime {
    /// Hours (0-23).
    pub hour: u8,
    /// Minutes (0-59).
    pub minute: u8,
    /// Total elapsed game seconds since start.
    pub elapsed_seconds: f64,
    /// Day count (starting from 1).
    pub day: u32,
}

impl Default for WorldTime {
    fn default() -> Self {
        Self {
            hour: 12,
            minute: 0,
            elapsed_seconds: 0.0,
            day: 1,
        }
    }
}

/// Weather conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Weather {
    Clear,
    Cloudy,
    Rain,
    Snow,
    Storm,
    Fog,
}

impl Default for Weather {
    fn default() -> Self {
        Self::Clear
    }
}

// ---------------------------------------------------------------------------
// World State
// ---------------------------------------------------------------------------

/// Readable snapshot of the game world for AI systems.
/// Intentionally flat and simple: no pointers, no references, just data.
/// An AI Director or agent reads this to decide what actions to take.
///
/// **Not yet connected to the game loop.**
/// When integrated, the editor/game loop will update this each frame.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorldState {
    // -- Time --
    /// Current in-game time.
    pub time: WorldTime,
    /// Current weather.
    pub weather: Weather,
    /// Temperature in Celsius (for AI environment reactions).
    pub temperature: f32,

    // -- Player / Camera --
    /// Player or camera position in world space.
    pub camera_position: Vec3,
    /// Direction the player/camera is facing (normalized).
    pub camera_forward: Vec3,

    // -- Scene stats --
    /// Total number of active entities in the scene.
    pub entity_count: usize,
    /// Number of entities near the camera (within interaction range).
    pub nearby_entity_count: usize,
    /// Interaction range in world units.
    pub interaction_range: f32,

    // -- Biome / Region --
    /// Current biome or region name (e.g., "forest", "desert", "city").
    /// Empty if not configured.
    pub biome: String,
    /// Terrain height at camera position.
    pub terrain_height: f32,

    // -- Resources (for survival/building games) --
    /// Generic resource counters. Key = resource name, value = amount.
    /// Only populated if the game uses resources.
    pub resources: Vec<(String, f64)>,

    // -- Custom data --
    /// User-defined key-value pairs for game-specific AI context.
    /// Keeps WorldState extensible without changing the struct.
    pub custom_data: Vec<(String, String)>,
}

impl WorldState {
    /// Create a minimal world state (for games that don't need all fields).
    pub fn minimal() -> Self {
        Self {
            interaction_range: 10.0,
            camera_forward: Vec3::NEG_Z,
            ..Default::default()
        }
    }

    /// Set a custom data value.
    pub fn set_custom(&mut self, key: &str, value: &str) {
        if let Some(entry) = self.custom_data.iter_mut().find(|(k, _)| k == key) {
            entry.1 = value.to_string();
        } else {
            self.custom_data.push((key.to_string(), value.to_string()));
        }
    }

    /// Get a custom data value.
    pub fn get_custom(&self, key: &str) -> Option<&str> {
        self.custom_data.iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Check if it's night time (for AI light/behavior changes).
    pub fn is_night(&self) -> bool {
        self.time.hour < 6 || self.time.hour >= 20
    }

    /// Check if weather is adverse (rain, snow, storm).
    pub fn is_adverse_weather(&self) -> bool {
        matches!(self.weather, Weather::Rain | Weather::Snow | Weather::Storm)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_world_state() {
        let ws = WorldState::default();
        assert_eq!(ws.time.hour, 12);
        assert_eq!(ws.weather, Weather::Clear);
        assert_eq!(ws.entity_count, 0);
    }

    #[test]
    fn night_detection() {
        let mut ws = WorldState::default();
        ws.time.hour = 22;
        assert!(ws.is_night());
        ws.time.hour = 10;
        assert!(!ws.is_night());
    }

    #[test]
    fn custom_data() {
        let mut ws = WorldState::default();
        ws.set_custom("quest_active", "dragon_hunt");
        assert_eq!(ws.get_custom("quest_active"), Some("dragon_hunt"));
        assert_eq!(ws.get_custom("nonexistent"), None);
    }
}
