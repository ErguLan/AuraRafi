//! Attached-editor commands and editor bridge calls.

use crate::attached::AttachedClient;
use crate::cli::args::{named_command_request, observation_command_request, Options, OutputFormat};
use crate::cli::commands::run_command_request;
use crate::cli::output::print_response;
use crate::cli::CliResult;
use crate::discovery;
use raf_core::{CommandEndpoint, CommandSource, EngineCommandRequest, EngineCommandResponse};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(super) fn execute_task_cli<H: CommandEndpoint>(
    engine: &mut H,
    options: &Options,
    subcommand_index: usize,
) -> CliResult<EngineCommandResponse> {
    let subcommand = options
        .positionals
        .get(subcommand_index)
        .map(String::as_str)
        .unwrap_or("list");
    let (name, params) = match subcommand {
        "list" => ("task.list", json!({})),
        "get" | "cancel" => {
            let id = options
                .value("id")
                .map(str::to_string)
                .or_else(|| options.positionals.get(subcommand_index + 1).cloned())
                .ok_or_else(|| format!("task {subcommand} requires a task id."))?;
            (
                if subcommand == "get" {
                    "task.get"
                } else {
                    "task.cancel"
                },
                json!({"id": id}),
            )
        }
        "events" => {
            let since = options
                .value("since")
                .map(str::to_string)
                .or_else(|| options.positionals.get(subcommand_index + 1).cloned())
                .unwrap_or_else(|| "0".to_string())
                .parse::<u64>()
                .map_err(|_| "task events requires an unsigned sequence.".to_string())?;
            ("task.events", json!({"since": since}))
        }
        other => return Err(format!("Unknown task command: {other}.")),
    };
    Ok(engine.execute(EngineCommandRequest::new(name, params, CommandSource::Cli)))
}

pub(super) fn run_attached_cli(options: &Options, format: OutputFormat) -> CliResult<()> {
    let explicit_path = options.path("project").or_else(|| {
        options.positionals.get(1).and_then(|value| {
            // A bare positional is only a project path when it exists on
            // disk; anything else is an attached action name.
            let path = PathBuf::from(value);
            let known_action = matches!(
                value.as_str(),
                "command"
                    | "status"
                    | "engine.status"
                    | "context"
                    | "engine.context"
                    | "capabilities"
                    | "scene"
                    | "selection"
                    | "assets"
                    | "viewport"
                    | "task"
                    | "scripts"
                    | "health"
                    | "project.health"
                    | "project"
                    | "project.info"
                    | "save"
                    | "checkpoint"
                    | "session"
                    | "workspace"
            );
            (path.exists() && !known_action).then_some(path)
        })
    });
    let project_path = match explicit_path {
        Some(path) => path,
        None => discovery::pick_single_live(None)?.project_path,
    };
    let mut client = AttachedClient::connect(&project_path, "raf-cli")?;
    let response = match options.positionals.get(1).map(String::as_str) {
        Some("command") => run_command_request(&mut client, options)?,
        Some("status") | Some("engine.status") => client.execute(EngineCommandRequest::new(
            "engine.status",
            json!({}),
            CommandSource::Cli,
        )),
        Some("capabilities") => {
            let query = if options.positionals.get(2).map(String::as_str) == Some("search") {
                options.positionals[3..].join(" ")
            } else {
                String::new()
            };
            let name = if query.is_empty() {
                "capabilities.list"
            } else {
                "capabilities.search"
            };
            client.execute(EngineCommandRequest::new(
                name,
                json!({"query": query}),
                CommandSource::Cli,
            ))
        }
        Some("project") => match options.positionals.get(2).map(String::as_str) {
            Some("save" | "checkpoint") => {
                client.execute(project_save_request(options)?)
            }
            Some("open") => {
                let path = options
                    .positionals
                    .get(3)
                    .cloned()
                    .or_else(|| options.value("path").map(str::to_string))
                    .ok_or_else(|| "raf attach project open requires a path.".to_string())?;
                client.execute(EngineCommandRequest::new(
                    "project.open",
                    json!({"path": path}),
                    CommandSource::Cli,
                ))
            }
            _ => client.execute(EngineCommandRequest::new(
                "project.info",
                json!({}),
                CommandSource::Cli,
            )),
        },
        Some("project.info") => client.execute(EngineCommandRequest::new(
            "project.info",
            json!({}),
            CommandSource::Cli,
        )),
        Some("save") | Some("checkpoint") => {
            client.execute(project_save_request(options)?)
        }
        Some("session") if options.positionals.get(2).map(String::as_str) == Some("list") => client
            .execute(EngineCommandRequest::new(
                "session.list",
                json!({}),
                CommandSource::Cli,
            )),
        Some("workspace") if options.positionals.get(2).map(String::as_str) == Some("describe") => {
            client.execute(EngineCommandRequest::new(
                "workspace.describe",
                json!({}),
                CommandSource::Cli,
            ))
        }
        Some("context") | Some("engine.context") => client.execute(EngineCommandRequest::new(
            "engine.context",
            json!({}),
            CommandSource::Cli,
        )),
        Some("viewport") => match options.positionals.get(2).map(String::as_str) {
            Some("capture") | None => {
                client.execute(observation_command_request(options, "viewport.capture", json!({}))?)
            }
            Some(other) => return Err(format!("Unknown attached viewport command: {other}.")),
        },
        Some("task") => execute_task_cli(
            &mut client,
            &options,
            2,
        )?,
        Some("scene") => {
            let subcommand = options.positionals.get(2).map(String::as_str).unwrap_or("outline");
            match subcommand {
                "outline" => client.execute(observation_command_request(options, "scene.outline", json!({}))?),
                "digest" | "snapshot" => client.execute(observation_command_request(
                    options,
                    "scene.digest",
                    json!({"check_collisions": options.has("collisions")}),
                )?),
                "map" | "top-down" | "top_down" => {
                    let mut request =
                        observation_command_request(options, "viewport.capture", json!({}))?;
                    if let Some(object) = request.params.as_object_mut() {
                        object
                            .entry("preset".to_string())
                            .or_insert_with(|| json!("top_down_map"));
                    }
                    client.execute(request)
                }
                "visual-review" | "vision" => {
                    client.execute(named_command_request(options, "scene.visual_review", false)?)
                }
                "program" | "plan-world" => client.execute(named_command_request(options, "game.plan", false)?),
                "commit" => client.execute(named_command_request(options, "game.commit", false)?),
                "cancel" | "cancel-plan" => {
                    client.execute(named_command_request(options, "game.cancel", false)?)
                }
                "brief" => client.execute(named_command_request(options, "world.brief", false)?),
                "recipe" => client.execute(named_command_request(options, "world.recipe", false)?),
                "script" | "authoring-script" => {
                    client.execute(named_command_request(options, "world.script", false)?)
                }
                "visual-diff" | "image-diff" => {
                    client.execute(named_command_request(options, "scene.visual_diff", false)?)
                }
                "spatial" | "spatial-map" => client.execute(observation_command_request(
                    options,
                    "scene.spatial_map",
                    json!({}),
                )?),
                "design" | "design-audit" | "layout-audit" => client.execute(
                    observation_command_request(options, "scene.design_audit", json!({}))?,
                ),
                "navigation" | "reachability" => client.execute(
                    observation_command_request(options, "scene.navigation_audit", json!({}))?,
                ),
                "query" => client.execute(observation_command_request(
                    options,
                    "scene.query",
                    json!({"query": options.value("query").map(str::to_string).unwrap_or_else(|| options.positionals.get(3..).map(|values| values.join(" ")).unwrap_or_default())}),
                )?),
                "inspect" => client.execute(observation_command_request(
                    options,
                    "scene.inspect",
                    json!({"target": options.value("target").map(str::to_string).or_else(|| options.positionals.get(3).cloned())}),
                )?),
                "selection" => client.execute(observation_command_request(options, "selection.get", json!({}))?),
                "verify" => client.execute(observation_command_request(
                    options,
                    "scene.verify",
                    json!({"target": options.value("target").map(str::to_string).or_else(|| options.positionals.get(3).cloned())}),
                )?),
                "create" => client.execute(named_command_request(options, "game.add", false)?),
                "update" => client.execute(named_command_request(options, "game.update", false)?),
                "build" => client.execute(named_command_request(options, "game.build", false)?),
                "reconcile" => {
                    client.execute(named_command_request(options, "game.reconcile", false)?)
                }
                "repair" => client.execute(named_command_request(options, "game.repair", false)?),
                "plan" => client.execute(named_command_request(options, "game.build", true)?),
                "apply" => client.execute(named_command_request(options, "game.build", false)?),
                "batch" => client.execute(named_command_request(options, "game.batch", false)?),
                other => return Err(format!("Unknown attached scene command: {other}.")),
            }
        }
        Some("selection") => client.execute(observation_command_request(options, "selection.get", json!({}))?),
        Some("assets")
            if matches!(
                options.positionals.get(2).map(String::as_str),
                Some("search" | "inspect")
            ) =>
        {
            let subcommand = options.positionals.get(2).map(String::as_str).unwrap_or("search");
            let name = if subcommand == "inspect" {
                "assets.inspect"
            } else {
                "assets.catalog"
            };
            let value = options
                .value(if subcommand == "inspect" { "target" } else { "query" })
                .map(str::to_string)
                .or_else(|| {
                    options
                        .positionals
                        .get(3..)
                        .map(|values| values.join(" "))
                })
                .unwrap_or_default();
            client.execute(observation_command_request(
                options,
                name,
                if subcommand == "inspect" {
                    json!({"target": value})
                } else {
                    json!({"query": value})
                },
            )?)
        }
        Some("scripts") if options.positionals.get(2).map(String::as_str) == Some("search") => {
            client.execute(observation_command_request(
                options,
                "scripts.catalog",
                json!({"query": options.value("query").map(str::to_string).or_else(|| options.positionals.get(3..).map(|values| values.join(" "))).unwrap_or_default()}),
            )?)
        }
        Some("health") | Some("project.health") => {
            client.execute(observation_command_request(options, "project.health", json!({}))?)
        }
        Some(other) => return Err(format!("Unknown attached command: {other}.")),
        None => client.execute(EngineCommandRequest::new(
            "engine.status",
            json!({}),
            CommandSource::Cli,
        )),
    };
    print_response(&response, format)
}

fn editor_binary_path() -> CliResult<PathBuf> {
    if let Some(path) = std::env::var_os("AURA_RAFI_EDITOR") {
        return Ok(PathBuf::from(path));
    }
    let current = std::env::current_exe()
        .map_err(|error| format!("Could not resolve the current CLI executable: {error}"))?;
    let name = if cfg!(windows) {
        "aura_rafi_editor.exe"
    } else {
        "aura_rafi_editor"
    };
    let candidate = current
        .parent()
        .map(|parent| parent.join(name))
        .ok_or_else(|| "The CLI executable has no parent directory.".to_string())?;
    if candidate.is_file() {
        Ok(candidate)
    } else {
        Err(format!(
            "Could not find the editor binary next to the CLI ({}). Set AURA_RAFI_EDITOR to its path.",
            candidate.display()
        ))
    }
}

fn editor_is_live(path: &Path) -> bool {
    let target = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    discovery::discover_editors(Some(path))
        .iter()
        .any(|editor| {
            editor.alive
                && editor
                    .project_path
                    .canonicalize()
                    .unwrap_or_else(|_| editor.project_path.clone())
                    == target
        })
}

pub(super) fn launch_editor_if_needed(path: &Path) -> CliResult<()> {
    if editor_is_live(path) {
        return Ok(());
    }
    let editor = editor_binary_path()?;
    // MCP owns stdin/stdout for JSON-RPC. A launched editor must never inherit
    // those handles: its tracing output uses ANSI colors and corrupts the
    // protocol stream for the whole session.
    std::process::Command::new(&editor)
        .arg("--project")
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| format!("Could not launch {}: {error}", editor.display()))?;
    for _ in 0..60 {
        std::thread::sleep(Duration::from_millis(250));
        if editor_is_live(path) {
            return Ok(());
        }
    }
    Err(format!(
        "The editor started but did not publish a live endpoint for {} within 15 seconds.",
        path.display()
    ))
}

pub(super) fn open_editor_project(options: &Options) -> CliResult<()> {
    let path = options
        .positionals
        .get(1)
        .map(PathBuf::from)
        .or_else(|| options.path("project"))
        .ok_or_else(|| "raf open requires a project path.".to_string())?;
    if !path.is_dir() {
        return Err(format!(
            "Project path does not exist or is not a directory: {}",
            path.display()
        ));
    }
    launch_editor_if_needed(&path)?;
    println!("Editor ready for {}", path.display());
    Ok(())
}

pub(super) fn project_save_request(options: &Options) -> CliResult<EngineCommandRequest> {
    let mut request = named_command_request(options, "project.save", false)?;
    if options.has("wait") {
        if let Some(object) = request.params.as_object_mut() {
            object.insert("wait".to_string(), json!(true));
        }
    }
    Ok(request)
}
