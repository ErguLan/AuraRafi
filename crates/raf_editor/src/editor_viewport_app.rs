//! Minimal editor application boundary after the editor-chrome teardown.
//!
//! Loading and Project Hub remain available as transitional entry surfaces.
//! Once a project opens, Game and Electronics keep the client area dedicated
//! to their viewport/CAD canvas while the beta downbar owns compact utility
//! surfaces. Hierarchy is mounted as a retained RafUI/ApiGraphicBasic panel;
//! Inspector is mounted beside the retained Hierarchy surface.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::Duration;

use eframe::{egui, egui_wgpu};
use raf_core::config::{EngineSettings, Theme};
use raf_core::i18n::t;
use raf_core::ipc::command_protocol_is_current;
use raf_core::project::{Project, ProjectType, RecentProjects};
use raf_core::scene::{Collider, NodeColor, Primitive, SceneGraph, SceneNodeId, VariableValue};
use raf_core::session::ProjectSessionRegistry;
use raf_core::{
    EngineCommandRequest, EngineCommandResponse, TransactionId, UndoToken, VerificationSummary,
};
use raf_electronics::{PcbLayout, Schematic};
use raf_render::api_graphic_basic::device::SharedGraphicsContext;
use raf_render::bridge::{
    EditorCameraBlock, EditorCameraBookmark, GraphicsSurfaceKind, RenderRuntime,
    RenderRuntimeSnapshot,
};
use raf_render::render_config::RenderConfig;
use raf_render::WorldStreamConfig;
use raf_ui::UiWindowCommand;
use raf_ui::{BottomDockLayout, StudioUiPalette};
use serde_json::Value;

use crate::agent_executor::{AgentEditorAction, AgentProjectContext, AgentToolExecutor};
use crate::application_bar_host::ApplicationBarHost;
use crate::application_bar_surface::AgentBarStatus;
use crate::application_menu::{
    command as application_command, ApplicationMenuState, ApplicationView,
};
use crate::attached::{AttachedCommandHost, PendingAttachedCommand};
use crate::commands::game::{GameViewportPort, SceneSelectionState};
use crate::commands::{
    self, parse_console_input, CommandCatalog, CommandOutput, ParsedCommand, ParsedInput,
};
use crate::console::{ConsolePanel, ConsoleSubmission};
use crate::electronics_history::{ElectronicsDocumentSnapshot, ElectronicsHistory};
use crate::panels::agent_surface::AgentSurfaceHost;
use crate::panels::ai_chat::{AgentAction, AgentPanel};
use crate::panels::editor_bottom_dock_host::{sanitize_layout_for_project, EditorBottomDockHost};
use crate::panels::editor_bottom_dock_surface::AssetsIntent;
use crate::panels::electronics_inspector_surface::{
    ElectronicsInspectorAction, ElectronicsInspectorSurfaceHost,
};
use crate::panels::electronics_navigator_surface::{
    ElectronicsNavigatorAction, ElectronicsNavigatorSurfaceHost,
};
use crate::panels::electronics_surface::{
    ElectronicsAnalysisSurfaceAction, ElectronicsAnalysisSurfaceHost,
};
use crate::panels::electronics_toolbar_surface::{
    ElectronicsToolbarAction, ElectronicsToolbarSurfaceHost,
};
use crate::panels::hierarchy_surface_host::{HierarchyIntent, HierarchySurfaceHost};
use crate::panels::hub_surface_host::{HubSurfaceHost, HubSurfaceIntent};
use crate::panels::inspector_surface_host::{InspectorIntent, InspectorSurfaceHost};
use crate::panels::loading_surface::LoadingSurfaceHost;
use crate::panels::new_project_surface::{NewProjectSurfaceAction, NewProjectSurfaceHost};
use crate::panels::nodes_surface::{NodeEditorDocument, NodesIntent};
use crate::panels::pcb_view::PcbViewPanel;
use crate::panels::schematic_view::SchematicViewPanel;
use crate::panels::search_surface::{SearchResult, SearchResultKind};
use crate::panels::search_surface_host::{SearchIntent, SearchSurfaceHost};
use crate::panels::settings_surface_host::{SettingsSurfaceHost, SettingsSurfaceIntent};
use crate::panels::viewport::{viewport_toolbar_rect, RenderStyle, ViewportMode, ViewportPanel};
use crate::panels::viewport_toolbar_surface::ViewportToolbarAction;
use crate::panels::viewport_toolbar_surface_host::ViewportToolbarSurfaceHost;
use crate::pcb_document::load_pcb_document;
use crate::project_catalog::ProjectCatalog;
use crate::scene_history::SceneHistory;
use crate::schematic_document::load_schematic_document;
use crate::theme as app_theme;

#[derive(Debug, Clone, PartialEq)]
enum AppScreen {
    Loading {
        start_time: f64,
    },
    ProjectHub,
    NewProject {
        name: String,
        path: String,
        project_type: ProjectType,
    },
    Settings,
    Editor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CanvasMode {
    Game,
    Schematic,
}

enum ElectronicsAnalysisJobResult {
    Drc(raf_electronics::drc::DrcReport),
    Simulation(raf_electronics::simulation::SimulationResults),
}

struct PersistenceWrite {
    path: PathBuf,
    data: String,
}

struct PersistenceJob {
    label: String,
    writes: Vec<PersistenceWrite>,
}

struct PersistenceResult {
    label: String,
    error: Option<String>,
}

/// Attached-mode undo is deliberately a thin capability over the editor's
/// real `SceneHistory`. The token is valid only for the project/session and
/// revision that produced it; a later edit invalidates it instead of risking
/// an undo of the wrong document.
#[derive(Debug, Clone)]
struct AttachedUndoRecord {
    project_id: uuid::Uuid,
    session_id: Option<raf_core::session::SessionId>,
    revision_after: u64,
    before_nodes: usize,
    after_nodes: usize,
}

/// Serializes on the UI thread, but performs all filesystem work in order on
/// one worker. Ordering matters when linear save and an explicit save happen
/// in the same frame.
struct PersistenceQueue {
    sender: Sender<PersistenceJob>,
    receiver: Receiver<PersistenceResult>,
    pending: AtomicUsize,
}

impl Default for PersistenceQueue {
    fn default() -> Self {
        let (sender, jobs) = mpsc::channel::<PersistenceJob>();
        let (results, receiver) = mpsc::channel::<PersistenceResult>();
        let _ = std::thread::Builder::new()
            .name("raf-editor-persistence".to_string())
            .spawn(move || {
                while let Ok(job) = jobs.recv() {
                    let mut error = None;
                    for write in job.writes {
                        if let Some(parent) = write.path.parent() {
                            if let Err(io_error) = std::fs::create_dir_all(parent) {
                                error = Some(format!("{}: {io_error}", parent.display()));
                                break;
                            }
                        }
                        if let Err(io_error) = std::fs::write(&write.path, write.data) {
                            error = Some(format!("{}: {io_error}", write.path.display()));
                            break;
                        }
                    }
                    let _ = results.send(PersistenceResult {
                        label: job.label,
                        error,
                    });
                }
            });
        Self {
            sender,
            receiver,
            pending: AtomicUsize::new(0),
        }
    }
}

impl PersistenceQueue {
    fn queue(&self, label: impl Into<String>, writes: Vec<PersistenceWrite>) -> Result<(), String> {
        if writes.is_empty() {
            return Ok(());
        }
        self.pending.fetch_add(1, Ordering::Release);
        if self
            .sender
            .send(PersistenceJob {
                label: label.into(),
                writes,
            })
            .is_err()
        {
            self.pending.fetch_sub(1, Ordering::AcqRel);
            return Err("editor persistence worker is unavailable".to_string());
        }
        Ok(())
    }

    fn take_results(&self) -> Vec<PersistenceResult> {
        let mut completed = Vec::new();
        loop {
            match self.receiver.try_recv() {
                Ok(result) => {
                    self.pending.fetch_sub(1, Ordering::AcqRel);
                    completed.push(result);
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        completed
    }

    fn wait_for_idle(&self) -> Vec<PersistenceResult> {
        let mut completed = Vec::new();
        while self.pending.load(Ordering::Acquire) != 0 {
            match self.receiver.recv_timeout(Duration::from_secs(5)) {
                Ok(result) => {
                    self.pending.fetch_sub(1, Ordering::AcqRel);
                    completed.push(result);
                }
                Err(mpsc::RecvTimeoutError::Timeout | mpsc::RecvTimeoutError::Disconnected) => {
                    break;
                }
            }
        }
        completed
    }

    fn is_pending(&self) -> bool {
        self.pending.load(Ordering::Acquire) != 0
    }
}

struct DockLayoutLoadResult {
    generation: u64,
    path: PathBuf,
    layout: Option<BottomDockLayout>,
}

struct GameDocumentsLoadResult {
    generation: u64,
    project_path: PathBuf,
    session_id: raf_core::session::SessionId,
    scene: SceneGraph,
    nodes: NodeEditorDocument,
    camera: Option<EditorCameraBlock>,
}

struct ProjectOpenLoadResult {
    generation: u64,
    project: Result<Project, String>,
    sessions: Option<ProjectSessionRegistry>,
}

pub struct AuraRafiApp {
    screen: AppScreen,
    settings: EngineSettings,
    recent_projects: RecentProjects,
    current_project: Option<Project>,
    sessions: ProjectSessionRegistry,
    scene: SceneGraph,
    viewport: ViewportPanel,
    viewport_toolbar_surface: ViewportToolbarSurfaceHost,
    canvas_mode: CanvasMode,
    schematic_view: SchematicViewPanel,
    pcb_view: PcbViewPanel,
    graphics_runtime: RenderRuntime,
    egui_wgpu_render_state: Option<egui_wgpu::RenderState>,
    hub_surface: HubSurfaceHost,
    loading_surface: LoadingSurfaceHost,
    new_project_surface: NewProjectSurfaceHost,
    application_bar: ApplicationBarHost,
    settings_surface: SettingsSurfaceHost,
    settings_draft: Option<EngineSettings>,
    settings_return_to_editor: bool,
    bottom_dock: EditorBottomDockHost,
    nodes_document: NodeEditorDocument,
    nodes_history: Vec<NodeEditorDocument>,
    nodes_history_cursor: usize,
    nodes_drag_changed: bool,
    nodes_clipboard: Option<raf_nodes::Node>,
    nodes_session_id: Option<raf_core::session::SessionId>,
    search_surface: SearchSurfaceHost,
    search_results_key: Option<(String, u64, u64, usize, raf_core::config::Language)>,
    agent: AgentPanel,
    agent_surface: AgentSurfaceHost,
    hierarchy: HierarchySurfaceHost,
    hierarchy_open: bool,
    inspector: InspectorSurfaceHost,
    inspector_open: bool,
    electronics_navigator_surface: ElectronicsNavigatorSurfaceHost,
    electronics_inspector_surface: ElectronicsInspectorSurfaceHost,
    electronics_toolbar_surface: ElectronicsToolbarSurfaceHost,
    electronics_analysis_surface: ElectronicsAnalysisSurfaceHost,
    electronics_drc_report: Option<raf_electronics::drc::DrcReport>,
    electronics_simulation_results: Option<raf_electronics::simulation::SimulationResults>,
    electronics_analysis_job: Option<Receiver<ElectronicsAnalysisJobResult>>,
    scene_history: SceneHistory,
    scene_revision: u64,
    electronics_history: ElectronicsHistory,
    electronics_edit_snapshot: Option<ElectronicsDocumentSnapshot>,
    electronics_edit_changed: bool,
    project_catalog: ProjectCatalog,
    camera_bookmarks: [Option<EditorCameraBookmark>; 3],
    agent_scene_snapshot: Option<SceneGraph>,
    agent_electronics_snapshot: Option<ElectronicsDocumentSnapshot>,
    scene_mutation_group: Option<String>,
    agent_editor_actions: Vec<AgentEditorAction>,
    console: ConsolePanel,
    command_catalog: CommandCatalog,
    attached_host: AttachedCommandHost,
    attached_idempotency: HashMap<String, EngineCommandResponse>,
    attached_undo: HashMap<UndoToken, AttachedUndoRecord>,
    pending_session_events: Vec<crate::commands::sessions::SessionCommandEvent>,
    scene_selection: SceneSelectionState,
    persistence: PersistenceQueue,
    dock_layout_path: Option<PathBuf>,
    dock_layout_generation: u64,
    dock_layout_receiver: Option<Receiver<DockLayoutLoadResult>>,
    game_documents_generation: u64,
    game_documents_receiver: Option<Receiver<GameDocumentsLoadResult>>,
    game_documents_ready: bool,
    project_open_generation: u64,
    project_open_receiver: Option<Receiver<ProjectOpenLoadResult>>,
    last_action: String,
    hub_search_query: String,
    hub_filter: crate::studio_surface::HubSurfaceFilter,
    last_dropped_files: Vec<PathBuf>,
    frame_count: u64,
}

impl AuraRafiApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let config_dir = dirs_config_dir();
        let settings = EngineSettings::load(&config_dir);
        let recent_projects = RecentProjects::load(&config_dir);
        app_theme::apply_theme(&cc.egui_ctx, settings.theme, settings.theme_experimental);

        let mut style = (*cc.egui_ctx.style()).clone();
        for (_, font_id) in &mut style.text_styles {
            font_id.size = settings.font_size;
        }
        cc.egui_ctx.set_style(style);

        let egui_wgpu_render_state = cc.wgpu_render_state.clone();
        let mut graphics_runtime = RenderRuntime::default();
        if let Some(render_state) = &egui_wgpu_render_state {
            graphics_runtime.set_shared_graphics_context(Some(SharedGraphicsContext::from_host(
                render_state.device.clone(),
                render_state.queue.clone(),
            )));
        }

        Self {
            screen: AppScreen::Loading { start_time: 0.0 },
            settings,
            recent_projects,
            current_project: None,
            sessions: ProjectSessionRegistry::new(ProjectType::Game),
            scene: SceneGraph::new(),
            viewport: ViewportPanel::default(),
            viewport_toolbar_surface: ViewportToolbarSurfaceHost::default(),
            canvas_mode: CanvasMode::Game,
            schematic_view: SchematicViewPanel::default(),
            pcb_view: PcbViewPanel::default(),
            graphics_runtime,
            egui_wgpu_render_state,
            hub_surface: HubSurfaceHost::default(),
            loading_surface: LoadingSurfaceHost::default(),
            new_project_surface: NewProjectSurfaceHost::default(),
            application_bar: ApplicationBarHost::default(),
            settings_surface: SettingsSurfaceHost::default(),
            settings_draft: None,
            settings_return_to_editor: false,
            bottom_dock: EditorBottomDockHost::default(),
            nodes_document: NodeEditorDocument::default(),
            nodes_history: vec![NodeEditorDocument::default()],
            nodes_history_cursor: 0,
            nodes_drag_changed: false,
            nodes_clipboard: None,
            nodes_session_id: None,
            search_surface: SearchSurfaceHost::default(),
            search_results_key: None,
            agent: AgentPanel::default(),
            agent_surface: AgentSurfaceHost::default(),
            hierarchy: HierarchySurfaceHost::default(),
            hierarchy_open: true,
            inspector: InspectorSurfaceHost::default(),
            inspector_open: true,
            electronics_navigator_surface: ElectronicsNavigatorSurfaceHost::default(),
            electronics_inspector_surface: ElectronicsInspectorSurfaceHost::default(),
            electronics_toolbar_surface: ElectronicsToolbarSurfaceHost::default(),
            electronics_analysis_surface: ElectronicsAnalysisSurfaceHost::default(),
            electronics_drc_report: None,
            electronics_simulation_results: None,
            electronics_analysis_job: None,
            scene_history: SceneHistory::default(),
            scene_revision: 0,
            electronics_history: ElectronicsHistory::new(),
            electronics_edit_snapshot: None,
            electronics_edit_changed: false,
            project_catalog: ProjectCatalog::default(),
            camera_bookmarks: std::array::from_fn(|_| None),
            agent_scene_snapshot: None,
            agent_electronics_snapshot: None,
            scene_mutation_group: None,
            agent_editor_actions: Vec::new(),
            console: ConsolePanel::default(),
            command_catalog: CommandCatalog::builtin(),
            attached_host: AttachedCommandHost::start(),
            attached_idempotency: HashMap::new(),
            attached_undo: HashMap::new(),
            pending_session_events: Vec::new(),
            scene_selection: SceneSelectionState::default(),
            persistence: PersistenceQueue::default(),
            dock_layout_path: None,
            dock_layout_generation: 0,
            dock_layout_receiver: None,
            game_documents_generation: 0,
            game_documents_receiver: None,
            game_documents_ready: false,
            project_open_generation: 0,
            project_open_receiver: None,
            last_action: "Editor ready".to_string(),
            hub_search_query: String::new(),
            hub_filter: crate::studio_surface::HubSurfaceFilter::All,
            last_dropped_files: Vec::new(),
            frame_count: 0,
        }
    }

    fn attached_capabilities(&self) -> Vec<String> {
        self.command_catalog
            .commands
            .iter()
            .map(|command| command.name.clone())
            .collect()
    }

    fn update_attached_descriptor(&self) {
        let active = self.sessions.active();
        self.attached_host.update_project(
            self.current_project.as_ref(),
            self.scene_revision,
            active.map(|session| session.id.0),
            active.map(|session| session.name.clone()),
            self.attached_capabilities(),
        );
    }

    fn poll_attached_commands(&mut self) {
        for pending in self.attached_host.drain() {
            let response = self.execute_attached_command(&pending);
            self.attached_host.respond(pending, response);
        }
    }

    fn execute_attached_command(
        &mut self,
        pending: &PendingAttachedCommand,
    ) -> EngineCommandResponse {
        let request = &pending.request;
        let id = request.id;
        let Some(project) = self.current_project.clone() else {
            return attached_error(id, self.scene_revision, "No project is open in the editor.");
        };
        if pending.project_id != Some(project.id)
            || !same_project_path(
                pending.project_path.as_deref(),
                Some(project.path.as_path()),
            )
        {
            return attached_error(
                id,
                self.scene_revision,
                "The attached request is scoped to a different project.",
            );
        }
        if let Err(error) = command_protocol_is_current(request) {
            return attached_error(id, self.scene_revision, error);
        }
        if let Some(expected) = request.expected_revision {
            if expected != self.scene_revision {
                return attached_error(
                    id,
                    self.scene_revision,
                    format!(
                        "Revision conflict: request expected {}, editor is at {}.",
                        expected, self.scene_revision
                    ),
                );
            }
        }
        if let Some(session) = request.session.as_deref() {
            let active = self.sessions.active();
            let matches = active.is_some_and(|current| {
                current.name == session || current.id.0.to_string() == session
            });
            if !matches {
                return attached_error(
                    id,
                    self.scene_revision,
                    "The requested session is not the active editor session.",
                );
            }
        }
        if let Some(key) = request.idempotency_key.as_deref() {
            if let Some(previous) = self.attached_idempotency.get(key) {
                let mut replay = previous.clone();
                replay.id = id;
                replay.data = serde_json::json!({
                    "replayed": true,
                    "original_response_id": previous.id,
                });
                replay.revision = self.scene_revision;
                if replay
                    .undo_token
                    .is_some_and(|token| !self.attached_undo.contains_key(&token))
                {
                    replay.undo_available = false;
                    replay.warnings.push(
                        "The original undo token is stale after a later editor change.".to_string(),
                    );
                }
                return replay;
            }
        }

        let name = request
            .name
            .trim()
            .trim_start_matches('/')
            .to_ascii_lowercase();
        if matches!(name.as_str(), "play" | "stop" | "runtime" | "game.runtime")
            || name.contains("runtime")
        {
            return attached_error(
                id,
                self.scene_revision,
                "Play, Stop and Runtime are intentionally unavailable in attached authoring mode.",
            );
        }
        if matches!(
            name.as_str(),
            "engine.status"
                | "status"
                | "capabilities.list"
                | "capabilities.search"
                | "project.info"
                | "workspace.describe"
        ) {
            let response = self.execute_attached_read_command(&name, request);
            self.remember_attached_response(request, &response);
            return response;
        }
        let Some(definition) = self.command_catalog.find(&name).cloned() else {
            return attached_error(
                id,
                self.scene_revision,
                format!("Unknown editor command: {name}"),
            );
        };
        if definition.domain != "shared" && definition.domain != "game" {
            return attached_error(
                id,
                self.scene_revision,
                format!(
                    "Command domain '{}' is not enabled for the game-first attached slice.",
                    definition.domain
                ),
            );
        }
        if project.project_type != ProjectType::Game && definition.domain == "game" {
            return attached_error(
                id,
                self.scene_revision,
                "Game commands require an open Game project.",
            );
        }
        if name == "transaction.undo" {
            let response = self.execute_attached_undo(request, project.id);
            self.remember_attached_response(request, &response);
            return response;
        }
        if request.dry_run && !definition.is_read_only() {
            let response = EngineCommandResponse {
                protocol: raf_core::COMMAND_PROTOCOL_VERSION,
                id,
                ok: true,
                changed: false,
                title: format!("Preview: {name}"),
                lines: vec![
                    "No document was changed. Repeat with confirm=true to apply.".to_string(),
                ],
                data: serde_json::json!({
                    "command": name,
                    "params": request.params,
                    "project_id": project.id,
                "session": self.sessions.active().map(|session| session.name.clone()),
                    "requires_confirmation": true,
                }),
                warnings: Vec::new(),
                diff: None,
                undo_available: false,
                revision: self.scene_revision,
                transaction_id: request.transaction_id,
                undo_token: None,
                artifacts: Vec::new(),
                metrics: Value::Null,
                verification: Some(VerificationSummary {
                    status: "preview".to_string(),
                    checks: Vec::new(),
                    failures: Vec::new(),
                }),
            };
            return response;
        }
        if !definition.is_read_only() && !request.confirm {
            return attached_error(
                id,
                self.scene_revision,
                "This command changes the project. Repeat with confirm=true.",
            );
        }

        if name == "project.save" {
            let result = self.persist_active_project();
            let response = match result {
                Ok(()) => EngineCommandResponse {
                    protocol: raf_core::COMMAND_PROTOCOL_VERSION,
                    id,
                    ok: true,
                    changed: false,
                    title: "Project save queued".to_string(),
                    lines: vec!["Project, scene, nodes and camera writes were queued.".to_string()],
                    data: serde_json::json!({"queued": true}),
                    warnings: Vec::new(),
                    diff: None,
                    undo_available: false,
                    revision: self.scene_revision,
                    transaction_id: request.transaction_id,
                    undo_token: None,
                    artifacts: Vec::new(),
                    metrics: Value::Null,
                    verification: Some(VerificationSummary {
                        status: "not_run".to_string(),
                        checks: Vec::new(),
                        failures: Vec::new(),
                    }),
                },
                Err(error) => attached_error(id, self.scene_revision, error),
            };
            self.remember_attached_response(request, &response);
            return response;
        }

        let args = match attached_args(&request.params) {
            Ok(args) => args,
            Err(error) => return attached_error(id, self.scene_revision, error),
        };
        let parsed = ParsedCommand {
            raw: format!("/{name}"),
            name: definition.name.clone(),
            args,
            positional: Vec::new(),
        };
        let before = self.scene.clone();
        let output = if definition.domain == "game" {
            if !self.game_documents_ready {
                return attached_error(
                    id,
                    self.scene_revision,
                    "Game session documents are still loading; retry after the editor reports readiness.",
                );
            }
            let mut context = commands::game::GameCommandContext {
                scene: &mut self.scene,
                selection: &mut self.scene_selection,
                viewport: &mut self.viewport,
            };
            commands::game::execute(&definition.name, &parsed, &mut context)
        } else {
            self.execute_shared_console_command(&definition.name, &parsed)
        };
        let changed = output.changed;
        let mut response = crate::commands::gateway::response_from_output(id, output);
        if changed {
            let before_count = before.len();
            let scene_diff = scene_graph_diff(&before, &self.scene, &name);
            self.commit_scene_change(before, "Agent attached changed the scene");
            self.process_session_events();
            let undo_token = UndoToken::new();
            self.attached_undo.insert(
                undo_token,
                AttachedUndoRecord {
                    project_id: project.id,
                    session_id: self.sessions.active().map(|session| session.id),
                    revision_after: self.scene_revision,
                    before_nodes: before_count,
                    after_nodes: self.scene.len(),
                },
            );
            response.diff = Some(scene_diff);
            response.transaction_id =
                Some(request.transaction_id.unwrap_or_else(TransactionId::new));
            response.undo_available = true;
            response.undo_token = Some(undo_token);
        }
        response.revision = self.scene_revision;
        response.metrics = serde_json::json!({"attached": true});
        response.verification = Some(VerificationSummary {
            status: if changed { "passed" } else { "not_required" }.to_string(),
            checks: if changed {
                vec![
                    "project_scope".to_string(),
                    "scene_history_recorded".to_string(),
                    "revision_advanced".to_string(),
                    "attached_session".to_string(),
                ]
            } else {
                Vec::new()
            },
            failures: Vec::new(),
        });
        self.remember_attached_response(request, &response);
        response
    }

    fn execute_attached_undo(
        &mut self,
        request: &EngineCommandRequest,
        project_id: uuid::Uuid,
    ) -> EngineCommandResponse {
        let id = request.id;
        let token = match parse_undo_token(&request.params) {
            Ok(token) => token,
            Err(error) => return attached_error(id, self.scene_revision, error),
        };
        let Some(record) = self.attached_undo.get(&token).cloned() else {
            return attached_error(
                id,
                self.scene_revision,
                "The undo token is unknown, expired, or belongs to another editor session.",
            );
        };
        let active_session = self.sessions.active().map(|session| session.id);
        if record.project_id != project_id
            || record.session_id != active_session
            || record.revision_after != self.scene_revision
        {
            return attached_error(
                id,
                self.scene_revision,
                "The undo token is stale. The project, session, or revision changed since it was issued.",
            );
        }
        if request.dry_run {
            let mut response = EngineCommandResponse {
                protocol: raf_core::COMMAND_PROTOCOL_VERSION,
                id,
                ok: true,
                changed: false,
                title: "Preview: transaction.undo".to_string(),
                lines: vec![
                    "No document was changed. Repeat with confirm=true to apply.".to_string(),
                ],
                data: serde_json::json!({
                    "token": token.0.to_string(),
                    "before_nodes": record.after_nodes,
                    "after_nodes": record.before_nodes,
                    "requires_confirmation": true,
                }),
                warnings: Vec::new(),
                diff: None,
                undo_available: true,
                revision: self.scene_revision,
                transaction_id: request.transaction_id,
                undo_token: Some(token),
                artifacts: Vec::new(),
                metrics: serde_json::json!({"attached": true}),
                verification: Some(VerificationSummary {
                    status: "preview".to_string(),
                    checks: vec!["token_scope".to_string(), "revision_match".to_string()],
                    failures: Vec::new(),
                }),
            };
            response
                .warnings
                .push("Confirmation is required to apply the undo.".to_string());
            return response;
        }
        if !request.confirm {
            return attached_error(
                id,
                self.scene_revision,
                "Undo changes the scene. Repeat with confirm=true.",
            );
        }
        self.attached_undo.remove(&token);
        let before_nodes = self.scene.len();
        if !self.undo_scene() {
            return attached_error(
                id,
                self.scene_revision,
                "The editor history no longer contains the change represented by this token.",
            );
        }
        let response = EngineCommandResponse {
            protocol: raf_core::COMMAND_PROTOCOL_VERSION,
            id,
            ok: true,
            changed: true,
            title: "Attached undo applied".to_string(),
            lines: vec!["The scene was restored through the editor's real history.".to_string()],
            data: serde_json::json!({
                "undone_token": token.0.to_string(),
                "before_nodes": before_nodes,
                "after_nodes": self.scene.len(),
            }),
            warnings: Vec::new(),
            diff: Some(serde_json::json!({
                "operation": "scene_undo",
                "before_nodes": before_nodes,
                "after_nodes": self.scene.len(),
                "token": token.0.to_string(),
            })),
            undo_available: false,
            revision: self.scene_revision,
            transaction_id: Some(request.transaction_id.unwrap_or_else(TransactionId::new)),
            undo_token: None,
            artifacts: Vec::new(),
            metrics: serde_json::json!({"attached": true}),
            verification: Some(VerificationSummary {
                status: "passed".to_string(),
                checks: vec![
                    "token_scope".to_string(),
                    "revision_match".to_string(),
                    "scene_history_restored".to_string(),
                    "revision_advanced".to_string(),
                ],
                failures: Vec::new(),
            }),
        };
        self.console.log(
            crate::console::LogLevel::Info,
            "Attached undo token applied",
        );
        response
    }

    fn remember_attached_response(
        &mut self,
        request: &EngineCommandRequest,
        response: &EngineCommandResponse,
    ) {
        if let Some(key) = request.idempotency_key.as_ref() {
            self.attached_idempotency
                .insert(key.clone(), response.clone());
        }
    }

    fn execute_attached_read_command(
        &self,
        name: &str,
        request: &EngineCommandRequest,
    ) -> EngineCommandResponse {
        let id = request.id;
        let project = self.current_project.as_ref();
        let response_data = match name {
            "engine.status" | "status" => serde_json::json!({
                "attached": true,
                "project": project.map(|project| project.name.clone()),
                "project_id": project.map(|project| project.id),
                "project_path": project.map(|project| project.path.clone()),
                "session": self.sessions.active().map(|session| session.name.clone()),
                "revision": self.scene_revision,
                "runtime_enabled": false,
                "play_enabled": false,
            }),
            "capabilities.list" => serde_json::json!({
                "capabilities": self.attached_capabilities(),
            }),
            "capabilities.search" => {
                let query = request
                    .params
                    .get("query")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_ascii_lowercase();
                let capabilities = self
                    .command_catalog
                    .commands
                    .iter()
                    .filter(|command| {
                        query.is_empty()
                            || command.name.to_ascii_lowercase().contains(&query)
                            || command.domain.to_ascii_lowercase().contains(&query)
                            || command.category.to_ascii_lowercase().contains(&query)
                    })
                    .map(|command| command.name.clone())
                    .collect::<Vec<_>>();
                serde_json::json!({"query": query, "capabilities": capabilities})
            }
            "project.info" => serde_json::json!({
                "id": project.map(|project| project.id),
                "name": project.map(|project| project.name.clone()),
                "type": project.map(|project| format!("{:?}", project.project_type)),
                "path": project.map(|project| project.path.clone()),
                "engine_version": project.map(|project| project.engine_version.clone()),
                "settings": project.map(|project| project.settings.clone()),
            }),
            "workspace.describe" => {
                let (files, directories) = project
                    .map(|project| count_workspace_entries(&project.path))
                    .unwrap_or_default();
                serde_json::json!({
                    "path": project.map(|project| project.path.clone()),
                    "files": files,
                    "directories": directories,
                })
            }
            _ => Value::Null,
        };
        EngineCommandResponse {
            protocol: raf_core::COMMAND_PROTOCOL_VERSION,
            id,
            ok: project.is_some(),
            changed: false,
            title: name.to_string(),
            lines: if project.is_some() {
                vec![format!("{name} read from the attached editor.")]
            } else {
                vec!["No project is open in the editor.".to_string()]
            },
            data: response_data,
            warnings: Vec::new(),
            diff: None,
            undo_available: false,
            revision: self.scene_revision,
            transaction_id: None,
            undo_token: None,
            artifacts: Vec::new(),
            metrics: serde_json::json!({"attached": true}),
            verification: Some(VerificationSummary {
                status: if project.is_some() {
                    "not_required"
                } else {
                    "blocked"
                }
                .to_string(),
                checks: Vec::new(),
                failures: Vec::new(),
            }),
        }
    }

    fn palette(&self, ctx: &egui::Context) -> StudioUiPalette {
        if ctx.style().visuals.dark_mode {
            StudioUiPalette::IndustrialDark
        } else {
            StudioUiPalette::PaperLight
        }
    }

    fn show_loading(&mut self, ctx: &egui::Context, start_time: f64) {
        let now = ctx.input(|input| input.time);
        let start = if start_time == 0.0 { now } else { start_time };
        let progress = ((now - start) / 0.85).clamp(0.0, 1.0) as f32;
        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                self.loading_surface.show(
                    ui,
                    self.egui_wgpu_render_state.as_ref(),
                    StudioUiPalette::IndustrialDark,
                    progress,
                    self.settings.language,
                );
            });

        if progress >= 1.0 {
            self.expand_window(ctx);
            self.screen = AppScreen::ProjectHub;
        } else {
            self.screen = AppScreen::Loading { start_time: start };
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn show_hub(&mut self, ctx: &egui::Context) {
        self.poll_project_open();
        if self.project_open_receiver.is_some() {
            ctx.request_repaint_after(Duration::from_millis(32));
        }
        let palette = self.palette(ctx);
        let projects = self.recent_projects.projects.clone();
        let intents = egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                self.hub_surface.show(
                    ui,
                    self.egui_wgpu_render_state.as_ref(),
                    palette,
                    self.settings.language,
                    self.settings.theme,
                    self.hub_filter,
                    &projects,
                    &projects,
                    &self.hub_search_query,
                )
            })
            .inner;

        for intent in intents {
            match intent {
                HubSurfaceIntent::Open(path) => self.open_project(&path),
                HubSurfaceIntent::NewProject(project_type) => {
                    self.screen = AppScreen::NewProject {
                        name: String::new(),
                        path: default_projects_dir(),
                        project_type,
                    };
                }
                HubSurfaceIntent::Forget(path) => {
                    self.recent_projects
                        .projects
                        .retain(|entry| entry.path != path);
                    let _ = self.recent_projects.save(&dirs_config_dir());
                }
                HubSurfaceIntent::Duplicate(path) => self.duplicate_project(&path),
                HubSurfaceIntent::SetSearch(query) => self.hub_search_query = query,
                HubSurfaceIntent::SetFilter(filter) => self.hub_filter = filter,
                HubSurfaceIntent::SetTheme(theme) => {
                    self.settings.theme = theme;
                    app_theme::apply_theme(ctx, theme, self.settings.theme_experimental);
                    let _ = self.settings.save(&dirs_config_dir());
                }
                HubSurfaceIntent::OpenSettings => self.open_settings(false),
                HubSurfaceIntent::Window(command) => {
                    self.dispatch_window_command(ctx, command);
                }
            }
        }
    }

    fn open_settings(&mut self, return_to_editor: bool) {
        self.settings_return_to_editor = return_to_editor;
        self.settings_draft = Some(self.settings.clone());
        self.settings_surface.section = crate::settings_surface::SettingsSection::Appearance;
        self.screen = AppScreen::Settings;
    }

    fn show_settings(&mut self, ctx: &egui::Context) {
        let mut draft = self
            .settings_draft
            .take()
            .unwrap_or_else(|| self.settings.clone());
        let palette = self.palette(ctx);
        let language = self.settings.language;
        let intents = egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                self.settings_surface.show(
                    ui,
                    self.egui_wgpu_render_state.as_ref(),
                    palette,
                    language,
                    &mut draft,
                )
            })
            .inner;

        if intents
            .iter()
            .any(|intent| matches!(intent, SettingsSurfaceIntent::Save))
        {
            self.settings = draft;
            self.settings_draft = None;
            let _ = self.settings.save(&dirs_config_dir());
            app_theme::apply_theme(ctx, self.settings.theme, self.settings.theme_experimental);
            self.screen = if self.settings_return_to_editor {
                AppScreen::Editor
            } else {
                AppScreen::ProjectHub
            };
        } else if intents
            .iter()
            .any(|intent| matches!(intent, SettingsSurfaceIntent::Cancel))
        {
            self.settings_draft = None;
            self.screen = if self.settings_return_to_editor {
                AppScreen::Editor
            } else {
                AppScreen::ProjectHub
            };
        } else {
            self.settings_draft = Some(draft);
        }
    }

    fn show_new_project(
        &mut self,
        ctx: &egui::Context,
        mut name: String,
        mut path: String,
        project_type: ProjectType,
    ) {
        let palette = self.palette(ctx);
        let actions = egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                self.new_project_surface.show(
                    ui,
                    self.egui_wgpu_render_state.as_ref(),
                    palette,
                    &name,
                    &path,
                    project_type,
                    self.settings.language,
                )
            })
            .inner;
        let mut create = false;
        for action in actions {
            match action {
                NewProjectSurfaceAction::SetName(value) => name = value,
                NewProjectSurfaceAction::SetPath(value) => path = value,
                NewProjectSurfaceAction::Cancel => {
                    self.screen = AppScreen::ProjectHub;
                    return;
                }
                NewProjectSurfaceAction::Create => create = true,
            }
        }
        if create && !name.trim().is_empty() && !path.trim().is_empty() {
            match Project::create(name.trim(), project_type, Path::new(path.trim())) {
                Ok(project) => {
                    self.recent_projects.add(&project);
                    let _ = self.recent_projects.save(&dirs_config_dir());
                    self.open_loaded_project(project);
                    return;
                }
                Err(error) => tracing::error!(%error, "project creation failed"),
            }
        }
        self.screen = AppScreen::NewProject {
            name,
            path,
            project_type,
        };
    }

    fn show_editor(&mut self, ctx: &egui::Context) {
        // RafUI surfaces OR their keyboard capture into this per-frame flag;
        // reset it before panels are painted so a hidden/closed text field
        // cannot keep blocking viewport shortcuts from the previous frame.
        ctx.data_mut(|data| {
            data.insert_temp(egui::Id::new(raf_ui::KEYBOARD_CAPTURE_TEMP_ID), false);
        });
        let palette = self.palette(ctx);
        let language = self.settings.language;
        self.poll_persistence();
        self.poll_dock_layout();
        self.poll_game_documents();
        self.poll_electronics_analysis();
        if self.persistence.is_pending() {
            ctx.request_repaint_after(Duration::from_millis(32));
        }
        if self.dock_layout_receiver.is_some() {
            ctx.request_repaint_after(Duration::from_millis(32));
        }
        if self.electronics_analysis_job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(32));
        }
        if self.game_documents_receiver.is_some() {
            ctx.request_repaint_after(Duration::from_millis(32));
        }
        let agent_readiness = self.agent.prepare(
            &self.settings,
            self.current_project.as_ref(),
            &self.command_catalog,
        );
        let agent_is_active = matches!(
            self.agent.runtime.status,
            raf_ai::AgentStatus::Thinking
                | raf_ai::AgentStatus::ExecutingTools
                | raf_ai::AgentStatus::AwaitingApproval
        );
        if agent_is_active
            && (!self.is_game_project() || self.game_documents_ready)
            && self.agent_scene_snapshot.is_none()
        {
            self.agent_scene_snapshot = Some(self.scene.clone());
        }
        if agent_is_active
            && self.is_electronics_project()
            && self.agent_electronics_snapshot.is_none()
        {
            self.agent_electronics_snapshot = Some(self.electronics_snapshot());
        }
        {
            let allow_agent_poll = !self.is_game_project() || self.game_documents_ready;
            let project_context = AgentProjectContext::from_project(self.current_project.as_ref());
            let tool_name_map = self.agent.tool_name_map.clone();
            let mut executor = AgentToolExecutor {
                scene: &mut self.scene,
                selection: &mut self.scene_selection,
                viewport: &mut self.viewport,
                schematic_view: &mut self.schematic_view,
                pcb_view: &mut self.pcb_view,
                project: project_context,
                catalog: &self.command_catalog,
                tool_name_map,
                editor_actions: &mut self.agent_editor_actions,
            };
            if allow_agent_poll {
                self.agent.poll(&mut executor);
            }
        }
        let agent_still_active = matches!(
            self.agent.runtime.status,
            raf_ai::AgentStatus::Thinking
                | raf_ai::AgentStatus::ExecutingTools
                | raf_ai::AgentStatus::AwaitingApproval
        );
        if !agent_still_active {
            if let Some(before) = self.agent_scene_snapshot.take() {
                self.commit_scene_change(before, "Agent changed the scene");
            }
            if let Some(before) = self.agent_electronics_snapshot.take() {
                self.record_electronics_change(before, "Agent changed the electronics document");
            }
        }
        for action in std::mem::take(&mut self.agent_editor_actions) {
            self.apply_agent_editor_action(action);
        }
        let project_path = self
            .current_project
            .as_ref()
            .map(|project| project.path.clone());
        self.project_catalog.sync_project(project_path.as_deref());
        if self.project_catalog.poll() {
            ctx.request_repaint();
        }
        if self.project_catalog.is_pending() {
            ctx.request_repaint_after(Duration::from_millis(32));
        }
        if let Some((imported, skipped)) = self.project_catalog.take_import_report() {
            let language = self.settings.language;
            self.last_action = if skipped == 0 {
                format!("{}: {imported}", t("app.assets.imported", language))
            } else {
                format!(
                    "{}: {imported}; {}: {skipped}",
                    t("app.assets.imported", language),
                    t("app.assets.imported_skipped", language)
                )
            };
            self.console.log(
                if skipped == 0 {
                    crate::console::LogLevel::Info
                } else {
                    crate::console::LogLevel::Warning
                },
                self.last_action.clone(),
            );
        }
        self.sync_dock_layout(project_path.as_deref());
        let project_name = self
            .current_project
            .as_ref()
            .map(|project| project.name.clone())
            .unwrap_or_default();
        let session_name = self
            .sessions
            .active()
            .map(|session| session.name.clone())
            .unwrap_or_default();
        let command_names = self.command_catalog.command_names();
        let input_enabled = self.current_project.as_ref().is_some_and(|project| {
            self.settings.command_console_enabled || project.settings.enable_console_commands
        });
        let status = self.status_items(ctx);

        let project_type = self
            .current_project
            .as_ref()
            .map(|project| project.project_type)
            .unwrap_or(ProjectType::Game);
        let nodes_active =
            project_type == ProjectType::Game && self.bottom_dock.active_tab_is("nodes");
        let dropped_files =
            if project_type == ProjectType::Game && self.bottom_dock.active_tab_is("assets") {
                ctx.input(|input| {
                    input
                        .raw
                        .dropped_files
                        .iter()
                        .filter_map(|file| file.path.clone())
                        .collect::<Vec<_>>()
                })
            } else {
                Vec::new()
            };
        if dropped_files != self.last_dropped_files {
            if !dropped_files.is_empty() {
                self.project_catalog
                    .import_external_files(dropped_files.clone());
                self.last_action = t("app.assets.import_requested", self.settings.language);
            }
            self.last_dropped_files = dropped_files;
        }
        let active_view = match (project_type, self.canvas_mode) {
            (ProjectType::Game, _) => ApplicationView::Scene,
            (ProjectType::Electronics, CanvasMode::Schematic) => ApplicationView::Schematic,
            (ProjectType::Electronics, CanvasMode::Game) => ApplicationView::Pcb,
        };
        let hierarchy_visible = self.hierarchy_open
            && (project_type == ProjectType::Game || project_type == ProjectType::Electronics)
            && (project_type != ProjectType::Game || self.game_documents_ready)
            && self
                .current_project
                .as_ref()
                .is_some_and(|project| project.settings.show_hierarchy_panel);
        let inspector_visible = self.inspector_open
            && (project_type == ProjectType::Game || project_type == ProjectType::Electronics)
            && (project_type != ProjectType::Game || self.game_documents_ready)
            && self
                .current_project
                .as_ref()
                .is_some_and(|project| project.settings.show_properties_panel);
        let application_menu_state = ApplicationMenuState {
            project_type,
            active_view,
            grid_visible: self.settings.grid_visible,
            hierarchy_visible,
            inspector_visible,
            undo_available: if nodes_active {
                self.nodes_history_cursor > 0
            } else if project_type == ProjectType::Electronics {
                self.electronics_history.can_undo()
            } else {
                self.scene_history.can_undo()
            },
            redo_available: if nodes_active {
                self.nodes_history_cursor + 1 < self.nodes_history.len()
            } else if project_type == ProjectType::Electronics {
                self.electronics_history.can_redo()
            } else {
                self.scene_history.can_redo()
            },
            selection_available: if project_type == ProjectType::Electronics {
                !matches!(
                    self.canvas_mode,
                    CanvasMode::Schematic if matches!(
                        self.schematic_view.selection(),
                        crate::panels::schematic_view::SchematicSelection::None
                    )
                ) && !matches!(
                    self.canvas_mode,
                    CanvasMode::Game if matches!(
                        self.pcb_view.selection(),
                        crate::panels::pcb_view::PcbSelection::None
                    )
                )
            } else {
                !self.scene_selection.selected_nodes.is_empty() || !self.scene.is_empty()
            },
        };
        let application_bar_commands = egui::TopBottomPanel::top("editor.application-bar")
            .exact_height(crate::application_bar_surface::APPLICATION_BAR_HEIGHT)
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                self.application_bar.show(
                    ui,
                    self.egui_wgpu_render_state.as_ref(),
                    palette,
                    language,
                    &project_name,
                    application_menu_state,
                    self.agent_bar_status(),
                )
            })
            .inner;
        for command in application_bar_commands {
            self.dispatch_application_bar_command(ctx, &command);
        }
        if ctx.input(|input| input.modifiers.ctrl && input.key_pressed(egui::Key::K)) {
            self.search_surface
                .open(self.application_bar.command_query().to_string());
        }
        if self.search_surface.is_open() {
            let key = (
                self.search_surface.query().to_string(),
                self.project_catalog.revision(),
                self.scene_revision,
                self.command_catalog.commands.len(),
                language,
            );
            if self.search_results_key.as_ref() != Some(&key) {
                if self.project_catalog.is_pending() {
                    self.search_surface.set_loading();
                } else if let Some(error) = self.project_catalog.error() {
                    self.search_surface.set_error(error.to_string());
                } else {
                    self.search_surface
                        .set_results(self.build_search_results(language));
                }
                self.search_results_key = Some(key);
            }
        }
        let search_intents =
            self.search_surface
                .show(ctx, self.egui_wgpu_render_state.as_ref(), palette, language);
        for intent in search_intents {
            self.apply_search_intent(ctx, intent);
        }

        // Game is the default layout: the status bar and downbar must be
        // registered before the side panels so they span the full editor
        // width. Electronics is registered below its navigator/inspector so
        // its CAD utility dock remains scoped to the center workspace.
        if project_type != ProjectType::Electronics {
            self.show_editor_bottom_dock(
                ctx,
                palette,
                language,
                &status,
                &project_name,
                &session_name,
                &command_names,
                input_enabled,
                agent_readiness,
                project_type,
            );
        }

        let show_hierarchy = self.hierarchy_open
            && (project_type == ProjectType::Game || project_type == ProjectType::Electronics)
            && (project_type != ProjectType::Game || self.game_documents_ready)
            && self
                .current_project
                .as_ref()
                .is_some_and(|project| project.settings.show_hierarchy_panel);
        let hierarchy_intents = if show_hierarchy && project_type == ProjectType::Game {
            let selected = self.scene_selection.selected_nodes.clone();
            let row_height = self.settings.hierarchy_row_height;
            let indent_width = self.settings.hierarchy_indent_width;
            let show_icons = self.settings.hierarchy_show_icons;
            let show_visibility = self.settings.hierarchy_show_visibility;
            let show_locked = self.settings.hierarchy_show_locked;
            self.hierarchy
                .sync_show_hidden_setting(self.settings.hierarchy_show_hidden);
            self.hierarchy
                .sync_bookmark_state(std::array::from_fn(|index| {
                    self.camera_bookmarks[index].is_some()
                }));
            egui::SidePanel::left("editor.hierarchy")
                .resizable(true)
                .default_width(430.0)
                .min_width(360.0)
                .max_width(520.0)
                .frame(egui::Frame::none())
                .show(ctx, |ui| {
                    self.hierarchy.show(
                        ui,
                        self.egui_wgpu_render_state.as_ref(),
                        palette,
                        language,
                        &self.scene,
                        &selected,
                        row_height,
                        indent_width,
                        show_icons,
                        show_visibility,
                        show_locked,
                        self.settings.hierarchy_animations,
                    )
                })
                .inner
        } else {
            Vec::new()
        };
        for intent in hierarchy_intents {
            self.apply_hierarchy_intent(intent);
        }

        if show_hierarchy && project_type == ProjectType::Electronics {
            let electronics_intents = egui::SidePanel::left("editor.electronics.navigator")
                .resizable(true)
                .default_width(268.0)
                .width_range(236.0..=304.0)
                .frame(egui::Frame::none())
                .show(ctx, |ui| {
                    self.show_electronics_navigator(ui, palette, language)
                })
                .inner;
            self.apply_electronics_navigator_actions(electronics_intents);
        }

        let show_inspector = self.inspector_open
            && (project_type == ProjectType::Game || project_type == ProjectType::Electronics)
            && (project_type != ProjectType::Game || self.game_documents_ready)
            && self
                .current_project
                .as_ref()
                .is_some_and(|project| project.settings.show_properties_panel);
        let inspector_intents = if show_inspector && project_type == ProjectType::Game {
            let selected = self.scene_selection.selected_nodes.clone();
            egui::SidePanel::right("editor.inspector")
                .resizable(true)
                .default_width(360.0)
                .min_width(300.0)
                .max_width(520.0)
                .frame(egui::Frame::none())
                .show(ctx, |ui| {
                    self.inspector.show(
                        ui,
                        self.egui_wgpu_render_state.as_ref(),
                        palette,
                        language,
                        &self.scene,
                        &self.sessions,
                        &selected,
                        self.settings.hierarchy_animations,
                    )
                })
                .inner
        } else {
            Vec::new()
        };
        for intent in inspector_intents {
            self.apply_inspector_intent(intent);
        }

        if show_inspector && project_type == ProjectType::Electronics {
            let electronics_intents = egui::SidePanel::right("editor.electronics.inspector")
                .resizable(true)
                .default_width(320.0)
                .width_range(288.0..=360.0)
                .frame(egui::Frame::none())
                .show(ctx, |ui| {
                    self.show_electronics_inspector(ui, palette, language)
                })
                .inner;
            self.apply_electronics_inspector_actions(electronics_intents);
        }

        if project_type == ProjectType::Electronics {
            self.show_editor_bottom_dock(
                ctx,
                palette,
                language,
                &status,
                &project_name,
                &session_name,
                &command_names,
                input_enabled,
                agent_readiness,
                project_type,
            );
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| match self.canvas_mode {
                CanvasMode::Game => self.show_game_viewport(ctx, ui),
                CanvasMode::Schematic => self.show_electronics_viewport(ui),
            });
    }

    fn show_editor_bottom_dock(
        &mut self,
        ctx: &egui::Context,
        palette: StudioUiPalette,
        language: raf_core::config::Language,
        status: &[String],
        project_name: &str,
        session_name: &str,
        command_names: &[String],
        input_enabled: bool,
        agent_readiness: crate::panels::ai_chat::AgentReadiness,
        project_type: ProjectType,
    ) {
        egui::TopBottomPanel::bottom("editor.status")
            .exact_height(28.0)
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                self.bottom_dock.show_status(
                    ui,
                    self.egui_wgpu_render_state.as_ref(),
                    palette,
                    language,
                    status,
                );
            });

        let asset_rows = self.project_catalog.assets();
        let project_entries = self.project_catalog.project_entries();
        let dock_response = egui::TopBottomPanel::bottom("editor.downbar")
            .resizable(true)
            .default_height(self.bottom_dock.layout.height)
            .min_height(crate::panels::editor_bottom_dock_host::DOWNBAR_COLLAPSED_HEIGHT)
            .max_height(320.0)
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                self.bottom_dock.show(
                    ui,
                    self.egui_wgpu_render_state.as_ref(),
                    palette,
                    language,
                    &mut self.console,
                    input_enabled,
                    command_names,
                    &asset_rows,
                    self.project_catalog.revision(),
                    project_name,
                    session_name,
                    &project_entries,
                    self.current_project.as_mut(),
                    self.settings.command_console_enabled,
                    &mut self.agent,
                    &mut self.agent_surface,
                    agent_readiness,
                    &self.settings,
                    &mut self.electronics_analysis_surface,
                    self.electronics_drc_report.as_ref(),
                    self.electronics_simulation_results.as_ref(),
                    &self.schematic_view.schematic,
                    project_type == ProjectType::Electronics,
                    &self.nodes_document,
                )
            });
        self.bottom_dock
            .set_height(dock_response.response.rect.height());

        for action in dock_response.inner.agent_actions {
            self.apply_agent_action(action);
        }
        for submission in dock_response.inner.submissions {
            self.execute_console_submission(submission);
        }
        for action in dock_response.inner.electronics_analysis_actions {
            match action {
                ElectronicsAnalysisSurfaceAction::RunDrc => self.run_electronics_drc(),
                ElectronicsAnalysisSurfaceAction::RunSimulation => {
                    self.run_electronics_simulation()
                }
            }
        }
        for action in dock_response.inner.nodes_actions {
            self.apply_nodes_intent(action);
        }
        for action in dock_response.inner.assets_actions {
            match action {
                AssetsIntent::QueryChanged(_) | AssetsIntent::FilterChanged(_) => {}
                AssetsIntent::Refresh => {
                    self.project_catalog.refresh();
                    self.last_action = "Asset catalog refresh requested".to_string();
                }
                AssetsIntent::OpenFolder => {
                    if let Some(project) = self.current_project.as_ref() {
                        let _ = std::process::Command::new("explorer")
                            .arg(project.path.join("assets"))
                            .spawn();
                        self.last_action = "Assets folder opened".to_string();
                    }
                }
                AssetsIntent::OpenAsset(relative_path) => {
                    let Some(project) = self.current_project.as_ref() else {
                        continue;
                    };
                    let path = project.path.join("assets").join(&relative_path);
                    if crate::script_support::is_script_file(&relative_path) {
                        if crate::script_support::open_script_in_external_editor(&path) {
                            self.last_action = format!("Script opened: {relative_path}");
                        } else {
                            self.last_action = format!("Could not open script: {relative_path}");
                        }
                    } else {
                        self.last_action = format!("Asset selected: {relative_path}");
                    }
                }
                AssetsIntent::CreateScript { language, name } => {
                    let safe_name = sanitize_script_name(&name);
                    let command_text =
                        format!("/script.create lang={} name={}", language, safe_name);
                    self.console.log_user("Assets", &command_text);
                    let output = match parse_console_input(&command_text) {
                        Ok(ParsedInput::Command(command)) => {
                            self.execute_shared_console_command("script.create", &command)
                        }
                        Ok(ParsedInput::Message(text)) => CommandOutput::error(
                            "Create script",
                            format!("Unexpected text input: {text}"),
                        ),
                        Err(error) => CommandOutput::error("Create script", error),
                    };
                    self.last_action = output.title.clone();
                    self.console.log_command_output(output);
                    self.project_catalog.refresh();
                }
            }
        }
        self.persist_dock_layout_if_dirty();
    }

    fn poll_persistence(&mut self) {
        let results = self.persistence.take_results();
        self.report_persistence(results);
    }

    fn flush_persistence(&mut self) -> bool {
        let results = self.persistence.wait_for_idle();
        let succeeded =
            !self.persistence.is_pending() && results.iter().all(|result| result.error.is_none());
        self.report_persistence(results);
        succeeded
    }

    fn report_persistence(&mut self, results: Vec<PersistenceResult>) {
        for result in results {
            if let Some(error) = result.error {
                self.last_action = format!("{} save failed", result.label);
                self.console.log(
                    crate::console::LogLevel::Error,
                    format!("{} persistence failed: {error}", result.label),
                );
                tracing::error!(label = %result.label, %error, "editor persistence failed");
            } else {
                self.console.log(
                    crate::console::LogLevel::Info,
                    format!("{} saved", result.label),
                );
                tracing::debug!(label = %result.label, "editor persistence completed");
            }
        }
    }

    fn sync_dock_layout(&mut self, project_path: Option<&Path>) {
        let next_path = project_path.map(|path| path.join(".aura_rafi").join("editor_downbar.ron"));
        if self.dock_layout_path == next_path {
            return;
        }

        self.dock_layout_path = next_path.clone();
        self.dock_layout_generation = self.dock_layout_generation.wrapping_add(1);
        self.dock_layout_receiver = None;
        let project_type = self
            .current_project
            .as_ref()
            .map(|project| project.project_type)
            .unwrap_or(ProjectType::Game);
        self.bottom_dock.prepare_for_project(project_type);

        let Some(path) = next_path else {
            return;
        };
        let generation = self.dock_layout_generation;
        let (sender, receiver) = mpsc::channel();
        self.dock_layout_receiver = Some(receiver);
        let _ = std::thread::Builder::new()
            .name("raf-editor-dock-layout".to_string())
            .spawn(move || {
                let layout = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|raw| ron::from_str::<BottomDockLayout>(&raw).ok());
                let _ = sender.send(DockLayoutLoadResult {
                    generation,
                    path,
                    layout,
                });
            });
    }

    fn poll_dock_layout(&mut self) {
        let Some(receiver) = self.dock_layout_receiver.as_ref() else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                self.dock_layout_receiver = None;
                return;
            }
        };
        self.dock_layout_receiver = None;
        if result.generation != self.dock_layout_generation
            || self.dock_layout_path.as_ref() != Some(&result.path)
        {
            return;
        }
        if let Some(layout) = result.layout {
            let project_type = self
                .current_project
                .as_ref()
                .map(|project| project.project_type)
                .unwrap_or(ProjectType::Game);
            self.bottom_dock.layout = sanitize_layout_for_project(layout, project_type);
            self.console
                .log(crate::console::LogLevel::Info, "Project dock layout loaded");
        }
    }

    fn begin_game_documents_load(&mut self, project: &Project) {
        self.game_documents_ready = false;
        let Some(session) = self.sessions.active().cloned() else {
            self.console.log(
                crate::console::LogLevel::Error,
                "Game project has no active session",
            );
            return;
        };
        self.game_documents_generation = self.game_documents_generation.wrapping_add(1);
        self.game_documents_receiver = None;
        let generation = self.game_documents_generation;
        let project_path = project.path.clone();
        let (sender, receiver) = mpsc::channel();
        self.game_documents_receiver = Some(receiver);
        let _ = std::thread::Builder::new()
            .name("raf-game-session-documents".to_string())
            .spawn(move || {
                let scene_path = session.path(&project_path, &session.scene_file);
                let scene = if scene_path.is_file() {
                    SceneGraph::load_ron(&scene_path)
                } else {
                    SceneGraph::new()
                };
                let camera_path = session.path(&project_path, &session.editor_camera_file());
                let camera = std::fs::read_to_string(camera_path)
                    .ok()
                    .and_then(|raw| ron::from_str::<EditorCameraBlock>(&raw).ok());
                let nodes_path = session.path(&project_path, &session.nodes_file);
                let nodes = std::fs::read_to_string(nodes_path)
                    .ok()
                    .and_then(|raw| ron::from_str::<NodeEditorDocument>(&raw).ok())
                    .unwrap_or_default();
                let _ = sender.send(GameDocumentsLoadResult {
                    generation,
                    project_path,
                    session_id: session.id,
                    scene,
                    nodes,
                    camera,
                });
            });
    }

    fn poll_project_open(&mut self) {
        let Some(receiver) = self.project_open_receiver.as_ref() else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                self.project_open_receiver = None;
                self.last_action = "Project open worker disconnected".to_string();
                self.console
                    .log(crate::console::LogLevel::Error, self.last_action.clone());
                return;
            }
        };
        self.project_open_receiver = None;
        if result.generation != self.project_open_generation {
            return;
        }
        match result.project {
            Ok(project) => {
                if let Some(sessions) = result.sessions {
                    self.open_loaded_project_with_sessions(project, sessions);
                } else {
                    self.open_loaded_project(project);
                }
            }
            Err(error) => {
                self.last_action = format!("Project load failed: {error}");
                self.console
                    .log(crate::console::LogLevel::Error, self.last_action.clone());
                tracing::error!(%error, "project load failed");
            }
        }
    }

    fn poll_game_documents(&mut self) {
        let Some(receiver) = self.game_documents_receiver.as_ref() else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                self.game_documents_receiver = None;
                self.console.log(
                    crate::console::LogLevel::Error,
                    "Game session document worker disconnected",
                );
                return;
            }
        };
        self.game_documents_receiver = None;
        let active_project_path = self
            .current_project
            .as_ref()
            .map(|project| project.path.clone());
        let active_session_id = self.sessions.active().map(|session| session.id);
        if result.generation != self.game_documents_generation
            || active_project_path.as_ref() != Some(&result.project_path)
            || active_session_id != Some(result.session_id)
            || !self.is_game_project()
        {
            return;
        }
        self.game_documents_ready = true;

        self.scene = result.scene;
        if self.scene.is_empty() {
            let root = self.scene.add_root("Scene Root");
            self.scene.add_child(root, "Directional Light");
        }
        self.nodes_document = result.nodes;
        self.nodes_session_id = Some(result.session_id);
        self.camera_bookmarks = std::array::from_fn(|_| None);
        if let Some(camera) = result.camera {
            let mut fallback_slot: usize = 0;
            for bookmark in camera.bookmarks.iter() {
                let named_slot = bookmark
                    .name
                    .strip_prefix("bookmark_")
                    .and_then(|value| value.parse::<usize>().ok())
                    .and_then(|slot| (1..=3).contains(&slot).then_some(slot - 1));
                let slot = named_slot.or_else(|| {
                    let slot = fallback_slot;
                    fallback_slot = fallback_slot.saturating_add(1);
                    (slot < self.camera_bookmarks.len()).then_some(slot)
                });
                if let Some(slot) = slot {
                    self.camera_bookmarks[slot] = Some(bookmark.clone());
                }
            }
            self.viewport.apply_editor_camera_block(&camera.sanitized());
        }
        self.reset_nodes_history();
        self.scene_history.clear();
        self.agent_scene_snapshot = None;
        self.scene_selection = SceneSelectionState::default();
        self.viewport.clear_scene_edit_snapshot();
        self.viewport.set_selected_ids(Vec::new());
        self.viewport_toolbar_surface.reset();
        self.hierarchy.reset_for_scene();
        self.inspector.reset_for_scene();
        self.bottom_dock.reset_nodes();
        self.bottom_dock.sync_nodes_selection(None);
        self.bottom_dock.mark_nodes_changed();
        self.scene_revision = self.scene_revision.wrapping_add(1);
        self.search_results_key = None;
        self.console.log(
            crate::console::LogLevel::Info,
            format!(
                "Game session documents ready: {} scene node(s), {} node graph(s), {} camera bookmark(s)",
                self.scene.all_valid_ids().len(),
                self.nodes_document.graphs.len(),
                self.camera_bookmarks.iter().flatten().count(),
            ),
        );
        self.last_action = "Game session documents loaded".to_string();
    }

    fn persist_dock_layout_if_dirty(&mut self) {
        if self.dock_layout_receiver.is_some() {
            return;
        }
        if !self.bottom_dock.take_layout_dirty() {
            return;
        }
        let Some(path) = self.dock_layout_path.clone() else {
            return;
        };
        let data = match ron::ser::to_string_pretty(
            &self.bottom_dock.layout,
            ron::ser::PrettyConfig::default(),
        ) {
            Ok(data) => data,
            Err(error) => {
                self.console.log(
                    crate::console::LogLevel::Error,
                    format!("Dock layout serialization failed: {error}"),
                );
                return;
            }
        };
        if let Err(error) =
            self.queue_persistence("Dock layout", vec![PersistenceWrite { path, data }])
        {
            self.console.log(
                crate::console::LogLevel::Error,
                format!("Dock layout save failed: {error}"),
            );
        }
    }

    fn queue_persistence(
        &self,
        label: impl Into<String>,
        writes: Vec<PersistenceWrite>,
    ) -> Result<(), String> {
        self.persistence.queue(label, writes)
    }

    fn agent_bar_status(&self) -> AgentBarStatus {
        match self.agent.runtime.status {
            raf_ai::AgentStatus::Thinking | raf_ai::AgentStatus::ExecutingTools => {
                AgentBarStatus::Thinking
            }
            raf_ai::AgentStatus::AwaitingApproval => AgentBarStatus::Approval,
            raf_ai::AgentStatus::Error => AgentBarStatus::Error,
            raf_ai::AgentStatus::Done => AgentBarStatus::Ready,
        }
    }

    fn apply_agent_action(&mut self, action: AgentAction) {
        match action {
            AgentAction::OpenSettings => self.open_settings(true),
            AgentAction::Approve | AgentAction::Deny
                if self.is_game_project() && !self.game_documents_ready =>
            {
                self.console.log(
                    crate::console::LogLevel::Warning,
                    "Agent action deferred until Game documents finish loading",
                );
            }
            AgentAction::Approve => {
                let electronics_project = self.is_electronics_project();
                if electronics_project && self.agent_electronics_snapshot.is_none() {
                    self.agent_electronics_snapshot = Some(self.electronics_snapshot());
                }
                let before = (!electronics_project).then(|| self.scene.clone());
                let project_context =
                    AgentProjectContext::from_project(self.current_project.as_ref());
                let tool_name_map = self.agent.tool_name_map.clone();
                let mut executor = AgentToolExecutor {
                    scene: &mut self.scene,
                    selection: &mut self.scene_selection,
                    viewport: &mut self.viewport,
                    schematic_view: &mut self.schematic_view,
                    pcb_view: &mut self.pcb_view,
                    project: project_context,
                    catalog: &self.command_catalog,
                    tool_name_map,
                    editor_actions: &mut self.agent_editor_actions,
                };
                self.agent.approve_with_executor(&mut executor);
                if let Some(before) = before {
                    self.commit_scene_change(before, "Agent approved scene change");
                }
            }
            AgentAction::Deny => {
                let electronics_project = self.is_electronics_project();
                if electronics_project && self.agent_electronics_snapshot.is_none() {
                    self.agent_electronics_snapshot = Some(self.electronics_snapshot());
                }
                let before = (!electronics_project).then(|| self.scene.clone());
                let project_context =
                    AgentProjectContext::from_project(self.current_project.as_ref());
                let tool_name_map = self.agent.tool_name_map.clone();
                let mut executor = AgentToolExecutor {
                    scene: &mut self.scene,
                    selection: &mut self.scene_selection,
                    viewport: &mut self.viewport,
                    schematic_view: &mut self.schematic_view,
                    pcb_view: &mut self.pcb_view,
                    project: project_context,
                    catalog: &self.command_catalog,
                    tool_name_map,
                    editor_actions: &mut self.agent_editor_actions,
                };
                self.agent.deny_with_executor(&mut executor);
                if let Some(before) = before {
                    self.commit_scene_change(before, "Agent denied scene change");
                }
            }
            action => {
                self.agent.apply_action(action, &mut self.settings);
                if self.agent.settings_changed {
                    self.agent.settings_changed = false;
                    let _ = self.settings.save(&dirs_config_dir());
                }
            }
        }
    }

    fn apply_hierarchy_intent(&mut self, intent: HierarchyIntent) {
        if !self.is_game_project() || !self.game_documents_ready {
            return;
        }
        match intent {
            HierarchyIntent::Select(ids) => {
                self.set_scene_selection(ids);
            }
            HierarchyIntent::ToggleVisibility(id) => {
                let before = self.scene.clone();
                if let Some(node) = self.scene.get_mut(id) {
                    node.visible = !node.visible;
                    let name = node.name.clone();
                    self.commit_scene_change(before, &format!("Visibility changed: {name}"));
                }
            }
            HierarchyIntent::ToggleLocked(id) => {
                let before = self.scene.clone();
                if let Some(node) = self.scene.get_mut(id) {
                    node.locked = !node.locked;
                    let name = node.name.clone();
                    self.commit_scene_change(before, &format!("Lock changed: {name}"));
                }
            }
            HierarchyIntent::Rename { id, name } => {
                let before = self.scene.clone();
                if let Some(node) = self.scene.get_mut(id) {
                    if node.name != name {
                        node.name = name;
                        self.commit_scene_change(before, "Hierarchy node renamed");
                    }
                }
            }
            HierarchyIntent::CreateFolder { parent } => {
                let before = self.scene.clone();
                let id = parent
                    .filter(|parent| self.scene.is_valid_node(*parent))
                    .map(|parent| self.scene.add_child_folder(parent, "New Folder"))
                    .unwrap_or_else(|| self.scene.add_root_folder("New Folder"));
                self.set_scene_selection(vec![id]);
                self.commit_scene_change(before, "Folder created");
            }
            HierarchyIntent::CreateEntity { parent } => {
                let before = self.scene.clone();
                let id = parent
                    .filter(|parent| self.scene.is_valid_node(*parent))
                    .map(|parent| self.scene.add_child(parent, "New Entity"))
                    .unwrap_or_else(|| self.scene.add_root("New Entity"));
                self.set_scene_selection(vec![id]);
                self.commit_scene_change(before, "Entity created");
            }
            HierarchyIntent::Duplicate(id) => {
                let before = self.scene.clone();
                if let Some(duplicate) = self.scene.duplicate_node(id) {
                    self.set_scene_selection(vec![duplicate]);
                    self.commit_scene_change(before, "Hierarchy node duplicated");
                }
            }
            HierarchyIntent::Delete(id) => {
                let before = self.scene.clone();
                if self.scene.remove_node(id) {
                    let ids = self
                        .scene_selection
                        .selected_nodes
                        .iter()
                        .copied()
                        .filter(|selected| self.scene.is_valid_node(*selected))
                        .collect::<Vec<_>>();
                    self.set_scene_selection(ids);
                    self.commit_scene_change(before, "Hierarchy node deleted");
                }
            }
            HierarchyIntent::Ungroup(id) => {
                let before = self.scene.clone();
                if self.scene.ungroup_node(id) {
                    self.scene_selection
                        .selected_nodes
                        .retain(|selected| self.scene.is_valid_node(*selected));
                    self.scene_selection.selected_node =
                        self.scene_selection.selected_nodes.first().copied();
                    self.viewport
                        .set_selected_ids(self.scene_selection.selected_nodes.clone());
                    self.commit_scene_change(before, "Folder ungrouped");
                }
            }
            HierarchyIntent::Paste { sources, parent } => {
                let before = self.scene.clone();
                let parent = parent.filter(|id| self.scene.is_valid_node(*id));
                let mut pasted = Vec::new();
                for source in sources {
                    if let Some(id) = self.scene.duplicate_node_into(source, parent) {
                        pasted.push(id);
                    }
                }
                if !pasted.is_empty() {
                    self.set_scene_selection(pasted);
                    self.commit_scene_change(before, "Hierarchy nodes pasted");
                }
            }
            HierarchyIntent::Reparent {
                sources,
                target,
                before: insert_before,
            } => {
                let snapshot = self.scene.clone();
                if self
                    .scene
                    .reparent_nodes_before(&sources, target, insert_before)
                {
                    self.commit_scene_change(snapshot, "Hierarchy node reparented");
                }
            }
            HierarchyIntent::Focus(id) => {
                if self.scene.is_valid_node(id) {
                    self.set_scene_selection(vec![id]);
                    self.viewport.focus_entity(&self.scene, Some(id));
                    self.last_action = "Entity focused".to_string();
                }
            }
            HierarchyIntent::SaveBookmark(slot) => self.save_camera_bookmark(slot),
            HierarchyIntent::RestoreBookmark(slot) => self.restore_camera_bookmark(slot),
            HierarchyIntent::OpenBottomTab(tab) => {
                self.bottom_dock.open_tab(&tab);
            }
            HierarchyIntent::OpenSearch(query) => {
                self.search_surface.open(query);
            }
            HierarchyIntent::TogglePanel => {
                self.hierarchy_open = false;
            }
        }
    }

    fn build_search_results(&self, language: raf_core::config::Language) -> Vec<SearchResult> {
        let query = self.search_surface.query().trim().to_ascii_lowercase();
        let matches = |label: &str, detail: &str| {
            query.is_empty()
                || label.to_ascii_lowercase().contains(&query)
                || detail.to_ascii_lowercase().contains(&query)
        };
        let mut results = Vec::new();
        for command in self.command_catalog.command_names() {
            if matches(&command, "Command") {
                results.push(SearchResult {
                    label: command.clone(),
                    detail: t("search.detail.command", language),
                    kind: SearchResultKind::Command(command),
                    icon: raf_ui::UiIconId::Settings,
                });
            }
        }
        if self.is_game_project() {
            for (id, node) in self.scene.iter() {
                if matches(&node.name, "Scene entity") {
                    results.push(SearchResult {
                        label: node.name.clone(),
                        detail: t("search.detail.entity", language),
                        kind: SearchResultKind::Hierarchy(id),
                        icon: if node.is_folder {
                            raf_ui::UiIconId::Folder
                        } else {
                            raf_ui::UiIconId::Entity
                        },
                    });
                }
            }
        }
        if self.is_game_project() {
            for asset in self.project_catalog.assets() {
                if matches(asset, "Asset") {
                    results.push(SearchResult {
                        label: asset.clone(),
                        detail: t("search.detail.asset", language),
                        kind: SearchResultKind::Asset(asset.clone()),
                        icon: raf_ui::UiIconId::Assets,
                    });
                }
            }
            for entry in self.project_catalog.project_entries() {
                if matches(&entry.label, "Project file") {
                    results.push(SearchResult {
                        label: entry.label.clone(),
                        detail: t("search.detail.project", language),
                        kind: SearchResultKind::Project(entry.label.clone()),
                        icon: entry.icon,
                    });
                }
            }
        }
        results.truncate(120);
        results
    }

    fn apply_search_intent(&mut self, ctx: &egui::Context, intent: SearchIntent) {
        match intent {
            SearchIntent::QueryChanged(_) => {}
            SearchIntent::Close => {}
            SearchIntent::Activate(result) => match result.kind {
                SearchResultKind::Command(command) => {
                    let command = command.trim_start_matches('/');
                    if self.command_catalog.find(command).is_some() {
                        self.bottom_dock.open_tab("console");
                        self.console.set_input(format!("/{command}"));
                        self.last_action = format!("Command ready: /{command}");
                        self.console.log(
                            crate::console::LogLevel::Info,
                            format!("Search activated command: /{command}"),
                        );
                    } else {
                        self.dispatch_application_bar_command(ctx, command);
                    }
                }
                SearchResultKind::Hierarchy(id) => {
                    self.apply_hierarchy_intent(HierarchyIntent::Focus(id));
                    self.console.log(
                        crate::console::LogLevel::Info,
                        format!("Search focused hierarchy node: {}", id.0),
                    );
                }
                SearchResultKind::Asset(asset) => {
                    self.bottom_dock.open_tab("assets");
                    self.last_action = format!("Asset: {asset}");
                    self.console.log(
                        crate::console::LogLevel::Info,
                        format!("Search selected asset: {asset}"),
                    );
                }
                SearchResultKind::Project(path) => {
                    if let Some(project) = self.current_project.as_ref() {
                        let path = project.path.join(path.replace('/', "\\"));
                        // Project is intentionally no longer a permanent dock
                        // tab. Search is the only current project-file entry
                        // point; reveal it without probing the filesystem on
                        // the UI thread.
                        let _ = std::process::Command::new("explorer")
                            .arg("/select,")
                            .arg(&path)
                            .spawn();
                        self.last_action = format!("Project path revealed: {}", path.display());
                        self.console.log(
                            crate::console::LogLevel::Info,
                            format!("Search revealed project path: {}", path.display()),
                        );
                    }
                }
            },
        }
    }

    fn record_nodes_history(&mut self) {
        self.nodes_history
            .truncate(self.nodes_history_cursor.saturating_add(1));
        self.nodes_history.push(self.nodes_document.clone());
        self.nodes_history_cursor = self.nodes_history.len().saturating_sub(1);
        if self.nodes_history.len() > 64 {
            self.nodes_history.remove(0);
            self.nodes_history_cursor = self.nodes_history_cursor.saturating_sub(1);
        }
    }

    fn reset_nodes_history(&mut self) {
        self.nodes_history.clear();
        self.nodes_history.push(self.nodes_document.clone());
        self.nodes_history_cursor = 0;
        self.nodes_drag_changed = false;
    }

    fn undo_nodes(&mut self) {
        if !self.is_game_project() || !self.game_documents_ready || self.nodes_history_cursor == 0 {
            return;
        }
        self.nodes_history_cursor = self.nodes_history_cursor.saturating_sub(1);
        if let Some(document) = self.nodes_history.get(self.nodes_history_cursor).cloned() {
            self.nodes_document = document;
            self.bottom_dock.sync_nodes_selection(None);
            self.bottom_dock.mark_nodes_changed();
            self.save_nodes_if_linear();
            self.last_action = "Node graph change undone".to_string();
        }
    }

    fn redo_nodes(&mut self) {
        if !self.is_game_project()
            || !self.game_documents_ready
            || self.nodes_history_cursor + 1 >= self.nodes_history.len()
        {
            return;
        }
        self.nodes_history_cursor += 1;
        if let Some(document) = self.nodes_history.get(self.nodes_history_cursor).cloned() {
            self.nodes_document = document;
            self.bottom_dock.sync_nodes_selection(None);
            self.bottom_dock.mark_nodes_changed();
            self.save_nodes_if_linear();
            self.last_action = "Node graph change redone".to_string();
        }
    }

    fn apply_nodes_intent(&mut self, intent: NodesIntent) {
        if !self.is_game_project() || !self.game_documents_ready {
            return;
        }
        match intent {
            NodesIntent::SelectGraph(index) => {
                if index < self.nodes_document.graphs.len() {
                    self.nodes_document.active_graph_index = index;
                    self.bottom_dock.sync_nodes_selection(None);
                    self.last_action = format!("Node graph selected: {index}");
                }
            }
            NodesIntent::DeleteGraph(index) => {
                if self.nodes_document.graphs.len() <= 1
                    || index >= self.nodes_document.graphs.len()
                {
                    return;
                }
                self.nodes_document.graphs.remove(index);
                self.nodes_document.active_graph_index = self
                    .nodes_document
                    .active_graph_index
                    .min(self.nodes_document.graphs.len().saturating_sub(1));
                self.bottom_dock.sync_nodes_selection(None);
                self.bottom_dock.mark_nodes_changed();
                self.record_nodes_history();
                self.save_nodes_if_linear();
                self.last_action = format!("Node graph deleted: {index}");
            }
            NodesIntent::NewGraph => {
                let name = format!("Graph {}", self.nodes_document.graphs.len() + 1);
                self.nodes_document
                    .graphs
                    .push(raf_nodes::NodeGraph::new(&name));
                self.nodes_document.active_graph_index = self.nodes_document.graphs.len() - 1;
                self.bottom_dock.sync_nodes_selection(None);
                self.bottom_dock.mark_nodes_changed();
                self.record_nodes_history();
                self.save_nodes_if_linear();
                self.last_action = format!("Node graph created: {name}");
            }
            NodesIntent::AddNode(preset) => {
                let active = self
                    .nodes_document
                    .active_graph_index
                    .min(self.nodes_document.graphs.len().saturating_sub(1));
                let node = preset.create();
                let id = node.id;
                if let Some(graph) = self.nodes_document.graphs.get_mut(active) {
                    graph.add_node(node);
                    self.bottom_dock.sync_nodes_selection(Some(id));
                    self.bottom_dock.mark_nodes_changed();
                    self.record_nodes_history();
                    self.save_nodes_if_linear();
                    self.last_action = format!("Node added: {}", preset.label());
                }
            }
            NodesIntent::SelectNode(index) => {
                let active = self
                    .nodes_document
                    .active_graph_index
                    .min(self.nodes_document.graphs.len().saturating_sub(1));
                let selected = self
                    .nodes_document
                    .graphs
                    .get(active)
                    .and_then(|graph| graph.nodes.get(index))
                    .map(|node| node.id);
                self.bottom_dock.sync_nodes_selection(selected);
            }
            NodesIntent::DeleteNode(index) => {
                let active = self
                    .nodes_document
                    .active_graph_index
                    .min(self.nodes_document.graphs.len().saturating_sub(1));
                let target = self
                    .nodes_document
                    .graphs
                    .get(active)
                    .and_then(|graph| graph.nodes.get(index))
                    .map(|node| (node.id, node.name.clone()));
                if let Some((id, name)) = target {
                    if let Some(graph) = self.nodes_document.graphs.get_mut(active) {
                        graph.remove_node(id);
                        self.bottom_dock.sync_nodes_selection(None);
                        self.bottom_dock.mark_nodes_changed();
                        self.record_nodes_history();
                        self.save_nodes_if_linear();
                        self.last_action = format!("Node deleted: {name}");
                    }
                }
            }
            NodesIntent::CopyNode(index) => {
                let active = self
                    .nodes_document
                    .active_graph_index
                    .min(self.nodes_document.graphs.len().saturating_sub(1));
                self.nodes_clipboard = self
                    .nodes_document
                    .graphs
                    .get(active)
                    .and_then(|graph| graph.nodes.get(index))
                    .cloned();
                if self.nodes_clipboard.is_some() {
                    self.last_action = "Node copied".to_string();
                }
            }
            NodesIntent::PasteNode => {
                let Some(mut node) = self.nodes_clipboard.clone() else {
                    return;
                };
                node.id = raf_nodes::NodeId::new();
                node.name = format!("{} (copy)", node.name);
                node.position[0] += 28.0;
                node.position[1] += 28.0;
                for pin in &mut node.pins {
                    pin.id = uuid::Uuid::new_v4();
                }
                let id = node.id;
                let active = self
                    .nodes_document
                    .active_graph_index
                    .min(self.nodes_document.graphs.len().saturating_sub(1));
                if let Some(graph) = self.nodes_document.graphs.get_mut(active) {
                    graph.add_node(node);
                    self.bottom_dock.sync_nodes_selection(Some(id));
                    self.bottom_dock.mark_nodes_changed();
                    self.record_nodes_history();
                    self.save_nodes_if_linear();
                    self.last_action = "Node pasted".to_string();
                }
            }
            NodesIntent::BeginNodeDrag(_) => {
                self.nodes_drag_changed = false;
            }
            NodesIntent::MoveNode { index, position } => {
                let active = self
                    .nodes_document
                    .active_graph_index
                    .min(self.nodes_document.graphs.len().saturating_sub(1));
                if let Some(node) = self
                    .nodes_document
                    .graphs
                    .get_mut(active)
                    .and_then(|graph| graph.nodes.get_mut(index))
                {
                    if position.iter().all(|value| value.is_finite()) {
                        let next = [position[0].max(-10_000.0), position[1].max(-10_000.0)];
                        if node.position != next {
                            node.position = next;
                            self.nodes_drag_changed = true;
                            self.bottom_dock.mark_nodes_changed();
                        }
                    }
                }
            }
            NodesIntent::EndNodeDrag => {
                if self.nodes_drag_changed {
                    self.record_nodes_history();
                    self.save_nodes_if_linear();
                }
                self.nodes_drag_changed = false;
            }
            NodesIntent::SelectPin {
                node_index,
                pin_index,
            } => {
                self.last_action = format!("Node pin selected: {node_index}:{pin_index}");
            }
            NodesIntent::ConnectPins {
                from_node,
                from_pin,
                to_node,
                to_pin,
            } => {
                let active = self
                    .nodes_document
                    .active_graph_index
                    .min(self.nodes_document.graphs.len().saturating_sub(1));
                let endpoint_data = self.nodes_document.graphs.get(active).map(|graph| {
                    let endpoint = |node_index: usize, pin_index: usize| {
                        graph
                            .nodes
                            .get(node_index)
                            .and_then(|node| node.pins.get(pin_index))
                            .map(|pin| (pin.id, pin.kind, pin.data_type))
                    };
                    (endpoint(from_node, from_pin), endpoint(to_node, to_pin))
                });
                let Some((Some((from_id, from_kind, from_type)), Some((to_id, to_kind, to_type)))) =
                    endpoint_data
                else {
                    return;
                };
                let (
                    source_node,
                    source_pin,
                    source_kind,
                    source_type,
                    target_node,
                    target_pin,
                    target_kind,
                    target_type,
                ) = if from_kind == raf_nodes::node::PinKind::Output
                    && to_kind == raf_nodes::node::PinKind::Input
                {
                    (
                        from_node, from_id, from_kind, from_type, to_node, to_id, to_kind, to_type,
                    )
                } else if from_kind == raf_nodes::node::PinKind::Input
                    && to_kind == raf_nodes::node::PinKind::Output
                {
                    (
                        to_node, to_id, to_kind, to_type, from_node, from_id, from_kind, from_type,
                    )
                } else {
                    return;
                };
                let compatible = source_kind == raf_nodes::node::PinKind::Output
                    && target_kind == raf_nodes::node::PinKind::Input
                    && (source_type == target_type
                        || source_type == raf_nodes::node::PinDataType::Any
                        || target_type == raf_nodes::node::PinDataType::Any);
                let duplicate = self.nodes_document.graphs.get(active).is_some_and(|graph| {
                    graph.connections.iter().any(|connection| {
                        connection.from_node == graph.nodes[source_node].id
                            && connection.from_pin == source_pin
                            && connection.to_node == graph.nodes[target_node].id
                            && connection.to_pin == target_pin
                    })
                });
                if compatible && !duplicate {
                    let Some(graph) = self.nodes_document.graphs.get_mut(active) else {
                        return;
                    };
                    let connection_id = graph.connect(
                        graph.nodes[source_node].id,
                        source_pin,
                        graph.nodes[target_node].id,
                        target_pin,
                    );
                    self.bottom_dock.mark_nodes_changed();
                    self.record_nodes_history();
                    self.save_nodes_if_linear();
                    self.last_action = format!("Node connection created: {connection_id}");
                } else {
                    self.last_action = "Node pins are incompatible".to_string();
                }
            }
            NodesIntent::ZoomIn | NodesIntent::ZoomOut | NodesIntent::ResetZoom => {}
            NodesIntent::Undo => self.undo_nodes(),
            NodesIntent::Redo => self.redo_nodes(),
        }
    }

    fn apply_inspector_intent(&mut self, intent: InspectorIntent) {
        if !self.is_game_project() || !self.game_documents_ready {
            return;
        }
        match intent {
            InspectorIntent::SelectTab(_) => {}
            InspectorIntent::CreateSession { name } => {
                self.execute_session_ui_command(
                    "session.create",
                    [
                        ("name".to_string(), name),
                        ("kind".to_string(), "world".to_string()),
                    ],
                );
            }
            InspectorIntent::OpenSession(id) => {
                self.execute_session_ui_command(
                    "session.open",
                    [("session".to_string(), id.0.to_string())],
                );
            }
            InspectorIntent::DuplicateSession { source, name } => {
                self.execute_session_ui_command(
                    "session.duplicate",
                    [
                        ("source".to_string(), source.0.to_string()),
                        ("name".to_string(), name),
                    ],
                );
            }
            InspectorIntent::RemoveSession(id) => {
                self.execute_session_ui_command(
                    "session.remove",
                    [("session".to_string(), id.0.to_string())],
                );
            }
            InspectorIntent::Rename { id, name } => {
                self.apply_hierarchy_intent(HierarchyIntent::Rename { id, name });
            }
            InspectorIntent::ToggleVisibility(id) => {
                let before = self.scene.clone();
                let Some(next_visible) = self.scene.get(id).map(|node| !node.visible) else {
                    return;
                };
                let ids = if self.scene_selection.selected_nodes.contains(&id) {
                    self.scene_selection.selected_nodes.clone()
                } else {
                    vec![id]
                };
                for selected_id in ids {
                    if let Some(node) = self.scene.get_mut(selected_id) {
                        node.visible = next_visible;
                    }
                }
                self.commit_scene_change(before, "Visibility changed: Inspector selection");
            }
            InspectorIntent::ToggleLocked(id) => {
                let before = self.scene.clone();
                let Some(next_locked) = self.scene.get(id).map(|node| !node.locked) else {
                    return;
                };
                let ids = if self.scene_selection.selected_nodes.contains(&id) {
                    self.scene_selection.selected_nodes.clone()
                } else {
                    vec![id]
                };
                for selected_id in ids {
                    if let Some(node) = self.scene.get_mut(selected_id) {
                        node.locked = next_locked;
                    }
                }
                self.commit_scene_change(before, "Lock changed: Inspector selection");
            }
            InspectorIntent::SetPrimitive { id, primitive } => {
                let before = self.scene.clone();
                if let Some(node) = self.scene.get_mut(id) {
                    if node.primitive != primitive {
                        node.primitive = primitive;
                        node.color = NodeColor::for_primitive(primitive);
                        self.commit_scene_change(before, "Primitive changed");
                    }
                }
            }
            InspectorIntent::SetColor { id, color } => {
                let before = self.scene.clone();
                let selected_nodes = self.scene_selection.selected_nodes.clone();
                let mut changed = false;
                if let Some(node) = self.scene.get_mut(id) {
                    if node.color != color {
                        node.color = color;
                        changed = true;
                    }
                }
                if changed {
                    for selected_id in selected_nodes {
                        if selected_id != id {
                            if let Some(node) = self.scene.get_mut(selected_id) {
                                node.color = color;
                            }
                        }
                    }
                    self.commit_scene_change(before, "Material changed");
                }
            }
            InspectorIntent::SetAudioEnabled { id, enabled } => {
                let before = self.scene.clone();
                if let Some(node) = self.scene.get_mut(id) {
                    if node.audio_source.enabled != enabled {
                        node.audio_source.enabled = enabled;
                        self.commit_scene_change(before, "Audio changed");
                    }
                }
            }
            InspectorIntent::SetAudioClip { id, clip } => {
                let before = self.scene.clone();
                if let Some(node) = self.scene.get_mut(id) {
                    if node.audio_source.clip != clip {
                        node.audio_source.clip = clip;
                        self.commit_scene_change(before, "Audio changed");
                    }
                }
            }
            InspectorIntent::SetAudioAutoplay { id, autoplay } => {
                let before = self.scene.clone();
                if let Some(node) = self.scene.get_mut(id) {
                    if node.audio_source.autoplay != autoplay {
                        node.audio_source.autoplay = autoplay;
                        self.commit_scene_change(before, "Audio changed");
                    }
                }
            }
            InspectorIntent::SetAudioLooping { id, looping } => {
                let before = self.scene.clone();
                if let Some(node) = self.scene.get_mut(id) {
                    if node.audio_source.looping != looping {
                        node.audio_source.looping = looping;
                        self.commit_scene_change(before, "Audio changed");
                    }
                }
            }
            InspectorIntent::SetAudioVolume { id, volume } => {
                let before = self.scene.clone();
                if let Some(node) = self.scene.get_mut(id) {
                    let volume = volume.clamp(0.0, 1.0);
                    if (node.audio_source.volume - volume).abs() > f32::EPSILON {
                        node.audio_source.volume = volume;
                        self.commit_scene_change(before, "Audio volume changed");
                    }
                }
            }
            InspectorIntent::SetRigidBodyEnabled { id, enabled } => {
                let before = self.scene.clone();
                if let Some(node) = self.scene.get_mut(id) {
                    if node.rigid_body.enabled != enabled {
                        node.rigid_body.enabled = enabled;
                        self.commit_scene_change(before, "Physics changed");
                    }
                }
            }
            InspectorIntent::SetColliderType { id, collider_type } => {
                let before = self.scene.clone();
                if let Some(node) = self.scene.get_mut(id) {
                    if node.collider.collider_type != collider_type {
                        node.collider = if collider_type == raf_core::scene::ColliderType::None {
                            Collider::default()
                        } else {
                            Collider::auto_fit(
                                &primitive_collider_points(node.primitive),
                                collider_type,
                            )
                        };
                        self.commit_scene_change(before, "Physics changed");
                    }
                }
            }
            InspectorIntent::SetRigidBodyType { id, body_type } => {
                let before = self.scene.clone();
                if let Some(node) = self.scene.get_mut(id) {
                    if node.rigid_body.body_type != body_type {
                        node.rigid_body.body_type = body_type;
                        self.commit_scene_change(before, "Physics changed");
                    }
                }
            }
            InspectorIntent::SetGravity { id, enabled } => {
                let before = self.scene.clone();
                if let Some(node) = self.scene.get_mut(id) {
                    if node.rigid_body.use_gravity != enabled {
                        node.rigid_body.use_gravity = enabled;
                        self.commit_scene_change(before, "Physics changed");
                    }
                }
            }
            InspectorIntent::SetTrigger { id, enabled } => {
                let before = self.scene.clone();
                if let Some(node) = self.scene.get_mut(id) {
                    if node.rigid_body.is_trigger != enabled {
                        node.rigid_body.is_trigger = enabled;
                        self.commit_scene_change(before, "Physics changed");
                    }
                }
            }
            InspectorIntent::SetDamping { id, damping } => {
                let before = self.scene.clone();
                if let Some(node) = self.scene.get_mut(id) {
                    let damping = damping.clamp(0.0, 1.0);
                    if (node.rigid_body.damping - damping).abs() > f32::EPSILON {
                        node.rigid_body.damping = damping;
                        self.commit_scene_change(before, "Physics damping changed");
                    }
                }
            }
            InspectorIntent::SetVelocity { id, velocity } => {
                let before = self.scene.clone();
                if let Some(node) = self.scene.get_mut(id) {
                    if node.rigid_body.velocity != velocity {
                        node.rigid_body.velocity = velocity;
                        self.commit_scene_change(before, "Physics changed");
                    }
                }
            }
            InspectorIntent::SetVariableName { id, index, name } => {
                let before = self.scene.clone();
                if let Some(node) = self.scene.get_mut(id) {
                    if let Some(variable) = node.variables.get_mut(index) {
                        if variable.name != name {
                            variable.name = name;
                            self.commit_scene_change(before, "Variable changed");
                        }
                    }
                }
            }
            InspectorIntent::SetVariableValue { id, index, value } => {
                let before = self.scene.clone();
                if let Some(node) = self.scene.get_mut(id) {
                    if let Some(variable) = node.variables.get_mut(index) {
                        let next = match &variable.value {
                            VariableValue::Bool(_) => value
                                .parse::<bool>()
                                .map(VariableValue::Bool)
                                .unwrap_or_else(|_| variable.value.clone()),
                            VariableValue::Number(_) => value
                                .parse::<f32>()
                                .map(VariableValue::Number)
                                .unwrap_or_else(|_| variable.value.clone()),
                            VariableValue::Text(_) => VariableValue::Text(value),
                        };
                        if variable.value != next {
                            variable.value = next;
                            self.commit_scene_change(before, "Variable changed");
                        }
                    }
                }
            }
            InspectorIntent::AddVariable(id) => {
                let before = self.scene.clone();
                if let Some(node) = self.scene.get_mut(id) {
                    let index = node.variables.len() + 1;
                    node.variables.push(raf_core::scene::SceneVariable {
                        name: format!("var_{index}"),
                        value: VariableValue::Number(0.0),
                    });
                    self.commit_scene_change(before, "Variable added");
                }
            }
            InspectorIntent::RemoveVariable { id, index } => {
                let before = self.scene.clone();
                if let Some(node) = self.scene.get_mut(id) {
                    if index < node.variables.len() {
                        node.variables.remove(index);
                        self.commit_scene_change(before, "Variable removed");
                    }
                }
            }
            InspectorIntent::CycleVariableType { id, index } => {
                let before = self.scene.clone();
                if let Some(node) = self.scene.get_mut(id) {
                    if let Some(variable) = node.variables.get_mut(index) {
                        variable.value = match &variable.value {
                            VariableValue::Bool(_) => VariableValue::Number(0.0),
                            VariableValue::Number(_) => VariableValue::Text(String::new()),
                            VariableValue::Text(_) => VariableValue::Bool(false),
                        };
                        self.commit_scene_change(before, "Variable type changed");
                    }
                }
            }
            InspectorIntent::EndGesture => {
                self.scene_mutation_group = None;
            }
            InspectorIntent::SetTransform {
                id,
                position,
                rotation,
                scale,
            } => {
                let before = self.scene.clone();
                if let Some(node) = self.scene.get_mut(id) {
                    node.position = position;
                    node.rotation = rotation;
                    node.scale = scale;
                    self.commit_scene_change(before, "Transform changed");
                }
            }
            InspectorIntent::ResetTransform(id) => {
                let before = self.scene.clone();
                if let Some(node) = self.scene.get_mut(id) {
                    node.position = glam::Vec3::ZERO;
                    node.rotation = glam::Vec3::ZERO;
                    node.scale = glam::Vec3::ONE;
                    self.commit_scene_change(before, "Transform reset");
                }
            }
            InspectorIntent::ResetAll(id) => {
                let before = self.scene.clone();
                if let Some(node) = self.scene.get_mut(id) {
                    node.position = glam::Vec3::ZERO;
                    node.rotation = glam::Vec3::ZERO;
                    node.scale = glam::Vec3::ONE;
                    node.color = NodeColor::for_primitive(node.primitive);
                    node.visible = true;
                    self.commit_scene_change(before, "Inspector reset all");
                }
            }
            InspectorIntent::TogglePanel => {
                self.inspector_open = false;
            }
        }
    }

    fn execute_session_ui_command<const N: usize>(
        &mut self,
        name: &str,
        args: [(String, String); N],
    ) {
        let command = ParsedCommand {
            raw: name.to_string(),
            name: name.to_string(),
            args: args
                .into_iter()
                .collect::<std::collections::BTreeMap<_, _>>(),
            positional: Vec::new(),
        };
        let output = self.execute_shared_console_command(name, &command);
        self.last_action = output.title.clone();
        self.console.log_command_output(output);
        self.process_session_events();
    }

    fn commit_scene_change(&mut self, before: SceneGraph, action: &str) {
        let coalescible = matches!(
            action,
            "Transform changed"
                | "Material changed"
                | "Audio volume changed"
                | "Physics damping changed"
        );
        let coalesce = coalescible && self.scene_mutation_group.as_deref() == Some(action);
        if self
            .scene_history
            .record_if_changed_grouped(before, &self.scene, coalesce)
        {
            self.scene_revision = self.scene_revision.wrapping_add(1);
            // Any new edit makes older attached tokens unsafe. The token for
            // the current attached edit is inserted immediately afterwards.
            self.attached_undo.clear();
            self.scene_mutation_group = Some(action.to_string());
            if matches!(
                action,
                "Agent changed the scene"
                    | "Agent attached changed the scene"
                    | "Agent approved scene change"
                    | "Agent denied scene change"
                    | "Console changed the scene"
                    | "Visibility changed"
                    | "Lock changed"
                    | "Hierarchy node renamed"
                    | "Folder created"
                    | "Entity created"
                    | "Hierarchy node duplicated"
                    | "Hierarchy node deleted"
                    | "Folder ungrouped"
                    | "Hierarchy node reparented"
                    | "Hierarchy nodes pasted"
                    | "Primitive changed"
                    | "Inspector reset all"
            ) || action.starts_with("Visibility changed:")
                || action.starts_with("Lock changed:")
            {
                self.hierarchy.sync_scene(&self.scene);
            }
            self.last_action = action.to_string();
            self.console.log(
                crate::console::LogLevel::Info,
                format!("Scene change: {action}"),
            );
            self.save_scene_if_linear();
            self.attached_host.update_revision(self.scene_revision);
        }
    }

    fn apply_agent_editor_action(&mut self, action: AgentEditorAction) {
        match action {
            AgentEditorAction::Undo if self.is_electronics_project() => self.undo_electronics(),
            AgentEditorAction::Redo if self.is_electronics_project() => self.redo_electronics(),
            AgentEditorAction::Undo => {
                self.undo_scene();
            }
            AgentEditorAction::Redo => {
                self.redo_scene();
            }
        }
    }

    fn undo_scene(&mut self) -> bool {
        self.attached_undo.clear();
        if self.scene_history.undo(&mut self.scene) {
            self.scene_revision = self.scene_revision.wrapping_add(1);
            self.viewport.clear_scene_edit_snapshot();
            self.scene_selection = SceneSelectionState::default();
            self.viewport.set_selected_ids(Vec::new());
            self.hierarchy.sync_scene(&self.scene);
            self.save_scene_if_linear();
            self.last_action = "Undo".to_string();
            self.attached_host.update_revision(self.scene_revision);
            return true;
        }
        false
    }

    fn redo_scene(&mut self) -> bool {
        self.attached_undo.clear();
        if self.scene_history.redo(&mut self.scene) {
            self.scene_revision = self.scene_revision.wrapping_add(1);
            self.viewport.clear_scene_edit_snapshot();
            self.scene_selection = SceneSelectionState::default();
            self.viewport.set_selected_ids(Vec::new());
            self.hierarchy.sync_scene(&self.scene);
            self.save_scene_if_linear();
            self.last_action = "Redo".to_string();
            self.attached_host.update_revision(self.scene_revision);
            return true;
        }
        false
    }

    fn set_scene_selection(&mut self, ids: Vec<SceneNodeId>) {
        if !self.is_game_project() || !self.game_documents_ready {
            return;
        }
        let ids = ids
            .into_iter()
            .filter(|id| self.scene.is_valid_node(*id))
            .collect::<Vec<_>>();
        self.scene_selection.selected_node = ids.first().copied();
        self.scene_selection.selected_nodes = ids.clone();
        self.viewport.set_selected_ids(ids);
        if self.settings.hierarchy_auto_reveal_selection {
            if let Some(id) = self.scene_selection.selected_node {
                self.hierarchy.reveal_node_with_options(
                    &self.scene,
                    id,
                    self.settings.hierarchy_expand_on_select,
                );
            }
        }
    }

    fn save_scene_if_linear(&mut self) {
        let Some(project) = self.current_project.as_ref() else {
            return;
        };
        if project.project_type != ProjectType::Game
            || !project.settings.linear_save
            || !self.game_documents_ready
        {
            return;
        }
        let Some(session) = self.sessions.active() else {
            return;
        };
        let scene_path = session.path(&project.path, &session.scene_file);
        let data = match ron::ser::to_string_pretty(&self.scene, ron::ser::PrettyConfig::default())
        {
            Ok(data) => data,
            Err(error) => {
                tracing::error!(%error, "linear scene serialization failed");
                return;
            }
        };
        if let Err(error) = self.queue_persistence(
            "Scene",
            vec![PersistenceWrite {
                path: scene_path,
                data,
            }],
        ) {
            tracing::error!(%error, "linear scene save failed");
        }
    }

    fn save_nodes_if_linear(&mut self) {
        let Some(project) = self.current_project.as_ref() else {
            return;
        };
        if project.project_type != ProjectType::Game
            || !project.settings.linear_save
            || !self.game_documents_ready
        {
            return;
        }
        if let Err(error) = self.save_nodes_document() {
            tracing::error!(%error, "linear node graph save failed");
        }
    }

    fn save_scene_document(&mut self) -> Result<(), String> {
        let project = self
            .current_project
            .as_ref()
            .ok_or_else(|| "No active project".to_string())?;
        let session = self
            .sessions
            .active()
            .ok_or_else(|| "No active project session".to_string())?;
        match project.project_type {
            ProjectType::Game if !self.game_documents_ready => Ok(()),
            ProjectType::Game => {
                let scene_path = session.path(&project.path, &session.scene_file);
                let data =
                    ron::ser::to_string_pretty(&self.scene, ron::ser::PrettyConfig::default())
                        .map_err(|error| error.to_string())?;
                self.queue_persistence(
                    "Scene",
                    vec![PersistenceWrite {
                        path: scene_path,
                        data,
                    }],
                )
            }
            ProjectType::Electronics => {
                let schematic_path = session.path(&project.path, &session.schematic_file);
                let pcb_path = session.path(&project.path, &session.pcb_file);
                let schematic_data = ron::ser::to_string_pretty(
                    &self.schematic_view.schematic,
                    ron::ser::PrettyConfig::default(),
                )
                .map_err(|error| error.to_string())?;
                let pcb_data = ron::ser::to_string_pretty(
                    &self.pcb_view.layout,
                    ron::ser::PrettyConfig::default(),
                )
                .map_err(|error| error.to_string())?;
                self.queue_persistence(
                    "Electronics",
                    vec![
                        PersistenceWrite {
                            path: schematic_path,
                            data: schematic_data,
                        },
                        PersistenceWrite {
                            path: pcb_path,
                            data: pcb_data,
                        },
                    ],
                )
            }
        }
    }

    fn save_nodes_document(&mut self) -> Result<(), String> {
        let project = self
            .current_project
            .as_ref()
            .ok_or_else(|| "No active project".to_string())?;
        if project.project_type != ProjectType::Game {
            return Ok(());
        }
        if !self.game_documents_ready {
            return Ok(());
        }
        let session = self
            .sessions
            .active()
            .ok_or_else(|| "No active project session".to_string())?;
        if self.nodes_session_id != Some(session.id) {
            return Err("Node document is not scoped to the active Game session".to_string());
        }
        let path = session.path(&project.path, &session.nodes_file);
        let data =
            ron::ser::to_string_pretty(&self.nodes_document, ron::ser::PrettyConfig::default())
                .map_err(|error| error.to_string())?;
        self.queue_persistence("Nodes", vec![PersistenceWrite { path, data }])
    }

    fn save_editor_camera_document(&mut self) -> Result<(), String> {
        let project = self
            .current_project
            .as_ref()
            .ok_or_else(|| "No active project".to_string())?;
        if project.project_type != ProjectType::Game {
            return Ok(());
        }
        if !self.game_documents_ready {
            return Ok(());
        }
        let session = self
            .sessions
            .active()
            .ok_or_else(|| "No active project session".to_string())?;
        let mut camera = self.viewport.editor_camera_block();
        camera.bookmarks = self
            .camera_bookmarks
            .iter()
            .filter_map(|bookmark| bookmark.clone())
            .collect();
        let path = session.path(&project.path, &session.editor_camera_file());
        let data = ron::ser::to_string_pretty(&camera, ron::ser::PrettyConfig::default())
            .map_err(|error| error.to_string())?;
        self.queue_persistence("Editor camera", vec![PersistenceWrite { path, data }])
    }

    fn persist_active_project(&mut self) -> Result<(), String> {
        let project = self
            .current_project
            .as_ref()
            .ok_or_else(|| "No active project".to_string())?;
        let project_path = project.path.join(Project::META_FILE);
        let project_data = ron::ser::to_string_pretty(project, ron::ser::PrettyConfig::default())
            .map_err(|error| error.to_string())?;
        self.queue_persistence(
            "Project",
            vec![PersistenceWrite {
                path: project_path,
                data: project_data,
            }],
        )?;
        self.save_scene_document()?;
        self.save_nodes_document()?;
        self.save_editor_camera_document()
    }

    fn save_camera_bookmark(&mut self, slot: usize) {
        if !self.is_game_project()
            || !self.game_documents_ready
            || self.canvas_mode != CanvasMode::Game
            || slot >= self.camera_bookmarks.len()
        {
            return;
        }
        let block = self.viewport.editor_camera_block();
        self.camera_bookmarks[slot] = Some(block.bookmark(format!("bookmark_{}", slot + 1)));
        self.last_action = format!(
            "{} {}",
            t("app.bookmark_saved", self.settings.language),
            slot + 1
        );
        self.console
            .log(crate::console::LogLevel::Info, self.last_action.clone());
        if let Err(error) = self.save_editor_camera_document() {
            tracing::warn!(%error, slot = slot + 1, "camera bookmark could not be persisted yet");
        }
    }

    fn restore_camera_bookmark(&mut self, slot: usize) {
        if !self.is_game_project()
            || !self.game_documents_ready
            || self.canvas_mode != CanvasMode::Game
            || slot >= self.camera_bookmarks.len()
        {
            return;
        }
        let Some(bookmark) = self.camera_bookmarks[slot].clone() else {
            self.last_action = format!(
                "{} {}",
                t("app.bookmark_empty", self.settings.language),
                slot + 1
            );
            self.console
                .log(crate::console::LogLevel::Info, self.last_action.clone());
            return;
        };
        let mut block = self.viewport.editor_camera_block();
        block.mode = bookmark.mode;
        block.target = bookmark.target;
        block.yaw = bookmark.yaw;
        block.pitch = bookmark.pitch;
        block.distance = bookmark.distance;
        block.zoom_2d = bookmark.zoom_2d;
        self.viewport.apply_editor_camera_block(&block);
        self.last_action = format!(
            "{} {}",
            t("app.bookmark_restored", self.settings.language),
            slot + 1
        );
        self.console
            .log(crate::console::LogLevel::Info, self.last_action.clone());
    }
    fn dispatch_application_bar_command(&mut self, ctx: &egui::Context, command: &str) {
        match command {
            application_command::EDIT_UNDO => {
                if self.is_game_project() && self.bottom_dock.active_tab_is("nodes") {
                    self.undo_nodes();
                } else if self.is_electronics_project() {
                    self.undo_electronics();
                } else {
                    self.undo_scene();
                }
            }
            application_command::EDIT_REDO => {
                if self.is_game_project() && self.bottom_dock.active_tab_is("nodes") {
                    self.redo_nodes();
                } else if self.is_electronics_project() {
                    self.redo_electronics();
                } else {
                    self.redo_scene();
                }
            }
            application_command::EDIT_DUPLICATE => {
                if self.is_electronics_project() {
                    let before = self.electronics_snapshot();
                    let changed = match self.canvas_mode {
                        CanvasMode::Schematic => self.schematic_view.duplicate_selection(),
                        CanvasMode::Game => self.pcb_view.duplicate_selection(),
                    };
                    if changed {
                        self.schematic_view.schematic.sync_wire_anchors();
                        self.pcb_view.layout.rebuild_airwires();
                        self.record_electronics_change(
                            before,
                            &t("app.electronics_document_changed", self.settings.language),
                        );
                    }
                } else if let Some(id) = self.scene_selection.selected_node {
                    self.apply_hierarchy_intent(HierarchyIntent::Duplicate(id));
                }
            }
            application_command::EDIT_DELETE => {
                if self.is_electronics_project() {
                    let before = self.electronics_snapshot();
                    let changed = match self.canvas_mode {
                        CanvasMode::Schematic => self.schematic_view.delete_selection(),
                        CanvasMode::Game => self.pcb_view.delete_selection(),
                    };
                    if changed {
                        self.schematic_view.schematic.sync_wire_anchors();
                        self.pcb_view.layout.rebuild_airwires();
                        self.record_electronics_change(
                            before,
                            &t("app.electronics_document_changed", self.settings.language),
                        );
                    }
                } else {
                    let ids = self.scene_selection.selected_nodes.clone();
                    for id in ids {
                        self.apply_hierarchy_intent(HierarchyIntent::Delete(id));
                    }
                }
            }
            application_command::EDIT_SELECT_ALL => {
                if self.is_electronics_project() {
                    match self.canvas_mode {
                        CanvasMode::Schematic => self.schematic_view.select_all_components(),
                        CanvasMode::Game => {
                            self.last_action = t(
                                "app.electronics_select_all_unavailable",
                                self.settings.language,
                            );
                        }
                    }
                } else {
                    self.set_scene_selection(self.scene.all_valid_ids());
                }
            }
            application_command::PROJECT_NEW | application_command::PROJECT_OPEN => {
                let _ = self.persist_active_project();
                let _ = self.flush_persistence();
                self.current_project = None;
                self.attached_idempotency.clear();
                self.attached_undo.clear();
                self.update_attached_descriptor();
                self.screen = AppScreen::ProjectHub;
                self.last_action = "Project Hub opened".to_string();
            }
            application_command::EXIT_TO_HUB | application_command::PROJECT_CLOSE => {
                let _ = self.persist_active_project();
                let _ = self.flush_persistence();
                self.current_project = None;
                self.attached_idempotency.clear();
                self.attached_undo.clear();
                self.update_attached_descriptor();
                self.screen = AppScreen::ProjectHub;
                self.last_action = "Project closed".to_string();
            }
            application_command::EDITOR_SETTINGS => self.open_settings(true),
            application_command::PROJECT_SETTINGS => {
                self.bottom_dock.select_tab("project-settings");
            }
            application_command::AGENT_OPEN => {
                self.bottom_dock.open_tab("agent");
            }
            "search.open" => {
                self.search_surface
                    .open(self.application_bar.command_query().to_string());
            }
            "nodes.open" => {
                if self.is_game_project() {
                    self.bottom_dock.open_tab("nodes");
                }
            }
            application_command::PROJECT_SAVE => {
                if let Err(error) = self.persist_active_project() {
                    self.last_action = format!("Project save failed: {error}");
                } else if self.flush_persistence() {
                    self.last_action = "Project saved".to_string();
                } else {
                    self.last_action = "Project save failed".to_string();
                }
            }
            application_command::VIEW_GRID => {
                self.settings.grid_visible = !self.settings.grid_visible;
                let _ = self.settings.save(&dirs_config_dir());
            }
            application_command::VIEW_SCENE => {
                if self.is_game_project() {
                    self.canvas_mode = CanvasMode::Game;
                }
            }
            application_command::VIEW_HIERARCHY => {
                if self.is_game_project() {
                    self.hierarchy_open = !self.hierarchy_open;
                }
            }
            application_command::VIEW_INSPECTOR => {
                if self.is_game_project() {
                    self.inspector_open = !self.inspector_open;
                }
            }
            application_command::VIEW_SCHEMATIC => {
                if self.is_electronics_project() {
                    self.canvas_mode = CanvasMode::Schematic;
                }
            }
            application_command::VIEW_PCB => {
                if self.is_electronics_project() {
                    self.canvas_mode = CanvasMode::Game;
                }
            }
            application_command::PROJECT_OPEN_FOLDER => {
                if let Some(project) = self.current_project.as_ref() {
                    let _ = std::process::Command::new("explorer")
                        .arg(&project.path)
                        .spawn();
                }
            }
            application_command::HELP_KEYBOARD_SHORTCUTS => {
                self.last_action = "Keyboard shortcuts: Ctrl+S, Ctrl+K, Ctrl+Z, Ctrl+Y".to_string();
            }
            "window.minimize" => self.dispatch_window_command(ctx, UiWindowCommand::Minimize),
            "window.maximize" => self.dispatch_window_command(ctx, UiWindowCommand::ToggleMaximize),
            "window.drag" => self.dispatch_window_command(ctx, UiWindowCommand::BeginDrag),
            "window.close" => self.dispatch_window_command(ctx, UiWindowCommand::Close),
            _ => {}
        }
    }

    fn dispatch_window_command(&mut self, ctx: &egui::Context, command: UiWindowCommand) {
        match command {
            UiWindowCommand::BeginDrag => ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag),
            UiWindowCommand::Minimize => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true))
            }
            UiWindowCommand::ToggleMaximize => {
                let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            }
            UiWindowCommand::Close => {
                let _ = self.persist_active_project();
                let _ = self.flush_persistence();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            UiWindowCommand::BeginResize(_) | UiWindowCommand::ShowSystemMenu => {}
        }
    }

    fn status_items(&self, ctx: &egui::Context) -> Vec<String> {
        let language = self.settings.language;
        let project_name = self
            .current_project
            .as_ref()
            .map(|project| project.name.as_str())
            .unwrap_or("Untitled");
        if self.is_electronics_project() {
            let (selected, primary_count, secondary_count, primary_label, secondary_label) =
                match self.canvas_mode {
                    CanvasMode::Schematic => (
                        !matches!(
                            self.schematic_view.selection(),
                            crate::panels::schematic_view::SchematicSelection::None
                        ),
                        self.schematic_view.schematic.components.len(),
                        self.schematic_view.schematic.wires.len(),
                        t("app.schematic_components", language),
                        t("app.schematic_wires", language),
                    ),
                    CanvasMode::Game => (
                        !matches!(
                            self.pcb_view.selection(),
                            crate::panels::pcb_view::PcbSelection::None
                        ),
                        self.pcb_view.layout.components.len(),
                        self.pcb_view.layout.traces.len(),
                        t("app.pcb_components", language),
                        t("app.pcb_traces", language),
                    ),
                };
            let fps = ctx.input(|input| 1.0 / input.stable_dt.max(1.0 / 240.0));
            let mut items = vec![
                project_name.to_string(),
                format!(
                    "{}: {}",
                    t("app.status.selected", language),
                    if selected { 1 } else { 0 }
                ),
                format!("{primary_label}: {primary_count}"),
                format!("{secondary_label}: {secondary_count}"),
                format!("{fps:.0} FPS"),
            ];
            if !self.settings.show_fps_counter {
                items.pop();
            }
            return items;
        }
        let selected = self.viewport.selected.len();
        let actors = self.scene.all_valid_ids().len();
        let hidden = self
            .scene
            .iter()
            .filter(|(_, node)| !node.name.is_empty() && !node.visible)
            .count();
        let fps = ctx.input(|input| 1.0 / input.stable_dt.max(1.0 / 240.0));
        let grid_cm = (self.settings.grid_size.max(0.01) * 100.0).round();

        let mut items = vec![
            project_name.to_string(),
            format!(
                "{}: {selected}",
                raf_core::i18n::t("app.status.selected", language)
            ),
            format!(
                "{}: {actors}",
                raf_core::i18n::t("app.status.actors", language)
            ),
            format!(
                "{}: {hidden}",
                raf_core::i18n::t("app.status.hidden", language)
            ),
            format!(
                "{}: {}",
                raf_core::i18n::t("app.status.snap", language),
                if self.settings.snap_to_grid {
                    raf_core::i18n::t("app.status.on", language)
                } else {
                    raf_core::i18n::t("app.status.off", language)
                }
            ),
            format!(
                "{}: {grid_cm:.0} cm",
                raf_core::i18n::t("app.status.grid", language)
            ),
            format!("{}: 15°", raf_core::i18n::t("app.status.angle", language)),
            format!("{fps:.0} FPS"),
        ];
        if !self.settings.show_fps_counter {
            items.pop();
        }
        items
    }

    fn execute_console_submission(&mut self, submission: ConsoleSubmission) {
        let text = submission.text;
        self.console.log_user("User", &text);
        let parsed = match parse_console_input(&text) {
            Ok(parsed) => parsed,
            Err(error) => {
                self.console
                    .log_command_output(CommandOutput::error("Console parse", error));
                return;
            }
        };

        let ParsedInput::Command(command) = parsed else {
            if !text.trim().is_empty() {
                self.console
                    .log(crate::console::LogLevel::Info, text.trim());
            }
            return;
        };
        let Some(definition) = self.command_catalog.find(&command.name) else {
            self.console.log_command_output(CommandOutput::error(
                "Unknown command",
                format!("Command not found: /{}", command.name),
            ));
            return;
        };
        let command_name = definition.name.clone();
        let domain = definition.domain.clone();

        if !self.command_console_enabled() {
            self.console.log_command_output(CommandOutput::warning(
                "Console commands disabled",
                vec![
                    "Enable command_console_enabled in Settings or enable_console_commands in the active project.".to_string(),
                ],
                serde_json::json!({ "ok": false, "command": command_name }),
            ));
            return;
        }

        if (domain == "game" && (!self.is_game_project() || !self.game_documents_ready))
            || ((domain == "electronics" || domain == "pcb") && !self.is_electronics_project())
        {
            self.console.log_command_output(CommandOutput::warning(
                "Command unavailable",
                vec![format!(
                    "/{command_name} is not available for this project type."
                )],
                serde_json::json!({ "ok": false, "command": command_name }),
            ));
            return;
        }

        let output = match domain.as_str() {
            "game" => {
                let before = self.scene.clone();
                let mut context = commands::game::GameCommandContext {
                    scene: &mut self.scene,
                    selection: &mut self.scene_selection,
                    viewport: &mut self.viewport,
                };
                let output = commands::game::execute(&command_name, &command, &mut context);
                drop(context);
                if output.changed {
                    self.commit_scene_change(before, "Console changed the scene");
                }
                output
            }
            "electronics" | "pcb" => {
                let before = self.electronics_snapshot();
                let mut context = commands::electronics::ElectronicsCommandContext {
                    schematic_view: &mut self.schematic_view,
                    pcb_view: &mut self.pcb_view,
                };
                let output = commands::electronics::execute(&command_name, &command, &mut context);
                drop(context);
                if output.changed {
                    self.record_electronics_change(before, &output.title);
                }
                output
            }
            "shared" => self.execute_shared_console_command(&command_name, &command),
            _ => CommandOutput::error(
                "Command unavailable",
                format!("No editor executor is registered for domain: {domain}"),
            ),
        };
        if domain == "game" && self.game_documents_ready {
            if let Some(id) = self.scene_selection.selected_node {
                if self.settings.hierarchy_auto_reveal_selection {
                    self.hierarchy.reveal_node_with_options(
                        &self.scene,
                        id,
                        self.settings.hierarchy_expand_on_select,
                    );
                }
            }
        }
        self.last_action = output.title.clone();
        self.console.log_command_output(output);
        self.process_session_events();
    }

    fn command_console_enabled(&self) -> bool {
        self.settings.command_console_enabled
            || self
                .current_project
                .as_ref()
                .is_some_and(|project| project.settings.enable_console_commands)
    }

    fn execute_shared_console_command(
        &mut self,
        command_name: &str,
        command: &crate::commands::ParsedCommand,
    ) -> CommandOutput {
        match command_name {
            "help" | "commands" => {
                let lines = self
                    .command_catalog
                    .commands
                    .iter()
                    .map(|definition| format!("/{:<28} {}", definition.name, definition.domain))
                    .collect::<Vec<_>>();
                CommandOutput::info(
                    "Available commands",
                    lines,
                    serde_json::json!({ "ok": true }),
                )
            }
            "workspace.read" => self
                .current_project
                .as_ref()
                .map(|project| commands::workspace::read_file(command, &project.path))
                .unwrap_or_else(|| CommandOutput::error("Workspace read", "No active project.")),
            "workspace.search" => self
                .current_project
                .as_ref()
                .map(|project| commands::workspace::search(command, &project.path))
                .unwrap_or_else(|| CommandOutput::error("Workspace search", "No active project.")),
            "project.save" => match self.persist_active_project() {
                Ok(()) => CommandOutput::info(
                    "Project save queued",
                    vec!["Project, scene, nodes and camera writes were queued.".to_string()],
                    serde_json::json!({"queued": true}),
                ),
                Err(error) => CommandOutput::error("Project save", error),
            },
            name if name.starts_with("session.") => {
                let project = self.current_project.as_ref();
                let mut context = commands::sessions::SessionCommandContext {
                    project,
                    registry: &mut self.sessions,
                    events: &mut self.pending_session_events,
                };
                let output = commands::sessions::execute(command_name, command, &mut context);
                drop(context);
                output
            }
            "undo" => {
                let nodes_active =
                    self.is_game_project() && self.bottom_dock.active_tab_is("nodes");
                let available = if nodes_active {
                    self.nodes_history_cursor > 0
                } else if self.is_electronics_project() {
                    self.electronics_history.can_undo()
                } else {
                    self.scene_history.can_undo()
                };
                if nodes_active {
                    self.undo_nodes();
                } else if self.is_electronics_project() {
                    self.undo_electronics();
                } else {
                    self.undo_scene();
                }
                if available {
                    CommandOutput::info(
                        "Undo",
                        vec!["Active document restored.".to_string()],
                        serde_json::json!({ "ok": true }),
                    )
                } else {
                    CommandOutput::warning(
                        "Undo",
                        vec!["Nothing to undo.".to_string()],
                        serde_json::json!({ "ok": false }),
                    )
                }
            }
            "redo" => {
                let nodes_active =
                    self.is_game_project() && self.bottom_dock.active_tab_is("nodes");
                let available = if nodes_active {
                    self.nodes_history_cursor + 1 < self.nodes_history.len()
                } else if self.is_electronics_project() {
                    self.electronics_history.can_redo()
                } else {
                    self.scene_history.can_redo()
                };
                if nodes_active {
                    self.redo_nodes();
                } else if self.is_electronics_project() {
                    self.redo_electronics();
                } else {
                    self.redo_scene();
                }
                if available {
                    CommandOutput::info(
                        "Redo",
                        vec!["Active document restored.".to_string()],
                        serde_json::json!({ "ok": true }),
                    )
                } else {
                    CommandOutput::warning(
                        "Redo",
                        vec!["Nothing to redo.".to_string()],
                        serde_json::json!({ "ok": false }),
                    )
                }
            }
            "transaction.undo" => CommandOutput::warning(
                "Attached undo token",
                vec![
                    "Undo tokens are scoped to an attached CLI/MCP request. Send this command through the attached endpoint with confirm=true.".to_string(),
                ],
                serde_json::json!({"ok": false, "attached_only": true}),
            ),
            name if name.starts_with("script.") => {
                if !self.is_game_project() || !self.game_documents_ready {
                    return CommandOutput::warning(
                        "Script command unavailable",
                        vec!["Script commands require an active Game project.".to_string()],
                        serde_json::json!({ "ok": false, "command": command_name }),
                    );
                }
                let assets_root = self
                    .current_project
                    .as_ref()
                    .map(|project| project.path.join("assets"));
                let mut context = commands::script::ScriptCommandContext {
                    scene: &mut self.scene,
                    assets_root: assets_root.as_deref(),
                };
                commands::script::execute(command_name, command, &mut context)
            }
            _ => CommandOutput::error(
                "Shared command",
                format!("Unknown shared command: {command_name}"),
            ),
        }
    }

    fn process_session_events(&mut self) {
        for event in std::mem::take(&mut self.pending_session_events) {
            match event {
                commands::sessions::SessionCommandEvent::Activate(id) => {
                    if let Err(error) = self.activate_session(id) {
                        self.console
                            .log(crate::console::LogLevel::Error, error.clone());
                        self.last_action = error;
                    }
                }
            }
        }
    }

    fn activate_session(&mut self, session_id: raf_core::session::SessionId) -> Result<(), String> {
        let project = self
            .current_project
            .clone()
            .ok_or_else(|| "No active project.".to_string())?;
        if self.sessions.active_session == session_id {
            return Ok(());
        }
        if !self
            .sessions
            .sessions
            .iter()
            .any(|session| session.id == session_id)
        {
            return Err("Requested session does not exist.".to_string());
        }
        if project.project_type == ProjectType::Game
            && (!self.game_documents_ready || self.game_documents_receiver.is_some())
        {
            return Err("Game session documents are still loading.".to_string());
        }

        self.persist_active_project()?;
        let _ = self.flush_persistence();
        self.sessions.set_active(session_id);
        if project.project_type == ProjectType::Game {
            self.scene = SceneGraph::new();
            self.nodes_document = NodeEditorDocument::default();
            self.nodes_session_id = Some(session_id);
            self.camera_bookmarks = std::array::from_fn(|_| None);
            self.viewport
                .apply_editor_camera_block(&EditorCameraBlock::default());
            self.begin_game_documents_load(&project);
        } else {
            self.game_documents_generation = self.game_documents_generation.wrapping_add(1);
            self.game_documents_receiver = None;
            self.game_documents_ready = false;
            self.load_electronics_documents(&project)?;
        }
        self.reset_nodes_history();
        self.scene_history.clear();
        self.attached_undo.clear();
        self.electronics_history.clear();
        self.electronics_edit_snapshot = None;
        self.electronics_edit_changed = false;
        self.agent_electronics_snapshot = None;
        self.scene_selection = SceneSelectionState::default();
        self.viewport.clear_scene_edit_snapshot();
        self.viewport.set_selected_ids(Vec::new());
        self.hierarchy.reset_for_scene();
        self.inspector.reset_for_scene();
        self.bottom_dock.reset_nodes();
        self.bottom_dock.sync_nodes_selection(None);
        self.search_results_key = None;
        self.last_action = format!(
            "Active session: {}",
            self.sessions
                .active()
                .map(|session| session.name.as_str())
                .unwrap_or("Main")
        );
        self.console
            .log(crate::console::LogLevel::Info, self.last_action.clone());
        self.queue_session_registry_save(&project)?;
        self.update_attached_descriptor();
        Ok(())
    }

    fn queue_session_registry_save(&self, project: &Project) -> Result<(), String> {
        let path = project.path.join(ProjectSessionRegistry::FILE_NAME);
        let data = ron::ser::to_string_pretty(&self.sessions, ron::ser::PrettyConfig::default())
            .map_err(|error| error.to_string())?;
        self.queue_persistence("Session registry", vec![PersistenceWrite { path, data }])
    }

    fn show_game_viewport(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let Some(project) = self
            .current_project
            .clone()
            .filter(|project| project.project_type == ProjectType::Game)
        else {
            return;
        };
        if !self.game_documents_ready {
            return;
        }

        let viewport_rect = ui.available_rect_before_wrap();
        let toolbar_rect = viewport_toolbar_rect(viewport_rect);
        let toolbar_compact = toolbar_rect.width() < 396.0;
        let snapshot = self.prepare_graphics_surface(GraphicsSurfaceKind::SceneViewport);
        self.viewport.set_render_runtime(snapshot);
        self.configure_viewport(&project);
        let viewport_changed = self.viewport.show_canvas_only(
            ctx,
            ui,
            self.egui_wgpu_render_state.as_ref(),
            &mut self.graphics_runtime,
            &mut self.scene,
            self.settings.theme != Theme::Light,
        );
        if let Some(before) = self.viewport.take_completed_scene_edit_snapshot() {
            if viewport_changed {
                self.commit_scene_change(before, "Viewport transform changed");
            }
        }
        self.sync_selection_from_viewport();

        let toolbar_state = self.viewport.toolbar_state(toolbar_compact);
        let toolbar_palette = self.palette(ctx);
        let toolbar_language = self.settings.language;
        let toolbar_render_state = self.egui_wgpu_render_state.as_ref();
        let toolbar_actions = ui
            .allocate_new_ui(
                egui::UiBuilder::new().max_rect(toolbar_rect),
                |toolbar_ui| {
                    self.viewport_toolbar_surface.show(
                        toolbar_ui,
                        toolbar_render_state,
                        toolbar_palette,
                        toolbar_language,
                        toolbar_state,
                    )
                },
            )
            .inner;
        for action in toolbar_actions {
            self.apply_viewport_toolbar_action(action);
        }
    }

    fn apply_viewport_toolbar_action(&mut self, action: ViewportToolbarAction) {
        use raf_render::gizmo::GizmoMode;

        match action {
            ViewportToolbarAction::Select => self.viewport.set_select_mode_from_ui(),
            ViewportToolbarAction::Move => {
                self.viewport.set_gizmo_mode_from_ui(GizmoMode::Translate)
            }
            ViewportToolbarAction::Rotate => {
                self.viewport.set_gizmo_mode_from_ui(GizmoMode::Rotate)
            }
            ViewportToolbarAction::Scale => self.viewport.set_gizmo_mode_from_ui(GizmoMode::Scale),
            ViewportToolbarAction::Focus => {
                let selected = self
                    .scene_selection
                    .selected_nodes
                    .first()
                    .copied()
                    .or_else(|| self.viewport.selected.first().copied());
                self.viewport.focus_selected_entity(&self.scene, selected);
            }
            ViewportToolbarAction::Solid => {
                self.viewport.set_render_style_from_ui(RenderStyle::Solid);
                self.settings.viewport_render_mode = raf_core::config::ViewportRenderMode::Solid;
            }
            ViewportToolbarAction::Wireframe => {
                self.viewport
                    .set_render_style_from_ui(RenderStyle::Wireframe);
                self.settings.viewport_render_mode =
                    raf_core::config::ViewportRenderMode::Wireframe;
            }
            ViewportToolbarAction::Preview => {
                self.viewport.set_render_style_from_ui(RenderStyle::Preview);
                self.settings.viewport_render_mode = raf_core::config::ViewportRenderMode::Preview;
            }
            ViewportToolbarAction::ToggleGrid => {
                self.viewport.grid_visible = !self.viewport.grid_visible;
                self.settings.grid_visible = self.viewport.grid_visible;
            }
            ViewportToolbarAction::ToggleLabels => {
                self.viewport.show_labels = !self.viewport.show_labels;
                self.settings.show_viewport_labels = self.viewport.show_labels;
            }
            ViewportToolbarAction::View2d => {
                self.viewport.set_view_mode_from_ui(ViewportMode::View2D)
            }
            ViewportToolbarAction::View3d => {
                self.viewport.set_view_mode_from_ui(ViewportMode::View3D)
            }
            ViewportToolbarAction::ResetView => self.viewport.reset_view_from_ui(),
        }
    }

    fn sync_selection_from_viewport(&mut self) {
        if !self.is_game_project() || !self.game_documents_ready {
            return;
        }
        let ids = self.viewport.selected.clone();
        if ids == self.scene_selection.selected_nodes {
            return;
        }
        self.scene_selection.selected_node = ids.first().copied();
        self.scene_selection.selected_nodes = ids.clone();
        if self.settings.hierarchy_auto_reveal_selection {
            if let Some(id) = ids.first().copied() {
                self.hierarchy.reveal_node_with_options(
                    &self.scene,
                    id,
                    self.settings.hierarchy_expand_on_select,
                );
            }
        }
    }

    fn electronics_snapshot(&self) -> ElectronicsDocumentSnapshot {
        ElectronicsDocumentSnapshot {
            schematic: self.schematic_view.schematic.clone(),
            pcb: self.pcb_view.layout.clone(),
        }
    }

    fn is_electronics_project(&self) -> bool {
        self.current_project
            .as_ref()
            .is_some_and(|project| project.project_type == ProjectType::Electronics)
    }

    fn is_game_project(&self) -> bool {
        self.current_project
            .as_ref()
            .is_some_and(|project| project.project_type == ProjectType::Game)
    }

    fn record_electronics_change(
        &mut self,
        before: ElectronicsDocumentSnapshot,
        action: &str,
    ) -> bool {
        let current = self.electronics_snapshot();
        if !self.electronics_history.record(before, &current) {
            return false;
        }
        self.schematic_view.mark_document_changed();
        self.pcb_view.mark_document_changed();
        self.electronics_edit_snapshot = None;
        self.electronics_edit_changed = false;
        self.electronics_drc_report = None;
        self.electronics_simulation_results = None;
        self.last_action = action.to_string();
        self.save_electronics_if_linear();
        true
    }

    fn begin_electronics_edit_transaction(&mut self) {
        if self.electronics_edit_snapshot.is_none() {
            self.electronics_edit_snapshot = Some(self.electronics_snapshot());
        }
        self.electronics_edit_changed = true;
    }

    fn undo_electronics(&mut self) {
        if self.electronics_history.undo(
            &mut self.schematic_view.schematic,
            &mut self.pcb_view.layout,
        ) {
            self.schematic_view.mark_document_changed();
            self.pcb_view.mark_document_changed();
            self.electronics_edit_snapshot = None;
            self.electronics_edit_changed = false;
            self.electronics_drc_report = None;
            self.electronics_simulation_results = None;
            self.last_action = t("app.electronics_undo", self.settings.language);
            self.save_electronics_if_linear();
        }
    }

    fn redo_electronics(&mut self) {
        if self.electronics_history.redo(
            &mut self.schematic_view.schematic,
            &mut self.pcb_view.layout,
        ) {
            self.schematic_view.mark_document_changed();
            self.pcb_view.mark_document_changed();
            self.electronics_edit_snapshot = None;
            self.electronics_edit_changed = false;
            self.electronics_drc_report = None;
            self.electronics_simulation_results = None;
            self.last_action = t("app.electronics_redo", self.settings.language);
            self.save_electronics_if_linear();
        }
    }

    fn show_electronics_navigator(
        &mut self,
        ui: &mut egui::Ui,
        palette: StudioUiPalette,
        language: raf_core::config::Language,
    ) -> Vec<ElectronicsNavigatorAction> {
        match self.canvas_mode {
            CanvasMode::Schematic => self.electronics_navigator_surface.show_schematic(
                ui,
                self.egui_wgpu_render_state.as_ref(),
                palette,
                &self.schematic_view,
                language,
            ),
            CanvasMode::Game => self.electronics_navigator_surface.show_pcb(
                ui,
                self.egui_wgpu_render_state.as_ref(),
                palette,
                &self.pcb_view,
                language,
            ),
        }
    }

    fn show_electronics_inspector(
        &mut self,
        ui: &mut egui::Ui,
        palette: StudioUiPalette,
        language: raf_core::config::Language,
    ) -> Vec<ElectronicsInspectorAction> {
        match self.canvas_mode {
            CanvasMode::Schematic => self.electronics_inspector_surface.show_schematic(
                ui,
                self.egui_wgpu_render_state.as_ref(),
                palette,
                &self.schematic_view,
                &self.sessions,
                language,
            ),
            CanvasMode::Game => self.electronics_inspector_surface.show_pcb(
                ui,
                self.egui_wgpu_render_state.as_ref(),
                palette,
                &self.pcb_view,
                &self.sessions,
                language,
            ),
        }
    }

    fn apply_electronics_navigator_actions(&mut self, actions: Vec<ElectronicsNavigatorAction>) {
        for action in actions {
            match action {
                ElectronicsNavigatorAction::SchematicRoot => self.schematic_view.clear_selection(),
                ElectronicsNavigatorAction::SchematicComponent(index) => {
                    self.schematic_view.select_component(index);
                    if let Some(designator) = self.schematic_view.selected_designator() {
                        self.pcb_view.select_by_designator(&designator);
                    }
                }
                ElectronicsNavigatorAction::SchematicWire(index) => {
                    self.schematic_view.select_wire(index)
                }
                ElectronicsNavigatorAction::PlaceComponent(index) => {
                    self.schematic_view.begin_component_placement(index)
                }
                ElectronicsNavigatorAction::PcbRoot => self.pcb_view.clear_selection(),
                ElectronicsNavigatorAction::PcbComponent(index) => {
                    self.pcb_view.select_component(index);
                    if let Some(designator) = self.pcb_view.selected_designator() {
                        self.schematic_view.select_by_designator(&designator);
                    }
                }
                ElectronicsNavigatorAction::PcbTrace(index) => self.pcb_view.select_trace(index),
                ElectronicsNavigatorAction::PcbAirwire(index) => {
                    self.pcb_view.select_airwire(index)
                }
            }
        }
    }

    fn apply_electronics_toolbar_actions(
        &mut self,
        actions: Vec<ElectronicsToolbarAction>,
        canvas_width: f32,
        canvas_height: f32,
    ) -> bool {
        let mut changed = false;
        for action in actions {
            match action {
                ElectronicsToolbarAction::SwitchToSchematic => {
                    self.canvas_mode = CanvasMode::Schematic;
                }
                ElectronicsToolbarAction::SwitchToPcb => {
                    self.canvas_mode = CanvasMode::Game;
                }
                ElectronicsToolbarAction::SchematicSelect => {
                    self.schematic_view.set_select_tool_from_ui()
                }
                ElectronicsToolbarAction::SchematicWire => {
                    self.schematic_view.set_wire_tool_from_ui()
                }
                ElectronicsToolbarAction::SchematicRotate => {
                    self.schematic_view.rotate_placement_from_ui()
                }
                ElectronicsToolbarAction::SchematicFit => self
                    .schematic_view
                    .fit_view_from_ui(canvas_width, canvas_height),
                ElectronicsToolbarAction::SchematicLibrary => {
                    self.schematic_view.toggle_library_from_ui()
                }
                ElectronicsToolbarAction::SchematicTest => {
                    self.schematic_view.run_electrical_test_from_ui();
                    self.run_electronics_drc();
                }
                ElectronicsToolbarAction::SchematicDelete => {
                    changed |= self.schematic_view.delete_selection()
                }
                ElectronicsToolbarAction::SchematicZoomIn => self.schematic_view.zoom_in_from_ui(),
                ElectronicsToolbarAction::SchematicZoomOut => {
                    self.schematic_view.zoom_out_from_ui()
                }
                ElectronicsToolbarAction::PcbSelect => self.pcb_view.set_select_tool_from_ui(),
                ElectronicsToolbarAction::PcbRoute => self.pcb_view.set_route_tool_from_ui(),
                ElectronicsToolbarAction::PcbOutline => self.pcb_view.set_outline_tool_from_ui(),
                ElectronicsToolbarAction::PcbAirwires => self.pcb_view.toggle_airwires_from_ui(),
                ElectronicsToolbarAction::PcbFit => {
                    self.pcb_view.fit_view_from_ui(canvas_width, canvas_height)
                }
                ElectronicsToolbarAction::PcbNewOutline => {
                    self.pcb_view.clear_outline_draft_from_ui();
                    self.pcb_view.set_outline_tool_from_ui();
                }
                ElectronicsToolbarAction::PcbRouteSelected => {
                    changed |= self.pcb_view.route_selected_airwire_from_ui()
                }
                ElectronicsToolbarAction::PcbZoomIn => self.pcb_view.zoom_in_from_ui(),
                ElectronicsToolbarAction::PcbZoomOut => self.pcb_view.zoom_out_from_ui(),
            }
        }
        if changed {
            match self.canvas_mode {
                CanvasMode::Schematic => self.schematic_view.mark_document_changed(),
                CanvasMode::Game => self.pcb_view.mark_document_changed(),
            }
            self.last_action = t("app.electronics_document_changed", self.settings.language);
        }
        changed
    }

    fn apply_electronics_inspector_actions(&mut self, actions: Vec<ElectronicsInspectorAction>) {
        let mut document_action = false;
        for action in &actions {
            if matches!(
                action,
                ElectronicsInspectorAction::Text { .. }
                    | ElectronicsInspectorAction::Range { .. }
                    | ElectronicsInspectorAction::Toggle { .. }
                    | ElectronicsInspectorAction::Layer { .. }
            ) {
                document_action = true;
                break;
            }
        }
        let selection = match self.canvas_mode {
            CanvasMode::Schematic => self.schematic_view.selection(),
            CanvasMode::Game => return self.apply_pcb_inspector_actions(actions),
        };
        if document_action && self.electronics_edit_snapshot.is_none() {
            self.electronics_edit_snapshot = Some(self.electronics_snapshot());
        }
        let mut changed = false;
        for action in actions {
            match action {
                ElectronicsInspectorAction::SwitchTab(tab) => {
                    self.electronics_inspector_surface.set_tab(tab);
                }
                ElectronicsInspectorAction::SessionCreate { name } => {
                    self.execute_session_ui_command(
                        "session.create",
                        [
                            ("name".to_string(), name),
                            ("kind".to_string(), "electronics".to_string()),
                        ],
                    );
                }
                ElectronicsInspectorAction::SessionOpen(id) => {
                    self.execute_session_ui_command(
                        "session.open",
                        [("session".to_string(), id.0.to_string())],
                    );
                }
                ElectronicsInspectorAction::SessionDuplicate { source, name } => {
                    self.execute_session_ui_command(
                        "session.duplicate",
                        [
                            ("source".to_string(), source.0.to_string()),
                            ("name".to_string(), name),
                        ],
                    );
                }
                ElectronicsInspectorAction::SessionRemove(id) => {
                    self.execute_session_ui_command(
                        "session.remove",
                        [("session".to_string(), id.0.to_string())],
                    );
                }
                ElectronicsInspectorAction::Text { field, value } => match field.as_str() {
                    "electronics.schematic.reference" => {
                        if let crate::panels::schematic_view::SchematicSelection::Component(index) =
                            selection
                        {
                            if let Some(component) =
                                self.schematic_view.schematic.components.get_mut(index)
                            {
                                if component.designator != value {
                                    component.designator = value;
                                    changed = true;
                                }
                            }
                        }
                    }
                    "electronics.schematic.value" => {
                        if let crate::panels::schematic_view::SchematicSelection::Component(index) =
                            selection
                        {
                            if let Some(component) =
                                self.schematic_view.schematic.components.get_mut(index)
                            {
                                if component.value != value {
                                    component.value = value;
                                    component.sync_sim_model_from_value();
                                    changed = true;
                                }
                            }
                        }
                    }
                    "electronics.schematic.net" => {
                        if let crate::panels::schematic_view::SchematicSelection::Wire(index) =
                            selection
                        {
                            let indices = self.schematic_view.selected_wire_indices();
                            let targets = if indices.is_empty() {
                                vec![index]
                            } else {
                                indices
                            };
                            for wire_index in targets {
                                if let Some(wire) =
                                    self.schematic_view.schematic.wires.get_mut(wire_index)
                                {
                                    if wire.net != value {
                                        wire.net = value.clone();
                                        changed = true;
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                },
                ElectronicsInspectorAction::Range { field, value } => {
                    let previous_component = match selection {
                        crate::panels::schematic_view::SchematicSelection::Component(index) => {
                            self.schematic_view.schematic.components.get(index).cloned()
                        }
                        _ => None,
                    };
                    if let crate::panels::schematic_view::SchematicSelection::Component(index) =
                        selection
                    {
                        if let Some(component) =
                            self.schematic_view.schematic.components.get_mut(index)
                        {
                            match field.as_str() {
                                "electronics.schematic.position.x"
                                    if component.position.x != value =>
                                {
                                    component.position.x = value;
                                    changed = true;
                                }
                                "electronics.schematic.position.y"
                                    if component.position.y != value =>
                                {
                                    component.position.y = value;
                                    changed = true;
                                }
                                "electronics.schematic.rotation" if component.rotation != value => {
                                    component.rotation = value;
                                    changed = true;
                                }
                                _ => continue,
                            }
                        }
                    }
                    if let Some(previous_component) = previous_component {
                        self.schematic_view
                            .ensure_wire_anchors_for_component_snapshot(&previous_component);
                    }
                }
                ElectronicsInspectorAction::Toggle { field, value } => {
                    if let crate::panels::schematic_view::SchematicSelection::Component(index) =
                        selection
                    {
                        if let Some(component) =
                            self.schematic_view.schematic.components.get_mut(index)
                        {
                            match field.as_str() {
                                "electronics.schematic.visible" if component.visible != value => {
                                    component.visible = value;
                                    changed = true;
                                }
                                "electronics.schematic.locked" if component.locked != value => {
                                    component.locked = value;
                                    changed = true;
                                }
                                _ => continue,
                            }
                        }
                    }
                }
                ElectronicsInspectorAction::Layer { .. } => {}
            }
        }
        if changed {
            self.schematic_view.schematic.sync_wire_anchors();
            self.schematic_view.mark_document_changed();
            self.begin_electronics_edit_transaction();
            self.last_action = t("app.electronics_schematic_changed", self.settings.language);
        }
    }

    fn apply_pcb_inspector_actions(&mut self, actions: Vec<ElectronicsInspectorAction>) {
        let selection = self.pcb_view.selection();
        let document_action = actions.iter().any(|action| {
            matches!(
                action,
                ElectronicsInspectorAction::Text { .. }
                    | ElectronicsInspectorAction::Range { .. }
                    | ElectronicsInspectorAction::Toggle { .. }
                    | ElectronicsInspectorAction::Layer { .. }
            )
        });
        if document_action && self.electronics_edit_snapshot.is_none() {
            self.electronics_edit_snapshot = Some(self.electronics_snapshot());
        }
        let mut changed = false;
        for action in actions {
            match action {
                ElectronicsInspectorAction::SwitchTab(tab) => {
                    self.electronics_inspector_surface.set_tab(tab);
                }
                ElectronicsInspectorAction::SessionCreate { name } => {
                    self.execute_session_ui_command(
                        "session.create",
                        [
                            ("name".to_string(), name),
                            ("kind".to_string(), "electronics".to_string()),
                        ],
                    );
                }
                ElectronicsInspectorAction::SessionOpen(id) => {
                    self.execute_session_ui_command(
                        "session.open",
                        [("session".to_string(), id.0.to_string())],
                    );
                }
                ElectronicsInspectorAction::SessionDuplicate { source, name } => {
                    self.execute_session_ui_command(
                        "session.duplicate",
                        [
                            ("source".to_string(), source.0.to_string()),
                            ("name".to_string(), name),
                        ],
                    );
                }
                ElectronicsInspectorAction::SessionRemove(id) => {
                    self.execute_session_ui_command(
                        "session.remove",
                        [("session".to_string(), id.0.to_string())],
                    );
                }
                ElectronicsInspectorAction::Text { field, value } => {
                    if let crate::panels::pcb_view::PcbSelection::Component(index) = selection {
                        if let Some(component) = self.pcb_view.layout.components.get_mut(index) {
                            match field.as_str() {
                                "electronics.pcb.reference" if component.designator != value => {
                                    component.designator = value;
                                    changed = true;
                                }
                                "electronics.pcb.value" if component.value != value => {
                                    if component.value != value {
                                        component.value = value;
                                        changed = true;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                ElectronicsInspectorAction::Range { field, value } => match selection {
                    crate::panels::pcb_view::PcbSelection::Component(index) => {
                        if let Some(component) = self.pcb_view.layout.components.get_mut(index) {
                            match field.as_str() {
                                "electronics.pcb.position.x" if component.position.x != value => {
                                    component.position.x = value;
                                    changed = true;
                                }
                                "electronics.pcb.position.y" if component.position.y != value => {
                                    component.position.y = value;
                                    changed = true;
                                }
                                "electronics.pcb.rotation" if component.rotation != value => {
                                    component.rotation = value;
                                    changed = true;
                                }
                                _ => continue,
                            }
                        }
                    }
                    crate::panels::pcb_view::PcbSelection::Trace(index) => {
                        if field == "electronics.pcb.trace.width" {
                            if let Some(trace) = self.pcb_view.layout.traces.get_mut(index) {
                                if trace.width != value {
                                    trace.width = value;
                                    changed = true;
                                }
                            }
                        }
                    }
                    _ => {}
                },
                ElectronicsInspectorAction::Toggle { field, value } => {
                    if let crate::panels::pcb_view::PcbSelection::Component(index) = selection {
                        if field == "electronics.pcb.locked" {
                            if let Some(component) = self.pcb_view.layout.components.get_mut(index)
                            {
                                if component.locked != value {
                                    component.locked = value;
                                    changed = true;
                                }
                            }
                        }
                    }
                }
                ElectronicsInspectorAction::Layer { field, value } => match selection {
                    crate::panels::pcb_view::PcbSelection::Component(index)
                        if field == "electronics.pcb.layer" =>
                    {
                        if let Some(component) = self.pcb_view.layout.components.get_mut(index) {
                            if component.layer != value {
                                component.layer = value;
                                changed = true;
                            }
                        }
                    }
                    crate::panels::pcb_view::PcbSelection::Trace(index)
                        if field == "electronics.pcb.trace.layer" =>
                    {
                        if let Some(trace) = self.pcb_view.layout.traces.get_mut(index) {
                            if trace.layer != value {
                                trace.layer = value;
                                changed = true;
                            }
                        }
                    }
                    _ => {}
                },
            }
        }
        if changed {
            self.pcb_view.layout.rebuild_airwires();
            self.pcb_view.mark_document_changed();
            self.begin_electronics_edit_transaction();
            self.last_action = t("app.electronics_pcb_changed", self.settings.language);
        }
    }

    fn run_electronics_drc(&mut self) {
        if self
            .current_project
            .as_ref()
            .is_some_and(|project| project.project_type != ProjectType::Electronics)
        {
            return;
        }
        if self.electronics_analysis_job.is_some() {
            return;
        }
        let schematic = self.schematic_view.schematic.clone();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(ElectronicsAnalysisJobResult::Drc(
                raf_electronics::drc::run_drc(&schematic),
            ));
        });
        self.electronics_analysis_job = Some(receiver);
        self.last_action = t("app.electronics_running_drc", self.settings.language);
        self.bottom_dock.open_tab("drc");
    }

    fn run_electronics_simulation(&mut self) {
        if self
            .current_project
            .as_ref()
            .is_some_and(|project| project.project_type != ProjectType::Electronics)
        {
            return;
        }
        if self.electronics_analysis_job.is_some() {
            return;
        }
        let schematic = self.schematic_view.schematic.clone();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(ElectronicsAnalysisJobResult::Simulation(
                raf_electronics::simulation::simulate_dc(&schematic),
            ));
        });
        self.electronics_analysis_job = Some(receiver);
        self.last_action = t("app.electronics_running_simulation", self.settings.language);
        self.bottom_dock.open_tab("simulation");
    }

    fn poll_electronics_analysis(&mut self) {
        let Some(receiver) = self.electronics_analysis_job.as_ref() else {
            return;
        };
        match receiver.try_recv() {
            Ok(ElectronicsAnalysisJobResult::Drc(report)) => {
                let passed = report.passed();
                self.electronics_drc_report = Some(report);
                self.last_action = if passed {
                    t("app.drc_ok", self.settings.language)
                } else {
                    t("app.electronics_drc", self.settings.language)
                };
                self.electronics_analysis_job = None;
            }
            Ok(ElectronicsAnalysisJobResult::Simulation(results)) => {
                self.electronics_simulation_results = Some(results);
                self.last_action = t("app.electronics_simulation", self.settings.language);
                self.electronics_analysis_job = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.electronics_analysis_job = None;
                self.last_action = t("app.electronics_analysis_failed", self.settings.language);
            }
        }
    }

    fn save_electronics_if_linear(&mut self) {
        let Some(project) = self.current_project.as_ref() else {
            return;
        };
        if project.project_type != ProjectType::Electronics || !project.settings.linear_save {
            return;
        }
        let Some(session) = self.sessions.active() else {
            return;
        };
        let schematic_path = session.path(&project.path, &session.schematic_file);
        let pcb_path = session.path(&project.path, &session.pcb_file);
        let schematic_data = match ron::ser::to_string_pretty(
            &self.schematic_view.schematic,
            ron::ser::PrettyConfig::default(),
        ) {
            Ok(data) => data,
            Err(error) => {
                tracing::error!(%error, "linear schematic serialization failed");
                return;
            }
        };
        let pcb_data = match ron::ser::to_string_pretty(
            &self.pcb_view.layout,
            ron::ser::PrettyConfig::default(),
        ) {
            Ok(data) => data,
            Err(error) => {
                tracing::error!(%error, "linear pcb serialization failed");
                return;
            }
        };
        if let Err(error) = self.queue_persistence(
            "Electronics (linear)",
            vec![
                PersistenceWrite {
                    path: schematic_path,
                    data: schematic_data,
                },
                PersistenceWrite {
                    path: pcb_path,
                    data: pcb_data,
                },
            ],
        ) {
            tracing::error!(%error, "linear electronics save queue failed");
        }
    }

    fn show_electronics_viewport(&mut self, ui: &mut egui::Ui) {
        self.schematic_view
            .set_minimap_visible(self.settings.electronics_show_minimap);
        self.schematic_view
            .set_status_visible(self.settings.electronics_show_status);
        self.pcb_view
            .set_minimap_visible(self.settings.electronics_show_minimap);
        self.pcb_view
            .set_status_visible(self.settings.electronics_show_status);
        let canvas_rect = ui.available_rect_before_wrap();
        let canvas_width = canvas_rect.width();
        let canvas_height = canvas_rect.height();
        self.capture_electronics_edit_snapshot(ui.ctx(), canvas_rect);
        let palette = self.palette(ui.ctx());
        let language = self.settings.language;
        let mut changed = false;

        match self.canvas_mode {
            CanvasMode::Schematic => {
                let snapshot = self.prepare_graphics_surface(GraphicsSurfaceKind::SchematicCanvas);
                self.schematic_view.set_render_runtime(snapshot);
                changed |= self.schematic_view.show_canvas_only_without_toolbar(
                    ui,
                    self.egui_wgpu_render_state.as_ref(),
                    &mut self.graphics_runtime,
                );
            }
            CanvasMode::Game => {
                let snapshot = self.prepare_graphics_surface(GraphicsSurfaceKind::PcbCanvas);
                self.pcb_view.set_render_runtime(snapshot);
                changed |= self.pcb_view.show_canvas_only_without_toolbar(
                    ui,
                    self.egui_wgpu_render_state.as_ref(),
                    &mut self.graphics_runtime,
                );
            }
        }

        // The toolbar is a retained RafUI surface, but its placement is an
        // overlay owned by the viewport. This keeps the CAD document visible
        // below it and avoids reserving a permanent strip of canvas height.
        let toolbar_width = (canvas_width - 40.0).clamp(180.0, 620.0);
        let toolbar_rect = egui::Rect::from_min_size(
            canvas_rect.left_top() + egui::vec2(20.0, 16.0),
            egui::vec2(toolbar_width, 38.0),
        );
        // Electronics uses a floating toolbar: the controls carry their own
        // translucent surfaces, while the empty flex space stays invisible.
        // Painting the host rectangle here made the spacer look like a long
        // blue separator between the tool groups.
        let toolbar_fill = egui::Color32::TRANSPARENT;
        let toolbar_border = egui::Color32::TRANSPARENT;
        let toolbar_background = ui.painter_at(toolbar_rect);
        toolbar_background.rect_filled(toolbar_rect, 0.0, toolbar_fill);
        toolbar_background.rect_stroke(toolbar_rect, 0.0, egui::Stroke::new(1.0, toolbar_border));
        let toolbar_actions = ui
            .allocate_new_ui(
                egui::UiBuilder::new().max_rect(toolbar_rect.shrink2(egui::vec2(7.0, 4.0))),
                |toolbar_ui| match self.canvas_mode {
                    CanvasMode::Schematic => self.electronics_toolbar_surface.show_schematic(
                        toolbar_ui,
                        self.egui_wgpu_render_state.as_ref(),
                        palette,
                        &self.schematic_view,
                        language,
                        toolbar_width < 560.0,
                    ),
                    CanvasMode::Game => self.electronics_toolbar_surface.show_pcb(
                        toolbar_ui,
                        self.egui_wgpu_render_state.as_ref(),
                        palette,
                        &self.pcb_view,
                        language,
                        toolbar_width < 560.0,
                    ),
                },
            )
            .inner;
        if self.electronics_edit_snapshot.is_none()
            && toolbar_actions.iter().any(|action| {
                matches!(
                    action,
                    ElectronicsToolbarAction::SchematicDelete
                        | ElectronicsToolbarAction::PcbRouteSelected
                )
            })
        {
            self.electronics_edit_snapshot = Some(self.electronics_snapshot());
        }
        changed |=
            self.apply_electronics_toolbar_actions(toolbar_actions, canvas_width, canvas_height);
        self.sync_electronics_selection();
        if changed {
            self.electronics_edit_changed = true;
        }
        let pointer_down = ui.input(|input| {
            input.pointer.primary_down()
                || input.pointer.secondary_down()
                || input.pointer.middle_down()
        });
        if self.electronics_edit_changed && !pointer_down {
            let before = self
                .electronics_edit_snapshot
                .take()
                .unwrap_or_else(|| self.electronics_snapshot());
            self.electronics_edit_changed = false;
            self.record_electronics_change(
                before,
                &t("app.electronics_document_changed", self.settings.language),
            );
        } else if !pointer_down && !changed {
            self.electronics_edit_snapshot = None;
        }
    }

    /// Takes the undo baseline only when a canvas gesture or canvas keyboard
    /// command can mutate the document. Cloning both CAD documents every idle
    /// frame made opening Electronics scale with the whole project size.
    fn capture_electronics_edit_snapshot(&mut self, ctx: &egui::Context, rect: egui::Rect) {
        if self.electronics_edit_snapshot.is_some() {
            return;
        }
        let should_capture = ctx.input(|input| {
            let pointer_event_inside = input.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::PointerButton {
                        pos,
                        ..
                    } if rect.contains(*pos)
                )
            });
            let canvas_has_keyboard_activity = rect.contains(
                input
                    .pointer
                    .hover_pos()
                    .unwrap_or(egui::Pos2::new(f32::NAN, f32::NAN)),
            ) && input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Key { pressed: true, .. }));
            pointer_event_inside || canvas_has_keyboard_activity
        });
        if should_capture {
            self.electronics_edit_snapshot = Some(self.electronics_snapshot());
        }
    }

    fn sync_electronics_selection(&mut self) {
        match self.canvas_mode {
            CanvasMode::Schematic => {
                if let Some(designator) = self.schematic_view.selected_designator() {
                    self.pcb_view.select_by_designator(&designator);
                }
            }
            CanvasMode::Game => {
                if let Some(designator) = self.pcb_view.selected_designator() {
                    self.schematic_view.select_by_designator(&designator);
                }
            }
        }
    }

    fn configure_viewport(&mut self, project: &Project) {
        self.viewport.frame_time_hint = 1.0 / 60.0;
        self.viewport.render_cfg = self.project_render_config(project);
        self.viewport.world_stream_config =
            WorldStreamConfig::from_project_settings(&project.settings);
        self.viewport.grid_visible = self.settings.grid_visible;
        self.viewport.grid_spacing = self.settings.grid_size.max(0.1);
        self.viewport.grid_load_distance = self.settings.grid_load_distance.max(0.0);
        self.viewport.fps_limit = self.settings.fps_limit;
        self.viewport.invert_mouse_x = self.settings.invert_mouse_x;
        self.viewport.invert_mouse_y = self.settings.invert_mouse_y;
        self.viewport.invert_ws = self.settings.invert_ws;
        self.viewport.focus_lock_enabled = self.settings.focus_lock_enabled;
        self.viewport.wasd_speed = self.settings.wasd_speed;
        self.viewport.move_sensitivity = self.settings.move_gizmo_sensitivity;
        self.viewport.rotate_sensitivity = self.settings.rotate_gizmo_sensitivity;
        self.viewport.scale_sensitivity = self.settings.scale_gizmo_sensitivity;
        self.viewport.uniform_scale_by_default = self.settings.uniform_scale_by_default;
        self.viewport.gizmo_growth_scale = self.settings.gizmo_growth_scale;
        self.viewport.solid_show_surface_edges = self.settings.solid_show_surface_edges;
        self.viewport.solid_xray_mode = self.settings.solid_xray_mode;
        self.viewport.solid_face_tonality = self.settings.solid_face_tonality;
        self.viewport.show_labels = self.settings.show_viewport_labels;
        self.viewport.render_style = match self.settings.viewport_render_mode {
            raf_core::config::ViewportRenderMode::Solid => RenderStyle::Solid,
            raf_core::config::ViewportRenderMode::Wireframe => RenderStyle::Wireframe,
            raf_core::config::ViewportRenderMode::Preview => RenderStyle::Preview,
        };
    }

    fn prepare_graphics_surface(&mut self, surface: GraphicsSurfaceKind) -> RenderRuntimeSnapshot {
        let allow_advanced_gpu_features = self
            .current_project
            .as_ref()
            .is_some_and(|project| project.settings.allow_gpu_features);
        self.graphics_runtime.configure(
            self.settings.render_execution_policy,
            allow_advanced_gpu_features,
        );
        self.graphics_runtime.activate_surface(surface);
        self.graphics_runtime.snapshot()
    }

    fn project_render_config(&self, project: &Project) -> RenderConfig {
        let mut config = RenderConfig::for_preset(project.settings.runtime_render_preset);
        config.apply_execution_policy(self.settings.render_execution_policy);
        config.apply_project_gpu_gate(project.settings.allow_gpu_features);
        config.depth_accurate = project.settings.depth_accurate;
        config.depth_resolution_scale = project.settings.depth_resolution_scale.clamp(0.35, 1.0);
        config
    }

    fn open_project(&mut self, path: &Path) {
        self.project_open_generation = self.project_open_generation.wrapping_add(1);
        self.project_open_receiver = None;
        let generation = self.project_open_generation;
        let path = path.to_path_buf();
        let (sender, receiver) = mpsc::channel();
        self.project_open_receiver = Some(receiver);
        self.last_action = format!("Loading project: {}", path.display());
        let _ = std::thread::Builder::new()
            .name("raf-project-open".to_string())
            .spawn(move || {
                let project = Project::load(&path).map_err(|error| error.to_string());
                let sessions = project.as_ref().ok().map(|project| {
                    ProjectSessionRegistry::load_or_legacy(&project.path, project.project_type)
                });
                let _ = sender.send(ProjectOpenLoadResult {
                    generation,
                    project,
                    sessions,
                });
            });
    }

    fn open_loaded_project(&mut self, project: Project) {
        let sessions = ProjectSessionRegistry::load_or_legacy(&project.path, project.project_type);
        self.open_loaded_project_with_sessions(project, sessions);
    }

    fn open_loaded_project_with_sessions(
        &mut self,
        project: Project,
        sessions: ProjectSessionRegistry,
    ) {
        self.game_documents_generation = self.game_documents_generation.wrapping_add(1);
        self.game_documents_receiver = None;
        self.game_documents_ready = false;
        self.sessions = sessions;
        if project.project_type == ProjectType::Electronics {
            if let Err(error) = self.load_electronics_documents(&project) {
                tracing::error!(%error, "project documents failed to load");
                return;
            }
        } else {
            self.scene = SceneGraph::new();
            self.nodes_document = NodeEditorDocument::default();
            self.nodes_session_id = self.sessions.active().map(|session| session.id);
            self.camera_bookmarks = std::array::from_fn(|_| None);
            self.viewport
                .apply_editor_camera_block(&EditorCameraBlock::default());
        }
        self.reset_nodes_history();
        self.nodes_clipboard = None;
        self.recent_projects.add(&project);
        let _ = self.recent_projects.save(&dirs_config_dir());
        self.current_project = Some(project.clone());
        self.attached_idempotency.clear();
        self.attached_undo.clear();
        self.screen = AppScreen::Editor;
        self.console = ConsolePanel::default();
        self.console.log(
            crate::console::LogLevel::Info,
            format!("Project loaded: {}", project.name),
        );
        if let Some(session) = self.sessions.active() {
            self.console.log(
                crate::console::LogLevel::Info,
                format!("Session loaded: {}", session.name),
            );
        }
        if project.project_type == ProjectType::Game {
            self.console.log(
                crate::console::LogLevel::Info,
                "Game session documents scheduled on worker",
            );
        }
        self.scene_selection = SceneSelectionState::default();
        self.scene_history.clear();
        self.scene_revision = self.scene_revision.wrapping_add(1);
        self.attached_host.update_revision(self.scene_revision);
        self.agent_scene_snapshot = None;
        self.agent_electronics_snapshot = None;
        self.search_results_key = None;
        self.last_dropped_files.clear();
        self.electronics_history.clear();
        self.electronics_drc_report = None;
        self.electronics_simulation_results = None;
        self.electronics_analysis_job = None;
        self.viewport.clear_scene_edit_snapshot();
        self.hierarchy.reset_for_scene();
        self.inspector.reset_for_scene();
        self.bottom_dock.reset_nodes();
        self.bottom_dock.sync_nodes_selection(None);
        self.bottom_dock.mark_nodes_changed();
        self.hierarchy_open = project.settings.show_hierarchy_panel;
        self.inspector_open = project.settings.show_properties_panel;
        self.last_action = format!("Project loaded: {}", project.name);
        self.update_attached_descriptor();
        self.canvas_mode = if project.project_type == ProjectType::Game {
            CanvasMode::Game
        } else {
            CanvasMode::Schematic
        };
        if project.project_type == ProjectType::Game {
            self.begin_game_documents_load(&project);
            self.console.log(
                crate::console::LogLevel::Info,
                "Loading Game session documents in worker",
            );
        }
    }

    fn load_electronics_documents(&mut self, project: &Project) -> Result<(), String> {
        let session = self
            .sessions
            .active()
            .cloned()
            .ok_or_else(|| "project has no active session".to_string())?;
        if project.project_type != ProjectType::Electronics {
            return Err(
                "electronics documents requested for a non-Electronics project".to_string(),
            );
        }
        self.nodes_session_id = None;
        let schematic_path = session.path(&project.path, &session.schematic_file);
        let pcb_path = session.path(&project.path, &session.pcb_file);
        let schematic = schematic_path
            .exists()
            .then(|| load_schematic_document(&schematic_path))
            .flatten()
            .unwrap_or_else(|| Schematic::new(&project.name));
        self.schematic_view.set_schematic(schematic);
        let pcb_layout = pcb_path
            .exists()
            .then(|| load_pcb_document(&pcb_path))
            .flatten()
            .unwrap_or_else(|| PcbLayout::new(&project.name));
        self.pcb_view.set_layout(pcb_layout);
        self.pcb_view
            .sync_from_schematic(&self.schematic_view.schematic);
        Ok(())
    }

    fn duplicate_project(&mut self, path: &Path) {
        let Ok(project) = Project::load(path) else {
            return;
        };
        let Some(parent) = project.path.parent() else {
            return;
        };
        let _ = Project::create(
            &format!("{} Copy", project.name),
            project.project_type,
            parent,
        );
    }

    fn expand_window(&self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Transparent(false));
        // RafUI owns the application bar; keep the OS title bar disabled so
        // the editor does not render two independent window chrome layers.
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(
            800.0, 500.0,
        )));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1280.0, 720.0)));
    }
}

fn primitive_collider_points(primitive: Primitive) -> Vec<glam::Vec3> {
    match primitive {
        Primitive::Plane => vec![
            glam::Vec3::new(-0.5, -0.05, -0.5),
            glam::Vec3::new(0.5, 0.05, 0.5),
        ],
        _ => vec![glam::Vec3::splat(-0.5), glam::Vec3::splat(0.5)],
    }
}

fn sanitize_script_name(value: &str) -> String {
    let mut name = value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
        .take(64)
        .collect::<String>();
    if name.is_empty() {
        name = "new_script".to_string();
    }
    name
}

fn attached_error(
    id: raf_core::CommandId,
    revision: u64,
    message: impl Into<String>,
) -> EngineCommandResponse {
    let mut response = EngineCommandResponse::error(id, "Attached command rejected", message);
    response.revision = revision;
    response.verification = Some(VerificationSummary {
        status: "blocked".to_string(),
        checks: Vec::new(),
        failures: vec!["command_not_applied".to_string()],
    });
    response
}

fn attached_args(params: &Value) -> Result<std::collections::BTreeMap<String, String>, String> {
    match params {
        Value::Null => Ok(std::collections::BTreeMap::new()),
        Value::Object(object) => Ok(object
            .iter()
            .map(|(key, value)| {
                let value = match value {
                    Value::String(value) => value.clone(),
                    Value::Null => String::new(),
                    Value::Bool(value) => value.to_string(),
                    Value::Number(value) => value.to_string(),
                    Value::Array(_) | Value::Object(_) => value.to_string(),
                };
                (key.clone(), value)
            })
            .collect()),
        _ => Err("params must be a JSON object.".to_string()),
    }
}

fn parse_undo_token(params: &Value) -> Result<UndoToken, String> {
    let raw = params
        .get("token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "transaction.undo requires a non-empty token parameter.".to_string())?;
    uuid::Uuid::parse_str(raw)
        .map(UndoToken)
        .map_err(|_| "transaction.undo received an invalid UUID token.".to_string())
}

fn scene_graph_diff(before: &SceneGraph, after: &SceneGraph, action: &str) -> Value {
    let before_nodes = before
        .iter()
        .map(|(_, node)| (node.uuid, node))
        .collect::<HashMap<_, _>>();
    let after_nodes = after
        .iter()
        .map(|(_, node)| (node.uuid, node))
        .collect::<HashMap<_, _>>();
    let created = after_nodes
        .iter()
        .filter(|(uuid, _)| !before_nodes.contains_key(uuid))
        .map(|(uuid, node)| serde_json::json!({"id": uuid, "name": node.name}))
        .collect::<Vec<_>>();
    let deleted = before_nodes
        .iter()
        .filter(|(uuid, _)| !after_nodes.contains_key(uuid))
        .map(|(uuid, node)| serde_json::json!({"id": uuid, "name": node.name}))
        .collect::<Vec<_>>();
    let modified = after_nodes
        .iter()
        .filter_map(|(uuid, node)| {
            let previous = before_nodes.get(uuid)?;
            (format!("{previous:?}") != format!("{node:?}"))
                .then(|| serde_json::json!({"id": uuid, "name": node.name}))
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "operation": "scene_change",
        "action": action,
        "before_nodes": before.len(),
        "after_nodes": after.len(),
        "created": created,
        "modified": modified,
        "deleted": deleted,
    })
}

fn same_project_path(left: Option<&Path>, right: Option<&Path>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => {
            left == right || left.canonicalize().ok() == right.canonicalize().ok()
        }
        _ => false,
    }
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
            if matches!(name.as_str(), ".git" | "target" | ".codex" | ".aura_rafi") {
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

impl eframe::App for AuraRafiApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [8.0 / 255.0, 11.0 / 255.0, 15.0 / 255.0, 1.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_attached_commands();
        self.frame_count = self.frame_count.saturating_add(1);
        match self.screen.clone() {
            AppScreen::Loading { start_time } => self.show_loading(ctx, start_time),
            AppScreen::ProjectHub => self.show_hub(ctx),
            AppScreen::NewProject {
                name,
                path,
                project_type,
            } => self.show_new_project(ctx, name, path, project_type),
            AppScreen::Settings => self.show_settings(ctx),
            AppScreen::Editor => {
                self.show_editor(ctx);
                let keyboard_captured = ctx.data(|data| {
                    data.get_temp::<bool>(egui::Id::new(raf_ui::KEYBOARD_CAPTURE_TEMP_ID))
                        .unwrap_or(false)
                });
                if !keyboard_captured {
                    let camera_shortcuts = ctx.input(|input| {
                        (
                            input.modifiers.command,
                            input.key_pressed(egui::Key::Num1),
                            input.key_pressed(egui::Key::Num2),
                            input.key_pressed(egui::Key::Num3),
                        )
                    });
                    let slot = if camera_shortcuts.1 {
                        Some(0)
                    } else if camera_shortcuts.2 {
                        Some(1)
                    } else if camera_shortcuts.3 {
                        Some(2)
                    } else {
                        None
                    };
                    if let Some(slot) = slot {
                        if camera_shortcuts.0 {
                            self.save_camera_bookmark(slot);
                        } else {
                            self.restore_camera_bookmark(slot);
                        }
                    }
                    let electronics_project = self.is_electronics_project();
                    let nodes_active =
                        self.is_game_project() && self.bottom_dock.active_tab_is("nodes");
                    ctx.input(|input| {
                        if input.modifiers.command && input.key_pressed(egui::Key::Z) {
                            if input.modifiers.shift {
                                if nodes_active {
                                    self.redo_nodes();
                                } else if electronics_project {
                                    self.redo_electronics();
                                } else {
                                    self.redo_scene();
                                }
                            } else {
                                if nodes_active {
                                    self.undo_nodes();
                                } else if electronics_project {
                                    self.undo_electronics();
                                } else {
                                    self.undo_scene();
                                }
                            }
                        } else if input.modifiers.command && input.key_pressed(egui::Key::Y) {
                            if nodes_active {
                                self.redo_nodes();
                            } else if electronics_project {
                                self.redo_electronics();
                            } else {
                                self.redo_scene();
                            }
                        }
                    });
                }
            }
        }
    }
}

fn dirs_config_dir() -> PathBuf {
    let dir = dirs_next::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("AuraRafi");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn default_projects_dir() -> String {
    dirs_next::document_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("AuraRafi Projects")
        .display()
        .to_string()
}

#[cfg(test)]
mod attached_protocol_tests {
    use super::*;

    #[test]
    fn undo_token_parser_accepts_uuid_string_only() {
        let token = UndoToken::new();
        let parsed = parse_undo_token(&serde_json::json!({
            "token": token.0.to_string()
        }))
        .unwrap();
        assert_eq!(parsed, token);
        assert!(parse_undo_token(&serde_json::json!({"token": "not-a-uuid"})).is_err());
        assert!(parse_undo_token(&serde_json::json!({})).is_err());
    }
}
