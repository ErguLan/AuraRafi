use egui::{Color32, Pos2, Rect, Stroke, Vec2};
use raf_core::i18n::t;
use raf_electronics::component::{ElectronicComponent, Pin, PinDirection, SimModel};
use raf_electronics::simulation::SimulationResults;
use raf_render::api_graphic_basic::schematic_symbols::{schematic_symbol_recipe, SchematicSymbolKind};

use super::{
    COMP_BODY_H, COMP_BODY_W, ConnectionCandidate, ConnectionKind, ContextMenu,
    GRID_STEP, PIN_DOT_RADIUS, PIN_SNAP_DISTANCE, PlacementMode, SchematicSelection,
    SchematicViewPanel, WIRE_ENDPOINT_SNAP_DISTANCE, WIRE_HIT_DISTANCE,
    WIRE_JUNCTION_SNAP_DISTANCE,
};
use crate::theme;

impl SchematicViewPanel {
    pub(super) fn draw_canvas(&mut self, ui: &mut egui::Ui, rect: Rect) -> bool {
        let mut changed = false;
        let hover_mouse = ui.input(|i| i.pointer.hover_pos()).filter(|mouse| rect.contains(*mouse));
        let hover_candidate = hover_mouse.map(|mouse| self.resolve_connection_candidate(mouse, rect));
        let hovered_component = hover_mouse.and_then(|mouse| self.hit_test_component(mouse, rect));
        let hovered_wire = hover_mouse.and_then(|mouse| self.hit_test_wire(mouse, rect));

        {
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 0.0, Color32::from_rgb(13, 14, 18));
            painter.rect_stroke(
                rect.shrink(0.5),
                0.0,
                Stroke::new(1.0, Color32::from_rgb(34, 36, 42)),
            );

            self.draw_grid(&painter, rect);

            let selected_wire = match self.selection {
                SchematicSelection::Wire(idx) => Some(idx),
                _ => None,
            };

            for (idx, wire) in self.schematic.wires.iter().enumerate() {
                let start = self.world_to_screen(Pos2::new(wire.start.x, wire.start.y), rect);
                let end = self.world_to_screen(Pos2::new(wire.end.x, wire.end.y), rect);
                let is_selected = selected_wire == Some(idx);
                let is_hovered = hovered_wire == Some(idx);
                let wire_color = if is_selected {
                    theme::ACCENT
                } else if is_hovered || self.placement == PlacementMode::Wire {
                    Color32::from_rgb(118, 226, 134)
                } else {
                    Color32::from_rgb(82, 200, 100)
                };
                let wire_width = if is_selected {
                    (3.0 * self.zoom).max(2.5)
                } else if is_hovered {
                    (2.5 * self.zoom).max(2.0)
                } else {
                    (2.0 * self.zoom).max(1.5)
                };

                painter.line_segment([start, end], Stroke::new(wire_width, wire_color));

                if is_selected || is_hovered || self.placement == PlacementMode::Wire {
                    let node_radius = (3.5 * self.zoom).max(3.0);
                    for point in [start, end] {
                        painter.circle_filled(point, node_radius, Color32::from_rgb(18, 18, 24));
                        painter.circle_stroke(point, node_radius, Stroke::new(1.0, wire_color));
                    }
                }

                if !wire.net.is_empty() {
                    let mid = Pos2::new((start.x + end.x) * 0.5, (start.y + end.y) * 0.5 - 10.0);
                    painter.text(
                        mid,
                        egui::Align2::CENTER_BOTTOM,
                        &wire.net,
                        egui::FontId::proportional((9.0 * self.zoom).max(9.0)),
                        Color32::from_rgb(120, 235, 140),
                    );
                }
            }

            for (idx, comp) in self.schematic.components.iter().enumerate() {
                let is_selected = matches!(self.selection, SchematicSelection::Component(sel) if sel == idx);
                let is_hovered = hovered_component == Some(idx);
                let hovered_pin = hover_candidate.and_then(|candidate| {
                    if candidate.component_index == Some(idx) {
                        candidate.pin_index
                    } else {
                        None
                    }
                });
                self.draw_component(&painter, rect, comp, is_selected, is_hovered, hovered_pin);
            }

            if let (PlacementMode::Wire, Some(start)) = (&self.placement, self.wire_start) {
                let end_candidate = hover_candidate.unwrap_or_else(|| ConnectionCandidate::grid(start.world));
                let route = orthogonal_route_points(start.world, end_candidate.world);
                self.draw_wire_preview(&painter, rect, &route);
                self.draw_connection_candidate(&painter, rect, start, true);
                self.draw_connection_candidate(&painter, rect, end_candidate, true);
            } else if let Some(candidate) = hover_candidate {
                if matches!(candidate.kind, ConnectionKind::Pin | ConnectionKind::WireEndpoint | ConnectionKind::WireJunction) {
                    self.draw_connection_candidate(&painter, rect, candidate, false);
                }
            }

            if let PlacementMode::Component(idx) = &self.placement {
                if let Some(mouse) = hover_mouse {
                    let world = self.snap_to_grid(self.screen_to_world(mouse, rect));
                    let screen = self.world_to_screen(world, rect);
                    let body = self.component_body_rect(screen, SchematicSymbolKind::Generic);
                    painter.rect_filled(
                        body,
                        6.0 * self.zoom,
                        Color32::from_rgba_premultiplied(212, 119, 26, 52),
                    );
                    painter.rect_stroke(
                        body,
                        6.0 * self.zoom,
                        Stroke::new(1.5, Color32::from_rgba_premultiplied(212, 119, 26, 140)),
                    );
                    if let Some(template) = self.library.components.get(*idx) {
                        painter.text(
                            Pos2::new(body.center().x, body.bottom() + 10.0),
                            egui::Align2::CENTER_TOP,
                            &template.name,
                            egui::FontId::proportional((10.0 * self.zoom).max(10.0)),
                            theme::ACCENT,
                        );
                    }
                }
            }

            if self.show_test_results && !self.test_results.is_empty() {
                self.draw_test_results(&painter, rect);
            }

            if self.sim_active {
                if let Some(ref sim) = self.sim_results {
                    self.draw_sim_overlay(&painter, rect, sim);
                }
                self.sim_phase = (self.sim_phase + 0.015) % 1.0;
                ui.ctx().request_repaint();
            }

            if self.show_export_menu {
                self.draw_export_menu(&painter, rect);
            }

            if let Some(candidate) = hover_candidate {
                self.draw_hover_hint(&painter, rect, candidate);
            }

            let help_copy = if self.placement == PlacementMode::Wire {
                t("app.schematic_canvas_hint_wire", self.lang)
            } else {
                t("app.schematic_canvas_hint", self.lang)
            };
            let info_text = format!("Zoom: {:.1}x | {}", self.zoom, help_copy);
            painter.text(
                Pos2::new(rect.left() + 10.0, rect.top() + 10.0),
                egui::Align2::LEFT_TOP,
                info_text,
                egui::FontId::proportional(10.0),
                Color32::from_rgb(92, 98, 112),
            );
        }

        let resp = ui.allocate_rect(rect, egui::Sense::click_and_drag());

        if self.placement == PlacementMode::None {
            if resp.drag_started_by(egui::PointerButton::Primary) {
                if let Some(mouse) = hover_mouse {
                    if let Some(idx) = self.hit_test_component(mouse, rect) {
                        let comp_screen = self.world_to_screen(
                            Pos2::new(
                                self.schematic.components[idx].position.x,
                                self.schematic.components[idx].position.y,
                            ),
                            rect,
                        );
                        let offset = Vec2::new(mouse.x - comp_screen.x, mouse.y - comp_screen.y);
                        self.drag_state = Some((idx, offset));
                        self.selection = SchematicSelection::Component(idx);
                    }
                }
            }

            if resp.dragged_by(egui::PointerButton::Primary) {
                if let Some((idx, offset)) = self.drag_state {
                    if let Some(mouse) = hover_mouse {
                        let target_screen = Pos2::new(mouse.x - offset.x, mouse.y - offset.y);
                        let world = self.snap_to_grid(self.screen_to_world(target_screen, rect));
                        if idx < self.schematic.components.len() {
                            let new_pos = glam::Vec2::new(world.x, world.y);
                            if self.schematic.components[idx].position != new_pos {
                                self.schematic.components[idx].position = new_pos;
                                changed = true;
                            }
                        }
                    }
                }
            }

            if resp.drag_stopped_by(egui::PointerButton::Primary) {
                self.drag_state = None;
            }
        }

        if resp.clicked() && self.drag_state.is_none() {
            self.context_menu = None;

            if let Some(mouse) = hover_mouse {
                let candidate = self.resolve_connection_candidate(mouse, rect);

                match &self.placement {
                    PlacementMode::Component(idx) => {
                        if let Some(template) = self.library.components.get(*idx) {
                            let mut comp = template.instantiate();
                            comp.position = glam::Vec2::new(candidate.world.x, candidate.world.y);
                            self.schematic.add_component(comp);
                            let last = self.schematic.components.len().saturating_sub(1);
                            self.selection = SchematicSelection::Component(last);
                            changed = true;
                        }
                    }
                    PlacementMode::Wire => {
                        if let Some(start) = self.wire_start {
                            let (start_world, start_changed) = self.prepare_connection_for_commit(start);
                            let (end_world, end_changed) = self.prepare_connection_for_commit(candidate);
                            changed |= start_changed || end_changed;

                            if start_world.distance(end_world) > 0.01 {
                                let route = orthogonal_route_points(start_world, end_world);
                                let route_points: Vec<glam::Vec2> = route
                                    .iter()
                                    .map(|point| glam::Vec2::new(point.x, point.y))
                                    .collect();
                                changed |= self.schematic.add_wire_path(&route_points, "") > 0;
                            }

                            self.wire_start = Some(candidate.chain_anchor(end_world));
                        } else {
                            self.wire_start = Some(candidate);
                        }
                    }
                    PlacementMode::None => {
                        if let Some((idx, _, _)) = self.hit_test_pin(mouse, rect) {
                            self.selection = SchematicSelection::Component(idx);
                        } else if let Some(idx) = self.hit_test_component(mouse, rect) {
                            self.selection = SchematicSelection::Component(idx);
                        } else if let Some(idx) = self.hit_test_wire(mouse, rect) {
                            self.selection = SchematicSelection::Wire(idx);
                        } else {
                            self.selection = SchematicSelection::None;
                        }
                    }
                }
            }
        }

        if resp.clicked_by(egui::PointerButton::Secondary) {
            if let PlacementMode::Wire = self.placement {
                if self.wire_start.is_some() {
                    self.wire_start = None;
                    return changed;
                }
            }

            if let Some(mouse) = hover_mouse {
                let target = if let Some((idx, _, _)) = self.hit_test_pin(mouse, rect) {
                    self.selection = SchematicSelection::Component(idx);
                    SchematicSelection::Component(idx)
                } else if let Some(idx) = self.hit_test_component(mouse, rect) {
                    self.selection = SchematicSelection::Component(idx);
                    SchematicSelection::Component(idx)
                } else if let Some(idx) = self.hit_test_wire(mouse, rect) {
                    self.selection = SchematicSelection::Wire(idx);
                    SchematicSelection::Wire(idx)
                } else {
                    SchematicSelection::None
                };

                self.context_menu = Some(ContextMenu {
                    screen_pos: mouse,
                    target,
                });
            }
        }

        if resp.dragged_by(egui::PointerButton::Middle) {
            self.offset += resp.drag_delta();
        }

        if self.context_menu.is_none() && resp.dragged_by(egui::PointerButton::Secondary) {
            self.offset += resp.drag_delta();
        }

        if ui.rect_contains_pointer(rect) {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                if let Some(mouse) = hover_mouse {
                    let old_zoom = self.zoom;
                    self.zoom = (self.zoom + scroll * 0.002).clamp(0.3, 4.0);
                    let factor = self.zoom / old_zoom;
                    self.offset.x = mouse.x - rect.left() - (mouse.x - rect.left() - self.offset.x) * factor;
                    self.offset.y = mouse.y - rect.top() - (mouse.y - rect.top() - self.offset.y) * factor;
                } else {
                    self.zoom = (self.zoom + scroll * 0.002).clamp(0.3, 4.0);
                }
            }
        }

        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.show_export_menu {
                self.show_export_menu = false;
            } else if self.sim_active {
                self.sim_active = false;
                self.sim_results = None;
                self.sim_phase = 0.0;
            } else if self.placement != PlacementMode::None {
                self.placement = PlacementMode::None;
                self.wire_start = None;
            } else if self.show_test_results {
                self.show_test_results = false;
            } else if self.context_menu.is_some() {
                self.context_menu = None;
            } else if self.editing_value.is_some() {
                self.editing_value = None;
            } else {
                self.selection = SchematicSelection::None;
            }
        }

        if self.show_export_menu {
            let key_1 = ui.input(|i| i.key_pressed(egui::Key::Num1));
            let key_2 = ui.input(|i| i.key_pressed(egui::Key::Num2));
            let key_3 = ui.input(|i| i.key_pressed(egui::Key::Num3));

            if key_1 {
                let result = raf_electronics::export_netlist_text(&self.schematic);
                self.export_message = Some(format!("Netlist: {} bytes", result.content.len()));
                self.show_export_menu = false;
                tracing::info!("Export netlist:\n{}", result.content);
            }
            if key_2 {
                let result = raf_electronics::export_bom_csv(&self.schematic);
                self.export_message = Some(format!("BOM CSV: {} bytes", result.content.len()));
                self.show_export_menu = false;
                tracing::info!("Export BOM:\n{}", result.content);
            }
            if key_3 {
                let result = raf_electronics::export_svg(&self.schematic);
                self.export_message = Some(format!("SVG: {} bytes", result.content.len()));
                self.show_export_menu = false;
                tracing::info!("Export SVG:\n{}", result.content);
            }
        }

        if ui.input(|i| i.key_pressed(egui::Key::Delete)) {
            changed |= self.delete_selection();
        }

        if ui.input(|i| i.key_pressed(egui::Key::R)) {
            if let Some(idx) = self.selected_component_index() {
                let comp = &mut self.schematic.components[idx];
                comp.rotation = (comp.rotation + 90.0) % 360.0;
                changed = true;
            }
        }

        if ui.input(|i| i.key_pressed(egui::Key::M)) {
            if let Some(idx) = self.selected_component_index() {
                let comp = &mut self.schematic.components[idx];
                for pin in &mut comp.pins {
                    pin.offset.x = -pin.offset.x;
                }
                changed = true;
            }
        }

        if ui.input(|i| i.key_pressed(egui::Key::F)) {
            if let Some(idx) = self.selected_component_index() {
                let comp = &self.schematic.components[idx];
                self.offset.x = rect.width() * 0.5 - comp.position.x * self.zoom;
                self.offset.y = rect.height() * 0.5 - comp.position.y * self.zoom;
            }
        }

        let ctrl_d = ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::D));
        if ctrl_d {
            changed |= self.duplicate_selection();
        }

        changed
    }

    pub(super) fn draw_context_menu(&mut self, ui: &mut egui::Ui) -> bool {
        let menu = match self.context_menu.clone() {
            Some(menu) => menu,
            None => return false,
        };

        let mut close_menu = false;
        let mut changed = false;
        let menu_id = egui::Id::new("schematic_context_menu");

        egui::Area::new(menu_id)
            .fixed_pos(menu.screen_pos)
            .pivot(egui::Align2::LEFT_TOP)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(150.0);

                    match menu.target {
                        SchematicSelection::Component(idx) => {
                            if ui.button(t("app.rotate_r", self.lang)).clicked() {
                                if idx < self.schematic.components.len() {
                                    self.schematic.components[idx].rotation =
                                        (self.schematic.components[idx].rotation + 90.0) % 360.0;
                                    changed = true;
                                }
                                close_menu = true;
                            }

                            if ui.button(t("app.edit_value", self.lang)).clicked() {
                                if idx < self.schematic.components.len() {
                                    let value = self.schematic.components[idx].value.clone();
                                    self.editing_value = Some((idx, value));
                                }
                                close_menu = true;
                            }

                            if ui.button(t("app.duplicate_ctrl_d", self.lang)).clicked() {
                                self.selection = SchematicSelection::Component(idx);
                                changed |= self.duplicate_selection();
                                close_menu = true;
                            }

                            ui.separator();

                            if ui
                                .button(egui::RichText::new(t("app.delete_del", self.lang)).color(theme::STATUS_ERROR))
                                .clicked()
                            {
                                self.selection = SchematicSelection::Component(idx);
                                changed |= self.delete_selection();
                                close_menu = true;
                            }
                        }
                        SchematicSelection::Wire(idx) => {
                            if ui
                                .button(egui::RichText::new(t("app.delete_wire_del", self.lang)).color(theme::STATUS_ERROR))
                                .clicked()
                            {
                                self.selection = SchematicSelection::Wire(idx);
                                changed |= self.delete_selection();
                                close_menu = true;
                            }
                        }
                        SchematicSelection::None => {
                            if ui.button(t("app.wire_mode", self.lang)).clicked() {
                                self.placement = PlacementMode::Wire;
                                close_menu = true;
                            }

                            if ui.button(t("app.electrical_test", self.lang)).clicked() {
                                self.test_results = self.schematic.electrical_test();
                                self.show_test_results = true;
                                close_menu = true;
                            }
                        }
                    }
                });
            });

        if close_menu {
            self.context_menu = None;
        }

        changed
    }

    pub(super) fn draw_value_editor(&mut self, ui: &mut egui::Ui) -> bool {
        let (idx, mut buffer) = match self.editing_value.take() {
            Some(value) => value,
            None => return false,
        };

        let mut keep_open = true;
        let mut changed = false;

        egui::Window::new(t("app.edit_value", self.lang))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                ui.horizontal(|ui| {
                    ui.label(t("app.value", self.lang));
                    let response = ui.text_edit_singleline(&mut buffer);
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        if idx < self.schematic.components.len() {
                            self.schematic.components[idx].value = buffer.clone();
                            self.schematic.components[idx].sync_sim_model_from_value();
                            changed = true;
                        }
                        keep_open = false;
                    }
                });

                ui.horizontal(|ui| {
                    if ui.button(t("app.ok", self.lang)).clicked() {
                        if idx < self.schematic.components.len() {
                            self.schematic.components[idx].value = buffer.clone();
                            self.schematic.components[idx].sync_sim_model_from_value();
                            changed = true;
                        }
                        keep_open = false;
                    }
                    if ui.button(t("app.cancel", self.lang)).clicked() {
                        keep_open = false;
                    }
                });
            });

        if keep_open {
            self.editing_value = Some((idx, buffer));
        }

        changed
    }

    fn draw_grid(&self, painter: &egui::Painter, rect: Rect) {
        let minor = Color32::from_rgba_premultiplied(180, 186, 200, 14);
        let major = Color32::from_rgba_premultiplied(220, 226, 240, 30);
        let axis = Color32::from_rgba_premultiplied(212, 119, 26, 46);
        let step = GRID_STEP * self.zoom;
        if step < 4.0 {
            return;
        }

        let world_left = ((rect.left() - rect.left() - self.offset.x) / self.zoom / GRID_STEP).floor() as i32 - 2;
        let world_right = ((rect.right() - rect.left() - self.offset.x) / self.zoom / GRID_STEP).ceil() as i32 + 2;
        let world_top = ((rect.top() - rect.top() - self.offset.y) / self.zoom / GRID_STEP).floor() as i32 - 2;
        let world_bottom = ((rect.bottom() - rect.top() - self.offset.y) / self.zoom / GRID_STEP).ceil() as i32 + 2;

        for ix in world_left..=world_right {
            let x_world = ix as f32 * GRID_STEP;
            let x = rect.left() + x_world * self.zoom + self.offset.x;
            let color = if ix == 0 { axis } else if ix % 5 == 0 { major } else { minor };
            let width = if ix == 0 { 1.2 } else { 0.5 };
            painter.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], Stroke::new(width, color));
        }

        for iy in world_top..=world_bottom {
            let y_world = iy as f32 * GRID_STEP;
            let y = rect.top() + y_world * self.zoom + self.offset.y;
            let color = if iy == 0 { axis } else if iy % 5 == 0 { major } else { minor };
            let width = if iy == 0 { 1.2 } else { 0.5 };
            painter.line_segment([Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)], Stroke::new(width, color));
        }
    }

    fn draw_component(
        &self,
        painter: &egui::Painter,
        canvas_rect: Rect,
        comp: &ElectronicComponent,
        is_selected: bool,
        is_hovered: bool,
        hovered_pin: Option<usize>,
    ) {
        let center = self.world_to_screen(Pos2::new(comp.position.x, comp.position.y), canvas_rect);
        let symbol_kind = symbol_kind_for_component(comp);
        let recipe = schematic_symbol_recipe(symbol_kind);
        let body = self.component_body_rect(center, symbol_kind);

        let fill = if is_selected {
            Color32::from_rgb(44, 38, 28)
        } else if is_hovered {
            Color32::from_rgb(28, 30, 36)
        } else {
            Color32::from_rgb(20, 22, 28)
        };
        let border = if is_selected {
            theme::ACCENT
        } else if is_hovered {
            Color32::from_rgb(92, 98, 112)
        } else {
            Color32::from_rgb(44, 46, 54)
        };

        painter.rect_filled(body, 6.0 * self.zoom, fill);
        painter.rect_stroke(body, 6.0 * self.zoom, Stroke::new(1.0, border));

        for segment in recipe.segments {
            let a = transform_local_point(center, segment[0], comp.rotation, self.zoom);
            let b = transform_local_point(center, segment[1], comp.rotation, self.zoom);
            painter.line_segment([a, b], Stroke::new((1.6 * self.zoom).max(1.2), Color32::from_rgb(228, 232, 240)));
        }

        for circle in recipe.open_circles {
            let screen = transform_local_point(center, circle.center, comp.rotation, self.zoom);
            painter.circle_stroke(screen, circle.radius * self.zoom, Stroke::new((1.2 * self.zoom).max(1.0), Color32::from_rgb(228, 232, 240)));
        }

        for circle in recipe.filled_circles {
            let screen = transform_local_point(center, circle.center, comp.rotation, self.zoom);
            painter.circle_filled(screen, circle.radius * self.zoom, Color32::from_rgb(228, 232, 240));
        }

        if symbol_kind == SchematicSymbolKind::Magnet {
            let left = transform_local_point(center, [-7.0, 0.0], comp.rotation, self.zoom);
            let right = transform_local_point(center, [7.0, 0.0], comp.rotation, self.zoom);
            painter.text(left, egui::Align2::CENTER_CENTER, "N", egui::FontId::proportional((9.0 * self.zoom).max(9.0)), theme::ACCENT);
            painter.text(right, egui::Align2::CENTER_CENTER, "S", egui::FontId::proportional((9.0 * self.zoom).max(9.0)), Color32::from_rgb(170, 198, 236));
        }

        painter.text(
            Pos2::new(center.x, body.top() - 5.0 * self.zoom),
            egui::Align2::CENTER_BOTTOM,
            &comp.designator,
            egui::FontId::proportional((10.0 * self.zoom).max(10.0)),
            theme::ACCENT,
        );

        painter.text(
            Pos2::new(center.x, body.bottom() + 5.0 * self.zoom),
            egui::Align2::CENTER_TOP,
            &comp.value,
            egui::FontId::proportional((9.0 * self.zoom).max(9.0)),
            Color32::from_rgb(190, 190, 198),
        );

        if !comp.footprint.trim().is_empty() {
            painter.text(
                Pos2::new(body.right(), body.bottom() + 17.0 * self.zoom),
                egui::Align2::RIGHT_TOP,
                &comp.footprint,
                egui::FontId::proportional((8.0 * self.zoom).max(8.0)),
                Color32::from_rgb(118, 122, 132),
            );
        }

        for (pin_idx, pin) in comp.pins.iter().enumerate() {
            let pin_world = component_pin_world(comp, pin);
            let pin_screen = self.world_to_screen(pin_world, canvas_rect);
            let pin_color = pin_direction_color(pin.direction);
            let pin_radius = if hovered_pin == Some(pin_idx) {
                (PIN_DOT_RADIUS + 2.0) * self.zoom
            } else {
                PIN_DOT_RADIUS * self.zoom
            }
            .max(3.0);

            painter.circle_filled(pin_screen, pin_radius, Color32::from_rgb(13, 14, 18));
            painter.circle_stroke(pin_screen, pin_radius, Stroke::new((1.5 * self.zoom).max(1.2), pin_color));

            let label_offset = if pin.offset.x < 0.0 { -9.0 * self.zoom } else { 9.0 * self.zoom };
            let align = if pin.offset.x < 0.0 {
                egui::Align2::RIGHT_CENTER
            } else {
                egui::Align2::LEFT_CENTER
            };
            painter.text(
                Pos2::new(pin_screen.x + label_offset, pin_screen.y),
                align,
                &pin.name,
                egui::FontId::proportional((8.0 * self.zoom).max(8.0)),
                if hovered_pin == Some(pin_idx) { Color32::WHITE } else { Color32::from_rgb(148, 152, 162) },
            );
        }
    }

    fn draw_wire_preview(&self, painter: &egui::Painter, canvas_rect: Rect, route: &[Pos2]) {
        for window in route.windows(2) {
            let start = self.world_to_screen(window[0], canvas_rect);
            let end = self.world_to_screen(window[1], canvas_rect);
            painter.line_segment(
                [start, end],
                Stroke::new((2.0 * self.zoom).max(1.5), Color32::from_rgb(120, 235, 140)),
            );
        }

        for point in route.iter().skip(1).take(route.len().saturating_sub(2)) {
            painter.circle_filled(
                self.world_to_screen(*point, canvas_rect),
                (3.0 * self.zoom).max(2.5),
                Color32::from_rgb(120, 235, 140),
            );
        }
    }

    fn draw_connection_candidate(
        &self,
        painter: &egui::Painter,
        canvas_rect: Rect,
        candidate: ConnectionCandidate,
        strong: bool,
    ) {
        let center = self.world_to_screen(candidate.world, canvas_rect);
        let color = match candidate.kind {
            ConnectionKind::Pin => Color32::from_rgb(110, 210, 255),
            ConnectionKind::WireEndpoint => Color32::from_rgb(118, 226, 134),
            ConnectionKind::WireJunction => theme::ACCENT,
            ConnectionKind::Grid => Color32::from_rgb(90, 98, 112),
        };
        let radius = if strong { (5.5 * self.zoom).max(4.0) } else { (4.5 * self.zoom).max(3.5) };
        painter.circle_filled(center, radius, Color32::from_rgb(13, 14, 18));
        painter.circle_stroke(center, radius, Stroke::new(1.4, color));
    }

    fn draw_hover_hint(&self, painter: &egui::Painter, canvas_rect: Rect, candidate: ConnectionCandidate) {
        let label = match candidate.kind {
            ConnectionKind::Pin => t("app.schematic_hover_pin", self.lang),
            ConnectionKind::WireEndpoint => t("app.schematic_hover_wire_end", self.lang),
            ConnectionKind::WireJunction => t("app.schematic_hover_junction", self.lang),
            ConnectionKind::Grid => t("app.schematic_hover_grid", self.lang),
        };

        painter.text(
            Pos2::new(canvas_rect.right() - 12.0, canvas_rect.bottom() - 12.0),
            egui::Align2::RIGHT_BOTTOM,
            label,
            egui::FontId::proportional(10.0),
            Color32::from_rgb(130, 136, 148),
        );
    }

    fn draw_test_results(&mut self, painter: &egui::Painter, canvas_rect: Rect) {
        let results_w = 320.0;
        let line_h = 18.0;
        let results_h = 40.0 + self.test_results.len() as f32 * line_h;
        let results_rect = Rect::from_min_size(
            Pos2::new(canvas_rect.right() - results_w - 10.0, canvas_rect.top() + 30.0),
            Vec2::new(results_w, results_h.min(300.0)),
        );

        painter.rect_filled(results_rect, 6.0, Color32::from_rgba_premultiplied(20, 20, 28, 230));
        painter.rect_stroke(results_rect, 6.0, Stroke::new(1.0, Color32::from_rgb(60, 60, 68)));

        painter.text(
            Pos2::new(results_rect.center().x, results_rect.top() + 14.0),
            egui::Align2::CENTER_CENTER,
            t("app.electrical_test_results", self.lang),
            egui::FontId::proportional(12.0),
            theme::ACCENT,
        );

        painter.text(
            Pos2::new(results_rect.right() - 8.0, results_rect.top() + 14.0),
            egui::Align2::RIGHT_CENTER,
            t("app.esc_close", self.lang),
            egui::FontId::proportional(9.0),
            Color32::from_rgb(150, 150, 160),
        );

        let mut y = results_rect.top() + 32.0;
        for result in &self.test_results {
            if y > results_rect.bottom() - 10.0 {
                break;
            }
            let color = if result.contains("passed") {
                Color32::from_rgb(100, 220, 100)
            } else if result.contains("Unconnected") {
                Color32::from_rgb(230, 160, 60)
            } else {
                Color32::from_rgb(200, 200, 210)
            };

            painter.text(
                Pos2::new(results_rect.left() + 10.0, y),
                egui::Align2::LEFT_CENTER,
                result,
                egui::FontId::proportional(10.0),
                color,
            );
            y += line_h;
        }
    }

    fn draw_sim_overlay(
        &self,
        painter: &egui::Painter,
        canvas_rect: Rect,
        sim: &SimulationResults,
    ) {
        for wire in &self.schematic.wires {
            let start = self.world_to_screen(Pos2::new(wire.start.x, wire.start.y), canvas_rect);
            let end = self.world_to_screen(Pos2::new(wire.end.x, wire.end.y), canvas_rect);
            let dx = end.x - start.x;
            let dy = end.y - start.y;
            let wire_len = (dx * dx + dy * dy).sqrt();
            if wire_len < 1.0 {
                continue;
            }

            let dot_count = ((wire_len / (20.0 * self.zoom)) as usize).clamp(2, 6);
            let nx = dx / wire_len;
            let ny = dy / wire_len;

            for i in 0..dot_count {
                let base_t = i as f32 / dot_count as f32;
                let t = (base_t + self.sim_phase) % 1.0;
                let px = start.x + dx * t;
                let py = start.y + dy * t;
                let size = 3.0 * self.zoom;
                let tip = Pos2::new(px + nx * size, py + ny * size);
                let left = Pos2::new(
                    px - nx * size * 0.5 - ny * size * 0.6,
                    py - ny * size * 0.5 + nx * size * 0.6,
                );
                let right = Pos2::new(
                    px - nx * size * 0.5 + ny * size * 0.6,
                    py - ny * size * 0.5 - nx * size * 0.6,
                );

                painter.add(egui::Shape::convex_polygon(
                    vec![tip, left, right],
                    Color32::from_rgb(80, 220, 255),
                    Stroke::NONE,
                ));
            }
        }

        for (ci, comp) in self.schematic.components.iter().enumerate() {
            let center = self.world_to_screen(Pos2::new(comp.position.x, comp.position.y), canvas_rect);

            if let Some(&power) = sim.component_power.get(&ci) {
                let heat = (power * 50.0).clamp(0.0, 1.0) as f32;
                if heat > 0.01 {
                    let r = (80.0 + heat * 175.0) as u8;
                    let g = (220.0 - heat * 180.0) as u8;
                    let b = (80.0 - heat * 60.0) as u8;
                    let heat_rect = Rect::from_center_size(
                        center,
                        Vec2::new(COMP_BODY_W * self.zoom + 8.0, COMP_BODY_H * self.zoom + 8.0),
                    );
                    painter.rect_stroke(
                        heat_rect,
                        6.0 * self.zoom,
                        Stroke::new(2.0, Color32::from_rgb(r, g, b)),
                    );
                }
            }

            if let Some(&current) = sim.component_currents.get(&ci) {
                if current.abs() > 1e-9 {
                    let label = if current.abs() < 0.001 {
                        format!("{:.1}uA", current * 1_000_000.0)
                    } else if current.abs() < 1.0 {
                        format!("{:.2}mA", current * 1000.0)
                    } else {
                        format!("{:.3}A", current)
                    };
                    painter.text(
                        Pos2::new(center.x, center.y + COMP_BODY_H * self.zoom * 0.5 + 18.0 * self.zoom),
                        egui::Align2::CENTER_TOP,
                        label,
                        egui::FontId::proportional((8.0 * self.zoom).max(8.0)),
                        Color32::from_rgb(80, 200, 255),
                    );
                }
            }

            if matches!(comp.sim_model, SimModel::Led { .. }) {
                if let Some(&current) = sim.component_currents.get(&ci) {
                    if current > 0.001 {
                        let glow_alpha = (current * 5000.0).clamp(0.0, 180.0) as u8;
                        let glow_radius = (12.0 + current * 2000.0).min(30.0) as f32 * self.zoom;
                        painter.circle_filled(
                            center,
                            glow_radius,
                            Color32::from_rgba_premultiplied(255, 200, 50, glow_alpha),
                        );
                    }
                }
            }
        }

        let status = if sim.converged {
            if self.lang == raf_core::config::Language::Spanish {
                "Simulacion DC activa"
            } else {
                "DC Simulation active"
            }
        } else if self.lang == raf_core::config::Language::Spanish {
            "Simulacion no convergio"
        } else {
            "Simulation did not converge"
        };

        painter.text(
            Pos2::new(canvas_rect.right() - 10.0, canvas_rect.top() + 10.0),
            egui::Align2::RIGHT_TOP,
            status,
            egui::FontId::proportional(11.0),
            if sim.converged {
                Color32::from_rgb(80, 220, 120)
            } else {
                Color32::from_rgb(255, 100, 80)
            },
        );
    }

    fn draw_export_menu(&self, painter: &egui::Painter, canvas_rect: Rect) {
        let menu_rect = Rect::from_min_size(
            Pos2::new(canvas_rect.center().x - 110.0, canvas_rect.top() + 60.0),
            Vec2::new(220.0, 130.0),
        );

        painter.rect_filled(menu_rect, 8.0, Color32::from_rgba_premultiplied(25, 25, 35, 240));
        painter.rect_stroke(menu_rect, 8.0, Stroke::new(1.0, theme::ACCENT));

        painter.text(
            Pos2::new(menu_rect.center().x, menu_rect.top() + 16.0),
            egui::Align2::CENTER_CENTER,
            t("app.export_schematic", self.lang),
            egui::FontId::proportional(12.0),
            theme::ACCENT,
        );

        let options = if self.lang == raf_core::config::Language::Spanish {
            ["1. Netlist (texto)", "2. BOM (CSV)", "3. SVG (imagen vectorial)"]
        } else {
            ["1. Netlist (text)", "2. BOM (CSV)", "3. SVG (vector image)"]
        };

        let mut y = menu_rect.top() + 38.0;
        for option in options {
            painter.text(
                Pos2::new(menu_rect.left() + 16.0, y),
                egui::Align2::LEFT_CENTER,
                option,
                egui::FontId::proportional(11.0),
                Color32::from_rgb(200, 200, 210),
            );
            y += 22.0;
        }

        painter.text(
            Pos2::new(menu_rect.center().x, menu_rect.bottom() - 10.0),
            egui::Align2::CENTER_BOTTOM,
            t("app.esc_close_1_2_3_export", self.lang),
            egui::FontId::proportional(9.0),
            Color32::from_rgb(130, 130, 140),
        );
    }

    fn hit_test_component(&self, mouse: Pos2, canvas_rect: Rect) -> Option<usize> {
        for (idx, comp) in self.schematic.components.iter().enumerate().rev() {
            let center = self.world_to_screen(Pos2::new(comp.position.x, comp.position.y), canvas_rect);
            let body = self.component_body_rect(center, symbol_kind_for_component(comp));
            if body.expand(6.0).contains(mouse) {
                return Some(idx);
            }
        }
        None
    }

    fn hit_test_pin(&self, mouse: Pos2, canvas_rect: Rect) -> Option<(usize, usize, Pos2)> {
        let mut best: Option<(usize, usize, Pos2, f32)> = None;

        for (comp_idx, comp) in self.schematic.components.iter().enumerate() {
            for (pin_idx, pin) in comp.pins.iter().enumerate() {
                let world = component_pin_world(comp, pin);
                let screen = self.world_to_screen(world, canvas_rect);
                let distance = screen.distance(mouse);
                if distance <= PIN_SNAP_DISTANCE && best.map(|entry| distance < entry.3).unwrap_or(true) {
                    best = Some((comp_idx, pin_idx, world, distance));
                }
            }
        }

        best.map(|(comp_idx, pin_idx, world, _)| (comp_idx, pin_idx, world))
    }

    fn hit_test_wire(&self, mouse: Pos2, canvas_rect: Rect) -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;

        for (idx, wire) in self.schematic.wires.iter().enumerate() {
            let a = self.world_to_screen(Pos2::new(wire.start.x, wire.start.y), canvas_rect);
            let b = self.world_to_screen(Pos2::new(wire.end.x, wire.end.y), canvas_rect);
            let dist = point_to_segment_distance(mouse, a, b);
            if dist < WIRE_HIT_DISTANCE && best.map(|entry| dist < entry.1).unwrap_or(true) {
                best = Some((idx, dist));
            }
        }

        best.map(|(idx, _)| idx)
    }

    fn resolve_connection_candidate(&self, mouse: Pos2, canvas_rect: Rect) -> ConnectionCandidate {
        let mut candidate = ConnectionCandidate::grid(self.snap_to_grid(self.screen_to_world(mouse, canvas_rect)));
        let mut best_distance = f32::MAX;

        if let Some((comp_idx, pin_idx, world)) = self.hit_test_pin(mouse, canvas_rect) {
            let distance = self.world_to_screen(world, canvas_rect).distance(mouse);
            if distance < best_distance {
                best_distance = distance;
                candidate = ConnectionCandidate {
                    world,
                    kind: ConnectionKind::Pin,
                    component_index: Some(comp_idx),
                    pin_index: Some(pin_idx),
                    wire_index: None,
                };
            }
        }

        for (idx, wire) in self.schematic.wires.iter().enumerate() {
            for endpoint in [Pos2::new(wire.start.x, wire.start.y), Pos2::new(wire.end.x, wire.end.y)] {
                let screen = self.world_to_screen(endpoint, canvas_rect);
                let distance = screen.distance(mouse);
                if distance <= WIRE_ENDPOINT_SNAP_DISTANCE && distance < best_distance {
                    best_distance = distance;
                    candidate = ConnectionCandidate {
                        world: endpoint,
                        kind: ConnectionKind::WireEndpoint,
                        component_index: None,
                        pin_index: None,
                        wire_index: Some(idx),
                    };
                }
            }

            let start = self.world_to_screen(Pos2::new(wire.start.x, wire.start.y), canvas_rect);
            let end = self.world_to_screen(Pos2::new(wire.end.x, wire.end.y), canvas_rect);
            let (projected, distance, t) = project_point_to_segment(mouse, start, end);
            if distance <= WIRE_JUNCTION_SNAP_DISTANCE && t > 0.05 && t < 0.95 && distance < best_distance {
                best_distance = distance;
                candidate = ConnectionCandidate {
                    world: self.snap_to_grid(self.screen_to_world(projected, canvas_rect)),
                    kind: ConnectionKind::WireJunction,
                    component_index: None,
                    pin_index: None,
                    wire_index: Some(idx),
                };
            }
        }

        candidate
    }

    fn prepare_connection_for_commit(&mut self, candidate: ConnectionCandidate) -> (Pos2, bool) {
        let changed = match candidate.kind {
            ConnectionKind::WireJunction => self.split_wire_at_world(candidate.world, candidate.wire_index),
            _ => false,
        };

        (candidate.world, changed)
    }

    fn split_wire_at_world(&mut self, world: Pos2, preferred_index: Option<usize>) -> bool {
        let world_vec = glam::Vec2::new(world.x, world.y);

        if let Some(idx) = preferred_index {
            if idx < self.schematic.wires.len() && self.schematic.split_wire_at(idx, world_vec) {
                return true;
            }
        }

        for idx in 0..self.schematic.wires.len() {
            let wire = &self.schematic.wires[idx];
            let start = Pos2::new(wire.start.x, wire.start.y);
            let end = Pos2::new(wire.end.x, wire.end.y);
            let distance = point_to_segment_distance(world, start, end);
            if distance <= 0.5 && self.schematic.split_wire_at(idx, world_vec) {
                return true;
            }
        }

        false
    }

    fn component_body_rect(&self, center: Pos2, kind: SchematicSymbolKind) -> Rect {
        let recipe = schematic_symbol_recipe(kind);
        Rect::from_center_size(
            center,
            Vec2::new((recipe.half_size[0] * 2.0 + 16.0) * self.zoom, (recipe.half_size[1] * 2.0 + 14.0) * self.zoom),
        )
    }

    fn world_to_screen(&self, world: Pos2, canvas_rect: Rect) -> Pos2 {
        Pos2::new(
            canvas_rect.left() + world.x * self.zoom + self.offset.x,
            canvas_rect.top() + world.y * self.zoom + self.offset.y,
        )
    }

    fn screen_to_world(&self, screen: Pos2, canvas_rect: Rect) -> Pos2 {
        Pos2::new(
            (screen.x - canvas_rect.left() - self.offset.x) / self.zoom,
            (screen.y - canvas_rect.top() - self.offset.y) / self.zoom,
        )
    }

    fn snap_to_grid(&self, pos: Pos2) -> Pos2 {
        Pos2::new(
            (pos.x / GRID_STEP).round() * GRID_STEP,
            (pos.y / GRID_STEP).round() * GRID_STEP,
        )
    }
}

fn component_pin_world(comp: &ElectronicComponent, pin: &Pin) -> Pos2 {
    let rot_rad = comp.rotation.to_radians();
    let cos_r = rot_rad.cos();
    let sin_r = rot_rad.sin();
    let raw_ox = pin.offset.x * GRID_STEP;
    let raw_oy = pin.offset.y * GRID_STEP;
    let rot_ox = raw_ox * cos_r - raw_oy * sin_r;
    let rot_oy = raw_ox * sin_r + raw_oy * cos_r;
    Pos2::new(comp.position.x + rot_ox, comp.position.y + rot_oy)
}

fn symbol_kind_for_component(comp: &ElectronicComponent) -> SchematicSymbolKind {
    match comp.sim_model {
        SimModel::Resistor { .. } => SchematicSymbolKind::Resistor,
        SimModel::Capacitor { .. } => SchematicSymbolKind::Capacitor,
        SimModel::Led { .. } => SchematicSymbolKind::Led,
        SimModel::Magnet { .. } => SchematicSymbolKind::Magnet,
        SimModel::DcSource { .. } => SchematicSymbolKind::Battery,
        SimModel::Wire if comp.designator.eq_ignore_ascii_case("GND") => SchematicSymbolKind::Ground,
        _ => SchematicSymbolKind::Generic,
    }
}

fn transform_local_point(center: Pos2, local: [f32; 2], rotation_deg: f32, zoom: f32) -> Pos2 {
    let radians = rotation_deg.to_radians();
    let cos_r = radians.cos();
    let sin_r = radians.sin();
    let x = local[0] * cos_r - local[1] * sin_r;
    let y = local[0] * sin_r + local[1] * cos_r;
    Pos2::new(center.x + x * zoom, center.y + y * zoom)
}

fn orthogonal_route_points(start: Pos2, end: Pos2) -> Vec<Pos2> {
    if (start.x - end.x).abs() < 0.01 || (start.y - end.y).abs() < 0.01 {
        vec![start, end]
    } else {
        vec![start, Pos2::new(end.x, start.y), end]
    }
}

fn project_point_to_segment(point: Pos2, start: Pos2, end: Pos2) -> (Pos2, f32, f32) {
    let ab = Vec2::new(end.x - start.x, end.y - start.y);
    let ap = Vec2::new(point.x - start.x, point.y - start.y);
    let len_sq = ab.x * ab.x + ab.y * ab.y;
    if len_sq < 0.001 {
        return (start, ap.length(), 0.0);
    }

    let t = ((ap.x * ab.x + ap.y * ab.y) / len_sq).clamp(0.0, 1.0);
    let projected = Pos2::new(start.x + ab.x * t, start.y + ab.y * t);
    let distance = point.distance(projected);
    (projected, distance, t)
}

fn point_to_segment_distance(point: Pos2, start: Pos2, end: Pos2) -> f32 {
    let (_, distance, _) = project_point_to_segment(point, start, end);
    distance
}

fn pin_direction_color(direction: PinDirection) -> Color32 {
    match direction {
        PinDirection::Input => Color32::from_rgb(100, 180, 255),
        PinDirection::Output => Color32::from_rgb(255, 140, 60),
        PinDirection::Bidirectional => Color32::from_rgb(150, 220, 150),
        PinDirection::Power => Color32::from_rgb(255, 80, 80),
        PinDirection::Ground => Color32::from_rgb(120, 120, 130),
    }
}