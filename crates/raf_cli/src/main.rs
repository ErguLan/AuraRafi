//! `raf`: the lightweight, headless command and MCP surface for AuraRafi.
//!
//! This binary intentionally depends on `raf_core` only. It can inspect and
//! create projects without opening the renderer or editor window, or attach
//! to an already-open editor through the project-scoped local IPC descriptor.

use raf_core::agent_context::{self, AgentObservationResult};
use raf_core::capabilities::{CapabilityCatalog, CapabilityDefinition};
use raf_core::project::{Project, ProjectType};
use raf_core::scene::{SceneGraph, SceneNodeId};
use raf_core::session::ProjectSessionRegistry;
use raf_core::transaction::{
    ArtifactRef, ExecutionBudget, TransactionId, TransactionLedger, VerificationSummary,
};
use raf_core::{
    serve_lines, CommandEndpoint, CommandSource, EngineCommandRequest, EngineCommandResponse,
    MAX_COMMAND_FRAME_BYTES,
};
use serde_json::json;
use serde_json::Value;
use std::collections::{hash_map::DefaultHasher, BTreeMap};
use std::env;
use std::hash::{Hash, Hasher};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process;

mod attached;
mod discovery;
use attached::AttachedClient;
use discovery::DiscoveredEditor;

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
        let ledger = project
            .as_ref()
            .map(|project| TransactionLedger::load_persisted_for_project(&project.path))
            .unwrap_or_default();
        Ok(Self {
            project,
            catalog: CapabilityCatalog::builtin(),
            ledger,
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
        let record =
            self.ledger
                .record_without_undo(transaction_id, changed, Some(diff), idempotency_key);
        self.persist_ledger();
        Some(record)
    }

    fn persist_ledger(&self) {
        if let Some(project) = self.project.as_ref() {
            if let Err(error) = self.ledger.persist_for_project(&project.path) {
                eprintln!(
                    "{PROGRAM}: headless agent state persistence failed for {}: {error}",
                    project.path.display()
                );
            }
        }
    }

    /// Reconcile the durable command clock with the document currently on
    /// disk. Headless commands may be issued by more than one process, so a
    /// persisted revision alone is not enough to detect an external scene
    /// edit. The semantic fingerprint keeps this check independent from
    /// renderer caches and preserves the same identity used by Agent tools.
    fn synchronize_document_revision(&mut self) {
        let fingerprint = {
            let Some(project) = self.project.as_ref() else {
                return;
            };
            if project.project_type != ProjectType::Game {
                return;
            }
            let scene = load_project_scene(project);
            scene_document_fingerprint(&scene)
        };
        let previous = self.ledger.document_fingerprint();
        let changed = self.ledger.observe_document(fingerprint);
        if changed || previous.is_none() {
            self.persist_ledger();
        }
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
        self.ledger = TransactionLedger::new();
        self.synchronize_document_revision();
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
        self.ledger = self
            .project
            .as_ref()
            .map(|project| TransactionLedger::load_persisted_for_project(&project.path))
            .unwrap_or_default();
        self.synchronize_document_revision();
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

    fn execute_context(&self, request: &EngineCommandRequest) -> EngineCommandResponse {
        let Some(project) = &self.project else {
            return self.error(request.id, "Project context", "No project is open.");
        };
        let registry = ProjectSessionRegistry::load_or_legacy(&project.path, project.project_type);
        let session = registry.active().map(|session| {
            json!({
                "id": session.id,
                "name": session.name,
                "kind": format!("{:?}", session.kind),
            })
        });
        let (files, directories) = count_workspace_entries(&project.path);
        let (scene_graph, assets) = if project.project_type == ProjectType::Game {
            let scene = load_project_scene(project);
            let assets = project_asset_paths(&project.path);
            (Some(scene), assets)
        } else {
            (None, Vec::new())
        };
        let scene = scene_graph
            .as_ref()
            .map(|scene| scene_context(scene, &[]))
            .unwrap_or(Value::Null);
        let used_assets = scene_graph
            .as_ref()
            .map(|scene| used_asset_count(scene, &assets))
            .unwrap_or(0);
        self.response(
            request.id,
            "Project context",
            vec![format!(
                "Headless {} context at revision {}.",
                project_type_label(project.project_type),
                self.ledger.revision()
            )],
            json!({
                "project": self.project_info(project),
                "session": session,
                "revision": self.ledger.revision(),
                "capabilities": headless_capability_values(&self.catalog),
                "workspace": {"files": files, "directories": directories, "internal_metadata_skipped": true},
                "scene": scene,
                "assets": {"imported": assets.len(), "used": used_assets, "unused": assets.len().saturating_sub(used_assets)},
                "runtime_enabled": false,
                "play_enabled": false,
            }),
            false,
            None,
        )
    }

    fn execute_observation(
        &self,
        request: &EngineCommandRequest,
        name: &str,
    ) -> EngineCommandResponse {
        let Some(project) = &self.project else {
            return self.error(request.id, "Project observation", "No project is open.");
        };
        if project.project_type != ProjectType::Game
            && matches!(
                name,
                "scene.outline"
                    | "scene.query"
                    | "scene.spatial_map"
                    | "scene.spatial"
                    | "scene.overlaps"
                    | "scene.check_overlaps"
                    | "scene.diff"
                    | "scene.design_audit"
                    | "scene.layout_audit"
                    | "scene.design_check"
                    | "scene.inspect"
                    | "selection.get"
                    | "scene.verify"
                    | "game.validate_layout"
            )
        {
            return self.error(
                request.id,
                "Project observation",
                "Scene observation is only available for Game projects.",
            );
        }
        if project.project_type != ProjectType::Game && name == "project.health" {
            return self.error(
                request.id,
                "Project observation",
                "Scene health diagnostics are currently available only for Game projects.",
            );
        }
        if name == "scene.diff" {
            let from_revision = request
                .params
                .get("from_revision")
                .and_then(Value::as_u64)
                .unwrap_or_else(|| self.ledger.revision().saturating_sub(1));
            return match self.ledger.diff_since(from_revision) {
                Ok(data) => self.response(
                    request.id,
                    "Scene diff",
                    vec![format!(
                        "Scene changes from revision {from_revision} to {}.",
                        self.ledger.revision()
                    )],
                    data,
                    false,
                    None,
                ),
                Err(error) => self.error(request.id, "Scene diff", error),
            };
        }
        let scene = load_project_scene(project);
        let assets = project_asset_paths(&project.path);
        let observation = match name {
            "scene.outline" => agent_context::scene_outline(&scene, &request.params, &[]),
            "scene.query" => agent_context::scene_query(&scene, &request.params, &[]),
            "scene.spatial_map" | "scene.spatial" => {
                agent_context::scene_spatial_map(&scene, &request.params, &[])
            }
            "scene.overlaps" | "scene.check_overlaps" => {
                agent_context::scene_check_overlaps(&scene, &request.params, &[])
            }
            "scene.design_audit" | "scene.layout_audit" | "scene.design_check" => {
                agent_context::scene_design_audit(&scene, &request.params, &[])
            }
            "scene.inspect" => agent_context::scene_inspect(&scene, &request.params, &[]),
            "selection.get" => agent_context::selection_info(&scene, &[]),
            "assets.inspect" => {
                agent_context::asset_inspect(&scene, &assets, &request.params, false, None)
            }
            "assets.catalog" | "assets.search" => {
                agent_context::assets_catalog(&scene, &assets, &request.params, false, None)
            }
            "assets.recommend" => {
                agent_context::assets_recommend(&scene, &assets, &request.params, false, None)
            }
            "scripts.catalog" | "scripts.search" => {
                agent_context::scripts_catalog(&scene, &assets, &request.params)
            }
            "project.health" => {
                agent_context::project_health_scoped(&scene, &assets, &request.params)
            }
            "scene.verify" | "game.validate_layout" => {
                agent_context::scene_verify(&scene, &request.params, &[])
            }
            _ => {
                return self.error(
                    request.id,
                    "Project observation",
                    format!("Unknown observation command: {name}"),
                )
            }
        };
        self.observation_response(request, observation)
    }

    fn observation_response(
        &self,
        request: &EngineCommandRequest,
        observation: AgentObservationResult,
    ) -> EngineCommandResponse {
        let ok = observation.is_success();
        EngineCommandResponse {
            protocol: raf_core::COMMAND_PROTOCOL_VERSION,
            id: request.id,
            ok,
            changed: false,
            title: observation.title,
            lines: vec![observation.summary],
            data: observation.data,
            warnings: observation.warnings,
            diff: None,
            undo_available: false,
            revision: self.ledger.revision(),
            transaction_id: None,
            undo_token: None,
            artifacts: Vec::new(),
            metrics: json!({"headless": true, "observation": true}),
            verification: observation.verification,
        }
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
            "raf://context" => {
                let Some(project) = &self.project else {
                    return Ok(json!({
                        "project": null,
                        "revision": self.ledger.revision(),
                        "context_loaded": false,
                    }));
                };
                let registry =
                    ProjectSessionRegistry::load_or_legacy(&project.path, project.project_type);
                let session = registry.active().map(|session| {
                    json!({
                        "id": session.id,
                        "name": session.name,
                        "kind": format!("{:?}", session.kind),
                    })
                });
                let assets = if project.project_type == ProjectType::Game {
                    project_asset_paths(&project.path)
                } else {
                    Vec::new()
                };
                let scene_graph = if project.project_type == ProjectType::Game {
                    Some(load_project_scene(project))
                } else {
                    None
                };
                let scene = scene_graph
                    .as_ref()
                    .map(|scene| scene_context(scene, &[]))
                    .unwrap_or(Value::Null);
                let used_assets = scene_graph
                    .as_ref()
                    .map(|scene| used_asset_count(scene, &assets))
                    .unwrap_or(0);
                Ok(json!({
                    "project": self.project_info(project),
                    "session": session,
                    "revision": self.ledger.revision(),
                    "capabilities": headless_capability_values(&self.catalog),
                    "scene": scene,
                    "assets": {"imported": assets.len(), "used": used_assets, "unused": assets.len().saturating_sub(used_assets)},
                    "runtime_enabled": false,
                    "play_enabled": false,
                    "context_loaded": true,
                }))
            }
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
        if !matches!(name.as_str(), "project.create" | "project.open") {
            self.synchronize_document_revision();
        }
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
            "engine.context" | "agent.context" | "project.summary" => {
                self.execute_context(&request)
            }
            "scene.outline"
            | "scene.query"
            | "scene.spatial_map"
            | "scene.spatial"
            | "scene.overlaps"
            | "scene.check_overlaps"
            | "scene.diff"
            | "scene.design_audit"
            | "scene.layout_audit"
            | "scene.design_check"
            | "scene.inspect"
            | "selection.get"
            | "assets.catalog"
            | "assets.search"
            | "assets.inspect"
            | "assets.recommend"
            | "scripts.catalog"
            | "scripts.search"
            | "project.health"
            | "scene.verify"
            | "game.validate_layout" => self.execute_observation(&request, &name),
            "viewport.capture" | "viewport.screenshot" => self.error(
                request.id,
                "Viewport capture unavailable",
                "Viewport capture requires an attached editor with a rendered Game viewport.",
            ),
            "task.list" | "task.get" | "task.events" | "task.cancel" => self.error(
                request.id,
                "Agent task unavailable",
                "Agent task control requires an attached native editor.",
            ),
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
        if depth > 8 || *files + *directories >= 8_192 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if matches!(
                name.as_str(),
                ".git"
                    | ".aura_rafi"
                    | ".ai"
                    | ".codex"
                    | "target"
                    | "target_gnu"
                    | "node_modules"
                    | ".cache"
            ) || name == "agent_history.ron"
                || name == "agent_endpoint.json"
            {
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

fn headless_capability_values(catalog: &CapabilityCatalog) -> Vec<Value> {
    [
        "engine.status",
        "engine.context",
        "scene.outline",
        "scene.query",
        "scene.spatial_map",
        "scene.overlaps",
        "scene.diff",
        "scene.design_audit",
        "scene.inspect",
        "selection.get",
        "assets.catalog",
        "assets.recommend",
        "assets.inspect",
        "scripts.catalog",
        "project.health",
        "scene.verify",
        "engine.doctor",
        "capabilities.list",
        "capabilities.search",
        "project.create",
        "project.open",
        "project.info",
        "project.save",
        "session.list",
        "workspace.describe",
    ]
    .into_iter()
    .map(|name| {
        catalog.find(name).map(capability_value).unwrap_or_else(|| {
            json!({
                "name": name,
                "aliases": [],
                "domain": "shared",
                "category": "inspection",
                "description_key": "commands.headless_metadata.desc",
                "parameters": [],
                "examples": [],
                "risk": "read",
                "read_only": true,
            })
        })
    })
    .collect()
}

fn project_type_label(project_type: ProjectType) -> &'static str {
    match project_type {
        ProjectType::Game => "game",
        ProjectType::Electronics => "electronics",
    }
}

fn load_project_scene(project: &Project) -> SceneGraph {
    let registry = ProjectSessionRegistry::load_or_legacy(&project.path, project.project_type);
    let Some(session) = registry.active() else {
        return SceneGraph::new();
    };
    SceneGraph::load_ron(&session.path(&project.path, &session.scene_file))
}

fn scene_document_fingerprint(scene: &SceneGraph) -> u64 {
    let semantic = agent_context::scene_fingerprint(scene);
    let encoded = serde_json::to_string(&semantic).unwrap_or_default();
    let mut hasher = DefaultHasher::new();
    encoded.hash(&mut hasher);
    hasher.finish()
}

fn scene_context(scene: &SceneGraph, selected: &[SceneNodeId]) -> Value {
    agent_context::scene_context(scene, selected)
}

const MAX_PROJECT_ASSETS: usize = 2_048;

fn project_asset_paths(root: &Path) -> Vec<String> {
    fn visit(root: &Path, current: &Path, output: &mut Vec<String>, depth: u8) {
        if depth > 24 || output.len() >= MAX_PROJECT_ASSETS {
            return;
        }
        let Ok(entries) = std::fs::read_dir(current) else {
            return;
        };
        for entry in entries.flatten() {
            if output.len() >= MAX_PROJECT_ASSETS {
                break;
            }
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if matches!(
                name.as_str(),
                ".git"
                    | ".aura_rafi"
                    | ".ai"
                    | ".codex"
                    | "target"
                    | "target_gnu"
                    | "node_modules"
                    | ".cache"
            ) {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_dir() {
                visit(root, &path, output, depth.saturating_add(1));
            } else if kind.is_file() {
                let relative = path
                    .strip_prefix(root)
                    .unwrap_or(path.as_path())
                    .to_string_lossy()
                    .replace('\\', "/");
                output.push(relative);
            }
        }
    }

    let assets_root = root.join("assets");
    let mut assets = Vec::new();
    if assets_root.is_dir() {
        visit(root, &assets_root, &mut assets, 0);
    }
    assets.sort_unstable();
    assets
}

fn used_asset_count(scene: &SceneGraph, assets: &[String]) -> usize {
    let used = scene
        .iter()
        .filter_map(|(_, node)| node.source_asset.as_deref())
        .map(normalize_asset_path)
        .collect::<Vec<_>>();
    assets
        .iter()
        .filter(|asset| {
            let asset = normalize_asset_path(asset);
            used.iter().any(|source| {
                asset == *source || asset.ends_with(source) || source.ends_with(&asset)
            })
        })
        .count()
}

fn normalize_asset_path(path: &str) -> String {
    path.trim()
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_ascii_lowercase()
}

fn observation_command_request(
    options: &Options,
    name: &str,
    fallback_params: Value,
) -> CliResult<EngineCommandRequest> {
    let params = if options.value("params").is_some() {
        parse_params(options)?
    } else {
        fallback_params
    };
    let mut request = EngineCommandRequest::new(name, params, CommandSource::Cli);
    apply_cli_controls(&mut request, options, false)?;
    Ok(request)
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
    let mut request = EngineCommandRequest::new(name, parse_params(options)?, CommandSource::Cli);
    apply_cli_controls(&mut request, options, false)?;
    Ok(request)
}

fn named_command_request(
    options: &Options,
    name: &str,
    force_dry_run: bool,
) -> CliResult<EngineCommandRequest> {
    let mut request = EngineCommandRequest::new(name, parse_params(options)?, CommandSource::Cli);
    apply_cli_controls(&mut request, options, force_dry_run)?;
    Ok(request)
}

fn parse_params(options: &Options) -> CliResult<Value> {
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
    Ok(params)
}

fn apply_cli_controls(
    request: &mut EngineCommandRequest,
    options: &Options,
    force_dry_run: bool,
) -> CliResult<()> {
    request.confirm = options.has("confirm");
    request.dry_run = force_dry_run || options.has("dry-run");
    request.expected_revision = options
        .value("expected-revision")
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| "expected-revision must be an integer.")
        })
        .transpose()?;
    request.idempotency_key = options.value("idempotency-key").map(str::to_string);
    request.budget = budget_from_options(options)?;
    Ok(())
}

fn budget_from_options(options: &Options) -> CliResult<Option<ExecutionBudget>> {
    let mut budget = options
        .value("budget")
        .map(|raw| {
            serde_json::from_str::<ExecutionBudget>(raw)
                .map_err(|error| format!("Invalid --budget JSON: {error}"))
        })
        .transpose()?
        .unwrap_or(ExecutionBudget {
            max_tool_calls: None,
            max_milliseconds: None,
            max_scene_operations: None,
            max_scene_entities: None,
            max_result_bytes: None,
            profile: None,
        });

    budget.max_tool_calls = parse_budget_u32(options, "max-tool-calls", budget.max_tool_calls)?;
    budget.max_milliseconds =
        parse_budget_u64(options, "max-milliseconds", budget.max_milliseconds)?;
    budget.max_scene_operations =
        parse_budget_u32(options, "max-scene-operations", budget.max_scene_operations)?;
    budget.max_scene_entities =
        parse_budget_u32(options, "max-scene-entities", budget.max_scene_entities)?;
    budget.max_result_bytes =
        parse_budget_u32(options, "max-result-bytes", budget.max_result_bytes)?;
    if let Some(profile) = options.value("budget-profile") {
        budget.profile = Some(profile.to_string());
    }

    let has_budget = budget.max_tool_calls.is_some()
        || budget.max_milliseconds.is_some()
        || budget.max_scene_operations.is_some()
        || budget.max_scene_entities.is_some()
        || budget.max_result_bytes.is_some()
        || budget.profile.is_some();
    Ok(has_budget.then_some(budget))
}

fn parse_budget_u32(options: &Options, name: &str, current: Option<u32>) -> CliResult<Option<u32>> {
    options
        .value(name)
        .map(|raw| {
            raw.parse::<u32>()
                .map_err(|_| format!("--{name} must be an unsigned integer."))
        })
        .transpose()
        .map(|value| value.or(current))
}

fn parse_budget_u64(options: &Options, name: &str, current: Option<u64>) -> CliResult<Option<u64>> {
    options
        .value(name)
        .map(|raw| {
            raw.parse::<u64>()
                .map_err(|_| format!("--{name} must be an unsigned integer."))
        })
        .transpose()
        .map(|value| value.or(current))
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
            for line in raf_core::agent_context::compact_result_lines(&response.lines) {
                println!("- {line}");
            }
            if !response.data.is_null() {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&raf_core::agent_context::compact_result_data(
                        &response.data,
                    ))
                    .map_err(|error| error.to_string())?
                );
            }
            if !response.warnings.is_empty() {
                println!("Warnings:");
                for warning in &response.warnings {
                    println!("- {warning}");
                }
            }
            if let Some(verification) = &response.verification {
                println!("Verification: {}", verification.status);
                for failure in &verification.failures {
                    println!("- {failure}");
                }
            }
            println!("Revision: {}", response.revision);
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
Usage:\n  raf doctor [--json]\n  raf editors [--json]                 list known projects and live editors\n  raf status [--project PATH] [--json]\n  raf attach [ACTION] [--project PATH]  PATH is optional when one editor is live\n  raf project create --name NAME [--parent PATH] [--type game|electronics] --confirm\n  raf project open PATH\n  raf project info [PATH]\n  raf capabilities [search QUERY]\n  raf session list [--project PATH]\n  raf workspace describe [PATH]\n  raf command NAME [--params JSON] [--dry-run] [--confirm]\n  raf mcp serve [--attach PATH]         PATH is optional when one editor is live\n  raf serve [--project PATH]            JSONL command endpoint\n\nGlobal output: --json, --ndjson, --format human|json"
    );
    println!("Project persistence: raf project save|checkpoint --confirm");
    println!("Budgets: --budget JSON or --max-tool-calls N --max-milliseconds N --max-scene-operations N --max-scene-entities N --max-result-bytes N --budget-profile NAME");
    println!("Additional inspection: raf context [--project PATH]");
    println!("Scene automation: raf scene outline|query|inspect|selection|verify|create|update|build|reconcile|repair|plan|apply|batch, raf selection, raf viewport capture, raf task list|get|events|cancel, raf assets search|inspect, raf scripts search, raf health");
}

/// Lists every known project that publishes an attach descriptor, marking
/// which editors are actually listening right now. Stale descriptors from
/// crashed sessions are reported as not alive instead of failing silently.
///
/// With `--wait[=SECONDS]` the scan retries every 400 ms until a live editor
/// appears or the budget expires, so automation can start before the engine.
/// The poll is bounded and only runs for this command's lifetime.
fn print_editors(
    extra: Option<&Path>,
    format: OutputFormat,
    wait_seconds: Option<f32>,
) -> CliResult<()> {
    const WAIT_POLL: std::time::Duration = std::time::Duration::from_millis(400);
    let deadline = wait_seconds.map(|seconds| {
        std::time::Instant::now() + std::time::Duration::from_secs_f32(seconds.max(0.5))
    });
    let editors = loop {
        let editors = discovery::discover_editors(extra.or(env::current_dir().ok().as_deref()));
        if !discovery::live_editors(&editors).is_empty() || deadline.is_none() {
            break editors;
        }
        if std::time::Instant::now() >= deadline.unwrap() {
            break editors;
        }
        std::thread::sleep(WAIT_POLL);
    };
    let live = discovery::live_editors(&editors).len();
    match format {
        OutputFormat::Human => {
            println!(
                "{live} live editor(s) across {} known project(s):",
                editors.len()
            );
            for editor in &editors {
                let state = if editor.alive { "LIVE" } else { "stale" };
                let session = editor
                    .descriptor
                    .session_name
                    .clone()
                    .unwrap_or_else(|| "-".to_string());
                let kind = editor
                    .descriptor
                    .project_type
                    .clone()
                    .unwrap_or_else(|| "?".to_string());
                println!(
                    "- [{state}] {} | type: {kind} | session: {} | pid {} | rev {}",
                    editor.project_path.display(),
                    session,
                    editor.descriptor.process_id,
                    editor.descriptor.revision
                );
            }
            if wait_seconds.is_some() && live == 0 {
                return Err("Timed out waiting for a live AuraRafi editor.".to_string());
            }
            Ok(())
        }
        OutputFormat::Json | OutputFormat::Ndjson => {
            let payload = json!({
                "live_count": live,
                "scanned": editors.len(),
                "timed_out": wait_seconds.is_some() && live == 0,
                "editors": editors.iter().map(DiscoveredEditor::to_json).collect::<Vec<_>>(),
            });
            println!(
                "{}",
                serde_json::to_string(&payload).map_err(|error| error.to_string())?
            );
            if wait_seconds.is_some() && live == 0 {
                return Err("Timed out waiting for a live AuraRafi editor.".to_string());
            }
            Ok(())
        }
    }
}
fn execute_task_cli<H: CommandEndpoint>(
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

fn run_attached_cli(options: &Options, format: OutputFormat) -> CliResult<()> {
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
                client.execute(named_command_request(options, "project.save", false)?)
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
            client.execute(named_command_request(options, "project.save", false)?)
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
                "spatial" | "spatial-map" => client.execute(observation_command_request(
                    options,
                    "scene.spatial_map",
                    json!({}),
                )?),
                "design" | "design-audit" | "layout-audit" => client.execute(
                    observation_command_request(options, "scene.design_audit", json!({}))?,
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

fn run_cli(arguments: Vec<String>) -> CliResult<()> {
    let options = parse_options(&arguments);
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
    if command == "mcp" && options.has("attach") {
        let attach_path = match options.path("attach").or_else(|| options.path("project")) {
            Some(path) => path,
            None => discovery::pick_single_live(None)?.project_path,
        };
        let client = AttachedClient::connect(&attach_path, "raf-mcp")?;
        return run_mcp_stdio(client);
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
                "spatial" | "spatial-map" => engine.execute(observation_command_request(
                    &options,
                    "scene.spatial_map",
                    json!({}),
                )?),
                "design" | "design-audit" | "layout-audit" => engine.execute(
                    observation_command_request(&options, "scene.design_audit", json!({}))?,
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
                engine.execute(named_command_request(&options, "project.save", false)?)
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
                {"uri": "raf://context", "name": "Compact project context", "mimeType": "application/json"},
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
        arguments = strip_mcp_controls(&params);
        generic_name
    } else {
        arguments = strip_mcp_controls(&arguments);
        command_name.to_string()
    };
    let mut request = EngineCommandRequest::new(command_name, arguments, CommandSource::Mcp);
    request.confirm = confirm;
    request.dry_run = dry_run || name == "raf_scene_plan";
    request.expected_revision = expected_revision;
    request.idempotency_key = idempotency_key;
    request.session = session;
    request.budget = budget;
    let response = engine.execute(request);
    let compact_lines = raf_core::agent_context::compact_result_lines(&response.lines);
    let text = compact_lines
        .first()
        .map(|line| format!("{}\n{}", response.title, line))
        .unwrap_or_else(|| response.title.clone());
    let mut structured = serde_json::to_value(&response).unwrap_or_else(|_| json!({"ok": false}));
    if let Some(object) = structured.as_object_mut() {
        object.insert(
            "data".to_string(),
            raf_core::agent_context::compact_result_data(&response.data),
        );
        object.insert("lines".to_string(), json!(compact_lines.clone()));
        object.insert(
            "summary".to_string(),
            json!(compact_lines
                .first()
                .cloned()
                .unwrap_or_else(|| response.title.clone())),
        );
    }
    Some(jsonrpc_result(
        id,
        json!({
            "isError": !response.ok,
            "content": [{"type": "text", "text": text}],
            "structuredContent": structured,
        }),
    ))
}

fn mcp_tool_command(name: &str) -> Option<&'static str> {
    match name {
        "raf_status" => Some("engine.status"),
        "raf_context" => Some("engine.context"),
        "raf_project_snapshot" => Some("engine.context"),
        "raf_scene_outline" => Some("scene.outline"),
        "raf_scene_query" => Some("scene.query"),
        "raf_scene_spatial_map" => Some("scene.spatial_map"),
        "raf_scene_overlaps" => Some("scene.overlaps"),
        "raf_scene_diff" => Some("scene.diff"),
        "raf_scene_design_audit" => Some("scene.design_audit"),
        "raf_scene_inspect" => Some("scene.inspect"),
        "raf_selection_get" => Some("selection.get"),
        "raf_viewport_capture" => Some("viewport.capture"),
        "raf_task_list" => Some("task.list"),
        "raf_task_get" => Some("task.get"),
        "raf_task_events" => Some("task.events"),
        "raf_task_cancel" => Some("task.cancel"),
        "raf_assets_search" => Some("assets.catalog"),
        "raf_assets_recommend" => Some("assets.recommend"),
        "raf_asset_inspect" => Some("assets.inspect"),
        "raf_scripts_search" => Some("scripts.catalog"),
        "raf_project_health" => Some("project.health"),
        "raf_scene_verify" => Some("scene.verify"),
        "raf_transaction_undo" => Some("transaction.undo"),
        "raf_scene_create" => Some("game.add"),
        "raf_scene_update" => Some("game.update"),
        "raf_scene_reparent" => Some("game.reparent"),
        "raf_scene_duplicate" => Some("game.duplicate"),
        "raf_scene_snap" => Some("game.snap"),
        "raf_scene_instantiate_template" => Some("game.generate_prefab"),
        "raf_scene_build" | "raf_scene_apply" | "raf_scene_plan" => Some("game.build"),
        "raf_scene_reconcile" => Some("game.reconcile"),
        "raf_scene_repair" => Some("game.repair"),
        "raf_scene_batch" => Some("game.batch"),
        "raf_diagnostics" => Some("project.health"),
        "raf_doctor" => Some("engine.doctor"),
        "raf_capabilities" => Some("capabilities.list"),
        "raf_capabilities_search" => Some("capabilities.search"),
        "raf_project_info" => Some("project.info"),
        "raf_project_open" => Some("project.open"),
        "raf_project_create" => Some("project.create"),
        "raf_project_save" => Some("project.save"),
        "raf_session_list" => Some("session.list"),
        "raf_workspace_describe" => Some("workspace.describe"),
        "raf_command" => Some("__generic__"),
        _ => None,
    }
}

fn strip_mcp_controls(arguments: &Value) -> Value {
    let Some(object) = arguments.as_object() else {
        return arguments.clone();
    };
    let mut params = object.clone();
    for key in [
        "confirm",
        "dry_run",
        "expected_revision",
        "idempotency_key",
        "session",
        "budget",
    ] {
        params.remove(key);
    }
    Value::Object(params)
}

fn mcp_tools() -> Vec<Value> {
    vec![
        mcp_tool(
            "raf_status",
            "Read headless engine and project status.",
            json!({"type":"object","properties":{}}),
        ),
        mcp_tool(
            "raf_context",
            "Read compact project, session, workspace, and scene context. The attached editor returns live hierarchy data.",
            json!({"type":"object","properties":{}}),
        ),
        mcp_tool(
            "raf_doctor",
            "Check the local Rafi CLI environment.",
            json!({"type":"object","properties":{}}),
        ),
        mcp_tool(
            "raf_capabilities",
            "List commands supported by the current headless engine or attached project domain.",
            json!({"type":"object","properties":{}}),
        ),
        mcp_tool(
            "raf_capabilities_search",
            "Search commands available in the current headless engine or attached project domain.",
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
            "raf_project_save",
            "Save the mounted project document and its active editor domain. Use confirm=true to commit; dry_run previews the write.",
            schema(Vec::new(), &[], true),
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
            "Call a supported project-scoped command by name. In attached mode this reaches the live editor and its real undo history.",
            json!({"type":"object","properties":{"name":{"type":"string"},"params":{"type":"object"},"confirm":{"type":"boolean"},"dry_run":{"type":"boolean"},"expected_revision":{"type":"integer","minimum":0},"idempotency_key":{"type":"string"},"session":{"type":"string"},"budget":budget_schema()},"required":["name"]}),
        ),
    ]
    .into_iter()
    .chain(mcp_semantic_tools())
    .collect()
}

fn mcp_semantic_tools() -> Vec<Value> {
    vec![
        mcp_tool(
            "raf_project_snapshot",
            "Read the compact project snapshot: active session, revision, scene identity, workspace and asset counts. Use this before authoring.",
            schema(Vec::new(), &[], false),
        ),
        mcp_tool(
            "raf_scene_outline",
            "Read the bounded scene hierarchy with UUID-first references, paths, parent/child links and compact transforms.",
            schema(
                vec![
                    ("root", json!({"type":"string","description":"Optional root ref, UUID, name or path to limit the outline."})),
                    ("cursor", json!({"type":"integer","minimum":0})),
                    ("limit", json!({"type":"integer","minimum":1,"maximum":128})),
                ],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_scene_query",
            "Find scene entities by name, path, asset, primitive or current selection without scanning project files.",
            schema(
                vec![
                    ("query", json!({"type":"string"})),
                    ("root", json!({"type":"string","description":"Optional root ref, UUID, name or path to limit the query."})),
                    ("primitive", json!({"type":"string","enum":["group","empty","cube","sphere","plane","cylinder"]})),
                    ("semantic_role", json!({"type":"string","description":"Exact role or parent role prefix."})),
                    ("tags", json!({"type":"array","items":{"type":"string"},"maxItems":32})),
                    ("match_all_tags", json!({"type":"boolean"})),
                    ("selected_only", json!({"type":"boolean"})),
                    ("include_hidden", json!({"type":"boolean"})),
                    ("cursor", json!({"type":"integer","minimum":0})),
                    ("limit", json!({"type":"integer","minimum":1,"maximum":128})),
                ],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_scene_spatial_map",
            "Read bounded world-space bounds, scene extents, visible renderable entities and conservative overlaps before layout changes.",
            schema(
                vec![
                    ("root", json!({"type":"string","description":"Optional root ref, UUID, name or path to limit the map."})),
                    ("include_hidden", json!({"type":"boolean"})),
                    ("check_collisions", json!({"type":"boolean"})),
                    ("cursor", json!({"type":"integer","minimum":0})),
                    ("limit", json!({"type":"integer","minimum":1,"maximum":128})),
                ],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_scene_overlaps",
            "Check bounded world-space overlaps and return penetration plus deterministic scene_snap repair suggestions.",
            schema(
                vec![
                    ("root", json!({"type":"string"})),
                    ("include_hidden", json!({"type":"boolean"})),
                    ("margin", json!({"type":"number","minimum":0})),
                    ("max_pairs", json!({"type":"integer","minimum":1,"maximum":256})),
                ],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_scene_diff",
            "Read retained created, updated and deleted scene UUIDs since a known revision.",
            schema(
                vec![("from_revision", json!({"type":"integer","minimum":0}))],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_scene_design_audit",
            "Audit whether a bounded scene scope reads as an intentional real-world place. This is read-only evidence for the next repair step.",
            schema(
                vec![
                    ("root", json!({"type":"string"})),
                    ("design_profile", json!({"type":"string","enum":["generic","real_world","building","store","supermarket","parking","outdoor"]})),
                    ("required_features", json!({"type":"array","items":{"type":"string","enum":["floor","enclosure","entrance","circulation","roof","primary_modules","details"]},"maxItems":7})),
                ],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_scene_inspect",
            "Inspect one entity by ref, UUID, path or exact name; selection is the fallback when target is omitted.",
            schema(
                vec![
                    ("target", json!({"type":"string"})),
                    ("targets", json!({"type":"array","items":{"type":"string"},"maxItems":64})),
                ],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_selection_get",
            "Read the live scene selection with UUID-first references and compact entity details.",
            schema(Vec::new(), &[], false),
        ),
        mcp_tool(
            "raf_viewport_capture",
            "Capture the attached Game viewport as a PNG artifact. Set refresh=true to request a fresh editor frame; this never starts Play mode.",
            schema(
                vec![("refresh", json!({"type":"boolean"}))],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_task_list",
            "List the current native Agent task in the attached editor.",
            schema(Vec::new(), &[], false),
        ),
        mcp_tool(
            "raf_task_get",
            "Read one native Agent task snapshot by id.",
            schema(vec![("id", json!({"type":"string","minLength":1}))], &["id"], false),
        ),
        mcp_tool(
            "raf_task_events",
            "Read retained native Agent task progress events after a sequence.",
            schema(vec![("since", json!({"type":"integer","minimum":0}))], &[], false),
        ),
        mcp_tool(
            "raf_task_cancel",
            "Request cooperative cancellation of the active native Agent task.",
            schema(vec![("id", json!({"type":"string","minLength":1}))], &["id"], false),
        ),
        mcp_tool(
            "raf_asset_inspect",
            "Inspect one imported asset by path and report kind, usage and scene references.",
            schema(
                vec![
                    ("target", json!({"type":"string","minLength":1})),
                ],
                &["target"],
                false,
            ),
        ),
        mcp_tool(
            "raf_assets_search",
            "Search the project asset catalog and report whether each asset is used by the live scene.",
            schema(
                vec![
                    ("query", json!({"type":"string"})),
                    ("usage", json!({"type":"string","enum":["all","used","unused"]})),
                    ("kind", json!({"type":"string","enum":["image","model","audio","script","data","file"]})),
                    ("cursor", json!({"type":"integer","minimum":0})),
                    ("limit", json!({"type":"integer","minimum":1,"maximum":128})),
                ],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_assets_recommend",
            "Rank imported project assets for an authoring intent and explain every match.",
            schema(
                vec![
                    ("intent", json!({"type":"string","minLength":1})),
                    ("kind", json!({"type":"string","enum":["image","model","audio","script","data","file"]})),
                    ("limit", json!({"type":"integer","minimum":1,"maximum":24})),
                ],
                &["intent"],
                false,
            ),
        ),
        mcp_tool(
            "raf_scripts_search",
            "List scripts attached to scene entities, bounded and searchable by path.",
            schema(
                vec![
                    ("query", json!({"type":"string"})),
                    ("cursor", json!({"type":"integer","minimum":0})),
                    ("limit", json!({"type":"integer","minimum":1,"maximum":128})),
                ],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_project_health",
            "Check a bounded scene scope for identity, duplicate names, hierarchy reachability and missing asset references. Use root to avoid unrelated legacy scenes.",
            schema(
                vec![
                    ("root", json!({"type":"string"})),
                    ("strict", json!({"type":"boolean"})),
                ],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_diagnostics",
            "Run the compact project health diagnostics used before and after authoring.",
            schema(Vec::new(), &[], false),
        ),
        mcp_tool(
            "raf_scene_verify",
            "Verify targets, hierarchy, transforms, expected counts and optional AABB collisions after a mutation. Scope checks with root and never infer success from model text alone.",
            schema(
                vec![
                    ("root", json!({"type":"string"})),
                    ("target", json!({"type":"string"})),
                    ("targets", json!({"type":"array","items":{"type":"string"}})),
                    ("expected_count", json!({"type":"integer","minimum":0})),
                    ("expected_names", json!({"type":"array","items":{"type":"string"},"maxItems":128})),
                    ("expected", json!({
                        "type":"object",
                        "properties": {
                            "primitive": {"type":"string","enum":["empty","cube","sphere","plane","cylinder","group"]},
                            "transform": transform_schema(),
                            "color_rgba": {"type":"array","minItems":3,"maxItems":4,"items":{"type":"integer","minimum":0,"maximum":255}},
                            "parent": {"type":"string"}
                        },
                        "additionalProperties": false
                    })),
                    ("check_collisions", json!({"type":"boolean"})),
                    ("strict", json!({"type":"boolean"})),
                ],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_transaction_undo",
            "Roll back the exact attached Game scene mutation represented by a scoped undo token.",
            schema(
                vec![("token", json!({"type":"string","minLength":1}))],
                &["token"],
                true,
            ),
        ),
        mcp_tool(
            "raf_scene_create",
            "Create one semantic scene entity through the shared game command kernel. Use a stable parent ref when possible.",
            schema(scene_create_properties(), &["name"], true),
        ),
        mcp_tool(
            "raf_scene_update",
            "Update one scene entity by stable ref, UUID, path or exact name. Unspecified transform fields remain unchanged.",
            schema(scene_update_properties(), &["target"], true),
        ),
        mcp_tool(
            "raf_scene_reparent",
            "Move an existing scene subtree to a new group while preserving its world transform by default.",
            schema(
                vec![
                    ("target", json!({"type":"string"})),
                    ("parent", json!({"type":"string"})),
                    ("preserve_world", json!({"type":"boolean"})),
                ],
                &["target"],
                true,
            ),
        ),
        mcp_tool(
            "raf_scene_duplicate",
            "Duplicate a scene subtree into an optional parent or repeated offset array.",
            schema(
                vec![
                    ("target", json!({"type":"string"})),
                    ("parent", json!({"type":"string"})),
                    ("name", json!({"type":"string"})),
                    ("offset", json!({"type":"array","minItems":3,"maxItems":3,"items":{"type":"number"}})),
                    ("count", json!({"type":"integer","minimum":1,"maximum":128})),
                    ("axis", json!({"type":"string","enum":["x","y","z"]})),
                    ("spacing", json!({"type":"number"})),
                    ("preserve_world", json!({"type":"boolean"})),
                ],
                &["target"],
                true,
            ),
        ),
        mcp_tool(
            "raf_scene_snap",
            "Snap an entity to the grid, world floor, or another entity's world bounds.",
            schema(
                vec![
                    ("target", json!({"type":"string"})),
                    ("mode", json!({"type":"string","enum":["grid","floor","surface"]})),
                    ("snap_to", json!({"type":"string"})),
                    ("axis", json!({"type":"string","enum":["x","y","z"]})),
                    ("placement", json!({"type":"string","enum":["before","after","center"]})),
                    ("gap", json!({"type":"number","minimum":0})),
                    ("grid", json!({"type":"number","exclusiveMinimum":0})),
                ],
                &["target", "mode"],
                true,
            ),
        ),
        mcp_tool(
            "raf_scene_instantiate_template",
            "Instantiate one or more registered native scene templates with parent and transform controls.",
            schema(
                vec![
                    ("kind", json!({"type":"string"})),
                    ("name", json!({"type":"string"})),
                    ("parent", json!({"type":"string"})),
                    ("transform", transform_schema()),
                    ("count", json!({"type":"integer","minimum":1,"maximum":64})),
                    ("axis", json!({"type":"string","enum":["x","y","z"]})),
                    ("spacing", json!({"type":"number"})),
                ],
                &["kind"],
                true,
            ),
        ),
        mcp_tool(
            "raf_scene_build",
            "Apply an ordered semantic scene build through the shared kernel. For real-world places, build the named envelope, floor, boundaries, entrances and circulation before repeated modules and details; use stable_key and semantic_role. Prefer this for multi-entity construction and use dry_run first.",
            schema(
                vec![
                    ("parent", json!({"type":"string","description":"Optional existing group that receives the generated structure."})),
                    ("design_profile", json!({"type":"string","enum":["generic","real_world","building","supermarket","parking","outdoor"],"description":"Optional contract that rejects incomplete real-world structure before mutation."})),
                    ("groups", json!({"type":"array","maxItems":32,"items":scene_group_input_schema()})),
                    ("entities", json!({"type":"array","maxItems":128,"items":scene_entity_input_schema()})),
                ],
                &[],
                true,
            ),
        ),
        mcp_tool(
            "raf_scene_reconcile",
            "Apply a keyed desired scene state. Existing stable_key values are updated instead of duplicated; use dry_run first.",
            scene_reconcile_input_schema(),
        ),
        mcp_tool(
            "raf_scene_repair",
            "Apply explicit audit-driven scene repairs atomically inside a bounded scope. The server never invents geometry from prose; use the audit and spatial map first.",
            schema(
                vec![
                    ("root", json!({"type":"string","description":"Optional audited root ref, UUID, name or path."})),
                    ("operations", json!({"type":"array","minItems":1,"maxItems":512,"items":batch_operation_input_schema()})),
                ],
                &["operations"],
                true,
            ),
        ),
        mcp_tool(
            "raf_scene_plan",
            "Preview a semantic scene build without mutating the editor. The server forces dry_run=true.",
            schema(
                vec![
                    ("parent", json!({"type":"string","description":"Optional existing group that receives the generated structure."})),
                    ("design_profile", json!({"type":"string","enum":["generic","real_world","building","supermarket","parking","outdoor"],"description":"Optional contract used by the preview validator."})),
                    ("groups", json!({"type":"array","maxItems":32,"items":scene_group_input_schema()})),
                    ("entities", json!({"type":"array","maxItems":128,"items":scene_entity_input_schema()})),
                ],
                &[],
                true,
            ),
        ),
        mcp_tool(
            "raf_scene_apply",
            "Apply a previously planned semantic scene build through the shared kernel after approval.",
            schema(
                vec![
                    ("parent", json!({"type":"string","description":"Optional existing group that receives the generated structure."})),
                    ("design_profile", json!({"type":"string","enum":["generic","real_world","building","supermarket","parking","outdoor"],"description":"Optional contract checked before applying the planned structure."})),
                    ("groups", json!({"type":"array","maxItems":32,"items":scene_group_input_schema()})),
                    ("entities", json!({"type":"array","maxItems":128,"items":scene_entity_input_schema()})),
                ],
                &[],
                true,
            ),
        ),
        mcp_tool(
            "raf_scene_batch",
            "Apply a bounded ordered batch of semantic scene operations atomically; use only for operations that belong together.",
            schema(
                vec![("operations", json!({"type":"array","minItems":1,"maxItems":512,"items":batch_operation_input_schema()}))],
                &["operations"],
                true,
            ),
        ),
    ]
}

fn scene_create_properties() -> Vec<(&'static str, Value)> {
    vec![
        ("name", json!({"type":"string","minLength":1})),
        (
            "kind",
            json!({"type":"string","enum":["empty","cube","sphere","plane","cylinder"]}),
        ),
        (
            "primitive",
            json!({"type":"string","enum":["empty","cube","sphere","plane","cylinder"]}),
        ),
        ("parent", json!({"type":"string"})),
        ("transform", transform_schema()),
        ("position", vec3_schema()),
        ("rotation_deg", vec3_schema()),
        ("scale", vec3_schema()),
        (
            "color_rgba",
            json!({"type":"array","minItems":3,"maxItems":4,"items":{"type":"integer","minimum":0,"maximum":255}}),
        ),
        (
            "visible",
            json!({"type":"boolean","description":"Entity visibility."}),
        ),
        (
            "locked",
            json!({"type":"boolean","description":"Editor lock state."}),
        ),
        ("semantic_role", json!({"type":"string"})),
        ("stable_key", json!({"type":"string"})),
        (
            "tags",
            json!({"type":"array","maxItems":32,"items":{"type":"string","maxLength":64}}),
        ),
        ("agent_origin", json!({"type":"string"})),
    ]
}

fn scene_update_properties() -> Vec<(&'static str, Value)> {
    let mut properties = scene_create_properties();
    properties.retain(|(name, _)| {
        *name != "name" && *name != "kind" && *name != "primitive" && *name != "parent"
    });
    properties.insert(0, ("target", json!({"type":"string","minLength":1})));
    properties
}

fn scene_group_input_schema() -> Value {
    schema(
        vec![
            ("name", json!({"type":"string","minLength":1})),
            ("parent", json!({"type":"string"})),
            ("transform", transform_schema()),
            ("semantic_role", json!({"type":"string"})),
            ("stable_key", json!({"type":"string"})),
            (
                "tags",
                json!({"type":"array","maxItems":32,"items":{"type":"string","maxLength":64}}),
            ),
            ("agent_origin", json!({"type":"string"})),
        ],
        &["name"],
        false,
    )
}

fn scene_entity_input_schema() -> Value {
    schema(scene_create_properties(), &["name", "kind"], false)
}

fn scene_reconcile_input_schema() -> Value {
    let mut schema = schema(
        vec![
            (
                "parent",
                json!({"type":"string","description":"Optional existing group that receives the keyed structure."}),
            ),
            (
                "design_profile",
                json!({"type":"string","enum":["generic","real_world","building","supermarket","parking","outdoor"],"description":"Optional contract that rejects incomplete real-world structure before reconciliation."}),
            ),
            (
                "groups",
                json!({"type":"array","maxItems":32,"items":scene_reconcile_group_input_schema()}),
            ),
            (
                "entities",
                json!({"type":"array","maxItems":128,"items":scene_reconcile_entity_input_schema()}),
            ),
        ],
        &[],
        true,
    );
    schema["description"] = Value::String(
        "Keyed desired scene state. Existing stable_key values are updated instead of duplicated."
            .to_string(),
    );
    schema["properties"]["groups"]["items"]["required"] = json!(["name", "stable_key"]);
    schema["properties"]["entities"]["items"]["required"] = json!(["kind", "name", "stable_key"]);
    schema
}

fn scene_reconcile_group_input_schema() -> Value {
    let mut value = scene_group_input_schema();
    value["required"] = json!(["name", "stable_key"]);
    value
}

fn scene_reconcile_entity_input_schema() -> Value {
    let mut value = scene_entity_input_schema();
    value["required"] = json!(["kind", "name", "stable_key"]);
    value
}

fn batch_operation_input_schema() -> Value {
    schema(
        vec![
            (
                "name",
                json!({
                    "type":"string",
                    "enum":["scene_create","scene_create_group","scene_update","scene_delete","scene_duplicate","scene_arrange","scene_reparent","scene_snap","scene_instantiate_template"]
                }),
            ),
            ("params", batch_operation_params_schema()),
        ],
        &["name", "params"],
        false,
    )
}

/// Keep nested batch operations useful to external models too. A generic
/// `params: object` tells a provider nothing about transforms, targets or
/// semantic identity, which is exactly where tool calls used to drift.
fn batch_operation_params_schema() -> Value {
    let mut properties = scene_create_properties();
    properties.extend([
        ("target", json!({"type":"string","description":"Entity or group target."})),
        ("preserve_world", json!({"type":"boolean","description":"Keep world transform during reparenting."})),
        ("parent", json!({"type":"string","description":"Optional destination parent."})),
        ("name", json!({"type":"string","description":"Optional generated name."})),
        ("offset", json!({"type":"array","minItems":3,"maxItems":3,"items":{"type":"number"},"description":"World/local offset for repeated operations."})),
        ("count", json!({"type":"integer","minimum":1,"maximum":128})),
        ("axis", json!({"type":"string","enum":["x","y","z"]})),
        ("spacing", json!({"type":"number","minimum":-10000.0,"maximum":10000.0,"description":"Grid or repetition spacing."})),
        ("mode", json!({"type":"string","enum":["grid","floor","surface"]})),
        ("snap_to", json!({"type":"string","description":"Surface target for scene_snap."})),
        ("placement", json!({"type":"string","enum":["before","after","center"]})),
        ("gap", json!({"type":"number","minimum":0})),
        ("grid", json!({"type":"number","exclusiveMinimum":0})),
        ("factor", json!({"type":"number","minimum":-10000.0,"maximum":10000.0,"description":"Scale factor."})),
        ("x", json!({"type":"number","minimum":-100000.0,"maximum":100000.0})),
        ("y", json!({"type":"number","minimum":-100000.0,"maximum":100000.0})),
        ("z", json!({"type":"number","minimum":-100000.0,"maximum":100000.0})),
        ("rx", json!({"type":"number","minimum":-36000.0,"maximum":36000.0})),
        ("ry", json!({"type":"number","minimum":-36000.0,"maximum":36000.0})),
        ("rz", json!({"type":"number","minimum":-36000.0,"maximum":36000.0})),
        ("sx", json!({"type":"number","minimum":-10000.0,"maximum":10000.0})),
        ("sy", json!({"type":"number","minimum":-10000.0,"maximum":10000.0})),
        ("sz", json!({"type":"number","minimum":-10000.0,"maximum":10000.0})),
    ]);
    schema(properties, &[], false)
}

fn transform_schema() -> Value {
    json!({
        "type":"object",
        "properties": {"position": vec3_schema(), "rotation_deg": vec3_schema(), "scale": vec3_schema()},
        "additionalProperties": false
    })
}

fn vec3_schema() -> Value {
    json!({"type":"array","minItems":3,"maxItems":3,"items":{"type":"number"}})
}

fn schema(properties: Vec<(&str, Value)>, required: &[&str], controls: bool) -> Value {
    let mut all = serde_json::Map::new();
    for (name, property) in properties {
        all.insert(name.to_string(), property);
    }
    if controls {
        all.insert("confirm".to_string(), json!({"type":"boolean"}));
        all.insert("dry_run".to_string(), json!({"type":"boolean"}));
        all.insert(
            "expected_revision".to_string(),
            json!({"type":"integer","minimum":0}),
        );
        all.insert("idempotency_key".to_string(), json!({"type":"string"}));
        all.insert("session".to_string(), json!({"type":"string"}));
        all.insert("budget".to_string(), budget_schema());
    }
    let mut result = json!({"type":"object","properties":all,"additionalProperties":false});
    if !required.is_empty() {
        result["required"] = json!(required);
    }
    result
}

fn budget_schema() -> Value {
    json!({
        "type": "object",
        "description": "Bound the agent operation and result before execution.",
        "properties": {
            "max_tool_calls": {"type":"integer","minimum":1,"maximum":10000},
            "max_milliseconds": {"type":"integer","minimum":1,"maximum":3_600_000},
            "max_scene_operations": {"type":"integer","minimum":1,"maximum":10000},
            "max_scene_entities": {"type":"integer","minimum":1,"maximum":1000000},
            "max_result_bytes": {"type":"integer","minimum":256,"maximum":1_048_576},
            "profile": {"type":"string","enum":["safe","balanced","large"]}
        },
        "additionalProperties": false
    })
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
        assert!(tools["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "raf_context"));
        assert!(tools["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "raf_capabilities"));
        let tools = tools["result"]["tools"].as_array().unwrap();
        for name in [
            "raf_scene_query",
            "raf_scene_reconcile",
            "raf_selection_get",
            "raf_viewport_capture",
            "raf_task_list",
            "raf_task_get",
            "raf_task_events",
            "raf_task_cancel",
            "raf_asset_inspect",
            "raf_transaction_undo",
        ] {
            assert!(
                tools.iter().any(|tool| tool["name"] == name),
                "missing MCP tool {name}"
            );
        }
        let reconcile = tools
            .iter()
            .find(|tool| tool["name"] == "raf_scene_reconcile")
            .unwrap();
        assert_eq!(reconcile["inputSchema"]["additionalProperties"], false);
        assert_eq!(
            reconcile["inputSchema"]["properties"]["confirm"]["type"],
            "boolean"
        );
        assert_eq!(
            reconcile["inputSchema"]["properties"]["groups"]["items"]["required"],
            json!(["name", "stable_key"])
        );
        let stripped = strip_mcp_controls(&json!({
            "confirm": true,
            "dry_run": false,
            "name": "Shelf",
            "stable_key": "store.shelf"
        }));
        assert_eq!(stripped, json!({"name":"Shelf","stable_key":"store.shelf"}));
    }

    #[test]
    fn headless_context_is_compact_and_does_not_scan_internal_metadata() {
        let root = std::env::temp_dir().join(format!(
            "raf-cli-context-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(&root).unwrap();
        let project = Project::create("ContextDemo", ProjectType::Game, &root).unwrap();
        let mut engine = HeadlessEngine::new(Some(&project.path)).unwrap();
        let response = engine.execute(EngineCommandRequest::new(
            "engine.context",
            json!({}),
            CommandSource::Cli,
        ));
        assert!(response.ok, "{}", response.lines.join(" "));
        assert_eq!(response.data["project"]["name"], "ContextDemo");
        assert!(response.data["scene"]["outline"].is_array());
        assert_eq!(
            response.data["workspace"]["internal_metadata_skipped"],
            true
        );
        let _ = fs::remove_dir_all(root);
    }
}
