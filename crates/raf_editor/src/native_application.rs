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
    EngineSettings, RenderExecutionPolicy, RenderQuality, ScriptExecutionMode, ScriptLanguage,
    Theme,
};
use raf_core::project::{
    BuildingStyle, GeometryDetailMode, Project, ProjectSettings, ProjectType, TextureQualityMode,
    ViewportResolutionMode,
};
use raf_core::scene::SceneGraph;
use raf_core::session::{ProjectSessionRegistry, SessionId};
use raf_core::TransactionLedger;
use raf_render::api_graphic_basic::ui_surface::{
    NativeUiInputBridge, NativeUiWindowConfig, NativeUiWindowHost, PresentationHealth,
    StudioUiPalette, UiAction, UiDispatchedAction,
};
use raf_render::api_graphic_basic::{GraphicsAdapterPreference, NativeEditorCompositor};
use raf_ui::{UiColorMode, UiEnvironment, UiResizeEdge, UiWindowCommand};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

use crate::attached::AttachedCommandHost;
use crate::commands::game::GameViewportPort;
use crate::editor_layout::EditorLayoutRequest;
use crate::electronics_controller::NativeElectronicsEditor;
use crate::native_attached_executor::poll_attached_commands;
use crate::native_editor_commands::apply_workbench_intents;
use crate::native_editor_runtime::NativeEditorRuntime;
use crate::native_electronics::NativeElectronicsCanvas;
use crate::native_project_controller::{
    initial_node_graph, initial_node_graph_for_session, initial_project, initial_scene_for_session,
    project_capabilities, save_project_document, save_project_document_for_session,
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

mod document;
mod event_loop;
mod input;
mod layout;
mod presentation;
mod project_settings;
mod runtime;
mod settings;
mod shortcuts;
mod studio;
#[cfg(test)]
mod tests;
mod window;

#[path = "native_agents.rs"]
mod agents;

pub use event_loop::run_native;

const NATIVE_EDITOR_SIZE: [f64; 2] = [1440.0, 900.0];
const NATIVE_EDITOR_MIN_SIZE: [f64; 2] = [900.0, 600.0];
const NATIVE_SPLASH_SIZE: [f64; 2] = [620.0, 500.0];

/// Same source asset as the embedded executable resource and the in-app
/// application bar icon: the authored Rafi mark shown before any surface
/// draws it.
const NATIVE_WINDOW_ICON_PNG: &[u8] = include_bytes!("../../../editor/icon.png");

/// Builds the OS-level window icon (title bar and taskbar) from the authored
/// Rafi mark. Returns `None` when the PNG or the RGBA conversion fails; the
/// window then keeps the icon embedded by the executable resource.
fn editor_window_icon() -> Option<winit::window::Icon> {
    let image = image::load_from_memory(NATIVE_WINDOW_ICON_PNG)
        .ok()?
        .to_rgba8();
    let (width, height) = image.dimensions();
    winit::window::Icon::from_rgba(image.into_raw(), width, height)
        .map_err(|error| tracing::debug!(%error, "editor window icon rejected"))
        .ok()
}

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

pub struct NativeEditorApplication {
    #[cfg(windows)]
    accessibility: Option<crate::native_accessibility::NativeAccessibility>,
    event_loop_proxy: Option<EventLoopProxy<()>>,
    window: Option<Arc<Window>>,
    window_host: Option<raf_render::api_graphic_basic::ui_surface::NativeUiWindowHost>,
    compositor: Option<NativeEditorCompositor>,
    input: NativeUiInputBridge,
    runtime: Option<NativeEditorRuntime>,
    game_runtime: crate::native_game_runtime::NativeRuntimeController,
    runtime_notice: Option<crate::native_game_runtime::RuntimeNotice>,
    pending_runtime_command: Option<String>,
    next_runtime_frame: Instant,
    workbench: Option<NativeGameWorkbench>,
    agents: crate::agent_service::AgentService,
    agent_surface: Option<crate::agent_workspace_surface_host::AgentWorkspaceSurfaceHost>,
    agents_open: bool,
    pending_editor_project: Option<std::path::PathBuf>,
    pending_editor_document: Option<SessionId>,
    editor_project_error: Option<String>,
    pending_agents_open: bool,
    pending_lab_open: Option<&'static str>,
    last_live_agent_poll: Instant,
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
    startup_loading_presented: bool,
    startup_graphics_ready_ms: f32,
    electronics_canvas: Option<NativeElectronicsCanvas>,
    electronics_editor: Option<NativeElectronicsEditor>,
    electronics_frame_key: Option<(u64, [u32; 2])>,
    scene: SceneGraph,
    project: Option<Project>,
    attached_host: AttachedCommandHost,
    attached_ledger: TransactionLedger,
    boot_started_at: Instant,
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
    observed_scene_revision: u64,
    observed_scene_render_fingerprint: u64,
    pending_document_saved: bool,
    /// Set when part of a save failed. The dirty marker must not be cleared while
    /// this is true, otherwise the editor claims to be saved while one of the
    /// documents never reached the disk.
    pending_document_save_failed: bool,
    last_auto_save_at: Instant,
}

impl Default for NativeEditorApplication {
    fn default() -> Self {
        let boot_started_at = Instant::now();
        // Project/session I/O is deliberately deferred until the loading
        // surface has presented. `Default` runs before Winit owns a window;
        // doing the reads here made `cargo run` pay the whole project cost
        // before the user could see any progress.
        let scene = SceneGraph::new();
        let observed_scene_revision = scene.document_revision();
        let observed_scene_render_fingerprint = scene.render_fingerprint();
        Self {
            #[cfg(windows)]
            accessibility: None,
            event_loop_proxy: None,
            window: None,
            window_host: None,
            compositor: None,
            input: NativeUiInputBridge::default(),
            runtime: None,
            game_runtime: crate::native_game_runtime::NativeRuntimeController::default(),
            runtime_notice: None,
            pending_runtime_command: None,
            next_runtime_frame: Instant::now(),
            workbench: None,
            agents: Default::default(),
            agent_surface: None,
            agents_open: false,
            pending_editor_project: None,
            pending_editor_document: None,
            editor_project_error: None,
            pending_agents_open: false,
            pending_lab_open: None,
            last_live_agent_poll: Instant::now(),
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
            startup_loading_presented: false,
            startup_graphics_ready_ms: 0.0,
            electronics_canvas: None,
            electronics_editor: None,
            electronics_frame_key: None,
            scene,
            project: None,
            attached_host: AttachedCommandHost::start(),
            attached_ledger: TransactionLedger::default(),
            boot_started_at,
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
            observed_scene_revision,
            observed_scene_render_fingerprint,
            pending_document_saved: false,
            pending_document_save_failed: false,
            last_auto_save_at: Instant::now(),
        }
    }
}
