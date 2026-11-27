//! Reusable behavior parameters and Assembly authoring shared by native/Agent/CLI/MCP.
//!
//! Command names keep the historical `prefab.` prefix and `.prefab.ron`
//! extension for compatibility; user-facing text calls the product concept an
//! Assembly.
use super::{output::CommandOutput, parser::ParsedCommand, script::ScriptCommandContext};
use raf_core::scene::{
    prefab::{
        assembly_source_hash, remap_entity_references, validate_assembly_source,
        walk_assembly_dependencies, AssemblyWalkSummary, ScenePrefab, MAX_PREFAB_NODES,
    },
    LinkedAssembly, SceneGraph, SceneNodeId, SceneVariable, VariableValue,
};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
use uuid::Uuid;

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

/// One validated Assembly source and its dependency graph.
pub(crate) struct LoadedAssembly {
    pub source: String,
    pub prefab: ScenePrefab,
    pub source_hash: String,
    pub dependencies: AssemblyWalkSummary,
}

fn normalized_assembly_source(value: &str) -> Result<String, String> {
    let normalized = raf_assets::asset_paths::normalize_reference(value)
        .map_err(|error| format!("Invalid Assembly source '{value}': {error}"))?;
    validate_assembly_source(&normalized)?;
    Ok(normalized)
}

fn read_assembly_prefab(assets_root: &Path, source: &str) -> Result<(ScenePrefab, String), String> {
    let normalized = normalized_assembly_source(source)?;
    let path = local_asset(assets_root, &normalized, true).map_err(|error| {
        format!("Assembly source '{normalized}' is missing or unavailable: {error}")
    })?;
    let mut text = String::new();
    fs::File::open(&path)
        .map_err(|error| format!("Assembly source '{normalized}' cannot be opened: {error}"))?
        .take(8 * 1024 * 1024 + 1)
        .read_to_string(&mut text)
        .map_err(|error| format!("Assembly source '{normalized}' cannot be read: {error}"))?;
    if text.len() > 8 * 1024 * 1024 {
        return Err(format!("Assembly source '{normalized}' exceeds 8MiB."));
    }
    let prefab: ScenePrefab = ron::from_str(&text)
        .map_err(|error| format!("Assembly source '{normalized}' is not valid RON: {error}"))?;
    prefab
        .validate()
        .map_err(|error| format!("Assembly source '{normalized}' is invalid: {error}"))?;
    Ok((prefab, assembly_source_hash(text.as_bytes())))
}

/// Read an Assembly source once and validate its nested links before any scene
/// mutation. `raf_core` owns the cycle/depth/budget walk; this function owns I/O.
pub(crate) fn load_assembly(assets_root: &Path, source: &str) -> Result<LoadedAssembly, String> {
    let source = normalized_assembly_source(source)?;
    let (prefab, source_hash) = read_assembly_prefab(assets_root, &source)?;
    let dependencies = walk_assembly_dependencies(&source, |dependency| {
        if dependency == source {
            Ok(prefab.clone())
        } else {
            read_assembly_prefab(assets_root, dependency).map(|(prefab, _)| prefab)
        }
    })?;
    Ok(LoadedAssembly {
        source,
        prefab,
        source_hash,
        dependencies,
    })
}

fn assembly_link(scene: &SceneGraph, id: SceneNodeId) -> Result<LinkedAssembly, String> {
    let node = scene
        .get(id)
        .filter(|node| !node.name.is_empty())
        .ok_or("The Assembly target no longer exists.")?;
    let link = node.linked_assembly.clone().ok_or(
        "Target is not a linked Assembly instance. Select an Assembly root or pass target=<uuid|path|name>.",
    )?;
    validate_assembly_source(&link.source)
        .map_err(|error| format!("The target Assembly link is invalid: {error}"))?;
    Ok(link)
}

/// Resolve `target` for Assembly commands.
///
/// The canonical command context has no editor selection, so `selected` (or an
/// omitted target) resolves only when the scene contains exactly one linked
/// Assembly. Selection-aware callers can pass the selected root's UUID/path.
fn assembly_target(scene: &SceneGraph, command: &ParsedCommand) -> Result<SceneNodeId, String> {
    let target = command
        .arg("target")
        .or_else(|| command.first_positional())
        .map(str::trim)
        .filter(|target| !target.is_empty());
    let Some(target) = target else {
        return single_linked_assembly(scene, "No Assembly target was provided");
    };
    if target.eq_ignore_ascii_case("selected") {
        return single_linked_assembly(scene, "No selected Assembly instance is available");
    }
    resolve(scene, target)
}

fn single_linked_assembly(scene: &SceneGraph, context: &str) -> Result<SceneNodeId, String> {
    let mut linked = scene
        .iter()
        .filter(|(id, node)| scene.is_valid_node(*id) && node.linked_assembly.is_some())
        .map(|(id, _)| id);
    match (linked.next(), linked.next()) {
        (Some(id), None) => Ok(id),
        (None, _) => Err(format!(
            "{context}: the scene has no linked Assembly instance. Pass target=<uuid|path|name>."
        )),
        (Some(_), Some(_)) => Err(format!(
            "{context}: multiple Assemblies exist and this command path has no editor selection. Pass target=<uuid|path|name>."
        )),
    }
}

/// Parent-before-child ids for one live subtree.
fn assembly_subtree_ids(scene: &SceneGraph, root: SceneNodeId) -> Vec<SceneNodeId> {
    let mut pending = vec![root];
    let mut ids = Vec::new();
    let mut seen = HashSet::new();
    while let Some(id) = pending.pop() {
        if !scene.is_valid_node(id) || !seen.insert(id) {
            continue;
        }
        ids.push(id);
        if let Some(node) = scene.get(id) {
            pending.extend(node.children.iter().rev().copied());
        }
    }
    ids
}

fn clear_assembly_links(scene: &mut SceneGraph, root: SceneNodeId, source: &str) -> usize {
    let normalized_source =
        raf_assets::asset_paths::normalize_reference(source).unwrap_or_else(|_| source.to_string());
    let mut pending = vec![root];
    let mut seen = HashSet::new();
    let mut cleared = 0;
    while let Some(id) = pending.pop() {
        if !scene.is_valid_node(id) || !seen.insert(id) {
            continue;
        }
        let children = scene
            .get(id)
            .map(|node| node.children.clone())
            .unwrap_or_default();
        pending.extend(children);
        if let Some(node) = scene.get_mut(id) {
            let matches = node.linked_assembly.as_ref().is_some_and(|link| {
                link.source == source
                    || raf_assets::asset_paths::normalize_reference(&link.source)
                        .is_ok_and(|candidate| candidate == normalized_source)
            });
            if matches {
                node.linked_assembly = None;
                cleared += 1;
            }
        }
    }
    cleared
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AssemblyRefresh {
    /// Replace the whole source subtree; keep only stable root presentation.
    Revert,
    /// Keep every instance-root field and replace its children from the source.
    Update,
}

/// Refresh one linked instance in place.
///
/// Existing subtree slots are reused in parent-before-child order, so a
/// same-size update does not consume new scene slots. Growth is rejected before
/// mutation when it would cross the 10000-slot limit. A shrinking refresh
/// soft-removes extra slots because scene ids are stable indices.
fn replace_assembly_subtree(
    scene: &mut SceneGraph,
    root: SceneNodeId,
    prefab: &ScenePrefab,
    mode: AssemblyRefresh,
    source: &str,
    source_hash: &str,
) -> Result<(), String> {
    if prefab.nodes.is_empty() || prefab.nodes.len() > MAX_PREFAB_NODES {
        return Err(format!(
            "Assembly source must contain 1..{MAX_PREFAB_NODES} nodes."
        ));
    }
    let current_root = scene
        .get(root)
        .filter(|node| !node.name.is_empty())
        .ok_or("The Assembly instance root no longer exists.")?
        .clone();
    let existing = assembly_subtree_ids(scene, root);
    let target_len = prefab.nodes.len();
    let added = target_len.saturating_sub(existing.len());
    if scene.len().saturating_add(added) > 10_000 {
        return Err(format!(
            "Refreshing this Assembly needs {added} new scene slots and would exceed the 10000-slot scene limit. Delete unused nodes before updating."
        ));
    }

    let mut mapped: Vec<SceneNodeId> = existing.iter().copied().take(target_len).collect();
    for extra in existing.iter().skip(target_len) {
        scene.remove_node(*extra);
    }
    for index in mapped.len()..target_len {
        let source_node = prefab
            .nodes
            .get(index)
            .ok_or("Assembly topology is incomplete.")?;
        let parent_index = source_node
            .parent
            .ok_or("Assembly child is missing its parent.")?
            .0;
        let parent_id = *mapped
            .get(parent_index)
            .ok_or("Assembly parent appears after its child.")?;
        mapped.push(scene.add_child(parent_id, &source_node.name));
    }

    let identities: HashMap<Uuid, Uuid> = prefab
        .nodes
        .iter()
        .zip(mapped.iter())
        .enumerate()
        .map(|(index, (source_node, target_id))| {
            let target_uuid = if index == 0 {
                current_root.uuid
            } else {
                scene
                    .get(*target_id)
                    .map(|node| node.uuid)
                    .unwrap_or_else(Uuid::new_v4)
            };
            (source_node.uuid, target_uuid)
        })
        .collect();

    for (index, source_node) in prefab.nodes.iter().enumerate() {
        let id = mapped[index];
        let mut node = source_node.clone();
        node.uuid = identities[&source_node.uuid];
        node.entity_index = None;
        node.parent = source_node.parent.map(|parent| mapped[parent.0]);
        node.children = source_node
            .children
            .iter()
            .map(|child| mapped[child.0])
            .collect();
        if index == 0 {
            node.parent = current_root.parent;
            let link = Some(LinkedAssembly {
                source: source.to_string(),
                version: prefab.version,
                source_hash: Some(source_hash.to_string()),
            });
            if mode == AssemblyRefresh::Revert {
                node.uuid = current_root.uuid;
                node.name = current_root.name.clone();
                node.position = current_root.position;
                node.rotation = current_root.rotation;
                node.scale = current_root.scale;
                node.color = current_root.color;
                node.visible = current_root.visible;
                node.locked = current_root.locked;
                node.variables = current_root.variables.clone();
                node.linked_assembly = link;
                node.source_asset = Some(source.to_string());
                node.source_schema_version = Some(prefab.version);
            } else {
                // v1 preserves every instance-root override and replaces only
                // the child topology plus the source fingerprint.
                node = current_root.clone();
                node.children = source_node
                    .children
                    .iter()
                    .map(|child| mapped[child.0])
                    .collect();
                node.entity_index = None;
                node.linked_assembly = link;
                node.source_asset = Some(source.to_string());
                node.source_schema_version = Some(prefab.version);
            }
        } else if index < existing.len() {
            // A reused slot keeps authored reconciliation identity.
            node.stable_key = scene.get(id).and_then(|current| current.stable_key.clone());
        } else {
            // Copies must not alias reconciliation keys from the source.
            node.stable_key = None;
        }
        if index != 0 || mode == AssemblyRefresh::Revert {
            remap_entity_references(&mut node, &identities);
        }
        *scene
            .get_mut(id)
            .ok_or("The Assembly instance changed during the refresh.")? = node;
    }
    Ok(())
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
            let file = normalized_assembly_source(arg("file")?)?;
            let path = local_asset(root, &file, false)?;
            let prefab = ScenePrefab::capture(ctx.scene, resolve(ctx.scene, arg("target")?)?)?;
            let source = ron::ser::to_string_pretty(&prefab, ron::ser::PrettyConfig::default())
                .map_err(|e| e.to_string())?;
            if source.len() > 8 * 1024 * 1024 {
                return Err("Assembly source exceeds 8MiB.".into());
            }
            if !preview {
                let mut output = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .map_err(|e| e.to_string())?;
                output.write_all(source.as_bytes()).map_err(|e| format!("Could not finish {}: {e}. A partial new file may remain; no existing asset was overwritten.", path.display()))?;
            }
            serde_json::json!({"file": file, "nodes": prefab.nodes.len(), "source_hash": assembly_source_hash(source.as_bytes()), "preview": preview, "file_undo": false})
        }
        "prefab.instantiate" => {
            let root = ctx.assets_root.ok_or("No assets root")?;
            let loaded = load_assembly(root, arg("file")?)?;
            let parent = command
                .arg("parent")
                .filter(|p| !p.is_empty())
                .map(|p| resolve(ctx.scene, p))
                .transpose()?;
            let id = loaded.prefab.instantiate(ctx.scene, parent)?;
            let node = ctx.scene.get_mut(id).expect("created entity");
            node.linked_assembly = Some(LinkedAssembly {
                source: loaded.source.clone(),
                version: loaded.prefab.version,
                source_hash: Some(loaded.source_hash.clone()),
            });
            node.source_asset = Some(loaded.source.clone());
            node.source_schema_version = Some(loaded.prefab.version);
            serde_json::json!({
                "file": loaded.source,
                "nodes": loaded.prefab.nodes.len(),
                "dependencies": loaded.dependencies.sources.len().saturating_sub(1),
                "source_hash": loaded.source_hash,
                "root": ctx.scene.get(id).expect("created entity").uuid,
                "preview": preview
            })
        }
        "assembly.revert" | "assembly.update" | "assembly.make_unique" => {
            let target = assembly_target(ctx.scene, command)?;
            let link = assembly_link(ctx.scene, target)?;
            if command.name == "assembly.make_unique" {
                let cleared = clear_assembly_links(ctx.scene, target, &link.source);
                let uuid = ctx.scene.get(target).expect("resolved target").uuid;
                serde_json::json!({
                    "target": uuid,
                    "source": link.source,
                    "cleared": cleared,
                    "root_only": false
                })
            } else {
                let assets_root = ctx.assets_root.ok_or("No assets root")?;
                let loaded = load_assembly(assets_root, &link.source)?;
                let mode = if command.name == "assembly.revert" {
                    AssemblyRefresh::Revert
                } else {
                    AssemblyRefresh::Update
                };
                replace_assembly_subtree(
                    ctx.scene,
                    target,
                    &loaded.prefab,
                    mode,
                    &loaded.source,
                    &loaded.source_hash,
                )?;
                let node = ctx.scene.get(target).expect("refreshed target");
                serde_json::json!({
                    "target": node.uuid,
                    "source": loaded.source,
                    "version": loaded.prefab.version,
                    "nodes": loaded.prefab.nodes.len(),
                    "dependencies": loaded.dependencies.sources.len().saturating_sub(1),
                    "source_hash": loaded.source_hash,
                    "mode": command.name,
                    "root_preserved": true,
                    "children_replaced": true,
                    "root_only_overrides": command.name == "assembly.update"
                })
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::output::CommandLevel;
    use glam::Vec3;
    use raf_core::scene::{NodeColor, Primitive};

    fn command(name: &str, args: &[(&str, String)]) -> ParsedCommand {
        ParsedCommand {
            raw: format!("/{name}"),
            name: name.into(),
            args: args
                .iter()
                .map(|(key, value)| ((*key).to_string(), value.clone()))
                .collect(),
            positional: Vec::new(),
            structured_args: None,
        }
    }

    struct TempProject(PathBuf);

    impl TempProject {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("raf-assembly-{}", Uuid::new_v4()));
            fs::create_dir_all(root.join("assets/parts")).unwrap();
            Self(root)
        }

        fn assets(&self) -> PathBuf {
            self.0.join("assets")
        }
    }

    impl Drop for TempProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write_prefab(path: &Path, prefab: &ScenePrefab) {
        fs::write(
            path,
            ron::ser::to_string_pretty(prefab, ron::ser::PrettyConfig::default()).unwrap(),
        )
        .unwrap();
    }

    fn instance(scene: &mut SceneGraph, source: &str) -> SceneNodeId {
        let root = scene.add_root_with_primitive("Custom Rig", Primitive::Empty);
        {
            let node = scene.get_mut(root).unwrap();
            node.position = Vec3::new(3.0, 4.0, 5.0);
            node.rotation = Vec3::new(10.0, 20.0, 30.0);
            node.color = NodeColor::rgb(9, 8, 7);
            node.visible = false;
            node.locked = true;
            node.set_variable("speed", VariableValue::Number(2.0));
            node.linked_assembly = Some(LinkedAssembly {
                source: source.into(),
                version: 1,
                source_hash: None,
            });
        }
        scene.add_child_with_primitive(root, "Old Child", Primitive::Torus);
        root
    }

    #[test]
    fn make_unique_clears_matching_links_and_preserves_values() {
        let mut scene = SceneGraph::new();
        let root = scene.add_root("Rig");
        let same = scene.add_child(root, "Bolt");
        let other = scene.add_child(root, "Nested");
        let deep = scene.add_child(other, "Deep");
        let untouched = scene.add_child(root, "Plain");
        let link = |source: &str| {
            Some(LinkedAssembly {
                source: source.into(),
                version: 1,
                source_hash: None,
            })
        };
        scene.get_mut(root).unwrap().linked_assembly = link("parts/rig.prefab.ron");
        scene.get_mut(same).unwrap().linked_assembly = link("parts/rig.prefab.ron");
        scene.get_mut(other).unwrap().linked_assembly = link("parts/other.prefab.ron");
        scene.get_mut(deep).unwrap().linked_assembly = link("parts/rig.prefab.ron");
        scene.get_mut(root).unwrap().position = Vec3::new(1.0, 2.0, 3.0);
        scene
            .get_mut(root)
            .unwrap()
            .set_variable("mode", VariableValue::Text("keep".into()));
        let uuid = scene.get(root).unwrap().uuid;

        let mut context = ScriptCommandContext {
            scene: &mut scene,
            assets_root: None,
            editor_view: None,
        };
        let output = execute(
            &command("assembly.make_unique", &[("target", uuid.to_string())]),
            &mut context,
            false,
        );
        assert_eq!(output.level, CommandLevel::Info);
        assert!(output.changed);
        assert_eq!(output.json["cleared"], serde_json::json!(3));
        assert!(context.scene.get(root).unwrap().linked_assembly.is_none());
        assert!(context.scene.get(same).unwrap().linked_assembly.is_none());
        assert!(context.scene.get(deep).unwrap().linked_assembly.is_none());
        assert!(context.scene.get(other).unwrap().linked_assembly.is_some());
        assert!(context
            .scene
            .get(untouched)
            .unwrap()
            .linked_assembly
            .is_none());
        assert_eq!(
            context.scene.get(root).unwrap().position,
            Vec3::new(1.0, 2.0, 3.0)
        );
        assert_eq!(
            context.scene.get(root).unwrap().get_variable("mode"),
            Some(&VariableValue::Text("keep".into()))
        );
    }

    #[test]
    fn revert_and_update_reuse_the_instance_and_replace_children() {
        let project = TempProject::new();
        let source_path = project.assets().join("parts/rig.prefab.ron");

        let mut source_scene = SceneGraph::new();
        let source_root = source_scene.add_root_with_primitive("Source Rig", Primitive::Cube);
        let bolt = source_scene.add_child_with_primitive(source_root, "Bolt", Primitive::Sphere);
        source_scene.get_mut(bolt).unwrap().position = Vec3::new(0.0, 0.5, 0.0);
        let mut source_prefab = ScenePrefab::capture(&source_scene, source_root).unwrap();
        write_prefab(&source_path, &source_prefab);

        let mut scene = SceneGraph::new();
        let root = instance(&mut scene, "parts/rig.prefab.ron");
        let uuid = scene.get(root).unwrap().uuid;
        let slots_before = scene.len();
        let assets = project.assets();
        let mut context = ScriptCommandContext {
            scene: &mut scene,
            assets_root: Some(&assets),
            editor_view: None,
        };
        let output = execute(
            &command("assembly.revert", &[("target", uuid.to_string())]),
            &mut context,
            false,
        );
        assert_eq!(output.level, CommandLevel::Info, "{:?}", output.lines);
        assert!(output.changed);
        assert_eq!(context.scene.len(), slots_before);
        let node = context.scene.get(root).unwrap();
        assert_eq!(node.uuid, uuid);
        assert_eq!(node.name, "Custom Rig");
        assert_eq!(node.position, Vec3::new(3.0, 4.0, 5.0));
        assert_eq!(node.rotation, Vec3::new(10.0, 20.0, 30.0));
        assert_eq!(node.color, NodeColor::rgb(9, 8, 7));
        assert!(!node.visible);
        assert!(node.locked);
        assert_eq!(
            node.primitive,
            Primitive::Cube,
            "revert applies source components"
        );
        assert_eq!(node.children.len(), 1);
        let child = context.scene.get(node.children[0]).unwrap();
        assert_eq!(child.name, "Bolt");
        assert_eq!(child.primitive, Primitive::Sphere);
        let link = node.linked_assembly.as_ref().unwrap();
        assert_eq!(link.source, "parts/rig.prefab.ron");
        assert_eq!(link.version, 1);
        assert!(link.source_hash.as_deref().unwrap().starts_with("fnv1a64:"));
        assert_eq!(
            context.scene.get(root).unwrap().get_variable("speed"),
            Some(&VariableValue::Number(2.0))
        );

        // Add a child to the source. Update replaces children but keeps every
        // instance-root override, including the root primitive.
        let mut washer = source_prefab.nodes[0].clone();
        washer.uuid = Uuid::new_v4();
        washer.name = "Washer".into();
        washer.parent = Some(raf_core::scene::SceneNodeId(0));
        washer.children.clear();
        washer.position = Vec3::new(0.0, 1.0, 0.0);
        source_prefab.nodes.push(washer);
        let added = source_prefab.nodes.len() - 1;
        source_prefab.nodes[0]
            .children
            .push(raf_core::scene::SceneNodeId(added));
        source_prefab.validate().unwrap();
        write_prefab(&source_path, &source_prefab);
        let slots_before = context.scene.len();
        let output = execute(
            &command("assembly.update", &[("target", uuid.to_string())]),
            &mut context,
            false,
        );
        assert_eq!(output.level, CommandLevel::Info, "{:?}", output.lines);
        assert!(output.changed);
        assert_eq!(context.scene.len(), slots_before + 1);
        let node = context.scene.get(root).unwrap();
        assert_eq!(node.name, "Custom Rig");
        assert_eq!(node.primitive, Primitive::Cube);
        let names = node
            .children
            .iter()
            .map(|child| context.scene.get(*child).unwrap().name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["Bolt", "Washer"]);

        context.scene.get_mut(root).unwrap().linked_assembly = Some(LinkedAssembly {
            source: "parts/missing.prefab.ron".into(),
            version: 1,
            source_hash: None,
        });
        let output = execute(
            &command("assembly.revert", &[("target", uuid.to_string())]),
            &mut context,
            false,
        );
        assert_eq!(output.level, CommandLevel::Error);
        assert!(
            output.lines[0].contains("missing or unavailable"),
            "{:?}",
            output.lines
        );
        assert_eq!(
            context
                .scene
                .get(root)
                .unwrap()
                .linked_assembly
                .as_ref()
                .unwrap()
                .source,
            "parts/missing.prefab.ron",
            "a failed revert must leave the instance unchanged"
        );
    }
}
