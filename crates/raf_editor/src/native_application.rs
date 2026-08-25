//! Winit application handler for the RafUI/ApiGraphicBasic editor path.
//!
//! This module is the native editor entry point. Winit owns the window loop,
//! RafUI owns retained document surfaces, and ApiGraphicBasic owns composition.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use raf_core::config::{EngineSettings, Language};
use raf_core::project::{Project, ProjectType};
use raf_core::scene::SceneGraph;
use raf_core::TransactionLedger;
use raf_render::api_graphic_basic::ui_surface::{
    NativeUiInputBridge, NativeUiWindowConfig, StudioUiPalette, UiAction, UiDispatchedAction,
};
use raf_render::api_graphic_basic::NativeEditorCompositor;
use raf_ui::{UiResizeEdge, UiWindowCommand};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

use crate::attached::AttachedCommandHost;
use crate::editor_layout::EditorLayoutRequest;
use crate::electronics_controller::NativeElectronicsEditor;
use crate::native_attached_executor::poll_attached_commands;
use crate::native_editor_commands::apply_workbench_intents;
use crate::native_editor_runtime::NativeEditorRuntime;
use crate::native_electronics::NativeElectronicsCanvas;
use crate::native_project_controller::{
    game_capabilities, initial_node_graph, initial_project, initial_scene, save_project_document,
};
use crate::native_studio::{
    forget_project, remember_project, NativeStudioIntent, NativeStudioSurface,
};
use crate::native_workbench::NativeGameWorkbench;
use crate::panels::exit_confirmation_surface_host::{
    ExitConfirmationAction, ExitConfirmationSurfaceHost,
};
use crate::panels::loading_surface::LoadingSurfaceHost;
use crate::panels::settings_surface_host::SettingsSurfaceHost;
use crate::settings_surface::SettingsSection;

const NATIVE_LOADING_SECONDS: f64 = 1.15;

pub fn run_native() -> Result<(), String> {
    // A user-event proxy lets attached CLI/MCP clients wake an idle editor
    // without any polling: the loop sleeps in the OS pump until wake_up().
    let event_loop = EventLoop::<()>::with_user_event()
        .build()
        .map_err(|error| format!("native event loop: {error}"))?;
    let mut application = NativeEditorApplication::default();
    application
        .attached_host
        .set_wakeup(event_loop.create_proxy());
    event_loop
        .run_app(&mut application)
        .map_err(|error| format!("native editor event loop: {error}"))
}

pub struct NativeEditorApplication {
    window: Option<Arc<Window>>,
    window_host: Option<raf_render::api_graphic_basic::ui_surface::NativeUiWindowHost>,
    compositor: Option<NativeEditorCompositor>,
    input: NativeUiInputBridge,
    runtime: Option<NativeEditorRuntime>,
    workbench: Option<NativeGameWorkbench>,
    studio: Option<NativeStudioSurface>,
    settings_surface: Option<SettingsSurfaceHost>,
    exit_confirmation_surface: Option<ExitConfirmationSurfaceHost>,
    settings_state: EngineSettings,
    settings_open: bool,
    settings_section: SettingsSection,
    exit_confirmation_open: bool,
    loading: Option<LoadingSurfaceHost>,
    show_loading: bool,
    electronics_canvas: Option<NativeElectronicsCanvas>,
    electronics_editor: Option<NativeElectronicsEditor>,
    electronics_frame_key: Option<(u64, [u32; 2])>,
    scene: SceneGraph,
    project: Option<Project>,
    attached_host: AttachedCommandHost,
    attached_ledger: TransactionLedger,
    started_at: Instant,
    close_requested: bool,
    pending_window_commands: Vec<UiWindowCommand>,
    pending_settings_open: Option<SettingsSection>,
    pending_return_to_hub: Option<bool>,
    pending_project_setting: Option<(String, bool)>,
    pending_project_range: Option<(String, f32)>,
    saved_document_fingerprint: Option<u64>,
    pending_document_saved: bool,
}

impl Default for NativeEditorApplication {
    fn default() -> Self {
        let project = initial_project();
        Self {
            window: None,
            window_host: None,
            compositor: None,
            input: NativeUiInputBridge::default(),
            runtime: None,
            workbench: None,
            studio: None,
            settings_surface: None,
            exit_confirmation_surface: None,
            settings_state: EngineSettings::load(&EngineSettings::user_config_dir()),
            settings_open: false,
            settings_section: SettingsSection::Appearance,
            exit_confirmation_open: false,
            loading: None,
            show_loading: false,
            electronics_canvas: None,
            electronics_editor: None,
            electronics_frame_key: None,
            scene: initial_scene(project.as_ref()),
            project,
            attached_host: AttachedCommandHost::start(),
            attached_ledger: TransactionLedger::new(),
            started_at: Instant::now(),
            close_requested: false,
            pending_window_commands: Vec::new(),
            pending_settings_open: None,
            pending_return_to_hub: None,
            pending_project_setting: None,
            pending_project_range: None,
            saved_document_fingerprint: None,
            pending_document_saved: false,
        }
    }
}

impl NativeEditorApplication {
    fn resumed_native(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title("Proyecto Rafi")
            .with_decorations(false)
            .with_inner_size(LogicalSize::new(1440.0, 900.0))
            .with_min_inner_size(LogicalSize::new(900.0, 600.0));
        let Ok(window) = event_loop.create_window(attributes) else {
            tracing::error!("native editor window could not be created");
            event_loop.exit();
            return;
        };
        let window = Arc::new(window);
        let size = window.inner_size();
        let scale_factor = window.scale_factor() as f32;
        let host = match pollster::block_on(
            raf_render::api_graphic_basic::ui_surface::NativeUiWindowHost::create_with_config(
                window.clone(),
                NativeUiWindowConfig::default(),
            ),
        ) {
            Ok(host) => host,
            Err(error) => {
                tracing::error!(%error, "native ApiGraphicBasic host could not be created");
                event_loop.exit();
                return;
            }
        };

        let project_type = self
            .project
            .as_ref()
            .map(|project| project.project_type)
            .unwrap_or(ProjectType::Game);
        let logical_size = [
            size.width as f32 / scale_factor.max(0.25),
            size.height as f32 / scale_factor.max(0.25),
        ];
        let layout_request =
            project_layout_request(project_type, logical_size, self.project.as_ref());
        let mut runtime = NativeEditorRuntime::new(layout_request);
        runtime.set_project_type(project_type);
        runtime.resize([size.width, size.height], scale_factor);
        runtime
            .graphics_mut()
            .set_shared_graphics_context(Some(host.shared_graphics_context()));
        runtime.set_node_graph(initial_node_graph(self.project.as_ref()));

        let graphics = host.graphics_context();
        let compositor = NativeEditorCompositor::new(&graphics, [8, 11, 15, 255]);
        let workbench = NativeGameWorkbench::new(
            &graphics,
            raf_render::api_graphic_basic::ui_surface::StudioUiPalette::IndustrialDark,
            runtime.layout().window,
        );
        let studio = NativeStudioSurface::new(&graphics, runtime.layout().window);
        let settings_surface = SettingsSurfaceHost::new(
            &graphics,
            runtime.layout().window,
            raf_render::api_graphic_basic::ui_surface::StudioUiPalette::IndustrialDark,
        );
        let exit_confirmation_surface = ExitConfirmationSurfaceHost::new(
            &graphics,
            runtime.layout().window,
            raf_render::api_graphic_basic::ui_surface::StudioUiPalette::IndustrialDark,
        );
        let loading = LoadingSurfaceHost::new(
            &graphics,
            runtime.layout().window,
            raf_render::api_graphic_basic::ui_surface::StudioUiPalette::IndustrialDark,
        );
        let electronics_canvas = NativeElectronicsCanvas::new(
            &graphics,
            [10, 10, 11, 255],
            raf_render::bridge::GraphicsSurfaceKind::SchematicCanvas,
        );
        let mut workbench = workbench;
        workbench.set_engine_settings(self.settings_state.clone());
        if let Some(project) = self.project.as_ref() {
            workbench.set_project_info(project.name.clone(), project.project_type);
        }

        self.input.set_scale_factor(window.scale_factor());
        self.window = Some(window.clone());
        self.window_host = Some(host);
        self.compositor = Some(compositor);
        self.runtime = Some(runtime);
        self.workbench = Some(workbench);
        self.studio = Some(studio);
        self.settings_surface = Some(settings_surface);
        self.exit_confirmation_surface = Some(exit_confirmation_surface);
        self.loading = Some(loading);
        self.show_loading = self.project.is_none();
        self.electronics_canvas = Some(electronics_canvas);
        self.electronics_editor = self
            .project
            .as_ref()
            .map(NativeElectronicsEditor::from_project);
        if let Some(editor) = self.electronics_editor.as_mut() {
            editor.apply_engine_settings(&self.settings_state);
        }
        self.electronics_frame_key = None;
        let capabilities = game_capabilities(project_type);
        self.attached_host.update_project(
            self.project.as_ref(),
            self.attached_ledger.revision(),
            None,
            None,
            capabilities,
        );
        self.mark_document_saved();
        self.started_at = Instant::now();
        window.request_redraw();
    }

    fn elapsed_seconds(&self) -> f64 {
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

    fn mark_document_saved(&mut self) {
        self.saved_document_fingerprint = self.document_fingerprint();
    }

    fn has_unsaved_changes(&self) -> bool {
        self.project.is_some()
            && self
                .saved_document_fingerprint
                .zip(self.document_fingerprint())
                .is_some_and(|(saved, current)| saved != current)
    }

    fn process_exit_confirmation_input(&mut self) -> Option<ExitConfirmationAction> {
        if !self.exit_confirmation_surface.is_some() {
            return None;
        }
        let Some(runtime) = self.runtime.as_mut() else {
            return None;
        };
        let rect = runtime.layout().window;
        let surface = self.exit_confirmation_surface.as_mut()?;
        surface.sync(StudioUiPalette::IndustrialDark, rect);
        surface.process_input(&self.input, runtime.input_router_mut())
    }

    fn apply_exit_confirmation_action(&mut self, action: Option<ExitConfirmationAction>) {
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

    fn save_current_documents(&mut self) -> Result<(), String> {
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

    fn open_settings(&mut self, section: SettingsSection) {
        if let Some(workbench) = self.workbench.as_ref() {
            self.settings_state = workbench.engine_settings().clone();
        }
        self.settings_section = section;
        self.settings_open = true;
        if let Some(settings_surface) = self.settings_surface.as_mut() {
            settings_surface.reset_draft(&self.settings_state);
        }
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.request_animation_frame();
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    fn process_settings_input(&mut self) -> Vec<UiDispatchedAction> {
        if !self.settings_open {
            return Vec::new();
        }
        let Some(runtime) = self.runtime.as_mut() else {
            return Vec::new();
        };
        let rect = runtime.layout().window;
        let Some(settings_surface) = self.settings_surface.as_mut() else {
            return Vec::new();
        };
        settings_surface.sync(
            StudioUiPalette::IndustrialDark,
            &self.settings_state,
            self.settings_section,
            rect,
        );
        settings_surface.process_input(&self.input, runtime.input_router_mut())
    }

    fn apply_settings_actions(&mut self, actions: Vec<UiDispatchedAction>) {
        if !self.settings_open {
            return;
        }
        let mut changed = false;
        for dispatched in actions {
            match dispatched.action {
                UiAction::SetToggle { key, value } => {
                    if let Some(settings_surface) = self.settings_surface.as_mut() {
                        if crate::native_workbench::apply_settings_toggle(
                            &mut settings_surface.draft,
                            &key,
                            value,
                        ) {
                            settings_surface.mark_dirty();
                            changed = true;
                        }
                    }
                }
                UiAction::SetRange { key, value } => {
                    if let Some(settings_surface) = self.settings_surface.as_mut() {
                        if crate::native_workbench::apply_settings_range(
                            &mut settings_surface.draft,
                            &key,
                            value,
                        ) {
                            settings_surface.mark_dirty();
                            changed = true;
                        }
                    }
                }
                UiAction::SetText { key, value } => {
                    if let Some(settings_surface) = self.settings_surface.as_mut() {
                        if crate::native_workbench::apply_settings_text(
                            &mut settings_surface.draft,
                            &key,
                            &value,
                        ) {
                            settings_surface.mark_dirty();
                            changed = true;
                        }
                    }
                }
                UiAction::Command { name } => {
                    if let Some(section) = SettingsSection::from_command(&name) {
                        self.settings_section = section;
                        if let Some(settings_surface) = self.settings_surface.as_mut() {
                            settings_surface.mark_dirty();
                        }
                        changed = true;
                    } else if let Some(key) = name.strip_prefix("settings.commit_numeric:") {
                        if let Some(settings_surface) = self.settings_surface.as_mut() {
                            let text = settings_surface.text(&format!("{key}.text"));
                            if let Ok(value) = text.trim().parse::<f32>() {
                                if crate::native_workbench::apply_settings_range(
                                    &mut settings_surface.draft,
                                    key,
                                    value,
                                ) {
                                    settings_surface.mark_dirty();
                                    changed = true;
                                }
                            }
                        }
                    } else if let Some(provider_id) =
                        name.strip_prefix("settings.ai_provider.reveal.")
                    {
                        if let Some(provider) =
                            crate::native_workbench::ai_provider_from_id(provider_id)
                        {
                            if let Some(settings_surface) = self.settings_surface.as_mut() {
                                settings_surface.toggle_api_key(provider);
                            }
                            changed = true;
                        }
                    } else if let Some(provider_id) =
                        name.strip_prefix("settings.ai_provider.clear.")
                    {
                        if crate::native_workbench::ai_provider_from_id(provider_id).is_some() {
                            if let Some(settings_surface) = self.settings_surface.as_mut() {
                                let key = format!("settings.ai_provider.{}.api_key", provider_id);
                                if crate::native_workbench::apply_settings_text(
                                    &mut settings_surface.draft,
                                    &key,
                                    "",
                                ) {
                                    settings_surface.mark_dirty();
                                    changed = true;
                                }
                            }
                        }
                    } else if name == "settings.save" {
                        if let Some(settings_surface) = self.settings_surface.as_mut() {
                            self.settings_state = settings_surface.accept();
                        }
                        self.persist_settings();
                        if let Some(workbench) = self.workbench.as_mut() {
                            workbench.set_engine_settings(self.settings_state.clone());
                        }
                        if let Some(editor) = self.electronics_editor.as_mut() {
                            editor.apply_engine_settings(&self.settings_state);
                        }
                        self.settings_open = false;
                        changed = true;
                    } else if name == "settings.cancel" {
                        if let Some(settings_surface) = self.settings_surface.as_mut() {
                            settings_surface.cancel();
                        }
                        self.settings_open = false;
                        changed = true;
                    } else if let Some(settings_surface) = self.settings_surface.as_mut() {
                        if crate::native_workbench::apply_settings_command(
                            &mut settings_surface.draft,
                            &name,
                        ) {
                            settings_surface.mark_dirty();
                            changed = true;
                        }
                    }
                }
                _ => {}
            }
        }
        if self
            .input
            .snapshot()
            .key_pressed(raf_core::InputKey::Escape)
        {
            if let Some(settings_surface) = self.settings_surface.as_mut() {
                settings_surface.cancel();
            }
            self.settings_open = false;
            changed = true;
        }
        if changed {
            if let Some(runtime) = self.runtime.as_mut() {
                runtime.request_animation_frame();
            }
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
        }
    }

    fn redraw(&mut self) {
        let attached_changed = poll_attached_commands(
            &mut self.attached_host,
            &mut self.attached_ledger,
            self.runtime.as_mut(),
            &mut self.scene,
            self.electronics_editor.as_mut(),
            self.project.as_ref(),
        );
        if attached_changed {
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
        }
        let now = self.elapsed_seconds();
        self.input.set_time_seconds(now);
        let loading_active = self.show_loading && now < NATIVE_LOADING_SECONDS;
        let exit_confirmation_was_open = self.exit_confirmation_open;

        if self.exit_confirmation_open {
            let action = self.process_exit_confirmation_input();
            self.apply_exit_confirmation_action(action);
        }

        if self.project.is_none() && !loading_active {
            if self.settings_open && !self.exit_confirmation_open && !exit_confirmation_was_open {
                let settings_actions = self.process_settings_input();
                self.apply_settings_actions(settings_actions);
            }
            let intents = if self.settings_open
                || self.exit_confirmation_open
                || exit_confirmation_was_open
            {
                if let Some(runtime) = self.runtime.as_mut() {
                    runtime.request_animation_frame();
                }
                Vec::new()
            } else {
                let Some(runtime) = self.runtime.as_mut() else {
                    return;
                };
                let Some(studio) = self.studio.as_mut() else {
                    return;
                };
                studio.sync(runtime.layout());
                let intents = studio.process_input(&self.input, runtime.input_router_mut());
                let needs_ui_frame = !intents.is_empty() || studio.needs_surface_sync();
                let has_active_motion = studio.has_active_motion();
                runtime.set_continuous_ui_motion(has_active_motion);
                if needs_ui_frame || has_active_motion {
                    runtime.request_animation_frame();
                }
                intents
            };
            self.apply_studio_intents(intents);
            if self.project.is_some() {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
                self.input.begin_frame();
                return;
            }
        } else {
            if self.settings_open && !self.exit_confirmation_open && !exit_confirmation_was_open {
                let settings_actions = self.process_settings_input();
                self.apply_settings_actions(settings_actions);
            }
            let Some(runtime) = self.runtime.as_mut() else {
                return;
            };
            let mut cursor_hint = None;
            if let Some(workbench) = self.workbench.as_mut() {
                let analysis_changed = self
                    .electronics_editor
                    .as_mut()
                    .is_some_and(NativeElectronicsEditor::poll_analysis);
                let analysis_running = self
                    .electronics_editor
                    .as_ref()
                    .is_some_and(NativeElectronicsEditor::analysis_running);
                if analysis_changed || analysis_running {
                    runtime.request_animation_frame();
                }
                if let Some(project) = self.project.as_ref() {
                    workbench.set_project_info(project.name.clone(), project.project_type);
                }
                workbench.set_presented_fps(runtime.presented_fps());
                let selected = runtime.game_viewport().selected.clone();
                workbench.sync(
                    runtime.layout(),
                    &self.scene,
                    &selected,
                    runtime.history().can_undo(),
                    runtime.history().can_redo(),
                    runtime.has_clipboard(),
                    runtime.node_graph(),
                    runtime.selected_graph_node(),
                    self.project.as_ref(),
                    now,
                    self.electronics_editor.as_ref(),
                );
                synchronize_bottom_dock_layout(runtime, workbench);
                cursor_hint = Some(workbench.cursor_hint());
                if self.settings_open || self.exit_confirmation_open || exit_confirmation_was_open {
                    runtime.request_animation_frame();
                    runtime.set_continuous_ui_motion(false);
                } else {
                    let (intents, actions) = workbench.process_input(
                        &self.input,
                        runtime.input_router_mut(),
                        &self.scene,
                        &selected,
                        self.project.as_ref(),
                        |key| raf_core::i18n::t(key, Language::English),
                    );
                    let had_ui_activity = !intents.is_empty() || !actions.is_empty();
                    let mut domain_intents = Vec::new();
                    for intent in intents {
                        match intent {
                            crate::native_workbench::NativeWorkbenchIntent::Window(command) => {
                                self.pending_window_commands.push(command);
                            }
                            crate::native_workbench::NativeWorkbenchIntent::OpenSettings {
                                section,
                            } => {
                                self.pending_settings_open = Some(section);
                            }
                            crate::native_workbench::NativeWorkbenchIntent::ReturnToHub {
                                open_create,
                            } => {
                                self.pending_return_to_hub = Some(open_create);
                            }
                            crate::native_workbench::NativeWorkbenchIntent::ProjectSettingToggle {
                                key,
                                value,
                            } => {
                                self.pending_project_setting = Some((key, value));
                            }
                            crate::native_workbench::NativeWorkbenchIntent::ProjectSettingRange {
                                key,
                                value,
                            } => {
                                self.pending_project_range = Some((key, value));
                            }
                            crate::native_workbench::NativeWorkbenchIntent::Command(command) => {
                                if let Some(raw_value) = command.strip_prefix("layout.resize.bottom:")
                                {
                                    if let Ok(value) = raw_value.parse::<f32>() {
                                        workbench.set_bottom_dock_height(value);
                                    }
                                }
                                domain_intents.push(
                                    crate::native_workbench::NativeWorkbenchIntent::Command(
                                        command,
                                    ),
                                );
                            }
                            other => domain_intents.push(other),
                        }
                    }
                    if runtime.project_type() == ProjectType::Electronics {
                        if domain_intents.iter().any(|intent| {
                            matches!(
                                intent,
                                crate::native_workbench::NativeWorkbenchIntent::Command(command)
                                    if command == "sessions.reload"
                            )
                        }) {
                            self.electronics_editor = self
                                .project
                                .as_ref()
                                .map(NativeElectronicsEditor::from_project);
                            self.electronics_frame_key = None;
                        }
                        if let Some(editor) = self.electronics_editor.as_mut() {
                            for intent in &domain_intents {
                                if let crate::native_workbench::NativeWorkbenchIntent::Command(
                                    command,
                                ) = intent
                                {
                                    if command != "sessions.reload" {
                                        let response =
                                            crate::native_attached_executor::execute_native_ui_intent(
                                                editor,
                                                &mut self.attached_ledger,
                                                command,
                                            );
                                        if !response.ok {
                                            tracing::warn!(
                                                command,
                                                title = %response.title,
                                                "native Electronics UI command was rejected"
                                            );
                                        }
                                    }
                                }
                            }
                            if domain_intents.iter().any(|intent| {
                                matches!(
                                    intent,
                                    crate::native_workbench::NativeWorkbenchIntent::Command(
                                        command
                                    ) if command == crate::application_menu::command::PROJECT_SAVE
                                )
                            }) {
                                if let Some(project) = self.project.as_ref() {
                                    if let Err(error) = save_project_document(
                                        project,
                                        &self.scene,
                                        runtime.node_graph(),
                                    ) {
                                        tracing::warn!(%error, "native project save failed");
                                    }
                                    if let Err(error) = editor.save(project) {
                                        tracing::warn!(%error, "native electronics document save failed");
                                    } else {
                                        self.pending_document_saved = true;
                                    }
                                }
                            }
                        }
                    }
                    synchronize_bottom_dock_layout(runtime, workbench);
                    if runtime.project_type() == ProjectType::Game {
                        let (console_outputs, saved_document) = apply_workbench_intents(
                            runtime,
                            &mut self.scene,
                            self.project.as_ref(),
                            domain_intents,
                        );
                        self.pending_document_saved |= saved_document;
                        for output in console_outputs {
                            workbench.log_console_output(output);
                        }
                    }
                    if runtime.project_type() == ProjectType::Electronics {
                        let electronics_canvas_rect = runtime.layout().electronics_canvas();
                        if !workbench.has_interactive_hover() {
                            if let Some(editor) = self.electronics_editor.as_mut() {
                                let result = editor.process_input(
                                    self.input.snapshot(),
                                    runtime.input_router_mut(),
                                    electronics_canvas_rect,
                                );
                                if result.request_redraw {
                                    runtime.request_animation_frame();
                                }
                            }
                        }
                    }
                    let agent_actions = workbench.poll_agent(
                        &mut self.scene,
                        runtime.game_viewport_mut(),
                        self.electronics_editor.as_mut(),
                        self.project.as_ref(),
                    );
                    for action in agent_actions {
                        match action {
                            crate::agent_executor::AgentEditorAction::Undo => {
                                runtime.undo(&mut self.scene)
                            }
                            crate::agent_executor::AgentEditorAction::Redo => {
                                runtime.redo(&mut self.scene)
                            }
                        }
                    }
                    if let Some(section) = workbench.take_settings_request() {
                        self.pending_settings_open = Some(section);
                    }
                    let has_active_motion = workbench.has_active_ui_motion();
                    if had_ui_activity || workbench.needs_ui_frame() {
                        runtime.request_animation_frame();
                    }
                    runtime.set_continuous_ui_motion(
                        has_active_motion
                            || self
                                .electronics_editor
                                .as_ref()
                                .is_some_and(NativeElectronicsEditor::analysis_running),
                    );
                }
            }
            if let Some(cursor) = cursor_hint {
                if let Some(window) = self.window.as_ref() {
                    window.set_cursor(native_cursor_icon(cursor));
                }
            }
            let text_input_focused = self
                .workbench
                .as_ref()
                .is_some_and(|workbench| workbench.captures_keyboard_input());
            let dispatched = if !self.settings_open
                && !self.exit_confirmation_open
                && !exit_confirmation_was_open
                && runtime.project_type() == ProjectType::Game
            {
                runtime.dispatch_shortcuts(
                    self.input.snapshot(),
                    &mut self.scene,
                    text_input_focused,
                    self.project.is_some(),
                )
            } else {
                Vec::new()
            };
            if dispatched
                .iter()
                .any(|command| command == crate::application_menu::command::PROJECT_SAVE)
            {
                if let Some(project) = self.project.as_ref() {
                    if let Err(error) =
                        save_project_document(project, &self.scene, runtime.node_graph())
                    {
                        tracing::warn!(%error, "native project save failed");
                    } else {
                        self.pending_document_saved = true;
                        tracing::info!(path = %project.path.display(), "native project and scene saved");
                    }
                    if runtime.project_type() == ProjectType::Electronics {
                        if let Some(editor) = self.electronics_editor.as_mut() {
                            if let Err(error) = editor.save(project) {
                                tracing::warn!(%error, "native electronics document save failed");
                            } else {
                                self.pending_document_saved = true;
                                tracing::info!(path = %project.path.display(), "native electronics documents saved");
                            }
                        }
                    }
                }
            }
            if !self.settings_open
                && !self.exit_confirmation_open
                && !exit_confirmation_was_open
                && runtime.project_type() == ProjectType::Game
            {
                if let Some(workbench) = self.workbench.as_ref() {
                    let viewport = runtime.game_viewport_mut();
                    viewport.apply_engine_settings(workbench.engine_settings());
                    if let Some(project) = self.project.as_ref() {
                        viewport.building_organized = project.settings.building_style
                            == raf_core::project::BuildingStyle::Organized;
                        viewport.building_snap_step = project.settings.building_snap_step;
                    }
                }
                runtime.update_game_input(self.input.snapshot(), &mut self.scene);
            }
        }

        let Some(runtime) = self.runtime.as_mut() else {
            return;
        };
        runtime.finish_input_frame(self.input.snapshot());

        if !runtime.begin_frame(now) {
            self.input.begin_frame();
            return;
        }
        let Some(host) = self.window_host.as_mut() else {
            runtime.cancel_frame();
            self.input.begin_frame();
            return;
        };
        let Some(compositor) = self.compositor.as_mut() else {
            runtime.cancel_frame();
            self.input.begin_frame();
            return;
        };
        let target_size = [host.width(), host.height()];
        let project_open = self.project.is_some();
        let studio_model = (!project_open)
            .then(|| {
                self.studio
                    .as_ref()
                    .map(NativeStudioSurface::model_snapshot)
            })
            .flatten();
        let canvas_layer = if project_open && runtime.project_type() == ProjectType::Game {
            let Some(canvas_layer) = runtime.render_game_canvas(&self.scene) else {
                runtime.cancel_frame();
                self.input.begin_frame();
                return;
            };
            Some(canvas_layer)
        } else if project_open {
            let electronics_canvas_rect = runtime.layout().electronics_canvas();
            let canvas_target =
                electronics_canvas_rect.to_physical(self.input.scale_factor() as f32, target_size);
            sync_electronics_canvas(
                self.electronics_editor.as_mut(),
                self.electronics_canvas.as_mut(),
                &mut self.electronics_frame_key,
                electronics_canvas_rect,
                canvas_target,
            );
            self.electronics_canvas
                .as_mut()
                .and_then(|canvas| canvas.render_layer(runtime.graphics_mut(), canvas_target))
        } else {
            None
        };
        if project_open {
            let Some(workbench) = self.workbench.as_mut() else {
                runtime.cancel_frame();
                self.input.begin_frame();
                return;
            };
            if runtime.project_type() == ProjectType::Electronics {
                workbench.sync_electronics_overlay(
                    self.electronics_editor.as_ref(),
                    runtime.layout().electronics_canvas(),
                );
            }
            let mut layers =
                workbench.compositor_layers(self.input.scale_factor() as f32, target_size);
            if self.settings_open {
                if let Some(settings_surface) = self.settings_surface.as_mut() {
                    layers.push(
                        settings_surface
                            .compositor_layer(self.input.scale_factor() as f32, target_size),
                    );
                }
            }
            if self.exit_confirmation_open {
                if let Some(surface) = self.exit_confirmation_surface.as_mut() {
                    layers.push(
                        surface.compositor_layer(self.input.scale_factor() as f32, target_size),
                    );
                }
            }
            let _ = host.render_editor_layers(compositor, canvas_layer, &mut layers, |key| {
                studio_model
                    .as_ref()
                    .map(|model| crate::native_studio::resolve_hub_text(key, model))
                    .unwrap_or_else(|| raf_core::i18n::t(key, Language::English))
            });
        } else if loading_active {
            let Some(loading) = self.loading.as_mut() else {
                runtime.cancel_frame();
                self.input.begin_frame();
                return;
            };
            loading.sync(
                runtime.layout().window,
                raf_render::api_graphic_basic::ui_surface::StudioUiPalette::IndustrialDark,
                (now / NATIVE_LOADING_SECONDS).clamp(0.0, 1.0) as f32,
                Language::English,
            );
            let mut layer = loading.compositor_layer(self.input.scale_factor() as f32, target_size);
            let _ = host.render_editor_layers(
                compositor,
                None,
                std::slice::from_mut(&mut layer),
                |key| raf_core::i18n::t(key, Language::English),
            );
        } else {
            let Some(studio) = self.studio.as_mut() else {
                runtime.cancel_frame();
                self.input.begin_frame();
                return;
            };
            let mut layers =
                vec![studio.compositor_layer(self.input.scale_factor() as f32, target_size)];
            if self.settings_open {
                if let Some(settings_surface) = self.settings_surface.as_mut() {
                    layers.push(
                        settings_surface
                            .compositor_layer(self.input.scale_factor() as f32, target_size),
                    );
                }
            }
            if self.exit_confirmation_open {
                if let Some(surface) = self.exit_confirmation_surface.as_mut() {
                    layers.push(
                        surface.compositor_layer(self.input.scale_factor() as f32, target_size),
                    );
                }
            }
            let _ = host.render_editor_layers(compositor, canvas_layer, &mut layers, |key| {
                studio_model
                    .as_ref()
                    .map(|model| crate::native_studio::resolve_hub_text(key, model))
                    .unwrap_or_else(|| raf_core::i18n::t(key, Language::English))
            });
        }
        let frame_cpu_ms = runtime
            .graphics()
            .snapshot()
            .last_frame_metrics
            .frame_cpu_ms;
        runtime.finish_frame(now, frame_cpu_ms, 0.0);
        if loading_active {
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
        }
        self.input.begin_frame();
    }

    fn apply_studio_intents(&mut self, intents: Vec<NativeStudioIntent>) {
        for intent in intents {
            match intent {
                NativeStudioIntent::Create {
                    name,
                    parent,
                    project_type,
                } => match Project::create(&name, project_type, &parent) {
                    Ok(project) => self.activate_project(project),
                    Err(error) => {
                        tracing::warn!(%error, "native project creation failed");
                        if let Some(studio) = self.studio.as_mut() {
                            studio.set_create_error(true);
                        }
                    }
                },
                NativeStudioIntent::OpenSettings { section } => {
                    self.pending_settings_open = Some(section);
                }
                NativeStudioIntent::Window(command) => self.execute_window_command(command),
                NativeStudioIntent::Open(path) => match Project::load(&path) {
                    Ok(project) => self.activate_project(project),
                    Err(error) => {
                        tracing::warn!(%error, path = %path.display(), "native project open failed")
                    }
                },
                NativeStudioIntent::Duplicate(path) => {
                    if let Err(error) = self.duplicate_project(&path) {
                        tracing::warn!(%error, path = %path.display(), "native Hub project duplicate failed");
                    } else if let Some(studio) = self.studio.as_mut() {
                        studio.reset_for_hub(false);
                    }
                }
                NativeStudioIntent::Forget(path) => {
                    forget_project(&path);
                    if let Some(studio) = self.studio.as_mut() {
                        studio.reset_for_hub(false);
                    }
                }
            }
        }
    }

    fn activate_project(&mut self, project: Project) {
        let project_type = project.project_type;
        self.exit_confirmation_open = false;
        self.pending_return_to_hub = None;
        remember_project(&project);
        self.scene = initial_scene(Some(&project));
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.reset_document();
            runtime.set_project_type(project_type);
            runtime.set_node_graph(initial_node_graph(Some(&project)));
        }
        if let Some(workbench) = self.workbench.as_mut() {
            workbench.set_engine_settings(self.settings_state.clone());
            workbench.set_project_info(project.name.clone(), project.project_type);
        }
        self.project = Some(project);
        self.electronics_editor = self
            .project
            .as_ref()
            .map(NativeElectronicsEditor::from_project);
        if let Some(editor) = self.electronics_editor.as_mut() {
            editor.apply_engine_settings(&self.settings_state);
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
                game_capabilities(project.project_type),
            );
        }
        self.mark_document_saved();
    }

    fn duplicate_project(&self, path: &Path) -> Result<(), String> {
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

    fn execute_window_command(&mut self, command: UiWindowCommand) {
        let Some(host) = self.window_host.as_ref() else {
            return;
        };
        match host.execute_window_command(command) {
            Ok(
                raf_render::api_graphic_basic::ui_surface::NativeWindowCommandResult::RequestClose,
            ) => {
                self.close_requested = true;
            }
            Ok(_) => {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            Err(error) => tracing::warn!(%error, ?command, "native window command failed"),
        }
    }

    fn window_event_native(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.clone() else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => {
                self.persist_settings();
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(host) = self.window_host.as_mut() {
                    host.resize(size.width, size.height);
                }
                if let Some(runtime) = self.runtime.as_mut() {
                    runtime.resize([size.width, size.height], window.scale_factor() as f32);
                }
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => self.redraw(),
            event @ WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } if !window.is_maximized() => {
                let logical_size = [
                    window.inner_size().width as f32 / self.input.scale_factor() as f32,
                    window.inner_size().height as f32 / self.input.scale_factor() as f32,
                ];
                let edge = self
                    .input
                    .snapshot()
                    .pointer_position
                    .and_then(|position| native_resize_edge(position, logical_size));
                if let Some(edge) = edge {
                    if let Some(host) = self.window_host.as_ref() {
                        if host
                            .execute_window_command(UiWindowCommand::BeginResize(edge))
                            .is_ok()
                        {
                            return;
                        }
                    }
                }
                if self.input.ingest(&event) {
                    window.request_redraw();
                }
            }
            other => {
                if self.input.ingest(&other) {
                    window.request_redraw();
                }
            }
        }
    }

    fn about_to_wait_native(&mut self, event_loop: &ActiveEventLoop) {
        // Attached CLI/MCP clients wake this loop through the user-event
        // proxy; draining here answers them without waiting for a rendered
        // frame. Zero cost while no client sends anything.
        let attached_changed = poll_attached_commands(
            &mut self.attached_host,
            &mut self.attached_ledger,
            self.runtime.as_mut(),
            &mut self.scene,
            self.electronics_editor.as_mut(),
            self.project.as_ref(),
        );
        if attached_changed {
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
        }
        if self.pending_document_saved {
            self.pending_document_saved = false;
            self.mark_document_saved();
        }
        if let Some((key, value)) = self.pending_project_range.take() {
            if let Some(project) = self.project.as_mut() {
                match key.as_str() {
                    "project-settings.building-snap-step" => {
                        project.settings.building_snap_step = value.clamp(0.5, 2.0);
                    }
                    _ => {}
                }
                if let Err(error) = project.save() {
                    tracing::warn!(%error, "project settings save failed");
                }
            }
        }
        if let Some((key, value)) = self.pending_project_setting.take() {
            if let Some(project) = self.project.as_mut() {
                match key.as_str() {
                    "project-settings.show-hierarchy" => {
                        project.settings.show_hierarchy_panel = value;
                    }
                    "project-settings.show-properties" => {
                        project.settings.show_properties_panel = value;
                    }
                    "project-settings.enable-audio" => {
                        project.settings.enable_audio = value;
                    }
                    "project-settings.enable-physics" => {
                        project.settings.enable_physics = value;
                    }
                    "project-settings.pause-unfocused" => {
                        project.settings.pause_when_unfocused = value;
                    }
                    "project-settings.enable-complements" => {
                        project.settings.enable_complements = value;
                    }
                    "project-settings.building-style.free" if value => {
                        project.settings.building_style = raf_core::project::BuildingStyle::Free;
                    }
                    "project-settings.building-style.professional" if value => {
                        project.settings.building_style =
                            raf_core::project::BuildingStyle::Organized;
                    }
                    _ => {}
                }
                if let Some(runtime) = self.runtime.as_mut() {
                    let size = runtime.layout().window.logical_size();
                    runtime.set_layout_request(project_layout_request(
                        project.project_type,
                        [size[0] as f32, size[1] as f32],
                        Some(project),
                    ));
                }
                if let Err(error) = project.save() {
                    tracing::warn!(%error, "project settings save failed");
                }
            }
        }
        if let Some(section) = self.pending_settings_open.take() {
            self.open_settings(section);
        }
        if !self.exit_confirmation_open {
            if let Some(open_create) = self.pending_return_to_hub {
                if self.has_unsaved_changes() {
                    self.exit_confirmation_open = true;
                    if let Some(runtime) = self.runtime.as_mut() {
                        runtime.request_animation_frame();
                    }
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                } else {
                    self.pending_return_to_hub = None;
                    self.return_to_hub(open_create);
                }
            }
        }
        for command in std::mem::take(&mut self.pending_window_commands) {
            self.execute_window_command(command);
        }
        if self.close_requested {
            self.persist_settings();
            event_loop.exit();
            return;
        }
        if let (Some(window), Some(runtime)) = (self.window.as_ref(), self.runtime.as_ref()) {
            if runtime
                .seconds_until_next_frame(self.elapsed_seconds())
                .is_some()
            {
                window.request_redraw();
            }
        }
    }

    fn return_to_hub(&mut self, open_create: bool) {
        self.settings_open = false;
        self.pending_settings_open = None;
        self.exit_confirmation_open = false;
        self.project = None;
        self.electronics_editor = None;
        self.electronics_frame_key = None;
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

    fn persist_settings(&self) {
        let directory = EngineSettings::user_config_dir();
        if let Err(error) = self.settings_state.save(&directory) {
            tracing::warn!(
                path = %directory.display(),
                %error,
                "global engine settings could not be saved"
            );
        }
    }
}

const NATIVE_RESIZE_BORDER: f32 = 8.0;

fn native_cursor_icon(cursor: raf_ui::UiCursorIcon) -> winit::window::CursorIcon {
    use winit::window::CursorIcon;

    match cursor {
        raf_ui::UiCursorIcon::Default => CursorIcon::Default,
        raf_ui::UiCursorIcon::PointingHand => CursorIcon::Pointer,
        raf_ui::UiCursorIcon::Text => CursorIcon::Text,
        raf_ui::UiCursorIcon::ResizeHorizontal => CursorIcon::EwResize,
        raf_ui::UiCursorIcon::ResizeVertical => CursorIcon::NsResize,
        raf_ui::UiCursorIcon::ResizeNorthEastSouthWest => CursorIcon::NeswResize,
        raf_ui::UiCursorIcon::ResizeNorthWestSouthEast => CursorIcon::NwseResize,
    }
}

fn native_resize_edge(position: [f32; 2], size: [f32; 2]) -> Option<UiResizeEdge> {
    let [x, y] = position;
    let [width, height] = size;
    if width <= 0.0 || height <= 0.0 || x < 0.0 || y < 0.0 || x > width || y > height {
        return None;
    }
    let near_left = x <= NATIVE_RESIZE_BORDER;
    let near_right = x >= width - NATIVE_RESIZE_BORDER;
    let near_top = y <= NATIVE_RESIZE_BORDER;
    let near_bottom = y >= height - NATIVE_RESIZE_BORDER;
    match (near_left, near_right, near_top, near_bottom) {
        (true, _, true, _) => Some(UiResizeEdge::NorthWest),
        (_, true, true, _) => Some(UiResizeEdge::NorthEast),
        (true, _, _, true) => Some(UiResizeEdge::SouthWest),
        (_, true, _, true) => Some(UiResizeEdge::SouthEast),
        (true, _, _, _) => Some(UiResizeEdge::West),
        (_, true, _, _) => Some(UiResizeEdge::East),
        (_, _, true, _) => Some(UiResizeEdge::North),
        (_, _, _, true) => Some(UiResizeEdge::South),
        _ => None,
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

fn project_layout_request(
    project_type: ProjectType,
    logical_size: [f32; 2],
    project: Option<&Project>,
) -> EditorLayoutRequest {
    let request = match project_type {
        ProjectType::Game => EditorLayoutRequest::game(logical_size),
        ProjectType::Electronics => EditorLayoutRequest::electronics(logical_size),
    };
    let Some(project) = project else {
        return request;
    };
    EditorLayoutRequest {
        left_visible: project.settings.show_hierarchy_panel,
        right_visible: project.settings.show_properties_panel,
        ..request
    }
}

fn synchronize_bottom_dock_layout(
    runtime: &mut NativeEditorRuntime,
    workbench: &NativeGameWorkbench,
) {
    let collapsed = workbench.bottom_dock_collapsed();
    if runtime.bottom_dock_is_collapsed() != collapsed {
        runtime.set_bottom_dock_collapsed(collapsed);
    }

    if !collapsed {
        let requested = workbench.bottom_dock_height();
        if (runtime.requested_bottom_dock_height() - requested).abs() > 0.5 {
            runtime.resize_bottom_dock(requested);
        }
    }
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

fn sync_electronics_canvas(
    editor: Option<&mut NativeElectronicsEditor>,
    canvas: Option<&mut NativeElectronicsCanvas>,
    frame_key: &mut Option<(u64, [u32; 2])>,
    logical_rect: crate::editor_layout::EditorRect,
    target_rect: raf_render::api_graphic_basic::CanvasTargetRect,
) {
    let Some(editor) = editor else {
        return;
    };
    let key = (
        editor.revision(),
        [target_rect.width.max(1), target_rect.height.max(1)],
    );
    if *frame_key == Some(key) {
        return;
    }
    let Some(canvas) = canvas else {
        return;
    };
    canvas.set_surface(match editor.active_surface() {
        raf_electronics::CadSurfaceKind::Schematic => {
            raf_render::bridge::GraphicsSurfaceKind::SchematicCanvas
        }
        raf_electronics::CadSurfaceKind::Pcb => raf_render::bridge::GraphicsSurfaceKind::PcbCanvas,
    });
    let options = editor.render_options(logical_rect);
    canvas.rebuild(editor.scene(), key.1, options);
    *frame_key = Some(key);
}

impl ApplicationHandler for NativeEditorApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.resumed_native(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        self.window_event_native(event_loop, window_id, event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.about_to_wait_native(event_loop);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_resize_hit_test_prioritizes_corners() {
        assert_eq!(
            native_resize_edge([2.0, 2.0], [800.0, 600.0]),
            Some(UiResizeEdge::NorthWest)
        );
        assert_eq!(
            native_resize_edge([798.0, 598.0], [800.0, 600.0]),
            Some(UiResizeEdge::SouthEast)
        );
    }

    #[test]
    fn native_resize_hit_test_leaves_content_clicks_alone() {
        assert_eq!(native_resize_edge([400.0, 300.0], [800.0, 600.0]), None);
        assert_eq!(native_resize_edge([-1.0, 300.0], [800.0, 600.0]), None);
    }
}
