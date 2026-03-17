//! ML / AI training data bridge (future).
//!
//! Provides interfaces for:
//! - Exporting simulation data as training datasets (CSV, JSON)
//! - Running inference from a trained model to control entities
//! - Headless batch simulation for parallel training runs
//!
//! This module is a structural placeholder. The actual ML runtime
//! will be integrated when the node executor and headless mode
//! are fully operational.

use serde::{Deserialize, Serialize};

/// Format for exporting training data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportFormat {
    /// JSON Lines: one JSON object per line.
    JsonLines,
    /// CSV with headers.
    Csv,
}

/// Configuration for an ML training session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingConfig {
    /// Number of simulation ticks to record.
    pub max_ticks: u64,
    /// Export format.
    pub format: ExportFormat,
    /// Output file path.
    pub output_path: String,
    /// Number of parallel instances to run (headless mode).
    pub parallel_instances: u32,
    /// Whether to run in headless mode (no UI).
    pub headless: bool,
}

impl Default for TrainingConfig {
    fn default() -> Self {
        Self {
            max_ticks: 10_000,
            format: ExportFormat::JsonLines,
            output_path: "training_data.jsonl".to_string(),
            parallel_instances: 1,
            headless: true,
        }
    }
}

/// Inference bridge: feeds sensor data to a model and gets commands back.
///
/// Structural placeholder. The actual inference will depend on
/// the ML framework chosen (ONNX Runtime, tch-rs, or custom).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceConfig {
    /// Path to the trained model file.
    pub model_path: String,
    /// Input tensor shape description.
    pub input_shape: Vec<usize>,
    /// Output tensor shape description.
    pub output_shape: Vec<usize>,
}

impl Default for InferenceConfig {
    fn default() -> Self {
        Self {
            model_path: String::new(),
            input_shape: Vec::new(),
            output_shape: Vec::new(),
        }
    }
}
