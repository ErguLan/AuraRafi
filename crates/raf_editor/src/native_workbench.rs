//! Native RafUI workbench state and lifecycle coordinator.
//!
//! The retained composition lives in `native_workbench_surface.rs` and input
//! routing in `native_workbench_input.rs`. ApiGraphicBasic presents the result
//! directly over the scene canvas through the native compositor. Panel
//! builders remain presentation-only and the application consumes the
//! semantic actions returned by RafUI.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use raf_core::config::EngineSettings;
use raf_core::project::{Project, ProjectType};
use raf_core::scene::{SceneGraph, SceneNodeId};
use raf_core::session::{ProjectSessionRegistry, SessionId};
use raf_core::{InputRegionId, InputRouter};
use raf_nodes::{NodeGraph, NodeId};
use raf_render::api_graphic_basic::ui_surface::{
    DirectUiSurfaceHost, NativeGraphicsContext, NativeUiInputBridge, StudioUiPalette, UiAction,
    UiDispatchedAction, UiFlow, UiLayout, UiNode, UiNodeKind, UiStyle, UiStyleSheet, UiSurface,
};
use raf_render::api_graphic_basic::EditorUiLayer;
use raf_ui::{UiMotionSpec, UiRect, UiTween, UiWindowCommand};

use crate::agent_executor::{AgentEditorAction, AgentProjectContext, AgentToolExecutor};
use crate::application_bar_host::{register_bar_images, register_electronics_images};
use crate::application_bar_surface::{
    application_menu_popup_height, build_application_bar_surface,
    build_application_menu_popup_surface, AgentBarStatus, APPLICATION_MENU_POPUP_WIDTH,
};
use crate::application_menu::{build_application_menu, ApplicationMenuState, ApplicationView};
use crate::commands::catalog::CommandCatalog;
use crate::commands::game::SceneSelectionState;
use crate::commands::parser::ParsedCommand;
use crate::console::ConsolePanel;
use crate::editor_layout::{EditorFrameLayout, EditorRect};
use crate::electronics_controller::{ElectronicsTool, NativeElectronicsEditor};
use crate::electronics_minimap;
use crate::panels::agent_surface::AgentSurfaceHost;
use crate::panels::ai_chat::{AgentAction, AgentPanel, AgentReadiness};
use crate::panels::editor_bottom_dock_host::EditorBottomDockHost;
use crate::panels::editor_bottom_dock_surface::{
    asset_rows_with_builtins, build_assets_surface, build_dock_splitter_surface,
    build_drop_preview_surface, build_status_surface, build_tab_context_menu_surface,
    build_tab_strip_surface, AssetFilter, BottomTabDragPreview,
};
use crate::panels::editor_panel_splitter_surface::{
    build_editor_splitter_surface, EditorSplitterKind,
};
use crate::panels::electronics_canvas_overlay_surface::build_electronics_canvas_overlay_surface;
use crate::panels::electronics_inspector_surface::build_electronics_inspector_surface;
use crate::panels::electronics_navigator_surface::build_electronics_navigator_surface;
use crate::panels::electronics_toolbar_surface::build_electronics_toolbar_surface;
use crate::panels::hierarchy_model::HierarchyModel;
use crate::panels::hierarchy_surface::{
    build_hierarchy_context_overlay_surface, build_hierarchy_surface,
};
use crate::panels::inspector_surface::{
    build_inspector_surface, InspectorDropdown, InspectorTab, InspectorViewState,
};
use crate::panels::nodes_surface::build_nodes_surface_with_zoom;
use crate::panels::viewport_toolbar_surface::{
    build_viewport_toolbar_surface, parse_viewport_toolbar_action, ViewportRenderStyle,
    ViewportTool, ViewportToolbarAction, ViewportToolbarState, ViewportViewMode,
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
    hierarchy_fingerprint, inspector_section_from_slug, node_graph_fingerprint,
    numeric_commit_field, set_inspector_section, unique_session_name,
};
pub(crate) use settings::{
    ai_provider_from_id, apply_settings_command, apply_settings_range, apply_settings_text,
    apply_settings_toggle,
};

const WORKBENCH_CLEAR: [u8; 4] = [0, 0, 0, 0];
const DEFAULT_PROJECT_NAME: &str = "Untitled Game";

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
    electronics_overlay_key: Option<(u64, u64, [u32; 2], bool)>,
    electronics_overlay_canvas: EditorRect,
    electronics_overlay_active: bool,
    agent_surface: AgentSurfaceHost,
    agent_panel: AgentPanel,
    agent_settings: EngineSettings,
    agent_catalog: CommandCatalog,
    agent_readiness: AgentReadiness,
    pending_agent_decision: Option<bool>,
    pending_settings_section: Option<SettingsSection>,
    settings_section: SettingsSection,
    project_catalog: ProjectCatalog,
    last_catalog_revision: u64,
    assets_query: String,
    assets_filter: AssetFilter,
    assets_script_menu_open: bool,
    assets_script_name: String,
    assets_file_menu_open: bool,
    assets_file_name: String,
    assets_refresh_requested: bool,
    console: ConsolePanel,
    palette: StudioUiPalette,
    project_name: String,
    project_type: ProjectType,
    hierarchy_model: HierarchyModel,
    hierarchy_folder_ids: HashSet<SceneNodeId>,
    hierarchy_query: String,
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
    toolbar_state: ViewportToolbarState,
    bottom_dock: EditorBottomDockHost,
    inspector_sessions: ProjectSessionRegistry,
    inspector_sessions_key: Option<(PathBuf, ProjectType)>,
    surface_revision: u64,
    toolbar_revision: u64,
    last_toolbar_revision: u64,
    last_console_revision: u64,
    last_electronics_revision: u64,
    open_menu: Option<String>,
    menu_motion: UiTween,
    drag_motion: UiTween,
    last_sync_time_seconds: f64,
    presented_fps: f32,
    last_status_fps: u32,
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
    assets_primitive_menu_open: bool,
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
        register_electronics_images(host.images_mut());
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
        let mut electronics_overlay_host = electronics_overlay_host;
        register_electronics_images(electronics_overlay_host.images_mut());
        Self {
            region: InputRegionId::from_static("native.editor.workbench"),
            rect,
            host,
            electronics_overlay_host,
            electronics_overlay_key: None,
            electronics_overlay_canvas: EditorRect::default(),
            electronics_overlay_active: false,
            agent_surface: AgentSurfaceHost::new(
                graphics,
                InputRegionId::from_static("native.editor.agent"),
                rect,
                palette,
            ),
            agent_panel: AgentPanel::default(),
            agent_settings: EngineSettings::default(),
            agent_catalog: CommandCatalog::builtin(),
            agent_readiness: AgentReadiness::ProviderDisabled,
            pending_agent_decision: None,
            pending_settings_section: None,
            settings_section: SettingsSection::Appearance,
            project_catalog: ProjectCatalog::default(),
            last_catalog_revision: 0,
            assets_query: String::new(),
            assets_filter: AssetFilter::All,
            assets_script_menu_open: false,
            assets_script_name: "new_script".to_string(),
            assets_file_menu_open: false,
            assets_file_name: "new_file".to_string(),
            assets_refresh_requested: false,
            console: ConsolePanel::default(),
            palette,
            project_name: DEFAULT_PROJECT_NAME.to_string(),
            project_type: ProjectType::Game,
            hierarchy_model: HierarchyModel::default(),
            hierarchy_folder_ids: HashSet::new(),
            hierarchy_query: String::new(),
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
            toolbar_state: default_toolbar_state(),
            bottom_dock: EditorBottomDockHost::default(),
            inspector_sessions: ProjectSessionRegistry::new(ProjectType::Game),
            inspector_sessions_key: None,
            surface_revision: 0,
            toolbar_revision: 0,
            last_toolbar_revision: 0,
            last_console_revision: 0,
            last_electronics_revision: 0,
            open_menu: None,
            menu_motion: UiTween::new(0.0, UiMotionSpec::dock()),
            drag_motion: UiTween::new(0.0, UiMotionSpec::dock()),
            last_sync_time_seconds: 0.0,
            presented_fps: 0.0,
            last_status_fps: 0,
            last_status_refresh_seconds: 0.0,
            last_scene_fingerprint: 0,
            last_hierarchy_fingerprint: 0,
            last_selection: Vec::new(),
            last_layout: None,
            last_history_state: (false, false, false),
            hierarchy_empty_menu_open: false,
            hierarchy_primitive_menu_open: false,
            hierarchy_menu_target: None,
            hierarchy_menu_position: None,
            assets_primitive_menu_open: false,
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
        self.host.cursor_hint()
    }

    pub fn has_interactive_hover(&self) -> bool {
        self.host.has_interactive_hover()
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

    pub fn take_settings_request(&mut self) -> Option<SettingsSection> {
        self.pending_settings_section.take()
    }

    pub fn engine_settings(&self) -> &EngineSettings {
        &self.agent_settings
    }

    pub fn set_engine_settings(&mut self, settings: EngineSettings) {
        if self.agent_settings == settings {
            return;
        }
        self.agent_settings = settings;
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
    }

    pub fn set_project_info(&mut self, name: impl Into<String>, project_type: ProjectType) {
        let name = name.into();
        if self.project_name == name && self.project_type == project_type {
            return;
        }
        self.project_name = name;
        self.project_type = project_type;
        self.electronics_overlay_key = None;
        self.electronics_overlay_canvas = EditorRect::default();
        self.electronics_overlay_active = false;
        self.hierarchy_query.clear();
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

    pub fn sync(
        &mut self,
        layout: EditorFrameLayout,
        scene: &SceneGraph,
        selected: &[SceneNodeId],
        can_undo: bool,
        can_redo: bool,
        can_paste: bool,
        node_graph: &NodeGraph,
        selected_graph_node: Option<NodeId>,
        project: Option<&Project>,
        now_seconds: f64,
        electronics: Option<&NativeElectronicsEditor>,
    ) {
        self.rect = layout.window;
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
        self.menu_motion.advance(delta, false);
        self.drag_motion.advance(delta, false);
        self.hierarchy_folder_ids = scene
            .iter()
            .filter_map(|(id, node)| node.is_folder.then_some(id))
            .collect();
        let hierarchy_fingerprint = hierarchy_fingerprint(scene);
        if self.last_hierarchy_fingerprint != hierarchy_fingerprint {
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
        if self.assets_refresh_requested {
            self.project_catalog.refresh();
            self.assets_refresh_requested = false;
        }
        let catalog_changed = self.project_catalog.poll();
        self.agent_readiness =
            self.agent_panel
                .prepare(&self.agent_settings, project, &self.agent_catalog);
        self.bottom_dock.sync_project_layout(project);
        if let Some(dock_content_rect) = self.dock_content_rect_for_tab(layout, "agent") {
            self.agent_surface.sync(
                dock_content_rect,
                self.palette,
                &self.agent_panel,
                &self.agent_settings,
                project,
                self.agent_readiness,
                now_seconds,
            );
        }
        let fingerprint = scene.render_fingerprint();
        let node_fingerprint = node_graph_fingerprint(node_graph, selected_graph_node);
        let selection_changed = self.last_selection != selected;
        let layout_changed = self.last_layout != Some(layout);
        let status_fps = if self.agent_settings.show_fps_counter {
            self.presented_fps
                .is_finite()
                .then_some(self.presented_fps.round().max(0.0) as u32)
                .unwrap_or(0)
        } else {
            0
        };
        let status_fps_changed = status_fps != self.last_status_fps
            && (self.last_status_fps == 0
                || now_seconds - self.last_status_refresh_seconds >= 0.25);
        let electronics_revision = electronics.map_or(0, NativeElectronicsEditor::ui_revision);
        if !selection_changed
            && !layout_changed
            && self.toolbar_revision == self.last_toolbar_revision
            && self.last_scene_fingerprint == fingerprint
            && self.last_hierarchy_fingerprint == hierarchy_fingerprint
            && self.last_node_fingerprint == node_fingerprint
            && self.last_history_state == (can_undo, can_redo, can_paste)
            && self.last_console_revision == self.console.revision()
            && self.last_electronics_revision == electronics_revision
            && !catalog_changed
            && self.last_catalog_revision == self.project_catalog.revision()
            && self.menu_motion.is_settled()
            && self.drag_motion.is_settled()
            && !status_fps_changed
        {
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
            &self.assets_query,
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
            &self.assets_script_name,
            128,
        );
        self.host.session_mut().interaction.controls.set_text(
            "assets.file-name",
            &self.assets_file_name,
            256,
        );
        self.host.session_mut().interaction.controls.set_text(
            "console.input",
            self.console.input(),
            4096,
        );
        self.seed_inspector_values(scene, selected);
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
        self.last_catalog_revision = self.project_catalog.revision();
        self.last_status_fps = status_fps;
        self.last_status_refresh_seconds = now_seconds;
    }

    pub fn set_presented_fps(&mut self, presented_fps: f32) {
        self.presented_fps = presented_fps;
    }

    /// Applies the session actions emitted by the shared Inspector surface.
    ///
    /// The registry remains the single source of truth. The native
    /// application receives `sessions.reload` after persistence and reloads
    /// the active Electronics document without routing through a legacy UI
    /// toolkit.
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
            || self.agent_surface.has_active_motion(&self.agent_panel)
            || self.host.has_active_motion()
    }

    pub fn needs_ui_frame(&self) -> bool {
        self.toolbar_revision != self.last_toolbar_revision
            || self.agent_surface.needs_surface_sync()
            || self.agent_panel.has_live_output()
            || self.has_active_ui_motion()
            || self.bottom_dock.drag().is_some()
            || self.bottom_dock.resize().is_some()
            || self.panel_resize.is_some()
    }

    /// Advances the non-blocking Agent runtime on the editor thread. The
    /// executor is assembled here so Agent, CLI/MCP and the viewport all use
    /// the same scene and selection objects.
    pub fn poll_agent(
        &mut self,
        scene: &mut SceneGraph,
        viewport: &mut crate::panels::viewport_controller::NativeGameViewportController,
        electronics: Option<&mut NativeElectronicsEditor>,
        project: Option<&Project>,
    ) -> Vec<AgentEditorAction> {
        self.agent_readiness =
            self.agent_panel
                .prepare(&self.agent_settings, project, &self.agent_catalog);
        if self.agent_panel.take_open_settings_request() {
            self.pending_settings_section = Some(SettingsSection::Ai);
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
        if !matches!(self.agent_readiness, AgentReadiness::Ready) {
            return Vec::new();
        }
        let mut selection = SceneSelectionState {
            selected_node: viewport.selected.first().copied(),
            selected_nodes: viewport.selected.clone(),
        };
        let mut editor_actions = Vec::new();
        let mut executor = AgentToolExecutor {
            scene,
            selection: &mut selection,
            viewport,
            electronics,
            project: AgentProjectContext::from_project(project),
            catalog: &self.agent_catalog,
            tool_name_map: self.agent_panel.tool_name_map.clone(),
            editor_actions: &mut editor_actions,
        };
        if let Some(approve) = self.pending_agent_decision.take() {
            if approve {
                self.agent_panel.approve_with_executor(&mut executor);
            } else {
                self.agent_panel.deny_with_executor(&mut executor);
            }
        }
        self.agent_panel.poll(&mut executor);
        viewport.selected = selection.selected_nodes;
        if viewport.selected.is_empty() {
            if let Some(id) = selection.selected_node {
                viewport.selected.push(id);
            }
        }
        editor_actions
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

    fn dock_content_rect_for_tab(
        &self,
        layout: EditorFrameLayout,
        tab_id: &str,
    ) -> Option<EditorRect> {
        let group_id = self
            .bottom_dock
            .group_containing_active_tab(tab_id)?
            .id
            .clone();
        let group_rect = self
            .dock_group_rects(layout)
            .into_iter()
            .find(|(id, _)| id == &group_id)
            .map(|(_, rect)| rect)?;
        let tab_height = group_rect.height.min(34.0).max(1.0);
        (group_rect.height > tab_height + 1.0).then_some(EditorRect::new(
            group_rect.x,
            group_rect.y + tab_height,
            group_rect.width,
            (group_rect.height - tab_height).max(1.0),
        ))
    }

    pub fn agent_compositor_layer(
        &mut self,
        scale_factor: f32,
        target_size: [u32; 2],
    ) -> Option<EditorUiLayer<'_>> {
        self.bottom_dock.has_active_tab("agent").then(|| {
            self.agent_surface
                .compositor_layer(scale_factor, target_size)
        })
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
        if self.bottom_dock.has_active_tab("agent") {
            layers.push(
                self.agent_surface
                    .compositor_layer(scale_factor, target_size),
            );
        }
        layers
    }

    pub fn captures_keyboard_input(&self) -> bool {
        if self.bottom_dock.has_active_tab("agent") {
            self.agent_surface.host().captures_keyboard_input()
                || self.host.captures_keyboard_input()
        } else {
            self.host.captures_keyboard_input()
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
                controls.set_text(
                    format!("inspector.{label}.{axis}.text"),
                    format!("{value:.4}"),
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

    fn apply_toolbar_action(&mut self, action: ViewportToolbarAction) {
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
            ViewportToolbarAction::Wireframe => {
                state.render_style = ViewportRenderStyle::Wireframe;
                state.shading_menu_open = false;
            }
            ViewportToolbarAction::Preview => {
                state.render_style = ViewportRenderStyle::Preview;
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
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
    }
}
