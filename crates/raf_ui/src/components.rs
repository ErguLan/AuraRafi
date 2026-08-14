//! Small semantic component recipes.
//!
//! These helpers keep repeated editor vocabulary in one place. They return
//! ordinary `UiNode`s, so authors can still add classes, controls, and event
//! bindings without introducing a second widget system.

use crate::events::{UiEventBinding, UiEventKind};
use crate::geometry::UiSpacing;
use crate::icons::{UiIcon, UiIconId, UiIconSize};
use crate::layout::{UiAlign, UiFlow, UiJustify, UiLayout, UiSizeMode};
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

/// Icon-button recipe with a semantic icon request. The icon stays data-only;
/// ApiGraphicBasic owns its high-density rasterization and sampling policy.
pub fn icon_button_with_icon(
    id: impl Into<String>,
    icon: UiIconId,
    command: impl Into<String>,
    tooltip_key: impl Into<String>,
) -> UiNode {
    icon_button(id, command, tooltip_key).with_icon(UiIcon::new(icon))
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
            inherit_color: true,
        })
}

pub fn tree_row(id: impl Into<String>, text_key: impl Into<String>) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("tree-row")
        .with_layout(UiLayout::fixed(0.0, 28.0))
        .with_text_key(text_key)
        .focusable()
}

/// Hierarchy row variant for documents that already know the entity kind.
pub fn tree_row_with_icon(
    id: impl Into<String>,
    text_key: impl Into<String>,
    icon: UiIconId,
) -> UiNode {
    tree_row(id, text_key).with_icon(UiIcon::new(icon).with_size(crate::icons::UiIconSize::Small))
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
        .with_text_style(UiTextStyle::button([237, 239, 242, 255]).inherit_theme_color())
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
/// added by the owning surface so authoring tools can inspect it as an
/// ordinary child.
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

/// Compact disclosure header used by retained Inspector sections.
///
/// The command remains owned by the surface host. Keeping expansion as a
/// semantic command avoids coupling the shared recipe to editor state.
pub fn disclosure_header(
    id: impl Into<String>,
    label_key: impl Into<String>,
    icon: UiIconId,
    expanded: bool,
    command: impl Into<String>,
    palette: StudioUiPalette,
) -> UiNode {
    let id = id.into();
    let label_key = label_key.into();
    let tokens = palette.tokens();
    UiNode::new(id.clone(), UiNodeKind::Button)
        .with_class("disclosure-header")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 5.0,
            padding: UiSpacing::xy(6.0, 0.0),
            ..UiLayout::fixed(0.0, 27.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new(format!("{id}.chevron"), UiNodeKind::Label)
                .with_icon(
                    UiIcon::new(if expanded {
                        UiIconId::ChevronDown
                    } else {
                        UiIconId::ChevronRight
                    })
                    .with_size(UiIconSize::Small)
                    .with_tint(tokens.text_muted),
                )
                .with_layout(UiLayout::fixed(14.0, 18.0)),
        )
        .with_child(
            UiNode::new(format!("{id}.icon"), UiNodeKind::Label)
                .with_icon(
                    UiIcon::new(icon)
                        .with_size(UiIconSize::Small)
                        .with_tint(tokens.text_muted),
                )
                .with_layout(UiLayout::fixed(16.0, 18.0)),
        )
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key.clone())
                .with_text_style(UiTextStyle::panel_title(tokens.text_muted))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content().with_text_safe_area(true)
                }),
        )
        .with_tooltip_key(label_key)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

/// Retained combobox trigger. Options are authored as ordinary menu nodes by
/// the owning surface, so dropdowns remain inspectable and backend-agnostic.
pub fn dropdown_trigger(
    id: impl Into<String>,
    selected_label_key: impl Into<String>,
    command: impl Into<String>,
    expanded: bool,
    palette: StudioUiPalette,
) -> UiNode {
    let id = id.into();
    let selected_label_key = selected_label_key.into();
    let command = command.into();
    let tokens = palette.tokens();
    let mut trigger = UiNode::new(id.clone(), UiNodeKind::Button)
        .with_class("dropdown-trigger")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 5.0,
            padding: UiSpacing::xy(8.0, 0.0),
            ..UiLayout::fixed(0.0, 29.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new(format!("{id}.value"), UiNodeKind::Label)
                .with_text_key(selected_label_key.clone())
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content().with_text_safe_area(true)
                }),
        )
        .with_child(
            UiNode::new(format!("{id}.chevron"), UiNodeKind::Label)
                .with_icon(
                    UiIcon::new(if expanded {
                        UiIconId::ChevronDown
                    } else {
                        UiIconId::ChevronRight
                    })
                    .with_size(UiIconSize::Small)
                    .with_tint(tokens.text_muted),
                )
                .with_layout(UiLayout::fixed(16.0, 18.0)),
        )
        .with_tooltip_key(selected_label_key)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command.clone()));
    if expanded {
        trigger = trigger.with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            command,
        ));
    }
    trigger
}

/// One full-width option inside a retained dropdown menu.
pub fn dropdown_option(
    id: impl Into<String>,
    label_key: impl Into<String>,
    command: impl Into<String>,
    selected: bool,
    palette: StudioUiPalette,
) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(id, UiNodeKind::Button)
        .with_class(if selected {
            "dropdown-option-selected"
        } else {
            "dropdown-option"
        })
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Start,
            padding: UiSpacing::xy(8.0, 0.0),
            ..UiLayout::fixed(0.0, 27.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::body(if selected {
            tokens.text
        } else {
            tokens.text_muted
        }))
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
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
        .with_text_style(UiTextStyle::button([237, 239, 242, 255]).inherit_theme_color())
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
            inherit_color: false,
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
    fn semantic_icon_recipe_does_not_require_a_png_path() {
        let button =
            icon_button_with_icon("grid", UiIconId::Grid, "viewport.grid", "ui.tooltip.grid");
        assert_eq!(button.icon.map(|icon| icon.id), Some(UiIconId::Grid));
    }

    #[test]
    fn dropdown_trigger_exposes_selection_and_keyboard_escape() {
        let trigger = dropdown_trigger(
            "shape",
            "app.primitive_cube",
            "shape.toggle",
            false,
            StudioUiPalette::IndustrialDark,
        );

        assert!(trigger.focusable);
        assert!(!trigger.event_handlers.iter().any(|binding| {
            matches!(binding.event, UiEventKind::KeyPress(ref key) if key == "escape")
        }));
        let expanded = dropdown_trigger(
            "shape.open",
            "app.primitive_cube",
            "shape.toggle",
            true,
            StudioUiPalette::IndustrialDark,
        );
        assert!(expanded.event_handlers.iter().any(|binding| {
            matches!(binding.event, UiEventKind::KeyPress(ref key) if key == "escape")
        }));
        assert_eq!(
            trigger.children[0].text_key.as_deref(),
            Some("app.primitive_cube")
        );
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

    #[test]
    fn literal_tooltip_value_is_kept_out_of_translation_keys() {
        let node = UiNode::new("metadata", UiNodeKind::Label).with_tooltip_value("World_Rafi");

        assert_eq!(node.tooltip_value.as_deref(), Some("World_Rafi"));
        assert!(node.tooltip_key.is_none());
    }
}
