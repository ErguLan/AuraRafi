//! Properties / Inspector panel - edit selected entity's properties.
//! Supports: name, transform, visibility, color, and primitive type.
//! All text translated ES/EN.

use egui::{Color32, Stroke, Ui};
use glam::Vec3;
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_core::scene::graph::{NodeColor, Primitive, SceneGraph, SceneNodeId};
use std::path::Path;

use crate::ui_icons::UiIconAtlas;

/// State for the properties panel.
pub struct PropertiesPanel {
    script_bindings: crate::panels::behaviors::ScriptBindingsState,
}

impl Default for PropertiesPanel {
    fn default() -> Self {
        Self {
            script_bindings: crate::panels::behaviors::ScriptBindingsState::default(),
        }
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
        icons: &UiIconAtlas,
        project_assets_path: Option<&Path>,
    ) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            if let Some(icon) = icons.get("shape.png") {
                ui.add(
                    egui::Image::new(icon)
                        .fit_to_exact_size(egui::Vec2::new(16.0, 16.0)),
                );
            }
            ui.label(
                egui::RichText::new(t("app.properties", lang))
                    .size(11.0).strong()
                    .color(egui::Color32::from_rgb(130, 130, 140)),
            );
        });
        ui.separator();

        let id = match selected {
            Some(id) => id,
            None => {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(t("app.no_entity_selected", lang))
                        .size(11.0)
                        .color(egui::Color32::from_rgb(100, 100, 110)),
                );
                return false;
            }
        };

        let metadata = match scene.get(id) {
            Some(n) => n,
            None => {
                ui.label(
                    egui::RichText::new(t("app.entity_not_found", lang))
                        .size(11.0)
                        .color(egui::Color32::from_rgb(100, 100, 110)),
                );
                return false;
            }
        };

        let parent_name = metadata
            .parent
            .and_then(|parent_id| scene.get(parent_id).map(|parent| parent.name.clone()))
            .unwrap_or_else(|| t("app.no_parent", lang));
        let children_count = metadata.children.len();
        let scripts_count = metadata.scripts.len();

        egui::ScrollArea::vertical().show(ui, |ui| {
            let node = match scene.get_mut(id) {
                Some(node) => node,
                None => return,
            };

            inspector_card(ui, icons, "scene.png", t("app.node_info", lang), |ui| {
                let name_response = ui.add_sized(
                    [ui.available_width(), 28.0],
                    egui::TextEdit::singleline(&mut node.name),
                );
                changed |= name_response.changed();

                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    info_chip(ui, t("app.type", lang), node.primitive.label());
                    info_chip(ui, t("app.parent", lang), &parent_name);
                    info_chip(ui, t("app.children", lang), &children_count.to_string());
                    info_chip(ui, t("app.scripts", lang), &scripts_count.to_string());
                });
            });

            ui.add_space(8.0);

            inspector_card(ui, icons, "transform.png", t("app.transform", lang), |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new(t("app.quick_actions", lang))
                            .size(11.0)
                            .color(Color32::from_rgb(125, 125, 135)),
                    );

                    if ui.button(t("app.reset", lang)).clicked() {
                        node.position = Vec3::ZERO;
                        node.rotation = Vec3::ZERO;
                        node.scale = Vec3::ONE;
                        changed = true;
                    }
                    if ui.button(t("app.reset_all", lang)).clicked() {
                        node.position = Vec3::ZERO;
                        node.rotation = Vec3::ZERO;
                        node.scale = Vec3::ONE;
                        node.color = NodeColor::for_primitive(node.primitive);
                        node.visible = true;
                        changed = true;
                    }
                });

                ui.add_space(8.0);
                changed |= edit_vec3(ui, t("app.position", lang), &mut node.position, 0.1);
                ui.add_space(6.0);
                changed |= edit_vec3(ui, t("app.rotation", lang), &mut node.rotation, 0.5);
                ui.add_space(6.0);
                changed |= edit_vec3(ui, t("app.scale", lang), &mut node.scale, 0.05);
            });

            ui.add_space(4.0);

            inspector_card(ui, icons, "material.png", t("app.material", lang), |ui| {
                ui.horizontal(|ui| {
                    ui.label(t("app.color", lang));

                    let mut rgba = [
                        node.color.r as f32 / 255.0,
                        node.color.g as f32 / 255.0,
                        node.color.b as f32 / 255.0,
                    ];
                    if ui.color_edit_button_rgb(&mut rgba).changed() {
                        node.color.r = (rgba[0] * 255.0) as u8;
                        node.color.g = (rgba[1] * 255.0) as u8;
                        node.color.b = (rgba[2] * 255.0) as u8;
                        changed = true;
                    }
                });
            });

            ui.add_space(4.0);

            inspector_card(ui, icons, "shape.png", t("app.shape", lang), |ui| {
                ui.horizontal(|ui| {
                    ui.label(t("app.type", lang));
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
                                if ui
                                    .selectable_value(&mut node.primitive, prim, prim.label())
                                    .changed()
                                {
                                    node.color = NodeColor::for_primitive(prim);
                                    changed = true;
                                }
                            }
                        });
                });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(t("app.visible", lang));
                    changed |= ui.checkbox(&mut node.visible, "").changed();
                });
            });

            ui.add_space(4.0);

            inspector_card(ui, icons, "variables.png", t("app.variables", lang), |ui| {
                ui.label(
                    egui::RichText::new(t("app.no_variables", lang))
                        .size(11.0)
                        .color(egui::Color32::from_rgb(100, 100, 110)),
                );
            });
        });

        // Add modular "Attached Behaviors" UI below standard properties.
        // Needs `&mut SceneGraph`, so we call it outside the previous `node` borrow.
        changed |= crate::panels::behaviors::show_attached_behaviors(
            ui,
            scene,
            id,
            lang,
            project_assets_path,
            icons,
            &mut self.script_bindings,
        );

        changed
    }
}

fn inspector_card(
    ui: &mut Ui,
    icons: &UiIconAtlas,
    icon_name: &'static str,
    title: String,
    add_contents: impl FnOnce(&mut Ui),
) {
    egui::Frame::none()
        .fill(Color32::from_rgb(22, 22, 26))
        .rounding(8.0)
        .inner_margin(12.0)
        .stroke(Stroke::new(1.0, Color32::from_rgb(42, 42, 48)))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(icon) = icons.get(icon_name) {
                    ui.add(
                        egui::Image::new(icon)
                            .fit_to_exact_size(egui::Vec2::new(16.0, 16.0)),
                    );
                }
                ui.label(
                    egui::RichText::new(title)
                        .size(12.0)
                        .strong()
                        .color(Color32::from_rgb(205, 205, 210)),
                );
            });
            ui.add_space(8.0);
            add_contents(ui);
        });
}

fn info_chip(ui: &mut Ui, label: String, value: &str) {
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

fn edit_vec3(ui: &mut Ui, label: String, value: &mut Vec3, speed: f64) -> bool {
    let mut changed = false;
    ui.label(
        egui::RichText::new(label)
            .size(11.0)
            .color(Color32::from_rgb(145, 145, 155)),
    );
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.label("X");
        changed |= ui.add(egui::DragValue::new(&mut value.x).speed(speed)).changed();
        ui.label("Y");
        changed |= ui.add(egui::DragValue::new(&mut value.y).speed(speed)).changed();
        ui.label("Z");
        changed |= ui.add(egui::DragValue::new(&mut value.z).speed(speed)).changed();
    });
    changed
}
