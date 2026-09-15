//! Native Electronics document controller.
//!
//! This module is deliberately renderer-neutral.  RafUI owns the chrome and
//! menus, ApiGraphicBasic owns the CAD drawing, while this controller owns the
//! live documents, camera, scene and analysis state. Authoring mechanics live
//! in `electronics_controller_interaction.rs`; together they replace the
//! interaction that used to live in the legacy widget canvas without bringing
//! that toolkit back into the engine.

use glam::Vec2;
use raf_core::config::EngineSettings;
use raf_core::project::{Project, ProjectSettings};
use raf_core::session::ProjectSessionRegistry;
use raf_core::{CaptureMode, InputKey, InputOwner, InputRouter, InputSnapshot, PointerButton};
use raf_electronics::cad_interaction::{pick, CadInteractionState};
use raf_electronics::library::ComponentLibrary;
use raf_electronics::schematic::{component_pin_world_position, WireAnchor};
use raf_electronics::{
    orthogonal_wire_points, CadLayerKind, CadObject, CadObjectKind, CadPickPriority, CadScene,
    CadSurfaceKind, DrcReport, PcbLayout, Schematic,
};
use raf_render::api_graphic_basic::cad_surface::CadSurfaceOptions;
use uuid::Uuid;

use crate::editor_layout::EditorRect;
use crate::electronics_history::{ElectronicsDocumentSnapshot, ElectronicsHistory};
use crate::pcb_document::{load_pcb_document, save_pcb_document};
use crate::schematic_document::{load_schematic_document, save_schematic_document};

#[path = "electronics_analysis.rs"]
mod analysis;
#[path = "electronics_command_adapter.rs"]
mod command_adapter;
#[path = "electronics_controller_interaction.rs"]
mod interaction;

use analysis::{AnalysisKind, AnalysisResult, AnalysisTask};

const MIN_ZOOM: f32 = 0.15;
const MAX_ZOOM: f32 = 12.0;
const DEFAULT_ZOOM: f32 = 1.0;
const PICK_TOLERANCE_SCREEN: f32 = 10.0;
const PIN_SNAP_TOLERANCE_SCREEN: f32 = 16.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElectronicsTool {
    Select,
    Pan,
    Wire,
    Route,
    Place,
    BoardOutline,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CadCamera {
    pub center: Vec2,
    /// Screen pixels per schematic world unit.
    pub zoom: f32,
}

impl Default for CadCamera {
    fn default() -> Self {
        Self {
            center: Vec2::ZERO,
            zoom: DEFAULT_ZOOM,
        }
    }
}

impl CadCamera {
    pub fn world_from_screen(self, local: Vec2, size: Vec2) -> Vec2 {
        self.center + (local - size * 0.5) / self.zoom.max(MIN_ZOOM)
    }

    pub fn screen_from_world(self, world: Vec2, size: Vec2) -> Vec2 {
        size * 0.5 + (world - self.center) * self.zoom.max(MIN_ZOOM)
    }

    pub fn world_bounds(self, size: Vec2) -> [f32; 4] {
        let half = size * 0.5 / self.zoom.max(MIN_ZOOM);
        [
            self.center.x - half.x,
            self.center.x + half.x,
            self.center.y - half.y,
            self.center.y + half.y,
        ]
    }

    pub fn zoom_at(&mut self, factor: f32, cursor: Vec2, size: Vec2) {
        let before = self.world_from_screen(cursor, size);
        self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let after = (cursor - size * 0.5) / self.zoom;
        self.center = before - after;
    }
}

#[derive(Debug, Clone)]
struct ComponentDrag {
    component_id: Uuid,
    pointer_offset: Vec2,
    before: ElectronicsDocumentSnapshot,
}

#[derive(Debug, Clone, Copy)]
struct WireStart {
    world: Vec2,
    anchor: Option<WireAnchor>,
}

#[derive(Debug, Clone, Copy)]
struct SecondaryPointerState {
    start: Vec2,
    dragging: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElectronicsSelectionKind {
    Component,
    Pin,
    Wire,
    Trace,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElectronicsSelection {
    pub source_id: Uuid,
    pub kind: ElectronicsSelectionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElectronicsInputResult {
    pub changed: bool,
    pub request_redraw: bool,
}

impl Default for ElectronicsInputResult {
    fn default() -> Self {
        Self {
            changed: false,
            request_redraw: false,
        }
    }
}

/// Live Electronics state for one project/session.
pub struct NativeElectronicsEditor {
    schematic: Schematic,
    pcb: PcbLayout,
    scene: CadScene,
    history: ElectronicsHistory,
    interaction: CadInteractionState,
    selection: Option<ElectronicsSelection>,
    active_surface: CadSurfaceKind,
    camera: CadCamera,
    tool: ElectronicsTool,
    library: ComponentLibrary,
    placement_template: Option<usize>,
    grid_visible: bool,
    snap_enabled: bool,
    grid_step: f32,
    schematic_grid_step: f32,
    pcb_grid_step: f32,
    placement_drag_active: bool,
    placement_preview: Option<Vec2>,
    grid_opacity: f32,
    labels_visible: bool,
    drc_lines: Vec<String>,
    drc_report: Option<DrcReport>,
    simulation_lines: Vec<String>,
    analysis_task: Option<AnalysisTask>,
    wire_start: Option<WireStart>,
    board_outline_start: Option<Vec2>,
    pointer_world: Option<Vec2>,
    context_menu_position: Option<[f32; 2]>,
    secondary_pointer: Option<SecondaryPointerState>,
    component_drag: Option<ComponentDrag>,
    pan_pointer: Option<PointerButton>,
    minimap_drag: bool,
    inactive_camera: Option<CadCamera>,
    camera_initialized: bool,
    revision: u64,
    ui_revision: u64,
    dirty: bool,
}

impl NativeElectronicsEditor {
    pub fn empty(name: &str) -> Self {
        let schematic = Schematic::new(name);
        let pcb = PcbLayout::new(name);
        let scene = CadScene::from_schematic(&schematic);
        Self {
            schematic,
            pcb,
            scene,
            history: ElectronicsHistory::new(),
            interaction: CadInteractionState::default(),
            selection: None,
            active_surface: CadSurfaceKind::Schematic,
            camera: CadCamera::default(),
            tool: ElectronicsTool::Select,
            library: ComponentLibrary::default_library(),
            placement_template: None,
            grid_visible: true,
            snap_enabled: true,
            grid_step: 20.0,
            schematic_grid_step: 20.0,
            pcb_grid_step: 20.0,
            placement_drag_active: false,
            placement_preview: None,
            grid_opacity: 0.55,
            labels_visible: true,
            drc_lines: Vec::new(),
            drc_report: None,
            simulation_lines: Vec::new(),
            analysis_task: None,
            wire_start: None,
            board_outline_start: None,
            pointer_world: None,
            context_menu_position: None,
            secondary_pointer: None,
            component_drag: None,
            pan_pointer: None,
            minimap_drag: false,
            inactive_camera: None,
            camera_initialized: false,
            revision: 1,
            ui_revision: 1,
            dirty: false,
        }
    }

    pub fn from_project(project: &Project) -> Self {
        let mut editor = Self::empty(&project.name);
        if project.project_type != raf_core::project::ProjectType::Electronics {
            return editor;
        }

        let registry = ProjectSessionRegistry::load_or_legacy(&project.path, project.project_type);
        if let Some(session) = registry.active() {
            let schematic_path = session.path(&project.path, &session.schematic_file);
            let pcb_path = session.path(&project.path, &session.pcb_file);
            if let Some(schematic) = load_schematic_document(&schematic_path) {
                editor.schematic = schematic;
            }
            if let Some(pcb) = load_pcb_document(&pcb_path) {
                editor.pcb = pcb;
            }
        }
        editor.library.load_external_assets_from(&project.path);
        editor.rebuild_scene();
        editor.dirty = false;
        editor
    }

    pub fn schematic(&self) -> &Schematic {
        &self.schematic
    }

    pub fn pcb(&self) -> &PcbLayout {
        &self.pcb
    }

    pub fn scene(&self) -> &CadScene {
        &self.scene
    }

    pub fn active_surface(&self) -> CadSurfaceKind {
        self.active_surface
    }

    pub fn camera(&self) -> CadCamera {
        self.camera
    }

    pub fn tool(&self) -> ElectronicsTool {
        self.tool
    }

    pub fn library(&self) -> &ComponentLibrary {
        &self.library
    }

    /// Re-read external component templates from `<project_path>/ElectricalAssets/`
    /// and merge them into the live library. Safe to call after the editor is
    /// already open; touch the UI revision so the navigator refreshes.
    pub fn refresh_library(&mut self, project_path: &std::path::Path) {
        self.library.load_external_assets_from(project_path);
        self.touch_ui();
    }

    pub fn grid_visible(&self) -> bool {
        self.grid_visible
    }

    pub fn grid_step(&self) -> f32 {
        self.grid_step
    }

    pub fn apply_engine_settings(&mut self, settings: &EngineSettings) {
        let grid_step = settings.electronics_grid_step_mm.clamp(5.0, 100.0);
        let grid_opacity = settings.electronics_grid_opacity.clamp(0.2, 1.0);
        let changed = self.grid_visible != settings.grid_visible
            || self.snap_enabled != settings.snap_to_grid
            || (self.grid_step - grid_step).abs() > f32::EPSILON
            || (self.grid_opacity - grid_opacity).abs() > f32::EPSILON;
        let labels_changed = self.labels_visible != settings.show_viewport_labels;
        self.grid_visible = settings.grid_visible;
        self.snap_enabled = settings.snap_to_grid;
        self.grid_step = grid_step;
        self.schematic_grid_step = grid_step;
        self.pcb_grid_step = grid_step;
        self.grid_opacity = grid_opacity;
        self.labels_visible = settings.show_viewport_labels;
        if changed || labels_changed {
            self.touch_ui();
            self.touch();
        }
    }

    pub fn apply_project_settings(&mut self, settings: &ProjectSettings) {
        let schematic_step = settings
            .electronics_schematic_grid_step_mm
            .clamp(1.0, 100.0);
        let pcb_step = settings.electronics_pcb_grid_step_mm.clamp(1.0, 100.0);
        let active_step = match self.active_surface {
            CadSurfaceKind::Schematic => schematic_step,
            CadSurfaceKind::Pcb => pcb_step,
        };
        let changed = (self.schematic_grid_step - schematic_step).abs() > f32::EPSILON
            || (self.pcb_grid_step - pcb_step).abs() > f32::EPSILON
            || self.snap_enabled != settings.electronics_snap_to_grid
            || (self.grid_step - active_step).abs() > f32::EPSILON;
        self.schematic_grid_step = schematic_step;
        self.pcb_grid_step = pcb_step;
        self.grid_step = active_step;
        self.snap_enabled = settings.electronics_snap_to_grid;
        if changed {
            self.touch_ui();
            self.touch();
        }
    }

    pub fn labels_visible(&self) -> bool {
        self.labels_visible
    }

    pub fn component_asset_key(&self, source_id: Uuid) -> &'static str {
        self.schematic
            .components
            .iter()
            .find(|component| component.id == source_id)
            .map(|component| component_asset_key(component.kind_label()))
            .unwrap_or("electronics://library/generic.png")
    }

    /// Asset used by the pointer-driven library placement ghost. Keeping this
    /// lookup in the controller means the canvas overlay does not need to know
    /// how component templates are classified.
    pub fn placement_preview_asset_key(&self) -> Option<&'static str> {
        self.placement_drag_active.then(|| {
            self.placement_template
                .and_then(|index| self.library.components.get(index))
                .map(|template| component_asset_key(template.template.kind_label()))
                .unwrap_or("electronics://library/generic.png")
        })
    }

    pub fn drc_lines(&self) -> &[String] {
        &self.drc_lines
    }

    pub fn drc_report(&self) -> Option<&DrcReport> {
        self.drc_report.as_ref()
    }

    pub fn simulation_lines(&self) -> &[String] {
        &self.simulation_lines
    }

    pub(crate) fn start_drc_analysis(&mut self) {
        self.start_analysis(AnalysisKind::Drc);
    }

    pub(crate) fn start_simulation_analysis(&mut self) {
        self.start_analysis(AnalysisKind::Simulation);
    }

    pub(crate) fn cancel_analysis(&mut self) {
        if let Some(task) = self.analysis_task.take() {
            let kind = task.kind();
            task.cancel();
            match kind {
                AnalysisKind::Drc => {
                    self.drc_report = None;
                    self.drc_lines = vec!["Status: cancelled".to_string()];
                }
                AnalysisKind::Simulation => {
                    self.simulation_lines = vec!["Status: cancelled".to_string()];
                }
            }
            self.touch_ui();
        }
    }

    pub(crate) fn analysis_running(&self) -> bool {
        self.analysis_task.is_some()
    }

    pub(crate) fn poll_analysis(&mut self) -> bool {
        let Some(task) = self.analysis_task.take() else {
            return false;
        };
        match task.try_result() {
            Ok(Some(AnalysisResult::Drc(report))) => {
                self.drc_lines = report.to_string_list();
                self.drc_report = Some(report);
                self.rebuild_scene_with_drc();
                self.touch_ui();
                true
            }
            Ok(Some(AnalysisResult::Simulation(result))) => {
                let mut lines = vec![format!(
                    "Status: {}",
                    if result.converged {
                        "converged"
                    } else {
                        "not converged"
                    }
                )];
                lines.push(format!("Nodes solved: {}", result.node_voltages.len()));
                lines.push(format!(
                    "Components evaluated: {}",
                    result.component_currents.len()
                ));
                lines.extend(result.messages);
                self.simulation_lines = lines;
                self.touch_ui();
                true
            }
            Ok(None) => {
                self.analysis_task = Some(task);
                false
            }
            Err(()) => {
                match task.kind() {
                    AnalysisKind::Drc => {
                        self.drc_report = None;
                        self.drc_lines = vec!["Status: analysis failed".to_string()];
                    }
                    AnalysisKind::Simulation => {
                        self.simulation_lines = vec!["Status: analysis failed".to_string()];
                    }
                }
                self.touch_ui();
                true
            }
        }
    }

    pub fn selection(&self) -> Option<ElectronicsSelection> {
        self.selection
    }

    pub fn select_component(&mut self, index: usize) -> bool {
        if self.active_surface == CadSurfaceKind::Pcb {
            let Some(component) = self.pcb.components.get(index) else {
                return false;
            };
            self.selection = Some(ElectronicsSelection {
                source_id: component.component_id,
                kind: ElectronicsSelectionKind::Component,
            });
            self.interaction.selected = Some(raf_electronics::cad_interaction::CadSelection {
                object_id: format!("pcb_component:{}", component.component_id),
                source_id: Some(component.component_id),
                kind: CadObjectKind::Component,
                layer: match component.layer {
                    raf_electronics::PcbLayer::TopCopper => CadLayerKind::PcbTopCopper,
                    raf_electronics::PcbLayer::BottomCopper => CadLayerKind::PcbBottomCopper,
                },
            });
            self.touch_ui();
            return true;
        }
        let Some(component) = self.schematic.components.get(index) else {
            return false;
        };
        self.selection = Some(ElectronicsSelection {
            source_id: component.id,
            kind: ElectronicsSelectionKind::Component,
        });
        self.interaction.selected = Some(raf_electronics::cad_interaction::CadSelection {
            object_id: format!("component:{}", component.id),
            source_id: Some(component.id),
            kind: CadObjectKind::Component,
            layer: CadLayerKind::Schematic,
        });
        self.touch_ui();
        true
    }

    pub fn select_wire(&mut self, index: usize) -> bool {
        let Some(wire) = self.schematic.wires.get(index) else {
            return false;
        };
        self.selection = Some(ElectronicsSelection {
            source_id: wire.id,
            kind: ElectronicsSelectionKind::Wire,
        });
        self.interaction.selected = Some(raf_electronics::cad_interaction::CadSelection {
            object_id: format!("wire:{}", wire.id),
            source_id: Some(wire.id),
            kind: CadObjectKind::Wire,
            layer: CadLayerKind::Schematic,
        });
        self.touch_ui();
        true
    }

    pub fn select_trace(&mut self, index: usize) -> bool {
        if self.active_surface != CadSurfaceKind::Pcb {
            return false;
        }
        let Some(trace) = self.pcb.traces.get(index) else {
            return false;
        };
        self.selection = Some(ElectronicsSelection {
            source_id: trace.id,
            kind: ElectronicsSelectionKind::Trace,
        });
        self.interaction.selected = Some(raf_electronics::cad_interaction::CadSelection {
            object_id: format!("trace:{}", trace.id),
            source_id: Some(trace.id),
            kind: CadObjectKind::Trace,
            layer: match trace.layer {
                raf_electronics::PcbLayer::TopCopper => CadLayerKind::PcbTopCopper,
                raf_electronics::PcbLayer::BottomCopper => CadLayerKind::PcbBottomCopper,
            },
        });
        self.touch_ui();
        true
    }

    pub fn zoom_in(&mut self) {
        self.camera.zoom = (self.camera.zoom * 1.15).clamp(MIN_ZOOM, MAX_ZOOM);
        self.touch();
    }

    pub fn zoom_out(&mut self) {
        self.camera.zoom = (self.camera.zoom / 1.15).clamp(MIN_ZOOM, MAX_ZOOM);
        self.touch();
    }

    pub fn apply_ui_command(&mut self, command: &str) -> bool {
        match command {
            "electronics.select" => self.set_tool(ElectronicsTool::Select),
            "electronics.pan" => self.set_tool(ElectronicsTool::Pan),
            "electronics.wire" => self.set_tool(ElectronicsTool::Wire),
            "electronics.route" => self.set_tool(ElectronicsTool::Route),
            "electronics.place" => self.set_tool(ElectronicsTool::Place),
            "electronics.board-outline" => self.set_tool(ElectronicsTool::BoardOutline),
            "electronics.fit" => self.fit_view(),
            "electronics.grid.toggle" => self.toggle_grid(),
            "electronics.labels.toggle" => self.toggle_labels(),
            "electronics.zoom-in" => self.zoom_in(),
            "electronics.zoom-out" => self.zoom_out(),
            "electronics.rotate" => {
                self.rotate_selected();
            }
            "electronics.analysis.drc" => self.run_drc(),
            "electronics.analysis.simulation" => self.run_simulation(),
            "electronics.analysis.cancel" => {
                self.cancel_analysis();
            }
            "electronics.mode.pcb" => self.set_surface(CadSurfaceKind::Pcb),
            "electronics.mode.schematic" => self.set_surface(CadSurfaceKind::Schematic),
            "electronics.pcb.sync" => {
                self.sync_pcb_from_schematic();
            }
            "edit.undo" => {
                self.undo();
            }
            "edit.redo" => {
                self.redo();
            }
            "edit.delete" => {
                self.delete_selected();
            }
            "project.save" => {}
            "electronics.context.delete" => {
                self.delete_selected();
                self.context_menu_position = None;
            }
            "electronics.context.route" => {
                self.route_selected_airwire();
                self.context_menu_position = None;
            }
            "electronics.context.select" | "electronics.context.properties" => {
                self.context_menu_position = None;
                self.touch_ui();
            }
            "electronics.context.duplicate" => {
                if let Some(selection) = self.selection {
                    if let Some(index) = self
                        .schematic
                        .components
                        .iter()
                        .position(|component| component.id == selection.source_id)
                    {
                        let before = self.snapshot();
                        if let Some(id) = self.schematic.duplicate_component(index) {
                            if self.active_surface == CadSurfaceKind::Pcb {
                                self.pcb.sync_from_schematic(&self.schematic);
                            }
                            self.selection = Some(ElectronicsSelection {
                                source_id: id,
                                kind: ElectronicsSelectionKind::Component,
                            });
                            self.history.record(before, &self.snapshot());
                            self.dirty = true;
                            self.rebuild_scene();
                        }
                    }
                }
                self.context_menu_position = None;
                self.touch_ui();
            }
            "electronics.context.cancel" => {
                self.wire_start = None;
                self.context_menu_position = None;
                self.touch_ui();
            }
            _ => {
                if let Some(value) = command.strip_prefix("electronics.inspector.value.commit:") {
                    return self.set_selected_value(value);
                }
                if let Some(index) = command.strip_prefix("electronics.navigator.select.") {
                    if let Ok(index) = index.parse::<usize>() {
                        return self.select_component(index);
                    }
                }
                if let Some(index) = command.strip_prefix("electronics.navigator.select-wire.") {
                    if let Ok(index) = index.parse::<usize>() {
                        return self.select_wire(index);
                    }
                }
                if let Some(index) = command.strip_prefix("electronics.navigator.select-trace.") {
                    if let Ok(index) = index.parse::<usize>() {
                        return self.select_trace(index);
                    }
                }
                if let Some(index) = command.strip_prefix("electronics.place.template.") {
                    if let Ok(index) = index.parse::<usize>() {
                        if index < self.library.components.len() {
                            self.placement_template = Some(index);
                            self.set_tool(ElectronicsTool::Place);
                            return true;
                        }
                    }
                }
                if let Some(index) = command.strip_prefix("electronics.library.drag.start.") {
                    if let Ok(index) = index.parse::<usize>() {
                        return self.begin_library_drag(index);
                    }
                }
                if command.starts_with("electronics.library.drag.move.")
                    || command.starts_with("electronics.library.drag.end.")
                {
                    // Pointer coordinates are consumed by the native frame
                    // so the controller can convert them against the actual
                    // Electronics canvas. The command itself is retained as
                    // a semantic no-op for direct/headless callers.
                    return self.placement_drag_active;
                }
                return false;
            }
        }
        true
    }

    pub fn set_surface(&mut self, surface: CadSurfaceKind) {
        if self.active_surface == surface {
            return;
        }
        let cross_probe_component = self
            .selection
            .filter(|selection| {
                matches!(
                    selection.kind,
                    ElectronicsSelectionKind::Component | ElectronicsSelectionKind::Pin
                )
            })
            .map(|selection| selection.source_id);
        let previous_camera = self.camera_initialized.then_some(self.camera);
        let next_camera = self.inactive_camera.take();
        self.inactive_camera = previous_camera;
        self.camera = next_camera.unwrap_or_default();
        self.camera_initialized = next_camera.is_some();
        self.active_surface = surface;
        // A PCB view is a derived document. Keep the established workflow in
        // which entering it materializes missing footprints/links from the
        // schematic, while still preserving an independent camera per view.
        if surface == CadSurfaceKind::Pcb {
            self.sync_pcb_from_schematic();
        }
        self.grid_step = match surface {
            CadSurfaceKind::Schematic => self.schematic_grid_step,
            CadSurfaceKind::Pcb => self.pcb_grid_step,
        };
        self.selection = None;
        self.interaction.clear_selection();
        self.wire_start = None;
        self.board_outline_start = None;
        self.tool = ElectronicsTool::Select;
        self.placement_template = None;
        self.placement_drag_active = false;
        self.placement_preview = None;
        self.pointer_world = None;
        self.rebuild_scene_internal();
        if let Some(component_id) = cross_probe_component {
            let index = match surface {
                CadSurfaceKind::Schematic => self
                    .schematic
                    .components
                    .iter()
                    .position(|component| component.id == component_id),
                CadSurfaceKind::Pcb => self
                    .pcb
                    .components
                    .iter()
                    .position(|component| component.component_id == component_id),
            };
            if let Some(index) = index {
                self.select_component(index);
            }
        }
        self.touch_ui();
    }

    pub fn interaction(&self) -> &CadInteractionState {
        &self.interaction
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn ui_revision(&self) -> u64 {
        self.ui_revision
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn context_menu_position(&self) -> Option<[f32; 2]> {
        self.context_menu_position
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    pub fn can_rotate_selection(&self) -> bool {
        let Some(selection) = self.selection else {
            return false;
        };
        if !matches!(
            selection.kind,
            ElectronicsSelectionKind::Component | ElectronicsSelectionKind::Pin
        ) {
            return false;
        }
        match self.active_surface {
            CadSurfaceKind::Schematic => self
                .schematic
                .components
                .iter()
                .any(|component| component.id == selection.source_id && !component.locked),
            CadSurfaceKind::Pcb => self.pcb.components.iter().any(|component| {
                component.component_id == selection.source_id && !component.locked
            }),
        }
    }

    pub fn set_tool(&mut self, tool: ElectronicsTool) {
        if self.tool != tool {
            self.tool = tool;
            if tool != ElectronicsTool::Place {
                self.placement_template = None;
                self.placement_drag_active = false;
                self.placement_preview = None;
            }
            if tool != ElectronicsTool::BoardOutline {
                self.board_outline_start = None;
            }
            if tool != ElectronicsTool::Wire {
                self.wire_start = None;
                self.rebuild_scene_with_drc();
            }
            self.touch_ui();
        }
    }

    pub fn render_options(&mut self, canvas: EditorRect) -> CadSurfaceOptions {
        self.ensure_camera(canvas);
        let size = Vec2::new(canvas.width.max(1.0), canvas.height.max(1.0));
        let mut options = CadSurfaceOptions::default();
        options.clear_color = [16, 20, 28, 255];
        options.world_bounds = Some(self.camera.world_bounds(size));
        options.selected_source_ids = self
            .selection
            .map(|selection| vec![*selection.source_id.as_bytes()])
            .unwrap_or_default();
        options.grid_step = self.grid_step;
        options.grid_color = [140, 175, 220, 18];
        options.major_grid_color = [170, 205, 250, 36];
        options.axis_color = [185, 215, 255, 48];
        options.grid_color[3] = (f32::from(options.grid_color[3]) * self.grid_opacity)
            .round()
            .clamp(0.0, 255.0) as u8;
        options.major_grid_color[3] = (f32::from(options.major_grid_color[3]) * self.grid_opacity)
            .round()
            .clamp(0.0, 255.0) as u8;
        options.axis_color[3] = (f32::from(options.axis_color[3]) * self.grid_opacity)
            .round()
            .clamp(0.0, 255.0) as u8;
        options.show_grid = self.grid_visible;
        options.show_labels = self.labels_visible;
        options
    }

    pub fn fit_view(&mut self) {
        self.camera_initialized = false;
        self.touch();
    }

    pub fn toggle_grid(&mut self) {
        self.grid_visible = !self.grid_visible;
        self.touch_ui();
    }

    pub fn toggle_labels(&mut self) {
        self.labels_visible = !self.labels_visible;
        self.touch_ui();
    }

    pub fn sync_pcb_from_schematic(&mut self) -> raf_electronics::PcbSyncSummary {
        let before = self.pcb.clone();
        let summary = self.pcb.sync_from_schematic(&self.schematic);
        if self.pcb != before {
            self.history.record(
                ElectronicsDocumentSnapshot {
                    schematic: self.schematic.clone(),
                    pcb: before,
                },
                &self.snapshot(),
            );
            self.dirty = true;
            self.rebuild_scene();
        }
        self.touch_ui();
        summary
    }

    pub fn run_drc(&mut self) {
        self.cancel_analysis();
        let report = self.schematic.run_drc();
        self.drc_lines = report.to_string_list();
        self.drc_report = Some(report);
        self.rebuild_scene_with_drc();
        self.touch_ui();
    }

    pub fn run_simulation(&mut self) {
        self.cancel_analysis();
        let result = self.schematic.simulate_dc();
        let mut lines = vec![format!(
            "Status: {}",
            if result.converged {
                "converged"
            } else {
                "not converged"
            }
        )];
        lines.push(format!("Nodes solved: {}", result.node_voltages.len()));
        lines.push(format!(
            "Components evaluated: {}",
            result.component_currents.len()
        ));
        lines.extend(result.messages);
        self.simulation_lines = lines;
        self.touch_ui();
    }

    fn set_selected_value(&mut self, value: &str) -> bool {
        let Some(selection) = self.selection else {
            return false;
        };
        if !matches!(
            selection.kind,
            ElectronicsSelectionKind::Component | ElectronicsSelectionKind::Pin
        ) {
            return false;
        }
        if self.active_surface == CadSurfaceKind::Pcb {
            let before = self.snapshot();
            let Some(component) = self
                .pcb
                .components
                .iter_mut()
                .find(|component| component.component_id == selection.source_id)
            else {
                return false;
            };
            if component.locked || component.value == value {
                return false;
            }
            component.value = value.to_string();
            if let Some(schematic_component) = self
                .schematic
                .components
                .iter_mut()
                .find(|component| component.id == selection.source_id)
            {
                schematic_component.value = value.to_string();
                schematic_component.sync_sim_model_from_value();
            }
            self.history.record(before, &self.snapshot());
            self.dirty = true;
            self.rebuild_scene();
            self.touch_ui();
            return true;
        }
        let before = self.snapshot();
        let Some(component) = self
            .schematic
            .components
            .iter_mut()
            .find(|component| component.id == selection.source_id)
        else {
            return false;
        };
        if component.locked || component.value == value {
            return false;
        }
        component.value = value.to_string();
        component.sync_sim_model_from_value();
        self.history.record(before, &self.snapshot());
        self.dirty = true;
        self.rebuild_scene();
        self.touch_ui();
        true
    }

    fn next_net_name(&self) -> String {
        format!("N{:03}", self.schematic.wires.len() + 1)
    }

    fn snapshot(&self) -> ElectronicsDocumentSnapshot {
        ElectronicsDocumentSnapshot {
            schematic: self.schematic.clone(),
            pcb: self.pcb.clone(),
        }
    }

    fn ensure_camera(&mut self, canvas: EditorRect) {
        if self.camera_initialized {
            return;
        }
        self.camera_initialized = true;
        let size = Vec2::new(canvas.width.max(1.0), canvas.height.max(1.0));
        let Some((min, max)) = scene_bounds(&self.scene) else {
            self.camera = CadCamera::default();
            return;
        };
        let span = (max - min).max(Vec2::splat(40.0));
        let padding = Vec2::splat(120.0);
        self.camera.center = (min + max) * 0.5;
        self.camera.zoom = (size.x / (span.x + padding.x))
            .min(size.y / (span.y + padding.y))
            .clamp(MIN_ZOOM, MAX_ZOOM);
    }

    fn rebuild_scene(&mut self) {
        self.cancel_analysis();
        self.drc_report = None;
        self.drc_lines.clear();
        self.simulation_lines.clear();
        self.rebuild_scene_internal();
    }

    fn rebuild_scene_with_drc(&mut self) {
        self.rebuild_scene_internal();
    }

    fn start_analysis(&mut self, kind: AnalysisKind) {
        if let Some(task) = self.analysis_task.take() {
            task.cancel();
        }
        match kind {
            AnalysisKind::Drc => {
                self.drc_report = None;
                self.drc_lines.clear();
            }
            AnalysisKind::Simulation => {
                self.simulation_lines.clear();
            }
        }
        self.analysis_task = Some(AnalysisTask::spawn(kind, self.schematic.clone()));
        self.touch_ui();
    }

    fn rebuild_scene_internal(&mut self) {
        self.scene = match self.active_surface {
            CadSurfaceKind::Schematic => self
                .drc_report
                .as_ref()
                .map(|report| CadScene::from_schematic_with_drc(&self.schematic, report))
                .unwrap_or_else(|| CadScene::from_schematic(&self.schematic)),
            CadSurfaceKind::Pcb => CadScene::from_pcb(&self.pcb),
        };
        if let (Some(template_index), Some(position)) =
            (self.placement_template, self.placement_preview)
        {
            if let Some(template) = self.library.components.get(template_index) {
                let size = raf_electronics::cad_scene::schematic_component_size(&template.template);
                let min = position - size * 0.5;
                let max = position + size * 0.5;
                self.scene.objects.push(CadObject {
                    id: "component-placement-preview".to_string(),
                    source_id: None,
                    kind: CadObjectKind::Component,
                    layer: CadLayerKind::Overlay,
                    pick_priority: CadPickPriority::Overlay,
                    rect: Some(raf_electronics::CadRect::new(position, size)),
                    points: Vec::new(),
                    line_paths: vec![vec![
                        min,
                        Vec2::new(max.x, min.y),
                        max,
                        Vec2::new(min.x, max.y),
                        min,
                    ]],
                    label: None,
                    net: None,
                    color_rgba: [255, 172, 64, 74],
                });
            }
        }
        if self.active_surface == CadSurfaceKind::Pcb {
            if let (Some(start), Some(end)) = (self.board_outline_start, self.pointer_world) {
                let min = start.min(end);
                let max = self.snap_world(start.max(end));
                self.scene.objects.push(CadObject {
                    id: "board-outline-preview".to_string(),
                    source_id: None,
                    kind: CadObjectKind::BoardOutline,
                    layer: CadLayerKind::Overlay,
                    pick_priority: CadPickPriority::Overlay,
                    rect: None,
                    points: vec![
                        Vec2::new(min.x, min.y),
                        Vec2::new(max.x, min.y),
                        Vec2::new(max.x, max.y),
                        Vec2::new(min.x, max.y),
                        Vec2::new(min.x, min.y),
                    ],
                    line_paths: Vec::new(),
                    label: None,
                    net: None,
                    color_rgba: [255, 172, 64, 220],
                });
            }
        }
        if let (Some(start), Some(end)) = (self.wire_start, self.pointer_world) {
            let pin_hit = self.pin_endpoint_at(end);
            let is_snapped = pin_hit.is_some();
            let end_point = pin_hit
                .map(|(_, position, _)| position)
                .unwrap_or_else(|| self.snap_world(end));
            let points = orthogonal_wire_points(start.world, end_point);
            let wire_color = if is_snapped {
                [0, 220, 255, 230]
            } else {
                [255, 172, 64, 190]
            };
            self.scene.objects.push(CadObject {
                id: "wire-preview".to_string(),
                source_id: None,
                kind: CadObjectKind::Wire,
                layer: CadLayerKind::Overlay,
                pick_priority: CadPickPriority::Overlay,
                rect: None,
                points,
                line_paths: Vec::new(),
                label: None,
                net: None,
                color_rgba: wire_color,
            });
            if is_snapped {
                self.scene.objects.push(CadObject {
                    id: "pin-snap-target".to_string(),
                    source_id: None,
                    kind: CadObjectKind::Pin,
                    layer: CadLayerKind::Overlay,
                    pick_priority: CadPickPriority::Overlay,
                    rect: None,
                    points: vec![end_point],
                    line_paths: Vec::new(),
                    label: None,
                    net: None,
                    color_rgba: [0, 220, 255, 255],
                });
            }
        }
    }

    fn snap_world(&self, value: Vec2) -> Vec2 {
        if self.snap_enabled {
            snap_to_grid(value, self.grid_step)
        } else {
            value
        }
    }

    fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1).max(1);
    }

    fn touch_ui(&mut self) {
        self.ui_revision = self.ui_revision.wrapping_add(1).max(1);
        self.touch();
    }
}

fn component_asset_key(kind_label: &str) -> &'static str {
    match kind_label {
        "Resistor" => "electronics://library/resistor.png",
        "Capacitor" => "electronics://library/capacitor.png",
        "LED" => "electronics://library/led.png",
        "Magnet" => "electronics://library/magnet.png",
        "Battery" => "electronics://library/battery.png",
        "Ground" => "electronics://library/ground.png",
        _ => "electronics://library/generic.png",
    }
}

fn anchor_component_id(anchor: WireAnchor) -> Uuid {
    match anchor {
        WireAnchor::Pin { component_id, .. } => component_id,
        WireAnchor::Point(_) => Uuid::nil(),
    }
}

fn scene_bounds(scene: &CadScene) -> Option<(Vec2, Vec2)> {
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    let mut found = false;
    for object in &scene.objects {
        if let Some(rect) = object.rect {
            let half = rect.size.abs() * 0.5;
            min = min.min(rect.center - half);
            max = max.max(rect.center + half);
            found = true;
        }
        for point in &object.points {
            min = min.min(*point);
            max = max.max(*point);
            found = true;
        }
        for path in &object.line_paths {
            for point in path {
                min = min.min(*point);
                max = max.max(*point);
                found = true;
            }
        }
    }
    found.then_some((min, max))
}

fn snap_to_grid(value: Vec2, step: f32) -> Vec2 {
    let step = step.max(f32::EPSILON);
    Vec2::new(
        (value.x / step).round() * step,
        (value.y / step).round() * step,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_electronics::component::ElectronicComponent;

    #[test]
    fn camera_zoom_keeps_cursor_world_point_stable() {
        let camera = CadCamera {
            center: Vec2::new(100.0, 50.0),
            zoom: 2.0,
        };
        let size = Vec2::new(800.0, 600.0);
        let cursor = Vec2::new(140.0, 220.0);
        let world = camera.world_from_screen(cursor, size);
        let mut changed = camera;
        changed.zoom_at(2.0, cursor, size);
        assert!(changed.world_from_screen(cursor, size).distance(world) < 0.001);
    }

    #[test]
    fn empty_editor_starts_with_schematic_scene() {
        let editor = NativeElectronicsEditor::empty("Test");
        assert_eq!(editor.scene().surface, CadSurfaceKind::Schematic);
        assert!(editor.scene().objects.is_empty());
    }

    #[test]
    fn camera_motion_does_not_invalidate_retained_ui() {
        let mut editor = NativeElectronicsEditor::empty("Test");
        let ui_revision = editor.ui_revision();
        let render_revision = editor.revision();

        editor.zoom_in();

        assert_eq!(editor.ui_revision(), ui_revision);
        assert_ne!(editor.revision(), render_revision);
    }

    #[test]
    fn analysis_commands_retain_backend_results_for_the_dock() {
        let mut editor = NativeElectronicsEditor::empty("Test");

        assert!(editor.apply_ui_command("electronics.analysis.drc"));
        assert!(!editor.drc_lines().is_empty());

        assert!(editor.apply_ui_command("electronics.analysis.simulation"));
        assert!(!editor.simulation_lines().is_empty());
    }

    #[test]
    fn background_analysis_exposes_running_and_cancelled_states() {
        let mut editor = NativeElectronicsEditor::empty("Test");
        editor.run_drc();

        editor.start_drc_analysis();

        assert!(editor.analysis_running());
        assert!(editor.drc_report().is_none());
        assert!(editor.drc_lines().is_empty());

        editor.cancel_analysis();

        assert!(!editor.analysis_running());
        assert_eq!(editor.drc_lines(), &["Status: cancelled".to_string()]);
    }

    #[test]
    fn document_rebuild_invalidates_cached_analysis_text() {
        let mut editor = NativeElectronicsEditor::empty("Test");
        editor.run_drc();
        editor.run_simulation();
        assert!(!editor.drc_lines().is_empty());
        assert!(!editor.simulation_lines().is_empty());

        editor.rebuild_scene();

        assert!(editor.drc_lines().is_empty());
        assert!(editor.simulation_lines().is_empty());
        assert!(editor.drc_report().is_none());
    }

    #[test]
    fn surface_switch_preserves_component_selection_across_views() {
        let mut editor = NativeElectronicsEditor::empty("Test");
        editor
            .schematic
            .add_component(ElectronicComponent::resistor("10k"));
        let component_id = editor.schematic.components[0].id;

        assert!(editor.select_component(0));
        editor.set_surface(CadSurfaceKind::Pcb);
        assert_eq!(
            editor.selection().map(|selection| selection.source_id),
            Some(component_id)
        );
        assert_eq!(
            editor.selection().map(|selection| selection.kind),
            Some(ElectronicsSelectionKind::Component)
        );
        assert_eq!(
            editor
                .interaction()
                .selected
                .as_ref()
                .map(|selected| selected.source_id),
            Some(Some(component_id))
        );

        editor.set_surface(CadSurfaceKind::Schematic);
        assert_eq!(
            editor.selection().map(|selection| selection.source_id),
            Some(component_id)
        );
        assert_eq!(
            editor.selection().map(|selection| selection.kind),
            Some(ElectronicsSelectionKind::Component)
        );
        assert_eq!(
            editor
                .interaction()
                .selected
                .as_ref()
                .map(|selected| selected.source_id),
            Some(Some(component_id))
        );
    }

    #[test]
    fn library_selection_switches_to_placement_without_rebuilding_ui_per_frame() {
        let mut editor = NativeElectronicsEditor::empty("Test");
        let ui_revision = editor.ui_revision();

        assert!(editor.apply_ui_command("electronics.place.template.0"));
        assert_eq!(editor.tool(), ElectronicsTool::Place);
        assert!(editor.ui_revision() > ui_revision);
    }

    #[test]
    fn library_drag_only_commits_inside_the_canvas() {
        let mut editor = NativeElectronicsEditor::empty("Test");
        let canvas = EditorRect::new(10.0, 20.0, 240.0, 180.0);

        assert!(editor.begin_library_drag(0));
        assert_eq!(
            editor.placement_preview_asset_key(),
            Some("electronics://library/resistor.png")
        );
        assert!(!editor.finish_library_drag_at(Some([400.0, 400.0]), canvas));
        assert!(editor.schematic.components.is_empty());
        assert_eq!(editor.placement_preview_asset_key(), None);

        assert!(editor.begin_library_drag(0));
        assert!(editor.finish_library_drag_at(Some([40.0, 50.0]), canvas));
        assert_eq!(editor.schematic.components.len(), 1);
        assert!(!editor.placement_drag_active);
    }

    #[test]
    fn entering_pcb_syncs_an_empty_layout_from_the_schematic() {
        let mut editor = NativeElectronicsEditor::empty("Test");
        editor
            .schematic
            .add_component(ElectronicComponent::resistor("10k"));

        editor.set_surface(CadSurfaceKind::Pcb);

        assert_eq!(editor.pcb.components.len(), 1);
        assert_eq!(editor.scene.surface, CadSurfaceKind::Pcb);
        assert!(editor.is_dirty());
        assert!(editor.can_undo());
    }

    #[test]
    fn pcb_library_placement_creates_a_linked_schematic_component() {
        let mut editor = NativeElectronicsEditor::empty("Test");
        editor.set_surface(CadSurfaceKind::Pcb);
        editor.placement_template = Some(0);

        editor.place_component(Vec2::new(83.0, 117.0));

        assert_eq!(editor.schematic.components.len(), 1);
        assert_eq!(editor.pcb.components.len(), 1);
        assert_eq!(
            editor.pcb.components[0].component_id,
            editor.schematic.components[0].id
        );
        assert_eq!(editor.pcb.components[0].position, Vec2::new(80.0, 120.0));
    }

    #[test]
    fn pcb_route_tool_turns_selected_airwire_into_a_trace() {
        let mut editor = NativeElectronicsEditor::empty("Test");
        editor.set_surface(CadSurfaceKind::Pcb);
        editor.pcb.airwires.push(raf_electronics::PcbAirwire {
            net: "N001".to_string(),
            from_component_id: Uuid::new_v4(),
            from: Vec2::new(20.0, 20.0),
            to_component_id: Uuid::new_v4(),
            to: Vec2::new(80.0, 60.0),
        });
        editor.rebuild_scene();
        editor.interaction.selected = Some(raf_electronics::cad_interaction::CadSelection {
            object_id: "airwire:0".to_string(),
            source_id: None,
            kind: raf_electronics::CadObjectKind::Airwire,
            layer: CadLayerKind::Airwire,
        });

        assert!(editor.apply_ui_command("electronics.context.route"));
        assert_eq!(editor.pcb.traces.len(), 1);
        assert!(editor.pcb.airwires.is_empty());
        assert_eq!(
            editor.selection.map(|selection| selection.kind),
            Some(ElectronicsSelectionKind::Trace)
        );
    }

    #[test]
    fn inspector_value_commit_updates_simulation_model_and_history() {
        let mut editor = NativeElectronicsEditor::empty("Test");
        editor
            .schematic
            .add_component(ElectronicComponent::resistor("10k"));
        assert!(editor.select_component(0));

        assert!(editor.apply_ui_command("electronics.inspector.value.commit:22k"));
        assert_eq!(editor.schematic.components[0].value, "22k");
        match &editor.schematic.components[0].sim_model {
            raf_electronics::component::SimModel::Resistor { ohms } => {
                assert!((*ohms - 22_000.0).abs() < f64::EPSILON)
            }
            other => panic!("expected resistor model, got {other:?}"),
        }
        assert!(editor.can_undo());
    }
}
