//! Schematic view panel - visual editor for electronic schematics.
//!
//! Renders electronic components, wires, and pins on a 2D canvas
//! with grid snapping, pan/zoom, component placement, drag-to-move,
//! right-click context menu, rotation, wire selection, DC simulation
//! visualization, DRC, and export.
//!
//! Technology based on Yoll AU - yoll.site

use raf_core::Language;
use raf_core::i18n::t;
use egui::{Color32, Pos2, Rect, Stroke, Ui, Vec2};
use raf_electronics::component::{ElectronicComponent, PinDirection, SimModel};
use raf_electronics::library::ComponentLibrary;
use raf_electronics::schematic::Schematic;
use raf_electronics::simulation::SimulationResults;

use crate::theme;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const GRID_STEP: f32 = 20.0;
const COMP_BODY_W: f32 = 60.0;
const COMP_BODY_H: f32 = 30.0;
const PIN_DOT_RADIUS: f32 = 3.0;
/// Distance threshold for selecting a wire (in screen pixels).
const WIRE_HIT_DISTANCE: f32 = 6.0;

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

/// What kind of item is selected on the canvas.
#[derive(Debug, Clone, PartialEq)]
enum Selection {
    None,
    Component(usize),
    Wire(usize),
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
    /// Current selection.
    selection: Selection,
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
    /// Drag state: the component index being dragged and the offset from the
    /// mouse to the component center at drag start.
    drag_state: Option<(usize, Vec2)>,
    /// Context menu state: position + what was right-clicked.
    context_menu: Option<ContextMenu>,
    /// Inline editing state: component index and the editing buffer.
    editing_value: Option<(usize, String)>,
    /// Whether ES language is active (set externally each frame).
    pub lang: Language,
    // --- Simulation state ---
    /// Simulation results (None = not yet run).
    sim_results: Option<SimulationResults>,
    /// Whether simulation visualization is active.
    sim_active: bool,
    /// Animation phase for current flow arrows (0.0..1.0 cyclic).
    sim_phase: f32,
    // --- Export state ---
    /// Whether export menu is visible.
    show_export_menu: bool,
    /// Last export message (status feedback).
    export_message: Option<String>,
}

/// Simple context menu descriptor.
#[derive(Debug, Clone)]
struct ContextMenu {
    screen_pos: Pos2,
    target: Selection,
}

impl Default for SchematicViewPanel {
    fn default() -> Self {
        Self {
            schematic: Schematic::new("Untitled"),
            library: ComponentLibrary::default_library(),
            offset: Vec2::new(200.0, 150.0),
            zoom: 1.0,
            selection: Selection::None,
            placement: PlacementMode::None,
            wire_start: None,
            show_library: true,
            test_results: Vec::new(),
            show_test_results: false,
            drag_state: None,
            context_menu: None,
            editing_value: None,
            lang: Language::English,
            sim_results: None,
            sim_active: false,
            sim_phase: 0.0,
            show_export_menu: false,
            export_message: None,
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

        // Inline value editor (egui window).
        self.draw_value_editor(ui);

        // Context menu (egui window).
        self.draw_context_menu(ui);
    }

    // -----------------------------------------------------------------------
    // Toolbar
    // -----------------------------------------------------------------------

    fn draw_toolbar(&mut self, ui: &mut Ui) {
        let _lang = self.lang;

        ui.horizontal(|ui| {
            // Use subtle spacing
            ui.spacing_mut().item_spacing.x = 2.0;

            let active_bg = egui::Color32::from_rgb(45, 45, 52);
            let inactive_bg = egui::Color32::TRANSPARENT;
            let icon_color = egui::Color32::from_rgb(200, 200, 205);

            // Library toggle (☰ or ▤)
            let lib_bg = if self.show_library { active_bg } else { inactive_bg };
            let lib_btn = egui::Button::new(egui::RichText::new("▤").size(14.0).color(icon_color))
                .fill(lib_bg).frame(self.show_library).rounding(4.0);
            if ui.add_sized([28.0, 28.0], lib_btn).on_hover_text("Toggle Library").clicked() {
                self.show_library = !self.show_library;
            }

            // Wire mode toggle (↘)
            let wire_active = self.placement == PlacementMode::Wire;
            let wire_bg = if wire_active { active_bg } else { inactive_bg };
            let wire_btn = egui::Button::new(egui::RichText::new("↘").size(14.0).color(icon_color))
                .fill(wire_bg).frame(wire_active).rounding(4.0);
            if ui.add_sized([28.0, 28.0], wire_btn).on_hover_text("Draw Wire").clicked() {
                if wire_active {
                    self.placement = PlacementMode::None;
                    self.wire_start = None;
                } else {
                    self.placement = PlacementMode::Wire;
                }
            }

            ui.add_space(8.0);

            // DRC / Electrical test button (✓)
            let drc_btn = egui::Button::new(egui::RichText::new("✓").size(14.0).color(icon_color))
                .fill(inactive_bg).frame(false);
            if ui.add_sized([28.0, 28.0], drc_btn).on_hover_text("Run DRC / Electrical Test").clicked() {
                self.test_results = self.schematic.electrical_test();
                self.show_test_results = true;
            }

            // Simulate button (▶ or ⏹)
            if self.sim_active {
                let stop_btn = egui::Button::new(egui::RichText::new("⏹").size(14.0).color(egui::Color32::from_rgb(220, 90, 90)))
                    .fill(inactive_bg).frame(false);
                if ui.add_sized([28.0, 28.0], stop_btn).on_hover_text("Stop Simulation").clicked() {
                    self.sim_active = false;
                    self.sim_results = None;
                    self.sim_phase = 0.0;
                }
            } else {
                let sim_btn = egui::Button::new(egui::RichText::new("▶").size(14.0).color(icon_color))
                    .fill(inactive_bg).frame(false);
                if ui.add_sized([28.0, 28.0], sim_btn).on_hover_text("Simulate DC").clicked() {
                    let results = self.schematic.simulate_dc();
                    self.sim_active = results.converged;
                    self.sim_results = Some(results);
                }
            }

            ui.add_space(8.0);

            // Export button (⎘)
            let export_btn = egui::Button::new(egui::RichText::new("⎘").size(14.0).color(icon_color))
                .fill(inactive_bg).frame(false);
            if ui.add_sized([28.0, 28.0], export_btn).on_hover_text("Export Schematic").clicked() {
                self.show_export_menu = !self.show_export_menu;
            }

            // Push the rest of the text info to the right
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.placement != PlacementMode::None {
                    if ui.button(egui::RichText::new("Cancel (Esc)").size(11.0)).clicked() {
                        self.placement = PlacementMode::None;
                        self.wire_start = None;
                    }

                    let mode_text = match &self.placement {
                        PlacementMode::Component(idx) => {
                            if let Some(tmpl) = self.library.components.get(*idx) {
                                format!("Placing: {}", tmpl.name)
                            } else {
                                "Placing...".to_string()
                            }
                        }
                        PlacementMode::Wire => "Drawing Wire".to_string(),
                        PlacementMode::None => "".to_string(),
                    };
                    ui.label(egui::RichText::new(mode_text).size(11.0).color(theme::ACCENT));
                    ui.add_space(8.0);
                }

                let info = format!("Components: {} | Wires: {}", self.schematic.components.len(), self.schematic.wires.len());
                ui.label(egui::RichText::new(info).size(11.0).color(theme::DARK_TEXT_DIM));
            });
        });
    }

    // -----------------------------------------------------------------------
    // Component Library Sidebar
    // -----------------------------------------------------------------------

    fn draw_library(&mut self, ui: &mut Ui, rect: Rect) {
        let _lang = self.lang;
        let painter = ui.painter_at(rect);

        // Background.
        painter.rect_filled(rect, 0.0, Color32::from_rgb(24, 24, 28));
        painter.line_segment(
            [rect.right_top(), rect.right_bottom()],
            Stroke::new(1.0, Color32::from_rgb(45, 45, 50)),
        );

        // Title - uppercase, professional style.
        painter.text(
            Pos2::new(rect.left() + 12.0, rect.top() + 16.0),
            egui::Align2::LEFT_CENTER,
            "COMPONENT LIBRARY",
            egui::FontId::proportional(10.0),
            Color32::from_rgb(130, 130, 140),
        );

        // Separator line under title.
        painter.line_segment(
            [
                Pos2::new(rect.left() + 8.0, rect.top() + 28.0),
                Pos2::new(rect.right() - 8.0, rect.top() + 28.0),
            ],
            Stroke::new(0.5, Color32::from_rgb(45, 45, 50)),
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
                Color32::from_rgb(50, 42, 28)
            } else if resp.hovered() {
                Color32::from_rgb(38, 38, 44)
            } else {
                Color32::from_rgb(30, 30, 36)
            };

            painter.rect_filled(btn_rect, 4.0, bg);

            if is_selected {
                // Left accent bar instead of full border.
                let accent_rect = Rect::from_min_size(
                    Pos2::new(btn_rect.left(), btn_rect.top() + 4.0),
                    Vec2::new(2.0, btn_rect.height() - 8.0),
                );
                painter.rect_filled(accent_rect, 1.0, theme::ACCENT);
            }

            painter.text(
                Pos2::new(btn_rect.left() + 10.0, btn_rect.top() + 14.0),
                egui::Align2::LEFT_CENTER,
                &name,
                egui::FontId::proportional(11.0),
                Color32::from_rgb(210, 210, 220),
            );

            painter.text(
                Pos2::new(btn_rect.left() + 10.0, btn_rect.top() + 32.0),
                egui::Align2::LEFT_CENTER,
                &format!("{} - {}", category, desc),
                egui::FontId::proportional(9.0),
                Color32::from_rgb(100, 100, 115),
            );

            if resp.clicked() {
                self.placement = PlacementMode::Component(idx);
            }

            if resp.hovered() {
                let tip = format!("Click to select and place {}", name);
                resp.on_hover_text(tip);
            }

            y += 54.0;
        }
    }

    // -----------------------------------------------------------------------
    // Canvas
    // -----------------------------------------------------------------------

    fn draw_canvas(&mut self, ui: &mut Ui, rect: Rect) {
        let _lang = self.lang;

        // -- Painting phase: all read-only drawing --
        {
            let painter = ui.painter_at(rect);

            // Background.
            painter.rect_filled(rect, 0.0, Color32::from_rgb(18, 18, 24));

            // Draw grid.
            self.draw_grid(&painter, rect);

            // Draw wires.
            let selected_wire = if let Selection::Wire(idx) = self.selection {
                Some(idx)
            } else {
                None
            };
            for (idx, wire) in self.schematic.wires.iter().enumerate() {
                let start = self.world_to_screen(
                    Pos2::new(wire.start.x, wire.start.y),
                    rect,
                );
                let end = self.world_to_screen(
                    Pos2::new(wire.end.x, wire.end.y),
                    rect,
                );

                let wire_color = if selected_wire == Some(idx) {
                    theme::ACCENT
                } else {
                    Color32::from_rgb(60, 200, 60)
                };
                let wire_width = if selected_wire == Some(idx) {
                    3.0 * self.zoom
                } else {
                    2.0 * self.zoom
                };

                painter.line_segment([start, end], Stroke::new(wire_width, wire_color));

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
                let is_selected = match self.selection {
                    Selection::Component(sel) => sel == idx,
                    _ => false,
                };
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

            // --- Simulation visualization overlay ---
            if self.sim_active {
                if let Some(ref sim) = self.sim_results {
                    self.draw_sim_overlay(&painter, rect, sim);
                }
                // Advance animation phase.
                self.sim_phase = (self.sim_phase + 0.015) % 1.0;
                ui.ctx().request_repaint();
            }

            // --- Export menu overlay ---
            if self.show_export_menu {
                self.draw_export_menu(&painter, rect);
            }
            let info_text = format!(
                    "Zoom: {:.1}x | R: Rotate | Esc: Cancel | Del: Remove | Right-click: Menu",
                    self.zoom
                );
            painter.text(
                Pos2::new(rect.left() + 10.0, rect.top() + 10.0),
                egui::Align2::LEFT_TOP,
                info_text,
                egui::FontId::proportional(10.0),
                Color32::from_rgb(85, 85, 95),
            );
        } // painter dropped here

        // -- Interaction phase: mutable borrows of ui --
        let resp = ui.allocate_rect(rect, egui::Sense::click_and_drag());

        // --- Drag to move selected component ---
        if self.placement == PlacementMode::None {
            // Start drag on left-press over a component.
            if resp.drag_started_by(egui::PointerButton::Primary) {
                if let Some(mouse) = ui.input(|i| i.pointer.hover_pos()) {
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
                        self.selection = Selection::Component(idx);
                    }
                }
            }
            // Continue drag.
            if resp.dragged_by(egui::PointerButton::Primary) {
                if let Some((idx, offset)) = self.drag_state {
                    if let Some(mouse) = ui.input(|i| i.pointer.hover_pos()) {
                        let target_screen = Pos2::new(mouse.x - offset.x, mouse.y - offset.y);
                        let world = self.snap_to_grid(self.screen_to_world(target_screen, rect));
                        if idx < self.schematic.components.len() {
                            self.schematic.components[idx].position =
                                glam::Vec2::new(world.x, world.y);
                        }
                    }
                }
            }
            // End drag.
            if resp.drag_stopped_by(egui::PointerButton::Primary) {
                self.drag_state = None;
            }
        }

        // Left-click: place component or wire point, or select.
        if resp.clicked() && self.drag_state.is_none() {
            // Close context menu on any click.
            self.context_menu = None;

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
                        // Try to select a component first, then a wire.
                        if let Some(idx) = self.hit_test_component(mouse, rect) {
                            self.selection = Selection::Component(idx);
                        } else if let Some(idx) = self.hit_test_wire(mouse, rect) {
                            self.selection = Selection::Wire(idx);
                        } else {
                            self.selection = Selection::None;
                        }
                    }
                }
            }
        }

        // Right-click: open context menu (NOT pan).
        if resp.clicked_by(egui::PointerButton::Secondary) {
            if let Some(mouse) = ui.input(|i| i.pointer.hover_pos()) {
                // Determine what was right-clicked.
                let target = if let Some(idx) = self.hit_test_component(mouse, rect) {
                    self.selection = Selection::Component(idx);
                    Selection::Component(idx)
                } else if let Some(idx) = self.hit_test_wire(mouse, rect) {
                    self.selection = Selection::Wire(idx);
                    Selection::Wire(idx)
                } else {
                    Selection::None
                };
                self.context_menu = Some(ContextMenu {
                    screen_pos: mouse,
                    target,
                });
            }
        }

        // Middle-drag to pan.
        if resp.dragged_by(egui::PointerButton::Middle) {
            self.offset += resp.drag_delta();
        }

        // Right-drag to pan (only if no context menu active).
        if self.context_menu.is_none() && resp.dragged_by(egui::PointerButton::Secondary) {
            self.offset += resp.drag_delta();
        }

        // Scroll to zoom (centered on mouse cursor).
        if ui.rect_contains_pointer(rect) {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                if let Some(mouse) = ui.input(|i| i.pointer.hover_pos()) {
                    let old_zoom = self.zoom;
                    self.zoom = (self.zoom + scroll * 0.002).clamp(0.3, 4.0);
                    // Adjust offset so zoom centers on cursor position.
                    let factor = self.zoom / old_zoom;
                    self.offset.x = mouse.x - rect.left() - (mouse.x - rect.left() - self.offset.x) * factor;
                    self.offset.y = mouse.y - rect.top() - (mouse.y - rect.top() - self.offset.y) * factor;
                } else {
                    self.zoom = (self.zoom + scroll * 0.002).clamp(0.3, 4.0);
                }
            }
        }

        // Escape to cancel placement or close overlays.
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
                self.selection = Selection::None;
            }
        }

        // Export key shortcuts (1/2/3) when export menu is open.
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

        // Delete selected item.
        if ui.input(|i| i.key_pressed(egui::Key::Delete)) {
            match self.selection {
                Selection::Component(idx) => {
                    if idx < self.schematic.components.len() {
                        self.schematic.components.remove(idx);
                        self.selection = Selection::None;
                    }
                }
                Selection::Wire(idx) => {
                    self.schematic.remove_wire(idx);
                    self.selection = Selection::None;
                }
                Selection::None => {}
            }
        }

        // R to rotate selected component.
        if ui.input(|i| i.key_pressed(egui::Key::R)) {
            if let Selection::Component(idx) = self.selection {
                if idx < self.schematic.components.len() {
                    let comp = &mut self.schematic.components[idx];
                    comp.rotation = (comp.rotation + 90.0) % 360.0;
                }
            }
        }

        // M to mirror (horizontal flip) selected component.
        if ui.input(|i| i.key_pressed(egui::Key::M)) {
            if let Selection::Component(idx) = self.selection {
                if idx < self.schematic.components.len() {
                    let comp = &mut self.schematic.components[idx];
                    for pin in &mut comp.pins {
                        pin.offset.x = -pin.offset.x;
                    }
                }
            }
        }

        // F to center view on selected component.
        if ui.input(|i| i.key_pressed(egui::Key::F)) {
            if let Selection::Component(idx) = self.selection {
                if idx < self.schematic.components.len() {
                    let comp = &self.schematic.components[idx];
                    self.offset.x = rect.width() / 2.0 - comp.position.x * self.zoom;
                    self.offset.y = rect.height() / 2.0 - comp.position.y * self.zoom;
                }
            }
        }

        // Ctrl+D to duplicate selected component.
        let ctrl_d = ui.input(|i| {
            i.modifiers.ctrl && i.key_pressed(egui::Key::D)
        });
        if ctrl_d {
            if let Selection::Component(idx) = self.selection {
                if let Some(_id) = self.schematic.duplicate_component(idx) {
                    let new_idx = self.schematic.components.len() - 1;
                    self.selection = Selection::Component(new_idx);
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Context Menu
    // -----------------------------------------------------------------------

    fn draw_context_menu(&mut self, ui: &mut Ui) {
        let menu = match self.context_menu.clone() {
            Some(m) => m,
            None => return,
        };

        let _lang = self.lang;
        let mut close_menu = false;

        let menu_id = egui::Id::new("schematic_context_menu");
        let pivot = egui::Align2::LEFT_TOP;

        egui::Area::new(menu_id)
            .fixed_pos(menu.screen_pos)
            .pivot(pivot)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(140.0);

                    match &menu.target {
                        Selection::Component(idx) => {
                            let idx = *idx;

                            // Rotate.
                            let rotate_label = t("app.rotate_r", self.lang);
                            if ui.button(rotate_label).clicked() {
                                if idx < self.schematic.components.len() {
                                    let comp = &mut self.schematic.components[idx];
                                    comp.rotation = (comp.rotation + 90.0) % 360.0;
                                }
                                close_menu = true;
                            }

                            // Edit value.
                            let edit_label = t("app.edit_value", self.lang);
                            if ui.button(edit_label).clicked() {
                                if idx < self.schematic.components.len() {
                                    let val = self.schematic.components[idx].value.clone();
                                    self.editing_value = Some((idx, val));
                                }
                                close_menu = true;
                            }

                            // Duplicate.
                            let dup_label = t("app.duplicate_ctrl_d", self.lang);
                            if ui.button(dup_label).clicked() {
                                if let Some(_id) = self.schematic.duplicate_component(idx) {
                                    let new_idx = self.schematic.components.len() - 1;
                                    self.selection = Selection::Component(new_idx);
                                }
                                close_menu = true;
                            }

                            ui.separator();

                            // Delete.
                            let del_label = t("app.delete_del", self.lang);
                            if ui
                                .button(
                                    egui::RichText::new(del_label)
                                        .color(theme::STATUS_ERROR),
                                )
                                .clicked()
                            {
                                if idx < self.schematic.components.len() {
                                    self.schematic.components.remove(idx);
                                    self.selection = Selection::None;
                                }
                                close_menu = true;
                            }
                        }
                        Selection::Wire(idx) => {
                            let idx = *idx;

                            // Delete wire.
                            let del_label = t("app.delete_wire_del", self.lang);
                            if ui
                                .button(
                                    egui::RichText::new(del_label)
                                        .color(theme::STATUS_ERROR),
                                )
                                .clicked()
                            {
                                self.schematic.remove_wire(idx);
                                self.selection = Selection::None;
                                close_menu = true;
                            }
                        }
                        Selection::None => {
                            // Canvas context menu (no item under cursor).
                            let paste_label = t("app.wire_mode", self.lang);
                            if ui.button(paste_label).clicked() {
                                self.placement = PlacementMode::Wire;
                                close_menu = true;
                            }

                            let test_label = t("app.electrical_test", self.lang);
                            if ui.button(test_label).clicked() {
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
    }

    // -----------------------------------------------------------------------
    // Inline Value Editor
    // -----------------------------------------------------------------------

    fn draw_value_editor(&mut self, ui: &mut Ui) {
        let (idx, mut buf) = match self.editing_value.take() {
            Some(v) => v,
            None => return,
        };

        let _lang = self.lang;
        let mut keep_open = true;

        let win_title = t("app.edit_value", self.lang);
        egui::Window::new(win_title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                ui.horizontal(|ui| {
                    let label = t("app.value", self.lang);
                    ui.label(label);
                    let resp = ui.text_edit_singleline(&mut buf);
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        if idx < self.schematic.components.len() {
                            self.schematic.components[idx].value = buf.clone();
                        }
                        keep_open = false;
                    }
                });
                ui.horizontal(|ui| {
                    let ok_label = t("app.ok", self.lang);
                    if ui.button(ok_label).clicked() {
                        if idx < self.schematic.components.len() {
                            self.schematic.components[idx].value = buf.clone();
                        }
                        keep_open = false;
                    }
                    let cancel_label = t("app.cancel", self.lang);
                    if ui.button(cancel_label).clicked() {
                        keep_open = false;
                    }
                });
            });

        if keep_open {
            self.editing_value = Some((idx, buf));
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
    // Component rendering (with rotation support)
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

        // Rotation indicator: small rotation text if not 0.
        if comp.rotation != 0.0 {
            painter.text(
                Pos2::new(body.right() - 2.0 * self.zoom, body.top() + 2.0 * self.zoom),
                egui::Align2::RIGHT_TOP,
                &format!("{}deg", comp.rotation as i32),
                egui::FontId::proportional(7.0 * self.zoom),
                Color32::from_rgb(90, 90, 100),
            );
        }

        // Draw pins (applying rotation).
        let rot_rad = comp.rotation.to_radians();
        let cos_r = rot_rad.cos();
        let sin_r = rot_rad.sin();

        for pin in &comp.pins {
            // Rotate pin offset around the component center.
            let raw_ox = pin.offset.x * GRID_STEP;
            let raw_oy = pin.offset.y * GRID_STEP;
            let rot_ox = raw_ox * cos_r - raw_oy * sin_r;
            let rot_oy = raw_ox * sin_r + raw_oy * cos_r;

            let pin_world = Pos2::new(
                comp.position.x + rot_ox,
                comp.position.y + rot_oy,
            );
            let pin_screen = self.world_to_screen(pin_world, canvas_rect);

            // Pin line from body edge to pin position.
            let body_edge = if rot_ox < 0.0 {
                Pos2::new(body.left(), center.y + rot_oy * self.zoom)
            } else {
                Pos2::new(body.right(), center.y + rot_oy * self.zoom)
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
            let text_offset = if rot_ox < 0.0 {
                -8.0 * self.zoom
            } else {
                8.0 * self.zoom
            };
            let align = if rot_ox < 0.0 {
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
    // Test results overlay (with close button)
    // -----------------------------------------------------------------------

    fn draw_test_results(&mut self, painter: &egui::Painter, canvas_rect: Rect) {
        let _lang = self.lang;
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

        let title = t("app.electrical_test_results", self.lang);
        painter.text(
            Pos2::new(results_rect.center().x, results_rect.top() + 14.0),
            egui::Align2::CENTER_CENTER,
            title,
            egui::FontId::proportional(12.0),
            theme::ACCENT,
        );

        // Close hint.
        let close_hint = t("app.esc_close", self.lang);
        painter.text(
            Pos2::new(results_rect.right() - 8.0, results_rect.top() + 14.0),
            egui::Align2::RIGHT_CENTER,
            close_hint,
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

    // -----------------------------------------------------------------------
    // Simulation visualization overlay
    // -----------------------------------------------------------------------

    fn draw_sim_overlay(
        &self,
        painter: &egui::Painter,
        canvas_rect: Rect,
        sim: &SimulationResults,
    ) {
        let _netlist = self.schematic.netlist();

        // Draw animated current flow arrows on wires.
        for (_wi, wire) in self.schematic.wires.iter().enumerate() {
            let start = self.world_to_screen(
                Pos2::new(wire.start.x, wire.start.y),
                canvas_rect,
            );
            let end = self.world_to_screen(
                Pos2::new(wire.end.x, wire.end.y),
                canvas_rect,
            );

            let dx = end.x - start.x;
            let dy = end.y - start.y;
            let wire_len = (dx * dx + dy * dy).sqrt();
            if wire_len < 1.0 {
                continue;
            }

            // Draw 3-5 animated dots along the wire.
            let dot_count = ((wire_len / (20.0 * self.zoom)) as usize).clamp(2, 6);
            let nx = dx / wire_len;
            let ny = dy / wire_len;

            for i in 0..dot_count {
                let base_t = i as f32 / dot_count as f32;
                let t = (base_t + self.sim_phase) % 1.0;
                let px = start.x + dx * t;
                let py = start.y + dy * t;

                // Arrow-like triangle pointing in flow direction.
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

        // Draw voltage labels at component pins and power/heat on components.
        for (ci, comp) in self.schematic.components.iter().enumerate() {
            let center = self.world_to_screen(
                Pos2::new(comp.position.x, comp.position.y),
                canvas_rect,
            );

            // Component power heat coloring.
            if let Some(&power) = sim.component_power.get(&ci) {
                let heat = (power * 50.0).clamp(0.0, 1.0) as f32;
                if heat > 0.01 {
                    let r = (80.0 + heat * 175.0) as u8;
                    let g = (220.0 - heat * 180.0) as u8;
                    let b = (80.0 - heat * 60.0) as u8;
                    let body_w = COMP_BODY_W * self.zoom;
                    let body_h = COMP_BODY_H * self.zoom;
                    let heat_rect = Rect::from_center_size(
                        center,
                        Vec2::new(body_w + 4.0, body_h + 4.0),
                    );
                    painter.rect_stroke(
                        heat_rect,
                        4.0 * self.zoom,
                        Stroke::new(2.0, Color32::from_rgb(r, g, b)),
                    );
                }
            }

            // Current label below component.
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
                        &label,
                        egui::FontId::proportional(8.0 * self.zoom),
                        Color32::from_rgb(80, 200, 255),
                    );
                }
            }

            // LED glow effect.
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
                        painter.circle_filled(
                            center,
                            glow_radius * 0.5,
                            Color32::from_rgba_premultiplied(255, 240, 150, (glow_alpha / 2).min(120)),
                        );
                    }
                }
            }
        }

        // Sim status label.
        let status = if sim.converged {
            if self.lang == raf_core::config::Language::Spanish { "Simulacion DC activa" } else { "DC Simulation active" }
        } else {
            if self.lang == raf_core::config::Language::Spanish { "Simulacion no convergio" } else { "Simulation did not converge" }
        };
        let status_color = if sim.converged {
            Color32::from_rgb(80, 220, 120)
        } else {
            Color32::from_rgb(255, 100, 80)
        };
        painter.text(
            Pos2::new(canvas_rect.right() - 10.0, canvas_rect.top() + 10.0),
            egui::Align2::RIGHT_TOP,
            status,
            egui::FontId::proportional(11.0),
            status_color,
        );
    }

    // -----------------------------------------------------------------------
    // Export menu overlay
    // -----------------------------------------------------------------------

    fn draw_export_menu(&self, painter: &egui::Painter, canvas_rect: Rect) {
        let _lang = self.lang;
        let menu_w = 220.0;
        let menu_h = 130.0;
        let menu_rect = Rect::from_min_size(
            Pos2::new(
                canvas_rect.center().x - menu_w / 2.0,
                canvas_rect.top() + 60.0,
            ),
            Vec2::new(menu_w, menu_h),
        );

        painter.rect_filled(
            menu_rect,
            8.0,
            Color32::from_rgba_premultiplied(25, 25, 35, 240),
        );
        painter.rect_stroke(
            menu_rect,
            8.0,
            Stroke::new(1.0, theme::ACCENT),
        );

        let title = t("app.export_schematic", self.lang);
        painter.text(
            Pos2::new(menu_rect.center().x, menu_rect.top() + 16.0),
            egui::Align2::CENTER_CENTER,
            title,
            egui::FontId::proportional(12.0),
            theme::ACCENT,
        );

        let options = if _lang == raf_core::config::Language::Spanish {
            vec![
                "1. Netlist (texto)",
                "2. BOM (CSV)",
                "3. SVG (imagen vectorial)",
            ]
        } else {
            vec![
                "1. Netlist (text)",
                "2. BOM (CSV)",
                "3. SVG (vector image)",
            ]
        };

        let hint = t("app.esc_close_1_2_3_export", self.lang);

        let mut y = menu_rect.top() + 38.0;
        for opt in &options {
            painter.text(
                Pos2::new(menu_rect.left() + 16.0, y),
                egui::Align2::LEFT_CENTER,
                opt,
                egui::FontId::proportional(11.0),
                Color32::from_rgb(200, 200, 210),
            );
            y += 22.0;
        }

        painter.text(
            Pos2::new(menu_rect.center().x, menu_rect.bottom() - 10.0),
            egui::Align2::CENTER_BOTTOM,
            hint,
            egui::FontId::proportional(9.0),
            Color32::from_rgb(130, 130, 140),
        );

        // Yoll AU credit in new panel.
        painter.text(
            Pos2::new(menu_rect.right() - 4.0, menu_rect.bottom() - 2.0),
            egui::Align2::RIGHT_BOTTOM,
            "Yoll AU - yoll.site",
            egui::FontId::proportional(7.0),
            Color32::from_rgb(60, 60, 70),
        );
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

    /// Hit test for wire segments: returns index of the nearest wire within
    /// the threshold distance, or None.
    fn hit_test_wire(&self, mouse: Pos2, canvas_rect: Rect) -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;

        for (idx, wire) in self.schematic.wires.iter().enumerate() {
            let a = self.world_to_screen(Pos2::new(wire.start.x, wire.start.y), canvas_rect);
            let b = self.world_to_screen(Pos2::new(wire.end.x, wire.end.y), canvas_rect);
            let dist = point_to_segment_distance(mouse, a, b);
            if dist < WIRE_HIT_DISTANCE {
                if best.is_none() || dist < best.unwrap().1 {
                    best = Some((idx, dist));
                }
            }
        }

        best.map(|(idx, _)| idx)
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
// Geometry helpers
// ---------------------------------------------------------------------------

/// Distance from a point to a line segment.
fn point_to_segment_distance(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = Vec2::new(b.x - a.x, b.y - a.y);
    let ap = Vec2::new(p.x - a.x, p.y - a.y);
    let len_sq = ab.x * ab.x + ab.y * ab.y;
    if len_sq < 0.001 {
        return ap.length();
    }
    let t = ((ap.x * ab.x + ap.y * ab.y) / len_sq).clamp(0.0, 1.0);
    let proj = Pos2::new(a.x + t * ab.x, a.y + t * ab.y);
    let dx = p.x - proj.x;
    let dy = p.y - proj.y;
    (dx * dx + dy * dy).sqrt()
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
