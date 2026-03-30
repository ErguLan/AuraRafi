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
        lang: Language,
    ) {
        let is_es = lang == Language::Spanish;
        let heading = if is_es { "Propiedades" } else { "Properties" };
        ui.heading(heading);
        ui.separator();

        let id = match selected {
            Some(id) => id,
            None => {
                let msg = if is_es {
                    "Ninguna entidad seleccionada"
                } else {
                    "No entity selected"
                };
                ui.label(msg);
                return;
            }
        };

        let node = match scene.get_mut(id) {
            Some(n) => n,
            None => {
                let msg = if is_es {
                    "Entidad no encontrada"
                } else {
                    "Entity not found"
                };
                ui.label(msg);
                return;
            }
        };

        egui::ScrollArea::vertical().show(ui, |ui| {
            // Name
            ui.horizontal(|ui| {
                let label = if is_es { "Nombre:" } else { "Name:" };
                ui.label(label);
                ui.text_edit_singleline(&mut node.name);
            });

            ui.add_space(8.0);

            // Transform section
            let transform_label = if is_es { "Transformar" } else { "Transform" };
            egui::CollapsingHeader::new(transform_label)
                .default_open(true)
                .show(ui, |ui| {
                    let pos_label = if is_es { "Posicion:" } else { "Position:" };
                    ui.horizontal(|ui| {
                        ui.label(pos_label);
                    });
                    ui.horizontal(|ui| {
                        ui.label("X:");
                        ui.add(egui::DragValue::new(&mut node.position.x).speed(0.1));
                        ui.label("Y:");
                        ui.add(egui::DragValue::new(&mut node.position.y).speed(0.1));
                        ui.label("Z:");
                        ui.add(egui::DragValue::new(&mut node.position.z).speed(0.1));
                    });

                    let rot_label = if is_es { "Rotacion:" } else { "Rotation:" };
                    ui.horizontal(|ui| {
                        ui.label(rot_label);
                    });
                    ui.horizontal(|ui| {
                        ui.label("X:");
                        ui.add(egui::DragValue::new(&mut node.rotation.x).speed(1.0));
                        ui.label("Y:");
                        ui.add(egui::DragValue::new(&mut node.rotation.y).speed(1.0));
                        ui.label("Z:");
                        ui.add(egui::DragValue::new(&mut node.rotation.z).speed(1.0));
                    });

                    let scale_label = if is_es { "Escala:" } else { "Scale:" };
                    ui.horizontal(|ui| {
                        ui.label(scale_label);
                    });
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
            let material_label = if is_es { "Material" } else { "Material" };
            egui::CollapsingHeader::new(material_label)
                .default_open(true)
                .show(ui, |ui| {
                    let color_label = if is_es { "Color:" } else { "Color:" };
                    ui.horizontal(|ui| {
                        ui.label(color_label);

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

                    // Quick color presets.
                    let presets_label = if is_es { "Preajustes:" } else { "Presets:" };
                    ui.horizontal(|ui| {
                        ui.label(presets_label);
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
                            let btn = egui::Button::new(
                                egui::RichText::new(label)
                                    .small()
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(btn_color)
                            .min_size(egui::Vec2::new(20.0, 18.0));
                            if ui.add(btn).clicked() {
                                node.color = preset_color;
                            }
                        }
                    });
                });

            ui.add_space(4.0);

            // Primitive type
            let shape_label = if is_es { "Forma" } else { "Shape" };
            egui::CollapsingHeader::new(shape_label)
                .default_open(false)
                .show(ui, |ui| {
                    let type_label = if is_es { "Tipo:" } else { "Type:" };
                    ui.horizontal(|ui| {
                        ui.label(type_label);
                        let current_label = if is_es {
                            node.primitive.label_es()
                        } else {
                            node.primitive.label()
                        };
                        egui::ComboBox::from_id_salt("primitive_select")
                            .selected_text(current_label)
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
                                    let label = if is_es { prim.label_es() } else { prim.label() };
                                    if ui.selectable_value(&mut node.primitive, prim, label).changed() {
                                        // Update color to match new primitive (optional).
                                        node.color = NodeColor::for_primitive(prim);
                                    }
                                }
                            });
                    });
                });

            ui.add_space(4.0);

            // Visibility
            ui.horizontal(|ui| {
                let vis_label = if is_es { "Visible:" } else { "Visible:" };
                ui.label(vis_label);
                ui.checkbox(&mut node.visible, "");
            });

            ui.add_space(8.0);

            // Variables (placeholder)
            let vars_label = if is_es { "Variables" } else { "Variables" };
            egui::CollapsingHeader::new(vars_label)
                .default_open(false)
                .show(ui, |ui| {
                    let msg = if is_es {
                        "Sin variables definidas"
                    } else {
                        "No custom variables defined"
                    };
                    ui.label(msg);
                });
        });
    }
}
