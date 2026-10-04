//! Sandboxed Rhai execution with persistent per-attachment scopes.
use super::{ExecutionResult, LoadedScript, ScriptTier};
use crate::host_api::ScriptContext;
use crate::node_handle::NodeHandle;
use crate::value::ScriptValue;
use crate::{ScriptError, ScriptResult};
use rhai::{Array, CallFnOptions, Dynamic, Engine, EvalAltResult, ImmutableString, Scope, INT};
use std::cell::{Cell, RefCell};
use std::panic::{catch_unwind, AssertUnwindSafe};

pub(super) type RhaiResult<T> = Result<T, Box<EvalAltResult>>;
thread_local! {
    static CTX_PTR: Cell<*mut ()> = const { Cell::new(std::ptr::null_mut()) };
    static LOGS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

struct ContextGuard;
impl ContextGuard {
    fn enter(ctx: &mut ScriptContext<'_>) -> ScriptResult<Self> {
        CTX_PTR.with(|cell| {
            if !cell.get().is_null() {
                return Err(ScriptError::InvalidArgument(
                    "reentrant script execution is forbidden".into(),
                ));
            }
            cell.set(ctx as *mut ScriptContext<'_> as *mut ());
            LOGS.with(|logs| logs.borrow_mut().clear());
            Ok(Self)
        })
    }
}
impl Drop for ContextGuard {
    fn drop(&mut self) {
        CTX_PTR.with(|cell| cell.set(std::ptr::null_mut()));
    }
}
pub(super) fn with_ctx<R>(
    f: impl FnOnce(&mut ScriptContext<'_>) -> ScriptResult<R>,
) -> RhaiResult<R> {
    CTX_PTR.with(|cell| {
        let ptr = cell.replace(std::ptr::null_mut());
        if ptr.is_null() {
            return Err("script context unavailable".into());
        }
        struct Restore<'a>(&'a Cell<*mut ()>, *mut ());
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                self.0.set(self.1);
            }
        }
        let _restore = Restore(cell, ptr);
        // SAFETY: ContextGuard scopes synchronous execution to the live context.
        // Clearing the cell during this borrow rejects nested mutable access.
        let ctx = unsafe { &mut *(ptr as *mut ScriptContext<'_>) };
        ctx.check_budget().and_then(|_| f(ctx)).map_err(as_rhai)
    })
}
fn as_rhai(error: ScriptError) -> Box<EvalAltResult> {
    error.to_string().into()
}
fn capture_log(message: &str) {
    LOGS.with(|logs| {
        let mut logs = logs.borrow_mut();
        if logs.len() < 128 {
            let mut end = message.len().min(1024);
            while !message.is_char_boundary(end) {
                end -= 1;
            }
            logs.push(message[..end].to_owned());
        }
    });
}
fn channel(value: INT) -> ScriptResult<u8> {
    u8::try_from(value)
        .map_err(|_| ScriptError::InvalidArgument("color channels require 0..255".into()))
}
fn check_audio_queue(ctx: &ScriptContext<'_>) -> ScriptResult<()> {
    if ctx.audio.commands.len() >= 256 {
        Err(ScriptError::InvalidArgument(
            "audio command limit exceeded".into(),
        ))
    } else {
        Ok(())
    }
}
fn to_dynamic(value: ScriptValue) -> Dynamic {
    match value {
        ScriptValue::None => Dynamic::UNIT,
        ScriptValue::Bool(v) => v.into(),
        ScriptValue::Int(v) => Dynamic::from_int(v),
        ScriptValue::Float(v) => Dynamic::from_float(v as f64),
        ScriptValue::String(v) => v.into(),
        ScriptValue::Vec3(v) => Dynamic::from_array(
            v.into_iter()
                .map(|v| Dynamic::from_float(v as f64))
                .collect(),
        ),
        value => Dynamic::from(value),
    }
}
fn from_dynamic(value: Dynamic) -> ScriptResult<ScriptValue> {
    if value.is_unit() {
        return Ok(ScriptValue::None);
    }
    if value.is::<bool>() {
        return Ok(ScriptValue::Bool(value.cast()));
    }
    if value.is::<INT>() {
        return Ok(ScriptValue::Int(value.cast()));
    }
    if value.is::<f64>() {
        let v = value.cast::<f64>() as f32;
        if v.is_finite() {
            return Ok(ScriptValue::Float(v));
        }
    } else if value.is::<ImmutableString>() {
        return Ok(ScriptValue::String(
            value.cast::<ImmutableString>().to_string(),
        ));
    } else if value.is::<ScriptValue>() {
        return Ok(value.cast());
    }
    Err(ScriptError::InvalidArgument(
        "unsupported property value".into(),
    ))
}

pub fn create_engine(max_operations: u64) -> Engine {
    let mut engine = Engine::new();
    engine.set_max_operations(max_operations.max(1));
    engine.set_max_call_levels(32);
    engine.set_max_variables(128);
    engine.set_max_functions(128);
    engine.set_max_string_size(4096);
    engine.set_max_array_size(2048);
    engine.set_max_map_size(256);
    engine.set_max_expr_depths(32, 32);
    // Project scripts cannot import arbitrary host files or recursively evaluate code.
    engine.set_module_resolver(rhai::module_resolvers::StaticModuleResolver::new());
    engine.disable_symbol("eval");
    engine.on_progress(|_| {
        with_ctx(|ctx| ctx.check_budget())
            .err()
            .map(|e| Dynamic::from(e.to_string()))
    });
    engine.on_print(capture_log);
    engine.on_debug(|message, _, _| capture_log(message));
    engine.register_type_with_name::<NodeHandle>("Handle");
    engine.register_type_with_name::<ScriptValue>("Value");
    engine.register_fn("vec3", |x: f64, y: f64, z: f64| {
        ScriptValue::vec3(x as f32, y as f32, z as f32)
    });
    engine.register_fn(
        "color",
        |r: INT, g: INT, b: INT| -> RhaiResult<ScriptValue> {
            Ok(ScriptValue::color(
                channel(r).map_err(as_rhai)?,
                channel(g).map_err(as_rhai)?,
                channel(b).map_err(as_rhai)?,
                255,
            ))
        },
    );
    engine.register_fn(
        "color_rgba",
        |r: INT, g: INT, b: INT, a: INT| -> RhaiResult<ScriptValue> {
            Ok(ScriptValue::color(
                channel(r).map_err(as_rhai)?,
                channel(g).map_err(as_rhai)?,
                channel(b).map_err(as_rhai)?,
                channel(a).map_err(as_rhai)?,
            ))
        },
    );
    engine.register_fn("self_node", || -> RhaiResult<NodeHandle> {
        with_ctx(|ctx| Ok(ctx.self_node().unwrap_or_else(NodeHandle::invalid)))
    });
    engine.register_fn(
        "emit_event",
        |name: ImmutableString, value: Dynamic| -> RhaiResult<()> {
            with_ctx(|ctx| ctx.emit_event(None, &name, from_dynamic(value)?))
        },
    );
    engine.register_fn(
        "send_event",
        |target: NodeHandle, name: ImmutableString, value: Dynamic| -> RhaiResult<()> {
            with_ctx(|ctx| ctx.emit_event(Some(target), &name, from_dynamic(value)?))
        },
    );
    engine.register_fn(
        "is_action_pressed",
        |name: ImmutableString| -> RhaiResult<bool> {
            with_ctx(|ctx| Ok(ctx.is_key_pressed(&format!("action:{name}"))))
        },
    );
    engine.register_fn(
        "was_action_just_pressed",
        |name: ImmutableString| -> RhaiResult<bool> {
            with_ctx(|ctx| Ok(ctx.was_key_just_pressed(&format!("action:{name}"))))
        },
    );
    engine.register_fn(
        "get_node",
        |name: ImmutableString| -> RhaiResult<NodeHandle> {
            with_ctx(|ctx| Ok(ctx.get_node(&name).unwrap_or_else(NodeHandle::invalid)))
        },
    );
    engine.register_fn("is_valid", |handle: NodeHandle| -> RhaiResult<bool> {
        with_ctx(|ctx| Ok(handle.is_valid(ctx)))
    });
    engine.register_fn(
        "find_child",
        |handle: NodeHandle, name: ImmutableString| -> RhaiResult<NodeHandle> {
            with_ctx(|ctx| {
                Ok(ctx
                    .find_child(handle, &name)?
                    .unwrap_or_else(NodeHandle::invalid))
            })
        },
    );
    engine.register_fn(
        "get_parent",
        |handle: NodeHandle| -> RhaiResult<NodeHandle> {
            with_ctx(|ctx| Ok(ctx.get_parent(handle)?.unwrap_or_else(NodeHandle::invalid)))
        },
    );
    engine.register_fn(
        "spawn_entity",
        |name: ImmutableString, primitive: ImmutableString| -> RhaiResult<NodeHandle> {
            with_ctx(|ctx| ctx.spawn_entity(&name, &primitive))
        },
    );
    engine.register_fn("destroy_entity", |handle: NodeHandle| -> RhaiResult<()> {
        with_ctx(|ctx| ctx.destroy_entity(handle))
    });
    // Mutable first arguments support method syntax without cloning entity state.
    engine.register_fn(
        "set_position",
        |handle: &mut NodeHandle, x: f64, y: f64, z: f64| -> RhaiResult<()> {
            with_ctx(|ctx| handle.set_position(ctx, x as f32, y as f32, z as f32))
        },
    );
    engine.register_fn(
        "set_rotation",
        |handle: &mut NodeHandle, x: f64, y: f64, z: f64| -> RhaiResult<()> {
            with_ctx(|ctx| handle.set_rotation(ctx, x as f32, y as f32, z as f32))
        },
    );
    engine.register_fn(
        "set_scale",
        |handle: &mut NodeHandle, x: f64, y: f64, z: f64| -> RhaiResult<()> {
            with_ctx(|ctx| handle.set_scale(ctx, x as f32, y as f32, z as f32))
        },
    );
    engine.register_fn(
        "move_by",
        |handle: &mut NodeHandle, x: f64, y: f64, z: f64| -> RhaiResult<()> {
            with_ctx(|ctx| handle.move_by(ctx, x as f32, y as f32, z as f32))
        },
    );
    engine.register_fn(
        "rotate_by",
        |handle: &mut NodeHandle, x: f64, y: f64, z: f64| -> RhaiResult<()> {
            with_ctx(|ctx| handle.rotate_by(ctx, x as f32, y as f32, z as f32))
        },
    );
    engine.register_fn(
        "get_position",
        |handle: &mut NodeHandle| -> RhaiResult<Array> {
            with_ctx(|ctx| {
                Ok(handle
                    .get_position(ctx)?
                    .into_iter()
                    .map(|v| Dynamic::from_float(v as f64))
                    .collect())
            })
        },
    );
    engine.register_fn(
        "get_rotation",
        |handle: &mut NodeHandle| -> RhaiResult<Array> {
            with_ctx(|ctx| {
                Ok(handle
                    .get_rotation(ctx)?
                    .into_iter()
                    .map(|v| Dynamic::from_float(v as f64))
                    .collect())
            })
        },
    );
    engine.register_fn(
        "get_scale",
        |handle: &mut NodeHandle| -> RhaiResult<Array> {
            with_ctx(|ctx| {
                Ok(handle
                    .get_scale(ctx)?
                    .into_iter()
                    .map(|v| Dynamic::from_float(v as f64))
                    .collect())
            })
        },
    );
    engine.register_fn(
        "set_color",
        |handle: &mut NodeHandle, r: INT, g: INT, b: INT, a: INT| -> RhaiResult<()> {
            with_ctx(|ctx| {
                handle.set_color(ctx, channel(r)?, channel(g)?, channel(b)?, channel(a)?)
            })
        },
    );
    engine.register_fn(
        "set_color_rgb",
        |handle: &mut NodeHandle, r: INT, g: INT, b: INT| -> RhaiResult<()> {
            with_ctx(|ctx| handle.set_color(ctx, channel(r)?, channel(g)?, channel(b)?, 255))
        },
    );
    engine.register_fn(
        "set_visible",
        |handle: &mut NodeHandle, visible: bool| -> RhaiResult<()> {
            with_ctx(|ctx| handle.set_visible(ctx, visible))
        },
    );
    engine.register_fn(
        "set_name",
        |handle: &mut NodeHandle, name: ImmutableString| -> RhaiResult<()> {
            with_ctx(|ctx| handle.set_name(ctx, &name))
        },
    );
    engine.register_fn(
        "get_property",
        |handle: &mut NodeHandle, name: ImmutableString| -> RhaiResult<Dynamic> {
            with_ctx(|ctx| handle.get_property(ctx, &name).map(to_dynamic))
        },
    );
    engine.register_fn(
        "set_property",
        |handle: &mut NodeHandle, name: ImmutableString, value: Dynamic| -> RhaiResult<()> {
            with_ctx(|ctx| handle.set_property(ctx, &name, from_dynamic(value)?))
        },
    );
    engine.register_fn(
        "is_key_pressed",
        |key: ImmutableString| -> RhaiResult<bool> { with_ctx(|ctx| Ok(ctx.is_key_pressed(&key))) },
    );
    engine.register_fn(
        "was_key_just_pressed",
        |key: ImmutableString| -> RhaiResult<bool> {
            with_ctx(|ctx| Ok(ctx.was_key_just_pressed(&key)))
        },
    );
    engine.register_fn("is_mouse_pressed", |button: INT| -> RhaiResult<bool> {
        with_ctx(|ctx| Ok(ctx.is_mouse_pressed(button as i32)))
    });
    engine.register_fn("play_audio", |name: ImmutableString| -> RhaiResult<()> {
        with_ctx(|ctx| {
            check_audio_queue(ctx)?;
            ctx.play_audio(&name);
            Ok(())
        })
    });
    engine.register_fn("stop_audio", |name: ImmutableString| -> RhaiResult<()> {
        with_ctx(|ctx| {
            check_audio_queue(ctx)?;
            ctx.stop_audio(&name);
            Ok(())
        })
    });
    engine.register_fn(
        "set_volume",
        |name: ImmutableString, volume: f64| -> RhaiResult<()> {
            with_ctx(|ctx| {
                check_audio_queue(ctx)?;
                if !volume.is_finite() {
                    return Err(ScriptError::InvalidArgument("volume must be finite".into()));
                }
                ctx.set_volume(&name, volume as f32);
                Ok(())
            })
        },
    );
    engine.register_fn("get_delta_time", || -> RhaiResult<f64> {
        with_ctx(|ctx| Ok(ctx.get_delta_time() as f64))
    });
    engine.register_fn("get_elapsed_time", || -> RhaiResult<f64> {
        with_ctx(|ctx| Ok(ctx.get_elapsed_time() as f64))
    });
    super::rhai_scene_api::register(&mut engine);
    engine
}

#[derive(Clone)]
pub struct CompiledRhai {
    pub(crate) ast: rhai::AST,
    pub path: String,
    pub has_on_start: bool,
    pub has_on_update: bool,
    pub has_on_fixed_update: bool,
    pub has_on_destroy: bool,
    pub has_on_event: bool,
    pub has_on_late_update: bool,
}
pub fn compile_source(engine: &Engine, path: &str, source: &str) -> ScriptResult<CompiledRhai> {
    let mut ast = engine
        .compile(source)
        .map_err(|e| ScriptError::RhaiCompile(e.to_string()))?;
    ast.set_source(path);
    let mut hooks = [false; 6];
    for function in ast.iter_functions() {
        let hook = match function.name {
            "on_start" => Some((0, 0)),
            "on_update" => Some((1, 1)),
            "on_fixed_update" => Some((2, 1)),
            "on_destroy" => Some((3, 0)),
            "on_event" => Some((4, 2)),
            "on_late_update" => Some((5, 1)),
            _ => None,
        };
        if let Some((index, arity)) = hook {
            if function.params.len() != arity || function.access != rhai::FnAccess::Public {
                return Err(ScriptError::RhaiCompile(format!(
                    "{path}: {} requires {arity} parameters and public access",
                    function.name
                )));
            }
            hooks[index] = true;
        }
    }
    Ok(CompiledRhai {
        ast,
        path: path.into(),
        has_on_start: hooks[0],
        has_on_update: hooks[1],
        has_on_fixed_update: hooks[2],
        has_on_destroy: hooks[3],
        has_on_event: hooks[4],
        has_on_late_update: hooks[5],
    })
}
pub fn call_event(
    engine: &Engine,
    script: &CompiledRhai,
    ctx: &mut ScriptContext<'_>,
    scope: &mut Scope<'static>,
    name: &str,
    value: ScriptValue,
) -> ExecutionResult {
    if !script.has_on_event {
        return ExecutionResult::ok();
    }
    evaluate(ctx, || {
        engine
            .call_fn_with_options::<Dynamic>(
                CallFnOptions::new().eval_ast(false),
                scope,
                &script.ast,
                "on_event",
                (name.to_owned(), to_dynamic(value)),
            )
            .map(|_| ())
    })
}
pub fn analyze_entry_points(source: &str) -> (bool, bool, bool) {
    compile_source(&create_engine(100_000), "validation.rhai", source)
        .map(|s| (s.has_on_start, s.has_on_update, s.has_on_destroy))
        .unwrap_or_default()
}
pub fn load_metadata(path: &str, source: &str) -> LoadedScript {
    let (has_on_start, has_on_update, has_on_destroy) = analyze_entry_points(source);
    LoadedScript {
        tier: ScriptTier::Rhai,
        path: path.into(),
        has_on_start,
        has_on_update,
        has_on_destroy,
    }
}

fn evaluate(ctx: &mut ScriptContext<'_>, run: impl FnOnce() -> RhaiResult<()>) -> ExecutionResult {
    let _guard = match ContextGuard::enter(ctx) {
        Ok(guard) => guard,
        Err(error) => return ExecutionResult::from_error(&error),
    };
    let mut result = match catch_unwind(AssertUnwindSafe(run)) {
        Ok(Ok(())) => ExecutionResult::ok(),
        Ok(Err(error)) => ExecutionResult::error(error.to_string()),
        Err(_) => ExecutionResult::error("script host panicked; attachment disabled"),
    };
    result.logs = LOGS.with(|logs| std::mem::take(&mut *logs.borrow_mut()));
    result
}
pub fn initialize_scope(
    engine: &Engine,
    script: &CompiledRhai,
    ctx: &mut ScriptContext<'_>,
    scope: &mut Scope<'static>,
) -> ExecutionResult {
    evaluate(ctx, || {
        engine
            .eval_ast_with_scope::<Dynamic>(scope, &script.ast)
            .map(|_| ())
    })
}
pub fn call_hook(
    engine: &Engine,
    script: &CompiledRhai,
    ctx: &mut ScriptContext<'_>,
    scope: &mut Scope<'static>,
    hook: &str,
    dt: f32,
) -> ExecutionResult {
    let enabled = match hook {
        "on_start" => script.has_on_start,
        "on_update" => script.has_on_update,
        "on_fixed_update" => script.has_on_fixed_update,
        "on_late_update" => script.has_on_late_update,
        "on_destroy" => script.has_on_destroy,
        _ => false,
    };
    if !enabled {
        return ExecutionResult::ok();
    }
    evaluate(ctx, || {
        let options = CallFnOptions::new().eval_ast(false);
        if hook == "on_update" || hook == "on_fixed_update" || hook == "on_late_update" {
            engine
                .call_fn_with_options::<Dynamic>(options, scope, &script.ast, hook, (dt as f64,))
                .map(|_| ())
        } else {
            engine
                .call_fn_with_options::<Dynamic>(options, scope, &script.ast, hook, ())
                .map(|_| ())
        }
    })
}

// Compatibility helpers for one-shot callers; the runtime owns persistent scopes.
fn one_shot(
    engine: &Engine,
    script: &CompiledRhai,
    ctx: &mut ScriptContext<'_>,
    hook: &str,
    dt: f32,
) -> ExecutionResult {
    let mut scope = Scope::new();
    let mut initialized = initialize_scope(engine, script, ctx, &mut scope);
    if initialized.success {
        let called = call_hook(engine, script, ctx, &mut scope, hook, dt);
        initialized.logs.extend(called.logs);
        initialized.errors.extend(called.errors);
        initialized.success = called.success;
    }
    initialized
}
pub fn call_on_start(
    engine: &Engine,
    script: &CompiledRhai,
    ctx: &mut ScriptContext<'_>,
) -> ExecutionResult {
    one_shot(engine, script, ctx, "on_start", 0.0)
}
pub fn call_on_update(
    engine: &Engine,
    script: &CompiledRhai,
    ctx: &mut ScriptContext<'_>,
    dt: f32,
) -> ExecutionResult {
    one_shot(engine, script, ctx, "on_update", dt)
}
pub fn call_on_destroy(
    engine: &Engine,
    script: &CompiledRhai,
    ctx: &mut ScriptContext<'_>,
) -> ExecutionResult {
    one_shot(engine, script, ctx, "on_destroy", 0.0)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hooks_are_validated_from_the_ast() {
        let engine = create_engine(100_000);
        let script = compile_source(
            &engine,
            "test.rhai",
            "// fn on_start() {}\n fn on_update ( dt ) {}",
        )
        .unwrap();
        assert!(!script.has_on_start);
        assert!(script.has_on_update);
        assert!(compile_source(&engine, "bad.rhai", "fn on_start(dt) {}").is_err());
        assert!(compile_source(&engine, "bad.rhai", "fn on_update(").is_err());
    }
}
