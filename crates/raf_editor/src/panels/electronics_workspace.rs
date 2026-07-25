//! Transitional Electronics CAD shell bodies.
//!
//! ApiGraphicBasic owns the canvas and retained shell navigation. These
//! helpers place the existing document-bound hierarchy, library and inspector
//! controls into the CAD-shaped docks while those form controls are migrated
//! one group at a time.

use egui::{Color32, RichText, Stroke, Ui};
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_electronics::drc::{DrcReport, DrcSeverity};
use raf_electronics::schematic::Schematic;
use raf_electronics::simulation::SimulationResults;

use crate::panels::pcb_panels;
use crate::panels::pcb_view::{PcbSelection, PcbViewPanel};
use crate::panels::schematic_panels;
use crate::panels::schematic_view::{electronics_palette, SchematicSelection, SchematicViewPanel};
use crate::theme;

const SECTION_GAP: f32 = 10.0;

/// Actions emitted by the Electronics analysis views. Running checks remains
/// owned by the editor application; this shell only presents existing data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElectronicsAnalysisAction {
    RunDrc,
    RunSimulation,
}

pub fn show_schematic_navigator(
    ui: &mut Ui,
    view: &mut SchematicViewPanel,
    lang: Language,
) -> bool {
    let mut changed = false;
    let palette = electronics_palette(ui.visuals().dark_mode);

    section_title(ui, &t("app.electronics_project", lang));
    let root_selected = view.selection() == SchematicSelection::None;
    if tree_row(
        ui,
        &t("app.electronics_main_schematic", lang),
        root_selected,
    ) {
        view.clear_selection();
        changed = true;
    }
    metadata_row(
        ui,
        &t("app.schematic_components", lang),
        view.schematic.components.len(),
        palette.text_dim,
    );
    metadata_row(
        ui,
        &t("app.schematic_nets", lang),
        view.schematic.netlist().nets.len(),
        palette.text_dim,
    );
    metadata_row(ui, &t("app.electronics_sheets", lang), 1, palette.text_dim);

    ui.add_space(SECTION_GAP);
    ui.separator();
    ui.add_space(SECTION_GAP - 3.0);
    if view.library_visible() {
        view.show_library_panel(ui);
    } else {
        ui.label(
            RichText::new(t("app.electronics_library_hidden", lang))
                .size(10.0)
                .color(palette.text_muted),
        );
    }
    changed
}

pub fn show_pcb_navigator(ui: &mut Ui, view: &mut PcbViewPanel, lang: Language) -> bool {
    let mut changed = false;
    let palette = electronics_palette(ui.visuals().dark_mode);

    section_title(ui, &t("app.electronics_project", lang));
    let root_selected = view.selection() == PcbSelection::None;
    if tree_row(ui, &t("app.electronics_main_pcb", lang), root_selected) {
        view.clear_selection();
        changed = true;
    }
    metadata_row(
        ui,
        &t("app.pcb_components", lang),
        view.layout.components.len(),
        palette.text_dim,
    );
    metadata_row(
        ui,
        &t("app.pcb_traces", lang),
        view.layout.traces.len(),
        palette.text_dim,
    );
    metadata_row(
        ui,
        &t("app.pcb_airwires", lang),
        view.layout.airwires.len(),
        palette.text_dim,
    );

    ui.add_space(SECTION_GAP);
    ui.separator();
    ui.add_space(SECTION_GAP - 3.0);
    section_title(ui, &t("app.electronics_board", lang));
    let size = view.layout.board_size();
    detail_row(
        ui,
        &t("app.pcb_board_size", lang),
        &format!("{:.0} x {:.0}", size.x, size.y),
    );
    let outline_status = if view.layout.outline_is_closed() {
        t("app.pcb_outline_closed", lang)
    } else {
        t("app.pcb_outline_open", lang)
    };
    detail_row(ui, &t("app.pcb_outline_status", lang), &outline_status);
    changed
}

pub fn show_schematic_inspector(
    ui: &mut Ui,
    view: &mut SchematicViewPanel,
    lang: Language,
) -> bool {
    let palette = electronics_palette(ui.visuals().dark_mode);
    section_title(ui, &t("app.electronics_inspector", lang));
    selection_summary_schematic(ui, view, lang, palette.text_dim);
    ui.add_space(6.0);
    ui.separator();
    schematic_panels::show_schematic_properties(ui, view, lang)
}

pub fn show_pcb_inspector(ui: &mut Ui, view: &mut PcbViewPanel, lang: Language) -> bool {
    let palette = electronics_palette(ui.visuals().dark_mode);
    section_title(ui, &t("app.electronics_inspector", lang));
    selection_summary_pcb(ui, view, lang, palette.text_dim);
    ui.add_space(6.0);
    ui.separator();
    pcb_panels::show_pcb_properties(ui, view, lang)
}

pub fn show_drc_results(
    ui: &mut Ui,
    report: Option<&DrcReport>,
    lang: Language,
) -> Option<ElectronicsAnalysisAction> {
    let palette = electronics_palette(ui.visuals().dark_mode);
    let mut action = None;

    ui.horizontal(|ui| {
        ui.label(
            RichText::new(t("app.electronics_drc", lang))
                .size(12.0)
                .strong()
                .color(palette.text),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(
                    egui::Button::new(
                        RichText::new(t("app.electronics_run_drc", lang))
                            .size(10.0)
                            .color(Color32::WHITE),
                    )
                    .fill(theme::ACCENT)
                    .stroke(Stroke::NONE)
                    .rounding(4.0),
                )
                .clicked()
            {
                action = Some(ElectronicsAnalysisAction::RunDrc);
            }
        });
    });
    ui.add_space(6.0);

    let Some(report) = report else {
        empty_analysis_state(ui, &t("app.electronics_drc_empty", lang));
        return action;
    };

    ui.horizontal_wrapped(|ui| {
        analysis_metric(
            ui,
            &t("app.electronics_errors", lang),
            report.errors.len(),
            Color32::from_rgb(224, 84, 84),
        );
        analysis_metric(
            ui,
            &t("app.electronics_warnings", lang),
            report.warnings.len(),
            Color32::from_rgb(236, 172, 72),
        );
        analysis_metric(
            ui,
            &t("app.electronics_info", lang),
            report.info.len(),
            Color32::from_rgb(94, 182, 228),
        );
    });
    ui.add_space(8.0);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if report.total() == 0 {
                empty_analysis_state(ui, &t("app.drc_ok", lang));
                return;
            }

            for issue in report.all_issues() {
                let (label, color) = match issue.severity {
                    DrcSeverity::Error => (
                        t("app.electronics_errors", lang),
                        Color32::from_rgb(224, 84, 84),
                    ),
                    DrcSeverity::Warning => (
                        t("app.electronics_warnings", lang),
                        Color32::from_rgb(236, 172, 72),
                    ),
                    DrcSeverity::Info => (
                        t("app.electronics_info", lang),
                        Color32::from_rgb(94, 182, 228),
                    ),
                };
                analysis_issue(ui, &label, &issue.rule, &issue.message, color);
            }
        });

    action
}

pub fn show_simulation_results(
    ui: &mut Ui,
    schematic: &Schematic,
    results: Option<&SimulationResults>,
    lang: Language,
) -> Option<ElectronicsAnalysisAction> {
    let palette = electronics_palette(ui.visuals().dark_mode);
    let mut action = None;

    ui.horizontal(|ui| {
        ui.label(
            RichText::new(t("app.electronics_simulation", lang))
                .size(12.0)
                .strong()
                .color(palette.text),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(
                    egui::Button::new(
                        RichText::new(t("app.electronics_run_simulation", lang))
                            .size(10.0)
                            .color(Color32::WHITE),
                    )
                    .fill(theme::ACCENT)
                    .stroke(Stroke::NONE)
                    .rounding(4.0),
                )
                .clicked()
            {
                action = Some(ElectronicsAnalysisAction::RunSimulation);
            }
        });
    });
    ui.add_space(6.0);

    let Some(results) = results else {
        empty_analysis_state(ui, &t("app.electronics_simulation_empty", lang));
        return action;
    };

    let (state, state_color) = if results.converged {
        (
            t("app.electronics_converged", lang),
            Color32::from_rgb(104, 204, 132),
        )
    } else {
        (
            t("app.electronics_failed", lang),
            Color32::from_rgb(224, 84, 84),
        )
    };
    ui.horizontal_wrapped(|ui| {
        analysis_metric(
            ui,
            &t("app.electronics_nets", lang),
            results.node_voltages.len(),
            palette.text_dim,
        );
        analysis_metric(
            ui,
            &t("app.electronics_branches", lang),
            results.component_currents.len(),
            palette.text_dim,
        );
        ui.label(RichText::new(state).size(10.0).color(state_color));
    });
    ui.add_space(8.0);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if !results.messages.is_empty() {
                for message in &results.messages {
                    analysis_issue(ui, "", "", message, palette.text_muted);
                }
                ui.add_space(5.0);
            }

            section_title(ui, &t("app.electronics_nets", lang));
            let mut voltages: Vec<_> = results.node_voltages.iter().collect();
            voltages.sort_by_key(|(id, _)| **id);
            for (net, voltage) in voltages {
                analysis_value_row(ui, &format!("Net-{net}"), &format!("{voltage:.3} V"));
            }

            ui.add_space(8.0);
            section_title(ui, &t("app.electronics_branches", lang));
            let mut currents: Vec<_> = results.component_currents.iter().collect();
            currents.sort_by_key(|(index, _)| **index);
            for (index, current) in currents {
                let label = schematic
                    .components
                    .get(*index)
                    .map(|component| component.designator.clone())
                    .unwrap_or_else(|| format!("#{}", index + 1));
                let power = results
                    .component_power
                    .get(index)
                    .copied()
                    .unwrap_or_default();
                analysis_value_row(ui, &label, &format!("{current:.4} A | {power:.4} W"));
            }
        });

    action
}

fn section_title(ui: &mut Ui, title: &str) {
    let palette = electronics_palette(ui.visuals().dark_mode);
    ui.label(
        RichText::new(title)
            .size(10.0)
            .strong()
            .color(palette.text_dim),
    );
    ui.add_space(5.0);
}

fn tree_row(ui: &mut Ui, title: &str, selected: bool) -> bool {
    let palette = electronics_palette(ui.visuals().dark_mode);
    let button = egui::Button::new(RichText::new(title).size(11.0).color(if selected {
        Color32::from_rgb(255, 190, 108)
    } else {
        palette.text
    }))
    .fill(if selected {
        Color32::from_rgb(47, 33, 18)
    } else {
        palette.card_bg
    })
    .stroke(Stroke::new(
        1.0,
        if selected {
            theme::ACCENT
        } else {
            palette.border
        },
    ))
    .rounding(5.0)
    .min_size(egui::vec2(ui.available_width(), 30.0));
    ui.add(button).clicked()
}

fn metadata_row(ui: &mut Ui, label: &str, count: usize, color: Color32) {
    ui.horizontal(|ui| {
        ui.add_space(9.0);
        ui.label(RichText::new(label).size(10.0).color(color));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(count.to_string())
                    .size(10.0)
                    .color(Color32::WHITE),
            );
        });
    });
    ui.add_space(2.0);
}

fn detail_row(ui: &mut Ui, label: &str, value: &str) {
    let palette = electronics_palette(ui.visuals().dark_mode);
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(10.0).color(palette.text_dim));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(value).size(10.0).color(palette.text));
        });
    });
    ui.add_space(3.0);
}

fn analysis_metric(ui: &mut Ui, label: &str, value: usize, color: Color32) {
    let palette = electronics_palette(ui.visuals().dark_mode);
    egui::Frame::default()
        .fill(palette.card_bg)
        .stroke(Stroke::new(1.0, palette.border))
        .rounding(4.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(value.to_string())
                        .size(12.0)
                        .strong()
                        .color(color),
                );
                ui.label(RichText::new(label).size(9.0).color(palette.text_dim));
            });
        });
}

fn empty_analysis_state(ui: &mut Ui, message: &str) {
    let palette = electronics_palette(ui.visuals().dark_mode);
    egui::Frame::default()
        .fill(palette.card_bg)
        .stroke(Stroke::new(1.0, palette.border))
        .rounding(5.0)
        .show(ui, |ui| {
            ui.add_space(10.0);
            ui.label(RichText::new(message).size(11.0).color(palette.text_dim));
            ui.add_space(10.0);
        });
}

fn analysis_issue(ui: &mut Ui, severity: &str, rule: &str, message: &str, color: Color32) {
    let palette = electronics_palette(ui.visuals().dark_mode);
    egui::Frame::default()
        .fill(palette.card_bg)
        .stroke(Stroke::new(1.0, palette.border))
        .rounding(4.0)
        .show(ui, |ui| {
            if !severity.is_empty() || !rule.is_empty() {
                ui.horizontal(|ui| {
                    if !severity.is_empty() {
                        ui.label(RichText::new(severity).size(9.0).strong().color(color));
                    }
                    if !rule.is_empty() {
                        ui.label(RichText::new(rule).size(9.0).color(palette.text_muted));
                    }
                });
                ui.add_space(2.0);
            }
            ui.label(RichText::new(message).size(10.0).color(palette.text));
        });
    ui.add_space(4.0);
}

fn analysis_value_row(ui: &mut Ui, label: &str, value: &str) {
    let palette = electronics_palette(ui.visuals().dark_mode);
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(10.0).color(palette.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(value).size(10.0).color(palette.text_dim));
        });
    });
    ui.add_space(2.0);
}

fn selection_summary_schematic(
    ui: &mut Ui,
    view: &SchematicViewPanel,
    lang: Language,
    muted: Color32,
) {
    let summary = match view.selection() {
        SchematicSelection::Component(index) => view
            .schematic
            .components
            .get(index)
            .map(|component| format!("{} ({})", component.designator, component.kind_label()))
            .unwrap_or_else(|| t("app.no_entity_selected", lang)),
        SchematicSelection::MultipleComponents(indices) => {
            format!("{}: {}", t("app.schematic_components", lang), indices.len())
        }
        SchematicSelection::Wire(index) => view
            .schematic
            .wires
            .get(index)
            .map(|wire| {
                if wire.net.trim().is_empty() {
                    format!("{} #{}", t("app.schematic_wire", lang), index + 1)
                } else {
                    wire.net.clone()
                }
            })
            .unwrap_or_else(|| t("app.no_entity_selected", lang)),
        SchematicSelection::None => t("app.electronics_no_selection", lang),
    };
    ui.label(RichText::new(summary).size(11.0).color(muted));
}

fn selection_summary_pcb(ui: &mut Ui, view: &PcbViewPanel, lang: Language, muted: Color32) {
    let summary = match view.selection() {
        PcbSelection::Component(index) => view
            .layout
            .components
            .get(index)
            .map(|component| format!("{} ({})", component.designator, component.footprint))
            .unwrap_or_else(|| t("app.no_entity_selected", lang)),
        PcbSelection::Trace(index) => view
            .layout
            .traces
            .get(index)
            .map(|trace| format!("{}: {}", t("app.pcb_trace", lang), trace.net))
            .unwrap_or_else(|| t("app.no_entity_selected", lang)),
        PcbSelection::Airwire(index) => view
            .layout
            .airwires
            .get(index)
            .map(|wire| format!("{}: {}", t("app.pcb_airwire", lang), wire.net))
            .unwrap_or_else(|| t("app.no_entity_selected", lang)),
        PcbSelection::None => t("app.electronics_no_selection", lang),
    };
    ui.label(RichText::new(summary).size(11.0).color(muted));
}
