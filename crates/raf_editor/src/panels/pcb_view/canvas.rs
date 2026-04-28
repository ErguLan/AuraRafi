use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui};
use glam::Vec2;
use raf_core::i18n::t;
use raf_electronics::footprint_definition;

use super::{PcbSelection, PcbTool, PcbViewPanel};

const GRID_STEP: f32 = 20.0;

impl PcbViewPanel {
    pub(super) fn draw_canvas(&mut self, ui: &mut Ui, rect: Rect) -> bool {
        let response = ui.allocate_rect(rect, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        let mut changed = false;

        painter.rect_filled(rect, 6.0, Color32::from_rgb(17, 17, 20));
        self.draw_grid(&painter, rect);
        self.draw_board(&painter, rect);
        self.draw_traces(&painter, rect);
        if self.show_airwires {
            self.draw_airwires(&painter, rect);
        }
        self.draw_components(&painter, rect);
        self.draw_outline_draft(&painter, rect);

        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                self.zoom = (self.zoom + scroll * 0.003).clamp(0.35, 4.0);
            }
        }

        if response.dragged_by(egui::PointerButton::Middle) || response.dragged_by(egui::PointerButton::Secondary) {
            let delta = ui.input(|i| i.pointer.delta());
            self.offset += delta;
        }

        let pointer_pos = ui.input(|i| i.pointer.interact_pos());
        let primary_clicked = response.clicked_by(egui::PointerButton::Primary);
        let secondary_clicked = response.clicked_by(egui::PointerButton::Secondary);

        match self.tool {
            PcbTool::Select => {
                if primary_clicked {
                    if let Some(pointer) = pointer_pos {
                        let world = self.snap_world(rect, self.screen_to_world(rect, pointer));
                        if let Some(component_index) = self.hit_component(rect, pointer) {
                            self.selection = PcbSelection::Component(component_index);
                            if let Some(component) = self.layout.components.get(component_index) {
                                self.drag_state = Some((component_index, component.position - world));
                            }
                        } else if let Some(trace_index) = self.hit_trace(rect, pointer) {
                            self.selection = PcbSelection::Trace(trace_index);
                            self.drag_state = None;
                        } else if let Some(airwire_index) = self.hit_airwire(rect, pointer) {
                            self.selection = PcbSelection::Airwire(airwire_index);
                            self.drag_state = None;
                        } else {
                            self.selection = PcbSelection::None;
                            self.drag_state = None;
                        }
                    }
                }

                if let Some((component_index, anchor)) = self.drag_state {
                    if ui.input(|i| i.pointer.primary_down()) {
                        if let Some(pointer) = pointer_pos {
                            let snapped = self.snap_world(
                                rect,
                                self.screen_to_world(rect, pointer) + anchor,
                            );
                            if let Some(component) = self.layout.components.get_mut(component_index) {
                                if !component.locked {
                                    component.position = snapped;
                                }
                            }
                        }
                    } else {
                        self.drag_state = None;
                        changed = true;
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
                            && self.outline_draft.first().map(|first| first.distance(world) <= GRID_STEP * 0.5).unwrap_or(false)
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
                }
            }
        }

        let hint = match self.tool {
            PcbTool::Select => t("app.pcb_canvas_hint", self.lang),
            PcbTool::Route => t("app.pcb_route_hint", self.lang),
            PcbTool::Outline => t("app.pcb_outline_hint", self.lang),
        };
        painter.text(
            Pos2::new(rect.left() + 12.0, rect.bottom() - 18.0),
            egui::Align2::LEFT_BOTTOM,
            hint,
            egui::FontId::proportional(11.0),
            Color32::from_rgb(140, 140, 150),
        );

        changed
    }

    fn draw_grid(&self, painter: &egui::Painter, rect: Rect) {
        let spacing = GRID_STEP * self.zoom.max(0.1);
        if spacing < 8.0 {
            return;
        }

        let start_x = ((rect.left() + self.offset.x) % spacing + spacing) % spacing;
        let start_y = ((rect.top() + self.offset.y) % spacing + spacing) % spacing;

        let grid_stroke = Stroke::new(1.0, Color32::from_rgb(27, 27, 32));
        let mut x = rect.left() + start_x;
        while x <= rect.right() {
            painter.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], grid_stroke);
            x += spacing;
        }

        let mut y = rect.top() + start_y;
        while y <= rect.bottom() {
            painter.line_segment([Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)], grid_stroke);
            y += spacing;
        }
    }

    fn draw_board(&self, painter: &egui::Painter, rect: Rect) {
        if self.layout.board_outline.points.len() < 2 {
            return;
        }

        let board_points = self
            .layout
            .board_outline
            .points
            .iter()
            .map(|point| self.world_to_screen(rect, *point))
            .collect::<Vec<_>>();

        if self.layout.outline_is_closed() && board_points.len() >= 4 {
            painter.add(egui::Shape::convex_polygon(
                board_points.clone(),
                Color32::from_rgb(23, 44, 28),
                Stroke::NONE,
            ));
        }

        for pair in board_points.windows(2) {
            painter.line_segment([pair[0], pair[1]], Stroke::new(2.0, Color32::from_rgb(178, 198, 130)));
        }
    }

    fn draw_components(&self, painter: &egui::Painter, rect: Rect) {
        for (index, component) in self.layout.components.iter().enumerate() {
            let footprint = footprint_definition(&component.footprint, component.pad_nets.len().max(1));
            let center = self.world_to_screen(rect, component.position);
            let body_size = egui::vec2(footprint.body_size.x * self.zoom, footprint.body_size.y * self.zoom);
            let body_rect = Rect::from_center_size(center, body_size);
            let selected = self.selection == PcbSelection::Component(index);

            painter.rect(
                body_rect,
                6.0,
                Color32::from_rgb(40, 40, 46),
                Stroke::new(
                    if selected { 2.0 } else { 1.0 },
                    if selected {
                        Color32::from_rgb(225, 160, 70)
                    } else {
                        Color32::from_rgb(70, 72, 82)
                    },
                ),
            );

            for (pad_index, pad) in footprint.pads.iter().enumerate() {
                if let Some(world) = self.layout.pad_world_position(index, pad_index) {
                    let pad_center = self.world_to_screen(rect, world);
                    let pad_rect = Rect::from_center_size(
                        pad_center,
                        egui::vec2(pad.size.x * self.zoom, pad.size.y * self.zoom),
                    );
                    painter.rect_filled(pad_rect, 3.0, Color32::from_rgb(210, 170, 96));
                }
            }

            painter.text(
                body_rect.center_top() + egui::vec2(0.0, -6.0),
                egui::Align2::CENTER_BOTTOM,
                &component.designator,
                egui::FontId::proportional(11.0),
                Color32::WHITE,
            );
            painter.text(
                body_rect.center_bottom() + egui::vec2(0.0, 4.0),
                egui::Align2::CENTER_TOP,
                &component.footprint,
                egui::FontId::proportional(10.0),
                Color32::from_rgb(180, 180, 190),
            );
        }
    }

    fn draw_traces(&self, painter: &egui::Painter, rect: Rect) {
        for (index, trace) in self.layout.traces.iter().enumerate() {
            let color = match trace.layer {
                raf_electronics::PcbLayer::TopCopper => Color32::from_rgb(224, 120, 72),
                raf_electronics::PcbLayer::BottomCopper => Color32::from_rgb(84, 172, 214),
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
                painter.line_segment(
                    [self.world_to_screen(rect, pair[0]), self.world_to_screen(rect, pair[1])],
                    stroke,
                );
            }
        }
    }

    fn draw_airwires(&self, painter: &egui::Painter, rect: Rect) {
        for (index, airwire) in self.layout.airwires.iter().enumerate() {
            let selected = self.selection == PcbSelection::Airwire(index);
            painter.line_segment(
                [self.world_to_screen(rect, airwire.from), self.world_to_screen(rect, airwire.to)],
                Stroke::new(
                    if selected { 2.0 } else { 1.0 },
                    if selected {
                        Color32::from_rgb(255, 220, 120)
                    } else {
                        Color32::from_rgb(170, 170, 60)
                    },
                ),
            );
        }
    }

    fn draw_outline_draft(&self, painter: &egui::Painter, rect: Rect) {
        if self.outline_draft.is_empty() {
            return;
        }

        let stroke = Stroke::new(1.5, Color32::from_rgb(250, 200, 120));
        for pair in self.outline_draft.windows(2) {
            painter.line_segment(
                [self.world_to_screen(rect, pair[0]), self.world_to_screen(rect, pair[1])],
                stroke,
            );
        }

        for point in &self.outline_draft {
            painter.circle_filled(self.world_to_screen(rect, *point), 3.0, Color32::from_rgb(250, 200, 120));
        }
    }

    fn hit_component(&self, rect: Rect, pointer: Pos2) -> Option<usize> {
        for (index, component) in self.layout.components.iter().enumerate().rev() {
            let footprint = footprint_definition(&component.footprint, component.pad_nets.len().max(1));
            let body_rect = Rect::from_center_size(
                self.world_to_screen(rect, component.position),
                egui::vec2(footprint.body_size.x * self.zoom, footprint.body_size.y * self.zoom),
            );
            if body_rect.expand(6.0).contains(pointer) {
                return Some(index);
            }
        }
        None
    }

    fn hit_trace(&self, rect: Rect, pointer: Pos2) -> Option<usize> {
        for (index, trace) in self.layout.traces.iter().enumerate().rev() {
            for pair in trace.points.windows(2) {
                let start = self.world_to_screen(rect, pair[0]);
                let end = self.world_to_screen(rect, pair[1]);
                if distance_to_segment(pointer, start, end) <= 6.0 {
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
            if distance_to_segment(pointer, start, end) <= 6.0 {
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