//! Script execution backends.
//!
//! Each backend bridges a different scripting language/paradigm to the
//! shared Host API:
//! - `rhai_backend`: Tier 1, primary scripting language (sandboxed, pure Rust).
//! - `wasm_backend`: Tier 2, native-performance modules via WASM (stub for now).
//! - `node_backend`: Tier 3, visual node graph executor wired to Host API.

pub mod node_backend;
pub mod rhai_backend;
pub mod wasm_backend;

use crate::errors::ScriptError;
use crate::value::ScriptValue;

/// The language a script is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptTier {
    Rhai,
    Wasm,
    Nodes,
}

impl ScriptTier {
    pub fn label(self) -> &'static str {
        match self {
            Self::Rhai => "Rhai",
            Self::Wasm => "WASM Native Module",
            Self::Nodes => "Visual Nodes",
        }
    }
}

/// Result of loading a script for execution.
#[derive(Debug)]
pub struct LoadedScript {
    pub tier: ScriptTier,
    pub path: String,
    pub has_on_start: bool,
    pub has_on_update: bool,
    pub has_on_destroy: bool,
}

/// Result of executing a script for one frame.
#[derive(Debug, Clone, Default)]
pub struct ExecutionResult {
    pub logs: Vec<String>,
    pub errors: Vec<String>,
    pub success: bool,
}

impl ExecutionResult {
    pub fn ok() -> Self {
        Self { logs: Vec::new(), errors: Vec::new(), success: true }
    }

    pub fn error(msg: impl Into<String>) -> Self {
        Self { logs: Vec::new(), errors: vec![msg.into()], success: false }
    }

    pub fn from_error(error: &ScriptError) -> Self {
        Self::error(error.to_string())
    }
}

/// A value returned from a script function call.
pub type ScriptReturn = ScriptValue;
