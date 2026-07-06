//! Script interop operations: call functions across scripts.

use crate::host_api::ScriptContext;
use crate::value::ScriptValue;
use crate::ScriptResult;

/// Call a function defined in another script.
/// Tier 1 only (Rhai-to-Rhai) for now. WASM interop arrives in Phase D.
pub fn call_script_function(
    ctx: &mut ScriptContext<'_>,
    script_path: &str,
    function: &str,
    args: Vec<ScriptValue>,
) -> ScriptResult<ScriptValue> {
    ctx.call_script_function(script_path, function, args)
}
