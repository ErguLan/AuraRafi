use egui::{Color32, Stroke, Ui};
use raf_core::config::Language;
use raf_core::i18n::t;

use crate::panels::pcb_view::{PcbSelection, PcbViewPanel};
use crate::theme;

pub fn show_pcb_hierarchy(ui: &mut Ui, view: &mut PcbViewPanel, lang: Language) -> bool {
    let mut selection_changed = false;

    ui.label(
        egui::RichText::new(t("app.hierarchy", lang))
            .size(11.0)
            .strong()
            .color(Color32::from_rgb(130, 130, 140)),
    );
    ui.separator();

    let root_selected = view.selection() == PcbSelection::None;
    if selectable_row(ui, &t("app.pcb_board_root", lang), root_selected) {
        view.clear_selection();
        selection_changed = true;
    }

    ui.add_space(8.0);
    summary_chip(ui, &t("app.pcb_components", lang), &view.layout.components.len().to_string());
    summary_chip(ui, &t("app.pcb_traces", lang), &view.layout.traces.len().to_string());
    summary_chip(ui, &t("app.pcb_airwires", lang), &view.layout.airwires.len().to_string());

    egui::ScrollArea::vertical().show(ui, |ui| {
        if !view.layout.components.is_empty() {
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new(t("app.pcb_components", lang))
                    .size(10.0)
                    .color(Color32::from_rgb(110, 110, 120)),
            );
            ui.add_space(4.0);

            for idx in 0..view.layout.components.len() {
                let component = &view.layout.components[idx];
                let selected = view.selection() == PcbSelection::Component(idx);
                let label = format!("{}  {}", component.designator, component.footprint);
                if selectable_row(ui, &label, selected) {
                    view.select_component(idx);
                    selection_changed = true;
                }
            }
        }

        if !view.layout.traces.is_empty() {
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new(t("app.pcb_traces", lang))
                    .size(10.0)
                    .color(Color32::from_rgb(110, 110, 120)),
            );
            ui.add_space(4.0);

            for idx in 0..view.layout.traces.len() {
                let trace = &view.layout.traces[idx];
                let selected = view.selection() == PcbSelection::Trace(idx);
                let label = format!("{}  {}", t("app.pcb_trace", lang), trace.net);
                if selectable_row(ui, &label, selected) {
                    view.select_trace(idx);
                    selection_changed = true;
                }
            }
        }

        if !view.layout.airwires.is_empty() {
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new(t("app.pcb_airwires", lang))
                    .size(10.0)
                    .color(Color32::from_rgb(110, 110, 120)),
            );
            ui.add_space(4.0);

            for idx in 0..view.layout.airwires.len() {
                let airwire = &view.layout.airwires[idx];
                let selected = view.selection() == PcbSelection::Airwire(idx);
                let label = format!("{}  {}", t("app.pcb_airwire", lang), airwire.net);
                if selectable_row(ui, &label, selected) {
                    view.select_airwire(idx);
                    selection_changed = true;
                }
            }
        }
    });

    selection_changed
}

pub fn show_pcb_properties(ui: &mut Ui, view: &mut PcbViewPanel, lang: Language) -> bool {
    let mut changed = false;

    ui.label(
        egui::RichText::new(t("app.properties", lang))
            .size(11.0)
            .strong()
            .color(Color32::from_rgb(130, 130, 140)),
    );
    ui.separator();

    egui::ScrollArea::vertical().show(ui, |ui| match view.selection() {
        PcbSelection::Component(idx) => {
            if idx >= view.layout.components.len() {
                return;
            }

            let component = &mut view.layout.components[idx];
            inspector_card(ui, t("app.pcb_component", lang), |ui| {
                info_chip(ui, &t("app.name", lang), &component.designator);
                info_chip(ui, &t("app.schematic_footprint", lang), &component.footprint);
                info_chip(ui, &t("app.value", lang), &component.value);
            });

            ui.add_space(8.0);
            inspector_card(ui, t("app.pcb_placement", lang), |ui| {
                ui.horizontal(|ui| {
                    ui.label(t("app.position", lang));
                    changed |= ui.add(egui::DragValue::new(&mut component.position.x).speed(1.0)).changed();
                    changed |= ui.add(egui::DragValue::new(&mut component.position.y).speed(1.0)).changed();
                });
                ui.horizontal(|ui| {
                    ui.label(t("app.rotation", lang));
                    changed |= ui.add(egui::DragValue::new(&mut component.rotation).speed(1.0)).changed();
                });
                ui.horizontal(|ui| {
                    ui.label(t("app.pcb_layer", lang));
                    egui::ComboBox::from_id_salt(format!("pcb_layer_{idx}"))
                        .selected_text(component.layer.display_name())
                        .show_ui(ui, |ui| {
                            changed |= ui
                                .selectable_value(
                                    &mut component.layer,
                                    raf_electronics::PcbLayer::TopCopper,
                                    t("app.pcb_layer_top", lang),
                                )
                                .changed();
                            changed |= ui
                                .selectable_value(
                                    &mut component.layer,
                                    raf_electronics::PcbLayer::BottomCopper,
                                    t("app.pcb_layer_bottom", lang),
                                )
                                .changed();
                        });
                });
                changed |= ui.checkbox(&mut component.locked, t("app.pcb_locked", lang)).changed();
            });
        }
        PcbSelection::Trace(idx) => {
            if idx >= view.layout.traces.len() {
                return;
            }

            let trace = &mut view.layout.traces[idx];
            inspector_card(ui, t("app.pcb_trace", lang), |ui| {
                info_chip(ui, &t("app.schematic_net", lang), &trace.net);
                ui.horizontal(|ui| {
                    ui.label(t("app.pcb_width", lang));
                    changed |= ui.add(egui::DragValue::new(&mut trace.width).speed(0.5)).changed();
                });
                ui.horizontal(|ui| {
                    ui.label(t("app.pcb_layer", lang));
                    egui::ComboBox::from_id_salt(format!("trace_layer_{idx}"))
                        .selected_text(trace.layer.display_name())
                        .show_ui(ui, |ui| {
                            changed |= ui
                                .selectable_value(
                                    &mut trace.layer,
                                    raf_electronics::PcbLayer::TopCopper,
                                    t("app.pcb_layer_top", lang),
                                )
                                .changed();
                            changed |= ui
                                .selectable_value(
                                    &mut trace.layer,
                                    raf_electronics::PcbLayer::BottomCopper,
                                    t("app.pcb_layer_bottom", lang),
                                )
                                .changed();
                        });
                });
                info_chip(ui, &t("app.pcb_segments", lang), &trace.points.len().saturating_sub(1).to_string());
            });
        }
        PcbSelection::Airwire(idx) => {
            if idx >= view.layout.airwires.len() {
                return;
            }

            let airwire = &view.layout.airwires[idx];
            inspector_card(ui, t("app.pcb_airwire", lang), |ui| {
                info_chip(ui, &t("app.schematic_net", lang), &airwire.net);
                ui.label(
                    egui::RichText::new(t("app.pcb_route_selected_hint", lang))
                        .size(10.0)
                        .color(Color32::from_rgb(180, 180, 188)),
                );
            });
        }
        PcbSelection::None => {
            inspector_card(ui, t("app.pcb_board_root", lang), |ui| {
                let outline_status = if view.layout.outline_is_closed() {
                    t("app.pcb_outline_closed", lang)
                } else {
                    t("app.pcb_outline_open", lang)
                };
                info_chip(ui, &t("app.pcb_components", lang), &view.layout.components.len().to_string());
                info_chip(ui, &t("app.pcb_traces", lang), &view.layout.traces.len().to_string());
                info_chip(ui, &t("app.pcb_airwires", lang), &view.layout.airwires.len().to_string());
                info_chip(ui, &t("app.pcb_outline_status", lang), &outline_status);
                let size = view.layout.board_size();
                info_chip(ui, &t("app.pcb_board_size", lang), &format!("{:.0} x {:.0}", size.x, size.y));
                info_chip(
                    ui,
                    &t("app.pcb_missing_footprints", lang),
                    &view.layout.missing_footprints().to_string(),
                );
            });
        }
    });

    if changed {
        view.layout.rebuild_airwires();
    }

    changed
}

fn selectable_row(ui: &mut Ui, label: &str, selected: bool) -> bool {
    let button = egui::Button::new(
        egui::RichText::new(label)
            .size(11.0)
            .color(if selected { Color32::WHITE } else { Color32::from_rgb(190, 190, 198) }),
    )
    .fill(if selected {
        Color32::from_rgba_premultiplied(212, 119, 26, 32)
    } else {
        Color32::from_rgb(21, 21, 26)
    })
    .stroke(Stroke::new(
        1.0,
        if selected { theme::ACCENT } else { Color32::from_rgb(40, 40, 46) },
    ))
    .min_size(egui::vec2(ui.available_width(), 24.0));

    ui.add(button).clicked()
}

fn summary_chip(ui: &mut Ui, label: &str, value: &str) {
    egui::Frame::none()
        .fill(Color32::from_rgb(24, 24, 28))
        .rounding(12.0)
        .inner_margin(egui::Margin::symmetric(8.0, 4.0))
        .stroke(Stroke::new(1.0, Color32::from_rgb(42, 42, 48)))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(label)
                        .size(10.0)
                        .color(Color32::from_rgb(120, 120, 130)),
                );
                ui.label(
                    egui::RichText::new(value)
                        .size(10.0)
                        .color(Color32::from_rgb(215, 215, 220)),
                );
            });
        });
}

fn inspector_card(ui: &mut Ui, title: String, add_contents: impl FnOnce(&mut Ui)) {
    egui::Frame::none()
        .fill(Color32::from_rgb(22, 22, 26))
        .rounding(8.0)
        .inner_margin(12.0)
        .stroke(Stroke::new(1.0, Color32::from_rgb(42, 42, 48)))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(title)
                    .size(12.0)
                    .strong()
                    .color(Color32::from_rgb(205, 205, 210)),
            );
            ui.add_space(8.0);
            add_contents(ui);
        });
}

fn info_chip(ui: &mut Ui, label: &str, value: &str) {
    egui::Frame::none()
        .fill(Color32::from_rgb(30, 30, 35))
        .rounding(14.0)
        .inner_margin(egui::Margin::symmetric(8.0, 4.0))
        .stroke(Stroke::new(1.0, Color32::from_rgb(48, 48, 56)))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(label)
                        .size(10.0)
                        .color(Color32::from_rgb(120, 120, 130)),
                );
                ui.label(
                    egui::RichText::new(value)
                        .size(10.0)
                        .color(Color32::from_rgb(215, 215, 220)),
                );
            });
        });
}