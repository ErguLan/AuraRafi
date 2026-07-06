//! WASM Native Module backend (Tier 2).
//!
//! Loads `.wasm` files compiled from C++, Rust, Zig, or AssemblyScript
//! and executes them in a sandboxed WASM runtime. The module imports
//! the AuraRafi Host ABI (a set of WASM import functions) to call the
//! engine.
//!
//! Status: STUB. The Host ABI is designed (see docs/SCRIPTING_SYSTEM.md
//! section 4) but no WASM runtime is wired yet. This module returns
//! `WasmNotImplemented` for all operations. Implementation is Phase D
//! of the roadmap.

use crate::errors::ScriptError;
use crate::host_api::ScriptContext;
use crate::ScriptResult;

use super::{ExecutionResult, LoadedScript, ScriptTier};

/// Load metadata for a WASM module from its path.
/// Checks if the file exists; entry points are always assumed present
/// (WASM modules export their entry functions explicitly).
pub fn load_metadata(path: &str, source_or_bytes: &[u8]) -> LoadedScript {
    let _ = source_or_bytes;
    LoadedScript {
        tier: ScriptTier::Wasm,
        path: path.to_string(),
        has_on_start: true,
        has_on_update: true,
        has_on_destroy: false,
    }
}

/// Compile (instantiate) a WASM module.
/// Not implemented yet.
pub fn compile_module(_path: &str, _wasm_bytes: &[u8]) -> ScriptResult<()> {
    Err(ScriptError::WasmNotImplemented)
}

/// Call the `on_start` exported function of a WASM module.
pub fn call_on_start(
    _module: &WasmModuleHandle,
    _ctx: &mut ScriptContext<'_>,
) -> ExecutionResult {
    ExecutionResult::error(ScriptError::WasmNotImplemented.to_string())
}

/// Call the `on_update(dt)` exported function of a WASM module.
pub fn call_on_update(
    _module: &WasmModuleHandle,
    _ctx: &mut ScriptContext<'_>,
    _dt: f32,
) -> ExecutionResult {
    ExecutionResult::error(ScriptError::WasmNotImplemented.to_string())
}

/// Call the `on_destroy` exported function of a WASM module.
pub fn call_on_destroy(
    _module: &WasmModuleHandle,
    _ctx: &mut ScriptContext<'_>,
) -> ExecutionResult {
    ExecutionResult::error(ScriptError::WasmNotImplemented.to_string())
}

/// Opaque handle to a loaded WASM module instance.
/// Not constructable until the WASM runtime is wired (Phase D).
pub struct WasmModuleHandle {
    _private: (),
}

impl WasmModuleHandle {
    /// Placeholder. Real construction happens when a WASM runtime
    /// (wasmtime, wasmer, or a lightweight interpreter) is integrated.
    pub fn load(_path: &str) -> ScriptResult<Self> {
        Err(ScriptError::WasmNotImplemented)
    }
}

/// The AuraRafi Host ABI version.
/// WASM modules must declare this in their custom section.
/// A mismatch causes `ScriptError::VersionMismatch`.
pub const HOST_ABI_VERSION: u32 = 1;
