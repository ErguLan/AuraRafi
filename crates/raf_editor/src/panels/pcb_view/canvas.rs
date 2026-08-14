use eframe::egui_wgpu;
use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui};
use glam::Vec2;
use raf_core::i18n::t;
use raf_electronics::{footprint_definition, CadObjectKind, CadScene};
use raf_render::api_graphic_basic::cad_surface::CadSurfaceHitRegion;
use raf_render::bridge::{GraphicsSurfaceKind, RenderRuntime};

use super::{PcbSelection, PcbTool, PcbViewPanel};
use crate::panels::electronics_cad_surface_host::CadSurfaceSelection;
use crate::panels::schematic_view::electronics_palette;
use crate::theme;

const GRID_STEP: f32 = 20.0;
const GPU_DETAIL_COMPONENT_LIMIT: usize = 96;
const GPU_DETAIL_ZOOM_THRESHOLD: f32 = 1.15;

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
        let mut document_changed = false;
        let hover_pos = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|pointer| rect.contains(*pointer));
        let render_w = rect.width().max(1.0).round() as u32;
        let render_h = rect.height().max(1.0).round() as u32;
        let (left, right, top, bottom) =
            self.visible_world_bounds(render_w as f32, render_h as f32);
        let document_revision = self.document_epoch;
        if self
            .cad_scene_cache
            .as_ref()
            .map_or(true, |(fingerprint, _)| *fingerprint != document_revision)
        {
            let mut cad_scene = CadScene::from_pcb(&self.layout);
            if !self.show_airwires {
                cad_scene
                    .objects
                    .retain(|object| object.kind != CadObjectKind::Airwire);
            }
            self.cad_scene_cache = Some((document_revision, cad_scene));
        }
        let cad_scene = &self
            .cad_scene_cache
            .as_ref()
            .expect("PCB CAD scene cache must exist")
            .1;
        let cad_selection = self.cad_surface_selection();
        self.cad_surface_host.present_with_revision(
            ui.ctx(),
            wgpu_render_state,
            render_runtime,
            GraphicsSurfaceKind::PcbCanvas,
            cad_scene,
            document_revision,
            [render_w, render_h],
            [left, right, top, bottom],
            self.canvas_dark_mode,
            &cad_selection,
        );
        self.render_runtime = self.cad_surface_host.last_runtime();
        let gpu_backdrop_ready = self.cad_surface_host.is_ready();
        let hovered_surface_region = hover_pos
            .and_then(|pointer| {
                self.cad_surface_host
                    .hit_test_world(self.screen_to_world(rect, pointer))
            })
            .cloned();
        let hovered_component = hovered_surface_region
            .as_ref()
            .and_then(|region| self.component_index_from_cad_region(region))
            .or_else(|| hover_pos.and_then(|pointer| self.hit_component(rect, pointer)));
        let hovered_trace = hovered_surface_region
            .as_ref()
            .and_then(|region| self.trace_index_from_cad_region(region))
            .or_else(|| hover_pos.and_then(|pointer| self.hit_trace(rect, pointer)));
        let hovered_airwire = hovered_surface_region
            .as_ref()
            .and_then(|region| self.airwire_index_from_cad_region(region))
            .or_else(|| hover_pos.and_then(|pointer| self.hit_airwire(rect, pointer)));

        if gpu_backdrop_ready {
            self.cad_surface_host.paint(&painter, rect);
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
        let draw_detail_overlay = !gpu_backdrop_ready
            || self.layout.components.len() <= GPU_DETAIL_COMPONENT_LIMIT
            || self.zoom >= GPU_DETAIL_ZOOM_THRESHOLD;
        self.draw_components(
            &painter,
            rect,
            !gpu_backdrop_ready,
            draw_detail_overlay,
            hovered_component,
        );
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
                        if let Some(component_index) =
                            hovered_component.or_else(|| self.hit_component(rect, pointer))
                        {
                            self.selection = PcbSelection::Component(component_index);
                            if let Some(component) = self.layout.components.get(component_index) {
                                self.drag_state =
                                    Some((component_index, component.position - world));
                            }
                        }
                    }
                }

                if primary_clicked {
                    if let Some(pointer) = pointer_pos {
                        if let Some(component_index) =
                            hovered_component.or_else(|| self.hit_component(rect, pointer))
                        {
                            self.selection = PcbSelection::Component(component_index);
                            self.drag_state = None;
                        } else if let Some(trace_index) =
                            hovered_trace.or_else(|| self.hit_trace(rect, pointer))
                        {
                            self.selection = PcbSelection::Trace(trace_index);
                            self.drag_state = None;
                        } else if let Some(airwire_index) =
                            hovered_airwire.or_else(|| self.hit_airwire(rect, pointer))
                        {
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
                                        document_changed = true;
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
                        if let Some(airwire_index) =
                            hovered_airwire.or_else(|| self.hit_airwire(rect, pointer))
                        {
                            self.selection = PcbSelection::Airwire(airwire_index);
                            if self.layout.route_airwire(airwire_index) {
                                self.selection = PcbSelection::None;
                                document_changed = true;
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
                            document_changed = true;
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
        let cursor_mm = hover_pos
            .map(|p| {
                let w = self.screen_to_world(rect, p);
                format!("Cursor: {:.1}, {:.1} mm | ", w.x, w.y)
            })
            .unwrap_or_default();
        let info_text = format!("{}{}", cursor_mm, hint);
        if self.show_status {
            painter.text(
                Pos2::new(rect.left() + 12.0, rect.bottom() - 18.0),
                egui::Align2::LEFT_BOTTOM,
                info_text,
                egui::FontId::proportional(11.0),
                palette.text_muted,
            );
        }
        if self.show_minimap {
            self.draw_minimap(&painter, rect);
        }

        if document_changed {
            self.mark_document_changed();
        }

        changed
    }

    fn draw_minimap(&self, painter: &egui::Painter, canvas: Rect) {
        let palette = electronics_palette(self.canvas_dark_mode);
        let size = egui::Vec2::new(148.0, 96.0);
        let rect = Rect::from_min_size(
            Pos2::new(canvas.left() + 14.0, canvas.bottom() - size.y - 14.0),
            size,
        );
        painter.rect_filled(rect, 5.0, palette.overlay_bg);
        painter.rect_stroke(rect, 5.0, Stroke::new(1.0, theme::ACCENT));

        let mut points = self.layout.board_outline.points.clone();
        points.extend(
            self.layout
                .components
                .iter()
                .map(|component| component.position),
        );
        for trace in &self.layout.traces {
            points.extend(trace.points.iter().copied());
        }
        if points.is_empty() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                t("app.electronics_minimap", self.lang),
                egui::FontId::proportional(9.0),
                palette.text_muted,
            );
            return;
        }

        let mut min = points[0];
        let mut max = points[0];
        for point in points.iter().copied() {
            min = min.min(point);
            max = max.max(point);
        }
        min -= Vec2::splat(24.0);
        max += Vec2::splat(24.0);
        let span = (max - min).max(Vec2::splat(1.0));
        let content = rect.shrink2(egui::Vec2::splat(8.0));
        let scale = (content.width() / span.x).min(content.height() / span.y);
        let fitted_size = span * scale;
        let fitted_origin = content.center() - egui::vec2(fitted_size.x * 0.5, fitted_size.y * 0.5);
        let project = |world: Vec2| {
            Pos2::new(
                fitted_origin.x + (world.x - min.x) * scale,
                fitted_origin.y + (world.y - min.y) * scale,
            )
        };

        let outline = self
            .layout
            .board_outline
            .points
            .iter()
            .map(|point| project(*point))
            .collect::<Vec<_>>();
        for segment in outline.windows(2) {
            painter.line_segment(
                [segment[0], segment[1]],
                Stroke::new(1.2, Color32::from_rgb(101, 187, 126)),
            );
        }
        if self.layout.outline_is_closed() && outline.len() > 2 {
            if let (Some(first), Some(last)) = (outline.first(), outline.last()) {
                painter.line_segment(
                    [*last, *first],
                    Stroke::new(1.2, Color32::from_rgb(101, 187, 126)),
                );
            }
        }
        for trace in &self.layout.traces {
            for segment in trace.points.windows(2) {
                let color = if self.selection
                    == PcbSelection::Trace(
                        self.layout
                            .traces
                            .iter()
                            .position(|candidate| candidate.id == trace.id)
                            .unwrap_or(usize::MAX),
                    ) {
                    theme::ACCENT
                } else {
                    Color32::from_rgb(92, 208, 116)
                };
                painter.line_segment(
                    [project(segment[0]), project(segment[1])],
                    Stroke::new(1.0, color),
                );
            }
        }
        for (index, component) in self.layout.components.iter().enumerate() {
            painter.rect_filled(
                Rect::from_center_size(
                    project(component.position),
                    egui::Vec2::splat(if self.selection == PcbSelection::Component(index) {
                        6.0
                    } else {
                        4.0
                    }),
                ),
                1.5,
                if self.selection == PcbSelection::Component(index) {
                    Color32::from_rgb(255, 172, 64)
                } else {
                    theme::ACCENT
                },
            );
        }

        let (left, right, top, bottom) = self.visible_world_bounds(canvas.width(), canvas.height());
        let visible_rect = Rect::from_two_pos(
            project(Vec2::new(left, top)),
            project(Vec2::new(right, bottom)),
        )
        .intersect(content);
        if visible_rect.width() > 1.0 && visible_rect.height() > 1.0 {
            painter.rect_stroke(
                visible_rect,
                2.0,
                Stroke::new(1.0, Color32::from_rgba_premultiplied(255, 172, 64, 220)),
            );
        }
    }

    fn component_index_from_cad_region(&self, region: &CadSurfaceHitRegion) -> Option<usize> {
        if !matches!(region.kind, CadObjectKind::Component | CadObjectKind::Pad) {
            return None;
        }
        let id = region.source_id.as_deref()?;
        self.layout
            .components
            .iter()
            .position(|component| component.component_id.to_string() == id)
    }

    fn trace_index_from_cad_region(&self, region: &CadSurfaceHitRegion) -> Option<usize> {
        if region.kind != CadObjectKind::Trace {
            return None;
        }
        let id = region.source_id.as_deref()?;
        self.layout
            .traces
            .iter()
            .position(|trace| trace.id.to_string() == id)
    }

    fn airwire_index_from_cad_region(&self, region: &CadSurfaceHitRegion) -> Option<usize> {
        if region.kind != CadObjectKind::Airwire {
            return None;
        }
        region.id.strip_prefix("airwire:")?.parse::<usize>().ok()
    }

    fn cad_surface_selection(&self) -> CadSurfaceSelection {
        let mut selection = CadSurfaceSelection::default();

        match self.selection {
            PcbSelection::Component(index) => {
                if let Some(component) = self.layout.components.get(index) {
                    selection
                        .source_ids
                        .push(*component.component_id.as_bytes());
                }
            }
            PcbSelection::Trace(index) => {
                if let Some(trace) = self.layout.traces.get(index) {
                    selection.source_ids.push(*trace.id.as_bytes());
                }
            }
            PcbSelection::Airwire(index) => {
                if index < self.layout.airwires.len() {
                    selection.object_ids.push(format!("airwire:{index}"));
                }
            }
            PcbSelection::None => {}
        }

        selection
    }

    fn visible_world_bounds(&self, width: f32, height: f32) -> (f32, f32, f32, f32) {
        let zoom = self.zoom.max(0.001);
        let left = (-self.offset.x) / zoom;
        let right = (width - self.offset.x) / zoom;
        let top = (-self.offset.y) / zoom;
        let bottom = (height - self.offset.y) / zoom;
        (left, right, top, bottom)
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
        draw_detail_overlay: bool,
        hovered_component: Option<usize>,
    ) {
        for (index, component) in self.layout.components.iter().enumerate() {
            let selected = self.selection == PcbSelection::Component(index);
            let hovered = hovered_component == Some(index);
            // AGB already owns the scalable component backdrop. At overview
            // zoom there is no reason to rebuild footprints, atlas bounds,
            // and pad labels for every component when only selected/hovered
            // overlays need to remain interactive.
            if !draw_detail_overlay && !selected && !hovered {
                continue;
            }
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
            if !rect.intersects(visual_rect.expand(24.0)) {
                continue;
            }
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
