//! Agent workspace integration and document ownership handoffs.
use super::*;
use crate::agent_preview::PreviewTarget;
use crate::agent_project_store::{read_document_files, DocumentSave};
use crate::agent_workspace_surface_host::{AgentWorkspaceIntent, AgentWorkspaceSurfaceHost};

/// Resolve the one live owner; a mounted editor leaves only a placeholder in
/// the service. Never render that placeholder or a different chat's world.
pub(super) fn preview_document<'a>(
    service: &'a crate::agent_service::AgentService,
    target: &PreviewTarget,
    attached: Option<(&Path, SessionId, &'a SceneGraph)>,
) -> Option<(&'a Project, &'a SceneGraph)> {
    let workspace = service
        .workspace(&target.root)
        .filter(|w| !w.closing && w.project.project_type == ProjectType::Game)?;
    if let Some((root, id, scene)) =
        attached.filter(|(root, id, _)| *root == target.root.as_path() && *id == target.document)
    {
        let _ = (root, id);
        return Some((&workspace.project, scene));
    }
    let document = workspace.documents.get(&target.document.0.to_string())?;
    Some((&workspace.project, &document.scene))
}

pub(super) fn render_preview_layer(
    surface: &mut AgentWorkspaceSurfaceHost,
    service: &crate::agent_service::AgentService,
    attached: Option<(&Path, SessionId, &SceneGraph)>,
    settings: &EngineSettings,
    host: &raf_render::api_graphic_basic::ui_surface::NativeUiWindowHost,
    scale: f32,
    size: [u32; 2],
) -> Option<raf_render::api_graphic_basic::EditorCanvasLayer> {
    let target = surface.preview.target.clone()?;
    let rect = surface.preview_rect(service)?;
    let (project, scene) = preview_document(service, &target, attached)?;
    surface.preview.render_layer(
        scene,
        project,
        settings,
        rect.to_physical(scale, size),
        || Some(host.shared_graphics_context()),
    )
}

pub(super) fn park_live_session(
    service: &mut crate::agent_service::AgentService,
    project: &Project,
    id: SessionId,
    scene: &SceneGraph,
    runtime: &NativeEditorRuntime,
    electronics: Option<&NativeElectronicsEditor>,
) -> Result<(), String> {
    if let Some(workspace) = service.workspace_mut(&project.path) {
        if let Some(document) = workspace.documents.get_mut(&id.0.to_string()) {
            document.save.borrow_mut().wait_for_save()?;
            document.scene = scene.clone();
            document.nodes = runtime.node_graph().clone();
            document.history = runtime.history().clone();
            document.electronics = electronics.map(|e| {
                NativeElectronicsEditor::from_documents(
                    project,
                    e.schematic().clone(),
                    e.pcb().clone(),
                )
            });
        }
    }
    Ok(())
}

pub(super) fn mount_live_session(
    service: &mut crate::agent_service::AgentService,
    project: &Project,
    previous: SessionId,
    next: SessionId,
    scene: &mut SceneGraph,
    runtime: &mut NativeEditorRuntime,
    electronics: &mut Option<NativeElectronicsEditor>,
    workbench: &mut NativeGameWorkbench,
) -> Result<(), String> {
    let Some(workspace) = service.workspace_mut(&project.path) else {
        return Ok(());
    };
    workspace.registry =
        ProjectSessionRegistry::load_or_legacy(&project.path, project.project_type);
    if let Some(old) = workspace.documents.get(&previous.0.to_string()) {
        old.save.borrow_mut().expected = read_document_files(project, &old.session)?;
    }
    if !workspace.documents.contains_key(&next.0.to_string()) {
        let session = workspace
            .registry
            .sessions
            .iter()
            .find(|s| s.id == next)
            .cloned()
            .ok_or("agent_workspace.document_missing")?;
        let save = crate::agent_project_store::ProjectSaveQueue::with_files(read_document_files(
            project, &session,
        )?);
        workspace.documents.insert(
            next.0.to_string(),
            crate::agent_service::ProjectDocumentHost {
                session,
                scene: scene.clone(),
                nodes: runtime.node_graph().clone(),
                electronics: electronics.as_ref().map(|e| {
                    NativeElectronicsEditor::from_documents(
                        project,
                        e.schematic().clone(),
                        e.pcb().clone(),
                    )
                }),
                history: crate::scene_history::SceneHistory::default(),
                viewport: Default::default(),
                save: std::rc::Rc::new(std::cell::RefCell::new(save)),
            },
        );
    }
    if let Some(document) = workspace.documents.get_mut(&next.0.to_string()) {
        document.save.borrow_mut().wait_for_save()?;
        *scene = std::mem::take(&mut document.scene);
        runtime.set_node_graph(std::mem::replace(
            &mut document.nodes,
            raf_nodes::NodeGraph::new("Main"),
        ));
        runtime.install_agent_history(std::mem::take(&mut document.history));
        *electronics = document.electronics.take();
        workbench.bind_agent_workspace(
            workspace.panel.clone(),
            workspace.assets.clone(),
            document.save.clone(),
        );
        workbench.set_agent_sessions(project, workspace.registry.clone());
    }
    Ok(())
}

impl NativeEditorApplication {
    pub(super) fn poll_agent_preview(&mut self) -> Option<Instant> {
        let surface = self.agent_surface.as_mut()?;
        if self.agents_open {
            surface.sync_preview(&self.agents, Instant::now());
        } else {
            surface.close_preview();
        }
        if !surface.preview.visible
            || self.settings_open
            || self.exit_confirmation_open
            || !self.input.snapshot().window_focused
        {
            if let Some(runtime) = self.runtime.as_mut() {
                surface.suspend_preview_input(runtime.input_router_mut());
            }
        }
        if surface.take_release_canvas() {
            if let Some(compositor) = self.compositor.as_mut() {
                compositor.release_canvas();
            }
        }
        let changed = surface
            .preview
            .target
            .as_ref()
            .and_then(|target| {
                let attached = self
                    .project
                    .as_ref()
                    .zip(self.workbench.as_ref())
                    .map(|(p, w)| (p.path.as_path(), w.active_session_id(), &self.scene));
                preview_document(&self.agents, target, attached)
                    .map(|(_, scene)| surface.preview.scene_changed(scene))
            })
            .unwrap_or(false);
        if changed || surface.preview.moving {
            if let Some(runtime) = self.runtime.as_mut() {
                runtime.request_ui_frame();
            }
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
        }
        if surface.preview.moving {
            Some(Instant::now() + Duration::from_millis(16))
        } else {
            surface.preview.deadline()
        }
    }
    pub(super) fn agent_projects(&self) -> Vec<crate::studio_surface::HubSurfaceProject> {
        let mut projects = self
            .studio
            .as_ref()
            .map(|s| s.model_snapshot().projects)
            .unwrap_or_default();
        for project in self.agents.projects() {
            if !projects.iter().any(|row| {
                row.path == project.path
                    || self
                        .agents
                        .workspace(&row.path)
                        .is_some_and(|workspace| workspace.project.path == project.path)
            }) {
                projects.push(crate::studio_surface::HubSurfaceProject {
                    name: project.name.clone(),
                    path: project.path.clone(),
                    project_type: project.project_type,
                    last_opened_label: String::new(),
                });
            }
        }
        projects
    }

    pub(super) fn ensure_agent_surface(&mut self) {
        if self.agent_surface.is_some() {
            return;
        }
        self.ensure_studio();
        let projects = self.agent_projects();
        let rect = self.current_window_rect();
        if let Some(host) = self.window_host.as_ref() {
            self.agent_surface = Some(AgentWorkspaceSurfaceHost::new(
                &host.graphics_context(),
                rect,
                native_palette(self.settings_state.theme),
                &self.agents,
                &projects,
                &self.settings_state,
            ));
        }
    }

    pub(super) fn open_agents(&mut self, path: Option<std::path::PathBuf>) {
        self.cancel_editor_project();
        let path = path.or_else(|| self.project.as_ref().map(|p| p.path.clone()));
        if let Some(path) = path {
            self.agents.request(path);
        }
        self.agents_open = true;
        self.settings_open = false;
        self.ensure_agent_surface();
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.input_router_mut().cancel_all();
            runtime.request_ui_frame();
            if let Some(surface) = self.agent_surface.as_mut() {
                surface.reset(runtime.input_router_mut());
            }
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub(super) fn request_editor_project(&mut self, path: std::path::PathBuf) {
        // Loading a project is not navigation to the Agents section.
        let from_agents = self.agents_open;
        if !from_agents && self.project.is_none() {
            self.ensure_studio();
        }
        self.agents.request(path);
        self.pending_editor_document = if from_agents {
            self.agents
                .selected_workspace()
                .and_then(|workspace| workspace.panel.borrow().document_id())
        } else {
            None
        };
        self.pending_editor_project = self.agents.selected.clone();
        self.editor_project_error = None;
        self.settings_open = false;
        self.update_editor_open_feedback();
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.input_router_mut().cancel_all();
            runtime.request_ui_frame();
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub(super) fn cancel_editor_project(&mut self) {
        self.pending_editor_project = None;
        self.pending_editor_document = None;
        self.editor_project_error = None;
        if let Some(studio) = self.studio.as_mut() {
            studio.set_project_opening(None, None);
        }
    }

    pub(super) fn retry_editor_project(&mut self) {
        if let Some(path) = self.pending_editor_project.clone() {
            self.agents.request(path);
            self.agents.retry_open();
            if let Some(workspace) = self.agents.selected_workspace() {
                for document in workspace.documents.values() {
                    document.save.borrow_mut().retry();
                }
            }
            self.editor_project_error = None;
            self.update_editor_open_feedback();
        }
    }

    fn update_editor_open_feedback(&mut self) {
        if !self.agents_open && self.project.is_none() {
            let name = self.pending_editor_project.as_ref().map(|path| {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
            if let Some(studio) = self.studio.as_mut() {
                studio.set_project_opening(name, self.editor_project_error.clone());
            }
        }
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.request_ui_frame();
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    fn poll_editor_open(&mut self) {
        let Some(path) = self.pending_editor_project.clone() else {
            return;
        };
        if self.agents.selected.as_ref() != Some(&path) {
            self.cancel_editor_project();
            return;
        }
        let prepared = self.agents.workspace_mut(&path).map(
            |workspace| -> Result<Option<(Project, SessionId)>, String> {
                if workspace.closing {
                    return Err("agent_workspace.project_closing".into());
                }
                let target = self
                    .pending_editor_document
                    .unwrap_or(workspace.registry.active_session);
                if !workspace
                    .registry
                    .sessions
                    .iter()
                    .any(|session| session.id == target)
                {
                    return Err("agent_workspace.document_missing".into());
                }
                let document = workspace
                    .documents
                    .get(&target.0.to_string())
                    .ok_or("agent_workspace.document_missing")?;
                let mut save = document.save.borrow_mut();
                save.poll();
                if let Some(error) = &save.error {
                    return Err(error.clone());
                }
                if save.is_running() {
                    return Ok(None);
                }
                if workspace.registry.active_session != target {
                    workspace.registry.active_session = target;
                    workspace.registry.save(&workspace.project.path)?;
                }
                Ok(Some((workspace.project.clone(), target)))
            },
        );
        let result = match prepared {
            Some(Ok(Some((project, target)))) => {
                // Opening a project is what makes it "recent". Without this,
                // the Hub list would only reorder on create/duplicate.
                crate::native_studio::remember_project(&project);
                if self
                    .project
                    .as_ref()
                    .is_some_and(|current| current.path == project.path)
                    && self
                        .workbench
                        .as_ref()
                        .is_some_and(|workbench| workbench.active_session_id() == target)
                {
                    self.agents_open = false;
                    if let Some(runtime) = self.runtime.as_mut() {
                        runtime.input_router_mut().cancel_all();
                        runtime.request_canvas_frame();
                    }
                    Ok(())
                } else {
                    self.activate_project(project)
                }
            }
            Some(Err(error)) => Err(error),
            None if self.agents.error().is_some() => Err(self.agents.error().unwrap().to_string()),
            _ => return,
        };
        match result {
            Ok(()) => {
                self.cancel_editor_project();
                self.update_editor_open_feedback();
            }
            Err(error) => {
                if self.editor_project_error.as_ref() != Some(&error) {
                    if let Some(workspace) = self.agents.workspace(&path) {
                        workspace.panel.borrow_mut().save_error = Some(error.clone());
                    }
                    self.editor_project_error = Some(error);
                    self.update_editor_open_feedback();
                }
            }
        }
    }

    pub(super) fn apply_agent_workspace_intents(&mut self, intents: Vec<AgentWorkspaceIntent>) {
        for intent in intents {
            match intent {
                AgentWorkspaceIntent::Back => {
                    self.cancel_editor_project();
                    let laboratory = self.agents.selected_lab_target()
                        .map(|(path, session, _)| (path.to_owned(), session));
                    if let Some((path, session)) = laboratory {
                        if let Some(editor) = self.laboratory_editor_mut(&path, session) {
                            editor.lab.leave();
                        }
                        if self.project.as_ref().is_some_and(|project| project.project_type == ProjectType::Electronics) {
                            self.agents_open = false;
                            if let Some(runtime) = self.runtime.as_mut() {
                                runtime.input_router_mut().cancel_all();
                                if let Some(surface) = self.agent_surface.as_mut() {
                                    surface.reset(runtime.input_router_mut());
                                }
                                runtime.request_canvas_frame();
                            }
                            if let Some(window) = self.window.as_ref() { window.request_redraw(); }
                        } else {
                            self.request_editor_project(path);
                        }
                        continue;
                    }
                    if self.project.is_some() {
                        self.return_to_hub(false);
                        continue;
                    }
                    self.agents_open = false;
                    if let Some(runtime) = self.runtime.as_mut() {
                        if let Some(surface) = self.agent_surface.as_mut() {
                            surface.reset(runtime.input_router_mut());
                        }
                        runtime.request_canvas_frame();
                    }
                }
                AgentWorkspaceIntent::Settings => {
                    self.pending_settings_open = Some(SettingsSection::Ai)
                }
                AgentWorkspaceIntent::Window(command) => self.pending_window_commands.push(command),
                AgentWorkspaceIntent::OpenEditor(path) => self.request_editor_project(path),
                AgentWorkspaceIntent::StartAllProjects => {
                    let projects = self.agent_projects();
                    self.agents.start_all(projects.into_iter().map(|p| p.path));
                }
                AgentWorkspaceIntent::CloseProjects(target) => self.close_agent_projects(target),
                AgentWorkspaceIntent::Laboratory(command) => self.apply_laboratory_command(&command),
                AgentWorkspaceIntent::LeaveLaboratory(path, session) => {
                    if let Some(editor) = self.laboratory_editor_mut(&path, session) {
                        editor.lab.leave();
                    }
                }
            }
        }
    }

    pub(super) fn focus_mounted_laboratory(&mut self) {
        let Some(project) = self.project.as_ref() else { return };
        if project.project_type != ProjectType::Electronics { return; }
        let Some(workspace) = self.agents.workspace(&project.path) else { return };
        let id = self.workbench.as_ref().map(NativeGameWorkbench::active_session_id)
            .unwrap_or(workspace.registry.active_session);
        if let Some(document) = workspace.documents.get(&id.0.to_string()) {
            let mut panel = workspace.panel.borrow_mut();
            if panel.document_id().is_some_and(|current| current != id) {
                use crate::panels::ai_chat::AgentAction;
                let action = panel.history.sessions.iter().position(|session| session.document_id == Some(id.0))
                    .map(AgentAction::SelectSession).unwrap_or(AgentAction::NewChat);
                panel.apply_action(action, &mut self.settings_state);
            }
            panel.bind_document(id, &document.session.name);
        }
    }

    fn laboratory_editor_mut(&mut self, path: &Path, id: SessionId) -> Option<&mut NativeElectronicsEditor> {
        if self.project.as_ref().is_some_and(|project| project.path == path)
            && self.workbench.as_ref().map(NativeGameWorkbench::active_session_id)
                .or_else(|| self.agents.workspace(path).map(|workspace| workspace.registry.active_session)) == Some(id)
        {
            return self.electronics_editor.as_mut();
        }
        self.agents.workspace_mut(path)?.documents.get_mut(&id.0.to_string())?.electronics.as_mut()
    }

    pub(super) fn apply_laboratory_command(&mut self, command: &str) {
        let Some((path, session, schematic)) = self.agents.selected_lab_target()
            .map(|(path, session, schematic)| (path.to_owned(), session, schematic.to_owned())) else { return };
        let language = self.settings_state.language;
        let Some(editor) = self.laboratory_editor_mut(&path, session) else { return };
        editor.prepare_lab(&path, &schematic);
        match command {
            "simulate" => {
                editor.lab.error = editor.start_lab_simulation(None, true).err();
                editor.lab.touch();
            }
            "cancel" => editor.cancel_analysis(),
            "ota" => {
                if let Some(file) = crate::folder_picker::pick_firmware(&raf_core::i18n::t("lab.ota", language)) {
                    if let Err(error) = editor.lab.prepare_ota(file) {
                        editor.lab.error = Some(error);
                        editor.lab.touch();
                    }
                }
            }
            "ota-save" => {
                if let Some(folder) = crate::folder_picker::pick_output_folder(&raf_core::i18n::t("lab.ota_save", language)) {
                    editor.lab.export_ota(folder);
                }
            }
            _ => editor.lab.action(command),
        }
        if let Some(runtime) = self.runtime.as_mut() { runtime.request_ui_frame(); }
        if let Some(window) = self.window.as_ref() { window.request_redraw(); }
    }

    pub(super) fn poll_electronics_laboratory(&mut self) {
        let visible = self.agents_open && !self.settings_open;
        let mut changed = self.agents.poll_laboratories(visible);
        if let Some(editor) = self.electronics_editor.as_mut() {
            let target = visible.then(|| self.agents.selected_lab_target()).flatten()
                .filter(|(root, id, _)| {
                    self.project.as_ref().is_some_and(|project| project.path == *root)
                        && self.workbench.as_ref().is_some_and(|workbench| workbench.active_session_id() == *id)
                });
            if let Some((root, _, schematic)) = target { editor.prepare_lab(root, schematic); }
            if target.is_some() || editor.lab.pending() || editor.lab.dc_requested {
                changed |= editor.poll_lab(target.is_some());
            }
        }
        if changed && self.agents_open {
            if let Some(runtime) = self.runtime.as_mut() { runtime.request_ui_frame(); }
            if let Some(window) = self.window.as_ref() { window.request_redraw(); }
        }
    }

    fn close_agent_projects(&mut self, target: Option<std::path::PathBuf>) {
        self.cancel_editor_project();
        let close_editor = self.project.as_ref().is_some_and(|project| {
            target.as_ref().is_none_or(|path| {
                self.agents
                    .workspace(path)
                    .is_some_and(|w| w.project.path == project.path)
            })
        });
        if close_editor {
            self.return_to_hub(false);
            // A failed save must retain the editor's authoritative document.
            if self.project.is_some() {
                return;
            }
        }
        if let Some(path) = target {
            self.agents.request(path.clone());
            self.agents.close_project(&path);
        } else {
            self.agents.close_all();
        }
        // A parked editor must not keep the closed project's shared panel,
        // asset jobs and history alive through its old dock bindings.
        if self.project.is_none() {
            self.workbench = None;
        }
        self.agents_open = true;
        if let Some(runtime) = self.runtime.as_mut() {
            if let Some(surface) = self.agent_surface.as_mut() {
                surface.reset(runtime.input_router_mut());
            }
            runtime.request_ui_frame();
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub(super) fn poll_agent_service(&mut self) {
        if let (Some(project), Some(workbench)) = (self.project.as_ref(), self.workbench.as_ref()) {
            if let Some(workspace) = self.agents.workspace_mut(&project.path) {
                workspace.registry = workbench.agent_sessions();
            }
        }
        let attached = self.project.as_ref().and_then(|project| {
            self.workbench
                .as_ref()
                .map(|w| (project.path.as_path(), w.active_session_id()))
        });
        let reserved = self.pending_editor_project.as_ref().and_then(|path| {
            self.agents.workspace(path).map(|workspace| {
                (
                    path.clone(),
                    self.pending_editor_document
                        .unwrap_or(workspace.registry.active_session),
                )
            })
        });
        let mut changed = self.agents.poll(
            &self.settings_state,
            attached,
            Some(&mut self.attached_ledger),
            reserved.as_ref().map(|(root, id)| (root.as_path(), *id)),
        );
        self.poll_editor_open();
        let live = self.project.is_some()
            && self
                .workbench
                .as_ref()
                .is_some_and(NativeGameWorkbench::has_agent_work);
        if live
            && self.project.is_some()
            && self.last_live_agent_poll.elapsed() >= Duration::from_millis(40)
        {
            self.last_live_agent_poll = Instant::now();
            if let (Some(project), Some(runtime), Some(workbench)) = (
                self.project.as_ref(),
                self.runtime.as_mut(),
                self.workbench.as_mut(),
            ) {
                workbench.set_engine_settings(self.settings_state.clone());
                if !runtime.game_viewport().transform_drag_active() {
                    let session = self
                        .agents
                        .workspace(&project.path)
                        .and_then(|workspace| {
                            workspace
                                .registry
                                .sessions
                                .iter()
                                .find(|session| session.id == workbench.active_session_id())
                        })
                        .cloned();
                    if let Some(session) = session {
                        let visual_before = workbench.agent_visual_revision();
                        let revision_before = self.attached_ledger.revision();
                        let (viewport, graphics, history, nodes) =
                            runtime.agent_document_ports_mut();
                        let actions = workbench.poll_agent(
                            &mut self.scene,
                            nodes,
                            &session,
                            viewport,
                            if self.agents_open {
                                None
                            } else {
                                Some(graphics)
                            },
                            self.electronics_editor.as_mut(),
                            Some(project),
                            &mut self.attached_ledger,
                            history,
                        );
                        let scene_changed = workbench.take_agent_canvas_changed();
                        changed |= visual_before != workbench.agent_visual_revision()
                            || scene_changed
                            || !actions.is_empty();
                        if scene_changed {
                            self.attached_ledger.mark_document(
                                crate::scene_history::scene_fingerprint(&self.scene),
                            );
                            runtime.request_document_frame();
                        }
                        for action in actions {
                            match action {
                                crate::agent_executor::AgentEditorAction::Undo => {
                                    runtime.undo(&mut self.scene)
                                }
                                crate::agent_executor::AgentEditorAction::Redo => {
                                    runtime.redo(&mut self.scene)
                                }
                            }
                        }
                        if revision_before != self.attached_ledger.revision() {
                            if let Err(error) =
                                self.attached_ledger.persist_for_project(&project.path)
                            {
                                tracing::warn!(%error, "Agent ledger save failed");
                            }
                        }
                    }
                }
            }
        }
        if changed {
            if let Some(runtime) = self.runtime.as_mut() {
                runtime.request_ui_frame();
            }
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
        }
    }

    pub(super) fn detach_agent_document(&mut self) -> Result<(), String> {
        let Some(project) = self.project.clone() else {
            return Ok(());
        };
        self.save_current_documents()?;
        let Some(runtime) = self.runtime.as_mut() else {
            return Ok(());
        };
        let session_id = self.workbench.as_ref().map(|w| w.active_session_id());
        let Some(workspace) = self.agents.workspace_mut(&project.path) else {
            return Ok(());
        };
        let Some(document) =
            session_id.and_then(|id| workspace.documents.get_mut(&id.0.to_string()))
        else {
            return Ok(());
        };
        document.scene = std::mem::take(&mut self.scene);
        document.nodes = runtime.take_agent_node_graph();
        document.history = runtime.take_agent_history();
        document
            .viewport
            .set_selected_ids(runtime.game_viewport().selected.clone());
        document.electronics = self.electronics_editor.take();
        workspace.ledger = std::mem::take(&mut self.attached_ledger);
        Ok(())
    }

    pub(super) fn save_agent_document(&mut self) -> Option<Result<(), String>> {
        let project = self.project.as_ref()?;
        if project.project_type == ProjectType::Electronics && self.electronics_editor.is_none() {
            return Some(Err("agent_workspace.document_missing".into()));
        }
        let runtime = self.runtime.as_ref()?;
        let session_id = self.workbench.as_ref()?.active_session_id();
        let workspace = self.agents.workspace(&project.path)?;
        let document = workspace.documents.get(&session_id.0.to_string())?;
        let mut save = document.save.borrow_mut();
        if let Err(error) = save.wait_for_save() {
            return Some(Err(error));
        }
        save.submit(DocumentSave::capture(
            project,
            &document.session,
            &self.scene,
            runtime.node_graph(),
            self.electronics_editor.as_ref(),
            &self.attached_ledger,
        ));
        let result = save.wait_for_save();
        workspace.panel.borrow_mut().saved_revision = save.saved_revision;
        Some(result)
    }

    pub(super) fn refresh_agent_save_receipt(&mut self) {
        if let (Some(project), Some(workbench)) = (self.project.as_ref(), self.workbench.as_ref()) {
            if let Some(workspace) = self.agents.workspace(&project.path) {
                if let Some(document) = workspace
                    .documents
                    .get(&workbench.active_session_id().0.to_string())
                {
                    if !document.save.borrow().is_pending() {
                        if let Ok(files) = read_document_files(project, &document.session) {
                            document.save.borrow_mut().expected = files;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loaded_application() -> (NativeEditorApplication, std::path::PathBuf, Project) {
        loaded_application_for(ProjectType::Game)
    }

    fn loaded_application_for(kind: ProjectType) -> (NativeEditorApplication, std::path::PathBuf, Project) {
        let root = std::env::temp_dir().join(format!("raf-editor-route-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let project = Project::create("Route", kind, &root).unwrap();
        let mut app = NativeEditorApplication::default();
        app.agents.request(project.path.clone());
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.agents.loading() && Instant::now() < deadline {
            app.agents.poll_loads(&app.settings_state);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(
            app.agents.selected_workspace().is_some(),
            "{:?}",
            app.agents.error()
        );
        (app, root, project)
    }

    #[test]
    fn electronics_laboratory_returns_to_the_mounted_document() {
        let (mut app, root, project) = loaded_application_for(ProjectType::Electronics);
        app.request_editor_project(project.path.clone());
        app.poll_editor_open();
        let session = app.agents.selected_workspace().unwrap().registry.active_session;
        let fingerprint = app.electronics_editor.as_ref().unwrap().lab_fingerprint();
        app.open_agents(None);
        app.focus_mounted_laboratory();
        app.apply_laboratory_command("device");
        assert!(app.agents_open);
        assert!(app.electronics_editor.as_ref().unwrap().lab.view == crate::electronics_lab::LabView::Device);
        app.apply_agent_workspace_intents(vec![AgentWorkspaceIntent::Back]);
        assert!(!app.agents_open);
        assert_eq!(app.project.as_ref().unwrap().id, project.id);
        assert_eq!(app.agents.selected_workspace().unwrap().registry.active_session, session);
        assert_eq!(app.electronics_editor.as_ref().unwrap().lab_fingerprint(), fingerprint);
        drop(app);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn agent_workspace_hub_open_ignores_chat_target_and_opens_editor_directly() {
        let (mut app, root, project) = loaded_application();
        // A stale saved conversation must not hijack normal Hub navigation.
        app.agents
            .selected_workspace()
            .unwrap()
            .panel
            .borrow_mut()
            .history
            .active_session_mut()
            .unwrap()
            .document_id = Some(uuid::Uuid::new_v4());
        app.request_editor_project(project.path.clone());
        assert!(!app.agents_open);
        assert!(app.agent_surface.is_none());
        assert!(app.pending_editor_document.is_none());
        app.poll_editor_open();
        assert_eq!(
            app.project.as_ref().map(|project| project.id),
            Some(project.id)
        );
        assert!(!app.agents_open);
        assert!(app.pending_editor_project.is_none());
        assert!(!app.scene.all_live_ids().is_empty());
        drop(app);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn agent_workspace_preview_borrows_mounted_scene_and_never_falls_back_to_another_world() {
        let (app, directory, project) = loaded_application();
        let workspace = app.agents.selected_workspace().unwrap();
        let id = workspace.registry.active_session;
        let target = PreviewTarget {
            root: workspace.project.path.clone(),
            document: id,
        };
        let mut live = SceneGraph::new();
        live.add_root_folder("Mounted live scene");
        let (_, resolved) = preview_document(
            &app.agents,
            &target,
            Some((workspace.project.path.as_path(), id, &live)),
        )
        .unwrap();
        assert!(std::ptr::eq(resolved, &live));
        let (_, parked) = preview_document(&app.agents, &target, None).unwrap();
        assert!(std::ptr::eq(
            parked,
            &workspace.documents[&id.0.to_string()].scene
        ));
        let missing = PreviewTarget {
            root: project.path,
            document: SessionId(uuid::Uuid::new_v4()),
        };
        assert!(preview_document(
            &app.agents,
            &missing,
            Some((target.root.as_path(), id, &live))
        )
        .is_none());
        drop(app);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn agent_workspace_editor_cannot_mount_a_closing_host() {
        let (mut app, root, project) = loaded_application();
        let workspace = app.agents.selected_workspace().unwrap();
        let id = workspace.registry.active_session;
        let save = workspace.documents[&id.0.to_string()].save.clone();
        save.borrow_mut().error = Some("agent_workspace.save_worker_failed".into());
        app.agents.close_project(&project.path);
        app.request_editor_project(project.path.clone());
        app.poll_editor_open();
        assert!(app.project.is_none());
        assert_eq!(
            app.editor_project_error.as_deref(),
            Some("agent_workspace.project_closing")
        );
        save.borrow_mut().error = None;
        drop(save);
        drop(app);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn agent_workspace_open_editor_reports_save_failure_and_completes_after_retry() {
        let (mut app, root, project) = loaded_application();
        let workspace = app.agents.selected_workspace().unwrap();
        let session = workspace.registry.active().unwrap();
        workspace
            .panel
            .borrow_mut()
            .bind_document(session.id, &session.name);
        let save = workspace.documents[&session.id.0.to_string()].save.clone();
        save.borrow_mut().error = Some("agent_workspace.save_worker_failed".into());
        app.agents_open = true;
        app.request_editor_project(project.path.clone());
        app.poll_editor_open();
        assert_eq!(
            app.editor_project_error.as_deref(),
            Some("agent_workspace.save_worker_failed")
        );
        assert!(app.project.is_none());
        save.borrow_mut().error = None;
        app.poll_editor_open();
        assert_eq!(
            app.project.as_ref().map(|project| project.id),
            Some(project.id)
        );
        assert!(!app.agents_open);
        assert!(app.pending_editor_project.is_none());
        drop(save);
        drop(app);
        std::fs::remove_dir_all(root).unwrap();
    }
}
