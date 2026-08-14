//! `raf`: the lightweight, headless command and MCP surface for AuraRafi.
//!
//! This binary intentionally depends on `raf_core` only. It can inspect and
//! create projects without opening Egui, WGPU or the editor window, or attach
//! to an already-open editor through the project-scoped local IPC descriptor.

use raf_core::capabilities::{CapabilityCatalog, CapabilityDefinition};
use raf_core::project::{Project, ProjectType};
use raf_core::session::ProjectSessionRegistry;
use raf_core::transaction::{ArtifactRef, TransactionId, TransactionLedger, VerificationSummary};
use raf_core::{
    serve_lines, CommandEndpoint, CommandSource, EngineCommandRequest, EngineCommandResponse,
    MAX_COMMAND_FRAME_BYTES,
};
use serde_json::json;
use serde_json::Value;
use std::collections::BTreeMap;
use std::env;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process;

mod attached;
use attached::AttachedClient;

const PROGRAM: &str = "raf";
const PROTOCOL_VERSION: &str = "2025-06-18";

type CliResult<T> = Result<T, String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Human,
    Json,
    Ndjson,
}

impl OutputFormat {
    fn from_options(options: &Options) -> Self {
        if options.has("ndjson") {
            Self::Ndjson
        } else if options.has("json") || options.value("format") == Some("json") {
            Self::Json
        } else {
            Self::Human
        }
    }
}

#[derive(Debug, Default)]
struct Options {
    flags: BTreeMap<String, Option<String>>,
    positionals: Vec<String>,
}

impl Options {
    fn has(&self, name: &str) -> bool {
        self.flags.contains_key(name)
    }

    fn value(&self, name: &str) -> Option<&str> {
        self.flags.get(name).and_then(Option::as_deref)
    }

    fn path(&self, name: &str) -> Option<PathBuf> {
        self.value(name).map(PathBuf::from)
    }
}

fn parse_options(arguments: &[String]) -> Options {
    let mut options = Options::default();
    let mut index = 0;
    while index < arguments.len() {
        let argument = &arguments[index];
        if let Some(raw) = argument.strip_prefix("--") {
            if let Some((name, value)) = raw.split_once('=') {
                options
                    .flags
                    .insert(name.to_ascii_lowercase(), Some(value.to_string()));
            } else if arguments
                .get(index + 1)
                .is_some_and(|next| !next.starts_with('-'))
            {
                options
                    .flags
                    .insert(raw.to_ascii_lowercase(), arguments.get(index + 1).cloned());
                index += 1;
            } else {
                options.flags.insert(raw.to_ascii_lowercase(), None);
            }
        } else if let Some(raw) = argument.strip_prefix('-') {
            if raw == "j" {
                options.flags.insert("json".to_string(), None);
            } else {
                options.positionals.push(argument.clone());
            }
        } else {
            options.positionals.push(argument.clone());
        }
        index += 1;
    }
    options
}

#[derive(Debug)]
struct HeadlessEngine {
    project: Option<Project>,
    catalog: CapabilityCatalog,
    ledger: TransactionLedger,
}

impl HeadlessEngine {
    fn new(project_path: Option<&Path>) -> CliResult<Self> {
        let project = match project_path {
            Some(path) => Some(load_project(path)?),
            None => None,
        };
        Ok(Self {
            project,
            catalog: CapabilityCatalog::builtin(),
            ledger: TransactionLedger::new(),
        })
    }

    fn project_path(&self) -> Option<&Path> {
        self.project.as_ref().map(|project| project.path.as_path())
    }

    fn response(
        &self,
        id: raf_core::CommandId,
        title: impl Into<String>,
        lines: Vec<String>,
        data: Value,
        changed: bool,
        record: Option<&raf_core::TransactionRecord>,
    ) -> EngineCommandResponse {
        EngineCommandResponse {
            protocol: raf_core::COMMAND_PROTOCOL_VERSION,
            id,
            ok: true,
            changed,
            title: title.into(),
            lines,
            data,
            warnings: Vec::new(),
            diff: record.and_then(|record| record.diff.clone()),
            undo_available: record.and_then(|record| record.undo_token).is_some(),
            revision: self.ledger.revision(),
            transaction_id: record.map(|record| record.id),
            undo_token: record.and_then(|record| record.undo_token),
            artifacts: Vec::<ArtifactRef>::new(),
            metrics: json!({"headless": true}),
            verification: Some(VerificationSummary {
                status: if changed { "not_run" } else { "not_required" }.to_string(),
                checks: Vec::new(),
                failures: Vec::new(),
            }),
        }
    }

    fn error(
        &self,
        id: raf_core::CommandId,
        title: &str,
        message: impl Into<String>,
    ) -> EngineCommandResponse {
        let mut response = EngineCommandResponse::error(id, title, message);
        response.revision = self.ledger.revision();
        response
    }

    fn request_transaction(&self, request: &EngineCommandRequest) -> CliResult<TransactionId> {
        self.ledger
            .check_expected(request.expected_revision)
            .map_err(|error| error.to_string())?;
        Ok(request.transaction_id.unwrap_or_default())
    }

    fn record_or_preview(
        &mut self,
        request: &EngineCommandRequest,
        transaction_id: TransactionId,
        changed: bool,
        diff: Value,
        idempotency_key: Option<String>,
    ) -> Option<raf_core::TransactionRecord> {
        if request.dry_run {
            return None;
        }
        Some(
            self.ledger
                .record_without_undo(transaction_id, changed, Some(diff), idempotency_key),
        )
    }

    fn project_info(&self, project: &Project) -> Value {
        json!({
            "id": project.id,
            "name": project.name,
            "type": format!("{:?}", project.project_type),
            "path": project.path,
            "engine_version": project.engine_version,
            "created_at": project.created_at,
            "modified_at": project.modified_at,
            "settings": project.settings,
        })
    }

    fn execute_capabilities(
        &self,
        request: &EngineCommandRequest,
        query: &str,
    ) -> EngineCommandResponse {
        let capabilities: Vec<Value> = self
            .catalog
            .search(query)
            .into_iter()
            .map(capability_value)
            .collect();
        self.response(
            request.id,
            "Capabilities",
            vec![format!("{} capabilities matched.", capabilities.len())],
            json!({"version": self.catalog.version, "capabilities": capabilities}),
            false,
            None,
        )
    }

    fn execute_project_create(&mut self, request: &EngineCommandRequest) -> EngineCommandResponse {
        let transaction_id = match self.request_transaction(request) {
            Ok(id) => id,
            Err(error) => return self.error(request.id, "Revision conflict", error),
        };
        if !request.dry_run && !request.confirm {
            return self.error(
                request.id,
                "Confirmation required",
                "project.create changes the filesystem; pass confirm=true or --confirm.",
            );
        }
        if let Some(record) = self
            .ledger
            .find_idempotency_key(request.idempotency_key.as_deref())
        {
            return self.response(
                request.id,
                "Project create (replayed)",
                vec!["The idempotency key was already applied.".to_string()],
                json!({"replayed": true}),
                record.changed,
                Some(record),
            );
        }
        let name = match string_param(&request.params, "name") {
            Some(name) if valid_project_name(name) => name.to_string(),
            _ => {
                return self.error(
                    request.id,
                    "Project create",
                    "name must be a safe directory name.",
                )
            }
        };
        let parent = string_param(&request.params, "parent")
            .map(PathBuf::from)
            .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let project_type = match string_param(&request.params, "project_type")
            .or_else(|| string_param(&request.params, "type"))
            .unwrap_or("game")
            .to_ascii_lowercase()
            .as_str()
        {
            "game" => ProjectType::Game,
            "electronics" | "electronics_design" => ProjectType::Electronics,
            _ => {
                return self.error(
                    request.id,
                    "Project create",
                    "type must be game or electronics.",
                )
            }
        };
        let destination = parent.join(&name);
        if destination.join(Project::META_FILE).exists() {
            return self.error(
                request.id,
                "Project create",
                format!("A project already exists at {}.", destination.display()),
            );
        }
        let diff = json!({
            "operation": "project.create",
            "path": destination,
            "name": name,
            "type": format!("{:?}", project_type),
        });
        if request.dry_run {
            let response = self.response(
                request.id,
                "Project create preview",
                vec!["No files were written; pass confirm=true to commit.".to_string()],
                json!({"preview": diff}),
                false,
                None,
            );
            return response;
        }
        let project = match Project::create(&name, project_type, &parent) {
            Ok(project) => project,
            Err(error) => return self.error(request.id, "Project create", error.to_string()),
        };
        let info = self.project_info(&project);
        self.project = Some(project);
        let record = self.record_or_preview(
            request,
            transaction_id,
            true,
            diff,
            request.idempotency_key.clone(),
        );
        self.response(
            request.id,
            "Project created",
            vec![format!("Created {}.", destination.display())],
            info,
            true,
            record.as_ref(),
        )
    }

    fn execute_project_open(&mut self, request: &EngineCommandRequest) -> EngineCommandResponse {
        let path = match string_param(&request.params, "path") {
            Some(path) => PathBuf::from(path),
            None => return self.error(request.id, "Project open", "path is required."),
        };
        let project = match load_project(&path) {
            Ok(project) => project,
            Err(error) => return self.error(request.id, "Project open", error),
        };
        let info = self.project_info(&project);
        self.project = Some(project);
        self.response(
            request.id,
            "Project opened",
            vec![format!("Loaded {}.", path.display())],
            info,
            false,
            None,
        )
    }

    fn execute_project_info(&self, request: &EngineCommandRequest) -> EngineCommandResponse {
        let project = if let Some(path) = string_param(&request.params, "path") {
            match load_project(Path::new(path)) {
                Ok(project) => project,
                Err(error) => return self.error(request.id, "Project info", error),
            }
        } else if let Some(project) = &self.project {
            project.clone()
        } else {
            return self.error(request.id, "Project info", "No project is open.");
        };
        self.response(
            request.id,
            "Project info",
            vec![format!("{} ({:?})", project.name, project.project_type)],
            self.project_info(&project),
            false,
            None,
        )
    }

    fn execute_project_save(&mut self, request: &EngineCommandRequest) -> EngineCommandResponse {
        let Some(project) = self.project.clone() else {
            return self.error(request.id, "Project save", "No project is open.");
        };
        let transaction_id = match self.request_transaction(request) {
            Ok(id) => id,
            Err(error) => return self.error(request.id, "Revision conflict", error),
        };
        if !request.dry_run && !request.confirm {
            return self.error(
                request.id,
                "Confirmation required",
                "project.save changes project metadata; pass confirm=true or --confirm.",
            );
        }
        if let Some(record) = self
            .ledger
            .find_idempotency_key(request.idempotency_key.as_deref())
        {
            return self.response(
                request.id,
                "Project save (replayed)",
                vec!["The idempotency key was already applied.".to_string()],
                json!({"replayed": true}),
                record.changed,
                Some(record),
            );
        }
        let diff = json!({"operation": "project.save", "path": project.path});
        if request.dry_run {
            return self.response(
                request.id,
                "Project save preview",
                vec!["No files were written; pass confirm=true to commit.".to_string()],
                json!({"preview": diff}),
                false,
                None,
            );
        }
        if let Err(error) = project.save() {
            return self.error(request.id, "Project save", error.to_string());
        }
        let record = self.record_or_preview(
            request,
            transaction_id,
            true,
            diff,
            request.idempotency_key.clone(),
        );
        self.response(
            request.id,
            "Project saved",
            vec![format!("Saved {}.", project.path.display())],
            self.project_info(&project),
            true,
            record.as_ref(),
        )
    }

    fn execute_session_list(&self, request: &EngineCommandRequest) -> EngineCommandResponse {
        let Some(project) = &self.project else {
            return self.error(request.id, "Sessions", "No project is open.");
        };
        let registry = ProjectSessionRegistry::load_or_legacy(&project.path, project.project_type);
        let sessions: Vec<Value> = registry
            .sessions
            .iter()
            .map(|session| {
                json!({
                    "id": session.id,
                    "name": session.name,
                    "kind": format!("{:?}", session.kind),
                    "directory": session.directory,
                    "active": session.id == registry.active_session,
                })
            })
            .collect();
        self.response(
            request.id,
            "Sessions",
            vec![format!("{} session(s).", sessions.len())],
            json!({"sessions": sessions, "active_session": registry.active_session}),
            false,
            None,
        )
    }

    fn execute_workspace_describe(&self, request: &EngineCommandRequest) -> EngineCommandResponse {
        let root = string_param(&request.params, "path")
            .map(PathBuf::from)
            .or_else(|| self.project_path().map(Path::to_path_buf));
        let Some(root) = root else {
            return self.error(
                request.id,
                "Workspace",
                "path is required when no project is open.",
            );
        };
        let (files, directories) = count_workspace_entries(&root);
        self.response(
            request.id,
            "Workspace",
            vec![format!("{} files, {} directories.", files, directories)],
            json!({"path": root, "files": files, "directories": directories}),
            false,
            None,
        )
    }

    fn execute_status(&self, request: &EngineCommandRequest) -> EngineCommandResponse {
        self.response(
            request.id,
            "Rafi status",
            vec![if self.project.is_some() {
                "Headless engine ready with project.".to_string()
            } else {
                "Headless engine ready; no project open.".to_string()
            }],
            json!({
                "program": PROGRAM,
                "version": env!("CARGO_PKG_VERSION"),
                "project": self.project.as_ref().map(|project| self.project_info(project)),
                "revision": self.ledger.revision(),
                "headless": true,
                "runtime_enabled": false,
                "play_enabled": false,
                "ui_dependency": false,
            }),
            false,
            None,
        )
    }

    fn execute_doctor(&self, request: &EngineCommandRequest) -> EngineCommandResponse {
        let cwd = env::current_dir().ok();
        let rust_project = cwd
            .as_ref()
            .is_some_and(|path| path.join("Cargo.toml").exists());
        self.response(
            request.id,
            "Rafi doctor",
            vec![if rust_project {
                "CLI environment looks usable.".to_string()
            } else {
                "CLI started outside a Rust workspace.".to_string()
            }],
            json!({
                "cwd": cwd,
                "cargo_manifest": rust_project,
                "catalog_version": self.catalog.version,
                "capability_count": self.catalog.capabilities.len(),
                "protocol": raf_core::COMMAND_PROTOCOL_VERSION,
                "mcp_protocol": PROTOCOL_VERSION,
            }),
            false,
            None,
        )
    }

    fn resource(&self, uri: &str) -> CliResult<Value> {
        match uri {
            "raf://project" => Ok(self
                .project
                .as_ref()
                .map(|project| self.project_info(project))
                .unwrap_or_else(|| json!({"project": null}))),
            "raf://capabilities" => Ok(self.catalog.as_json()),
            "raf://workspace" => {
                let Some(root) = self.project_path() else {
                    return Ok(json!({"workspace": null}));
                };
                let (files, directories) = count_workspace_entries(root);
                Ok(json!({"path": root, "files": files, "directories": directories}))
            }
            "raf://status" => Ok(json!({
                "revision": self.ledger.revision(),
                "project": self.project.as_ref().map(|project| project.name.clone()),
                "runtime_enabled": false,
                "play_enabled": false,
            })),
            _ => Err(format!("Unknown Rafi resource: {uri}")),
        }
    }
}

trait McpCommandHost: CommandEndpoint {
    fn resource(&self, uri: &str) -> CliResult<Value>;
}

impl McpCommandHost for HeadlessEngine {
    fn resource(&self, uri: &str) -> CliResult<Value> {
        HeadlessEngine::resource(self, uri)
    }
}

impl McpCommandHost for AttachedClient {
    fn resource(&self, uri: &str) -> CliResult<Value> {
        AttachedClient::resource(self, uri)
    }
}

impl CommandEndpoint for HeadlessEngine {
    fn execute(&mut self, request: EngineCommandRequest) -> EngineCommandResponse {
        if let Err(error) = request.validate() {
            return self.error(request.id, "Invalid command", error);
        }
        let name = request.name.trim_start_matches('/').to_ascii_lowercase();
        match name.as_str() {
            "help" | "commands" => self.execute_capabilities(&request, ""),
            "capabilities.list" => self.execute_capabilities(&request, ""),
            "capabilities.search" => self.execute_capabilities(
                &request,
                string_param(&request.params, "query").unwrap_or(""),
            ),
            "project.create" => self.execute_project_create(&request),
            "project.open" => self.execute_project_open(&request),
            "project.info" => self.execute_project_info(&request),
            "project.save" => self.execute_project_save(&request),
            "session.list" => self.execute_session_list(&request),
            "workspace.describe" => self.execute_workspace_describe(&request),
            "engine.status" | "status" => self.execute_status(&request),
            "engine.doctor" | "doctor" => self.execute_doctor(&request),
            _ => self.error(
                request.id,
                "Headless command unavailable",
                format!(
                    "'{name}' requires an attached editor/domain executor. No Play, Stop or Runtime operation is exposed by the headless CLI."
                ),
            ),
        }
    }
}

fn load_project(path: &Path) -> CliResult<Project> {
    let directory = if path.is_file() {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    Project::load(directory)
        .map_err(|error| format!("Unable to load project {}: {error}", directory.display()))
}

fn valid_project_name(name: &str) -> bool {
    !name.trim().is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains(':')
}

fn string_param<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params.as_object()?.get(key)?.as_str()
}

fn capability_value(capability: &CapabilityDefinition) -> Value {
    json!({
        "name": capability.name,
        "aliases": capability.aliases,
        "domain": capability.domain,
        "category": capability.category,
        "description_key": capability.description_key,
        "parameters": capability.parameters,
        "examples": capability.examples,
        "risk": capability.risk(),
        "read_only": capability.is_read_only(),
    })
}

fn count_workspace_entries(root: &Path) -> (usize, usize) {
    fn visit(path: &Path, files: &mut usize, directories: &mut usize, depth: u8) {
        if depth > 8 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if matches!(name.as_str(), ".git" | "target" | "target_gnu" | ".codex") {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                *directories += 1;
                visit(&entry.path(), files, directories, depth.saturating_add(1));
            } else if kind.is_file() {
                *files += 1;
            }
        }
    }
    let mut files = 0;
    let mut directories = 0;
    visit(root, &mut files, &mut directories, 0);
    (files, directories)
}

fn build_command_request(options: &Options) -> CliResult<EngineCommandRequest> {
    let name = options
        .positionals
        .get(
            if options.positionals.first().map(String::as_str) == Some("attach") {
                2
            } else {
                1
            },
        )
        .ok_or_else(|| "raf command requires a command name.".to_string())?;
    let params = options
        .value("params")
        .map(|raw| {
            serde_json::from_str(raw).map_err(|error| format!("Invalid --params JSON: {error}"))
        })
        .transpose()?
        .unwrap_or_else(|| json!({}));
    if !params.is_object() {
        return Err("--params must be a JSON object.".to_string());
    }
    let mut request = EngineCommandRequest::new(name, params, CommandSource::Cli);
    request.confirm = options.has("confirm");
    request.dry_run = options.has("dry-run");
    request.expected_revision = options
        .value("expected-revision")
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| "expected-revision must be an integer.")
        })
        .transpose()?;
    request.idempotency_key = options.value("idempotency-key").map(str::to_string);
    Ok(request)
}

fn run_command_request<E: CommandEndpoint>(
    engine: &mut E,
    options: &Options,
) -> CliResult<EngineCommandResponse> {
    let request = build_command_request(options)?;
    Ok(engine.execute(request))
}

fn print_response(response: &EngineCommandResponse, format: OutputFormat) -> CliResult<()> {
    match format {
        OutputFormat::Human => {
            println!("{}", response.title);
            for line in &response.lines {
                println!("- {line}");
            }
            if !response.data.is_null() {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&response.data)
                        .map_err(|error| error.to_string())?
                );
            }
        }
        OutputFormat::Json | OutputFormat::Ndjson => {
            println!(
                "{}",
                serde_json::to_string(response).map_err(|error| error.to_string())?
            );
        }
    }
    if response.ok {
        Ok(())
    } else {
        Err(response.lines.join(" "))
    }
}

fn print_help() {
    println!(
        "{PROGRAM} - lightweight AuraRafi command and MCP surface\n\n\
Usage:\n  raf doctor [--json]\n  raf status [--project PATH] [--json]\n  raf project create --name NAME [--parent PATH] [--type game|electronics] --confirm\n  raf project open PATH\n  raf project info [PATH]\n  raf capabilities [search QUERY]\n  raf session list [--project PATH]\n  raf workspace describe [PATH]\n  raf command NAME [--params JSON] [--dry-run] [--confirm]\n  raf mcp serve [--project PATH]\n  raf serve [--project PATH]            JSONL command endpoint\n\nGlobal output: --json, --ndjson, --format human|json"
    );
}

fn run_attached_cli(options: &Options, format: OutputFormat) -> CliResult<()> {
    let project_path = options
        .path("project")
        .or_else(|| options.positionals.get(1).map(PathBuf::from))
        .ok_or_else(|| "raf attach requires --project PATH.".to_string())?;
    let mut client = AttachedClient::connect(&project_path, "raf-cli")?;
    let response = match options.positionals.get(1).map(String::as_str) {
        Some("command") => run_command_request(&mut client, options)?,
        Some("status") => client.execute(EngineCommandRequest::new(
            "engine.status",
            json!({}),
            CommandSource::Cli,
        )),
        Some("capabilities") => client.execute(EngineCommandRequest::new(
            "capabilities.list",
            json!({}),
            CommandSource::Cli,
        )),
        Some("project") | Some("project.info") => client.execute(EngineCommandRequest::new(
            "project.info",
            json!({}),
            CommandSource::Cli,
        )),
        Some(other) => return Err(format!("Unknown attached command: {other}.")),
        None => client.execute(EngineCommandRequest::new(
            "engine.status",
            json!({}),
            CommandSource::Cli,
        )),
    };
    print_response(&response, format)
}

fn run_cli(arguments: Vec<String>) -> CliResult<()> {
    let options = parse_options(&arguments);
    let format = OutputFormat::from_options(&options);
    let command = options
        .positionals
        .first()
        .map(String::as_str)
        .unwrap_or("help");
    let project_path = options.path("project");
    if command == "attach" {
        return run_attached_cli(&options, format);
    }
    if command == "mcp" && options.has("attach") {
        let attach_path = options
            .path("attach")
            .or_else(|| options.path("project"))
            .ok_or_else(|| "raf mcp --attach requires a project path.".to_string())?;
        let client = AttachedClient::connect(&attach_path, "raf-mcp")?;
        return run_mcp_stdio(client);
    }
    let mut engine = HeadlessEngine::new(project_path.as_deref())?;

    let response = match command {
        "help" | "--help" | "-h" => {
            print_help();
            return Ok(());
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
            Some(other) => return Err(format!("Unknown project command: {other}")),
            None => return Err("project requires create, open or info.".to_string()),
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

fn run_mcp_stdio<H: McpCommandHost>(mut engine: H) -> CliResult<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let reader = BufReader::new(stdin.lock());
    let mut writer = BufWriter::new(stdout.lock());
    for line in reader.lines() {
        let line = line.map_err(|error| format!("MCP read: {error}"))?;
        if line.trim().is_empty() {
            continue;
        }
        if line.len() > MAX_COMMAND_FRAME_BYTES {
            let response = jsonrpc_error(
                &Value::Null,
                -32600,
                "MCP frame exceeds the 1 MiB safety limit.",
            );
            write_mcp_response(&mut writer, &response)?;
            continue;
        }
        let message: Value = match serde_json::from_str(&line) {
            Ok(message) => message,
            Err(error) => {
                let response = jsonrpc_error(&Value::Null, -32700, format!("MCP JSON: {error}"));
                write_mcp_response(&mut writer, &response)?;
                continue;
            }
        };
        if let Some(response) = handle_mcp_message(&mut engine, &message) {
            write_mcp_response(&mut writer, &response)?;
        }
    }
    Ok(())
}

fn write_mcp_response(writer: &mut impl Write, response: &Value) -> CliResult<()> {
    let encoded = serde_json::to_string(response).map_err(|error| error.to_string())?;
    writer
        .write_all(encoded.as_bytes())
        .and_then(|_| writer.write_all(b"\n"))
        .and_then(|_| writer.flush())
        .map_err(|error| format!("MCP write: {error}"))
}

fn jsonrpc_result(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn jsonrpc_error(id: &Value, code: i64, message: impl Into<String>) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message.into()}})
}

fn handle_mcp_message<H: McpCommandHost>(engine: &mut H, message: &Value) -> Option<Value> {
    let id = message.get("id").cloned().unwrap_or(Value::Null);
    let method = message.get("method")?.as_str()?;
    if !message.get("id").is_some() {
        return None;
    }
    match method {
        "initialize" => Some(jsonrpc_result(
            &id,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {
                    "tools": {"listChanged": false},
                    "resources": {"subscribe": false, "listChanged": false},
                    "prompts": {"listChanged": false},
                },
                "serverInfo": {"name": "raf", "version": env!("CARGO_PKG_VERSION")},
                "instructions": "AuraRafi headless command surface. Mutations are scoped to the project and never expose Play or Runtime.",
            }),
        )),
        "ping" => Some(jsonrpc_result(&id, json!({}))),
        "tools/list" => Some(jsonrpc_result(&id, json!({"tools": mcp_tools()}))),
        "resources/list" => Some(jsonrpc_result(
            &id,
            json!({"resources": [
                {"uri": "raf://status", "name": "Rafi status", "mimeType": "application/json"},
                {"uri": "raf://project", "name": "Open project", "mimeType": "application/json"},
                {"uri": "raf://capabilities", "name": "Engine capabilities", "mimeType": "application/json"},
                {"uri": "raf://workspace", "name": "Workspace summary", "mimeType": "application/json"},
            ]}),
        )),
        "resources/read" => {
            let uri = message
                .pointer("/params/uri")
                .and_then(Value::as_str)
                .unwrap_or("");
            match engine.resource(uri) {
                Ok(value) => Some(jsonrpc_result(
                    &id,
                    json!({"contents": [{"uri": uri, "mimeType": "application/json", "text": value.to_string()}]}),
                )),
                Err(error) => Some(jsonrpc_error(&id, -32602, error)),
            }
        }
        "prompts/list" => Some(jsonrpc_result(
            &id,
            json!({"prompts": [{"name": "raf_plan_scene_change", "description": "Create a dry-run plan for a scene change before committing."}]}),
        )),
        "prompts/get" => Some(jsonrpc_result(
            &id,
            json!({"description": "Rafi scene planning prompt", "messages": [{"role": "user", "content": {"type": "text", "text": "Plan the requested AuraRafi scene change. Inspect capabilities first, preview mutations, then verify the result. Do not use Play or Runtime."}}]}),
        )),
        "tools/call" => handle_mcp_tool_call(engine, &id, message),
        _ => Some(jsonrpc_error(
            &id,
            -32601,
            format!("Unknown MCP method: {method}"),
        )),
    }
}

fn handle_mcp_tool_call<H: McpCommandHost>(
    engine: &mut H,
    id: &Value,
    message: &Value,
) -> Option<Value> {
    let name = message
        .pointer("/params/name")
        .and_then(Value::as_str)
        .unwrap_or("");
    let arguments = message
        .pointer("/params/arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let confirm = arguments
        .get("confirm")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let dry_run = arguments
        .get("dry_run")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let expected_revision = match arguments.get("expected_revision") {
        Some(value) => match value.as_u64() {
            Some(revision) => Some(revision),
            None => {
                return Some(jsonrpc_error(
                    id,
                    -32602,
                    "expected_revision must be an unsigned integer.",
                ))
            }
        },
        None => None,
    };
    let idempotency_key = arguments
        .get("idempotency_key")
        .and_then(Value::as_str)
        .map(str::to_string);
    let session = arguments
        .get("session")
        .and_then(Value::as_str)
        .map(str::to_string);
    let budget = match arguments.get("budget") {
        Some(value) => match serde_json::from_value(value.clone()) {
            Ok(budget) => Some(budget),
            Err(error) => {
                return Some(jsonrpc_error(
                    id,
                    -32602,
                    format!("budget is invalid: {error}"),
                ))
            }
        },
        None => None,
    };
    let Some(command_name) = mcp_tool_command(name) else {
        return Some(jsonrpc_error(
            id,
            -32602,
            format!("Unknown Rafi MCP tool: {name}"),
        ));
    };
    let mut arguments = arguments;
    let command_name = if command_name == "__generic__" {
        let Some(generic_name) = arguments
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return Some(jsonrpc_error(id, -32602, "raf_command requires a name."));
        };
        let params = arguments
            .get("params")
            .cloned()
            .unwrap_or_else(|| json!({}));
        arguments = params;
        generic_name
    } else {
        command_name.to_string()
    };
    let mut request = EngineCommandRequest::new(command_name, arguments, CommandSource::Mcp);
    request.confirm = confirm;
    request.dry_run = dry_run;
    request.expected_revision = expected_revision;
    request.idempotency_key = idempotency_key;
    request.session = session;
    request.budget = budget;
    let response = engine.execute(request);
    let text = if response.lines.is_empty() {
        response.title.clone()
    } else {
        response.lines.join("\n")
    };
    Some(jsonrpc_result(
        id,
        json!({
            "isError": !response.ok,
            "content": [{"type": "text", "text": text}],
            "structuredContent": serde_json::to_value(&response).unwrap_or_else(|_| json!({"ok": false})),
        }),
    ))
}

fn mcp_tool_command(name: &str) -> Option<&'static str> {
    match name {
        "raf_status" => Some("engine.status"),
        "raf_doctor" => Some("engine.doctor"),
        "raf_capabilities_search" => Some("capabilities.search"),
        "raf_project_info" => Some("project.info"),
        "raf_project_open" => Some("project.open"),
        "raf_project_create" => Some("project.create"),
        "raf_session_list" => Some("session.list"),
        "raf_workspace_describe" => Some("workspace.describe"),
        "raf_command" => Some("__generic__"),
        _ => None,
    }
}

fn mcp_tools() -> Vec<Value> {
    vec![
        mcp_tool(
            "raf_status",
            "Read headless engine and project status.",
            json!({"type":"object","properties":{}}),
        ),
        mcp_tool(
            "raf_doctor",
            "Check the local Rafi CLI environment.",
            json!({"type":"object","properties":{}}),
        ),
        mcp_tool(
            "raf_capabilities_search",
            "Search the self-describing Rafi command catalog.",
            json!({"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}),
        ),
        mcp_tool(
            "raf_project_info",
            "Read project metadata.",
            json!({"type":"object","properties":{"path":{"type":"string"}}}),
        ),
        mcp_tool(
            "raf_project_open",
            "Open a project directory in this headless session.",
            json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}),
        ),
        mcp_tool(
            "raf_project_create",
            "Preview or create a project directory. Filesystem mutation requires confirm=true.",
            json!({"type":"object","properties":{"name":{"type":"string"},"parent":{"type":"string"},"type":{"type":"string","enum":["game","electronics"]},"confirm":{"type":"boolean"},"dry_run":{"type":"boolean"}},"required":["name"]}),
        ),
        mcp_tool(
            "raf_session_list",
            "List sessions in the open project.",
            json!({"type":"object","properties":{}}),
        ),
        mcp_tool(
            "raf_workspace_describe",
            "Return bounded workspace counts.",
            json!({"type":"object","properties":{"path":{"type":"string"}}}),
        ),
        mcp_tool(
            "raf_command",
            "Call a supported headless command by name.",
            json!({"type":"object","properties":{"name":{"type":"string"},"params":{"type":"object"},"confirm":{"type":"boolean"},"dry_run":{"type":"boolean"},"expected_revision":{"type":"integer","minimum":0},"idempotency_key":{"type":"string"},"session":{"type":"string"},"budget":{"type":"object"}},"required":["name"]}),
        ),
    ]
}

fn mcp_tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({"name": name, "description": description, "inputSchema": input_schema})
}

fn main() {
    if let Err(error) = run_cli(env::args().skip(1).collect()) {
        eprintln!("{PROGRAM}: {error}");
        process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::fs;

    #[test]
    fn parser_keeps_json_and_boolean_options() {
        let options = parse_options(&[
            "command".to_string(),
            "project.info".to_string(),
            "--params".to_string(),
            "{}".to_string(),
            "--dry-run".to_string(),
        ]);
        assert_eq!(options.positionals, vec!["command", "project.info"]);
        assert_eq!(options.value("params"), Some("{}"));
        assert!(options.has("dry-run"));
    }

    #[test]
    fn attached_command_builder_targets_the_command_after_attach_mode() {
        let options = parse_options(&[
            "attach".to_string(),
            "command".to_string(),
            "game.add".to_string(),
            "--confirm".to_string(),
        ]);
        let request = build_command_request(&options).unwrap();
        assert_eq!(request.name, "game.add");
        assert!(request.confirm);
        assert_eq!(request.source, CommandSource::Cli);
    }

    #[test]
    fn headless_project_lifecycle_is_ui_independent() {
        let root = std::env::temp_dir().join(format!(
            "raf-cli-test-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(&root).unwrap();
        let mut engine = HeadlessEngine::new(None).unwrap();
        let mut create = EngineCommandRequest::new(
            "project.create",
            json!({"name":"Demo","parent":root,"type":"game"}),
            CommandSource::Cli,
        );
        create.confirm = true;
        let response = engine.execute(create);
        assert!(response.ok, "{}", response.lines.join(" "));
        assert!(engine.project.is_some());
        let info = engine.execute(EngineCommandRequest::new(
            "project.info",
            json!({}),
            CommandSource::Cli,
        ));
        assert!(info.ok);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn project_create_dry_run_does_not_touch_disk() {
        let root = std::env::temp_dir().join(format!(
            "raf-cli-dry-run-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(&root).unwrap();
        let mut engine = HeadlessEngine::new(None).unwrap();
        let mut request = EngineCommandRequest::new(
            "project.create",
            json!({"name":"PreviewOnly","parent":root,"type":"game"}),
            CommandSource::Cli,
        );
        request.dry_run = true;
        let response = engine.execute(request);
        assert!(response.ok);
        assert!(!root.join("PreviewOnly").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn mcp_initialize_and_tool_catalog_are_json_rpc() {
        let mut engine = HeadlessEngine::new(None).unwrap();
        let initialize = handle_mcp_message(
            &mut engine,
            &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
        )
        .unwrap();
        assert_eq!(initialize["result"]["serverInfo"]["name"], "raf");
        let tools = handle_mcp_message(
            &mut engine,
            &json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        )
        .unwrap();
        assert!(tools["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "raf_status"));
    }
}
