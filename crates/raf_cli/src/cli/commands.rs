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
use serde_json::{json, Value};
use std::env;
use std::io::{self, BufWriter};
use std::path::{Path, PathBuf};

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
    // Performance commands route to the live editor directly, before the
    // headless engine is created, so they never load a project just to fail.
    if command == "perf" {
        return run_perf(&options, format);
    }
    if command == "benchmark" {
        return run_benchmark(&options, format);
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

/// Resolves the editor to attach to: an explicit `--project` wins, otherwise
/// the single live editor discovered on this machine.
fn attached_project_path(options: &Options) -> CliResult<PathBuf> {
    if let Some(path) = options.path("project") {
        return Ok(path);
    }
    discovery::pick_single_live(None)
        .map(|editor| editor.project_path)
        .map_err(|error| {
            format!(
                "{error} Open the project with `raf open PATH`, or run `raf editors` to list \
                 live editors."
            )
        })
}

fn attached_client(path: &Path) -> CliResult<AttachedClient> {
    AttachedClient::connect(path, "raf-cli").map_err(|error| {
        format!(
            "{error}\nOpen the project with `raf open {}` or run `raf editors` to list live editors.",
            path.display()
        )
    })
}

/// `raf perf`: one windowed snapshot from the attached editor. The editor owns
/// the frame loop, so there is no headless fallback for live telemetry.
fn run_perf(options: &Options, format: OutputFormat) -> CliResult<()> {
    let window_seconds = match options.value("window") {
        Some(raw) => raw
            .parse::<f64>()
            .ok()
            .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
            .ok_or_else(|| "perf --window must be a positive number of seconds.".to_string())?,
        None => f64::from(raf_core::performance::DEFAULT_WINDOW_SECONDS),
    };
    let path = attached_project_path(options)?;
    let mut client = attached_client(&path)?;
    let response = client.execute(EngineCommandRequest::new(
        "performance.stats",
        json!({"window_seconds": window_seconds}),
        CommandSource::Cli,
    ));
    print_response(&response, format)
}

/// `raf benchmark`: live commands require the attached editor, while `list`
/// and `compare` read the same on-disk store headlessly when no editor is up.
fn run_benchmark(options: &Options, format: OutputFormat) -> CliResult<()> {
    let subcommand = options
        .positionals
        .get(1)
        .map(String::as_str)
        .unwrap_or("list");
    match subcommand {
        "start" => run_benchmark_attached(
            options,
            format,
            "performance.benchmark.start",
            benchmark_start_params(options)?,
        ),
        "status" => run_benchmark_attached(
            options,
            format,
            "performance.benchmark.status",
            benchmark_id_params(options),
        ),
        "stop" => run_benchmark_attached(
            options,
            format,
            "performance.benchmark.stop",
            benchmark_id_params(options),
        ),
        "cancel" => run_benchmark_attached(
            options,
            format,
            "performance.benchmark.cancel",
            json!({}),
        ),
        "list" => run_benchmark_read(
            options,
            format,
            "performance.benchmark.list",
            json!({}),
        ),
        "compare" => run_benchmark_read(
            options,
            format,
            "performance.benchmark.compare",
            benchmark_compare_params(options)?,
        ),
        other => Err(format!(
            "Unknown benchmark command: {other}. Use start, status, stop, cancel, list or compare."
        )),
    }
}

fn run_benchmark_attached(
    options: &Options,
    format: OutputFormat,
    name: &str,
    params: Value,
) -> CliResult<()> {
    let path = attached_project_path(options)?;
    let mut client = attached_client(&path)?;
    let response = client.execute(EngineCommandRequest::new(name, params, CommandSource::Cli));
    print_response(&response, format)
}

fn run_benchmark_read(
    options: &Options,
    format: OutputFormat,
    name: &str,
    params: Value,
) -> CliResult<()> {
    if let Some(path) = options.path("project") {
        // Saved reports live on disk; reading them never needs a live editor,
        // and this keeps `list`/`compare` working across engine restarts.
        let mut engine = HeadlessEngine::new(Some(path.as_path()))?;
        let response = engine.execute(EngineCommandRequest::new(name, params, CommandSource::Cli));
        return print_response(&response, format);
    }
    let editor = discovery::pick_single_live(None).map_err(|error| {
        format!(
            "{error} Pass --project PATH to read saved benchmarks without a live editor, or run \
             `raf editors` to list live projects."
        )
    })?;
    let mut client = AttachedClient::connect(&editor.project_path, "raf-cli")?;
    let response = client.execute(EngineCommandRequest::new(name, params, CommandSource::Cli));
    print_response(&response, format)
}

fn benchmark_start_params(options: &Options) -> CliResult<Value> {
    let mut params = serde_json::Map::new();
    if let Some(raw) = options.value("seconds") {
        let seconds = raw
            .parse::<f64>()
            .map_err(|_| "benchmark start --seconds must be a number of seconds.".to_string())?;
        params.insert("seconds".to_string(), json!(seconds));
    }
    if let Some(raw) = options.value("warmup") {
        let warmup = raw
            .parse::<f64>()
            .map_err(|_| "benchmark start --warmup must be a number of seconds.".to_string())?;
        params.insert("warmup_seconds".to_string(), json!(warmup));
    }
    if let Some(label) = options.value("label") {
        let label = label.trim();
        if !label.is_empty() {
            params.insert("label".to_string(), json!(label));
        }
    }
    Ok(Value::Object(params))
}

fn benchmark_id_params(options: &Options) -> Value {
    let id = options
        .value("id")
        .or_else(|| options.positionals.get(2).map(String::as_str));
    match id.map(str::trim).filter(|id| !id.is_empty()) {
        Some(id) => json!({"id": id}),
        None => json!({}),
    }
}

fn benchmark_compare_params(options: &Options) -> CliResult<Value> {
    let left = options
        .value("a")
        .map(str::to_string)
        .or_else(|| options.positionals.get(2).cloned());
    let right = options
        .value("b")
        .map(str::to_string)
        .or_else(|| options.positionals.get(3).cloned());
    let (Some(left), Some(right)) = (left, right) else {
        return Err(
            "benchmark compare requires two benchmark ids: `raf benchmark compare A B` or \
             `--a A --b B`."
                .to_string(),
        );
    };
    Ok(json!({"a": left, "b": right}))
}
