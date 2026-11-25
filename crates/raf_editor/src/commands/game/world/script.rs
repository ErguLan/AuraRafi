//! Sandboxed Rhai authoring scripts for world plans.

use super::super::query::{parsed_command_from_params, text_argument};
use super::compile::compile_world_program;
use super::plan::{append_journal, sha256_hex};
use super::*;

// ---------------------------------------------------------------------------
// Sandboxed authoring script
// ---------------------------------------------------------------------------

#[derive(Default)]
struct ScriptWorld {
    groups: Vec<Value>,
    operations: Vec<Value>,
    parent: Option<String>,
}

impl ScriptWorld {
    fn push(&mut self, mut operation: Value) {
        if let Some(parent) = self.parent.as_deref() {
            if operation.get("parent").is_none() {
                operation["parent"] = Value::String(parent.to_string());
            }
        }
        self.operations.push(operation);
    }

    fn program(&self) -> Value {
        serde_json::json!({
            "version": 1,
            "groups": self.groups,
            "operations": self.operations,
        })
    }
}

fn script_error(message: impl Into<String>) -> Box<rhai::EvalAltResult> {
    message.into().into()
}

fn world_script_program(source: &str, scene: &SceneGraph) -> Result<(Value, usize), String> {
    if source.trim().is_empty() {
        return Err("world.script requires a non-empty Rhai script.".to_string());
    }
    if source.len() > 64 * 1024 {
        return Err("Authoring scripts are limited to 64 KiB.".to_string());
    }
    let mut engine = rhai::Engine::new_raw();
    engine.set_max_operations(200_000);
    engine.set_max_call_levels(32);
    engine.set_max_variables(256);
    engine.set_max_functions(128);
    engine.set_max_string_size(64 * 1024);
    engine.set_max_array_size(8192);
    engine.set_max_map_size(1024);
    engine.set_max_expr_depths(32, 32);
    engine.disable_symbol("eval");
    let state = std::sync::Arc::new(Mutex::new(ScriptWorld::default()));
    let scene_text = std::sync::Arc::new(
        scene
            .iter()
            .map(|(_, node)| {
                let mut text = node.name.to_ascii_lowercase();
                if let Some(role) = node.semantic_role.as_deref() {
                    text.push(' ');
                    text.push_str(&role.to_ascii_lowercase());
                }
                if let Some(key) = node.stable_key.as_deref() {
                    text.push(' ');
                    text.push_str(&key.to_ascii_lowercase());
                }
                for tag in &node.tags {
                    text.push(' ');
                    text.push_str(&tag.to_ascii_lowercase());
                }
                text
            })
            .collect::<Vec<_>>(),
    );

    let mut world_module = rhai::Module::new();
    let texts = scene_text.clone();
    world_module.set_native_fn(
        "count",
        move |query: rhai::ImmutableString| -> Result<i64, Box<rhai::EvalAltResult>> {
            let query = query.trim().to_ascii_lowercase();
            if query.is_empty() {
                return Ok(0);
            }
            Ok(texts.iter().filter(|text| text.contains(&query)).count() as i64)
        },
    );
    let world = state.clone();
    world_module.set_native_fn(
        "group",
        move |name: rhai::ImmutableString| -> Result<(), Box<rhai::EvalAltResult>> {
            let mut world = world
                .lock()
                .map_err(|_| script_error("Authoring script state was poisoned."))?;
            if world.groups.len() >= 512 {
                return Err(script_error("The plan already has 512 groups."));
            }
            world
                .groups
                .push(serde_json::json!({"name": name.to_string()}));
            Ok(())
        },
    );
    let world = state.clone();
    world_module.set_native_fn(
        "parent",
        move |name: rhai::ImmutableString| -> Result<(), Box<rhai::EvalAltResult>> {
            let mut world = world
                .lock()
                .map_err(|_| script_error("Authoring script state was poisoned."))?;
            world.parent = Some(name.to_string());
            Ok(())
        },
    );
    let world = state.clone();
    world_module.set_native_fn(
        "parent_clear",
        move || -> Result<(), Box<rhai::EvalAltResult>> {
            let mut world = world
                .lock()
                .map_err(|_| script_error("Authoring script state was poisoned."))?;
            world.parent = None;
            Ok(())
        },
    );
    let world = state.clone();
    world_module.set_native_fn(
        "camera",
        move |name: rhai::ImmutableString,
              x: f64,
              y: f64,
              z: f64,
              fov: f64|
              -> Result<(), Box<rhai::EvalAltResult>> {
            if !fov.is_finite() || !(1.0..=179.0).contains(&fov) {
                return Err(script_error("world::camera fov must be between 1 and 179."));
            }
            world
                .lock()
                .map_err(|_| script_error("Authoring script state was poisoned."))?
                .push(serde_json::json!({
                    "op": "camera",
                    "name": name.to_string(),
                    "position": [x, y, z],
                    "fov": fov,
                }));
            Ok(())
        },
    );
    let world = state.clone();
    world_module.set_native_fn(
        "script",
        move |target: rhai::ImmutableString,
              file: rhai::ImmutableString|
              -> Result<(), Box<rhai::EvalAltResult>> {
            if file.trim().is_empty() {
                return Err(script_error("world::script requires a script file."));
            }
            world
                .lock()
                .map_err(|_| script_error("Authoring script state was poisoned."))?
                .push(serde_json::json!({
                    "op": "script",
                    "target": target.to_string(),
                    "file": file.to_string(),
                }));
            Ok(())
        },
    );
    let world = state.clone();
    world_module.set_native_fn(
        "place",
        move |archetype: rhai::ImmutableString,
              x: f64,
              y: f64,
              z: f64|
              -> Result<(), Box<rhai::EvalAltResult>> {
            world
                .lock()
                .map_err(|_| script_error("Authoring script state was poisoned."))?
                .push(serde_json::json!({
                    "op": "place",
                    "archetype": archetype.to_string(),
                    "position": [x, y, z],
                }));
            Ok(())
        },
    );
    let world = state.clone();
    world_module.set_native_fn(
        "place_named",
        move |archetype: rhai::ImmutableString,
              name: rhai::ImmutableString,
              x: f64,
              y: f64,
              z: f64|
              -> Result<(), Box<rhai::EvalAltResult>> {
            world
                .lock()
                .map_err(|_| script_error("Authoring script state was poisoned."))?
                .push(serde_json::json!({
                    "op": "place",
                    "archetype": archetype.to_string(),
                    "name": name.to_string(),
                    "position": [x, y, z],
                }));
            Ok(())
        },
    );
    let world = state.clone();
    world_module.set_native_fn(
        "grid",
        move |archetype: rhai::ImmutableString,
              count_x: i64,
              count_z: i64,
              spacing_x: f64,
              spacing_z: f64|
              -> Result<(), Box<rhai::EvalAltResult>> {
            push_script_grid(
                &world,
                archetype.as_str(),
                count_x,
                count_z,
                spacing_x,
                spacing_z,
                0.0,
                0.0,
            )
        },
    );
    let world = state.clone();
    world_module.set_native_fn(
        "grid_at",
        move |archetype: rhai::ImmutableString,
              count_x: i64,
              count_z: i64,
              spacing_x: f64,
              spacing_z: f64,
              origin_x: f64,
              origin_z: f64|
              -> Result<(), Box<rhai::EvalAltResult>> {
            push_script_grid(
                &world,
                archetype.as_str(),
                count_x,
                count_z,
                spacing_x,
                spacing_z,
                origin_x,
                origin_z,
            )
        },
    );
    let world = state.clone();
    world_module.set_native_fn(
        "line",
        move |archetype: rhai::ImmutableString,
              x1: f64,
              z1: f64,
              x2: f64,
              z2: f64,
              spacing: f64|
              -> Result<(), Box<rhai::EvalAltResult>> {
            if !spacing.is_finite() || spacing <= 0.0 {
                return Err(script_error(
                    "world::line spacing must be greater than zero.",
                ));
            }
            world
                .lock()
                .map_err(|_| script_error("Authoring script state was poisoned."))?
                .push(serde_json::json!({
                    "op": "along",
                    "archetype": archetype.to_string(),
                    "points": [[x1, 0.0, z1], [x2, 0.0, z2]],
                    "spacing": spacing,
                }));
            Ok(())
        },
    );
    let world = state.clone();
    world_module.set_native_fn(
        "scatter",
        move |archetype: rhai::ImmutableString,
              count: i64,
              min_x: f64,
              min_z: f64,
              max_x: f64,
              max_z: f64,
              seed: i64|
              -> Result<(), Box<rhai::EvalAltResult>> {
            if count <= 0 || count > 4096 {
                return Err(script_error(
                    "world::scatter count must be between 1 and 4096.",
                ));
            }
            if min_x > max_x || min_z > max_z {
                return Err(script_error(
                    "world::scatter min must be <= max on both axes.",
                ));
            }
            world
                .lock()
                .map_err(|_| script_error("Authoring script state was poisoned."))?
                .push(serde_json::json!({
                    "op": "scatter",
                    "archetype": archetype.to_string(),
                    "count": count,
                    "region": {"min": [min_x, 0.0, min_z], "max": [max_x, 0.0, max_z]},
                    "seed": seed.max(1),
                }));
            Ok(())
        },
    );
    let world = state.clone();
    world_module.set_native_fn(
        "assert_max_entities",
        move |max: i64| -> Result<(), Box<rhai::EvalAltResult>> {
            if max < 0 {
                return Err(script_error(
                    "world::assert_max_entities requires a positive budget.",
                ));
            }
            let mut world = world
                .lock()
                .map_err(|_| script_error("Authoring script state was poisoned."))?;
            world.push(serde_json::json!({"op": "assert", "max_entities": max}));
            Ok(())
        },
    );
    let world = state.clone();
    world_module.set_native_fn(
        "emit",
        move || -> Result<String, Box<rhai::EvalAltResult>> {
            let world = world
                .lock()
                .map_err(|_| script_error("Authoring script state was poisoned."))?;
            serde_json::to_string(&world.program())
                .map_err(|error| script_error(format!("Could not emit the plan: {error}")))
        },
    );

    engine.register_static_module("world", world_module.into());

    engine
        .run(source)
        .map_err(|error| format!("Authoring script failed: {error}"))?;
    let program = {
        let state = state
            .lock()
            .map_err(|_| "Authoring script state was poisoned.".to_string())?;
        state.program()
    };
    let operation_count = program
        .get("operations")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    if operation_count == 0 {
        return Err(
            "The authoring script did not emit any operation. Call world::group, world::place, world::grid, world::line or world::scatter."
                .to_string(),
        );
    }
    compile_world_program(&program)?;
    Ok((program, operation_count))
}

fn push_script_grid(
    world: &std::sync::Arc<Mutex<ScriptWorld>>,
    archetype: &str,
    count_x: i64,
    count_z: i64,
    spacing_x: f64,
    spacing_z: f64,
    origin_x: f64,
    origin_z: f64,
) -> Result<(), Box<rhai::EvalAltResult>> {
    if count_x <= 0 || count_z <= 0 || count_x.saturating_mul(count_z) > 4096 {
        return Err(script_error(
            "world::grid requires positive counts with at most 4096 instances.",
        ));
    }
    if !spacing_x.is_finite() || !spacing_z.is_finite() {
        return Err(script_error("world::grid spacing must be finite."));
    }
    world
        .lock()
        .map_err(|_| script_error("Authoring script state was poisoned."))?
        .push(serde_json::json!({
            "op": "grid",
            "archetype": archetype,
            "count": [count_x, 1, count_z],
            "spacing": [spacing_x, 0.0, spacing_z],
            "origin": [origin_x, 0.0, origin_z],
        }));
    Ok(())
}

fn read_authoring_script(root: &Path, relative: &str) -> Result<String, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("Could not resolve the project root: {error}"))?;
    let candidate = Path::new(relative);
    let candidate = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        root.join(candidate)
    };
    let candidate = candidate
        .canonicalize()
        .map_err(|error| format!("Could not resolve script file '{relative}': {error}"))?;
    if !candidate.starts_with(&root) {
        return Err("Authoring scripts must live inside the active project.".to_string());
    }
    let bytes = std::fs::read(&candidate)
        .map_err(|error| format!("Could not read script file '{relative}': {error}"))?;
    if bytes.len() > 64 * 1024 {
        return Err("Authoring scripts are limited to 64 KiB.".to_string());
    }
    String::from_utf8(bytes)
        .map_err(|error| format!("Script file '{relative}' must be UTF-8: {error}"))
}

pub(crate) fn execute_world_script(
    request: &ParsedCommand,
    ctx: &mut GameCommandContext<'_>,
) -> CommandOutput {
    let result = (|| -> Result<CommandOutput, String> {
        let root = active_project_root()
            .ok_or_else(|| "World scripts require an active project root.".to_string())?;
        let (source, origin) = if let Some(source) = request
            .structured_arg("script")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| request.arg("script").map(str::to_string))
        {
            (source, "inline".to_string())
        } else if let Some(file) =
            text_argument(request, "file").or_else(|| text_argument(request, "path"))
        {
            let source = read_authoring_script(&root, &file)?;
            (source, file)
        } else {
            return Err(
                "world.script requires script=<rhai source> or file=<project-relative .rhai>."
                    .to_string(),
            );
        };
        let script_hash = sha256_hex(source.as_bytes());
        let script_path = format!(
            ".aura_rafi/world_scripts/authoring-{}.rhai",
            &script_hash[..12.min(script_hash.len())]
        );
        let absolute_script = root.join(&script_path);
        if let Some(parent) = absolute_script.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create the world script directory: {error}"))?;
        }
        if !absolute_script.exists() {
            std::fs::write(&absolute_script, source.as_bytes())
                .map_err(|error| format!("Could not save the world script: {error}"))?;
        }
        let (program, operation_count) = world_script_program(&source, ctx.scene)?;
        let group_count = program
            .get("groups")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0);
        let plan_request =
            parsed_command_from_params("game.plan", serde_json::json!({"program": program}));
        let mut output = execute_plan(&plan_request, ctx);
        if matches!(output.level, crate::commands::output::CommandLevel::Error) {
            return Ok(output);
        }
        let plan_id = output.json.get("plan_id").cloned().unwrap_or(Value::Null);
        output
            .lines
            .insert(0, format!("script: {operation_count} operation(s)"));
        if let Some(object) = output.json.as_object_mut() {
            object.insert(
                "script".to_string(),
                serde_json::json!({
                    "origin": origin,
                    "operations": operation_count,
                    "groups": group_count,
                    "path": script_path,
                    "sandbox": "rhai-authoring-v1",
                }),
            );
        }
        append_journal(
            &root,
            serde_json::json!({
                "event": "world_script_planned",
                "origin": origin,
                "operations": operation_count,
                "script": script_path,
                "plan_id": plan_id,
            }),
        );
        Ok(output)
    })();
    result.unwrap_or_else(|error| CommandOutput::error("World script", error))
}
