//! Visual node editor panel - no-code scripting through connected nodes.
//!
//! Features:
//! - Canvas with pan/zoom
//! - Node rendering with header, pins, and body
//! - Pin-to-pin connections drawn as bezier curves
//! - Node palette for adding new nodes
//! - Drag to connect pins
//! - Selection and deletion

use egui::{Color32, Pos2, Rect, Stroke, Ui, Vec2};
use raf_nodes::graph::NodeGraph;
use raf_nodes::node::{Node, NodeCategory, NodeId, PinDataType, PinKind};
use uuid::Uuid;

use crate::theme;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const NODE_WIDTH: f32 = 180.0;
const NODE_HEADER_HEIGHT: f32 = 28.0;
const PIN_ROW_HEIGHT: f32 = 22.0;
const PIN_RADIUS: f32 = 5.0;
const NODE_ROUNDING: f32 = 6.0;
const CONNECTION_THICKNESS: f32 = 2.5;

// ---------------------------------------------------------------------------
// Node Editor State
// ---------------------------------------------------------------------------

/// Interaction state for drag-connecting pins.
#[derive(Debug, Clone)]
struct DragConnection {
    from_node: NodeId,
    from_pin: Uuid,
    from_pos: Pos2,
    from_kind: PinKind,
}

/// Visual node editor panel state.
pub struct NodeEditorPanel {
    /// The node graph being edited.
    pub graph: NodeGraph,
    /// Canvas offset (panning).
    pub offset: Vec2,
    /// Canvas zoom level.
    pub zoom: f32,
    /// Currently selected node.
    pub selected_node: Option<NodeId>,
    /// Node being dragged.
    dragging_node: Option<NodeId>,
    /// Pin connection in progress.
    drag_connection: Option<DragConnection>,
    /// Whether to show the node palette.
    show_palette: bool,
    /// Position where the palette was opened (canvas coords).
    palette_pos: Pos2,
}

impl Default for NodeEditorPanel {
    fn default() -> Self {
        Self {
            graph: NodeGraph::new("Main"),
            offset: Vec2::ZERO,
            zoom: 1.0,
            selected_node: None,
            dragging_node: None,
            drag_connection: None,
            show_palette: false,
            palette_pos: Pos2::ZERO,
        }
    }
}

impl NodeEditorPanel {
    /// Draw the node editor.
    pub fn show(&mut self, ui: &mut Ui) {
        let available = ui.available_rect_before_wrap();
        let painter = ui.painter_at(available);

        // Background.
        painter.rect_filled(available, 0.0, Color32::from_rgb(22, 22, 28));

        // Draw grid.
        self.draw_grid(&painter, available);

        // Draw connections as bezier curves.
        let nodes_snapshot: Vec<Node> = self.graph.nodes.clone();
        let conns_snapshot: Vec<raf_nodes::graph::Connection> = self.graph.connections.clone();
        for conn in &conns_snapshot {
            self.draw_connection(&painter, available, conn, &nodes_snapshot);
        }

        // Draw active drag connection line.
        if let Some(drag) = &self.drag_connection {
            let mouse_pos = ui.input(|i| i.pointer.hover_pos().unwrap_or(drag.from_pos));
            Self::draw_bezier(
                &painter,
                drag.from_pos,
                mouse_pos,
                Color32::from_rgb(255, 200, 100),
                2.0,
            );
        }

        // Draw nodes (collect IDs first to avoid borrow issues).
        let node_data: Vec<(NodeId, [f32; 2], String, NodeCategory, Vec<raf_nodes::node::NodePin>)> =
            self.graph
                .nodes
                .iter()
                .map(|n| (n.id, n.position, n.name.clone(), n.category, n.pins.clone()))
                .collect();

        for (node_id, position, name, category, pins) in &node_data {
            self.draw_node_visual(ui, &painter, available, *node_id, *position, name, *category, pins);
        }

        // Handle canvas interaction.
        let response = ui.allocate_rect(available, egui::Sense::click_and_drag());

        // Right-click to open palette.
        if response.secondary_clicked() {
            self.show_palette = true;
            let mouse = ui.input(|i| i.pointer.hover_pos().unwrap_or(available.center()));
            self.palette_pos = self.screen_to_canvas(mouse, available);
        }

        // Middle-drag to pan.
        if response.dragged_by(egui::PointerButton::Middle) {
            self.offset += response.drag_delta();
        }

        // Left-click on empty space deselects.
        if response.clicked() && self.dragging_node.is_none() {
            self.selected_node = None;
            self.show_palette = false;
        }

        // Drop drag connection on empty space.
        if response.drag_stopped() {
            self.drag_connection = None;
            self.dragging_node = None;
        }

        // Scroll to zoom.
        if ui.rect_contains_pointer(available) {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.zoom = (self.zoom + scroll * 0.002).clamp(0.3, 3.0);
            }
        }

        // Delete key removes selected node.
        if ui.input(|i| i.key_pressed(egui::Key::Delete)) {
            self.delete_selected();
        }

        // Draw palette overlay.
        if self.show_palette {
            self.draw_palette(ui, available);
        }

        // Info overlay.
        painter.text(
            Pos2::new(available.left() + 10.0, available.top() + 10.0),
            egui::Align2::LEFT_TOP,
            format!(
                "Nodes: {} | Connections: {} | Right-click: Add Node | Del: Remove",
                self.graph.nodes.len(),
                self.graph.connections.len()
            ),
            egui::FontId::proportional(11.0),
            Color32::from_rgb(120, 120, 130),
        );
    }

    // -----------------------------------------------------------------------
    // Grid
    // -----------------------------------------------------------------------

    fn draw_grid(&self, painter: &egui::Painter, rect: Rect) {
        let grid_color = Color32::from_rgba_premultiplied(255, 255, 255, 8);
        let grid_major = Color32::from_rgba_premultiplied(255, 255, 255, 16);
        let step = 30.0 * self.zoom;
        if step < 3.0 {
            return;
        }

        let mut x = rect.left() + (self.offset.x % step);
        let mut ix = 0u32;
        while x < rect.right() {
            let c = if ix % 5 == 0 { grid_major } else { grid_color };
            painter.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(0.5, c),
            );
            x += step;
            ix += 1;
        }

        let mut y = rect.top() + (self.offset.y % step);
        let mut iy = 0u32;
        while y < rect.bottom() {
            let c = if iy % 5 == 0 { grid_major } else { grid_color };
            painter.line_segment(
                [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                Stroke::new(0.5, c),
            );
            y += step;
            iy += 1;
        }
    }

    // -----------------------------------------------------------------------
    // Node rendering
    // -----------------------------------------------------------------------

    fn draw_node_visual(
        &mut self,
        ui: &mut Ui,
        painter: &egui::Painter,
        canvas_rect: Rect,
        node_id: NodeId,
        position: [f32; 2],
        name: &str,
        category: NodeCategory,
        pins: &[raf_nodes::node::NodePin],
    ) {
        let pin_count = pins.len().max(1) as f32;
        let node_height = NODE_HEADER_HEIGHT + pin_count * PIN_ROW_HEIGHT + 8.0;
        let screen_pos =
            self.canvas_to_screen(Pos2::new(position[0], position[1]), canvas_rect);

        let node_rect = Rect::from_min_size(
            screen_pos,
            Vec2::new(NODE_WIDTH * self.zoom, node_height * self.zoom),
        );

        // Skip if off-screen.
        if !canvas_rect.intersects(node_rect) {
            return;
        }

        let is_selected = self.selected_node == Some(node_id);

        // Node body shadow.
        let shadow_rect = node_rect.translate(Vec2::new(3.0, 3.0));
        painter.rect_filled(
            shadow_rect,
            NODE_ROUNDING * self.zoom,
            Color32::from_rgba_premultiplied(0, 0, 0, 60),
        );

        // Node body.
        painter.rect_filled(
            node_rect,
            NODE_ROUNDING * self.zoom,
            Color32::from_rgb(38, 38, 46),
        );

        // Header with category color.
        let cat_color = category_color(category);
        let header_rect = Rect::from_min_size(
            node_rect.min,
            Vec2::new(node_rect.width(), NODE_HEADER_HEIGHT * self.zoom),
        );
        painter.rect_filled(
            header_rect,
            egui::Rounding {
                nw: NODE_ROUNDING * self.zoom,
                ne: NODE_ROUNDING * self.zoom,
                sw: 0.0,
                se: 0.0,
            },
            cat_color,
        );

        // Node name.
        painter.text(
            header_rect.center(),
            egui::Align2::CENTER_CENTER,
            name,
            egui::FontId::proportional(12.0 * self.zoom),
            Color32::WHITE,
        );

        // Selection border.
        if is_selected {
            painter.rect_stroke(
                node_rect,
                NODE_ROUNDING * self.zoom,
                Stroke::new(2.0, theme::ACCENT),
            );
        }

        // Draw pins.
        for (i, pin) in pins.iter().enumerate() {
            let y_offset = NODE_HEADER_HEIGHT + (i as f32 + 0.5) * PIN_ROW_HEIGHT;
            let pin_y = screen_pos.y + y_offset * self.zoom;

            let pin_x = match pin.kind {
                PinKind::Input => node_rect.left(),
                PinKind::Output => node_rect.right(),
            };

            let pin_center = Pos2::new(pin_x, pin_y);
            let pin_color = pin_data_type_color(pin.data_type);

            // Pin circle.
            painter.circle_filled(pin_center, PIN_RADIUS * self.zoom, pin_color);
            painter.circle_stroke(
                pin_center,
                PIN_RADIUS * self.zoom,
                Stroke::new(1.0, Color32::from_rgb(200, 200, 210)),
            );

            // Pin label.
            let (text_offset, text_align) = match pin.kind {
                PinKind::Input => (12.0 * self.zoom, egui::Align2::LEFT_CENTER),
                PinKind::Output => (-12.0 * self.zoom, egui::Align2::RIGHT_CENTER),
            };
            painter.text(
                Pos2::new(pin_x + text_offset, pin_y),
                text_align,
                &pin.name,
                egui::FontId::proportional(10.0 * self.zoom),
                Color32::from_rgb(200, 200, 210),
            );

            // Pin interaction area.
            let pin_hit = Rect::from_center_size(
                pin_center,
                Vec2::splat(PIN_RADIUS * 3.0 * self.zoom),
            );
            let pin_resp = ui.allocate_rect(pin_hit, egui::Sense::click_and_drag());

            if pin_resp.drag_started() {
                self.drag_connection = Some(DragConnection {
                    from_node: node_id,
                    from_pin: pin.id,
                    from_pos: pin_center,
                    from_kind: pin.kind,
                });
            }

            // Drop connection on this pin.
            if pin_resp.hovered() && ui.input(|i| i.pointer.any_released()) {
                if let Some(drag) = self.drag_connection.take() {
                    if drag.from_kind != pin.kind && drag.from_node != node_id {
                        match drag.from_kind {
                            PinKind::Output => {
                                self.graph
                                    .connect(drag.from_node, drag.from_pin, node_id, pin.id);
                            }
                            PinKind::Input => {
                                self.graph
                                    .connect(node_id, pin.id, drag.from_node, drag.from_pin);
                            }
                        }
                    }
                }
            }
        }

        // Header drag = move node / select.
        let header_resp = ui.allocate_rect(header_rect, egui::Sense::click_and_drag());
        if header_resp.clicked() {
            self.selected_node = Some(node_id);
        }
        if header_resp.dragged() {
            self.dragging_node = Some(node_id);
            let delta = header_resp.drag_delta() / self.zoom;
            if let Some(n) = self.graph.nodes.iter_mut().find(|n| n.id == node_id) {
                n.position[0] += delta.x;
                n.position[1] += delta.y;
            }
        }
    }

    // -----------------------------------------------------------------------
    // Connections
    // -----------------------------------------------------------------------

    fn draw_connection(
        &self,
        painter: &egui::Painter,
        canvas_rect: Rect,
        conn: &raf_nodes::graph::Connection,
        nodes: &[Node],
    ) {
        let from_pos = self.find_pin_screen_pos(canvas_rect, conn.from_node, conn.from_pin, nodes);
        let to_pos = self.find_pin_screen_pos(canvas_rect, conn.to_node, conn.to_pin, nodes);

        if let (Some(from), Some(to)) = (from_pos, to_pos) {
            let color = self.pin_color_from_nodes(conn.from_node, conn.from_pin, nodes);
            Self::draw_bezier(painter, from, to, color, CONNECTION_THICKNESS);
        }
    }

    fn draw_bezier(
        painter: &egui::Painter,
        from: Pos2,
        to: Pos2,
        color: Color32,
        thickness: f32,
    ) {
        let dx = (to.x - from.x).abs() * 0.5;
        let cp1 = Pos2::new(from.x + dx, from.y);
        let cp2 = Pos2::new(to.x - dx, to.y);

        let segments = 24;
        let mut points = Vec::with_capacity(segments + 1);
        for i in 0..=segments {
            let t = i as f32 / segments as f32;
            let it = 1.0 - t;
            let x = it * it * it * from.x
                + 3.0 * it * it * t * cp1.x
                + 3.0 * it * t * t * cp2.x
                + t * t * t * to.x;
            let y = it * it * it * from.y
                + 3.0 * it * it * t * cp1.y
                + 3.0 * it * t * t * cp2.y
                + t * t * t * to.y;
            points.push(Pos2::new(x, y));
        }

        for w in points.windows(2) {
            painter.line_segment([w[0], w[1]], Stroke::new(thickness, color));
        }
    }

    fn find_pin_screen_pos(
        &self,
        canvas_rect: Rect,
        node_id: NodeId,
        pin_id: Uuid,
        nodes: &[Node],
    ) -> Option<Pos2> {
        let node = nodes.iter().find(|n| n.id == node_id)?;
        let (pin_index, pin) = node.pins.iter().enumerate().find(|(_, p)| p.id == pin_id)?;

        let screen_pos =
            self.canvas_to_screen(Pos2::new(node.position[0], node.position[1]), canvas_rect);

        let y_offset = NODE_HEADER_HEIGHT + (pin_index as f32 + 0.5) * PIN_ROW_HEIGHT;
        let pin_y = screen_pos.y + y_offset * self.zoom;
        let pin_x = match pin.kind {
            PinKind::Input => screen_pos.x,
            PinKind::Output => screen_pos.x + NODE_WIDTH * self.zoom,
        };

        Some(Pos2::new(pin_x, pin_y))
    }

    fn pin_color_from_nodes(&self, node_id: NodeId, pin_id: Uuid, nodes: &[Node]) -> Color32 {
        if let Some(node) = nodes.iter().find(|n| n.id == node_id) {
            if let Some(pin) = node.pins.iter().find(|p| p.id == pin_id) {
                return pin_data_type_color(pin.data_type);
            }
        }
        Color32::from_rgb(180, 180, 190)
    }

    // -----------------------------------------------------------------------
    // Palette
    // -----------------------------------------------------------------------

    fn draw_palette(&mut self, ui: &mut Ui, canvas_rect: Rect) {
        let palette_screen = self.canvas_to_screen(self.palette_pos, canvas_rect);

        let node_templates: Vec<(&str, &str, fn() -> Node)> = vec![
            ("On Start", "Event", Node::on_start),
            ("On Update", "Event", Node::on_update),
            ("Print", "Action", Node::print_action),
            ("If Branch", "Logic", Node::if_branch),
            ("Add", "Math", Node::add_math),
        ];

        let palette_height = 44.0 + node_templates.len() as f32 * 32.0;
        let palette_rect =
            Rect::from_min_size(palette_screen, Vec2::new(200.0, palette_height));

        let painter = ui.painter_at(palette_rect);
        painter.rect_filled(palette_rect, 8.0, Color32::from_rgb(30, 30, 38));
        painter.rect_stroke(
            palette_rect,
            8.0,
            Stroke::new(1.0, Color32::from_rgb(60, 60, 68)),
        );

        painter.text(
            Pos2::new(palette_rect.center().x, palette_rect.top() + 16.0),
            egui::Align2::CENTER_CENTER,
            "Add Node",
            egui::FontId::proportional(13.0),
            theme::ACCENT,
        );

        let mut y = palette_rect.top() + 36.0;
        for (label, cat_label, factory) in node_templates {
            let btn_rect = Rect::from_min_size(
                Pos2::new(palette_rect.left() + 8.0, y),
                Vec2::new(palette_rect.width() - 16.0, 26.0),
            );

            let resp = ui.allocate_rect(btn_rect, egui::Sense::click());
            let bg = if resp.hovered() {
                Color32::from_rgb(50, 50, 58)
            } else {
                Color32::from_rgb(38, 38, 46)
            };
            painter.rect_filled(btn_rect, 4.0, bg);

            painter.text(
                Pos2::new(btn_rect.left() + 8.0, btn_rect.center().y),
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(12.0),
                Color32::from_rgb(220, 220, 230),
            );
            painter.text(
                Pos2::new(btn_rect.right() - 8.0, btn_rect.center().y),
                egui::Align2::RIGHT_CENTER,
                cat_label,
                egui::FontId::proportional(10.0),
                Color32::from_rgb(100, 100, 110),
            );

            if resp.clicked() {
                let mut node = factory();
                node.position = [self.palette_pos.x, self.palette_pos.y];
                self.graph.add_node(node);
                self.show_palette = false;
            }

            y += 32.0;
        }
    }

    // -----------------------------------------------------------------------
    // Coordinate conversion
    // -----------------------------------------------------------------------

    fn canvas_to_screen(&self, canvas_pos: Pos2, canvas_rect: Rect) -> Pos2 {
        Pos2::new(
            canvas_rect.left() + canvas_pos.x * self.zoom + self.offset.x,
            canvas_rect.top() + canvas_pos.y * self.zoom + self.offset.y,
        )
    }

    fn screen_to_canvas(&self, screen_pos: Pos2, canvas_rect: Rect) -> Pos2 {
        Pos2::new(
            (screen_pos.x - canvas_rect.left() - self.offset.x) / self.zoom,
            (screen_pos.y - canvas_rect.top() - self.offset.y) / self.zoom,
        )
    }

    /// Delete the currently selected node and its connections.
    pub fn delete_selected(&mut self) {
        if let Some(id) = self.selected_node.take() {
            self.graph.remove_node(id);
        }
    }
}

// ---------------------------------------------------------------------------
// Color helpers
// ---------------------------------------------------------------------------

fn category_color(category: NodeCategory) -> Color32 {
    let c = category.color();
    Color32::from_rgba_unmultiplied(
        (c[0] * 255.0) as u8,
        (c[1] * 255.0) as u8,
        (c[2] * 255.0) as u8,
        (c[3] * 255.0) as u8,
    )
}

fn pin_data_type_color(dt: PinDataType) -> Color32 {
    match dt {
        PinDataType::Flow => Color32::from_rgb(220, 220, 230),
        PinDataType::Bool => Color32::from_rgb(180, 60, 60),
        PinDataType::Int => Color32::from_rgb(60, 180, 180),
        PinDataType::Float => Color32::from_rgb(100, 200, 100),
        PinDataType::String => Color32::from_rgb(200, 100, 200),
        PinDataType::Vec3 => Color32::from_rgb(240, 200, 60),
        PinDataType::Any => Color32::from_rgb(160, 160, 170),
    }
}
