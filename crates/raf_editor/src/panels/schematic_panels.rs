use egui::{Color32, Stroke, Ui};
use raf_core::config::Language;
use raf_core::i18n::t;

use crate::panels::schematic_view::{SchematicSelection, SchematicViewPanel};
use crate::theme;

pub fn show_schematic_hierarchy(
    ui: &mut Ui,
    view: &mut SchematicViewPanel,
    lang: Language,
) -> bool {
    let mut selection_changed = false;

    ui.label(
        egui::RichText::new(t("app.hierarchy", lang))
            .size(11.0)
            .strong()
            .color(Color32::from_rgb(130, 130, 140)),
    );
    ui.separator();

    let root_selected = view.selection() == SchematicSelection::None;
    if selectable_schematic_row(ui, &t("app.schematic_root", lang), root_selected) {
        view.clear_selection();
        selection_changed = true;
    }

    ui.add_space(8.0);
    summary_chip(ui, &t("app.schematic_components", lang), &view.schematic.components.len().to_string());
    summary_chip(ui, &t("app.schematic_wires", lang), &view.schematic.wires.len().to_string());
    summary_chip(ui, &t("app.schematic_nets", lang), &view.schematic.netlist().nets.len().to_string());

    ui.add_space(10.0);
    ui.label(
        egui::RichText::new(t("app.schematic_components", lang))
            .size(10.0)
            .color(Color32::from_rgb(110, 110, 120)),
    );
    ui.add_space(4.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        for idx in 0..view.schematic.components.len() {
            let selected = view.selection() == SchematicSelection::Component(idx);
            let label = {
                let comp = &view.schematic.components[idx];
                format!("{}  {}", comp.designator, comp.value)
            };
            if selectable_schematic_row(ui, &label, selected) {
                view.select_component(idx);
                selection_changed = true;
            }
        }

        if !view.schematic.wires.is_empty() {
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new(t("app.schematic_wires", lang))
                    .size(10.0)
                    .color(Color32::from_rgb(110, 110, 120)),
            );
            ui.add_space(4.0);

            for idx in 0..view.schematic.wires.len() {
                let selected = view.selection() == SchematicSelection::Wire(idx);
                let label = if view.schematic.wires[idx].net.trim().is_empty() {
                    format!("{} #{idx}", t("app.schematic_wire", lang))
                } else {
                    format!("{}  {}", t("app.schematic_wire", lang), view.schematic.wires[idx].net)
                };
                if selectable_schematic_row(ui, &label, selected) {
                    view.select_wire(idx);
                    selection_changed = true;
                }
            }
        }
    });

    selection_changed
}

pub fn show_schematic_properties(
    ui: &mut Ui,
    view: &mut SchematicViewPanel,
    lang: Language,
) -> bool {
    let mut changed = false;

    ui.label(
        egui::RichText::new(t("app.properties", lang))
            .size(11.0)
            .strong()
            .color(Color32::from_rgb(130, 130, 140)),
    );
    ui.separator();

    egui::ScrollArea::vertical().show(ui, |ui| match view.selection() {
        SchematicSelection::Component(idx) => {
            if idx >= view.schematic.components.len() {
                return;
            }

            let pin_nets: Vec<String> = {
                let netlist = view.schematic.netlist();
                view.schematic.components[idx]
                    .pins
                    .iter()
                    .enumerate()
                    .map(|(pin_idx, _)| {
                        netlist
                            .net_for_pin(idx, pin_idx)
                            .map(|net| net.name.clone())
                            .unwrap_or_else(|| "-".to_string())
                    })
                    .collect()
            };
            let comp = &mut view.schematic.components[idx];

            inspector_card(ui, t("app.schematic_component", lang), |ui| {
                let designator_changed = field_row(ui, &t("app.name", lang), &mut comp.designator);
                let value_changed = field_row(ui, &t("app.value", lang), &mut comp.value);
                changed |= designator_changed || value_changed;
                if value_changed {
                    comp.sync_sim_model_from_value();
                }

                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    info_chip(ui, &t("app.schematic_kind", lang), comp.kind_label());
                    info_chip(ui, &t("app.schematic_pins", lang), &comp.pins.len().to_string());
                    info_chip(ui, &t("app.rotation", lang), &format!("{}deg", comp.rotation as i32));
                });
            });

            ui.add_space(8.0);
            inspector_card(ui, t("app.schematic_properties_root", lang), |ui| {
                changed |= field_row(ui, &t("app.schematic_footprint", lang), &mut comp.footprint);
                ui.horizontal(|ui| {
                    ui.label(t("app.rotation", lang));
                    changed |= ui.add(egui::DragValue::new(&mut comp.rotation).speed(1.0)).changed();
                });
            });

            ui.add_space(8.0);
            inspector_card(ui, t("app.schematic_pins", lang), |ui| {
                for (pin_idx, pin) in comp.pins.iter().enumerate() {
                    let net_name = pin_nets.get(pin_idx).map(String::as_str).unwrap_or("-");
                    let line = format!("{}  {}  {net_name}", pin.name, pin_direction_label(pin.direction, lang));
                    ui.label(
                        egui::RichText::new(line)
                            .size(10.0)
                            .color(Color32::from_rgb(180, 180, 190)),
                    );
                }
            });
        }
        SchematicSelection::Wire(idx) => {
            if idx >= view.schematic.wires.len() {
                return;
            }

            let wire = &mut view.schematic.wires[idx];
            let length = wire.start.distance(wire.end);

            inspector_card(ui, t("app.schematic_wire", lang), |ui| {
                changed |= field_row(ui, &t("app.schematic_net", lang), &mut wire.net);
                ui.add_space(6.0);
                info_chip(ui, &t("app.schematic_length", lang), &format!("{length:.0}"));
            });

            ui.add_space(8.0);
            inspector_card(ui, t("app.schematic_endpoints", lang), |ui| {
                endpoint_row(ui, "A", wire.start.x, wire.start.y);
                endpoint_row(ui, "B", wire.end.x, wire.end.y);
            });
        }
        SchematicSelection::None => {
            inspector_card(ui, t("app.schematic_properties_root", lang), |ui| {
                ui.label(
                    egui::RichText::new(t("app.schematic_root_summary", lang))
                        .size(11.0)
                        .color(Color32::from_rgb(150, 150, 160)),
                );
                ui.add_space(8.0);
                info_chip(ui, &t("app.schematic_components", lang), &view.schematic.components.len().to_string());
                info_chip(ui, &t("app.schematic_wires", lang), &view.schematic.wires.len().to_string());
                info_chip(ui, &t("app.schematic_nets", lang), &view.schematic.netlist().nets.len().to_string());
            });
        }
    });

    changed
}

fn selectable_schematic_row(ui: &mut Ui, label: &str, selected: bool) -> bool {
    let mut button = egui::Button::new(
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

    if !selected {
        button = button.rounding(4.0);
    }

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

fn field_row(ui: &mut Ui, label: &str, value: &mut String) -> bool {
    let mut changed = false;
    ui.label(
        egui::RichText::new(label)
            .size(10.0)
            .color(Color32::from_rgb(130, 130, 140)),
    );
    changed |= ui
        .add_sized([ui.available_width(), 26.0], egui::TextEdit::singleline(value))
        .changed();
    changed
}

fn endpoint_row(ui: &mut Ui, label: &str, x: f32, y: f32) {
    ui.label(
        egui::RichText::new(format!("{label}: ({x:.0}, {y:.0})"))
            .size(10.0)
            .color(Color32::from_rgb(175, 175, 184)),
    );
}

fn pin_direction_label(direction: raf_electronics::component::PinDirection, lang: Language) -> &'static str {
    match (lang, direction) {
        (Language::Spanish, raf_electronics::component::PinDirection::Input) => "Entrada",
        (Language::Spanish, raf_electronics::component::PinDirection::Output) => "Salida",
        (Language::Spanish, raf_electronics::component::PinDirection::Bidirectional) => "Bidireccional",
        (Language::Spanish, raf_electronics::component::PinDirection::Power) => "Energia",
        (Language::Spanish, raf_electronics::component::PinDirection::Ground) => "Tierra",
        (_, raf_electronics::component::PinDirection::Input) => "Input",
        (_, raf_electronics::component::PinDirection::Output) => "Output",
        (_, raf_electronics::component::PinDirection::Bidirectional) => "Bidirectional",
        (_, raf_electronics::component::PinDirection::Power) => "Power",
        (_, raf_electronics::component::PinDirection::Ground) => "Ground",
    }
}