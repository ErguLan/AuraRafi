//! Error types for the scripting system.

use std::fmt;

/// Result alias used across the Host API.
pub type ScriptResult<T> = Result<T, ScriptError>;

/// Errors that can occur during script compilation or execution.
#[derive(Debug, Clone)]
pub enum ScriptError {
    /// Script file was not found on disk.
    FileNotFound(String),
    /// Script language is not supported by the engine.
    UnsupportedLanguage(String),
    /// Rhai parse or compile error.
    RhaiCompile(String),
    /// Rhai runtime error (panic, type mismatch, etc.).
    RhaiRuntime(String),
    /// WASM module failed to compile or instantiate.
    WasmCompile(String),
    /// WASM trap during execution.
    WasmTrap(String),
    /// WASM backend is not yet implemented.
    WasmNotImplemented,
    /// A node handle refers to an entity that no longer exists.
    InvalidHandle(u64),
    /// A Host API function was called with invalid arguments.
    InvalidArgument(String),
    /// Script exceeded the per-frame operation limit.
    Timeout,
    /// The Host API version declared by the script does not match the engine.
    VersionMismatch { expected: u32, found: u32 },
    /// I/O error reading a script file.
    Io(String),
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileNotFound(p) => write!(f, "Script file not found: {}", p),
            Self::UnsupportedLanguage(lang) => write!(f, "Unsupported script language: {}", lang),
            Self::RhaiCompile(msg) => write!(f, "Rhai compile error: {}", msg),
            Self::RhaiRuntime(msg) => write!(f, "Rhai runtime error: {}", msg),
            Self::WasmCompile(msg) => write!(f, "WASM compile error: {}", msg),
            Self::WasmTrap(msg) => write!(f, "WASM trap: {}", msg),
            Self::WasmNotImplemented => write!(f, "WASM backend not implemented yet"),
            Self::InvalidHandle(h) => write!(f, "Invalid node handle: {}", h),
            Self::InvalidArgument(msg) => write!(f, "Invalid script argument: {}", msg),
            Self::Timeout => write!(f, "Script exceeded per-frame operation limit"),
            Self::VersionMismatch { expected, found } => {
                write!(f, "Host API version mismatch: expected {}, found {}", expected, found)
            }
            Self::Io(msg) => write!(f, "Script I/O error: {}", msg),
        }
    }
}

impl std::error::Error for ScriptError {}

impl From<std::io::Error> for ScriptError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}
