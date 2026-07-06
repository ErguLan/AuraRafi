mod canvas;

use eframe::egui_wgpu;
use egui::{Color32, Pos2, Rect, Stroke, Ui, Vec2};
use raf_core::i18n::t;
use raf_core::Language;
use raf_electronics::component::ElectronicComponent;
use raf_electronics::library::ComponentLibrary;
use raf_electronics::schematic::Schematic;
use raf_electronics::simulation::SimulationResults;
use raf_render::bridge::{RenderRuntime, RenderRuntimeSnapshot};

use super::gpu_canvas::GpuCanvas;
use crate::electronics_assets::ElectronicsAssetAtlas;
use crate::theme;

const GRID_STEP: f32 = 20.0;
const COMP_BODY_W: f32 = 60.0;
const COMP_BODY_H: f32 = 30.0;
const PIN_DOT_RADIUS: f32 = 3.0;
const PIN_SNAP_DISTANCE: f32 = 16.0;
const WIRE_ENDPOINT_SNAP_DISTANCE: f32 = 12.0;
const WIRE_JUNCTION_SNAP_DISTANCE: f32 = 10.0;
const WIRE_HIT_DISTANCE: f32 = 6.0;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ElectronicsPalette {
    pub canvas_bg: Color32,
    pub panel_bg: Color32,
    pub toolbar_bg: Color32,
    pub card_bg: Color32,
    pub card_hover: Color32,
    pub card_active: Color32,
    pub chip_bg: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_muted: Color32,
    pub node_bg: Color32,
    pub overlay_bg: Color32,
}

pub(crate) fn electronics_palette(is_dark: bool) -> ElectronicsPalette {
    if is_dark {
        ElectronicsPalette {
            canvas_bg: Color32::from_rgb(13, 14, 18),
            panel_bg: Color32::from_rgb(13, 16, 20),
            toolbar_bg: Color32::from_rgb(13, 16, 20),
            card_bg: Color32::from_rgb(18, 22, 28),
            card_hover: Color32::from_rgb(25, 30, 37),
            card_active: Color32::from_rgb(44, 36, 24),
            chip_bg: Color32::from_rgb(30, 30, 35),
            border: Color32::from_rgb(33, 39, 48),
            text: Color32::from_rgb(224, 226, 232),
            text_dim: Color32::from_rgb(150, 156, 168),
            text_muted: Color32::from_rgb(118, 122, 132),
            node_bg: Color32::from_rgb(13, 14, 18),
            overlay_bg: Color32::from_rgba_premultiplied(10, 13, 17, 242),
        }
    } else {
        ElectronicsPalette {
            canvas_bg: Color32::from_rgb(239, 243, 249),
            panel_bg: Color32::from_rgb(250, 252, 255),
            toolbar_bg: Color32::from_rgb(247, 250, 254),
            card_bg: Color32::from_rgb(255, 255, 255),
            card_hover: Color32::from_rgb(237, 243, 250),
            card_active: Color32::from_rgb(255, 238, 210),
            chip_bg: Color32::from_rgb(235, 239, 246),
            border: Color32::from_rgb(203, 211, 224),
            text: Color32::from_rgb(34, 39, 48),
            text_dim: Color32::from_rgb(84, 94, 110),
            text_muted: Color32::from_rgb(112, 121, 137),
            node_bg: Color32::from_rgb(250, 252, 255),
            overlay_bg: Color32::from_rgba_premultiplied(248, 251, 255, 244),
        }
    }
}

#[derive(Debug, Clone)]
enum PlacementMode {
    None,
    Component(usize),
    Clipboard(Vec<ElectronicComponent>),
    Wire,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchematicSelection {
    None,
    Component(usize),
    MultipleComponents(Vec<usize>),
    Wire(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectionKind {
    Grid,
    Pin,
    WireEndpoint,
    WireJunction,
}

#[derive(Debug, Clone, Copy)]
struct ConnectionCandidate {
    world: Pos2,
    kind: ConnectionKind,
    component_index: Option<usize>,
    pin_index: Option<usize>,
    wire_index: Option<usize>,
}

impl ConnectionCandidate {
    fn grid(world: Pos2) -> Self {
        Self {
            world,
            kind: ConnectionKind::Grid,
            component_index: None,
            pin_index: None,
            wire_index: None,
        }
    }

    fn chain_anchor(self, world: Pos2) -> Self {
        Self {
            world,
            kind: self.kind,
            component_index: self.component_index,
            pin_index: self.pin_index,
            wire_index: None,
        }
    }
}

#[derive(Debug, Clone)]
struct ContextMenu {
    screen_pos: Pos2,
    target: SchematicSelection,
}

pub struct SchematicViewPanel {
    pub schematic: Schematic,
    pub library: ComponentLibrary,
    pub offset: Vec2,
    pub zoom: f32,
    selection: SchematicSelection,
    placement: PlacementMode,
    placement_rotation: f32,
    wire_start: Option<ConnectionCandidate>,
    show_library: bool,
    library_search: String,
    quick_search_open: bool,
    quick_search_query: String,
    quick_search_selected: usize,
    clipboard_components: Vec<ElectronicComponent>,
    test_results: Vec<String>,
    show_test_results: bool,
    drag_state: Option<Vec<(usize, Vec2)>>,
    box_select_start: Option<Pos2>,
    box_select_rect: Option<Rect>,
    context_menu: Option<ContextMenu>,
    editing_value: Option<(usize, String)>,
    value_editor_just_opened: bool,
    /// Inline net name editor: (wire_index, draft_name).
    editing_net_name: Option<(usize, String)>,
    /// Measurement tool: first point in world coords (None = inactive).
    measurement_start: Option<Vec2>,
    /// Measurement tool: second point in world coords (when complete).
    measurement_end: Option<Vec2>,
    pub lang: Language,
    sim_results: Option<SimulationResults>,
    sim_active: bool,
    sim_phase: f32,
    show_export_menu: bool,
    export_message: Option<String>,
    render_runtime: RenderRuntimeSnapshot,
    gpu_canvas: GpuCanvas,
    asset_atlas: ElectronicsAssetAtlas,
    canvas_dark_mode: bool,
}

impl Default for SchematicViewPanel {
    fn default() -> Self {
        let mut library = ComponentLibrary::default_library();
        library.load_external_assets();
        Self {
            schematic: Schematic::new("Untitled"),
            library,
            offset: Vec2::new(200.0, 150.0),
            zoom: 1.0,
            selection: SchematicSelection::None,
            placement: PlacementMode::None,
            placement_rotation: 0.0,
            wire_start: None,
            show_library: true,
            library_search: String::new(),
            quick_search_open: false,
            quick_search_query: String::new(),
            quick_search_selected: 0,
            clipboard_components: Vec::new(),
            test_results: Vec::new(),
            show_test_results: false,
            drag_state: None,
            box_select_start: None,
            box_select_rect: None,
            context_menu: None,
            editing_value: None,
            value_editor_just_opened: false,
            editing_net_name: None,
            measurement_start: None,
            measurement_end: None,
            lang: Language::English,
            sim_results: None,
            sim_active: false,
            sim_phase: 0.0,
            show_export_menu: false,
            export_message: None,
            render_runtime: RenderRuntimeSnapshot::default(),
            gpu_canvas: GpuCanvas::new("schematic_canvas_render"),
            asset_atlas: ElectronicsAssetAtlas::default(),
            canvas_dark_mode: true,
        }
    }
}

impl SchematicViewPanel {
    pub fn show(
        &mut self,
        ui: &mut Ui,
        wgpu_render_state: Option<&egui_wgpu::RenderState>,
        render_runtime: &mut RenderRuntime,
    ) -> bool {
        let mut changed = false;

        self.asset_atlas.request_assets(ELECTRONICS_ASSETS);
        self.asset_atlas.process(ui.ctx());
        self.draw_toolbar(ui);
        self.draw_tool_status_hint(ui);

        let available = ui.available_rect_before_wrap();
        if self.show_library {
            let lib_width = 242.0;
            let lib_rect = Rect::from_min_size(
                available.left_top(),
                Vec2::new(lib_width, available.height()),
            );
            let canvas_rect = Rect::from_min_max(
                Pos2::new(available.left() + lib_width, available.top()),
                available.right_bottom(),
            );

            self.draw_library(ui, lib_rect);
            changed |= self.draw_canvas(ui, canvas_rect, wgpu_render_state, render_runtime);
        } else {
            changed |= self.draw_canvas(ui, available, wgpu_render_state, render_runtime);
        }

        changed |= self.draw_value_editor(ui);
        changed |= self.draw_net_name_editor(ui);
        changed |= self.draw_quick_search(ui);
        changed |= self.draw_context_menu(ui);
        changed
    }

    pub fn selection(&self) -> SchematicSelection {
        self.selection.clone()
    }

    pub fn set_render_runtime(&mut self, snapshot: RenderRuntimeSnapshot) {
        self.render_runtime = snapshot;
    }

    pub fn selected_component_index(&self) -> Option<usize> {
        match &self.selection {
            SchematicSelection::Component(idx) if *idx < self.schematic.components.len() => {
                Some(*idx)
            }
            _ => None,
        }
    }

    pub fn selected_component_indices(&self) -> Vec<usize> {
        match &self.selection {
            SchematicSelection::Component(idx) if *idx < self.schematic.components.len() => {
                vec![*idx]
            }
            SchematicSelection::MultipleComponents(indices) => indices
                .iter()
                .copied()
                .filter(|idx| *idx < self.schematic.components.len())
                .collect(),
            _ => Vec::new(),
        }
    }

    pub fn is_component_selected(&self, idx: usize) -> bool {
        match &self.selection {
            SchematicSelection::Component(selected) => *selected == idx,
            SchematicSelection::MultipleComponents(indices) => indices.contains(&idx),
            _ => false,
        }
    }

    pub fn selected_wire_index(&self) -> Option<usize> {
        match &self.selection {
            SchematicSelection::Wire(idx) if *idx < self.schematic.wires.len() => Some(*idx),
            _ => None,
        }
    }

    pub fn selected_wire_indices(&self) -> Vec<usize> {
        match &self.selection {
            SchematicSelection::Wire(idx) if *idx < self.schematic.wires.len() => {
                self.wire_group_indices(*idx)
            }
            _ => Vec::new(),
        }
    }

    pub fn clear_selection(&mut self) {
        self.selection = SchematicSelection::None;
    }

    pub(super) fn placement_rotation(&self) -> f32 {
        self.placement_rotation
    }

    pub(super) fn rotate_placement_preview(&mut self) {
        match &mut self.placement {
            PlacementMode::Clipboard(components) => {
                for component in components {
                    component.rotation = (component.rotation + 90.0) % 360.0;
                }
            }
            _ => {
                self.placement_rotation = (self.placement_rotation + 90.0) % 360.0;
            }
        }
    }

    pub fn select_component(&mut self, idx: usize) {
        if idx < self.schematic.components.len() {
            self.selection = SchematicSelection::Component(idx);
        }
    }

    /// Select a component by its designator (e.g. "R1", "C2").
    /// Used for cross-probe from PCB to Schematic.
    pub fn select_by_designator(&mut self, designator: &str) {
        let lower = designator.to_lowercase();
        for (idx, comp) in self.schematic.components.iter().enumerate() {
            if comp.designator.to_lowercase() == lower {
                self.selection = SchematicSelection::Component(idx);
                // Center view on the component.
                let pos = self.schematic.components[idx].position;
                self.offset = Vec2::new(-pos.x * self.zoom, -pos.y * self.zoom);
                return;
            }
        }
    }

    /// Returns the designator of the currently selected component, if any.
    pub fn selected_designator(&self) -> Option<String> {
        match &self.selection {
            SchematicSelection::Component(idx) => {
                self.schematic.components.get(*idx).map(|c| c.designator.clone())
            }
            _ => None,
        }
    }

    pub(super) fn select_component_with_modifiers(&mut self, idx: usize, shift: bool, ctrl: bool) {
        if idx >= self.schematic.components.len() {
            return;
        }

        if ctrl {
            let mut indices = self.selected_component_indices();
            indices.retain(|selected| *selected != idx);
            self.set_component_multi_selection(indices);
        } else if shift {
            let mut indices = self.selected_component_indices();
            if !indices.contains(&idx) {
                indices.push(idx);
            }
            self.set_component_multi_selection(indices);
        } else {
            self.selection = SchematicSelection::Component(idx);
        }
    }

    pub(super) fn select_components_with_modifiers(
        &mut self,
        mut indices: Vec<usize>,
        shift: bool,
        ctrl: bool,
    ) {
        indices.sort_unstable();
        indices.dedup();
        indices.retain(|idx| *idx < self.schematic.components.len());

        if ctrl {
            let mut current = self.selected_component_indices();
            current.retain(|idx| !indices.contains(idx));
            self.set_component_multi_selection(current);
        } else if shift {
            let mut current = self.selected_component_indices();
            for idx in indices {
                if !current.contains(&idx) {
                    current.push(idx);
                }
            }
            self.set_component_multi_selection(current);
        } else {
            self.set_component_multi_selection(indices);
        }
    }

    fn set_component_multi_selection(&mut self, mut indices: Vec<usize>) {
        indices.sort_unstable();
        indices.dedup();
        indices.retain(|idx| *idx < self.schematic.components.len());
        self.selection = match indices.len() {
            0 => SchematicSelection::None,
            1 => SchematicSelection::Component(indices[0]),
            _ => SchematicSelection::MultipleComponents(indices),
        };
    }

    pub fn select_wire(&mut self, idx: usize) {
        if idx < self.schematic.wires.len() {
            self.selection = SchematicSelection::Wire(idx);
        }
    }

    pub fn duplicate_selection(&mut self) -> bool {
        let selected = self.selected_component_indices();
        if selected.is_empty() {
            return false;
        }

        let mut new_indices = Vec::new();
        for idx in selected {
            if self.schematic.duplicate_component(idx).is_some() {
                new_indices.push(self.schematic.components.len().saturating_sub(1));
            }
        }

        let changed = !new_indices.is_empty();
        if changed {
            self.set_component_multi_selection(new_indices);
        }
        changed
    }

    pub fn delete_selection(&mut self) -> bool {
        match self.selection.clone() {
            SchematicSelection::Component(idx) => {
                if idx < self.schematic.components.len() {
                    self.schematic.components.remove(idx);
                    self.selection = SchematicSelection::None;
                    return true;
                }
            }
            SchematicSelection::MultipleComponents(mut indices) => {
                indices.sort_unstable();
                indices.dedup();
                let mut changed = false;
                for idx in indices.into_iter().rev() {
                    if idx < self.schematic.components.len() {
                        self.schematic.components.remove(idx);
                        changed = true;
                    }
                }
                if changed {
                    self.selection = SchematicSelection::None;
                    return true;
                }
            }
            SchematicSelection::Wire(idx) => {
                let mut indices = self.wire_group_indices(idx);
                indices.sort_unstable();
                indices.dedup();
                let mut changed = false;
                for wire_idx in indices.into_iter().rev() {
                    changed |= self.schematic.remove_wire(wire_idx);
                }
                if changed {
                    self.selection = SchematicSelection::None;
                    return true;
                }
            }
            SchematicSelection::None => {}
        }
        false
    }

    pub(super) fn copy_selected_components(&mut self) {
        self.clipboard_components = self
            .selected_component_indices()
            .into_iter()
            .filter_map(|idx| self.schematic.components.get(idx).cloned())
            .collect();
    }

    pub(super) fn start_clipboard_preview(&mut self) -> bool {
        if self.clipboard_components.is_empty() {
            return false;
        }
        self.placement = PlacementMode::Clipboard(self.clipboard_components.clone());
        self.wire_start = None;
        true
    }

    fn wire_group_indices(&self, start_idx: usize) -> Vec<usize> {
        if start_idx >= self.schematic.wires.len() {
            return Vec::new();
        }

        let net = self.schematic.wires[start_idx].net.trim();
        if !net.is_empty() {
            return self
                .schematic
                .wires
                .iter()
                .enumerate()
                .filter_map(|(idx, wire)| (wire.net.trim() == net).then_some(idx))
                .collect();
        }

        let mut group = vec![start_idx];
        let mut cursor = 0usize;
        while cursor < group.len() {
            let idx = group[cursor];
            cursor += 1;
            let wire = &self.schematic.wires[idx];
            let endpoints = [wire.start, wire.end];

            for (other_idx, other) in self.schematic.wires.iter().enumerate() {
                if group.contains(&other_idx) {
                    continue;
                }
                let other_endpoints = [other.start, other.end];
                let connected = endpoints
                    .iter()
                    .any(|a| other_endpoints.iter().any(|b| a.distance(*b) <= 0.5));
                if connected {
                    group.push(other_idx);
                }
            }
        }

        group
    }

    fn draw_toolbar(&mut self, ui: &mut Ui) {
        let palette = electronics_palette(ui.visuals().dark_mode);
        let rect = ui.available_rect_before_wrap();
        let toolbar_rect = Rect::from_min_size(rect.left_top(), Vec2::new(rect.width(), 42.0));
        let painter = ui.painter_at(toolbar_rect);
        painter.rect_filled(toolbar_rect, 0.0, palette.toolbar_bg);
        painter.line_segment(
            [toolbar_rect.left_bottom(), toolbar_rect.right_bottom()],
            Stroke::new(1.0, palette.border),
        );

        ui.allocate_ui_at_rect(toolbar_rect.shrink2(Vec2::new(10.0, 6.0)), |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 7.0;

                if self
                    .toolbar_icon_button(
                        ui,
                        "toolbar/select.png",
                        matches!(self.placement, PlacementMode::None),
                        t("app.electronics_tool_select", self.lang),
                    )
                    .clicked()
                {
                    self.placement = PlacementMode::None;
                    self.wire_start = None;
                }

                if self
                    .toolbar_icon_button(
                        ui,
                        "toolbar/rotate.png",
                        false,
                        t("app.rotate_r", self.lang),
                    )
                    .clicked()
                {
                    self.rotate_placement_preview();
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
                    self.offset = Vec2::new(260.0, 150.0);
                    self.zoom = 1.2;
                }

                if self
                    .toolbar_icon_button(
                        ui,
                        "toolbar/grid.png",
                        self.show_library,
                        t("app.electronics_toggle_library", self.lang),
                    )
                    .clicked()
                {
                    self.show_library = !self.show_library;
                }

                ui.add_space(10.0);

                if self
                    .toolbar_icon_button(
                        ui,
                        "toolbar/play.png",
                        self.show_test_results,
                        t("app.electronics_play_test", self.lang),
                    )
                    .clicked()
                {
                    self.test_results = self.schematic.electrical_test();
                    self.show_test_results = true;
                }

                if self
                    .toolbar_icon_button(
                        ui,
                        "toolbar/export.png",
                        self.show_export_menu,
                        t("app.export_schematic", self.lang),
                    )
                    .clicked()
                {
                    self.show_export_menu = !self.show_export_menu;
                    self.context_menu = None;
                }

                ui.add_space(10.0);

                ui.label(
                    egui::RichText::new("2D")
                        .size(11.0)
                        .strong()
                        .color(theme::ACCENT),
                );
                ui.label(
                    egui::RichText::new("3D")
                        .size(11.0)
                        .color(palette.text_muted),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(message) = &self.export_message {
                        ui.label(
                            egui::RichText::new(message)
                                .size(10.0)
                                .color(palette.text_muted),
                        );
                    }

                    ui.label(
                        egui::RichText::new(self.render_runtime.status_badge())
                            .size(10.0)
                            .color(if self.render_runtime.is_gpu_active() {
                                theme::ACCENT
                            } else {
                                palette.text_muted
                            }),
                    );

                    let summary = format!(
                        "{} {} | {} {}",
                        t("app.schematic_components", self.lang),
                        self.schematic.components.len(),
                        t("app.schematic_wires", self.lang),
                        self.schematic.wires.len()
                    );
                    ui.label(
                        egui::RichText::new(summary)
                            .size(11.0)
                            .color(palette.text_muted),
                    );
                });
            });
        });

        ui.allocate_space(Vec2::new(rect.width(), 42.0));

        if !matches!(self.placement, PlacementMode::None) {
            let placement_label = match &self.placement {
                PlacementMode::Component(idx) => self
                    .library
                    .components
                    .get(*idx)
                    .map(|template| {
                        format!(
                            "{}: {}",
                            t("app.schematic_component", self.lang),
                            template.name
                        )
                    })
                    .unwrap_or_else(|| t("app.schematic_component", self.lang).to_string()),
                PlacementMode::Clipboard(_) => {
                    t("app.electronics_paste_preview", self.lang).to_string()
                }
                PlacementMode::Wire => t("app.electronics_routing", self.lang).to_string(),
                PlacementMode::None => String::new(),
            };

            ui.label(
                egui::RichText::new(placement_label)
                    .size(11.0)
                    .color(theme::ACCENT),
            );
        }
    }

    fn draw_tool_status_hint(&mut self, ui: &mut Ui) {
        let hint = match &self.placement {
            PlacementMode::None => {
                if self.selection != SchematicSelection::None {
                    t("schematic_hint_select_active", self.lang)
                } else {
                    t("schematic_hint_select", self.lang)
                }
            }
            PlacementMode::Component(_) => t("schematic_hint_place", self.lang),
            PlacementMode::Clipboard(_) => t("schematic_hint_paste", self.lang),
            PlacementMode::Wire => t("schematic_hint_wire", self.lang),
        };
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new(hint)
                .size(10.0)
                .color(egui::Color32::from_rgb(130, 130, 140)),
        );
        ui.add_space(4.0);
    }

    fn draw_library(&mut self, ui: &mut Ui, rect: Rect) {
        let palette = electronics_palette(ui.visuals().dark_mode);
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, palette.panel_bg);
        painter.line_segment(
            [rect.right_top(), rect.right_bottom()],
            Stroke::new(1.0, palette.border),
        );

        painter.text(
            Pos2::new(rect.left() + 12.0, rect.top() + 16.0),
            egui::Align2::LEFT_CENTER,
            t("app.electronics_library", self.lang),
            egui::FontId::proportional(11.0),
            palette.text_dim,
        );

        let search_rect = Rect::from_min_size(
            Pos2::new(rect.left() + 10.0, rect.top() + 34.0),
            Vec2::new(rect.width() - 20.0, 28.0),
        );
        ui.allocate_ui_at_rect(search_rect, |ui| {
            ui.add_sized(
                search_rect.size(),
                egui::TextEdit::singleline(&mut self.library_search)
                    .hint_text(t("app.electronics_search_components", self.lang)),
            );
        });

        let mut y = rect.top() + 76.0;
        let query = self.library_search.trim().to_lowercase();
        let mut last_category = String::new();
        for idx in 0..self.library.components.len() {
            let template = &self.library.components[idx];
            if !query.is_empty()
                && !template.name.to_lowercase().contains(&query)
                && !template.category.to_lowercase().contains(&query)
                && !template
                    .keywords
                    .iter()
                    .any(|keyword| keyword.to_lowercase().contains(&query))
            {
                continue;
            }

            if template.category != last_category {
                last_category = template.category.clone();
                painter.text(
                    Pos2::new(rect.left() + 12.0, y),
                    egui::Align2::LEFT_CENTER,
                    last_category.to_uppercase(),
                    egui::FontId::proportional(9.0),
                    palette.text_muted,
                );
                y += 16.0;
            }

            let item_rect = Rect::from_min_size(
                Pos2::new(rect.left() + 10.0, y),
                Vec2::new(rect.width() - 20.0, 38.0),
            );
            let response = ui.allocate_rect(item_rect, egui::Sense::click());
            let is_active =
                matches!(self.placement, PlacementMode::Component(active_idx) if active_idx == idx);
            let fill = if is_active {
                palette.card_active
            } else if response.hovered() {
                palette.card_hover
            } else {
                palette.card_bg
            };

            painter.rect_filled(item_rect, 4.0, fill);
            painter.rect_stroke(item_rect, 4.0, Stroke::new(1.0, palette.border));

            if let Some(icon) = template.icon_asset {
                let icon_rect = Rect::from_center_size(
                    Pos2::new(item_rect.left() + 18.0, item_rect.center().y),
                    Vec2::splat(22.0),
                );
                let _ = self
                    .asset_atlas
                    .paint(&painter, icon, icon_rect, Color32::WHITE);
            }

            painter.text(
                Pos2::new(item_rect.left() + 40.0, item_rect.top() + 12.0),
                egui::Align2::LEFT_CENTER,
                &template.name,
                egui::FontId::proportional(11.0),
                palette.text,
            );

            painter.text(
                Pos2::new(item_rect.left() + 40.0, item_rect.top() + 27.0),
                egui::Align2::LEFT_CENTER,
                &template.description,
                egui::FontId::proportional(9.0),
                palette.text_muted,
            );

            if response.clicked() {
                self.placement = PlacementMode::Component(idx);
                self.placement_rotation = 0.0;
                self.wire_start = None;
            }

            y += 44.0;
            if y > rect.bottom() - 42.0 {
                break;
            }
        }
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
}

const ELECTRONICS_ASSETS: &[&str] = &[
    "toolbar/select.png",
    "toolbar/rotate.png",
    "toolbar/fit.png",
    "toolbar/grid.png",
    "toolbar/play.png",
    "toolbar/export.png",
    "library/resistor.png",
    "library/capacitor.png",
    "library/led.png",
    "library/magnet.png",
    "library/battery.png",
    "library/ground.png",
    "symbols/resistor.png",
    "symbols/capacitor.png",
    "symbols/led.png",
    "symbols/magnet.png",
    "symbols/battery.png",
    "symbols/ground.png",
    "symbols/generic.png",
];
