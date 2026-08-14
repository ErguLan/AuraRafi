mod canvas;

use eframe::egui_wgpu;
use egui::{Color32, Rect, Stroke, Ui, Vec2};
use raf_core::i18n::t;
use raf_core::Language;
use raf_electronics::{PcbLayout, PcbSyncSummary, Schematic};
use raf_render::bridge::{RenderRuntime, RenderRuntimeSnapshot};

use crate::electronics_assets::ElectronicsAssetAtlas;

use super::electronics_cad_surface_host::ElectronicsCadSurfaceHost;
use super::schematic_view::electronics_palette;
use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcbSelection {
    None,
    Component(usize),
    Trace(usize),
    Airwire(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PcbTool {
    Select,
    Route,
    Outline,
}

pub struct PcbViewPanel {
    pub layout: PcbLayout,
    pub offset: Vec2,
    pub zoom: f32,
    pub show_minimap: bool,
    pub show_status: bool,
    selection: PcbSelection,
    tool: PcbTool,
    drag_state: Option<(usize, glam::Vec2)>,
    outline_draft: Vec<glam::Vec2>,
    pub lang: Language,
    show_airwires: bool,
    last_sync: Option<PcbSyncSummary>,
    render_runtime: RenderRuntimeSnapshot,
    cad_surface_host: ElectronicsCadSurfaceHost,
    pub(super) cad_scene_cache: Option<(u64, raf_electronics::CadScene)>,
    asset_atlas: ElectronicsAssetAtlas,
    canvas_dark_mode: bool,
    auto_fit_pending: bool,
    document_epoch: u64,
}

impl Default for PcbViewPanel {
    fn default() -> Self {
        Self {
            layout: PcbLayout::new("Untitled PCB"),
            offset: Vec2::new(120.0, 80.0),
            zoom: 1.0,
            show_minimap: true,
            show_status: true,
            selection: PcbSelection::None,
            tool: PcbTool::Select,
            drag_state: None,
            outline_draft: Vec::new(),
            lang: Language::English,
            show_airwires: true,
            last_sync: None,
            render_runtime: RenderRuntimeSnapshot::default(),
            cad_surface_host: ElectronicsCadSurfaceHost::new("pcb_canvas_render"),
            cad_scene_cache: None,
            asset_atlas: ElectronicsAssetAtlas::default(),
            canvas_dark_mode: true,
            auto_fit_pending: true,
            document_epoch: 0,
        }
    }
}

impl PcbViewPanel {
    pub fn show(
        &mut self,
        ui: &mut Ui,
        wgpu_render_state: Option<&egui_wgpu::RenderState>,
        render_runtime: &mut RenderRuntime,
    ) -> bool {
        self.show_surface(ui, wgpu_render_state, render_runtime, true)
    }

    /// Symmetric entry point with the schematic CAD surface. The PCB shell
    /// owns project/navigation docks while this panel retains its canvas,
    /// selection, routing and GPU presentation logic.
    pub fn show_canvas_only(
        &mut self,
        ui: &mut Ui,
        wgpu_render_state: Option<&egui_wgpu::RenderState>,
        render_runtime: &mut RenderRuntime,
    ) -> bool {
        self.show_surface(ui, wgpu_render_state, render_runtime, true)
    }

    pub fn show_canvas_only_without_toolbar(
        &mut self,
        ui: &mut Ui,
        wgpu_render_state: Option<&egui_wgpu::RenderState>,
        render_runtime: &mut RenderRuntime,
    ) -> bool {
        self.show_surface(ui, wgpu_render_state, render_runtime, false)
    }

    fn show_surface(
        &mut self,
        ui: &mut Ui,
        wgpu_render_state: Option<&egui_wgpu::RenderState>,
        render_runtime: &mut RenderRuntime,
        draw_toolbar: bool,
    ) -> bool {
        let mut changed = false;
        self.canvas_dark_mode = ui.visuals().dark_mode;
        // The active Electronics shell uses the retained navigator. Keep the
        // old raster atlas dormant unless this panel is explicitly rendering
        // its embedded legacy chrome.
        if draw_toolbar {
            self.asset_atlas.request_assets(PCB_ASSETS);
            self.asset_atlas.process(ui.ctx());
        }
        if draw_toolbar {
            changed |= self.draw_toolbar(ui);
        }
        let available = ui.available_rect_before_wrap();
        if self.auto_fit_pending {
            self.fit_board_to_view(available.width(), available.height());
            self.auto_fit_pending = false;
        }
        changed |= self.draw_canvas(ui, available, wgpu_render_state, render_runtime);
        changed
    }

    pub fn sync_from_schematic(&mut self, schematic: &Schematic) -> PcbSyncSummary {
        let summary = self.layout.sync_from_schematic(schematic);
        self.last_sync = Some(summary.clone());
        self.selection = PcbSelection::None;
        self.cad_scene_cache = None;
        self.auto_fit_pending = true;
        self.document_epoch = self.document_epoch.wrapping_add(1);
        summary
    }

    pub fn selection(&self) -> PcbSelection {
        self.selection
    }

    pub fn set_render_runtime(&mut self, snapshot: RenderRuntimeSnapshot) {
        self.render_runtime = snapshot;
    }

    pub fn clear_selection(&mut self) {
        self.selection = PcbSelection::None;
    }

    pub fn set_select_tool_from_ui(&mut self) {
        self.tool = PcbTool::Select;
    }

    pub fn set_route_tool_from_ui(&mut self) {
        self.tool = PcbTool::Route;
    }

    pub fn set_outline_tool_from_ui(&mut self) {
        self.tool = PcbTool::Outline;
    }

    pub fn toggle_airwires_from_ui(&mut self) {
        self.show_airwires = !self.show_airwires;
        self.cad_scene_cache = None;
        self.document_epoch = self.document_epoch.wrapping_add(1);
    }

    pub fn route_tool_active(&self) -> bool {
        self.tool == PcbTool::Route
    }

    pub fn outline_tool_active(&self) -> bool {
        self.tool == PcbTool::Outline
    }

    pub fn airwires_visible(&self) -> bool {
        self.show_airwires
    }

    pub fn fit_view_from_ui(&mut self, width: f32, height: f32) {
        self.fit_board_to_view(width, height);
    }

    pub fn clear_outline_draft_from_ui(&mut self) {
        self.outline_draft.clear();
    }

    pub fn route_selected_airwire_from_ui(&mut self) -> bool {
        let Some(index) = self.selected_airwire_index() else {
            return false;
        };
        let changed = self.layout.route_airwire(index);
        if changed {
            self.selection = PcbSelection::None;
        }
        changed
    }

    pub fn zoom_in_from_ui(&mut self) {
        self.zoom = (self.zoom * 1.15).clamp(0.35, 4.0);
    }

    pub fn zoom_out_from_ui(&mut self) {
        self.zoom = (self.zoom / 1.15).clamp(0.35, 4.0);
    }

    pub fn set_minimap_visible(&mut self, visible: bool) {
        self.show_minimap = visible;
    }

    pub fn set_status_visible(&mut self, visible: bool) {
        self.show_status = visible;
    }

    pub fn set_layout(&mut self, layout: PcbLayout) {
        self.layout = layout;
        self.document_epoch = self.document_epoch.wrapping_add(1);
        self.selection = PcbSelection::None;
        self.tool = PcbTool::Select;
        self.drag_state = None;
        self.outline_draft.clear();
        self.cad_scene_cache = None;
        self.auto_fit_pending = true;
    }

    /// Cheap revision hint for retained side panels; the CAD cache owns the
    /// full structural fingerprint and is refreshed by the canvas path.
    pub(crate) fn surface_revision_hint(&self) -> (u64, u64) {
        (
            self.document_epoch,
            self.cad_scene_cache
                .as_ref()
                .map(|(fingerprint, _)| *fingerprint)
                .unwrap_or_default(),
        )
    }

    pub(crate) fn mark_document_changed(&mut self) {
        self.document_epoch = self.document_epoch.wrapping_add(1);
    }

    pub fn select_component(&mut self, idx: usize) {
        if idx < self.layout.components.len() {
            self.selection = PcbSelection::Component(idx);
        }
    }

    /// Select a component by its designator (e.g. "R1", "C2").
    /// Used for cross-probe from Schematic to PCB.
    pub fn select_by_designator(&mut self, designator: &str) {
        let lower = designator.to_lowercase();
        for (idx, comp) in self.layout.components.iter().enumerate() {
            if comp.designator.to_lowercase() == lower {
                self.selection = PcbSelection::Component(idx);
                return;
            }
        }
    }

    /// Returns the designator of the currently selected component, if any.
    pub fn selected_designator(&self) -> Option<String> {
        match &self.selection {
            PcbSelection::Component(idx) if *idx < self.layout.components.len() => {
                Some(self.layout.components[*idx].designator.clone())
            }
            _ => None,
        }
    }

    pub fn select_trace(&mut self, idx: usize) {
        if idx < self.layout.traces.len() {
            self.selection = PcbSelection::Trace(idx);
        }
    }

    pub fn select_airwire(&mut self, idx: usize) {
        if idx < self.layout.airwires.len() {
            self.selection = PcbSelection::Airwire(idx);
        }
    }

    pub fn selected_component_index(&self) -> Option<usize> {
        match self.selection {
            PcbSelection::Component(idx) if idx < self.layout.components.len() => Some(idx),
            _ => None,
        }
    }

    pub fn selected_trace_index(&self) -> Option<usize> {
        match self.selection {
            PcbSelection::Trace(idx) if idx < self.layout.traces.len() => Some(idx),
            _ => None,
        }
    }

    pub fn selected_airwire_index(&self) -> Option<usize> {
        match self.selection {
            PcbSelection::Airwire(idx) if idx < self.layout.airwires.len() => Some(idx),
            _ => None,
        }
    }

    pub fn delete_selection(&mut self) -> bool {
        match self.selection {
            PcbSelection::Trace(idx) => {
                if self.layout.delete_trace(idx) {
                    self.selection = PcbSelection::None;
                    return true;
                }
            }
            PcbSelection::None | PcbSelection::Component(_) | PcbSelection::Airwire(_) => {}
        }
        false
    }

    pub fn duplicate_selection(&mut self) -> bool {
        false
    }

    fn draw_toolbar(&mut self, ui: &mut Ui) -> bool {
        let mut changed = false;
        let palette = electronics_palette(ui.visuals().dark_mode);
        let rect = ui.available_rect_before_wrap();
        let toolbar_rect = Rect::from_min_size(rect.left_top(), Vec2::new(rect.width(), 42.0));
        let painter = ui.painter_at(toolbar_rect);
        painter.rect_filled(toolbar_rect, 0.0, palette.toolbar_bg);
        painter.line_segment(
            [toolbar_rect.left_bottom(), toolbar_rect.right_bottom()],
            Stroke::new(1.0, palette.border),
        );

        ui.allocate_new_ui(
            egui::UiBuilder::new().max_rect(toolbar_rect.shrink2(Vec2::new(10.0, 6.0))),
            |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 7.0;

                    if self
                        .toolbar_icon_button(
                            ui,
                            "toolbar/select.png",
                            self.tool == PcbTool::Select,
                            t("app.pcb_tool_select", self.lang),
                        )
                        .clicked()
                    {
                        self.tool = PcbTool::Select;
                    }

                    if self
                        .toolbar_icon_button(
                            ui,
                            "toolbar/wire.png",
                            self.tool == PcbTool::Route,
                            t("app.pcb_tool_route", self.lang),
                        )
                        .clicked()
                    {
                        self.tool = PcbTool::Route;
                    }

                    if self
                        .toolbar_icon_button(
                            ui,
                            "toolbar/outline.png",
                            self.tool == PcbTool::Outline,
                            t("app.pcb_tool_outline", self.lang),
                        )
                        .clicked()
                    {
                        self.tool = PcbTool::Outline;
                    }

                    if self
                        .toolbar_icon_button(
                            ui,
                            "toolbar/airwire.png",
                            self.show_airwires,
                            t("app.pcb_airwires", self.lang),
                        )
                        .clicked()
                    {
                        self.show_airwires = !self.show_airwires;
                    }

                    if self
                        .toolbar_icon_button(
                            ui,
                            "toolbar/fit.png",
                            false,
                            t("app.electronics_fit", self.lang),
                        )
                        .clicked()
                    {
                        self.fit_board_to_view(rect.width(), rect.height());
                    }

                    ui.add_space(10.0);

                    if self.tool == PcbTool::Outline
                        && ui
                            .add_sized(
                                [112.0, 30.0],
                                egui::Button::new(
                                    egui::RichText::new(t("app.pcb_new_outline", self.lang))
                                        .size(11.0)
                                        .color(palette.text),
                                )
                                .fill(palette.card_bg)
                                .stroke(Stroke::new(1.0, palette.border)),
                            )
                            .clicked()
                    {
                        self.outline_draft.clear();
                    }

                    if self.selected_airwire_index().is_some()
                        && ui
                            .add_sized(
                                [128.0, 30.0],
                                egui::Button::new(
                                    egui::RichText::new(t("app.pcb_route_selected", self.lang))
                                        .size(11.0)
                                        .color(Color32::WHITE),
                                )
                                .fill(Color32::from_rgb(121, 70, 22))
                                .stroke(Stroke::new(1.0, theme::ACCENT)),
                            )
                            .clicked()
                    {
                        if let Some(idx) = self.selected_airwire_index() {
                            changed |= self.layout.route_airwire(idx);
                            self.selection = PcbSelection::None;
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self
                            .toolbar_icon_button(
                                ui,
                                "toolbar/zoom-in.png",
                                false,
                                t("app.electronics_zoom_in", self.lang),
                            )
                            .clicked()
                        {
                            self.zoom = (self.zoom * 1.15).clamp(0.35, 4.0);
                        }
                        if self
                            .toolbar_icon_button(
                                ui,
                                "toolbar/zoom-out.png",
                                false,
                                t("app.electronics_zoom_out", self.lang),
                            )
                            .clicked()
                        {
                            self.zoom = (self.zoom / 1.15).clamp(0.35, 4.0);
                        }
                        ui.label(
                            egui::RichText::new(format!("{:.0}%", self.zoom * 100.0))
                                .size(10.0)
                                .color(palette.text_dim),
                        );
                    });
                });
            },
        );

        ui.allocate_space(Vec2::new(rect.width(), 42.0));
        changed
    }

    fn toolbar_icon_button(
        &self,
        ui: &mut Ui,
        asset: &'static str,
        active: bool,
        tooltip: impl Into<String>,
    ) -> egui::Response {
        let palette = electronics_palette(ui.visuals().dark_mode);
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(30.0), egui::Sense::click());
        let fill = if active {
            Color32::from_rgb(121, 70, 22)
        } else if response.hovered() {
            palette.card_hover
        } else {
            palette.card_bg
        };
        ui.painter().rect_filled(rect, 4.0, fill);
        ui.painter().rect_stroke(
            rect,
            4.0,
            Stroke::new(
                1.0,
                if active {
                    theme::ACCENT
                } else {
                    palette.border
                },
            ),
        );
        let icon_rect = Rect::from_center_size(rect.center(), Vec2::splat(18.0));
        let _ = self
            .asset_atlas
            .paint(ui.painter(), asset, icon_rect, Color32::WHITE);
        response.on_hover_text(tooltip.into())
    }

    fn fit_board_to_view(&mut self, width: f32, height: f32) {
        let size = self.layout.board_size();
        if size.x <= 1.0 || size.y <= 1.0 {
            self.offset = Vec2::new(160.0, 110.0);
            self.zoom = 1.0;
            return;
        }

        let zoom_x = (width - 160.0).max(160.0) / size.x;
        let zoom_y = (height - 120.0).max(160.0) / size.y;
        self.zoom = zoom_x.min(zoom_y).clamp(0.45, 2.5);
        self.offset = Vec2::new(
            (width - size.x * self.zoom) * 0.5,
            (height - size.y * self.zoom) * 0.5,
        );
    }
}

const PCB_ASSETS: &[&str] = &[
    "toolbar/select.png",
    "toolbar/wire.png",
    "toolbar/outline.png",
    "toolbar/airwire.png",
    "toolbar/fit.png",
    "toolbar/zoom-in.png",
    "toolbar/zoom-out.png",
    "toolbar/layers.png",
    "footprints/0805.png",
    "footprints/magnet-10x5.png",
    "footprints/battery-18650.png",
    "footprints/test-point.png",
    "footprints/generic.png",
];
