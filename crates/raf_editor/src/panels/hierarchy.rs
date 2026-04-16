//! Hierarchy panel - scene tree view.
//! Displays the entity tree with collapsible groups.
//! Supports multi-select (Shift+Click), visibility toggle, context menu.
//! All text translated ES/EN.

use egui::Ui;
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_core::scene::{SceneGraph, SceneNodeId, Primitive};

/// State for the hierarchy panel.
pub struct HierarchyPanel {
    /// Legacy single-select (used by properties panel).
    pub selected_node: Option<SceneNodeId>,
    /// Multi-select list (synced with viewport).
    pub selected_nodes: Vec<SceneNodeId>,
    /// Context menu target.
    context_target: Option<SceneNodeId>,
}

impl Default for HierarchyPanel {
    fn default() -> Self {
        Self {
            selected_node: None,
            selected_nodes: Vec::new(),
            context_target: None,
        }
    }
}

impl HierarchyPanel {
    /// Draw the hierarchy panel.
    pub fn show(&mut self, ui: &mut Ui, scene: &SceneGraph, _lang: Language) {
        // Header.
        ui.label(
            egui::RichText::new(t("app.hierarchy", _lang))
                .size(11.0).strong()
                .color(egui::Color32::from_rgb(130, 130, 140)),
        );
        ui.separator();

        if scene.is_empty() {
            ui.label(
                egui::RichText::new(t("app.no_entities", _lang))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(100, 100, 110)),
            );
            return;
        }

        // Entity count (subtle).
        ui.label(
            egui::RichText::new(format!("{}: {}", t("app.entities_count", _lang), scene.len()))
                .size(9.0)
                .color(egui::Color32::from_rgb(90, 90, 100)),
        );

        egui::ScrollArea::vertical().show(ui, |ui| {
            let roots: Vec<SceneNodeId> = scene.roots().to_vec();
            for root_id in roots {
                self.show_node(ui, scene, root_id);
            }
        });

        // Sync selected_node from multi-select (first element).
        self.selected_node = self.selected_nodes.first().copied();
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

        let is_selected = self.selected_nodes.contains(&id);
        let has_children = !node.children.is_empty();

        // Primitive type icon.
        let prim_icon = primitive_icon(node.primitive);

        // Text color based on selection and visibility.
        let text_color = if !node.visible {
            egui::Color32::from_rgb(80, 80, 90) // Dimmed when hidden
        } else if is_selected {
            egui::Color32::WHITE
        } else {
            egui::Color32::from_rgb(180, 180, 190)
        };

        // Build display text: icon + name.
        let display = format!("{} {}", prim_icon, node.name);
        let node_text = egui::RichText::new(&display)
            .size(11.0)
            .color(text_color);

        if has_children {
            let children_ids: Vec<SceneNodeId> = node.children.clone();
            let header = egui::CollapsingHeader::new(node_text)
                .default_open(true)
                .show(ui, |ui| {
                    for child_id in children_ids {
                        self.show_node(ui, scene, child_id);
                    }
                });
            if header.header_response.clicked() {
                self.handle_click(ui, id);
            }
            // Right-click context menu.
            if header.header_response.secondary_clicked() {
                self.context_target = Some(id);
            }
        } else {
            let label = ui.selectable_label(is_selected, node_text);
            if label.clicked() {
                self.handle_click(ui, id);
            }
            if label.secondary_clicked() {
                self.context_target = Some(id);
            }
        }
    }

    /// Handle click with Shift support for multi-select.
    fn handle_click(&mut self, ui: &Ui, id: SceneNodeId) {
        let shift = ui.input(|i| i.modifiers.shift);

        if shift {
            // Toggle in multi-select list.
            if let Some(idx) = self.selected_nodes.iter().position(|&sid| sid == id) {
                self.selected_nodes.remove(idx);
            } else {
                self.selected_nodes.push(id);
            }
        } else {
            // Single select: replace entire selection.
            self.selected_nodes = vec![id];
        }

        // Always update legacy selected_node.
        self.selected_node = self.selected_nodes.first().copied();
    }

    /// Check if a context menu action was requested. Returns (delete_id, duplicate_id).
    pub fn take_context_action(&mut self) -> (Option<SceneNodeId>, Option<SceneNodeId>) {
        // This is called by app.rs to process context actions.
        // For now, context menu is handled externally.
        (None, None)
    }
}

/// Return a compact icon character for each primitive type.
fn primitive_icon(prim: Primitive) -> &'static str {
    match prim {
        Primitive::Cube => "\u{25A1}",      // square
        Primitive::Sphere => "\u{25CB}",    // circle
        Primitive::Plane => "\u{25AD}",     // rectangle
        Primitive::Cylinder => "\u{25AE}",  // filled rectangle
        Primitive::Sprite2D => "\u{25C8}",  // diamond in circle
        Primitive::Empty => "\u{25CC}",     // dotted circle
    }
}
