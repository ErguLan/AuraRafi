//! Native RafUI workbench state and lifecycle coordinator.
//!
//! The retained composition lives in `native_workbench_surface.rs` and input
//! routing in `native_workbench_input.rs`. ApiGraphicBasic presents the result
//! directly over the scene canvas through the native compositor. Panel
//! builders remain presentation-only and the application consumes the
//! semantic actions returned by RafUI.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use raf_core::ai::AgentMode;
use raf_core::config::{EngineSettings, RenderQuality, Theme};
use raf_core::project::{Project, ProjectType};
use raf_core::scene::{SceneGraph, SceneNodeId};
use raf_core::session::{ProjectSessionRegistry, SessionId};
use raf_core::{
    AgentTaskEvent, AgentTaskId, AgentTaskSnapshot, InputRegionId, InputRouter, TransactionLedger,
};
use raf_nodes::{NodeGraph, NodeId};
use raf_render::api_graphic_basic::ui_surface::{
    DirectUiSurfaceHost, NativeGraphicsContext, NativeUiInputBridge, StudioUiPalette, UiAction,
    UiDispatchedAction, UiFlow, UiIconId, UiLayout, UiNode, UiNodeKind, UiStyle, UiStyleSheet,
    UiSurface,
};
use raf_render::api_graphic_basic::{EditorUiLayer, SceneFrameMetrics};
use raf_render::bridge::RenderRuntime;
use raf_ui::{UiColorMode, UiEnvironment, UiMotionSpec, UiRect, UiTween, UiWindowCommand};

use crate::agent_context::AgentObservationContext;
use crate::agent_executor::{AgentEditorAction, AgentProjectContext, AgentToolExecutor};
use crate::application_bar_host::{register_bar_images, register_electronics_images};
use crate::application_bar_surface::{
    application_menu_popup_height, build_application_bar_surface,
    build_application_menu_popup_surface, build_application_menu_popup_surface_with_submenu,
    APPLICATION_MENU_POPUP_WIDTH,
};
use crate::application_menu::{build_application_menu, ApplicationMenuState, ApplicationView};
use crate::commands::catalog::CommandCatalog;
use crate::commands::game::SceneSelectionState;
use crate::commands::parser::ParsedCommand;
use crate::console::ConsolePanel;
use crate::editor_layout::{EditorFrameLayout, EditorRect};
use crate::electronics_controller::{ElectronicsTool, NativeElectronicsEditor};
use crate::electronics_minimap;
use crate::panels::ai_chat::{AgentAction, AgentPanel, AgentReadiness};
use crate::panels::assets_surface::{asset_rows_with_builtins, build_assets_surface};
use crate::panels::assets_surface_host::AssetsSurfaceHost;
use crate::panels::editor_bottom_dock_host::EditorBottomDockHost;
use crate::panels::editor_bottom_dock_surface::{
    build_dock_splitter_surface, build_drop_preview_surface, build_tab_context_menu_surface,
    build_tab_strip_surface, BottomTabDragPreview,
};
use crate::panels::editor_panel_splitter_surface::{
    build_editor_splitter_surface, EditorSplitterKind,
};
use crate::panels::editor_status_surface::build_status_surface;
use crate::panels::electronics_canvas_overlay_surface::build_electronics_canvas_overlay_surface;
use crate::panels::electronics_inspector_surface::build_electronics_inspector_surface;
use crate::panels::electronics_navigator_surface::build_electronics_navigator_surface;
use crate::panels::electronics_toolbar_surface::build_electronics_toolbar_surface;
use crate::panels::hierarchy_model::HierarchyModel;
use crate::panels::hierarchy_surface::{
    build_hierarchy_context_overlay_surface, build_hierarchy_surface,
};
use crate::panels::inspector_surface::{
    build_inspector_surface_with_unit, InspectorDropdown, InspectorTab, InspectorViewState,
};
use crate::panels::nodes_surface::build_nodes_surface_with_zoom;
use crate::panels::search_surface::{SearchResult, SearchResultKind, SearchSurfaceState};
use crate::panels::search_surface_host::SearchSurfaceHost;
use crate::panels::viewport_compass::{
    ViewportCompassConfig, ViewportCompassHost, ViewportCompassState,
};
use crate::panels::viewport_toolbar_surface::{
    build_viewport_toolbar_surface, parse_viewport_toolbar_action,
    parse_viewport_toolbar_select_action, ViewportRenderStyle, ViewportTool, ViewportToolbarAction,
    ViewportToolbarState, ViewportViewMode,
};
use crate::project_catalog::ProjectCatalog;
use crate::settings_surface::SettingsSection;
use raf_core::project::BuildingStyle;
use uuid::Uuid;

#[path = "native_workbench_electronics.rs"]
mod electronics_model;
#[path = "native_workbench_helpers.rs"]
mod helpers;
#[path = "native_workbench_input.rs"]
mod input;
#[path = "native_workbench_settings.rs"]
mod settings;
#[path = "native_workbench_surface.rs"]
mod surface;

pub(crate) use helpers::{
    default_toolbar_state, hierarchy_drop_target_from_hovered, hierarchy_drop_target_is_valid,
    inspector_section_from_slug, numeric_commit_field, set_inspector_section, unique_session_name,
};
pub(crate) use settings::{
    ai_provider_from_id, apply_settings_command, apply_settings_range, apply_settings_select,
    apply_settings_text, apply_settings_toggle, is_settings_numeric_text_key,
};

const WORKBENCH_CLEAR: [u8; 4] = [0, 0, 0, 0];
const DEFAULT_PROJECT_NAME: &str = "Untitled Game";
const AGENT_SCROLL_PROJECTION_STEP: f32 = 96.0;
const MAX_ESTIMATED_CAPACITY_FPS: f32 = 10_000.0;

fn next_agent_scroll_projection(current: f32, next: f32) -> Option<f32> {
    let next = next.max(0.0);
    let delta = (next - current).abs();
    (delta > f32::EPSILON && (next <= f32::EPSILON || delta >= AGENT_SCROLL_PROJECTION_STEP))
        .then_some(next)
}

/// Estimates raw frame capacity from measured work time, independently from
/// the FPS that the window actually presents through VSync.
///
/// A missing GPU sample is intentionally ignored. The CPU sample still gives
/// the HUD a useful estimate without pretending that the GPU was measured.
fn estimated_capacity_fps(frame_cpu_ms: f32, frame_gpu_ms: Option<f32>) -> u32 {
    let cpu_ms = if frame_cpu_ms.is_finite() && frame_cpu_ms > f32::EPSILON {
        frame_cpu_ms
    } else {
        0.0
    };
    let gpu_ms = frame_gpu_ms
        .filter(|value| value.is_finite() && *value > f32::EPSILON)
        .unwrap_or(0.0);
    let bottleneck_ms = cpu_ms.max(gpu_ms);
    if bottleneck_ms <= f32::EPSILON {
        return 0;
    }
    (1000.0 / bottleneck_ms)
        .round()
        .clamp(1.0, MAX_ESTIMATED_CAPACITY_FPS) as u32
}

#[derive(Debug, Clone, PartialEq)]
pub enum NativeWorkbenchIntent {
    Window(UiWindowCommand),
    OpenSettings {
        section: SettingsSection,
    },
    ReturnToHub {
        open_create: bool,
    },
    ProjectSettingToggle {
        key: String,
        value: bool,
    },
    ProjectSettingRange {
        key: String,
        value: f32,
    },
    ProjectSettingText {
        key: String,
        value: String,
    },
    ProjectSettingCommand(String),
    AgentSettingsChanged(EngineSettings),
    OpenProjectFolder,
    Viewport(ViewportToolbarAction),
    Command(String),
    InspectorCommit {
        target: SceneNodeId,
        field: String,
        value: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum WorkbenchResizeKind {
    LeftPanel,
    RightPanel,
    BottomDock,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct WorkbenchResizeState {
    kind: WorkbenchResizeKind,
    origin_pointer: [f32; 2],
    origin_value: f32,
}

#[derive(Debug, Clone)]
struct HierarchyDragState {
    source: Vec<SceneNodeId>,
    target: Option<SceneNodeId>,
    pointer: [f32; 2],
    label: String,
}

pub struct NativeGameWorkbench {
    region: InputRegionId,
    rect: EditorRect,
    host: DirectUiSurfaceHost,
    electronics_overlay_host: DirectUiSurfaceHost,
    electronics_images_registered: bool,
    electronics_overlay_images_registered: bool,
    electronics_overlay_key: Option<(u64, u64, [u32; 2], bool)>,
    electronics_overlay_canvas: EditorRect,
    electronics_overlay_active: bool,
    search_surface: SearchSurfaceHost,
    viewport_compass: ViewportCompassHost,
    agent_panel: AgentPanel,
    agent_settings: EngineSettings,
    agent_catalog: CommandCatalog,
    agent_readiness: AgentReadiness,
    pending_agent_decision: Option<bool>,
    agent_canvas_changed: bool,
    settings_section: SettingsSection,
    project_catalog: ProjectCatalog,
    last_catalog_revision: u64,
    assets_surface: AssetsSurfaceHost,
    console: ConsolePanel,
    palette: StudioUiPalette,
    project_name: String,
    project_type: ProjectType,
    hierarchy_model: HierarchyModel,
    hierarchy_folder_ids: HashSet<SceneNodeId>,
    hierarchy_query: String,
    hierarchy_scroll_offset: f32,
    hierarchy_active_tab: String,
    electronics_navigator_tab: String,
    electronics_library_query: String,
    electronics_value_draft: Option<(Uuid, String)>,
    inspector_session_name: String,
    hierarchy_bookmarks: [bool; 3],
    hierarchy_renaming: Option<(SceneNodeId, String)>,
    hierarchy_drag: Option<HierarchyDragState>,
    inspector_name_editing: Option<(SceneNodeId, String)>,
    inspector_view: InspectorViewState,
    inspector_transform_drag_active: bool,
    toolbar_state: ViewportToolbarState,
    bottom_dock: EditorBottomDockHost,
    inspector_sessions: ProjectSessionRegistry,
    inspector_sessions_key: Option<(PathBuf, ProjectType)>,
    surface_revision: u64,
    toolbar_revision: u64,
    last_toolbar_revision: u64,
    last_console_revision: u64,
    last_electronics_revision: u64,
    last_agent_revision: u64,
    agent_scroll_projection_offset: f32,
    open_menu: Option<String>,
    open_submenu: Option<String>,
    search_restore_focus: Option<String>,
    menu_motion: UiTween,
    drag_motion: UiTween,
    agent_motion: UiTween,
    last_sync_time_seconds: f64,
    presented_fps: f32,
    estimated_capacity_fps: u32,
    viewport_fps: f32,
    frame_cpu_ms: f32,
    frame_gpu_ms: f32,
    frame_gpu_timing_sampled: bool,
    scene_draw_calls: u32,
    scene_upload_bytes: u64,
    scene_upload_budget_exceeded: bool,
    frame_activity_label: &'static str,
    redraw_reason_label: &'static str,
    canvas_status_label: &'static str,
    target_fps: u16,
    requested_target_fps: u16,
    p95_frame_time_ms: f32,
    hitch_count: u64,
    presentation_label: &'static str,
    last_status_text: String,
    last_status_refresh_seconds: f64,
    last_scene_fingerprint: u64,
    last_hierarchy_fingerprint: u64,
    last_selection: Vec<SceneNodeId>,
    last_layout: Option<EditorFrameLayout>,
    last_history_state: (bool, bool, bool),
    hierarchy_empty_menu_open: bool,
    hierarchy_primitive_menu_open: bool,
    hierarchy_menu_target: Option<(SceneNodeId, bool)>,
    hierarchy_menu_position: Option<[f32; 2]>,
    nodes_zoom: f32,
    last_node_fingerprint: u64,
    panel_resize: Option<WorkbenchResizeState>,
}

impl NativeGameWorkbench {
    pub fn new(
        graphics: &NativeGraphicsContext<'_>,
        palette: StudioUiPalette,
        rect: EditorRect,
    ) -> Self {
        let empty_surface = UiSurface::new(
            "editor.native.workbench.empty",
            palette,
            UiNode::new("editor.native.workbench.empty.root", UiNodeKind::Root)
                .with_layout(UiLayout::fill(UiFlow::None))
                .with_style(UiStyle::transparent()),
        );
        let mut host = graphics.create_ui_host(empty_surface, WORKBENCH_CLEAR);
        register_bar_images(host.images_mut());
        let overlay_surface = UiSurface::new(
            "editor.native.electronics.overlay.empty",
            palette,
            UiNode::new(
                "editor.native.electronics.overlay.empty.root",
                UiNodeKind::Root,
            )
            .with_layout(UiLayout::fill(UiFlow::None))
            .with_style(UiStyle::transparent()),
        );
        let electronics_overlay_host = graphics.create_ui_host(overlay_surface, WORKBENCH_CLEAR);
        Self {
            region: InputRegionId::from_static("native.editor.workbench"),
            rect,
            host,
            electronics_overlay_host,
            electronics_images_registered: false,
            electronics_overlay_images_registered: false,
            electronics_overlay_key: None,
            electronics_overlay_canvas: EditorRect::default(),
            electronics_overlay_active: false,
            search_surface: SearchSurfaceHost::new(graphics, rect, palette),
            viewport_compass: ViewportCompassHost::new(graphics, palette),
            agent_panel: AgentPanel::default(),
            agent_settings: EngineSettings::default(),
            agent_catalog: CommandCatalog::builtin(),
            agent_readiness: AgentReadiness::ProviderDisabled,
            pending_agent_decision: None,
            agent_canvas_changed: false,
            settings_section: SettingsSection::Appearance,
            project_catalog: ProjectCatalog::default(),
            last_catalog_revision: 0,
            assets_surface: AssetsSurfaceHost::default(),
            console: ConsolePanel::default(),
            palette,
            project_name: DEFAULT_PROJECT_NAME.to_string(),
            project_type: ProjectType::Game,
            hierarchy_model: HierarchyModel::default(),
            hierarchy_folder_ids: HashSet::new(),
            hierarchy_query: String::new(),
            hierarchy_scroll_offset: 0.0,
            hierarchy_active_tab: "hierarchy".to_string(),
            electronics_navigator_tab: "library".to_string(),
            electronics_library_query: String::new(),
            electronics_value_draft: None,
            inspector_session_name: String::new(),
            hierarchy_bookmarks: [false; 3],
            hierarchy_renaming: None,
            hierarchy_drag: None,
            inspector_name_editing: None,
            inspector_view: InspectorViewState::default(),
            inspector_transform_drag_active: false,
            toolbar_state: default_toolbar_state(),
            bottom_dock: EditorBottomDockHost::default(),
            inspector_sessions: ProjectSessionRegistry::new(ProjectType::Game),
            inspector_sessions_key: None,
            surface_revision: 0,
            toolbar_revision: 0,
            last_toolbar_revision: 0,
            last_console_revision: 0,
            last_electronics_revision: 0,
            last_agent_revision: 0,
            agent_scroll_projection_offset: 0.0,
            open_menu: None,
            open_submenu: None,
            search_restore_focus: None,
            menu_motion: UiTween::new(0.0, UiMotionSpec::dock()),
            drag_motion: UiTween::new(0.0, UiMotionSpec::dock()),
            agent_motion: UiTween::new(1.0, UiMotionSpec::dock()),
            last_sync_time_seconds: 0.0,
            presented_fps: 0.0,
            estimated_capacity_fps: 0,
            viewport_fps: 0.0,
            frame_cpu_ms: 0.0,
            frame_gpu_ms: 0.0,
            frame_gpu_timing_sampled: false,
            scene_draw_calls: 0,
            scene_upload_bytes: 0,
            scene_upload_budget_exceeded: false,
            frame_activity_label: "Idle",
            redraw_reason_label: "Idle",
            canvas_status_label: "Idle",
            target_fps: 0,
            requested_target_fps: 0,
            p95_frame_time_ms: 0.0,
            hitch_count: 0,
            presentation_label: "Present",
            last_status_text: String::new(),
            last_status_refresh_seconds: 0.0,
            last_scene_fingerprint: 0,
            last_hierarchy_fingerprint: u64::MAX,
            last_selection: Vec::new(),
            last_layout: None,
            last_history_state: (false, false, false),
            hierarchy_empty_menu_open: false,
            hierarchy_primitive_menu_open: false,
            hierarchy_menu_target: None,
            hierarchy_menu_position: None,
            nodes_zoom: 1.0,
            last_node_fingerprint: 0,
            panel_resize: None,
        }
    }

    pub fn region(&self) -> InputRegionId {
        self.region
    }

    pub fn owner(&self) -> raf_core::InputOwner {
        raf_core::InputOwner::RetainedUi(self.region)
    }

    pub fn rect(&self) -> EditorRect {
        self.rect
    }

    pub fn host(&self) -> &DirectUiSurfaceHost {
        &self.host
    }

    pub fn host_mut(&mut self) -> &mut DirectUiSurfaceHost {
        &mut self.host
    }

    pub fn cursor_hint(&self) -> raf_ui::UiCursorIcon {
        if self.viewport_compass.has_interactive_hover() {
            self.viewport_compass.cursor_hint()
        } else {
            self.host.cursor_hint()
        }
    }

    /// Exposes the compass presentation knobs without coupling future settings
    /// code to the retained host or to the viewport camera bridge.
    pub fn viewport_compass_config(&self) -> ViewportCompassConfig {
        self.viewport_compass.config()
    }

    pub fn set_viewport_compass_config(&mut self, config: ViewportCompassConfig) {
        self.viewport_compass.set_config(config);
    }

    pub fn focused_text_rect(&self) -> Option<UiRect> {
        if self.search_surface.is_open() {
            if let Some(rect) = self.search_surface.focused_text_rect() {
                let origin = self.search_surface.rect();
                return Some(UiRect::new(
                    rect.x + origin.x,
                    rect.y + origin.y,
                    rect.width,
                    rect.height,
                ));
            }
        }
        if let Some(rect) = self.host.focused_text_rect() {
            return Some(rect);
        }
        None
    }

    /// Returns the last retained workbench control that owned focus. Native
    /// modal hosts use this to return keyboard focus to the same place after
    /// a settings dialog is closed.
    pub fn focused_control_id(&self) -> Option<String> {
        self.host.session().interaction.focus.focused.clone()
    }

    /// Restores focus only when the retained workbench still contains the
    /// requested node. A project/context transition must never focus a stale
    /// node from the previous surface.
    pub fn restore_focus(&mut self, id: impl Into<String>) {
        let id = id.into();
        if self.host.surface().root.find(&id).is_some() {
            self.host.session_mut().interaction.focus.request_focus(id);
        }
    }

    pub fn open_search(&mut self) {
        self.search_restore_focus = self.host.session().interaction.focus.focused.clone();
        self.search_surface.set_query(
            self.host
                .session()
                .interaction
                .controls
                .text("application-bar.command-search.value"),
        );
        self.host.session_mut().interaction.focus.clear_focus();
        self.search_surface.open();
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
    }

    pub fn restore_search_focus(&mut self) {
        if let Some(id) = self.search_restore_focus.take() {
            self.host.session_mut().interaction.focus.request_focus(id);
        }
    }

    /// Clears transient UI input before the workbench changes context or the
    /// viewport becomes the active keyboard target. A focused Agent control
    /// must not keep the shared router owner after the user returns to the
    /// canvas.
    pub fn reset_input_state(&mut self, router: &mut raf_core::InputRouter) {
        self.search_surface.close();
        self.search_restore_focus = None;
        self.open_menu = None;
        self.open_submenu = None;
        self.agent_panel.close_menus();
        self.host
            .session_mut()
            .reset_interaction_for_surface_change(None);
        self.viewport_compass.clear_focus(router);
        router.cancel_owner(self.owner());
        router.cancel_owner(self.search_surface.owner());
    }

    /// Releases a retained control after a press lands on the passive
    /// viewport. RafUI's workbench surface covers the whole editor window, so
    /// that press cannot be represented as a normal outside click by the
    /// retained input state alone.
    pub fn clear_input_focus(&mut self, router: &mut raf_core::InputRouter) {
        self.search_surface.close();
        self.search_restore_focus = None;
        self.host.session_mut().interaction.focus.clear_focus();
        self.viewport_compass.clear_focus(router);
        router.cancel_owner(self.owner());
        router.cancel_owner(self.search_surface.owner());
    }

    /// Lets viewport navigation reclaim a stale retained-UI keyboard capture.
    /// A real text input remains authoritative: typing W/A/S/D/Q/E there must
    /// never move the scene camera. Buttons and other actionable controls do
    /// not need to keep the global navigation keys captured after activation.
    pub fn release_non_text_keyboard_capture(&mut self, router: &mut InputRouter) {
        if self.search_surface.is_open() || self.focused_text_rect().is_some() {
            return;
        }

        self.host.session_mut().interaction.focus.clear_focus();
        self.viewport_compass.clear_focus(router);
        if let Some(owner) = router.keyboard_owner() {
            if matches!(owner, raf_core::InputOwner::RetainedUi(_)) {
                router.cancel_owner(owner);
            }
        }
    }

    pub fn has_interactive_hover(&self) -> bool {
        self.host.has_interactive_hover() || self.viewport_compass.has_interactive_hover()
    }

    pub fn log_console_output(&mut self, output: crate::commands::CommandOutput) {
        self.console.log_command_output(output);
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
    }

    pub fn persist_project_layout(&mut self) {
        self.bottom_dock.persist_project_layout();
    }

    pub fn bottom_dock_height(&self) -> f32 {
        self.bottom_dock.layout.height
    }

    pub fn bottom_dock_collapsed(&self) -> bool {
        self.bottom_dock.is_collapsed()
    }

    pub fn bottom_dock_effective_height(&self) -> f32 {
        self.bottom_dock.effective_height()
    }

    pub fn set_bottom_dock_height(&mut self, height: f32) -> bool {
        let changed = self.bottom_dock.set_height(height);
        if changed {
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
        changed
    }

    pub fn engine_settings(&self) -> &EngineSettings {
        &self.agent_settings
    }

    pub(crate) fn active_session_id(&self) -> SessionId {
        self.inspector_sessions.active_session
    }

    pub fn set_engine_settings(&mut self, mut settings: EngineSettings) {
        settings.viewport_render_mode = settings.viewport_render_mode.normalized();
        let next_palette = match settings.theme {
            Theme::Light => StudioUiPalette::PaperLight,
            Theme::Dark | Theme::System => StudioUiPalette::IndustrialDark,
        };
        if self.agent_settings == settings && self.palette == next_palette {
            return;
        }
        self.agent_settings = settings;
        let palette_changed = self.palette != next_palette;
        self.palette = next_palette;
        if palette_changed {
            self.electronics_overlay_key = None;
        }
        self.toolbar_state.grid_visible = self.agent_settings.grid_visible;
        self.toolbar_state.labels_visible = self.agent_settings.show_viewport_labels;
        self.toolbar_state.render_style = ViewportRenderStyle::Solid;
        let mut environment = UiEnvironment::new(
            self.rect.width.max(1.0).round() as u32,
            self.rect.height.max(1.0).round() as u32,
        );
        environment.color_mode = match self.agent_settings.theme {
            Theme::Light => UiColorMode::Light,
            Theme::Dark => UiColorMode::Dark,
            Theme::System => UiColorMode::System,
        };
        environment.prefers_reduced_motion = self.agent_settings.prefers_reduced_motion;
        environment.high_contrast = self.agent_settings.high_contrast;
        environment.reduce_transparency = self.agent_settings.reduce_transparency
            || self.agent_settings.render_quality == RenderQuality::Potato;
        environment.font_size = self.agent_settings.font_size.clamp(10.0, 24.0);
        environment.theme_experimental = self.agent_settings.theme_experimental.clamp(0.0, 100.0);
        environment.ui_scale = if self.agent_settings.auto_ui_scale {
            1.0
        } else {
            self.agent_settings.ui_scale.clamp(0.5, 3.0)
        };
        self.host.set_environment(environment);
        self.electronics_overlay_host.set_environment(environment);
        self.search_surface.set_environment(environment);
        self.viewport_compass.set_environment(environment);
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
    }

    pub fn set_viewport_size(&mut self, logical_size: [f32; 2]) {
        let mut environment = UiEnvironment::new(
            logical_size[0].max(1.0).round() as u32,
            logical_size[1].max(1.0).round() as u32,
        );
        environment.color_mode = match self.agent_settings.theme {
            Theme::Light => UiColorMode::Light,
            Theme::Dark => UiColorMode::Dark,
            Theme::System => UiColorMode::System,
        };
        environment.prefers_reduced_motion = self.agent_settings.prefers_reduced_motion;
        environment.high_contrast = self.agent_settings.high_contrast;
        environment.reduce_transparency = self.agent_settings.reduce_transparency
            || self.agent_settings.render_quality == RenderQuality::Potato;
        environment.font_size = self.agent_settings.font_size.clamp(10.0, 24.0);
        environment.theme_experimental = self.agent_settings.theme_experimental.clamp(0.0, 100.0);
        environment.ui_scale = if self.agent_settings.auto_ui_scale {
            1.0
        } else {
            self.agent_settings.ui_scale.clamp(0.5, 3.0)
        };
        self.host.set_environment(environment);
        self.electronics_overlay_host.set_environment(environment);
        self.search_surface.set_environment(environment);
        self.viewport_compass.set_environment(environment);
    }

    pub fn set_project_info(&mut self, name: impl Into<String>, project_type: ProjectType) {
        let name = name.into();
        if project_type == ProjectType::Electronics && !self.electronics_images_registered {
            register_electronics_images(self.host.images_mut());
            self.electronics_images_registered = true;
        }
        if self.project_name == name && self.project_type == project_type {
            return;
        }
        self.project_name = name;
        self.project_type = project_type;
        self.electronics_overlay_key = None;
        self.electronics_overlay_canvas = EditorRect::default();
        self.electronics_overlay_active = false;
        self.hierarchy_query.clear();
        self.hierarchy_scroll_offset = 0.0;
        self.hierarchy_active_tab = "hierarchy".to_string();
        self.hierarchy_bookmarks = [false; 3];
        self.hierarchy_renaming = None;
        self.hierarchy_drag = None;
        self.inspector_name_editing = None;
        self.hierarchy_model.invalidate();
        self.last_hierarchy_fingerprint = 0;
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
    }

    /// Refreshes the canvas-local RafUI artwork without invalidating the full
    /// workbench chrome. The overlay is presented into the effective
    /// Electronics viewport, not the full window, so its z-order cannot cover
    /// the toolbar or side panels. The UI revision is included because
    /// selection, tools and analysis markers can change without changing CAD
    /// geometry.
    pub fn sync_electronics_overlay(
        &mut self,
        editor: Option<&NativeElectronicsEditor>,
        canvas: EditorRect,
    ) {
        let Some(editor) = editor.filter(|_| self.project_type == ProjectType::Electronics) else {
            self.electronics_overlay_active = false;
            self.electronics_overlay_key = None;
            self.electronics_overlay_canvas = EditorRect::default();
            return;
        };
        if !self.electronics_overlay_images_registered {
            register_electronics_images(self.electronics_overlay_host.images_mut());
            self.electronics_overlay_images_registered = true;
        }
        self.electronics_overlay_canvas = canvas;
        let key = (
            editor.revision(),
            editor.ui_revision(),
            canvas.logical_size(),
            editor.labels_visible(),
        );
        if self.electronics_overlay_key == Some(key) {
            self.electronics_overlay_active = true;
            return;
        }
        let (minimap_size, minimap_pixels) = electronics_minimap::build_rgba(
            editor.scene(),
            editor.camera(),
            glam::Vec2::new(canvas.width.max(1.0), canvas.height.max(1.0)),
            editor.selection(),
        );
        let _ = self.electronics_overlay_host.images_mut().insert_rgba(
            electronics_minimap::IMAGE_KEY,
            minimap_size,
            minimap_pixels,
        );
        self.electronics_overlay_host
            .set_surface(build_electronics_canvas_overlay_surface(
                self.palette,
                editor,
                canvas,
            ));
        self.electronics_overlay_key = Some(key);
        self.electronics_overlay_active = true;
    }

    pub(crate) fn sync(
        &mut self,
        layout: EditorFrameLayout,
        scene: &SceneGraph,
        selected: &[SceneNodeId],
        can_undo: bool,
        can_redo: bool,
        can_paste: bool,
        node_graph: &NodeGraph,
        selected_graph_node: Option<NodeId>,
        node_graph_revision: u64,
        project: Option<&Project>,
        now_seconds: f64,
        compass: ViewportCompassState,
        electronics: Option<&NativeElectronicsEditor>,
    ) {
        self.rect = layout.window;
        self.viewport_compass.sync(
            self.palette,
            layout.canvas,
            compass,
            self.project_type == ProjectType::Game,
        );
        self.agent_readiness =
            self.agent_panel
                .prepare(&self.agent_settings, project, &self.agent_catalog);
        if self
            .inspector_name_editing
            .as_ref()
            .is_some_and(|(id, _)| selected.first().copied() != Some(*id))
        {
            self.inspector_name_editing = None;
        }
        self.menu_motion
            .set_target(self.open_menu.as_ref().map_or(0.0, |_| 1.0));
        let delta = (now_seconds - self.last_sync_time_seconds).clamp(0.0, 0.25) as f32;
        self.last_sync_time_seconds = now_seconds;
        let hierarchy_motion_disabled =
            !self.agent_settings.hierarchy_animations || self.agent_settings.prefers_reduced_motion;
        self.menu_motion.advance(delta, hierarchy_motion_disabled);
        self.drag_motion.advance(delta, hierarchy_motion_disabled);
        self.agent_motion
            .set_target(self.agent_panel.sidebar_open.then_some(1.0).unwrap_or(0.0));
        self.agent_motion
            .advance(delta as f32, self.agent_settings.prefers_reduced_motion);
        let hierarchy_fingerprint = scene.document_revision();
        if self.last_hierarchy_fingerprint != hierarchy_fingerprint {
            self.hierarchy_folder_ids = scene
                .iter()
                .filter_map(|(id, node)| node.is_folder.then_some(id))
                .collect();
            self.hierarchy_model.invalidate();
        }
        self.project_catalog
            .sync_project(project.map(|project| project.path.as_path()));
        let session_key = project.map(|project| (project.path.clone(), project.project_type));
        if self.inspector_sessions_key != session_key {
            self.inspector_sessions = project
                .map(|project| {
                    ProjectSessionRegistry::load_or_legacy(&project.path, project.project_type)
                })
                .unwrap_or_else(|| ProjectSessionRegistry::new(self.project_type));
            self.inspector_sessions_key = session_key;
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
        if let Some(style) = project.map(|project| project.settings.building_style) {
            if self.toolbar_state.building_style != style {
                self.toolbar_state.building_style = style;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
        }
        if self.assets_surface.refresh_requested {
            self.project_catalog.refresh();
            self.assets_surface.refresh_requested = false;
        }
        let catalog_changed = self.project_catalog.poll();
        self.bottom_dock.sync_project_layout(project);
        self.sync_search_surface(layout, scene, project);
        let fingerprint = scene.render_fingerprint();
        let node_fingerprint = node_graph_revision;
        let selection_changed = self.last_selection != selected;
        if self.project_type == ProjectType::Game && selection_changed {
            if self.agent_settings.hierarchy_expand_on_select {
                if let Some(id) = selected.first().copied() {
                    self.hierarchy_model.expand_parent_chain(scene, id);
                }
            }
            if self.agent_settings.hierarchy_auto_reveal_selection {
                self.reveal_hierarchy_selection(layout.left_panel, scene, selected);
            }
        }
        let layout_changed = self.last_layout != Some(layout);
        let status_refresh_due = self.last_status_text.is_empty()
            || now_seconds - self.last_status_refresh_seconds >= 0.25;
        let status_text = status_refresh_due.then(|| self.performance_status_text());
        let status_fps_changed = status_text
            .as_ref()
            .is_some_and(|status_text| status_text != &self.last_status_text);
        let electronics_revision = electronics.map_or(0, NativeElectronicsEditor::ui_revision);
        let agent_revision = self.agent_panel.visual_revision();
        let surface_changed = selection_changed
            || layout_changed
            || self.toolbar_revision != self.last_toolbar_revision
            || self.last_scene_fingerprint != fingerprint
            || self.last_hierarchy_fingerprint != hierarchy_fingerprint
            || self.last_node_fingerprint != node_fingerprint
            || self.last_history_state != (can_undo, can_redo, can_paste)
            || self.last_console_revision != self.console.revision()
            || self.last_electronics_revision != electronics_revision
            || self.last_agent_revision != agent_revision
            || catalog_changed
            || self.last_catalog_revision != self.project_catalog.revision()
            || !self.menu_motion.is_settled()
            || !self.drag_motion.is_settled()
            || !self.agent_motion.is_settled();
        if !surface_changed {
            if status_fps_changed && self.project_type == ProjectType::Game {
                self.host
                    .patch_text_value("editor.status.item.4", status_text.clone().unwrap());
                self.last_status_text = status_text.unwrap();
                self.last_status_refresh_seconds = now_seconds;
            }
            return;
        }
        let surface = self.build_surface(
            layout,
            scene,
            selected,
            can_undo,
            can_redo,
            can_paste,
            node_graph,
            selected_graph_node,
            project,
            electronics,
        );
        self.host.set_surface(surface);
        self.host.session_mut().interaction.controls.set_text(
            "assets.search",
            &self.assets_surface.query,
            256,
        );
        self.host.session_mut().interaction.controls.set_text(
            "application-bar.command-search.value",
            self.search_surface.query(),
            256,
        );
        self.host.session_mut().interaction.controls.set_text(
            "hierarchy.search",
            &self.hierarchy_query,
            256,
        );
        self.host.session_mut().interaction.controls.set_text(
            "hierarchy.search.global",
            &self.hierarchy_query,
            256,
        );
        self.host.session_mut().interaction.controls.set_text(
            "electronics.library.search",
            &self.electronics_library_query,
            256,
        );
        let electronics_component = electronics.and_then(|editor| {
            let selection = editor.selection()?;
            editor
                .schematic()
                .components
                .iter()
                .find(|component| component.id == selection.source_id)
        });
        if let Some(component) = electronics_component {
            if self
                .electronics_value_draft
                .as_ref()
                .is_none_or(|(id, _)| *id != component.id)
            {
                self.electronics_value_draft = Some((component.id, component.value.clone()));
            }
        } else {
            self.electronics_value_draft = None;
        }
        let electronics_value = self
            .electronics_value_draft
            .as_ref()
            .map(|(_, value)| value.as_str());
        self.host.session_mut().interaction.controls.set_text(
            "electronics.inspector.value",
            electronics_value.unwrap_or_default(),
            256,
        );
        self.host.session_mut().interaction.controls.set_text(
            "inspector.session.new_name",
            &self.inspector_session_name,
            128,
        );
        self.host.session_mut().interaction.controls.set_text(
            "assets.script-name",
            &self.assets_surface.script_name,
            128,
        );
        self.host.session_mut().interaction.controls.set_text(
            "assets.file-name",
            &self.assets_surface.file_name,
            256,
        );
        self.host.session_mut().interaction.controls.set_text(
            "console.input",
            self.console.input(),
            4096,
        );
        self.host.session_mut().interaction.controls.set_text(
            "agent.input",
            &self.agent_panel.input_text,
            16_384,
        );
        self.host.session_mut().interaction.controls.set_text(
            "agent.new-model.label",
            &self.agent_panel.new_model_label,
            256,
        );
        self.host.session_mut().interaction.controls.set_text(
            "agent.new-model.id",
            &self.agent_panel.new_model_id,
            256,
        );
        if let Some(project) = project {
            self.seed_project_settings_values(project);
        }
        if self.agent_settings.inspector_live_transform_updates
            || !self.inspector_transform_drag_active
        {
            self.seed_inspector_values(scene, selected);
        }
        if let Some((id, value)) = self.hierarchy_renaming.as_ref() {
            self.host.session_mut().interaction.controls.set_text(
                &format!("hierarchy.rename.{id}", id = id.0),
                value,
                256,
            );
        }
        self.surface_revision = self.surface_revision.wrapping_add(1).max(1);
        self.last_toolbar_revision = self.toolbar_revision;
        self.last_scene_fingerprint = fingerprint;
        self.last_hierarchy_fingerprint = hierarchy_fingerprint;
        self.last_node_fingerprint = node_fingerprint;
        self.last_selection = selected.to_vec();
        self.last_layout = Some(layout);
        self.last_history_state = (can_undo, can_redo, can_paste);
        self.last_console_revision = self.console.revision();
        self.last_electronics_revision = electronics_revision;
        self.last_agent_revision = agent_revision;
        self.last_catalog_revision = self.project_catalog.revision();
        if let Some(status_text) = status_text {
            self.last_status_text = status_text;
            self.last_status_refresh_seconds = now_seconds;
        }
    }

    fn hierarchy_tree_viewport_height(&self, left: EditorRect) -> f32 {
        self.host
            .layout_rect("hierarchy.tree")
            .map(|rect| rect.height)
            .unwrap_or_else(|| (left.height - 130.0).max(1.0))
            .max(1.0)
    }

    fn reveal_hierarchy_selection(
        &mut self,
        left_panel: Option<EditorRect>,
        scene: &SceneGraph,
        selected: &[SceneNodeId],
    ) {
        let Some(left) = left_panel else {
            return;
        };
        let Some(selected_id) = selected.first().copied() else {
            return;
        };
        let row_height = self.agent_settings.hierarchy_row_height.max(1.0);
        let viewport_height = self.hierarchy_tree_viewport_height(left);
        let view = self.hierarchy_model.refresh(
            scene,
            &self.hierarchy_query,
            self.agent_settings.hierarchy_show_hidden,
            self.hierarchy_scroll_offset,
            viewport_height,
            row_height,
        );
        let Some(index) = self
            .hierarchy_model
            .row_ids()
            .position(|id| id == selected_id)
        else {
            return;
        };
        let selected_top = index as f32 * row_height;
        let selected_bottom = selected_top + row_height;
        let max_offset = (view.total_rows as f32 * row_height - viewport_height).max(0.0);
        let current = self.hierarchy_scroll_offset.clamp(0.0, max_offset);
        let next = if selected_top < current {
            selected_top
        } else if selected_bottom > current + viewport_height {
            selected_bottom - viewport_height
        } else {
            current
        }
        .clamp(0.0, max_offset);
        if (next - self.hierarchy_scroll_offset).abs() > f32::EPSILON {
            self.hierarchy_scroll_offset = next;
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
    }

    fn sync_search_surface(
        &mut self,
        layout: EditorFrameLayout,
        scene: &SceneGraph,
        project: Option<&Project>,
    ) {
        if !self.search_surface.is_open() {
            return;
        }
        let query = self.search_surface.query().trim().to_ascii_lowercase();
        let matches = |label: &str, detail: &str| {
            query.is_empty()
                || label.to_ascii_lowercase().contains(&query)
                || detail.to_ascii_lowercase().contains(&query)
        };
        let mut results = Vec::new();
        for command in &self.agent_catalog.commands {
            let label = format!("/{}", command.name);
            let detail = command.category.clone();
            if matches(&label, &detail) {
                results.push(SearchResult {
                    label,
                    detail,
                    kind: SearchResultKind::Command(format!("console.submit:/{}", command.name)),
                    icon: UiIconId::Menu,
                });
            }
        }
        if let Some(project) = project {
            let detail = project.path.display().to_string();
            if matches(&project.name, &detail) {
                results.push(SearchResult {
                    label: project.name.clone(),
                    detail,
                    kind: SearchResultKind::Project(project.name.clone()),
                    icon: UiIconId::Project,
                });
            }
        }
        for (id, node) in scene.iter() {
            if matches(&node.name, "Hierarchy") {
                results.push(SearchResult {
                    label: node.name.clone(),
                    detail: format!("Hierarchy / {}", id.0),
                    kind: SearchResultKind::Hierarchy(id),
                    icon: UiIconId::Cube,
                });
            }
        }
        let width = self.rect.width.min(620.0).max(320.0);
        let height = ((self.rect.height - 120.0).max(1.0)).min(480.0).max(180.0);
        self.search_surface
            .set_results(results, SearchSurfaceState::Ready);
        self.search_surface.sync(
            self.palette,
            EditorRect::new(
                (self.rect.width - width) * 0.5,
                layout.application_bar.height + 8.0,
                width,
                height,
            ),
        );
    }

    pub fn set_performance_metrics(
        &mut self,
        presented_fps: f32,
        viewport_fps: f32,
        frame_cpu_ms: f32,
        capacity_cpu_ms: f32,
        target_fps: u16,
        requested_target_fps: u16,
        p95_frame_time_ms: f32,
        hitch_count: u64,
        scene_metrics: SceneFrameMetrics,
        presentation_label: &'static str,
        frame_activity_label: &'static str,
        redraw_reason_label: &'static str,
        canvas_status_label: &'static str,
    ) {
        self.presented_fps = presented_fps.max(0.0);
        self.estimated_capacity_fps = estimated_capacity_fps(
            capacity_cpu_ms,
            scene_metrics
                .gpu_timing_sampled
                .then_some(scene_metrics.gpu_frame_ms),
        );
        self.viewport_fps = viewport_fps.max(0.0);
        self.frame_cpu_ms = frame_cpu_ms.max(0.0);
        self.target_fps = target_fps;
        self.requested_target_fps = requested_target_fps;
        self.p95_frame_time_ms = p95_frame_time_ms.max(0.0);
        self.hitch_count = hitch_count;
        self.frame_gpu_ms = scene_metrics.gpu_frame_ms.max(0.0);
        self.frame_gpu_timing_sampled = scene_metrics.gpu_timing_sampled;
        self.scene_draw_calls = scene_metrics.total_draw_calls();
        self.scene_upload_bytes = scene_metrics.frame_upload_bytes;
        self.scene_upload_budget_exceeded = scene_metrics.upload_budget_exceeded;
        self.presentation_label = presentation_label;
        self.frame_activity_label = frame_activity_label;
        self.redraw_reason_label = redraw_reason_label;
        self.canvas_status_label = canvas_status_label;
    }

    pub(crate) fn performance_status_text(&self) -> String {
        if !self.agent_settings.show_fps_counter {
            return "FPS: hidden".to_string();
        }
        let effective_target = if self.target_fps == 0 {
            "max".to_string()
        } else {
            self.target_fps.to_string()
        };
        let target = if self.requested_target_fps != self.target_fps {
            let requested_target = if self.requested_target_fps == 0 {
                "max".to_string()
            } else {
                self.requested_target_fps.to_string()
            };
            format!("{requested_target}>{effective_target}")
        } else {
            effective_target
        };
        let gpu = if self.frame_gpu_timing_sampled {
            format!("{:.1}ms", self.frame_gpu_ms)
        } else {
            "--".to_string()
        };
        let viewport = if self.viewport_fps > f32::EPSILON {
            format!("{:.0}", self.viewport_fps)
        } else {
            "--".to_string()
        };
        let upload_kib = self.scene_upload_bytes as f64 / 1024.0;
        let upload_warning = if self.scene_upload_budget_exceeded {
            "!"
        } else {
            ""
        };
        let capacity = if self.estimated_capacity_fps > 0 {
            format!("~{}", self.estimated_capacity_fps)
        } else {
            "--".to_string()
        };
        format!(
            "CAP:{capacity} FPS | FPS:{presented_fps:.0}/{target} | VP:{viewport} | CPU:{frame_cpu_ms:.1}ms | GPU:{gpu} | P95:{p95_frame_time_ms:.1}ms | D{scene_draw_calls} | U{upload_kib:.0}K{upload_warning} | H{hitch_count} | {frame_activity_label} | {redraw_reason_label} | {canvas_status_label} | {presentation_label}",
            capacity = capacity,
            presented_fps = self.presented_fps,
            target = target,
            viewport = viewport,
            frame_cpu_ms = self.frame_cpu_ms,
            gpu = gpu,
            p95_frame_time_ms = self.p95_frame_time_ms,
            scene_draw_calls = self.scene_draw_calls,
            upload_kib = upload_kib,
            upload_warning = upload_warning,
            hitch_count = self.hitch_count,
            frame_activity_label = self.frame_activity_label,
            redraw_reason_label = self.redraw_reason_label,
            canvas_status_label = self.canvas_status_label,
            presentation_label = self.presentation_label,
        )
    }

    pub fn set_inspector_transform_drag_active(&mut self, active: bool) {
        if self.inspector_transform_drag_active != active {
            self.inspector_transform_drag_active = active;
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
    }

    /// Applies the session actions emitted by the shared Inspector surface.
    ///
    /// The registry remains the single source of truth. The native
    /// application receives `sessions.reload` after persistence and reloads
    /// the active document without routing through a legacy UI toolkit.
    fn process_session_command(
        &mut self,
        name: &str,
        project: Option<&Project>,
    ) -> Option<NativeWorkbenchIntent> {
        let project = project?;
        let (command_name, target) = if name == "inspector.session.create" {
            ("session.create", None)
        } else if let Some(id) = name.strip_prefix("inspector.session.open:") {
            ("session.open", Some(id))
        } else if let Some(id) = name.strip_prefix("inspector.session.duplicate:") {
            ("session.duplicate", Some(id))
        } else if let Some(id) = name.strip_prefix("inspector.session.remove:") {
            ("session.remove", Some(id))
        } else {
            return None;
        };

        let mut args = BTreeMap::new();
        let positional = Vec::new();
        match command_name {
            "session.create" => {
                let base = self.inspector_session_name.trim();
                let fallback = if self.project_type == ProjectType::Electronics {
                    "Electronics session"
                } else {
                    "World session"
                };
                args.insert(
                    "name".to_string(),
                    unique_session_name(
                        &self.inspector_sessions,
                        if base.is_empty() { fallback } else { base },
                    ),
                );
                args.insert(
                    "kind".to_string(),
                    if self.project_type == ProjectType::Electronics {
                        "electronics".to_string()
                    } else {
                        "world".to_string()
                    },
                );
            }
            "session.open" => {
                args.insert("session".to_string(), target?.to_string());
            }
            "session.duplicate" => {
                let source_id = SessionId(uuid::Uuid::parse_str(target?).ok()?);
                let source = self
                    .inspector_sessions
                    .sessions
                    .iter()
                    .find(|session| session.id == source_id)?;
                args.insert("session".to_string(), target?.to_string());
                args.insert(
                    "name".to_string(),
                    unique_session_name(&self.inspector_sessions, &format!("{} Copy", source.name)),
                );
            }
            "session.remove" => {
                args.insert("session".to_string(), target?.to_string());
            }
            _ => return None,
        }

        let parsed = ParsedCommand {
            raw: name.to_string(),
            name: command_name.to_string(),
            args,
            positional,
            structured_args: None,
        };
        let mut events = Vec::new();
        let output = crate::commands::sessions::execute(
            command_name,
            &parsed,
            &mut crate::commands::sessions::SessionCommandContext {
                project: Some(project),
                registry: &mut self.inspector_sessions,
                events: &mut events,
            },
        );
        let mut changed = output.changed;
        for event in events {
            let crate::commands::sessions::SessionCommandEvent::Activate(id) = event;
            changed |= self.inspector_sessions.set_active(id);
        }
        if changed {
            if let Err(error) = self.inspector_sessions.save(&project.path) {
                tracing::warn!(%error, "session registry save failed");
                return None;
            }
            self.inspector_session_name.clear();
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return Some(NativeWorkbenchIntent::Command(
                "sessions.reload".to_string(),
            ));
        }
        None
    }

    pub fn has_active_ui_motion(&self) -> bool {
        !self.menu_motion.is_settled()
            || !self.drag_motion.is_settled()
            || !self.agent_motion.is_settled()
            || self.host.has_active_motion()
            || self.viewport_compass.has_active_motion()
    }

    pub fn has_active_text_repeat(&self) -> bool {
        self.host.has_active_text_repeat()
            || self.viewport_compass.has_active_text_repeat()
            || self.search_surface.has_active_text_repeat()
    }

    pub fn needs_ui_frame(&self) -> bool {
        self.toolbar_revision != self.last_toolbar_revision
            || self.agent_panel.has_live_output()
            || self.has_active_ui_motion()
            || self.has_active_text_repeat()
            || self.bottom_dock.drag().is_some()
            || self.bottom_dock.resize().is_some()
            || self.panel_resize.is_some()
            || !self.agent_motion.is_settled()
    }

    /// Returns the last immutable asset snapshot for the attached Agent
    /// bridge. The catalog worker owns filesystem discovery; callers only
    /// borrow published rows and never scan from the editor frame.
    pub(crate) fn agent_assets(&self) -> &[String] {
        self.project_catalog.assets()
    }

    pub(crate) fn agent_catalog_pending(&self) -> bool {
        self.project_catalog.is_pending()
    }

    pub(crate) fn agent_catalog_error(&self) -> Option<&str> {
        self.project_catalog.error()
    }

    pub(crate) fn agent_task_snapshot_by_id(&self, id: AgentTaskId) -> Option<AgentTaskSnapshot> {
        self.agent_panel.runtime.task_snapshot_by_id(id)
    }

    pub(crate) fn agent_task_snapshots(&self) -> Vec<AgentTaskSnapshot> {
        self.agent_panel.runtime.task_snapshots()
    }

    pub(crate) fn agent_task_events_since(&self, sequence: u64) -> Vec<AgentTaskEvent> {
        self.agent_panel.runtime.task_events_since(sequence)
    }

    pub(crate) fn cancel_agent_task(&mut self, id: AgentTaskId) -> bool {
        let Some(snapshot) = self.agent_panel.runtime.task_snapshot() else {
            return false;
        };
        if snapshot.id != id || snapshot.status.is_terminal() || !snapshot.cancellable {
            return false;
        }
        self.agent_panel.runtime.cancel();
        true
    }

    /// Advances the non-blocking Agent runtime on the editor thread. The
    /// executor is assembled here so Agent, CLI/MCP and the viewport all use
    /// the same scene and selection objects.
    pub fn poll_agent(
        &mut self,
        scene: &mut SceneGraph,
        viewport: &mut crate::panels::viewport_controller::NativeGameViewportController,
        graphics: &mut RenderRuntime,
        electronics: Option<&mut NativeElectronicsEditor>,
        project: Option<&Project>,
        ledger: &mut TransactionLedger,
    ) -> Vec<AgentEditorAction> {
        self.agent_readiness =
            self.agent_panel
                .prepare(&self.agent_settings, project, &self.agent_catalog);
        if !matches!(self.agent_readiness, AgentReadiness::Ready) {
            return Vec::new();
        }
        let mut selection = SceneSelectionState {
            selected_node: viewport.selected.first().copied(),
            selected_nodes: viewport.selected.clone(),
        };
        let active_session = self
            .inspector_sessions
            .active()
            .map(|session| session.name.clone())
            .unwrap_or_else(|| "Main".to_string());
        let snapshot = AgentObservationContext {
            scene,
            selection: &selection,
            project,
            assets: self.project_catalog.assets(),
            active_session: &active_session,
            revision: ledger.revision(),
            catalog_pending: self.project_catalog.is_pending(),
            catalog_error: self.project_catalog.error(),
        }
        .snapshot();
        self.agent_panel.start_pending_run(
            &snapshot,
            &self.agent_catalog,
            project.map(|project| project.project_type),
            self.agent_settings.agent_mode,
        );
        let mut editor_actions = Vec::new();
        let mut canvas_changed = false;
        let language = self.agent_panel.language;
        let mut executor = AgentToolExecutor {
            scene,
            selection: &mut selection,
            viewport,
            graphics,
            electronics,
            project: AgentProjectContext::from_project(project),
            project_info: project,
            project_assets: self.project_catalog.assets(),
            active_session: &active_session,
            catalog_pending: self.project_catalog.is_pending(),
            catalog_error: self.project_catalog.error(),
            catalog: &self.agent_catalog,
            language,
            routes: self.agent_panel.tool_routes.clone(),
            ledger,
            editor_actions: &mut editor_actions,
            canvas_changed: &mut canvas_changed,
        };
        if let Some(approve) = self.pending_agent_decision.take() {
            if approve {
                self.agent_panel.approve_with_executor(&mut executor);
            } else {
                self.agent_panel.deny_with_executor(&mut executor);
            }
        }
        self.agent_panel.poll(&mut executor);
        self.agent_canvas_changed |= canvas_changed;
        viewport.selected = selection.selected_nodes;
        if viewport.selected.is_empty() {
            if let Some(id) = selection.selected_node {
                viewport.selected.push(id);
            }
        }
        editor_actions
    }

    pub fn take_agent_canvas_changed(&mut self) -> bool {
        std::mem::take(&mut self.agent_canvas_changed)
    }

    fn hierarchy_target_is_folder(&self, id: SceneNodeId) -> bool {
        self.hierarchy_folder_ids.contains(&id)
    }

    pub fn compositor_layer(
        &mut self,
        scale_factor: f32,
        target_size: [u32; 2],
    ) -> EditorUiLayer<'_> {
        EditorUiLayer {
            host: &mut self.host,
            target_rect: self.rect.to_physical(scale_factor, target_size),
            logical_size: self.rect.logical_size(),
            raster_scale: scale_factor.max(1.0),
        }
    }

    fn dock_group_rects(&self, layout: EditorFrameLayout) -> Vec<(String, EditorRect)> {
        self.bottom_dock
            .layout()
            .resolve_columns(layout.bottom_dock.width, 4.0)
            .into_iter()
            .map(|(group_id, column)| {
                (
                    group_id,
                    EditorRect::new(
                        layout.bottom_dock.x + column.x,
                        layout.bottom_dock.y,
                        column.width,
                        layout.bottom_dock.height,
                    ),
                )
            })
            .collect()
    }

    pub fn compositor_layers(
        &mut self,
        scale_factor: f32,
        target_size: [u32; 2],
    ) -> Vec<EditorUiLayer<'_>> {
        let rect = self.rect.to_physical(scale_factor, target_size);
        let logical_size = self.rect.logical_size();
        let raster_scale = scale_factor.max(1.0);
        let main = EditorUiLayer {
            host: &mut self.host,
            target_rect: rect,
            logical_size,
            raster_scale,
        };
        let mut layers = vec![main];
        if self.viewport_compass.active() {
            layers.push(
                self.viewport_compass
                    .compositor_layer(scale_factor, target_size),
            );
        }
        if self.electronics_overlay_active {
            layers.push(EditorUiLayer {
                host: &mut self.electronics_overlay_host,
                target_rect: self
                    .electronics_overlay_canvas
                    .to_physical(scale_factor, target_size),
                logical_size: self.electronics_overlay_canvas.logical_size(),
                raster_scale,
            });
        }
        if self.search_surface.is_open() {
            layers.push(
                self.search_surface
                    .compositor_layer(scale_factor, target_size),
            );
        }
        layers
    }

    pub fn captures_keyboard_input(&self) -> bool {
        if self.search_surface.is_open() {
            true
        } else {
            self.host.captures_keyboard_input() || self.viewport_compass.captures_keyboard_input()
        }
    }

    fn seed_inspector_values(&mut self, scene: &SceneGraph, selected: &[SceneNodeId]) {
        let Some(id) = selected.first().copied() else {
            return;
        };
        let Some(node) = scene.get(id) else {
            return;
        };
        let controls = &mut self.host.session_mut().interaction.controls;
        let name = self
            .inspector_name_editing
            .as_ref()
            .filter(|(editing_id, _)| *editing_id == id)
            .map(|(_, value)| value.clone())
            .unwrap_or_else(|| node.name.clone());
        controls.set_text("inspector.name", name, 256);
        for (label, values) in [
            ("position", node.position.to_array()),
            ("rotation", node.rotation.to_array()),
            ("scale", node.scale.to_array()),
        ] {
            for (axis, value) in [("x", values[0]), ("y", values[1]), ("z", values[2])] {
                let display_value = if label == "position" {
                    self.agent_settings.display_unit.from_meters(value)
                } else {
                    value
                };
                controls.set_text(
                    format!("inspector.{label}.{axis}.text"),
                    format!("{display_value:.4}"),
                    24,
                );
            }
        }
        controls.set_text(
            "inspector.color.hex",
            format!(
                "#{:02X}{:02X}{:02X}{:02X}",
                node.color.r, node.color.g, node.color.b, node.color.a
            ),
            9,
        );
    }

    fn seed_project_settings_values(&mut self, project: &Project) {
        let focused = self.host.session().interaction.focus.focused.clone();
        let controls = &mut self.host.session_mut().interaction.controls;
        let values = [
            (
                "project-settings.building-snap-step.text",
                "project-settings.building-snap-step.value",
                format!("{:.2}", project.settings.building_snap_step),
            ),
            (
                "project-settings.depth-resolution-scale.text",
                "project-settings.depth-resolution-scale.value",
                format!("{:.2}", project.settings.depth_resolution_scale),
            ),
            (
                "project-settings.stream-region-size.text",
                "project-settings.stream-region-size.value",
                format!("{:.0}", project.settings.world_stream_region_size),
            ),
            (
                "project-settings.stream-radius.text",
                "project-settings.stream-radius.value",
                project.settings.world_stream_load_radius.to_string(),
            ),
            (
                "project-settings.stream-lod-bias.text",
                "project-settings.stream-lod-bias.value",
                project.settings.world_stream_lod_bias.to_string(),
            ),
            (
                "project-settings.default_scene_name",
                "project-settings.default-scene.control",
                project.settings.default_scene_name.clone(),
            ),
        ];
        for (key, node_id, value) in values {
            if !controls.has_text(key) || focused.as_deref() != Some(node_id) {
                controls.set_text(key, value, 256);
            }
        }
    }

    fn finish_bottom_drag(&mut self) -> bool {
        if self.bottom_dock.drag().is_none() {
            return false;
        }
        let changed = self.bottom_dock.end_drag();
        self.drag_motion.set_immediate(0.0);
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        if changed {
            self.bottom_dock.persist_project_layout();
        }
        true
    }

    fn apply_toolbar_action(&mut self, action: ViewportToolbarAction) -> bool {
        let state = &mut self.toolbar_state;
        match action {
            ViewportToolbarAction::Select => {
                state.select_mode = true;
                state.tool = ViewportTool::Select;
            }
            ViewportToolbarAction::Move => {
                state.select_mode = false;
                state.tool = ViewportTool::Move;
            }
            ViewportToolbarAction::Rotate => {
                state.select_mode = false;
                state.tool = ViewportTool::Rotate;
            }
            ViewportToolbarAction::Scale => {
                state.select_mode = false;
                state.tool = ViewportTool::Scale;
            }
            ViewportToolbarAction::Solid => {
                state.render_style = ViewportRenderStyle::Solid;
                state.shading_menu_open = false;
            }
            ViewportToolbarAction::TogglePolygons => {
                state.polygons_visible = !state.polygons_visible
            }
            ViewportToolbarAction::ToggleGrid => state.grid_visible = !state.grid_visible,
            ViewportToolbarAction::ToggleLabels => state.labels_visible = !state.labels_visible,
            ViewportToolbarAction::View2d => {
                state.view_mode = ViewportViewMode::View2d;
                state.view_menu_open = false;
            }
            ViewportToolbarAction::View3d => {
                state.view_mode = ViewportViewMode::View3d;
                state.view_menu_open = false;
            }
            ViewportToolbarAction::Focus
            | ViewportToolbarAction::ResetView
            | ViewportToolbarAction::CreatePrimitive(_) => {
                state.view_menu_open = false;
                state.shading_menu_open = false;
                state.primitive_menu_open = false;
            }
            ViewportToolbarAction::SetBuildingStyle(style) => {
                state.building_style = style;
                state.building_menu_open = false;
                state.view_menu_open = false;
                state.shading_menu_open = false;
                state.primitive_menu_open = false;
            }
        }
        let settings_changed = match action {
            ViewportToolbarAction::Solid => {
                let next = raf_core::config::ViewportRenderMode::Solid;
                let changed = self.agent_settings.viewport_render_mode != next;
                self.agent_settings.viewport_render_mode = next;
                changed
            }
            ViewportToolbarAction::ToggleGrid => {
                let next = self.toolbar_state.grid_visible;
                let changed = self.agent_settings.grid_visible != next;
                self.agent_settings.grid_visible = next;
                changed
            }
            ViewportToolbarAction::ToggleLabels => {
                let next = self.toolbar_state.labels_visible;
                let changed = self.agent_settings.show_viewport_labels != next;
                self.agent_settings.show_viewport_labels = next;
                changed
            }
            _ => false,
        };
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        settings_changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_virtual_window_does_not_rebuild_for_every_scroll_pixel() {
        assert_eq!(next_agent_scroll_projection(0.0, 1.0), None);
        assert_eq!(next_agent_scroll_projection(0.0, 95.0), None);
        assert_eq!(next_agent_scroll_projection(0.0, 96.0), Some(96.0));
        assert_eq!(next_agent_scroll_projection(96.0, 40.0), None);
        assert_eq!(next_agent_scroll_projection(96.0, 0.0), Some(0.0));
    }

    #[test]
    fn estimated_capacity_uses_the_slowest_measured_stage() {
        assert_eq!(estimated_capacity_fps(3.1, Some(0.2)), 323);
        assert_eq!(estimated_capacity_fps(1.0, Some(4.0)), 250);
        assert_eq!(estimated_capacity_fps(4.0, None), 250);
    }

    #[test]
    fn estimated_capacity_is_unknown_without_positive_samples() {
        assert_eq!(estimated_capacity_fps(0.0, None), 0);
        assert_eq!(estimated_capacity_fps(f32::NAN, Some(f32::INFINITY)), 0);
    }
}
