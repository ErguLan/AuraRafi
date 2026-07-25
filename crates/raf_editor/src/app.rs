//! Main application - ties together loading screen, project hub, and editor.
//!
//! Application flow:
//! 1. Loading screen (brief, shows branding)
//! 2. Project Hub (recent projects + create new: Game or Electronics)
//! 3. Main Editor (viewport, hierarchy, properties, assets, console, AI chat,
//!    node editor, schematic view)

use chrono::Utc;
use eframe::egui;
use eframe::egui_wgpu;
use raf_assets::PrimitiveModelManifest;
use raf_core::config::{EngineSettings, Language, Theme};
use raf_core::i18n::t;
use raf_core::project::{Project, ProjectType, RecentProjects};
use raf_core::scene::graph::Primitive;
use raf_core::scene::SceneGraph;
use raf_core::session::ProjectSessionRegistry;
use raf_render::api_graphic_basic::device::SharedGraphicsContext;
use raf_render::api_graphic_basic::ui_surface::{
    NativeApplicationMenuAdapter, NativeWindowApplicationMenuAdapter,
};
use raf_render::bridge::{GraphicsSurfaceKind, RenderRuntime, RenderRuntimeSnapshot};
use raf_render::render_config::RenderConfig;
use raf_render::WorldStreamConfig;
use raw_window_handle::HasWindowHandle;
use std::time::Duration;

#[path = "panels/hub.rs"]
mod hub;

use crate::agent_executor::AgentEditorAction;
use crate::application_menu::{
    build_editor_application_menu, command as application_menu_command,
    show_eframe_application_menu, EditorApplicationMenuState,
};
use crate::commands::sessions::SessionCommandEvent;
use crate::commands::{parse_console_input, CommandCatalog, CommandOutput, ParsedInput};
use crate::editor_shell::{EditorShellLayout, PANEL_BOTTOM, PANEL_HIERARCHY, PANEL_PROPERTIES};
use crate::editor_shell_surface::{EditorBottomDockTab, EditorCenterSurface, EditorInspectorTab};
use crate::frame_timing::FrameTiming;
use crate::game_runtime::GameRuntimeState;
use crate::panels::agent_surface::AgentSurfaceHost;
use crate::panels::ai_chat::AgentPanel;
use crate::panels::asset_browser::AssetBrowserPanel;
use crate::panels::asset_browser_surface::{
    AssetBrowserAction, AssetBrowserSurfaceHost, AssetDialogAction,
};
use crate::panels::common_dialog_surface::{CommonDialogAction, CommonDialogSurfaceHost};
use crate::panels::console::{ConsolePanel, LogLevel};
use crate::panels::console_surface_host::ConsoleSurfaceHost;
use crate::panels::editor_bottom_chrome_surface::{
    EditorBottomChromeAction, EditorBottomChromeSurfaceHost,
};
use crate::panels::editor_bottom_tabs_host::EditorBottomTabsHost;
use crate::panels::editor_context_actions_surface::{
    EditorContextAction, EditorContextActionsSurfaceHost,
};
use crate::panels::editor_context_tabs_host::EditorContextTabsHost;
use crate::panels::editor_inspector_tabs_host::EditorInspectorTabsHost;
use crate::panels::editor_status_surface::EditorStatusSurfaceHost;
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
use crate::panels::game_surface::{
    GameHierarchyAction, GameHierarchySurfaceHost, GamePropertiesAction, GamePropertiesSurfaceHost,
};
use crate::panels::game_viewport_surface::{GameViewportSurfaceAction, GameViewportSurfaceHost};
use crate::panels::hierarchy::HierarchyPanel;
use crate::panels::hub_surface_host::{HubSurfaceHost, HubSurfaceIntent};
use crate::panels::loading_surface::LoadingSurfaceHost;
use crate::panels::new_project_surface::{NewProjectSurfaceAction, NewProjectSurfaceHost};
use crate::panels::node_editor::NodeEditorDocument;
use crate::panels::node_editor::NodeEditorPanel;
use crate::panels::pcb_view::{PcbSelection, PcbViewPanel};
use crate::panels::project_settings_surface_host::ProjectSettingsSurfaceHost;
use crate::panels::raf_ui_studio_surface::RafUiStudioSurfaceHost;
use crate::panels::schematic_view::{SchematicSelection, SchematicViewPanel};
use crate::panels::sessions_surface::{SessionsSurfaceAction, SessionsSurfaceHost};
use crate::panels::settings_surface_host::{SettingsSurfaceHost, SettingsSurfaceIntent};
use crate::panels::viewport::ViewportPanel;
use crate::pcb_document::{load_pcb_document, save_pcb_document};
use crate::schematic_document::{load_schematic_document, save_schematic_document};
use crate::session_document::{load_game_session, load_ui_document, save_game_session};
use crate::theme as app_theme;
use crate::ui_icons::UiIconAtlas;
use raf_ai::{AgentStatus, AssetImageGenerationQueue, AssetImageJobStatus};
use raf_ui::{StudioUiPalette, UiDocument, UiRect};

// ---------------------------------------------------------------------------
// Application state machine
// ---------------------------------------------------------------------------

/// Current screen of the application.
#[derive(Debug, Clone, PartialEq)]
enum AppScreen {
    /// Loading screen with progress.
    Loading { progress: f32, start_time: f64 },
    /// Project hub: choose recent or create new.
    ProjectHub,
    /// Create new project form.
    NewProject {
        name: String,
        path: String,
        project_type: ProjectType,
    },
    /// Main editor.
    Editor,
    /// Settings screen (overlay).
    Settings,
    /// RafUI Studio authoring workspace.
    RafUiStudio,
}

/// Bottom panel tab selection in the editor.
#[derive(Debug, Clone, PartialEq, Eq)]
enum BottomTab {
    Assets,
    Console,
    Drc,
    Simulation,
    AiChat,
    NodeEditor,
    ProjectSettings,
    Complement(String),
}

/// Temporary inspector switch while the retained UI surface is introduced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InspectorTab {
    Properties,
    Sessions,
}

/// Central viewport mode for the editor body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewportMode {
    Scene,
    Schematic,
    Pcb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShellResizeEdge {
    Left,
    Right,
    Top,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingExitAction {
    ToHub,
    QuitApp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HubProjectFilter {
    All,
    Game,
    Electronics,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EditorHistorySnapshot {
    Scene(String),
    Schematic(String),
    Pcb(String),
    UiDocument(String),
}

const EDITOR_UI_ICONS: &[&str] = &[
    "3-dots vertical.png",
    "ai_chat.png",
    "assets.png",
    "audio.png",
    "console.png",
    "complement.png",
    "cube.png",
    "cylinder.png",
    "empty.png",
    "folder.png",
    "focus.png",
    "hidden.png",
    "image.png",
    "material.png",
    "model.png",
    "move.png",
    "node_editor.png",
    "object_mode.png",
    "opacity.png",
    "plane.png",
    "project_settings.png",
    "rotate.png",
    "scene.png",
    "script.png",
    "select.png",
    "shape.png",
    "sphere.png",
    "sprite.png",
    "transform.png",
    "variables.png",
    "vertex_mode.png",
    "visible.png",
];

const HUB_UI_ICONS: &[&str] = &[
    "delete_HUB.png",
    "duplicate_HUB.png",
    "favorite_pin_HUB.png",
    "open_HUB.png",
    "project_type_HUB.png",
    "search_filter_HUB.png",
    "settings_HUB.png",
    "project_game.png",
    "project_electronics.png",
];

const SPLASH_MIN_DURATION_SECONDS: f64 = 2.4;
const SPLASH_MAX_DURATION_SECONDS: f64 = 6.0;
const SPLASH_ICON_UPLOAD_BUDGET: usize = 12;

// ---------------------------------------------------------------------------
// Main app
// ---------------------------------------------------------------------------

/// The AuraRafi editor application.
pub struct AuraRafiApp {
    // State
    screen: AppScreen,
    previous_screen: Option<AppScreen>,
    settings: EngineSettings,
    settings_draft: Option<EngineSettings>,
    recent_projects: RecentProjects,

    // Active project
    current_project: Option<Project>,
    sessions: ProjectSessionRegistry,
    ui_document: UiDocument,
    scene: SceneGraph,
    runtime: Option<GameRuntimeState>,

    // Editor panels
    viewport: ViewportPanel,
    hierarchy: HierarchyPanel,
    game_hierarchy_surface: GameHierarchySurfaceHost,
    game_properties_surface: GamePropertiesSurfaceHost,
    game_viewport_surface: GameViewportSurfaceHost,
    sessions_surface: SessionsSurfaceHost,
    asset_browser: AssetBrowserPanel,
    asset_browser_surface: AssetBrowserSurfaceHost,
    console: ConsolePanel,
    console_surface: ConsoleSurfaceHost,
    ai_chat: AgentPanel,
    agent_surface: AgentSurfaceHost,
    node_editor: NodeEditorPanel,
    schematic_view: SchematicViewPanel,
    pcb_view: PcbViewPanel,
    electronics_navigator_surface: ElectronicsNavigatorSurfaceHost,
    electronics_inspector_surface: ElectronicsInspectorSurfaceHost,
    electronics_toolbar_surface: ElectronicsToolbarSurfaceHost,
    electronics_analysis_surface: ElectronicsAnalysisSurfaceHost,
    electronics_drc_report: Option<raf_electronics::drc::DrcReport>,
    electronics_simulation_results: Option<raf_electronics::simulation::SimulationResults>,

    // Editor state
    bottom_tab: BottomTab,
    bottom_panel_snap_height: Option<f32>,
    inspector_tab: InspectorTab,
    viewport_mode: ViewportMode,
    _show_settings: bool,

    // Extensions
    command_catalog: CommandCatalog,
    complement_registry: raf_core::complement::ComplementRegistry,
    frame_count: u64,
    frame_timing: FrameTiming,
    graphics_runtime: RenderRuntime,

    // v0.3.0: UX state
    /// Whether scene has unsaved changes.
    scene_modified: bool,
    /// Last status message for the status bar.
    last_action: String,
    /// Elapsed seconds since last auto-save.
    auto_save_elapsed: f32,
    /// Last absolute timestamp used to advance the auto-save timer.
    auto_save_last_tick: Option<f64>,
    /// Snapshots for undo in the active editor document.
    undo_stack: Vec<EditorHistorySnapshot>,
    /// Snapshots for redo.
    redo_stack: Vec<EditorHistorySnapshot>,
    /// Pending snapshot used to collapse continuous edits into one undo step.
    pending_history_snapshot: Option<EditorHistorySnapshot>,
    /// Project logo texture.
    logo_texture: Option<egui::TextureHandle>,
    egui_wgpu_render_state: Option<egui_wgpu::RenderState>,
    native_application_menu: Option<NativeWindowApplicationMenuAdapter>,
    native_menu_bound_to_frame: bool,
    ui_icons: UiIconAtlas,
    hub_surface: HubSurfaceHost,
    new_project_surface: NewProjectSurfaceHost,
    loading_surface: LoadingSurfaceHost,
    settings_surface: SettingsSurfaceHost,
    raf_ui_studio_surface: RafUiStudioSurfaceHost,
    project_settings_surface: ProjectSettingsSurfaceHost,
    bottom_tabs_surface: EditorBottomTabsHost,
    bottom_chrome_surface: EditorBottomChromeSurfaceHost,
    context_tabs_surface: EditorContextTabsHost,
    context_actions_surface: EditorContextActionsSurfaceHost,
    status_surface: EditorStatusSurfaceHost,
    inspector_tabs_surface: EditorInspectorTabsHost,
    common_dialog_surface: CommonDialogSurfaceHost,
    editor_shell: EditorShellLayout,
    editor_shell_dirty: bool,
    editor_shell_resize_panel: Option<&'static str>,
    hub_search_query: String,
    hub_filter: HubProjectFilter,
    pending_exit_action: Option<PendingExitAction>,
    allow_app_close: bool,
    show_settings_close_dialog: bool,
    /// Scene nodes copied with Ctrl+C (game mode clipboard).
    scene_clipboard: Vec<raf_core::scene::graph::SceneNode>,
    /// Camera bookmarks for 3 slots: (target, yaw, pitch, distance).
    camera_bookmarks: [Option<(glam::Vec3, f32, f32, f32)>; 3],
    /// Cross-probe: designator of the component to highlight when switching
    /// between Schematic and PCB views. Set when selecting a component in
    /// either view; consumed by the other view on mode switch.
    cross_probe_designator: Option<String>,
    image_generation: AssetImageGenerationQueue,
    pending_session_events: Vec<SessionCommandEvent>,
    pending_agent_editor_actions: Vec<AgentEditorAction>,
}

impl AuraRafiApp {
    /// Create the application.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Load settings from app data directory.
        let config_dir = dirs_config_dir();
        let settings = EngineSettings::load(&config_dir);
        let recent_projects = RecentProjects::load(&config_dir);

        // Apply initial theme.
        app_theme::apply_theme(&cc.egui_ctx, settings.theme, settings.theme_experimental);

        // Set font size.
        let mut style = (*cc.egui_ctx.style()).clone();
        style.text_styles.iter_mut().for_each(|(_, font_id)| {
            font_id.size = settings.font_size;
        });
        cc.egui_ctx.set_style(style);

        let egui_wgpu_render_state = cc.wgpu_render_state.clone();
        let native_application_menu = Some(NativeWindowApplicationMenuAdapter::default());
        let mut graphics_runtime = RenderRuntime::default();
        if let Some(render_state) = &egui_wgpu_render_state {
            graphics_runtime.set_shared_graphics_context(Some(SharedGraphicsContext::from_host(
                render_state.device.clone(),
                render_state.queue.clone(),
            )));
        }

        Self {
            screen: AppScreen::Loading {
                progress: 0.0,
                start_time: 0.0,
            },
            previous_screen: None,
            settings,
            settings_draft: None,
            recent_projects,
            current_project: None,
            sessions: ProjectSessionRegistry::new(ProjectType::Game),
            ui_document: UiDocument::default(),
            scene: SceneGraph::new(),
            runtime: None,
            viewport: ViewportPanel::default(),
            hierarchy: HierarchyPanel::default(),
            game_hierarchy_surface: GameHierarchySurfaceHost::default(),
            game_properties_surface: GamePropertiesSurfaceHost::default(),
            game_viewport_surface: GameViewportSurfaceHost::default(),
            sessions_surface: SessionsSurfaceHost::default(),
            asset_browser: AssetBrowserPanel::default(),
            asset_browser_surface: AssetBrowserSurfaceHost::default(),
            console: ConsolePanel::default(),
            console_surface: ConsoleSurfaceHost::default(),
            ai_chat: AgentPanel::default(),
            agent_surface: AgentSurfaceHost::default(),
            node_editor: NodeEditorPanel::default(),
            schematic_view: SchematicViewPanel::default(),
            pcb_view: PcbViewPanel::default(),
            electronics_navigator_surface: ElectronicsNavigatorSurfaceHost::default(),
            electronics_inspector_surface: ElectronicsInspectorSurfaceHost::default(),
            electronics_toolbar_surface: ElectronicsToolbarSurfaceHost::default(),
            electronics_analysis_surface: ElectronicsAnalysisSurfaceHost::default(),
            electronics_drc_report: None,
            electronics_simulation_results: None,

            command_catalog: CommandCatalog::builtin(),
            complement_registry: raf_core::complement::ComplementRegistry::new(),
            graphics_runtime,

            bottom_tab: BottomTab::Console,
            bottom_panel_snap_height: None,
            inspector_tab: InspectorTab::Properties,
            viewport_mode: ViewportMode::Scene,
            _show_settings: false,
            frame_count: 0,
            frame_timing: FrameTiming::default(),
            scene_modified: false,
            last_action: String::new(),
            auto_save_elapsed: 0.0,
            auto_save_last_tick: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            pending_history_snapshot: None,
            logo_texture: None,
            egui_wgpu_render_state,
            native_application_menu,
            native_menu_bound_to_frame: false,
            ui_icons: UiIconAtlas::default(),
            hub_surface: HubSurfaceHost::default(),
            new_project_surface: NewProjectSurfaceHost::default(),
            loading_surface: LoadingSurfaceHost::default(),
            settings_surface: SettingsSurfaceHost::default(),
            raf_ui_studio_surface: RafUiStudioSurfaceHost::default(),
            project_settings_surface: ProjectSettingsSurfaceHost::default(),
            bottom_tabs_surface: EditorBottomTabsHost::default(),
            bottom_chrome_surface: EditorBottomChromeSurfaceHost::default(),
            context_tabs_surface: EditorContextTabsHost::default(),
            context_actions_surface: EditorContextActionsSurfaceHost::default(),
            status_surface: EditorStatusSurfaceHost::default(),
            inspector_tabs_surface: EditorInspectorTabsHost::default(),
            common_dialog_surface: CommonDialogSurfaceHost::default(),
            editor_shell: EditorShellLayout::default(),
            editor_shell_dirty: false,
            editor_shell_resize_panel: None,
            hub_search_query: String::new(),
            hub_filter: HubProjectFilter::All,
            pending_exit_action: None,
            allow_app_close: false,
            show_settings_close_dialog: false,
            scene_clipboard: Vec::new(),
            camera_bookmarks: [None, None, None],
            cross_probe_designator: None,
            image_generation: AssetImageGenerationQueue::default(),
            pending_session_events: Vec::new(),
            pending_agent_editor_actions: Vec::new(),
        }
    }
}

impl eframe::App for AuraRafiApp {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        if matches!(self.screen, AppScreen::Loading { .. }) {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            let _ = visuals;
            [8.0 / 255.0, 11.0 / 255.0, 15.0 / 255.0, 1.0]
        }
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.frame_count += 1;
        let loading_screen = matches!(self.screen, AppScreen::Loading { .. });
        if !loading_screen {
            ctx.send_viewport_cmd(egui::ViewportCommand::Transparent(false));
            self.bind_native_application_menu_to_frame(frame);
            self.poll_native_application_menu();
        }
        self.poll_image_generation();

        // Auto-detect system DPI once at startup when auto_ui_scale is enabled.
        // Must run before borrowing active_settings to avoid borrow conflict.
        if self.settings.auto_ui_scale && self.frame_count == 1 {
            if let Some(native_ppp) = ctx.native_pixels_per_point() {
                if native_ppp > 0.0 {
                    self.settings.ui_scale = native_ppp.clamp(1.0, 2.0);
                    let _ = self.settings.save(&dirs_config_dir());
                }
            }
        }

        let active_settings = self.active_ui_settings();
        app_theme::apply_theme(
            ctx,
            active_settings.theme,
            active_settings.theme_experimental,
        );

        let scale = if active_settings.auto_ui_scale {
            self.settings.ui_scale
        } else {
            active_settings.ui_scale
        };
        ctx.set_pixels_per_point(scale.clamp(0.5, 3.0));
        let mut style = (*ctx.style()).clone();
        style.text_styles.iter_mut().for_each(|(_, font_id)| {
            font_id.size = active_settings.font_size;
        });
        ctx.set_style(style);

        self.handle_window_close_request(ctx);

        if !matches!(&self.screen, AppScreen::Editor) {
            self.graphics_runtime
                .activate_surface(GraphicsSurfaceKind::None);
        }

        match self.screen.clone() {
            AppScreen::Loading {
                progress,
                start_time,
            } => {
                self.show_loading(ctx, progress, start_time);
            }
            AppScreen::ProjectHub => {
                self.show_project_hub_raf_ui(ctx);
            }
            AppScreen::NewProject {
                name,
                path,
                project_type,
            } => {
                self.show_new_project(ctx, name, path, project_type);
            }
            AppScreen::Editor => {
                self.show_editor(ctx);
            }
            AppScreen::Settings => {
                self.show_settings_screen(ctx);
            }
            AppScreen::RafUiStudio => {
                self.show_rafui_studio_screen(ctx);
            }
        }

        self.show_unsaved_changes_dialog(ctx);
        if !matches!(self.screen, AppScreen::Loading { .. }) {
            self.sync_native_application_menu();
        }
    }
}

impl AuraRafiApp {
    fn active_ui_settings(&self) -> &EngineSettings {
        if matches!(self.screen, AppScreen::Settings) {
            self.settings_draft.as_ref().unwrap_or(&self.settings)
        } else {
            &self.settings
        }
    }

    /// Active retained project Hub. The legacy egui implementation remains
    /// compiled as a recovery path while the editor shell itself migrates.
    fn show_project_hub_raf_ui(&mut self, ctx: &egui::Context) {
        let lang = self.settings.language;
        let palette = if ctx.style().visuals.dark_mode {
            StudioUiPalette::IndustrialDark
        } else {
            StudioUiPalette::PaperLight
        };
        let surface_filter = match self.hub_filter {
            HubProjectFilter::All => crate::studio_surface::HubSurfaceFilter::All,
            HubProjectFilter::Game => crate::studio_surface::HubSurfaceFilter::Game,
            HubProjectFilter::Electronics => crate::studio_surface::HubSurfaceFilter::Electronics,
        };
        let visible_projects = hub::filtered_recent_projects(
            &self.recent_projects.projects,
            self.hub_filter,
            &self.hub_search_query,
        );
        let mut intents = Vec::new();

        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                intents = self.hub_surface.show(
                    ui,
                    self.egui_wgpu_render_state.as_ref(),
                    palette,
                    lang,
                    self.settings.theme,
                    surface_filter,
                    &self.recent_projects.projects,
                    &visible_projects,
                    &self.hub_search_query,
                );
            });

        for intent in intents {
            self.apply_hub_surface_intent(intent);
        }
    }

    fn apply_hub_surface_intent(&mut self, intent: HubSurfaceIntent) {
        match intent {
            HubSurfaceIntent::Open(path) => self.open_project(&path),
            HubSurfaceIntent::Duplicate(path) => self.duplicate_project_from_hub(&path),
            HubSurfaceIntent::Forget(path) => {
                self.recent_projects
                    .projects
                    .retain(|entry| entry.path != path);
                let _ = self.recent_projects.save(&dirs_config_dir());
            }
            HubSurfaceIntent::NewProject(project_type) => {
                self.screen = AppScreen::NewProject {
                    name: String::new(),
                    path: default_projects_dir(),
                    project_type,
                };
            }
            HubSurfaceIntent::OpenSettings => self.open_settings_screen(AppScreen::ProjectHub),
            HubSurfaceIntent::SetSearch(query) => self.hub_search_query = query,
            HubSurfaceIntent::SetFilter(filter) => {
                self.hub_filter = match filter {
                    crate::studio_surface::HubSurfaceFilter::All => HubProjectFilter::All,
                    crate::studio_surface::HubSurfaceFilter::Game => HubProjectFilter::Game,
                    crate::studio_surface::HubSurfaceFilter::Electronics => {
                        HubProjectFilter::Electronics
                    }
                };
            }
            HubSurfaceIntent::SetTheme(theme) => {
                self.settings.theme = theme;
                let _ = self.settings.save(&dirs_config_dir());
            }
        }
    }

    fn prepare_graphics_surface(&mut self, surface: GraphicsSurfaceKind) -> RenderRuntimeSnapshot {
        let allow_advanced_gpu_features = self
            .current_project
            .as_ref()
            .map(|project| project.settings.allow_gpu_features)
            .unwrap_or(false);

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

    // -----------------------------------------------------------------------
    // Loading Screen
    // -----------------------------------------------------------------------

    fn show_loading(&mut self, ctx: &egui::Context, _progress: f32, start_time: f64) {
        let time = ctx.input(|i| i.time);
        let start = if start_time == 0.0 { time } else { start_time };
        let elapsed = (time - start).max(0.0);

        // Warm the icons used by the Hub and editor while the splash is
        // visible. The splash progress therefore represents actual startup
        // work instead of only a fixed visual delay.
        self.ui_icons.request_icons(HUB_UI_ICONS);
        self.ui_icons.request_icons(EDITOR_UI_ICONS);
        self.ui_icons
            .process_load_budget(ctx, SPLASH_ICON_UPLOAD_BUDGET);
        let loaded_icons = self.ui_icons.ready_or_failed_count(HUB_UI_ICONS)
            + self.ui_icons.ready_or_failed_count(EDITOR_UI_ICONS);
        let icon_total = HUB_UI_ICONS.len() + EDITOR_UI_ICONS.len();
        let icon_progress = if icon_total == 0 {
            1.0
        } else {
            loaded_icons as f32 / icon_total as f32
        };
        let time_progress = (elapsed / SPLASH_MIN_DURATION_SECONDS).clamp(0.0, 1.0) as f32;
        let startup_ready = loaded_icons >= icon_total;
        let timed_out = elapsed >= SPLASH_MAX_DURATION_SECONDS;
        let ready_to_enter = (startup_ready && elapsed >= SPLASH_MIN_DURATION_SECONDS) || timed_out;
        let new_progress = if ready_to_enter {
            1.0
        } else {
            (icon_progress * 0.82 + time_progress * 0.18).min(0.96)
        };

        if self.frame_count == 1 {
            if let Some(command) = egui::ViewportCommand::center_on_screen(ctx) {
                ctx.send_viewport_cmd(command);
            }
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                self.loading_surface.show(
                    ui,
                    self.egui_wgpu_render_state.as_ref(),
                    StudioUiPalette::IndustrialDark,
                    new_progress,
                    self.settings.language,
                );
            });
        if ready_to_enter {
            self.expand_window_after_loading(ctx);
            self.screen = AppScreen::ProjectHub;
        } else {
            self.screen = AppScreen::Loading {
                progress: new_progress,
                start_time: start,
            };
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn expand_window_after_loading(&self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Transparent(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(
            800.0, 500.0,
        )));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1280.0, 720.0)));
    }

    // -----------------------------------------------------------------------
    // New Project Form
    // -----------------------------------------------------------------------

    fn show_new_project(
        &mut self,
        ctx: &egui::Context,
        mut name: String,
        mut path: String,
        project_type: ProjectType,
    ) {
        let palette = if ctx.style().visuals.dark_mode {
            StudioUiPalette::IndustrialDark
        } else {
            StudioUiPalette::PaperLight
        };
        let render_state = self.egui_wgpu_render_state.as_ref();
        let mut actions = Vec::new();
        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                actions = self.new_project_surface.show(
                    ui,
                    render_state,
                    palette,
                    &name,
                    &path,
                    project_type,
                    self.settings.language,
                );
            });

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
            let project_path = std::path::PathBuf::from(path.trim());
            match Project::create(name.trim(), project_type, &project_path) {
                Ok(project) => {
                    self.console.log(
                        LogLevel::Info,
                        &format!("Project '{}' created", project.name),
                    );
                    let config_dir = dirs_config_dir();
                    self.recent_projects.add(&project);
                    let _ = self.recent_projects.save(&config_dir);
                    self.current_project = Some(project.clone());
                    self.editor_shell = EditorShellLayout::load(&project.path);
                    self.editor_shell_dirty = false;
                    self.editor_shell_resize_panel = None;
                    self.sessions =
                        ProjectSessionRegistry::load_or_legacy(&project.path, project_type);
                    let assets_dir = std::path::PathBuf::from(&project.path).join("assets");
                    self.asset_browser.project_assets_path = Some(assets_dir);
                    self.asset_browser.scan_project_folder();
                    if let Err(error) = self.load_active_session_documents(&project) {
                        self.console.log(LogLevel::Error, &error);
                    }
                    self.screen = AppScreen::Editor;
                    return;
                }
                Err(error) => {
                    self.console.log(
                        LogLevel::Error,
                        &format!("Failed to create project: {error}"),
                    );
                }
            }
        }

        if self.screen != AppScreen::ProjectHub && self.screen != AppScreen::Editor {
            self.screen = AppScreen::NewProject {
                name,
                path,
                project_type,
            };
        }
    }

    // -----------------------------------------------------------------------
    // Main Editor
    // -----------------------------------------------------------------------

    fn apply_game_hierarchy_surface_actions(&mut self, actions: Vec<GameHierarchyAction>) {
        for action in actions {
            match action {
                GameHierarchyAction::Select(id) => {
                    if self.scene.get(id).is_some() {
                        self.hierarchy.selected_node = Some(id);
                        self.hierarchy.selected_nodes = vec![id];
                        self.viewport.selected = vec![id];
                    }
                }
                GameHierarchyAction::ToggleVisibility(id) => {
                    self.push_undo_snapshot();
                    let toggled_name = if let Some(node) = self.scene.get_mut(id) {
                        node.visible = !node.visible;
                        Some(node.name.clone())
                    } else {
                        None
                    };
                    if let Some(name) = toggled_name {
                        let msg = format!(
                            "{} {}",
                            t("app.visibility_toggled", self.settings.language),
                            name
                        );
                        self.last_action = msg.clone();
                        self.console.log(LogLevel::Info, &msg);
                        self.mark_scene_modified();
                    }
                }
                GameHierarchyAction::Delete(id) => {
                    self.push_undo_snapshot();
                    let name = self
                        .scene
                        .get(id)
                        .map(|node| node.name.clone())
                        .unwrap_or_default();
                    if self.scene.remove_node(id) {
                        self.hierarchy.selected_node = None;
                        self.hierarchy.selected_nodes.clear();
                        self.viewport.selected.clear();
                        let msg =
                            format!("{} {}", t("app.deleted_msg", self.settings.language), name);
                        self.last_action = msg.clone();
                        self.console.log(LogLevel::Info, &msg);
                        self.mark_scene_modified();
                    }
                }
                GameHierarchyAction::Duplicate(id) => {
                    self.push_undo_snapshot();
                    if let Some(new_id) = self.scene.duplicate_node(id) {
                        self.hierarchy.selected_node = Some(new_id);
                        self.hierarchy.selected_nodes = vec![new_id];
                        self.viewport.selected = vec![new_id];
                        let name = self
                            .scene
                            .get(new_id)
                            .map(|node| node.name.clone())
                            .unwrap_or_default();
                        let msg = format!(
                            "{} {}",
                            t("app.duplicated_msg", self.settings.language),
                            name
                        );
                        self.last_action = msg.clone();
                        self.console.log(LogLevel::Info, &msg);
                        self.mark_scene_modified();
                    }
                }
                GameHierarchyAction::AddFolder(parent) => {
                    self.push_undo_snapshot();
                    let new_id = if let Some(parent_id) = parent {
                        self.scene.add_child_folder(parent_id, "Folder")
                    } else {
                        self.scene.add_root_folder("Folder")
                    };
                    self.hierarchy.selected_node = Some(new_id);
                    self.hierarchy.selected_nodes = vec![new_id];
                    self.viewport.selected = vec![new_id];
                    let msg = t("app.folder_created", self.settings.language);
                    self.last_action = msg.clone();
                    self.console.log(LogLevel::Info, &msg);
                    self.mark_scene_modified();
                }
                GameHierarchyAction::Ungroup(id) => {
                    self.push_undo_snapshot();
                    if self.scene.ungroup_node(id) {
                        self.hierarchy.selected_node = None;
                        self.hierarchy.selected_nodes.clear();
                        self.viewport.selected.clear();
                        let msg = t("app.ungrouped_msg", self.settings.language);
                        self.last_action = msg.clone();
                        self.console.log(LogLevel::Info, &msg);
                        self.mark_scene_modified();
                    }
                }
            }
        }
    }

    fn apply_game_properties_surface_actions(&mut self, actions: Vec<GamePropertiesAction>) {
        let Some(selected) = self.hierarchy.selected_node else {
            return;
        };

        for action in actions {
            match action {
                GamePropertiesAction::SetName(name) => {
                    let trimmed = name.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let changed = self
                        .scene
                        .get(selected)
                        .map(|node| node.name != trimmed)
                        .unwrap_or(false);
                    if changed {
                        if let Some(node) = self.scene.get_mut(selected) {
                            node.name = trimmed.to_string();
                        }
                        self.mark_scene_modified();
                    }
                }
                GamePropertiesAction::SetVisible(value) => {
                    for id in self.hierarchy.selected_nodes.clone() {
                        if let Some(node) = self.scene.get_mut(id) {
                            node.visible = value;
                        }
                    }
                    self.mark_scene_modified();
                }
                GamePropertiesAction::SetRange { key, value } => {
                    let Some((group, axis)) = key
                        .strip_prefix("game.properties.")
                        .and_then(|value| value.split_once('.'))
                    else {
                        continue;
                    };
                    if self.scene.get(selected).is_none() {
                        continue;
                    }
                    let Some(node) = self.scene.get_mut(selected) else {
                        continue;
                    };
                    let target = match group {
                        "position" => &mut node.position,
                        "rotation" => &mut node.rotation,
                        "scale" => &mut node.scale,
                        _ => continue,
                    };
                    match axis {
                        "x" => target.x = value,
                        "y" => target.y = value,
                        "z" => target.z = value,
                        _ => continue,
                    }
                    self.mark_scene_modified();
                }
                GamePropertiesAction::SetPrimitive(primitive) => {
                    if self.scene.get(selected).is_some() {
                        if let Some(node) = self.scene.get_mut(selected) {
                            node.primitive = primitive;
                            node.color =
                                raf_core::scene::graph::NodeColor::for_primitive(primitive);
                        }
                        self.mark_scene_modified();
                    }
                }
                GamePropertiesAction::ResetTransform => {
                    if self.scene.get(selected).is_some() {
                        if let Some(node) = self.scene.get_mut(selected) {
                            node.position = glam::Vec3::ZERO;
                            node.rotation = glam::Vec3::ZERO;
                            node.scale = glam::Vec3::ONE;
                        }
                        self.mark_scene_modified();
                    }
                }
                GamePropertiesAction::ResetAll => {
                    if self.scene.get(selected).is_some() {
                        if let Some(node) = self.scene.get_mut(selected) {
                            node.position = glam::Vec3::ZERO;
                            node.rotation = glam::Vec3::ZERO;
                            node.scale = glam::Vec3::ONE;
                            node.color =
                                raf_core::scene::graph::NodeColor::for_primitive(node.primitive);
                            node.visible = true;
                        }
                        self.mark_scene_modified();
                    }
                }
            }
        }
    }

    fn apply_game_viewport_surface_actions(&mut self, actions: Vec<GameViewportSurfaceAction>) {
        for action in actions {
            match action {
                GameViewportSurfaceAction::SetGizmo(mode) => {
                    self.viewport.set_gizmo_mode_from_ui(mode);
                }
                GameViewportSurfaceAction::ToggleSelect => {
                    self.viewport.toggle_select_mode_from_ui();
                }
                GameViewportSurfaceAction::SetMode(mode) => {
                    self.viewport.mode = mode;
                }
                GameViewportSurfaceAction::ToggleGrid => {
                    self.viewport.grid_visible = !self.viewport.grid_visible;
                }
                GameViewportSurfaceAction::ToggleLabels => {
                    self.viewport.show_labels = !self.viewport.show_labels;
                }
                GameViewportSurfaceAction::ToggleFocusLock => {
                    self.viewport.toggle_focus_lock_from_ui(&self.scene);
                }
                GameViewportSurfaceAction::ToggleEditMode => {
                    self.viewport.toggle_edit_mode_from_ui(&self.scene);
                }
                GameViewportSurfaceAction::SetRenderStyle(style) => {
                    self.settings.viewport_render_mode = match style {
                        crate::panels::viewport::RenderStyle::Solid => {
                            raf_core::config::ViewportRenderMode::Solid
                        }
                        crate::panels::viewport::RenderStyle::Wireframe => {
                            raf_core::config::ViewportRenderMode::Wireframe
                        }
                        crate::panels::viewport::RenderStyle::Preview => {
                            raf_core::config::ViewportRenderMode::Preview
                        }
                    };
                    self.viewport.render_style = style;
                }
                GameViewportSurfaceAction::ResetView => {
                    self.viewport.reset_view_from_ui();
                }
                GameViewportSurfaceAction::Undo => self.do_undo(),
                GameViewportSurfaceAction::Redo => self.do_redo(),
            }
        }
    }

    fn apply_electronics_navigator_actions(&mut self, actions: Vec<ElectronicsNavigatorAction>) {
        for action in actions {
            match action {
                ElectronicsNavigatorAction::SchematicRoot => {
                    self.schematic_view.clear_selection();
                }
                ElectronicsNavigatorAction::SchematicComponent(index) => {
                    self.schematic_view.select_component(index);
                }
                ElectronicsNavigatorAction::SchematicWire(index) => {
                    self.schematic_view.select_wire(index);
                }
                ElectronicsNavigatorAction::PlaceComponent(index) => {
                    self.schematic_view.begin_component_placement(index);
                }
                ElectronicsNavigatorAction::PcbRoot => {
                    self.pcb_view.clear_selection();
                }
                ElectronicsNavigatorAction::PcbComponent(index) => {
                    self.pcb_view.select_component(index);
                }
                ElectronicsNavigatorAction::PcbTrace(index) => {
                    self.pcb_view.select_trace(index);
                }
                ElectronicsNavigatorAction::PcbAirwire(index) => {
                    self.pcb_view.select_airwire(index);
                }
            }
        }
    }

    fn add_primitive_to_scene(&mut self, primitive: Primitive) {
        self.push_undo_snapshot();
        let name = format!("{} {}", primitive.label(), self.scene.len() + 1);
        let source_asset = match primitive {
            Primitive::Cube => Some("builtin://primitive/cube"),
            Primitive::Sphere => Some("builtin://primitive/sphere"),
            Primitive::Plane => Some("builtin://primitive/plane"),
            Primitive::Cylinder => Some("builtin://primitive/cylinder"),
            Primitive::Empty => None,
        };
        match PrimitiveModelManifest::builtin_for_primitive(primitive) {
            Ok(Some(manifest)) => match manifest.instantiate_single_root_into_scene(
                &mut self.scene,
                Some(&name),
                source_asset,
            ) {
                Ok(id) => {
                    self.hierarchy.selected_node = Some(id);
                    self.hierarchy.selected_nodes = vec![id];
                    self.viewport.selected = vec![id];
                    self.mark_scene_modified();
                    let message = format!("Added: {name}");
                    self.last_action = message.clone();
                    self.console.log(LogLevel::Info, &message);
                }
                Err(error) => self.console.log(
                    LogLevel::Error,
                    &format!("Could not import primitive asset {name}: {error}"),
                ),
            },
            Ok(None) => self
                .console
                .log(LogLevel::Error, "This primitive has no asset manifest."),
            Err(error) => self.console.log(
                LogLevel::Error,
                &format!("Could not load primitive asset manifest: {error}"),
            ),
        }
    }

    fn apply_asset_browser_actions(&mut self, actions: Vec<AssetBrowserAction>) {
        let lang = self.settings.language;
        for action in actions {
            match action {
                AssetBrowserAction::SetSearch(value) => {
                    self.asset_browser.search_query = value;
                }
                AssetBrowserAction::SetFilter(filter) => {
                    self.asset_browser.selected_filter = filter;
                }
                AssetBrowserAction::OpenFolder => self.asset_browser.open_assets_folder(),
                AssetBrowserAction::Refresh => self.asset_browser.scan_project_folder(),
                AssetBrowserAction::CreateScript(kind) => {
                    self.asset_browser.create_script_from_kind(lang, &kind);
                }
                AssetBrowserAction::AddPrimitive(primitive) => {
                    self.add_primitive_to_scene(primitive);
                }
                AssetBrowserAction::OpenScript(index) => {
                    self.asset_browser.open_script_entry(index);
                }
            }
        }
    }

    fn apply_sessions_surface_actions(&mut self, actions: Vec<SessionsSurfaceAction>) {
        for action in actions {
            match action {
                SessionsSurfaceAction::Activate(id) => {
                    if let Err(error) = self.activate_session(id) {
                        self.console.log(LogLevel::Error, &error);
                    }
                }
                SessionsSurfaceAction::Create { name, kind } => {
                    if let Err(error) = self.create_and_activate_session(name, kind) {
                        self.console.log(LogLevel::Error, &error);
                    }
                }
            }
        }
    }

    fn apply_schematic_inspector_actions(
        &mut self,
        actions: Vec<ElectronicsInspectorAction>,
    ) -> bool {
        let selection = self.schematic_view.selection();
        let mut changed = false;
        for action in actions {
            match action {
                ElectronicsInspectorAction::Text { field, value } => match field.as_str() {
                    "electronics.schematic.reference" => {
                        if let SchematicSelection::Component(index) = selection {
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
                        if let SchematicSelection::Component(index) = selection {
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
                        if let SchematicSelection::Wire(index) = selection {
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
                    let before = match selection {
                        SchematicSelection::Component(index) => {
                            self.schematic_view.schematic.components.get(index).cloned()
                        }
                        _ => None,
                    };
                    if let SchematicSelection::Component(index) = selection {
                        if let Some(component) =
                            self.schematic_view.schematic.components.get_mut(index)
                        {
                            match field.as_str() {
                                "electronics.schematic.position.x" => component.position.x = value,
                                "electronics.schematic.position.y" => component.position.y = value,
                                "electronics.schematic.rotation" => component.rotation = value,
                                _ => continue,
                            }
                            changed = true;
                        }
                    }
                    if let Some(before) = before {
                        self.schematic_view
                            .ensure_wire_anchors_for_component_snapshot(&before);
                    }
                }
                ElectronicsInspectorAction::Toggle { field, value } => {
                    if let SchematicSelection::Component(index) = selection {
                        if let Some(component) =
                            self.schematic_view.schematic.components.get_mut(index)
                        {
                            match field.as_str() {
                                "electronics.schematic.visible" => component.visible = value,
                                "electronics.schematic.locked" => component.locked = value,
                                _ => continue,
                            }
                            changed = true;
                        }
                    }
                }
                ElectronicsInspectorAction::Layer { .. } => {}
            }
        }
        changed
    }

    fn apply_pcb_inspector_actions(&mut self, actions: Vec<ElectronicsInspectorAction>) -> bool {
        let selection = self.pcb_view.selection();
        let mut changed = false;
        for action in actions {
            match action {
                ElectronicsInspectorAction::Text { field, value } => {
                    if let PcbSelection::Component(index) = selection {
                        if let Some(component) = self.pcb_view.layout.components.get_mut(index) {
                            match field.as_str() {
                                "electronics.pcb.reference" => {
                                    if component.designator != value {
                                        component.designator = value;
                                        changed = true;
                                    }
                                }
                                "electronics.pcb.value" => {
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
                    PcbSelection::Component(index) => {
                        if let Some(component) = self.pcb_view.layout.components.get_mut(index) {
                            match field.as_str() {
                                "electronics.pcb.position.x" => component.position.x = value,
                                "electronics.pcb.position.y" => component.position.y = value,
                                "electronics.pcb.rotation" => component.rotation = value,
                                _ => continue,
                            }
                            changed = true;
                        }
                    }
                    PcbSelection::Trace(index) => {
                        if field == "electronics.pcb.trace.width" {
                            if let Some(trace) = self.pcb_view.layout.traces.get_mut(index) {
                                trace.width = value;
                                changed = true;
                            }
                        }
                    }
                    _ => {}
                },
                ElectronicsInspectorAction::Toggle { field, value } => {
                    if let PcbSelection::Component(index) = selection {
                        if field == "electronics.pcb.locked" {
                            if let Some(component) = self.pcb_view.layout.components.get_mut(index)
                            {
                                component.locked = value;
                                changed = true;
                            }
                        }
                    }
                }
                ElectronicsInspectorAction::Layer { field, value } => match selection {
                    PcbSelection::Component(index) if field == "electronics.pcb.layer" => {
                        if let Some(component) = self.pcb_view.layout.components.get_mut(index) {
                            component.layer = value;
                            changed = true;
                        }
                    }
                    PcbSelection::Trace(index) if field == "electronics.pcb.trace.layer" => {
                        if let Some(trace) = self.pcb_view.layout.traces.get_mut(index) {
                            trace.layer = value;
                            changed = true;
                        }
                    }
                    _ => {}
                },
            }
        }
        if changed {
            self.pcb_view.layout.rebuild_airwires();
        }
        changed
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
                ElectronicsToolbarAction::SchematicSelect => {
                    self.schematic_view.set_select_tool_from_ui();
                }
                ElectronicsToolbarAction::SchematicWire => {
                    self.schematic_view.set_wire_tool_from_ui();
                }
                ElectronicsToolbarAction::SchematicRotate => {
                    self.schematic_view.rotate_placement_from_ui();
                }
                ElectronicsToolbarAction::SchematicFit => {
                    self.schematic_view.fit_view_from_ui();
                }
                ElectronicsToolbarAction::SchematicLibrary => {
                    self.schematic_view.toggle_library_from_ui();
                }
                ElectronicsToolbarAction::SchematicTest => {
                    self.schematic_view.run_electrical_test_from_ui();
                }
                ElectronicsToolbarAction::SchematicDelete => {
                    changed |= self.schematic_view.delete_selection();
                }
                ElectronicsToolbarAction::SchematicZoomIn => {
                    self.schematic_view.zoom_in_from_ui();
                }
                ElectronicsToolbarAction::SchematicZoomOut => {
                    self.schematic_view.zoom_out_from_ui();
                }
                ElectronicsToolbarAction::PcbSelect => {
                    self.pcb_view.set_select_tool_from_ui();
                }
                ElectronicsToolbarAction::PcbRoute => {
                    self.pcb_view.set_route_tool_from_ui();
                }
                ElectronicsToolbarAction::PcbOutline => {
                    self.pcb_view.set_outline_tool_from_ui();
                }
                ElectronicsToolbarAction::PcbAirwires => {
                    self.pcb_view.toggle_airwires_from_ui();
                }
                ElectronicsToolbarAction::PcbFit => {
                    self.pcb_view.fit_view_from_ui(canvas_width, canvas_height);
                }
                ElectronicsToolbarAction::PcbNewOutline => {
                    self.pcb_view.clear_outline_draft_from_ui();
                    self.pcb_view.set_outline_tool_from_ui();
                }
                ElectronicsToolbarAction::PcbRouteSelected => {
                    changed |= self.pcb_view.route_selected_airwire_from_ui();
                }
                ElectronicsToolbarAction::PcbZoomIn => {
                    self.pcb_view.zoom_in_from_ui();
                }
                ElectronicsToolbarAction::PcbZoomOut => {
                    self.pcb_view.zoom_out_from_ui();
                }
            }
        }
        changed
    }

    fn show_editor(&mut self, ctx: &egui::Context) {
        self.frame_timing.tick();
        let _lang = self.settings.language;
        let mut document_changed_this_frame = false;
        let palette = app_theme::palette_for_visuals(
            ctx.style().visuals.dark_mode,
            self.settings.theme_experimental,
        );
        self.ui_icons.request_icons(EDITOR_UI_ICONS);
        self.ui_icons.process_load_budget(ctx, ui_icon_budget(ctx));

        // --- Global keyboard shortcuts ---
        self.handle_global_shortcuts(ctx);

        // --- Auto-save ---
        self.handle_auto_save(ctx);

        // The window title bar is provided by the native desktop host. Keep
        // branding out of the workbench and expose one compact command row;
        // the contextual strip lives next to Schematic/PCB below.

        let native_menu_installed = self
            .native_application_menu
            .as_ref()
            .is_some_and(NativeWindowApplicationMenuAdapter::is_installed);
        if !native_menu_installed {
            egui::TopBottomPanel::top("app_command_bar")
                .frame(
                    egui::Frame::default()
                        .fill(palette.faint_bg)
                        .stroke(egui::Stroke::new(1.0, palette.separator)),
                )
                .max_height(32.0)
                .show(ctx, |ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(430.0, 30.0),
                        egui::Layout::right_to_left(egui::Align::Center),
                        |menu_ui| self.show_editor_menu_items(menu_ui),
                    );
                });
        }

        let project_status = self.current_project.as_ref().map(|project| {
            let modified = if self.scene_modified { " *" } else { "" };
            (
                format!("{}{}", project.name, modified),
                match project.project_type {
                    ProjectType::Game => t("app.game_project", _lang),
                    ProjectType::Electronics => t("app.electronics_project", _lang),
                },
            )
        });
        let (status_counts, status_color) = match self.viewport_mode {
            ViewportMode::Scene => (
                format!(
                    "{} {}",
                    t("app.entities_count", _lang),
                    self.scene.all_valid_ids().len()
                ),
                [205, 208, 214, 255],
            ),
            ViewportMode::Schematic => {
                let comp_count = self.schematic_view.schematic.components.len();
                let wire_count = self.schematic_view.schematic.wires.len();
                let drc_report = raf_electronics::drc::run_drc(&self.schematic_view.schematic);
                let drc_errors = drc_report.errors.len();
                let drc_label = if drc_errors > 0 {
                    format!("DRC: {} {}", drc_errors, t("app.drc_errors", _lang))
                } else {
                    t("app.drc_ok", _lang).to_string()
                };
                (
                    format!(
                        "{} {} | {} {} | {}",
                        t("app.schematic_components", _lang),
                        comp_count,
                        t("app.schematic_wires", _lang),
                        wire_count,
                        drc_label,
                    ),
                    if drc_errors > 0 {
                        [220, 80, 80, 255]
                    } else {
                        [205, 208, 214, 255]
                    },
                )
            }
            ViewportMode::Pcb => (
                format!(
                    "{} {} | {} {} | {} {}",
                    t("app.pcb_components", _lang),
                    self.pcb_view.layout.components.len(),
                    t("app.pcb_traces", _lang),
                    self.pcb_view.layout.traces.len(),
                    t("app.pcb_airwires", _lang),
                    self.pcb_view.layout.airwires.len()
                ),
                [205, 208, 214, 255],
            ),
        };
        let status_palette = if ctx.style().visuals.dark_mode {
            StudioUiPalette::IndustrialDark
        } else {
            StudioUiPalette::PaperLight
        };
        egui::TopBottomPanel::bottom("status_bar")
            .frame(egui::Frame::none())
            .max_height(24.0)
            .show(ctx, |ui| {
                self.status_surface.show(
                    ui,
                    self.egui_wgpu_render_state.as_ref(),
                    status_palette,
                    project_status.clone(),
                    status_counts.clone(),
                    status_color,
                    self.undo_stack.len(),
                    self.redo_stack.len(),
                    &self.last_action,
                    self.settings.language,
                    self.settings.theme,
                );
            });

        let (show_hierarchy_panel, show_properties_panel, complements_enabled) = self
            .current_project
            .as_ref()
            .map(|project| {
                (
                    project.settings.show_hierarchy_panel,
                    project.settings.show_properties_panel,
                    project.settings.enable_complements,
                )
            })
            .unwrap_or((true, true, true));

        if !complements_enabled && matches!(self.bottom_tab, BottomTab::Complement(_)) {
            self.bottom_tab = BottomTab::ProjectSettings;
        }

        self.editor_shell
            .sync_legacy_visibility(show_hierarchy_panel, show_properties_panel);
        let shell_available = ctx.available_rect();
        let shell_workspace = UiRect::new(
            shell_available.min.x,
            shell_available.min.y,
            shell_available.width(),
            shell_available.height(),
        );
        let shell_palette = if ctx.style().visuals.dark_mode {
            StudioUiPalette::IndustrialDark
        } else {
            StudioUiPalette::PaperLight
        };
        let shell_bottom_height = self.editor_shell.preferred_height(PANEL_BOTTOM, 200.0);

        let mut bottom_panel = egui::TopBottomPanel::bottom("bottom_panel");
        bottom_panel = if let Some(height) = self.bottom_panel_snap_height {
            bottom_panel
                .resizable(false)
                .min_height(height)
                .max_height(height)
        } else {
            bottom_panel
                .resizable(true)
                .min_height(90.0)
                .default_height(shell_bottom_height)
        };

        let bottom_complements: Vec<(String, String)> = if complements_enabled {
            self.complement_registry
                .complements
                .iter()
                .map(|complement| (complement.id().to_string(), complement.name().to_string()))
                .collect()
        } else {
            Vec::new()
        };

        let bottom_response = bottom_panel.show(ctx, |ui| {
            ui.horizontal(|ui| {
                let mut selected_shell_tab = None;
                let active_shell_tab = bottom_tab_to_shell(&self.bottom_tab);
                let render_state = self.egui_wgpu_render_state.as_ref();
                let project_type = self
                    .current_project
                    .as_ref()
                    .map(|project| project.project_type)
                    .unwrap_or(ProjectType::Game);
                let tabs_width = (ui.available_width() - 260.0).max(180.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(tabs_width, 28.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |tabs_ui| {
                        selected_shell_tab = self.bottom_tabs_surface.show(
                            tabs_ui,
                            render_state,
                            shell_palette,
                            _lang,
                            project_type,
                            active_shell_tab,
                        );
                    },
                );

                let chrome_actions = self.bottom_chrome_surface.show(
                    ui,
                    render_state,
                    shell_palette,
                    _lang,
                    self.bottom_panel_snap_height,
                    &bottom_complements,
                    match &self.bottom_tab {
                        BottomTab::Complement(id) => Some(id.as_str()),
                        _ => None,
                    },
                );
                for action in chrome_actions {
                    match action {
                        EditorBottomChromeAction::SetSnap(height) => {
                            self.bottom_panel_snap_height = height;
                        }
                        EditorBottomChromeAction::SelectComplement(id) => {
                            if bottom_complements
                                .iter()
                                .any(|(candidate, _)| candidate == &id)
                            {
                                self.bottom_tab = BottomTab::Complement(id);
                            }
                        }
                    }
                }

                if let Some(tab) = selected_shell_tab {
                    match tab {
                        EditorBottomDockTab::Drc => {
                            self.run_electronics_drc();
                            self.bottom_tab = BottomTab::Drc;
                        }
                        EditorBottomDockTab::Simulation => {
                            self.handle_build();
                            self.bottom_tab = BottomTab::Simulation;
                        }
                        tab => self.bottom_tab = bottom_tab_from_shell(tab),
                    }
                }
            });
            ui.separator();

            match &self.bottom_tab {
                BottomTab::Drc => {
                    if matches!(
                        self.electronics_analysis_surface.show_drc(
                            ui,
                            self.egui_wgpu_render_state.as_ref(),
                            shell_palette,
                            self.electronics_drc_report.as_ref(),
                            self.settings.language,
                        ),
                        Some(ElectronicsAnalysisSurfaceAction::RunDrc)
                    ) {
                        self.run_electronics_drc();
                    }
                }
                BottomTab::Simulation => {
                    if matches!(
                        self.electronics_analysis_surface.show_simulation(
                            ui,
                            self.egui_wgpu_render_state.as_ref(),
                            shell_palette,
                            &self.schematic_view.schematic,
                            self.electronics_simulation_results.as_ref(),
                            self.settings.language,
                        ),
                        Some(ElectronicsAnalysisSurfaceAction::RunSimulation)
                    ) {
                        self.handle_build();
                    }
                }
                BottomTab::Assets => {
                    self.asset_browser.process_scan_budget(96);
                    let actions = self.asset_browser_surface.show(
                        ui,
                        self.egui_wgpu_render_state.as_ref(),
                        shell_palette,
                        &self.asset_browser.entries,
                        &self.asset_browser.search_query,
                        self.asset_browser.selected_filter,
                        self.asset_browser.scan_in_progress,
                        self.asset_browser.status_message(),
                        self.settings.language,
                    );
                    self.apply_asset_browser_actions(actions);

                    let dropped = ui.input(|input| input.raw.dropped_files.clone());
                    if !dropped.is_empty() {
                        self.asset_browser
                            .handle_dropped_files(&dropped, self.settings.language);
                    }

                    if self.asset_browser.ide_dialog_open() {
                        let mut dialog_action = None;
                        egui::Window::new(t("app.ide_dialog_title", self.settings.language))
                            .collapsible(false)
                            .resizable(false)
                            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                            .show(ctx, |dialog_ui| {
                                dialog_ui.set_min_width(440.0);
                                dialog_action = self.asset_browser_surface.show_ide_dialog(
                                    dialog_ui,
                                    self.egui_wgpu_render_state.as_ref(),
                                    shell_palette,
                                    self.asset_browser.ide_dialog_file(),
                                    self.settings.language,
                                );
                            });
                        match dialog_action {
                            Some(AssetDialogAction::OpenYoll) => {
                                self.asset_browser.open_ide_yoll();
                            }
                            Some(AssetDialogAction::OpenVscode) => {
                                self.asset_browser.open_ide_vscode();
                            }
                            Some(AssetDialogAction::Cancel) => {
                                self.asset_browser.close_ide_dialog();
                            }
                            None => {}
                        }
                    }
                }
                BottomTab::Console => {
                    let cmds = self.command_catalog.command_names();
                    let input_enabled = self.settings.command_console_enabled
                        && self
                            .current_project
                            .as_ref()
                            .map(|project| project.settings.enable_console_commands)
                            .unwrap_or(false);
                    let render_state = self.egui_wgpu_render_state.as_ref();
                    let submissions = self.console_surface.show(
                        ui,
                        render_state,
                        shell_palette,
                        &mut self.console,
                        input_enabled,
                        &cmds,
                        self.settings.language,
                    );
                    self.process_console_submissions(submissions);
                }
                BottomTab::ProjectSettings => {
                    let before_global_console_commands = self.settings.command_console_enabled;
                    let language = self.settings.language;
                    let render_state = self.egui_wgpu_render_state.as_ref();
                    let changed = if let Some(project) = self.current_project.as_mut() {
                        self.project_settings_surface.show(
                            ui,
                            render_state,
                            shell_palette,
                            project,
                            &mut self.settings.command_console_enabled,
                            language,
                        )
                    } else {
                        ui.label(
                            egui::RichText::new(t(
                                "app.no_entity_selected",
                                self.settings.language,
                            ))
                            .size(11.0)
                            .color(palette.text_dim),
                        );
                        false
                    };

                    if changed {
                        if let Some(project) = &self.current_project {
                            let _ = project.save();
                        }
                        let msg = t("app.project_settings_saved", self.settings.language);
                        self.last_action = msg.clone();
                        self.console.log(LogLevel::Info, &msg);
                    }
                    if self.settings.command_console_enabled != before_global_console_commands {
                        let _ = self.settings.save(&dirs_config_dir());
                    }
                }
                BottomTab::AiChat => {
                    let project = self.current_project.clone();
                    let catalog = &self.command_catalog;
                    // The agent runtime executes at most one tool per poll. Capture only
                    // that frame so normal chat does not serialize editor documents.
                    let before_agent_tool =
                        if self.ai_chat.runtime.status == AgentStatus::ExecutingTools {
                            self.all_history_snapshots()
                        } else {
                            Vec::new()
                        };
                    let open_settings_requested = {
                        let tool_name_map = self.ai_chat.tool_name_map.clone();
                        let ai_chat = &mut self.ai_chat;
                        let mut executor = crate::agent_executor::AgentToolExecutor {
                            scene: &mut self.scene,
                            hierarchy: &mut self.hierarchy,
                            viewport: &mut self.viewport,
                            schematic_view: &mut self.schematic_view,
                            pcb_view: &mut self.pcb_view,
                            project: project.as_ref(),
                            catalog,
                            console: Some(&mut self.console),
                            image_queue: &mut self.image_generation,
                            sessions: &mut self.sessions,
                            session_events: &mut self.pending_session_events,
                            editor_actions: &mut self.pending_agent_editor_actions,
                            ui_document: &mut self.ui_document,
                            tool_name_map: &tool_name_map,
                        };
                        let readiness = ai_chat.prepare_retained_surface(
                            &mut self.settings,
                            project.as_ref(),
                            catalog,
                            &mut executor,
                        );
                        let actions = self.agent_surface.show(
                            ui,
                            self.egui_wgpu_render_state.as_ref(),
                            shell_palette,
                            ai_chat,
                            &self.settings,
                            project.as_ref(),
                            readiness,
                        );
                        let mut open_settings = false;
                        for action in actions {
                            open_settings |= ai_chat.apply_retained_action(
                                action,
                                &mut self.settings,
                                project.as_ref(),
                                &mut executor,
                            );
                        }
                        open_settings || ai_chat.open_settings_requested
                    };
                    if open_settings_requested {
                        self.ai_chat.open_settings_requested = false;
                        self.open_settings_screen(AppScreen::Editor);
                    }
                    self.process_session_events();
                    if !self.process_agent_editor_actions() {
                        self.record_agent_history_changes(before_agent_tool);
                    }
                    if self.ai_chat.settings_changed {
                        self.ai_chat.settings_changed = false;
                        let _ = self.settings.save(&dirs_config_dir());
                    }
                }
                BottomTab::NodeEditor => {
                    self.node_editor.show(
                        ui,
                        self.egui_wgpu_render_state.as_ref(),
                        shell_palette,
                        self.settings.language,
                    );
                }
                BottomTab::Complement(_) => {
                    ui.label("Complement tab");
                }
            }
        });
        let bottom_rect = bottom_response.response.rect;
        self.track_editor_shell_resize(
            ctx,
            PANEL_BOTTOM,
            UiRect::new(
                bottom_rect.min.x,
                bottom_rect.min.y,
                bottom_rect.width(),
                bottom_rect.height(),
            ),
            shell_workspace,
            ShellResizeEdge::Top,
        );

        // Left panel: Hierarchy.
        if show_hierarchy_panel {
            let is_electronics_project = self
                .current_project
                .as_ref()
                .map(|project| project.project_type == ProjectType::Electronics)
                .unwrap_or(false);
            let hierarchy_default_width = if is_electronics_project {
                self.editor_shell
                    .preferred_width(PANEL_HIERARCHY, 284.0)
                    .clamp(260.0, 360.0)
            } else {
                self.editor_shell
                    .preferred_width(PANEL_HIERARCHY, 224.0)
                    .clamp(196.0, 320.0)
            };
            let hierarchy_min_width = if is_electronics_project { 230.0 } else { 196.0 };
            let hierarchy_max_width = if is_electronics_project { 360.0 } else { 320.0 };
            let hierarchy_response = egui::SidePanel::left("hierarchy_panel")
                .resizable(true)
                .default_width(hierarchy_default_width)
                .min_width(hierarchy_min_width)
                .max_width(hierarchy_max_width)
                .show(ctx, |ui| match self.viewport_mode {
                    ViewportMode::Scene => {
                        if self.runtime.is_some() {
                            ui.label(
                                egui::RichText::new(t(
                                    "app.runtime_scene_locked",
                                    self.settings.language,
                                ))
                                .size(11.0)
                                .color(palette.text_dim),
                            );
                            return;
                        }

                        let prev_hier_vec = self.hierarchy.selected_nodes.clone();
                        let retained_actions = self.game_hierarchy_surface.show(
                            ui,
                            self.egui_wgpu_render_state.as_ref(),
                            shell_palette,
                            &self.scene,
                            &self.hierarchy.selected_nodes,
                            self.settings.language,
                        );
                        self.apply_game_hierarchy_surface_actions(retained_actions);

                        if self.hierarchy.selected_nodes != prev_hier_vec {
                            self.viewport.selected = self.hierarchy.selected_nodes.clone();
                        }

                        if self.viewport.selected != self.hierarchy.selected_nodes {
                            self.hierarchy.selected_nodes = self.viewport.selected.clone();
                            self.hierarchy.selected_node = self.viewport.selected.first().copied();
                        }
                        return;

                        /* Legacy Egui hierarchy action path retained below as a
                         * recovery reference while the RafUI scene tree settles.
                         */
                        /*
                        self.hierarchy.show(
                            ui,
                            &mut self.scene,
                            self.settings.language,
                            &self.ui_icons,
                        );

                        let actions = self.hierarchy.take_actions();
                        if let Some(del_id) = actions.delete {
                            self.push_undo_snapshot();
                            let name = self
                                .scene
                                .get(del_id)
                                .map(|n| n.name.clone())
                                .unwrap_or_default();
                            if self.scene.remove_node(del_id) {
                                self.hierarchy.selected_node = None;
                                self.hierarchy.selected_nodes.clear();
                                self.viewport.selected.clear();
                                let msg = format!(
                                    "{} {}",
                                    t("app.deleted_msg", self.settings.language),
                                    name
                                );
                                self.last_action = msg.clone();
                                self.console.log(LogLevel::Info, &msg);
                            }
                        }
                        if let Some(dup_id) = actions.duplicate {
                            self.push_undo_snapshot();
                            if let Some(new_id) = self.scene.duplicate_node(dup_id) {
                                self.hierarchy.selected_node = Some(new_id);
                                self.hierarchy.selected_nodes = vec![new_id];
                                self.viewport.selected = vec![new_id];
                                let name = self
                                    .scene
                                    .get(new_id)
                                    .map(|n| n.name.clone())
                                    .unwrap_or_default();
                                let msg = format!(
                                    "{} {}",
                                    t("app.duplicated_msg", self.settings.language),
                                    name
                                );
                                self.last_action = msg.clone();
                                self.console.log(LogLevel::Info, &msg);
                            }
                        }
                        if let Some(folder_id) = actions.ungroup {
                            self.push_undo_snapshot();
                            let name = self
                                .scene
                                .get(folder_id)
                                .map(|n| n.name.clone())
                                .unwrap_or_default();
                            if self.scene.ungroup_node(folder_id) {
                                self.hierarchy.selected_node = None;
                                self.hierarchy.selected_nodes.clear();
                                self.viewport.selected.clear();
                                let msg = format!(
                                    "{} {}",
                                    t("app.ungrouped_msg", self.settings.language),
                                    name
                                );
                                self.last_action = msg.clone();
                                self.console.log(LogLevel::Info, &msg);
                            }
                        }
                        if let Some(toggle_id) = actions.toggle_visibility {
                            self.push_undo_snapshot();
                            if let Some(node) = self.scene.get_mut(toggle_id) {
                                node.visible = !node.visible;
                                let msg = format!(
                                    "{} {}",
                                    t("app.visibility_toggled", self.settings.language),
                                    node.name
                                );
                                self.last_action = msg.clone();
                                self.console.log(LogLevel::Info, &msg);
                            }
                        }
                        if let Some(parent) = actions.create_folder_parent {
                            self.push_undo_snapshot();
                            let new_id = if let Some(parent_id) = parent {
                                self.scene.add_child_folder(parent_id, "Folder")
                            } else {
                                self.scene.add_root_folder("Folder")
                            };
                            self.hierarchy.selected_node = Some(new_id);
                            self.hierarchy.selected_nodes = vec![new_id];
                            self.viewport.selected = vec![new_id];
                            let msg = t("app.folder_created", self.settings.language);
                            self.last_action = msg.clone();
                            self.console.log(LogLevel::Info, &msg);
                        }
                        if let Some((dragged, new_parent)) = actions.reparent {
                            self.push_undo_snapshot();
                            let before = actions.reparent_before;
                            if self.scene.reparent_node_before(dragged, new_parent, before) {
                                self.hierarchy.selected_node = Some(dragged);
                                self.hierarchy.selected_nodes = vec![dragged];
                                self.viewport.selected = vec![dragged];
                                let msg = t("app.node_reparented", self.settings.language);
                                self.last_action = msg.clone();
                                self.console.log(LogLevel::Info, &msg);
                            }
                        }
                        if actions.edited {
                            self.mark_scene_modified();
                        }

                        if self.hierarchy.selected_nodes != prev_hier_vec {
                            self.viewport.selected = self.hierarchy.selected_nodes.clone();
                        }

                        if self.viewport.selected != self.hierarchy.selected_nodes {
                            self.hierarchy.selected_nodes = self.viewport.selected.clone();
                            self.hierarchy.selected_node = self.viewport.selected.first().copied();
                        }
                        */
                    }
                    ViewportMode::Schematic => {
                        let actions = self.electronics_navigator_surface.show_schematic(
                            ui,
                            self.egui_wgpu_render_state.as_ref(),
                            shell_palette,
                            &self.schematic_view,
                            self.settings.language,
                        );
                        self.apply_electronics_navigator_actions(actions);
                    }
                    ViewportMode::Pcb => {
                        let actions = self.electronics_navigator_surface.show_pcb(
                            ui,
                            self.egui_wgpu_render_state.as_ref(),
                            shell_palette,
                            &self.pcb_view,
                            self.settings.language,
                        );
                        self.apply_electronics_navigator_actions(actions);
                    }
                });
            let hierarchy_rect = hierarchy_response.response.rect;
            self.track_editor_shell_resize(
                ctx,
                PANEL_HIERARCHY,
                UiRect::new(
                    hierarchy_rect.min.x,
                    hierarchy_rect.min.y,
                    hierarchy_rect.width(),
                    hierarchy_rect.height(),
                ),
                shell_workspace,
                ShellResizeEdge::Right,
            );
        }

        // Right panel: Properties.
        if show_properties_panel {
            let properties_response = egui::SidePanel::right("properties_panel")
                .resizable(true)
                .frame(
                    egui::Frame::none()
                        .stroke(egui::Stroke::new(1.0, palette.border))
                        .inner_margin(egui::Margin::same(2.0)),
                )
                .default_width(
                    self.editor_shell
                        .preferred_width(PANEL_PROPERTIES, 320.0)
                        .clamp(260.0, 380.0),
                )
                .min_width(260.0)
                .max_width(380.0)
                .show(ctx, |ui| {
                    let mut selected_shell_tab = None;
                    let active_shell_tab = inspector_tab_to_shell(self.inspector_tab);
                    let render_state = self.egui_wgpu_render_state.as_ref();
                    let tabs_width = ui.available_width();
                    ui.allocate_ui_with_layout(
                        egui::vec2(tabs_width, 28.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |tabs_ui| {
                            selected_shell_tab = self.inspector_tabs_surface.show(
                                tabs_ui,
                                render_state,
                                shell_palette,
                                self.settings.language,
                                active_shell_tab,
                            );
                        },
                    );
                    if let Some(tab) = selected_shell_tab {
                        self.inspector_tab = inspector_tab_from_shell(tab);
                    }
                    ui.separator();

                    if self.inspector_tab == InspectorTab::Sessions {
                        let actions = self.sessions_surface.show(
                            ui,
                            self.egui_wgpu_render_state.as_ref(),
                            shell_palette,
                            &self.sessions,
                            self.settings.language,
                        );
                        self.apply_sessions_surface_actions(actions);
                        return;
                    }

                    match self.viewport_mode {
                        ViewportMode::Scene => {
                            if self.runtime.is_some() {
                                ui.label(
                                    egui::RichText::new(t(
                                        "app.runtime_scene_locked",
                                        self.settings.language,
                                    ))
                                    .size(11.0)
                                    .color(palette.text_dim),
                                );
                                return;
                            }

                            let before_snapshot = self.current_history_snapshot();
                            let retained_actions = self.game_properties_surface.show(
                                ui,
                                self.egui_wgpu_render_state.as_ref(),
                                shell_palette,
                                &self.scene,
                                self.hierarchy.selected_node,
                                self.settings.language,
                            );
                            self.apply_game_properties_surface_actions(retained_actions);
                            if self.current_history_snapshot() != before_snapshot {
                                document_changed_this_frame |=
                                    self.record_document_change(before_snapshot);
                            }
                        }
                        ViewportMode::Schematic => {
                            let before_snapshot = self.current_history_snapshot();
                            let actions = self.electronics_inspector_surface.show_schematic(
                                ui,
                                self.egui_wgpu_render_state.as_ref(),
                                shell_palette,
                                &self.schematic_view,
                                self.settings.language,
                            );
                            if self.apply_schematic_inspector_actions(actions) {
                                document_changed_this_frame |=
                                    self.record_document_change(before_snapshot);
                            }
                        }
                        ViewportMode::Pcb => {
                            let before_snapshot = self.current_history_snapshot();
                            let actions = self.electronics_inspector_surface.show_pcb(
                                ui,
                                self.egui_wgpu_render_state.as_ref(),
                                shell_palette,
                                &self.pcb_view,
                                self.settings.language,
                            );
                            if self.apply_pcb_inspector_actions(actions) {
                                document_changed_this_frame |=
                                    self.record_document_change(before_snapshot);
                            }
                        }
                    }
                });
            let properties_rect = properties_response.response.rect;
            self.track_editor_shell_resize(
                ctx,
                PANEL_PROPERTIES,
                UiRect::new(
                    properties_rect.min.x,
                    properties_rect.min.y,
                    properties_rect.width(),
                    properties_rect.height(),
                ),
                shell_workspace,
                ShellResizeEdge::Left,
            );
        }

        // Central panel: Viewport or Schematic.
        egui::CentralPanel::default().show(ctx, |ui| {
            if self
                .current_project
                .as_ref()
                .map(|project| project.project_type == ProjectType::Electronics)
                .unwrap_or(false)
            {
                let mut selected_surface = None;
                let active_surface = viewport_mode_to_shell(self.viewport_mode);
                let row_width = ui.available_width();
                let tabs_width = 236.0;
                ui.allocate_ui_with_layout(
                    egui::vec2(row_width, 42.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |row_ui| {
                        row_ui.spacing_mut().item_spacing.x = 0.0;
                        row_ui.allocate_ui_with_layout(
                            egui::vec2(tabs_width, 42.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |tabs_ui| {
                                selected_surface = self.context_tabs_surface.show(
                                    tabs_ui,
                                    self.egui_wgpu_render_state.as_ref(),
                                    shell_palette,
                                    self.settings.language,
                                    ProjectType::Electronics,
                                    active_surface,
                                );
                            },
                        );

                        row_ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |actions_ui| {
                                self.show_editor_context_actions(actions_ui, &palette, _lang);
                            },
                        );
                    },
                );
                match selected_surface {
                    Some(EditorCenterSurface::Schematic) => {
                        // Cross-probe: if coming from PCB, try to select the
                        // same component in schematic by designator.
                        if let Some(designator) = self.cross_probe_designator.take() {
                            self.schematic_view.select_by_designator(&designator);
                        }
                        self.viewport_mode = ViewportMode::Schematic;
                    }
                    Some(EditorCenterSurface::Pcb) => {
                        self.sync_pcb_from_schematic();
                        // Cross-probe: if coming from schematic, try to select
                        // the same component in PCB by designator.
                        if let Some(designator) = self.cross_probe_designator.take() {
                            self.pcb_view.select_by_designator(&designator);
                        }
                        self.viewport_mode = ViewportMode::Pcb;
                    }
                    Some(EditorCenterSurface::Scene) | None => {}
                }
                ui.add_space(8.0);
            }

            match self.viewport_mode {
                ViewportMode::Scene => {
                    let runtime_snapshot =
                        self.prepare_graphics_surface(GraphicsSurfaceKind::SceneViewport);
                    self.viewport.set_render_runtime(runtime_snapshot);
                    self.viewport.frame_time_hint = self.frame_timing.frame_time_s();
                    self.viewport.render_cfg = self
                        .current_project
                        .as_ref()
                        .map(|project| self.project_render_config(project))
                        .unwrap_or_else(RenderConfig::potato);
                    self.viewport.world_stream_config = self
                        .current_project
                        .as_ref()
                        .map(|project| WorldStreamConfig::from_project_settings(&project.settings))
                        .unwrap_or_default();
                    self.viewport.grid_visible = self.settings.grid_visible;
                    self.viewport.grid_spacing = self.settings.grid_size.max(0.1);
                    self.viewport.grid_load_distance = self.settings.grid_load_distance.max(0.0);
                    self.viewport.fps_limit = self.settings.fps_limit;
                    self.viewport.invert_mouse_x = self.settings.invert_mouse_x;
                    self.viewport.invert_mouse_y = self.settings.invert_mouse_y;
                    self.viewport.move_sensitivity = self.settings.move_gizmo_sensitivity.max(0.1);
                    self.viewport.rotate_sensitivity =
                        self.settings.rotate_gizmo_sensitivity.max(0.1);
                    self.viewport.scale_sensitivity =
                        self.settings.scale_gizmo_sensitivity.max(0.1);
                    self.viewport.uniform_scale_by_default = self.settings.uniform_scale_by_default;
                    self.viewport.invert_ws = self.settings.invert_ws;
                    if !self.settings.focus_lock_enabled {
                        self.viewport.focus_locked = false;
                    }
                    self.viewport.focus_lock_enabled = self.settings.focus_lock_enabled;
                    self.viewport.gizmo_growth_scale = self.settings.gizmo_growth_scale;
                    self.viewport.wasd_speed = self.settings.wasd_speed;
                    self.viewport.solid_show_surface_edges = self.settings.solid_show_surface_edges;
                    self.viewport.solid_xray_mode = self.settings.solid_xray_mode;
                    self.viewport.solid_face_tonality = self.settings.solid_face_tonality;
                    self.viewport.render_style = match self.settings.viewport_render_mode {
                        raf_core::config::ViewportRenderMode::Solid => {
                            crate::panels::viewport::RenderStyle::Solid
                        }
                        raf_core::config::ViewportRenderMode::Wireframe => {
                            crate::panels::viewport::RenderStyle::Wireframe
                        }
                        raf_core::config::ViewportRenderMode::Preview => {
                            crate::panels::viewport::RenderStyle::Preview
                        }
                    };
                    self.viewport.show_labels = self.settings.show_viewport_labels;
                    let mut viewport_toolbar_actions = Vec::new();
                    let mut viewport_overlay_actions = Vec::new();
                    let toolbar_width = ui.available_width();
                    ui.allocate_ui_with_layout(
                        egui::vec2(toolbar_width, 38.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |toolbar_ui| {
                            let status_width = 220.0_f32.min((toolbar_width - 260.0).max(0.0));
                            let viewport_width = (toolbar_width - status_width).max(260.0);
                            toolbar_ui.allocate_ui_with_layout(
                                egui::vec2(viewport_width, 38.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |viewport_toolbar_ui| {
                                    viewport_toolbar_actions = self.game_viewport_surface.show(
                                        viewport_toolbar_ui,
                                        self.egui_wgpu_render_state.as_ref(),
                                        shell_palette,
                                        &self.viewport,
                                        self.settings.language,
                                    );
                                },
                            );
                            if status_width > 0.0 {
                                toolbar_ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |status_ui| {
                                        self.context_actions_surface.show_game_status(
                                            status_ui,
                                            self.egui_wgpu_render_state.as_ref(),
                                            shell_palette,
                                            self.settings.language,
                                            self.active_session_name().to_string(),
                                            self.settings.show_fps_counter,
                                            self.frame_timing.fps(),
                                        );
                                    },
                                );
                            }
                        },
                    );
                    self.apply_game_viewport_surface_actions(viewport_toolbar_actions);
                    ui.add_space(3.0);
                    let before_snapshot = self.current_history_snapshot();
                    let viewport_frame = egui::Frame::none()
                        .outer_margin(egui::Margin::same(2.0))
                        .inner_margin(egui::Margin::same(1.0))
                        .stroke(egui::Stroke::new(1.0, palette.border));
                    let viewport_changed = viewport_frame
                        .show(ui, |viewport_ui| {
                            let viewport_rect = viewport_ui.available_rect_before_wrap();
                            let top_overlay_rect = egui::Rect::from_min_size(
                                viewport_rect.min + egui::vec2(10.0, 10.0),
                                egui::vec2(156.0, 36.0),
                            );
                            let bottom_overlay_rect = egui::Rect::from_min_size(
                                egui::pos2(
                                    (viewport_rect.right() - 166.0)
                                        .max(viewport_rect.left() + 10.0),
                                    (viewport_rect.bottom() - 42.0).max(viewport_rect.top() + 10.0),
                                ),
                                egui::vec2(156.0, 36.0),
                            );
                            self.viewport.set_retained_overlay_rects([
                                Some(top_overlay_rect),
                                Some(bottom_overlay_rect),
                            ]);

                            let changed = self.viewport.show_with_retained_toolbar(
                                ctx,
                                viewport_ui,
                                self.egui_wgpu_render_state.as_ref(),
                                &mut self.graphics_runtime,
                                &mut self.scene,
                                self.settings.theme != Theme::Light,
                                self.settings.language,
                                &self.ui_icons,
                            );

                            viewport_ui.allocate_new_ui(
                                egui::UiBuilder::new().max_rect(top_overlay_rect),
                                |overlay_ui| {
                                    viewport_overlay_actions =
                                        self.game_viewport_surface.show_top_overlay(
                                            overlay_ui,
                                            self.egui_wgpu_render_state.as_ref(),
                                            shell_palette,
                                            &self.viewport,
                                            self.settings.language,
                                        );
                                },
                            );
                            viewport_ui.allocate_new_ui(
                                egui::UiBuilder::new().max_rect(bottom_overlay_rect),
                                |overlay_ui| {
                                    viewport_overlay_actions.extend(
                                        self.game_viewport_surface.show_bottom_overlay(
                                            overlay_ui,
                                            self.egui_wgpu_render_state.as_ref(),
                                            shell_palette,
                                            &self.viewport,
                                            self.settings.language,
                                        ),
                                    );
                                },
                            );
                            changed
                        })
                        .inner;
                    self.apply_game_viewport_surface_actions(viewport_overlay_actions);
                    if viewport_changed {
                        document_changed_this_frame |= self.record_document_change(before_snapshot);
                    }
                }
                ViewportMode::Schematic => {
                    let runtime_snapshot =
                        self.prepare_graphics_surface(GraphicsSurfaceKind::SchematicCanvas);
                    self.schematic_view.lang = self.settings.language;
                    self.schematic_view.set_render_runtime(runtime_snapshot);
                    let toolbar_width = ui.available_width();
                    let mut toolbar_actions = Vec::new();
                    ui.allocate_ui_with_layout(
                        egui::vec2(toolbar_width, 38.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |toolbar_ui| {
                            toolbar_actions = self.electronics_toolbar_surface.show_schematic(
                                toolbar_ui,
                                self.egui_wgpu_render_state.as_ref(),
                                shell_palette,
                                &self.schematic_view,
                                self.settings.language,
                            );
                        },
                    );
                    let before_snapshot = self.current_history_snapshot();
                    let toolbar_changed = self.apply_electronics_toolbar_actions(
                        toolbar_actions,
                        toolbar_width,
                        ui.available_height(),
                    );
                    if toolbar_changed {
                        document_changed_this_frame |= self.record_document_change(before_snapshot);
                    }
                    ui.add_space(3.0);
                    let before_snapshot = self.current_history_snapshot();
                    if self.schematic_view.show_canvas_only_without_toolbar(
                        ui,
                        self.egui_wgpu_render_state.as_ref(),
                        &mut self.graphics_runtime,
                    ) {
                        document_changed_this_frame |= self.record_document_change(before_snapshot);
                    }
                    // Cross-probe: capture selected component designator.
                    if let Some(designator) = self.schematic_view.selected_designator() {
                        self.cross_probe_designator = Some(designator);
                    }
                }
                ViewportMode::Pcb => {
                    let runtime_snapshot =
                        self.prepare_graphics_surface(GraphicsSurfaceKind::PcbCanvas);
                    self.pcb_view.lang = self.settings.language;
                    self.pcb_view.set_render_runtime(runtime_snapshot);
                    let toolbar_width = ui.available_width();
                    let mut toolbar_actions = Vec::new();
                    ui.allocate_ui_with_layout(
                        egui::vec2(toolbar_width, 38.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |toolbar_ui| {
                            toolbar_actions = self.electronics_toolbar_surface.show_pcb(
                                toolbar_ui,
                                self.egui_wgpu_render_state.as_ref(),
                                shell_palette,
                                &self.pcb_view,
                                self.settings.language,
                            );
                        },
                    );
                    let before_snapshot = self.current_history_snapshot();
                    let toolbar_changed = self.apply_electronics_toolbar_actions(
                        toolbar_actions,
                        toolbar_width,
                        ui.available_height(),
                    );
                    if toolbar_changed {
                        document_changed_this_frame |= self.record_document_change(before_snapshot);
                    }
                    ui.add_space(3.0);
                    let before_snapshot = self.current_history_snapshot();
                    if self.pcb_view.show_canvas_only_without_toolbar(
                        ui,
                        self.egui_wgpu_render_state.as_ref(),
                        &mut self.graphics_runtime,
                    ) {
                        document_changed_this_frame |= self.record_document_change(before_snapshot);
                    }
                    // Cross-probe: capture selected component designator.
                    if let Some(designator) = self.pcb_view.selected_designator() {
                        self.cross_probe_designator = Some(designator);
                    }
                }
            }
        });

        if document_changed_this_frame
            && self
                .current_project
                .as_ref()
                .map(|project| project.project_type == ProjectType::Electronics)
                .unwrap_or(false)
        {
            self.electronics_drc_report = None;
            self.electronics_simulation_results = None;
        }

        self.persist_editor_shell_if_idle(ctx);

        if !document_changed_this_frame {
            self.finalize_pending_history_snapshot();
        }

        if self.viewport_mode == ViewportMode::Scene
            && self.viewport.selected != self.hierarchy.selected_nodes
        {
            self.hierarchy.selected_nodes = self.viewport.selected.clone();
            self.hierarchy.selected_node = self.viewport.selected.first().copied();
        }
    }

    // -----------------------------------------------------------------------
    // Settings Screen
    // -----------------------------------------------------------------------

    fn open_settings_screen(&mut self, previous_screen: AppScreen) {
        self.previous_screen = Some(previous_screen);
        self.settings_draft = Some(self.settings.clone());
        self.screen = AppScreen::Settings;
    }

    fn show_settings_screen(&mut self, ctx: &egui::Context) {
        if self.settings_draft.is_none() {
            self.settings_draft = Some(self.settings.clone());
        }

        let draft_changed_before_ui = self.settings_draft.as_ref() != Some(&self.settings);
        let mut close = false;
        let mut save = false;
        let mut cancel_clicked = false;

        let keyboard_save =
            ctx.input(|i| (i.modifiers.ctrl || i.modifiers.mac_cmd) && i.key_pressed(egui::Key::S));
        let keyboard_close = ctx.input(|i| i.key_pressed(egui::Key::Escape));

        if keyboard_save {
            save = true;
            close = true;
        } else if keyboard_close && !self.show_settings_close_dialog {
            // Esc: only prompt if there are unsaved changes; otherwise close clean.
            if draft_changed_before_ui {
                self.show_settings_close_dialog = true;
            } else {
                close = true;
            }
        }

        let render_state = self.egui_wgpu_render_state.as_ref();
        let palette = if ctx.style().visuals.dark_mode {
            StudioUiPalette::IndustrialDark
        } else {
            StudioUiPalette::PaperLight
        };
        let mut surface_intents = Vec::new();
        egui::CentralPanel::default().show(ctx, |ui| {
            let draft = self
                .settings_draft
                .as_mut()
                .expect("settings draft initialized");
            surface_intents = self.settings_surface.show(ui, render_state, palette, draft);
        });

        for intent in surface_intents {
            match intent {
                SettingsSurfaceIntent::Save => {
                    save = true;
                    close = true;
                }
                SettingsSurfaceIntent::Cancel => cancel_clicked = true,
            }
        }

        let draft_changed = self.settings_draft.as_ref() != Some(&self.settings);
        if cancel_clicked {
            if draft_changed {
                self.show_settings_close_dialog = true;
            } else {
                close = true;
            }
        }

        // Settings close confirmation dialog.
        if self.show_settings_close_dialog {
            let lang = self.settings.language;
            let palette = if ctx.style().visuals.dark_mode {
                StudioUiPalette::IndustrialDark
            } else {
                StudioUiPalette::PaperLight
            };
            let mut dialog_action = None;
            egui::Window::new(t("app.unsaved_changes_title", lang))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.set_min_width(430.0);
                    dialog_action = self.common_dialog_surface.show_unsaved(
                        ui,
                        self.egui_wgpu_render_state.as_ref(),
                        palette,
                        "app.unsaved_changes_title",
                        "app.unsaved_changes_message",
                        lang,
                    );
                });
            match dialog_action {
                Some(CommonDialogAction::Cancel) => self.show_settings_close_dialog = false,
                Some(CommonDialogAction::Discard) => {
                    self.show_settings_close_dialog = false;
                    close = true;
                }
                Some(CommonDialogAction::Save) => {
                    self.show_settings_close_dialog = false;
                    save = true;
                    close = true;
                }
                None => {}
            }
            if self.show_settings_close_dialog {
                return;
            }
        }

        if close {
            let next_screen = self.previous_screen.take().unwrap_or(AppScreen::ProjectHub);
            if save {
                if let Some(draft) = self.settings_draft.take() {
                    self.settings = draft;
                    let config_dir = dirs_config_dir();
                    let _ = self.settings.save(&config_dir);
                    self.console.log(LogLevel::Info, "Settings saved.");
                }
            } else {
                self.settings_draft = None;
            }
            self.screen = next_screen;
        }
    }

    fn show_rafui_studio_screen(&mut self, ctx: &egui::Context) {
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.screen = AppScreen::Editor;
            return;
        }
        let palette = if ctx.style().visuals.dark_mode {
            StudioUiPalette::IndustrialDark
        } else {
            StudioUiPalette::PaperLight
        };
        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                let close = self.raf_ui_studio_surface.show(
                    ui,
                    self.egui_wgpu_render_state.as_ref(),
                    palette,
                    &self.ui_document,
                    self.settings.language,
                    ctx.pixels_per_point().clamp(1.0, 4.0),
                );
                if close {
                    self.screen = AppScreen::Editor;
                }
            });
    }

    fn handle_window_close_request(&mut self, ctx: &egui::Context) {
        let close_requested = ctx.input(|input| input.viewport().close_requested());
        if !close_requested || self.allow_app_close {
            return;
        }

        if self.current_project.is_some() && self.scene_modified {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.pending_exit_action.is_none() {
                self.pending_exit_action = Some(PendingExitAction::QuitApp);
            }
        }
    }

    fn show_unsaved_changes_dialog(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending_exit_action else {
            return;
        };

        let mut perform_action = None;
        let lang = self.settings.language;
        let palette = if ctx.style().visuals.dark_mode {
            StudioUiPalette::IndustrialDark
        } else {
            StudioUiPalette::PaperLight
        };
        let mut dialog_action = None;

        egui::Window::new(t("app.unsaved_changes_title", lang))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_min_width(430.0);
                dialog_action = self.common_dialog_surface.show_unsaved(
                    ui,
                    self.egui_wgpu_render_state.as_ref(),
                    palette,
                    "app.unsaved_changes_title",
                    "app.unsaved_changes_message",
                    lang,
                );
            });

        match dialog_action {
            Some(CommonDialogAction::Cancel) => {
                self.pending_exit_action = None;
                self.allow_app_close = false;
            }
            Some(CommonDialogAction::Discard) => {
                self.pending_exit_action = None;
                perform_action = Some(action);
            }
            Some(CommonDialogAction::Save) => {
                if self.do_save() {
                    self.pending_exit_action = None;
                    perform_action = Some(action);
                }
            }
            None => {}
        }

        if let Some(action) = perform_action {
            self.perform_exit_action(action, ctx);
        }
    }

    fn request_exit_action(&mut self, action: PendingExitAction, ctx: Option<&egui::Context>) {
        if self.scene_modified {
            self.pending_exit_action = Some(action);
            return;
        }

        if let Some(ctx) = ctx {
            self.perform_exit_action(action, ctx);
        } else {
            self.perform_exit_action_without_context(action);
        }
    }

    fn perform_exit_action(&mut self, action: PendingExitAction, ctx: &egui::Context) {
        match action {
            PendingExitAction::ToHub => self.unload_current_project_to_hub(),
            PendingExitAction::QuitApp => {
                self.allow_app_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn perform_exit_action_without_context(&mut self, action: PendingExitAction) {
        if action == PendingExitAction::ToHub {
            self.unload_current_project_to_hub();
        }
    }

    fn unload_current_project_to_hub(&mut self) {
        self.runtime = None;
        self.current_project = None;
        self.scene = SceneGraph::new();
        self.viewport.selected.clear();
        self.hierarchy.selected_node = None;
        self.hierarchy.selected_nodes.clear();
        self.schematic_view.clear_selection();
        self.pcb_view.clear_selection();
        self.asset_browser.project_assets_path = None;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.pending_history_snapshot = None;
        self.pending_agent_editor_actions.clear();
        self.pending_session_events.clear();
        self.scene_modified = false;
        self.auto_save_elapsed = 0.0;
        self.auto_save_last_tick = None;
        self.pending_exit_action = None;
        self.allow_app_close = false;
        self.editor_shell = EditorShellLayout::default();
        self.editor_shell_dirty = false;
        self.editor_shell_resize_panel = None;
        self.screen = AppScreen::ProjectHub;
    }

    fn persist_editor_shell_if_idle(&mut self, ctx: &egui::Context) {
        if !self.editor_shell_dirty || ctx.input(|input| input.pointer.primary_down()) {
            return;
        }
        let Some(project_path) = self
            .current_project
            .as_ref()
            .map(|project| project.path.clone())
        else {
            self.editor_shell_dirty = false;
            return;
        };

        self.editor_shell_dirty = false;
        if let Err(error) = self.editor_shell.save(&project_path) {
            self.console.log(LogLevel::Error, &error);
        }
    }

    fn track_editor_shell_resize(
        &mut self,
        ctx: &egui::Context,
        panel: &'static str,
        rect: UiRect,
        workspace: UiRect,
        edge: ShellResizeEdge,
    ) {
        let (pointer, pressed, down, released) = ctx.input(|input| {
            (
                input.pointer.interact_pos(),
                input.pointer.button_pressed(egui::PointerButton::Primary),
                input.pointer.primary_down(),
                input.pointer.button_released(egui::PointerButton::Primary),
            )
        });
        let near_resize_edge = pointer
            .map(|pointer| match edge {
                ShellResizeEdge::Left => (pointer.x - rect.x).abs() <= 8.0,
                ShellResizeEdge::Right => (pointer.x - rect.right()).abs() <= 8.0,
                ShellResizeEdge::Top => (pointer.y - rect.y).abs() <= 8.0,
            })
            .unwrap_or(false);

        if pressed && near_resize_edge {
            self.editor_shell_resize_panel = Some(panel);
        }
        if self.editor_shell_resize_panel != Some(panel) {
            return;
        }

        if down || released {
            self.editor_shell_dirty |= self.editor_shell.observe_host_rect(panel, rect, workspace);
        }
        if released {
            self.editor_shell_resize_panel = None;
        }
    }

    // -----------------------------------------------------------------------
    // v0.3.0: Undo/Redo (scene snapshot based, max 50)
    // -----------------------------------------------------------------------

    /// Push current document state to undo stack.
    fn push_undo_snapshot(&mut self) {
        self.finalize_pending_history_snapshot();
        if let Some(snapshot) = self.current_history_snapshot() {
            self.push_history_snapshot(snapshot);
        }
    }

    fn current_history_snapshot(&self) -> Option<EditorHistorySnapshot> {
        match self.viewport_mode {
            ViewportMode::Scene => ron::ser::to_string(&self.scene)
                .ok()
                .map(EditorHistorySnapshot::Scene),
            ViewportMode::Schematic => ron::ser::to_string(&self.schematic_view.schematic)
                .ok()
                .map(EditorHistorySnapshot::Schematic),
            ViewportMode::Pcb => ron::ser::to_string(&self.pcb_view.layout)
                .ok()
                .map(EditorHistorySnapshot::Pcb),
        }
    }

    fn all_history_snapshots(&self) -> Vec<EditorHistorySnapshot> {
        let mut snapshots = Vec::with_capacity(4);
        if let Ok(scene) = ron::ser::to_string(&self.scene) {
            snapshots.push(EditorHistorySnapshot::Scene(scene));
        }
        if let Ok(schematic) = ron::ser::to_string(&self.schematic_view.schematic) {
            snapshots.push(EditorHistorySnapshot::Schematic(schematic));
        }
        if let Ok(pcb) = ron::ser::to_string(&self.pcb_view.layout) {
            snapshots.push(EditorHistorySnapshot::Pcb(pcb));
        }
        if let Ok(document) = ron::ser::to_string(&self.ui_document) {
            snapshots.push(EditorHistorySnapshot::UiDocument(document));
        }
        snapshots
    }

    fn process_agent_editor_actions(&mut self) -> bool {
        let actions = std::mem::take(&mut self.pending_agent_editor_actions);
        if actions.is_empty() {
            return false;
        }

        for action in actions {
            match action {
                AgentEditorAction::Undo => self.do_undo(),
                AgentEditorAction::Redo => self.do_redo(),
            }
        }
        true
    }

    fn record_agent_history_changes(&mut self, before: Vec<EditorHistorySnapshot>) {
        for snapshot in before {
            let changed = self
                .current_history_snapshot_like(&snapshot)
                .map(|current| current != snapshot)
                .unwrap_or(true);
            if !changed {
                continue;
            }

            self.finalize_pending_history_snapshot();
            self.push_history_snapshot(snapshot);
        }
    }

    fn current_history_snapshot_like(
        &self,
        snapshot: &EditorHistorySnapshot,
    ) -> Option<EditorHistorySnapshot> {
        match snapshot {
            EditorHistorySnapshot::Scene(_) => ron::ser::to_string(&self.scene)
                .ok()
                .map(EditorHistorySnapshot::Scene),
            EditorHistorySnapshot::Schematic(_) => {
                ron::ser::to_string(&self.schematic_view.schematic)
                    .ok()
                    .map(EditorHistorySnapshot::Schematic)
            }
            EditorHistorySnapshot::Pcb(_) => ron::ser::to_string(&self.pcb_view.layout)
                .ok()
                .map(EditorHistorySnapshot::Pcb),
            EditorHistorySnapshot::UiDocument(_) => ron::ser::to_string(&self.ui_document)
                .ok()
                .map(EditorHistorySnapshot::UiDocument),
        }
    }

    fn push_history_snapshot(&mut self, snapshot: EditorHistorySnapshot) {
        self.undo_stack.push(snapshot);
        if self.undo_stack.len() > 50 {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
        self.mark_scene_modified();
    }

    fn record_document_change(&mut self, before_snapshot: Option<EditorHistorySnapshot>) -> bool {
        let Some(snapshot) = before_snapshot else {
            self.mark_scene_modified();
            return true;
        };

        let current_snapshot = self.current_history_snapshot_like(&snapshot);
        let document_changed = current_snapshot
            .as_ref()
            .map(|current| current != &snapshot)
            .unwrap_or(true);
        let scene_drag_pending =
            matches!(snapshot, EditorHistorySnapshot::Scene(_)) && self.viewport.is_drag_ongoing();

        if !document_changed && !scene_drag_pending {
            return false;
        }

        if self.pending_history_snapshot.is_none() {
            self.pending_history_snapshot = Some(snapshot);
            self.redo_stack.clear();
        }

        if document_changed {
            self.mark_scene_modified();
        }

        true
    }

    fn finalize_pending_history_snapshot(&mut self) {
        if let Some(snapshot) = self.pending_history_snapshot.take() {
            let changed = self
                .current_history_snapshot_like(&snapshot)
                .map(|current| current != snapshot)
                .unwrap_or(true);
            if changed {
                self.push_history_snapshot(snapshot);
            }
        }
    }

    fn apply_history_snapshot(&mut self, snapshot: EditorHistorySnapshot) -> bool {
        match snapshot {
            EditorHistorySnapshot::Scene(data) => {
                if let Ok(restored) = ron::from_str::<SceneGraph>(&data) {
                    self.scene = restored;
                    self.hierarchy.selected_node = None;
                    self.hierarchy.selected_nodes.clear();
                    self.viewport.selected.clear();
                    return true;
                }
            }
            EditorHistorySnapshot::Schematic(data) => {
                if let Ok(restored) = ron::from_str::<raf_electronics::schematic::Schematic>(&data)
                {
                    self.schematic_view.schematic = restored;
                    self.schematic_view.clear_selection();
                    self.hierarchy.selected_node = None;
                    self.hierarchy.selected_nodes.clear();
                    self.viewport.selected.clear();
                    return true;
                }
            }
            EditorHistorySnapshot::Pcb(data) => {
                if let Ok(restored) = ron::from_str::<raf_electronics::PcbLayout>(&data) {
                    self.pcb_view.layout = restored;
                    self.pcb_view.clear_selection();
                    self.hierarchy.selected_node = None;
                    self.hierarchy.selected_nodes.clear();
                    self.viewport.selected.clear();
                    return true;
                }
            }
            EditorHistorySnapshot::UiDocument(data) => {
                if let Ok(restored) = ron::from_str::<UiDocument>(&data) {
                    self.ui_document = restored;
                    return true;
                }
            }
        }

        false
    }

    fn do_undo(&mut self) {
        self.finalize_pending_history_snapshot();
        if let Some(snapshot) = self.undo_stack.pop() {
            if let Some(current) = self.current_history_snapshot_like(&snapshot) {
                self.redo_stack.push(current);
            }
            if self.apply_history_snapshot(snapshot) {
                let msg = t("app.undo", self.settings.language);
                self.last_action = msg.to_string();
                self.console.log(LogLevel::Info, &msg);
                self.mark_scene_modified();
            }
        }
    }

    fn do_redo(&mut self) {
        self.finalize_pending_history_snapshot();
        if let Some(snapshot) = self.redo_stack.pop() {
            if let Some(current) = self.current_history_snapshot_like(&snapshot) {
                self.undo_stack.push(current);
            }
            if self.apply_history_snapshot(snapshot) {
                let msg = t("app.redo", self.settings.language);
                self.last_action = msg.to_string();
                self.console.log(LogLevel::Info, &msg);
                self.mark_scene_modified();
            }
        }
    }

    // -----------------------------------------------------------------------
    // v0.3.0: Scene actions
    // -----------------------------------------------------------------------

    fn do_delete(&mut self) {
        if self.runtime.is_some() && self.viewport_mode == ViewportMode::Scene {
            let msg = t("app.runtime_scene_locked", self.settings.language);
            self.last_action = msg.clone();
            self.console.log(LogLevel::Info, &msg);
            return;
        }

        let _lang = self.settings.language;
        if self.viewport_mode == ViewportMode::Schematic {
            let undo_len = self.undo_stack.len();
            self.push_undo_snapshot();
            if self.schematic_view.delete_selection() {
                self.mark_scene_modified();
                let msg = t("app.delete_del", _lang);
                self.last_action = msg.to_string();
                self.console.log(LogLevel::Info, &msg);
            } else if self.undo_stack.len() > undo_len {
                self.undo_stack.pop();
            }
            return;
        }
        if self.viewport_mode == ViewportMode::Pcb {
            let undo_len = self.undo_stack.len();
            self.push_undo_snapshot();
            if self.pcb_view.delete_selection() {
                self.mark_scene_modified();
                let msg = t("app.delete_del", _lang);
                self.last_action = msg.to_string();
                self.console.log(LogLevel::Info, &msg);
            } else if self.undo_stack.len() > undo_len {
                self.undo_stack.pop();
            }
            return;
        }
        if let Some(id) = self.hierarchy.selected_node {
            self.push_undo_snapshot();
            let name = self
                .scene
                .get(id)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            if self.scene.remove_node(id) {
                self.hierarchy.selected_node = None;
                self.hierarchy.selected_nodes.clear();
                self.viewport.selected.clear();
                let msg = format!("{} {}", t("app.deleted_msg", _lang), name);
                self.last_action = msg.clone();
                self.console.log(LogLevel::Info, &msg);
            }
        }
    }

    fn do_duplicate(&mut self) {
        if self.runtime.is_some() && self.viewport_mode == ViewportMode::Scene {
            let msg = t("app.runtime_scene_locked", self.settings.language);
            self.last_action = msg.clone();
            self.console.log(LogLevel::Info, &msg);
            return;
        }

        let _lang = self.settings.language;
        if self.viewport_mode == ViewportMode::Schematic {
            let undo_len = self.undo_stack.len();
            self.push_undo_snapshot();
            if self.schematic_view.duplicate_selection() {
                self.mark_scene_modified();
                let msg = t("app.duplicate_menu", _lang);
                self.last_action = msg.to_string();
                self.console.log(LogLevel::Info, &msg);
            } else if self.undo_stack.len() > undo_len {
                self.undo_stack.pop();
            }
            return;
        }
        if self.viewport_mode == ViewportMode::Pcb {
            let msg = t("app.pcb_duplicate_disabled", _lang);
            self.last_action = msg.to_string();
            self.console.log(LogLevel::Info, &msg);
            return;
        }
        let ids = self.hierarchy.selected_nodes.clone();
        if ids.is_empty() {
            if let Some(id) = self.hierarchy.selected_node {
                self.push_undo_snapshot();
                if let Some(new_id) = self.scene.duplicate_node(id) {
                    self.hierarchy.selected_node = Some(new_id);
                    self.hierarchy.selected_nodes = vec![new_id];
                    self.viewport.selected = vec![new_id];
                    let name = self
                        .scene
                        .get(new_id)
                        .map(|n| n.name.clone())
                        .unwrap_or_default();
                    let msg = format!("{} {}", t("app.duplicated_msg", _lang), name);
                    self.last_action = msg.clone();
                    self.console.log(LogLevel::Info, &msg);
                }
            }
        } else {
            self.push_undo_snapshot();
            let mut new_ids = Vec::new();
            for id in &ids {
                if let Some(new_id) = self.scene.duplicate_node(*id) {
                    new_ids.push(new_id);
                }
            }
            if !new_ids.is_empty() {
                self.hierarchy.selected_node = Some(new_ids[0]);
                self.hierarchy.selected_nodes = new_ids.clone();
                self.viewport.selected = new_ids;
                let msg = format!("{} {} nodes", t("app.duplicated_msg", _lang), ids.len());
                self.last_action = msg.clone();
                self.console.log(LogLevel::Info, &msg);
            }
        }
    }

    fn do_select_all(&mut self) {
        if self.runtime.is_some() && self.viewport_mode == ViewportMode::Scene {
            let msg = t("app.runtime_scene_locked", self.settings.language);
            self.last_action = msg.clone();
            self.console.log(LogLevel::Info, &msg);
            return;
        }

        if self.viewport_mode == ViewportMode::Schematic {
            self.schematic_view.clear_selection();
            let _lang = self.settings.language;
            let msg = format!(
                "{} {} | {} {}",
                t("app.schematic_components", _lang),
                self.schematic_view.schematic.components.len(),
                t("app.schematic_wires", _lang),
                self.schematic_view.schematic.wires.len()
            );
            self.last_action = msg.clone();
            self.console.log(LogLevel::Info, &msg);
            return;
        }
        if self.viewport_mode == ViewportMode::Pcb {
            self.pcb_view.clear_selection();
            let _lang = self.settings.language;
            let msg = format!(
                "{} {} | {} {} | {} {}",
                t("app.pcb_components", _lang),
                self.pcb_view.layout.components.len(),
                t("app.pcb_traces", _lang),
                self.pcb_view.layout.traces.len(),
                t("app.pcb_airwires", _lang),
                self.pcb_view.layout.airwires.len()
            );
            self.last_action = msg.clone();
            self.console.log(LogLevel::Info, &msg);
            return;
        }
        let ids = self.scene.all_valid_ids();
        self.hierarchy.selected_nodes = ids.clone();
        self.hierarchy.selected_node = ids.first().copied();
        self.viewport.selected = ids.clone();
        let _lang = self.settings.language;
        let msg = format!("{} {}", ids.len(), t("app.entities_found_msg", _lang));
        self.last_action = msg.clone();
        self.console.log(LogLevel::Info, &msg);
    }

    fn show_editor_context_actions(
        &mut self,
        ui: &mut egui::Ui,
        _palette: &app_theme::ThemePalette,
        lang: Language,
    ) {
        let is_electronics_project = self
            .current_project
            .as_ref()
            .map(|project| project.project_type == ProjectType::Electronics)
            .unwrap_or(false);
        let mode_text = match self.viewport_mode {
            ViewportMode::Scene => t("app.scene_view", lang),
            ViewportMode::Schematic => t("app.schematic_view", lang),
            ViewportMode::Pcb => t("app.pcb_view", lang),
        };
        let palette = if ui.visuals().dark_mode {
            StudioUiPalette::IndustrialDark
        } else {
            StudioUiPalette::PaperLight
        };
        let actions = self.context_actions_surface.show(
            ui,
            self.egui_wgpu_render_state.as_ref(),
            palette,
            lang,
            is_electronics_project,
            !self.undo_stack.is_empty(),
            !self.redo_stack.is_empty(),
            self.settings.show_fps_counter,
            self.frame_timing.fps(),
            !self.scene_modified,
            mode_text,
        );
        for action in actions {
            match action {
                EditorContextAction::Build => self.handle_build(),
                EditorContextAction::Undo => self.do_undo(),
                EditorContextAction::Redo => self.do_redo(),
            }
        }
    }

    /// Renders the shared command tree inside eframe while it remains the
    /// temporary window shell. A native host consumes the exact same model.
    fn show_editor_menu_items(&mut self, ui: &mut egui::Ui) {
        let state = self.editor_application_menu_state();
        let menu = build_editor_application_menu(state);
        let language = self.settings.language;
        show_eframe_application_menu(ui, &menu, language, state, &mut |command_id| {
            self.dispatch_application_menu_command(command_id);
        });
    }

    fn editor_application_menu_state(&self) -> EditorApplicationMenuState {
        let electronics_project = self
            .current_project
            .as_ref()
            .is_some_and(|project| project.project_type == ProjectType::Electronics);
        EditorApplicationMenuState {
            electronics_project,
            project_open: self.current_project.is_some(),
            can_undo: !self.undo_stack.is_empty(),
            can_redo: !self.redo_stack.is_empty(),
            grid_visible: self.settings.grid_visible,
            scene_active: self.viewport_mode == ViewportMode::Scene,
            schematic_active: self.viewport_mode == ViewportMode::Schematic,
            pcb_active: self.viewport_mode == ViewportMode::Pcb,
            undo_count: self.undo_stack.len(),
            redo_count: self.redo_stack.len(),
        }
    }

    fn poll_native_application_menu(&mut self) {
        let activations = self
            .native_application_menu
            .as_mut()
            .map(NativeWindowApplicationMenuAdapter::drain_activations)
            .unwrap_or_default();
        for activation in activations {
            self.dispatch_application_menu_command(&activation.command_id);
        }
    }

    fn bind_native_application_menu_to_frame(&mut self, frame: &eframe::Frame) {
        if self.native_menu_bound_to_frame {
            return;
        }
        self.native_menu_bound_to_frame = true;

        let Ok(window_handle) = frame.window_handle() else {
            tracing::warn!("native application menu frame handle unavailable");
            return;
        };
        tracing::info!(handle = ?window_handle.as_raw(), "binding native application menu to frame");
        let menu = build_editor_application_menu(self.editor_application_menu_state());
        let language = self.settings.language;
        let Some(adapter) = self.native_application_menu.as_mut() else {
            return;
        };
        if let Err(error) =
            adapter.install_raw_window_handle(window_handle.as_raw(), &menu, |key| {
                t(key, language).to_string()
            })
        {
            tracing::warn!(error = %error, "native application menu unavailable");
        }
    }

    fn sync_native_application_menu(&mut self) {
        let state = self.editor_application_menu_state();
        let menu = build_editor_application_menu(state);
        let language = self.settings.language;
        let Some(adapter) = self.native_application_menu.as_mut() else {
            return;
        };
        if !adapter.is_installed() {
            return;
        }
        if let Err(error) = adapter.sync(&menu, |key| t(key, language).to_string()) {
            tracing::debug!(error = %error, "native application menu sync failed");
        }
    }

    fn dispatch_application_menu_command(&mut self, command_id: &str) {
        match command_id {
            application_menu_command::PROJECT_NEW
            | application_menu_command::PROJECT_EXIT_TO_HUB
            | application_menu_command::PROJECT_CLOSE => {
                self.request_exit_action(PendingExitAction::ToHub, None);
            }
            application_menu_command::PROJECT_SAVE => {
                self.do_save();
            }
            application_menu_command::EDITOR_SETTINGS => {
                self.open_settings_screen(AppScreen::Editor);
            }
            application_menu_command::EDIT_UNDO => self.do_undo(),
            application_menu_command::EDIT_REDO => self.do_redo(),
            application_menu_command::EDIT_DUPLICATE => self.do_duplicate(),
            application_menu_command::EDIT_DELETE => self.do_delete(),
            application_menu_command::EDIT_SELECT_ALL => self.do_select_all(),
            application_menu_command::VIEW_GRID => {
                self.settings.grid_visible = !self.settings.grid_visible;
            }
            application_menu_command::VIEW_SCENE => {
                self.viewport_mode = ViewportMode::Scene;
            }
            application_menu_command::VIEW_SCHEMATIC => {
                self.viewport_mode = ViewportMode::Schematic;
            }
            application_menu_command::VIEW_PCB => {
                self.sync_pcb_from_schematic();
                self.viewport_mode = ViewportMode::Pcb;
            }
            application_menu_command::PROJECT_OPEN_FOLDER => {
                #[cfg(target_os = "windows")]
                if let Some(project) = &self.current_project {
                    let _ = std::process::Command::new("explorer")
                        .arg(project.path.as_os_str())
                        .spawn();
                }
            }
            application_menu_command::RAFUI_STUDIO_PREVIEW => {
                self.screen = AppScreen::RafUiStudio;
                let project = self.current_project.clone();
                let output = match parse_console_input("/rafui.studio.preview") {
                    Ok(ParsedInput::Command(command)) => self.execute_shared_console_command(
                        "rafui.studio.preview",
                        &command,
                        project.as_ref(),
                    ),
                    _ => CommandOutput::error("RafUI Studio", "Unable to build preview command."),
                };
                self.console
                    .log_user("RafUI Studio", "/rafui.studio.preview");
                self.console.log_command_output(output);
                self.bottom_tab = BottomTab::Console;
                self.last_action = "RafUI Studio preview generated".to_string();
            }
            application_menu_command::HELP_KEYBOARD_SHORTCUTS => {
                self.last_action = format!(
                    "{} v{}",
                    t("app.editor_brand", self.settings.language),
                    env!("CARGO_PKG_VERSION")
                );
            }
            _ => {}
        }
    }

    fn do_save(&mut self) -> bool {
        match self.save_current_project() {
            Ok(()) => {
                let msg = t("app.project_saved", self.settings.language);
                self.last_action = msg.to_string();
                self.console.log(LogLevel::Info, &msg);
                true
            }
            Err(error) => {
                let msg = format!(
                    "{}: {}",
                    t("app.project_save_failed", self.settings.language),
                    error
                );
                self.last_action = msg.clone();
                self.console.log(LogLevel::Error, &msg);
                false
            }
        }
    }

    fn save_current_project(&mut self) -> Result<(), String> {
        let mut project = self
            .current_project
            .clone()
            .ok_or_else(|| "No active project".to_string())?;

        project.modified_at = Utc::now();
        let active_session = self
            .sessions
            .active()
            .cloned()
            .ok_or_else(|| "No active project session".to_string())?;

        match project.project_type {
            ProjectType::Game => {
                save_game_session(
                    &project,
                    &active_session,
                    &self.scene,
                    &self.node_editor.document(),
                    &self.ui_document,
                    &self.viewport.editor_camera_block(),
                )?;
            }
            ProjectType::Electronics => {
                self.sync_pcb_from_schematic();
                let schematic_path =
                    active_session.path(&project.path, &active_session.schematic_file);
                let pcb_path = active_session.path(&project.path, &active_session.pcb_file);

                save_schematic_document(&schematic_path, &self.schematic_view.schematic).map_err(
                    |error| format!("{}: {}", active_session.schematic_file.display(), error),
                )?;
                save_pcb_document(&pcb_path, &self.pcb_view.layout)
                    .map_err(|error| format!("{}: {}", active_session.pcb_file.display(), error))?;
                crate::session_document::save_ui_document(
                    &active_session.path(&project.path, &active_session.ui_document_file),
                    &self.ui_document,
                    &active_session.ui_document_file,
                )?;
            }
        }

        self.sessions.save(&project.path)?;
        self.editor_shell.save(&project.path)?;
        self.editor_shell_dirty = false;

        project
            .save()
            .map_err(|error| format!("project.ron: {}", error))?;

        self.current_project = Some(project);
        self.scene_modified = false;
        self.auto_save_elapsed = 0.0;
        self.auto_save_last_tick = None;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // v0.3.0: Global shortcuts
    // -----------------------------------------------------------------------

    fn do_copy(&mut self) {
        if self.viewport_mode != ViewportMode::Scene {
            return;
        }
        if self.hierarchy.selected_nodes.is_empty() {
            return;
        }
        self.scene_clipboard.clear();
        for &id in &self.hierarchy.selected_nodes {
            if let Some(node) = self.scene.get(id) {
                self.scene_clipboard.push(node.clone());
            }
        }
        let _lang = self.settings.language;
        let msg = format!(
            "{} {}",
            self.scene_clipboard.len(),
            t("app.copied_msg", _lang)
        );
        self.last_action = msg.clone();
        self.console.log(LogLevel::Info, &msg);
    }

    fn do_paste(&mut self) {
        if self.viewport_mode != ViewportMode::Scene {
            return;
        }
        if self.scene_clipboard.is_empty() {
            return;
        }
        if self.runtime.is_some() {
            let msg = t("app.runtime_scene_locked", self.settings.language);
            self.last_action = msg.clone();
            self.console.log(LogLevel::Info, &msg);
            return;
        }
        self.push_undo_snapshot();
        let _lang = self.settings.language;
        let paste_count = self.scene_clipboard.len();
        let mut new_ids = Vec::new();
        let mut offset_index = 0u32;
        for node in &self.scene_clipboard {
            // Add a root node with the clipboard node's data, then offset it.
            let id = self
                .scene
                .add_root_with_primitive(&node.name, node.primitive);
            if let Some(n) = self.scene.get_mut(id) {
                n.position =
                    node.position + glam::Vec3::new(1.0, 0.0, 1.0) * (offset_index as f32 + 1.0);
                n.rotation = node.rotation;
                n.scale = node.scale;
                n.color = node.color;
                n.visible = node.visible;
                n.name = format!("{} copy", node.name);
            }
            new_ids.push(id);
            offset_index += 1;
        }
        if !new_ids.is_empty() {
            self.hierarchy.selected_nodes = new_ids.clone();
            self.hierarchy.selected_node = new_ids.first().copied();
            self.viewport.selected = new_ids;
            self.mark_scene_modified();
            let msg = format!("{} {}", paste_count, t("app.pasted_msg", _lang));
            self.last_action = msg.clone();
            self.console.log(LogLevel::Info, &msg);
        }
    }

    fn do_bookmark_save(&mut self, slot: usize) {
        if self.viewport_mode != ViewportMode::Scene {
            return;
        }
        let snapshot = self.viewport.camera_bookmark_snapshot();
        self.camera_bookmarks[slot] = Some(snapshot);
        let _lang = self.settings.language;
        let msg = format!("{} {}", t("app.bookmark_saved", _lang), slot + 1);
        self.last_action = msg.clone();
        self.console.log(LogLevel::Info, &msg);
    }

    fn do_bookmark_restore(&mut self, slot: usize) {
        if self.viewport_mode != ViewportMode::Scene {
            return;
        }
        let Some((target, yaw, pitch, dist)) = self.camera_bookmarks[slot] else {
            let _lang = self.settings.language;
            let msg = format!("{} {}", t("app.bookmark_empty", _lang), slot + 1);
            self.last_action = msg.clone();
            self.console.log(LogLevel::Info, &msg);
            return;
        };
        self.viewport
            .restore_camera_bookmark(target, yaw, pitch, dist);
        let _lang = self.settings.language;
        let msg = format!("{} {}", t("app.bookmark_restored", _lang), slot + 1);
        self.last_action = msg.clone();
        self.console.log(LogLevel::Info, &msg);
    }

    fn handle_global_shortcuts(&mut self, ctx: &egui::Context) {
        let text_input_active = ctx.wants_keyboard_input();
        let node_editor_owns_history = self.bottom_tab == BottomTab::NodeEditor;
        let action: Option<u8> = ctx.input(|i| {
            let ctrl = i.modifiers.ctrl || i.modifiers.mac_cmd;
            if ctrl && i.key_pressed(egui::Key::S) {
                return Some(4);
            }
            if text_input_active {
                return None;
            }
            if ctrl && i.modifiers.shift && i.key_pressed(egui::Key::Z) && !node_editor_owns_history
            {
                return Some(2);
            }
            if ctrl && i.key_pressed(egui::Key::Z) && !node_editor_owns_history {
                return Some(1);
            }
            if ctrl && i.key_pressed(egui::Key::Y) && !node_editor_owns_history {
                return Some(2);
            }
            if ctrl && i.key_pressed(egui::Key::D) {
                return Some(3);
            }
            if ctrl && i.key_pressed(egui::Key::A) {
                return Some(5);
            }
            if i.key_pressed(egui::Key::Delete) {
                return Some(6);
            }
            if ctrl && i.key_pressed(egui::Key::C) {
                return Some(7);
            }
            if ctrl && i.key_pressed(egui::Key::V) {
                return Some(8);
            }
            if ctrl && i.key_pressed(egui::Key::Num1) {
                return Some(9);
            }
            if ctrl && i.key_pressed(egui::Key::Num2) {
                return Some(10);
            }
            if ctrl && i.key_pressed(egui::Key::Num3) {
                return Some(11);
            }
            if !ctrl && i.key_pressed(egui::Key::Num1) {
                return Some(12);
            }
            if !ctrl && i.key_pressed(egui::Key::Num2) {
                return Some(13);
            }
            if !ctrl && i.key_pressed(egui::Key::Num3) {
                return Some(14);
            }
            None
        });
        match action {
            Some(1) => self.do_undo(),
            Some(2) => self.do_redo(),
            Some(3) => self.do_duplicate(),
            Some(4) => {
                let _ = self.do_save();
            }
            Some(5) => self.do_select_all(),
            Some(6) => self.do_delete(),
            Some(7) => self.do_copy(),
            Some(8) => self.do_paste(),
            Some(9) => self.do_bookmark_save(0),
            Some(10) => self.do_bookmark_save(1),
            Some(11) => self.do_bookmark_save(2),
            Some(12) => self.do_bookmark_restore(0),
            Some(13) => self.do_bookmark_restore(1),
            Some(14) => self.do_bookmark_restore(2),
            _ => {}
        }
    }

    // -----------------------------------------------------------------------
    // v0.3.0: Auto-save
    // -----------------------------------------------------------------------

    fn handle_auto_save(&mut self, ctx: &egui::Context) {
        if !self.scene_modified {
            self.auto_save_last_tick = None;
            return;
        }

        let now = ctx.input(|i| i.time);
        let dt = if let Some(last_tick) = self.auto_save_last_tick {
            (now - last_tick).max(0.0) as f32
        } else {
            0.0
        };
        self.auto_save_last_tick = Some(now);
        self.auto_save_elapsed += dt;
        let interval = self.settings.auto_save_interval_seconds as f32;
        if interval > 0.0 && self.auto_save_elapsed >= interval {
            match self.save_current_project() {
                Ok(()) => {
                    let msg = t("app.auto_saved", self.settings.language);
                    self.last_action = msg.to_string();
                    self.console.log(LogLevel::Info, &msg);
                }
                Err(error) => {
                    let msg = format!(
                        "{}: {}",
                        t("app.auto_save_failed", self.settings.language),
                        error
                    );
                    self.last_action = msg.clone();
                    self.console.log(LogLevel::Error, &msg);
                    self.auto_save_elapsed = 0.0;
                    self.auto_save_last_tick = Some(now);
                }
            }
        }
    }

    fn mark_scene_modified(&mut self) {
        self.scene_modified = true;
        self.auto_save_elapsed = 0.0;
        self.auto_save_last_tick = None;
    }

    // -----------------------------------------------------------------------
    // Build / Run
    // -----------------------------------------------------------------------

    fn handle_build(&mut self) {
        if let Some(project) = self.current_project.clone() {
            match project.project_type {
                ProjectType::Game => {
                    self.runtime = None;
                    let msg = t("app.runtime_temporarily_disabled", self.settings.language);
                    self.last_action = msg.clone();
                    self.console.log(LogLevel::Info, &msg);
                }
                ProjectType::Electronics => {
                    self.console.log(LogLevel::Info, "Running DC Simulation...");

                    // 1. Run design checks first
                    let results = self.schematic_view.schematic.electrical_test();
                    for result in &results {
                        if result.contains("passed") {
                            self.console.log(LogLevel::Info, &result);
                        } else {
                            self.console.log(LogLevel::Warning, &result);
                        }
                    }

                    // 2. Run actual math simulation
                    let sim_results =
                        raf_electronics::simulation::simulate_dc(&self.schematic_view.schematic);
                    if sim_results.converged {
                        self.console
                            .log(LogLevel::Info, "Simulation converged successfully.");
                        for (ci, current) in &sim_results.component_currents {
                            let component_label = self
                                .schematic_view
                                .schematic
                                .components
                                .get(*ci)
                                .map(|component| component.designator.clone())
                                .unwrap_or_else(|| format!("#{ci}"));
                            let msg = format!(
                                "Component [{component_label}]: Current = {:.5} A",
                                current
                            );
                            self.console.log(LogLevel::Info, &msg);
                        }
                        for (net_id, voltage) in &sim_results.node_voltages {
                            let msg = format!("Net [N{net_id:03}]: Voltage = {voltage:.2} V");
                            self.console.log(LogLevel::Info, &msg);
                        }
                    } else {
                        self.console
                            .log(LogLevel::Error, "Simulation failed to converge.");
                        for err in &sim_results.messages {
                            self.console.log(LogLevel::Error, &err);
                        }
                    }
                    self.electronics_simulation_results = Some(sim_results);
                }
            }
        }
    }

    fn run_electronics_drc(&mut self) {
        let Some(project) = self.current_project.as_ref() else {
            return;
        };
        if project.project_type != ProjectType::Electronics {
            return;
        }

        let report = raf_electronics::drc::run_drc(&self.schematic_view.schematic);
        let summary = if report.passed() {
            t("app.drc_ok", self.settings.language)
        } else {
            format!(
                "DRC: {} {}",
                report.total(),
                t("app.drc_errors", self.settings.language)
            )
        };
        self.last_action = summary.clone();
        self.console.log(
            if report.passed() {
                LogLevel::Info
            } else {
                LogLevel::Warning
            },
            &summary,
        );
        for issue in report.to_string_list() {
            self.console.log(
                if issue.starts_with("[ERROR]") {
                    LogLevel::Error
                } else if issue.starts_with("[WARNING]") {
                    LogLevel::Warning
                } else {
                    LogLevel::Info
                },
                &issue,
            );
        }
        self.electronics_drc_report = Some(report);
    }

    fn sync_pcb_from_schematic(&mut self) {
        let summary = self
            .pcb_view
            .sync_from_schematic(&self.schematic_view.schematic);
        let msg = format!(
            "PCB sync: +{} / ~{} / -{} / {} nets",
            summary.added_components,
            summary.updated_components,
            summary.removed_components,
            summary.nets,
        );
        self.last_action = msg.clone();
        self.console.log(LogLevel::Info, &msg);
    }

    fn poll_image_generation(&mut self) {
        for completed in self.image_generation.poll() {
            match completed.status {
                AssetImageJobStatus::Ready(asset) => {
                    self.last_action = format!(
                        "{}: {}",
                        t("app.asset_image_ready", self.settings.language),
                        asset.image_path.display()
                    );
                    self.console.log(LogLevel::Info, &self.last_action);
                    self.asset_browser.scan_project_folder();
                }
                AssetImageJobStatus::Failed(error) => {
                    self.last_action = format!(
                        "{}: {error}",
                        t("app.asset_image_failed", self.settings.language)
                    );
                    self.console.log(LogLevel::Error, &self.last_action);
                }
                AssetImageJobStatus::Cancelled => {
                    self.last_action = t("app.asset_image_cancelled", self.settings.language);
                    self.console.log(LogLevel::Info, &self.last_action);
                }
                AssetImageJobStatus::Running => {}
            }
        }
    }

    fn process_console_submissions(
        &mut self,
        submissions: Vec<crate::panels::console::ConsoleSubmission>,
    ) {
        for submission in submissions {
            self.console.log_user("User1", &submission.text);
            let parsed = match parse_console_input(&submission.text) {
                Ok(parsed) => parsed,
                Err(error) => {
                    self.console.log(
                        LogLevel::Error,
                        &format!(
                            "{}: {error}",
                            t("console.parse_error", self.settings.language)
                        ),
                    );
                    continue;
                }
            };
            let ParsedInput::Command(command) = parsed else {
                continue;
            };
            let Some(definition) = self.command_catalog.find(&command.name).cloned() else {
                self.console.log(
                    LogLevel::Error,
                    &format!(
                        "{}: {}",
                        t("console.unknown_command", self.settings.language),
                        command.name
                    ),
                );
                continue;
            };
            let project = self.current_project.clone();
            if !console_domain_allowed(&definition.domain, project.as_ref()) {
                self.console.log(
                    LogLevel::Error,
                    &t("console.command_unavailable", self.settings.language),
                );
                continue;
            }

            let before_snapshot = if definition.name.starts_with("ui.") {
                ron::ser::to_string(&self.ui_document)
                    .ok()
                    .map(EditorHistorySnapshot::UiDocument)
            } else {
                self.current_history_snapshot()
            };
            let output = match definition.domain.as_str() {
                "game" => {
                    let mut context = crate::commands::game::GameCommandContext {
                        scene: &mut self.scene,
                        hierarchy: &mut self.hierarchy,
                        viewport: &mut self.viewport,
                    };
                    crate::commands::game::execute(&definition.name, &command, &mut context)
                }
                "electronics" => {
                    let mut context = crate::commands::electronics::ElectronicsCommandContext {
                        schematic_view: &mut self.schematic_view,
                        pcb_view: &mut self.pcb_view,
                    };
                    crate::commands::electronics::execute(&definition.name, &command, &mut context)
                }
                "shared" => self.execute_shared_console_command(
                    &definition.name,
                    &command,
                    project.as_ref(),
                ),
                _ => CommandOutput::error("Console", "Unsupported command domain."),
            };
            if output.changed {
                if let Some(snapshot) = before_snapshot {
                    let document_changed = self
                        .current_history_snapshot_like(&snapshot)
                        .map(|current| current != snapshot)
                        .unwrap_or(true);
                    if document_changed {
                        self.push_history_snapshot(snapshot);
                    }
                }
            }
            self.console.log_command_output(output);
            self.process_session_events();
        }
    }

    fn execute_shared_console_command(
        &mut self,
        name: &str,
        command: &crate::commands::ParsedCommand,
        project: Option<&Project>,
    ) -> CommandOutput {
        match name {
            "workspace.read" => project
                .map(|project| crate::commands::workspace::read_file(command, &project.path))
                .unwrap_or_else(|| CommandOutput::error("Workspace", "No active project.")),
            "workspace.search" => project
                .map(|project| crate::commands::workspace::search(command, &project.path))
                .unwrap_or_else(|| CommandOutput::error("Workspace", "No active project.")),
            "script.create"
            | "script.attach"
            | "script.detach"
            | "script.list"
            | "script.validate"
            | "script.run"
            | "script.compile_nodes" => {
                let assets_root = project.map(|project| project.path.join("assets"));
                let mut context = crate::commands::script::ScriptCommandContext {
                    scene: &mut self.scene,
                    assets_root: assets_root.as_deref(),
                };
                crate::commands::script::execute(name, command, &mut context)
            }
            "asset.generate_image"
            | "asset.generate_local_png"
            | "asset.image_status"
            | "asset.cancel_image" => {
                let project_root = project.map(|project| project.path.as_path());
                let mut context = crate::commands::assets::AssetCommandContext {
                    project_root,
                    image_queue: &mut self.image_generation,
                };
                crate::commands::assets::execute(name, command, &mut context)
            }
            "session.list" | "session.create" | "session.open" | "session.duplicate"
            | "session.remove" => {
                let mut context = crate::commands::sessions::SessionCommandContext {
                    project,
                    registry: &mut self.sessions,
                    events: &mut self.pending_session_events,
                };
                crate::commands::sessions::execute(name, command, &mut context)
            }
            "ui.document.describe"
            | "ui.node.add"
            | "ui.node.remove"
            | "ui.document.set_space"
            | "ui.document.bind_camera"
            | "ui.document.clear_camera"
            | "rafui.studio.preview" => {
                let mut context = crate::commands::ui_document::UiDocumentCommandContext {
                    document: &mut self.ui_document,
                };
                crate::commands::ui_document::execute(name, command, &mut context)
            }
            "undo" => {
                self.do_undo();
                CommandOutput::info(
                    "Undo",
                    vec!["status: applied".to_string()],
                    serde_json::json!({"ok": true}),
                )
            }
            "redo" => {
                self.do_redo();
                CommandOutput::info(
                    "Redo",
                    vec!["status: applied".to_string()],
                    serde_json::json!({"ok": true}),
                )
            }
            "project.info" => {
                let lines = project
                    .map(|project| {
                        vec![
                            format!("name: {}", project.name),
                            format!("type: {:?}", project.project_type),
                            format!("session: {}", self.active_session_name()),
                        ]
                    })
                    .unwrap_or_else(|| vec!["No active project.".to_string()]);
                CommandOutput::info(
                    "Project info",
                    lines,
                    serde_json::json!({"ok": project.is_some()}),
                )
            }
            "help" | "commands" | "describe" | "history" | "clear" => CommandOutput::info(
                "Console",
                vec![
                    format!("command: {name}"),
                    format!("available: {}", self.command_catalog.commands.len()),
                ],
                serde_json::json!({"ok": true, "command": name}),
            ),
            _ => CommandOutput::error("Console", format!("Not routed: {name}")),
        }
    }

    fn process_session_events(&mut self) {
        let events = std::mem::take(&mut self.pending_session_events);
        for event in events {
            match event {
                SessionCommandEvent::Activate(session_id) => {
                    if let Err(error) = self.activate_session(session_id) {
                        self.console.log(LogLevel::Error, &error);
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
        if self.scene_modified {
            self.save_current_project()?;
        }
        self.sessions.set_active(session_id);
        self.load_active_session_documents(&project)?;
        self.sessions.save(&project.path)?;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.pending_history_snapshot = None;
        self.scene_modified = false;
        self.auto_save_elapsed = 0.0;
        self.auto_save_last_tick = None;
        self.last_action = format!("Active session: {}", self.active_session_name());
        self.console.log(LogLevel::Info, &self.last_action);
        Ok(())
    }

    fn create_and_activate_session(
        &mut self,
        name: String,
        kind: raf_core::session::ProjectSessionKind,
    ) -> Result<(), String> {
        let project = self
            .current_project
            .clone()
            .ok_or_else(|| "No active project.".to_string())?;
        let id = self.sessions.create(name, kind);
        let session = self
            .sessions
            .sessions
            .iter()
            .find(|session| session.id == id)
            .ok_or_else(|| "Created session was not registered.".to_string())?;
        session
            .ensure_storage(&project.path)
            .map_err(|error| format!("session storage: {error}"))?;
        self.sessions.save(&project.path)?;
        self.activate_session(id)
    }

    fn active_session_name(&self) -> &str {
        self.sessions
            .active()
            .map(|session| session.name.as_str())
            .unwrap_or("Main")
    }

    fn load_active_session_documents(&mut self, project: &Project) -> Result<(), String> {
        let active_session = self
            .sessions
            .active()
            .cloned()
            .ok_or_else(|| "Project has no active session.".to_string())?;
        self.init_scene_for_type(project.project_type);
        self.runtime = None;
        self.viewport
            .apply_editor_camera_block(&raf_render::bridge::EditorCameraBlock::default());
        self.ui_document = load_ui_document(
            &active_session.path(&project.path, &active_session.ui_document_file),
            &active_session,
        );

        match project.project_type {
            ProjectType::Game => {
                let document = load_game_session(project, &active_session);
                if let Some(scene) = document.scene {
                    self.scene = scene;
                    let msg = t("app.scene_loaded_from_file", self.settings.language);
                    self.console.log(LogLevel::Info, &msg);
                }
                self.node_editor.load_document(document.nodes);
                self.ui_document = document.ui_document;
                if let Some(camera) = document.editor_camera {
                    self.viewport.apply_editor_camera_block(&camera);
                }
            }
            ProjectType::Electronics => {
                self.node_editor
                    .load_document(NodeEditorDocument::default());
                let schematic_path =
                    active_session.path(&project.path, &active_session.schematic_file);
                let pcb_path = active_session.path(&project.path, &active_session.pcb_file);
                self.schematic_view.schematic = if schematic_path.exists() {
                    if let Some(schematic) = load_schematic_document(&schematic_path) {
                        let msg = t("app.schematic_loaded_from_file", self.settings.language);
                        self.console.log(LogLevel::Info, &msg);
                        schematic
                    } else {
                        raf_electronics::schematic::Schematic::new(&project.name)
                    }
                } else {
                    raf_electronics::schematic::Schematic::new(&project.name)
                };
                self.pcb_view.layout = if pcb_path.exists() {
                    if let Some(layout) = load_pcb_document(&pcb_path) {
                        let msg = t("app.pcb_loaded_from_file", self.settings.language);
                        self.console.log(LogLevel::Info, &msg);
                        layout
                    } else {
                        raf_electronics::PcbLayout::new(&project.name)
                    }
                } else {
                    raf_electronics::PcbLayout::new(&project.name)
                };
                self.sync_pcb_from_schematic();
                self.schematic_view.clear_selection();
                self.pcb_view.clear_selection();
            }
        }

        self.viewport_mode = match project.project_type {
            ProjectType::Game => ViewportMode::Scene,
            ProjectType::Electronics => ViewportMode::Schematic,
        };
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn open_project(&mut self, path: &std::path::Path) {
        match Project::load(path) {
            Ok(project) => {
                let config_dir = dirs_config_dir();
                self.recent_projects.add(&project);
                let _ = self.recent_projects.save(&config_dir);
                let ptype = project.project_type;
                self.sessions = ProjectSessionRegistry::load_or_legacy(&project.path, ptype);
                if let Err(error) = self.load_active_session_documents(&project) {
                    self.console.log(LogLevel::Error, &error);
                    return;
                }

                // Clear undo/redo for new session.
                self.undo_stack.clear();
                self.redo_stack.clear();
                self.scene_modified = false;
                self.auto_save_elapsed = 0.0;
                self.auto_save_last_tick = None;
                self.last_action.clear();

                self.current_project = Some(project.clone());
                self.editor_shell = EditorShellLayout::load(&project.path);
                self.editor_shell_dirty = false;
                self.editor_shell_resize_panel = None;
                // Wire assets path to browser.
                let assets_dir = std::path::PathBuf::from(&project.path).join("assets");
                self.asset_browser.project_assets_path = Some(assets_dir);
                self.asset_browser.scan_project_folder();
                self.screen = AppScreen::Editor;
                let _lang = self.settings.language;
                let msg = t("app.project_loaded", self.settings.language);
                self.console.log(LogLevel::Info, &msg);
            }
            Err(e) => {
                self.console
                    .log(LogLevel::Error, &format!("Failed to open project: {}", e));
            }
        }
    }

    fn init_scene_for_type(&mut self, project_type: ProjectType) {
        self.scene = SceneGraph::new();
        self.electronics_drc_report = None;
        self.electronics_simulation_results = None;
        match project_type {
            ProjectType::Game => {
                let root = self.scene.add_root("Scene Root");
                self.scene.add_child(root, "Directional Light");
            }
            ProjectType::Electronics => {
                self.scene.add_root("Schematic Root");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Utility functions
// ---------------------------------------------------------------------------

fn console_domain_allowed(domain: &str, project: Option<&Project>) -> bool {
    match domain {
        "shared" => true,
        "game" => project.map(|project| project.project_type) == Some(ProjectType::Game),
        "electronics" => {
            project.map(|project| project.project_type) == Some(ProjectType::Electronics)
        }
        _ => false,
    }
}

/// Get the config directory for storing settings. Creates it if needed.
fn dirs_config_dir() -> std::path::PathBuf {
    let dir = dirs_next::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("AuraRafi");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Default directory for new projects.
fn default_projects_dir() -> String {
    dirs_next::document_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("AuraRafi Projects")
        .display()
        .to_string()
}

fn bottom_tab_to_shell(tab: &BottomTab) -> EditorBottomDockTab {
    match tab {
        BottomTab::Assets => EditorBottomDockTab::Assets,
        BottomTab::Console => EditorBottomDockTab::Console,
        BottomTab::Drc => EditorBottomDockTab::Drc,
        BottomTab::Simulation => EditorBottomDockTab::Simulation,
        BottomTab::AiChat => EditorBottomDockTab::Agent,
        BottomTab::NodeEditor => EditorBottomDockTab::NodeEditor,
        BottomTab::ProjectSettings | BottomTab::Complement(_) => {
            EditorBottomDockTab::ProjectSettings
        }
    }
}

fn viewport_mode_to_shell(mode: ViewportMode) -> EditorCenterSurface {
    match mode {
        ViewportMode::Scene => EditorCenterSurface::Scene,
        ViewportMode::Schematic => EditorCenterSurface::Schematic,
        ViewportMode::Pcb => EditorCenterSurface::Pcb,
    }
}

fn bottom_tab_from_shell(tab: EditorBottomDockTab) -> BottomTab {
    match tab {
        EditorBottomDockTab::Assets => BottomTab::Assets,
        EditorBottomDockTab::Console => BottomTab::Console,
        EditorBottomDockTab::Drc => BottomTab::Drc,
        EditorBottomDockTab::Simulation => BottomTab::Simulation,
        EditorBottomDockTab::ProjectSettings => BottomTab::ProjectSettings,
        EditorBottomDockTab::NodeEditor => BottomTab::NodeEditor,
        EditorBottomDockTab::Agent => BottomTab::AiChat,
    }
}

fn inspector_tab_to_shell(tab: InspectorTab) -> EditorInspectorTab {
    match tab {
        InspectorTab::Properties => EditorInspectorTab::Properties,
        InspectorTab::Sessions => EditorInspectorTab::Sessions,
    }
}

fn inspector_tab_from_shell(tab: EditorInspectorTab) -> InspectorTab {
    match tab {
        EditorInspectorTab::Properties => InspectorTab::Properties,
        EditorInspectorTab::Sessions => InspectorTab::Sessions,
    }
}

#[cfg(test)]
mod shell_adapter_tests {
    use super::*;

    #[test]
    fn retained_bottom_tabs_preserve_existing_editor_destinations() {
        for tab in [
            BottomTab::Console,
            BottomTab::Drc,
            BottomTab::Simulation,
            BottomTab::Assets,
            BottomTab::ProjectSettings,
            BottomTab::NodeEditor,
            BottomTab::AiChat,
        ] {
            let shell = bottom_tab_to_shell(&tab);
            assert_eq!(bottom_tab_from_shell(shell), tab);
        }
    }

    #[test]
    fn complement_tab_falls_back_to_project_settings_in_the_shared_strip() {
        assert_eq!(
            bottom_tab_to_shell(&BottomTab::Complement("custom".to_string())),
            EditorBottomDockTab::ProjectSettings
        );
    }

    #[test]
    fn retained_inspector_tabs_preserve_properties_and_sessions() {
        for tab in [InspectorTab::Properties, InspectorTab::Sessions] {
            assert_eq!(inspector_tab_from_shell(inspector_tab_to_shell(tab)), tab);
        }
    }

    #[test]
    fn retained_context_tabs_preserve_every_center_surface() {
        for mode in [
            ViewportMode::Scene,
            ViewportMode::Schematic,
            ViewportMode::Pcb,
        ] {
            let shell = viewport_mode_to_shell(mode);
            let round_trip = match shell {
                EditorCenterSurface::Scene => ViewportMode::Scene,
                EditorCenterSurface::Schematic => ViewportMode::Schematic,
                EditorCenterSurface::Pcb => ViewportMode::Pcb,
            };
            assert_eq!(round_trip, mode);
        }
    }
}

fn ui_icon_budget(ctx: &egui::Context) -> usize {
    let dt = ctx.input(|i| i.predicted_dt.max(1.0 / 240.0));
    if dt > (1.0 / 45.0) {
        1
    } else {
        2
    }
}
include!("panels/complements.rs");
