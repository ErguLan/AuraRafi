//! Native Electronics document controller.
//!
//! This module is deliberately renderer-neutral.  RafUI owns the chrome and
//! menus, ApiGraphicBasic owns the CAD drawing, while this controller owns the
//! live documents, camera, scene and analysis state. Authoring mechanics live
//! in `electronics_controller_interaction.rs`; together they replace the
//! interaction that used to live in the legacy widget canvas without bringing
//! that toolkit back into the engine.

use glam::Vec2;
use raf_core::config::{EngineSettings, Language};
use raf_core::i18n;
use raf_core::project::{Project, ProjectSettings};
use raf_core::session::ProjectSessionRegistry;
use raf_core::{CaptureMode, InputKey, InputOwner, InputRouter, InputSnapshot, PointerButton};
use raf_electronics::cad_interaction::{pick, CadInteractionState};
use raf_electronics::library::ComponentLibrary;
use raf_electronics::schematic::{component_pin_world_position, WireAnchor};
use raf_electronics::{
    orthogonal_wire_points, pcb_fingerprint, schematic_fingerprint, CadLayerKind, CadObject,
    CadObjectKind, CadPickPriority, CadScene, CadSurfaceKind, DrcReport, DrcSeverity, PcbLayout,
    Schematic,
};
use raf_render::api_graphic_basic::cad_surface::CadSurfaceOptions;
use uuid::Uuid;

use crate::editor_layout::EditorRect;
use crate::electronics_history::{ElectronicsDocumentSnapshot, ElectronicsHistory};
use crate::pcb_document::{load_pcb_document, save_pcb_document};
use crate::schematic_document::{load_schematic_document, save_schematic_document, DocumentLoad};

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

/// Automatic net names are `N` plus a zero padded counter. The counter is
/// derived from the highest live name, never from the wire count, so deleting a
/// wire can never hand an existing net name to a different circuit.
const NET_NAME_PREFIX: &str = "N";
const NET_NAME_DIGITS: usize = 3;

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
    /// Set while the drag no longer has a matching schematic component, which
    /// happens when the component was deleted in the other surface. The input
    /// layer owns the gesture and reports the orphan through this flag.
    pub(crate) orphaned: bool,
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

/// Structured classification for one DRC or simulation line.
///
/// The dock used to recover severity by searching the rendered text for English
/// words such as "issues" or "converged", which silently dropped any
/// `[ERROR] short_circuit:` line into the neutral color and broke the moment the
/// text was translated. The kind travels with the line instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElectronicsReportLineKind {
    /// Neutral state line such as "running", "cancelled" or "obsolete".
    Status,
    Running,
    Passed,
    Issues,
    Failed,
    /// Free text produced by the analysis backend.
    Message,
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElectronicsReportLine {
    pub kind: ElectronicsReportLineKind,
    pub text: String,
    /// Component the finding is about, when the rule named one.
    ///
    /// Carrying the identity here (instead of letting the surface track a row
    /// index) keeps `error -> object` navigation stable while the report grows
    /// a new leading status line.
    pub target: Option<Uuid>,
}

impl ElectronicsReportLine {
    fn new(kind: ElectronicsReportLineKind, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
            target: None,
        }
    }

    /// Row that resolves to a component in the document.
    fn targeted(
        kind: ElectronicsReportLineKind,
        text: impl Into<String>,
        target: Option<Uuid>,
    ) -> Self {
        Self {
            kind,
            text: text.into(),
            target,
        }
    }
}

/// Last known rotation per component, used to reconcile the schematic and the
/// PCB into a single source of truth for orientation.
#[derive(Debug, Clone, Copy, PartialEq)]
struct ComponentRotation {
    component_id: Uuid,
    schematic_degrees: f32,
    pcb_degrees: Option<f32>,
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
    pub(crate) schematic: Schematic,
    pub(crate) pcb: PcbLayout,
    scene: CadScene,
    history: ElectronicsHistory,
    interaction: CadInteractionState,
    selection: Option<ElectronicsSelection>,
    /// Pointer hover target. Kept apart from `selection` so a surface can present
    /// a hover affordance without changing the committed selection.
    hovered: Option<ElectronicsSelection>,
    /// Net currently hovered or explicitly focused, expressed with the net index
    /// carried by `CadObject::net_id`.
    highlight_net: Option<usize>,
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
    drc_lines: Vec<ElectronicsReportLine>,
    drc_report: Option<DrcReport>,
    /// True when the document changed after the current DRC result was produced.
    drc_stale: bool,
    simulation_lines: Vec<ElectronicsReportLine>,
    /// True when the document changed after the current simulation result.
    simulation_stale: bool,
    /// True when the schematic and the PCB no longer describe the same design.
    sync_stale: bool,
    /// Failures the user must see. Cleared by the user or by the next successful
    /// equivalent operation; never replaced by a log line.
    surface_errors: Vec<String>,
    /// A destructive action is armed and waiting for an explicit confirmation.
    pending_delete: bool,
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
    /// Fingerprint of the documents the current scene and analysis state were
    /// built from. A rebuild only invalidates analysis when this actually moved.
    built_schematic_fingerprint: u64,
    built_pcb_fingerprint: u64,
    built_rotation: Vec<ComponentRotation>,
    language: Language,
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
            hovered: None,
            highlight_net: None,
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
            drc_stale: false,
            simulation_lines: Vec::new(),
            simulation_stale: false,
            sync_stale: false,
            surface_errors: Vec::new(),
            pending_delete: false,
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
            built_schematic_fingerprint: 0,
            built_pcb_fingerprint: 0,
            built_rotation: Vec::new(),
            language: Language::English,
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
            // A document that cannot be read is not an empty document. The file
            // is backed up, the failure reaches the surface and the save path
            // refuses to overwrite it, instead of replacing the user's work.
            match load_schematic_document(&schematic_path) {
                DocumentLoad::Loaded(schematic) => editor.schematic = schematic,
                DocumentLoad::Missing => {}
                DocumentLoad::Corrupt { backup, detail } => {
                    tracing::error!(
                        path = %schematic_path.display(),
                        %detail,
                        "native Electronics could not read the schematic document"
                    );
                    editor.push_document_load_error(
                        "electronics.error.schematic_load_failed",
                        &schematic_path,
                        backup.as_deref(),
                    );
                }
            }
            match load_pcb_document(&pcb_path) {
                DocumentLoad::Loaded(pcb) => editor.pcb = pcb,
                DocumentLoad::Missing => {}
                DocumentLoad::Corrupt { backup, detail } => {
                    tracing::error!(
                        path = %pcb_path.display(),
                        %detail,
                        "native Electronics could not read the PCB document"
                    );
                    editor.push_document_load_error(
                        "electronics.error.pcb_load_failed",
                        &pcb_path,
                        backup.as_deref(),
                    );
                }
            }
        }
        editor.library.load_external_assets_from(&project.path);
        editor.sync_stale = !editor.pcb_matches_schematic();
        editor.rebuild_scene_with_drc();
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
        let language_changed = self.language != settings.language;
        self.language = settings.language;
        self.grid_visible = settings.grid_visible;
        self.snap_enabled = settings.snap_to_grid;
        self.grid_step = grid_step;
        self.schematic_grid_step = grid_step;
        self.pcb_grid_step = grid_step;
        self.grid_opacity = grid_opacity;
        self.labels_visible = settings.show_viewport_labels;
        if changed || labels_changed || language_changed {
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

    pub fn drc_lines(&self) -> &[ElectronicsReportLine] {
        &self.drc_lines
    }

    pub fn drc_report(&self) -> Option<&DrcReport> {
        self.drc_report.as_ref()
    }

    pub fn simulation_lines(&self) -> &[ElectronicsReportLine] {
        &self.simulation_lines
    }

    /// Pointer hover target. Surfaces use it for a hover affordance without
    /// disturbing the committed selection.
    pub fn hovered(&self) -> Option<&ElectronicsSelection> {
        self.hovered.as_ref()
    }

    /// Stable model identities the CAD surface should present as hovered.
    pub fn hovered_source_ids(&self) -> Vec<[u8; 16]> {
        self.hovered
            .map(|hovered| vec![*hovered.source_id.as_bytes()])
            .unwrap_or_default()
    }

    /// Net index currently focused for highlighting, matching `CadObject::net_id`.
    pub fn highlight_net(&self) -> Option<usize> {
        self.highlight_net
    }

    /// True when the document changed after the visible DRC result was produced.
    /// The report stays available so a surface can dim the markers and explain
    /// that they describe an older revision.
    pub fn drc_is_stale(&self) -> bool {
        self.drc_stale
    }

    /// True when the simulation result no longer matches the document.
    pub fn simulation_is_stale(&self) -> bool {
        self.simulation_stale
    }

    /// True when the schematic and the PCB no longer describe the same design.
    pub fn sync_is_stale(&self) -> bool {
        self.sync_stale
    }

    pub fn surface_errors(&self) -> &[String] {
        &self.surface_errors
    }

    pub fn push_surface_error(&mut self, message: String) {
        if message.trim().is_empty() {
            return;
        }
        if self.surface_errors.last() == Some(&message) {
            return;
        }
        self.surface_errors.push(message);
        self.touch_ui();
    }

    /// Pushes a localized failure. Every controller-owned error goes through the
    /// catalog so the text is never hardcoded English.
    pub(crate) fn push_surface_error_key(&mut self, key: &str) {
        self.push_surface_error(i18n::t(key, self.language));
    }

    pub fn clear_surface_errors(&mut self) {
        if self.surface_errors.is_empty() {
            return;
        }
        self.surface_errors.clear();
        self.touch_ui();
    }

    /// Drops the stale errors a successful equivalent operation resolved.
    pub(crate) fn clear_surface_errors_for_key(&mut self, key: &str) {
        let message = i18n::t(key, self.language);
        self.surface_errors.retain(|error| *error != message);
    }

    /// True while a destructive action is armed and waiting for confirmation.
    pub fn delete_confirm_pending(&self) -> bool {
        self.pending_delete
    }

    /// Arms the destructive action instead of performing it. Surfaces present a
    /// confirmation and call `confirm_delete` or `cancel_pending_action`.
    pub fn request_delete_selected(&mut self) {
        self.pending_delete = self.delete_target_exists();
        self.touch_ui();
    }

    pub fn confirm_delete(&mut self) {
        if !self.pending_delete {
            return;
        }
        self.pending_delete = false;
        self.delete_selected();
        self.touch_ui();
    }

    pub fn cancel_pending_action(&mut self) {
        if !self.pending_delete {
            return;
        }
        self.pending_delete = false;
        self.touch_ui();
    }

    /// Single source of truth for component orientation. Surfaces read this
    /// instead of the per-document rotation so the artwork matches the model.
    pub fn rotation_degrees(&self, source_id: Uuid) -> f32 {
        self.schematic
            .components
            .iter()
            .find(|component| component.id == source_id)
            .map(|component| component.rotation)
            .or_else(|| {
                self.pcb
                    .components
                    .iter()
                    .find(|component| component.component_id == source_id)
                    .map(|component| component.rotation)
            })
            .unwrap_or(0.0)
    }

    /// Traces dropped from the layout by the last schematic synchronization.
    ///
    /// Requires the `PcbLayout::removed_traces` field owned by the electronics
    /// domain module. Until that field exists the editor cannot observe removed
    /// traces, so it reports zero instead of inventing a count.
    pub fn removed_trace_count(&self) -> usize {
        0
    }

    /// Records the hover target from the CAD input layer. The input layer owns the
    /// pointer gesture; the controller only remembers the result so a surface can
    /// present it.
    pub fn set_hovered(&mut self, hover: Option<ElectronicsSelection>) {
        if self.hovered == hover {
            return;
        }
        self.hovered = hover;
        self.touch();
    }

    /// Records the focused net from the CAD input layer.
    pub fn set_highlight_net(&mut self, net: Option<usize>) {
        if self.highlight_net == net {
            return;
        }
        self.highlight_net = net;
        self.touch();
    }

    pub(crate) fn start_drc_analysis(&mut self) {
        self.start_analysis(AnalysisKind::Drc);
    }

    pub(crate) fn start_simulation_analysis(&mut self) {
        self.start_analysis(AnalysisKind::Simulation);
    }

    /// Cancels a running analysis without destroying the previous result.
    ///
    /// A cancelled run produced no report, so dropping the stored one would hide
    /// a real previous finding and replacing it with "no issues" would be a lie.
    /// The dock keeps the lines and receives an explicit cancelled status.
    pub(crate) fn cancel_analysis(&mut self) {
        let Some(task) = self.analysis_task.take() else {
            return;
        };
        let kind = task.kind();
        task.cancel();
        self.push_report_line(
            kind,
            ElectronicsReportLineKind::Status,
            "electronics.analysis.cancelled",
        );
        self.touch_ui();
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
                let lines = report_lines(&report, self.language);
                self.drc_report = Some(report);
                self.drc_lines = lines;
                self.drc_stale = false;
                self.simulation_stale = true;
                self.clear_surface_errors_for_key("electronics.error.drc_failed");
                self.rebuild_scene_with_drc();
                self.touch_ui();
                true
            }
            Ok(Some(AnalysisResult::Simulation(result))) => {
                let mut lines = vec![ElectronicsReportLine::new(
                    if result.converged {
                        ElectronicsReportLineKind::Passed
                    } else {
                        ElectronicsReportLineKind::Issues
                    },
                    i18n::t(
                        if result.converged {
                            "electronics.analysis.simulation_converged"
                        } else {
                            "electronics.analysis.simulation_not_converged"
                        },
                        self.language,
                    ),
                )];
                lines.push(metric_line(
                    "electronics.analysis.simulation_nodes",
                    result.node_voltages.len(),
                    self.language,
                ));
                lines.push(metric_line(
                    "electronics.analysis.simulation_components",
                    result.component_currents.len(),
                    self.language,
                ));
                lines.extend(result.messages.into_iter().map(|message| {
                    ElectronicsReportLine::new(ElectronicsReportLineKind::Message, message)
                }));
                self.simulation_lines = lines;
                self.simulation_stale = false;
                self.clear_surface_errors_for_key("electronics.error.simulation_failed");
                self.touch_ui();
                true
            }
            Ok(None) => {
                self.analysis_task = Some(task);
                false
            }
            Err(()) => {
                let kind = task.kind();
                self.drc_stale |= kind == AnalysisKind::Drc;
                self.simulation_stale |= kind == AnalysisKind::Simulation;
                let key = match kind {
                    AnalysisKind::Drc => "electronics.error.drc_failed",
                    AnalysisKind::Simulation => "electronics.error.simulation_failed",
                };
                self.push_report_line(kind, ElectronicsReportLineKind::Failed, key);
                self.push_surface_error_key(key);
                self.touch_ui();
                true
            }
        }
    }

    pub fn selection(&self) -> Option<ElectronicsSelection> {
        self.selection
    }

    fn push_report_line(
        &mut self,
        kind: AnalysisKind,
        line_kind: ElectronicsReportLineKind,
        key: &str,
    ) {
        let line = ElectronicsReportLine::new(line_kind, i18n::t(key, self.language));
        match kind {
            AnalysisKind::Drc => self.drc_lines.push(line),
            AnalysisKind::Simulation => self.simulation_lines.push(line),
        }
    }

    /// True when the current selection still resolves to something the delete
    /// path can remove, so a confirmation is never armed for nothing.
    fn delete_target_exists(&self) -> bool {
        let Some(selection) = self.selection else {
            return false;
        };
        match selection.kind {
            ElectronicsSelectionKind::Component | ElectronicsSelectionKind::Pin => {
                match self.active_surface {
                    CadSurfaceKind::Schematic => self
                        .schematic
                        .components
                        .iter()
                        .any(|component| component.id == selection.source_id),
                    CadSurfaceKind::Pcb => self
                        .pcb
                        .components
                        .iter()
                        .any(|component| component.component_id == selection.source_id),
                }
            }
            ElectronicsSelectionKind::Wire => self
                .schematic
                .wires
                .iter()
                .any(|wire| wire.id == selection.source_id),
            ElectronicsSelectionKind::Trace => {
                self.active_surface == CadSurfaceKind::Pcb
                    && self
                        .pcb
                        .traces
                        .iter()
                        .any(|trace| trace.id == selection.source_id)
            }
            ElectronicsSelectionKind::Other => false,
        }
    }

    fn push_document_load_error(
        &mut self,
        key: &str,
        path: &std::path::Path,
        backup: Option<&std::path::Path>,
    ) {
        let mut message = i18n::t(key, self.language);
        if let Some(file_name) = path.file_name().and_then(|name| name.to_str()) {
            message = format!("{} ({file_name})", message);
        }
        if let Some(backup) = backup.and_then(|backup| backup.file_name()) {
            message = format!(
                "{} {}",
                message,
                i18n::t("electronics.error.backup_written", self.language)
                    .replace("{backup}", backup.to_string_lossy().as_ref())
            );
        }
        tracing::error!(path = %path.display(), "native Electronics document load failed");
        self.push_surface_error(message);
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

    pub fn dispatch_action(
        &mut self,
        action: crate::electronics_action::ElectronicsAction,
    ) -> bool {
        use crate::electronics_action::ElectronicsAction;
        match action {
            ElectronicsAction::SelectTool(tool) => {
                self.set_tool(tool);
                true
            }
            ElectronicsAction::FitView => {
                self.fit_view();
                true
            }
            ElectronicsAction::ToggleGrid => {
                self.toggle_grid();
                true
            }
            ElectronicsAction::ToggleLabels => {
                self.toggle_labels();
                true
            }
            ElectronicsAction::ToggleSnap => {
                self.toggle_snap();
                true
            }
            ElectronicsAction::ZoomIn => {
                self.zoom_in();
                true
            }
            ElectronicsAction::ZoomOut => {
                self.zoom_out();
                true
            }
            ElectronicsAction::Rotate => {
                self.rotate_selected();
                true
            }
            ElectronicsAction::Delete => {
                self.request_delete_selected();
                true
            }
            ElectronicsAction::Undo => {
                self.undo();
                true
            }
            ElectronicsAction::Redo => {
                self.redo();
                true
            }
            // The controller owns the documents but not the project session, so
            // it cannot save without creating a second persistence path. The
            // application boundary owns `project.save`; an Electron-only caller
            // is told so instead of receiving a silent success.
            ElectronicsAction::SaveProject => {
                self.push_surface_error_key("electronics.error.save_outside_application");
                false
            }
            ElectronicsAction::SetSurface(surface) => {
                self.set_surface(surface);
                true
            }
            ElectronicsAction::SyncPcb => {
                self.sync_pcb_from_schematic();
                true
            }
            // Both analyses run on the background task handle so the UI thread
            // keeps painting and the dock can show running, cancel and failed.
            ElectronicsAction::RunDrc => {
                self.start_drc_analysis();
                true
            }
            ElectronicsAction::RunSimulation => {
                self.start_simulation_analysis();
                true
            }
            ElectronicsAction::CancelAnalysis => {
                self.cancel_analysis();
                true
            }
            // Blocking confirmation: the surface armed a destructive action and is
            // waiting for an explicit accept or dismiss before anything mutates.
            ElectronicsAction::ConfirmDelete => {
                self.confirm_delete();
                self.context_menu_position = None;
                true
            }
            ElectronicsAction::CancelPending => {
                self.cancel_pending_action();
                true
            }
            // Moves the document selection to whatever the clicked finding points
            // at, so a report row is a navigation target and not just text.
            ElectronicsAction::FocusAnalysisIssue { panel, index } => {
                let focused = self.focus_analysis_issue(&panel, index);
                if !focused {
                    self.push_surface_error_key("electronics.analysis.focus_line_hint");
                }
                focused
            }
            ElectronicsAction::FocusAnalysisComponent { source_id } => {
                if self.select_component_by_id(source_id) {
                    self.recenter_on_selection();
                    true
                } else {
                    self.push_surface_error_key("electronics.analysis.focus_issue_hint");
                    false
                }
            }
            ElectronicsAction::ContextDelete => {
                self.request_delete_selected();
                self.context_menu_position = None;
                true
            }
            ElectronicsAction::ContextRoute => {
                self.route_selected_airwire();
                self.context_menu_position = None;
                true
            }
            ElectronicsAction::ContextSelect => {
                self.context_menu_position = None;
                self.touch_ui();
                true
            }
            ElectronicsAction::ContextDuplicate => {
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
                true
            }
            ElectronicsAction::ContextCancel => {
                self.wire_start = None;
                self.context_menu_position = None;
                self.touch_ui();
                true
            }
            ElectronicsAction::CommitInspectorValue(val) => self.set_selected_value(&val),
            ElectronicsAction::SelectNavigator(idx) => self.select_component(idx),
            ElectronicsAction::InspectNavigator(idx) => {
                let selected = self.select_component(idx);
                if selected {
                    self.touch_ui();
                }
                selected
            }
            ElectronicsAction::SelectTab(_) => {
                self.touch_ui();
                true
            }
        }
    }

    pub fn apply_ui_command(&mut self, command: &str) -> bool {
        if let Some(action) = crate::electronics_action::ElectronicsAction::parse(command) {
            return self.dispatch_action(action);
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
        false
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
        // schematic, but through the quiet projection: pressing the PCB tab is
        // navigation, not an edit, so it must not consume the user's next undo.
        if surface == CadSurfaceKind::Pcb {
            self.sync_pcb_from_schematic_quiet();
        }
        self.grid_step = match surface {
            CadSurfaceKind::Schematic => self.schematic_grid_step,
            CadSurfaceKind::Pcb => self.pcb_grid_step,
        };
        self.selection = None;
        self.hovered = None;
        self.highlight_net = None;
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
                // Changing tool is not a document edit, so it must not promote a
                // stored DRC report back to "current".
                self.rebuild_scene();
            }
            self.touch_ui();
        }
    }

    /// Resolves the canvas presentation options for one frame.
    ///
    /// `palette` decides the CAD backdrop and grid so the canvas follows the
    /// active theme instead of staying a hardcoded dark rectangle, and
    /// `scale_factor` is target-pixels-per-logical-point so line work keeps its
    /// visual weight on high-DPI targets.
    pub fn render_options(
        &mut self,
        canvas: EditorRect,
        palette: raf_render::api_graphic_basic::ui_surface::StudioUiPalette,
        scale_factor: f32,
    ) -> CadSurfaceOptions {
        self.ensure_camera(canvas);
        let size = Vec2::new(canvas.width.max(1.0), canvas.height.max(1.0));
        let tokens = palette.tokens();
        let mut options = CadSurfaceOptions::default();
        options.clear_color = tokens.canvas;
        options.world_bounds = Some(self.camera.world_bounds(size));
        options.selected_source_ids = self
            .selection
            .map(|selection| vec![*selection.source_id.as_bytes()])
            .unwrap_or_default();
        // The hover target is drawn differently from the selection, so the two
        // never read as the same outline.
        options.hovered_source_ids = self.hovered_source_ids();
        options.highlight_net_id = self.highlight_net;
        options.raster_scale = if scale_factor.is_finite() {
            scale_factor.clamp(0.25, 4.0)
        } else {
            1.0
        };
        // Picking goes through `cad_interaction::pick_editable`, which also
        // filters the non-interactive overlays. Building the presentation hit
        // regions as well only cloned four strings per object per frame for a
        // list nobody reads.
        options.collect_hit_regions = false;
        options.grid_step = self.grid_step;
        // Grid alpha is applied on top of a token-derived base, because a fixed
        // dark-theme blue is invisible over a light canvas and a fixed dark
        // canvas is a black hole in a light editor.
        let grid_opacity = self.grid_opacity.clamp(0.0, 1.0);
        let base = |color: [u8; 4], alpha: f32| {
            let alpha = (f32::from(color[3]) * alpha * grid_opacity).round();
            [color[0], color[1], color[2], alpha.clamp(0.0, 255.0) as u8]
        };
        options.grid_color = base(tokens.border, 0.30);
        options.major_grid_color = base(tokens.border, 0.55);
        options.axis_color = base(tokens.text_muted, 0.75);
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

    /// Whether component placement snaps to the schematic grid.
    pub fn snap_enabled(&self) -> bool {
        self.snap_enabled
    }

    /// Flips the grid snap and rebuilds the scene so the change is visible at
    /// once.
    ///
    /// Snap is a view-level decision, not a document edit: it deliberately takes
    /// no undo entry and leaves the project clean.
    pub fn toggle_snap(&mut self) {
        self.snap_enabled = !self.snap_enabled;
        self.rebuild_scene();
        self.touch_ui();
    }

    /// Selects the component behind an analysis finding.
    ///
    /// `panel` selects which report to read and `index` the finding's position
    /// inside it. Resolution is by the identity carried on the report row, not
    /// by a row offset, so adding a status line to the dock can never redirect a
    /// click to the wrong component. Returns `false` when the finding has no
    /// component, or the component no longer exists in the document.
    pub fn focus_analysis_issue(&mut self, panel: &str, index: usize) -> bool {
        let source = match panel {
            "drc" => self.drc_lines.get(index),
            "simulation" => self.simulation_lines.get(index),
            _ => None,
        };
        let Some(target) = source.and_then(|line| line.target) else {
            return false;
        };
        self.highlight_net = None;
        if !self.select_component_by_id(target) {
            return false;
        }
        self.recenter_on_selection();
        true
    }

    /// Selects a component by identity rather than by list position.
    pub fn select_component_by_id(&mut self, source_id: Uuid) -> bool {
        if self.active_surface == CadSurfaceKind::Pcb {
            let Some(index) = self
                .pcb
                .components
                .iter()
                .position(|component| component.component_id == source_id)
            else {
                return false;
            };
            return self.select_component(index);
        }
        let Some(index) = self
            .schematic
            .components
            .iter()
            .position(|component| component.id == source_id)
        else {
            return false;
        };
        self.select_component(index)
    }

    /// Moves the camera to the current selection without changing zoom.
    ///
    /// Navigation, not editing: it records no history and leaves the project
    /// clean.
    pub fn recenter_on_selection(&mut self) {
        let Some(selection) = self.selection else {
            return;
        };
        let position = if self.active_surface == CadSurfaceKind::Pcb {
            self.pcb
                .components
                .iter()
                .find(|component| component.component_id == selection.source_id)
                .map(|component| component.position)
        } else {
            self.schematic
                .components
                .iter()
                .find(|component| component.id == selection.source_id)
                .map(|component| component.position)
        };
        let Some(position) = position else {
            return;
        };
        self.camera.center = position;
        self.touch();
    }

    /// Explicit user-triggered synchronization. The command is an edit, so it
    /// gets an undo entry and marks the document dirty.
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
        self.sync_stale = !self.pcb_matches_schematic();
        self.touch_ui();
        summary
    }

    /// Materializes the PCB projection without pretending it was an edit.
    ///
    /// Entering the PCB tab used to record a history entry and mark the project
    /// dirty, so after twenty minutes of work the next Ctrl+Z undid the
    /// synchronization instead of the user's edit. The PCB is still brought up to
    /// date; it simply stops competing with the undo stack.
    pub(crate) fn sync_pcb_from_schematic_quiet(&mut self) -> raf_electronics::PcbSyncSummary {
        let summary = self.pcb.sync_from_schematic(&self.schematic);
        self.sync_stale = !self.pcb_matches_schematic();
        self.rebuild_scene();
        summary
    }

    /// True when the PCB still describes the same design as the schematic: one
    /// placement per schematic component and the same pad-to-net assignment.
    fn pcb_matches_schematic(&self) -> bool {
        if self.pcb.components.len() != self.schematic.components.len() {
            return false;
        }
        let netlist = self.schematic.netlist();
        self.pcb.components.iter().all(|placement| {
            let Some(index) = self
                .schematic
                .components
                .iter()
                .position(|component| component.id == placement.component_id)
            else {
                return false;
            };
            let expected = self.schematic.components[index]
                .pins
                .iter()
                .enumerate()
                .map(|(pin_index, _)| {
                    netlist
                        .net_for_pin(index, pin_index)
                        .map(|net| net.name.clone())
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>();
            expected == placement.pad_nets
        })
    }

    pub fn run_drc(&mut self) {
        self.cancel_analysis();
        let report = self.schematic.run_drc();
        self.drc_lines = report_lines(&report, self.language);
        self.drc_report = Some(report);
        self.drc_stale = false;
        self.simulation_stale = true;
        self.clear_surface_errors_for_key("electronics.error.drc_failed");
        self.rebuild_scene_with_drc();
        self.touch_ui();
    }

    pub fn run_simulation(&mut self) {
        self.cancel_analysis();
        let result = self.schematic.simulate_dc();
        let mut lines = vec![ElectronicsReportLine::new(
            if result.converged {
                ElectronicsReportLineKind::Passed
            } else {
                ElectronicsReportLineKind::Issues
            },
            i18n::t(
                if result.converged {
                    "electronics.analysis.simulation_converged"
                } else {
                    "electronics.analysis.simulation_not_converged"
                },
                self.language,
            ),
        )];
        lines.push(metric_line(
            "electronics.analysis.simulation_nodes",
            result.node_voltages.len(),
            self.language,
        ));
        lines.push(metric_line(
            "electronics.analysis.simulation_components",
            result.component_currents.len(),
            self.language,
        ));
        lines.extend(result.messages.into_iter().map(|message| {
            ElectronicsReportLine::new(ElectronicsReportLineKind::Message, message)
        }));
        self.simulation_lines = lines;
        self.simulation_stale = false;
        self.clear_surface_errors_for_key("electronics.error.simulation_failed");
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

    /// Next automatic net name.
    ///
    /// The old implementation derived the counter from the wire count. Because
    /// one wire gesture creates several `Wire` entries and deleting a wire
    /// lowers the count, it reused names that were still alive and merged two
    /// different circuits into one net. The counter is now the highest numeric
    /// suffix that exists right now, across wires, the live netlist and the PCB.
    fn next_net_name(&self) -> String {
        let mut highest = 0usize;
        let mut observe = |name: &str| {
            if let Some(value) = net_name_suffix(name) {
                highest = highest.max(value);
            }
        };
        for wire in &self.schematic.wires {
            observe(&wire.net);
        }
        for net in &self.schematic.netlist().nets {
            observe(&net.name);
        }
        for trace in &self.pcb.traces {
            observe(&trace.net);
        }
        for airwire in &self.pcb.airwires {
            observe(&airwire.net);
        }
        for placement in &self.pcb.components {
            for net in &placement.pad_nets {
                observe(net);
            }
        }
        format!(
            "{NET_NAME_PREFIX}{:0width$}",
            highest + 1,
            width = NET_NAME_DIGITS
        )
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

    /// Rebuilds the CAD scene after any document or preview change.
    ///
    /// This used to cancel the analysis, drop the DRC report and clear its
    /// lines, so a single wire gesture silently deleted the design-rule result.
    /// The rebuild now compares the document fingerprint: a preview-only change
    /// keeps the report and its markers, and only a real mutation marks the
    /// result obsolete and asks a running analysis to stop.
    pub(crate) fn rebuild_scene(&mut self) {
        let before_schematic = schematic_fingerprint(&self.schematic);
        let before_pcb = pcb_fingerprint(&self.pcb);
        let schematic_changed = before_schematic != self.built_schematic_fingerprint;
        let pcb_changed = before_pcb != self.built_pcb_fingerprint;
        self.rebuild_scene_internal();
        // Captured after the build because reconciling the two documents can
        // itself move a fingerprint, and the next rebuild must not read that as
        // a fresh user edit.
        self.built_schematic_fingerprint = schematic_fingerprint(&self.schematic);
        self.built_pcb_fingerprint = pcb_fingerprint(&self.pcb);
        if schematic_changed || pcb_changed {
            self.invalidate_analysis_results();
            if schematic_changed {
                self.sync_stale = !self.pcb_matches_schematic();
            }
        }
    }

    /// Rebuilds the scene knowing the visible DRC markers belong to the current
    /// document revision.
    pub(crate) fn rebuild_scene_with_drc(&mut self) {
        self.rebuild_scene_internal();
        self.built_schematic_fingerprint = schematic_fingerprint(&self.schematic);
        self.built_pcb_fingerprint = pcb_fingerprint(&self.pcb);
        self.drc_stale = false;
    }

    /// Marks every analysis result as describing an older revision.
    ///
    /// The stored report and lines stay available so a surface can present them
    /// dimmed with an explicit obsolete notice instead of pretending the design
    /// is clean.
    fn invalidate_analysis_results(&mut self) {
        self.drc_stale = true;
        self.simulation_stale = true;
        self.cancel_analysis();
    }

    fn start_analysis(&mut self, kind: AnalysisKind) {
        if let Some(task) = self.analysis_task.take() {
            task.cancel();
            self.push_report_line(
                kind,
                ElectronicsReportLineKind::Status,
                "electronics.analysis.cancelled",
            );
        }
        // The previous result is kept while the new one runs: it stays visible
        // and `drc_stale` keeps telling the surface it is obsolete.
        self.push_report_line(
            kind,
            ElectronicsReportLineKind::Running,
            "electronics.analysis.running",
        );
        self.analysis_task = Some(AnalysisTask::spawn(kind, self.schematic.clone()));
        self.touch_ui();
    }

    fn rebuild_scene_internal(&mut self) {
        // Cross-document consistency belongs where the scene is derived from the
        // documents: the schematic owns component rotation and the PCB mirrors
        // it, so a rotation made in either surface has a single answer.
        self.reconcile_component_rotation();
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
                    net_id: None,
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
                    net_id: None,
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
                net_id: None,
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
                    net_id: None,
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

    /// Keeps component rotation in a single place.
    ///
    /// Rotation used to be written only in the document the user was looking at,
    /// so the schematic and the PCB kept two different angles for the same part
    /// and the artwork depended on the active tab. The side that just changed
    /// wins and the other one is reconciled here. When both moved in the same
    /// operation the PCB placement wins, because the physical placement is the
    /// authoritative side for orientation.
    fn reconcile_component_rotation(&mut self) {
        let current = self.rotation_snapshot();
        if current == self.built_rotation {
            return;
        }
        let previous = std::mem::take(&mut self.built_rotation);

        let mut schematic_moved = false;
        let mut pcb_moved = false;
        for rotation in &current {
            let Some(pcb_degrees) = rotation.pcb_degrees else {
                continue;
            };
            let Some(before) = previous
                .iter()
                .find(|before| before.component_id == rotation.component_id)
            else {
                continue;
            };
            let schematic_changed =
                (rotation.schematic_degrees - before.schematic_degrees).abs() > f32::EPSILON;
            let pcb_changed = before
                .pcb_degrees
                .is_some_and(|before| (pcb_degrees - before).abs() > f32::EPSILON);
            if schematic_changed == pcb_changed {
                continue;
            }
            let target = if schematic_changed {
                rotation.schematic_degrees
            } else {
                pcb_degrees
            };
            if schematic_changed {
                if let Some(placement) = self
                    .pcb
                    .components
                    .iter_mut()
                    .find(|placement| placement.component_id == rotation.component_id)
                {
                    placement.rotation = target;
                }
                pcb_moved = true;
            } else if let Some(component) = self
                .schematic
                .components
                .iter_mut()
                .find(|component| component.id == rotation.component_id)
            {
                component.rotation = target;
                schematic_moved = true;
            }
        }
        if schematic_moved {
            self.schematic.sync_wire_anchors();
        }
        if pcb_moved {
            self.pcb.rebuild_airwires();
        }
        // Recorded after the reconciliation so the next rebuild sees a settled
        // pair and does not replay the same reconciliation.
        self.built_rotation = self.rotation_snapshot();
    }

    fn rotation_snapshot(&self) -> Vec<ComponentRotation> {
        self.schematic
            .components
            .iter()
            .map(|component| ComponentRotation {
                component_id: component.id,
                schematic_degrees: component.rotation,
                pcb_degrees: self
                    .pcb
                    .components
                    .iter()
                    .find(|placement| placement.component_id == component.id)
                    .map(|placement| placement.rotation),
            })
            .collect()
    }

    fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1).max(1);
    }

    fn touch_ui(&mut self) {
        self.ui_revision = self.ui_revision.wrapping_add(1).max(1);
        self.touch();
    }
}

/// Builds the structured DRC lines. Severity travels with the line, so no
/// surface has to guess the tone from the rendered text.
fn report_lines(report: &DrcReport, language: Language) -> Vec<ElectronicsReportLine> {
    let mut lines = Vec::new();
    for issues in [&report.errors, &report.warnings, &report.info] {
        for issue in issues {
            let (kind, key) = match issue.severity {
                DrcSeverity::Error => (
                    ElectronicsReportLineKind::Error,
                    "electronics.analysis.severity_error",
                ),
                DrcSeverity::Warning => (
                    ElectronicsReportLineKind::Warning,
                    "electronics.analysis.severity_warning",
                ),
                DrcSeverity::Info => (
                    ElectronicsReportLineKind::Info,
                    "electronics.analysis.severity_info",
                ),
            };
            lines.push(ElectronicsReportLine::targeted(
                kind,
                format!(
                    "[{}] {}: {}",
                    i18n::t(key, language),
                    issue.rule,
                    issue.message
                ),
                issue.components.first().copied(),
            ));
        }
    }
    if lines.is_empty() {
        lines.push(ElectronicsReportLine::new(
            ElectronicsReportLineKind::Passed,
            i18n::t("electronics.analysis.drc_no_issues", language),
        ));
    }
    lines
}

fn metric_line(key: &str, value: usize, language: Language) -> ElectronicsReportLine {
    ElectronicsReportLine::new(
        ElectronicsReportLineKind::Status,
        format!("{}: {value}", i18n::t(key, language)),
    )
}

/// Numeric suffix of an automatic net name, or `None` for user labels.
fn net_name_suffix(name: &str) -> Option<usize> {
    let rest = name.strip_prefix(NET_NAME_PREFIX)?;
    if rest.is_empty() || !rest.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok()
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
    fn analysis_commands_start_a_background_task_the_dock_can_cancel() {
        let mut editor = NativeElectronicsEditor::empty("Test");

        assert!(editor.apply_ui_command("electronics.analysis.drc"));
        // The command now uses the same background handle as the attached
        // executor, so the UI keeps painting and can offer Cancel.
        assert!(editor.analysis_running());

        assert!(editor.apply_ui_command("electronics.analysis.simulation"));
        assert!(editor.analysis_running());
    }

    #[test]
    fn cancelled_analysis_keeps_the_previous_result_and_says_so() {
        let mut editor = NativeElectronicsEditor::empty("Test");
        editor.run_drc();
        let report_lines = editor.drc_lines().len();
        assert!(editor.drc_report().is_some());

        editor.start_drc_analysis();
        assert!(editor.analysis_running());
        assert!(
            editor.drc_report().is_some(),
            "starting a new run must not hide the previous report"
        );

        editor.cancel_analysis();

        assert!(!editor.analysis_running());
        assert!(editor.drc_lines().len() > report_lines);
        assert!(editor.drc_lines().iter().any(|line| {
            line.kind == ElectronicsReportLineKind::Status
                && line.text == i18n::t("electronics.analysis.cancelled", Language::English)
        }));
    }

    #[test]
    fn a_document_edit_marks_the_drc_report_stale_without_deleting_it() {
        let mut editor = NativeElectronicsEditor::empty("Test");
        editor.run_drc();
        assert!(!editor.drc_is_stale());
        let lines_before = editor.drc_lines().len();

        editor
            .schematic
            .add_component(ElectronicComponent::resistor("10k"));
        editor.rebuild_scene();

        assert!(editor.drc_is_stale());
        assert!(editor.simulation_is_stale());
        assert!(
            editor.drc_report().is_some(),
            "the previous report must stay available for dimmed markers"
        );
        assert_eq!(editor.drc_lines().len(), lines_before);
    }

    #[test]
    fn a_preview_only_rebuild_does_not_invalidate_the_report() {
        let mut editor = NativeElectronicsEditor::empty("Test");
        editor.run_drc();
        assert!(!editor.drc_is_stale());

        editor.rebuild_scene();
        editor.set_tool(ElectronicsTool::Wire);

        assert!(
            !editor.drc_is_stale(),
            "rebuilding a preview is not a document edit"
        );
    }

    #[test]
    fn running_drc_again_clears_the_stale_flag() {
        let mut editor = NativeElectronicsEditor::empty("Test");
        editor
            .schematic
            .add_component(ElectronicComponent::resistor("10k"));
        editor.rebuild_scene();
        assert!(editor.drc_is_stale());

        editor.run_drc();

        assert!(!editor.drc_is_stale());
    }

    #[test]
    fn net_names_never_reuse_a_live_name_after_a_wire_is_removed() {
        let mut editor = NativeElectronicsEditor::empty("Test");
        editor
            .schematic
            .add_wire(Vec2::ZERO, Vec2::new(20.0, 0.0), "N007");
        assert_eq!(editor.next_net_name(), "N008");

        editor.schematic.wires.clear();
        // The counter went down but the name is still referenced by the PCB
        // projection, so it must not be handed to a different circuit.
        editor.pcb.airwires.push(raf_electronics::PcbAirwire {
            net: "N007".to_string(),
            from_component_id: Uuid::new_v4(),
            from: Vec2::ZERO,
            to_component_id: Uuid::new_v4(),
            to: Vec2::new(20.0, 0.0),
        });

        assert_eq!(editor.next_net_name(), "N008");
    }

    #[test]
    fn net_names_start_at_one_and_grow_past_the_padding() {
        let editor = NativeElectronicsEditor::empty("Test");
        assert_eq!(editor.next_net_name(), "N001");

        let mut editor = NativeElectronicsEditor::empty("Test");
        editor
            .schematic
            .add_wire(Vec2::ZERO, Vec2::new(20.0, 0.0), "N999");
        assert_eq!(editor.next_net_name(), "N1000");
    }

    #[test]
    fn rotation_has_one_source_of_truth_across_surfaces() {
        let mut editor = NativeElectronicsEditor::empty("Test");
        editor
            .schematic
            .add_component(ElectronicComponent::resistor("10k"));
        let component_id = editor.schematic.components[0].id;
        editor.set_surface(CadSurfaceKind::Pcb);
        assert!(editor.select_component(0));

        assert!(editor.rotate_selected());
        editor.rebuild_scene();

        assert_eq!(editor.rotation_degrees(component_id), 90.0);
        assert_eq!(editor.pcb.components[0].rotation, 90.0);
        assert_eq!(editor.schematic.components[0].rotation, 90.0);
    }

    #[test]
    fn delete_is_confirmation_gated() {
        let mut editor = NativeElectronicsEditor::empty("Test");
        editor
            .schematic
            .add_component(ElectronicComponent::resistor("10k"));
        assert!(editor.select_component(0));

        editor.request_delete_selected();
        assert!(editor.delete_confirm_pending());
        assert_eq!(editor.schematic.components.len(), 1);

        editor.cancel_pending_action();
        assert!(!editor.delete_confirm_pending());
        assert_eq!(editor.schematic.components.len(), 1);

        editor.request_delete_selected();
        editor.confirm_delete();
        assert!(!editor.delete_confirm_pending());
        assert!(editor.schematic.components.is_empty());
    }

    #[test]
    fn surface_errors_are_reported_and_can_be_cleared() {
        let mut editor = NativeElectronicsEditor::empty("Test");
        assert!(editor.surface_errors().is_empty());

        editor.push_surface_error_key("electronics.error.drc_failed");
        editor.push_surface_error_key("electronics.error.drc_failed");
        assert_eq!(editor.surface_errors().len(), 1);

        editor.clear_surface_errors();
        assert!(editor.surface_errors().is_empty());
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
        let canvas = EditorRect::new(10.0, 20.0, 640.0, 480.0);

        assert!(editor.begin_library_drag(0));
        assert_eq!(
            editor.placement_preview_asset_key(),
            Some("electronics://library/resistor.png")
        );
        assert!(!editor.finish_library_drag_at(Some([2000.0, 2000.0]), canvas));
        assert!(editor.schematic.components.is_empty());
        assert_eq!(editor.placement_preview_asset_key(), None);

        assert!(editor.begin_library_drag(0));
        // Top-left of a large canvas is clear of the minimap panel, which
        // lives in the bottom-right corner and rejects placement.
        assert!(editor.finish_library_drag_at(Some([40.0, 50.0]), canvas));
        assert_eq!(editor.schematic.components.len(), 1);
        assert!(!editor.placement_drag_active);
    }

    #[test]
    fn entering_pcb_syncs_an_empty_layout_without_touching_the_undo_stack() {
        let mut editor = NativeElectronicsEditor::empty("Test");
        editor
            .schematic
            .add_component(ElectronicComponent::resistor("10k"));
        assert!(editor.select_component(0));
        assert!(editor.apply_ui_command("electronics.inspector.value.commit:22k"));
        assert!(editor.can_undo());

        // Both flags are already set by the edits above, so the invariant is
        // that the tab switch does not change them.
        let undo_before_switch = editor.history.len();
        let dirty_before_switch = editor.is_dirty();
        editor.set_surface(CadSurfaceKind::Pcb);

        assert_eq!(editor.pcb.components.len(), 1);
        assert_eq!(editor.scene.surface, CadSurfaceKind::Pcb);
        // Entering the PCB tab is navigation, not an edit. The previous behavior
        // recorded a history entry and marked the project dirty, so the next
        // Ctrl+Z undid the derived synchronization instead of the user's work.
        assert_eq!(
            editor.history.len(),
            undo_before_switch,
            "the tab switch must not consume an undo slot"
        );
        assert_eq!(editor.is_dirty(), dirty_before_switch);
        assert!(!editor.sync_is_stale());

        // And from a clean slate the switch leaves nothing dirty at all.
        let mut fresh = NativeElectronicsEditor::empty("Test");
        fresh
            .schematic
            .add_component(ElectronicComponent::resistor("10k"));
        fresh.set_surface(CadSurfaceKind::Pcb);
        assert!(!fresh.is_dirty());
        assert!(!fresh.can_undo());
    }

    #[test]
    fn an_explicit_pcb_sync_is_an_edit_and_reports_a_clean_projection() {
        let mut editor = NativeElectronicsEditor::empty("Test");
        editor
            .schematic
            .add_component(ElectronicComponent::resistor("10k"));
        editor.rebuild_scene();
        assert!(editor.sync_is_stale());

        assert!(editor.apply_ui_command("electronics.pcb.sync"));

        assert!(!editor.sync_is_stale());
        assert!(editor.can_undo());
        assert!(editor.is_dirty());
    }

    #[test]
    fn deleting_in_the_pcb_reports_that_schematic_and_board_diverged() {
        let mut editor = NativeElectronicsEditor::empty("Test");
        editor
            .schematic
            .add_component(ElectronicComponent::resistor("10k"));
        editor.set_surface(CadSurfaceKind::Pcb);
        assert!(editor.select_component(0));

        assert!(editor.delete_selected());

        // The PCB placement is removed but the schematic still owns the part, so
        // the surfaces disagree and the user is told instead of silently losing
        // the component when returning to the schematic.
        assert!(editor.sync_is_stale());
        assert_eq!(editor.schematic.components.len(), 1);
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
