//! Dependency-free extension points. A compiler is not a WASM execution engine.
use super::wasm_host::GuestImports;
use crate::{ScriptError, ScriptResult, ScriptValue};
use std::sync::Arc;

pub const HOST_ABI_VERSION: u32 = crate::HOST_API_VERSION;

#[derive(Debug, Clone, Copy)]
pub struct WasmLimits {
    pub module_bytes: usize,
    pub memory_pages: u32,
    pub instructions_per_call: u64,
    pub entity_handles: usize,
}

impl Default for WasmLimits {
    fn default() -> Self {
        Self {
            module_bytes: 4 * 1024 * 1024,
            memory_pages: 256,
            instructions_per_call: 100_000,
            entity_handles: 1024,
        }
    }
}

impl WasmLimits {
    pub fn validate(self) -> ScriptResult<()> {
        if self.module_bytes == 0
            || self.module_bytes > 16 * 1024 * 1024
            || self.memory_pages == 0
            || self.memory_pages > 1024
            || self.instructions_per_call == 0
            || self.instructions_per_call > 10_000_000
            || self.entity_handles == 0
            || self.entity_handles > 10_000
        {
            return Err(ScriptError::InvalidArgument(
                "Invalid WASM resource limits".into(),
            ));
        }
        Ok(())
    }
}

/// Cheap ingress check only; full module validation belongs to the executor.
pub(crate) fn validate_artifact(bytes: &[u8], limits: WasmLimits) -> ScriptResult<()> {
    limits.validate()?;
    if bytes.len() > limits.module_bytes || !bytes.starts_with(b"\0asm\x01\0\0\0") {
        return Err(ScriptError::WasmCompile(
            "Expected bounded WASM version 1 bytes".into(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmHook {
    Start,
    Update,
    FixedUpdate,
    LateUpdate,
    Event,
    Destroy,
}

impl WasmHook {
    pub fn export(self) -> &'static str {
        match self {
            Self::Start => "on_start",
            Self::Update => "on_update",
            Self::FixedUpdate => "on_fixed_update",
            Self::LateUpdate => "on_late_update",
            Self::Event => "on_event",
            Self::Destroy => "on_destroy",
        }
    }

    pub(crate) fn validate_args(self, args: &[ScriptValue]) -> ScriptResult<()> {
        let valid = match self {
            Self::Start | Self::Destroy => args.is_empty(),
            Self::Update | Self::FixedUpdate | Self::LateUpdate => {
                matches!(args, [ScriptValue::Float(dt)] if dt.is_finite() && *dt >= 0.0)
            }
            Self::Event => {
                matches!(args, [ScriptValue::String(name), _] if !name.is_empty() && name.len() <= 128)
            }
        };
        if !valid {
            return Err(ScriptError::InvalidArgument(
                "Invalid WASM hook arguments".into(),
            ));
        }
        for arg in args {
            arg.validate_event_payload()?;
        }
        Ok(())
    }
}

/// Read from the actual module by the provider, not inferred from its filename.
pub struct WasmExports {
    pub abi_version: u32,
    pub hooks: Vec<WasmHook>,
}

/// Trusted engine adapter. Implementations must validate the complete module,
/// deny WASI/ambient imports and unbounded start functions, and enforce memory,
/// instruction, stack/table and compilation-work limits. No provider ships today.
pub trait WasmExecutor: Send + Sync {
    fn compile(
        &self,
        bytes: &[u8],
        limits: WasmLimits,
    ) -> ScriptResult<Arc<dyn WasmCompiledModule>>;
}

/// Immutable code may be shared; each attachment receives an independent instance.
pub trait WasmCompiledModule: Send + Sync {
    fn exports(&self) -> &WasmExports;
    fn instantiate(&self, limits: WasmLimits) -> ScriptResult<Box<dyn WasmInstance>>;
}

pub trait WasmInstance: Send {
    /// Reset fuel to the supplied per-call limit and trap on exhaustion. Check
    /// cancellation/deadline within execution, not only when calling imports.
    /// Do not retain imports or references after returning. Decode guest memory
    /// with checked pointer/length/UTF-8 validation before invoking an operation.
    fn invoke(
        &mut self,
        hook: WasmHook,
        args: &[ScriptValue],
        imports: &mut dyn GuestImports,
        limits: WasmLimits,
    ) -> ScriptResult<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompiledLanguage {
    Cpp,
    Rust,
}

/// In-memory build input. An optional external toolchain adapter owns explicit,
/// confined file/process work; neither Play nor this contract spawns a compiler.
pub struct BuildSource {
    pub relative_path: String,
    pub text: String,
}
pub struct WasmBuildRequest {
    pub language: CompiledLanguage,
    pub sources: Vec<BuildSource>,
}

impl WasmBuildRequest {
    /// Explicit preparation step, never called by the frame loop. The caller
    /// supplies an adapter; this module never locates or installs a toolchain.
    pub fn build_with(
        &self,
        toolchain: &dyn WasmToolchain,
        limits: WasmLimits,
    ) -> ScriptResult<Vec<u8>> {
        self.validate()?;
        limits.validate()?;
        let bytes = toolchain.build(self, limits)?;
        validate_artifact(&bytes, limits)?;
        Ok(bytes)
    }

    pub fn validate(&self) -> ScriptResult<()> {
        if self.sources.is_empty() || self.sources.len() > 64 {
            return Err(ScriptError::InvalidArgument(
                "Build requires 1..64 source files".into(),
            ));
        }
        let mut names = std::collections::HashSet::new();
        let mut bytes = 0usize;
        for source in &self.sources {
            let path = &source.relative_path;
            bytes = bytes.saturating_add(source.text.len());
            if path.is_empty()
                || path.len() > 256
                || path.chars().any(|c| matches!(c, ':' | '\\' | '\0'))
                || path
                    .split('/')
                    .any(|p| p.is_empty() || p == "." || p == "..")
                || !names.insert(path.to_ascii_lowercase())
                || bytes > 4 * 1024 * 1024
            {
                return Err(ScriptError::InvalidArgument(
                    "Invalid, duplicate or oversized build input".into(),
                ));
            }
        }
        Ok(())
    }
}

/// No implementation, download, PATH probing, shell command or automatic build.
/// The adapter must bound process time/output and validate the returned artifact.
pub trait WasmToolchain {
    fn build(&self, request: &WasmBuildRequest, limits: WasmLimits) -> ScriptResult<Vec<u8>>;
}
