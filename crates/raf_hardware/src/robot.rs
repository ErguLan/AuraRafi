//! Robot control interface (future).
//!
//! High-level abstraction for controlling robots using AuraRafi.
//! Combines sensor inputs and actuator outputs into a unified
//! control loop. This module is a structural placeholder that
//! will be implemented when the hardware layer is fully active.

use serde::{Deserialize, Serialize};
use crate::sensor::SensorData;
use crate::actuator::ActuatorCommand;

/// Robot control mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RobotMode {
    /// Manual control via UI or node graph.
    Manual,
    /// Autonomous using programmed node graph logic.
    Autonomous,
    /// ML-controlled: actions decided by a trained model.
    MlControlled,
    /// Calibration: special mode for sensor/actuator calibration.
    Calibration,
}

impl Default for RobotMode {
    fn default() -> Self {
        Self::Manual
    }
}

/// Robot state snapshot.
///
/// Captures the current state of all sensors and the last
/// commands sent to actuators. This snapshot can be serialized
/// and used as training data for ML models.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RobotState {
    /// Current control mode.
    pub mode: RobotMode,
    /// All current sensor readings.
    pub sensors: Vec<SensorData>,
    /// All pending actuator commands.
    pub commands: Vec<ActuatorCommand>,
    /// Timestep counter.
    pub tick: u64,
    /// Delta time in seconds since last update.
    pub dt: f64,
}

impl Default for RobotState {
    fn default() -> Self {
        Self {
            mode: RobotMode::Manual,
            sensors: Vec::new(),
            commands: Vec::new(),
            tick: 0,
            dt: 0.0,
        }
    }
}

impl RobotState {
    /// Export state as a JSON line for ML training data.
    pub fn to_training_record(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}
