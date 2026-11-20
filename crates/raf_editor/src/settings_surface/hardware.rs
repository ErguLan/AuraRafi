//! Electronics workspace settings.

use raf_core::config::EngineSettings;
use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiNode};

use super::controls::{
    card, group_label, range_row, range_row_with_description, toggle_row, toggle_row_with_key,
};

pub(super) fn electronics(palette: StudioUiPalette, settings: &EngineSettings) -> UiNode {
    card(palette, "settings.electronics")
        .with_child(group_label(palette, "settings.group.electronics"))
        .with_child(toggle_row(
            palette,
            "settings.electronics-grid-visible",
            "settings.show_grid",
            settings.grid_visible,
        ))
        .with_child(range_row(
            palette,
            "settings.electronics-grid-step",
            "settings.electronics_grid_step_mm",
            settings.electronics_grid_step_mm,
            5.0,
            100.0,
            1.0,
            format!("{:.0} mm", settings.electronics_grid_step_mm),
        ))
        .with_child(range_row_with_description(
            palette,
            "settings.electronics-grid-opacity",
            "settings.electronics_grid_opacity",
            settings.electronics_grid_opacity,
            0.2,
            1.0,
            0.05,
            format!("{:.0}%", settings.electronics_grid_opacity * 100.0),
            "settings.electronics_grid_opacity_desc",
        ))
        .with_child(toggle_row_with_key(
            palette,
            "settings.electronics-status",
            "settings.electronics-status",
            "settings.electronics_show_status",
            settings.electronics_show_status,
        ))
}
