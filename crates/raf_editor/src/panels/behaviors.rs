//! Attached Behaviors modular UI for the Properties panel.
//! Follows the Studio-Grade UI guidelines (Lean, Muted Colors, English-only by default).
//! Keeps the Properties panel code clean.

use egui::Ui;
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_core::scene::graph::SceneNodeId;
use raf_core::scene::SceneGraph;
use crate::theme::ACCENT;

pub fn show_attached_behaviors(ui: &mut Ui, scene: &mut SceneGraph, id: SceneNodeId, lang: Language) {
    let node = match scene.get_mut(id) {
        Some(n) => n,
        None => return,
    };

    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(t("app.attached_behaviors", lang))
            .size(12.0)
            .color(egui::Color32::from_gray(120))
            .strong(),
    );
    ui.add_space(4.0);

    ui.group(|ui| {
        ui.set_min_width(ui.available_width());
        
        let mut script_to_remove = None;

        if node.scripts.is_empty() {
            ui.label(
                egui::RichText::new(t("app.no_behaviors", lang))
                    .size(12.0)
                    .color(egui::Color32::from_gray(100)),
            );
        } else {
            for (index, script) in node.scripts.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("⚙")
                            .color(ACCENT)
                            .size(12.0),
                    );
                    
                    // Simple filename display
                    let display_name = script.split('/').last().unwrap_or(script);
                    ui.label(egui::RichText::new(display_name).size(12.0));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let btn = egui::Button::new(
                            egui::RichText::new(t("app.edit", lang))
                                .size(11.0)
                                .color(egui::Color32::from_gray(160))
                        ).frame(true).rounding(4.0);
                        
                        let edit_response = ui.add_sized([40.0, 18.0], btn);

                        // If user tries to edit... show standard warning modal
                        if edit_response.clicked() {
                            // Normally we would launch VS Code. For now, pop up or log.
                            // We will use egui Context to show a generic modal message if needed, 
                            // but simpler: we just show a subtle hover or console log based on Studio Grade
                        }
                        edit_response.on_hover_text("Editor not available yet.\nUse VS Code or an IDE to modify files.");

                        let del_btn = egui::Button::new(
                            egui::RichText::new("X")
                                .size(11.0)
                                .color(egui::Color32::from_gray(100))
                        ).frame(false);
                        
                        if ui.add(del_btn).clicked() {
                            script_to_remove = Some(index);
                        }
                    });
                });
                ui.add_space(2.0);
            }
        }

        if let Some(idx) = script_to_remove {
            node.scripts.remove(idx);
        }

        ui.add_space(8.0);
        
        // Add Behavior Button
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let add_btn = egui::Button::new(
                egui::RichText::new(t("app.add_behavior", lang))
                    .size(11.0)
                    .color(egui::Color32::from_gray(200))
            ).rounding(4.0);

            if ui.add_sized([100.0, 22.0], add_btn).clicked() {
                // In a real scenario, this opens a file dialog.
                // For now, attach a placeholder name to test the UI.
                node.scripts.push(format!("scripts/behavior_{}.rs", node.scripts.len() + 1));
            }
        });
    });
}
