//! Host operation modules.
//!
//! These modules organize the Host API functions by domain. The actual
//! implementation lives in `host_api.rs` (on `ScriptContext` and
//! `NodeHandle`). These modules provide free-function wrappers for use
//! by backends that need standalone functions (e.g. Rhai registration
//! closures, WASM import trampolines).

pub mod audio_ops;
pub mod input_ops;
pub mod interop_ops;
pub mod property_ops;
pub mod scene_ops;
pub mod time_ops;
pub mod transform_ops;
