//! Temporary eframe placement host for the retained RafUI editor downbar.
//!
//! Eframe owns only the rectangles and window loop here. RafUI owns the tab
//! documents, hit testing, text controls, icon requests, and visual tokens.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use eframe::{egui, egui_wgpu};
use raf_core::config::{EngineSettings, Language};
use raf_core::i18n::t;
use raf_core::project::{Project, ProjectType};
use raf_electronics::drc::DrcReport;
use raf_electronics::schematic::Schematic;
use raf_electronics::simulation::SimulationResults;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAction, UiDispatchedAction, UiIconId, UiSurface,
};
use raf_ui::{
    BottomDockLayout, DockTab, DockTabGroup, UiMotionSpec, UiTween, MAX_BOTTOM_DOCK_GROUPS,
};

use crate::console::{ConsoleEntryId, ConsolePanel, ConsoleSubmission, LogLevel};
use crate::panels::agent_surface::AgentSurfaceHost;
use crate::panels::ai_chat::{AgentAction, AgentPanel, AgentReadiness};
use crate::panels::electronics_surface::{
    ElectronicsAnalysisSurfaceAction, ElectronicsAnalysisSurfaceHost,
};
use crate::panels::nodes_surface::{NodeEditorDocument, NodesIntent, NodesSurfaceHost};

use super::editor_bottom_dock_surface::{
    build_assets_surface, build_console_surface, build_drop_preview_surface, build_status_surface,
    build_tab_context_menu_surface, build_tab_strip_surface, console_visible_range, AssetFilter,
    AssetsIntent, BottomTabDragPreview, ProjectTreeEntry,
};
use super::project_settings_surface_host::{
    ProjectSettingsSurfaceHost, ProjectSettingsSurfaceIntent,
};
use super::raf_ui_surface_bridge::RafUiSurfaceBridge;

const DOWNBAR_TAB_HEIGHT: f32 = 32.0;
const DOWNBAR_GAP: f32 = 4.0;
pub(crate) const DOWNBAR_COLLAPSED_HEIGHT: f32 = 32.0;
const DOWNBAR_MIN_HEIGHT: f32 = 112.0;
const DOWNBAR_MAX_HEIGHT: f32 = 320.0;
const TAB_CONTEXT_WIDTH: f32 = 244.0;

#[derive(Debug, Clone)]
struct BottomTabContextMenu {
    group_id: String,
    tab_id: String,
    anchor: egui::Pos2,
    opened_at_seconds: f64,
}

pub struct EditorBottomDockHost {
    pub layout: BottomDockLayout,
    tabs_surfaces: BTreeMap<String, RafUiSurfaceBridge>,
    drag_preview_surface: RafUiSurfaceBridge,
    tab_context_surface: RafUiSurfaceBridge,
    tab_context_menu: Option<BottomTabContextMenu>,
    console_surface: RafUiSurfaceBridge,
    console_surface_key: Option<(
        StudioUiPalette,
        Option<PathBuf>,
        u64,
        String,
        Option<LogLevel>,
        bool,
        bool,
        Vec<ConsoleEntryId>,
        usize,
        usize,
    )>,
    console_surface_document: Option<UiSurface>,
    console_surface_revision: u64,
    assets_surface: RafUiSurfaceBridge,
    assets_surface_key: Option<(
        StudioUiPalette,
        Option<PathBuf>,
        String,
        AssetFilter,
        u64,
        usize,
        usize,
        bool,
        String,
    )>,
    assets_surface_document: Option<UiSurface>,
    assets_surface_revision: u64,
    project_settings_surface: ProjectSettingsSurfaceHost,
    nodes_surface: NodesSurfaceHost,
    status_surface: RafUiSurfaceBridge,
    status_surface_key: Option<(StudioUiPalette, Vec<String>)>,
    status_surface_last_refresh: f64,
    status_surface_document: Option<UiSurface>,
    status_surface_revision: u64,
    tabs_surface_keys: BTreeMap<
        String,
        (
            StudioUiPalette,
            DockTabGroup,
            bool,
            Option<BottomTabDragPreview>,
        ),
    >,
    tabs_surface_documents: BTreeMap<String, UiSurface>,
    tabs_surface_revisions: BTreeMap<String, u64>,
    expanded_blocks: BTreeSet<ConsoleEntryId>,
    assets_query: String,
    assets_filter: AssetFilter,
    assets_script_menu_open: bool,
    assets_script_name: String,
    dragging_tab: Option<(String, String)>,
    drag_preview: Option<BottomTabDragPreview>,
    drag_slot_motion: UiTween,
    last_drag_target: Option<(String, usize, Option<bool>)>,
    last_drag_time_seconds: f64,
    group_rects: Vec<(String, egui::Rect)>,
    display_group_rects: Vec<(String, egui::Rect)>,
    layout_transition_from: Vec<(String, egui::Rect)>,
    layout_motion: UiTween,
    last_layout_time_seconds: f64,
    active_splitter: Option<usize>,
    last_splitter_delta: f32,
    project_layout_path: Option<PathBuf>,
    layout_dirty: bool,
}

pub(crate) struct BottomDockOutput {
    pub(crate) submissions: Vec<ConsoleSubmission>,
    pub(crate) agent_actions: Vec<AgentAction>,
    pub(crate) electronics_analysis_actions: Vec<ElectronicsAnalysisSurfaceAction>,
    pub(crate) nodes_actions: Vec<NodesIntent>,
    pub(crate) assets_actions: Vec<AssetsIntent>,
}

impl Default for EditorBottomDockHost {
    fn default() -> Self {
        Self {
            layout: layout_for_project(ProjectType::Game),
            tabs_surfaces: BTreeMap::new(),
            drag_preview_surface: RafUiSurfaceBridge::new("raf_ui_editor_bottom_drag_preview"),
            tab_context_surface: RafUiSurfaceBridge::new("raf_ui_editor_bottom_tab_context"),
            tab_context_menu: None,
            console_surface: RafUiSurfaceBridge::new("raf_ui_editor_bottom_console"),
            console_surface_key: None,
            console_surface_document: None,
            console_surface_revision: 0,
            assets_surface: RafUiSurfaceBridge::new("raf_ui_editor_bottom_assets"),
            assets_surface_key: None,
            assets_surface_document: None,
            assets_surface_revision: 0,
            project_settings_surface: ProjectSettingsSurfaceHost::default(),
            nodes_surface: NodesSurfaceHost::default(),
            status_surface: RafUiSurfaceBridge::new("raf_ui_editor_status"),
            status_surface_key: None,
            status_surface_last_refresh: 0.0,
            status_surface_document: None,
            status_surface_revision: 0,
            tabs_surface_keys: BTreeMap::new(),
            tabs_surface_documents: BTreeMap::new(),
            tabs_surface_revisions: BTreeMap::new(),
            expanded_blocks: BTreeSet::new(),
            assets_query: String::new(),
            assets_filter: AssetFilter::All,
            assets_script_menu_open: false,
            assets_script_name: "new_script".to_string(),
            dragging_tab: None,
            drag_preview: None,
            drag_slot_motion: UiTween::new(1.0, UiMotionSpec::dock()),
            last_drag_target: None,
            last_drag_time_seconds: 0.0,
            group_rects: Vec::new(),
            display_group_rects: Vec::new(),
            layout_transition_from: Vec::new(),
            layout_motion: UiTween::new(1.0, UiMotionSpec::layout()),
            last_layout_time_seconds: 0.0,
            active_splitter: None,
            last_splitter_delta: 0.0,
            project_layout_path: None,
            layout_dirty: false,
        }
    }
}

impl EditorBottomDockHost {
    pub fn active_tab_is(&self, tab_id: &str) -> bool {
        self.layout
            .groups
            .iter()
            .any(|group| group.active_tab == tab_id)
    }

    /// Uses the correct project-specific default while a persisted layout is
    /// being loaded asynchronously.
    pub(crate) fn prepare_for_project(&mut self, project_type: ProjectType) {
        self.reset_layout_for(project_type);
        self.layout_dirty = false;
    }

    const LAYOUT_DIRECTORY: &'static str = ".aura_rafi";
    const LAYOUT_FILE: &'static str = "editor_downbar.ron";

    /// Loads the downbar layout belonging to the active project. The editor
    /// intentionally keeps this outside project.ron so dock geometry can
    /// evolve without coupling core project metadata to RafUI types.
    pub fn sync_project_layout(&mut self, project_path: Option<&Path>) {
        let next_path = project_path
            .filter(|path| path.is_dir())
            .map(|path| path.join(Self::LAYOUT_DIRECTORY).join(Self::LAYOUT_FILE));
        if self.project_layout_path == next_path {
            return;
        }

        self.project_layout_path = next_path;
        let mut repaired = false;
        let loaded_layout = self
            .project_layout_path
            .as_deref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|data| ron::from_str::<BottomDockLayout>(&data).ok());
        let loaded_from_disk = loaded_layout.is_some();
        let loaded_layout = loaded_layout.and_then(|mut layout| {
            if layout.version < raf_ui::BOTTOM_DOCK_LAYOUT_VERSION {
                repaired = true;
                return None;
            }
            repaired = sanitize_layout(&mut layout, ProjectType::Game);
            Some(layout)
        });
        self.layout = loaded_layout.unwrap_or_else(|| layout_for_project(ProjectType::Game));
        self.layout_dirty = loaded_from_disk && repaired;
    }

    pub fn persist_project_layout(&mut self) {
        if !self.layout_dirty {
            return;
        }
        let Some(path) = self.project_layout_path.as_deref() else {
            self.layout_dirty = false;
            return;
        };
        let Some(parent) = path.parent() else {
            return;
        };
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
        let pretty = ron::ser::PrettyConfig::default();
        if let Ok(data) = ron::ser::to_string_pretty(&self.layout, pretty) {
            if std::fs::write(path, data).is_ok() {
                self.layout_dirty = false;
            }
        }
    }

    /// Restores the clean one-group arrangement and persists it for the active
    /// project. Project settings uses this as the user-facing recovery path for
    /// stale or confusing panel arrangements.
    pub fn reset_layout(&mut self) {
        self.reset_layout_for(ProjectType::Game);
    }

    fn reset_layout_for(&mut self, project_type: ProjectType) {
        self.layout = layout_for_project(project_type);
        self.dragging_tab = None;
        self.drag_preview = None;
        self.tab_context_menu = None;
        self.last_drag_target = None;
        self.drag_slot_motion.set_immediate(1.0);
        for surface in self.tabs_surfaces.values_mut() {
            surface.cancel_pointer_gesture();
        }
        self.tabs_surfaces.clear();
        self.tabs_surface_keys.clear();
        self.tabs_surface_documents.clear();
        self.tabs_surface_revisions.clear();
        self.status_surface_key = None;
        self.status_surface_document = None;
        self.status_surface_revision = self.status_surface_revision.wrapping_add(1).max(1);
        self.console_surface_key = None;
        self.console_surface_document = None;
        self.console_surface_revision = self.console_surface_revision.wrapping_add(1).max(1);
        self.assets_surface_key = None;
        self.assets_surface_document = None;
        self.assets_surface_revision = self.assets_surface_revision.wrapping_add(1).max(1);
        self.group_rects.clear();
        self.display_group_rects.clear();
        self.layout_transition_from.clear();
        self.layout_motion.set_immediate(1.0);
        self.active_splitter = None;
        self.last_splitter_delta = 0.0;
        self.layout_dirty = true;
    }

    pub fn take_layout_dirty(&mut self) -> bool {
        std::mem::replace(&mut self.layout_dirty, false)
    }

    pub fn select_tab(&mut self, tab_id: &str) -> bool {
        let Some(group_id) = self
            .layout
            .groups
            .iter()
            .find(|group| group.tabs.iter().any(|tab| tab.id == tab_id))
            .map(|group| group.id.clone())
        else {
            return false;
        };
        let was_collapsed = self.layout.collapsed;
        if was_collapsed {
            sanitize_dock_height(&mut self.layout);
            self.layout.collapsed = false;
            self.layout.height = self
                .layout
                .expanded_height
                .clamp(DOWNBAR_MIN_HEIGHT, DOWNBAR_MAX_HEIGHT);
        }
        let changed = self.layout.select_tab(&group_id, tab_id);
        self.layout_dirty |= changed || was_collapsed;
        changed || was_collapsed
    }

    pub(crate) fn open_tab(&mut self, tab_id: &str) -> bool {
        self.select_tab(tab_id)
    }

    pub(crate) fn sync_nodes_selection(&mut self, selected: Option<raf_nodes::NodeId>) {
        self.nodes_surface.apply_selection(selected);
    }

    pub(crate) fn reset_nodes(&mut self) {
        self.nodes_surface.reset();
    }

    pub(crate) fn mark_nodes_changed(&mut self) {
        self.nodes_surface.mark_changed();
    }

    fn normalize_runtime_layout(&mut self, project_type: ProjectType) {
        let before = self.layout.clone();
        sanitize_layout(&mut self.layout, project_type);
        sanitize_dock_height(&mut self.layout);
        if self.layout != before {
            self.layout_dirty = true;
        }
    }

    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        language: Language,
        console: &mut ConsolePanel,
        input_enabled: bool,
        command_names: &[String],
        asset_rows: &[String],
        catalog_revision: u64,
        _project_name: &str,
        _session_name: &str,
        _project_entries: &[ProjectTreeEntry],
        project: Option<&mut Project>,
        global_console_commands_enabled: bool,
        agent: &mut AgentPanel,
        agent_surface: &mut AgentSurfaceHost,
        agent_readiness: AgentReadiness,
        settings: &EngineSettings,
        electronics_analysis_surface: &mut ElectronicsAnalysisSurfaceHost,
        electronics_drc_report: Option<&DrcReport>,
        electronics_simulation_results: Option<&SimulationResults>,
        schematic: &Schematic,
        electronics_project: bool,
        nodes_document: &NodeEditorDocument,
    ) -> BottomDockOutput {
        let project_type = if electronics_project {
            ProjectType::Electronics
        } else {
            ProjectType::Game
        };
        self.normalize_runtime_layout(project_type);
        let rect = ui.available_rect_before_wrap();
        let _ = ui.allocate_rect(rect, egui::Sense::hover());
        self.advance_drag_motion(ui.ctx());
        let columns = self.layout.resolve_columns(rect.width(), DOWNBAR_GAP);
        let target_group_rects: Vec<(String, egui::Rect)> = columns
            .iter()
            .map(|(group_id, local_rect)| {
                (
                    group_id.clone(),
                    bounded_group_rect(rect, local_rect.x, local_rect.width),
                )
            })
            .collect();
        let previous_group_ids = self
            .group_rects
            .iter()
            .map(|(group_id, _)| group_id)
            .collect::<Vec<_>>();
        let target_group_ids = target_group_rects
            .iter()
            .map(|(group_id, _)| group_id)
            .collect::<Vec<_>>();
        if self.group_rects.is_empty() {
            self.layout_motion.set_immediate(1.0);
            self.display_group_rects = target_group_rects.clone();
        } else if previous_group_ids != target_group_ids {
            self.layout_transition_from = self.display_group_rects.clone();
            self.layout_motion.set_immediate(0.0);
            self.layout_motion.set_target(1.0);
        }
        self.group_rects = target_group_rects;
        let visible_group_ids = self
            .group_rects
            .iter()
            .map(|(group_id, _)| group_id.clone())
            .collect::<BTreeSet<_>>();
        self.tabs_surfaces.retain(|group_id, surface| {
            if visible_group_ids.contains(group_id) {
                true
            } else {
                surface.cancel_pointer_gesture();
                false
            }
        });
        self.tabs_surface_keys
            .retain(|group_id, _| visible_group_ids.contains(group_id));
        self.tabs_surface_documents
            .retain(|group_id, _| visible_group_ids.contains(group_id));
        self.tabs_surface_revisions
            .retain(|group_id, _| visible_group_ids.contains(group_id));
        let now_seconds = ui.ctx().input(|input| input.time);
        let delta_seconds = (now_seconds - self.last_layout_time_seconds)
            .max(0.0)
            .min(0.25) as f32;
        self.last_layout_time_seconds = now_seconds;
        let layout_progress = self.layout_motion.advance(delta_seconds, false);
        if !self.layout_transition_from.is_empty() && !self.layout_motion.is_settled() {
            self.display_group_rects = bounded_group_rects(
                interpolate_group_rects(
                    &self.layout_transition_from,
                    &self.group_rects,
                    layout_progress,
                ),
                rect,
            );
            ui.ctx().request_repaint();
        } else {
            self.display_group_rects = self.group_rects.clone();
            self.layout_transition_from.clear();
        }

        if !self.layout.collapsed {
            for index in 0..self.group_rects.len().saturating_sub(1) {
                let boundary_x = self.group_rects[index].1.right() - DOWNBAR_GAP * 0.5;
                let splitter_rect = egui::Rect::from_min_max(
                    egui::pos2((boundary_x - 4.0).max(rect.left()), rect.top()),
                    egui::pos2((boundary_x + 4.0).min(rect.right()), rect.bottom()),
                );
                let response = ui
                    .interact(
                        splitter_rect,
                        egui::Id::new(("rafui.editor.downbar.splitter", index)),
                        egui::Sense::drag(),
                    )
                    .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);

                if response.drag_started_by(egui::PointerButton::Primary) {
                    self.active_splitter = Some(index);
                    self.last_splitter_delta = 0.0;
                }
                if response.dragged_by(egui::PointerButton::Primary)
                    && self.active_splitter == Some(index)
                {
                    let total_delta = response.drag_delta().x;
                    let incremental_delta = total_delta - self.last_splitter_delta;
                    self.last_splitter_delta = total_delta;
                    let left_group_id = self.group_rects[index].0.clone();
                    let right_group_id = self.group_rects[index + 1].0.clone();
                    if self.layout.resize_boundary(
                        &left_group_id,
                        &right_group_id,
                        rect.width(),
                        DOWNBAR_GAP,
                        incremental_delta,
                    ) {
                        self.layout_dirty = true;
                    }
                }
                if response.drag_stopped_by(egui::PointerButton::Primary)
                    && self.active_splitter == Some(index)
                {
                    self.active_splitter = None;
                    self.last_splitter_delta = 0.0;
                }
            }
        }
        let mut submissions = Vec::new();
        let mut agent_actions = Vec::new();
        let mut electronics_analysis_actions = Vec::new();
        let mut nodes_actions = Vec::new();
        let mut assets_actions = Vec::new();
        let mut project = project;
        let mut reset_layout_requested = false;

        for (group_id, group_rect) in self.display_group_rects.clone() {
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(group_rect), |group_ui| {
                let group_bounds = group_ui.max_rect();
                let tab_height = DOWNBAR_TAB_HEIGHT.min(group_bounds.height().max(0.0));
                let tab_rect = egui::Rect::from_min_size(
                    group_bounds.min,
                    egui::vec2(group_bounds.width(), tab_height),
                );
                group_ui.allocate_new_ui(egui::UiBuilder::new().max_rect(tab_rect), |tab_ui| {
                    let actions =
                        self.show_tabs(tab_ui, render_state, palette, language, &group_id);
                    let pointer = tab_ui.ctx().input(|input| input.pointer.interact_pos());
                    let time_seconds = tab_ui.ctx().input(|input| input.time);
                    self.apply_dock_actions(actions, pointer, time_seconds);
                    if self.drag_preview.is_some() {
                        tab_ui.ctx().request_repaint();
                    }
                });

                if self.layout.collapsed {
                    return;
                }
                let body_rect = egui::Rect::from_min_max(
                    egui::pos2(group_bounds.left(), group_bounds.top() + tab_height),
                    group_bounds.max,
                );
                group_ui.allocate_new_ui(egui::UiBuilder::new().max_rect(body_rect), |body_ui| {
                    let actions = match self
                        .layout
                        .groups
                        .iter()
                        .find(|group| group.id == group_id)
                        .and_then(|group| group.active_tab())
                        .map(|tab| tab.id.as_str())
                    {
                        Some("console") => {
                            let input = console.input().to_string();
                            let auto_scroll = console.auto_scroll;
                            let expanded_blocks =
                                self.expanded_blocks.iter().copied().collect::<Vec<_>>();
                            let scroll_offset = self
                                .console_surface
                                .with_control_state_read(|controls| {
                                    controls.scroll_offset("console.log-scroll")[1]
                                })
                                .unwrap_or(0.0);
                            let visible_range = console_visible_range(
                                console,
                                scroll_offset,
                                body_ui.available_height().max(1.0),
                            );
                            let key = (
                                palette,
                                self.project_layout_path.clone(),
                                console.revision(),
                                input.clone(),
                                console.filter_level,
                                input_enabled,
                                auto_scroll,
                                expanded_blocks.clone(),
                                visible_range.0,
                                visible_range.1,
                            );
                            if self.console_surface_key.as_ref() != Some(&key) {
                                self.console_surface_document = Some(build_console_surface(
                                    palette,
                                    console,
                                    input_enabled,
                                    &expanded_blocks,
                                    Some(visible_range),
                                ));
                                self.console_surface_key = Some(key);
                                self.console_surface_revision =
                                    self.console_surface_revision.wrapping_add(1).max(1);
                            }
                            let surface = self
                                .console_surface_document
                                .as_ref()
                                .expect("console surface document must exist after cache fill");
                            self.console_surface.show_with_control_state_ref_revision(
                                body_ui,
                                render_state,
                                palette,
                                surface,
                                self.console_surface_revision,
                                |controls| {
                                    controls.set_text("console.input", input.clone(), 4096);
                                    if auto_scroll {
                                        controls.scroll_by("console.log-scroll", [0.0, 16_384.0]);
                                    }
                                },
                                |key| t(key, language),
                            )
                        }
                        Some("assets") => {
                            let scroll_offset = self
                                .assets_surface
                                .with_control_state_read(|controls| {
                                    controls.scroll_offset("assets.list")[1]
                                })
                                .unwrap_or(0.0);
                            let visible_range =
                                super::editor_bottom_dock_surface::asset_visible_range(
                                    asset_rows,
                                    &self.assets_query,
                                    self.assets_filter,
                                    scroll_offset,
                                    body_ui.available_height().max(1.0),
                                );
                            let key = (
                                palette,
                                self.project_layout_path.clone(),
                                self.assets_query.clone(),
                                self.assets_filter,
                                catalog_revision,
                                visible_range.0,
                                visible_range.1,
                                self.assets_script_menu_open,
                                self.assets_script_name.clone(),
                            );
                            if self.assets_surface_key.as_ref() != Some(&key) {
                                self.assets_surface_document = Some(build_assets_surface(
                                    palette,
                                    asset_rows,
                                    &self.assets_query,
                                    self.assets_filter,
                                    Some(visible_range),
                                    self.assets_script_menu_open,
                                    &self.assets_script_name,
                                ));
                                self.assets_surface_key = Some(key);
                                self.assets_surface_revision =
                                    self.assets_surface_revision.wrapping_add(1).max(1);
                            }
                            let surface = self
                                .assets_surface_document
                                .as_ref()
                                .expect("assets surface document must exist after cache fill");
                            let assets_query = self.assets_query.clone();
                            let assets_script_name = self.assets_script_name.clone();
                            let actions = self.assets_surface.show_with_control_state_ref_revision(
                                body_ui,
                                render_state,
                                palette,
                                surface,
                                self.assets_surface_revision,
                                |controls| {
                                    controls.set_text("assets.search", assets_query.clone(), 256);
                                    controls.set_text(
                                        "assets.script-name",
                                        assets_script_name.clone(),
                                        96,
                                    );
                                },
                                |key| t(key, language),
                            );
                            assets_actions.extend(self.apply_assets_actions(actions));
                            Vec::new()
                        }
                        Some("project-settings") => {
                            if let Some(project) = project.as_deref_mut() {
                                let intents = self.project_settings_surface.show(
                                    body_ui,
                                    render_state,
                                    palette,
                                    language,
                                    project,
                                    global_console_commands_enabled,
                                );
                                if intents.iter().any(|intent| {
                                    matches!(intent, ProjectSettingsSurfaceIntent::Changed)
                                }) {
                                    let _ = project.save();
                                }
                                reset_layout_requested |= intents.iter().any(|intent| {
                                    matches!(intent, ProjectSettingsSurfaceIntent::ResetPanels)
                                });
                            }
                            Vec::new()
                        }
                        Some("nodes") => {
                            nodes_actions.extend(self.nodes_surface.show(
                                body_ui,
                                render_state,
                                palette,
                                language,
                                nodes_document,
                            ));
                            Vec::new()
                        }
                        Some("agent") => {
                            let actions = agent_surface.show(
                                body_ui,
                                render_state,
                                palette,
                                agent,
                                settings,
                                project.as_deref(),
                                agent_readiness,
                            );
                            agent_actions.extend(actions);
                            Vec::new()
                        }
                        Some("drc") => {
                            if electronics_project {
                                if let Some(action) = electronics_analysis_surface.show_drc(
                                    body_ui,
                                    render_state,
                                    palette,
                                    electronics_drc_report,
                                    language,
                                ) {
                                    electronics_analysis_actions.push(action);
                                }
                            } else {
                                electronics_analysis_surface.show_unavailable(
                                    body_ui,
                                    render_state,
                                    palette,
                                    language,
                                );
                            }
                            Vec::new()
                        }
                        Some("simulation") => {
                            if electronics_project {
                                if let Some(action) = electronics_analysis_surface.show_simulation(
                                    body_ui,
                                    render_state,
                                    palette,
                                    schematic,
                                    electronics_simulation_results,
                                    language,
                                ) {
                                    electronics_analysis_actions.push(action);
                                }
                            } else {
                                electronics_analysis_surface.show_unavailable(
                                    body_ui,
                                    render_state,
                                    palette,
                                    language,
                                );
                            }
                            Vec::new()
                        }
                        _ => Vec::new(),
                    };
                    submissions.extend(self.apply_console_actions(actions, console, command_names));
                });
                let split_preview = self
                    .drag_preview
                    .as_ref()
                    .filter(|preview| {
                        preview.target_group_id == group_id && preview.split_before.is_some()
                    })
                    .map(|preview| (preview.pulse, preview.split_before.unwrap_or(false)));
                if let Some((pulse, split_before)) = split_preview {
                    group_ui.allocate_new_ui(
                        egui::UiBuilder::new().max_rect(body_rect),
                        |preview_ui| {
                            let surface = build_drop_preview_surface(palette, pulse, split_before);
                            let _ = self.drag_preview_surface.show_transparent(
                                preview_ui,
                                render_state,
                                palette,
                                surface,
                                |key| t(key, language),
                            );
                        },
                    );
                }
            });
        }

        self.show_tab_context_menu(ui, render_state, palette, language, project_type);

        if self.dragging_tab.is_some() {
            let pointer = ui.ctx().input(|input| input.pointer.interact_pos());
            let time_seconds = ui.ctx().input(|input| input.time);
            self.update_drag_preview(pointer, time_seconds);
            ui.ctx().request_repaint();
        }

        if reset_layout_requested {
            self.reset_layout_for(project_type);
        }

        BottomDockOutput {
            submissions,
            agent_actions,
            electronics_analysis_actions,
            nodes_actions,
            assets_actions,
        }
    }

    pub fn show_status(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        language: Language,
        status: &[String],
    ) {
        let now = ui.ctx().input(|input| input.time);
        let key = (
            palette,
            if status
                .last()
                .is_some_and(|item| item.trim_end().ends_with("FPS"))
            {
                status[..status.len().saturating_sub(1)].to_vec()
            } else {
                status.to_vec()
            },
        );
        let refresh_fps = now - self.status_surface_last_refresh >= 0.25;
        if self.status_surface_key.as_ref() != Some(&key)
            || self.status_surface_document.is_none()
            || refresh_fps
        {
            self.status_surface_document = Some(build_status_surface(palette, status));
            self.status_surface_key = Some(key);
            self.status_surface_last_refresh = now;
            self.status_surface_revision = self.status_surface_revision.wrapping_add(1).max(1);
        }
        let surface = self
            .status_surface_document
            .as_ref()
            .expect("status surface document must exist after cache fill");
        let _ = self.status_surface.show_with_control_state_ref_revision(
            ui,
            render_state,
            palette,
            surface,
            self.status_surface_revision,
            |_| {},
            |key| t(key, language),
        );
    }

    pub fn set_height(&mut self, height: f32) {
        if self.layout.collapsed {
            if !self.layout.height.is_finite()
                || (self.layout.height - DOWNBAR_COLLAPSED_HEIGHT).abs() > f32::EPSILON
            {
                self.layout.height = DOWNBAR_COLLAPSED_HEIGHT;
                self.layout_dirty = true;
            }
            return;
        }
        if self
            .layout
            .set_height(height, DOWNBAR_MIN_HEIGHT, DOWNBAR_MAX_HEIGHT)
        {
            self.layout_dirty = true;
        }
    }

    pub fn toggle_collapsed(&mut self) {
        sanitize_dock_height(&mut self.layout);
        self.layout
            .toggle_collapsed(DOWNBAR_MIN_HEIGHT, DOWNBAR_MAX_HEIGHT);
        if self.layout.collapsed {
            self.layout.height = DOWNBAR_COLLAPSED_HEIGHT;
        } else {
            self.layout.height = self
                .layout
                .expanded_height
                .clamp(DOWNBAR_MIN_HEIGHT, DOWNBAR_MAX_HEIGHT);
        }
        self.layout_dirty = true;
    }

    fn show_tabs(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        language: Language,
        group_id: &str,
    ) -> Vec<UiDispatchedAction> {
        let Some(group) = self
            .layout
            .groups
            .iter()
            .find(|group| group.id == group_id)
            .cloned()
        else {
            return Vec::new();
        };
        let key = (
            palette,
            group.clone(),
            self.layout.collapsed,
            self.drag_preview.as_ref().cloned(),
        );
        if self.tabs_surface_keys.get(group_id) != Some(&key) {
            let surface = build_tab_strip_surface(
                palette,
                &group,
                self.layout.collapsed,
                self.drag_preview.as_ref(),
            );
            self.tabs_surface_documents
                .insert(group_id.to_string(), surface);
            self.tabs_surface_keys.insert(group_id.to_string(), key);
            let revision = self
                .tabs_surface_revisions
                .entry(group_id.to_string())
                .or_insert(0);
            *revision = revision.wrapping_add(1).max(1);
        }
        let surface = self
            .tabs_surface_documents
            .get(group_id)
            .expect("tab strip surface document must exist after cache fill");
        let tabs_surface = self
            .tabs_surfaces
            .entry(group_id.to_string())
            .or_insert_with(|| {
                RafUiSurfaceBridge::new(format!("raf_ui_editor_bottom_tabs.{group_id}"))
            });
        tabs_surface.show_with_control_state_ref_revision(
            ui,
            render_state,
            palette,
            surface,
            *self.tabs_surface_revisions.get(group_id).unwrap_or(&1),
            |_| {},
            |key| t(key, language),
        )
    }

    fn show_tab_context_menu(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        language: Language,
        project_type: ProjectType,
    ) {
        let Some(menu) = self.tab_context_menu.clone() else {
            return;
        };
        let can_split = self.layout.groups.len() < MAX_BOTTOM_DOCK_GROUPS;
        let popup_height = if can_split { 102.0 } else { 128.0 };
        let popup_size = egui::vec2(TAB_CONTEXT_WIDTH, popup_height);
        let screen = ui.ctx().screen_rect();
        let popup_pos = egui::pos2(
            menu.anchor.x.clamp(
                screen.left() + 8.0,
                (screen.right() - popup_size.x - 8.0).max(screen.left()),
            ),
            menu.anchor.y.clamp(
                screen.top() + 8.0,
                (screen.bottom() - popup_size.y - 8.0).max(screen.top()),
            ),
        );
        let popup_rect = egui::Rect::from_min_size(popup_pos, popup_size);
        let surface =
            build_tab_context_menu_surface(palette, &menu.group_id, &menu.tab_id, can_split);
        let actions = egui::Area::new(egui::Id::new("rafui.editor-bottom-tab-context"))
            .order(egui::Order::Foreground)
            .fixed_pos(popup_pos)
            .show(ui.ctx(), |popup_ui| {
                popup_ui
                    .allocate_ui_with_layout(
                        popup_size,
                        egui::Layout::top_down(egui::Align::Min),
                        |popup_ui| {
                            self.tab_context_surface.show_transparent(
                                popup_ui,
                                render_state,
                                palette,
                                surface,
                                |key| t(key, language),
                            )
                        },
                    )
                    .inner
            })
            .inner;
        let mut close = false;
        for dispatched in actions {
            let UiAction::Command { name } = dispatched.action else {
                continue;
            };
            if name == "bottom.context.close" {
                close = true;
            } else if name == "bottom.context.reset" {
                self.reset_layout_for(project_type);
                close = true;
            } else if let Some(rest) = name.strip_prefix("bottom.context.split-left.") {
                if let Some((group_id, tab_id)) = rest.split_once('.') {
                    close |= self.split_tab_from_context(group_id, tab_id, true);
                }
            } else if let Some(rest) = name.strip_prefix("bottom.context.split-right.") {
                if let Some((group_id, tab_id)) = rest.split_once('.') {
                    close |= self.split_tab_from_context(group_id, tab_id, false);
                }
            }
        }
        let (pointer, current_time, pointer_pressed) = ui.ctx().input(|input| {
            (
                input.pointer.interact_pos(),
                input.time,
                input.pointer.any_pressed(),
            )
        });
        let just_opened = current_time - menu.opened_at_seconds <= 0.04;
        let outside_pressed = !just_opened
            && pointer_pressed
            && !pointer.is_some_and(|position| popup_rect.contains(position));
        if close || outside_pressed || ui.ctx().input(|input| input.key_pressed(egui::Key::Escape))
        {
            self.tab_context_menu = None;
            self.tab_context_surface.cancel_pointer_gesture();
        }
    }

    fn split_tab_from_context(&mut self, group_id: &str, tab_id: &str, before: bool) -> bool {
        if self.layout.groups.len() >= MAX_BOTTOM_DOCK_GROUPS {
            return false;
        }
        let new_group_id = next_group_id(&self.layout);
        let changed =
            self.layout
                .split_tab_next_to(group_id, tab_id, new_group_id.clone(), group_id, before);
        if changed {
            self.layout.select_tab(&new_group_id, tab_id);
            self.layout.normalize();
            repair_group_ids(&mut self.layout);
            self.layout_dirty = true;
        }
        changed
    }

    fn begin_drag_preview(&mut self, group_id: &str, tab_id: &str, time_seconds: f64) {
        let Some(tab) = self
            .layout
            .groups
            .iter()
            .find(|group| group.id == group_id)
            .and_then(|group| group.tabs.iter().find(|tab| tab.id == tab_id))
            .cloned()
        else {
            return;
        };
        let width = tab_slot_width(&tab);
        self.dragging_tab = Some((group_id.to_string(), tab_id.to_string()));
        self.last_drag_target = None;
        self.drag_slot_motion.set_immediate(0.0);
        self.drag_slot_motion.set_target(1.0);
        self.drag_preview = Some(BottomTabDragPreview {
            source_group_id: group_id.to_string(),
            source_tab_id: tab_id.to_string(),
            moving_tab: tab,
            target_group_id: group_id.to_string(),
            insertion_index: 0,
            split_before: None,
            pulse: drag_pulse(time_seconds),
            transition: 0.0,
            width,
        });
    }

    fn update_drag_preview(&mut self, pointer: Option<egui::Pos2>, time_seconds: f64) {
        let Some((source_group_id, source_tab_id)) = self.dragging_tab.as_ref() else {
            return;
        };
        let Some(point) = pointer else {
            return;
        };
        let Some((target_group_id, target_rect)) = self
            .group_rects
            .iter()
            .find(|(_, rect)| rect.contains(point))
            .map(|(group_id, rect)| (group_id.clone(), *rect))
        else {
            return;
        };
        let split_before = if self.layout.groups.len() < MAX_BOTTOM_DOCK_GROUPS {
            split_edge_for(
                &self.layout,
                &target_group_id,
                target_rect,
                point.x,
                source_group_id,
                source_tab_id,
            )
        } else {
            None
        };
        let insertion_index = if split_before.is_some() {
            0
        } else {
            insertion_index_for(
                &self.layout,
                &target_group_id,
                target_rect,
                point.x,
                source_group_id,
                source_tab_id,
            )
        };
        let target_signature = (target_group_id.clone(), insertion_index, split_before);
        if self.last_drag_target.as_ref() != Some(&target_signature) {
            self.last_drag_target = Some(target_signature);
            self.drag_slot_motion.set_immediate(0.22);
            self.drag_slot_motion.set_target(1.0);
        }
        if let Some(preview) = self.drag_preview.as_mut() {
            preview.target_group_id = target_group_id;
            preview.insertion_index = insertion_index;
            preview.split_before = split_before;
            preview.pulse = drag_pulse(time_seconds);
            preview.transition = self.drag_slot_motion.value();
        }
    }

    fn apply_dock_actions(
        &mut self,
        actions: Vec<UiDispatchedAction>,
        pointer: Option<egui::Pos2>,
        time_seconds: f64,
    ) {
        for dispatched in actions {
            let UiAction::Command { name } = dispatched.action else {
                continue;
            };
            if name == "bottom.toggle-collapsed" {
                self.toggle_collapsed();
                continue;
            }
            if let Some((group_id, tab_id)) = name
                .strip_prefix("bottom.context.open.")
                .and_then(|rest| rest.split_once('.'))
            {
                if let Some(anchor) = pointer {
                    self.tab_context_menu = Some(BottomTabContextMenu {
                        group_id: group_id.to_string(),
                        tab_id: tab_id.to_string(),
                        anchor,
                        opened_at_seconds: time_seconds,
                    });
                }
                continue;
            }
            if let Some((group_id, tab_id)) = name
                .strip_prefix("bottom.tab.")
                .and_then(|rest| rest.split_once('.'))
            {
                self.dragging_tab = None;
                self.drag_preview = None;
                self.last_drag_target = None;
                self.drag_slot_motion.set_immediate(1.0);
                if self.open_tab(tab_id) {
                    self.layout_dirty = true;
                } else {
                    self.layout_dirty |= self.layout.select_tab(group_id, tab_id);
                }
                continue;
            }
            if let Some((group_id, tab_id)) = name
                .strip_prefix("bottom.drag.start.")
                .and_then(|rest| rest.split_once('.'))
            {
                self.begin_drag_preview(group_id, tab_id, time_seconds);
                self.update_drag_preview(pointer, time_seconds);
                continue;
            }
            if name
                .strip_prefix("bottom.drag.move.")
                .and_then(|rest| rest.split_once('.'))
                .is_some()
            {
                self.update_drag_preview(pointer, time_seconds);
                continue;
            }
            if let Some((group_id, tab_id)) = name
                .strip_prefix("bottom.drag.end.")
                .and_then(|rest| rest.split_once('.'))
            {
                let source = self
                    .dragging_tab
                    .take()
                    .unwrap_or_else(|| (group_id.to_string(), tab_id.to_string()));
                let preview = self.drag_preview.take();
                let Some(preview) = preview else {
                    continue;
                };
                self.last_drag_target = None;
                self.drag_slot_motion.set_immediate(1.0);
                let target_group = preview.target_group_id.clone();
                let changed = if let Some(split_before) = preview.split_before {
                    if self.layout.groups.len() < MAX_BOTTOM_DOCK_GROUPS {
                        let new_group_id = next_group_id(&self.layout);
                        let changed = self.layout.split_tab_next_to(
                            &source.0,
                            &source.1,
                            new_group_id.clone(),
                            &target_group,
                            split_before,
                        );
                        if changed {
                            self.layout.select_tab(&new_group_id, &source.1);
                        }
                        changed
                    } else {
                        false
                    }
                } else if source.0 == target_group {
                    self.layout
                        .move_tab_within_group(&source.0, &source.1, preview.insertion_index)
                } else {
                    let changed =
                        self.layout
                            .move_tab_to_group(&source.0, &target_group, &source.1);
                    if changed {
                        self.layout.select_tab(&target_group, &source.1);
                    }
                    changed
                };
                if changed {
                    self.layout.normalize();
                    repair_group_ids(&mut self.layout);
                    self.layout_dirty = true;
                }
            }
        }
    }

    fn advance_drag_motion(&mut self, ctx: &egui::Context) {
        let now_seconds = ctx.input(|input| input.time);
        let delta_seconds = (now_seconds - self.last_drag_time_seconds)
            .max(0.0)
            .min(0.25) as f32;
        self.last_drag_time_seconds = now_seconds;
        if self.dragging_tab.is_none() {
            return;
        }
        let value = self.drag_slot_motion.advance(delta_seconds, false);
        if let Some(preview) = self.drag_preview.as_mut() {
            preview.transition = value;
        }
        if !self.drag_slot_motion.is_settled() {
            ctx.request_repaint();
        }
    }

    fn apply_console_actions(
        &mut self,
        actions: Vec<UiDispatchedAction>,
        console: &mut ConsolePanel,
        command_names: &[String],
    ) -> Vec<ConsoleSubmission> {
        let mut submissions = Vec::new();
        for dispatched in actions {
            match dispatched.action {
                UiAction::SetText { key, value } if key == "console.input" => {
                    console.set_input(value);
                }
                UiAction::SetToggle { key, value } if key == "console.auto-scroll" => {
                    console.auto_scroll = value;
                }
                UiAction::Command { name } if name == "console.clear" => console.clear_entries(),
                UiAction::Command { name } if name == "console.submit" => {
                    if let Some(submission) = console.submit_input() {
                        submissions.push(submission);
                    }
                }
                UiAction::Command { name } if name == "console.autocomplete" => {
                    console.autocomplete_command(command_names);
                }
                UiAction::Command { name } if name == "console.history.previous" => {
                    console.select_previous_history();
                }
                UiAction::Command { name } if name == "console.history.next" => {
                    console.select_next_history();
                }
                UiAction::Command { name } if name.starts_with("console.block.toggle.") => {
                    if let Some(id) = name
                        .strip_prefix("console.block.toggle.")
                        .and_then(|value| value.parse::<ConsoleEntryId>().ok())
                    {
                        if !self.expanded_blocks.insert(id) {
                            self.expanded_blocks.remove(&id);
                        }
                    }
                }
                UiAction::Command { name } if name.starts_with("console.filter.") => {
                    console.filter_level = match name.as_str() {
                        "console.filter.all" => None,
                        "console.filter.info" => Some(LogLevel::Info),
                        "console.filter.warning" => Some(LogLevel::Warning),
                        "console.filter.error" => Some(LogLevel::Error),
                        _ => console.filter_level,
                    };
                }
                _ => {}
            }
        }
        submissions
    }

    fn apply_assets_actions(&mut self, actions: Vec<UiDispatchedAction>) -> Vec<AssetsIntent> {
        let mut intents = Vec::new();
        for dispatched in actions {
            match dispatched.action {
                UiAction::SetText { key, value } if key == "assets.search" => {
                    self.assets_query = value.clone();
                    self.assets_surface_key = None;
                    intents.push(AssetsIntent::QueryChanged(value));
                }
                UiAction::Command { name } if name.starts_with("assets.filter.") => {
                    let filter = match name.as_str() {
                        "assets.filter.images" => AssetFilter::Images,
                        "assets.filter.models" => AssetFilter::Models,
                        "assets.filter.audio" => AssetFilter::Audio,
                        "assets.filter.scripts" => AssetFilter::Scripts,
                        _ => AssetFilter::All,
                    };
                    self.assets_filter = filter;
                    self.assets_surface_key = None;
                    intents.push(AssetsIntent::FilterChanged(filter));
                }
                UiAction::Command { name } if name == "assets.open-folder" => {
                    intents.push(AssetsIntent::OpenFolder);
                }
                UiAction::Command { name } if name == "assets.refresh" => {
                    intents.push(AssetsIntent::Refresh);
                }
                UiAction::Command { name } if name.starts_with("assets.open:") => {
                    if let Some(path) = name.strip_prefix("assets.open:") {
                        intents.push(AssetsIntent::OpenAsset(path.to_string()));
                    }
                }
                UiAction::Command { name } if name == "assets.create-script" => {
                    self.assets_script_menu_open = true;
                    self.assets_surface_key = None;
                }
                UiAction::Command { name } if name == "assets.script.cancel" => {
                    self.assets_script_menu_open = false;
                    self.assets_surface_key = None;
                }
                UiAction::SetText { key, value } if key == "assets.script-name" => {
                    self.assets_script_name = value.chars().take(96).collect();
                    self.assets_surface_key = None;
                }
                UiAction::Command { name } if name.starts_with("assets.script.create:") => {
                    if let Some(language) = name.strip_prefix("assets.script.create:") {
                        self.assets_script_menu_open = false;
                        self.assets_surface_key = None;
                        intents.push(AssetsIntent::CreateScript {
                            language: language.to_string(),
                            name: self.assets_script_name.clone(),
                        });
                    }
                }
                _ => {}
            }
        }
        intents
    }
}

fn drag_pulse(time_seconds: f64) -> f32 {
    ((time_seconds * 4.0).sin() * 0.5 + 0.5) as f32
}

fn bounded_group_rect(parent: egui::Rect, x: f32, width: f32) -> egui::Rect {
    let left = (parent.left() + x).clamp(parent.left(), parent.right());
    let right = (left + width.max(0.0)).clamp(left, parent.right());
    egui::Rect::from_min_max(
        egui::pos2(left, parent.top()),
        egui::pos2(right, parent.bottom()),
    )
}

fn bounded_group_rects(
    rects: Vec<(String, egui::Rect)>,
    parent: egui::Rect,
) -> Vec<(String, egui::Rect)> {
    rects
        .into_iter()
        .map(|(group_id, rect)| {
            (
                group_id,
                bounded_group_rect(parent, rect.left() - parent.left(), rect.width()),
            )
        })
        .collect()
}

fn interpolate_group_rects(
    from: &[(String, egui::Rect)],
    target: &[(String, egui::Rect)],
    progress: f32,
) -> Vec<(String, egui::Rect)> {
    let progress = progress.clamp(0.0, 1.0);
    target
        .iter()
        .map(|(group_id, target_rect)| {
            let from_rect = from
                .iter()
                .find(|(from_id, _)| from_id == group_id)
                .map(|(_, rect)| *rect)
                .unwrap_or_else(|| {
                    let center = target_rect.center().x;
                    egui::Rect::from_min_max(
                        egui::pos2(center, target_rect.top()),
                        egui::pos2(center, target_rect.bottom()),
                    )
                });
            let min = from_rect.min + (target_rect.min - from_rect.min) * progress;
            let max = from_rect.max + (target_rect.max - from_rect.max) * progress;
            (group_id.clone(), egui::Rect::from_min_max(min, max))
        })
        .collect()
}

fn tab_slot_width(tab: &DockTab) -> f32 {
    match tab.id.as_str() {
        "project-settings" => 104.0,
        "nodes" => 92.0,
        "console" => 96.0,
        "assets" => 86.0,
        "drc" => 86.0,
        "simulation" => 112.0,
        _ => 96.0,
    }
}

fn insertion_index_for(
    layout: &BottomDockLayout,
    target_group_id: &str,
    target_rect: egui::Rect,
    pointer_x: f32,
    source_group_id: &str,
    source_tab_id: &str,
) -> usize {
    let Some(group) = layout
        .groups
        .iter()
        .find(|group| group.id == target_group_id)
    else {
        return 0;
    };
    let mut cursor = target_rect.left() + 6.0;
    let mut index = 0;
    for tab in &group.tabs {
        if group.id == source_group_id && tab.id == source_tab_id {
            continue;
        }
        let width = tab_slot_width(tab);
        if pointer_x < cursor + width * 0.5 {
            return index;
        }
        cursor += width + 2.0;
        index += 1;
    }
    index
}

/// Returns a split direction only after the pointer crosses half of the
/// unused outer track. This lets the tab preview travel across existing tabs
/// before a new group preview appears.
fn split_edge_for(
    layout: &BottomDockLayout,
    target_group_id: &str,
    target_rect: egui::Rect,
    pointer_x: f32,
    source_group_id: &str,
    source_tab_id: &str,
) -> Option<bool> {
    let Some(group) = layout
        .groups
        .iter()
        .find(|group| group.id == target_group_id)
    else {
        return None;
    };
    let content_start = target_rect.left() + 6.0;
    let mut content_end = content_start;
    for tab in &group.tabs {
        if group.id == source_group_id && tab.id == source_tab_id {
            continue;
        }
        content_end += tab_slot_width(tab) + 2.0;
    }
    let right_gap = target_rect.right() - content_end;
    if right_gap > 40.0 && pointer_x >= content_end + right_gap * 0.5 {
        return Some(false);
    }
    let left_gap = content_start - target_rect.left();
    if left_gap > 4.0 && pointer_x <= target_rect.left() + left_gap * 0.5 {
        return Some(true);
    }
    if pointer_x <= target_rect.left() + 24.0 {
        return Some(true);
    }
    if pointer_x >= target_rect.right() - 24.0 {
        return Some(false);
    }
    None
}

fn layout_for_project(project_type: ProjectType) -> BottomDockLayout {
    let common_tabs = vec![
        DockTab::new("console", "app.studio_console", UiIconId::Console),
        DockTab::new("assets", "app.studio_assets", UiIconId::Assets),
        DockTab::new("project-settings", "app.studio_project", UiIconId::Settings),
    ];
    let groups = match project_type {
        ProjectType::Game => {
            let mut tabs = common_tabs;
            tabs.push(DockTab::new("nodes", "app.studio_nodes", UiIconId::Node));
            tabs.push(DockTab::new("agent", "app.agent_tab", UiIconId::Agent));
            vec![DockTabGroup::new("main", tabs)
                .with_weight(1.0)
                .with_min_width(360.0)]
        }
        ProjectType::Electronics => {
            let mut workspace_tabs = common_tabs;
            workspace_tabs.push(DockTab::new("agent", "app.agent_tab", UiIconId::Agent));
            let analysis_tabs = vec![
                DockTab::new("drc", "app.electronics_drc", UiIconId::Warning),
                DockTab::new("simulation", "app.electronics_simulation", UiIconId::Play),
            ];
            vec![
                DockTabGroup::new("workspace", workspace_tabs)
                    .with_weight(1.45)
                    .with_min_width(420.0),
                DockTabGroup::new("analysis", analysis_tabs)
                    .with_weight(0.85)
                    .with_min_width(300.0),
            ]
        }
    };
    let mut layout = BottomDockLayout::new(groups);
    // A fresh Electronics workspace starts compact enough to leave the
    // schematic/PCB viewport useful. Explicit user resizing remains persisted
    // and authoritative after the workspace has been customized.
    let default_height = if project_type == ProjectType::Electronics {
        112.0
    } else {
        144.0
    };
    layout.height = default_height;
    layout.expanded_height = default_height;
    layout
}

pub(crate) fn sanitize_layout_for_project(
    mut layout: BottomDockLayout,
    project_type: ProjectType,
) -> BottomDockLayout {
    sanitize_layout(&mut layout, project_type);
    layout
}

fn supported_tabs(project_type: ProjectType) -> Vec<DockTab> {
    layout_for_project(project_type)
        .groups
        .into_iter()
        .flat_map(|group| group.tabs)
        .collect()
}

fn sanitize_layout(layout: &mut BottomDockLayout, project_type: ProjectType) -> bool {
    let before = layout.clone();
    if layout.version < raf_ui::BOTTOM_DOCK_LAYOUT_VERSION {
        *layout = layout_for_project(project_type);
        return *layout != before;
    }
    let supported = supported_tabs(project_type);
    let supported_ids = supported
        .iter()
        .map(|tab| tab.id.as_str())
        .collect::<BTreeSet<_>>();
    let preferred_active = layout.groups.iter().find_map(|group| {
        supported_ids
            .contains(group.active_tab.as_str())
            .then(|| group.active_tab.clone())
    });
    let mut seen_tabs = BTreeSet::new();
    for group in &mut layout.groups {
        group.tabs.retain(|tab| {
            supported_ids.contains(tab.id.as_str()) && seen_tabs.insert(tab.id.clone())
        });
        for tab in &mut group.tabs {
            if let Some(canonical) = supported.iter().find(|candidate| candidate.id == tab.id) {
                tab.title_key = canonical.title_key.clone();
                tab.icon = canonical.icon;
            }
        }
    }
    layout.normalize();
    repair_group_ids(layout);
    if layout.groups.is_empty() {
        *layout = layout_for_project(project_type);
        sanitize_dock_height(layout);
        return *layout != before;
    }
    let existing = layout
        .groups
        .iter()
        .flat_map(|group| group.tabs.iter().map(|tab| tab.id.clone()))
        .collect::<Vec<_>>();
    for tab in supported {
        if existing.iter().any(|id| *id == tab.id) {
            continue;
        }
        let target_index = if project_type == ProjectType::Electronics
            && matches!(tab.id.as_str(), "drc" | "simulation")
        {
            layout
                .groups
                .iter()
                .position(|group| group.id == "analysis")
                .unwrap_or(0)
        } else {
            0
        };
        if let Some(target_group) = layout.groups.get_mut(target_index) {
            target_group.tabs.push(tab);
        }
    }
    if project_type == ProjectType::Electronics && layout.groups.len() == 1 {
        split_electronics_analysis_group(layout);
    }
    if let Some(active_tab) = preferred_active {
        if let Some(group) = layout
            .groups
            .iter_mut()
            .find(|group| group.tabs.iter().any(|tab| tab.id == active_tab))
        {
            group.active_tab = active_tab;
        }
    }
    layout.normalize();
    sanitize_dock_height(layout);
    // Migrate the old 144 px default for Electronics once. A deliberately
    // resized layout remains untouched; only the untouched legacy default is
    // compacted for the CAD canvas.
    if project_type == ProjectType::Electronics
        && !layout.collapsed
        && (layout.height - 144.0).abs() < f32::EPSILON
        && (layout.expanded_height - 144.0).abs() < f32::EPSILON
    {
        layout.height = 112.0;
        layout.expanded_height = 112.0;
    }
    *layout != before
}

fn split_electronics_analysis_group(layout: &mut BottomDockLayout) -> bool {
    if layout.groups.len() != 1 {
        return false;
    }
    let source = &mut layout.groups[0];
    let original_active = source.active_tab.clone();
    let mut analysis_tabs = Vec::new();
    source.tabs.retain(|tab| {
        if matches!(tab.id.as_str(), "drc" | "simulation") {
            analysis_tabs.push(tab.clone());
            false
        } else {
            true
        }
    });
    if analysis_tabs.is_empty() || source.tabs.is_empty() {
        source.tabs.extend(analysis_tabs);
        return false;
    }
    let active_analysis = analysis_tabs.iter().any(|tab| tab.id == original_active);
    if !source.tabs.iter().any(|tab| tab.id == source.active_tab) {
        source.active_tab = source
            .tabs
            .first()
            .map(|tab| tab.id.clone())
            .unwrap_or_default();
    }
    source.id = "workspace".to_string();
    source.weight = 1.45;
    source.min_width = 420.0;
    let mut analysis = DockTabGroup::new("analysis", analysis_tabs)
        .with_weight(0.85)
        .with_min_width(300.0);
    if active_analysis {
        analysis.active_tab = original_active;
    }
    layout.groups.push(analysis);
    true
}

fn sanitize_dock_height(layout: &mut BottomDockLayout) -> bool {
    let before = (layout.height, layout.expanded_height);
    let default_height = BottomDockLayout::new(Vec::new()).height;
    layout.expanded_height = if layout.expanded_height.is_finite() {
        layout
            .expanded_height
            .clamp(DOWNBAR_MIN_HEIGHT, DOWNBAR_MAX_HEIGHT)
    } else {
        default_height.clamp(DOWNBAR_MIN_HEIGHT, DOWNBAR_MAX_HEIGHT)
    };
    layout.height = if layout.collapsed {
        DOWNBAR_COLLAPSED_HEIGHT
    } else if layout.height.is_finite() {
        layout.height.clamp(DOWNBAR_MIN_HEIGHT, DOWNBAR_MAX_HEIGHT)
    } else {
        layout.expanded_height
    };
    before != (layout.height, layout.expanded_height)
}

fn repair_group_ids(layout: &mut BottomDockLayout) {
    let mut seen = BTreeSet::new();
    for (index, group) in layout.groups.iter_mut().enumerate() {
        let base = if group.id.trim().is_empty() {
            format!("group-{}", index + 1)
        } else {
            group.id.clone()
        };
        let mut candidate = base.clone();
        let mut suffix = 2;
        while !seen.insert(candidate.clone()) {
            candidate = format!("{base}-{suffix}");
            suffix += 1;
        }
        group.id = candidate;
    }
}

fn next_group_id(layout: &BottomDockLayout) -> String {
    (1..=MAX_BOTTOM_DOCK_GROUPS + 1)
        .map(|index| format!("group-{index}"))
        .find(|candidate| !layout.groups.iter().any(|group| group.id == *candidate))
        .unwrap_or_else(|| format!("group-{}", layout.groups.len() + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_layout_repairs_tabs_duplicated_across_split_groups() {
        let tabs = vec![
            DockTab::new("console", "app.studio_console", UiIconId::Console),
            DockTab::new("agent", "app.agent_tab", UiIconId::Agent),
            DockTab::new(
                "project-settings",
                "app.project_settings_tab",
                UiIconId::Settings,
            ),
            DockTab::new("assets", "app.studio_assets", UiIconId::Assets),
        ];
        let mut layout = BottomDockLayout::new(vec![
            DockTabGroup::new("left", tabs.clone()),
            DockTabGroup::new("right", tabs),
        ]);
        layout.groups[0].active_tab = "project".to_string();

        assert!(sanitize_layout(&mut layout, ProjectType::Game));
        assert_eq!(layout.groups.len(), 1);
        assert_eq!(layout.groups[0].active_tab, "console");
        assert_eq!(layout.groups[0].tabs.len(), 5);
        assert_eq!(
            layout.groups[0]
                .tabs
                .iter()
                .filter(|tab| tab.id == "console")
                .count(),
            1
        );
    }

    #[test]
    fn reset_layout_restores_one_group_without_touching_project_data() {
        let mut host = EditorBottomDockHost::default();
        assert!(host
            .layout
            .split_tab_next_to("main", "console", "split", "main", false));

        host.reset_layout();

        assert_eq!(host.layout.groups.len(), 1);
        assert_eq!(host.layout.groups[0].tabs.len(), 5);
        assert_eq!(
            host.layout.groups[0]
                .tabs
                .iter()
                .map(|tab| tab.id.as_str())
                .collect::<Vec<_>>(),
            vec!["console", "assets", "project-settings", "nodes", "agent"]
        );
        assert_eq!(host.layout.version, raf_ui::BOTTOM_DOCK_LAYOUT_VERSION);
        assert!(host.layout_dirty);
        assert!(host.dragging_tab.is_none());
        assert!(host.drag_preview.is_none());
    }

    #[test]
    fn game_layout_excludes_electronics_analysis_tabs() {
        let layout = layout_for_project(ProjectType::Game);
        let ids = layout.groups[0]
            .tabs
            .iter()
            .map(|tab| tab.id.as_str())
            .collect::<Vec<_>>();
        assert!(ids.contains(&"nodes"));
        assert!(!ids.contains(&"drc"));
        assert!(!ids.contains(&"simulation"));
    }

    #[test]
    fn electronics_layout_excludes_game_nodes_tab() {
        let layout = layout_for_project(ProjectType::Electronics);
        let ids = layout
            .groups
            .iter()
            .flat_map(|group| group.tabs.iter())
            .map(|tab| tab.id.as_str())
            .collect::<Vec<_>>();
        assert!(!ids.contains(&"nodes"));
        assert!(ids.contains(&"drc"));
        assert!(ids.contains(&"simulation"));
        assert_eq!(layout.groups.len(), 2);
        assert_eq!(layout.groups[0].id, "workspace");
        assert_eq!(layout.groups[1].id, "analysis");
        assert_eq!(layout.height, 112.0);
        assert_eq!(layout.expanded_height, 112.0);
    }

    #[test]
    fn electronics_single_group_layout_migrates_to_workspace_and_analysis() {
        let mut layout = BottomDockLayout::new(vec![DockTabGroup::new(
            "main",
            vec![
                DockTab::new("console", "app.studio_console", UiIconId::Console),
                DockTab::new("agent", "app.agent_tab", UiIconId::Agent),
                DockTab::new("drc", "app.electronics_drc", UiIconId::Warning),
                DockTab::new("simulation", "app.electronics_simulation", UiIconId::Play),
            ],
        )]);

        assert!(sanitize_layout(&mut layout, ProjectType::Electronics));
        assert_eq!(layout.groups.len(), 2);
        assert_eq!(layout.groups[0].id, "workspace");
        assert_eq!(layout.groups[1].id, "analysis");
        assert!(layout.groups[1]
            .tabs
            .iter()
            .all(|tab| matches!(tab.id.as_str(), "drc" | "simulation")));
    }

    #[test]
    fn sanitize_layout_refreshes_legacy_project_tab_presentation() {
        let mut layout = BottomDockLayout::new(vec![DockTabGroup::new(
            "main",
            vec![DockTab::new(
                "project-settings",
                "app.project_settings_tab",
                UiIconId::Settings,
            )],
        )]);

        assert!(sanitize_layout(&mut layout, ProjectType::Game));
        let project_tab = layout.groups[0]
            .tabs
            .iter()
            .find(|tab| tab.id == "project-settings")
            .expect("project tab");
        assert_eq!(project_tab.title_key, "app.studio_project");
    }
}
