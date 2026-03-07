//! Properties / Inspector panel - edit selected entity's properties.

use egui::Ui;
use raf_core::scene::{SceneGraph, SceneNodeId};

/// State for the properties panel.
pub struct PropertiesPanel;

impl Default for PropertiesPanel {
    fn default() -> Self {
        Self
    }
}

impl PropertiesPanel {
    /// Draw the properties panel for the selected node.
    pub fn show(&mut self, ui: &mut Ui, scene: &mut SceneGraph, selected: Option<SceneNodeId>) {
        ui.heading("Properties");
        ui.separator();

        let id = match selected {
            Some(id) => id,
            None => {
                ui.label("No entity selected");
                return;
            }
        };

        let node = match scene.get_mut(id) {
            Some(n) => n,
            None => {
                ui.label("Entity not found");
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
                    ui.horizontal(|ui| {
                        ui.label("Position:");
                    });
                    ui.horizontal(|ui| {
                        ui.label("X:");
                        ui.add(egui::DragValue::new(&mut node.position.x).speed(0.1));
                        ui.label("Y:");
                        ui.add(egui::DragValue::new(&mut node.position.y).speed(0.1));
                        ui.label("Z:");
                        ui.add(egui::DragValue::new(&mut node.position.z).speed(0.1));
                    });

                    ui.horizontal(|ui| {
                        ui.label("Rotation:");
                    });
                    ui.horizontal(|ui| {
                        ui.label("X:");
                        ui.add(egui::DragValue::new(&mut node.rotation.x).speed(1.0));
                        ui.label("Y:");
                        ui.add(egui::DragValue::new(&mut node.rotation.y).speed(1.0));
                        ui.label("Z:");
                        ui.add(egui::DragValue::new(&mut node.rotation.z).speed(1.0));
                    });

                    ui.horizontal(|ui| {
                        ui.label("Scale:");
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

            // Visibility
            ui.horizontal(|ui| {
                ui.label("Visible:");
                ui.checkbox(&mut node.visible, "");
            });

            ui.add_space(8.0);

            // Custom variables section (placeholder for future)
            egui::CollapsingHeader::new("Variables")
                .default_open(false)
                .show(ui, |ui| {
                    ui.label("No custom variables defined");
                });
        });
    }
}
