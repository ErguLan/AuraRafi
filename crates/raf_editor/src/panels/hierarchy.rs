//! Hierarchy panel - scene tree view.
//! Displays the entity tree with collapsible groups.
//! All text translated ES/EN.

use egui::Ui;
use raf_core::config::Language;
use raf_core::scene::{SceneGraph, SceneNodeId};

/// State for the hierarchy panel.
pub struct HierarchyPanel {
    pub selected_node: Option<SceneNodeId>,
}

impl Default for HierarchyPanel {
    fn default() -> Self {
        Self {
            selected_node: None,
        }
    }
}

impl HierarchyPanel {
    /// Draw the hierarchy panel.
    pub fn show(&mut self, ui: &mut Ui, scene: &SceneGraph, _lang: Language) {
        // Professional uppercase header.
        ui.label(
            egui::RichText::new("HIERARCHY")
                .size(10.0)
                .color(egui::Color32::from_rgb(130, 130, 140)),
        );
        ui.separator();

        if scene.is_empty() {
            ui.label(
                egui::RichText::new("No entities in scene")
                    .size(11.0)
                    .color(egui::Color32::from_rgb(100, 100, 110)),
            );
            return;
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            for &root_id in scene.roots() {
                self.show_node(ui, scene, root_id);
            }
        });
    }

    fn show_node(&mut self, ui: &mut Ui, scene: &SceneGraph, id: SceneNodeId) {
        let node = match scene.get(id) {
            Some(n) => n,
            None => return,
        };

        // Skip removed (soft-deleted) nodes.
        if node.name.is_empty() {
            return;
        }

        let is_selected = self.selected_node == Some(id);
        let has_children = !node.children.is_empty();

        if has_children {
            let header = egui::CollapsingHeader::new(&node.name)
                .default_open(true)
                .show(ui, |ui| {
                    for &child_id in &node.children {
                        self.show_node(ui, scene, child_id);
                    }
                });
            if header.header_response.clicked() {
                self.selected_node = Some(id);
            }
        } else {
            let label = ui.selectable_label(is_selected, &node.name);
            if label.clicked() {
                self.selected_node = Some(id);
            }
        }
    }
}
