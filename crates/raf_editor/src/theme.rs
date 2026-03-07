//! Theme system for AuraRafi editor.
//!
//! Provides dark and light theme definitions with a consistent warm orange
//! accent color. All color tokens are exported as public constants for use
//! across the editor.

use eframe::egui;
use egui::{Color32, Rounding, Stroke, Visuals};
use raf_core::config::Theme;

// ---------------------------------------------------------------------------
// Brand and accent colors
// ---------------------------------------------------------------------------

/// Primary accent color - warm orange used throughout the UI.
pub const ACCENT: Color32 = Color32::from_rgb(212, 119, 26);

/// Accent hover state - slightly brighter.
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(230, 140, 40);

/// Accent muted state - dimmer for backgrounds.
pub const ACCENT_MUTED: Color32 = Color32::from_rgb(120, 70, 20);

// ---------------------------------------------------------------------------
// Dark theme tokens
// ---------------------------------------------------------------------------

/// Dark theme main background - true black.
pub const DARK_BG: Color32 = Color32::from_rgb(15, 15, 15);

/// Dark theme panel background (slightly lighter).
pub const DARK_PANEL: Color32 = Color32::from_rgb(22, 22, 22);

/// Dark theme widget background (buttons, inputs).
pub const DARK_WIDGET: Color32 = Color32::from_rgb(38, 38, 38);

/// Dark theme widget hover.
pub const DARK_WIDGET_HOVER: Color32 = Color32::from_rgb(52, 52, 52);

/// Dark theme widget active/pressed.
pub const DARK_WIDGET_ACTIVE: Color32 = Color32::from_rgb(58, 58, 58);

/// Dark theme primary text.
pub const DARK_TEXT: Color32 = Color32::from_rgb(220, 220, 230);

/// Dark theme dimmed/secondary text.
pub const DARK_TEXT_DIM: Color32 = Color32::from_rgb(120, 120, 120);

/// Dark theme selection highlight.
pub const DARK_SELECTION: Color32 = Color32::from_rgb(60, 45, 25);

/// Dark theme border color.
pub const DARK_BORDER: Color32 = Color32::from_rgb(50, 50, 50);

/// Dark theme separator/divider color.
pub const DARK_SEPARATOR: Color32 = Color32::from_rgb(40, 40, 40);

// ---------------------------------------------------------------------------
// Light theme tokens
// ---------------------------------------------------------------------------

/// Light theme main background.
pub const LIGHT_BG: Color32 = Color32::from_rgb(242, 242, 246);

/// Light theme panel background.
pub const LIGHT_PANEL: Color32 = Color32::from_rgb(250, 250, 252);

/// Light theme widget background.
pub const LIGHT_WIDGET: Color32 = Color32::from_rgb(228, 228, 234);

/// Light theme widget hover.
pub const LIGHT_WIDGET_HOVER: Color32 = Color32::from_rgb(215, 215, 222);

/// Light theme primary text.
pub const LIGHT_TEXT: Color32 = Color32::from_rgb(30, 30, 40);

/// Light theme dimmed/secondary text.
pub const LIGHT_TEXT_DIM: Color32 = Color32::from_rgb(110, 110, 125);

/// Light theme border.
pub const LIGHT_BORDER: Color32 = Color32::from_rgb(200, 200, 210);

// ---------------------------------------------------------------------------
// Status colors
// ---------------------------------------------------------------------------

/// Success / info green.
pub const STATUS_OK: Color32 = Color32::from_rgb(80, 200, 100);

/// Warning amber.
pub const STATUS_WARN: Color32 = Color32::from_rgb(230, 180, 60);

/// Error red.
pub const STATUS_ERROR: Color32 = Color32::from_rgb(220, 70, 70);

// ---------------------------------------------------------------------------
// Theme application
// ---------------------------------------------------------------------------

/// Apply the selected theme to the egui context.
pub fn apply_theme(ctx: &egui::Context, theme: Theme) {
    match theme {
        Theme::Dark | Theme::System => apply_dark(ctx),
        Theme::Light => apply_light(ctx),
    }
}

fn apply_dark(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();

    // Window / panel backgrounds.
    visuals.panel_fill = DARK_PANEL;
    visuals.window_fill = DARK_PANEL;
    visuals.extreme_bg_color = DARK_BG;
    visuals.faint_bg_color = Color32::from_rgb(28, 28, 28);

    // Hyperlinks.
    visuals.hyperlink_color = ACCENT;

    // Selection.
    visuals.selection.bg_fill = DARK_SELECTION;
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);

    // Separator.
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(0.5, DARK_SEPARATOR);

    // Rounding for a modern look.
    let rounding = Rounding::same(6.0);
    let small_rounding = Rounding::same(4.0);

    // Non-interactive widgets (labels, frames).
    visuals.widgets.noninteractive.bg_fill = DARK_PANEL;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, DARK_TEXT);
    visuals.widgets.noninteractive.rounding = small_rounding;

    // Inactive widgets (buttons when not hovered).
    visuals.widgets.inactive.bg_fill = DARK_WIDGET;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, DARK_TEXT);
    visuals.widgets.inactive.rounding = rounding;
    visuals.widgets.inactive.bg_stroke = Stroke::new(0.5, DARK_BORDER);
    visuals.widgets.inactive.weak_bg_fill = DARK_WIDGET;

    // Hovered widgets.
    visuals.widgets.hovered.bg_fill = DARK_WIDGET_HOVER;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.hovered.rounding = rounding;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.hovered.weak_bg_fill = DARK_WIDGET_HOVER;

    // Active (pressed) widgets.
    visuals.widgets.active.bg_fill = DARK_WIDGET_ACTIVE;
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.active.rounding = rounding;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.active.weak_bg_fill = DARK_WIDGET_ACTIVE;

    // Open (expanded/dropdown) widgets.
    visuals.widgets.open.bg_fill = DARK_WIDGET;
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.open.rounding = rounding;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, ACCENT);

    // Window.
    visuals.window_rounding = Rounding::same(8.0);
    visuals.window_stroke = Stroke::new(1.0, DARK_BORDER);
    visuals.menu_rounding = Rounding::same(6.0);

    // Apply.
    ctx.set_visuals(visuals);
}

fn apply_light(ctx: &egui::Context) {
    let mut visuals = Visuals::light();

    // Window / panel backgrounds.
    visuals.panel_fill = LIGHT_PANEL;
    visuals.window_fill = LIGHT_PANEL;
    visuals.extreme_bg_color = LIGHT_BG;
    visuals.faint_bg_color = Color32::from_rgb(235, 235, 240);

    // Hyperlinks.
    visuals.hyperlink_color = ACCENT;

    // Selection.
    visuals.selection.bg_fill = Color32::from_rgb(255, 230, 190);
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);

    // Rounding.
    let rounding = Rounding::same(6.0);
    let small_rounding = Rounding::same(4.0);

    // Non-interactive.
    visuals.widgets.noninteractive.bg_fill = LIGHT_PANEL;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, LIGHT_TEXT);
    visuals.widgets.noninteractive.rounding = small_rounding;

    // Inactive.
    visuals.widgets.inactive.bg_fill = LIGHT_WIDGET;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, LIGHT_TEXT);
    visuals.widgets.inactive.rounding = rounding;
    visuals.widgets.inactive.bg_stroke = Stroke::new(0.5, LIGHT_BORDER);
    visuals.widgets.inactive.weak_bg_fill = LIGHT_WIDGET;

    // Hovered.
    visuals.widgets.hovered.bg_fill = LIGHT_WIDGET_HOVER;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, LIGHT_TEXT);
    visuals.widgets.hovered.rounding = rounding;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.hovered.weak_bg_fill = LIGHT_WIDGET_HOVER;

    // Active.
    visuals.widgets.active.bg_fill = Color32::from_rgb(200, 200, 210);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, LIGHT_TEXT);
    visuals.widgets.active.rounding = rounding;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);

    // Window.
    visuals.window_rounding = Rounding::same(8.0);
    visuals.window_stroke = Stroke::new(1.0, LIGHT_BORDER);
    visuals.menu_rounding = Rounding::same(6.0);

    // Apply.
    ctx.set_visuals(visuals);
}
