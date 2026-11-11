//! Script domain commands.
//!
//! Follows the pattern in docs/COMMANDS.md. These commands let the console
//! (and future AI agents) create, attach, list, validate, and run scripts
//! without touching the editor UI directly.
//!
//! All script commands live in the `shared` domain so they work in both
//! game and electronics projects.

use std::path::Path;

use raf_core::config::ScriptLanguage;
use raf_core::scene::{Primitive, SceneGraph};
use raf_script::{InputSnapshot, RhaiScriptRuntime, ScriptRuntimeOptions};

use crate::commands::output::CommandOutput;
use crate::commands::parser::ParsedCommand;
use crate::script_support::{scan_script_catalog, validate_attached_script};

/// Context for script domain commands.
pub struct ScriptCommandContext<'a> {
    pub scene: &'a mut SceneGraph,
    pub assets_root: Option<&'a Path>,
}

/// Entry point for script domain commands.
pub fn execute(
    command_name: &str,
    command: &ParsedCommand,
    ctx: &mut ScriptCommandContext<'_>,
) -> CommandOutput {
    match command_name {
        "script.create" => create_script(command, ctx),
        "script.write" => write_script(command, ctx),
        "script.attach" => attach_script(command, ctx),
        "script.detach" => detach_script(command, ctx),
        "script.list" => list_scripts(ctx),
        "script.validate" => validate_script(command, ctx),
        "script.run" => run_script(command, ctx),
        "script.compile_nodes" => compile_nodes(command, ctx),
        _ => CommandOutput::error(
            "Script command",
            format!("Unknown script command: {command_name}"),
        ),
    }
}

/// Create a new script file from a template.
fn create_script(command: &ParsedCommand, ctx: &mut ScriptCommandContext<'_>) -> CommandOutput {
    let Some(root) = ctx.assets_root else {
        return CommandOutput::error("Create script", "No assets folder for this project.");
    };

    let lang_str = command.arg("lang").unwrap_or("rhai");
    let lang = match lang_str {
        "rhai" => ScriptLanguage::Rhai,
        "cpp" => ScriptLanguage::Cpp,
        "nodes" => ScriptLanguage::Nodes,
        other => {
            return CommandOutput::error(
                "Create script",
                format!("Unknown language: {}. Use rhai, cpp, or nodes.", other),
            );
        }
    };

    let name = sanitize_script_name(command.arg("name").unwrap_or("new_script"));
    if lang_str == "rust" {
        return create_script_file(root, &name, "rs", rust_template(&name), "Rust");
    }
    let scripts_dir = root.join("scripts");
    let _ = std::fs::create_dir_all(&scripts_dir);

    let (file_name, content) = match lang {
        ScriptLanguage::Rhai => {
            let file = format!("{}.rhai", name);
            let body = rhai_template(&name);
            (file, body)
        }
        ScriptLanguage::Cpp => {
            let file = format!("{}.cpp", name);
            let body = cpp_template(&name);
            (file, body)
        }
        ScriptLanguage::Nodes => {
            return CommandOutput::error(
                "Create script",
                "Visual node scripts are created from the Node Editor panel, not from files.",
            );
        }
    };

    let file_path = scripts_dir.join(&file_name);
    let relative = format!("scripts/{}", file_name);

    if file_path.exists() {
        return CommandOutput::error(
            "Create script",
            format!("File already exists: {}", relative),
        );
    }

    if let Err(e) = std::fs::write(&file_path, &content) {
        return CommandOutput::error("Create script", format!("Failed to write: {}", e));
    }

    CommandOutput::changed(
        "Script created",
        vec![
            format!("Language: {}", lang.label()),
            format!("Path: {}", relative),
            "Edit it in an external editor, then attach it to an entity.".to_string(),
        ],
        serde_json::json!({
            "path": relative,
            "language": lang.label(),
        }),
    )
}

fn create_script_file(
    assets_root: &Path,
    name: &str,
    extension: &str,
    content: String,
    language: &str,
) -> CommandOutput {
    let scripts_dir = assets_root.join("scripts");
    let _ = std::fs::create_dir_all(&scripts_dir);
    let file_name = format!("{name}.{extension}");
    let file_path = scripts_dir.join(&file_name);
    let relative = format!("scripts/{file_name}");
    if file_path.exists() {
        return CommandOutput::error("Create script", format!("File already exists: {relative}"));
    }
    if let Err(error) = std::fs::write(&file_path, content) {
        return CommandOutput::error("Create script", format!("Failed to write: {error}"));
    }
    CommandOutput::changed(
        "Script created",
        vec![
            format!("Language: {language}"),
            format!("Path: {relative}"),
            "Edit it in an external editor, then attach it to an entity.".to_string(),
        ],
        serde_json::json!({"path": relative, "language": language}),
    )
}

/// Create or replace a project-confined Rhai source file.
///
/// The source is compiled before it reaches disk, the write is staged in the
/// same directory, and a failed replacement restores the previous file. This
/// is the native authoring path that lets the internal Agent write gameplay
/// code without a shell or arbitrary filesystem access.
fn write_script(command: &ParsedCommand, ctx: &mut ScriptCommandContext<'_>) -> CommandOutput {
    let Some(assets_root) = ctx.assets_root else {
        return CommandOutput::error("Write script", "No assets folder for this project.");
    };
    let Some(file) = command.arg("file") else {
        return CommandOutput::error("Write script", "Missing file=<relative .rhai path>.");
    };
    let Some(content) = command.arg("content") else {
        return CommandOutput::error("Write script", "Missing content=<Rhai source>.");
    };
    let overwrite = matches!(
        command.arg("overwrite").map(|value| value.to_ascii_lowercase()),
        Some(value) if matches!(value.as_str(), "true" | "1" | "yes")
    );
    if content.len() > 262_144 {
        return CommandOutput::error(
            "Write script",
            "Script source exceeds the 256 KiB authoring limit.",
        );
    }
    let relative = match normalize_write_path(file) {
        Ok(relative) => relative,
        Err(error) => return CommandOutput::error("Write script", error),
    };
    let engine = raf_script::backends::rhai_backend::create_engine(100_000);
    let compiled =
        match raf_script::backends::rhai_backend::compile_source(&engine, &relative, content) {
            Ok(compiled) => compiled,
            Err(error) => {
                return CommandOutput::error(
                    "Write script",
                    format!("Rhai validation failed: {error}"),
                )
            }
        };
    let root = match assets_root.canonicalize() {
        Ok(root) => root,
        Err(error) => return CommandOutput::error("Write script", error.to_string()),
    };
    let target = assets_root.join(&relative);
    let Some(parent) = target.parent() else {
        return CommandOutput::error("Write script", "The script path has no parent folder.");
    };
    if let Err(error) = std::fs::create_dir_all(parent) {
        return CommandOutput::error("Write script", error.to_string());
    }
    let parent = match parent.canonicalize() {
        Ok(parent) if parent.starts_with(&root) => parent,
        Ok(_) => {
            return CommandOutput::error(
                "Write script",
                "Script paths must stay inside the project assets folder.",
            )
        }
        Err(error) => return CommandOutput::error("Write script", error.to_string()),
    };
    let target = parent.join(
        target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("script.rhai"),
    );
    if target.exists() && !overwrite {
        return CommandOutput::error(
            "Write script",
            format!("File already exists: {relative}. Pass overwrite=true to replace it."),
        );
    }
    let previous = if target.exists() {
        std::fs::read(&target).ok()
    } else {
        None
    };
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("script"),
        uuid::Uuid::new_v4()
    ));
    if let Err(error) = std::fs::write(&temporary, content) {
        return CommandOutput::error("Write script", error.to_string());
    }
    if target.exists() {
        let _ = std::fs::remove_file(&target);
    }
    if let Err(error) = std::fs::rename(&temporary, &target) {
        let _ = std::fs::remove_file(&temporary);
        if let Some(previous) = previous {
            let _ = std::fs::write(&target, previous);
        }
        return CommandOutput::error("Write script", error.to_string());
    }
    CommandOutput::changed(
        "Script written",
        vec![
            format!("Path: {relative}"),
            format!(
                "Hooks: {}{}{}",
                if compiled.has_on_start { "on_start " } else { "" },
                if compiled.has_on_update { "on_update " } else { "" },
                if compiled.has_on_destroy { "on_destroy " } else { "" },
            )
            .trim_end()
            .to_string(),
            format!("Bytes: {}", content.len()),
        ],
        serde_json::json!({
            "path": relative,
            "language": "Rhai",
            "overwritten": overwrite,
            "bytes": content.len(),
            "has_on_start": compiled.has_on_start,
            "has_on_update": compiled.has_on_update,
            "has_on_destroy": compiled.has_on_destroy,
            "status": "ready",
        }),
    )
}

/// Validate `script.write` source without touching the project.
pub fn preview_write(command: &ParsedCommand) -> CommandOutput {
    let Some(file) = command.arg("file") else {
        return CommandOutput::error("Preview script.write", "Missing file=<relative .rhai path>.");
    };
    let Some(content) = command.arg("content") else {
        return CommandOutput::error("Preview script.write", "Missing content=<Rhai source>.");
    };
    if content.len() > 262_144 {
        return CommandOutput::error(
            "Preview script.write",
            "Script source exceeds the 256 KiB authoring limit.",
        );
    }
    let relative = match normalize_write_path(file) {
        Ok(relative) => relative,
        Err(error) => return CommandOutput::error("Preview script.write", error),
    };
    let engine = raf_script::backends::rhai_backend::create_engine(100_000);
    match raf_script::backends::rhai_backend::compile_source(&engine, &relative, content) {
        Ok(compiled) => CommandOutput::changed(
            "Preview script.write",
            vec![
                format!("Path: {relative}"),
                format!("Bytes: {}", content.len()),
                "No file was written.".to_string(),
            ],
            serde_json::json!({
                "file": relative,
                "bytes": content.len(),
                "has_on_start": compiled.has_on_start,
                "has_on_update": compiled.has_on_update,
                "has_on_destroy": compiled.has_on_destroy,
                "status": "validated",
                "preview": true,
            }),
        ),
        Err(error) => CommandOutput::error(
            "Preview script.write",
            format!("Rhai validation failed: {error}"),
        ),
    }
}

fn normalize_write_path(file: &str) -> Result<String, String> {
    let path = Path::new(file);
    if path.is_absolute() {
        return Err("Script paths must be relative to the project assets folder.".into());
    }
    let mut segments = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(value) => {
                let value = value.to_string_lossy();
                if value.is_empty() {
                    continue;
                }
                segments.push(value.to_string());
            }
            std::path::Component::CurDir => {}
            _ => return Err("Script paths cannot contain '..' or a drive prefix.".into()),
        }
    }
    let mut relative = segments.join("/");
    if relative.is_empty() {
        return Err("Missing script file name.".into());
    }
    if !relative.starts_with("scripts/") {
        relative = format!("scripts/{relative}");
    }
    if !relative.to_ascii_lowercase().ends_with(".rhai") {
        return Err("script.write accepts .rhai sources; use script.create for other stubs.".into());
    }
    Ok(relative)
}

fn sanitize_script_name(value: &str) -> String {
    let name: String = value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
        .take(64)
        .collect();
    if name.is_empty() {
        "new_script".to_string()
    } else {
        name
    }
}

fn rust_template(name: &str) -> String {
    format!(
        "// Raf Engine Rust script: {name}\n// This template is an external authoring stub; runtime attachment remains explicit.\n\npub fn on_start() {{\n    // Initialize the entity or system here.\n}}\n\npub fn on_update(delta_seconds: f32) {{\n    let _ = delta_seconds;\n}}\n"
    )
}

/// Attach a script file to a scene entity by name.
fn attach_script(command: &ParsedCommand, ctx: &mut ScriptCommandContext<'_>) -> CommandOutput {
    let Some(file) = command.arg("file") else {
        return CommandOutput::error("Attach script", "Missing file=<relative path>.");
    };
    let Some(entity_name) = command.arg("entity") else {
        return CommandOutput::error("Attach script", "Missing entity=<name>.");
    };

    let relative = if file.starts_with("scripts/") {
        file.to_string()
    } else {
        format!("scripts/{}", file)
    };

    let target_id = ctx.scene.roots().iter().find_map(|&id| {
        let node = ctx.scene.get(id)?;
        if node.name == entity_name {
            Some(id)
        } else {
            None
        }
    });

    let Some(id) = target_id else {
        return CommandOutput::error(
            "Attach script",
            format!("Entity not found: {}", entity_name),
        );
    };

    let node = ctx.scene.get_mut(id).unwrap();
    if node.scripts.iter().any(|s| s == &relative) {
        return CommandOutput::warning(
            "Attach script",
            vec![format!("Already attached: {}", relative)],
            serde_json::json!({"entity": entity_name, "script": relative, "already": true}),
        );
    }

    node.scripts.push(relative.clone());
    CommandOutput::changed(
        "Script attached",
        vec![
            format!("Entity: {}", entity_name),
            format!("Script: {}", relative),
        ],
        serde_json::json!({
            "entity": entity_name,
            "script": relative,
        }),
    )
}

/// Remove a script from a scene entity.
fn detach_script(command: &ParsedCommand, ctx: &mut ScriptCommandContext<'_>) -> CommandOutput {
    let Some(file) = command.arg("file") else {
        return CommandOutput::error("Detach script", "Missing file=<relative path>.");
    };
    let Some(entity_name) = command.arg("entity") else {
        return CommandOutput::error("Detach script", "Missing entity=<name>.");
    };

    let relative = if file.starts_with("scripts/") {
        file.to_string()
    } else {
        format!("scripts/{}", file)
    };

    let target_id = ctx.scene.roots().iter().find_map(|&id| {
        let node = ctx.scene.get(id)?;
        if node.name == entity_name {
            Some(id)
        } else {
            None
        }
    });

    let Some(id) = target_id else {
        return CommandOutput::error(
            "Detach script",
            format!("Entity not found: {}", entity_name),
        );
    };

    let node = ctx.scene.get_mut(id).unwrap();
    let before = node.scripts.len();
    node.scripts.retain(|s| s != &relative);
    let removed = before != node.scripts.len();

    if removed {
        CommandOutput::changed(
            "Script detached",
            vec![
                format!("Entity: {}", entity_name),
                format!("Script: {}", relative),
            ],
            serde_json::json!({"entity": entity_name, "script": relative}),
        )
    } else {
        CommandOutput::warning(
            "Detach script",
            vec![format!("Script was not attached: {}", relative)],
            serde_json::json!({"entity": entity_name, "script": relative, "was_attached": false}),
        )
    }
}

/// List all scripts in the project scripts folder.
fn list_scripts(ctx: &mut ScriptCommandContext<'_>) -> CommandOutput {
    let Some(root) = ctx.assets_root else {
        return CommandOutput::info(
            "Script list",
            vec!["No assets folder for this project.".to_string()],
            serde_json::json!({"scripts": []}),
        );
    };

    let catalog = scan_script_catalog(root);
    let mut lines = Vec::new();
    let mut entries = Vec::new();

    for entry in &catalog {
        let hooks = match (entry.has_on_start, entry.has_on_update) {
            (true, true) => "on_start + on_update",
            (true, false) => "on_start",
            (false, true) => "on_update",
            (false, false) => "no entry points",
        };
        lines.push(format!(
            "{} [{}] {}",
            entry.relative_path,
            entry.language.label(),
            hooks
        ));
        entries.push(serde_json::json!({
            "path": entry.relative_path,
            "language": entry.language.label(),
            "has_on_start": entry.has_on_start,
            "has_on_update": entry.has_on_update,
        }));
    }

    if lines.is_empty() {
        lines.push("No scripts found. Use /script.create to make one.".to_string());
    }

    CommandOutput::info(
        "Script list",
        lines,
        serde_json::json!({"scripts": entries, "count": catalog.len()}),
    )
}

/// Validate a script file for syntax errors.
fn validate_script(command: &ParsedCommand, ctx: &mut ScriptCommandContext<'_>) -> CommandOutput {
    let Some(file) = command.arg("file") else {
        return CommandOutput::error("Validate script", "Missing file=<relative path>.");
    };

    let validation = validate_attached_script(ctx.assets_root, file);

    let status = if validation.syntax_error.is_some() {
        "syntax error"
    } else if !validation.exists {
        "missing"
    } else if !validation.supported {
        "unsupported"
    } else if !validation.has_on_start && !validation.has_on_update {
        "no entry points"
    } else {
        "ready"
    };

    let mut lines = vec![
        format!("File: {}", file),
        format!("Language: {}", validation.language.label()),
        format!("Status: {}", status),
    ];
    if let Some(error) = &validation.syntax_error {
        lines.push(error.clone());
    }

    if validation.has_on_start || validation.has_on_update {
        let hooks = match (validation.has_on_start, validation.has_on_update) {
            (true, true) => "on_start + on_update",
            (true, false) => "on_start",
            (false, true) => "on_update",
            (false, false) => "-",
        };
        lines.push(format!("Hooks: {}", hooks));
    }

    let level = if validation.exists && validation.supported && validation.syntax_error.is_none() {
        "info"
    } else {
        "warning"
    };

    let output = CommandOutput::info(
        "Script validation",
        lines,
        serde_json::json!({
            "file": file,
            "exists": validation.exists,
            "supported": validation.supported,
            "language": validation.language.label(),
            "has_on_start": validation.has_on_start,
            "has_on_update": validation.has_on_update,
            "status": status,
            "syntax_error": validation.syntax_error,
        }),
    );

    if level == "warning" {
        CommandOutput::warning("Script validation", output.lines, output.json)
    } else {
        output
    }
}

/// Run one script on a cloned scene without entering Play.
///
/// The editor document is never mutated. An explicit `entity` keeps that
/// entity as the behavior owner (including its typed parameters); otherwise
/// the script runs on a temporary `__script_run__` root. Optional `ticks` and
/// `keys` exercise update, fixed-update, bounded physics and late-update with a
/// synthetic input snapshot so missing runtime functions are reported before a
/// manual Play test.
fn run_script(command: &ParsedCommand, ctx: &mut ScriptCommandContext<'_>) -> CommandOutput {
    let Some(file) = command.arg("file") else {
        return CommandOutput::error("Run script", "Missing file=<relative path>.");
    };

    let relative = normalize_script_path(file);
    let ticks = match command.arg("ticks") {
        Some(raw) => match raw.parse::<usize>() {
            Ok(value) if value <= 600 => value,
            _ => {
                return CommandOutput::error(
                    "Run script",
                    "ticks must be an integer between 0 and 600.",
                )
            }
        },
        None => 0,
    };
    let keys = match run_keys(command) {
        Ok(keys) => keys,
        Err(error) => return CommandOutput::error("Run script", error),
    };
    let target = command
        .arg("entity")
        .or_else(|| command.arg("target"))
        .map(str::to_string);

    let mut runtime_scene = ctx.scene.clone();
    for id in runtime_scene.all_valid_ids() {
        if let Some(node) = runtime_scene.get_mut(id) {
            node.scripts.clear();
        }
    }

    let owner_id = match target.as_deref() {
        Some(reference) => match crate::commands::behavior::resolve(&runtime_scene, reference) {
            Ok(id) => {
                if let Some(node) = runtime_scene.get_mut(id) {
                    node.scripts.push(relative.clone());
                }
                id
            }
            Err(error) => return CommandOutput::error("Run script", error),
        },
        None => {
            let runner = runtime_scene.add_root_with_primitive("__script_run__", Primitive::Empty);
            if let Some(node) = runtime_scene.get_mut(runner) {
                node.scripts.push(relative.clone());
            }
            runner
        }
    };

    let (mut runtime, load_report) = RhaiScriptRuntime::load_from_scene(
        &runtime_scene,
        ctx.assets_root,
        ScriptRuntimeOptions::default(),
    );
    let loaded_scripts = runtime.script_count();
    let start_report = if loaded_scripts > 0 {
        runtime.call_start(&mut runtime_scene)
    } else {
        Default::default()
    };

    let mut errors: Vec<String> = load_report
        .errors
        .into_iter()
        .chain(start_report.errors)
        .collect();
    let mut logs: Vec<String> = load_report
        .logs
        .into_iter()
        .chain(start_report.logs)
        .collect();
    let mut physics_steps = 0usize;
    let mut physics_error: Option<String> = None;

    if ticks > 0 && loaded_scripts > 0 && errors.is_empty() {
        match raf_runtime::physics::PhysicsWorld::prepare(&runtime_scene) {
            Ok(mut physics) => {
                let dt = 1.0f32 / 60.0;
                for tick in 0..ticks {
                    let mut input = InputSnapshot::default();
                    input.keys_held = keys.clone();
                    if tick == 0 {
                        input.keys_pressed = keys.clone();
                    }
                    let update = runtime.update(&mut runtime_scene, dt, &input);
                    errors.extend(update.errors);
                    logs.extend(update.logs);
                    let fixed = runtime.fixed_update(&mut runtime_scene, dt, &input);
                    errors.extend(fixed.errors);
                    logs.extend(fixed.logs);
                    if let Err(error) = physics.step(&mut runtime_scene, dt) {
                        physics_error = Some(error.clone());
                        errors.push(format!("physics: {error}"));
                        break;
                    }
                    physics_steps += 1;
                    let late = runtime.late_update(&mut runtime_scene, dt, &input);
                    errors.extend(late.errors);
                    logs.extend(late.logs);
                }
            }
            Err(error) => {
                let detail = error.join("; ");
                physics_error = Some(detail.clone());
                errors.push(format!("physics preparation: {detail}"));
            }
        }
    }

    let owner_name = runtime_scene
        .get(owner_id)
        .map(|node| node.name.clone())
        .unwrap_or_default();
    let final_position = runtime_scene
        .world_matrix(owner_id)
        .w_axis
        .truncate()
        .to_array()
        .map(|value| (value * 1000.0).round() / 1000.0);

    let mut lines = vec![
        format!("Target: {relative}"),
        format!("Owner: {owner_name} ({})", owner_id.0),
        format!("Loaded scripts: {loaded_scripts}"),
        format!(
            "Ticks: {ticks}, physics steps: {physics_steps}, keys: {}",
            if keys.is_empty() {
                "none".to_string()
            } else {
                keys.join(",")
            }
        ),
        "Executed on a cloned scene; editor scene was not mutated.".to_string(),
    ];
    lines.extend(logs.clone());
    lines.extend(errors.iter().map(|error| format!("Error: {error}")));

    let status = if errors.is_empty() { "ok" } else { "error" };
    let json = serde_json::json!({
        "file": relative,
        "status": status,
        "entity": target,
        "owner": owner_name,
        "owner_id": owner_id.0,
        "loaded_scripts": loaded_scripts,
        "ticks": ticks,
        "physics_steps": physics_steps,
        "keys": keys,
        "final_position": final_position,
        "physics_error": physics_error,
        "logs": logs,
        "errors": errors,
    });

    if status == "ok" {
        CommandOutput::info("Run script", lines, json)
    } else {
        CommandOutput::warning("Run script", lines, json)
    }
}

fn run_keys(command: &ParsedCommand) -> Result<Vec<String>, String> {
    let mut keys = Vec::new();
    if let Some(value) = command.structured_arg("keys") {
        match value {
            serde_json::Value::Array(values) => {
                for value in values {
                    let Some(value) = value.as_str() else {
                        return Err("keys must contain only strings.".to_string());
                    };
                    keys.push(value.to_string());
                }
            }
            serde_json::Value::String(value) => {
                keys.extend(value.split(',').map(str::to_string));
            }
            _ => return Err("keys must be an array of strings or a comma list.".to_string()),
        }
    } else if let Some(raw) = command.arg("keys") {
        keys.extend(raw.split(',').map(str::to_string));
    }

    let mut normalized = Vec::new();
    for key in keys {
        let key = key.trim().to_ascii_lowercase();
        if key.is_empty() {
            continue;
        }
        if key.len() > 32
            || !key
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
        {
            return Err("keys accepts lowercase key names such as w, space, shift or arrow_left.".to_string());
        }
        if !normalized.contains(&key) {
            normalized.push(key);
        }
    }
    if normalized.len() > 16 {
        return Err("keys supports at most 16 entries.".to_string());
    }
    Ok(normalized)
}

/// Compile a node graph flow to Rhai source.
/// Returns real Rhai source without executing it or overwriting a file.
fn compile_nodes(command: &ParsedCommand, ctx: &mut ScriptCommandContext<'_>) -> CommandOutput {
    let result = (|| -> Result<String, String> {
        let root = ctx
            .assets_root
            .and_then(Path::parent)
            .ok_or("No project folder.")?
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let relative = command
            .arg("file")
            .ok_or("Missing file= relative path to saved nodes.ron.")?;
        let path = root
            .join(relative)
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if !path.starts_with(&root) || path.extension().and_then(|s| s.to_str()) != Some("ron") {
            return Err("Node graph must be a project-local RON file.".into());
        }
        if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > 1024 * 1024 {
            return Err("Node graph exceeds 1 MiB.".into());
        }
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .map_err(|e| e.to_string())?
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 1024 * 1024 {
            return Err("Node graph exceeds 1 MiB.".into());
        }
        let source = String::from_utf8(bytes).map_err(|e| e.to_string())?;
        let graph: raf_nodes::NodeGraph = ron::from_str(&source).map_err(|e| e.to_string())?;
        let source = raf_nodes::runtime_compiler::to_rhai(&graph)?;
        let engine = raf_script::backends::rhai_backend::create_engine(100_000);
        raf_script::backends::rhai_backend::compile_source(
            &engine,
            "nodes-generated.rhai",
            &source,
        )
        .map_err(|e| e.to_string())?;
        Ok(source)
    })();
    match result {
        Ok(source) => CommandOutput::info(
            "Compile nodes",
            vec!["Generated Rhai source; no execution or file write.".into()],
            serde_json::json!({"status":"compiled","source":source}),
        ),
        Err(error) => CommandOutput::error("Compile nodes", error),
    }
}

fn normalize_script_path(file: &str) -> String {
    if Path::new(file).is_absolute() || file.starts_with("scripts/") {
        file.to_string()
    } else {
        format!("scripts/{}", file)
    }
}

// ---------------------------------------------------------------------------
// Script templates
// ---------------------------------------------------------------------------

/// Template for a new Rhai script.
fn rhai_template(name: &str) -> String {
    format!(
        r#"// {name}.rhai
// AuraRafi script. See docs/SCRIPTING_SYSTEM.md for the Host API.
//
// Lifecycle hooks (all optional):
//   fn on_start()        - called once when the scene loads
//   fn on_update(dt)     - called each simulation tick, dt in seconds
//   fn on_fixed_update(dt) - called before physics
//   fn on_late_update(dt) - called after physics; camera follow belongs here
//   fn on_destroy()      - called once when the scene unloads
//
// Available functions:
//   owner() -> attachment entity; get_parent(owner()) -> real hierarchy parent
//   parameter(name, default) -> typed scalar parameter
//   entity_parameter(name) -> typed Entity parameter; always check is_valid
//   get_node(name) -> Handle
//   entity(uuid_or_path) -> Handle; check is_valid(handle) before use
//   activate_camera(handle) -> select a camera for this runtime instance
//   spawn_entity(name, primitive) -> Handle
//   destroy_entity(handle)
//   set_position(handle, x, y, z)     // meters
//   set_rotation(handle, x, y, z)     // radians
//   set_scale(handle, x, y, z)
//   move_by(handle, dx, dy, dz)       // meters
//   set_color(handle, r, g, b, a)     // 0-255
//   set_color_rgb(handle, r, g, b)    // 0-255, alpha=255
//   set_visible(handle, bool)
//   is_key_pressed(key) -> bool
//   was_key_just_pressed(key) -> bool
//   play_audio(name)
//   get_delta_time() -> f32
//   get_elapsed_time() -> f32
//
// Helpers:
//   vec3(x, y, z) -> Value
//   color(r, g, b) -> Value
//   color_rgba(r, g, b, a) -> Value

fn on_start() {{
    // Initialize your authored behavior here; no scene object is assumed.
}}

fn on_update(dt) {{
    let body = owner();
    if !is_valid(body) {{ return; }}
    let speed = parameter("speed", 5.0);

    if is_key_pressed("w") {{
        body.move_by(0.0, 0.0, speed * dt);
    }}
    if is_key_pressed("s") {{
        body.move_by(0.0, 0.0, -speed * dt);
    }}
}}
"#,
        name = name
    )
}

/// Template for a new C++ script (WASM target, Phase D).
fn cpp_template(name: &str) -> String {
    format!(
        r#"// {name}.cpp
// AuraRafi WASM Native Module.
// See docs/SCRIPTING_SYSTEM.md section 4 for the Host ABI.
//
// Compile to .wasm:
//   clang++ --target=wasm32 -O2 -nostdlib -o {name}.wasm {name}.cpp
//
// CONTRACT TEMPLATE ONLY: local Play does not load or execute C++/WASM yet.
// The declarations below describe a proposed import ABI, not a shipping SDK.
// Host API functions are imported from the "aurarafi" import module.

extern "C" {{

// Imported from the engine (Host ABI).
__attribute__((import_module("aurarafi")))
void* get_node(const char* name, int name_len);

__attribute__((import_module("aurarafi")))
void set_position(void* handle, float x, float y, float z);

__attribute__((import_module("aurarafi")))
void set_color(void* handle, int r, int g, int b, int a);

__attribute__((import_module("aurarafi")))
float get_delta_time();

__attribute__((import_module("aurarafi")))
int is_key_pressed(const char* key, int key_len);

// Exported to the engine.
__attribute__((export_name("on_start")))
void on_start() {{
    // Add module initialization through the future Host ABI here.
}}

__attribute__((export_name("on_update")))
void on_update(float dt) {{
    // Add frame behavior through the future Host ABI here.
}}

}}
"#,
        name = name
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::output::CommandLevel;
    use crate::commands::parser::{parse_console_input, ParsedInput};
    use glam::Vec3;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn script_write_validates_confines_and_preserves_existing_source() {
        use std::collections::BTreeMap;

        let project_root =
            std::env::temp_dir().join(format!("raf_script_write_{}", Uuid::new_v4()));
        let assets = project_root.join("assets");
        fs::create_dir_all(&assets).unwrap();
        let mut scene = SceneGraph::new();
        let mut ctx = ScriptCommandContext {
            scene: &mut scene,
            assets_root: Some(&assets),
        };
        let command = |args: &[(&str, &str)]| ParsedCommand {
            raw: String::new(),
            name: "script.write".into(),
            args: args
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect::<BTreeMap<_, _>>(),
            positional: Vec::new(),
            structured_args: None,
        };

        let valid = command(&[
            ("file", "player.rhai"),
            (
                "content",
                "fn on_update(dt) { owner().move_by(0.0, 0.0, 2.0 * dt); }",
            ),
        ]);
        let output = execute("script.write", &valid, &mut ctx);
        assert_eq!(output.level, CommandLevel::Info);
        assert!(output.changed);
        assert!(assets.join("scripts/player.rhai").exists());

        let duplicate = execute("script.write", &valid, &mut ctx);
        assert_eq!(duplicate.level, CommandLevel::Error);
        let previous = fs::read_to_string(assets.join("scripts/player.rhai")).unwrap();
        let invalid = command(&[
            ("file", "player.rhai"),
            ("content", "fn on_update("),
            ("overwrite", "true"),
        ]);
        let failed = execute("script.write", &invalid, &mut ctx);
        assert_eq!(failed.level, CommandLevel::Error);
        assert_eq!(
            fs::read_to_string(assets.join("scripts/player.rhai")).unwrap(),
            previous
        );

        let escape = command(&[("file", "../escape.rhai"), ("content", "fn on_start() {}")]);
        let rejected = execute("script.write", &escape, &mut ctx);
        assert_eq!(rejected.level, CommandLevel::Error);
        assert!(!project_root.join("escape.rhai").exists());

        let _ = fs::remove_dir_all(project_root);
    }

    #[test]
    fn run_script_executes_on_clone_without_mutating_editor_scene() {
        let project_root =
            std::env::temp_dir().join(format!("raf_script_command_{}", Uuid::new_v4()));
        let script_dir = project_root.join("assets").join("scripts");
        fs::create_dir_all(&script_dir).unwrap();
        fs::write(
            script_dir.join("run_once.rhai"),
            r#"
fn on_start() {
    let body = get_node("Body");
    body.set_position(9.0, 0.0, 0.0);
}
"#,
        )
        .unwrap();

        let mut scene = SceneGraph::new();
        let body = scene.add_root_with_primitive("Body", Primitive::Cube);
        let ParsedInput::Command(command) =
            parse_console_input("/script.run file=run_once.rhai").unwrap()
        else {
            panic!("expected command");
        };
        let mut ctx = ScriptCommandContext {
            scene: &mut scene,
            assets_root: Some(&project_root.join("assets")),
        };

        let output = execute("script.run", &command, &mut ctx);

        assert_eq!(output.level, CommandLevel::Info);
        assert_eq!(output.json["status"], "ok");
        assert_eq!(ctx.scene.get(body).unwrap().position, Vec3::ZERO);

        let _ = fs::remove_dir_all(project_root);
    }

    #[test]
    fn run_script_targets_an_entity_and_exercises_ticks_with_synthetic_keys() {
        let project_root =
            std::env::temp_dir().join(format!("raf_script_ticks_{}", Uuid::new_v4()));
        let script_dir = project_root.join("assets").join("scripts");
        fs::create_dir_all(&script_dir).unwrap();
        fs::write(
            script_dir.join("tick.rhai"),
            r#"
fn on_fixed_update(dt) {
    if is_key_pressed("w") {
        print("tick");
    }
    owner().set_property("angle", atan2(-1.0, -1.0) + dt * 0.0);
}
"#,
        )
        .unwrap();

        let mut scene = SceneGraph::new();
        let body = scene.add_root_with_primitive("Body", Primitive::Cube);
        let ParsedInput::Command(command) =
            parse_console_input("/script.run file=tick.rhai entity=Body ticks=2 keys=w").unwrap()
        else {
            panic!("expected command");
        };
        let mut ctx = ScriptCommandContext {
            scene: &mut scene,
            assets_root: Some(&project_root.join("assets")),
        };

        let output = execute("script.run", &command, &mut ctx);

        assert_eq!(output.level, CommandLevel::Info, "{:?}", output.lines);
        assert_eq!(output.json["status"], "ok");
        assert_eq!(output.json["owner"], "Body");
        assert_eq!(output.json["ticks"], 2);
        assert_eq!(output.json["physics_steps"], 2);
        assert!(output.json["logs"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|log| log.as_str())
            .any(|log| log.ends_with(": tick")));
        assert!(ctx.scene.get(body).unwrap().get_variable("angle").is_none());

        let _ = fs::remove_dir_all(project_root);
    }
}
