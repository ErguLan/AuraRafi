//! Schematic Editor Canvas Rendering and Interaction.
//!
//! This module manages the hybrid rendering of schematic designs (with WGPU GPU acceleration
//! and high-performance rendering, falling back to egui-based drawing if the GPU is not ready).
//! It also handles mouse/keyboard events for selection, dragging, wire routing,
//! component placement, and contextual menu interactions.

use eframe::egui_wgpu;
use egui::{Color32, Pos2, Rect, Stroke, Vec2};
use glam::Vec3;
use raf_core::i18n::t;
use raf_electronics::component::{ElectronicComponent, Pin, PinDirection, SimModel};
use raf_electronics::schematic::WireAnchor;
use raf_electronics::simulation::SimulationResults;
use raf_render::api_graphic_basic::command_list::BasicCommandList;
use raf_render::api_graphic_basic::schematic_symbols::{
    schematic_symbol_recipe, SchematicSymbolKind,
};
use raf_render::bridge::RenderRuntime;
use raf_render::scene_renderer::{FrameStats, SceneRenderFrame};

use super::{
    electronics_palette, ConnectionCandidate, ConnectionKind, ContextMenu, PlacementMode,
    SchematicSelection, SchematicViewPanel, COMP_BODY_H, COMP_BODY_W, GRID_STEP, PIN_DOT_RADIUS,
    PIN_SNAP_DISTANCE, WIRE_ENDPOINT_SNAP_DISTANCE, WIRE_HIT_DISTANCE, WIRE_JUNCTION_SNAP_DISTANCE,
};
use crate::panels::gpu_canvas::canvas_view_projection;
use crate::theme;

impl SchematicViewPanel {
    /// Renders and handles interaction events for the schematic canvas container.
    ///
    /// The interactive flow incorporates:
    /// 1. Mouse/pointer feedback: Selection clicks, bounding-box selection, dragging, panning, zooming.
    /// 2. Hybrid rasterization: If WGPU state is active and ready, delegates rendering of static lines,
    ///    symbol geometries, and major grids to GPU command buffers. Otherwise, falls back to raw egui shapes.
    /// 3. Overlays: Renders dynamic metadata (designators, physical parameters, live currents, heatmaps,
    ///    connection candidate dots, and routing previews) directly in the UI view overlay.
    /// 4. Placement and routing machine: Steers component insertion and orthogonal wire paths.
    ///
    /// Returns `true` if any mutating schematic action occurred during this pass.
    pub(super) fn draw_canvas(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        wgpu_render_state: Option<&egui_wgpu::RenderState>,
        render_runtime: &mut RenderRuntime,
    ) -> bool {
        let mut changed = false;
        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        self.canvas_dark_mode = ui.visuals().dark_mode;
        let palette = electronics_palette(self.canvas_dark_mode);
        let hover_mouse = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|mouse| rect.contains(*mouse));
        let hover_candidate =
            hover_mouse.map(|mouse| self.resolve_connection_candidate(mouse, rect));
        let hovered_component = hover_mouse.and_then(|mouse| self.hit_test_component(mouse, rect));
        let hovered_wire = hover_mouse.and_then(|mouse| self.hit_test_wire(mouse, rect));
        let render_w = rect.width().max(1.0).round() as u32;
        let render_h = rect.height().max(1.0).round() as u32;
        let gpu_frame = self.build_gpu_canvas_frame(render_w, render_h, hovered_wire);
        let render_output = render_runtime.render_scene_frame(&gpu_frame);
        self.render_runtime = render_runtime.snapshot();
        self.gpu_canvas.present(
            ui.ctx(),
            wgpu_render_state,
            render_output,
            render_w,
            render_h,
        );
        let gpu_backdrop_ready = self.gpu_canvas.is_ready();

        {
            if gpu_backdrop_ready {
                self.gpu_canvas.paint(&painter, rect);
            } else {
                painter.rect_filled(rect, 0.0, palette.canvas_bg);
            }
            painter.rect_stroke(rect.shrink(0.5), 0.0, Stroke::new(1.0, palette.border));

            if !gpu_backdrop_ready {
                self.draw_grid(&painter, rect);
            }

            let selected_wires = self.selected_wire_indices();
            let hovered_wires = hovered_wire
                .map(|idx| self.wire_group_indices(idx))
                .unwrap_or_default();

            for (idx, wire) in self.schematic.wires.iter().enumerate() {
                let start = self.world_to_screen(Pos2::new(wire.start.x, wire.start.y), rect);
                let end = self.world_to_screen(Pos2::new(wire.end.x, wire.end.y), rect);
                let is_selected = selected_wires.contains(&idx);
                let is_hovered = hovered_wires.contains(&idx);
                let is_related = hovered_component
                    .map(|component_idx| self.wire_touches_component(wire, component_idx))
                    .unwrap_or(false);
                let wire_color = if is_selected {
                    theme::ACCENT
                } else if is_hovered || is_related || matches!(self.placement, PlacementMode::Wire)
                {
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

                if !gpu_backdrop_ready {
                    painter.line_segment([start, end], Stroke::new(wire_width, wire_color));
                }

                if is_selected || is_hovered || matches!(self.placement, PlacementMode::Wire) {
                    let node_radius = (3.5 * self.zoom).max(3.0);
                    for point in [start, end] {
                        painter.circle_filled(point, node_radius, palette.node_bg);
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

            // Measurement tool overlay.
            if let Some(start) = self.measurement_start {
                let start_screen = self.world_to_screen(Pos2::new(start.x, start.y), rect);
                let end_world = self.measurement_end.or_else(|| {
                    hover_mouse.map(|m| {
                        let w = self.screen_to_world(m, rect);
                        Vec2::new(w.x, w.y)
                    })
                });
                if let Some(end) = end_world {
                    let end_screen = self.world_to_screen(Pos2::new(end.x, end.y), rect);
                    // Dashed line between the two points.
                    painter.line_segment(
                        [start_screen, end_screen],
                        Stroke::new(2.0, Color32::from_rgb(100, 200, 255)),
                    );
                    // Endpoints.
                    painter.circle_filled(start_screen, 4.0, Color32::from_rgb(100, 200, 255));
                    painter.circle_filled(end_screen, 4.0, Color32::from_rgb(100, 200, 255));
                    // Distance label.
                    let dist = (end - start).length();
                    let mid = Pos2::new(
                        (start_screen.x + end_screen.x) * 0.5,
                        (start_screen.y + end_screen.y) * 0.5 - 12.0,
                    );
                    let label = format!("{:.2} mm", dist);
                    painter.text(
                        mid,
                        egui::Align2::CENTER_CENTER,
                        &label,
                        egui::FontId::proportional(12.0),
                        Color32::from_rgb(100, 200, 255),
                    );
                }
            }

            for (idx, comp) in self.schematic.components.iter().enumerate() {
                let preview_selected = self.box_select_rect.map_or(false, |box_rect| {
                    let center = self.world_to_screen(
                        Pos2::new(comp.position.x, comp.position.y),
                        rect,
                    );
                    let body = self.component_body_rect(
                        center,
                        symbol_kind_for_component(comp),
                    );
                    box_rect.intersects(body)
                });
                let is_selected = self.is_component_selected(idx) || preview_selected;
                let is_hovered = hovered_component == Some(idx);
                let hovered_pin = hover_candidate.and_then(|candidate| {
                    if candidate.component_index == Some(idx) {
                        candidate.pin_index
                    } else {
                        None
                    }
                });
                self.draw_component(
                    &painter,
                    rect,
                    comp,
                    is_selected,
                    is_hovered,
                    hovered_pin,
                    !gpu_backdrop_ready,
                );
            }

            if let (PlacementMode::Wire, Some(start)) = (&self.placement, self.wire_start) {
                let end_candidate =
                    hover_candidate.unwrap_or_else(|| ConnectionCandidate::grid(start.world));
                let route = orthogonal_route_points(start.world, end_candidate.world);
                self.draw_wire_preview(&painter, rect, &route);
                self.draw_connection_candidate(&painter, rect, start, true);
                self.draw_connection_candidate(&painter, rect, end_candidate, true);
            } else if let Some(candidate) = hover_candidate {
                if matches!(
                    candidate.kind,
                    ConnectionKind::Pin
                        | ConnectionKind::WireEndpoint
                        | ConnectionKind::WireJunction
                ) {
                    self.draw_connection_candidate(&painter, rect, candidate, false);
                }
            }

            if let PlacementMode::Component(idx) = &self.placement {
                if let Some(mouse) = hover_mouse {
                    let world = self.snap_to_grid(self.screen_to_world(mouse, rect));
                    if let Some(template) = self.library.components.get(*idx) {
                        let mut preview = template.instantiate();
                        preview.position = glam::Vec2::new(world.x, world.y);
                        preview.rotation = self.placement_rotation();
                        self.draw_component(&painter, rect, &preview, true, true, None, true);

                        let screen = self.world_to_screen(world, rect);
                        let body =
                            self.component_body_rect(screen, symbol_kind_for_component(&preview));
                        painter.rect_filled(
                            body,
                            6.0 * self.zoom,
                            Color32::from_rgba_premultiplied(212, 119, 26, 36),
                        );
                        painter.rect_stroke(
                            body,
                            6.0 * self.zoom,
                            Stroke::new(1.2, Color32::from_rgba_premultiplied(212, 119, 26, 120)),
                        );
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

            if let PlacementMode::Clipboard(components) = &self.placement {
                if let Some(mouse) = hover_mouse {
                    let world = self.snap_to_grid(self.screen_to_world(mouse, rect));
                    if let Some(origin) = components.first().map(|component| component.position) {
                        for component in components {
                            let mut preview = component.clone();
                            preview.position =
                                glam::Vec2::new(world.x, world.y) + (component.position - origin);
                            self.draw_component(&painter, rect, &preview, true, true, None, true);
                        }
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

            if let Some(candidate) = hover_candidate {
                self.draw_hover_hint(&painter, rect, candidate);
            }

            let help_copy = if matches!(self.placement, PlacementMode::Wire) {
                t("app.schematic_canvas_hint_wire", self.lang)
            } else {
                t("app.schematic_canvas_hint", self.lang)
            };
            // Show cursor position in mm (schematic unit = 1mm).
            let cursor_mm = hover_mouse.map(|m| {
                let w = self.screen_to_world(m, rect);
                format!("Cursor: {:.1}, {:.1} mm", w.x, w.y)
            }).unwrap_or_default();
            let info_text = format!("Zoom: {:.1}x | {} | {}", self.zoom, cursor_mm, help_copy);
            painter.text(
                Pos2::new(rect.left() + 10.0, rect.top() + 10.0),
                egui::Align2::LEFT_TOP,
                info_text,
                egui::FontId::proportional(10.0),
                palette.text_muted,
            );
        }

        if !self.show_export_menu && matches!(self.placement, PlacementMode::None) {
            if response.drag_started_by(egui::PointerButton::Primary) {
                if let Some(mouse) = hover_mouse {
                    if let Some(idx) = self.hit_test_component(mouse, rect) {
                        let shift = ui.input(|i| i.modifiers.shift);
                        let ctrl = ui.input(|i| i.modifiers.ctrl);
                        if !self.is_component_selected(idx) || shift || ctrl {
                            self.select_component_with_modifiers(idx, shift, ctrl);
                        }

                        if self
                            .schematic
                            .components
                            .get(idx)
                            .map(|component| component.locked)
                            .unwrap_or(false)
                        {
                            self.drag_state = None;
                            return changed;
                        }

                        let drag_indices = if self.is_component_selected(idx) {
                            self.selected_component_indices()
                        } else {
                            vec![idx]
                        };

                        let mut drag_items = Vec::new();
                        for component_idx in drag_indices {
                            if self
                                .schematic
                                .components
                                .get(component_idx)
                                .map(|component| component.locked)
                                .unwrap_or(false)
                            {
                                continue;
                            }
                            self.anchor_wires_near_component(component_idx);
                            if let Some(component) = self.schematic.components.get(component_idx) {
                                let comp_screen = self.world_to_screen(
                                    Pos2::new(component.position.x, component.position.y),
                                    rect,
                                );
                                let offset =
                                    Vec2::new(mouse.x - comp_screen.x, mouse.y - comp_screen.y);
                                drag_items.push((component_idx, offset));
                            }
                        }

                        if !drag_items.is_empty() {
                            self.drag_state = Some(drag_items);
                        }
                    } else {
                        self.box_select_start = Some(mouse);
                        self.box_select_rect = Some(Rect::from_two_pos(mouse, mouse));
                    }
                }
            }

            if response.dragged_by(egui::PointerButton::Primary) {
                if let Some(drag_items) = self.drag_state.clone() {
                    if let Some(mouse) = hover_mouse {
                        for (idx, offset) in drag_items {
                            let target_screen = Pos2::new(mouse.x - offset.x, mouse.y - offset.y);
                            let world =
                                self.snap_to_grid(self.screen_to_world(target_screen, rect));
                            if idx < self.schematic.components.len() {
                                let new_pos = glam::Vec2::new(world.x, world.y);
                                if self.schematic.components[idx].position != new_pos {
                                    self.schematic.components[idx].position = new_pos;
                                    changed = true;
                                }
                            }
                        }
                        if changed {
                            self.schematic.sync_wire_anchors();
                        }
                    }
                } else if let (Some(start), Some(mouse)) = (self.box_select_start, hover_mouse) {
                    self.box_select_rect = Some(Rect::from_two_pos(start, mouse));
                }
            }

            if let Some(box_rect) = self.box_select_rect {
                painter.rect_filled(
                    box_rect,
                    2.0,
                    Color32::from_rgba_premultiplied(212, 119, 26, 28),
                );
                painter.rect_stroke(
                    box_rect,
                    2.0,
                    Stroke::new(1.0, Color32::from_rgba_premultiplied(212, 119, 26, 150)),
                );
            }

            if response.drag_stopped_by(egui::PointerButton::Primary) {
                if let Some(box_rect) = self.box_select_rect.take() {
                    if self.drag_state.is_none()
                        && box_rect.width().abs() > 4.0
                        && box_rect.height().abs() > 4.0
                    {
                        let mut selected = Vec::new();
                        for (idx, component) in self.schematic.components.iter().enumerate() {
                            let center = self.world_to_screen(
                                Pos2::new(component.position.x, component.position.y),
                                rect,
                            );
                            let body = self
                                .component_body_rect(center, symbol_kind_for_component(component));
                            if box_rect.intersects(body) {
                                selected.push(idx);
                            }
                        }
                        let shift = ui.input(|i| i.modifiers.shift);
                        let ctrl = ui.input(|i| i.modifiers.ctrl);
                        self.select_components_with_modifiers(selected, shift, ctrl);
                    }
                }
                self.drag_state = None;
                self.box_select_start = None;
            }
        }

        if !self.show_export_menu && response.clicked() && self.drag_state.is_none() {
            self.context_menu = None;

            // Esc clears measurement.
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.measurement_start = None;
                self.measurement_end = None;
            }

            // Measurement tool: when measurement_start is set, the next click
            // captures the end point. M key activates the first point.
            if self.measurement_start.is_some() && self.measurement_end.is_none() {
                if let Some(mouse) = hover_mouse {
                    let world = self.snap_to_grid(self.screen_to_world(mouse, rect));
                    self.measurement_end = Some(Vec2::new(world.x, world.y));
                    return changed;
                }
            }

            if let Some(mouse) = hover_mouse {
                let candidate = self.resolve_connection_candidate(mouse, rect);

                match &self.placement {
                    PlacementMode::Component(idx) => {
                        if let Some(template) = self.library.components.get(*idx) {
                            let mut comp = template.instantiate();
                            comp.position = glam::Vec2::new(candidate.world.x, candidate.world.y);
                            comp.rotation = self.placement_rotation();
                            self.schematic.add_component(comp);
                            let last = self.schematic.components.len().saturating_sub(1);
                            self.selection = SchematicSelection::Component(last);
                            changed = true;
                        }
                    }
                    PlacementMode::Clipboard(components) => {
                        let components = components.clone();
                        let origin = components
                            .first()
                            .map(|component| component.position)
                            .unwrap_or(glam::Vec2::ZERO);
                        let mut new_indices = Vec::new();
                        for component in components {
                            let mut comp = clone_component_for_paste(&component);
                            comp.position = glam::Vec2::new(candidate.world.x, candidate.world.y)
                                + (component.position - origin);
                            self.schematic.add_component(comp);
                            new_indices.push(self.schematic.components.len().saturating_sub(1));
                        }
                        self.select_components_with_modifiers(new_indices, false, false);
                        self.placement = PlacementMode::None;
                        changed = true;
                    }
                    PlacementMode::Wire => {
                        if let Some(start) = self.wire_start {
                            let (start_world, start_changed) =
                                self.prepare_connection_for_commit(start);
                            let (end_world, end_changed) =
                                self.prepare_connection_for_commit(candidate);
                            changed |= start_changed || end_changed;

                            if start_world.distance(end_world) > 0.01 {
                                let route = orthogonal_route_points(start_world, end_world);
                                let route_points: Vec<glam::Vec2> = route
                                    .iter()
                                    .map(|point| glam::Vec2::new(point.x, point.y))
                                    .collect();
                                changed |= self.schematic.add_wire_path_anchored(
                                    &route_points,
                                    "",
                                    wire_anchor_for_candidate(&self.schematic, start),
                                    wire_anchor_for_candidate(&self.schematic, candidate),
                                ) > 0;
                            }

                            let finish_wire = response
                                .double_clicked_by(egui::PointerButton::Primary)
                                || !matches!(candidate.kind, ConnectionKind::Grid);

                            if finish_wire {
                                self.wire_start = None;
                                self.placement = PlacementMode::None;
                            } else {
                                self.wire_start = Some(candidate.chain_anchor(end_world));
                            }
                        } else {
                            if response.double_clicked_by(egui::PointerButton::Primary) {
                                self.wire_start = None;
                                self.placement = PlacementMode::None;
                            } else {
                                self.wire_start = Some(candidate);
                            }
                        }
                    }
                    PlacementMode::None => {
                        if response.double_clicked_by(egui::PointerButton::Primary) {
                            if let Some(idx) = self.hit_test_component(mouse, rect) {
                                if idx < self.schematic.components.len() {
                                    self.editing_value =
                                        Some((idx, self.schematic.components[idx].value.clone()));
                                        self.value_editor_just_opened = true;
                                    self.selection = SchematicSelection::Component(idx);
                                }
                            } else if let Some(idx) = self.hit_test_wire(mouse, rect) {
                                let world = self.snap_to_grid(self.screen_to_world(mouse, rect));
                                changed |= self.split_wire_at_world(world, Some(idx));
                                self.selection = SchematicSelection::Wire(
                                    idx.min(self.schematic.wires.len().saturating_sub(1)),
                                );
                            }
                        } else if let Some((_idx, _, _)) = self.hit_test_pin(mouse, rect) {
                            self.placement = PlacementMode::Wire;
                            self.wire_start = Some(candidate);
                        } else if let Some(idx) = self.hit_test_component(mouse, rect) {
                            let shift = ui.input(|i| i.modifiers.shift);
                            let ctrl = ui.input(|i| i.modifiers.ctrl);
                            self.select_component_with_modifiers(idx, shift, ctrl);
                        } else if let Some(idx) = self.hit_test_wire(mouse, rect) {
                            self.selection = SchematicSelection::Wire(idx);
                        } else {
                            self.selection = SchematicSelection::None;
                        }
                    }
                }
            }
        }

        // M key activates measurement mode: captures the first point at the
        // current mouse position.
        if ui.input(|i| i.key_pressed(egui::Key::M)) && !self.show_export_menu {
            if let Some(mouse) = hover_mouse {
                let world = self.snap_to_grid(self.screen_to_world(mouse, rect));
                self.measurement_start = Some(Vec2::new(world.x, world.y));
                self.measurement_end = None;
            }
        }

        if response.clicked_by(egui::PointerButton::Secondary) {
            if !matches!(self.placement, PlacementMode::None) {
                self.wire_start = None;
                self.placement = PlacementMode::None;
                self.context_menu = None;
                return changed;
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

        if !self.show_export_menu && response.dragged_by(egui::PointerButton::Middle) {
            self.offset += response.drag_delta();
        }

        if !self.show_export_menu
            && self.context_menu.is_none()
            && response.dragged_by(egui::PointerButton::Secondary)
        {
            self.offset += response.drag_delta();
        }

        if !self.show_export_menu && ui.rect_contains_pointer(rect) {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                if let Some(mouse) = hover_mouse {
                    let old_zoom = self.zoom;
                    self.zoom = (self.zoom + scroll * 0.002).clamp(0.3, 4.0);
                    let factor = self.zoom / old_zoom;
                    self.offset.x =
                        mouse.x - rect.left() - (mouse.x - rect.left() - self.offset.x) * factor;
                    self.offset.y =
                        mouse.y - rect.top() - (mouse.y - rect.top() - self.offset.y) * factor;
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
            } else if !matches!(self.placement, PlacementMode::None) {
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
                self.export_netlist(ui.ctx());
            }
            if key_2 {
                self.export_bom_csv(ui.ctx());
            }
            if key_3 {
                self.export_svg(ui.ctx());
            }

            self.draw_export_menu(ui, rect);
        }

        if ui.input(|i| i.key_pressed(egui::Key::Delete)) {
            changed |= self.delete_selection();
        }

        if ui.input(|i| i.key_pressed(egui::Key::R)) {
            if matches!(self.placement, PlacementMode::Component(_)) {
                self.rotate_placement_preview();
                changed = true;
            } else if let Some(idx) = self.selected_component_index() {
                if let Some(snapshot) = self.schematic.components.get(idx).cloned() {
                    self.ensure_wire_anchors_for_component_snapshot(&snapshot);
                }
                let comp = &mut self.schematic.components[idx];
                comp.rotation = (comp.rotation + 90.0) % 360.0;
                self.schematic.sync_wire_anchors();
                changed = true;
            }
        }

        if ui.input(|i| i.key_pressed(egui::Key::M)) {
            if let Some(idx) = self.selected_component_index() {
                if let Some(snapshot) = self.schematic.components.get(idx).cloned() {
                    self.ensure_wire_anchors_for_component_snapshot(&snapshot);
                }
                let comp = &mut self.schematic.components[idx];
                for pin in &mut comp.pins {
                    pin.offset.x = -pin.offset.x;
                }
                self.schematic.sync_wire_anchors();
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

        if ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::C)) {
            self.copy_selected_components();
        }

        if ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::V)) {
            let _ = self.start_clipboard_preview();
        }

        if ui.input(|i| i.key_pressed(egui::Key::Tab)) {
            self.quick_search_open = true;
            self.quick_search_query.clear();
            self.quick_search_selected = 0;
        }

        changed
    }

    /// Generates a GPU-backed rendering frame for high-performance backdrop presentation.
    ///
    /// Constructs a `SceneRenderFrame` by recording draw commands for:
    /// - The clearing color (background palette).
    /// - The orthogonal/coordinate grid via `record_gpu_grid`.
    /// - Static wires (including highlighted states).
    /// - Electronic component symbol shapes (lines, open/filled circles) using instanced recipes.
    fn build_gpu_canvas_frame(
        &self,
        width: u32,
        height: u32,
        hovered_wire: Option<usize>,
    ) -> SceneRenderFrame {
        let mut commands = BasicCommandList::new();
        let palette = electronics_palette(self.canvas_dark_mode);
        commands.clear(color32_to_rgba8(palette.canvas_bg));

        let (left, right, top, bottom) = self.visible_world_bounds(width as f32, height as f32);
        self.record_gpu_grid(&mut commands, left, right, top, bottom);

        let selected_wires = self.selected_wire_indices();

        for (idx, wire) in self.schematic.wires.iter().enumerate() {
            let color = if selected_wires.contains(&idx) {
                color32_to_rgba8(theme::ACCENT)
            } else if hovered_wire == Some(idx) || matches!(self.placement, PlacementMode::Wire) {
                [118, 226, 134, 255]
            } else {
                [82, 200, 100, 255]
            };

            commands.draw_line(
                Vec3::new(wire.start.x, wire.start.y, 0.0),
                Vec3::new(wire.end.x, wire.end.y, 0.0),
                color,
                1.0,
                true,
                0.0,
            );
        }

        for (idx, comp) in self.schematic.components.iter().enumerate() {
            let color = if self.is_component_selected(idx) {
                color32_to_rgba8(theme::ACCENT)
            } else {
                [228, 232, 240, 255]
            };
            self.record_component_symbol_geometry(&mut commands, comp, color);
        }

        SceneRenderFrame {
            commands,
            view_proj: canvas_view_projection(left, right, top, bottom),
            light_dir: Vec3::Z,
            width,
            height,
            stats: FrameStats::default(),
        }
    }

    /// Computes the visible coordinate boundaries in world space.
    ///
    /// Maps screen viewport dimensions to schematic coordinates based on the current camera
    /// offset and magnification zoom factor.
    /// Returns `(left, right, top, bottom)` bounds.
    fn visible_world_bounds(&self, width: f32, height: f32) -> (f32, f32, f32, f32) {
        let zoom = self.zoom.max(0.001);
        let left = (-self.offset.x) / zoom;
        let right = (width - self.offset.x) / zoom;
        let top = (-self.offset.y) / zoom;
        let bottom = (height - self.offset.y) / zoom;
        (left, right, top, bottom)
    }

    /// Records line primitives representing the background grid into the provided command list.
    ///
    /// Generates subdividing coordinates based on the current zoom level and grid step,
    /// drawing primary axes, major subdivision lines (every 5 steps), and minor lines with highly
    /// optimized opacity-transparent strokes depending on the canvas color scheme (dark vs light).
    fn record_gpu_grid(
        &self,
        commands: &mut BasicCommandList,
        left: f32,
        right: f32,
        top: f32,
        bottom: f32,
    ) {
        let step = GRID_STEP * self.zoom;
        if step < 4.0 {
            return;
        }

        let (minor, major, axis) = if self.canvas_dark_mode {
            ([180, 186, 200, 14], [220, 226, 240, 30], [212, 119, 26, 46])
        } else {
            ([70, 80, 96, 22], [70, 80, 96, 42], [212, 119, 26, 74])
        };
        let world_left = (left / GRID_STEP).floor() as i32 - 2;
        let world_right = (right / GRID_STEP).ceil() as i32 + 2;
        let world_top = (top / GRID_STEP).floor() as i32 - 2;
        let world_bottom = (bottom / GRID_STEP).ceil() as i32 + 2;

        for ix in world_left..=world_right {
            let x = ix as f32 * GRID_STEP;
            let color = if ix == 0 {
                axis
            } else if ix % 5 == 0 {
                major
            } else {
                minor
            };

            commands.draw_line(
                Vec3::new(x, top, 0.0),
                Vec3::new(x, bottom, 0.0),
                color,
                1.0,
                true,
                0.0,
            );
        }

        for iy in world_top..=world_bottom {
            let y = iy as f32 * GRID_STEP;
            let color = if iy == 0 {
                axis
            } else if iy % 5 == 0 {
                major
            } else {
                minor
            };

            commands.draw_line(
                Vec3::new(left, y, 0.0),
                Vec3::new(right, y, 0.0),
                color,
                1.0,
                true,
                0.0,
            );
        }
    }

    /// Adds vector geometric components for a given electronic symbol into the draw command buffer.
    ///
    /// Translates, rotates, and renders localized segment lines and concentric circle elements
    /// extracted from the layout schema template.
    fn record_component_symbol_geometry(
        &self,
        commands: &mut BasicCommandList,
        comp: &ElectronicComponent,
        color: [u8; 4],
    ) {
        let center = Pos2::new(comp.position.x, comp.position.y);
        let recipe = schematic_symbol_recipe(symbol_kind_for_component(comp));

        for segment in recipe.segments {
            let start = transform_local_world_point(center, segment[0], comp.rotation);
            let end = transform_local_world_point(center, segment[1], comp.rotation);
            commands.draw_line(
                Vec3::new(start.x, start.y, 0.0),
                Vec3::new(end.x, end.y, 0.0),
                color,
                1.0,
                true,
                0.0,
            );
        }

        for circle in recipe.open_circles {
            let center = transform_local_world_point(center, circle.center, comp.rotation);
            record_circle_outline(commands, center, circle.radius, color);
        }

        for circle in recipe.filled_circles {
            let center = transform_local_world_point(center, circle.center, comp.rotation);
            record_circle_outline(commands, center, circle.radius, color);
        }
    }

    /// Renders the floating popup context menu at the mouse cursor position.
    ///
    /// Depending on the `SchematicSelection` click target (Component, MultipleComponents, Wire, or None),
    /// displays actions such as:
    /// - Changing/editing values.
    /// - Rotating (`R` key) or Mirroring (`M` key) components.
    /// - Duplicating (`Ctrl+D` key).
    /// - Locking/unlocking components.
    /// - Deleting selections (`Delete` key).
    /// - Placing components and testing electrical rules.
    ///
    /// Returns `true` if any interactive action altered the schematics.
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
                            if ui.button(t("app.edit_value", self.lang)).clicked() {
                                if idx < self.schematic.components.len() {
                                    let value = self.schematic.components[idx].value.clone();
                                    self.editing_value = Some((idx, value));
                                    self.value_editor_just_opened = true;
                                }
                                close_menu = true;
                            }

                            if ui.button(t("app.rotate_r", self.lang)).clicked() {
                                if idx < self.schematic.components.len() {
                                    let snapshot = self.schematic.components[idx].clone();
                                    self.ensure_wire_anchors_for_component_snapshot(&snapshot);
                                    self.schematic.components[idx].rotation =
                                        (self.schematic.components[idx].rotation + 90.0) % 360.0;
                                    self.schematic.sync_wire_anchors();
                                    changed = true;
                                }
                                close_menu = true;
                            }

                            if ui.button(t("app.duplicate_ctrl_d", self.lang)).clicked() {
                                self.selection = SchematicSelection::Component(idx);
                                changed |= self.duplicate_selection();
                                close_menu = true;
                            }

                            if idx < self.schematic.components.len() {
                                let locked = self.schematic.components[idx].locked;
                                if ui
                                    .button(if locked {
                                        t("app.electronics_unlock", self.lang)
                                    } else {
                                        t("app.electronics_lock", self.lang)
                                    })
                                    .clicked()
                                {
                                    self.schematic.components[idx].locked = !locked;
                                    changed = true;
                                    close_menu = true;
                                }

                                let has_datasheet = self.schematic.components[idx]
                                    .datasheet
                                    .as_ref()
                                    .map(|value| !value.trim().is_empty())
                                    .unwrap_or(false);
                                ui.add_enabled(
                                    has_datasheet,
                                    egui::Button::new(t("app.electronics_datasheet", self.lang)),
                                );
                            }

                            ui.separator();

                            if ui
                                .button(
                                    egui::RichText::new(t("app.delete_del", self.lang))
                                        .color(theme::STATUS_ERROR),
                                )
                                .clicked()
                            {
                                self.selection = SchematicSelection::Component(idx);
                                changed |= self.delete_selection();
                                close_menu = true;
                            }
                        }
                        SchematicSelection::MultipleComponents(indices) => {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{}: {}",
                                    t("app.electronics_selection", self.lang),
                                    indices.len()
                                ))
                                .size(11.0)
                                .color(Color32::from_rgb(160, 166, 178)),
                            );

                            if ui.button(t("app.duplicate_ctrl_d", self.lang)).clicked() {
                                changed |= self.duplicate_selection();
                                close_menu = true;
                            }

                            if ui
                                .button(
                                    egui::RichText::new(t("app.delete_del", self.lang))
                                        .color(theme::STATUS_ERROR),
                                )
                                .clicked()
                            {
                                changed |= self.delete_selection();
                                close_menu = true;
                            }
                        }
                        SchematicSelection::Wire(idx) => {
                            if ui
                                .button(t("app.electronics_rename_net", self.lang))
                                .clicked()
                            {
                                let current_name = self
                                    .schematic
                                    .wires
                                    .get(idx)
                                    .map(|w| w.net.clone())
                                    .unwrap_or_default();
                                self.editing_net_name = Some((idx, current_name));
                                self.selection = SchematicSelection::Wire(idx);
                                close_menu = true;
                            }

                            if ui
                                .button(
                                    egui::RichText::new(t("app.delete_wire_del", self.lang))
                                        .color(theme::STATUS_ERROR),
                                )
                                .clicked()
                            {
                                self.selection = SchematicSelection::Wire(idx);
                                changed |= self.delete_selection();
                                close_menu = true;
                            }
                        }
                        SchematicSelection::None => {
                            if ui
                                .button(t("app.electronics_place_component", self.lang))
                                .clicked()
                            {
                                self.quick_search_open = true;
                                self.quick_search_query.clear();
                                self.quick_search_selected = 0;
                                close_menu = true;
                            }

                            if ui
                                .add_enabled(
                                    !self.clipboard_components.is_empty(),
                                    egui::Button::new(t("app.electronics_paste", self.lang)),
                                )
                                .clicked()
                            {
                                let _ = self.start_clipboard_preview();
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

    /// Renders an in-line modal dialog window to edit the physical electrical value of a component.
    ///
    /// Synchronizes input changes with the component structure on verification (via the `Enter` key or clicking `OK`).
    /// Also updates the internal SPICE simulation parameters for DC nodes/pins to match the new physical value.
    ///
    /// Returns `true` if any component parameter changes occurred.
    pub(super) fn draw_value_editor(&mut self, ui: &mut egui::Ui) -> bool {
        let (idx, mut buffer) = match self.editing_value.take() {
            Some(value) => value,
            None => return false,
        };

        let mut keep_open = true;
        let mut changed = false;
        let mut window_rect = None;

        if let Some(window) = egui::Window::new(t("app.edit_value", self.lang))
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
            })
        {
            window_rect = Some(window.response.rect);
        }

        if self.selected_component_index() != Some(idx) {
            keep_open = false;
        }

        if !self.value_editor_just_opened {
            let clicked_outside = ui.ctx().input(|i| {
                i.pointer.primary_clicked() || i.pointer.secondary_clicked()
            });
            if clicked_outside {
                if let (Some(rect), Some(pointer)) =
                    (window_rect, ui.ctx().input(|i| i.pointer.interact_pos()))
                {
                    if !rect.contains(pointer) {
                        keep_open = false;
                    }
                }
            }
        }

        self.value_editor_just_opened = false;

        if keep_open {
            self.editing_value = Some((idx, buffer));
        }

        changed
    }

    /// Renders a floating popup to edit the net name of a wire.
    /// On confirm, assigns the name to all wires in the same net group.
    pub(super) fn draw_net_name_editor(&mut self, ui: &mut egui::Ui) -> bool {
        let (wire_idx, mut buffer) = match self.editing_net_name.take() {
            Some(data) => data,
            None => return false,
        };

        let mut changed = false;
        let mut keep_open = false;
        let lang = self.lang;

        egui::Window::new(t("app.electronics_rename_net", lang))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .fixed_size(egui::vec2(280.0, 120.0))
            .show(ui.ctx(), |ui| {
                ui.label(t("app.electronics_net_name_prompt", lang));
                ui.add_space(6.0);
                let response = ui.add_sized(
                    [ui.available_width(), 24.0],
                    egui::TextEdit::singleline(&mut buffer)
                        .hint_text("VCC / GND / N001"),
                );
                response.request_focus();
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(t("app.save_and_close", lang)).clicked() {
                        changed = true;
                    }
                    if ui.button(t("app.cancel", lang)).clicked() {
                        keep_open = false;
                        return;
                    }
                });
                if response.lost_focus() {
                    if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        changed = true;
                    } else if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        keep_open = false;
                        return;
                    }
                }
                keep_open = !changed;
            });

        if changed && wire_idx < self.schematic.wires.len() {
            let trimmed = buffer.trim().to_string();
            // Assign the name to all wires in the same group.
            let group_indices = self.wire_group_indices(wire_idx);
            for &idx in &group_indices {
                if idx < self.schematic.wires.len() {
                    self.schematic.wires[idx].net = trimmed.clone();
                }
            }
        }

        if keep_open {
            self.editing_net_name = Some((wire_idx, buffer));
        }

        changed
    }

    /// Renders a floating search modal allowing users to quickly query and instantiate components from the asset library.
    ///
    /// Pressing `Tab` flags this popup of components, with full support for:
    /// - Text matching across category names, component descriptors, and tags/keywords.
    /// - Navigational arrows up/down to inspect matches.
    /// - Placement activation using `Enter` or clicking on the specific result list.
    ///
    /// Returns `true` if a component was selected for instantiation.
    pub(super) fn draw_quick_search(&mut self, ui: &mut egui::Ui) -> bool {
        if !self.quick_search_open {
            return false;
        }

        let palette = electronics_palette(ui.visuals().dark_mode);
        let mut changed = false;
        let mut close = false;
        let query = self.quick_search_query.trim().to_lowercase();
        let matches = self
            .library
            .components
            .iter()
            .enumerate()
            .filter(|(_, template)| {
                query.is_empty()
                    || template.name.to_lowercase().contains(&query)
                    || template.category.to_lowercase().contains(&query)
                    || template
                        .keywords
                        .iter()
                        .any(|keyword| keyword.to_lowercase().contains(&query))
            })
            .map(|(idx, _)| idx)
            .collect::<Vec<_>>();

        if !matches.is_empty() {
            self.quick_search_selected = self
                .quick_search_selected
                .min(matches.len().saturating_sub(1));
        } else {
            self.quick_search_selected = 0;
        }

        if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) && !matches.is_empty() {
            self.quick_search_selected = (self.quick_search_selected + 1).min(matches.len() - 1);
        }
        if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) && !matches.is_empty() {
            self.quick_search_selected = self.quick_search_selected.saturating_sub(1);
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            if let Some(component_index) = matches.get(self.quick_search_selected).copied() {
                self.placement = PlacementMode::Component(component_index);
                self.placement_rotation = 0.0;
                self.wire_start = None;
                close = true;
                changed = true;
            }
        }

        egui::Window::new(t("app.electronics_search_components_title", self.lang))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 92.0])
            .fixed_size(egui::vec2(360.0, 260.0))
            .show(ui.ctx(), |ui| {
                ui.add_sized(
                    [ui.available_width(), 30.0],
                    egui::TextEdit::singleline(&mut self.quick_search_query)
                        .hint_text(t("app.electronics_search_components", self.lang)),
                );
                ui.add_space(8.0);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (result_index, component_index) in
                        matches.iter().copied().take(8).enumerate()
                    {
                        let template = &self.library.components[component_index];
                        let selected = result_index == self.quick_search_selected;
                        let response = ui.add(
                            egui::Button::new(
                                egui::RichText::new(format!(
                                    "{}  {}",
                                    template.name, template.description
                                ))
                                .size(12.0)
                                .color(if selected {
                                    Color32::WHITE
                                } else {
                                    palette.text
                                }),
                            )
                            .fill(if selected {
                                Color32::from_rgb(92, 52, 18)
                            } else {
                                palette.card_bg
                            })
                            .stroke(Stroke::new(
                                1.0,
                                if selected {
                                    theme::ACCENT
                                } else {
                                    palette.border
                                },
                            ))
                            .min_size(egui::vec2(ui.available_width(), 32.0)),
                        );
                        if response.clicked() {
                            self.quick_search_selected = result_index;
                            self.placement = PlacementMode::Component(component_index);
                            self.placement_rotation = 0.0;
                            self.wire_start = None;
                            close = true;
                            changed = true;
                        }
                    }
                });
            });

        if close {
            self.quick_search_open = false;
        }

        changed
    }

    /// Draws the background layout grid on the canvas using egui painter as a fallback.
    ///
    /// Configures stroke weights and colors for axis grids (center coordinates), major grid divisions
    /// (every 5 steps), and minor sub-grids depending on the current zoom level and workspace dark/light modes.
    fn draw_grid(&self, painter: &egui::Painter, rect: Rect) {
        let (minor, major, axis) = if self.canvas_dark_mode {
            (
                Color32::from_rgba_premultiplied(180, 186, 200, 14),
                Color32::from_rgba_premultiplied(220, 226, 240, 30),
                Color32::from_rgba_premultiplied(212, 119, 26, 46),
            )
        } else {
            (
                Color32::from_rgba_premultiplied(70, 80, 96, 22),
                Color32::from_rgba_premultiplied(70, 80, 96, 42),
                Color32::from_rgba_premultiplied(212, 119, 26, 74),
            )
        };
        let step = GRID_STEP * self.zoom;
        if step < 4.0 {
            return;
        }

        let world_left = ((rect.left() - rect.left() - self.offset.x) / self.zoom / GRID_STEP)
            .floor() as i32
            - 2;
        let world_right = ((rect.right() - rect.left() - self.offset.x) / self.zoom / GRID_STEP)
            .ceil() as i32
            + 2;
        let world_top =
            ((rect.top() - rect.top() - self.offset.y) / self.zoom / GRID_STEP).floor() as i32 - 2;
        let world_bottom = ((rect.bottom() - rect.top() - self.offset.y) / self.zoom / GRID_STEP)
            .ceil() as i32
            + 2;

        for ix in world_left..=world_right {
            let x_world = ix as f32 * GRID_STEP;
            let x = rect.left() + x_world * self.zoom + self.offset.x;
            let color = if ix == 0 {
                axis
            } else if ix % 5 == 0 {
                major
            } else {
                minor
            };
            let width = if ix == 0 { 1.2 } else { 0.5 };
            painter.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(width, color),
            );
        }

        for iy in world_top..=world_bottom {
            let y_world = iy as f32 * GRID_STEP;
            let y = rect.top() + y_world * self.zoom + self.offset.y;
            let color = if iy == 0 {
                axis
            } else if iy % 5 == 0 {
                major
            } else {
                minor
            };
            let width = if iy == 0 { 1.2 } else { 0.5 };
            painter.line_segment(
                [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                Stroke::new(width, color),
            );
        }
    }

    /// Renders a single electronic component entity onto the canvas viewport.
    ///
    /// This procedure handles:
    /// - Drawing background body boxes with focus/hover highlight states.
    /// - Rendering symbolic graphics representation (falling back to vector lines if atlas texture is not ready or if rotated).
    /// - Rendering designators, schematic names, physical value strings, and target footprints.
    /// - Drawing terminal connection Pins with color identification (Input vs Output, Ground, or Power) and localized tags.
    fn draw_component(
        &self,
        painter: &egui::Painter,
        canvas_rect: Rect,
        comp: &ElectronicComponent,
        is_selected: bool,
        is_hovered: bool,
        hovered_pin: Option<usize>,
        draw_symbol_geometry: bool,
    ) {
        let palette = electronics_palette(self.canvas_dark_mode);
        let center = self.world_to_screen(Pos2::new(comp.position.x, comp.position.y), canvas_rect);
        let symbol_kind = symbol_kind_for_component(comp);
        let recipe = schematic_symbol_recipe(symbol_kind);
        let body = self.component_body_rect(center, symbol_kind);

        let fill = if is_selected {
            palette.card_active
        } else if is_hovered {
            palette.card_hover
        } else {
            palette.card_bg
        };
        let border = if is_selected {
            theme::ACCENT
        } else if is_hovered {
            palette.text_muted
        } else {
            palette.border
        };

        if draw_symbol_geometry {
            painter.rect_filled(body, 6.0 * self.zoom, fill);
            painter.rect_stroke(body, 6.0 * self.zoom, Stroke::new(1.0, border));
        } else if is_selected || is_hovered {
            painter.rect_stroke(body, 6.0 * self.zoom, Stroke::new(1.0, border));
        }

        let painted_asset = if comp.rotation.abs() < 0.1 || (comp.rotation % 360.0).abs() < 0.1 {
            self.asset_atlas.paint(
                painter,
                symbol_asset_for_component(comp),
                body.shrink(2.0 * self.zoom),
                Color32::WHITE,
            )
        } else {
            false
        };

        if draw_symbol_geometry && !painted_asset {
            for segment in recipe.segments {
                let a = transform_local_point(center, segment[0], comp.rotation, self.zoom);
                let b = transform_local_point(center, segment[1], comp.rotation, self.zoom);
                painter.line_segment(
                    [a, b],
                    Stroke::new((1.6 * self.zoom).max(1.2), palette.text),
                );
            }

            for circle in recipe.open_circles {
                let screen = transform_local_point(center, circle.center, comp.rotation, self.zoom);
                painter.circle_stroke(
                    screen,
                    circle.radius * self.zoom,
                    Stroke::new((1.2 * self.zoom).max(1.0), palette.text),
                );
            }

            for circle in recipe.filled_circles {
                let screen = transform_local_point(center, circle.center, comp.rotation, self.zoom);
                painter.circle_filled(screen, circle.radius * self.zoom, palette.text);
            }
        }

        if symbol_kind == SchematicSymbolKind::Magnet {
            let left = transform_local_point(center, [-7.0, 0.0], comp.rotation, self.zoom);
            let right = transform_local_point(center, [7.0, 0.0], comp.rotation, self.zoom);
            painter.text(
                left,
                egui::Align2::CENTER_CENTER,
                "N",
                egui::FontId::proportional((9.0 * self.zoom).max(9.0)),
                theme::ACCENT,
            );
            painter.text(
                right,
                egui::Align2::CENTER_CENTER,
                "S",
                egui::FontId::proportional((9.0 * self.zoom).max(9.0)),
                Color32::from_rgb(170, 198, 236),
            );
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
            palette.text_dim,
        );

        if !comp.footprint.trim().is_empty() {
            painter.text(
                Pos2::new(body.right(), body.bottom() + 17.0 * self.zoom),
                egui::Align2::RIGHT_TOP,
                &comp.footprint,
                egui::FontId::proportional((8.0 * self.zoom).max(8.0)),
                palette.text_muted,
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

            painter.circle_filled(pin_screen, pin_radius, palette.node_bg);
            painter.circle_stroke(
                pin_screen,
                pin_radius,
                Stroke::new((1.5 * self.zoom).max(1.2), pin_color),
            );

            let label_offset = if pin.offset.x < 0.0 {
                -9.0 * self.zoom
            } else {
                9.0 * self.zoom
            };
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
                if hovered_pin == Some(pin_idx) {
                    palette.text
                } else {
                    palette.text_muted
                },
            );
        }
    }

    /// Renders a dynamic line path overlay displaying a real-time routing preview of connections.
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

    /// Renders a highlighted snap circle where a wire endpoint or component terminal is going to anchor.
    fn draw_connection_candidate(
        &self,
        painter: &egui::Painter,
        canvas_rect: Rect,
        candidate: ConnectionCandidate,
        strong: bool,
    ) {
        let palette = electronics_palette(self.canvas_dark_mode);
        let center = self.world_to_screen(candidate.world, canvas_rect);
        let color = match candidate.kind {
            ConnectionKind::Pin => Color32::from_rgb(110, 210, 255),
            ConnectionKind::WireEndpoint => Color32::from_rgb(118, 226, 134),
            ConnectionKind::WireJunction => theme::ACCENT,
            ConnectionKind::Grid => Color32::from_rgb(90, 98, 112),
        };
        let radius = if strong {
            (5.5 * self.zoom).max(4.0)
        } else {
            (4.5 * self.zoom).max(3.5)
        };
        painter.circle_filled(center, radius, palette.node_bg);
        painter.circle_stroke(center, radius, Stroke::new(1.4, color));
    }

    /// Renders a diagnostic text element indicating what object resides under the mouse.
    fn draw_hover_hint(
        &self,
        painter: &egui::Painter,
        canvas_rect: Rect,
        candidate: ConnectionCandidate,
    ) {
        let palette = electronics_palette(self.canvas_dark_mode);
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
            palette.text_muted,
        );
    }

    /// Renders the test details and compiler report list at the bottom of the canvas viewport.
    ///
    /// Outlines:
    /// - Active tab pages (Electronics Console, SPICE Simulator, Electrical Rules Check, and DRC errors).
    /// - Log entries showing warning tags, unconnected nodes, or verified OK connections with color markers.
    fn draw_test_results(&mut self, painter: &egui::Painter, canvas_rect: Rect) {
        let palette = electronics_palette(self.canvas_dark_mode);
        let line_h = 18.0;
        let results_h = (74.0 + self.test_results.len() as f32 * line_h).clamp(116.0, 220.0);
        let results_rect = Rect::from_min_size(
            Pos2::new(canvas_rect.left(), canvas_rect.bottom() - results_h),
            Vec2::new(canvas_rect.width(), results_h),
        );

        painter.rect_filled(results_rect, 0.0, palette.overlay_bg);
        painter.line_segment(
            [results_rect.left_top(), results_rect.right_top()],
            Stroke::new(1.0, palette.border),
        );

        let tabs = [
            (t("app.electronics_console", self.lang), true),
            (t("app.electronics_simulator", self.lang), false),
            (t("app.electronics_erc", self.lang), false),
            (t("app.electronics_design_rules", self.lang), false),
        ];

        let mut tab_x = results_rect.left() + 12.0;
        for (label, active) in tabs {
            painter.text(
                Pos2::new(tab_x, results_rect.top() + 18.0),
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(10.5),
                if active {
                    theme::ACCENT
                } else {
                    palette.text_muted
                },
            );
            if active {
                painter.line_segment(
                    [
                        Pos2::new(tab_x, results_rect.top() + 31.0),
                        Pos2::new(tab_x + 58.0, results_rect.top() + 31.0),
                    ],
                    Stroke::new(1.0, theme::ACCENT),
                );
            }
            tab_x += 104.0;
        }

        painter.text(
            Pos2::new(results_rect.right() - 10.0, results_rect.top() + 18.0),
            egui::Align2::RIGHT_CENTER,
            t("app.esc_close", self.lang),
            egui::FontId::proportional(9.0),
            palette.text_muted,
        );

        painter.text(
            Pos2::new(results_rect.left() + 12.0, results_rect.top() + 48.0),
            egui::Align2::LEFT_CENTER,
            t("app.electrical_test_results", self.lang),
            egui::FontId::proportional(12.0),
            theme::ACCENT,
        );

        painter.text(
            Pos2::new(results_rect.right() - 12.0, results_rect.top() + 48.0),
            egui::Align2::RIGHT_CENTER,
            format!("{} {}", self.test_results.len(), t("app.info", self.lang)),
            egui::FontId::proportional(9.0),
            palette.text_muted,
        );

        let mut y = results_rect.top() + 70.0;
        for result in &self.test_results {
            if y > results_rect.bottom() - 8.0 {
                break;
            }
            let lower = result.to_lowercase();
            let color =
                if lower.contains("passed") || lower.contains("ready") || lower.contains("ok") {
                    Color32::from_rgb(100, 220, 100)
                } else if lower.contains("unconnected")
                    || lower.contains("warning")
                    || lower.contains("open")
                {
                    Color32::from_rgb(230, 160, 60)
                } else {
                    palette.text_dim
                };

            painter.circle_filled(Pos2::new(results_rect.left() + 16.0, y), 3.0, color);
            painter.text(
                Pos2::new(results_rect.left() + 28.0, y),
                egui::Align2::LEFT_CENTER,
                result,
                egui::FontId::proportional(10.0),
                color,
            );
            y += line_h;
        }
    }

    /// Renders overlays displaying active simulation states (heat signatures, live currents, and LED brightness animations).
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
            let center =
                self.world_to_screen(Pos2::new(comp.position.x, comp.position.y), canvas_rect);

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
                        Pos2::new(
                            center.x,
                            center.y + COMP_BODY_H * self.zoom * 0.5 + 18.0 * self.zoom,
                        ),
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

    /// Exports the completed netlist graph as custom SPICE representation, copies it, and logs statistics.
    fn export_netlist(&mut self, ctx: &egui::Context) {
        let result = raf_electronics::export_netlist_text(&self.schematic);
        ctx.copy_text(result.content.clone());
        self.export_message = Some(format!(
            "{}: {} bytes | {} | {}",
            t("app.export_netlist_label", self.lang),
            result.content.len(),
            t("app.export_copied_clipboard", self.lang),
            t("app.export_log_only", self.lang),
        ));
        self.show_export_menu = false;
        tracing::info!("Export netlist:\n{}", result.content);
    }

    /// Compiles Bills of Materials (BOM) in CSV format, exports results, and sets diagnostic clipboard content.
    fn export_bom_csv(&mut self, ctx: &egui::Context) {
        let result = raf_electronics::export_bom_csv(&self.schematic);
        ctx.copy_text(result.content.clone());
        self.export_message = Some(format!(
            "{}: {} bytes | {} | {}",
            t("app.export_bom_csv_label", self.lang),
            result.content.len(),
            t("app.export_copied_clipboard", self.lang),
            t("app.export_log_only", self.lang),
        ));
        self.show_export_menu = false;
        tracing::info!("Export BOM:\n{}", result.content);
    }

    /// Rasterizes modern scalable vector drawings (SVG) for the active layout and pushes content to clipboards.
    fn export_svg(&mut self, ctx: &egui::Context) {
        let result = raf_electronics::export_svg(&self.schematic);
        ctx.copy_text(result.content.clone());
        self.export_message = Some(format!(
            "{}: {} bytes | {} | {}",
            t("app.export_svg_label", self.lang),
            result.content.len(),
            t("app.export_copied_clipboard", self.lang),
            t("app.export_log_only", self.lang),
        ));
        self.show_export_menu = false;
        tracing::info!("Export SVG:\n{}", result.content);
    }

    /// Renders the floating popup panel displaying export format choices (Netlist, CSV BOM, and vector format SVGs).
    fn draw_export_menu(&mut self, ui: &mut egui::Ui, canvas_rect: Rect) {
        let menu_id = egui::Id::new("schematic_export_menu");
        let menu_pos = Pos2::new(canvas_rect.center().x, canvas_rect.top() + 76.0);

        egui::Area::new(menu_id)
            .fixed_pos(menu_pos)
            .pivot(egui::Align2::CENTER_TOP)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(244.0);

                    ui.label(
                        egui::RichText::new(t("app.export_schematic", self.lang))
                            .color(theme::ACCENT)
                            .size(12.0),
                    );
                    ui.add_space(6.0);

                    if ui
                        .add_sized(
                            [220.0, 28.0],
                            egui::Button::new(t("app.export_netlist_label", self.lang)),
                        )
                        .clicked()
                    {
                        self.export_netlist(ui.ctx());
                    }

                    if ui
                        .add_sized(
                            [220.0, 28.0],
                            egui::Button::new(t("app.export_bom_csv_label", self.lang)),
                        )
                        .clicked()
                    {
                        self.export_bom_csv(ui.ctx());
                    }

                    if ui
                        .add_sized(
                            [220.0, 28.0],
                            egui::Button::new(t("app.export_svg_label", self.lang)),
                        )
                        .clicked()
                    {
                        self.export_svg(ui.ctx());
                    }

                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(t("app.export_log_notice", self.lang))
                            .size(9.5)
                            .color(Color32::from_rgb(130, 130, 140)),
                    );
                    ui.label(
                        egui::RichText::new(t("app.esc_close_1_2_3_export", self.lang))
                            .size(9.5)
                            .color(Color32::from_rgb(130, 130, 140)),
                    );
                    ui.add_space(4.0);

                    if ui.button(t("app.cancel", self.lang)).clicked() {
                        self.show_export_menu = false;
                    }
                });
            });
    }

    /// Performs a collision hit-test against active components relative to screen coordinates.
    ///
    /// Iterates in reverse drawing order to guarantee selection matching of foreground elements.
    fn hit_test_component(&self, mouse: Pos2, canvas_rect: Rect) -> Option<usize> {
        for (idx, comp) in self.schematic.components.iter().enumerate().rev() {
            let center =
                self.world_to_screen(Pos2::new(comp.position.x, comp.position.y), canvas_rect);
            let body = self.component_body_rect(center, symbol_kind_for_component(comp));
            if body.expand(6.0).contains(mouse) {
                return Some(idx);
            }
        }
        None
    }

    /// Tests for click focus proximity on local pins of placed components, returning (Component Index, Pin Index, World Pos) if within snap distance.
    fn hit_test_pin(&self, mouse: Pos2, canvas_rect: Rect) -> Option<(usize, usize, Pos2)> {
        let mut best: Option<(usize, usize, Pos2, f32)> = None;

        for (comp_idx, comp) in self.schematic.components.iter().enumerate() {
            for (pin_idx, pin) in comp.pins.iter().enumerate() {
                let world = component_pin_world(comp, pin);
                let screen = self.world_to_screen(world, canvas_rect);
                let distance = screen.distance(mouse);
                if distance <= PIN_SNAP_DISTANCE
                    && best.map(|entry| distance < entry.3).unwrap_or(true)
                {
                    best = Some((comp_idx, pin_idx, world, distance));
                }
            }
        }

        best.map(|(comp_idx, pin_idx, world, _)| (comp_idx, pin_idx, world))
    }

    /// Finds if mouse lies on top of a wire path segment using 2D geometry algorithms.
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

    /// Resolves closest connection targets (Terminal pin, Wire ends, mid-wire segment intersection/junction, or Grid location).
    fn resolve_connection_candidate(&self, mouse: Pos2, canvas_rect: Rect) -> ConnectionCandidate {
        let mut candidate =
            ConnectionCandidate::grid(self.snap_to_grid(self.screen_to_world(mouse, canvas_rect)));
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
            for endpoint in [
                Pos2::new(wire.start.x, wire.start.y),
                Pos2::new(wire.end.x, wire.end.y),
            ] {
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
            if distance <= WIRE_JUNCTION_SNAP_DISTANCE
                && t > 0.05
                && t < 0.95
                && distance < best_distance
            {
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

    /// Splits wires if placing an anchoring connection mid-segment, returning adjusted coordinates.
    fn prepare_connection_for_commit(&mut self, candidate: ConnectionCandidate) -> (Pos2, bool) {
        let changed = match candidate.kind {
            ConnectionKind::WireJunction => {
                self.split_wire_at_world(candidate.world, candidate.wire_index)
            }
            _ => false,
        };

        (candidate.world, changed)
    }

    /// Splits a wire segment in two parts at the requested division point.
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

    /// Scans wire endpoints hovering in proximity to component pins and creates locking anchor relations.
    fn anchor_wires_near_component(&mut self, component_index: usize) {
        let Some(component) = self.schematic.components.get(component_index).cloned() else {
            return;
        };

        self.ensure_wire_anchors_for_component_snapshot(&component);
    }

    pub(crate) fn ensure_wire_anchors_for_component_snapshot(
        &mut self,
        component: &ElectronicComponent,
    ) {

        for wire in &mut self.schematic.wires {
            for pin in &component.pins {
                let pin_world = component_pin_world(&component, pin);
                let pin_vec = glam::Vec2::new(pin_world.x, pin_world.y);
                let anchor = WireAnchor::Pin {
                    component_id: component.id,
                    pin_id: pin.id,
                };

                if wire.start_anchor.is_none() && pin_vec.distance(wire.start) <= PIN_SNAP_DISTANCE
                {
                    wire.start_anchor = Some(anchor);
                }
                if wire.end_anchor.is_none() && pin_vec.distance(wire.end) <= PIN_SNAP_DISTANCE {
                    wire.end_anchor = Some(anchor);
                }
            }
        }
    }

    /// Returns `true` if a segment of a wire is touching/overlapping the bounding box or pins of a component.
    fn wire_touches_component(
        &self,
        wire: &raf_electronics::schematic::Wire,
        component_index: usize,
    ) -> bool {
        let Some(component) = self.schematic.components.get(component_index) else {
            return false;
        };

        for anchor in [wire.start_anchor, wire.end_anchor].into_iter().flatten() {
            if let WireAnchor::Pin { component_id, .. } = anchor {
                if component_id == component.id {
                    return true;
                }
            }
        }

        component.pins.iter().any(|pin| {
            let pin_world = component_pin_world(component, pin);
            let pin_vec = glam::Vec2::new(pin_world.x, pin_world.y);
            pin_vec.distance(wire.start) <= PIN_SNAP_DISTANCE
                || pin_vec.distance(wire.end) <= PIN_SNAP_DISTANCE
        })
    }

    /// Returns the screen rectangle matching a component's symbolic size.
    fn component_body_rect(&self, center: Pos2, kind: SchematicSymbolKind) -> Rect {
        let recipe = schematic_symbol_recipe(kind);
        Rect::from_center_size(
            center,
            Vec2::new(
                (recipe.half_size[0] * 2.0 + 16.0) * self.zoom,
                (recipe.half_size[1] * 2.0 + 14.0) * self.zoom,
            ),
        )
    }

    /// Maps world coordinates into screen coordinates.
    fn world_to_screen(&self, world: Pos2, canvas_rect: Rect) -> Pos2 {
        Pos2::new(
            canvas_rect.left() + world.x * self.zoom + self.offset.x,
            canvas_rect.top() + world.y * self.zoom + self.offset.y,
        )
    }

    /// Maps screen coordinates back into world schematic space.
    fn screen_to_world(&self, screen: Pos2, canvas_rect: Rect) -> Pos2 {
        Pos2::new(
            (screen.x - canvas_rect.left() - self.offset.x) / self.zoom,
            (screen.y - canvas_rect.top() - self.offset.y) / self.zoom,
        )
    }

    /// Snaps world coordinates onto the regular step-grid.
    fn snap_to_grid(&self, pos: Pos2) -> Pos2 {
        Pos2::new(
            (pos.x / GRID_STEP).round() * GRID_STEP,
            (pos.y / GRID_STEP).round() * GRID_STEP,
        )
    }
}

/// Computes the exact absolute world coordinates of a terminal Pin helper. Handles orientation angles.
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

/// Identifies the correct symbolic type to render based on model parameters.
fn symbol_kind_for_component(comp: &ElectronicComponent) -> SchematicSymbolKind {
    match comp.sim_model {
        SimModel::Resistor { .. } => SchematicSymbolKind::Resistor,
        SimModel::Capacitor { .. } => SchematicSymbolKind::Capacitor,
        SimModel::Led { .. } => SchematicSymbolKind::Led,
        SimModel::Magnet { .. } => SchematicSymbolKind::Magnet,
        SimModel::DcSource { .. } => SchematicSymbolKind::Battery,
        SimModel::Wire if comp.designator.eq_ignore_ascii_case("GND") => {
            SchematicSymbolKind::Ground
        }
        _ => SchematicSymbolKind::Generic,
    }
}

/// Resolves resource strings pointing to internal raster icons.
fn symbol_asset_for_component(comp: &ElectronicComponent) -> &'static str {
    match comp.sim_model {
        SimModel::Resistor { .. } => "symbols/resistor.png",
        SimModel::Capacitor { .. } => "symbols/capacitor.png",
        SimModel::Led { .. } => "symbols/led.png",
        SimModel::Magnet { .. } => "symbols/magnet.png",
        SimModel::DcSource { .. } => "symbols/battery.png",
        SimModel::Wire if comp.designator.eq_ignore_ascii_case("GND") => "symbols/ground.png",
        _ => "symbols/generic.png",
    }
}

/// Clones components and clears physical IDs and net routing connections during Clipboard copies.
fn clone_component_for_paste(component: &ElectronicComponent) -> ElectronicComponent {
    let mut cloned = component.clone();
    cloned.id = uuid::Uuid::new_v4();
    for pin in &mut cloned.pins {
        pin.id = uuid::Uuid::new_v4();
        pin.net.clear();
    }

    let prefix = cloned
        .designator
        .chars()
        .take_while(|c| c.is_alphabetic())
        .collect::<String>();
    if !prefix.is_empty() {
        cloned.designator = format!("{prefix}?");
    }

    cloned
}

/// Associates current routing interactions with exact component connection pins or absolute physical coordinates.
fn wire_anchor_for_candidate(
    schematic: &raf_electronics::schematic::Schematic,
    candidate: ConnectionCandidate,
) -> Option<WireAnchor> {
    match candidate.kind {
        ConnectionKind::Pin => {
            let component_index = candidate.component_index?;
            let pin_index = candidate.pin_index?;
            let component = schematic.components.get(component_index)?;
            let pin = component.pins.get(pin_index)?;
            Some(WireAnchor::Pin {
                component_id: component.id,
                pin_id: pin.id,
            })
        }
        ConnectionKind::Grid | ConnectionKind::WireEndpoint | ConnectionKind::WireJunction => Some(
            WireAnchor::Point(glam::Vec2::new(candidate.world.x, candidate.world.y)),
        ),
    }
}

/// Rotates dynamic coordinates around a center projection anchor by a specific angle.
fn transform_local_world_point(center: Pos2, local: [f32; 2], rotation_deg: f32) -> Pos2 {
    let radians = rotation_deg.to_radians();
    let cos_r = radians.cos();
    let sin_r = radians.sin();
    let x = local[0] * cos_r - local[1] * sin_r;
    let y = local[0] * sin_r + local[1] * cos_r;
    Pos2::new(center.x + x, center.y + y)
}

/// Records a circular shape using a set number of segment approximations.
fn record_circle_outline(
    commands: &mut BasicCommandList,
    center: Pos2,
    radius: f32,
    color: [u8; 4],
) {
    const SEGMENTS: usize = 20;

    let mut previous = Pos2::new(center.x + radius, center.y);
    for step in 1..=SEGMENTS {
        let angle = step as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
        let next = Pos2::new(
            center.x + angle.cos() * radius,
            center.y + angle.sin() * radius,
        );
        commands.draw_line(
            Vec3::new(previous.x, previous.y, 0.0),
            Vec3::new(next.x, next.y, 0.0),
            color,
            1.0,
            true,
            0.0,
        );
        previous = next;
    }
}

/// Converts a 32-bit egui Color object down to raw RGBA arrays.
fn color32_to_rgba8(color: Color32) -> [u8; 4] {
    [color.r(), color.g(), color.b(), color.a()]
}

/// Transforms local geometric elements based on rotation angle and zoom factor coordinates.
fn transform_local_point(center: Pos2, local: [f32; 2], rotation_deg: f32, zoom: f32) -> Pos2 {
    let radians = rotation_deg.to_radians();
    let cos_r = radians.cos();
    let sin_r = radians.sin();
    let x = local[0] * cos_r - local[1] * sin_r;
    let y = local[0] * sin_r + local[1] * cos_r;
    Pos2::new(center.x + x * zoom, center.y + y * zoom)
}

/// Calculates orthogonal routing steps between start and end layout coordinates.
fn orthogonal_route_points(start: Pos2, end: Pos2) -> Vec<Pos2> {
    if (start.x - end.x).abs() < 0.01 || (start.y - end.y).abs() < 0.01 {
        vec![start, end]
    } else {
        vec![start, Pos2::new(end.x, start.y), end]
    }
}

/// Projects a point segment down to nearest points and evaluates linear progress ratios.
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

/// Calculates point to segment metrics and distance.
fn point_to_segment_distance(point: Pos2, start: Pos2, end: Pos2) -> f32 {
    let (_, distance, _) = project_point_to_segment(point, start, end);
    distance
}

/// Maps distinct terminal types to matching visual color representations.
fn pin_direction_color(direction: PinDirection) -> Color32 {
    match direction {
        PinDirection::Input => Color32::from_rgb(100, 180, 255),
        PinDirection::Output => Color32::from_rgb(255, 140, 60),
        PinDirection::Bidirectional => Color32::from_rgb(150, 220, 150),
        PinDirection::Power => Color32::from_rgb(255, 80, 80),
        PinDirection::Ground => Color32::from_rgb(120, 120, 130),
    }
}
