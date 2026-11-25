//! Headless command execution and top-level CLI dispatch.

pub(super) mod engine;
mod support;

use crate::attached::AttachedClient;
use crate::cli::args::{
    budget_from_options, build_command_request, named_command_request, observation_command_request,
    parse_options, print_help, Options, OutputFormat,
};
use crate::cli::attach::{
    execute_task_cli, launch_editor_if_needed, open_editor_project, project_save_request,
    run_attached_cli,
};
use crate::cli::mcp::{run_mcp_stdio, McpHost};
use crate::cli::output::{print_editors, print_project_path, print_response};
use crate::cli::{CliResult, PROGRAM};
use crate::discovery;
use raf_core::{
    serve_lines, CommandEndpoint, CommandSource, EngineCommandRequest, EngineCommandResponse,
};
use self::engine::HeadlessEngine;
use serde_json::json;
use std::env;
use std::io::{self, BufWriter};
use std::path::PathBuf;

pub(super) fn run_command_request<E: CommandEndpoint>(
    engine: &mut E,
    options: &Options,
) -> CliResult<EngineCommandResponse> {
    let request = build_command_request(options)?;
    Ok(engine.execute(request))
}

pub(crate) fn run_cli(arguments: Vec<String>) -> CliResult<()> {    let options = parse_options(&arguments);
    let format = OutputFormat::from_options(&options);
    let command = options
        .positionals
        .first()
        .map(String::as_str)
        .unwrap_or("help");
    let project_path = options.path("project");
    if options.has("help")
        || options
            .positionals
            .iter()
            .any(|argument| matches!(argument.as_str(), "--help" | "-h"))
    {
        print_help();
        return Ok(());
    }
    if command == "attach" {
        return run_attached_cli(&options, format);
    }
    if command == "open" {
        return open_editor_project(&options);
    }
    if command == "project" && options.positionals.get(1).map(String::as_str) == Some("path") {
        return print_project_path(&options, format);
    }
    if command == "mcp" && options.has("attach") {
        let attach_path = match options.path("attach").or_else(|| options.path("project")) {
            Some(path) => path,
            None => discovery::pick_single_live(None)?.project_path,
        };
        if options.has("launch") {
            launch_editor_if_needed(&attach_path)?;
        }
        let client = AttachedClient::connect(&attach_path, "raf-mcp")?;
        return run_mcp_stdio(client);
    }
    // Without an explicit project the MCP server starts unattached: it never
    // launches or opens a project, and resolves the live editor per call via
    // raf_editors / raf_attach or a single-live-editor auto-attach.
    if command == "mcp" && !options.has("project") {
        return run_mcp_stdio(McpHost::unattached());
    }
    let mut engine = HeadlessEngine::new(project_path.as_deref())?;

    let response = match command {
        "help" | "--help" | "-h" => {
            print_help();
            return Ok(());
        }
        "editors" | "ls" => {
            let wait = options
                .value("wait")
                .and_then(|raw| raw.parse::<f32>().ok())
                .or_else(|| options.has("wait").then(|| 15.0));
            return print_editors(options.path("project").as_deref(), format, wait);
        }
        "doctor" => engine.execute(EngineCommandRequest::new(
            "engine.doctor",
            json!({}),
            CommandSource::Cli,
        )),
        "status" => engine.execute(EngineCommandRequest::new(
            "engine.status",
            json!({}),
            CommandSource::Cli,
        )),
        "context" => engine.execute(EngineCommandRequest::new(
            "engine.context",
            json!({}),
            CommandSource::Cli,
        )),
        "scene" => {
            let subcommand = options.positionals.get(1).map(String::as_str).unwrap_or("outline");
            match subcommand {
                "outline" => engine.execute(observation_command_request(&options, "scene.outline", json!({}))?),
                "digest" | "snapshot" => engine.execute(observation_command_request(
                    &options,
                    "scene.digest",
                    json!({"check_collisions": options.has("collisions")}),
                )?),
                "spatial" | "spatial-map" => engine.execute(observation_command_request(
                    &options,
                    "scene.spatial_map",
                    json!({}),
                )?),
                "design" | "design-audit" | "layout-audit" => engine.execute(
                    observation_command_request(&options, "scene.design_audit", json!({}))?,
                ),
                "navigation" | "reachability" => engine.execute(
                    observation_command_request(&options, "scene.navigation_audit", json!({}))?,
                ),
                "query" => engine.execute(observation_command_request(
                    &options,
                    "scene.query",
                    json!({"query": options.value("query").map(str::to_string).unwrap_or_else(|| options.positionals.get(2..).map(|values| values.join(" ")).unwrap_or_default())}),
                )?),
                "inspect" => engine.execute(observation_command_request(
                    &options,
                    "scene.inspect",
                    json!({"target": options.value("target").map(str::to_string).or_else(|| options.positionals.get(2).cloned())}),
                )?),
                "selection" => engine.execute(observation_command_request(&options, "selection.get", json!({}))?),
                "verify" => engine.execute(observation_command_request(
                    &options,
                    "scene.verify",
                    json!({"target": options.value("target").map(str::to_string).or_else(|| options.positionals.get(2).cloned())}),
                )?),
                "create" => engine.execute(named_command_request(&options, "game.add", false)?),
                "update" => engine.execute(named_command_request(&options, "game.update", false)?),
                "build" => engine.execute(named_command_request(&options, "game.build", false)?),
                "reconcile" => {
                    engine.execute(named_command_request(&options, "game.reconcile", false)?)
                }
                "repair" => engine.execute(named_command_request(&options, "game.repair", false)?),
                "plan" => engine.execute(named_command_request(&options, "game.build", true)?),
                "apply" => engine.execute(named_command_request(&options, "game.build", false)?),
                "batch" => engine.execute(named_command_request(&options, "game.batch", false)?),
                other => return Err(format!("Unknown scene command: {other}.")),
            }
        }
        "selection" => engine.execute(observation_command_request(&options, "selection.get", json!({}))?),
        "assets"
            if matches!(
                options.positionals.get(1).map(String::as_str),
                Some("search" | "inspect")
            ) =>
        {
            let subcommand = options.positionals.get(1).map(String::as_str).unwrap_or("search");
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
                        .get(2..)
                        .map(|values| values.join(" "))
                })
                .unwrap_or_default();
            engine.execute(observation_command_request(
                &options,
                name,
                if subcommand == "inspect" {
                    json!({"target": value})
                } else {
                    json!({"query": value})
                },
            )?)
        }
        "scripts" if options.positionals.get(1).map(String::as_str) == Some("search") => {
            engine.execute(observation_command_request(
                &options,
                "scripts.catalog",
                json!({"query": options.value("query").map(str::to_string).or_else(|| options.positionals.get(2..).map(|values| values.join(" "))).unwrap_or_default()}),
            )?)
        }
        "health" | "project.health" => {
            engine.execute(observation_command_request(&options, "project.health", json!({}))?)
        }
        "viewport" => match options.positionals.get(1).map(String::as_str) {
            Some("capture") | None => engine.execute(observation_command_request(
                &options,
                "viewport.capture",
                json!({}),
            )?),
            Some(other) => return Err(format!("Unknown viewport command: {other}.")),
        },
        "task" => execute_task_cli(&mut engine, &options, 1)?,
        "capabilities" => {
            let query = if options.positionals.get(1).map(String::as_str) == Some("search") {
                options.positionals[2..].join(" ")
            } else {
                String::new()
            };
            let name = if query.is_empty() {
                "capabilities.list"
            } else {
                "capabilities.search"
            };
            engine.execute(EngineCommandRequest::new(
                name,
                json!({"query": query}),
                CommandSource::Cli,
            ))
        }
        "project" => match options.positionals.get(1).map(String::as_str) {
            Some("create") => {
                let name = options
                    .value("name")
                    .ok_or_else(|| "project create requires --name.".to_string())?;
                let parent = options
                    .value("parent")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
                let project_type = options.value("type").unwrap_or("game");
                let mut request = EngineCommandRequest::new(
                    "project.create",
                    json!({"name": name, "parent": parent, "type": project_type}),
                    CommandSource::Cli,
                );
                request.confirm = options.has("confirm");
                request.dry_run = options.has("dry-run");
                request.idempotency_key = options.value("idempotency-key").map(str::to_string);
                request.budget = budget_from_options(&options)?;
                engine.execute(request)
            }
            Some("open") => {
                let path = options
                    .positionals
                    .get(2)
                    .ok_or_else(|| "project open requires a path.".to_string())?;
                engine.execute(EngineCommandRequest::new(
                    "project.open",
                    json!({"path": path}),
                    CommandSource::Cli,
                ))
            }
            Some("info") => {
                let path = options.positionals.get(2).cloned();
                engine.execute(EngineCommandRequest::new(
                    "project.info",
                    path.map(|path| json!({"path": path}))
                        .unwrap_or_else(|| json!({})),
                    CommandSource::Cli,
                ))
            }
            Some("save" | "checkpoint") => {
                engine.execute(project_save_request(&options)?)
            }
            Some(other) => return Err(format!("Unknown project command: {other}")),
            None => return Err("project requires create, open, info, save or checkpoint.".to_string()),
        },
        "session" => match options.positionals.get(1).map(String::as_str) {
            Some("list") => engine.execute(EngineCommandRequest::new(
                "session.list",
                json!({}),
                CommandSource::Cli,
            )),
            _ => return Err("session currently supports only list.".to_string()),
        },
        "workspace" => match options.positionals.get(1).map(String::as_str) {
            Some("describe") => {
                let path = options.positionals.get(2).cloned();
                engine.execute(EngineCommandRequest::new(
                    "workspace.describe",
                    path.map(|path| json!({"path": path}))
                        .unwrap_or_else(|| json!({})),
                    CommandSource::Cli,
                ))
            }
            _ => return Err("workspace currently supports only describe.".to_string()),
        },
        "command" => run_command_request(&mut engine, &options)?,
        "mcp" => match options.positionals.get(1).map(String::as_str) {
            Some("serve") => return run_mcp_stdio(engine),
            _ => return Err("mcp currently supports only serve.".to_string()),
        },
        "serve" => return run_jsonl_stdio(engine),
        other => return Err(format!("Unknown command: {other}. Use raf help.")),
    };
    print_response(&response, format)
}

fn run_jsonl_stdio(mut engine: HeadlessEngine) -> CliResult<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let count = serve_lines(stdin.lock(), BufWriter::new(stdout.lock()), &mut engine)?;
    eprintln!("{PROGRAM}: processed {count} command frame(s)");
    Ok(())
}
