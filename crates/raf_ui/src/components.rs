//! Small semantic component recipes.
//!
//! These helpers keep repeated editor vocabulary in one place. They return
//! ordinary `UiNode`s, so authors can still add classes, controls, and event
//! bindings without introducing a second widget system.

use crate::events::{UiEventBinding, UiEventKind};
use crate::layout::{UiAlign, UiFlow, UiLayout, UiSizeMode};
use crate::node::{UiNode, UiNodeKind};
use crate::style::{StudioUiPalette, UiStyle};
use crate::text::{UiFontWeight, UiTextRole, UiTextStyle};

pub fn icon_button(
    id: impl Into<String>,
    command: impl Into<String>,
    tooltip_key: impl Into<String>,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("icon-button")
        .with_layout(UiLayout::fixed(32.0, 32.0))
        .focusable()
        .with_tooltip_key(tooltip_key)
        .with_accessibility_label_key("ui.icon_button")
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

pub fn panel_header(id: impl Into<String>, text_key: impl Into<String>) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_class("panel-header")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            ..UiLayout::fixed(0.0, 32.0)
        })
        .with_text_key(text_key)
        .with_text_style(UiTextStyle {
            role: UiTextRole::PanelTitle,
            size_px: 12.0,
            line_height_px: 16.0,
            weight: UiFontWeight::Bold,
            color: [237, 239, 242, 255],
        })
}

pub fn tree_row(id: impl Into<String>, text_key: impl Into<String>) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("tree-row")
        .with_layout(UiLayout::fixed(0.0, 28.0))
        .with_text_key(text_key)
        .focusable()
}

/// Structural command row shared by viewport and editor shell surfaces.
pub fn technical_toolbar(id: impl Into<String>) -> UiNode {
    UiNode::new(id, UiNodeKind::Toolbar)
        .with_class("technical-toolbar")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 4.0,
            ..UiLayout::fixed(0.0, 32.0)
        })
}

/// One option inside a segmented control. The parent owns the selected state.
pub fn segmented_option(
    id: impl Into<String>,
    text_key: impl Into<String>,
    command: impl Into<String>,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("segmented-option")
        .with_layout(UiLayout::fit_content())
        .with_text_key(text_key)
        .with_text_style(UiTextStyle::button([237, 239, 242, 255]))
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

/// Lower-canvas action cluster. It is intentionally a structural overlay so
/// it cannot expand the viewport or change scene coordinates.
pub fn floating_action_rail(id: impl Into<String>) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_class("floating-action-rail")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 2.0,
            ..UiLayout::fixed(0.0, 36.0).with_z_index(40)
        })
}

/// Label/value container used by inspector sections. The value control is
/// added by the surface so RafUI Studio can inspect it as an ordinary child.
pub fn inspector_field(id: impl Into<String>, label_key: impl Into<String>) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_class("inspector-field")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            ..UiLayout::fixed(0.0, 28.0)
        })
        .with_text_key(label_key)
}

pub fn editor_tab(
    id: impl Into<String>,
    text_key: impl Into<String>,
    command: impl Into<String>,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("editor-tab")
        .with_layout(UiLayout::fit_content())
        .with_text_key(text_key)
        .with_text_style(UiTextStyle::button([237, 239, 242, 255]))
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

pub fn empty_state(id: impl Into<String>, text_key: impl Into<String>) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_class("empty-state")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Center,
            justify_content: crate::layout::UiJustify::Center,
            padding: crate::geometry::UiSpacing::xy(16.0, 16.0),
            ..UiLayout::fit_content()
        })
        .with_text_key(text_key)
        .with_text_style(UiTextStyle::body([151, 159, 170, 255]))
}

/// Builds the visual node used by every retained tooltip host. The host is
/// responsible for placing the node in a global overlay layer; this function
/// owns the compact design language and keeps it out of bridge code.
pub fn tooltip_node(
    id: impl Into<String>,
    text_key: impl Into<String>,
    palette: StudioUiPalette,
    opacity: f32,
) -> UiNode {
    let tokens = palette.tokens();
    let fill = match palette {
        StudioUiPalette::IndustrialDark => [58, 61, 65, 236],
        StudioUiPalette::PaperLight => [70, 73, 78, 232],
    };
    let border = match palette {
        StudioUiPalette::IndustrialDark => [104, 108, 115, 228],
        StudioUiPalette::PaperLight => [136, 140, 148, 224],
    };
    UiNode::new(id, UiNodeKind::Tooltip)
        .with_class("tooltip")
        .with_text_key(text_key)
        .with_layout(
            UiLayout::fit_content()
                .with_width_mode(UiSizeMode::FitContent)
                .with_height_mode(UiSizeMode::FitContent),
        )
        .with_style(UiStyle {
            fill,
            border,
            text: tokens.text,
            border_width: 1.0,
            radius: 4.0,
            opacity: opacity.clamp(0.0, 1.0),
        })
        .with_text_style(UiTextStyle {
            role: UiTextRole::Tooltip,
            size_px: 10.5,
            line_height_px: 14.0,
            weight: UiFontWeight::Regular,
            color: tokens.text,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_button_is_keyboard_accessible_and_has_tooltip_semantics() {
        let button = icon_button("grid", "viewport.grid", "ui.tooltip.grid");
        assert!(button.focusable);
        assert_eq!(button.tooltip_key.as_deref(), Some("ui.tooltip.grid"));
        assert!(button.accessibility_label_key.is_some());
    }

    #[test]
    fn tooltip_recipe_is_intrinsic_and_compact() {
        let tooltip = tooltip_node(
            "tooltip",
            "ui.tooltip.grid",
            StudioUiPalette::IndustrialDark,
            1.0,
        );
        assert_eq!(tooltip.kind, UiNodeKind::Tooltip);
        assert_eq!(tooltip.layout.width_mode, UiSizeMode::FitContent);
        assert_eq!(tooltip.layout.height_mode, UiSizeMode::FitContent);
        assert_eq!(tooltip.layout.basis, [0.0, 0.0]);
    }
}
