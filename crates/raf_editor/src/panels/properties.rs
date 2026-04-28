//! Properties / Inspector panel - edit selected entity's properties.
//! Supports: name, transform, visibility, color, and primitive type.
//! All text translated ES/EN.

use egui::{Color32, Stroke, Ui};
use glam::Vec3;
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_core::scene::graph::{NodeColor, Primitive, SceneGraph, SceneNode, SceneNodeId};
use raf_core::scene::{Collider, ColliderType, SceneVariable, VariableValue, RigidBodyType};
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
                changed |= show_variables_editor(ui, node, lang);
            });

            ui.add_space(4.0);

            inspector_card(ui, icons, "audio.png", t("app.audio_source", lang), |ui| {
                changed |= show_audio_source_editor(ui, node, lang);
            });

            ui.add_space(4.0);

            inspector_card(ui, icons, "shape.png", t("app.physics", lang), |ui| {
                changed |= show_physics_editor(ui, node, lang);
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

fn show_variables_editor(ui: &mut Ui, node: &mut SceneNode, lang: Language) -> bool {
    let mut changed = false;
    let mut remove_index = None;

    if node.variables.is_empty() {
        ui.label(
            egui::RichText::new(t("app.no_variables", lang))
                .size(11.0)
                .color(egui::Color32::from_rgb(100, 100, 110)),
        );
    } else {
        for (index, variable) in node.variables.iter_mut().enumerate() {
            egui::Frame::none()
                .fill(Color32::from_rgb(28, 28, 33))
                .rounding(6.0)
                .inner_margin(8.0)
                .stroke(Stroke::new(1.0, Color32::from_rgb(44, 44, 52)))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        changed |= ui
                            .add(
                                egui::TextEdit::singleline(&mut variable.name)
                                    .hint_text(t("app.variable_name", lang))
                                    .desired_width(120.0),
                            )
                            .changed();

                        let mut selected_kind = variable_kind(variable);
                        egui::ComboBox::from_id_salt(format!("variable_kind_{}", index))
                            .selected_text(selected_kind)
                            .show_ui(ui, |ui| {
                                for kind in ["Bool", "Number", "Text"] {
                                    if ui.selectable_value(&mut selected_kind, kind, kind).changed() {
                                        variable.value = match kind {
                                            "Bool" => VariableValue::Bool(false),
                                            "Number" => VariableValue::Number(0.0),
                                            _ => VariableValue::Text(String::new()),
                                        };
                                        changed = true;
                                    }
                                }
                            });

                        changed |= edit_variable_value(ui, &mut variable.value);

                        if ui.button(t("app.remove", lang)).clicked() {
                            remove_index = Some(index);
                        }
                    });
                });
            ui.add_space(4.0);
        }
    }

    if let Some(index) = remove_index {
        node.variables.remove(index);
        changed = true;
    }

    if ui.button(t("app.add_variable", lang)).clicked() {
        node.variables.push(SceneVariable {
            name: format!("var_{}", node.variables.len() + 1),
            value: VariableValue::Number(0.0),
        });
        changed = true;
    }

    changed
}

fn show_audio_source_editor(ui: &mut Ui, node: &mut SceneNode, lang: Language) -> bool {
    let mut changed = false;

    changed |= ui
        .checkbox(&mut node.audio_source.enabled, t("app.audio_enabled", lang))
        .changed();

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(t("app.audio_clip", lang));
        changed |= ui
            .add(
                egui::TextEdit::singleline(&mut node.audio_source.clip)
                    .hint_text("audio/clip.ogg")
                    .desired_width(180.0),
            )
            .changed();
    });

    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        changed |= ui
            .checkbox(&mut node.audio_source.autoplay, t("app.audio_autoplay", lang))
            .changed();
        changed |= ui
            .checkbox(&mut node.audio_source.looping, t("app.audio_looping", lang))
            .changed();
    });

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(t("app.audio_volume", lang));
        changed |= ui
            .add(egui::Slider::new(&mut node.audio_source.volume, 0.0..=1.0).show_value(true))
            .changed();
    });

    changed
}

fn show_physics_editor(ui: &mut Ui, node: &mut SceneNode, lang: Language) -> bool {
    let mut changed = false;

    changed |= ui
        .checkbox(&mut node.rigid_body.enabled, t("app.enable_rigidbody", lang))
        .changed();

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(t("app.collider_type", lang));
        let mut collider_type = node.collider.collider_type;
        egui::ComboBox::from_id_salt("collider_type_select")
            .selected_text(collider_type_label(collider_type, lang))
            .show_ui(ui, |ui| {
                for kind in [
                    ColliderType::None,
                    ColliderType::Aabb,
                    ColliderType::ConvexHull,
                    ColliderType::MeshCollider,
                ] {
                    if ui
                        .selectable_value(&mut collider_type, kind, collider_type_label(kind, lang))
                        .changed()
                    {
                        node.collider = if kind == ColliderType::None {
                            Collider::default()
                        } else {
                            Collider::auto_fit(&primitive_collider_points(node.primitive), kind)
                        };
                        changed = true;
                    }
                }
            });
    });

    ui.add_space(6.0);
    changed |= ui
        .checkbox(&mut node.rigid_body.is_trigger, t("app.is_trigger", lang))
        .changed();

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(t("app.body_type", lang));
        egui::ComboBox::from_id_salt("body_type_select")
            .selected_text(rigid_body_type_label(node.rigid_body.body_type, lang))
            .show_ui(ui, |ui| {
                for body_type in [
                    RigidBodyType::Static,
                    RigidBodyType::Dynamic,
                    RigidBodyType::Kinematic,
                ] {
                    changed |= ui
                        .selectable_value(
                            &mut node.rigid_body.body_type,
                            body_type,
                            rigid_body_type_label(body_type, lang),
                        )
                        .changed();
                }
            });
    });

    ui.add_space(6.0);
    changed |= ui
        .checkbox(&mut node.rigid_body.use_gravity, t("app.use_gravity", lang))
        .changed();

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(t("app.physics_damping", lang));
        changed |= ui
            .add(egui::Slider::new(&mut node.rigid_body.damping, 0.0..=1.0).show_value(true))
            .changed();
    });

    ui.add_space(6.0);
    changed |= edit_vec3(ui, t("app.velocity", lang), &mut node.rigid_body.velocity, 0.05);

    changed
}

fn edit_variable_value(ui: &mut Ui, value: &mut VariableValue) -> bool {
    match value {
        VariableValue::Bool(current) => ui.checkbox(current, "").changed(),
        VariableValue::Number(current) => ui.add(egui::DragValue::new(current).speed(0.1)).changed(),
        VariableValue::Text(current) => ui
            .add(egui::TextEdit::singleline(current).desired_width(120.0))
            .changed(),
    }
}

fn variable_kind(variable: &SceneVariable) -> &'static str {
    match variable.value {
        VariableValue::Bool(_) => "Bool",
        VariableValue::Number(_) => "Number",
        VariableValue::Text(_) => "Text",
    }
}

fn collider_type_label(kind: ColliderType, lang: Language) -> &'static str {
    match lang {
        Language::Spanish => match kind {
            ColliderType::None => "Ninguno",
            ColliderType::Aabb => "AABB",
            ColliderType::ConvexHull => "Hull Convexo",
            ColliderType::MeshCollider => "Malla",
        },
        _ => match kind {
            ColliderType::None => "None",
            ColliderType::Aabb => "AABB",
            ColliderType::ConvexHull => "Convex Hull",
            ColliderType::MeshCollider => "Mesh",
        },
    }
}

fn rigid_body_type_label(kind: RigidBodyType, lang: Language) -> &'static str {
    match lang {
        Language::Spanish => match kind {
            RigidBodyType::Static => "Estatico",
            RigidBodyType::Dynamic => "Dinamico",
            RigidBodyType::Kinematic => "Cinematico",
        },
        _ => match kind {
            RigidBodyType::Static => "Static",
            RigidBodyType::Dynamic => "Dynamic",
            RigidBodyType::Kinematic => "Kinematic",
        },
    }
}

fn primitive_collider_points(primitive: Primitive) -> Vec<Vec3> {
    match primitive {
        Primitive::Plane => vec![Vec3::new(-0.5, -0.05, -0.5), Vec3::new(0.5, 0.05, 0.5)],
        Primitive::Sprite2D => vec![Vec3::new(-0.5, -0.5, -0.05), Vec3::new(0.5, 0.5, 0.05)],
        _ => vec![Vec3::splat(-0.5), Vec3::splat(0.5)],
    }
}
