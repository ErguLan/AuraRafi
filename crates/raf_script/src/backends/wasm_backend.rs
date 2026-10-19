//! Optional WASM adapter boundary, disabled by default. No executor dependency.
use super::wasm_contract::{
    validate_artifact, WasmCompiledModule, WasmExecutor, WasmHook, WasmInstance, WasmLimits,
};
use super::wasm_host::{AttachmentIdentity, GuestHandles, HostBridge};
use super::{ExecutionResult, LoadedScript, ScriptTier};
use crate::{ScriptContext, ScriptError, ScriptResult, ScriptValue};
use std::sync::Arc;

pub use super::wasm_contract::HOST_ABI_VERSION;

#[derive(Default)]
pub struct WasmBackend {
    executor: Option<Arc<dyn WasmExecutor>>,
}

impl WasmBackend {
    /// Engine integrators explicitly supply a trusted adapter. Installing a
    /// compiler alone never enables arbitrary native execution or Play support.
    pub fn with_executor(executor: Arc<dyn WasmExecutor>) -> Self {
        Self {
            executor: Some(executor),
        }
    }
    pub fn is_available(&self) -> bool {
        self.executor.is_some()
    }

    pub fn compile(&self, bytes: &[u8], limits: WasmLimits) -> ScriptResult<CompiledWasm> {
        let executor = self
            .executor
            .as_ref()
            .ok_or(ScriptError::WasmNotImplemented)?;
        validate_artifact(bytes, limits)?;
        // The header check is NOT a validator or sandbox. The provider validates
        // all sections, signatures, imports and resource limits before returning.
        let module = executor.compile(bytes, limits)?;
        let exports = module.exports();
        if exports.abi_version != HOST_ABI_VERSION {
            return Err(ScriptError::VersionMismatch {
                expected: HOST_ABI_VERSION,
                found: exports.abi_version,
            });
        }
        if exports.hooks.len() > 6
            || exports
                .hooks
                .iter()
                .enumerate()
                .any(|(i, hook)| exports.hooks[..i].contains(hook))
        {
            return Err(ScriptError::WasmCompile(
                "Invalid or duplicate lifecycle exports".into(),
            ));
        }
        Ok(CompiledWasm { module, limits })
    }
}

/// Share this immutable compilation; never share an attachment's mutable instance.
pub struct CompiledWasm {
    module: Arc<dyn WasmCompiledModule>,
    limits: WasmLimits,
}

impl CompiledWasm {
    pub fn instantiate(&self, ctx: &ScriptContext<'_>) -> ScriptResult<WasmModuleHandle> {
        ctx.check_budget()?;
        let identity = AttachmentIdentity::from_context(ctx)?;
        let instance = self.module.instantiate(self.limits)?;
        ctx.check_budget()?;
        Ok(WasmModuleHandle {
            instance,
            identity,
            handles: GuestHandles::default(),
            hooks: self.module.exports().hooks.clone(),
            limits: self.limits,
            started: false,
            stopped: false,
            failed: false,
        })
    }
}

pub struct WasmModuleHandle {
    instance: Box<dyn WasmInstance>,
    identity: AttachmentIdentity,
    handles: GuestHandles,
    hooks: Vec<WasmHook>,
    limits: WasmLimits,
    started: bool,
    stopped: bool,
    failed: bool,
}

impl WasmModuleHandle {
    /// Legacy entry point remains explicitly unavailable: no implicit provider,
    /// filesystem probing, compiler launch or DLL fallback is allowed.
    pub fn load(_path: &str) -> ScriptResult<Self> {
        Err(ScriptError::WasmNotImplemented)
    }

    pub fn invoke(
        &mut self,
        hook: WasmHook,
        args: &[ScriptValue],
        ctx: &mut ScriptContext<'_>,
    ) -> ScriptResult<()> {
        if self.stopped || self.failed {
            return Err(ScriptError::WasmTrap(
                "WASM attachment is stopped or failed".into(),
            ));
        }
        ctx.check_budget()?;
        if self.identity != AttachmentIdentity::from_context(ctx)? {
            return Err(ScriptError::WasmTrap(
                "WASM attachment belongs to another runtime, owner or source".into(),
            ));
        }
        hook.validate_args(args)?;
        if hook == WasmHook::Start {
            if self.started {
                return Err(ScriptError::WasmTrap(
                    "WASM attachment was already started".into(),
                ));
            }
            self.started = true;
        } else if !self.started {
            return Err(ScriptError::WasmTrap(
                "Start the WASM attachment before dispatching hooks".into(),
            ));
        }
        if hook == WasmHook::Destroy {
            self.stopped = true;
        }
        if !self.hooks.contains(&hook) {
            return Ok(());
        }
        let mut imports = HostBridge {
            ctx,
            handles: &mut self.handles,
            max_handles: self.limits.entity_handles,
        };
        let result = self
            .instance
            .invoke(hook, args, &mut imports, self.limits)
            .and_then(|()| imports.ctx.check_budget());
        if result.is_err() {
            self.failed = true;
        }
        result
    }
}

/// Without a provider, filenames/bytes are not evidence that exports exist.
pub fn load_metadata(path: &str, _bytes: &[u8]) -> LoadedScript {
    LoadedScript {
        tier: ScriptTier::Wasm,
        path: path.into(),
        has_on_start: false,
        has_on_update: false,
        has_on_destroy: false,
    }
}
pub fn compile_module(_path: &str, bytes: &[u8]) -> ScriptResult<()> {
    WasmBackend::default()
        .compile(bytes, WasmLimits::default())
        .map(|_| ())
}
fn result(value: ScriptResult<()>) -> ExecutionResult {
    match value {
        Ok(()) => ExecutionResult::ok(),
        Err(error) => ExecutionResult::from_error(&error),
    }
}
pub fn call_on_start(
    module: &mut WasmModuleHandle,
    ctx: &mut ScriptContext<'_>,
) -> ExecutionResult {
    result(module.invoke(WasmHook::Start, &[], ctx))
}
pub fn call_on_update(
    module: &mut WasmModuleHandle,
    ctx: &mut ScriptContext<'_>,
    dt: f32,
) -> ExecutionResult {
    result(module.invoke(WasmHook::Update, &[ScriptValue::Float(dt)], ctx))
}
pub fn call_on_destroy(
    module: &mut WasmModuleHandle,
    ctx: &mut ScriptContext<'_>,
) -> ExecutionResult {
    result(module.invoke(WasmHook::Destroy, &[], ctx))
}
