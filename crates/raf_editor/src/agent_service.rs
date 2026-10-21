//! Project-scoped Agent supervision, independent of the visible native surface.

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use raf_ai::agent_history::AgentHistory;
use raf_ai::agent_runtime::{AgentToolKind, AgentToolResult, ToolExecutionMode, ToolExecutor};
use raf_core::project_store::{confined_path, read_recoverable, write_atomic, ProjectWriteLease};
use raf_core::session::{ProjectSession, ProjectSessionRegistry, SessionId};
use raf_core::{EngineSettings, Project, ProjectType, SceneGraph, TransactionLedger};
use raf_nodes::NodeGraph;
use raf_render::bridge::RenderRuntime;

use crate::agent_asset_tools::AgentAssetTools;
use crate::agent_context::AgentObservationContext;
use crate::agent_executor::{AgentEditorAction, AgentProjectContext, AgentToolExecutor};
use crate::agent_project_store::{self, DocumentSave, ProjectSaveQueue};
use crate::commands::catalog::CommandCatalog;
use crate::commands::game::{GameViewportPort, HeadlessGameViewportPort, SceneSelectionState};
use crate::electronics_controller::NativeElectronicsEditor;
use crate::panels::ai_chat::{AgentDecision, AgentPanel, AgentReadiness, SharedAgentPanel};
use crate::scene_history::SceneHistory;

pub(crate) type SharedAssetTools = Rc<RefCell<AgentAssetTools>>;
pub(crate) type SharedSave = Rc<RefCell<ProjectSaveQueue>>;

#[cfg(test)]
thread_local! { static OBSERVATION_BUILDS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) }; }

pub(crate) struct ProjectDocumentHost {
    pub session: ProjectSession,
    pub scene: SceneGraph,
    pub nodes: NodeGraph,
    pub electronics: Option<NativeElectronicsEditor>,
    pub history: SceneHistory,
    pub viewport: HeadlessGameViewportPort,
    pub save: SharedSave,
}

pub(crate) struct AgentWorkspace {
    pub project: Project,
    pub registry: ProjectSessionRegistry,
    pub panel: SharedAgentPanel,
    pub assets: SharedAssetTools,
    pub documents: BTreeMap<String, ProjectDocumentHost>,
    pub ledger: TransactionLedger,
    pub closing: bool,
    close_error: Option<String>,
    catalog: crate::project_catalog::ProjectCatalog,
    _lease: ProjectWriteLease,
}

struct LoadedWorkspace {
    project: Project,
    registry: ProjectSessionRegistry,
    documents: Vec<LoadedDocument>,
    history: AgentHistory,
    lease: ProjectWriteLease,
}

struct LoadedDocument {
    session: ProjectSession,
    scene: SceneGraph,
    nodes: NodeGraph,
    electronics: Option<(raf_electronics::Schematic, raf_electronics::PcbLayout)>,
    files: BTreeMap<PathBuf, Option<Vec<u8>>>,
}

enum WorkspaceSlot {
    Queued,
    Loading {
        receiver: Receiver<Result<LoadedWorkspace, String>>,
        cancelled: bool,
    },
    Ready(AgentWorkspace),
    Failed(String),
}

#[derive(Default)]
pub(crate) struct AgentService {
    entries: BTreeMap<PathBuf, WorkspaceSlot>,
    aliases: BTreeMap<PathBuf, PathBuf>,
    pub selected: Option<PathBuf>,
    pub revision: u64,
    last_poll: Option<Instant>,
    open_queue: VecDeque<PathBuf>,
    command_catalog: Option<CommandCatalog>,
}

impl AgentService {
    pub fn projects(&self) -> Vec<&Project> {
        self.entries
            .values()
            .filter_map(|slot| match slot {
                WorkspaceSlot::Ready(w) => Some(&w.project),
                _ => None,
            })
            .collect()
    }
    pub fn request(&mut self, path: PathBuf) {
        let path = self.enqueue(path);
        self.selected = Some(path.clone());
        self.start_next_load();
    }

    fn enqueue(&mut self, requested: PathBuf) -> PathBuf {
        let path = self.aliases.get(&requested).cloned().unwrap_or_else(|| {
            requested
                .canonicalize()
                .unwrap_or_else(|_| requested.clone())
        });
        self.aliases.insert(requested, path.clone());
        if self.entries.contains_key(&path) {
            return path;
        }
        self.entries.insert(path.clone(), WorkspaceSlot::Queued);
        self.open_queue.push_back(path.clone());
        self.revision = self.revision.wrapping_add(1);
        path
    }

    /// Load authoring hosts, never submit prompts or start gameplay.
    pub fn start_all(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        for path in paths {
            self.enqueue(path);
        }
        self.start_next_load();
    }

    fn start_next_load(&mut self) {
        if self
            .entries
            .values()
            .any(|slot| matches!(slot, WorkspaceSlot::Loading { .. }))
        {
            return;
        }
        let Some(path) = self.open_queue.pop_front() else {
            return;
        };
        let (sender, receiver) = mpsc::channel();
        self.entries.insert(
            path.clone(),
            WorkspaceSlot::Loading {
                receiver,
                cancelled: false,
            },
        );
        self.revision = self.revision.wrapping_add(1);
        let root = path.clone();
        if let Err(error) = std::thread::Builder::new()
            .name("raf-agent-project-open".into())
            .spawn(move || {
                let _ = sender.send(load_workspace(&root));
            })
        {
            self.entries
                .insert(path, WorkspaceSlot::Failed(error.to_string()));
        }
    }

    pub fn is_open(&self, path: &Path) -> bool {
        self.workspace(path).is_some()
    }

    pub fn is_loading(&self, path: &Path) -> bool {
        let path = self.aliases.get(path).map(PathBuf::as_path).unwrap_or(path);
        matches!(
            self.entries.get(path),
            Some(
                WorkspaceSlot::Queued
                    | WorkspaceSlot::Loading {
                        cancelled: false,
                        ..
                    }
            )
        )
    }

    pub fn is_closing(&self, path: &Path) -> bool {
        let path = self.aliases.get(path).map(PathBuf::as_path).unwrap_or(path);
        matches!(self.entries.get(path), Some(WorkspaceSlot::Ready(w)) if w.closing)
            || matches!(
                self.entries.get(path),
                Some(WorkspaceSlot::Loading {
                    cancelled: true,
                    ..
                })
            )
    }

    pub fn has_projects(&self) -> bool {
        !self.entries.is_empty()
    }

    pub fn close_project(&mut self, path: &Path) {
        let path = self
            .aliases
            .get(path)
            .map(PathBuf::as_path)
            .unwrap_or(path)
            .to_path_buf();
        match self.entries.get_mut(&path) {
            Some(WorkspaceSlot::Ready(workspace)) => {
                workspace.panel.borrow_mut().cancel_all();
                workspace.assets.borrow_mut().cancel_abandoned(&[]);
                workspace.catalog.sync_project(None);
                workspace.closing = true;
            }
            Some(WorkspaceSlot::Loading { cancelled, .. }) => *cancelled = true,
            _ => {
                self.entries.remove(&path);
            }
        }
        self.open_queue.retain(|queued| queued != &path);
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn close_all(&mut self) {
        for path in self.entries.keys().cloned().collect::<Vec<_>>() {
            self.close_project(&path);
        }
    }

    /// Failed saves keep the host and lease alive; the user can retry safely.
    fn poll_closing(&mut self) -> bool {
        let mut finished = Vec::new();
        let mut changed = false;
        for (path, slot) in &mut self.entries {
            let WorkspaceSlot::Ready(workspace) = slot else {
                continue;
            };
            if !workspace.closing || workspace.close_error.is_some() {
                continue;
            }
            let mut pending = false;
            for document in workspace.documents.values() {
                let mut save = document.save.borrow_mut();
                changed |= save.poll();
                pending |= save.is_pending();
            }
            if !pending {
                match workspace.panel.borrow_mut().finish_history_writes() {
                    Ok(true) => finished.push(path.clone()),
                    Ok(false) => {}
                    Err(error) => {
                        workspace.close_error = Some(error);
                        changed = true;
                    }
                }
            }
        }
        for path in &finished {
            self.entries.remove(path);
        }
        changed |= !finished.is_empty();
        if changed {
            self.revision = self.revision.wrapping_add(1);
        }
        changed
    }

    pub fn selected_close_error(&self) -> Option<String> {
        let workspace = self.selected_workspace().filter(|w| w.closing)?;
        workspace.close_error.clone().or_else(|| {
            workspace
                .documents
                .values()
                .find_map(|d| d.save.borrow().error.clone())
        })
    }

    pub fn retry_open(&mut self) {
        if let Some(path) = self.selected.clone() {
            if let Some(workspace) = self.workspace_mut(&path).filter(|w| w.closing) {
                if workspace.close_error.take().is_some() {
                    workspace.panel.borrow_mut().retry_history_writes();
                }
                for document in workspace.documents.values() {
                    document.save.borrow_mut().retry();
                }
                self.revision = self.revision.wrapping_add(1);
                return;
            }
            if matches!(self.entries.get(&path), Some(WorkspaceSlot::Failed(_))) {
                self.entries.remove(&path);
                self.request(path);
            }
        }
    }

    pub fn workspace(&self, path: &Path) -> Option<&AgentWorkspace> {
        let path = self.aliases.get(path).map(PathBuf::as_path).unwrap_or(path);
        match self.entries.get(path) {
            Some(WorkspaceSlot::Ready(workspace)) => Some(workspace),
            _ => None,
        }
    }

    pub fn workspace_mut(&mut self, path: &Path) -> Option<&mut AgentWorkspace> {
        let path = self.aliases.get(path).map(PathBuf::as_path).unwrap_or(path);
        match self.entries.get_mut(path) {
            Some(WorkspaceSlot::Ready(workspace)) => Some(workspace),
            _ => None,
        }
    }

    pub fn selected_workspace(&self) -> Option<&AgentWorkspace> {
        self.workspace(self.selected.as_deref()?)
    }
    pub fn is_selected(&self, path: &Path) -> bool {
        let path = self.aliases.get(path).map(PathBuf::as_path).unwrap_or(path);
        self.selected.as_deref() == Some(path)
    }
    pub fn selected_workspace_mut(&mut self) -> Option<&mut AgentWorkspace> {
        let path = self.selected.clone()?;
        self.workspace_mut(&path)
    }

    pub fn loading(&self) -> bool {
        self.selected
            .as_deref()
            .is_some_and(|path| self.is_loading(path))
    }

    pub fn error(&self) -> Option<&str> {
        match self.entries.get(self.selected.as_ref()?) {
            Some(WorkspaceSlot::Failed(error)) => Some(error),
            _ => None,
        }
    }

    pub fn poll_loads(&mut self, settings: &EngineSettings) -> bool {
        let completed: Vec<_> = self
            .entries
            .iter()
            .filter_map(|(path, slot)| {
                let WorkspaceSlot::Loading {
                    receiver,
                    cancelled,
                } = slot
                else {
                    return None;
                };
                match receiver.try_recv() {
                    Ok(result) => Some((path.clone(), *cancelled, result)),
                    Err(mpsc::TryRecvError::Disconnected) => Some((
                        path.clone(),
                        *cancelled,
                        Err("agent_workspace.load_failed".into()),
                    )),
                    Err(mpsc::TryRecvError::Empty) => None,
                }
            })
            .collect();
        let changed = !completed.is_empty();
        for (path, cancelled, result) in completed {
            if cancelled {
                self.entries.remove(&path);
                self.revision = self.revision.wrapping_add(1);
                continue;
            }
            let slot = match result {
                Err(error) => WorkspaceSlot::Failed(error),
                Ok(loaded) => {
                    let mut panel = AgentPanel::default();
                    panel.language = settings.language;
                    panel.initialize(&loaded.project, loaded.history);
                    let scene_fingerprint = loaded
                        .documents
                        .iter()
                        .find(|d| d.session.id == loaded.registry.active_session)
                        .map(|d| crate::scene_history::scene_fingerprint(&d.scene))
                        .unwrap_or_default();
                    let mut catalog = crate::project_catalog::ProjectCatalog::default();
                    catalog.sync_project(Some(&loaded.project.path));
                    let documents = loaded
                        .documents
                        .into_iter()
                        .map(|document| {
                            let electronics = document.electronics.map(|(schematic, pcb)| {
                                let mut editor = NativeElectronicsEditor::from_documents(
                                    &loaded.project,
                                    schematic,
                                    pcb,
                                );
                                editor.apply_engine_settings(settings);
                                editor.apply_project_settings(&loaded.project.settings);
                                editor
                            });
                            let save = ProjectSaveQueue::with_files(document.files);
                            (
                                document.session.id.0.to_string(),
                                ProjectDocumentHost {
                                    session: document.session,
                                    scene: document.scene,
                                    nodes: document.nodes,
                                    electronics,
                                    history: SceneHistory::default(),
                                    viewport: HeadlessGameViewportPort::default(),
                                    save: Rc::new(RefCell::new(save)),
                                },
                            )
                        })
                        .collect();
                    WorkspaceSlot::Ready(AgentWorkspace {
                        ledger: TransactionLedger::load_for_project(
                            &loaded.project.path,
                            scene_fingerprint,
                        ),
                        project: loaded.project,
                        registry: loaded.registry,
                        documents,
                        panel: Rc::new(RefCell::new(panel)),
                        assets: Default::default(),
                        catalog,
                        _lease: loaded.lease,
                        closing: false,
                        close_error: None,
                    })
                }
            };
            self.entries.insert(path, slot);
            self.revision = self.revision.wrapping_add(1);
        }
        self.start_next_load();
        changed
    }

    pub fn has_work(&self) -> bool {
        self.entries.values().any(|slot| match slot {
            WorkspaceSlot::Queued | WorkspaceSlot::Loading { .. } => true,
            WorkspaceSlot::Ready(workspace) => {
                if workspace.closing {
                    return workspace
                        .documents
                        .values()
                        .any(|d| d.save.borrow().is_running())
                        || (workspace.close_error.is_none()
                            && !workspace
                                .documents
                                .values()
                                .any(|d| d.save.borrow().error.is_some()));
                }
                workspace.catalog.is_pending()
                    || workspace.panel.borrow().has_live_output()
                    || workspace
                        .documents
                        .values()
                        .any(|d| d.save.borrow().is_running())
            }
            _ => false,
        })
    }

    /// Poll unopened documents on a bounded scheduler, never on a paint callback.
    pub fn poll(
        &mut self,
        settings: &EngineSettings,
        attached: Option<(&Path, SessionId)>,
        mut attached_ledger: Option<&mut TransactionLedger>,
        reserved_document: Option<(&Path, SessionId)>,
    ) -> bool {
        let mut changed = self.poll_loads(settings);
        changed |= self.poll_closing();
        if self.entries.is_empty() {
            return changed;
        }
        if self
            .last_poll
            .is_some_and(|last| last.elapsed() < Duration::from_millis(40))
        {
            return changed;
        }
        self.last_poll = Some(Instant::now());
        let catalog: &CommandCatalog = self
            .command_catalog
            .get_or_insert_with(CommandCatalog::builtin);
        for (path, slot) in &mut self.entries {
            let WorkspaceSlot::Ready(workspace) = slot else {
                continue;
            };
            if workspace.closing {
                continue;
            }
            let mut save_changed = false;
            for document in workspace.documents.values() {
                save_changed |= document.save.borrow_mut().poll();
            }
            changed |= save_changed;
            changed |= workspace.catalog.poll();
            workspace
                .panel
                .borrow_mut()
                .prepare(settings, Some(&workspace.project), catalog);
            if !save_changed && !workspace.panel.borrow().has_live_output() {
                continue;
            }
            let revision_before = workspace.panel.borrow().visual_revision();
            let mut panel = workspace.panel.borrow_mut();
            let attached_here = attached.filter(|(root, _)| *root == path.as_path());
            let ledger = if attached_here.is_some() {
                attached_ledger
                    .as_deref_mut()
                    .unwrap_or(&mut workspace.ledger)
            } else {
                &mut workspace.ledger
            };
            let ledger_before = ledger.revision();
            panel.visit_runs(|panel| {
                let document_id = panel
                    .document_id()
                    .unwrap_or(workspace.registry.active_session);
                if attached_here.is_some_and(|(_, id)| id == document_id) {
                    return;
                }
                if reserved_document.is_some_and(|(root, id)| root == path && id == document_id) {
                    return;
                }
                if !workspace
                    .registry
                    .sessions
                    .iter()
                    .any(|session| session.id == document_id)
                {
                    panel.save_error = Some("agent_workspace.document_missing".into());
                    return;
                }
                let Some(document) = workspace.documents.get_mut(&document_id.0.to_string()) else {
                    panel.save_error = Some("agent_workspace.document_missing".into());
                    return;
                };
                let mut viewport = std::mem::take(&mut document.viewport);
                let actions = poll_conversation(
                    panel,
                    &mut workspace.assets.borrow_mut(),
                    &mut document.save.borrow_mut(),
                    settings,
                    catalog,
                    &workspace.project,
                    &document.session,
                    &mut document.scene,
                    &document.nodes,
                    &mut viewport,
                    None,
                    document.electronics.as_mut(),
                    ledger,
                    workspace.catalog.assets(),
                    workspace.catalog.is_pending(),
                    workspace.catalog.error(),
                    &mut document.history,
                );
                document.viewport = viewport;
                for action in actions {
                    let changed = match action {
                        AgentEditorAction::Undo => document.history.undo(&mut document.scene),
                        AgentEditorAction::Redo => document.history.redo(&mut document.scene),
                    };
                    if changed {
                        ledger.observe_document(crate::scene_history::scene_fingerprint(
                            &document.scene,
                        ));
                        document.save.borrow_mut().submit(DocumentSave::capture(
                            &workspace.project,
                            &document.session,
                            &document.scene,
                            &document.nodes,
                            document.electronics.as_ref(),
                            ledger,
                        ));
                    }
                }
            });
            let calls = panel.pending_calls();
            workspace.assets.borrow_mut().cancel_abandoned(&calls);
            for document in workspace.documents.values() {
                let mut save = document.save.borrow_mut();
                if !save.is_running()
                    && save
                        .pending_call
                        .as_ref()
                        .is_some_and(|id| !calls.iter().any(|call| &call.id == id))
                {
                    save.pending_call = None;
                    save.pending_result = None;
                }
            }
            changed |= revision_before != panel.visual_revision();
            if ledger_before != ledger.revision() {
                if attached_here.is_none() {
                    if let Some(document) = workspace
                        .documents
                        .get(&workspace.registry.active_session.0.to_string())
                    {
                        ledger.mark_document(crate::scene_history::scene_fingerprint(
                            &document.scene,
                        ));
                    }
                }
                if let Err(error) = ledger.persist_for_project(&workspace.project.path) {
                    panel.save_error = Some(error);
                }
            }
            if workspace.assets.borrow_mut().take_catalog_dirty() {
                workspace.catalog.refresh();
            }
        }
        if changed {
            self.revision = self.revision.wrapping_add(1);
        }
        changed
    }

    pub fn shutdown(&mut self) -> Result<(), String> {
        let mut failures = Vec::new();
        for slot in self.entries.values_mut() {
            if let WorkspaceSlot::Ready(workspace) = slot {
                workspace.panel.borrow_mut().cancel_all();
                workspace.assets.borrow_mut().cancel_abandoned(&[]);
                for document in workspace.documents.values() {
                    if let Err(error) = document.save.borrow_mut().wait_for_save() {
                        failures.push(error);
                    }
                }
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("\n"))
        }
    }
}

fn load_workspace(path: &Path) -> Result<LoadedWorkspace, String> {
    let root = path.canonicalize().map_err(|e| e.to_string())?;
    if let Ok(endpoint) = raf_core::EndpointDescriptor::load_from_project(&root) {
        if endpoint.process_id != std::process::id()
            && endpoint
                .address
                .parse()
                .ok()
                .is_some_and(|address: std::net::SocketAddr| {
                    address.ip().is_loopback()
                        && std::net::TcpStream::connect_timeout(
                            &address,
                            Duration::from_millis(100),
                        )
                        .is_ok()
                })
        {
            return Err("agent_workspace.project_busy".into());
        }
    }
    let lease = ProjectWriteLease::acquire(&root)?;
    let mut project = Project::load(&root).map_err(|e| e.to_string())?;
    // The opened directory is authoritative, not a serialized path from a
    // previous location (or a non-verbatim Windows alias of this directory).
    project.path = root.clone();
    let supported: Vec<_> = env!("CARGO_PKG_VERSION").split('.').take(2).collect();
    let requested: Vec<_> = project.engine_version.split('.').take(2).collect();
    if requested
        .iter()
        .map(|s| s.parse::<u32>().unwrap_or(0))
        .collect::<Vec<_>>()
        > supported
            .iter()
            .map(|s| s.parse::<u32>().unwrap_or(0))
            .collect::<Vec<_>>()
    {
        return Err("agent_workspace.engine_incompatible".into());
    }
    let registry = load_registry(&project)?;
    if registry.sessions.len() > 64 {
        return Err("agent_workspace.document_budget".into());
    }
    let history = AgentHistory::load_checked(&root)?;
    let mut documents = Vec::new();
    for session in &registry.sessions {
        agent_project_store::recover(&project, session)?;
        let files = agent_project_store::read_document_files(&project, session)?;
        let decode = |path: &Path| files.get(path).and_then(|v| v.as_deref());
        let scene = decode(&session.scene_file)
            .map(ron::de::from_bytes)
            .transpose()
            .map_err(|e| e.to_string())?
            .unwrap_or_else(SceneGraph::new);
        let nodes = decode(&session.nodes_file)
            .map(ron::de::from_bytes)
            .transpose()
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| NodeGraph::new("Main"));
        let electronics = if project.project_type == ProjectType::Electronics {
            Some((
                decode(&session.schematic_file)
                    .map(ron::de::from_bytes)
                    .transpose()
                    .map_err(|e| e.to_string())?
                    .unwrap_or_else(|| raf_electronics::Schematic::new(&project.name)),
                decode(&session.pcb_file)
                    .map(ron::de::from_bytes)
                    .transpose()
                    .map_err(|e| e.to_string())?
                    .unwrap_or_else(|| raf_electronics::PcbLayout::new(&project.name)),
            ))
        } else {
            None
        };
        documents.push(LoadedDocument {
            session: session.clone(),
            scene,
            nodes,
            electronics,
            files,
        });
    }
    Ok(LoadedWorkspace {
        project,
        registry,
        documents,
        history,
        lease,
    })
}

fn load_registry(project: &Project) -> Result<ProjectSessionRegistry, String> {
    let path = confined_path(&project.path, Path::new(ProjectSessionRegistry::FILE_NAME))?;
    let existing = read_recoverable(&path)?;
    let registry: ProjectSessionRegistry = match existing.as_deref() {
        Some(bytes) => ron::de::from_bytes(bytes).map_err(|error| error.to_string())?,
        None => ProjectSessionRegistry::load_or_legacy(&project.path, project.project_type),
    };
    if registry.version > raf_core::session::SESSION_REGISTRY_VERSION
        || registry.sessions.is_empty()
        || registry.active().is_none()
    {
        return Err("agent_workspace.document_missing".into());
    }
    let mut ids = std::collections::HashSet::new();
    let mut paths = std::collections::HashSet::new();
    for session in &registry.sessions {
        if !ids.insert(session.id) {
            return Err("agent_workspace.invalid_document_path".into());
        }
        for relative in agent_project_store::document_files(
            session,
            project.project_type == ProjectType::Electronics,
        )
        .into_iter()
        .chain(std::iter::once(session.ui_document_file.clone()))
        {
            let target = confined_path(&project.path, &relative)?;
            if !paths.insert(target) {
                return Err("agent_workspace.invalid_document_path".into());
            }
        }
    }
    // Give legacy worlds stable IDs without moving their existing root documents.
    if existing.is_none() {
        let bytes = ron::ser::to_string_pretty(&registry, ron::ser::PrettyConfig::default())
            .map_err(|error| error.to_string())?;
        write_atomic(&path, bytes.as_bytes())?;
    }
    Ok(registry)
}

/// Shared attached/headless loop. A tool result is released only after its save receipt.
#[allow(clippy::too_many_arguments)]
pub(crate) fn poll_conversation(
    panel: &mut AgentPanel,
    assets: &mut AgentAssetTools,
    save: &mut ProjectSaveQueue,
    settings: &EngineSettings,
    catalog: &CommandCatalog,
    project: &Project,
    session: &ProjectSession,
    scene: &mut SceneGraph,
    nodes: &NodeGraph,
    viewport: &mut dyn GameViewportPort,
    graphics: Option<&mut RenderRuntime>,
    electronics: Option<&mut NativeElectronicsEditor>,
    ledger: &mut TransactionLedger,
    project_assets: &[String],
    catalog_pending: bool,
    catalog_error: Option<&str>,
    history: &mut SceneHistory,
) -> Vec<AgentEditorAction> {
    save.poll();
    panel.set_save_state(save.saved_revision, save.error.clone(), save.is_running());
    if panel.defer_context_open() || save.error.is_some() {
        return Vec::new();
    }
    panel.bind_document(session.id, &session.name);
    if panel.document_id() != Some(session.id) {
        return Vec::new();
    }
    // One document publishes one operation at a time, even across conversations.
    // Keep the original receipt until its owning runtime has consumed it.
    if save.pending_call.as_ref().is_some_and(|id| {
        !panel
            .runtime
            .pending_calls
            .iter()
            .any(|call| &call.id == id)
    }) {
        return Vec::new();
    }
    if !panel.runtime.status.blocks_input() && panel.readiness(settings) != AgentReadiness::Ready {
        return Vec::new();
    }
    if !panel.current_needs_poll() {
        return Vec::new();
    }
    let mut selection = SceneSelectionState {
        selected_node: viewport.selected_ids().first().copied(),
        selected_nodes: viewport.selected_ids(),
    };
    // Scene observations traverse real documents. Build them only for a new
    // submission, not for every streaming or HTTP-wait scheduler tick.
    if panel.has_pending_submission() {
        #[cfg(test)]
        OBSERVATION_BUILDS.with(|count| count.set(count.get() + 1));
        let snapshot = AgentObservationContext {
            scene,
            selection: &selection,
            project: Some(project),
            assets: project_assets,
            active_session: &session.name,
            revision: ledger.revision(),
            catalog_pending,
            catalog_error,
        }
        .snapshot();
        panel.start_pending_run(
            &snapshot,
            catalog,
            Some(project.project_type),
            settings.agent_mode,
            graphics.is_some(),
        );
    }
    let (image_provider, image_model) = panel
        .image_provider(settings)
        .map(|(p, m)| (Some(p), m))
        .unwrap_or_default();
    let mut actions = Vec::new();
    let mut changed = false;
    let inner = AgentToolExecutor {
        asset_tools: assets,
        asset_studio: &settings.external_asset_studio,
        image_provider,
        image_model,
        scene,
        selection: &mut selection,
        viewport: &mut *viewport,
        graphics,
        electronics,
        project: Some(AgentProjectContext::from_project(Some(project)).unwrap()),
        project_info: Some(project),
        project_assets,
        active_session: &session.id.0.to_string(),
        catalog_pending,
        catalog_error,
        catalog,
        language: settings.language,
        routes: panel.tool_routes.clone(),
        ledger,
        editor_actions: &mut actions,
        canvas_changed: &mut changed,
    };
    let mut executor = DurableExecutor {
        inner,
        save,
        session,
        nodes,
        history,
    };
    if let Some(decision) = panel.decision.take() {
        match decision {
            AgentDecision::ApproveAll => panel.approve_with_executor(&mut executor),
            AgentDecision::DenyAll => panel.deny_with_executor(&mut executor),
            AgentDecision::Approve(id) => panel.runtime.decide_call(&id, true),
            AgentDecision::Deny(id) => panel.runtime.decide_call(&id, false),
        }
    }
    panel.runtime.set_review_mutations(panel.review_writes);
    panel.poll(&mut executor);
    drop(executor);
    viewport.set_selected_ids(selection.selected_nodes);
    actions
}

struct DurableExecutor<'a> {
    inner: AgentToolExecutor<'a>,
    save: &'a mut ProjectSaveQueue,
    session: &'a ProjectSession,
    nodes: &'a NodeGraph,
    history: &'a mut SceneHistory,
}

impl ToolExecutor for DurableExecutor<'_> {
    fn revision(&self) -> Option<u64> {
        self.inner.revision()
    }
    fn preview_for_review(
        &mut self,
        id: &str,
        name: &str,
        arguments: serde_json::Value,
    ) -> Option<AgentToolResult> {
        self.inner.preview_for_review(id, name, arguments)
    }
    fn kind(&self, name: &str) -> AgentToolKind {
        self.inner.kind(name)
    }
    fn describe(&self, name: &str) -> String {
        self.inner.describe(name)
    }
    fn is_pending(&self, id: &str) -> bool {
        self.save.pending_call.as_deref() == Some(id) || self.inner.is_pending(id)
    }
    fn poll_pending(&mut self, id: &str) -> Option<Result<AgentToolResult, String>> {
        if self.save.pending_call.as_deref() == Some(id) {
            self.save.poll();
            if self.save.is_pending() {
                return None;
            }
            self.save.pending_call = None;
            let mut result = self.save.pending_result.take()?;
            if !result.data.is_object() {
                result.data = serde_json::json!({"value": result.data});
            }
            result.data["persistence"] = serde_json::json!({"saved": true, "revision": self.save.saved_revision, "document_id": self.session.id.0});
            return Some(Ok(result));
        }
        self.inner.poll_pending(id)
    }
    fn execute(
        &mut self,
        id: &str,
        name: &str,
        args: serde_json::Value,
        mode: ToolExecutionMode,
    ) -> Result<AgentToolResult, String> {
        if self.save.is_pending() {
            return Err("agent_workspace.save_pending".into());
        }
        let before = (mode == ToolExecutionMode::Apply
            && self.inner.kind(name) == AgentToolKind::Mutation)
            .then(|| self.inner.scene.clone());
        let mut result = self.inner.execute(id, name, args, mode)?;
        if mode == ToolExecutionMode::Apply && !self.inner.editor_actions.is_empty() {
            for action in self.inner.editor_actions.drain(..) {
                result.changed |= match action {
                    AgentEditorAction::Undo => self.history.undo(self.inner.scene),
                    AgentEditorAction::Redo => self.history.redo(self.inner.scene),
                };
            }
            if result.changed {
                self.inner
                    .ledger
                    .observe_document(crate::scene_history::scene_fingerprint(self.inner.scene));
                result.revision = Some(self.inner.ledger.revision());
                *self.inner.canvas_changed = true;
            }
        } else if let Some(before) = before {
            self.history.record_if_changed(before, self.inner.scene);
        }
        if result.changed && mode == ToolExecutionMode::Apply {
            if let Some(project) = self.inner.project_info {
                let request = DocumentSave::capture(
                    project,
                    self.session,
                    self.inner.scene,
                    self.nodes,
                    self.inner.electronics.as_deref(),
                    self.inner.ledger,
                );
                self.save.pending_call = Some(id.to_string());
                self.save.pending_result = Some(result.clone());
                self.save.submit(request);
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiSurfaceSession};

    fn drive_until(service: &mut AgentService, mut done: impl FnMut(&AgentService) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while !done(service) && Instant::now() < deadline {
            service.last_poll = None;
            service.poll(&EngineSettings::default(), None, None, None);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(done(service), "workspace did not settle");
    }

    #[test]
    fn agent_workspace_preview_surface_matches_canvas_geometry_and_both_locales() {
        use crate::agent_workspace_layout::WorkspaceLayout;
        use crate::agent_workspace_surface::{build_workspace_surface_with_layout, EvidenceTab};
        let root =
            std::env::temp_dir().join(format!("raf-agent-preview-layout-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let project = Project::create("Preview layout", ProjectType::Game, &root).unwrap();
        let mut service = AgentService::default();
        service.request(project.path.clone());
        drive_until(&mut service, |s| s.is_open(&project.path) && !s.has_work());
        let workspace = service.selected_workspace_mut().unwrap();
        let id = workspace.registry.active_session;
        workspace
            .documents
            .get_mut(&id.0.to_string())
            .unwrap()
            .scene
            .add_root_with_primitive("Read-only cube", raf_core::scene::Primitive::Cube);
        let layout = WorkspaceLayout {
            review: 480.0,
            ..Default::default()
        };
        let rows = [crate::studio_surface::HubSurfaceProject {
            name: project.name.clone(),
            path: project.path.clone(),
            project_type: ProjectType::Game,
            last_opened_label: String::new(),
        }];
        for language in [raf_core::Language::English, raf_core::Language::Spanish] {
            for palette in [StudioUiPalette::IndustrialDark, StudioUiPalette::PaperLight] {
                for width in [900u32, 1440] {
                    let settings = EngineSettings {
                        language,
                        theme: if palette == StudioUiPalette::PaperLight {
                            raf_core::Theme::Light
                        } else {
                            raf_core::Theme::Dark
                        },
                        render_execution_policy: raf_core::config::RenderExecutionPolicy::CpuOnly,
                        ..Default::default()
                    };
                    let geometry = layout.resolve([width as f32, 900.0], true, true);
                    let surface = build_workspace_surface_with_layout(
                        palette,
                        &service,
                        &rows,
                        &settings,
                        [width as f32, 900.0],
                        0.0,
                        0,
                        EvidenceTab::Preview,
                        true,
                        &layout,
                    );
                    for id in [
                        "agent.workspace.preview",
                        "agent.workspace.tab.preview",
                        "agent.workspace.preview.fit",
                        "agent.workspace.preview.close",
                    ] {
                        let icon = surface.root.find(id).unwrap().icon.unwrap();
                        assert_eq!(
                            icon.tint,
                            Some(crate::agent_workspace_surface::workspace_tokens(palette).text),
                            "{id} must stay visible in both themes"
                        );
                    }
                    let mut session = UiSurfaceSession::default();
                    let frame = session.build_frame_with_resolved_text(
                        &surface,
                        width,
                        900,
                        [0; 4],
                        |key| raf_core::i18n::t(key, language),
                    );
                    let canvas = frame
                        .layout_boxes
                        .iter()
                        .find(|item| item.id == "agent.workspace.preview.canvas")
                        .unwrap();
                    let wanted = geometry.preview_rect().unwrap();
                    assert_eq!(
                        canvas.rect, wanted,
                        "renderer and UI must share the same canvas rectangle"
                    );
                    assert_eq!(canvas.style.fill[3], 0);
                    let center = [
                        wanted.x + wanted.width * 0.5,
                        wanted.y + wanted.height * 0.5,
                    ];
                    assert!(
                        !frame
                            .layout_boxes
                            .iter()
                            .any(|b| b.style.fill[3] == 255 && b.rect.contains(center)),
                        "opaque UI covered the preview"
                    );
                    for id in [
                        "agent.workspace.preview.close",
                        "agent.workspace.preview.fit",
                        "agent.workspace.divider.review",
                        "agent.input",
                    ] {
                        let item = frame.layout_boxes.iter().find(|b| b.id == id).unwrap();
                        assert!(
                            item.rect.x >= 0.0
                                && item.rect.x + item.rect.width <= width as f32 + 1.0,
                            "{id}: {:?}",
                            item.rect
                        );
                    }
                    if let Some(directory) = std::env::var_os("RAF_AGENT_UI_QA_DIR") {
                        std::fs::create_dir_all(&directory).unwrap();
                        let mut host =
                            raf_render::api_graphic_basic::ui_surface::CpuUiSurfaceHost::new(
                                surface, [0; 4],
                            );
                        let rendered =
                            host.render([width, 900], |key| raf_core::i18n::t(key, language));
                        let ui = image::RgbaImage::from_raw(width, 900, rendered.pixels.to_vec())
                            .unwrap();
                        let workspace = service.selected_workspace().unwrap();
                        let scene = &workspace.documents[&id.0.to_string()].scene;
                        let mut preview = crate::agent_preview::AgentPreview::default();
                        preview.sync(
                            Some(crate::agent_preview::PreviewTarget {
                                root: workspace.project.path.clone(),
                                document: id,
                            }),
                            true,
                            Instant::now(),
                        );
                        let target = raf_render::api_graphic_basic::CanvasTargetRect::new(
                            wanted.x as u32,
                            wanted.y as u32,
                            wanted.width as u32,
                            wanted.height as u32,
                        );
                        let layer = preview
                            .render_layer(scene, &workspace.project, &settings, target, || None)
                            .unwrap();
                        if let raf_render::api_graphic_basic::device::SceneFrameOutput::CpuPixels(
                            pixels,
                        ) = layer.output.as_ref()
                        {
                            let canvas = image::RgbaImage::from_raw(
                                layer.source_size[0],
                                layer.source_size[1],
                                pixels.clone(),
                            )
                            .unwrap();
                            let canvas = image::imageops::resize(
                                &canvas,
                                target.width,
                                target.height,
                                image::imageops::FilterType::Nearest,
                            );
                            let mut composed = image::RgbaImage::from_pixel(
                                width,
                                900,
                                image::Rgba([9, 9, 10, 255]),
                            );
                            image::imageops::overlay(
                                &mut composed,
                                &canvas,
                                target.x as i64,
                                target.y as i64,
                            );
                            image::imageops::overlay(&mut composed, &ui, 0, 0);
                            composed
                                .save(PathBuf::from(&directory).join(format!(
                                    "agents-preview-{width}-{palette:?}-{language:?}.png"
                                )))
                                .unwrap();
                        }
                    }
                }
            }
        }
        service.close_all();
        drive_until(&mut service, |s| !s.has_projects());
        drop(service);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn agent_workspace_startup_is_lazy_and_bulk_load_is_serial() {
        let root = std::env::temp_dir().join(format!("raf-agent-bulk-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let first = Project::create("First", ProjectType::Game, &root).unwrap();
        let second = Project::create("Second", ProjectType::Game, &root).unwrap();
        let mut service = AgentService::default();
        service.poll(&EngineSettings::default(), None, None, None);
        assert!(service.entries.is_empty());
        assert!(service.command_catalog.is_none());
        assert!(!service.has_work());
        service.start_all([first.path.clone(), second.path.clone(), first.path.clone()]);
        assert!(
            service.selected.is_none(),
            "bulk loading must not hijack navigation"
        );
        assert_eq!(service.entries.len(), 2);
        assert_eq!(
            service
                .entries
                .values()
                .filter(|s| matches!(s, WorkspaceSlot::Loading { .. }))
                .count(),
            1
        );
        assert_eq!(
            service
                .entries
                .values()
                .filter(|s| matches!(s, WorkspaceSlot::Queued))
                .count(),
            1
        );
        service.request(first.path.clone());
        let builds_before = OBSERVATION_BUILDS.with(|c| c.get());
        drive_until(&mut service, |s| s.projects().len() == 2 && !s.has_work());
        assert!(service.is_open(&first.path) && service.is_open(&second.path));
        assert!(service.is_selected(&first.path));
        assert_eq!(
            OBSERVATION_BUILDS.with(|c| c.get()),
            builds_before,
            "opening projects must not traverse the scene for a model request"
        );
        assert!(service.entries.values().all(|slot| matches!(slot, WorkspaceSlot::Ready(w) if w.panel.borrow().runtime.pending_calls.is_empty())));
        let started = Instant::now();
        for _ in 0..250 {
            service.last_poll = None;
            assert!(!service.poll(&EngineSettings::default(), None, None, None));
        }
        eprintln!(
            "agent_workspace: 250 idle polls, two loaded projects: {:?}; scene observations: {}",
            started.elapsed(),
            OBSERVATION_BUILDS.with(|c| c.get()) - builds_before
        );
        let panel = service.selected_workspace().unwrap().panel.clone();
        panel.borrow_mut().apply_action(
            crate::panels::ai_chat::AgentAction::NewChat,
            &mut EngineSettings::default(),
        );
        panel.borrow_mut().apply_action(
            crate::panels::ai_chat::AgentAction::SelectSession(0),
            &mut EngineSettings::default(),
        );
        drive_until(&mut service, |s| !s.has_work());
        assert_eq!(panel.borrow().history.active_index, Some(0));
        service.close_project(&first.path);
        drive_until(&mut service, |s| !s.is_open(&first.path));
        assert!(
            service.is_open(&second.path),
            "individual close must retain the other project"
        );
        drop(panel);
        service.close_all();
        drive_until(&mut service, |s| !s.has_projects());
        drop(service);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn agent_workspace_close_cancels_submissions_and_waits_for_durable_save() {
        let root = std::env::temp_dir().join(format!("raf-agent-close-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let project = Project::create("Close", ProjectType::Game, &root).unwrap();
        let mut service = AgentService::default();
        service.request(project.path.clone());
        drive_until(&mut service, |s| s.is_open(&project.path) && !s.has_work());
        let workspace = service.selected_workspace_mut().unwrap();
        let id = workspace.registry.active_session;
        let save = workspace.documents[&id.0.to_string()].save.clone();
        let panel = workspace.panel.clone();
        let mut settings = EngineSettings::default();
        panel.borrow_mut().apply_action(
            crate::panels::ai_chat::AgentAction::SetInput("Do not send this".into()),
            &mut settings,
        );
        panel
            .borrow_mut()
            .apply_action(crate::panels::ai_chat::AgentAction::Submit, &mut settings);
        assert!(panel.borrow().has_pending_submission());
        panel.borrow_mut().apply_action(
            crate::panels::ai_chat::AgentAction::SetInput("Keep this unsent draft".into()),
            &mut settings,
        );
        save.borrow_mut().error = Some("agent_workspace.save_worker_failed".into());
        service.close_project(&project.path);
        service.poll(&settings, None, None, None);
        assert!(!panel.borrow().has_pending_submission());
        assert!(service.is_open(&project.path));
        assert!(
            !service.has_work(),
            "a failed close must not spin the idle scheduler"
        );
        assert!(ProjectWriteLease::acquire(&project.path).is_err());
        let workspace = service.selected_workspace_mut().unwrap();
        let document = workspace.documents.get_mut(&id.0.to_string()).unwrap();
        document.scene.add_root_folder("SavedBeforeClose");
        save.borrow_mut().submit(DocumentSave::capture(
            &workspace.project,
            &document.session,
            &document.scene,
            &document.nodes,
            None,
            &workspace.ledger,
        ));
        let scene_path = project.path.join(&document.session.scene_file);
        drive_until(&mut service, |s| !s.has_projects());
        let scene: SceneGraph = ron::de::from_bytes(&std::fs::read(scene_path).unwrap()).unwrap();
        assert!(scene
            .all_live_ids()
            .iter()
            .any(|id| scene.get(*id).unwrap().name == "SavedBeforeClose"));
        let lease = ProjectWriteLease::acquire(&project.path).unwrap();
        drop(lease);
        assert_eq!(
            AgentHistory::load_checked(&project.path)
                .unwrap()
                .active_session()
                .unwrap()
                .draft,
            "Keep this unsent draft"
        );
        drop(panel);
        drop(save);
        drop(service);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn agent_workspace_close_all_discards_queued_and_late_loads() {
        let root =
            std::env::temp_dir().join(format!("raf-agent-cancel-load-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let first = Project::create("First", ProjectType::Game, &root).unwrap();
        let second = Project::create("Second", ProjectType::Game, &root).unwrap();
        let mut service = AgentService::default();
        service.start_all([first.path.clone(), second.path.clone()]);
        service.close_all();
        assert!(service.open_queue.is_empty());
        assert!(!service.is_loading(&second.path));
        drive_until(&mut service, |s| !s.has_projects());
        assert!(service.projects().is_empty());
        drop(ProjectWriteLease::acquire(&first.path).unwrap());
        drop(service);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn agent_workspace_wait_ticks_do_not_rebuild_project_observations() {
        let root =
            std::env::temp_dir().join(format!("raf-agent-idle-context-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let project = Project::create("Context", ProjectType::Game, &root).unwrap();
        let mut service = AgentService::default();
        service.request(project.path.clone());
        drive_until(&mut service, |s| s.is_open(&project.path) && !s.has_work());
        let workspace = service.selected_workspace_mut().unwrap();
        let document = workspace
            .documents
            .get_mut(&workspace.registry.active_session.0.to_string())
            .unwrap();
        for index in 0..2_000 {
            document.scene.add_root_folder(&format!("Context_{index}"));
        }
        let catalog = CommandCatalog::builtin();
        let settings = EngineSettings::default();
        let before = OBSERVATION_BUILDS.with(|c| c.get());
        let started = Instant::now();
        for _ in 0..25 {
            let mut panel = workspace.panel.borrow_mut();
            // No provider thread is launched. Exercise the runtime's disconnected
            // wait edge to prove that a poll itself cannot build scene context.
            panel.runtime.status = raf_ai::agent_runtime::AgentStatus::Thinking;
            poll_conversation(
                &mut panel,
                &mut workspace.assets.borrow_mut(),
                &mut document.save.borrow_mut(),
                &settings,
                &catalog,
                &workspace.project,
                &document.session,
                &mut document.scene,
                &document.nodes,
                &mut document.viewport,
                None,
                None,
                &mut workspace.ledger,
                &[],
                false,
                None,
                &mut document.history,
            );
        }
        eprintln!(
            "agent_workspace: 25 wait-edge polls with 2,000 entities: {:?}; scene observations: {}",
            started.elapsed(),
            OBSERVATION_BUILDS.with(|c| c.get()) - before
        );
        assert_eq!(OBSERVATION_BUILDS.with(|c| c.get()), before);
        service.close_all();
        drive_until(&mut service, |s| !s.has_projects());
        drop(service);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn agent_workspace_close_retains_project_when_history_write_fails() {
        let root =
            std::env::temp_dir().join(format!("raf-agent-history-close-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let project = Project::create("History", ProjectType::Game, &root).unwrap();
        let mut service = AgentService::default();
        service.request(project.path.clone());
        drive_until(&mut service, |s| s.is_open(&project.path) && !s.has_work());
        while !service
            .selected_workspace()
            .unwrap()
            .panel
            .borrow_mut()
            .finish_history_writes()
            .unwrap()
        {
            std::thread::sleep(Duration::from_millis(2));
        }
        let backup =
            AgentHistory::path_for(&project.path).with_file_name(".agent_history.ron.previous");
        if backup.exists() {
            std::fs::remove_file(&backup).unwrap();
        }
        std::fs::create_dir_all(&backup).unwrap();
        service
            .selected_workspace()
            .unwrap()
            .panel
            .borrow_mut()
            .retry_history_writes();
        service
            .selected_workspace()
            .unwrap()
            .panel
            .borrow_mut()
            .apply_action(
                crate::panels::ai_chat::AgentAction::NewChat,
                &mut EngineSettings::default(),
            );
        service.close_project(&project.path);
        drive_until(&mut service, |s| s.selected_close_error().is_some());
        assert!(service.is_open(&project.path));
        assert!(!service.has_work());
        std::fs::remove_dir(&backup).unwrap();
        service.retry_open();
        drive_until(&mut service, |s| !s.has_projects());
        assert!(
            AgentHistory::load_checked(&project.path)
                .unwrap()
                .sessions
                .len()
                >= 2
        );
        drop(service);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn agent_workspace_real_project_load_and_native_layout() {
        let temporary =
            std::env::temp_dir().join(format!("raf-agent-host-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temporary).unwrap();
        let project = Project::create("Workspace", ProjectType::Game, &temporary).unwrap();
        let mut service = AgentService::default();
        service.request(project.path.clone());
        let settings = EngineSettings {
            language: raf_core::Language::Spanish,
            ..Default::default()
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        while service.loading() && Instant::now() < deadline {
            service.poll_loads(&settings);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(service.error().is_none(), "{:?}", service.error());
        let workspace = service
            .selected_workspace()
            .expect("worker mounted the real project");
        assert_eq!(workspace.project.id, project.id);
        assert_eq!(workspace.project.path, project.path.canonicalize().unwrap());
        assert!(service.workspace(&project.path).is_some());
        assert!(workspace
            .documents
            .contains_key(&workspace.registry.active_session.0.to_string()));
        let active = workspace.registry.active().unwrap();
        workspace
            .panel
            .borrow_mut()
            .bind_document(active.id, &active.name);
        let projects = [crate::studio_surface::HubSurfaceProject {
            name: project.name.clone(),
            path: service.selected.clone().unwrap(),
            project_type: project.project_type,
            last_opened_label: String::new(),
        }];
        for width in [900u32, 1440] {
            let surface = crate::agent_workspace_surface::build_workspace_surface(
                StudioUiPalette::IndustrialDark,
                &service,
                &projects,
                &settings,
                [width as f32, 900.0],
                0.0,
                3,
                crate::agent_workspace_surface::EvidenceTab::Changes,
                width >= 1200,
            );
            let mut session = UiSurfaceSession::default();
            let frame = session.build_frame_with_resolved_text(
                &surface,
                width,
                900,
                [9, 9, 10, 255],
                |key| raf_core::i18n::t(key, settings.language),
            );
            for id in [
                "agent.workspace.back",
                "agent.workspace.editor",
                "agent.input",
            ] {
                let control = frame
                    .layout_boxes
                    .iter()
                    .find(|item| item.id == id)
                    .expect(id);
                assert!(control.rect.width > 0.0 && control.rect.height > 0.0);
                assert!(
                    control.rect.x >= 0.0
                        && control.rect.x + control.rect.width <= width as f32 + 1.0,
                    "{id}: {:?}",
                    control.rect
                );
            }
            assert_eq!(
                frame
                    .layout_boxes
                    .iter()
                    .any(|item| item.id == "agent.workspace.evidence"),
                width >= 1200
            );
            if width >= 1200 {
                let tabs: Vec<_> = ["changes", "artifacts", "verification"]
                    .iter()
                    .map(|name| {
                        frame
                            .layout_boxes
                            .iter()
                            .find(|item| item.id == format!("agent.workspace.tab.{name}"))
                            .unwrap()
                            .rect
                    })
                    .collect();
                assert!(tabs
                    .windows(2)
                    .all(|pair| pair[0].x + pair[0].width <= pair[1].x + 0.5));
                let empty = frame
                    .layout_boxes
                    .iter()
                    .find(|item| item.id == "agent.workspace.evidence.empty")
                    .unwrap();
                assert!(empty.rect.y >= tabs[0].y + tabs[0].height);
            }
            let review = frame
                .layout_boxes
                .iter()
                .find(|item| item.id == "agent.review")
                .unwrap();
            assert!(review.rect.y + review.rect.height < 900.0);
            if let Some(directory) = std::env::var_os("RAF_AGENT_UI_QA_DIR") {
                let directory = PathBuf::from(directory);
                std::fs::create_dir_all(&directory).unwrap();
                let mut host = raf_render::api_graphic_basic::ui_surface::CpuUiSurfaceHost::new(
                    surface,
                    [9, 9, 10, 255],
                );
                let rendered = host.render([width, 900], |key| {
                    raf_core::i18n::t(key, settings.language)
                });
                image::save_buffer(
                    directory.join(format!("agents-{width}.png")),
                    rendered.pixels,
                    width,
                    900,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
        }
        service.shutdown().unwrap();
        drop(service);
        std::fs::remove_dir_all(temporary).unwrap();
    }

    #[test]
    fn agent_workspace_legacy_ids_are_stable_and_corrupt_registry_is_preserved() {
        let temporary =
            std::env::temp_dir().join(format!("raf-agent-legacy-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temporary).unwrap();
        let project = Project::create("Legacy", ProjectType::Game, &temporary).unwrap();
        let path = project.path.join(ProjectSessionRegistry::FILE_NAME);
        std::fs::remove_file(&path).unwrap();
        let first = load_registry(&project).unwrap();
        let second = load_registry(&project).unwrap();
        assert_eq!(first.active_session, second.active_session);
        assert_eq!(
            first.active().unwrap().scene_file,
            PathBuf::from("scene.ron")
        );
        std::fs::write(&path, b"corrupt registry").unwrap();
        assert!(load_registry(&project).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"corrupt registry");
        std::fs::remove_dir_all(temporary).unwrap();
    }
}
