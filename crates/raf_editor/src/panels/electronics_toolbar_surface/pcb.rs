//! PCB tools and desktop preview controls of the Electronics toolbar band.

use raf_electronics::CadSurfaceKind;
use raf_render::api_graphic_basic::ui_surface::{
    UiAccessibilityRole, UiIcon, UiIconId,
};
use raf_ui::{
    UiEventBinding, UiEventKind, UiLayout, UiNode, UiNodeKind, UiTextStyle, UiTokens,
};

use super::{separator, ToolbarControl, CLASS_BUTTON};

/// The PCB option of the document segment.
pub(super) fn mode_option() -> (ToolbarControl, CadSurfaceKind) {
    (
        ToolbarControl::segment(
            "electronics.mode.pcb",
            UiIconId::Pcb,
            Some("electronics.toolbar.pcb"),
        ),
        CadSurfaceKind::Pcb,
    )
}

/// PCB tools, appended after the shared selection and pan tools.
pub(super) fn document_tools(controls: &mut Vec<ToolbarControl>) {
    controls.push(ToolbarControl::tool(
        "electronics.route",
        UiIconId::CadRoute,
        Some("electronics.toolbar.route"),
    ));
    controls.push(ToolbarControl::command("electronics.pcb.via", UiIconId::CadVia,
        Some("electronics.pcb.via_label"), true));
    controls.push(ToolbarControl::tool(
        "electronics.board-outline",
        UiIconId::CadOutline,
        Some("app.pcb_outline_status"),
    ));
    controls.push(ToolbarControl::tool(
        "electronics.place",
        UiIconId::CadPlace,
        Some("electronics.toolbar.place"),
    ));
    // Synchronizing footprints is a one-way projection of the schematic
    // onto the board, not a view action, so it belongs with the tools
    // even though its glyph reads like a refresh. Entering the PCB
    // surface already projects it, which is why it stays a glyph: it is
    // the least frequent command of the band.
    controls.push(ToolbarControl::command(
        "electronics.pcb.sync",
        UiIconId::Refresh,
        None,
        true,
    ));
    controls.push(ToolbarControl::command("electronics.pcb.menu:rules", UiIconId::Settings,
        None, true));
}

/// PCB-only preview, layers, export and 3D controls of the wide ribbon.
pub(super) fn append_desktop_view_controls(
    root: UiNode,
    tokens: &UiTokens,
    surface: CadSurfaceKind,
    width: f32,
) -> UiNode {
    let mut root = root;
    if surface == CadSurfaceKind::Pcb && width >= 1320.0 {
        root = root.with_child(separator("electronics.toolbar.view-separator"));
        for (id, key, text, command, active) in [
            (
                "2d",
                "electronics.desktop.view2d",
                "2D",
                "electronics.pcb.preview.close",
                true,
            ),
            (
                "3d",
                "electronics.desktop.preview_beta",
                "3D",
                "electronics.pcb.preview.toggle",
                false,
            ),
            (
                "layers",
                "electronics.rail.layers",
                "",
                "electronics.navigator.tab:layers",
                false,
            ),
            (
                "export",
                "electronics.pcb.fabrication",
                "",
                "electronics.fabrication.export",
                false,
            ),
        ] {
            let mut button = UiNode::new(format!("electronics.toolbar.{id}"), UiNodeKind::Button)
                .with_layout(UiLayout::fixed(
                    if id == "layers" || id == "export" {
                        88.0
                    } else {
                        38.0
                    },
                    32.0,
                ))
                .with_text_style(UiTextStyle {
                    size_px: 13.0,
                    line_height_px: 18.0,
                    ..UiTextStyle::button(if active {
                        tokens.background
                    } else {
                        tokens.text
                    })
                })
                .with_class(if active {
                    "electronics-desktop-mode-active"
                } else {
                    CLASS_BUTTON
                })
                .with_accessibility_role(UiAccessibilityRole::Button)
                .with_accessibility_label_key(key)
                .with_tooltip_key(key)
                .focusable()
                .with_event(UiEventBinding::command(UiEventKind::Click, command));
            button = if text.is_empty() {
                button.with_text_key(key)
            } else {
                button.with_text_value(text)
            };
            root = root.with_child(button);
        }
    }
    if surface == CadSurfaceKind::Pcb && width < 1320.0 {
        root = root.with_child(
            UiNode::new("electronics.toolbar.3d", UiNodeKind::Button)
                .with_icon(UiIcon::new(UiIconId::Cube))
                .with_layout(UiLayout::fixed(28.0, 32.0))
                .with_accessibility_label_key("electronics.desktop.preview_beta")
                .with_tooltip_key("electronics.desktop.preview_beta")
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "electronics.pcb.preview.toggle",
                )),
        );
    }
    root
}
