//! Retained RafUI toolbar for the native Electronics workspace.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiIcon, UiIconId, UiIconSize, UiSurface,
};
use raf_ui::{
    UiAlign, UiEventBinding, UiEventKind, UiFlow, UiFontWeight, UiLayout, UiNode, UiNodeKind,
    UiOverflow, UiSizeMode, UiSpacing, UiStylePatch, UiStyleRule, UiStyleRuleState,
    UiStyleSelector, UiStyleSheet, UiTextRole, UiTextStyle,
};

use crate::editor_layout::ELECTRONICS_TOOLBAR_HEIGHT;
use crate::electronics_controller::ElectronicsTool;
use raf_electronics::CadSurfaceKind;

pub fn build_electronics_toolbar_surface(
    palette: StudioUiPalette,
    surface: CadSurfaceKind,
    tool: ElectronicsTool,
    grid_visible: bool,
    labels_visible: bool,
    can_undo: bool,
    can_redo: bool,
    has_selection: bool,
    can_rotate: bool,
) -> UiSurface {
    let mut root = UiNode::new("electronics.toolbar", UiNodeKind::Toolbar)
        .with_class("electronics-toolbar")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 3.0,
            padding: UiSpacing::xy(6.0, 3.0),
            overflow: UiOverflow::ScrollX,
            ..UiLayout::fixed(0.0, ELECTRONICS_TOOLBAR_HEIGHT).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(mode_button(
            palette,
            "electronics.mode.schematic",
            "Schematic",
            UiIconId::Schematic,
            surface == CadSurfaceKind::Schematic,
        ))
        .with_child(mode_button(
            palette,
            "electronics.mode.pcb",
            "PCB",
            UiIconId::Pcb,
            surface == CadSurfaceKind::Pcb,
        ))
        .with_child(separator("electronics.toolbar.mode-separator"))
        .with_child(tool_button(
            palette,
            "electronics.select",
            "Select",
            UiIconId::Select,
            tool == ElectronicsTool::Select,
        ))
        .with_child(tool_button(
            palette,
            "electronics.pan",
            "Pan",
            UiIconId::Move,
            tool == ElectronicsTool::Pan,
        ));

    if surface == CadSurfaceKind::Schematic {
        root = root
            .with_child(tool_button(
                palette,
                "electronics.wire",
                "Wire",
                UiIconId::Wire,
                tool == ElectronicsTool::Wire,
            ))
            .with_child(tool_button(
                palette,
                "electronics.place",
                "Place",
                UiIconId::Add,
                tool == ElectronicsTool::Place,
            ));
    } else {
        root = root
            .with_child(tool_button(
                palette,
                "electronics.route",
                "Route",
                UiIconId::Route,
                tool == ElectronicsTool::Route,
            ))
            .with_child(tool_button(
                palette,
                "electronics.board-outline",
                "Board",
                UiIconId::BoardOutline,
                tool == ElectronicsTool::BoardOutline,
            ))
            .with_child(tool_button(
                palette,
                "electronics.place",
                "Place",
                UiIconId::Add,
                tool == ElectronicsTool::Place,
            ))
            .with_child(icon_button(
                palette,
                "electronics.pcb.sync",
                "Sync PCB from schematic",
                UiIconId::Refresh,
            ));
    }

    root = root
        .with_child(separator("electronics.toolbar.view-separator"))
        .with_child(icon_button(
            palette,
            "electronics.fit",
            "Fit view",
            UiIconId::Focus,
        ))
        .with_child(icon_button(
            palette,
            "electronics.zoom-out",
            "Zoom out",
            UiIconId::ZoomOut,
        ))
        .with_child(icon_button(
            palette,
            "electronics.zoom-in",
            "Zoom in",
            UiIconId::ZoomIn,
        ))
        .with_child(separator("electronics.toolbar.options-separator"))
        .with_child(toggle_button(
            palette,
            "electronics.grid.toggle",
            "Grid",
            UiIconId::Grid,
            grid_visible,
        ))
        .with_child(toggle_button(
            palette,
            "electronics.labels.toggle",
            "Labels",
            UiIconId::Eye,
            labels_visible,
        ))
        .with_child(separator("electronics.toolbar.edit-separator"))
        .with_child(action_icon_button(
            palette,
            "electronics.rotate",
            "Rotate",
            UiIconId::Rotate,
            !can_rotate,
        ))
        .with_child(action_icon_button(
            palette,
            "edit.delete",
            "Delete",
            UiIconId::Trash,
            !has_selection,
        ))
        .with_child(action_icon_button(
            palette,
            "edit.undo",
            "Undo",
            UiIconId::Undo,
            !can_undo,
        ))
        .with_child(action_icon_button(
            palette,
            "edit.redo",
            "Redo",
            UiIconId::Redo,
            !can_redo,
        ));

    let mut surface = UiSurface::new("electronics.toolbar", palette, root);
    surface.style_sheet = style_sheet(palette);
    surface
}

fn mode_button(
    palette: StudioUiPalette,
    command: &str,
    label: &str,
    icon: UiIconId,
    active: bool,
) -> UiNode {
    button(
        palette,
        command,
        label,
        icon,
        active,
        "electronics-mode-button",
    )
}

fn tool_button(
    palette: StudioUiPalette,
    command: &str,
    label: &str,
    icon: UiIconId,
    active: bool,
) -> UiNode {
    button(
        palette,
        command,
        label,
        icon,
        active,
        "electronics-tool-button",
    )
}

fn toggle_button(
    palette: StudioUiPalette,
    command: &str,
    label: &str,
    icon: UiIconId,
    active: bool,
) -> UiNode {
    button(
        palette,
        command,
        label,
        icon,
        active,
        "electronics-toggle-button",
    )
}

fn button(
    palette: StudioUiPalette,
    command: &str,
    _label: &str,
    icon: UiIconId,
    active: bool,
    class: &str,
) -> UiNode {
    let tokens = palette.tokens();
    let is_mode = class.contains("mode");
    let icon_tint = if active {
        if is_mode {
            [0, 180, 220, 255]
        } else {
            tokens.accent
        }
    } else {
        tokens.text_muted
    };
    UiNode::new(format!("electronics.toolbar.{command}"), UiNodeKind::Button)
        .with_class(class)
        .with_class(if active {
            "electronics-toolbar-active"
        } else {
            ""
        })
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 5.0,
            padding: UiSpacing::xy(8.0, 2.0),
            ..UiLayout::fixed(0.0, 26.0).with_text_safe_area(true)
        })
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Small).with_tint(icon_tint))
        .with_text_key(label_key(command))
        .with_text_style(UiTextStyle {
            role: UiTextRole::Button,
            size_px: 11.0,
            line_height_px: 15.0,
            weight: UiFontWeight::Bold,
            color: if active {
                palette.tokens().text
            } else {
                palette.tokens().text_muted
            },
            inherit_color: false,
        })
        .with_tooltip_key(tooltip_key(command))
        .with_accessibility_label_key(tooltip_key(command))
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn icon_button(palette: StudioUiPalette, command: &str, _label: &str, icon: UiIconId) -> UiNode {
    let tokens = palette.tokens();
    let is_delete = command.contains("delete");
    let class = if is_delete {
        "electronics-icon-button electronics-delete-button"
    } else {
        "electronics-icon-button"
    };
    UiNode::new(format!("electronics.toolbar.{command}"), UiNodeKind::Button)
        .with_class(class)
        .with_layout(UiLayout::fixed(26.0, 26.0))
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Small).with_tint(tokens.text_muted))
        .with_tooltip_key(tooltip_key(command))
        .with_accessibility_label_key(tooltip_key(command))
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn action_icon_button(
    palette: StudioUiPalette,
    command: &str,
    label: &str,
    icon: UiIconId,
    disabled: bool,
) -> UiNode {
    icon_button(palette, command, label, icon).disabled(disabled)
}

fn separator(id: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_class("electronics-toolbar-separator")
        .with_layout(UiLayout::fixed(1.0, 18.0))
}

fn tooltip_key(command: &str) -> &'static str {
    match command {
        "electronics.mode.schematic" => "electronics.tooltip.mode.schematic",
        "electronics.mode.pcb" => "electronics.tooltip.mode.pcb",
        "electronics.select" => "electronics.tooltip.select",
        "electronics.pan" => "electronics.tooltip.pan",
        "electronics.wire" => "electronics.tooltip.wire",
        "electronics.place" => "electronics.tooltip.place",
        "electronics.route" => "electronics.tooltip.route",
        "electronics.board-outline" => "electronics.tooltip.board_outline",
        "electronics.pcb.sync" => "electronics.tooltip.pcb_sync",
        "electronics.fit" => "electronics.tooltip.fit",
        "electronics.zoom-out" => "electronics.tooltip.zoom_out",
        "electronics.zoom-in" => "electronics.tooltip.zoom_in",
        "electronics.grid.toggle" => "electronics.tooltip.grid",
        "electronics.labels.toggle" => "electronics.tooltip.labels",
        "electronics.rotate" => "electronics.tooltip.rotate",
        "edit.delete" => "electronics.tooltip.delete",
        "edit.undo" => "electronics.tooltip.undo",
        "edit.redo" => "electronics.tooltip.redo",
        _ => "electronics.tooltip.command",
    }
}

fn label_key(command: &str) -> &'static str {
    match command {
        "electronics.mode.schematic" => "electronics.toolbar.schematic",
        "electronics.mode.pcb" => "electronics.toolbar.pcb",
        "electronics.select" => "electronics.toolbar.select",
        "electronics.pan" => "electronics.toolbar.pan",
        "electronics.wire" => "electronics.toolbar.wire",
        "electronics.place" => "electronics.toolbar.place",
        "electronics.route" => "electronics.toolbar.route",
        "electronics.board-outline" => "electronics.toolbar.board",
        "electronics.grid.toggle" => "electronics.toolbar.grid",
        "electronics.labels.toggle" => "electronics.toolbar.labels",
        _ => "electronics.tooltip.command",
    }
}

fn style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    let mut rules = vec![
        UiStyleRule::new(
            UiStyleSelector::Class("electronics-toolbar".to_string()),
            UiStylePatch {
                fill: Some(tokens.surface),
                border: Some(tokens.border),
                text: Some(tokens.text),
                border_width: Some(1.0),
                radius: Some(6.0),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Always),
        UiStyleRule::new(
            UiStyleSelector::Class("electronics-toolbar-separator".to_string()),
            UiStylePatch {
                fill: Some([tokens.border[0], tokens.border[1], tokens.border[2], 120]),
                border: Some([0, 0, 0, 0]),
                text: Some([0, 0, 0, 0]),
                border_width: Some(0.0),
                radius: Some(0.0),
                opacity: Some(0.65),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Always),
    ];

    for class in [
        "electronics-mode-button",
        "electronics-tool-button",
        "electronics-toggle-button",
        "electronics-icon-button",
    ] {
        rules.push(
            UiStyleRule::new(
                UiStyleSelector::Class(class.to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    text: Some(tokens.text_muted),
                    border_width: Some(0.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
        );
        rules.push(
            UiStyleRule::new(
                UiStyleSelector::Class(class.to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some([0, 0, 0, 0]),
                    text: Some(tokens.text),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
        );
    }

    rules.push(
        UiStyleRule::new(
            UiStyleSelector::Class("electronics-toolbar-active".to_string()),
            UiStylePatch {
                fill: Some(tokens.surface_raised),
                border: Some(tokens.border),
                text: Some(tokens.text),
                border_width: Some(1.0),
                radius: Some(4.0),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Always),
    );

    rules.push(
        UiStyleRule::new(
            UiStyleSelector::Class("electronics-delete-button".to_string()),
            UiStylePatch {
                fill: Some([180, 45, 45, 90]),
                border: Some([0, 0, 0, 0]),
                text: Some([255, 200, 200, 255]),
                radius: Some(4.0),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Hovered),
    );

    rules.push(
        UiStyleRule::new(
            UiStyleSelector::Class("electronics-icon-button".to_string()),
            UiStylePatch {
                fill: Some([0, 0, 0, 0]),
                border: Some([0, 0, 0, 0]),
                text: Some(tokens.text_muted),
                opacity: Some(0.35),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Disabled),
    );

    UiStyleSheet { rules }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_buttons_have_tooltips(node: &UiNode) {
        if node.kind == UiNodeKind::Button {
            assert!(
                node.tooltip_key.is_some(),
                "Electronics toolbar button {} must expose a localized tooltip",
                node.id
            );
        }
        for child in &node.children {
            assert_buttons_have_tooltips(child);
        }
    }

    #[test]
    fn every_schematic_toolbar_action_exposes_a_localized_tooltip() {
        let surface = build_electronics_toolbar_surface(
            StudioUiPalette::IndustrialDark,
            CadSurfaceKind::Schematic,
            ElectronicsTool::Select,
            true,
            true,
            true,
            true,
            false,
            false,
        );

        assert_buttons_have_tooltips(&surface.root);
        assert_eq!(
            surface
                .root
                .children
                .iter()
                .find(|node| node.id == "electronics.toolbar.electronics.grid.toggle")
                .and_then(|node| node.tooltip_key.as_deref()),
            Some("electronics.tooltip.grid")
        );
    }

    #[test]
    fn pcb_toolbar_keeps_mode_specific_actions_described() {
        let surface = build_electronics_toolbar_surface(
            StudioUiPalette::IndustrialDark,
            CadSurfaceKind::Pcb,
            ElectronicsTool::Route,
            true,
            true,
            false,
            false,
            true,
            true,
        );
        assert_buttons_have_tooltips(&surface.root);
        assert!(surface.root.children.iter().any(|node| {
            node.id == "electronics.toolbar.electronics.pcb.sync"
                && node.tooltip_key.as_deref() == Some("electronics.tooltip.pcb_sync")
        }));
        assert!(surface.root.children.iter().any(|node| {
            node.id == "electronics.toolbar.electronics.route"
                && node.icon.is_some_and(|icon| icon.id == UiIconId::Route)
        }));
    }
}
