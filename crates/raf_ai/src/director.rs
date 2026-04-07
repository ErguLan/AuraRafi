//! AI Director - observes the world and emits actions.
//!
//! The Director reads a WorldState snapshot and decides what should happen
//! in the game world (grow plants, change weather, spawn entities, etc.).
//! It communicates via DirectorAction commands that the engine processes.
//!
//! **Status**: Interface prepared. Not yet connected to game loop or AI providers.
//! The Director needs:
//! 1. WorldState updates each frame (from raf_core)
//! 2. An AI provider to generate decisions (from provider.rs)
//! 3. The engine to process DirectorActions (from CommandBus)
//!
//! See ROADMAP.md for the AI integration milestone.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Director Actions (what the AI can tell the engine to do)
// ---------------------------------------------------------------------------

/// An action the AI Director wants the engine to execute.
/// These are high-level intentions, not raw engine commands.
/// The engine translates these into actual scene mutations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DirectorAction {
    /// Spawn an entity at a position.
    SpawnEntity {
        name: String,
        primitive: String,  // "cube", "sphere", etc.
        position: [f32; 3],
    },

    /// Remove an entity by name.
    RemoveEntity {
        name: String,
    },

    /// Change weather.
    SetWeather {
        weather: String, // "clear", "rain", "storm", etc.
    },

    /// Change time of day.
    SetTime {
        hour: u8,
        minute: u8,
    },

    /// Modify an entity's scale (e.g., "grow a plant").
    ScaleEntity {
        name: String,
        scale: [f32; 3],
    },

    /// Change an entity's color/material (e.g., "leaves turn brown in autumn").
    SetEntityColor {
        name: String,
        color_rgb: [u8; 3],
    },

    /// Log a message to the console (AI narration / debug).
    LogMessage {
        message: String,
    },

    /// Play a sound effect (future).
    PlaySound {
        sound_name: String,
    },

    /// Custom action with key-value data (extensible).
    Custom {
        action_type: String,
        data: Vec<(String, String)>,
    },
}

// ---------------------------------------------------------------------------
// Director config
// ---------------------------------------------------------------------------

/// Director behavior mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DirectorMode {
    /// Director is completely off, zero CPU cost.
    Disabled,
    /// Director observes but only suggests (shows hints in console, no auto-actions).
    Observer,
    /// Director actively modifies the world.
    Active,
}

impl Default for DirectorMode {
    fn default() -> Self {
        // Off by default - user must opt in. Zero cost when disabled.
        Self::Disabled
    }
}

/// Configuration for the AI Director.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectorConfig {
    /// Operating mode.
    pub mode: DirectorMode,
    /// How often the Director evaluates the world (seconds between checks).
    /// Higher = less CPU. Lower = more responsive.
    pub update_interval_secs: f32,
    /// Maximum actions per evaluation cycle (prevents AI spam).
    pub max_actions_per_cycle: usize,
    /// Whether the Director can spawn new entities.
    pub can_spawn: bool,
    /// Whether the Director can remove entities.
    pub can_remove: bool,
    /// Whether the Director can change weather/time.
    pub can_change_environment: bool,
}

impl Default for DirectorConfig {
    fn default() -> Self {
        Self {
            mode: DirectorMode::Disabled,
            update_interval_secs: 5.0,
            max_actions_per_cycle: 3,
            can_spawn: true,
            can_remove: false, // Destructive by default OFF
            can_change_environment: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Director state
// ---------------------------------------------------------------------------

/// Runtime state for the AI Director.
#[derive(Debug, Clone, Default)]
pub struct DirectorState {
    /// Time since last evaluation.
    pub time_since_last_eval: f32,
    /// Actions pending execution.
    pub pending_actions: Vec<DirectorAction>,
    /// Total actions executed so far.
    pub total_actions_executed: usize,
    /// Whether the Director is currently evaluating (waiting for AI response).
    pub evaluating: bool,
}

impl DirectorState {
    /// Check if enough time has passed for a new evaluation.
    pub fn should_evaluate(&self, config: &DirectorConfig) -> bool {
        config.mode != DirectorMode::Disabled
            && !self.evaluating
            && self.time_since_last_eval >= config.update_interval_secs
    }

    /// Advance the timer by delta time (seconds).
    pub fn tick(&mut self, dt: f32) {
        self.time_since_last_eval += dt;
    }

    /// Reset timer after an evaluation.
    pub fn reset_timer(&mut self) {
        self.time_since_last_eval = 0.0;
    }

    /// Queue an action for execution.
    pub fn queue_action(&mut self, action: DirectorAction) {
        self.pending_actions.push(action);
    }

    /// Drain pending actions (engine consumes them).
    pub fn drain_actions(&mut self) -> Vec<DirectorAction> {
        let actions = std::mem::take(&mut self.pending_actions);
        self.total_actions_executed += actions.len();
        actions
    }
}

impl DirectorConfig {
    /// UI label (English).
    pub fn mode_label(&self) -> &'static str {
        match self.mode {
            DirectorMode::Disabled => "Disabled",
            DirectorMode::Observer => "Observer (suggestions only)",
            DirectorMode::Active => "Active (modifies world)",
        }
    }

    /// UI label (Spanish).
    pub fn mode_label_es(&self) -> &'static str {
        match self.mode {
            DirectorMode::Disabled => "Desactivado",
            DirectorMode::Observer => "Observador (solo sugerencias)",
            DirectorMode::Active => "Activo (modifica el mundo)",
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
    fn disabled_by_default() {
        let config = DirectorConfig::default();
        assert_eq!(config.mode, DirectorMode::Disabled);
    }

    #[test]
    fn should_not_evaluate_when_disabled() {
        let config = DirectorConfig::default();
        let mut state = DirectorState::default();
        state.time_since_last_eval = 100.0;
        assert!(!state.should_evaluate(&config));
    }

    #[test]
    fn should_evaluate_when_active_and_time_passed() {
        let mut config = DirectorConfig::default();
        config.mode = DirectorMode::Active;
        let mut state = DirectorState::default();
        state.time_since_last_eval = 10.0;
        assert!(state.should_evaluate(&config));
    }

    #[test]
    fn drain_actions() {
        let mut state = DirectorState::default();
        state.queue_action(DirectorAction::LogMessage {
            message: "test".into(),
        });
        let actions = state.drain_actions();
        assert_eq!(actions.len(), 1);
        assert!(state.pending_actions.is_empty());
        assert_eq!(state.total_actions_executed, 1);
    }
}
