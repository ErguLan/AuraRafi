//! Hierarchy panel - scene tree view.
//! Displays the entity tree with collapsible groups.
//! Supports multi-select (Shift+Click), visibility toggle, context menu.
//! All text translated ES/EN.

use egui::Ui;
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_core::scene::{Primitive, SceneGraph, SceneNodeId};
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
    /// If set alongside reparent, insert before this sibling instead of appending.
    pub reparent_before: Option<SceneNodeId>,
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
    /// Pending action: reparent node via drag and drop (dragged, new_parent).
    pending_reparent: Option<(SceneNodeId, Option<SceneNodeId>)>,
    /// If set alongside pending_reparent, insert before this sibling.
    pending_insert_before: Option<SceneNodeId>,
    rename_needs_focus: bool,
    scene_edited: bool,
    collapsed_nodes: HashSet<SceneNodeId>,
    dragged_node: Option<SceneNodeId>,
    search_query: String,
    box_select_start: Option<egui::Pos2>,
    box_select_rect: Option<egui::Rect>,
    box_select_candidates: HashSet<SceneNodeId>,
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
            pending_insert_before: None,
            rename_needs_focus: false,
            scene_edited: false,
            collapsed_nodes: HashSet::new(),
            dragged_node: None,
            search_query: String::new(),
            box_select_start: None,
            box_select_rect: None,
            box_select_candidates: HashSet::new(),
        }
    }
}

impl HierarchyPanel {
    /// Draw the hierarchy panel.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        scene: &mut SceneGraph,
        lang: Language,
        icons: &UiIconAtlas,
    ) {
        // Header.
        ui.label(
            egui::RichText::new(t("app.hierarchy", lang))
                .size(11.0)
                .strong()
                .color(egui::Color32::from_rgb(130, 130, 140)),
        );

        // Search bar.
        let search_text_color = if self.search_query.is_empty() {
            egui::Color32::from_rgb(100, 100, 110)
        } else {
            egui::Color32::from_rgb(180, 180, 190)
        };
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.search_query)
                    .hint_text(t("app.search", lang))
                    .desired_width(f32::INFINITY)
                    .text_color(search_text_color)
                    .font(egui::TextStyle::Body),
            );
            if !self.search_query.is_empty() {
                if ui
                    .add(egui::Button::new("✕").frame(false).min_size(egui::vec2(16.0, 16.0)))
                    .clicked()
                {
                    self.search_query.clear();
                }
            }
        });
        ui.separator();

        if scene.is_empty() {
            ui.label(
                egui::RichText::new(t("app.no_entities", lang))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(100, 100, 110)),
            );
            return;
        }

        // Entity count filter status.
        ui.horizontal(|ui| {
            let showing_all = self.search_query.trim().is_empty();
            let total = scene.len();
            if showing_all {
                ui.label(
                    egui::RichText::new(format!("{}: {}", t("app.entities_count", lang), total))
                        .size(9.0)
                        .color(egui::Color32::from_rgb(90, 90, 100)),
                );
            } else {
                ui.label(
                    egui::RichText::new(format!("{}", total))
                        .size(9.0)
                        .color(egui::Color32::from_rgb(90, 90, 100)),
                );
            }
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
        let search_lower = self.search_query.to_lowercase();
        let is_filtering = !search_lower.is_empty();

        egui::ScrollArea::vertical().show(ui, |ui| {
            // Background click to deselect.
            let bg_response = ui.interact(
                ui.max_rect(),
                ui.make_persistent_id("hierarchy_bg"),
                egui::Sense { click: true, drag: true, focusable: false },
            );

            // Box select start.
            if bg_response.drag_started() {
                self.box_select_start = ui.input(|i| i.pointer.interact_pos());
                self.box_select_candidates.clear();
                let ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.mac_cmd);
                if !ctrl {
                    self.selected_nodes.clear();
                    self.selected_node = None;
                }
                self.dragged_node = None;
                self.pending_reparent = None;
                self.pending_insert_before = None;
            }

            // Compute box select rect from start to current pointer each frame.
            self.box_select_rect = self.box_select_start.and_then(|start| {
                ui.input(|i| i.pointer.interact_pos()).map(|current| {
                    egui::Rect::from_two_pos(start, current)
                })
            });

            if bg_response.clicked() {
                self.selected_nodes.clear();
                self.selected_node = None;
                self.dragged_node = None;
                self.pending_reparent = None;
                self.pending_insert_before = None;
                self.box_select_start = None;
                self.box_select_candidates.clear();
            }

            let roots: Vec<SceneNodeId> = scene.roots().to_vec();
            let mut visible_roots = roots.clone();
            if is_filtering {
                visible_roots.retain(|id| self.node_matches_filter(scene, *id, &search_lower));
            }

            for root_id in &visible_roots {
                self.show_node(
                    ui,
                    scene,
                    *root_id,
                    0,
                    lang,
                    icons,
                    panel_width,
                    is_filtering,
                    &search_lower,
                    0,
                );
            }

            // Box selection rect visual.
            if let Some(box_rect) = self.box_select_rect {
                ui.painter().rect_stroke(
                    box_rect,
                    1.0,
                    egui::Stroke::new(1.5, egui::Color32::from_rgb(212, 119, 26)),
                );
                ui.painter().rect_filled(
                    box_rect,
                    1.0,
                    egui::Color32::from_rgba_premultiplied(212, 119, 26, 20),
                );
            }

            // Commit box select on drag release.
            if bg_response.drag_stopped() {
                let ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.mac_cmd);
                for &id in &self.box_select_candidates {
                    if ctrl {
                        if let Some(idx) = self.selected_nodes.iter().position(|&sid| sid == id) {
                            self.selected_nodes.remove(idx);
                        } else {
                            self.selected_nodes.push(id);
                        }
                    } else if !self.selected_nodes.contains(&id) {
                        self.selected_nodes.push(id);
                    }
                }
                self.selected_node = self.selected_nodes.first().copied();
                self.box_select_start = None;
                self.box_select_rect = None;
                self.box_select_candidates.clear();
            }

            // Drop on empty panel area -> reparent to root.
            if self.dragged_node.is_some() {
                let released = ui.input(|i| i.pointer.any_released());
                if released {
                    let dropped_in_panel = ui
                        .input(|i| i.pointer.hover_pos())
                        .map(|pos| ui.max_rect().contains(pos))
                        .unwrap_or(false);

                    if dropped_in_panel && self.pending_reparent.is_none() {
                        if let Some(dragged) = self.dragged_node {
                            self.pending_reparent = Some((dragged, None));
                            self.pending_insert_before = None;
                        }
                    }

                    self.dragged_node = None;
                }
            }
        });

        // Drag ghost preview.
        if let Some(dragged) = self.dragged_node {
            if let (Some(pointer), Some(node)) =
                (ui.input(|i| i.pointer.hover_pos()), scene.get(dragged))
            {
                let selection_extra = if self.selected_nodes.contains(&dragged) {
                    self.selected_nodes.len().saturating_sub(1)
                } else {
                    0
                };
                let ghost_label = if selection_extra > 0 {
                    format!("{} (+{})", node.name, selection_extra)
                } else {
                    node.name.clone()
                };
                paint_drag_preview(
                    ui,
                    pointer,
                    &ghost_label,
                    icons,
                    primitive_icon_name(node.primitive, node.is_folder),
                    primitive_icon_fallback(node.primitive, node.is_folder),
                );
            }
        }

        // Sync selected_node from multi-select (first element).
        self.selected_node = self.selected_nodes.first().copied();
    }

    fn node_matches_filter(&self, scene: &SceneGraph, id: SceneNodeId, lower: &str) -> bool {
        if let Some(node) = scene.get(id) {
            if node.name.to_lowercase().contains(lower) {
                return true;
            }
            // Check children recursively.
            for child in &node.children {
                if self.node_matches_filter(scene, *child, lower) {
                    return true;
                }
            }
        }
        false
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
        is_filtering: bool,
        filter_lower: &str,
        _sibling_index: usize,
    ) {
        let node = match scene.get(id) {
            Some(n) => n,
            None => return,
        };

        // Skip removed (soft-deleted) nodes.
        if node.name.is_empty() {
            return;
        }

        // When filtering, hide nodes that don't match (but still render matching children).
        if is_filtering && !node.name.to_lowercase().contains(filter_lower) {
            // Check if any descendant matches.
            let child_matches = node.children.iter().any(|&cid| {
                self.node_matches_filter(scene, cid, filter_lower)
            });
            if !child_matches {
                return;
            }
        }

        let node_name = node.name.clone();
        let node_visible = node.visible;
        let primitive = node.primitive;
        let is_folder = node.is_folder;
        let child_ids = node.children.clone();
        let has_children = !child_ids.is_empty();
        let parent_id = node.parent;

        let is_selected = self.selected_nodes.contains(&id) || self.box_select_candidates.contains(&id);
        let is_primary = self.selected_node == Some(id);
        let is_open = !self.collapsed_nodes.contains(&id);
        let is_renaming = matches!(self.renaming, Some((rename_id, _)) if rename_id == id);

        // Text color based on selection and visibility.
        let text_color = if !node_visible {
            egui::Color32::from_rgb(80, 80, 90)
        } else if is_selected {
            egui::Color32::WHITE
        } else {
            egui::Color32::from_rgb(180, 180, 190)
        };

        let row_width = panel_width.max(120.0);
        let row_height = 24.0;
        let inner = ui.allocate_ui_with_layout(
            egui::Vec2::new(row_width, row_height),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let indent_width = depth as f32 * 14.0;
                ui.add_space(indent_width);

                let toggle_width = if has_children { 14.0 } else { 18.0 };
                let leading_icon_width = 20.0;
                let actions_width = 48.0;
                let base_label_width = (row_width
                    - indent_width
                    - toggle_width
                    - leading_icon_width
                    - actions_width
                    - 16.0)
                    .max(48.0);

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
                        let response =
                            ui.add_sized([input_width, 22.0], egui::TextEdit::singleline(new_name));
                        if self.rename_needs_focus {
                            response.request_focus();
                            self.rename_needs_focus = false;
                        }

                        let enter_pressed =
                            response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        let escape_pressed =
                            response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape));
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

                    let response = selectable_node_label(
                        ui,
                        &node_name,
                        base_label_width,
                        is_selected,
                        is_primary,
                        text_color,
                    );

                    if response.clicked() {
                        self.handle_click(ui, id);
                    }
                    if response.double_clicked() {
                        self.begin_rename(id, node_name.clone());
                    }

                    if response.drag_started() {
                        self.dragged_node = Some(id);
                    }

                    // Drag-over drop zones with insertion line support.
                    let can_drop_here = matches!(self.dragged_node, Some(dragged) if dragged != id);
                    if can_drop_here && response.hovered() {
                        let row_rect = response.rect;
                        let rel_y = ui.input(|i| i.pointer.hover_pos())
                            .map(|p| (p.y - row_rect.top()) / row_rect.height())
                            .unwrap_or(0.5);

                        let (is_above, is_below, _is_center) = if rel_y < 0.25 {
                            (true, false, false)
                        } else if rel_y > 0.75 {
                            (false, true, false)
                        } else {
                            (false, false, true)
                        };

                        // Draw insertion line indicator.
                        if is_above {
                            let line_y = row_rect.top();
                            ui.painter().line_segment(
                                [egui::pos2(row_rect.left(), line_y), egui::pos2(row_rect.right(), line_y)],
                                egui::Stroke::new(2.0, egui::Color32::from_rgb(212, 119, 26)),
                            );
                        } else if is_below {
                            let line_y = row_rect.bottom();
                            ui.painter().line_segment(
                                [egui::pos2(row_rect.left(), line_y), egui::pos2(row_rect.right(), line_y)],
                                egui::Stroke::new(2.0, egui::Color32::from_rgb(212, 119, 26)),
                            );
                        } else {
                            // Center: highlight as drop target (child).
                            // Stronger fill + thicker stroke + accent bar so the
                            // user sees clearly which node receives the drop.
                            let fill_alpha = if is_folder { 60 } else { 40 };
                            ui.painter().rect_filled(
                                row_rect.expand(1.0),
                                4.0,
                                egui::Color32::from_rgba_premultiplied(212, 119, 26, fill_alpha),
                            );
                            ui.painter().rect_stroke(
                                row_rect.expand(1.0),
                                4.0,
                                egui::Stroke::new(2.0, egui::Color32::from_rgb(212, 119, 26)),
                            );
                            // Accent bar on the left edge (same style as primary selection).
                            let accent_rect = egui::Rect::from_min_max(
                                egui::pos2(row_rect.left(), row_rect.top()),
                                egui::pos2(row_rect.left() + 3.0, row_rect.bottom()),
                            );
                            ui.painter()
                                .rect_filled(accent_rect, 2.0, egui::Color32::from_rgb(232, 152, 58));
                            // Drop target marker dot.
                            ui.painter().circle_filled(
                                egui::pos2(row_rect.left() + 8.0, row_rect.center().y),
                                if is_folder { 4.5 } else { 3.5 },
                                egui::Color32::from_rgb(232, 152, 58),
                            );
                            // Folder receptor indicator: second concentric ring.
                            if is_folder {
                                ui.painter().circle_stroke(
                                    egui::pos2(row_rect.left() + 8.0, row_rect.center().y),
                                    7.0,
                                    egui::Stroke::new(1.0, egui::Color32::from_rgb(212, 119, 26)),
                                );
                            }
                        }

                        if ui.input(|i| i.pointer.any_released()) {
                            if let Some(dragged) = self.dragged_node {
                                if is_above {
                                    // Insert BEFORE this node (same parent).
                                    self.pending_reparent = Some((dragged, parent_id));
                                    self.pending_insert_before = Some(id);
                                } else if is_below {
                                    // Insert AFTER this node (same parent).
                                    // Find the next sibling to insert before, or None to append.
                                    let siblings = if let Some(pid) = parent_id {
                                        scene.get(pid).map(|p| p.children.clone()).unwrap_or_default()
                                    } else {
                                        scene.roots().to_vec()
                                    };
                                    let pos = siblings.iter().position(|&s| s == id);
                                    let next_sibling = pos.and_then(|p| siblings.get(p + 1).copied());
                                    self.pending_reparent = Some((dragged, parent_id));
                                    self.pending_insert_before = next_sibling;
                                } else {
                                    // Center: make child.
                                    self.pending_reparent = Some((dragged, Some(id)));
                                    self.pending_insert_before = None;
                                }
                                self.dragged_node = None;
                            }
                        }
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

                            let visibility_icon = if node_visible {
                                "visible.png"
                            } else {
                                "hidden.png"
                            };
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
            },
        );
        let row_rect = inner.response.rect;

        // Box-select intersection: mark candidates during drag.
        if let Some(box_rect) = self.box_select_rect {
            if box_rect.intersects(row_rect) {
                self.box_select_candidates.insert(id);
            }
        }

        // Render children.
        if has_children && (is_open || is_filtering) {
            for (ci, child_id) in child_ids.iter().enumerate() {
                self.show_node(
                    ui, scene, *child_id, depth + 1, lang, icons, panel_width,
                    is_filtering, filter_lower, ci,
                );
            }
        }
    }

    /// Handle click with Ctrl and Shift support for multi-select.
    fn handle_click(&mut self, ui: &Ui, id: SceneNodeId) {
        let ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.mac_cmd);
        let shift = ui.input(|i| i.modifiers.shift);

        if ctrl {
            // Ctrl+click: toggle individual node in multi-select (like Roblox Studio).
            if let Some(idx) = self.selected_nodes.iter().position(|&sid| sid == id) {
                self.selected_nodes.remove(idx);
            } else {
                self.selected_nodes.push(id);
            }
        } else if shift {
            // Shift+click: toggle in multi-select list.
            if let Some(idx) = self.selected_nodes.iter().position(|&sid| sid == id) {
                self.selected_nodes.remove(idx);
            } else {
                self.selected_nodes.push(id);
            }
        } else {
            // Single click: replace entire selection.
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
            reparent_before: self.pending_insert_before.take(),
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

    fn show_node_menu(
        &mut self,
        ui: &mut Ui,
        id: SceneNodeId,
        node_name: &str,
        is_folder: bool,
        lang: Language,
    ) {
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
        if ui
            .button(
                egui::RichText::new(t("app.delete_menu", lang))
                    .color(egui::Color32::from_rgb(220, 80, 80)),
            )
            .clicked()
        {
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
    primary: bool,
    text_color: egui::Color32,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, 22.0), egui::Sense::click_and_drag());

    if selected {
        ui.painter().rect_filled(
            rect,
            4.0,
            if primary {
                egui::Color32::from_rgba_premultiplied(212, 119, 26, 42)
            } else {
                egui::Color32::from_rgba_premultiplied(212, 119, 26, 24)
            },
        );

        ui.painter().rect_stroke(
            rect,
            4.0,
            egui::Stroke::new(
                if primary { 1.4 } else { 1.0 },
                if primary {
                    egui::Color32::from_rgb(232, 152, 58)
                } else {
                    egui::Color32::from_rgba_premultiplied(212, 119, 26, 120)
                },
            ),
        );

        if primary {
            let accent_rect = egui::Rect::from_min_max(
                egui::pos2(rect.left(), rect.top()),
                egui::pos2(rect.left() + 3.0, rect.bottom()),
            );
            ui.painter()
                .rect_filled(accent_rect, 2.0, egui::Color32::from_rgb(232, 152, 58));
        }
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

fn paint_drag_preview(
    ui: &Ui,
    pointer: egui::Pos2,
    label: &str,
    icons: &UiIconAtlas,
    icon_name: &'static str,
    fallback: &'static str,
) {
    let layer_id = egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("hierarchy_drag_preview"),
    );
    let painter = ui.ctx().layer_painter(layer_id);
    let font_id = egui::FontId::proportional(11.0);
    let galley = painter.layout_no_wrap(label.to_string(), font_id.clone(), egui::Color32::WHITE);

    // Layout: [icon 18px] [gap 6px] [text] with padding around the whole pill.
    let icon_size = 16.0;
    let icon_gap = 6.0;
    let pad_x = 10.0;
    let pad_y = 7.0;
    let inner_w = icon_size + icon_gap + galley.size().x;
    let inner_h = galley.size().y.max(icon_size);
    let size = egui::vec2(inner_w + pad_x * 2.0, inner_h + pad_y * 2.0);
    let rect = egui::Rect::from_min_size(pointer + egui::vec2(14.0, 12.0), size);

    painter.rect_filled(
        rect,
        6.0,
        egui::Color32::from_rgba_premultiplied(26, 27, 33, 230),
    );
    painter.rect_stroke(
        rect,
        6.0,
        egui::Stroke::new(1.0, egui::Color32::from_rgb(212, 119, 26)),
    );

    // Icon column.
    let icon_rect = egui::Rect::from_min_size(
        egui::pos2(rect.left() + pad_x, rect.center().y - icon_size * 0.5),
        egui::vec2(icon_size, icon_size),
    );
    let icon_tint = egui::Color32::from_rgb(232, 152, 58);
    if !icons.paint(&painter, icon_name, icon_rect, icon_tint) {
        painter.text(
            icon_rect.center(),
            egui::Align2::CENTER_CENTER,
            fallback,
            egui::FontId::proportional(12.0),
            icon_tint,
        );
    }

    // Text column to the right of the icon.
    painter.galley(
        egui::pos2(icon_rect.right() + icon_gap, rect.center().y - galley.size().y * 0.5),
        galley,
        egui::Color32::WHITE,
    );
}
