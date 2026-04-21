//! Theme system for AuraRafi editor.
//!
//! Provides dark and light theme definitions with a consistent warm orange
//! accent color. Base color tokens are exported as public constants and can be
//! shifted through an experimental tint slider.

use eframe::egui;
use egui::{Color32, Rounding, Stroke, Visuals};
use raf_core::config::Theme;

#[derive(Debug, Clone, Copy)]
pub struct ThemePalette {
    pub bg: Color32,
    pub panel: Color32,
    pub widget: Color32,
    pub widget_hover: Color32,
    pub widget_active: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub selection: Color32,
    pub border: Color32,
    pub separator: Color32,
    pub faint_bg: Color32,
}

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

/// Dark theme main background - charcoal black.
pub const DARK_BG: Color32 = Color32::from_rgb(11, 11, 12);

/// Dark theme panel background (slightly lighter).
pub const DARK_PANEL: Color32 = Color32::from_rgb(17, 17, 18);

/// Dark theme widget background (buttons, backgrounds for cards).
pub const DARK_WIDGET: Color32 = Color32::from_rgb(24, 24, 26);

/// Dark theme widget hover.
pub const DARK_WIDGET_HOVER: Color32 = Color32::from_rgb(32, 32, 34);

/// Dark theme widget active/pressed.
pub const DARK_WIDGET_ACTIVE: Color32 = Color32::from_rgb(38, 38, 40);

/// Dark theme primary text.
pub const DARK_TEXT: Color32 = Color32::from_rgb(210, 210, 220);

/// Dark theme dimmed/secondary text.
pub const DARK_TEXT_DIM: Color32 = Color32::from_rgb(100, 100, 110);

/// Dark theme selection highlight.
pub const DARK_SELECTION: Color32 = Color32::from_rgb(50, 40, 30);

/// Dark theme border color.
pub const DARK_BORDER: Color32 = Color32::from_rgb(40, 40, 42);

/// Dark theme separator/divider color.
pub const DARK_SEPARATOR: Color32 = Color32::from_rgb(28, 28, 30);

// ---------------------------------------------------------------------------
// Light theme tokens
// ---------------------------------------------------------------------------

/// Light theme main background - Soft snow.
pub const LIGHT_BG: Color32 = Color32::from_rgb(248, 249, 252);

/// Light theme panel background - Pure white.
pub const LIGHT_PANEL: Color32 = Color32::from_rgb(255, 255, 255);

/// Light theme widget background.
pub const LIGHT_WIDGET: Color32 = Color32::from_rgb(238, 240, 245);

/// Light theme widget hover.
pub const LIGHT_WIDGET_HOVER: Color32 = Color32::from_rgb(230, 233, 240);

/// Light theme primary text.
pub const LIGHT_TEXT: Color32 = Color32::from_rgb(45, 47, 54);

/// Light theme dimmed/secondary text.
pub const LIGHT_TEXT_DIM: Color32 = Color32::from_rgb(120, 125, 140);

/// Light theme border.
pub const LIGHT_BORDER: Color32 = Color32::from_rgb(215, 220, 230);

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
pub fn apply_theme(ctx: &egui::Context, theme: Theme, experimental: f32) {
    let palette = palette_for(theme, experimental);
    match theme {
        Theme::Dark | Theme::System => apply_dark(ctx, palette),
        Theme::Light => apply_light(ctx, palette),
    }
}

pub fn palette_for(theme: Theme, experimental: f32) -> ThemePalette {
    match theme {
        Theme::Dark | Theme::System => dark_palette(experimental),
        Theme::Light => light_palette(experimental),
    }
}

pub fn palette_for_visuals(is_dark: bool, experimental: f32) -> ThemePalette {
    if is_dark {
        dark_palette(experimental)
    } else {
        light_palette(experimental)
    }
}

pub fn normalize_experimental(value: f32) -> f32 {
    (value / 100.0).clamp(0.0, 1.0)
}

fn dark_palette(experimental: f32) -> ThemePalette {
    let factor = normalize_experimental(experimental);
    ThemePalette {
        bg: mix_color(DARK_BG, Color32::from_rgb(10, 22, 34), factor),
        panel: mix_color(DARK_PANEL, Color32::from_rgb(16, 30, 46), factor),
        widget: mix_color(DARK_WIDGET, Color32::from_rgb(24, 41, 60), factor),
        widget_hover: mix_color(DARK_WIDGET_HOVER, Color32::from_rgb(31, 53, 76), factor),
        widget_active: mix_color(DARK_WIDGET_ACTIVE, Color32::from_rgb(40, 66, 92), factor),
        text: mix_color(DARK_TEXT, Color32::from_rgb(222, 235, 255), factor),
        text_dim: mix_color(DARK_TEXT_DIM, Color32::from_rgb(132, 150, 176), factor),
        selection: mix_color(DARK_SELECTION, Color32::from_rgb(38, 78, 104), factor),
        border: mix_color(DARK_BORDER, Color32::from_rgb(66, 92, 122), factor),
        separator: mix_color(DARK_SEPARATOR, Color32::from_rgb(42, 62, 82), factor),
        faint_bg: mix_color(Color32::from_rgb(28, 28, 28), Color32::from_rgb(24, 36, 50), factor),
    }
}

fn light_palette(experimental: f32) -> ThemePalette {
    let factor = normalize_experimental(experimental);
    ThemePalette {
        bg: mix_color(LIGHT_BG, Color32::from_rgb(226, 239, 255), factor),
        panel: mix_color(LIGHT_PANEL, Color32::from_rgb(236, 247, 255), factor),
        widget: mix_color(LIGHT_WIDGET, Color32::from_rgb(211, 228, 245), factor),
        widget_hover: mix_color(LIGHT_WIDGET_HOVER, Color32::from_rgb(197, 218, 238), factor),
        widget_active: mix_color(Color32::from_rgb(200, 200, 210), Color32::from_rgb(184, 208, 232), factor),
        text: mix_color(LIGHT_TEXT, Color32::from_rgb(27, 43, 66), factor),
        text_dim: mix_color(LIGHT_TEXT_DIM, Color32::from_rgb(88, 106, 132), factor),
        selection: mix_color(Color32::from_rgb(255, 230, 190), Color32::from_rgb(176, 214, 248), factor),
        border: mix_color(LIGHT_BORDER, Color32::from_rgb(170, 198, 224), factor),
        separator: mix_color(Color32::from_rgb(225, 227, 232), Color32::from_rgb(186, 210, 232), factor),
        faint_bg: mix_color(Color32::from_rgb(235, 235, 240), Color32::from_rgb(218, 231, 244), factor),
    }
}

fn mix_color(from: Color32, to: Color32, factor: f32) -> Color32 {
    let factor = factor.clamp(0.0, 1.0);
    let red = lerp_u8(from.r(), to.r(), factor);
    let green = lerp_u8(from.g(), to.g(), factor);
    let blue = lerp_u8(from.b(), to.b(), factor);
    let alpha = lerp_u8(from.a(), to.a(), factor);
    Color32::from_rgba_unmultiplied(red, green, blue, alpha)
}

fn lerp_u8(from: u8, to: u8, factor: f32) -> u8 {
    (from as f32 + ((to as f32 - from as f32) * factor)).round() as u8
}

fn apply_dark(ctx: &egui::Context, palette: ThemePalette) {
    let mut visuals = Visuals::dark();

    // Window / panel backgrounds.
    visuals.dark_mode = true;
    visuals.panel_fill = palette.panel;
    visuals.window_fill = palette.panel;
    visuals.extreme_bg_color = palette.bg;
    visuals.faint_bg_color = palette.faint_bg;

    // Hyperlinks.
    visuals.hyperlink_color = ACCENT;

    // Selection.
    visuals.selection.bg_fill = palette.selection;
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);

    // Separator.
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(0.5, palette.separator);

    // Rounding for a modern look.
    let rounding = Rounding::same(6.0);
    let small_rounding = Rounding::same(4.0);

    // Non-interactive widgets (labels, frames).
    visuals.widgets.noninteractive.bg_fill = palette.panel;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, palette.text);
    visuals.widgets.noninteractive.rounding = small_rounding;

    // Inactive widgets (buttons when not hovered).
    visuals.widgets.inactive.bg_fill = palette.widget;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, palette.text);
    visuals.widgets.inactive.rounding = rounding;
    visuals.widgets.inactive.bg_stroke = Stroke::new(0.5, palette.border);
    visuals.widgets.inactive.weak_bg_fill = palette.widget;

    // Hovered widgets.
    visuals.widgets.hovered.bg_fill = palette.widget_hover;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.hovered.rounding = rounding;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.hovered.weak_bg_fill = palette.widget_hover;

    // Active (pressed) widgets.
    visuals.widgets.active.bg_fill = palette.widget_active;
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.active.rounding = rounding;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.active.weak_bg_fill = palette.widget_active;

    // Open (expanded/dropdown) widgets.
    visuals.widgets.open.bg_fill = palette.widget;
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.open.rounding = rounding;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, ACCENT);

    // Window.
    visuals.window_rounding = Rounding::same(8.0);
    visuals.window_stroke = Stroke::new(1.0, palette.border);
    visuals.menu_rounding = Rounding::same(6.0);

    // Apply.
    ctx.set_visuals(visuals);
}

fn apply_light(ctx: &egui::Context, palette: ThemePalette) {
    let mut visuals = Visuals::light();

    // Window / panel backgrounds.
    visuals.dark_mode = false;
    visuals.panel_fill = palette.panel;
    visuals.window_fill = palette.panel;
    visuals.extreme_bg_color = palette.bg;
    visuals.faint_bg_color = palette.faint_bg;

    // Hyperlinks.
    visuals.hyperlink_color = ACCENT;

    // Selection.
    visuals.selection.bg_fill = palette.selection;
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);

    // Rounding.
    let rounding = Rounding::same(6.0);
    let small_rounding = Rounding::same(4.0);

    // Non-interactive.
    visuals.widgets.noninteractive.bg_fill = palette.panel;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, palette.text);
    visuals.widgets.noninteractive.rounding = small_rounding;

    // Inactive.
    visuals.widgets.inactive.bg_fill = palette.widget;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, palette.text);
    visuals.widgets.inactive.rounding = rounding;
    visuals.widgets.inactive.bg_stroke = Stroke::new(0.5, palette.border);
    visuals.widgets.inactive.weak_bg_fill = palette.widget;

    // Hovered.
    visuals.widgets.hovered.bg_fill = palette.widget_hover;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, palette.text);
    visuals.widgets.hovered.rounding = rounding;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.hovered.weak_bg_fill = palette.widget_hover;

    // Active.
    visuals.widgets.active.bg_fill = palette.widget_active;
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, palette.text);
    visuals.widgets.active.rounding = rounding;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);

    // Window.
    visuals.window_rounding = Rounding::same(8.0);
    visuals.window_stroke = Stroke::new(1.0, palette.border);
    visuals.menu_rounding = Rounding::same(6.0);

    // Apply.
    ctx.set_visuals(visuals);
}
