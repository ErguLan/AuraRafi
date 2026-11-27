//! Headless engine implementation and command routing.

use crate::cli::commands::support::{
    capability_value, count_workspace_entries, headless_capability_values, load_project,
    load_project_scene, project_asset_paths, project_type_label, scene_context,
    scene_document_fingerprint, string_param, used_asset_count, valid_project_name,
};
use crate::cli::mcp::read_plan_resource;
use crate::cli::{CliResult, PROTOCOL_VERSION, PROGRAM};
use raf_core::agent_context::{self, AgentObservationResult};
use raf_core::capabilities::CapabilityCatalog;
use raf_core::performance::BenchmarkStore;
use raf_core::project::{Project, ProjectType};
use raf_core::session::ProjectSessionRegistry;
use raf_core::transaction::{
    ArtifactRef, TransactionId, TransactionLedger, VerificationSummary,
};
use raf_core::{CommandEndpoint, EngineCommandRequest, EngineCommandResponse};
use serde_json::json;
use serde_json::Value;
use std::env;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub(crate) struct HeadlessEngine {
    pub(crate) project: Option<Project>,
    catalog: CapabilityCatalog,
    ledger: TransactionLedger,
}

impl HeadlessEngine {
    pub(crate) fn new(project_path: Option<&Path>) -> CliResult<Self> {
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

    pub(crate) fn project_path(&self) -> Option<&Path> {
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

    fn execute_project_close(&mut self, request: &EngineCommandRequest) -> EngineCommandResponse {
        let Some(project) = self.project.take() else {
            return self.error(request.id, "Project close", "No project is open.");
        };
        self.ledger = TransactionLedger::default();
        self.response(
            request.id,
            "Project closed",
            vec![format!("Closed {}.", project.path.display())],
            json!({"closed": true, "path": project.path, "name": project.name}),
            true,
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

    fn execute_session_use(&self, request: &EngineCommandRequest) -> EngineCommandResponse {
        let Some(project) = &self.project else {
            return self.error(request.id, "Sessions", "No project is open.");
        };
        let target = string_param(&request.params, "id")
            .or_else(|| string_param(&request.params, "session"))
            .or_else(|| string_param(&request.params, "name"));
        let Some(target) = target else {
            return self.error(
                request.id,
                "Session use",
                "Provide id, session or name for the session to activate.",
            );
        };
        let mut registry = ProjectSessionRegistry::load_or_legacy(&project.path, project.project_type);
        let Some(session) = registry.sessions.iter().find(|session| {
            session.id.0.to_string() == target
                || session.name.eq_ignore_ascii_case(target)
        }) else {
            return self.error(
                request.id,
                "Session use",
                format!(
                    "Session '{target}' was not found. Available: {}.",
                    registry
                        .sessions
                        .iter()
                        .map(|session| session.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            );
        };
        let selected = session.id;
        let selected_name = session.name.clone();
        registry.active_session = selected;
        if let Err(error) = registry.save(&project.path) {
            return self.error(request.id, "Session use", error);
        }
        self.response(
            request.id,
            "Session activated",
            vec![format!("Active session: {selected_name}.")],
            json!({
                "active_session": selected,
                "name": selected_name,
            }),
            true,
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

    fn execute_benchmark_list(&self, request: &EngineCommandRequest) -> EngineCommandResponse {
        let Some(root) = self.project_path() else {
            return self.error(
                request.id,
                "Performance benchmark",
                "Benchmark history requires a mounted project. Pass --project PATH or run \
                 `raf perf` with a live editor.",
            );
        };
        match BenchmarkStore::for_project(root).list() {
            Ok(benchmarks) => {
                let mut lines: Vec<String> = benchmarks
                    .iter()
                    .take(6)
                    .map(|benchmark| {
                        format!(
                            "{} — {:.1} FPS avg, p95 {:.2} ms ({})",
                            benchmark.id,
                            benchmark.fps_avg,
                            benchmark.frame_p95,
                            benchmark.verdict
                        )
                    })
                    .collect();
                if lines.is_empty() {
                    lines.push("No saved benchmarks yet.".to_string());
                }
                self.response(
                    request.id,
                    "Benchmarks",
                    lines,
                    json!({"count": benchmarks.len(), "benchmarks": benchmarks}),
                    false,
                    None,
                )
            }
            Err(error) => self.error(request.id, "Performance benchmark", error),
        }
    }

    fn execute_benchmark_compare(&self, request: &EngineCommandRequest) -> EngineCommandResponse {
        let (Some(left), Some(right)) = (
            string_param(&request.params, "a"),
            string_param(&request.params, "b"),
        ) else {
            return self.error(
                request.id,
                "Performance benchmark",
                "performance.benchmark.compare requires 'a' and 'b' benchmark ids.",
            );
        };
        let Some(root) = self.project_path() else {
            return self.error(
                request.id,
                "Performance benchmark",
                "Benchmark history requires a mounted project. Pass --project PATH or run \
                 `raf perf` with a live editor.",
            );
        };
        match BenchmarkStore::for_project(root).compare(left, right) {
            Ok(comparison) => {
                let mut lines = vec![format!(
                    "{} → {}: {} | FPS {:+.1}% | p95 frame {:+.1}%",
                    comparison.left.id,
                    comparison.right.id,
                    comparison.verdict,
                    comparison.fps.delta_percent,
                    comparison.frame_p95.delta_percent
                )];
                lines.extend(comparison.warnings.clone());
                self.response(
                    request.id,
                    "Benchmark comparison",
                    lines,
                    json!(comparison),
                    false,
                    None,
                )
            }
            Err(error) => self.error(request.id, "Performance benchmark", error),
        }
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
                    | "scene.digest"
                    | "scene.query"
                    | "scene.spatial_map"
                    | "scene.spatial"
                    | "scene.overlaps"
                    | "scene.check_overlaps"
                    | "scene.navigation_audit"
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
            "scene.digest" => agent_context::scene_digest(&scene, &assets, &request.params, &[]),
            "scene.query" => agent_context::scene_query(&scene, &request.params, &[]),
            "scene.spatial_map" | "scene.spatial" => {
                agent_context::scene_spatial_map(&scene, &request.params, &[])
            }
            "scene.overlaps" | "scene.check_overlaps" => {
                agent_context::scene_check_overlaps(&scene, &request.params, &[])
            }
            "scene.navigation_audit" => {
                agent_context::scene_navigation_audit(&scene, &request.params, &[])
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
        let hardware = raf_core::hardware::report();
        let hardware_line = hardware_summary(&hardware);
        self.response(
            request.id,
            "Rafi doctor",
            vec![
                if rust_project {
                    "CLI environment looks usable.".to_string()
                } else {
                    "CLI started outside a Rust workspace.".to_string()
                },
                hardware_line,
            ],
            json!({
                "cwd": cwd,
                "cargo_manifest": rust_project,
                "catalog_version": self.catalog.version,
                "capability_count": self.catalog.capabilities.len(),
                "protocol": raf_core::COMMAND_PROTOCOL_VERSION,
                "mcp_protocol": PROTOCOL_VERSION,
                "hardware": hardware,
            }),
            false,
            None,
        )
    }

    pub(crate) fn resource(&self, uri: &str) -> CliResult<Value> {
        if let Some(id) = uri.strip_prefix("raf://plan/") {
            let Some(root) = self.project_path() else {
                return Err("No project is mounted.".to_string());
            };
            return read_plan_resource(root, id);
        }
        match uri {
            "raf://project" => Ok(self
                .project
                .as_ref()
                .map(|project| self.project_info(project))
                .unwrap_or_else(|| json!({"project": null}))),
            "raf://capabilities" => Ok(self.catalog.as_json()),
            "raf://brief" => {
                let Some(root) = self.project_path() else {
                    return Ok(json!({"found": false, "brief": null}));
                };
                let path = root.join(".aura_rafi/world/brief.json");
                let content = std::fs::read(&path).ok();
                let brief = content
                    .as_deref()
                    .and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok())
                    .unwrap_or(Value::Null);
                Ok(json!({
                    "found": content.is_some(),
                    "path": ".aura_rafi/world/brief.json",
                    "brief": brief,
                }))
            }
            "raf://plans" => {
                let Some(root) = self.project_path() else {
                    return Ok(json!({"plans": []}));
                };
                let directory = root.join(".aura_rafi/world/plans");
                let mut plans = Vec::new();
                if let Ok(entries) = std::fs::read_dir(&directory) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                            continue;
                        }
                        let Ok(bytes) = std::fs::read(&path) else {
                            continue;
                        };
                        let Ok(plan) = serde_json::from_slice::<Value>(&bytes) else {
                            continue;
                        };
                        plans.push(json!({
                            "id": plan.get("id").cloned().unwrap_or(Value::Null),
                            "status": plan.get("status").cloned().unwrap_or(Value::Null),
                            "mode": plan.get("mode").cloned().unwrap_or(Value::Null),
                            "groups": plan.get("groups").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
                            "entities": plan.get("entities").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
                            "updates": plan.get("updates").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
                            "deletions": plan.get("deletions").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
                        }));
                    }
                }
                plans.sort_by(|left, right| {
                    right
                        .get("id")
                        .and_then(Value::as_str)
                        .cmp(&left.get("id").and_then(Value::as_str))
                });
                plans.truncate(64);
                Ok(json!({"count": plans.len(), "plans": plans}))
            }
            "raf://digest" => {
                let Some(project) = &self.project else {
                    return Ok(json!({"digest": null}));
                };
                let scene = load_project_scene(project);
                let digest = agent_context::scene_digest(&scene, &[], &json!({}), &[]);
                Ok(json!({"digest": digest.data, "revision": self.ledger.revision()}))
            }
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

/// One-line machine summary for `doctor`. The OS constant is lowercase
/// (`windows`), so common values get their display spelling.
fn hardware_summary(hardware: &raf_core::hardware::HardwareReport) -> String {
    let os = match hardware.os.as_str() {
        "windows" => "Windows",
        "macos" => "macOS",
        "linux" => "Linux",
        other => other,
    };
    let ram = hardware
        .ram_total_mb
        .map(|megabytes| format!("{megabytes} MB RAM"))
        .unwrap_or_else(|| "unknown RAM".to_string());
    format!("{os} · {} cores · {ram}", hardware.cpu_cores)
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
            "project.close" => self.execute_project_close(&request),
            "project.info" => self.execute_project_info(&request),
            "project.save" => self.execute_project_save(&request),
            "session.list" => self.execute_session_list(&request),
            "session.use" | "session.activate" => self.execute_session_use(&request),
            "workspace.describe" => self.execute_workspace_describe(&request),
            "engine.status" | "status" => self.execute_status(&request),
            "engine.context" | "agent.context" | "project.summary" => {
                self.execute_context(&request)
            }
            "scene.outline"
            | "scene.digest"
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
            "performance.stats"
            | "performance.benchmark.start"
            | "performance.benchmark.status"
            | "performance.benchmark.stop"
            | "performance.benchmark.cancel" => self.error(
                request.id,
                "Performance telemetry unavailable",
                "Performance telemetry requires an attached editor viewport. Run `raf perf` with \
                 a live editor.",
            ),
            "performance.benchmark.list" => self.execute_benchmark_list(&request),
            "performance.benchmark.compare" => self.execute_benchmark_compare(&request),
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
