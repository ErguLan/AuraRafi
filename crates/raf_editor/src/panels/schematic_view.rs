//! Schematic view panel - visual editor for electronic schematics.
//!
//! Renders electronic components, wires, and pins on a 2D canvas
//! with grid snapping, pan/zoom, and component placement.

use egui::{Color32, Pos2, Rect, Stroke, Ui, Vec2};
use raf_electronics::component::{ElectronicComponent, PinDirection};
use raf_electronics::library::ComponentLibrary;
use raf_electronics::schematic::Schematic;

use crate::theme;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const GRID_STEP: f32 = 20.0;
const COMP_BODY_W: f32 = 60.0;
const COMP_BODY_H: f32 = 30.0;
const PIN_DOT_RADIUS: f32 = 3.0;

// ---------------------------------------------------------------------------
// Panel state
// ---------------------------------------------------------------------------

/// Placement mode for interactive component placement.
#[derive(Debug, Clone, PartialEq)]
enum PlacementMode {
    None,
    Component(usize), // index into library
    Wire,
}

/// State for the schematic view panel.
pub struct SchematicViewPanel {
    /// Active schematic.
    pub schematic: Schematic,
    /// Component library.
    pub library: ComponentLibrary,
    /// Canvas offset (panning).
    pub offset: Vec2,
    /// Canvas zoom level.
    pub zoom: f32,
    /// Currently selected component index.
    pub selected_component: Option<usize>,
    /// Current placement mode.
    placement: PlacementMode,
    /// Wire drawing start point.
    wire_start: Option<Pos2>,
    /// Whether to show the component library sidebar.
    show_library: bool,
    /// Last test results.
    test_results: Vec<String>,
    /// Whether to show test results overlay.
    show_test_results: bool,
}

impl Default for SchematicViewPanel {
    fn default() -> Self {
        Self {
            schematic: Schematic::new("Untitled"),
            library: ComponentLibrary::default_library(),
            offset: Vec2::new(200.0, 150.0),
            zoom: 1.0,
            selected_component: None,
            placement: PlacementMode::None,
            wire_start: None,
            show_library: true,
            test_results: Vec::new(),
            show_test_results: false,
        }
    }
}

impl SchematicViewPanel {
    /// Draw the schematic view.
    pub fn show(&mut self, ui: &mut Ui) {
        // Top toolbar.
        self.draw_toolbar(ui);

        ui.separator();

        // Main area: library sidebar + canvas.
        let available = ui.available_rect_before_wrap();

        if self.show_library {
            // Library takes 180px on the left.
            let lib_width = 180.0;
            let lib_rect = Rect::from_min_size(
                available.left_top(),
                Vec2::new(lib_width, available.height()),
            );
            let canvas_rect = Rect::from_min_max(
                Pos2::new(available.left() + lib_width + 2.0, available.top()),
                available.right_bottom(),
            );

            self.draw_library(ui, lib_rect);
            self.draw_canvas(ui, canvas_rect);
        } else {
            self.draw_canvas(ui, available);
        }
    }

    // -----------------------------------------------------------------------
    // Toolbar
    // -----------------------------------------------------------------------

    fn draw_toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            // Library toggle.
            let lib_label = if self.show_library {
                "Hide Library"
            } else {
                "Show Library"
            };
            if ui.button(lib_label).clicked() {
                self.show_library = !self.show_library;
            }

            ui.separator();

            // Wire mode toggle.
            let wire_active = self.placement == PlacementMode::Wire;
            if ui
                .selectable_label(wire_active, "Draw Wire")
                .clicked()
            {
                if wire_active {
                    self.placement = PlacementMode::None;
                    self.wire_start = None;
                } else {
                    self.placement = PlacementMode::Wire;
                }
            }

            ui.separator();

            // Electrical test button.
            if ui
                .button(
                    egui::RichText::new("Electrical Test")
                        .color(Color32::WHITE),
                )
                .clicked()
            {
                self.test_results = self.schematic.electrical_test();
                self.show_test_results = true;
            }

            ui.separator();

            // Info.
            ui.label(
                egui::RichText::new(format!(
                    "Components: {} | Wires: {}",
                    self.schematic.components.len(),
                    self.schematic.wires.len()
                ))
                .small(),
            );

            if self.placement != PlacementMode::None {
                ui.separator();
                let mode_text = match &self.placement {
                    PlacementMode::Component(idx) => {
                        if let Some(tmpl) = self.library.components.get(*idx) {
                            format!("Placing: {}", tmpl.name)
                        } else {
                            "Placing...".to_string()
                        }
                    }
                    PlacementMode::Wire => "Drawing Wire (click to place points)".to_string(),
                    PlacementMode::None => String::new(),
                };
                ui.label(
                    egui::RichText::new(mode_text).color(theme::ACCENT),
                );

                if ui.button("Cancel (Esc)").clicked() {
                    self.placement = PlacementMode::None;
                    self.wire_start = None;
                }
            }
        });
    }

    // -----------------------------------------------------------------------
    // Component Library Sidebar
    // -----------------------------------------------------------------------

    fn draw_library(&mut self, ui: &mut Ui, rect: Rect) {
        let painter = ui.painter_at(rect);

        // Background.
        painter.rect_filled(rect, 0.0, Color32::from_rgb(26, 26, 34));
        painter.line_segment(
            [rect.right_top(), rect.right_bottom()],
            Stroke::new(1.0, Color32::from_rgb(50, 50, 58)),
        );

        // Title.
        painter.text(
            Pos2::new(rect.center().x, rect.top() + 16.0),
            egui::Align2::CENTER_CENTER,
            "Component Library",
            egui::FontId::proportional(12.0),
            theme::ACCENT,
        );

        let mut y = rect.top() + 36.0;
        let template_count = self.library.components.len();
        for idx in 0..template_count {
            let name = self.library.components[idx].name.clone();
            let category = self.library.components[idx].category.clone();
            let desc = self.library.components[idx].description.clone();

            let btn_rect = Rect::from_min_size(
                Pos2::new(rect.left() + 6.0, y),
                Vec2::new(rect.width() - 12.0, 48.0),
            );

            let resp = ui.allocate_rect(btn_rect, egui::Sense::click());
            let is_selected = self.placement == PlacementMode::Component(idx);
            let bg = if is_selected {
                Color32::from_rgb(60, 45, 25)
            } else if resp.hovered() {
                Color32::from_rgb(42, 42, 50)
            } else {
                Color32::from_rgb(34, 34, 42)
            };

            painter.rect_filled(btn_rect, 4.0, bg);

            if is_selected {
                painter.rect_stroke(btn_rect, 4.0, Stroke::new(1.0, theme::ACCENT));
            }

            painter.text(
                Pos2::new(btn_rect.left() + 8.0, btn_rect.top() + 14.0),
                egui::Align2::LEFT_CENTER,
                &name,
                egui::FontId::proportional(12.0),
                Color32::from_rgb(220, 220, 230),
            );

            painter.text(
                Pos2::new(btn_rect.left() + 8.0, btn_rect.top() + 32.0),
                egui::Align2::LEFT_CENTER,
                &format!("{} - {}", category, desc),
                egui::FontId::proportional(9.0),
                Color32::from_rgb(120, 120, 130),
            );

            if resp.clicked() {
                self.placement = PlacementMode::Component(idx);
            }

            y += 54.0;
        }
    }

    // -----------------------------------------------------------------------
    // Canvas
    // -----------------------------------------------------------------------

    fn draw_canvas(&mut self, ui: &mut Ui, rect: Rect) {
        // -- Painting phase: all read-only drawing --
        {
            let painter = ui.painter_at(rect);

            // Background.
            painter.rect_filled(rect, 0.0, Color32::from_rgb(18, 18, 24));

            // Draw grid.
            self.draw_grid(&painter, rect);

            // Draw wires.
            for wire in &self.schematic.wires {
                let start = self.world_to_screen(
                    Pos2::new(wire.start.x, wire.start.y),
                    rect,
                );
                let end = self.world_to_screen(
                    Pos2::new(wire.end.x, wire.end.y),
                    rect,
                );
                painter.line_segment(
                    [start, end],
                    Stroke::new(2.0 * self.zoom, Color32::from_rgb(60, 200, 60)),
                );

                // Net label.
                if !wire.net.is_empty() {
                    let mid = Pos2::new((start.x + end.x) / 2.0, (start.y + end.y) / 2.0 - 8.0);
                    painter.text(
                        mid,
                        egui::Align2::CENTER_BOTTOM,
                        &wire.net,
                        egui::FontId::proportional(9.0 * self.zoom),
                        Color32::from_rgb(100, 220, 100),
                    );
                }
            }

            // Draw components.
            for (idx, comp) in self.schematic.components.iter().enumerate() {
                let is_selected = self.selected_component == Some(idx);
                self.draw_component(&painter, rect, comp, is_selected);
            }

            // Draw wire-in-progress.
            if let (PlacementMode::Wire, Some(start)) = (&self.placement, self.wire_start) {
                let start_screen = self.world_to_screen(start, rect);
                let mouse = ui.input(|i| i.pointer.hover_pos().unwrap_or(start_screen));
                let snapped = self.snap_to_grid(self.screen_to_world(mouse, rect));
                let end_screen = self.world_to_screen(snapped, rect);
                painter.line_segment(
                    [start_screen, end_screen],
                    Stroke::new(2.0 * self.zoom, Color32::from_rgb(60, 200, 60)),
                );
            }

            // Draw placement preview.
            if let PlacementMode::Component(_idx) = &self.placement {
                if let Some(mouse) = ui.input(|i| i.pointer.hover_pos()) {
                    if rect.contains(mouse) {
                        let world = self.snap_to_grid(self.screen_to_world(mouse, rect));
                        let screen = self.world_to_screen(world, rect);
                        // Ghost preview.
                        let body = Rect::from_center_size(
                            screen,
                            Vec2::new(COMP_BODY_W * self.zoom, COMP_BODY_H * self.zoom),
                        );
                        painter.rect_filled(
                            body,
                            4.0 * self.zoom,
                            Color32::from_rgba_premultiplied(212, 119, 26, 60),
                        );
                        painter.rect_stroke(
                            body,
                            4.0 * self.zoom,
                            Stroke::new(1.5, Color32::from_rgba_premultiplied(212, 119, 26, 120)),
                        );
                    }
                }
            }

            // Test results overlay.
            if self.show_test_results && !self.test_results.is_empty() {
                self.draw_test_results(&painter, rect);
            }

            // Info overlay.
            painter.text(
                Pos2::new(rect.left() + 10.0, rect.top() + 10.0),
                egui::Align2::LEFT_TOP,
                format!("Zoom: {:.1}x | Esc: Cancel | Del: Remove", self.zoom),
                egui::FontId::proportional(11.0),
                Color32::from_rgb(100, 100, 110),
            );
        } // painter dropped here

        // -- Interaction phase: mutable borrows of ui --
        let resp = ui.allocate_rect(rect, egui::Sense::click_and_drag());

        // Left-click: place component or wire point.
        if resp.clicked() {
            if let Some(mouse) = ui.input(|i| i.pointer.hover_pos()) {
                let world = self.snap_to_grid(self.screen_to_world(mouse, rect));

                match &self.placement {
                    PlacementMode::Component(idx) => {
                        let idx_val = *idx;
                        if let Some(tmpl) = self.library.components.get(idx_val) {
                            let mut comp = (tmpl.create)();
                            comp.position = glam::Vec2::new(world.x, world.y);
                            self.schematic.add_component(comp);
                        }
                    }
                    PlacementMode::Wire => {
                        if let Some(start) = self.wire_start.take() {
                            self.schematic.add_wire(
                                glam::Vec2::new(start.x, start.y),
                                glam::Vec2::new(world.x, world.y),
                                "",
                            );
                        } else {
                            self.wire_start = Some(world);
                        }
                    }
                    PlacementMode::None => {
                        self.selected_component = self.hit_test_component(mouse, rect);
                    }
                }
            }
        }

        // Middle-drag to pan.
        if resp.dragged_by(egui::PointerButton::Middle) {
            self.offset += resp.drag_delta();
        }

        // Right-click to pan too.
        if resp.dragged_by(egui::PointerButton::Secondary) {
            self.offset += resp.drag_delta();
        }

        // Scroll to zoom.
        if ui.rect_contains_pointer(rect) {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.zoom = (self.zoom + scroll * 0.002).clamp(0.3, 4.0);
            }
        }

        // Escape to cancel placement.
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.placement = PlacementMode::None;
            self.wire_start = None;
        }

        // Delete selected component.
        if ui.input(|i| i.key_pressed(egui::Key::Delete)) {
            if let Some(idx) = self.selected_component.take() {
                if idx < self.schematic.components.len() {
                    self.schematic.components.remove(idx);
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Grid
    // -----------------------------------------------------------------------

    fn draw_grid(&self, painter: &egui::Painter, rect: Rect) {
        let grid_color = Color32::from_rgba_premultiplied(255, 255, 255, 10);
        let grid_major = Color32::from_rgba_premultiplied(255, 255, 255, 20);
        let step = GRID_STEP * self.zoom;
        if step < 4.0 {
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
    // Component rendering
    // -----------------------------------------------------------------------

    fn draw_component(
        &self,
        painter: &egui::Painter,
        canvas_rect: Rect,
        comp: &ElectronicComponent,
        is_selected: bool,
    ) {
        let center = self.world_to_screen(
            Pos2::new(comp.position.x, comp.position.y),
            canvas_rect,
        );

        let body_w = COMP_BODY_W * self.zoom;
        let body_h = COMP_BODY_H * self.zoom;
        let body = Rect::from_center_size(center, Vec2::new(body_w, body_h));

        // Component body.
        let body_color = if is_selected {
            Color32::from_rgb(50, 45, 35)
        } else {
            Color32::from_rgb(35, 35, 45)
        };
        painter.rect_filled(body, 3.0 * self.zoom, body_color);

        let border_color = if is_selected {
            theme::ACCENT
        } else {
            Color32::from_rgb(80, 80, 92)
        };
        painter.rect_stroke(body, 3.0 * self.zoom, Stroke::new(1.5, border_color));

        // Designator (top).
        painter.text(
            Pos2::new(center.x, body.top() - 4.0 * self.zoom),
            egui::Align2::CENTER_BOTTOM,
            &comp.designator,
            egui::FontId::proportional(10.0 * self.zoom),
            theme::ACCENT,
        );

        // Value (center).
        painter.text(
            center,
            egui::Align2::CENTER_CENTER,
            &comp.value,
            egui::FontId::proportional(10.0 * self.zoom),
            Color32::from_rgb(200, 200, 210),
        );

        // Draw pins.
        for pin in &comp.pins {
            let pin_world = Pos2::new(
                comp.position.x + pin.offset.x * GRID_STEP,
                comp.position.y + pin.offset.y * GRID_STEP,
            );
            let pin_screen = self.world_to_screen(pin_world, canvas_rect);

            // Pin line from body edge to pin position.
            let body_edge = if pin.offset.x < 0.0 {
                Pos2::new(body.left(), center.y + pin.offset.y * GRID_STEP * self.zoom)
            } else {
                Pos2::new(body.right(), center.y + pin.offset.y * GRID_STEP * self.zoom)
            };

            painter.line_segment(
                [body_edge, pin_screen],
                Stroke::new(1.5 * self.zoom, pin_direction_color(pin.direction)),
            );

            // Pin dot.
            painter.circle_filled(
                pin_screen,
                PIN_DOT_RADIUS * self.zoom,
                pin_direction_color(pin.direction),
            );

            // Pin name.
            let text_offset = if pin.offset.x < 0.0 {
                -8.0 * self.zoom
            } else {
                8.0 * self.zoom
            };
            let align = if pin.offset.x < 0.0 {
                egui::Align2::RIGHT_CENTER
            } else {
                egui::Align2::LEFT_CENTER
            };
            painter.text(
                Pos2::new(pin_screen.x + text_offset, pin_screen.y),
                align,
                &pin.name,
                egui::FontId::proportional(8.0 * self.zoom),
                Color32::from_rgb(150, 150, 160),
            );
        }

        // Footprint label (bottom).
        if !comp.footprint.is_empty() {
            painter.text(
                Pos2::new(center.x, body.bottom() + 4.0 * self.zoom),
                egui::Align2::CENTER_TOP,
                &comp.footprint,
                egui::FontId::proportional(8.0 * self.zoom),
                Color32::from_rgb(90, 90, 100),
            );
        }
    }

    // -----------------------------------------------------------------------
    // Test results overlay
    // -----------------------------------------------------------------------

    fn draw_test_results(&mut self, painter: &egui::Painter, canvas_rect: Rect) {
        let results_w = 320.0;
        let line_h = 18.0;
        let results_h = 40.0 + self.test_results.len() as f32 * line_h;
        let results_rect = Rect::from_min_size(
            Pos2::new(
                canvas_rect.right() - results_w - 10.0,
                canvas_rect.top() + 30.0,
            ),
            Vec2::new(results_w, results_h.min(300.0)),
        );

        painter.rect_filled(
            results_rect,
            6.0,
            Color32::from_rgba_premultiplied(20, 20, 28, 230),
        );
        painter.rect_stroke(
            results_rect,
            6.0,
            Stroke::new(1.0, Color32::from_rgb(60, 60, 68)),
        );

        painter.text(
            Pos2::new(results_rect.center().x, results_rect.top() + 14.0),
            egui::Align2::CENTER_CENTER,
            "Electrical Test Results",
            egui::FontId::proportional(12.0),
            theme::ACCENT,
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

    // -----------------------------------------------------------------------
    // Hit testing
    // -----------------------------------------------------------------------

    fn hit_test_component(&self, mouse: Pos2, canvas_rect: Rect) -> Option<usize> {
        for (idx, comp) in self.schematic.components.iter().enumerate().rev() {
            let center = self.world_to_screen(
                Pos2::new(comp.position.x, comp.position.y),
                canvas_rect,
            );
            let body = Rect::from_center_size(
                center,
                Vec2::new(COMP_BODY_W * self.zoom, COMP_BODY_H * self.zoom),
            );
            if body.contains(mouse) {
                return Some(idx);
            }
        }
        None
    }

    // -----------------------------------------------------------------------
    // Coordinate conversion
    // -----------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Color helpers
// ---------------------------------------------------------------------------

fn pin_direction_color(dir: PinDirection) -> Color32 {
    match dir {
        PinDirection::Input => Color32::from_rgb(100, 180, 255),
        PinDirection::Output => Color32::from_rgb(255, 140, 60),
        PinDirection::Bidirectional => Color32::from_rgb(150, 220, 150),
        PinDirection::Power => Color32::from_rgb(255, 80, 80),
        PinDirection::Ground => Color32::from_rgb(120, 120, 130),
    }
}
