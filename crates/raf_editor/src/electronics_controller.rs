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
    Schematic, SimulationResults,
};
use raf_render::api_graphic_basic::cad_surface::CadSurfaceOptions;
use uuid::Uuid;

use crate::editor_layout::EditorRect;
use crate::electronics_history::{ElectronicsDocumentSnapshot, ElectronicsHistory};
use crate::pcb_document::load_pcb_document;
use crate::schematic_document::{load_schematic_document, DocumentLoad};

#[path = "electronics_analysis.rs"]
mod analysis;
#[path = "electronics_command_adapter.rs"]
mod command_adapter;
#[path = "electronics_component_authoring.rs"]
mod component_authoring;
#[path = "electronics_document.rs"]
mod documents;
#[path = "electronics_controller_footprints.rs"]
mod footprints;
#[path = "electronics_controller_interaction.rs"]
mod interaction;
#[path = "electronics_manufacturing.rs"]
mod manufacturing;
#[path = "electronics_pcb_authoring.rs"]
mod pcb_authoring;

#[path = "electronics_controller/commands.rs"]
mod commands;
#[path = "electronics_controller/history.rs"]
mod history;
#[path = "electronics_controller/inspector.rs"]
mod inspector;
#[path = "electronics_controller/library.rs"]
mod library;
#[path = "electronics_controller/pcb.rs"]
mod pcb;
#[path = "electronics_controller/placement.rs"]
mod placement;
#[path = "electronics_controller/selection.rs"]
mod selection;
#[cfg(test)]
#[path = "electronics_controller/tests.rs"]
mod tests;
#[path = "electronics_controller/wiring.rs"]
mod wiring;

use analysis::{AnalysisKind, AnalysisResult, AnalysisTask, AnalysisWorker};
pub use component_authoring::{
    preset_from_slug, preset_slug, ComponentCreatorDraft, ComponentCreatorError,
    ComponentCreatorField, ComponentCreatorPhase, ComponentCreatorState,
    COMPONENT_CREATOR_CANCEL_COMMAND, COMPONENT_CREATOR_CATEGORY_VALUE_KEY,
    COMPONENT_CREATOR_DESIGNATOR_VALUE_KEY, COMPONENT_CREATOR_FIELD_PREFIX,
    COMPONENT_CREATOR_FOOTPRINT_VALUE_KEY, COMPONENT_CREATOR_NAME_VALUE_KEY,
    COMPONENT_CREATOR_OPEN_COMMAND, COMPONENT_CREATOR_PINS_VALUE_KEY,
    COMPONENT_CREATOR_PRESET_PREFIX, COMPONENT_CREATOR_SAVE_COMMAND,
    COMPONENT_CREATOR_VALUE_VALUE_KEY,
};

pub use inspector::{ElectronicsReportLine, ElectronicsReportLineKind};
pub use pcb::{PcbLayerVisibility, PCB_LAYER_KEYS};
pub use selection::{ElectronicsSelection, ElectronicsSelectionKind};

const MIN_ZOOM: f32 = 0.15;
const MAX_ZOOM: f32 = 160.0;
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
    footprint_project_root: Option<std::path::PathBuf>,
    placement_template: Option<usize>,
    grid_visible: bool,
    snap_enabled: bool,
    grid_step: f32,
    schematic_grid_step: f32,
    pcb_grid_step: f32,
    pcb_route_layer: raf_electronics::PcbLayer,
    pcb_route_width: f32,
    pcb_preview_open: bool,
    pcb_route_draft: Option<pcb_authoring::PcbRouteDraft>,
    placement_drag_active: bool,
    placement_preview: Option<Vec2>,
    grid_opacity: f32,
    labels_visible: bool,
    pcb_reference_visible: bool,
    pcb_values_visible: bool,
    minimap_collapsed: bool,
    pcb_layer_visibility: PcbLayerVisibility,
    drc_lines: Vec<ElectronicsReportLine>,
    drc_report: Option<DrcReport>,
    drc_report_surface: Option<CadSurfaceKind>,
    /// True when the document changed after the current DRC result was produced.
    drc_stale: bool,
    simulation_lines: Vec<ElectronicsReportLine>,
    simulation_results: Option<SimulationResults>,
    /// True when the document changed after the current simulation result.
    simulation_stale: bool,
    /// True when the schematic and the PCB no longer describe the same design.
    sync_stale: bool,
    /// Failures the user must see. Cleared by the user or by the next successful
    /// equivalent operation; never replaced by a log line.
    surface_errors: Vec<String>,
    fabrication_status: Option<String>,
    fabrication_error: Option<String>,
    fabrication_job: Option<std::path::PathBuf>,
    fabrication_summary: Vec<String>,
    fabrication_warnings: Vec<String>,
    route_error: Option<String>,
    /// A destructive action is armed and waiting for an explicit confirmation.
    pending_delete: bool,
    analysis_task: Option<AnalysisTask>,
    analysis_worker: Option<AnalysisWorker>,
    /// Custom component authoring draft and lifecycle.
    component_creator: ComponentCreatorState,
    pub(crate) lab: crate::electronics_lab::ElectronicsLab,
    wire_start: Option<WireStart>,
    board_outline_start: Option<Vec2>,
    board_outline_vertices: Vec<Vec2>,
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
    base_scene_fingerprint: Option<(CadSurfaceKind, u64)>,
    base_scene_object_count: usize,
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
            footprint_project_root: None,
            placement_template: None,
            grid_visible: true,
            snap_enabled: true,
            grid_step: 20.0,
            schematic_grid_step: 20.0,
            pcb_grid_step: 0.25,
            pcb_route_layer: raf_electronics::PcbLayer::TopCopper,
            pcb_route_width: 0.25,
            pcb_preview_open: false,
            pcb_route_draft: None,
            placement_drag_active: false,
            placement_preview: None,
            grid_opacity: 0.55,
            labels_visible: true,
            pcb_reference_visible: true,
            pcb_values_visible: true,
            minimap_collapsed: false,
            pcb_layer_visibility: PcbLayerVisibility::default(),
            drc_lines: Vec::new(),
            drc_report: None,
            drc_report_surface: None,
            drc_stale: false,
            simulation_lines: Vec::new(),
            simulation_results: None,
            simulation_stale: false,
            sync_stale: false,
            surface_errors: Vec::new(),
            fabrication_status: None,
            fabrication_error: None,
            fabrication_job: None,
            fabrication_summary: Vec::new(),
            fabrication_warnings: Vec::new(),
            route_error: None,
            pending_delete: false,
            analysis_task: None,
            analysis_worker: None,
            component_creator: ComponentCreatorState::default(),
            lab: crate::electronics_lab::ElectronicsLab::default(),
            wire_start: None,
            board_outline_start: None,
            board_outline_vertices: Vec::new(),
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
            base_scene_fingerprint: None,
            base_scene_object_count: 0,
            language: Language::English,
        }
    }

    pub fn from_project(project: &Project) -> Self {
        let mut editor = Self::empty(&project.name);
        editor.footprint_project_root = Some(project.path.clone());
        if project.project_type != raf_core::project::ProjectType::Electronics {
            return editor;
        }

        let registry = ProjectSessionRegistry::load_or_legacy(&project.path, project.project_type);
        if let Some(session) = registry.active() {
            let schematic_path = session.path(&project.path, &session.schematic_file);
            let pcb_path = session.path(&project.path, &session.pcb_file);
            if let Err(error) = documents::recover(&schematic_path, &pcb_path) {
                editor.push_surface_error(error);
            }
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

    /// Mount validated project documents without a canvas or disk side effects.
    pub(crate) fn from_documents(project: &Project, schematic: Schematic, pcb: PcbLayout) -> Self {
        let mut editor = Self::empty(&project.name);
        editor.footprint_project_root = Some(project.path.clone());
        editor.schematic = schematic;
        editor.pcb = pcb;
        editor.library.load_external_assets_from(&project.path);
        editor.sync_stale = !editor.pcb_matches_schematic();
        editor.rebuild_scene_with_drc();
        editor.dirty = false;
        editor
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

    pub fn grid_visible(&self) -> bool {
        self.grid_visible
    }

    pub fn grid_step(&self) -> f32 {
        self.grid_step
    }

    pub fn labels_visible(&self) -> bool {
        self.labels_visible
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

    pub fn language(&self) -> Language {
        self.language
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

    pub fn interaction(&self) -> &CadInteractionState {
        &self.interaction
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn ui_revision(&self) -> u64 {
        self.ui_revision
    }

    /// Overview ink changes with document geometry and UI appearance, never
    /// with camera motion or passive pointer hover.
    pub fn overview_revision(&self) -> (CadSurfaceKind, u64, u64) {
        (
            self.active_surface,
            self.base_scene_fingerprint.map_or(0, |(_, hash)| hash),
            self.ui_revision,
        )
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn context_menu_position(&self) -> Option<[f32; 2]> {
        self.context_menu_position
    }

    pub fn minimap_collapsed(&self) -> bool {
        self.minimap_collapsed
    }

    pub fn minimap_rects(&self, size: Vec2) -> Option<crate::electronics_minimap::MinimapRects> {
        crate::electronics_minimap::visible_rects_with_state(
            &self.scene,
            size,
            self.minimap_collapsed,
        )
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
        let physical_pcb = self.active_surface == CadSurfaceKind::Pcb;
        let span = (max - min).max(Vec2::splat(if physical_pcb { 1.0 } else { 40.0 }));
        // PCB coordinates are millimeters. The schematic's old 120-unit
        // margin left a small real board as a tiny island after Fit View.
        let padding = if physical_pcb {
            span * 0.16 + Vec2::splat(4.0)
        } else {
            Vec2::splat(120.0)
        };
        // Fit into the clear drawing area while the overview is expanded.
        // The camera still uses the full canvas coordinate system for input.
        let reserved = if physical_pcb && !self.minimap_collapsed && size.x >= 600.0 {
            self.minimap_rects(size)
                .map_or(0.0, |r| r.panel.width + 24.0)
        } else {
            0.0
        };
        self.camera.center = (min + max) * 0.5;
        self.camera.zoom = ((size.x - reserved).max(1.0) / (span.x + padding.x))
            .min(size.y / (span.y + padding.y))
            .clamp(MIN_ZOOM, MAX_ZOOM);
        self.camera.center.x += reserved / (2.0 * self.camera.zoom);
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
        self.built_schematic_fingerprint = before_schematic;
        self.built_pcb_fingerprint = before_pcb;
        if schematic_changed || pcb_changed {
            if self.fabrication_error.is_none() {
                self.fabrication_status = None;
            }
            self.fabrication_job = None;
            self.fabrication_summary.clear();
            self.fabrication_warnings.clear();
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

    fn rebuild_scene_internal(&mut self) {
        let fingerprint = match self.active_surface {
            CadSurfaceKind::Schematic => schematic_fingerprint(&self.schematic),
            CadSurfaceKind::Pcb => pcb_fingerprint(&self.pcb),
        };
        if self.base_scene_fingerprint != Some((self.active_surface, fingerprint)) {
            self.scene = match self.active_surface {
                CadSurfaceKind::Schematic => CadScene::from_schematic(&self.schematic),
                CadSurfaceKind::Pcb => CadScene::from_pcb(&self.pcb),
            };
            self.base_scene_object_count = self.scene.objects.len();
            self.base_scene_fingerprint = Some((self.active_surface, fingerprint));
        } else {
            self.scene.objects.truncate(self.base_scene_object_count);
        }
        // Presentation filtering runs after the base build but before the
        // analysis markers and previews, so a DRC marker always stays visible
        // even when the graph it names is pruned.
        if self.active_surface == CadSurfaceKind::Pcb && !self.pcb_layer_visibility.is_all_visible()
        {
            let visibility = self.pcb_layer_visibility;
            self.scene.objects.retain(|object| match object.kind {
                CadObjectKind::Pad if object.id.starts_with("via:") => match object.layer {
                    CadLayerKind::PcbTopCopper => visibility.top_traces,
                    CadLayerKind::PcbBottomCopper => visibility.bottom_traces,
                    _ => true,
                },
                CadObjectKind::Component | CadObjectKind::Pad => visibility.components,
                CadObjectKind::Trace => match object.layer {
                    CadLayerKind::PcbTopCopper => visibility.top_traces,
                    CadLayerKind::PcbBottomCopper => visibility.bottom_traces,
                    _ => true,
                },
                CadObjectKind::BoardOutline => visibility.silkscreen,
                CadObjectKind::Airwire => visibility.airwires,
                _ => true,
            });
        }
        if self.drc_report_surface == Some(self.active_surface) {
            if let Some(report) = &self.drc_report {
                self.scene.push_drc_report(report);
            }
        }
        if let (Some(template_index), Some(position)) =
            (self.placement_template, self.placement_preview)
        {
            if let Some(template) = self.library.components.get(template_index) {
                let size = raf_electronics::cad_scene::schematic_component_size(&template.template);
                let min = position - size * 0.5;
                let max = position + size * 0.5;
                self.scene.objects.push(CadObject {
                    shape: Default::default(),
                    rotation_degrees: 0.0,
                    drill: None,
                    stroke_width_world: None,
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
                let mut preview = self.board_outline_vertices.clone();
                preview.push(self.snap_world(end));
                preview.push(start);
                self.scene.objects.push(CadObject {
                    shape: Default::default(),
                    rotation_degrees: 0.0,
                    drill: None,
                    stroke_width_world: None,
                    id: "board-outline-preview".to_string(),
                    source_id: None,
                    kind: CadObjectKind::BoardOutline,
                    layer: CadLayerKind::Overlay,
                    pick_priority: CadPickPriority::Overlay,
                    rect: None,
                    points: preview,
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
                shape: Default::default(),
                rotation_degrees: 0.0,
                drill: None,
                stroke_width_world: None,
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
                    shape: Default::default(),
                    rotation_degrees: 0.0,
                    drill: None,
                    stroke_width_world: None,
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
        self.push_route_preview();
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
