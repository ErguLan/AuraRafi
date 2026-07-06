//! Rhai scripting backend (Tier 1).
//!
//! Creates a `rhai::Engine`, registers all Host API functions, and provides
//! helpers to compile and call `on_start` / `on_update` / `on_destroy`.
//!
//! The Host API functions look up the live `ScriptContext` through a
//! thread-local pointer that is set before each call. This is safe because
//! Rhai execution is synchronous and single-threaded: the context outlives
//! every call because it lives on the caller's stack frame.

use std::cell::Cell;

use rhai::{Dynamic, Engine, ImmutableString, INT};

use crate::errors::ScriptError;
use crate::host_api::ScriptContext;
use crate::node_handle::NodeHandle;
use crate::value::ScriptValue;
use crate::ScriptResult;

use super::{ExecutionResult, LoadedScript, ScriptTier};

// ---------------------------------------------------------------------------
// Thread-local context pointer
// ---------------------------------------------------------------------------

thread_local! {
    /// Raw pointer to the current `ScriptContext`. Set before each script
    /// call, cleared after. The pointer is erased to `*mut ()` to avoid
    /// lifetime issues; it is cast back inside `with_ctx`.
    static CTX_PTR: Cell<*mut ()> = Cell::new(std::ptr::null_mut());
}

/// Set the current context pointer (called before script execution).
fn set_context(ctx: *mut ScriptContext<'_>) {
    CTX_PTR.with(|cell| cell.set(ctx as *mut ()));
}

/// Clear the context pointer (called after script execution).
fn clear_context() {
    CTX_PTR.with(|cell| cell.set(std::ptr::null_mut()));
}

/// Access the current context. Panics if not set (programming error).
fn with_ctx<F, R>(f: F) -> R
where
    F: FnOnce(&mut ScriptContext<'_>) -> R,
{
    CTX_PTR.with(|cell| {
        let ptr = cell.get();
        assert!(!ptr.is_null(), "ScriptContext not set before script call");
        // SAFETY: the pointer was set by `set_context` from a live &mut
        // ScriptContext on the caller's stack. It is valid for the duration
        // of this call because Rhai execution is synchronous.
        let ctx = unsafe { &mut *(ptr as *mut ScriptContext<'_>) };
        f(ctx)
    })
}

// ---------------------------------------------------------------------------
// Entry point analysis
// ---------------------------------------------------------------------------

/// Analyze a Rhai source file for entry points by text search.
/// Returns (has_on_start, has_on_update, has_on_destroy).
pub fn analyze_entry_points(source: &str) -> (bool, bool, bool) {
    let lower = source.to_lowercase();
    let has_on_start = lower.contains("fn on_start(") || lower.contains("fn on_start()");
    let has_on_update = lower.contains("fn on_update(") || lower.contains("fn on_update()");
    let has_on_destroy = lower.contains("fn on_destroy(") || lower.contains("fn on_destroy()");
    (has_on_start, has_on_update, has_on_destroy)
}

// ---------------------------------------------------------------------------
// Engine creation and Host API registration
// ---------------------------------------------------------------------------

/// Create a Rhai engine with the Host API registered.
///
/// The engine is ready to compile and run scripts. The `ScriptContext`
/// is provided at call time through a thread-local, not at registration
/// time, because the context is rebuilt every frame.
pub fn create_engine(timeout_ops: u64) -> Engine {
    let mut engine = Engine::new();

    // Safety limit: prevent infinite loops from hanging the engine.
    engine.set_max_operations(timeout_ops);

    // --- Type registrations ---
    engine.register_type::<NodeHandle>();
    engine.register_type_with_name::<NodeHandle>("Handle");
    engine.register_type::<ScriptValue>();
    engine.register_type_with_name::<ScriptValue>("Value");

    // --- Constants and helpers ---

    // vec3(x, y, z) -> ScriptValue::Vec3
    engine.register_fn("vec3", |x: f32, y: f32, z: f32| ScriptValue::vec3(x, y, z));

    // color(r, g, b) -> ScriptValue::Color (alpha defaults to 255)
    engine.register_fn("color", |r: INT, g: INT, b: INT| {
        ScriptValue::color(r as u8, g as u8, b as u8, 255)
    });

    // color_rgba(r, g, b, a) -> ScriptValue::Color
    engine.register_fn("color_rgba", |r: INT, g: INT, b: INT, a: INT| {
        ScriptValue::color(r as u8, g as u8, b as u8, a as u8)
    });

    // --- Scene operations ---
    engine.register_fn("get_node", |name: ImmutableString| -> NodeHandle {
        with_ctx(|ctx| ctx.get_node(&name).unwrap_or(NodeHandle::from_raw(0)))
    });

    engine.register_fn("spawn_entity", |name: ImmutableString, prim: ImmutableString| -> NodeHandle {
        with_ctx(|ctx| ctx.spawn_entity(&name, &prim).unwrap_or(NodeHandle::from_raw(0)))
    });

    engine.register_fn("destroy_entity", |handle: NodeHandle| {
        with_ctx(|ctx| {
            let _ = ctx.destroy_entity(handle);
        });
    });

    // --- Transform operations (all in meters, radians) ---
    engine.register_fn("set_position", |handle: NodeHandle, x: f32, y: f32, z: f32| {
        with_ctx(|ctx| {
            let _ = handle.set_position(ctx, x, y, z);
        });
    });

    engine.register_fn("set_rotation", |handle: NodeHandle, x: f32, y: f32, z: f32| {
        with_ctx(|ctx| {
            let _ = handle.set_rotation(ctx, x, y, z);
        });
    });

    engine.register_fn("set_scale", |handle: NodeHandle, x: f32, y: f32, z: f32| {
        with_ctx(|ctx| {
            let _ = handle.set_scale(ctx, x, y, z);
        });
    });

    engine.register_fn("get_position", |handle: NodeHandle| -> [f32; 3] {
        with_ctx(|ctx| handle.get_position(ctx).unwrap_or([0.0, 0.0, 0.0]))
    });

    engine.register_fn("move_by", |handle: NodeHandle, dx: f32, dy: f32, dz: f32| {
        with_ctx(|ctx| {
            let _ = handle.move_by(ctx, dx, dy, dz);
        });
    });

    engine.register_fn("rotate_by", |handle: NodeHandle, dx: f32, dy: f32, dz: f32| {
        with_ctx(|ctx| {
            let _ = handle.rotate_by(ctx, dx, dy, dz);
        });
    });

    // --- Property operations ---
    engine.register_fn("set_color", |handle: NodeHandle, r: INT, g: INT, b: INT, a: INT| {
        with_ctx(|ctx| {
            let _ = handle.set_color(ctx, r as u8, g as u8, b as u8, a as u8);
        });
    });

    engine.register_fn("set_color_rgb", |handle: NodeHandle, r: INT, g: INT, b: INT| {
        with_ctx(|ctx| {
            let _ = handle.set_color(ctx, r as u8, g as u8, b as u8, 255);
        });
    });

    engine.register_fn("set_visible", |handle: NodeHandle, visible: bool| {
        with_ctx(|ctx| {
            let _ = handle.set_visible(ctx, visible);
        });
    });

    engine.register_fn("set_name", |handle: NodeHandle, name: ImmutableString| {
        with_ctx(|ctx| {
            let _ = handle.set_name(ctx, &name);
        });
    });

    // --- Input operations ---
    engine.register_fn("is_key_pressed", |key: ImmutableString| -> bool {
        with_ctx(|ctx| ctx.is_key_pressed(&key))
    });

    engine.register_fn("was_key_just_pressed", |key: ImmutableString| -> bool {
        with_ctx(|ctx| ctx.was_key_just_pressed(&key))
    });

    engine.register_fn("is_mouse_pressed", |button: INT| -> bool {
        with_ctx(|ctx| ctx.is_mouse_pressed(button as i32))
    });

    // --- Audio operations ---
    engine.register_fn("play_audio", |name: ImmutableString| {
        with_ctx(|ctx| ctx.play_audio(&name));
    });

    engine.register_fn("stop_audio", |name: ImmutableString| {
        with_ctx(|ctx| ctx.stop_audio(&name));
    });

    engine.register_fn("set_volume", |name: ImmutableString, volume: f32| {
        with_ctx(|ctx| ctx.set_volume(&name, volume));
    });

    // --- Time operations ---
    engine.register_fn("get_delta_time", || -> f32 {
        with_ctx(|ctx| ctx.get_delta_time())
    });

    engine.register_fn("get_elapsed_time", || -> f32 {
        with_ctx(|ctx| ctx.get_elapsed_time())
    });

    engine
}

// ---------------------------------------------------------------------------
// Compilation and execution
// ---------------------------------------------------------------------------

/// A compiled Rhai script ready for execution.
pub struct CompiledRhai {
    ast: rhai::AST,
    pub path: String,
    pub has_on_start: bool,
    pub has_on_update: bool,
    pub has_on_destroy: bool,
}

/// Compile a Rhai script from source.
pub fn compile_source(engine: &Engine, path: &str, source: &str) -> ScriptResult<CompiledRhai> {
    let ast = engine
        .compile(source)
        .map_err(|e| ScriptError::RhaiCompile(e.to_string()))?;

    let (has_on_start, has_on_update, has_on_destroy) = analyze_entry_points(source);

    Ok(CompiledRhai {
        ast,
        path: path.to_string(),
        has_on_start,
        has_on_update,
        has_on_destroy,
    })
}

/// Load script metadata from a path and source without compiling.
pub fn load_metadata(path: &str, source: &str) -> LoadedScript {
    let (has_on_start, has_on_update, has_on_destroy) = analyze_entry_points(source);
    LoadedScript {
        tier: ScriptTier::Rhai,
        path: path.to_string(),
        has_on_start,
        has_on_update,
        has_on_destroy,
    }
}

/// Call the `on_start` function of a compiled script.
pub fn call_on_start(
    engine: &Engine,
    script: &CompiledRhai,
    ctx: &mut ScriptContext<'_>,
) -> ExecutionResult {
    if !script.has_on_start {
        return ExecutionResult::ok();
    }
    call_fn_with_ctx(engine, &script.ast, "on_start", ctx, vec![])
}

/// Call the `on_update(dt)` function of a compiled script.
pub fn call_on_update(
    engine: &Engine,
    script: &CompiledRhai,
    ctx: &mut ScriptContext<'_>,
    dt: f32,
) -> ExecutionResult {
    if !script.has_on_update {
        return ExecutionResult::ok();
    }
    call_fn_with_ctx(engine, &script.ast, "on_update", ctx, vec![Dynamic::from_float(dt as f64)])
}

/// Call the `on_destroy` function of a compiled script.
pub fn call_on_destroy(
    engine: &Engine,
    script: &CompiledRhai,
    ctx: &mut ScriptContext<'_>,
) -> ExecutionResult {
    if !script.has_on_destroy {
        return ExecutionResult::ok();
    }
    call_fn_with_ctx(engine, &script.ast, "on_destroy", ctx, vec![])
}

/// Call a named function on a compiled script with context access.
fn call_fn_with_ctx(
    engine: &Engine,
    ast: &rhai::AST,
    fn_name: &str,
    ctx: &mut ScriptContext<'_>,
    args: Vec<Dynamic>,
) -> ExecutionResult {
    set_context(ctx);
    let result = run_fn_safe(engine, ast, fn_name, args);
    clear_context();
    result
}

/// Run a function, catching Rhai errors and mapping to ExecutionResult.
fn run_fn_safe(
    engine: &Engine,
    ast: &rhai::AST,
    fn_name: &str,
    args: Vec<Dynamic>,
) -> ExecutionResult {
    let mut scope = rhai::Scope::new();
    match engine.call_fn::<Dynamic>(&mut scope, ast, fn_name, args) {
        Ok(_) => ExecutionResult::ok(),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("not found") || msg.contains("FnNotFound") {
                ExecutionResult::ok()
            } else {
                ExecutionResult::error(msg)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyze_simple_script() {
        let src = "fn on_start() { print(\"hello\"); } fn on_update(dt) { }";
        let (s, u, d) = analyze_entry_points(src);
        assert!(s);
        assert!(u);
        assert!(!d);
    }

    #[test]
    fn compile_simple() {
        let engine = create_engine(100_000);
        let src = "fn on_start() { let x = 1 + 2; }";
        let compiled = compile_source(&engine, "test.rhai", src);
        assert!(compiled.is_ok());
    }
}
