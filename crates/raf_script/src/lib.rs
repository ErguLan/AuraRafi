//! # raf_script
//!
//! The scripting runtime and Host API for AuraRafi.
//!
//! Three tiers of scripting share one Host API:
//! - **Tier 1 (Rhai)**: sandboxed, pure Rust, beginner-friendly. Primary language.
//! - **Tier 2 (WASM)**: native-performance modules compiled from C++/Rust/Zig.
//!   Sandboxed via WASM. Stub for now (Phase D).
//! - **Tier 3 (Visual Nodes)**: no-code node graphs from `raf_nodes`.
//!   Supported Game graphs compile to Rhai and call the same Host API.
//!
//! All tiers call the same `ScriptContext` functions. No tier touches
//! `SceneGraph`, `InputState`, or audio internals directly.
//!
//! See `docs/SCRIPTING_SYSTEM.md` for the full architecture.

pub mod backends;
pub mod camera_api;
pub mod errors;
pub mod host;
pub mod host_api;
pub mod lifetime;
pub mod node_handle;
pub mod prelude;
pub mod runtime;
pub mod value;
pub mod view;

pub use backends::{ExecutionResult, LoadedScript, ScriptTier};
pub use errors::{ScriptError, ScriptResult};
pub use host_api::{AudioCommand, AudioCommandQueue, InputSnapshot, ScriptContext, TimeInfo};
pub use node_handle::{NodeHandle, HOST_API_VERSION};
pub use runtime::{
    RhaiScriptRuntime, ScriptLoadProgress, ScriptRuntimeOptions, ScriptRuntimeReport,
};
pub use value::ScriptValue;

pub use prelude::*;
