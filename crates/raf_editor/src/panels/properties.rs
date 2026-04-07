//! Properties / Inspector panel - edit selected entity's properties.
//! Supports: name, transform, visibility, color, and primitive type.
//! All text translated ES/EN.

use egui::Ui;
use raf_core::config::Language;
use raf_core::scene::graph::{NodeColor, Primitive, SceneGraph, SceneNodeId};

/// State for the properties panel.
pub struct PropertiesPanel;

impl Default for PropertiesPanel {
    fn default() -> Self {
        Self
    }
}

impl PropertiesPanel {
    /// Draw the properties panel for the selected node.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        scene: &mut SceneGraph,
        selected: Option<SceneNodeId>,
        _lang: Language,
    ) {
        // Professional uppercase header.
        ui.label(
            egui::RichText::new("PROPERTIES")
                .size(10.0)
                .color(egui::Color32::from_rgb(130, 130, 140)),
        );
        ui.separator();

        let id = match selected {
            Some(id) => id,
            None => {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("No entity selected")
                        .size(11.0)
                        .color(egui::Color32::from_rgb(100, 100, 110)),
                );
                return;
            }
        };

        let node = match scene.get_mut(id) {
            Some(n) => n,
            None => {
                ui.label(
                    egui::RichText::new("Entity not found")
                        .size(11.0)
                        .color(egui::Color32::from_rgb(100, 100, 110)),
                );
                return;
            }
        };

        egui::ScrollArea::vertical().show(ui, |ui| {
            // Name
            ui.horizontal(|ui| {
                ui.label("Name:");
                ui.text_edit_singleline(&mut node.name);
            });

            ui.add_space(8.0);

            // Transform section
            egui::CollapsingHeader::new("Transform")
                .default_open(true)
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Position:").size(11.0));
                    ui.horizontal(|ui| {
                        ui.label("X:");
                        ui.add(egui::DragValue::new(&mut node.position.x).speed(0.1));
                        ui.label("Y:");
                        ui.add(egui::DragValue::new(&mut node.position.y).speed(0.1));
                        ui.label("Z:");
                        ui.add(egui::DragValue::new(&mut node.position.z).speed(0.1));
                    });

                    ui.label(egui::RichText::new("Rotation:").size(11.0));
                    ui.horizontal(|ui| {
                        ui.label("X:");
                        ui.add(egui::DragValue::new(&mut node.rotation.x).speed(1.0));
                        ui.label("Y:");
                        ui.add(egui::DragValue::new(&mut node.rotation.y).speed(1.0));
                        ui.label("Z:");
                        ui.add(egui::DragValue::new(&mut node.rotation.z).speed(1.0));
                    });

                    ui.label(egui::RichText::new("Scale:").size(11.0));
                    ui.horizontal(|ui| {
                        ui.label("X:");
                        ui.add(egui::DragValue::new(&mut node.scale.x).speed(0.05));
                        ui.label("Y:");
                        ui.add(egui::DragValue::new(&mut node.scale.y).speed(0.05));
                        ui.label("Z:");
                        ui.add(egui::DragValue::new(&mut node.scale.z).speed(0.05));
                    });
                });

            ui.add_space(4.0);

            // Material / Color section
            egui::CollapsingHeader::new("Material")
                .default_open(true)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Color:");

                        // Convert NodeColor to egui color for the picker.
                        let mut rgba = [
                            node.color.r as f32 / 255.0,
                            node.color.g as f32 / 255.0,
                            node.color.b as f32 / 255.0,
                        ];
                        if ui.color_edit_button_rgb(&mut rgba).changed() {
                            node.color = NodeColor::rgb(
                                (rgba[0] * 255.0) as u8,
                                (rgba[1] * 255.0) as u8,
                                (rgba[2] * 255.0) as u8,
                            );
                        }
                    });

                    // Quick color presets - refined small squares with border.
                    ui.horizontal(|ui| {
                        ui.label("Presets:");
                        let color_presets = [
                            ("R", NodeColor::rgb(220, 80, 80)),
                            ("G", NodeColor::rgb(80, 200, 80)),
                            ("B", NodeColor::rgb(80, 130, 220)),
                            ("Y", NodeColor::rgb(220, 200, 60)),
                            ("O", NodeColor::rgb(220, 140, 50)),
                            ("P", NodeColor::rgb(160, 80, 200)),
                            ("W", NodeColor::rgb(220, 220, 220)),
                        ];
                        for (label, preset_color) in color_presets {
                            let btn_color = egui::Color32::from_rgb(
                                preset_color.r,
                                preset_color.g,
                                preset_color.b,
                            );
                            let is_current = node.color.r == preset_color.r
                                && node.color.g == preset_color.g
                                && node.color.b == preset_color.b;
                            let btn = egui::Button::new(
                                egui::RichText::new(label)
                                    .size(9.0)
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(btn_color)
                            .rounding(3.0)
                            .stroke(if is_current {
                                egui::Stroke::new(1.5, egui::Color32::WHITE)
                            } else {
                                egui::Stroke::new(0.5, egui::Color32::from_rgb(60, 60, 65))
                            })
                            .min_size(egui::Vec2::new(20.0, 18.0));
                            if ui.add(btn).clicked() {
                                node.color = preset_color;
                            }
                        }
                    });
                });

            ui.add_space(4.0);

            // Primitive type
            egui::CollapsingHeader::new("Shape")
                .default_open(false)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Type:");
                        egui::ComboBox::from_id_salt("primitive_select")
                            .selected_text(node.primitive.label())
                            .show_ui(ui, |ui| {
                                let primitives = [
                                    Primitive::Cube,
                                    Primitive::Sphere,
                                    Primitive::Plane,
                                    Primitive::Cylinder,
                                    Primitive::Sprite2D,
                                    Primitive::Empty,
                                ];
                                for prim in primitives {
                                    if ui.selectable_value(&mut node.primitive, prim, prim.label()).changed() {
                                        node.color = NodeColor::for_primitive(prim);
                                    }
                                }
                            });
                    });
                });

            ui.add_space(4.0);

            // Visibility
            ui.horizontal(|ui| {
                ui.label("Visible:");
                ui.checkbox(&mut node.visible, "");
            });

            ui.add_space(8.0);

            // Variables (placeholder)
            egui::CollapsingHeader::new("Variables")
                .default_open(false)
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("No custom variables defined")
                            .size(11.0)
                            .color(egui::Color32::from_rgb(100, 100, 110)),
                    );
                });
        });
    }
}
