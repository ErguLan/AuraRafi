mod canvas;

use egui::{Color32, Stroke, Ui, Vec2};
use raf_core::i18n::t;
use raf_core::Language;
use raf_electronics::{PcbLayout, PcbSyncSummary, Schematic};

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
    selection: PcbSelection,
    tool: PcbTool,
    drag_state: Option<(usize, glam::Vec2)>,
    outline_draft: Vec<glam::Vec2>,
    pub lang: Language,
    show_airwires: bool,
    last_sync: Option<PcbSyncSummary>,
}

impl Default for PcbViewPanel {
    fn default() -> Self {
        Self {
            layout: PcbLayout::new("Untitled PCB"),
            offset: Vec2::new(120.0, 80.0),
            zoom: 1.0,
            selection: PcbSelection::None,
            tool: PcbTool::Select,
            drag_state: None,
            outline_draft: Vec::new(),
            lang: Language::English,
            show_airwires: true,
            last_sync: None,
        }
    }
}

impl PcbViewPanel {
    pub fn show(&mut self, ui: &mut Ui) -> bool {
        let mut changed = false;
        changed |= self.draw_toolbar(ui);
        ui.separator();
        let available = ui.available_rect_before_wrap();
        changed |= self.draw_canvas(ui, available);
        changed
    }

    pub fn sync_from_schematic(&mut self, schematic: &Schematic) -> PcbSyncSummary {
        let summary = self.layout.sync_from_schematic(schematic);
        self.last_sync = Some(summary.clone());
        self.selection = PcbSelection::None;
        summary
    }

    pub fn selection(&self) -> PcbSelection {
        self.selection
    }

    pub fn clear_selection(&mut self) {
        self.selection = PcbSelection::None;
    }

    pub fn select_component(&mut self, idx: usize) {
        if idx < self.layout.components.len() {
            self.selection = PcbSelection::Component(idx);
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
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;

            let active_fill = Color32::from_rgb(43, 43, 43);
            let idle_fill = Color32::from_rgb(26, 27, 33);

            for (tool, label) in [
                (PcbTool::Select, t("app.pcb_tool_select", self.lang)),
                (PcbTool::Route, t("app.pcb_tool_route", self.lang)),
                (PcbTool::Outline, t("app.pcb_tool_outline", self.lang)),
            ] {
                if ui
                    .add_sized(
                        [92.0, 28.0],
                        egui::Button::new(label)
                            .fill(if self.tool == tool { active_fill } else { idle_fill })
                            .stroke(Stroke::new(1.0, Color32::from_rgb(56, 58, 66))),
                    )
                    .clicked()
                {
                    self.tool = tool;
                }
            }

            if ui
                .add_sized(
                    [74.0, 28.0],
                    egui::Button::new(t("app.pcb_airwires", self.lang))
                        .fill(if self.show_airwires { active_fill } else { idle_fill })
                        .stroke(Stroke::new(1.0, Color32::from_rgb(56, 58, 66))),
                )
                .clicked()
            {
                self.show_airwires = !self.show_airwires;
            }

            if self.tool == PcbTool::Outline
                && ui
                    .add_sized([96.0, 28.0], egui::Button::new(t("app.pcb_new_outline", self.lang)))
                    .clicked()
            {
                self.outline_draft.clear();
            }

            if self.selected_airwire_index().is_some()
                && ui
                    .add_sized([126.0, 28.0], egui::Button::new(t("app.pcb_route_selected", self.lang)))
                    .clicked()
            {
                if let Some(idx) = self.selected_airwire_index() {
                    changed |= self.layout.route_airwire(idx);
                    self.selection = PcbSelection::None;
                }
            }

            ui.separator();
            let outline_color = if self.layout.outline_is_closed() {
                Color32::from_rgb(120, 180, 120)
            } else {
                Color32::from_rgb(200, 120, 90)
            };
            ui.label(
                egui::RichText::new(format!(
                    "{}: {}",
                    t("app.pcb_outline_status", self.lang),
                    if self.layout.outline_is_closed() {
                        t("app.pcb_outline_closed", self.lang)
                    } else {
                        t("app.pcb_outline_open", self.lang)
                    }
                ))
                .size(11.0)
                .color(outline_color),
            );

            if let Some(summary) = &self.last_sync {
                ui.separator();
                ui.label(
                    egui::RichText::new(format!(
                        "+{}  ~{}  -{}  |  {} {}",
                        summary.added_components,
                        summary.updated_components,
                        summary.removed_components,
                        summary.nets,
                        t("app.schematic_nets", self.lang),
                    ))
                    .size(11.0)
                    .color(Color32::from_rgb(150, 150, 160)),
                );
            }
        });

        changed
    }
}