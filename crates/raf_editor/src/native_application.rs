//! Winit application handler for the RafUI/ApiGraphicBasic editor path.
//!
//! This module is the native editor entry point. Winit owns the window loop,
//! RafUI owns retained document surfaces, and ApiGraphicBasic owns composition.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use raf_core::config::{
    EngineSettings, RenderPreset, RenderQuality, ScriptExecutionMode, ScriptLanguage, Theme,
};
use raf_core::project::{BuildingStyle, Project, ProjectSettings, ProjectType};
use raf_core::scene::SceneGraph;
use raf_core::TransactionLedger;
use raf_render::api_graphic_basic::ui_surface::{
    NativeUiInputBridge, NativeUiWindowConfig, StudioUiPalette, UiAction, UiDispatchedAction,
};
use raf_render::api_graphic_basic::NativeEditorCompositor;
use raf_ui::{UiColorMode, UiEnvironment, UiResizeEdge, UiWindowCommand};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use crate::attached::AttachedCommandHost;
use crate::editor_layout::EditorLayoutRequest;
use crate::electronics_controller::NativeElectronicsEditor;
use crate::native_attached_executor::poll_attached_commands;
use crate::native_editor_commands::apply_workbench_intents;
use crate::native_editor_runtime::NativeEditorRuntime;
use crate::native_electronics::NativeElectronicsCanvas;
use crate::native_project_controller::{
    initial_node_graph, initial_project, initial_scene, project_capabilities, save_project_document,
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
use crate::panels::viewport_compass::ViewportCompassState;
use crate::panels::viewport_controller::NativeViewportMode;
use crate::settings_surface::SettingsSection;

const NATIVE_LOADING_SECONDS: f64 = 1.15;
const NATIVE_EDITOR_SIZE: [f64; 2] = [1440.0, 900.0];
const NATIVE_EDITOR_MIN_SIZE: [f64; 2] = [900.0, 600.0];
const NATIVE_SPLASH_SIZE: [f64; 2] = [620.0, 500.0];

fn native_palette(theme: Theme) -> StudioUiPalette {
    match theme {
        Theme::Light => StudioUiPalette::PaperLight,
        Theme::Dark | Theme::System => StudioUiPalette::IndustrialDark,
    }
}

fn native_environment(settings: &EngineSettings, logical_size: [f32; 2]) -> UiEnvironment {
    let mut environment = UiEnvironment::new(
        logical_size[0].max(1.0).round() as u32,
        logical_size[1].max(1.0).round() as u32,
    );
    environment.color_mode = match settings.theme {
        Theme::Light => UiColorMode::Light,
        Theme::Dark => UiColorMode::Dark,
        Theme::System => UiColorMode::System,
    };
    environment.prefers_reduced_motion = settings.prefers_reduced_motion;
    environment.high_contrast = settings.high_contrast;
    environment.reduce_transparency =
        settings.reduce_transparency || settings.render_quality == RenderQuality::Potato;
    environment.font_size = settings.font_size.clamp(10.0, 24.0);
    environment.theme_experimental = settings.theme_experimental.clamp(0.0, 100.0);
    environment.ui_scale = if settings.auto_ui_scale {
        1.0
    } else {
        settings.ui_scale.clamp(0.5, 3.0)
    };
    environment
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ProjectSettingsMutation {
    changed: bool,
    layout_changed: bool,
}

impl ProjectSettingsMutation {
    const fn changed(layout_changed: bool) -> Self {
        Self {
            changed: true,
            layout_changed,
        }
    }

    const fn without_layout(changed: bool) -> Self {
        Self {
            changed,
            layout_changed: false,
        }
    }
}

fn apply_project_setting_toggle(
    settings: &mut ProjectSettings,
    key: &str,
    value: bool,
) -> ProjectSettingsMutation {
    let mut layout_changed = false;
    let changed = match key {
        "project-settings.show-hierarchy" => {
            layout_changed = settings.show_hierarchy_panel != value;
            settings.show_hierarchy_panel = value;
            layout_changed
        }
        "project-settings.show-properties" => {
            layout_changed = settings.show_properties_panel != value;
            settings.show_properties_panel = value;
            layout_changed
        }
        "project-settings.enable-console" => {
            let changed = settings.enable_console_commands != value;
            settings.enable_console_commands = value;
            changed
        }
        "project-settings.enable-audio" => {
            let changed = settings.enable_audio != value;
            settings.enable_audio = value;
            changed
        }
        "project-settings.enable-physics" => {
            let changed = settings.enable_physics != value;
            settings.enable_physics = value;
            changed
        }
        "project-settings.pause-unfocused" => {
            let changed = settings.pause_when_unfocused != value;
            settings.pause_when_unfocused = value;
            changed
        }
        "project-settings.enable-scripting" => {
            let changed = settings.enable_scripting != value;
            settings.enable_scripting = value;
            changed
        }
        "project-settings.auto-attach-scripts" => {
            let changed = settings.auto_attach_scripts != value;
            settings.auto_attach_scripts = value;
            changed
        }
        "project-settings.allow-gpu-features" => {
            let changed = settings.allow_gpu_features != value;
            settings.allow_gpu_features = value;
            changed
        }
        "project-settings.depth-accurate" => {
            let changed = settings.depth_accurate != value;
            settings.depth_accurate = value;
            changed
        }
        "project-settings.world-streaming" => {
            let changed = settings.world_streaming_enabled != value;
            settings.world_streaming_enabled = value;
            changed
        }
        "project-settings.electronics-snap-to-grid" => {
            let changed = settings.electronics_snap_to_grid != value;
            settings.electronics_snap_to_grid = value;
            changed
        }
        "project-settings.language.rhai" => {
            let before = settings.allowed_script_languages.has(ScriptLanguage::Rhai);
            settings
                .allowed_script_languages
                .set(ScriptLanguage::Rhai, value);
            before != value
        }
        "project-settings.language.cpp" => {
            let before = settings.allowed_script_languages.has(ScriptLanguage::Cpp);
            settings
                .allowed_script_languages
                .set(ScriptLanguage::Cpp, value);
            before != value
        }
        // Visual Nodes intentionally remains outside this pass. Its settings
        // contract is being replaced independently.
        "project-settings.language.nodes" => false,
        "project-settings.building-style.free" if value => {
            let changed = settings.building_style != BuildingStyle::Free;
            settings.building_style = BuildingStyle::Free;
            changed
        }
        "project-settings.building-style.professional" if value => {
            let changed = settings.building_style != BuildingStyle::Organized;
            settings.building_style = BuildingStyle::Organized;
            changed
        }
        _ => false,
    };
    ProjectSettingsMutation {
        changed,
        layout_changed,
    }
}

fn apply_project_setting_range(
    settings: &mut ProjectSettings,
    key: &str,
    value: f32,
) -> ProjectSettingsMutation {
    if !value.is_finite() {
        return ProjectSettingsMutation::default();
    }
    let snap = |value: f32, min: f32, max: f32, step: f32| {
        (min + ((value.clamp(min, max) - min) / step).round() * step).clamp(min, max)
    };
    let changed = match key {
        "project-settings.building-snap-step" => {
            let next = snap(value, 0.5, 2.0, 0.05);
            if (settings.building_snap_step - next).abs() <= f32::EPSILON {
                false
            } else {
                settings.building_snap_step = next;
                true
            }
        }
        "project-settings.electronics-schematic-grid-step" => {
            let next = snap(value, 1.0, 100.0, 1.0);
            if (settings.electronics_schematic_grid_step_mm - next).abs() <= f32::EPSILON {
                false
            } else {
                settings.electronics_schematic_grid_step_mm = next;
                true
            }
        }
        "project-settings.electronics-pcb-grid-step" => {
            let next = snap(value, 1.0, 100.0, 1.0);
            if (settings.electronics_pcb_grid_step_mm - next).abs() <= f32::EPSILON {
                false
            } else {
                settings.electronics_pcb_grid_step_mm = next;
                true
            }
        }
        "project-settings.depth-resolution-scale" => {
            let next = snap(value, 0.35, 1.0, 0.05);
            if (settings.depth_resolution_scale - next).abs() <= f32::EPSILON {
                false
            } else {
                settings.depth_resolution_scale = next;
                true
            }
        }
        "project-settings.stream-region-size" => {
            let next = snap(value, 32.0, 512.0, 16.0);
            if (settings.world_stream_region_size - next).abs() <= f32::EPSILON {
                false
            } else {
                settings.world_stream_region_size = next;
                true
            }
        }
        "project-settings.stream-radius" => {
            let next = snap(value, 1.0, 8.0, 1.0) as u32;
            if settings.world_stream_load_radius == next {
                false
            } else {
                settings.world_stream_load_radius = next;
                true
            }
        }
        "project-settings.stream-lod-bias" => {
            let next = snap(value, 0.0, 4.0, 1.0) as i8;
            if settings.world_stream_lod_bias == next {
                false
            } else {
                settings.world_stream_lod_bias = next;
                true
            }
        }
        _ => false,
    };
    ProjectSettingsMutation {
        changed,
        layout_changed: false,
    }
}

fn apply_project_setting_command(
    settings: &mut ProjectSettings,
    command: &str,
) -> ProjectSettingsMutation {
    match command {
        "project-settings.reset-panels" => {
            let changed = !settings.show_hierarchy_panel || !settings.show_properties_panel;
            settings.show_hierarchy_panel = true;
            settings.show_properties_panel = true;
            if changed {
                ProjectSettingsMutation::changed(true)
            } else {
                ProjectSettingsMutation::default()
            }
        }
        "project-settings.save.standard" => {
            let changed = settings.linear_save;
            settings.linear_save = false;
            ProjectSettingsMutation::without_layout(changed)
        }
        "project-settings.save.linear" => {
            let changed = !settings.linear_save;
            settings.linear_save = true;
            ProjectSettingsMutation::without_layout(changed)
        }
        "project-settings.script-mode.disabled" if settings.enable_scripting => {
            let changed = settings.script_execution_mode != ScriptExecutionMode::Disabled;
            settings.script_execution_mode = ScriptExecutionMode::Disabled;
            ProjectSettingsMutation::without_layout(changed)
        }
        "project-settings.script-mode.editor" if settings.enable_scripting => {
            let changed = settings.script_execution_mode != ScriptExecutionMode::EditorOnly;
            settings.script_execution_mode = ScriptExecutionMode::EditorOnly;
            ProjectSettingsMutation::without_layout(changed)
        }
        "project-settings.script-mode.runtime" if settings.enable_scripting => {
            let changed = settings.script_execution_mode != ScriptExecutionMode::Runtime;
            settings.script_execution_mode = ScriptExecutionMode::Runtime;
            ProjectSettingsMutation::without_layout(changed)
        }
        "project-settings.preset.potato" => {
            let changed = settings.runtime_render_preset != RenderPreset::Potato;
            settings.runtime_render_preset = RenderPreset::Potato;
            ProjectSettingsMutation::without_layout(changed)
        }
        "project-settings.preset.low" => {
            let changed = settings.runtime_render_preset != RenderPreset::Low;
            settings.runtime_render_preset = RenderPreset::Low;
            ProjectSettingsMutation::without_layout(changed)
        }
        "project-settings.preset.medium" if settings.allow_gpu_features => {
            let changed = settings.runtime_render_preset != RenderPreset::Medium;
            settings.runtime_render_preset = RenderPreset::Medium;
            ProjectSettingsMutation::without_layout(changed)
        }
        "project-settings.preset.high" if settings.allow_gpu_features => {
            let changed = settings.runtime_render_preset != RenderPreset::High;
            settings.runtime_render_preset = RenderPreset::High;
            ProjectSettingsMutation::without_layout(changed)
        }
        _ => ProjectSettingsMutation::default(),
    }
}

fn center_native_window(window: &Window, size: PhysicalSize<u32>) {
    let Some(monitor) = window.current_monitor() else {
        return;
    };
    let monitor_position = monitor.position();
    let monitor_size = monitor.size();
    let x =
        i64::from(monitor_position.x) + (i64::from(monitor_size.width) - i64::from(size.width)) / 2;
    let y = i64::from(monitor_position.y)
        + (i64::from(monitor_size.height) - i64::from(size.height)) / 2;
    window.set_outer_position(PhysicalPosition::new(
        x.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        y.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
    ));
}

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
    settings_restore_focus: Option<String>,
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
    pending_project_text: Option<(String, String)>,
    pending_project_command: Option<String>,
    saved_document_fingerprint: Option<u64>,
    pending_document_saved: bool,
    last_auto_save_at: Instant,
}

impl Default for NativeEditorApplication {
    fn default() -> Self {
        let project = initial_project();
        let scene = initial_scene(project.as_ref());
        let attached_ledger = project
            .as_ref()
            .map(|project| {
                let ledger = TransactionLedger::load_for_project(
                    &project.path,
                    crate::scene_history::scene_fingerprint(&scene),
                );
                if let Err(error) = ledger.persist_for_project(&project.path) {
                    tracing::warn!(
                        %error,
                        path = %project.path.display(),
                        "initial attached agent state persistence failed"
                    );
                }
                ledger
            })
            .unwrap_or_default();
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
            settings_restore_focus: None,
            settings_section: SettingsSection::Appearance,
            exit_confirmation_open: false,
            loading: None,
            show_loading: false,
            electronics_canvas: None,
            electronics_editor: None,
            electronics_frame_key: None,
            scene,
            project,
            attached_host: AttachedCommandHost::start(),
            attached_ledger,
            started_at: Instant::now(),
            close_requested: false,
            pending_window_commands: Vec::new(),
            pending_settings_open: None,
            pending_return_to_hub: None,
            pending_project_setting: None,
            pending_project_range: None,
            pending_project_text: None,
            pending_project_command: None,
            saved_document_fingerprint: None,
            pending_document_saved: false,
            last_auto_save_at: Instant::now(),
        }
    }
}

impl NativeEditorApplication {
    fn resumed_native(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let startup_loading = self.project.is_none();
        let mut attributes = Window::default_attributes()
            .with_title("Proyecto Rafi")
            .with_decorations(false)
            .with_resizable(!startup_loading);
        attributes = if startup_loading {
            attributes.with_inner_size(LogicalSize::new(
                NATIVE_SPLASH_SIZE[0],
                NATIVE_SPLASH_SIZE[1],
            ))
        } else {
            attributes
                .with_inner_size(LogicalSize::new(
                    NATIVE_EDITOR_SIZE[0],
                    NATIVE_EDITOR_SIZE[1],
                ))
                .with_min_inner_size(LogicalSize::new(
                    NATIVE_EDITOR_MIN_SIZE[0],
                    NATIVE_EDITOR_MIN_SIZE[1],
                ))
        };
        let Ok(window) = event_loop.create_window(attributes) else {
            tracing::error!("native editor window could not be created");
            event_loop.exit();
            return;
        };
        let window = Arc::new(window);
        let size = window.inner_size();
        if startup_loading {
            center_native_window(&window, size);
        }
        let scale_factor = window.scale_factor() as f32;
        let mut host = match pollster::block_on(
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
        host.set_vsync(self.settings_state.vsync);

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
        let allow_gpu_features = self
            .project
            .as_ref()
            .is_some_and(|project| project.settings.allow_gpu_features);
        runtime.apply_engine_settings(&self.settings_state, allow_gpu_features);
        runtime.resize([size.width, size.height], scale_factor);
        runtime
            .graphics_mut()
            .set_shared_graphics_context(Some(host.shared_graphics_context()));
        runtime.set_node_graph(initial_node_graph(self.project.as_ref()));

        let graphics = host.graphics_context();
        let compositor = NativeEditorCompositor::new(&graphics, [8, 11, 15, 255]);
        let palette = native_palette(self.settings_state.theme);
        let workbench = NativeGameWorkbench::new(&graphics, palette, runtime.layout().window);
        let studio = NativeStudioSurface::new(&graphics, runtime.layout().window, palette);
        let settings_surface =
            SettingsSurfaceHost::new(&graphics, runtime.layout().window, palette);
        let exit_confirmation_surface =
            ExitConfirmationSurfaceHost::new(&graphics, runtime.layout().window, palette);
        let loading = LoadingSurfaceHost::new(&graphics, runtime.layout().window, palette);
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
        if let Some(studio) = self.studio.as_mut() {
            studio.set_language(self.settings_state.language);
            let window = self
                .runtime
                .as_ref()
                .map(|runtime| runtime.layout().window)
                .unwrap_or_default();
            studio.set_environment(native_environment(
                &self.settings_state,
                [window.width.max(1.0), window.height.max(1.0)],
            ));
        }
        let initial_environment = native_environment(
            &self.settings_state,
            self.runtime
                .as_ref()
                .map(|runtime| {
                    let window = runtime.layout().window;
                    [window.width, window.height]
                })
                .unwrap_or([1.0; 2]),
        );
        self.settings_surface = Some(settings_surface);
        self.exit_confirmation_surface = Some(exit_confirmation_surface);
        self.loading = Some(loading);
        if let Some(settings_surface) = self.settings_surface.as_mut() {
            settings_surface.set_environment(initial_environment);
        }
        if let Some(exit_confirmation_surface) = self.exit_confirmation_surface.as_mut() {
            exit_confirmation_surface.set_environment(initial_environment);
        }
        if let Some(loading) = self.loading.as_mut() {
            loading.set_environment(initial_environment);
        }
        self.show_loading = self.project.is_none();
        self.electronics_canvas = Some(electronics_canvas);
        self.electronics_editor = self
            .project
            .as_ref()
            .map(NativeElectronicsEditor::from_project);
        if let Some(editor) = self.electronics_editor.as_mut() {
            editor.apply_engine_settings(&self.settings_state);
            if let Some(project) = self.project.as_ref() {
                editor.apply_project_settings(&project.settings);
            }
        }
        self.electronics_frame_key = None;
        let capabilities = project_capabilities(project_type);
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

    fn finish_startup_loading(&mut self) {
        if !self.show_loading {
            return;
        }
        self.show_loading = false;
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let scale_factor = window.scale_factor().max(0.25);
        let target_size = PhysicalSize::new(
            (NATIVE_EDITOR_SIZE[0] * scale_factor).round().max(1.0) as u32,
            (NATIVE_EDITOR_SIZE[1] * scale_factor).round().max(1.0) as u32,
        );
        window.set_resizable(true);
        window.set_min_inner_size(Some(LogicalSize::new(
            NATIVE_EDITOR_MIN_SIZE[0],
            NATIVE_EDITOR_MIN_SIZE[1],
        )));
        let applied_size = window
            .request_inner_size(LogicalSize::new(
                NATIVE_EDITOR_SIZE[0],
                NATIVE_EDITOR_SIZE[1],
            ))
            .unwrap_or(target_size);
        center_native_window(window, applied_size);
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.request_animation_frame();
        }
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
        self.last_auto_save_at = Instant::now();
    }

    fn has_unsaved_changes(&self) -> bool {
        self.project.is_some()
            && self
                .saved_document_fingerprint
                .zip(self.document_fingerprint())
                .is_some_and(|(saved, current)| saved != current)
    }

    fn auto_save_deadline(&self) -> Option<Instant> {
        self.project.as_ref()?;
        let interval = self
            .settings_state
            .auto_save_interval_seconds
            .clamp(30, 600);
        Some(self.last_auto_save_at + Duration::from_secs(u64::from(interval)))
    }

    fn maybe_auto_save(&mut self) {
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
            Err(error) => tracing::warn!(%error, "native automatic document save failed"),
        }
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
        surface.sync(native_palette(self.settings_state.theme), rect);
        surface.set_environment(native_environment(
            &self.settings_state,
            [rect.width, rect.height],
        ));
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
        self.settings_restore_focus = self
            .workbench
            .as_ref()
            .and_then(NativeGameWorkbench::focused_control_id);
        let workbench_settings = self
            .workbench
            .as_ref()
            .map(|workbench| workbench.engine_settings().clone());
        if let Some(workbench_settings) = workbench_settings {
            self.settings_state = workbench_settings;
        }
        self.settings_section = section;
        self.settings_open = true;
        if let Some(settings_surface) = self.settings_surface.as_mut() {
            settings_surface.reset_draft(&self.settings_state);
        }
        // Settings replaces the workbench as the active retained surface. If
        // a workbench textbox or command control owned the shared keyboard,
        // Settings could still receive pointer selection but its text events
        // were rejected by InputRouter::try_capture_keyboard.
        let studio_owner = self.studio.as_ref().map(NativeStudioSurface::owner);
        if let Some(runtime) = self.runtime.as_mut() {
            if let Some(workbench) = self.workbench.as_mut() {
                workbench.reset_input_state(runtime.input_router_mut());
            }
            if let Some(owner) = studio_owner {
                runtime.input_router_mut().cancel_owner(owner);
            }
            if let Some(settings_surface) = self.settings_surface.as_mut() {
                settings_surface.reset_input_state(runtime.input_router_mut());
            }
            // The viewport and electronics canvas are separate consumers of
            // the same router. Opening a modal revokes any drag they may
            // still own so the modal cannot render while filtering input for
            // an obsolete owner.
            runtime.input_router_mut().cancel_all();
            runtime.request_animation_frame();
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    fn close_settings_input(&mut self) {
        if let (Some(settings_surface), Some(runtime)) =
            (self.settings_surface.as_mut(), self.runtime.as_mut())
        {
            settings_surface.reset_input_state(runtime.input_router_mut());
        }
        self.settings_open = false;
        if let Some(id) = self.settings_restore_focus.take() {
            if let Some(workbench) = self.workbench.as_mut() {
                workbench.restore_focus(id);
            }
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
        let preview_theme = if settings_surface.is_dirty() {
            settings_surface.draft.theme
        } else {
            self.settings_state.theme
        };
        settings_surface.sync(
            native_palette(preview_theme),
            &self.settings_state,
            self.settings_section,
            rect,
        );
        let preview_settings = settings_surface.draft.clone();
        settings_surface.set_environment(native_environment(
            &preview_settings,
            [rect.width, rect.height],
        ));
        settings_surface.process_input(&self.input, runtime.input_router_mut())
    }

    fn commit_project_settings_mutation(&mut self, mutation: ProjectSettingsMutation) {
        if !mutation.changed {
            return;
        }

        let layout_request = if mutation.layout_changed {
            match (self.runtime.as_ref(), self.project.as_ref()) {
                (Some(runtime), Some(project)) => {
                    let size = runtime.layout().window.logical_size();
                    Some(project_layout_request(
                        project.project_type,
                        [size[0] as f32, size[1] as f32],
                        Some(project),
                    ))
                }
                _ => None,
            }
        } else {
            None
        };

        if let Some(project) = self.project.as_mut() {
            if let Err(error) = project.save() {
                tracing::warn!(%error, "project settings save failed");
            }
        }
        if let Some(request) = layout_request {
            if let Some(runtime) = self.runtime.as_mut() {
                runtime.set_layout_request(request);
            }
        }
        self.apply_live_project_settings();
    }

    fn apply_project_setting_toggle_intent(&mut self, key: String, value: bool) {
        // The project flag is intentionally subordinate to the global
        // command capability. The disabled UI never emits this path, but the
        // domain boundary also enforces it for automation and stale surfaces.
        if key == "project-settings.enable-console" && !self.settings_state.command_console_enabled
        {
            return;
        }
        let mutation = self
            .project
            .as_mut()
            .map(|project| apply_project_setting_toggle(&mut project.settings, &key, value))
            .unwrap_or_default();
        self.commit_project_settings_mutation(mutation);
    }

    fn apply_project_setting_range_intent(&mut self, key: String, value: f32) {
        let mutation = self
            .project
            .as_mut()
            .map(|project| apply_project_setting_range(&mut project.settings, &key, value))
            .unwrap_or_default();
        self.commit_project_settings_mutation(mutation);
    }

    fn apply_project_setting_text_intent(&mut self, key: String, value: String) {
        let mutation = if key == "project-settings.default_scene_name" {
            let value = value.trim();
            if value.is_empty() {
                ProjectSettingsMutation::default()
            } else if let Some(project) = self.project.as_mut() {
                if project.settings.default_scene_name == value {
                    ProjectSettingsMutation::default()
                } else {
                    project.settings.default_scene_name = value.to_string();
                    ProjectSettingsMutation::without_layout(true)
                }
            } else {
                ProjectSettingsMutation::default()
            }
        } else {
            ProjectSettingsMutation::default()
        };
        self.commit_project_settings_mutation(mutation);
    }

    fn apply_project_setting_command_intent(&mut self, command: String) {
        let mutation = self
            .project
            .as_mut()
            .map(|project| apply_project_setting_command(&mut project.settings, &command))
            .unwrap_or_default();
        self.commit_project_settings_mutation(mutation);
    }

    fn apply_live_project_settings(&mut self) {
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let project_settings = project.settings.clone();
        let global_settings = self.settings_state.clone();
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.apply_engine_settings(&global_settings, project_settings.allow_gpu_features);
            runtime
                .game_viewport_mut()
                .apply_project_settings(&project_settings);
            runtime.request_canvas_frame();
        }
        if let Some(editor) = self.electronics_editor.as_mut() {
            editor.apply_project_settings(&project_settings);
        }
    }

    fn apply_committed_settings(&mut self) {
        self.apply_live_project_settings();
        if let Some(window_host) = self.window_host.as_mut() {
            window_host.set_vsync(self.settings_state.vsync);
        }
        if let Some(workbench) = self.workbench.as_mut() {
            workbench.set_engine_settings(self.settings_state.clone());
        }
        let window_size = self
            .runtime
            .as_ref()
            .map(|runtime| {
                let window = runtime.layout().window;
                [window.width, window.height]
            })
            .unwrap_or([1.0; 2]);
        if let Some(studio) = self.studio.as_mut() {
            studio.set_language(self.settings_state.language);
            studio.set_environment(native_environment(&self.settings_state, window_size));
        }
        if let Some(exit_confirmation_surface) = self.exit_confirmation_surface.as_mut() {
            exit_confirmation_surface
                .set_environment(native_environment(&self.settings_state, window_size));
        }
        if let Some(loading) = self.loading.as_mut() {
            loading.set_environment(native_environment(&self.settings_state, window_size));
        }
        if let Some(editor) = self.electronics_editor.as_mut() {
            editor.apply_engine_settings(&self.settings_state);
            if let Some(project) = self.project.as_ref() {
                editor.apply_project_settings(&project.settings);
            }
        }
    }

    fn accept_settings_draft(&mut self) -> bool {
        let Some(settings_surface) = self.settings_surface.as_mut() else {
            return false;
        };
        settings_surface.commit_numeric_drafts();
        self.settings_state = settings_surface.accept();
        self.apply_committed_settings();
        self.persist_settings();
        true
    }

    fn apply_settings_actions(&mut self, actions: Vec<UiDispatchedAction>) {
        if !self.settings_open {
            return;
        }
        let mut changed = false;
        if let Some(settings_surface) = self.settings_surface.as_mut() {
            changed |= settings_surface.commit_numeric_drafts();
        }
        let escape_closed_select = actions.iter().any(|dispatched| {
            matches!(
                &dispatched.action,
                UiAction::SetSelectOpen { open: false, .. }
            )
        });
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
                UiAction::SetSelect { key, value, .. } => {
                    if let Some(settings_surface) = self.settings_surface.as_mut() {
                        if crate::native_workbench::apply_settings_select(
                            &mut settings_surface.draft,
                            &key,
                            &value,
                        ) {
                            settings_surface.mark_dirty();
                            changed = true;
                        }
                    }
                }
                UiAction::SetSelectOpen { id, open } => {
                    if let Some(settings_surface) = self.settings_surface.as_mut() {
                        settings_surface.set_select_open(&id, open);
                        changed = true;
                    }
                }
                UiAction::SetText { key, value } => {
                    if key == "settings.search" {
                        if let Some(settings_surface) = self.settings_surface.as_mut() {
                            settings_surface.set_search_query(value);
                            self.settings_section = settings_surface.section();
                            changed = true;
                        }
                    } else if let Some(settings_surface) = self.settings_surface.as_mut() {
                        if crate::native_workbench::is_settings_numeric_text_key(&key) {
                            settings_surface.mark_draft_dirty();
                            changed = true;
                        } else if crate::native_workbench::apply_settings_text(
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
                            settings_surface.set_section(section);
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
                                changed = true;
                            }
                        }
                    } else if let Some(provider_id) =
                        name.strip_prefix("settings.ai_provider.clear.")
                    {
                        if crate::native_workbench::ai_provider_from_id(provider_id).is_some() {
                            let key = format!("settings.ai_provider.{provider_id}.api_key");
                            if let Some(settings_surface) = self.settings_surface.as_mut() {
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
                    } else if name == "settings.apply" {
                        changed |= self.accept_settings_draft();
                    } else if name == "settings.save" {
                        changed |= self.accept_settings_draft();
                        if changed {
                            self.close_settings_input();
                        }
                    } else if name == "settings.cancel" {
                        if let Some(settings_surface) = self.settings_surface.as_mut() {
                            settings_surface.cancel(&self.settings_state);
                        }
                        self.close_settings_input();
                        changed = true;
                    } else if name == "settings.reset_defaults" {
                        if let Some(settings_surface) = self.settings_surface.as_mut() {
                            settings_surface.restore_defaults();
                            settings_surface.set_section(SettingsSection::Appearance);
                            self.settings_section = SettingsSection::Appearance;
                        }
                        changed = true;
                    } else if name == "settings.dismiss" {
                        let can_dismiss = self
                            .settings_surface
                            .as_ref()
                            .is_none_or(|settings_surface| !settings_surface.is_dirty());
                        if can_dismiss {
                            if let Some(settings_surface) = self.settings_surface.as_mut() {
                                settings_surface.cancel(&self.settings_state);
                            }
                            self.close_settings_input();
                            changed = true;
                        }
                    } else if name.starts_with("settings.modal.drag.")
                        || name.starts_with("settings.modal.resize.")
                    {
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
        if self.settings_open
            && !escape_closed_select
            && self
                .input
                .snapshot()
                .key_pressed(raf_core::InputKey::Escape)
        {
            let can_dismiss = self
                .settings_surface
                .as_ref()
                .is_none_or(|settings_surface| !settings_surface.is_dirty());
            if can_dismiss {
                if let Some(settings_surface) = self.settings_surface.as_mut() {
                    settings_surface.cancel(&self.settings_state);
                }
                self.close_settings_input();
                changed = true;
            }
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
        self.sync_attached_document_revision();
        let project_assets = self
            .workbench
            .as_ref()
            .map(|workbench| workbench.agent_assets().to_vec())
            .unwrap_or_default();
        let catalog_pending = self
            .workbench
            .as_ref()
            .is_some_and(|workbench| workbench.agent_catalog_pending());
        let catalog_error = self
            .workbench
            .as_ref()
            .and_then(|workbench| workbench.agent_catalog_error().map(str::to_string));
        let attached_result = poll_attached_commands(
            &mut self.attached_host,
            &mut self.attached_ledger,
            self.runtime.as_mut(),
            self.workbench.as_mut(),
            &mut self.scene,
            self.electronics_editor.as_mut(),
            self.project.as_ref(),
            &project_assets,
            catalog_pending,
            catalog_error.as_deref(),
            self.settings_state.language,
        );
        if attached_result.changed {
            self.attached_ledger
                .mark_document(crate::scene_history::scene_fingerprint(&self.scene));
            self.persist_attached_ledger();
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
        }
        if attached_result.document_saved {
            self.pending_document_saved = true;
        }
        let now = self.elapsed_seconds();
        let Some(runtime) = self.runtime.as_ref() else {
            return;
        };
        let Some(seconds_until_frame) = runtime.seconds_until_next_frame(now) else {
            return;
        };
        if seconds_until_frame > f64::EPSILON {
            return;
        }
        self.input.set_time_seconds(now);
        if let Some(runtime) = self.runtime.as_mut() {
            runtime
                .input_router_mut()
                .reconcile_input(self.input.snapshot());
        }
        let loading_active = self.show_loading && now < NATIVE_LOADING_SECONDS;
        if self.show_loading && !loading_active {
            self.finish_startup_loading();
        }
        let exit_confirmation_was_open = self.exit_confirmation_open;
        let mut persist_agent_settings = false;
        let mut linear_save_requested = false;

        if self.exit_confirmation_open {
            let action = self.process_exit_confirmation_input();
            self.apply_exit_confirmation_action(action);
        }

        if self.project.is_none() && !loading_active {
            if self.settings_open && !self.exit_confirmation_open && !exit_confirmation_was_open {
                let settings_actions = self.process_settings_input();
                self.apply_settings_actions(settings_actions);
            }
            if self.settings_open {
                if let (Some(window), Some(settings_surface)) =
                    (self.window.as_ref(), self.settings_surface.as_ref())
                {
                    window.set_cursor(native_cursor_icon(settings_surface.cursor_hint()));
                }
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
                studio.sync(runtime.layout(), native_palette(self.settings_state.theme));
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
            linear_save_requested = self
                .project
                .as_ref()
                .is_some_and(|project| project.settings.linear_save);
            if self.settings_open && !self.exit_confirmation_open && !exit_confirmation_was_open {
                let settings_actions = self.process_settings_input();
                self.apply_settings_actions(settings_actions);
            }
            let Some(runtime) = self.runtime.as_mut() else {
                return;
            };
            let mut cursor_hint = self
                .settings_open
                .then(|| {
                    self.settings_surface
                        .as_ref()
                        .map(SettingsSurfaceHost::cursor_hint)
                })
                .flatten();
            if let Some(workbench) = self.workbench.as_mut() {
                workbench.set_inspector_transform_drag_active(
                    runtime.project_type() == ProjectType::Game
                        && runtime.game_viewport().transform_drag_active(),
                );
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
                let compass_state = if runtime.project_type() == ProjectType::Game
                    && runtime.game_viewport().mode == NativeViewportMode::View3d
                {
                    ViewportCompassState::from_orbit(
                        runtime.game_viewport().bridge().orbit_yaw(),
                        runtime.game_viewport().bridge().orbit_pitch(),
                    )
                } else {
                    ViewportCompassState::hidden()
                };
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
                    compass_state,
                    self.electronics_editor.as_ref(),
                );
                synchronize_bottom_dock_layout(runtime, workbench);
                if !self.settings_open {
                    cursor_hint = Some(workbench.cursor_hint());
                }
                if self.settings_open || self.exit_confirmation_open || exit_confirmation_was_open {
                    runtime.request_animation_frame();
                    runtime.set_continuous_ui_motion(false);
                } else {
                    let ui_language = self.settings_state.language;
                    let (intents, actions) = workbench.process_input(
                        &self.input,
                        runtime.input_router_mut(),
                        &self.scene,
                        &selected,
                        self.project.as_ref(),
                        |key| raf_core::i18n::t(key, ui_language),
                    );
                    let viewport_press =
                        self.input.snapshot().pointer_position.is_some_and(|point| {
                            let canvas = match runtime.project_type() {
                                ProjectType::Game => runtime.layout().canvas,
                                ProjectType::Electronics => runtime.layout().electronics_canvas(),
                            };
                            canvas.contains(point)
                        }) && [
                            raf_core::PointerButton::Primary,
                            raf_core::PointerButton::Secondary,
                            raf_core::PointerButton::Middle,
                        ]
                        .into_iter()
                        .any(|button| self.input.snapshot().button_pressed(button));
                    if viewport_press && !workbench.has_interactive_hover() {
                        workbench.clear_input_focus(runtime.input_router_mut());
                    }
                    let viewport_navigation_key_down = [
                        raf_core::InputKey::Q,
                        raf_core::InputKey::W,
                        raf_core::InputKey::E,
                        raf_core::InputKey::A,
                        raf_core::InputKey::S,
                        raf_core::InputKey::D,
                    ]
                    .into_iter()
                    .any(|key| self.input.snapshot().key_down(key));
                    if runtime.project_type() == ProjectType::Game && viewport_navigation_key_down {
                        workbench.release_non_text_keyboard_capture(runtime.input_router_mut());
                    }
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
                            crate::native_workbench::NativeWorkbenchIntent::ProjectSettingText {
                                key,
                                value,
                            } => {
                                self.pending_project_text = Some((key, value));
                            }
                            crate::native_workbench::NativeWorkbenchIntent::ProjectSettingCommand(
                                command,
                            ) => {
                                self.pending_project_command = Some(command);
                            }
                            crate::native_workbench::NativeWorkbenchIntent::AgentSettingsChanged(
                                settings,
                            ) => {
                                self.settings_state = settings;
                                persist_agent_settings = true;
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
                            let electronics_canvas = runtime.layout().electronics_canvas();
                            for intent in &domain_intents {
                                if let crate::native_workbench::NativeWorkbenchIntent::Command(
                                    command,
                                ) = intent
                                {
                                    if let Some(raw_index) =
                                        command.strip_prefix("electronics.library.drag.start.")
                                    {
                                        if let Ok(index) = raw_index.parse::<usize>() {
                                            editor.begin_library_drag(index);
                                        }
                                        continue;
                                    }
                                    if command.starts_with("electronics.library.drag.move.") {
                                        continue;
                                    }
                                    if command.starts_with("electronics.library.drag.end.") {
                                        editor.finish_library_drag_at(
                                            self.input.snapshot().pointer_position,
                                            electronics_canvas,
                                        );
                                        continue;
                                    }
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
                    let agent_project_path =
                        self.project.as_ref().map(|project| project.path.clone());
                    let agent_revision_before = self.attached_ledger.revision();
                    let (viewport, graphics) = runtime.game_viewport_and_graphics_mut();
                    let agent_actions = workbench.poll_agent(
                        &mut self.scene,
                        viewport,
                        graphics,
                        self.electronics_editor.as_mut(),
                        self.project.as_ref(),
                        &mut self.attached_ledger,
                    );
                    let agent_changed = workbench.take_agent_canvas_changed();
                    let agent_ledger_changed =
                        self.attached_ledger.revision() != agent_revision_before;
                    if agent_changed || agent_ledger_changed {
                        runtime.request_canvas_frame();
                        // Agent mutations already advanced the shared ledger.
                        // Mark the observed document here so the next redraw
                        // does not count the same mutation a second time, and
                        // persist the clock for attached clients after a
                        // native Agent run as well.
                        if agent_changed {
                            self.attached_ledger.mark_document(
                                crate::scene_history::scene_fingerprint(&self.scene),
                            );
                        }
                        if let Some(project_path) = agent_project_path {
                            if let Err(error) =
                                self.attached_ledger.persist_for_project(&project_path)
                            {
                                tracing::warn!(
                                    %error,
                                    path = %project_path.display(),
                                    "native Agent state persistence failed"
                                );
                            }
                        }
                    }
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
                    let has_active_motion = workbench.has_active_ui_motion();
                    let has_active_text_repeat = workbench.has_active_text_repeat();
                    if had_ui_activity || workbench.needs_ui_frame() {
                        runtime.request_animation_frame();
                    }
                    runtime.set_continuous_ui_motion(
                        has_active_motion
                            || has_active_text_repeat
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
            let focused_text_rect = if self.settings_open {
                self.settings_surface
                    .as_ref()
                    .and_then(SettingsSurfaceHost::focused_text_rect)
            } else {
                self.workbench
                    .as_ref()
                    .and_then(NativeGameWorkbench::focused_text_rect)
            };
            if let Some(window) = self.window.as_ref() {
                window.set_ime_allowed(focused_text_rect.is_some());
                if let Some(rect) = focused_text_rect {
                    let scale = self.input.scale_factor().max(0.25) as f32;
                    window.set_ime_cursor_area(
                        PhysicalPosition::new(
                            (rect.x * scale).round() as i32,
                            (rect.y * scale).round() as i32,
                        ),
                        PhysicalSize::new(
                            (rect.width * scale).round().max(1.0) as u32,
                            (rect.height * scale).round().max(1.0) as u32,
                        ),
                    );
                }
            }
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
                .any(|command| command == crate::application_menu::command::SEARCH_OPEN)
            {
                if let Some(workbench) = self.workbench.as_mut() {
                    workbench.open_search();
                }
                runtime.request_ui_frame();
            }
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
                        viewport.apply_project_settings(&project.settings);
                    }
                }
                runtime.update_game_input(self.input.snapshot(), &mut self.scene);
            }
        }

        if linear_save_requested && self.has_unsaved_changes() {
            match self.save_current_documents() {
                Ok(()) => {
                    self.mark_document_saved();
                    tracing::info!("native linear project save completed");
                }
                Err(error) => tracing::warn!(%error, "native linear project save failed"),
            }
        }

        if persist_agent_settings {
            self.apply_committed_settings();
            self.persist_settings();
        }

        let has_active_text_repeat =
            if loading_active || self.exit_confirmation_open || exit_confirmation_was_open {
                false
            } else if self.settings_open {
                self.settings_surface
                    .as_ref()
                    .is_some_and(SettingsSurfaceHost::has_active_text_repeat)
            } else if self.project.is_some() {
                self.workbench
                    .as_ref()
                    .is_some_and(NativeGameWorkbench::has_active_text_repeat)
            } else {
                self.studio
                    .as_ref()
                    .is_some_and(NativeStudioSurface::has_active_text_repeat)
            };
        let Some(runtime) = self.runtime.as_mut() else {
            return;
        };
        runtime.set_continuous_text_input(has_active_text_repeat);
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
        // Let the open settings draft preview its language in the modal. The
        // committed settings remain authoritative everywhere else until the
        // user applies the draft, but using the draft resolver here keeps the
        // preview internally consistent with the controls being edited.
        let ui_language = if self.settings_open {
            self.settings_surface
                .as_ref()
                .map(|surface| surface.draft.language)
                .unwrap_or(self.settings_state.language)
        } else {
            self.settings_state.language
        };
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
                    .map(|model| crate::native_studio::resolve_hub_text(key, model, ui_language))
                    .unwrap_or_else(|| raf_core::i18n::t(key, ui_language))
            });
        } else if loading_active {
            let Some(loading) = self.loading.as_mut() else {
                runtime.cancel_frame();
                self.input.begin_frame();
                return;
            };
            loading.sync(
                runtime.layout().window,
                native_palette(self.settings_state.theme),
                (now / NATIVE_LOADING_SECONDS).clamp(0.0, 1.0) as f32,
                self.settings_state.language,
            );
            let mut layer = loading.compositor_layer(self.input.scale_factor() as f32, target_size);
            let _ = host.render_editor_layers(
                compositor,
                None,
                std::slice::from_mut(&mut layer),
                |key| raf_core::i18n::t(key, self.settings_state.language),
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
                    .map(|model| crate::native_studio::resolve_hub_text(key, model, ui_language))
                    .unwrap_or_else(|| raf_core::i18n::t(key, ui_language))
            });
        }
        let frame_cpu_ms = runtime
            .graphics()
            .snapshot()
            .last_frame_metrics
            .frame_cpu_ms;
        runtime.finish_frame(now, frame_cpu_ms, 0.0);
        if loading_active {
            runtime.request_animation_frame();
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
                            studio.set_create_error(error.to_string());
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
        self.settings_restore_focus = None;

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
        remember_project(&project);
        self.scene = initial_scene(Some(&project));
        self.attached_ledger = TransactionLedger::load_for_project(
            &project.path,
            crate::scene_history::scene_fingerprint(&self.scene),
        );
        self.persist_attached_ledger_for(&project);
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
        let allow_gpu_features = self
            .project
            .as_ref()
            .is_some_and(|project| project.settings.allow_gpu_features);
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.apply_engine_settings(&self.settings_state, allow_gpu_features);
        }
        self.electronics_editor = self
            .project
            .as_ref()
            .map(NativeElectronicsEditor::from_project);
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
                let scale = self.input.scale_factor() as f32;
                let logical_size = [
                    size.width as f32 / scale.max(0.25),
                    size.height as f32 / scale.max(0.25),
                ];
                if let Some(workbench) = self.workbench.as_mut() {
                    workbench.set_viewport_size(logical_size);
                }
                let environment = native_environment(&self.settings_state, logical_size);
                if let Some(studio) = self.studio.as_mut() {
                    studio.set_environment(environment);
                }
                if let Some(settings_surface) = self.settings_surface.as_mut() {
                    settings_surface.set_environment(environment);
                }
                if let Some(exit_confirmation_surface) = self.exit_confirmation_surface.as_mut() {
                    exit_confirmation_surface.set_environment(environment);
                }
                if let Some(loading) = self.loading.as_mut() {
                    loading.set_environment(environment);
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
                    if let Some(runtime) = self.runtime.as_mut() {
                        runtime.request_ui_frame();
                    }
                    window.request_redraw();
                }
            }
            other => {
                let focused = match &other {
                    WindowEvent::Focused(focused) => Some(*focused),
                    _ => None,
                };
                if self.input.ingest(&other) {
                    if let Some(runtime) = self.runtime.as_mut() {
                        if let Some(focused) = focused {
                            runtime.set_window_focused(focused);
                            if !focused {
                                runtime.input_router_mut().cancel_all();
                            }
                        } else {
                            runtime.request_ui_frame();
                        }
                    }
                    window.request_redraw();
                }
            }
        }
    }

    fn about_to_wait_native(&mut self, event_loop: &ActiveEventLoop) {
        // Attached CLI/MCP clients wake this loop through the user-event
        // proxy; draining here answers them without waiting for a rendered
        // frame. Zero cost while no client sends anything.
        let project_assets = self
            .workbench
            .as_ref()
            .map(|workbench| workbench.agent_assets().to_vec())
            .unwrap_or_default();
        let catalog_pending = self
            .workbench
            .as_ref()
            .is_some_and(|workbench| workbench.agent_catalog_pending());
        let catalog_error = self
            .workbench
            .as_ref()
            .and_then(|workbench| workbench.agent_catalog_error().map(str::to_string));
        let attached_result = poll_attached_commands(
            &mut self.attached_host,
            &mut self.attached_ledger,
            self.runtime.as_mut(),
            self.workbench.as_mut(),
            &mut self.scene,
            self.electronics_editor.as_mut(),
            self.project.as_ref(),
            &project_assets,
            catalog_pending,
            catalog_error.as_deref(),
            self.settings_state.language,
        );
        if attached_result.changed {
            self.attached_ledger
                .mark_document(crate::scene_history::scene_fingerprint(&self.scene));
            self.persist_attached_ledger();
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
        }
        if attached_result.document_saved {
            self.pending_document_saved = true;
        }
        if self.pending_document_saved {
            self.pending_document_saved = false;
            self.mark_document_saved();
        }
        if let Some((key, value)) = self.pending_project_range.take() {
            self.apply_project_setting_range_intent(key, value);
        }
        if let Some((key, value)) = self.pending_project_text.take() {
            self.apply_project_setting_text_intent(key, value);
        }
        if let Some((key, value)) = self.pending_project_setting.take() {
            self.apply_project_setting_toggle_intent(key, value);
        }
        if let Some(command) = self.pending_project_command.take() {
            self.apply_project_setting_command_intent(command);
        }
        self.maybe_auto_save();
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
        if let Some(runtime) = self.runtime.as_ref() {
            let auto_save_deadline = self.auto_save_deadline();
            match runtime.seconds_until_next_frame(self.elapsed_seconds()) {
                Some(delay) if delay <= f64::EPSILON => {
                    event_loop.set_control_flow(ControlFlow::Wait);
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                }
                Some(delay) if delay.is_finite() => {
                    let frame_deadline =
                        Instant::now() + Duration::from_secs_f64(delay.max(0.0).min(60.0));
                    let deadline = auto_save_deadline
                        .map_or(frame_deadline, |auto_save| frame_deadline.min(auto_save));
                    event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
                }
                _ => {
                    if let Some(deadline) = auto_save_deadline {
                        event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
                    } else {
                        event_loop.set_control_flow(ControlFlow::Wait);
                    }
                }
            }
        } else {
            if let Some(deadline) = self.auto_save_deadline() {
                event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
            } else {
                event_loop.set_control_flow(ControlFlow::Wait);
            }
        }
    }

    fn return_to_hub(&mut self, open_create: bool) {
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
        self.attached_ledger = TransactionLedger::new();
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

    fn sync_attached_document_revision(&mut self) {
        let Some(project_path) = self.project.as_ref().map(|project| project.path.clone()) else {
            return;
        };
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

    fn persist_attached_ledger(&self) {
        if let Some(project) = self.project.as_ref() {
            self.persist_attached_ledger_for(project);
        }
    }

    fn persist_attached_ledger_for(&self, project: &Project) {
        if let Err(error) = self.attached_ledger.persist_for_project(&project.path) {
            tracing::warn!(%error, path = %project.path.display(), "attached agent state persistence failed");
        }
    }

    fn persist_settings(&self) {
        let directory = EngineSettings::user_config_dir();
        let mut settings = self.settings_state.clone();
        if !settings.ai_persist_credentials {
            for provider in &mut settings.ai_providers {
                provider.api_key.clear();
            }
        }
        if let Err(error) = settings.save(&directory) {
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

    #[test]
    fn project_setting_controls_mutate_persisted_values_and_runtime_policy() {
        let mut settings = ProjectSettings::default();

        assert!(
            apply_project_setting_toggle(
                &mut settings,
                "project-settings.allow-gpu-features",
                true,
            )
            .changed
        );
        assert!(settings.allow_gpu_features);

        assert!(
            apply_project_setting_range(
                &mut settings,
                "project-settings.depth-resolution-scale",
                0.73,
            )
            .changed
        );
        assert_eq!(settings.depth_resolution_scale, 0.75);

        assert!(
            apply_project_setting_command(&mut settings, "project-settings.preset.medium",).changed
        );
        assert_eq!(settings.runtime_render_preset, RenderPreset::Medium);

        assert!(
            apply_project_setting_toggle(&mut settings, "project-settings.language.cpp", true,)
                .changed
        );
        assert!(settings.allowed_script_languages.has(ScriptLanguage::Cpp));

        settings.show_hierarchy_panel = false;
        settings.show_properties_panel = false;
        let reset = apply_project_setting_command(&mut settings, "project-settings.reset-panels");
        assert!(reset.changed);
        assert!(reset.layout_changed);
        assert!(settings.show_hierarchy_panel && settings.show_properties_panel);
    }
}
