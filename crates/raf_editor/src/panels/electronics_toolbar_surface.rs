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
) -> UiSurface {
    let mut root = UiNode::new("electronics.toolbar", UiNodeKind::Toolbar)
        .with_class("electronics-toolbar")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 4.0,
            padding: UiSpacing::xy(7.0, 5.0),
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
                UiIconId::Schematic,
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
                UiIconId::Move,
                tool == ElectronicsTool::Route,
            ))
            .with_child(tool_button(
                palette,
                "electronics.board-outline",
                "Board",
                UiIconId::Pcb,
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
            UiIconId::Scale,
        ))
        .with_child(icon_button(
            palette,
            "electronics.zoom-in",
            "Zoom in",
            UiIconId::Add,
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
            !has_selection,
        ))
        .with_child(action_icon_button(
            palette,
            "edit.delete",
            "Delete",
            UiIconId::Close,
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
    label: &str,
    icon: UiIconId,
    active: bool,
    class: &str,
) -> UiNode {
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
            padding: UiSpacing::xy(8.0, 4.0),
            ..UiLayout::fit_content().with_text_safe_area(true)
        })
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Toolbar))
        .with_text_value(label.to_string())
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

fn icon_button(_palette: StudioUiPalette, command: &str, _label: &str, icon: UiIconId) -> UiNode {
    UiNode::new(format!("electronics.toolbar.{command}"), UiNodeKind::Button)
        .with_class("electronics-icon-button")
        .with_layout(UiLayout::fixed(30.0, 30.0))
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Toolbar))
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
    UiNode::new(id, UiNodeKind::Panel).with_layout(UiLayout::fixed(1.0, 26.0))
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

fn style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    let classes = [
        (
            "electronics-toolbar",
            tokens.surface,
            tokens.border,
            tokens.text,
        ),
        (
            "electronics-mode-button",
            tokens.surface_alt,
            tokens.border,
            tokens.text_muted,
        ),
        (
            "electronics-tool-button",
            tokens.surface_alt,
            tokens.border,
            tokens.text_muted,
        ),
        (
            "electronics-toggle-button",
            tokens.surface_alt,
            tokens.border,
            tokens.text_muted,
        ),
        (
            "electronics-icon-button",
            tokens.surface_alt,
            tokens.border,
            tokens.text_muted,
        ),
        (
            "electronics-toolbar-active",
            [116, 67, 24, 100],
            tokens.accent,
            tokens.text,
        ),
    ];
    let mut rules = classes
        .into_iter()
        .map(|(class, fill, border, text)| {
            UiStyleRule::new(
                UiStyleSelector::Class(class.to_string()),
                UiStylePatch {
                    fill: Some(fill),
                    border: Some(border),
                    text: Some(text),
                    border_width: Some(1.0),
                    radius: Some(5.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always)
        })
        .collect::<Vec<_>>();
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
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.focus),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
        );
    }
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
        );
        assert_buttons_have_tooltips(&surface.root);
        assert!(surface.root.children.iter().any(|node| {
            node.id == "electronics.toolbar.electronics.pcb.sync"
                && node.tooltip_key.as_deref() == Some("electronics.tooltip.pcb_sync")
        }));
    }
}
