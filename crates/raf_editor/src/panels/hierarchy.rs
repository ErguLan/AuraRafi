//! Hierarchy panel - scene tree view.
//! Displays the entity tree with collapsible groups.
//! Supports multi-select (Shift+Click), visibility toggle, context menu.
//! All text translated ES/EN.

use egui::Ui;
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_core::scene::{SceneGraph, SceneNodeId, Primitive};
use std::collections::HashSet;

use crate::ui_icons::UiIconAtlas;

pub struct HierarchyActions {
    pub delete: Option<SceneNodeId>,
    pub duplicate: Option<SceneNodeId>,
    pub ungroup: Option<SceneNodeId>,
    pub toggle_visibility: Option<SceneNodeId>,
    pub edited: bool,
    pub create_folder_parent: Option<Option<SceneNodeId>>,
    pub reparent: Option<(SceneNodeId, Option<SceneNodeId>)>,
}

/// State for the hierarchy panel.
pub struct HierarchyPanel {
    /// Legacy single-select (used by properties panel).
    pub selected_node: Option<SceneNodeId>,
    /// Multi-select list (synced with viewport).
    pub selected_nodes: Vec<SceneNodeId>,
    /// Pending action: delete requested.
    pub pending_delete: Option<SceneNodeId>,
    /// Pending action: duplicate requested.
    pub pending_duplicate: Option<SceneNodeId>,
    /// Pending action: rename requested (id, new_name).
    renaming: Option<(SceneNodeId, String)>,
    /// Pending action: ungroup folder requested.
    pub pending_ungroup: Option<SceneNodeId>,
    /// Pending action: create folder under parent or at root.
    pending_create_folder_parent: Option<Option<SceneNodeId>>,
    /// Pending action: toggle node visibility.
    pending_toggle_visibility: Option<SceneNodeId>,
    /// Pending action: reparent node via drag and drop.
    pending_reparent: Option<(SceneNodeId, Option<SceneNodeId>)>,
    rename_needs_focus: bool,
    scene_edited: bool,
    collapsed_nodes: HashSet<SceneNodeId>,
    dragged_node: Option<SceneNodeId>,
}

impl Default for HierarchyPanel {
    fn default() -> Self {
        Self {
            selected_node: None,
            selected_nodes: Vec::new(),
            pending_delete: None,
            pending_duplicate: None,
            renaming: None,
            pending_ungroup: None,
            pending_create_folder_parent: None,
            pending_toggle_visibility: None,
            pending_reparent: None,
            rename_needs_focus: false,
            scene_edited: false,
            collapsed_nodes: HashSet::new(),
            dragged_node: None,
        }
    }
}

impl HierarchyPanel {
    /// Draw the hierarchy panel.
    pub fn show(&mut self, ui: &mut Ui, scene: &mut SceneGraph, lang: Language, icons: &UiIconAtlas) {
        // Header.
        ui.label(
            egui::RichText::new(t("app.hierarchy", lang))
                .size(11.0).strong()
                .color(egui::Color32::from_rgb(130, 130, 140)),
        );
        ui.separator();

        if scene.is_empty() {
            ui.label(
                egui::RichText::new(t("app.no_entities", lang))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(100, 100, 110)),
            );
            return;
        }

        // Entity count (subtle).
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("{}: {}", t("app.entities_count", lang), scene.len()))
                    .size(9.0)
                    .color(egui::Color32::from_rgb(90, 90, 100)),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let folder_btn = icon_button(
                    ui,
                    icons,
                    "folder.png",
                    "+",
                    egui::vec2(18.0, 18.0),
                    egui::Color32::from_white_alpha(220),
                )
                .on_hover_text(t("app.add_folder", lang));
                if folder_btn.clicked() {
                    self.pending_create_folder_parent = Some(None);
                }
            });
        });

        let panel_width = ui.available_width().max(120.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            let roots: Vec<SceneNodeId> = scene.roots().to_vec();
            for root_id in roots {
                self.show_node(ui, scene, root_id, 0, lang, icons, panel_width);
            }

            let released = ui.input(|i| i.pointer.any_released());
            if released && self.dragged_node.is_some() {
                let dropped_in_panel = ui
                    .input(|i| i.pointer.hover_pos())
                    .map(|pos| ui.max_rect().contains(pos))
                    .unwrap_or(false);

                if dropped_in_panel && self.pending_reparent.is_none() {
                    if let Some(dragged) = self.dragged_node {
                        self.pending_reparent = Some((dragged, None));
                    }
                }

                self.dragged_node = None;
            }
        });

        // Sync selected_node from multi-select (first element).
        self.selected_node = self.selected_nodes.first().copied();
    }

    fn show_node(
        &mut self,
        ui: &mut Ui,
        scene: &mut SceneGraph,
        id: SceneNodeId,
        depth: usize,
        lang: Language,
        icons: &UiIconAtlas,
        panel_width: f32,
    ) {
        let node = match scene.get(id) {
            Some(n) => n,
            None => return,
        };

        // Skip removed (soft-deleted) nodes.
        if node.name.is_empty() {
            return;
        }

        let node_name = node.name.clone();
        let node_visible = node.visible;
        let primitive = node.primitive;
        let is_folder = node.is_folder;
        let child_ids = node.children.clone();
        let has_children = !child_ids.is_empty();

        let is_selected = self.selected_nodes.contains(&id);
        let is_open = !self.collapsed_nodes.contains(&id);
        let is_renaming = matches!(self.renaming, Some((rename_id, _)) if rename_id == id);

        // Primitive type icon.
        // Text color based on selection and visibility.
        let text_color = if !node_visible {
            egui::Color32::from_rgb(80, 80, 90) // Dimmed when hidden
        } else if is_selected {
            egui::Color32::WHITE
        } else {
            egui::Color32::from_rgb(180, 180, 190)
        };

        let row_width = panel_width.max(120.0);
        ui.allocate_ui_with_layout(
            egui::Vec2::new(row_width, 24.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let indent_width = depth as f32 * 14.0;
            ui.add_space(indent_width);

            let toggle_width = if has_children { 14.0 } else { 18.0 };
            let leading_icon_width = 20.0;
            let actions_width = 48.0;
            let base_label_width = (row_width - indent_width - toggle_width - leading_icon_width - actions_width - 16.0).max(48.0);

            if has_children {
                let toggle = if is_open { "▾" } else { "▸" };
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new(toggle).size(11.0))
                            .frame(false)
                            .min_size(egui::Vec2::new(14.0, 18.0)),
                    )
                    .clicked()
                {
                    if is_open {
                        self.collapsed_nodes.insert(id);
                    } else {
                        self.collapsed_nodes.remove(&id);
                    }
                }
            } else {
                ui.add_space(18.0);
            }

            if is_renaming {
                let mut commit = false;
                let mut cancel = false;
                let input_width = base_label_width.max(80.0);

                if let Some((_, new_name)) = self.renaming.as_mut() {
                    let response = ui.add_sized(
                        [input_width, 22.0],
                        egui::TextEdit::singleline(new_name),
                    );
                    if self.rename_needs_focus {
                        response.request_focus();
                        self.rename_needs_focus = false;
                    }

                    let enter_pressed = response.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let escape_pressed = response.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Escape));
                    let clicked_elsewhere = response.lost_focus()
                        && ui.input(|i| i.pointer.any_pressed())
                        && !ui.input(|i| i.key_pressed(egui::Key::Escape));

                    commit = enter_pressed || clicked_elsewhere;
                    cancel = escape_pressed;
                }

                if ui.small_button("OK").clicked() {
                    commit = true;
                }
                if ui.small_button("X").clicked() {
                    cancel = true;
                }

                if commit {
                    self.commit_rename(scene, id);
                } else if cancel {
                    self.cancel_rename();
                }
            } else {
                let icon_name = primitive_icon_name(primitive, is_folder);
                if let Some(texture) = icons.get(icon_name) {
                    ui.add(
                        egui::Image::new(texture)
                            .fit_to_exact_size(egui::Vec2::new(18.0, 18.0)),
                    );
                } else {
                    ui.label(
                        egui::RichText::new(primitive_icon_fallback(primitive, is_folder))
                            .size(11.0)
                            .color(text_color),
                    );
                }

                let response = selectable_node_label(ui, &node_name, base_label_width, is_selected, text_color);

                if response.clicked() {
                    self.handle_click(ui, id);
                }
                if response.double_clicked() {
                    self.begin_rename(id, node_name.clone());
                }

                if response.drag_started() {
                    self.dragged_node = Some(id);
                }

                let can_drop_here = matches!(self.dragged_node, Some(dragged) if dragged != id);
                if can_drop_here && response.hovered() && ui.input(|i| i.pointer.any_released()) {
                    if let Some(dragged) = self.dragged_node {
                        self.pending_reparent = Some((dragged, Some(id)));
                        self.dragged_node = None;
                    }
                }

                if can_drop_here && response.hovered() {
                    ui.painter().rect_stroke(
                        response.rect.expand(1.0),
                        4.0,
                        egui::Stroke::new(1.0, egui::Color32::from_rgb(212, 119, 26)),
                    );
                }

                response.context_menu(|ui| {
                    self.show_node_menu(ui, id, &node_name, is_folder, lang);
                });

                ui.allocate_ui_with_layout(
                    egui::vec2(actions_width, 22.0),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        let menu_id = ui.make_persistent_id(("hierarchy_node_menu", id));
                        let menu_btn = icon_button(
                            ui,
                            icons,
                            "3-dots vertical.png",
                            "...",
                            egui::vec2(16.0, 16.0),
                            egui::Color32::from_white_alpha(200),
                        );
                        if menu_btn.clicked() {
                            ui.memory_mut(|mem| mem.toggle_popup(menu_id));
                        }
                        egui::popup_above_or_below_widget(
                            ui,
                            menu_id,
                            &menu_btn,
                            egui::AboveOrBelow::Below,
                            egui::PopupCloseBehavior::CloseOnClickOutside,
                            |ui| {
                                self.show_node_menu(ui, id, &node_name, is_folder, lang);
                            },
                        );

                        let visibility_icon = if node_visible { "visible.png" } else { "hidden.png" };
                        let visibility_btn = icon_button(
                            ui,
                            icons,
                            visibility_icon,
                            if node_visible { "V" } else { "H" },
                            egui::vec2(16.0, 16.0),
                            if node_visible {
                                egui::Color32::from_white_alpha(215)
                            } else {
                                egui::Color32::from_white_alpha(120)
                            },
                        )
                        .on_hover_text(t("app.visible", lang));
                        if visibility_btn.clicked() {
                            self.pending_toggle_visibility = Some(id);
                        }
                    },
                );
            }
        });

        if has_children && is_open {
            for child_id in child_ids {
                self.show_node(ui, scene, child_id, depth + 1, lang, icons, panel_width);
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

    /// Consume pending context actions. Called by app.rs each frame.
    pub fn take_actions(&mut self) -> HierarchyActions {
        let edited = self.scene_edited;
        self.scene_edited = false;
        HierarchyActions {
            delete: self.pending_delete.take(),
            duplicate: self.pending_duplicate.take(),
            ungroup: self.pending_ungroup.take(),
            toggle_visibility: self.pending_toggle_visibility.take(),
            edited,
            create_folder_parent: self.pending_create_folder_parent.take(),
            reparent: self.pending_reparent.take(),
        }
    }

    fn begin_rename(&mut self, id: SceneNodeId, current_name: String) {
        self.renaming = Some((id, current_name));
        self.rename_needs_focus = true;
    }

    fn cancel_rename(&mut self) {
        self.renaming = None;
        self.rename_needs_focus = false;
    }

    fn commit_rename(&mut self, scene: &mut SceneGraph, id: SceneNodeId) {
        let Some((rename_id, new_name)) = self.renaming.take() else {
            return;
        };
        self.rename_needs_focus = false;

        if rename_id != id {
            return;
        }

        let trimmed = new_name.trim();
        if trimmed.is_empty() {
            return;
        }

        if let Some(node) = scene.get_mut(id) {
            node.name = trimmed.to_string();
            self.scene_edited = true;
        }
    }

    fn show_node_menu(&mut self, ui: &mut Ui, id: SceneNodeId, node_name: &str, is_folder: bool, lang: Language) {
        ui.set_min_width(150.0);
        ui.label(
            egui::RichText::new(node_name)
                .size(10.0)
                .strong()
                .color(egui::Color32::from_rgb(200, 200, 205)),
        );
        ui.separator();

        if ui.button(t("app.rename", lang)).clicked() {
            self.begin_rename(id, node_name.to_string());
            ui.close_menu();
        }
        if ui.button(t("app.add_folder", lang)).clicked() {
            self.pending_create_folder_parent = Some(Some(id));
            ui.close_menu();
        }
        if ui.button(t("app.duplicate_menu", lang)).clicked() {
            self.pending_duplicate = Some(id);
            ui.close_menu();
        }
        if is_folder && ui.button(t("app.ungroup", lang)).clicked() {
            self.pending_ungroup = Some(id);
            ui.close_menu();
        }
        ui.separator();
        if ui.button(
            egui::RichText::new(t("app.delete_menu", lang))
                .color(egui::Color32::from_rgb(220, 80, 80)),
        ).clicked() {
            self.pending_delete = Some(id);
            ui.close_menu();
        }
    }
}

fn primitive_icon_name(prim: Primitive, is_folder: bool) -> &'static str {
    if is_folder {
        return "folder.png";
    }

    match prim {
        Primitive::Cube => "cube.png",
        Primitive::Sphere => "sphere.png",
        Primitive::Plane => "plane.png",
        Primitive::Cylinder => "cylinder.png",
        Primitive::Sprite2D => "sprite.png",
        Primitive::Empty => "empty.png",
    }
}

fn primitive_icon_fallback(prim: Primitive, is_folder: bool) -> &'static str {
    if is_folder {
        return "▣";
    }

    match prim {
        Primitive::Cube => "\u{25A1}",
        Primitive::Sphere => "\u{25CB}",
        Primitive::Plane => "\u{25AD}",
        Primitive::Cylinder => "\u{25AE}",
        Primitive::Sprite2D => "\u{25C8}",
        Primitive::Empty => "\u{25CC}",
    }
}

fn icon_button(
    ui: &mut Ui,
    icons: &UiIconAtlas,
    icon_name: &'static str,
    fallback: &'static str,
    size: egui::Vec2,
    tint: egui::Color32,
) -> egui::Response {
    let hit_size = egui::vec2(size.x + 4.0, size.y + 4.0);
    let (rect, response) = ui.allocate_exact_size(hit_size, egui::Sense::click());

    if response.hovered() {
        ui.painter().rect_filled(
            rect,
            4.0,
            egui::Color32::from_rgba_premultiplied(255, 255, 255, 10),
        );
    }

    let icon_rect = egui::Rect::from_center_size(rect.center(), size);
    if !icons.paint(ui.painter(), icon_name, icon_rect, tint) {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            fallback,
            egui::FontId::proportional(10.0),
            tint,
        );
    }

    response
}

fn selectable_node_label(
    ui: &mut Ui,
    label: &str,
    width: f32,
    selected: bool,
    text_color: egui::Color32,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(width, 22.0),
        egui::Sense::click_and_drag(),
    );

    if selected {
        ui.painter().rect_filled(
            rect,
            4.0,
            egui::Color32::from_rgba_premultiplied(212, 119, 26, 28),
        );
    } else if response.hovered() {
        ui.painter().rect_filled(
            rect,
            4.0,
            egui::Color32::from_rgba_premultiplied(255, 255, 255, 8),
        );
    }

    ui.painter().text(
        egui::pos2(rect.left() + 6.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(11.0),
        text_color,
    );

    response
}
