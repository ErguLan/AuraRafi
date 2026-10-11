//! Reusable behavior parameters and prefab authoring shared by native/Agent/CLI/MCP.
use super::{output::CommandOutput, parser::ParsedCommand, script::ScriptCommandContext};
use raf_core::scene::{prefab::ScenePrefab, SceneGraph, SceneNodeId, SceneVariable, VariableValue};
use std::{
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

pub fn execute(
    command: &ParsedCommand,
    ctx: &mut ScriptCommandContext<'_>,
    preview: bool,
) -> CommandOutput {
    let before = ctx.scene.clone();
    match execute_inner(command, ctx, preview) {
        Ok(mut output) => {
            output.changed = command.name == "prefab.capture"
                || ron::to_string(ctx.scene).ok() != ron::to_string(&before).ok();
            output
        }
        Err(error) => {
            *ctx.scene = before;
            CommandOutput::error(&command.name, error)
        }
    }
}

/// A UUID or an unambiguous explicit path/name; never the first duplicate name.
pub fn resolve(scene: &SceneGraph, reference: &str) -> Result<SceneNodeId, String> {
    raf_script::references::entity(scene, reference)
        .ok_or_else(|| format!("Missing or ambiguous entity: {reference}"))
}

fn local_asset(root: &Path, value: &str, must_exist: bool) -> Result<PathBuf, String> {
    let relative = Path::new(value);
    if value.is_empty()
        || value.len() > 512
        || value.contains(':')
        || value.contains('\\')
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        || relative.is_absolute()
    {
        return Err("Use a confined assets-relative path without traversal".into());
    }
    let project = root
        .parent()
        .ok_or("Missing project root")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    if !root.starts_with(&project) {
        return Err("Assets root escapes the project".into());
    }
    let file = root.join(relative);
    let check = if must_exist {
        file.as_path()
    } else {
        file.parent().ok_or("Missing asset parent")?
    };
    let canonical = check.canonicalize().map_err(|e| e.to_string())?;
    if !canonical.starts_with(&root) {
        return Err("Asset escapes the project root".into());
    }
    if !must_exist && file.exists() {
        return Err("Asset already exists; choose a different filename".into());
    }
    Ok(if must_exist {
        canonical
    } else {
        canonical.join(file.file_name().ok_or("Missing filename")?)
    })
}

pub fn parse_value(scene: &SceneGraph, kind: &str, value: &str) -> Result<VariableValue, String> {
    match kind.to_ascii_lowercase().as_str() {
        "bool" => value
            .parse()
            .map(VariableValue::Bool)
            .map_err(|_| "Bool requires true or false".into()),
        "number" => value
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite())
            .map(VariableValue::Number)
            .ok_or_else(|| "Number must be finite".into()),
        "text" if value.len() <= 4096 => Ok(VariableValue::Text(value.to_string())),
        "entity" if value.is_empty() || value == "none" => Ok(VariableValue::Entity(None)),
        "entity" => Ok(VariableValue::Entity(Some(
            scene
                .get(resolve(scene, value)?)
                .expect("resolved entity")
                .uuid,
        ))),
        _ => Err("Use Bool, Number, Text (<=4096 bytes), or Entity".into()),
    }
}

fn execute_inner(
    command: &ParsedCommand,
    ctx: &mut ScriptCommandContext<'_>,
    preview: bool,
) -> Result<CommandOutput, String> {
    let arg = |name: &str| command.arg(name).ok_or_else(|| format!("Missing {name}"));
    let data = match command.name.as_str() {
        "behavior.parameter" => {
            let id = resolve(ctx.scene, arg("target")?)?;
            let name = arg("name")?;
            if name.trim().is_empty() || name.len() > 128 {
                return Err("Parameter name requires 1..128 bytes".into());
            }
            let source = command.arg("source").filter(|s| !s.is_empty());
            if source.is_some_and(|s| {
                !ctx.scene
                    .get(id)
                    .expect("resolved entity")
                    .scripts
                    .iter()
                    .any(|p| p == s)
            }) {
                return Err("Attach the behavior source before assigning its parameters".into());
            }
            let action = command.arg("action").unwrap_or("set");
            if !matches!(action, "set" | "remove" | "rename") {
                return Err("Use action=set|remove|rename".into());
            }
            let value = if action == "set" {
                Some(parse_value(
                    ctx.scene,
                    arg("type")?,
                    command.arg("value").unwrap_or_default(),
                )?)
            } else {
                None
            };
            let new_name = if action == "rename" {
                Some(arg("new_name")?)
            } else {
                None
            };
            let node = ctx.scene.get_mut(id).expect("resolved entity");
            if source.is_some_and(|source| !node.behavior_parameters.contains_key(source))
                && node.behavior_parameters.len() >= 64
            {
                return Err("Behavior parameter scope budget exceeded (64)".into());
            }
            let values = match source {
                Some(source) => node.behavior_parameters.entry(source.into()).or_default(),
                None => &mut node.variables,
            };
            if let Some(new_name) = new_name {
                if new_name.trim().is_empty()
                    || new_name.len() > 128
                    || values.iter().any(|p| p.name == new_name && p.name != name)
                {
                    return Err("Parameter name is empty, too long or already in use".into());
                }
                let parameter = values
                    .iter_mut()
                    .find(|p| p.name == name)
                    .ok_or("Parameter not found")?;
                parameter.name = new_name.to_string();
            } else if action == "remove" {
                values.retain(|p| p.name != name);
            } else if let Some(parameter) = values.iter_mut().find(|p| p.name == name) {
                parameter.value = value.expect("set action");
            } else {
                if values.len() >= 256 {
                    return Err("Parameter budget exceeded (256 per scope)".into());
                }
                values.push(SceneVariable {
                    name: name.into(),
                    value: value.expect("set action"),
                });
            }
            serde_json::json!({"target": node.uuid, "parameter": name, "source": source, "action": action})
        }
        "behavior.attach" | "behavior.detach" => {
            let id = resolve(ctx.scene, arg("target")?)?;
            let source = arg("source")?;
            if command.name == "behavior.attach" {
                let root = ctx.assets_root.ok_or("No assets root")?;
                let file = local_asset(root, source, true)?;
                if file.extension().and_then(|e| e.to_str()) != Some("rhai") {
                    return Err("Only Rhai behaviors are executable".into());
                }
                if fs::metadata(&file).map_err(|e| e.to_string())?.len() > 256 * 1024 {
                    return Err("Behavior source exceeds 256KiB".into());
                }
                let node = ctx.scene.get_mut(id).expect("resolved entity");
                if !node.scripts.iter().any(|s| s == source) {
                    if node.scripts.len() >= 64 {
                        return Err("Behavior attachment budget exceeded (64)".into());
                    }
                    node.scripts.push(source.into());
                }
            } else {
                // Retain overrides for an explicit reattachment; they are never executed alone.
                ctx.scene
                    .get_mut(id)
                    .expect("resolved entity")
                    .scripts
                    .retain(|s| s != source);
            }
            serde_json::json!({"target": ctx.scene.get(id).expect("resolved entity").uuid, "source": source})
        }
        "prefab.capture" => {
            let root = ctx.assets_root.ok_or("No assets root")?;
            let file = arg("file")?;
            if !file.ends_with(".prefab.ron") {
                return Err("Prefab filename must end in .prefab.ron".into());
            }
            let path = local_asset(root, file, false)?;
            let prefab = ScenePrefab::capture(ctx.scene, resolve(ctx.scene, arg("target")?)?)?;
            let source = ron::ser::to_string_pretty(&prefab, ron::ser::PrettyConfig::default())
                .map_err(|e| e.to_string())?;
            if source.len() > 8 * 1024 * 1024 {
                return Err("Prefab exceeds 8MiB".into());
            }
            if !preview {
                let mut output = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .map_err(|e| e.to_string())?;
                output.write_all(source.as_bytes()).map_err(|e| format!("Could not finish {}: {e}. A partial new file may remain; no existing asset was overwritten.", path.display()))?;
            }
            serde_json::json!({"file": file, "nodes": prefab.nodes.len(), "preview": preview, "file_undo": false})
        }
        "prefab.instantiate" => {
            let file = arg("file")?;
            if !file.ends_with(".prefab.ron") {
                return Err("Prefab filename must end in .prefab.ron".into());
            }
            let path = local_asset(ctx.assets_root.ok_or("No assets root")?, file, true)?;
            let mut source = String::new();
            fs::File::open(path)
                .map_err(|e| e.to_string())?
                .take(8 * 1024 * 1024 + 1)
                .read_to_string(&mut source)
                .map_err(|e| e.to_string())?;
            if source.len() > 8 * 1024 * 1024 {
                return Err("Prefab exceeds 8MiB".into());
            }
            let prefab: ScenePrefab = ron::from_str(&source).map_err(|e| e.to_string())?;
            let parent = command
                .arg("parent")
                .filter(|p| !p.is_empty())
                .map(|p| resolve(ctx.scene, p))
                .transpose()?;
            let id = prefab.instantiate(ctx.scene, parent)?;
            let node = ctx.scene.get_mut(id).expect("created entity");
            node.source_asset = Some(file.to_string());
            node.source_schema_version = Some(prefab.version);
            serde_json::json!({"file": file, "nodes": prefab.nodes.len(), "root": ctx.scene.get(id).expect("created entity").uuid, "preview": preview})
        }
        _ => return Err("Unknown behavior/prefab command".into()),
    };
    // The preview caller supplies a disposable scene; only prefab.capture can write a file.
    Ok(CommandOutput::changed(
        &command.name,
        vec![if preview {
            "Preview only".into()
        } else {
            "Authoring change applied".into()
        }],
        data,
    ))
}
