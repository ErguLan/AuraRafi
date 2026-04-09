use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Base schema version to ensure backward compatibility and avoid save-breaking crashes.
pub const SAVE_SCHEMA_VERSION: &str = "0.4.0";

/// Lightweight save structure decoupled from the heavy `SceneGraph`.
/// Designed to be manipulatable by the Native AI (OpenClaw) and exported natively.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameState {
    pub version: String,
    pub timestamp: f64,
    pub active_zone: String,
    pub player_id: Option<Uuid>,
    
    // Key-value store for arbitrary nested arrays, strings and metadata.
    // e.g. {"inventory_items": "[1, 2, 5]", "current_health": "100"}
    pub dynamic_data: HashMap<String, String>,
}

impl Default for GameState {
    fn default() -> Self {
        Self {
            version: SAVE_SCHEMA_VERSION.to_string(),
            timestamp: 0.0,
            active_zone: "DefaultSpawn".to_string(),
            player_id: None,
            dynamic_data: HashMap::new(),
        }
    }
}

impl GameState {
    /// Deserializes a save string. In future versions, this handles structure migrations.
    pub fn load_safely(data: &str) -> Result<Self, String> {
        let state: GameState = ron::from_str(data).map_err(|e| format!("Save Corrupted: {}", e))?;
        
        // Example Hook: Migration Logic
        if state.version != SAVE_SCHEMA_VERSION {
            // run future migrations over state.dynamic_data
        }
        
        Ok(state)
    }

    pub fn set_data(&mut self, key: &str, value: &str) {
        self.dynamic_data.insert(key.to_string(), value.to_string());
    }

    pub fn get_data(&self, key: &str) -> Option<&String> {
        self.dynamic_data.get(key)
    }
}
