use egui::{Color32, Stroke, Ui};
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_electronics::component::ElectronicComponent;

use crate::panels::schematic_view::{electronics_palette, SchematicSelection, SchematicViewPanel};
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
            .color(electronics_palette(ui.visuals().dark_mode).text_dim),
    );
    ui.separator();

    let root_selected = view.selection() == SchematicSelection::None;
    if selectable_schematic_row(ui, &t("app.schematic_root", lang), root_selected) {
        view.clear_selection();
        selection_changed = true;
    }

    ui.add_space(8.0);
    summary_chip(
        ui,
        &t("app.schematic_components", lang),
        &view.schematic.components.len().to_string(),
    );
    summary_chip(
        ui,
        &t("app.schematic_wires", lang),
        &view.schematic.wires.len().to_string(),
    );
    summary_chip(
        ui,
        &t("app.schematic_nets", lang),
        &view.schematic.netlist().nets.len().to_string(),
    );

    ui.add_space(10.0);
    ui.label(
        egui::RichText::new(t("app.schematic_components", lang))
            .size(10.0)
            .color(electronics_palette(ui.visuals().dark_mode).text_muted),
    );
    ui.add_space(4.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        for idx in 0..view.schematic.components.len() {
            let selected = view.is_component_selected(idx);
            let label = {
                let comp = &view.schematic.components[idx];
                format!("{}  {}", comp.designator, comp.value)
            };
            if selectable_schematic_row(ui, &label, selected) {
                let shift = ui.input(|input| input.modifiers.shift);
                let ctrl = ui.input(|input| input.modifiers.ctrl);
                view.select_component_with_modifiers(idx, shift, ctrl);
                selection_changed = true;
            }
        }

        if !view.schematic.wires.is_empty() {
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new(t("app.schematic_wires", lang))
                    .size(10.0)
                    .color(electronics_palette(ui.visuals().dark_mode).text_muted),
            );
            ui.add_space(4.0);

            for idx in 0..view.schematic.wires.len() {
                let selected = view.selected_wire_indices().contains(&idx);
                let label = if view.schematic.wires[idx].net.trim().is_empty() {
                    format!("{} #{idx}", t("app.schematic_wire", lang))
                } else {
                    format!(
                        "{}  {}",
                        t("app.schematic_wire", lang),
                        view.schematic.wires[idx].net
                    )
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
    let mut pending_anchor_snapshot: Option<ElectronicComponent> = None;

    ui.label(
        egui::RichText::new(t("app.properties", lang))
            .size(11.0)
            .strong()
            .color(electronics_palette(ui.visuals().dark_mode).text_dim),
    );
    ui.separator();

    egui::ScrollArea::vertical().show(ui, |ui| match view.selection() {
        SchematicSelection::Component(idx) => {
            if idx >= view.schematic.components.len() {
                return;
            }

            let component_before = view.schematic.components[idx].clone();

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
                let designator_changed = field_row(
                    ui,
                    &t("app.electronics_reference", lang),
                    &mut comp.designator,
                );
                let value_changed = field_row(ui, &t("app.value", lang), &mut comp.value);
                changed |= designator_changed || value_changed;
                if value_changed {
                    comp.sync_sim_model_from_value();
                }

                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    info_chip(ui, &t("app.schematic_kind", lang), comp.kind_label());
                    info_chip(
                        ui,
                        &t("app.schematic_pins", lang),
                        &comp.pins.len().to_string(),
                    );
                    info_chip(ui, &t("app.schematic_footprint", lang), &comp.footprint);
                });
            });

            ui.add_space(8.0);
            inspector_card(ui, t("app.electronics_position", lang), |ui| {
                let mut position_changed = false;
                ui.horizontal(|ui| {
                    ui.label("X");
                    position_changed |= ui
                        .add(egui::DragValue::new(&mut comp.position.x).speed(1.0))
                        .changed();
                    ui.label("Y");
                    position_changed |= ui
                        .add(egui::DragValue::new(&mut comp.position.y).speed(1.0))
                        .changed();
                });
                ui.horizontal(|ui| {
                    ui.label(t("app.rotation", lang));
                    position_changed |= ui
                        .add(egui::DragValue::new(&mut comp.rotation).speed(1.0))
                        .changed();
                });
                changed |= position_changed;
            });

            if changed
                && (component_before.position != comp.position
                    || (component_before.rotation - comp.rotation).abs() > f32::EPSILON)
            {
                pending_anchor_snapshot = Some(component_before);
            }

            ui.add_space(8.0);
            inspector_card(ui, t("app.electronics_appearance", lang), |ui| {
                changed |= color_row(ui, &t("app.color", lang), &mut comp.appearance.color);
                ui.horizontal(|ui| {
                    ui.label(t("app.electronics_size", lang));
                    egui::ComboBox::from_id_salt("schematic_component_size")
                        .selected_text(&comp.appearance.size)
                        .show_ui(ui, |ui| {
                            changed |= ui
                                .selectable_value(
                                    &mut comp.appearance.size,
                                    "Small".to_string(),
                                    t("app.electronics_size_small", lang),
                                )
                                .changed();
                            changed |= ui
                                .selectable_value(
                                    &mut comp.appearance.size,
                                    "Normal".to_string(),
                                    t("app.electronics_size_normal", lang),
                                )
                                .changed();
                            changed |= ui
                                .selectable_value(
                                    &mut comp.appearance.size,
                                    "Large".to_string(),
                                    t("app.electronics_size_large", lang),
                                )
                                .changed();
                        });
                });
                changed |= ui
                    .checkbox(&mut comp.visible, t("app.visible", lang))
                    .changed();
                changed |= ui
                    .checkbox(&mut comp.locked, t("app.pcb_locked", lang))
                    .changed();
            });

            ui.add_space(8.0);
            inspector_card(ui, t("app.electronics_electrical", lang), |ui| {
                changed |= field_row(ui, &t("app.schematic_footprint", lang), &mut comp.footprint);
                for (pin_idx, pin) in comp.pins.iter().enumerate() {
                    let net_name = pin_nets.get(pin_idx).map(String::as_str).unwrap_or("-");
                    let line = format!(
                        "{}  {}  {net_name}",
                        pin.name,
                        pin_direction_label(pin.direction, lang)
                    );
                    ui.label(
                        egui::RichText::new(line)
                            .size(10.0)
                            .color(electronics_palette(ui.visuals().dark_mode).text_dim),
                    );
                }
            });

            ui.add_space(8.0);
            inspector_card(ui, t("app.electronics_datasheet", lang), |ui| {
                let datasheet = comp.datasheet.get_or_insert_with(String::new);
                changed |= field_row(ui, &t("app.electronics_datasheet", lang), datasheet);
                ui.add_enabled(
                    !datasheet.trim().is_empty(),
                    egui::Button::new(t("app.electronics_open_datasheet", lang)),
                );
            });
        }
        SchematicSelection::MultipleComponents(indices) => {
            let count = indices
                .iter()
                .filter(|idx| **idx < view.schematic.components.len())
                .count();
            inspector_card(ui, t("app.electronics_selection", lang), |ui| {
                info_chip(ui, &t("app.schematic_components", lang), &count.to_string());
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(t("app.electronics_multi_select_hint", lang))
                        .size(10.0)
                        .color(electronics_palette(ui.visuals().dark_mode).text_dim),
                );
            });
        }
        SchematicSelection::Wire(idx) => {
            if idx >= view.schematic.wires.len() {
                return;
            }

            let wire_indices = view.selected_wire_indices();
            let length = wire_indices
                .iter()
                .filter_map(|wire_idx| view.schematic.wires.get(*wire_idx))
                .map(|wire| wire.start.distance(wire.end))
                .sum::<f32>();
            let mut net_name = view.schematic.wires[idx].net.clone();

            inspector_card(ui, t("app.schematic_wire", lang), |ui| {
                if field_row(ui, &t("app.schematic_net", lang), &mut net_name) {
                    for wire_idx in &wire_indices {
                        if let Some(wire) = view.schematic.wires.get_mut(*wire_idx) {
                            wire.net = net_name.clone();
                        }
                    }
                    changed = true;
                }
                ui.add_space(6.0);
                info_chip(
                    ui,
                    &t("app.schematic_length", lang),
                    &format!("{length:.0}"),
                );
                info_chip(
                    ui,
                    &t("app.electronics_segments", lang),
                    &wire_indices.len().to_string(),
                );
            });

            ui.add_space(8.0);
            inspector_card(ui, t("app.schematic_endpoints", lang), |ui| {
                let wire = &view.schematic.wires[idx];
                endpoint_row(ui, "A", wire.start.x, wire.start.y);
                endpoint_row(ui, "B", wire.end.x, wire.end.y);
            });
        }
        SchematicSelection::None => {
            inspector_card(ui, t("app.schematic_properties_root", lang), |ui| {
                ui.label(
                    egui::RichText::new(t("app.schematic_root_summary", lang))
                        .size(11.0)
                        .color(electronics_palette(ui.visuals().dark_mode).text_dim),
                );
                ui.add_space(8.0);
                info_chip(
                    ui,
                    &t("app.schematic_components", lang),
                    &view.schematic.components.len().to_string(),
                );
                info_chip(
                    ui,
                    &t("app.schematic_wires", lang),
                    &view.schematic.wires.len().to_string(),
                );
                info_chip(
                    ui,
                    &t("app.schematic_nets", lang),
                    &view.schematic.netlist().nets.len().to_string(),
                );
            });
        }
    });

    if let Some(component_before) = pending_anchor_snapshot.as_ref() {
        view.ensure_wire_anchors_for_component_snapshot(component_before);
    }

    if changed {
        view.schematic.sync_wire_anchors();
    }

    changed
}

fn selectable_schematic_row(ui: &mut Ui, label: &str, selected: bool) -> bool {
    let palette = electronics_palette(ui.visuals().dark_mode);
    let mut button = egui::Button::new(egui::RichText::new(label).size(11.0).color(if selected {
        Color32::WHITE
    } else {
        palette.text
    }))
    .fill(if selected {
        Color32::from_rgba_premultiplied(212, 119, 26, 32)
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
    .min_size(egui::vec2(ui.available_width(), 24.0));

    if !selected {
        button = button.rounding(4.0);
    }

    ui.add(button).clicked()
}

fn summary_chip(ui: &mut Ui, label: &str, value: &str) {
    let palette = electronics_palette(ui.visuals().dark_mode);
    egui::Frame::none()
        .fill(palette.chip_bg)
        .rounding(12.0)
        .inner_margin(egui::Margin::symmetric(8.0, 4.0))
        .stroke(Stroke::new(1.0, palette.border))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(label)
                        .size(10.0)
                        .color(palette.text_muted),
                );
                ui.label(egui::RichText::new(value).size(10.0).color(palette.text));
            });
        });
}

fn inspector_card(ui: &mut Ui, title: String, add_contents: impl FnOnce(&mut Ui)) {
    let palette = electronics_palette(ui.visuals().dark_mode);
    egui::Frame::none()
        .fill(palette.card_bg)
        .rounding(8.0)
        .inner_margin(12.0)
        .stroke(Stroke::new(1.0, palette.border))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(title)
                    .size(12.0)
                    .strong()
                    .color(palette.text),
            );
            ui.add_space(8.0);
            add_contents(ui);
        });
}

fn info_chip(ui: &mut Ui, label: &str, value: &str) {
    let palette = electronics_palette(ui.visuals().dark_mode);
    egui::Frame::none()
        .fill(palette.chip_bg)
        .rounding(14.0)
        .inner_margin(egui::Margin::symmetric(8.0, 4.0))
        .stroke(Stroke::new(1.0, palette.border))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(label)
                        .size(10.0)
                        .color(palette.text_muted),
                );
                ui.label(egui::RichText::new(value).size(10.0).color(palette.text));
            });
        });
}

fn field_row(ui: &mut Ui, label: &str, value: &mut String) -> bool {
    let palette = electronics_palette(ui.visuals().dark_mode);
    let mut changed = false;
    ui.label(
        egui::RichText::new(label)
            .size(10.0)
            .color(palette.text_dim),
    );
    changed |= ui
        .add_sized(
            [ui.available_width(), 26.0],
            egui::TextEdit::singleline(value),
        )
        .changed();
    changed
}

fn endpoint_row(ui: &mut Ui, label: &str, x: f32, y: f32) {
    let palette = electronics_palette(ui.visuals().dark_mode);
    ui.label(
        egui::RichText::new(format!("{label}: ({x:.0}, {y:.0})"))
            .size(10.0)
            .color(palette.text_dim),
    );
}

fn pin_direction_label(
    direction: raf_electronics::component::PinDirection,
    lang: Language,
) -> &'static str {
    match (lang, direction) {
        (Language::Spanish, raf_electronics::component::PinDirection::Input) => "Entrada",
        (Language::Spanish, raf_electronics::component::PinDirection::Output) => "Salida",
        (Language::Spanish, raf_electronics::component::PinDirection::Bidirectional) => {
            "Bidireccional"
        }
        (Language::Spanish, raf_electronics::component::PinDirection::Power) => "Energia",
        (Language::Spanish, raf_electronics::component::PinDirection::Ground) => "Tierra",
        (_, raf_electronics::component::PinDirection::Input) => "Input",
        (_, raf_electronics::component::PinDirection::Output) => "Output",
        (_, raf_electronics::component::PinDirection::Bidirectional) => "Bidirectional",
        (_, raf_electronics::component::PinDirection::Power) => "Power",
        (_, raf_electronics::component::PinDirection::Ground) => "Ground",
    }
}

fn color_row(ui: &mut Ui, label: &str, value: &mut [u8; 4]) -> bool {
    let palette = electronics_palette(ui.visuals().dark_mode);
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(label)
                .size(10.0)
                .color(palette.text_dim),
        );
        let mut color = Color32::from_rgba_premultiplied(value[0], value[1], value[2], value[3]);
        if ui.color_edit_button_srgba(&mut color).changed() {
            *value = [color.r(), color.g(), color.b(), color.a()];
            changed = true;
        }
    });
    changed
}
