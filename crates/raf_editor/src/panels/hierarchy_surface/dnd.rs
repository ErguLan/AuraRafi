//! Drag ghost and box-selection overlays of the retained Hierarchy.

use raf_render::api_graphic_basic::ui_surface::StudioUiPalette;
use raf_ui::{UiLayout, UiNode, UiNodeKind, UiStyle, UiTextStyle};

pub(super) fn drag_ghost_node(palette: StudioUiPalette, label: &str, pointer: [f32; 2]) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("hierarchy.drag-ghost", UiNodeKind::FloatingPanel)
        .with_class("hierarchy-drag-ghost")
        .with_layout(
            UiLayout::absolute(raf_ui::UiRect::new(
                (pointer[0] + 12.0).max(6.0),
                (pointer[1] + 12.0).max(6.0),
                188.0,
                28.0,
            ))
            .with_z_index(260),
        )
        .with_text_value(label.to_string())
        .with_text_style(UiTextStyle::body(tokens.text))
}

pub(super) fn box_selection_node(palette: StudioUiPalette, selection: raf_ui::UiRect) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("hierarchy.box-selection", UiNodeKind::FloatingPanel)
        .with_class("hierarchy-box-selection")
        .with_layout(UiLayout::absolute(selection).with_z_index(240))
        .with_style(UiStyle {
            fill: [232, 133, 28, 24],
            border: tokens.accent,
            text: tokens.accent,
            border_width: 1.0,
            radius: 2.0,
            opacity: 0.72,
        })
}
