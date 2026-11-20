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
use raf_core::config::{EngineSettings, Language, RenderExecutionPolicy, RenderQuality, Theme};
use raf_core::project::{Project, ProjectType};
use raf_core::scene::{SceneGraph, SceneNodeId};
use raf_core::session::{ProjectSessionRegistry, SessionId};
use raf_core::{
    AgentTaskEvent, AgentTaskId, AgentTaskSnapshot, InputRegionId, InputRouter, TransactionLedger,
};
use raf_nodes::{NodeGraph, NodeId};
use raf_render::api_graphic_basic::device::BasicDeviceConfig;
use raf_render::api_graphic_basic::ui_surface::{
    DirectUiSurfaceHost, NativeGraphicsContext, NativeUiInputBridge, StudioUiPalette, UiAction,
    UiDispatchedAction, UiFlow, UiIconId, UiLayout, UiNode, UiNodeKind, UiStyle, UiStyleSheet,
    UiSurface,
};
use raf_render::api_graphic_basic::{EditorUiLayer, SceneFrameMetrics};
use raf_render::bridge::RenderRuntime;
use raf_ui::{UiColorMode, UiEnvironment, UiMotionSpec, UiRect, UiTween, UiWindowCommand};

use crate::agent_executor::AgentEditorAction;
use crate::application_bar_host::{register_bar_images, register_electronics_images};
use crate::application_bar_surface::{
    application_menu_popup_height, build_application_bar_surface,
    build_application_menu_popup_surface, build_application_menu_popup_surface_with_submenu,
    APPLICATION_MENU_POPUP_WIDTH,
};
use crate::application_menu::{build_application_menu, ApplicationMenuState, ApplicationView};
use crate::commands::catalog::CommandCatalog;
use crate::commands::parser::ParsedCommand;
use crate::console::ConsolePanel;
use crate::editor_layout::{EditorFrameLayout, EditorRect};
use crate::electronics_controller::{ElectronicsTool, NativeElectronicsEditor};
use crate::electronics_minimap;
use crate::panels::ai_chat::{AgentAction, AgentPanel, AgentReadiness, SharedAgentPanel};
use crate::panels::assets_surface::{
    asset_absolute_path, asset_rows_with_builtins, asset_visible_range,
    build_assets_backdrop_surface, build_assets_context_menu_surface,
    build_assets_create_menu_surface, build_assets_delete_modal_surface,
    build_assets_file_popover_surface, build_assets_open_modal_surface,
    build_assets_primitive_popover_surface, build_assets_rename_modal_surface,
    build_assets_script_popover_surface, build_assets_surface, format_row_name, AssetFilter,
    AssetsMotion, AssetsOperation, AssetsStatus, AssetsSurfaceParams,
    ASSETS_CONTEXT_MENU_BUILTIN_HEIGHT, ASSETS_CONTEXT_MENU_HEIGHT, ASSETS_CREATE_MENU_HEIGHT,
    ASSETS_DELETE_MODAL_HEIGHT, ASSETS_FILE_POPOVER_HEIGHT, ASSETS_MENU_WIDTH, ASSETS_MODAL_WIDTH,
    ASSETS_OPEN_MODAL_HEIGHT, ASSETS_POPOVER_WIDTH, ASSETS_PRIMITIVE_POPOVER_HEIGHT,
    ASSETS_RENAME_MODAL_HEIGHT, ASSETS_SCRIPT_POPOVER_HEIGHT, ASSET_CARD_MIN_VIEWPORT,
    ASSET_SELECTION_BAR_HEIGHT, ASSET_TOOLBAR_HEIGHT,
};
use crate::panels::assets_surface_host::AssetsSurfaceHost;
use crate::panels::editor_bottom_dock_host::EditorBottomDockHost;
use crate::panels::editor_bottom_dock_surface::{
    build_dock_splitter_surface, build_drop_preview_surface, build_tab_context_menu_surface,
    BottomTabDragPreview,
};
use crate::panels::editor_panel_splitter_surface::{
    build_editor_splitter_surface, EditorSplitterKind,
};
use crate::panels::editor_status_surface::build_status_surface;
use crate::panels::electronics_canvas_overlay_surface::ElectronicsCanvasOverlayParams;
use crate::panels::electronics_toolbar_surface::build_electronics_toolbar_surface;
use crate::panels::hierarchy_model::HierarchyModel;
use crate::panels::hierarchy_surface::{
    build_hierarchy_context_overlay_surface, build_hierarchy_surface,
};
use crate::panels::inspector_surface::{
    build_inspector_surface_with_assets, InspectorDropdown, InspectorTab, InspectorViewState,
};
use crate::panels::nodes_surface_host::NodesSurfaceHost;
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
use crate::performance_pressure::{self, PerformanceReadout};
use crate::project_catalog::ProjectCatalog;
use crate::settings_surface::SettingsSection;
use raf_core::project::BuildingStyle;

#[path = "native_workbench_electronics_layout.rs"]
pub(crate) mod electronics_layout;
#[path = "native_workbench_electronics.rs"]
pub(crate) mod electronics_model;
#[path = "native_workbench_footprints.rs"]
mod footprints;
#[path = "native_workbench_helpers.rs"]
mod helpers;
#[path = "native_workbench_hierarchy.rs"]
mod hierarchy_state;
#[path = "native_workbench_input.rs"]
mod input;
#[path = "native_workbench_interfaces.rs"]
mod interfaces;
#[path = "native_workbench_performance.rs"]
mod performance;
#[path = "native_workbench_regions.rs"]
mod regions;
pub(crate) use input::parse_agent_action;
#[path = "native_workbench_lighting.rs"]
mod lighting;
#[path = "native_workbench_numeric.rs"]
mod numeric;
#[path = "native_workbench_settings.rs"]
mod settings;
#[path = "native_workbench_surface.rs"]
mod surface;

mod layout;
mod metrics;
mod status;
mod sync;
mod toolbar;

pub(crate) use helpers::{
    default_toolbar_state, hierarchy_drop_target_from_hovered, hierarchy_drop_target_is_valid,
    inspector_section_from_slug, numeric_commit_field, set_inspector_section, unique_session_name,
};
pub(crate) use settings::{
    ai_provider_from_id, apply_settings_command, apply_settings_range, apply_settings_select,
    apply_settings_text, apply_settings_toggle, is_settings_numeric_text_key,
};
pub(crate) use sync::nodes_graph_bounds;
use sync::sync_hierarchy_scroll_state;

const WORKBENCH_CLEAR: [u8; 4] = [0, 0, 0, 0];
const DEFAULT_PROJECT_NAME: &str = "Untitled Game";
const AGENT_SCROLL_PROJECTION_STEP: f32 = 96.0;
const ASSET_OPERATION_TIMEOUT_SECONDS: f64 = 6.0;
/// Retained scroll view id of the Assets grid. The input layer routes its
/// `ScrollTo` action with the same constant.
pub(crate) const ASSETS_GRID_ID: &str = "assets.grid";
const MAX_ESTIMATED_CAPACITY_FPS: f32 = 10_000.0;

fn next_agent_scroll_projection(current: f32, next: f32) -> Option<f32> {
    let next = next.max(0.0);
    let delta = (next - current).abs();
    (delta > f32::EPSILON && (next <= f32::EPSILON || delta >= AGENT_SCROLL_PROJECTION_STEP))
        .then_some(next)
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
    before: Option<SceneNodeId>,
    hovered: Option<SceneNodeId>,
    edge: i8,
    invalid: bool,
    target: Option<SceneNodeId>,
    pointer: [f32; 2],
    label: String,
}

pub struct NativeGameWorkbench {
    interface: crate::panels::interface_surface_host::InterfaceSurfaceHost,
    ui_performance: performance::UiPerformanceView,
    hierarchy_document_cache: Option<(regions::HierarchyDocumentKey, UiSurface)>,
    region: InputRegionId,
    rect: EditorRect,
    host: DirectUiSurfaceHost,
    electronics_overlay_host: DirectUiSurfaceHost,
    electronics_images_registered: bool,
    electronics_overlay_images_registered: bool,
    electronics_overlay_key: Option<(u64, u64, [u32; 2], bool, StudioUiPalette, Language, f32)>,
    electronics_minimap_cache: electronics_minimap::RasterCache,
    electronics_overlay_canvas: EditorRect,
    electronics_overlay_active: bool,
    search_surface: SearchSurfaceHost,
    viewport_compass: ViewportCompassHost,
    agent_panel: SharedAgentPanel,
    agent_settings: EngineSettings,
    agent_catalog: CommandCatalog,
    agent_asset_tools: std::rc::Rc<std::cell::RefCell<crate::agent_asset_tools::AgentAssetTools>>,
    agent_save: std::rc::Rc<std::cell::RefCell<crate::agent_project_store::ProjectSaveQueue>>,
    agent_readiness: AgentReadiness,
    agent_canvas_changed: bool,
    settings_section: SettingsSection,
    project_numeric_edits: raf_ui::numeric::UiNumericEditBank,
    project_numeric_scope: Option<uuid::Uuid>,
    project_catalog: ProjectCatalog,
    last_catalog_revision: u64,
    assets_surface: AssetsSurfaceHost,
    assets_menu_motion: UiTween,
    assets_view_motion: UiTween,
    assets_highlight_motion: UiTween,
    assets_filter_key: Option<AssetFilter>,
    assets_project_root: Option<PathBuf>,
    assets_scroll_offset: f32,
    asset_previews: crate::asset_previews::AssetPreviews,
    authoring_notice: Option<String>,
    console: ConsolePanel,
    palette: StudioUiPalette,
    project_name: String,
    project_type: ProjectType,
    hierarchy_model: HierarchyModel,
    script_relationships: crate::script_relationships::ScriptRelationshipIndex,
    hierarchy_folder_ids: HashSet<SceneNodeId>,
    hierarchy_query: String,
    hierarchy_scroll_offset: f32,
    hierarchy_active_tab: String,
    hierarchy_state_path: Option<PathBuf>,
    hierarchy_pending_state: Option<crate::panels::hierarchy_model::details::HierarchyViewState>,
    hierarchy_state_dirty: bool,
    hierarchy_state_writer: hierarchy_state::HierarchyStateWriter,
    lighting_section: String,
    lighting_preview_neutral: bool,
    lighting_error: bool,
    electronics_navigator_tab: String,
    electronics_library_query: String,
    electronics_workspace: electronics_layout::ElectronicsWorkspaceState,
    /// Entrance sample for Electronics menus and the canvas-local overlay.
    /// 1.0 means settled closed, 0.0 means fully open.
    electronics_menu_motion: UiTween,
    /// Crossfade sample used when the Electronics dock swaps between the DRC and
    /// simulation panels. 1.0 means the new panel is fully presented.
    electronics_tab_motion: UiTween,
    /// Latest observable Electronics menu state, refreshed from the editor before
    /// `tick_electronics_motion` runs so the tween stays a pure function of time.
    electronics_menu_open: bool,
    /// Last Electronics analysis dock tab, used to restart the crossfade.
    electronics_active_tab: Option<String>,
    /// Whether an Electronics analysis task is in flight, sampled from the editor
    /// so the indeterminate progress sweep only advances while work is running.
    electronics_analysis_running: bool,
    /// 0..1 sweep of the indeterminate analysis progress track.
    electronics_analysis_phase: f32,
    /// Opacity of the canvas tool hint. Recedes while a floating menu owns the
    /// pointer so the hint never competes with the menu it would sit under.
    electronics_hint_motion: UiTween,
    electronics_value_draft: Option<electronics_model::InspectorValueDraft>,
    footprint_chooser: footprints::FootprintChooserState,
    inspector_footprint_signature: String,
    inspector_footprint_presentation:
        Option<crate::panels::electronics_inspector_surface::ElectronicsInspectorPresentation>,
    inspector_session_name: String,
    hierarchy_bookmarks: [bool; 3],
    hierarchy_renaming: Option<(SceneNodeId, String)>,
    hierarchy_drag: Option<HierarchyDragState>,
    inspector_name_editing: Option<(SceneNodeId, String)>,
    inspector_view: InspectorViewState,
    inspector_transform_drag_active: bool,
    toolbar_state: ViewportToolbarState,
    popup_press_consumed: bool,
    runtime_toolbar: raf_player::surface::RuntimeToolbarState,
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
    shadow_metrics: SceneFrameMetrics,
    /// Latest bounded projection of the graphics ledger; rebuilt only when a
    /// displayed value changes.
    performance_readout: PerformanceReadout,
    /// Cached compact memory segment. Empty until the first metrics sample.
    last_performance_segment: String,
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
    /// Canvas pan in world units. The Nodes canvas never grows with the graph,
    /// so the viewport is panned instead of scrolled.
    nodes_pan: [f32; 2],
    /// Cached `[min_x, min_y, width, height]` of the authored graph, used by
    /// the Fit command so input never walks the graph.
    nodes_graph_bounds: [f32; 4],
    nodes_host: NodesSurfaceHost,
    last_node_fingerprint: u64,
    panel_resize: Option<WorkbenchResizeState>,
}

impl NativeGameWorkbench {
    pub fn new(
        graphics: &NativeGraphicsContext<'_>,
        palette: StudioUiPalette,
        rect: EditorRect,
    ) -> Self {
        Self::new_workbench(graphics, palette, rect)
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

    /// Live link from the title-bar command field: every keystroke lands in
    /// the unified Search everywhere modal, which already spans commands,
    /// files, assets, and scene entities. Focus moves to the modal input, so
    /// the user keeps typing there instead of in the stub field.
    pub fn open_search_with_query(&mut self, query: impl Into<String>) {
        self.search_restore_focus = self.host.session().interaction.focus.focused.clone();
        self.host.session_mut().interaction.focus.clear_focus();
        self.search_surface.set_query(query);
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
        self.agent_panel.borrow_mut().close_menus();
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
        self.popup_press_consumed
            || self.host.has_interactive_hover()
            || self.viewport_compass.has_interactive_hover()
            || (self.electronics_overlay_active
                && self.electronics_overlay_host.has_interactive_hover())
    }

    pub fn engine_settings(&self) -> &EngineSettings {
        &self.agent_settings
    }

    pub(crate) fn active_session_id(&self) -> SessionId {
        self.inspector_sessions.active_session
    }
    pub(crate) fn agent_sessions(&self) -> ProjectSessionRegistry {
        self.inspector_sessions.clone()
    }
    pub(crate) fn set_agent_sessions(
        &mut self,
        project: &Project,
        registry: ProjectSessionRegistry,
    ) {
        self.inspector_sessions = registry;
        self.inspector_sessions_key = Some((project.path.clone(), project.project_type));
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
            || !self.assets_menu_motion.is_settled()
            || !self.assets_view_motion.is_settled()
            || !self.assets_highlight_motion.is_settled()
            || !self.electronics_menu_motion.is_settled()
            || !self.electronics_tab_motion.is_settled()
            || self.nodes_host.is_animating()
            || self.host.has_active_motion()
            || self.viewport_compass.has_active_motion()
    }

    pub fn has_active_text_repeat(&self) -> bool {
        self.host.has_active_text_repeat()
            || self.viewport_compass.has_active_text_repeat()
            || self.search_surface.has_active_text_repeat()
    }

    pub fn set_script_relationship_wakeup(
        &mut self,
        wakeup: Option<winit::event_loop::EventLoopProxy<()>>,
    ) {
        self.asset_previews.wakeup = wakeup.clone().map(|proxy| {
            std::sync::Arc::new(move || {
                let _ = proxy.send_event(());
            }) as std::sync::Arc<dyn Fn() + Send + Sync>
        });
        self.script_relationships.set_wakeup(wakeup);
    }
    pub fn needs_ui_frame(&self) -> bool {
        self.toolbar_revision != self.last_toolbar_revision
            || self.has_pending_footprint_import()
            || self.agent_panel.borrow_mut().has_live_output()
            || self.has_active_ui_motion()
            || self.has_active_text_repeat()
            || self.hierarchy_drag.is_some()
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
        self.agent_panel
            .borrow_mut()
            .runtime
            .task_snapshot_by_id(id)
    }

    pub(crate) fn agent_task_snapshots(&self) -> Vec<AgentTaskSnapshot> {
        self.agent_panel.borrow_mut().runtime.task_snapshots()
    }

    pub(crate) fn agent_task_events_since(&self, sequence: u64) -> Vec<AgentTaskEvent> {
        self.agent_panel
            .borrow_mut()
            .runtime
            .task_events_since(sequence)
    }

    pub(crate) fn cancel_agent_task(&mut self, id: AgentTaskId) -> bool {
        let Some(snapshot) = self.agent_panel.borrow_mut().runtime.task_snapshot() else {
            return false;
        };
        if snapshot.id != id || snapshot.status.is_terminal() || !snapshot.cancellable {
            return false;
        }
        self.agent_panel.borrow_mut().runtime.cancel();
        true
    }

    /// Advances the non-blocking Agent runtime on the editor thread. The
    /// executor is assembled here so Agent, CLI/MCP and the viewport all use
    /// the same scene and selection objects.
    pub fn poll_agent(
        &mut self,
        scene: &mut SceneGraph,
        nodes: &NodeGraph,
        session: &raf_core::session::ProjectSession,
        viewport: &mut crate::panels::viewport_controller::NativeGameViewportController,
        mut graphics: Option<&mut RenderRuntime>,
        mut electronics: Option<&mut NativeElectronicsEditor>,
        project: Option<&Project>,
        ledger: &mut TransactionLedger,
        history: &mut crate::scene_history::SceneHistory,
    ) -> Vec<AgentEditorAction> {
        let Some(project) = project else {
            return Vec::new();
        };
        let before = scene.document_revision();
        let mut panel = self.agent_panel.borrow_mut();
        self.agent_readiness =
            panel.prepare(&self.agent_settings, Some(project), &self.agent_catalog);
        let mut actions = Vec::new();
        panel.visit_runs(|panel| {
            if panel.document_id().is_some_and(|id| id != session.id) {
                return;
            }
            actions.extend(crate::agent_service::poll_conversation(
                panel,
                &mut self.agent_asset_tools.borrow_mut(),
                &mut self.agent_save.borrow_mut(),
                &self.agent_settings,
                &self.agent_catalog,
                project,
                session,
                scene,
                nodes,
                viewport,
                graphics.as_deref_mut(),
                electronics.as_deref_mut(),
                ledger,
                self.project_catalog.assets(),
                self.project_catalog.is_pending(),
                self.project_catalog.error(),
                history,
            ));
        });
        self.agent_canvas_changed |= before != scene.document_revision();
        if self.agent_asset_tools.borrow_mut().take_catalog_dirty() {
            self.project_catalog.refresh();
        }
        actions
    }

    pub(crate) fn bind_agent_workspace(
        &mut self,
        panel: SharedAgentPanel,
        assets: crate::agent_service::SharedAssetTools,
        save: crate::agent_service::SharedSave,
    ) {
        self.agent_panel = panel;
        self.agent_asset_tools = assets;
        self.agent_save = save;
        self.agent_scroll_projection_offset = 0.0;
        self.host
            .session_mut()
            .reset_interaction_for_surface_change(None);
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1);
    }

    pub(crate) fn has_agent_work(&self) -> bool {
        self.agent_panel.borrow().has_live_output() || self.agent_save.borrow().is_running()
    }
    pub(crate) fn agent_visual_revision(&self) -> u64 {
        self.agent_panel.borrow().visual_revision()
    }
    pub(crate) fn agent_save_queue(&self) -> crate::agent_service::SharedSave {
        self.agent_save.clone()
    }

    pub fn take_agent_canvas_changed(&mut self) -> bool {
        std::mem::take(&mut self.agent_canvas_changed)
    }

    fn hierarchy_target_is_folder(&self, id: SceneNodeId) -> bool {
        self.hierarchy_folder_ids.contains(&id)
    }

    pub fn captures_keyboard_input(&self) -> bool {
        if self.search_surface.is_open() {
            true
        } else {
            self.interface.workspace.active()
                || self.host.captures_keyboard_input()
                || self.viewport_compass.captures_keyboard_input()
                || (self.electronics_overlay_active
                    && self.electronics_overlay_host.captures_keyboard_input())
        }
    }
}

#[cfg(test)]
mod tests;

