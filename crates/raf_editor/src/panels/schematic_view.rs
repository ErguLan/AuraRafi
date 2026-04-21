mod canvas;

use egui::{Color32, Pos2, Rect, Stroke, Ui, Vec2};
use raf_core::i18n::t;
use raf_core::Language;
use raf_electronics::library::ComponentLibrary;
use raf_electronics::schematic::Schematic;
use raf_electronics::simulation::SimulationResults;

use crate::theme;

const GRID_STEP: f32 = 20.0;
const COMP_BODY_W: f32 = 60.0;
const COMP_BODY_H: f32 = 30.0;
const PIN_DOT_RADIUS: f32 = 3.0;
const PIN_SNAP_DISTANCE: f32 = 16.0;
const WIRE_ENDPOINT_SNAP_DISTANCE: f32 = 12.0;
const WIRE_JUNCTION_SNAP_DISTANCE: f32 = 10.0;
const WIRE_HIT_DISTANCE: f32 = 6.0;

#[derive(Debug, Clone, PartialEq)]
enum PlacementMode {
    None,
    Component(usize),
    Wire,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchematicSelection {
    None,
    Component(usize),
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
    wire_start: Option<ConnectionCandidate>,
    show_library: bool,
    test_results: Vec<String>,
    show_test_results: bool,
    drag_state: Option<(usize, Vec2)>,
    context_menu: Option<ContextMenu>,
    editing_value: Option<(usize, String)>,
    pub lang: Language,
    sim_results: Option<SimulationResults>,
    sim_active: bool,
    sim_phase: f32,
    show_export_menu: bool,
    export_message: Option<String>,
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
            wire_start: None,
            show_library: true,
            test_results: Vec::new(),
            show_test_results: false,
            drag_state: None,
            context_menu: None,
            editing_value: None,
            lang: Language::English,
            sim_results: None,
            sim_active: false,
            sim_phase: 0.0,
            show_export_menu: false,
            export_message: None,
        }
    }
}

impl SchematicViewPanel {
    pub fn show(&mut self, ui: &mut Ui) -> bool {
        let mut changed = false;

        self.draw_toolbar(ui);
        ui.separator();

        let available = ui.available_rect_before_wrap();
        if self.show_library {
            let lib_width = 196.0;
            let lib_rect = Rect::from_min_size(available.left_top(), Vec2::new(lib_width, available.height()));
            let canvas_rect = Rect::from_min_max(
                Pos2::new(available.left() + lib_width + 2.0, available.top()),
                available.right_bottom(),
            );

            self.draw_library(ui, lib_rect);
            changed |= self.draw_canvas(ui, canvas_rect);
        } else {
            changed |= self.draw_canvas(ui, available);
        }

        changed |= self.draw_value_editor(ui);
        changed |= self.draw_context_menu(ui);
        changed
    }

    pub fn selection(&self) -> SchematicSelection {
        self.selection
    }

    pub fn selected_component_index(&self) -> Option<usize> {
        match self.selection {
            SchematicSelection::Component(idx) if idx < self.schematic.components.len() => Some(idx),
            _ => None,
        }
    }

    pub fn selected_wire_index(&self) -> Option<usize> {
        match self.selection {
            SchematicSelection::Wire(idx) if idx < self.schematic.wires.len() => Some(idx),
            _ => None,
        }
    }

    pub fn clear_selection(&mut self) {
        self.selection = SchematicSelection::None;
    }

    pub fn select_component(&mut self, idx: usize) {
        if idx < self.schematic.components.len() {
            self.selection = SchematicSelection::Component(idx);
        }
    }

    pub fn select_wire(&mut self, idx: usize) {
        if idx < self.schematic.wires.len() {
            self.selection = SchematicSelection::Wire(idx);
        }
    }

    pub fn duplicate_selection(&mut self) -> bool {
        if let Some(idx) = self.selected_component_index() {
            if self.schematic.duplicate_component(idx).is_some() {
                let new_idx = self.schematic.components.len().saturating_sub(1);
                self.selection = SchematicSelection::Component(new_idx);
                return true;
            }
        }
        false
    }

    pub fn delete_selection(&mut self) -> bool {
        match self.selection {
            SchematicSelection::Component(idx) => {
                if idx < self.schematic.components.len() {
                    self.schematic.components.remove(idx);
                    self.selection = SchematicSelection::None;
                    return true;
                }
            }
            SchematicSelection::Wire(idx) => {
                if self.schematic.remove_wire(idx) {
                    self.selection = SchematicSelection::None;
                    return true;
                }
            }
            SchematicSelection::None => {}
        }
        false
    }

    fn draw_toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;

            let active_fill = Color32::from_rgb(43, 43, 43);
            let idle_fill = Color32::from_rgb(26, 27, 33);

            if ui
                .add_sized(
                    [44.0, 28.0],
                    egui::Button::new("LIB")
                        .fill(if self.show_library { active_fill } else { idle_fill })
                        .stroke(Stroke::new(1.0, Color32::from_rgb(56, 58, 66))),
                )
                .clicked()
            {
                self.show_library = !self.show_library;
            }

            let wire_active = self.placement == PlacementMode::Wire;
            if ui
                .add_sized(
                    [92.0, 28.0],
                    egui::Button::new(t("app.wire_mode", self.lang))
                        .fill(if wire_active { active_fill } else { idle_fill })
                        .stroke(Stroke::new(1.0, Color32::from_rgb(56, 58, 66))),
                )
                .clicked()
            {
                if wire_active {
                    self.placement = PlacementMode::None;
                    self.wire_start = None;
                } else {
                    self.placement = PlacementMode::Wire;
                }
            }

            if ui
                .add_sized(
                    [96.0, 28.0],
                    egui::Button::new(t("app.electrical_test", self.lang))
                        .fill(idle_fill)
                        .stroke(Stroke::new(1.0, Color32::from_rgb(56, 58, 66))),
                )
                .clicked()
            {
                self.test_results = self.schematic.electrical_test();
                self.show_test_results = true;
            }

            let sim_label = if self.sim_active { "STOP" } else { "SIM" };
            if ui
                .add_sized(
                    [52.0, 28.0],
                    egui::Button::new(sim_label)
                        .fill(if self.sim_active { Color32::from_rgb(72, 28, 28) } else { idle_fill })
                        .stroke(Stroke::new(1.0, Color32::from_rgb(56, 58, 66))),
                )
                .clicked()
            {
                if self.sim_active {
                    self.sim_active = false;
                    self.sim_results = None;
                    self.sim_phase = 0.0;
                } else {
                    let results = self.schematic.simulate_dc();
                    self.sim_active = results.converged;
                    self.sim_results = Some(results);
                }
            }

            if ui
                .add_sized(
                    [72.0, 28.0],
                    egui::Button::new("EXPORT")
                        .fill(idle_fill)
                        .stroke(Stroke::new(1.0, Color32::from_rgb(56, 58, 66))),
                )
                .clicked()
            {
                self.show_export_menu = !self.show_export_menu;
            }

            ui.separator();

            if self.placement != PlacementMode::None {
                if ui.button(t("app.cancel", self.lang)).clicked() {
                    self.placement = PlacementMode::None;
                    self.wire_start = None;
                }

                let placement_label = match self.placement {
                    PlacementMode::Component(idx) => self
                        .library
                        .components
                        .get(idx)
                        .map(|template| format!("{}: {}", t("app.schematic_component", self.lang), template.name))
                        .unwrap_or_else(|| t("app.schematic_component", self.lang).to_string()),
                    PlacementMode::Wire => t("app.wire_mode", self.lang).to_string(),
                    PlacementMode::None => String::new(),
                };

                ui.label(
                    egui::RichText::new(placement_label)
                        .size(11.0)
                        .color(theme::ACCENT),
                );
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(message) = &self.export_message {
                    ui.label(
                        egui::RichText::new(message)
                            .size(10.0)
                            .color(Color32::from_rgb(128, 132, 142)),
                    );
                }

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
                        .color(Color32::from_rgb(120, 124, 136)),
                );
            });
        });
    }

    fn draw_library(&mut self, ui: &mut Ui, rect: Rect) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_rgb(18, 19, 24));
        painter.line_segment(
            [rect.right_top(), rect.right_bottom()],
            Stroke::new(1.0, Color32::from_rgb(36, 38, 44)),
        );

        painter.text(
            Pos2::new(rect.left() + 12.0, rect.top() + 16.0),
            egui::Align2::LEFT_CENTER,
            t("app.schematic_components", self.lang),
            egui::FontId::proportional(11.0),
            Color32::from_rgb(132, 136, 146),
        );

        let mut y = rect.top() + 32.0;
        for (idx, template) in self.library.components.iter().enumerate() {
            let item_rect = Rect::from_min_size(
                Pos2::new(rect.left() + 8.0, y),
                Vec2::new(rect.width() - 16.0, 52.0),
            );
            let response = ui.allocate_rect(item_rect, egui::Sense::click());
            let is_active = self.placement == PlacementMode::Component(idx);
            let fill = if is_active {
                Color32::from_rgb(44, 36, 24)
            } else if response.hovered() {
                Color32::from_rgb(30, 32, 38)
            } else {
                Color32::from_rgb(24, 25, 30)
            };

            painter.rect_filled(item_rect, 6.0, fill);
            painter.rect_stroke(item_rect, 6.0, Stroke::new(1.0, Color32::from_rgb(36, 38, 44)));

            painter.text(
                Pos2::new(item_rect.left() + 10.0, item_rect.top() + 15.0),
                egui::Align2::LEFT_CENTER,
                &template.name,
                egui::FontId::proportional(11.0),
                Color32::from_rgb(224, 226, 232),
            );

            painter.text(
                Pos2::new(item_rect.left() + 10.0, item_rect.top() + 34.0),
                egui::Align2::LEFT_CENTER,
                format!("{} | {}", template.category, template.description),
                egui::FontId::proportional(9.0),
                Color32::from_rgb(118, 122, 132),
            );

            if response.clicked() {
                self.placement = PlacementMode::Component(idx);
                self.wire_start = None;
            }

            y += 58.0;
            if y > rect.bottom() - 52.0 {
                break;
            }
        }
    }
}