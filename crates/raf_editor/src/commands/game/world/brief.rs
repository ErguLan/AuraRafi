//! Project world brief and reusable recipes.

use super::kits::slugify;
use super::plan::{append_journal, brief_path, load_planned_world, world_directory};
use super::super::query::{parsed_command_from_params, text_argument};
use super::*;

pub(crate) fn execute_world_brief(request: &ParsedCommand) -> CommandOutput {
    let result = (|| -> Result<CommandOutput, String> {
        let Some(root) = active_project_root() else {
            return Err(
                "The world brief requires an active project root. Open the project first.".to_string(),
            );
        };
        let path = brief_path(&root);
        let action = text_argument(request, "action")
            .or_else(|| {
                request
                    .name
                    .strip_suffix(".write")
                    .map(|_| "write".to_string())
            })
            .or_else(|| {
                request
                    .name
                    .strip_suffix(".read")
                    .map(|_| "read".to_string())
            })
            .unwrap_or_else(|| "read".to_string())
            .to_ascii_lowercase();
        match action.as_str() {
            "read" | "get" | "show" => {
                let content = std::fs::read(&path)
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes).ok())
                    .unwrap_or_default();
                let brief = if content.trim().is_empty() {
                    Value::Null
                } else {
                    serde_json::from_str(&content).unwrap_or_else(|_| Value::String(content.clone()))
                };
                Ok(CommandOutput::info(
                    "World brief",
                    vec![format!(
                        "brief: {}",
                        if content.is_empty() { "not set" } else { "loaded" }
                    )],
                    serde_json::json!({
                        "ok": true,
                        "found": !content.is_empty(),
                        "path": path.strip_prefix(&root).unwrap_or(&path).to_string_lossy().replace('\\', "/"),
                        "brief": brief,
                    }),
                ))
            }
            "write" | "set" | "save" => {
                let brief = request
                    .structured_arg("brief")
                    .cloned()
                    .or_else(|| request.structured_arg("content").cloned())
                    .or_else(|| {
                        request
                            .arg("brief")
                            .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                    })
                    .or_else(|| request.arg("content").map(|raw| Value::String(raw.to_string())))
                    .ok_or_else(|| "world.brief write requires a brief object or content string.".to_string())?;
                if brief.to_string().len() > 64 * 1024 {
                    return Err("World brief must be at most 64 KiB.".to_string());
                }
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|error| format!("Could not create the world directory: {error}"))?;
                }
                let encoded = serde_json::to_vec_pretty(&brief)
                    .map_err(|error| format!("Could not encode the world brief: {error}"))?;
                std::fs::write(&path, encoded)
                    .map_err(|error| format!("Could not write the world brief: {error}"))?;
                append_journal(
                    &root,
                    serde_json::json!({"event": "brief_updated", "path": "world/brief.json"}),
                );
                Ok(CommandOutput::changed(
                    "World brief saved",
                    vec![format!(
                        "path: {}",
                        path.strip_prefix(&root).unwrap_or(&path).to_string_lossy().replace('\\', "/")
                    )],
                    serde_json::json!({
                        "ok": true,
                        "saved": true,
                        "path": path.strip_prefix(&root).unwrap_or(&path).to_string_lossy().replace('\\', "/"),
                        "brief": brief,
                    }),
                ))
            }
            other => Err(format!(
                "Unknown world.brief action '{other}'. Use read or write."
            )),
        }
    })();
    result.unwrap_or_else(|error| CommandOutput::error("World brief", error))
}

// ---------------------------------------------------------------------------
// Reusable recipes captured from committed plans
// ---------------------------------------------------------------------------

fn recipes_directory(root: &Path) -> PathBuf {
    world_directory(root).join("recipes")
}

fn recipe_slug(value: &str) -> Result<String, String> {
    let slug = slugify(value);
    if slug.is_empty() {
        return Err("Recipe name must contain at least one letter or digit.".to_string());
    }
    if slug.len() > 64 {
        return Err("Recipe name must be at most 64 characters.".to_string());
    }
    Ok(slug)
}

fn load_recipe(root: &Path, name: &str) -> Result<(PathBuf, Value), String> {
    let slug = recipe_slug(name)?;
    let path = recipes_directory(root).join(format!("{slug}.json"));
    let bytes = std::fs::read(&path).map_err(|error| {
        format!("Recipe '{slug}' was not found in the project: {error}")
    })?;
    let recipe = serde_json::from_slice::<Value>(&bytes)
        .map_err(|error| format!("Recipe '{slug}' is not valid JSON: {error}"))?;
    if !recipe.is_object() {
        return Err(format!("Recipe '{slug}' must be a JSON object."));
    }
    Ok((path, recipe))
}

fn recipe_summary(slug: &str, recipe: &Value) -> Value {
    serde_json::json!({
        "name": slug,
        "title": recipe.get("name").cloned().unwrap_or(Value::String(slug.to_string())),
        "description": recipe.get("description").cloned().unwrap_or(Value::Null),
        "source_plan_id": recipe.get("source_plan_id").cloned().unwrap_or(Value::Null),
        "mode": recipe.get("mode").cloned().unwrap_or(Value::String("build".to_string())),
        "groups": recipe.get("groups").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
        "entities": recipe.get("entities").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
        "tags": recipe.get("tags").cloned().unwrap_or(Value::Array(Vec::new())),
    })
}

pub(crate) fn execute_world_recipe(
    request: &ParsedCommand,
    ctx: &mut GameCommandContext<'_>,
) -> CommandOutput {
    let result = (|| -> Result<CommandOutput, String> {
        let root = active_project_root()
            .ok_or_else(|| "World recipes require an active project root.".to_string())?;
        let action = text_argument(request, "action")
            .unwrap_or_else(|| "list".to_string())
            .to_ascii_lowercase();
        match action.as_str() {
            "save" | "capture" => {
                let name = text_argument(request, "name")
                    .or_else(|| text_argument(request, "recipe"))
                    .ok_or_else(|| "world.recipe save requires name.".to_string())?;
                let slug = recipe_slug(&name)?;
                let plan_id = text_argument(request, "plan_id")
                    .ok_or_else(|| "world.recipe save requires plan_id.".to_string())?;
                let plan = load_planned_world(&plan_id)?;
                let source_plan_id = plan.id.clone();
                let recipe = serde_json::json!({
                    "version": 1,
                    "name": name,
                    "slug": slug,
                    "description": text_argument(request, "description").unwrap_or_default(),
                    "source_plan_id": source_plan_id,
                    "mode": plan.mode,
                    "parent": plan.parent,
                    "design_profile": plan.design_profile,
                    "groups": plan.groups,
                    "entities": plan.entities,
                    "updates": plan.updates,
                    "deletions": plan.deletions,
                    "tags": request.structured_arg("tags").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
                });
                let encoded = serde_json::to_vec_pretty(&recipe)
                    .map_err(|error| format!("Could not encode recipe '{slug}': {error}"))?;
                if encoded.len() > 8 * 1024 * 1024 {
                    return Err("Recipe exceeds the 8 MiB project budget.".to_string());
                }
                let directory = recipes_directory(&root);
                std::fs::create_dir_all(&directory)
                    .map_err(|error| format!("Could not create the recipe directory: {error}"))?;
                let path = directory.join(format!("{slug}.json"));
                std::fs::write(&path, encoded)
                    .map_err(|error| format!("Could not write recipe '{slug}': {error}"))?;
                append_journal(
                    &root,
                    serde_json::json!({"event": "recipe_saved", "name": slug, "source_plan_id": source_plan_id}),
                );
                Ok(CommandOutput::changed(
                    "World recipe saved",
                    vec![
                        format!("recipe: {slug}"),
                        format!("source plan: {source_plan_id}"),
                    ],
                    serde_json::json!({
                        "ok": true,
                        "saved": true,
                        "recipe": recipe_summary(&slug, &recipe),
                        "path": path.strip_prefix(&root).unwrap_or(&path).to_string_lossy().replace('\\', "/"),
                    }),
                ))
            }
            "list" => {
                let directory = recipes_directory(&root);
                let mut recipes = Vec::new();
                if let Ok(entries) = std::fs::read_dir(&directory) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                            continue;
                        }
                        let Some(slug) = path.file_stem().and_then(|stem| stem.to_str()) else {
                            continue;
                        };
                        let Ok(bytes) = std::fs::read(&path) else {
                            continue;
                        };
                        let Ok(recipe) = serde_json::from_slice::<Value>(&bytes) else {
                            continue;
                        };
                        recipes.push(recipe_summary(slug, &recipe));
                    }
                }
                recipes.sort_by(|left, right| {
                    left.get("name")
                        .and_then(Value::as_str)
                        .cmp(&right.get("name").and_then(Value::as_str))
                });
                recipes.truncate(128);
                let count = recipes.len();
                Ok(CommandOutput::info(
                    "World recipes",
                    vec![format!("recipes: {count}")],
                    serde_json::json!({
                        "ok": true,
                        "count": count,
                        "recipes": recipes,
                    }),
                ))
            }
            "show" | "read" => {
                let name = text_argument(request, "name")
                    .or_else(|| text_argument(request, "recipe"))
                    .ok_or_else(|| "world.recipe show requires name.".to_string())?;
                let (path, recipe) = load_recipe(&root, &name)?;
                let slug = recipe_slug(&name)?;
                Ok(CommandOutput::info(
                    "World recipe",
                    vec![format!("recipe: {slug}")],
                    serde_json::json!({
                        "ok": true,
                        "recipe": recipe,
                        "path": path.strip_prefix(&root).unwrap_or(&path).to_string_lossy().replace('\\', "/"),
                    }),
                ))
            }
            "delete" | "remove" => {
                let name = text_argument(request, "name")
                    .or_else(|| text_argument(request, "recipe"))
                    .ok_or_else(|| "world.recipe delete requires name.".to_string())?;
                let slug = recipe_slug(&name)?;
                let path = recipes_directory(&root).join(format!("{slug}.json"));
                let existed = path.exists();
                if existed {
                    std::fs::remove_file(&path)
                        .map_err(|error| format!("Could not remove recipe '{slug}': {error}"))?;
                    append_journal(
                        &root,
                        serde_json::json!({"event": "recipe_deleted", "name": slug}),
                    );
                }
                Ok(CommandOutput {
                    changed: existed,
                    ..CommandOutput::info(
                        "World recipe deleted",
                        vec![format!("recipe: {slug}")],
                        serde_json::json!({
                            "ok": true,
                            "existed": existed,
                            "name": slug,
                        }),
                    )
                })
            }
            "apply" | "instantiate" => {
                let name = text_argument(request, "name")
                    .or_else(|| text_argument(request, "recipe"))
                    .ok_or_else(|| "world.recipe apply requires name.".to_string())?;
                let slug = recipe_slug(&name)?;
                let (_path, recipe) = load_recipe(&root, &slug)?;
                let program = serde_json::json!({
                    "version": 1,
                    "mode": recipe.get("mode").cloned().unwrap_or_else(|| Value::String("build".to_string())),
                    "parent": recipe.get("parent").cloned().unwrap_or(Value::Null),
                    "design_profile": recipe.get("design_profile").cloned().unwrap_or(Value::Null),
                    "groups": recipe.get("groups").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
                    "entities": recipe.get("entities").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
                    "updates": recipe.get("updates").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
                    "deletions": recipe.get("deletions").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
                });
                let plan_request = parsed_command_from_params(
                    "game.plan",
                    serde_json::json!({"program": program}),
                );
                let mut output = execute_plan(&plan_request, ctx);
                if matches!(
                    output.level,
                    crate::commands::output::CommandLevel::Error
                ) {
                    return Ok(output);
                }
                let plan_id = output.json.get("plan_id").cloned().unwrap_or(Value::Null);
                output.lines.insert(0, format!("recipe: {slug}"));
                if let Some(object) = output.json.as_object_mut() {
                    object.insert(
                        "recipe".to_string(),
                        serde_json::json!({
                            "name": slug,
                            "source_plan_id": recipe.get("source_plan_id").cloned().unwrap_or(Value::Null),
                        }),
                    );
                }
                append_journal(
                    &root,
                    serde_json::json!({"event": "recipe_applied", "name": slug, "plan_id": plan_id}),
                );
                Ok(output)
            }
            other => Err(format!(
                "Unknown world.recipe action '{other}'. Use save, list, show, delete or apply."
            )),
        }
    })();
    result.unwrap_or_else(|error| CommandOutput::error("World recipe", error))
}
