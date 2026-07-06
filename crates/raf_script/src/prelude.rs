//! Convenience re-exports for script authors and engine integrators.
//!
//! When writing code that uses the scripting system, import:
//! ```ignore
//! use raf_script::prelude::*;
//! ```

pub use crate::backends::{
    ExecutionResult, LoadedScript, ScriptReturn, ScriptTier,
};
pub use crate::errors::{ScriptError, ScriptResult};
pub use crate::host_api::{
    AudioCommand, AudioCommandQueue, InputSnapshot, ScriptContext, TimeInfo,
};
pub use crate::lifetime::{hook_description, HOOK_ON_DESTROY, HOOK_ON_START, HOOK_ON_UPDATE};
pub use crate::node_handle::{NodeHandle, HOST_API_VERSION};
pub use crate::value::ScriptValue;
