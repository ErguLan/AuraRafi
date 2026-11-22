//! Native editor document lifecycle: sessions, dirty state, saving, and the
//! Hub return path.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::{Duration, Instant};

use raf_core::project::{Project, ProjectType};
use raf_core::scene::SceneGraph;
use raf_core::session::SessionId;
use raf_core::TransactionLedger;

use crate::electronics_controller::NativeElectronicsEditor;
use crate::native_editor_runtime::NativeEditorRuntime;
use crate::native_project_controller::{
    initial_node_graph_for_session, initial_scene_for_session, project_capabilities,
    save_project_document, save_project_document_for_session,
};
use crate::native_studio::{remember_project, NativeStudioSurface};
use crate::panels::exit_confirmation_surface_host::ExitConfirmationAction;

use super::layout::project_layout_request;
use super::{native_environment, native_palette, NativeEditorApplication};

pub(super) fn switch_session_documents(
    project: &Project,
    previous_session: SessionId,
    next_session: SessionId,
    scene: &mut SceneGraph,
    runtime: &mut NativeEditorRuntime,
    electronics_editor: &mut Option<NativeElectronicsEditor>,
    attached_ledger: &mut TransactionLedger,
    observed_scene_revision: &mut u64,
    observed_scene_render_fingerprint: &mut u64,
) -> Result<(), String> {
    if previous_session == next_session {
        return Ok(());
    }

    // The registry has already been persisted by the session command. Save
    // the live document explicitly to the old session before loading the new
    // one; using the registry's active session here would write the old scene
    // into the newly selected directory.
    save_project_document_for_session(project, previous_session, scene, runtime.node_graph())?;
    if project.project_type == ProjectType::Electronics {
        if let Some(editor) = electronics_editor.as_mut() {
            editor.save_to_session(project, previous_session)?;
        }
    }

    *scene = initial_scene_for_session(project, next_session);
    runtime.reset_document();
    runtime.set_node_graph(initial_node_graph_for_session(project, next_session));
    if project.project_type == ProjectType::Electronics {
        *electronics_editor = Some(NativeElectronicsEditor::from_project(project));
    }

    *attached_ledger = TransactionLedger::load_for_project(
        &project.path,
        crate::scene_history::scene_fingerprint(scene),
    );
    *observed_scene_revision = scene.document_revision();
    *observed_scene_render_fingerprint = scene.render_fingerprint();
    if let Err(error) = attached_ledger.persist_for_project(&project.path) {
        tracing::warn!(%error, "session switch ledger persistence failed");
    }
    Ok(())
}

pub(super) fn open_project_folder(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let target = if path.is_dir() {
            path.to_path_buf()
        } else {
            path.parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| path.to_path_buf())
        };
        std::process::Command::new("explorer.exe")
            .arg(target)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("could not open project folder: {error}"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = path;
        Err("opening the project folder is only implemented for Windows".to_string())
    }
}

impl NativeEditorApplication {
    pub(super) fn elapsed_seconds(&self) -> f64 {
        self.started_at.elapsed().as_secs_f64()
    }

    fn document_fingerprint(&self) -> Option<u64> {
        let runtime = self.runtime.as_ref()?;
        let mut hasher = DefaultHasher::new();
        crate::scene_history::scene_fingerprint(&self.scene).hash(&mut hasher);
        format!("{:?}", runtime.node_graph()).hash(&mut hasher);
        if let Some(editor) = self.electronics_editor.as_ref() {
            ron::ser::to_string(editor.schematic())
                .unwrap_or_default()
                .hash(&mut hasher);
            ron::ser::to_string(editor.pcb())
                .unwrap_or_default()
                .hash(&mut hasher);
        }
        Some(hasher.finish())
    }

    pub(super) fn mark_document_saved(&mut self) {
        self.saved_document_fingerprint = self.document_fingerprint();
        self.last_auto_save_at = Instant::now();
    }

    pub(super) fn has_unsaved_changes(&self) -> bool {
        self.project.is_some()
            && self
                .saved_document_fingerprint
                .zip(self.document_fingerprint())
                .is_some_and(|(saved, current)| saved != current)
    }

    pub(super) fn auto_save_deadline(&self) -> Option<Instant> {
        self.project.as_ref()?;
        let interval = self
            .settings_state
            .auto_save_interval_seconds
            .clamp(30, 600);
        // Electronics authoring produces long, unrecoverable stretches: a user
        // can move parts and route traces for minutes without ever triggering a
        // scene checkpoint. Halving the interval there caps the exposure without
        // touching the Game path or the persisted setting.
        let interval = if self.is_electronics_project() {
            (interval / 2).clamp(15, 600)
        } else {
            interval
        };
        Some(self.last_auto_save_at + Duration::from_secs(u64::from(interval)))
    }

    fn is_electronics_project(&self) -> bool {
        self.project
            .as_ref()
            .is_some_and(|project| project.project_type == ProjectType::Electronics)
    }

    /// Pushes a localized failure into the live Electronics surface so nothing
    /// depends on the log alone.
    pub(super) fn push_electronics_error(&mut self, key: &str) {
        if let Some(editor) = self.electronics_editor.as_mut() {
            editor.push_surface_error(raf_core::i18n::t(key, self.settings_state.language));
        }
    }

    pub(super) fn maybe_auto_save(&mut self) {
        let Some(deadline) = self.auto_save_deadline() else {
            return;
        };
        if Instant::now() < deadline || !self.has_unsaved_changes() {
            return;
        }

        // Advance the next attempt before touching the filesystem. A failed
        // save must not spin the event loop or repeatedly write a broken
        // document on every wake-up.
        self.last_auto_save_at = Instant::now();
        match self.save_current_documents() {
            Ok(()) => {
                self.mark_document_saved();
                tracing::info!("native automatic document save completed");
            }
            Err(error) => {
                tracing::warn!(%error, "native automatic document save failed");
                self.push_electronics_error("electronics.error.save_failed");
            }
        }
    }

    pub(super) fn process_exit_confirmation_input(&mut self) -> Option<ExitConfirmationAction> {
        if !self.exit_confirmation_surface.is_some() {
            return None;
        }
        let Some(runtime) = self.runtime.as_mut() else {
            return None;
        };
        let rect = runtime.layout().window;
        let surface = self.exit_confirmation_surface.as_mut()?;
        surface.sync(native_palette(self.settings_state.theme), rect);
        surface.set_environment(native_environment(
            &self.settings_state,
            [rect.width, rect.height],
        ));
        surface.process_input(&self.input, runtime.input_router_mut())
    }

    pub(super) fn process_exit_confirmation_frame(&mut self) {
        let action = self.process_exit_confirmation_input();
        self.apply_exit_confirmation_action(action);
    }

    pub(super) fn apply_exit_confirmation_action(&mut self, action: Option<ExitConfirmationAction>) {
        let action = action.or_else(|| {
            self.input
                .snapshot()
                .key_pressed(raf_core::InputKey::Escape)
                .then_some(ExitConfirmationAction::Cancel)
        });
        let Some(action) = action else {
            if let Some(runtime) = self.runtime.as_mut() {
                runtime.request_animation_frame();
            }
            return;
        };

        match action {
            ExitConfirmationAction::Cancel => {
                self.pending_return_to_hub = None;
                self.exit_confirmation_open = false;
            }
            ExitConfirmationAction::Discard => {
                self.mark_document_saved();
                self.exit_confirmation_open = false;
            }
            ExitConfirmationAction::Save => {
                let saved = self.save_current_documents();
                match saved {
                    Ok(()) => {
                        self.mark_document_saved();
                        self.exit_confirmation_open = false;
                    }
                    Err(error) => {
                        tracing::warn!(%error, "native save before returning to Hub failed");
                        // The user is about to leave the project. A silent failure
                        // here is how unsaved work disappears.
                        self.push_electronics_error("electronics.error.save_failed");
                        if let Some(runtime) = self.runtime.as_mut() {
                            runtime.request_animation_frame();
                        }
                        return;
                    }
                }
            }
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub(super) fn save_current_documents(&mut self) -> Result<(), String> {
        if let Some(workbench)=&mut self.workbench{workbench.flush_interface_document()?;}
        if let Some(result) = self.save_agent_document() {
            return result;
        }
        let Some(project) = self.project.as_ref() else {
            return Ok(());
        };
        let Some(runtime) = self.runtime.as_ref() else {
            return Err("native editor runtime is not ready".to_string());
        };
        save_project_document(project, &self.scene, runtime.node_graph())?;
        if project.project_type == ProjectType::Electronics {
            if let Some(editor) = self.electronics_editor.as_mut() {
                editor.save(project)?;
            }
        }
        Ok(())
    }

    pub(super) fn activate_project(&mut self, project: Project) -> Result<(), String> {
        let workspace = self
            .agents
            .workspace(&project.path)
            .ok_or("agent_workspace.load_failed")?;
        if !workspace
            .documents
            .contains_key(&workspace.registry.active_session.0.to_string())
        {
            return Err("agent_workspace.document_missing".into());
        }
        self.detach_agent_document()?;
        let workspace = self
            .agents
            .workspace_mut(&project.path)
            .ok_or("agent_workspace.load_failed")?;
        let session_id = workspace.registry.active_session;
        let document = workspace
            .documents
            .get_mut(&session_id.0.to_string())
            .ok_or("agent_workspace.document_missing")?;
        let mounted_scene = std::mem::take(&mut document.scene);
        let mounted_nodes =
            std::mem::replace(&mut document.nodes, raf_nodes::NodeGraph::new("Main"));
        let mounted_history = std::mem::take(&mut document.history);
        let mounted_electronics = document.electronics.take();
        let panel = workspace.panel.clone();
        let asset_tools = workspace.assets.clone();
        let save = document.save.clone();
        let sessions = workspace.registry.clone();
        let mounted_ledger = std::mem::take(&mut workspace.ledger);
        self.agents_open = false;
        let project_type = project.project_type;
        self.exit_confirmation_open = false;
        self.pending_return_to_hub = None;
        self.settings_restore_focus = None;
        self.runtime_notice = None;

        // The Hub and the editor share the same native input router. A click
        // that opens a project can leave the Hub's focused control owning the
        // keyboard for the rest of that frame. The Hub is no longer processed
        // after this transition, so release its owner before the viewport
        // starts consuming the snapshot; otherwise WASD/QE are silently
        // filtered out by ViewportInputFrame.
        let studio_owner = self.studio.as_ref().map(NativeStudioSurface::owner);
        if let (Some(runtime), Some(owner)) = (self.runtime.as_mut(), studio_owner) {
            runtime.input_router_mut().cancel_owner(owner);
        }
        if let (Some(workbench), Some(runtime)) = (self.workbench.as_mut(), self.runtime.as_mut()) {
            workbench.reset_input_state(runtime.input_router_mut());
        }
        // Headless domain tests/preparation are not user-visible editor opens.
        if self.window.is_some() {
            remember_project(&project);
        }
        self.scene = mounted_scene;
        self.attached_ledger = mounted_ledger;
        self.observed_scene_revision = self.scene.document_revision();
        self.observed_scene_render_fingerprint = self.scene.render_fingerprint();
        self.persist_attached_ledger_for(&project);
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.reset_document();
            runtime.set_project_type(project_type);
            runtime.set_node_graph(mounted_nodes);
            runtime.install_agent_history(mounted_history);
        }
        self.ensure_workbench();
        if let Some(workbench) = self.workbench.as_mut() {
            workbench.set_runtime_notice(None);
            workbench.bind_agent_workspace(panel, asset_tools, save);
            workbench.set_agent_sessions(&project, sessions);
            workbench.set_engine_settings(self.settings_state.clone());
            workbench.set_project_info(project.name.clone(), project.project_type);
        }
        self.project = Some(project);
        if project_type == ProjectType::Electronics {
            self.ensure_electronics_canvas();
        } else {
            self.electronics_canvas = None;
        }
        let allow_gpu_features = self
            .project
            .as_ref()
            .is_some_and(|project| project.settings.allow_gpu_features);
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.apply_engine_settings(&self.settings_state, allow_gpu_features);
            if let Some(project) = self.project.as_ref() {
                runtime.apply_project_settings(&project.settings);
                runtime.set_project_asset_root(Some(project.path.as_path()));
            }
        }
        self.electronics_editor = mounted_electronics;
        if let Some(editor) = self.electronics_editor.as_mut() {
            editor.apply_engine_settings(&self.settings_state);
            if let Some(project) = self.project.as_ref() {
                editor.apply_project_settings(&project.settings);
            }
        }
        self.electronics_frame_key = None;
        if let Some(runtime) = self.runtime.as_mut() {
            let logical_size = runtime.layout().window.logical_size();
            let layout = project_layout_request(
                project_type,
                [logical_size[0] as f32, logical_size[1] as f32],
                self.project.as_ref(),
            );
            runtime.set_layout_request(layout);
        }
        if let Some(project) = self.project.as_ref() {
            self.attached_host.update_project(
                Some(project),
                self.attached_ledger.revision(),
                None,
                None,
                project_capabilities(project.project_type),
            );
        }
        self.mark_document_saved();
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.input_router_mut().cancel_all();
            runtime.request_canvas_frame();
            runtime.request_ui_frame();
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
        Ok(())
    }

    pub(super) fn duplicate_project(&self, path: &Path) -> Result<(), String> {
        let source = Project::load(path).map_err(|error| error.to_string())?;
        let parent = source
            .path
            .parent()
            .ok_or_else(|| "project has no parent directory".to_string())?;
        let name = unique_duplicate_name(parent, &source.name);
        let duplicate = Project::create(&name, source.project_type, parent)
            .map_err(|error| format!("create duplicate: {error}"))?;
        copy_project_contents(&source.path, &duplicate.path)
            .map_err(|error| format!("copy duplicate contents: {error}"))?;
        remember_project(&duplicate);
        Ok(())
    }

    pub(super) fn return_to_hub(&mut self, open_create: bool) {
        if let Err(error) = self.detach_agent_document() {
            if let Some(workspace) = self.agents.selected_workspace() {
                workspace.panel.borrow_mut().save_error = Some(error);
            }
            self.open_agents(None);
            return;
        }
        self.agents_open = false;
        self.game_runtime.stop_all();
        self.runtime_notice = None;
        if let Some(workbench) = &mut self.workbench {
            workbench.set_runtime_notice(None);
        }
        self.settings_open = false;
        self.settings_restore_focus = None;
        self.pending_settings_open = None;
        self.exit_confirmation_open = false;

        // The workbench stops receiving frames while the Hub is active. Do
        // not let a focused editor control keep the shared keyboard capture
        // alive across that surface transition.
        if let (Some(workbench), Some(runtime)) = (self.workbench.as_mut(), self.runtime.as_mut()) {
            workbench.reset_input_state(runtime.input_router_mut());
        }
        self.project = None;
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.set_project_asset_root(None);
        }
        self.attached_ledger = TransactionLedger::new();
        self.electronics_editor = None;
        self.electronics_canvas = None;
        self.electronics_frame_key = None;
        self.ensure_studio();
        if let Some(studio) = self.studio.as_mut() {
            studio.reset_for_hub(open_create);
        }
        self.attached_host.update_project(
            None,
            self.attached_ledger.revision(),
            None,
            None,
            Vec::new(),
        );
        self.saved_document_fingerprint = None;
        // Keep the document/runtime alive until the next project is opened.
        // This makes the Hub visible immediately and avoids dropping the
        // active render document in the same event turn as the exit command.
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub(super) fn sync_attached_document_revision(&mut self) {
        let Some(project_path) = self.project.as_ref().map(|project| project.path.clone()) else {
            return;
        };
        let scene_revision = self.scene.document_revision();
        let render_fingerprint = self.scene.render_fingerprint();
        if self.observed_scene_revision == scene_revision
            && self.observed_scene_render_fingerprint == render_fingerprint
        {
            return;
        }
        self.observed_scene_revision = scene_revision;
        self.observed_scene_render_fingerprint = render_fingerprint;
        let fingerprint = crate::scene_history::scene_fingerprint(&self.scene);
        if self.attached_ledger.observe_document(fingerprint) {
            if let Err(error) = self.attached_ledger.persist_for_project(&project_path) {
                tracing::warn!(
                    %error,
                    path = %project_path.display(),
                    "attached agent state persistence failed"
                );
            }
            self.attached_host
                .update_revision(self.attached_ledger.revision());
        }
    }

    pub(super) fn persist_attached_ledger(&self) {
        if let Some(project) = self.project.as_ref() {
            self.persist_attached_ledger_for(project);
        }
    }

    fn persist_attached_ledger_for(&self, project: &Project) {
        if let Err(error) = self.attached_ledger.persist_for_project(&project.path) {
            tracing::warn!(%error, path = %project.path.display(), "attached agent state persistence failed");
        }
    }

    pub(super) fn apply_deferred_persistence(
        &mut self,
        project_save_requested: bool,
        electronics_surface_switched: bool,
        linear_save_requested: bool,
        persist_agent_settings: bool,
    ) {
        if project_save_requested {
            let project_result = self.save_current_documents();
            let project_saved = match project_result {
                Ok(()) => {
                    if let Some(project) = self.project.as_ref() {
                        tracing::info!(path = %project.path.display(), "native project and scene saved");
                    }
                    true
                }
                Err(error) => {
                    tracing::warn!(%error, "native project save failed");
                    false
                }
            };
            if !project_saved {
                self.push_electronics_error("electronics.error.project_save_failed");
            }
            if project_saved {
                self.pending_document_saved = true;
            } else {
                self.pending_document_save_failed = true;
            }
        }

        if electronics_surface_switched && self.has_unsaved_changes() {
            // Risk moment: the PCB projection was just materialized. Persisting it
            // here means a crash right after the tab switch cannot lose the board.
            match self.save_current_documents() {
                Ok(()) => {
                    self.mark_document_saved();
                    tracing::info!("native Electronics save after surface switch completed");
                }
                Err(error) => {
                    tracing::warn!(
                        %error,
                        "native Electronics save after surface switch failed"
                    );
                    self.push_electronics_error("electronics.error.save_failed");
                }
            }
        }

        if linear_save_requested && self.has_unsaved_changes() {
            match self.save_current_documents() {
                Ok(()) => {
                    self.mark_document_saved();
                    tracing::info!("native linear project save completed");
                }
                Err(error) => {
                    tracing::warn!(%error, "native linear project save failed");
                    self.push_electronics_error("electronics.error.save_failed");
                }
            }
        }

        if persist_agent_settings {
            self.apply_committed_settings();
            self.persist_settings();
        }
    }
}

fn unique_duplicate_name(parent: &Path, source_name: &str) -> String {
    let base = format!("{source_name} Copy");
    if !parent.join(&base).exists() {
        return base;
    }
    for index in 2..=10_000 {
        let candidate = format!("{source_name} Copy {index}");
        if !parent.join(&candidate).exists() {
            return candidate;
        }
    }
    format!("{source_name} Copy {}", uuid::Uuid::new_v4())
}

fn copy_project_contents(source: &Path, destination: &Path) -> Result<(), String> {
    let entries = std::fs::read_dir(source).map_err(|error| error.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name();
        if name == Project::META_FILE || name == "recent_projects.ron" {
            continue;
        }
        let source_path = entry.path();
        let destination_path = destination.join(&name);
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_dir() {
            copy_directory(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            std::fs::copy(&source_path, &destination_path).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn copy_directory(source: &Path, destination: &Path) -> Result<(), String> {
    std::fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    for entry in std::fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_dir() {
            copy_directory(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            std::fs::copy(&source_path, &destination_path).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}
