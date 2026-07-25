//! RafUI tooltip surface recipe.
//!
//! The bridge owns placement and presentation resources, but the tooltip's
//! document shape and visual language live here so they cannot become ad-hoc
//! painter code in the editor shell.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiFlow, UiNode, UiNodeKind, UiStyle, UiSurface,
};
use raf_ui::components::tooltip_node;

pub const TOOLTIP_HEIGHT: u32 = 32;
pub const TOOLTIP_GAP: f32 = 8.0;

pub fn build_surface(palette: StudioUiPalette, text_key: &str, opacity: f32) -> UiSurface {
    let root = UiNode::new("rafui.tooltip.root", UiNodeKind::Root)
        .with_layout(raf_ui::UiLayout::fill(UiFlow::Column))
        .with_style(UiStyle::transparent())
        .with_child(tooltip_node(
            "rafui.tooltip.overlay",
            text_key,
            palette,
            opacity,
        ));
    UiSurface::new("rafui.tooltip.overlay", palette, root).with_retained_tooltips(false)
}

pub fn logical_size_for_text(text: &str) -> [u32; 2] {
    [
        (text.chars().count() as f32 * 6.2 + 14.0)
            .clamp(52.0, 300.0)
            .ceil() as u32,
        TOOLTIP_HEIGHT,
    ]
}
