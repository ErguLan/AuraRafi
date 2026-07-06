use std::sync::Arc;

use eframe::egui_wgpu;
use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui};
use glam::{Mat4, Vec2, Vec3};
use raf_core::i18n::t;
use raf_electronics::footprint_definition;
use raf_render::api_graphic_basic::command_list::BasicCommandList;
use raf_render::api_graphic_basic::mesh::BasicMesh;
use raf_render::bridge::RenderRuntime;
use raf_render::scene_renderer::{FrameStats, SceneRenderFrame};

use super::{PcbSelection, PcbTool, PcbViewPanel};
use crate::panels::gpu_canvas::canvas_view_projection;
use crate::panels::schematic_view::electronics_palette;
use crate::theme;

const GRID_STEP: f32 = 20.0;
const PCB_Z_GRID: f32 = 0.92;
const PCB_Z_BOARD_FILL: f32 = 0.78;
const PCB_Z_BOARD_OUTLINE: f32 = 0.74;
const PCB_Z_BOTTOM_TRACE: f32 = 0.58;
const PCB_Z_TOP_TRACE: f32 = 0.44;
const PCB_Z_AIRWIRE: f32 = 0.30;
const PCB_Z_COMPONENT_BODY: f32 = 0.18;
const PCB_Z_COMPONENT_PAD: f32 = 0.14;

impl PcbViewPanel {
    pub(super) fn draw_canvas(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        wgpu_render_state: Option<&egui_wgpu::RenderState>,
        render_runtime: &mut RenderRuntime,
    ) -> bool {
        let response = ui.allocate_rect(rect, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        self.canvas_dark_mode = ui.visuals().dark_mode;
        let palette = electronics_palette(self.canvas_dark_mode);
        let mut changed = false;
        let hover_pos = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|pointer| rect.contains(*pointer));
        let hovered_component = hover_pos.and_then(|pointer| self.hit_component(rect, pointer));
        let hovered_trace = hover_pos.and_then(|pointer| self.hit_trace(rect, pointer));
        let hovered_airwire = hover_pos.and_then(|pointer| self.hit_airwire(rect, pointer));
        let render_w = rect.width().max(1.0).round() as u32;
        let render_h = rect.height().max(1.0).round() as u32;
        let gpu_frame = self.build_gpu_canvas_frame(render_w, render_h);
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

        if gpu_backdrop_ready {
            self.gpu_canvas.paint(&painter, rect);
        } else {
            painter.rect_filled(rect, 0.0, palette.canvas_bg);
            self.draw_grid(&painter, rect);
            self.draw_board(&painter, rect);
            self.draw_traces(&painter, rect);
            if self.show_airwires {
                self.draw_airwires(&painter, rect);
            }
        }
        self.draw_trace_overlays(&painter, rect, hovered_trace, hovered_airwire);
        self.draw_components(&painter, rect, !gpu_backdrop_ready, hovered_component);
        self.draw_outline_draft(&painter, rect, hover_pos);

        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                self.zoom = (self.zoom + scroll * 0.003).clamp(0.35, 4.0);
            }
        }

        if response.dragged_by(egui::PointerButton::Middle)
            || response.dragged_by(egui::PointerButton::Secondary)
        {
            let delta = ui.input(|i| i.pointer.delta());
            self.offset += delta;
        }

        let pointer_pos = ui.input(|i| i.pointer.interact_pos());
        let primary_clicked = response.clicked_by(egui::PointerButton::Primary);
        let primary_drag_started = response.drag_started_by(egui::PointerButton::Primary);
        let secondary_clicked = response.clicked_by(egui::PointerButton::Secondary);

        match self.tool {
            PcbTool::Select => {
                if primary_drag_started {
                    if let Some(pointer) = pointer_pos {
                        let world = self.snap_world(rect, self.screen_to_world(rect, pointer));
                        if let Some(component_index) = self.hit_component(rect, pointer) {
                            self.selection = PcbSelection::Component(component_index);
                            if let Some(component) = self.layout.components.get(component_index) {
                                self.drag_state = Some((component_index, component.position - world));
                            }
                        }
                    }
                }

                if primary_clicked {
                    if let Some(pointer) = pointer_pos {
                        if let Some(component_index) = self.hit_component(rect, pointer) {
                            self.selection = PcbSelection::Component(component_index);
                            self.drag_state = None;
                        } else if let Some(trace_index) = self.hit_trace(rect, pointer) {
                            self.selection = PcbSelection::Trace(trace_index);
                            self.drag_state = None;
                        } else if let Some(airwire_index) = self.hit_airwire(rect, pointer) {
                            self.selection = PcbSelection::Airwire(airwire_index);
                            self.tool = PcbTool::Route;
                            self.drag_state = None;
                            changed = true;
                        } else {
                            self.selection = PcbSelection::None;
                            self.drag_state = None;
                        }
                    }
                }

                if let Some((component_index, anchor)) = self.drag_state {
                    if ui.input(|i| i.pointer.primary_down()) {
                        if let Some(pointer) = pointer_pos {
                            let snapped =
                                self.snap_world(rect, self.screen_to_world(rect, pointer) + anchor);
                            if let Some(component) = self.layout.components.get_mut(component_index)
                            {
                                if !component.locked {
                                    if component.position != snapped {
                                        component.position = snapped;
                                        self.layout.rebuild_airwires();
                                        changed = true;
                                    }
                                }
                            }
                        }
                    } else {
                        self.drag_state = None;
                    }
                }
            }
            PcbTool::Route => {
                if primary_clicked {
                    if let Some(pointer) = pointer_pos {
                        if let Some(airwire_index) = self.hit_airwire(rect, pointer) {
                            self.selection = PcbSelection::Airwire(airwire_index);
                            if self.layout.route_airwire(airwire_index) {
                                self.selection = PcbSelection::None;
                                changed = true;
                            }
                        }
                    }
                }
            }
            PcbTool::Outline => {
                if primary_clicked {
                    if let Some(pointer) = pointer_pos {
                        let world = self.snap_world(rect, self.screen_to_world(rect, pointer));
                        if self.outline_draft.len() >= 3
                            && self
                                .outline_draft
                                .first()
                                .map(|first| first.distance(world) <= GRID_STEP * 0.5)
                                .unwrap_or(false)
                        {
                            let mut closed = self.outline_draft.clone();
                            closed.push(self.outline_draft[0]);
                            self.layout.board_outline.points = closed;
                            self.outline_draft.clear();
                            self.tool = PcbTool::Select;
                            changed = true;
                        } else {
                            self.outline_draft.push(world);
                        }
                    }
                }

                if secondary_clicked {
                    self.outline_draft.clear();
                    self.tool = PcbTool::Select;
                }
            }
        }

        let hint = match self.tool {
            PcbTool::Select => {
                if self.selected_airwire_index().is_some() {
                    t("app.pcb_route_selected_hint", self.lang)
                } else if hovered_component.is_some() {
                    t("app.pcb_canvas_hint_hover_component", self.lang)
                } else {
                    t("app.pcb_canvas_hint", self.lang)
                }
            }
            PcbTool::Route => {
                if hovered_airwire.is_some() {
                    t("app.pcb_route_hint_hover", self.lang)
                } else {
                    t("app.pcb_route_hint", self.lang)
                }
            }
            PcbTool::Outline => t("app.pcb_outline_hint", self.lang),
        };
        // Show cursor position in mm (PCB canvas unit = 1mm).
        let cursor_mm = hover_pos.map(|p| {
            let w = self.screen_to_world(rect, p);
            format!("Cursor: {:.1}, {:.1} mm | ", w.x, w.y)
        }).unwrap_or_default();
        let info_text = format!("{}{}", cursor_mm, hint);
        painter.text(
            Pos2::new(rect.left() + 12.0, rect.bottom() - 18.0),
            egui::Align2::LEFT_BOTTOM,
            info_text,
            egui::FontId::proportional(11.0),
            palette.text_muted,
        );

        changed
    }

    fn build_gpu_canvas_frame(&self, width: u32, height: u32) -> SceneRenderFrame {
        let mut commands = BasicCommandList::new();
        let palette = electronics_palette(self.canvas_dark_mode);
        commands.clear(color_array(palette.canvas_bg));

        let (left, right, top, bottom) = self.visible_world_bounds(width as f32, height as f32);
        self.record_gpu_grid(&mut commands, left, right, top, bottom);
        self.record_board_geometry(&mut commands);
        self.record_trace_geometry(&mut commands);
        if self.show_airwires {
            self.record_airwire_geometry(&mut commands);
        }
        self.record_component_geometry(&mut commands);

        SceneRenderFrame {
            commands,
            view_proj: canvas_view_projection(left, right, top, bottom),
            light_dir: Vec3::Z,
            width,
            height,
            stats: FrameStats::default(),
        }
    }

    fn visible_world_bounds(&self, width: f32, height: f32) -> (f32, f32, f32, f32) {
        let zoom = self.zoom.max(0.001);
        let left = (-self.offset.x) / zoom;
        let right = (width - self.offset.x) / zoom;
        let top = (-self.offset.y) / zoom;
        let bottom = (height - self.offset.y) / zoom;
        (left, right, top, bottom)
    }

    fn record_gpu_grid(
        &self,
        commands: &mut BasicCommandList,
        left: f32,
        right: f32,
        top: f32,
        bottom: f32,
    ) {
        let spacing = GRID_STEP * self.zoom.max(0.1);
        if spacing < 8.0 {
            return;
        }

        let world_left = (left / GRID_STEP).floor() as i32 - 2;
        let world_right = (right / GRID_STEP).ceil() as i32 + 2;
        let world_top = (top / GRID_STEP).floor() as i32 - 2;
        let world_bottom = (bottom / GRID_STEP).ceil() as i32 + 2;

        for ix in world_left..=world_right {
            let x = ix as f32 * GRID_STEP;
            commands.draw_line(
                Vec3::new(x, top, PCB_Z_GRID),
                Vec3::new(x, bottom, PCB_Z_GRID),
                if self.canvas_dark_mode {
                    [25, 31, 38, 255]
                } else {
                    [211, 220, 232, 255]
                },
                1.0,
                false,
                0.0,
            );
        }

        for iy in world_top..=world_bottom {
            let y = iy as f32 * GRID_STEP;
            commands.draw_line(
                Vec3::new(left, y, PCB_Z_GRID),
                Vec3::new(right, y, PCB_Z_GRID),
                if self.canvas_dark_mode {
                    [25, 31, 38, 255]
                } else {
                    [211, 220, 232, 255]
                },
                1.0,
                false,
                0.0,
            );
        }
    }

    fn record_board_geometry(&self, commands: &mut BasicCommandList) {
        let outline_points = self.board_outline_points();
        if outline_points.len() < 2 {
            return;
        }

        if self.layout.outline_is_closed() && outline_points.len() >= 3 {
            let positions: Vec<Vec3> = outline_points
                .iter()
                .map(|point| Vec3::new(point.x, point.y, 0.0))
                .collect();
            let indices = triangle_fan_indices(outline_points.len());
            if !indices.is_empty() {
                let mesh_id = commands
                    .register_mesh(Arc::new(BasicMesh::from_positions(&positions, &indices)));
                commands.draw_mesh(
                    mesh_id,
                    Mat4::from_translation(Vec3::new(0.0, 0.0, PCB_Z_BOARD_FILL)),
                    if self.canvas_dark_mode {
                        [21, 46, 33, 255]
                    } else {
                        [199, 232, 209, 255]
                    },
                );
            }
        }

        let mut polyline = outline_points.clone();
        if self.layout.outline_is_closed() && polyline.first() != polyline.last() {
            if let Some(first) = polyline.first().copied() {
                polyline.push(first);
            }
        }

        for pair in polyline.windows(2) {
            commands.draw_line(
                Vec3::new(pair[0].x, pair[0].y, PCB_Z_BOARD_OUTLINE),
                Vec3::new(pair[1].x, pair[1].y, PCB_Z_BOARD_OUTLINE),
                if self.canvas_dark_mode {
                    [101, 187, 126, 255]
                } else {
                    [55, 145, 82, 255]
                },
                1.0,
                false,
                0.0,
            );
        }
    }

    fn record_trace_geometry(&self, commands: &mut BasicCommandList) {
        for (index, trace) in self.layout.traces.iter().enumerate() {
            let color = if self.selection == PcbSelection::Trace(index) {
                [255, 210, 140, 255]
            } else {
                match trace.layer {
                    raf_electronics::PcbLayer::TopCopper => [224, 120, 72, 255],
                    raf_electronics::PcbLayer::BottomCopper => [84, 172, 214, 255],
                }
            };

            let z = match trace.layer {
                raf_electronics::PcbLayer::TopCopper => PCB_Z_TOP_TRACE,
                raf_electronics::PcbLayer::BottomCopper => PCB_Z_BOTTOM_TRACE,
            };

            for pair in trace.points.windows(2) {
                commands.draw_line(
                    Vec3::new(pair[0].x, pair[0].y, z),
                    Vec3::new(pair[1].x, pair[1].y, z),
                    color,
                    trace.width.max(1.0),
                    false,
                    0.0,
                );
            }
        }
    }

    fn record_airwire_geometry(&self, commands: &mut BasicCommandList) {
        for (index, airwire) in self.layout.airwires.iter().enumerate() {
            let color = if self.selection == PcbSelection::Airwire(index) {
                [255, 220, 120, 255]
            } else {
                [170, 170, 60, 255]
            };

            commands.draw_line(
                Vec3::new(airwire.from.x, airwire.from.y, PCB_Z_AIRWIRE),
                Vec3::new(airwire.to.x, airwire.to.y, PCB_Z_AIRWIRE),
                color,
                1.0,
                false,
                0.0,
            );
        }
    }

    fn record_component_geometry(&self, commands: &mut BasicCommandList) {
        let body_mesh_id = commands.register_mesh(centered_unit_quad_mesh());
        let pad_mesh_id = commands.register_mesh(centered_unit_quad_mesh());

        for (index, component) in self.layout.components.iter().enumerate() {
            let footprint =
                footprint_definition(&component.footprint, component.pad_nets.len().max(1));
            let body_color = if self.selection == PcbSelection::Component(index) {
                [70, 52, 31, 255]
            } else if component.locked {
                [42, 45, 52, 255]
            } else {
                [26, 31, 39, 255]
            };

            commands.draw_mesh(
                body_mesh_id,
                quad_transform(
                    component.position,
                    footprint.body_size,
                    PCB_Z_COMPONENT_BODY,
                ),
                body_color,
            );

            for (pad_index, pad) in footprint.pads.iter().enumerate() {
                let Some(world) = self.layout.pad_world_position(index, pad_index) else {
                    continue;
                };
                let pad_color = if self.selection == PcbSelection::Component(index) {
                    [255, 205, 110, 255]
                } else {
                    [226, 132, 42, 255]
                };
                commands.draw_mesh(
                    pad_mesh_id,
                    quad_transform(world, pad.size, PCB_Z_COMPONENT_PAD),
                    pad_color,
                );
            }
        }
    }

    fn draw_grid(&self, painter: &egui::Painter, rect: Rect) {
        let spacing = GRID_STEP * self.zoom.max(0.1);
        if spacing < 8.0 {
            return;
        }

        let start_x = ((rect.left() + self.offset.x) % spacing + spacing) % spacing;
        let start_y = ((rect.top() + self.offset.y) % spacing + spacing) % spacing;

        let grid_stroke = Stroke::new(
            1.0,
            if self.canvas_dark_mode {
                Color32::from_rgb(25, 31, 38)
            } else {
                Color32::from_rgb(211, 220, 232)
            },
        );
        let mut x = rect.left() + start_x;
        while x <= rect.right() {
            painter.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                grid_stroke,
            );
            x += spacing;
        }

        let mut y = rect.top() + start_y;
        while y <= rect.bottom() {
            painter.line_segment(
                [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                grid_stroke,
            );
            y += spacing;
        }
    }

    fn draw_board(&self, painter: &egui::Painter, rect: Rect) {
        if self.layout.board_outline.points.len() < 2 {
            return;
        }
        let palette = electronics_palette(self.canvas_dark_mode);

        let board_points = self
            .layout
            .board_outline
            .points
            .iter()
            .map(|point| self.world_to_screen(rect, *point))
            .collect::<Vec<_>>();

        if self.layout.outline_is_closed() && board_points.len() >= 4 {
            let shadow_points = board_points
                .iter()
                .map(|point| *point + egui::vec2(0.0, 3.0))
                .collect::<Vec<_>>();
            painter.add(egui::Shape::convex_polygon(
                shadow_points,
                Color32::from_rgba_premultiplied(
                    0,
                    0,
                    0,
                    if self.canvas_dark_mode { 90 } else { 28 },
                ),
                Stroke::NONE,
            ));
            painter.add(egui::Shape::convex_polygon(
                board_points.clone(),
                if self.canvas_dark_mode {
                    Color32::from_rgb(21, 46, 33)
                } else {
                    Color32::from_rgb(199, 232, 209)
                },
                Stroke::NONE,
            ));
        }

        for pair in board_points.windows(2) {
            painter.line_segment(
                [pair[0], pair[1]],
                Stroke::new(
                    2.0,
                    if self.canvas_dark_mode {
                        Color32::from_rgb(101, 187, 126)
                    } else {
                        Color32::from_rgb(55, 145, 82)
                    },
                ),
            );
        }

        if let Some(first) = board_points.first().copied() {
            painter.text(
                first + egui::vec2(8.0, 18.0),
                egui::Align2::LEFT_TOP,
                &self.layout.name,
                egui::FontId::proportional(11.0),
                palette.text_dim,
            );
        }
    }

    fn draw_components(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        draw_fill_geometry: bool,
        hovered_component: Option<usize>,
    ) {
        for (index, component) in self.layout.components.iter().enumerate() {
            let footprint =
                footprint_definition(&component.footprint, component.pad_nets.len().max(1));
            let center = self.world_to_screen(rect, component.position);
            let asset_size =
                footprint_asset_screen_size(&component.footprint, footprint.body_size, self.zoom);
            let body_size = egui::vec2(
                footprint.body_size.x * self.zoom,
                footprint.body_size.y * self.zoom,
            );
            let visual_size =
                egui::vec2(body_size.x.max(asset_size.x), body_size.y.max(asset_size.y));
            let visual_rect = Rect::from_center_size(center, visual_size);
            let selected = self.selection == PcbSelection::Component(index);
            let hovered = hovered_component == Some(index);
            let palette = electronics_palette(self.canvas_dark_mode);
            let stroke_color = if selected {
                theme::ACCENT
            } else if hovered {
                Color32::from_rgb(255, 205, 110)
            } else if self.canvas_dark_mode {
                Color32::from_rgb(72, 82, 96)
            } else {
                Color32::from_rgb(146, 158, 174)
            };
            let fill_color = if selected {
                palette.card_active
            } else if hovered {
                palette.card_hover
            } else if component.locked {
                Color32::from_rgb(42, 45, 52)
            } else {
                palette.card_bg
            };

            if draw_fill_geometry {
                painter.rect(
                    visual_rect,
                    6.0,
                    fill_color,
                    Stroke::new(if selected { 2.0 } else { 1.0 }, stroke_color),
                );
            } else {
                painter.rect_stroke(
                    visual_rect,
                    6.0,
                    Stroke::new(if selected { 2.0 } else { 1.0 }, stroke_color),
                );
            }

            let asset_rect = visual_rect.shrink2(egui::vec2(4.0, 6.0));
            let tint = if component.locked {
                Color32::from_rgba_premultiplied(210, 214, 220, 170)
            } else {
                Color32::WHITE
            };
            let _ = self.asset_atlas.paint(
                painter,
                footprint_asset_name(&component.footprint),
                asset_rect,
                tint,
            );

            for (pad_index, pad) in footprint.pads.iter().enumerate() {
                if let Some(world) = self.layout.pad_world_position(index, pad_index) {
                    let pad_center = self.world_to_screen(rect, world);
                    let pad_rect = Rect::from_center_size(
                        pad_center,
                        egui::vec2(
                            (pad.size.x * self.zoom).max(7.0),
                            (pad.size.y * self.zoom).max(7.0),
                        ),
                    );
                    let pad_net = component
                        .pad_nets
                        .get(pad_index)
                        .map(String::as_str)
                        .unwrap_or("");
                    let pad_color = net_color(pad_net, pad_index);
                    painter.rect_filled(pad_rect, 3.0, pad_color);
                    painter.rect_stroke(
                        pad_rect,
                        3.0,
                        Stroke::new(1.0, Color32::from_rgb(255, 224, 150)),
                    );

                    if selected || hovered {
                        painter.text(
                            pad_center + egui::vec2(0.0, -10.0),
                            egui::Align2::CENTER_BOTTOM,
                            &pad.name,
                            egui::FontId::proportional(9.0),
                            palette.text_dim,
                        );
                        if !pad_net.trim().is_empty() {
                            painter.text(
                                pad_center + egui::vec2(0.0, 11.0),
                                egui::Align2::CENTER_TOP,
                                pad_net,
                                egui::FontId::proportional(8.0),
                                palette.text_muted,
                            );
                        }
                    }
                }
            }

            painter.text(
                visual_rect.center_top() + egui::vec2(0.0, -6.0),
                egui::Align2::CENTER_BOTTOM,
                &component.designator,
                egui::FontId::proportional(12.0),
                if selected {
                    theme::ACCENT
                } else {
                    palette.text
                },
            );
            painter.text(
                visual_rect.center_bottom() + egui::vec2(0.0, 4.0),
                egui::Align2::CENTER_TOP,
                if component.value.trim().is_empty() {
                    &component.footprint
                } else {
                    &component.value
                },
                egui::FontId::proportional(10.0),
                palette.text_dim,
            );

            if hovered && !selected {
                painter.rect_stroke(
                    visual_rect.expand(4.0),
                    8.0,
                    Stroke::new(1.0, Color32::from_rgb(255, 205, 110)),
                );
            }
        }
    }

    fn draw_trace_overlays(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        hovered_trace: Option<usize>,
        hovered_airwire: Option<usize>,
    ) {
        if let Some(index) = hovered_trace {
            if let Some(trace) = self.layout.traces.get(index) {
                for pair in trace.points.windows(2) {
                    painter.line_segment(
                        [
                            self.world_to_screen(rect, pair[0]),
                            self.world_to_screen(rect, pair[1]),
                        ],
                        Stroke::new(
                            (trace.width * self.zoom * 0.14).max(4.0),
                            Color32::from_rgb(255, 210, 140),
                        ),
                    );
                }
            }
        }

        if let Some(index) = hovered_airwire {
            if let Some(airwire) = self.layout.airwires.get(index) {
                let start = self.world_to_screen(rect, airwire.from);
                let end = self.world_to_screen(rect, airwire.to);
                draw_dashed_line(
                    painter,
                    start,
                    end,
                    Stroke::new(2.5, Color32::from_rgb(255, 220, 120)),
                    8.0,
                    5.0,
                );
                painter.circle_filled(start, 4.0, Color32::from_rgb(255, 220, 120));
                painter.circle_filled(end, 4.0, Color32::from_rgb(255, 220, 120));
            }
        }
    }

    fn draw_traces(&self, painter: &egui::Painter, rect: Rect) {
        for (index, trace) in self.layout.traces.iter().enumerate() {
            let color = match trace.layer {
                raf_electronics::PcbLayer::TopCopper => Color32::from_rgb(238, 132, 28),
                raf_electronics::PcbLayer::BottomCopper => Color32::from_rgb(94, 176, 245),
            };
            let stroke = Stroke::new(
                (trace.width * self.zoom * 0.12).max(2.0),
                if self.selection == PcbSelection::Trace(index) {
                    Color32::from_rgb(255, 210, 140)
                } else {
                    color
                },
            );

            for pair in trace.points.windows(2) {
                let start = self.world_to_screen(rect, pair[0]);
                let end = self.world_to_screen(rect, pair[1]);
                painter.line_segment([start, end], stroke);
                painter.circle_filled(start, stroke.width * 0.45, stroke.color);
                painter.circle_filled(end, stroke.width * 0.45, stroke.color);
            }
        }
    }

    fn draw_airwires(&self, painter: &egui::Painter, rect: Rect) {
        for (index, airwire) in self.layout.airwires.iter().enumerate() {
            let selected = self.selection == PcbSelection::Airwire(index);
            let start = self.world_to_screen(rect, airwire.from);
            let end = self.world_to_screen(rect, airwire.to);
            let stroke = Stroke::new(
                if selected { 2.0 } else { 1.2 },
                if selected {
                    Color32::from_rgb(255, 220, 120)
                } else {
                    Color32::from_rgba_premultiplied(255, 204, 86, 165)
                },
            );
            draw_dashed_line(
                painter,
                start,
                end,
                stroke,
                if selected { 9.0 } else { 7.0 },
                5.0,
            );
            painter.circle_stroke(
                start,
                if selected { 4.0 } else { 3.0 },
                Stroke::new(
                    1.0,
                    if selected {
                        Color32::from_rgb(255, 220, 120)
                    } else {
                        stroke.color
                    },
                ),
            );
            painter.circle_stroke(
                end,
                if selected { 4.0 } else { 3.0 },
                Stroke::new(
                    1.0,
                    if selected {
                        Color32::from_rgb(255, 220, 120)
                    } else {
                        stroke.color
                    },
                ),
            );
        }
    }

    fn draw_outline_draft(&self, painter: &egui::Painter, rect: Rect, hover_pos: Option<Pos2>) {
        if self.outline_draft.is_empty() {
            return;
        }

        let stroke = Stroke::new(1.5, Color32::from_rgb(250, 200, 120));
        for pair in self.outline_draft.windows(2) {
            painter.line_segment(
                [
                    self.world_to_screen(rect, pair[0]),
                    self.world_to_screen(rect, pair[1]),
                ],
                stroke,
            );
        }

        for point in &self.outline_draft {
            painter.circle_filled(
                self.world_to_screen(rect, *point),
                3.0,
                Color32::from_rgb(250, 200, 120),
            );
        }

        if self.tool == PcbTool::Outline {
            if let (Some(last), Some(pointer)) = (self.outline_draft.last().copied(), hover_pos) {
                painter.line_segment(
                    [self.world_to_screen(rect, last), pointer],
                    Stroke::new(1.0, Color32::from_rgba_premultiplied(250, 200, 120, 180)),
                );
            }
        }
    }

    fn hit_component(&self, rect: Rect, pointer: Pos2) -> Option<usize> {
        for (index, component) in self.layout.components.iter().enumerate().rev() {
            let footprint =
                footprint_definition(&component.footprint, component.pad_nets.len().max(1));
            let visual_size =
                footprint_asset_screen_size(&component.footprint, footprint.body_size, self.zoom);
            let body_rect =
                Rect::from_center_size(self.world_to_screen(rect, component.position), visual_size);
            if body_rect.expand(10.0).contains(pointer) {
                return Some(index);
            }

            for (pad_index, pad) in footprint.pads.iter().enumerate() {
                let Some(world) = self.layout.pad_world_position(index, pad_index) else {
                    continue;
                };
                let pad_rect = Rect::from_center_size(
                    self.world_to_screen(rect, world),
                    egui::vec2(pad.size.x * self.zoom, pad.size.y * self.zoom),
                );
                if pad_rect.expand(8.0).contains(pointer) {
                    return Some(index);
                }
            }
        }
        None
    }

    fn hit_trace(&self, rect: Rect, pointer: Pos2) -> Option<usize> {
        for (index, trace) in self.layout.traces.iter().enumerate().rev() {
            for pair in trace.points.windows(2) {
                let start = self.world_to_screen(rect, pair[0]);
                let end = self.world_to_screen(rect, pair[1]);
                if distance_to_segment(pointer, start, end) <= 10.0 {
                    return Some(index);
                }
            }
        }
        None
    }

    fn hit_airwire(&self, rect: Rect, pointer: Pos2) -> Option<usize> {
        for (index, airwire) in self.layout.airwires.iter().enumerate() {
            let start = self.world_to_screen(rect, airwire.from);
            let end = self.world_to_screen(rect, airwire.to);
            if distance_to_segment(pointer, start, end) <= 12.0 {
                return Some(index);
            }
        }
        None
    }

    fn board_outline_points(&self) -> Vec<Vec2> {
        let mut points = self.layout.board_outline.points.clone();
        if points.len() >= 2 && points.first() == points.last() {
            points.pop();
        }
        points
    }

    fn world_to_screen(&self, rect: Rect, world: Vec2) -> Pos2 {
        Pos2::new(
            rect.left() + self.offset.x + world.x * self.zoom,
            rect.top() + self.offset.y + world.y * self.zoom,
        )
    }

    fn screen_to_world(&self, rect: Rect, screen: Pos2) -> Vec2 {
        Vec2::new(
            (screen.x - rect.left() - self.offset.x) / self.zoom,
            (screen.y - rect.top() - self.offset.y) / self.zoom,
        )
    }

    fn snap_world(&self, _rect: Rect, world: Vec2) -> Vec2 {
        Vec2::new(
            (world.x / GRID_STEP).round() * GRID_STEP,
            (world.y / GRID_STEP).round() * GRID_STEP,
        )
    }
}

fn distance_to_segment(point: Pos2, start: Pos2, end: Pos2) -> f32 {
    let segment = end - start;
    let len_sq = segment.length_sq();
    if len_sq <= f32::EPSILON {
        return point.distance(start);
    }

    let to_point = point - start;
    let t = (to_point.dot(segment) / len_sq).clamp(0.0, 1.0);
    let projection = start + segment * t;
    point.distance(projection)
}

fn centered_unit_quad_mesh() -> Arc<BasicMesh> {
    Arc::new(BasicMesh::from_positions(
        &[
            Vec3::new(-0.5, -0.5, 0.0),
            Vec3::new(0.5, -0.5, 0.0),
            Vec3::new(0.5, 0.5, 0.0),
            Vec3::new(-0.5, 0.5, 0.0),
        ],
        &[0, 1, 2, 0, 2, 3],
    ))
}

fn quad_transform(center: Vec2, size: Vec2, z: f32) -> Mat4 {
    Mat4::from_translation(Vec3::new(center.x, center.y, z))
        * Mat4::from_scale(Vec3::new(size.x.max(0.001), size.y.max(0.001), 1.0))
}

fn triangle_fan_indices(vertex_count: usize) -> Vec<u32> {
    if vertex_count < 3 {
        return Vec::new();
    }

    let mut indices = Vec::with_capacity((vertex_count - 2) * 3);
    for index in 1..(vertex_count - 1) {
        indices.push(0);
        indices.push(index as u32);
        indices.push((index + 1) as u32);
    }
    indices
}

fn footprint_asset_name(footprint: &str) -> &'static str {
    let id = footprint.trim();
    if id.eq_ignore_ascii_case("0805") {
        "footprints/0805.png"
    } else if id.eq_ignore_ascii_case("MAG-10x5") {
        "footprints/magnet-10x5.png"
    } else if id.eq_ignore_ascii_case("BAT-18650") {
        "footprints/battery-18650.png"
    } else if id.eq_ignore_ascii_case("TP-GND") {
        "footprints/test-point.png"
    } else {
        "footprints/generic.png"
    }
}

fn footprint_asset_screen_size(footprint: &str, body_size: Vec2, zoom: f32) -> egui::Vec2 {
    let id = footprint.trim();
    let size = if id.eq_ignore_ascii_case("0805") {
        Vec2::new(66.0, 44.0)
    } else if id.eq_ignore_ascii_case("MAG-10x5") {
        Vec2::new(92.0, 54.0)
    } else if id.eq_ignore_ascii_case("BAT-18650") {
        Vec2::new(136.0, 56.0)
    } else if id.eq_ignore_ascii_case("TP-GND") {
        Vec2::new(48.0, 48.0)
    } else {
        Vec2::new(
            (body_size.x + 28.0).max(64.0),
            (body_size.y + 22.0).max(42.0),
        )
    };
    egui::vec2(size.x * zoom, size.y * zoom)
}

fn draw_dashed_line(
    painter: &egui::Painter,
    start: Pos2,
    end: Pos2,
    stroke: Stroke,
    dash: f32,
    gap: f32,
) {
    let delta = end - start;
    let length = delta.length();
    if length <= f32::EPSILON {
        return;
    }

    let dir = delta / length;
    let mut cursor = 0.0;
    while cursor < length {
        let next = (cursor + dash).min(length);
        painter.line_segment([start + dir * cursor, start + dir * next], stroke);
        cursor += dash + gap;
    }
}

fn color_array(color: Color32) -> [u8; 4] {
    [color.r(), color.g(), color.b(), color.a()]
}

fn net_color(net: &str, index: usize) -> Color32 {
    let trimmed = net.trim();
    if trimmed.eq_ignore_ascii_case("gnd") || trimmed == "0" {
        return Color32::from_rgb(94, 176, 245);
    }
    if trimmed.contains('+') || trimmed.to_ascii_lowercase().contains("vcc") {
        return Color32::from_rgb(238, 132, 28);
    }

    match index % 4 {
        0 => Color32::from_rgb(92, 214, 142),
        1 => Color32::from_rgb(238, 132, 28),
        2 => Color32::from_rgb(94, 176, 245),
        _ => Color32::from_rgb(255, 204, 86),
    }
}
