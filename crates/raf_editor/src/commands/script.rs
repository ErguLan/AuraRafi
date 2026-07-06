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
use raf_core::scene::SceneGraph;

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

    let name = command.arg("name").unwrap_or("new_script").to_string();
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
            vec![format!("Entity: {}", entity_name), format!("Script: {}", relative)],
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

    let status = if !validation.exists {
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

    if validation.has_on_start || validation.has_on_update {
        let hooks = match (validation.has_on_start, validation.has_on_update) {
            (true, true) => "on_start + on_update",
            (true, false) => "on_start",
            (false, true) => "on_update",
            (false, false) => "-",
        };
        lines.push(format!("Hooks: {}", hooks));
    }

    let level = if validation.exists && validation.supported {
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
        }),
    );

    if level == "warning" {
        CommandOutput::warning(
            "Script validation",
            output.lines,
            output.json,
        )
    } else {
        output
    }
}

/// Run a script's on_start once in the editor (testing).
/// Phase B: this will construct a ScriptContext and call the Rhai backend.
fn run_script(command: &ParsedCommand, _ctx: &mut ScriptCommandContext<'_>) -> CommandOutput {
    let Some(file) = command.arg("file") else {
        return CommandOutput::error("Run script", "Missing file=<relative path>.");
    };

    CommandOutput::warning(
        "Run script",
        vec![
            format!("Target: {}", file),
            "Script execution is not yet wired. The Host API and Rhai backend are".to_string(),
            "implemented in raf_script, but the ScriptRuntime system (Phase B) is not.".to_string(),
            "See docs/SCRIPTING_SYSTEM.md for the roadmap.".to_string(),
        ],
        serde_json::json!({
            "file": file,
            "status": "runtime_not_ready",
        }),
    )
}

/// Compile a node graph flow to Rhai source.
/// Phase E: this will use raf_nodes::compiler.
fn compile_nodes(command: &ParsedCommand, _ctx: &mut ScriptCommandContext<'_>) -> CommandOutput {
    let flow = command.arg("flow").unwrap_or("Main");
    let output = command.arg("output").unwrap_or("compiled.rhai");

    CommandOutput::warning(
        "Compile nodes",
        vec![
            format!("Flow: {}", flow),
            format!("Output: {}", output),
            "Node-to-Rhai compilation is Phase E of the scripting roadmap.".to_string(),
            "See docs/SCRIPTING_SYSTEM.md section 7.2 for details.".to_string(),
        ],
        serde_json::json!({
            "flow": flow,
            "output": output,
            "status": "phase_e_not_started",
        }),
    )
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
//   fn on_update(dt)     - called every frame, dt in seconds
//   fn on_destroy()      - called once when the scene unloads
//
// Available functions:
//   get_node(name) -> Handle
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
    let body = get_node("Body");
    body.set_color_rgb(255, 80, 80);
}}

fn on_update(dt) {{
    let body = get_node("Body");
    let speed = 5.0;

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
// The engine loads the .wasm and calls on_start / on_update.
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
    // TODO: implement
}}

__attribute__((export_name("on_update")))
void on_update(float dt) {{
    // TODO: implement
}}

}}
"#,
        name = name
    )
}
