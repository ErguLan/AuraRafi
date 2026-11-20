//! Wide desktop ribbon and document selector of the Electronics toolbar.
//!
//! The ribbon is the wide-window presentation: it always prints the localized
//! mode labels and keeps the tool, history and navigation groups on one track.
//! The document segment is shared with the compact band, so both densities
//! answer "which document am I editing" with the same control and treatment.

use raf_electronics::CadSurfaceKind;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiIcon, UiIconId, UiIconSize, UiSurface, UiTextOverflow,
};
use raf_ui::{
    UiAlign, UiFlow, UiFontWeight, UiJustify, UiLayout, UiNode, UiNodeKind, UiOverflow, UiSizeMode,
    UiSpacing, UiTextRole, UiTextStyle,
};

use crate::panels::electronics_surface::{
    electronics_class_rule, electronics_focus_rule, electronics_hover_rule, ELECTRONICS_BORDER_WIDTH,
};

use super::actions::{
    control_button, document_controls, history_controls, tool_is_engaged, view_controls,
};
use super::pcb;
use super::schematic;
use super::styles::style_sheet;
use super::{
    separator, ControlKind, ToolbarControl, ToolbarDensity, ToolbarState, SEGMENT_GAP,
    SEGMENT_INSET,
};

pub(super) fn build_desktop_ribbon(
    palette: StudioUiPalette,
    state: &ToolbarState,
    width: f32,
) -> UiSurface {
    let t = crate::panels::electronics_surface::electronics_tokens(palette);
    let mut root = UiNode::new("electronics.toolbar", UiNodeKind::Toolbar)
        .with_class("electronics-toolbar")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 4.0,
            padding: UiSpacing::xy(8.0, 6.0),
            overflow: UiOverflow::Clip,
            ..UiLayout::fixed(width, crate::editor_layout::ELECTRONICS_TOOLBAR_HEIGHT)
        });
    for (command, key, icon, active, w) in [
        (
            "electronics.mode.schematic",
            "electronics.toolbar.schematic",
            UiIconId::Schematic,
            state.surface == CadSurfaceKind::Schematic,
            132.0,
        ),
        (
            "electronics.mode.pcb",
            "electronics.toolbar.pcb",
            UiIconId::Pcb,
            state.surface == CadSurfaceKind::Pcb,
            76.0,
        ),
    ] {
        let c = ToolbarControl::segment(command, icon, Some(key));
        let mut node = control_button(palette, &c, ToolbarDensity::Labelled, 34.0, active);
        node.layout = UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            gap: 4.0,
            padding: UiSpacing::xy(6.0, 0.0),
            ..UiLayout::fixed(w, 34.0)
        };
        if active {
            node = node.with_class("electronics-desktop-mode-active");
        }
        node.icon = None;
        node.text_key = None;
        node = node
            .with_child(
                UiNode::new(format!("{command}.glyph"), UiNodeKind::Label)
                    .with_icon(
                        UiIcon::new(icon)
                            .with_size(UiIconSize::Custom(22))
                            .with_tint(if active { t.background } else { t.text }),
                    )
                    .with_layout(UiLayout::fixed(24.0, 24.0)),
            )
            .with_child(
                UiNode::new(format!("{command}.label"), UiNodeKind::Label)
                    .with_text_key(key)
                    .with_text_style(UiTextStyle {
                        role: UiTextRole::Label,
                        size_px: 13.0,
                        line_height_px: 18.0,
                        weight: UiFontWeight::Medium,
                        color: if active { t.background } else { t.text },
                        inherit_color: false,
                    })
                    .with_text_overflow(UiTextOverflow::Ellipsis)
                    .with_layout(UiLayout::fixed(w - 40.0, 22.0).with_text_safe_area(false)),
            );
        root = root.with_child(node);
    }
    root = root.with_child(separator("electronics.toolbar.surface-separator"));
    for mut c in document_controls(state) {
        if c.command == "electronics.pan" {
            c.label = Some("electronics.toolbar.pan");
        }
        let active = c.kind == ControlKind::Tool && tool_is_engaged(c.command, state.tool);
        let mut node = control_button(palette, &c, ToolbarDensity::Icon, 44.0, active);
        if let Some(key) = c.label {
            let w = if width < 1320.0 {
                match c.command {
                    "electronics.select" => 78.0,
                    "electronics.board-outline" => 72.0,
                    _ => 64.0,
                }
            } else {
                match c.command {
                    "electronics.select" => 82.0,
                    "electronics.board-outline" => 80.0,
                    _ => 68.0,
                }
            };
            node.text_key = None;
            node.icon = None;
            node.layout = UiLayout {
                flow: UiFlow::Column,
                align_items: UiAlign::Center,
                justify_content: UiJustify::Center,
                gap: 2.0,
                padding: UiSpacing::xy(2.0, 0.0),
                ..UiLayout::fixed(w, 48.0)
            };
            let color = if active { t.background } else { t.text };
            node = node
                .with_child(
                    UiNode::new(format!("{}.glyph", c.id()), UiNodeKind::Label)
                        .with_icon(
                            UiIcon::new(c.icon)
                                .with_size(UiIconSize::Custom(28))
                                .with_tint(color),
                        )
                        .with_layout(UiLayout::fixed(28.0, 28.0)),
                )
                .with_child(
                    UiNode::new(format!("{}.label", c.id()), UiNodeKind::Label)
                        .with_text_key(key)
                        .with_text_style(UiTextStyle {
                            role: UiTextRole::Label,
                            size_px: 13.0,
                            line_height_px: 18.0,
                            ..UiTextStyle::button(color)
                        })
                        .with_text_overflow(UiTextOverflow::Ellipsis)
                        .with_layout(
                            UiLayout::fixed(0.0, 18.0)
                                .with_width_mode(UiSizeMode::FitContent)
                                .with_text_safe_area(false),
                        ),
                );
        } else {
            node.layout = UiLayout::fixed(32.0, 34.0);
            node.icon = Some(UiIcon::new(c.icon).with_size(UiIconSize::Custom(22)));
        }
        if c.kind == ControlKind::Tool && !active {
            node = node.with_class("electronics-desktop-resting-tool");
        }
        root = root.with_child(node);
    }
    root = root.with_child(separator("electronics.toolbar.history-separator"));
    for c in history_controls(state) {
        let mut node = control_button(palette, &c, ToolbarDensity::Icon, 34.0, false);
        node.layout = UiLayout::fixed(32.0, 34.0);
        node.icon = Some(UiIcon::new(c.icon).with_size(UiIconSize::Custom(22)));
        root = root.with_child(node);
    }
    root = root.with_child(
        UiNode::new("electronics.toolbar.flex", UiNodeKind::Panel)
            .with_layout(UiLayout::fixed(0.0, 30.0).fill_width().grow(1.0)),
    );
    root = root.with_child(separator("electronics.toolbar.navigation-separator"));
    for (c, latched) in view_controls(state) {
        let mut node = control_button(palette, &c, ToolbarDensity::Icon, 32.0, latched);
        node.layout = UiLayout::fixed(if width < 1320.0 { 28.0 } else { 32.0 }, 34.0);
        node.icon = Some(
            UiIcon::new(c.icon)
                .with_size(UiIconSize::Custom(22))
                .with_tint(if latched { t.accent } else { t.text_muted }),
        );
        root = root.with_child(node);
    }
    root = pcb::append_desktop_view_controls(root, &t, state.surface, width);
    let mut surface = UiSurface::new("electronics.toolbar", palette, root);
    surface.style_sheet = style_sheet(&t);
    surface.style_sheet.rules.push(electronics_class_rule(
        "electronics-desktop-resting-tool",
        [0, 0, 0, 0],
        [0, 0, 0, 0],
        t.text,
    ));
    surface.style_sheet.rules.push(electronics_hover_rule(
        "electronics-desktop-resting-tool",
        t,
    ));
    surface.style_sheet.rules.push(electronics_focus_rule(
        "electronics-desktop-resting-tool",
        t,
    ));
    surface.style_sheet.rules.push(electronics_class_rule(
        "electronics-desktop-mode-active",
        t.accent,
        t.accent,
        t.background,
    ));
    surface
}

/// The schematic/PCB selector.
///
/// It is the only bordered container of the band, so "which document am I
/// editing" cannot be mistaken for a tool. A hairline separates it from the
/// first tool island, and its engaged option uses the pale selection wash.
pub(super) fn document_segment(
    palette: StudioUiPalette,
    state: &ToolbarState,
    density: ToolbarDensity,
    height: f32,
) -> UiNode {
    let option_height = height - 2.0 * (SEGMENT_INSET + ELECTRONICS_BORDER_WIDTH);
    let options = [schematic::mode_option(), pcb::mode_option()];
    let mut segment = UiNode::new("electronics.toolbar.segment", UiNodeKind::Toolbar)
        .with_class("electronics-toolbar-segment")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: SEGMENT_GAP,
            padding: UiSpacing::same(SEGMENT_INSET),
            height_mode: UiSizeMode::Fixed,
            basis: [0.0, height],
            width_mode: UiSizeMode::FitContent,
            ..UiLayout::default()
        });
    for (control, kind) in &options {
        segment = segment.with_child(control_button(
            palette,
            control,
            density,
            option_height,
            state.surface == *kind,
        ));
    }
    segment
}
