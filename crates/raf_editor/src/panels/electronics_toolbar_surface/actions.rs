//! Control table and node builders of the Electronics toolbar band.
//!
//! Every control of the band is declared once here, and one shared builder turns
//! a declared control into a retained button with its tooltip, accessible role
//! and engaged treatment. Surface-specific tools live in `schematic` and `pcb`.

use raf_electronics::CadSurfaceKind;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAccessibilityRole, UiIcon, UiIconId, UiIconSize, UiTextOverflow,
};
use raf_ui::{
    UiAlign, UiEventBinding, UiEventKind, UiFlow, UiFontWeight, UiJustify, UiLayout, UiNode,
    UiNodeKind, UiSizeMode, UiSpacing, UiTextRole, UiTextStyle,
};

use crate::electronics_controller::ElectronicsTool;
use crate::panels::electronics_surface::electronics_tokens;

use super::pcb;
use super::schematic;
use super::{
    ControlKind, ToolIsland, ToolbarControl, ToolbarDensity, ToolbarState, CLASS_BUTTON,
    CLASS_DANGER, CLASS_OPTION_ON, CLASS_TOOL_ACTIVE, CLASS_TOOL_ISLAND_CONNECT,
    CLASS_TOOL_ISLAND_PLACE, CLASS_TOOL_ISLAND_SELECT, CLASS_UNAVAILABLE,
    TOOLBAR_ICON_BUTTON_WIDTH,
};

/// Tools and selection edits of the active surface, in reading order.
///
/// `Pan` is the one tool that never prints a label. It is the only tool with a
/// permanent gesture equivalent (Space+drag, middle drag and right drag, all
/// stated in its own tooltip and in the canvas tool hint) and its four-way glyph
/// is a universal convention. Paying for its label is what pushed the Spanish
/// PCB row past the canvas of the default 1600-point window, which would have
/// cost the whole band its labels at the default size.
pub(super) fn document_controls(state: &ToolbarState) -> Vec<ToolbarControl> {
    let mut controls = vec![
        ToolbarControl::tool(
            "electronics.select",
            UiIconId::Select,
            Some("electronics.toolbar.select"),
        ),
        ToolbarControl::tool("electronics.pan", UiIconId::Move, None),
    ];
    match state.surface {
        CadSurfaceKind::Schematic => schematic::document_tools(&mut controls),
        CadSurfaceKind::Pcb => pcb::document_tools(&mut controls),
    }
    controls.push(ToolbarControl::command(
        "electronics.rotate",
        UiIconId::Rotate,
        None,
        state.can_rotate,
    ));
    controls.push(
        ToolbarControl::command("edit.delete", UiIconId::Trash, None, state.has_selection)
            .destructive(),
    );
    controls
}

/// Controls that only change how the document is presented or navigated,
/// each with the latched value the host reported.
pub(super) fn view_controls(state: &ToolbarState) -> Vec<(ToolbarControl, bool)> {
    vec![
        (
            ToolbarControl::option(
                "electronics.grid.toggle",
                UiIconId::Grid,
                Some("electronics.toolbar.grid"),
            ),
            state.grid_visible,
        ),
        (
            // Snapping is "held to the grid", so the option carries the magnet
            // glyph and every other Electronics command stays free of it.
            ToolbarControl::option(
                "electronics.snap.toggle",
                UiIconId::Magnet,
                Some("electronics.toolbar.snap"),
            ),
            state.snap_enabled,
        ),
        (
            ToolbarControl::option(
                "electronics.labels.toggle",
                UiIconId::Eye,
                Some("electronics.toolbar.labels"),
            ),
            state.labels_visible,
        ),
        // Fit is a framing command, so it carries the frame glyph: corner
        // brackets around the content that would fill the view. It keeps the
        // `F` shortcut, and no other Electronics command owns the frame.
        (
            ToolbarControl::command("electronics.fit", UiIconId::Frame, None, true),
            false,
        ),
        (
            ToolbarControl::command("electronics.zoom-out", UiIconId::ZoomOut, None, true),
            false,
        ),
        (
            ToolbarControl::command("electronics.zoom-in", UiIconId::ZoomIn, None, true),
            false,
        ),
    ]
}

/// Controls that step through the session history.
pub(super) fn history_controls(state: &ToolbarState) -> Vec<ToolbarControl> {
    vec![
        ToolbarControl::command("edit.undo", UiIconId::Undo, None, state.can_undo),
        ToolbarControl::command("edit.redo", UiIconId::Redo, None, state.can_redo),
    ]
}

/// Builds one control of the band.
///
/// A labelled control never combines a glyph and text: the compositor reserves
/// the glyph track outside the measured text track, so a control carrying both
/// would print its label past its own box and crowd the next control. The
/// labelled density therefore prints the label alone and the glyph densities
/// print the glyph alone.
pub(super) fn control_button(
    palette: StudioUiPalette,
    control: &ToolbarControl,
    density: ToolbarDensity,
    height: f32,
    engaged: bool,
) -> UiNode {
    let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
    let unavailable = control.available.is_some_and(|available| !available);
    let compact_mode = control.kind == ControlKind::Segment && !density.prints_labels();
    // The workspace identity must remain explicit even when every other control
    // has collapsed to an icon. The 9px mode labels are measured to keep both
    // full names visible at the 530pt one-row breakpoint.
    let labelled =
        control.kind == ControlKind::Segment || (density.prints_labels() && control.needs_label());
    let action = tooltip_key(control.command);
    let tooltip = if unavailable {
        unavailable_reason_key(control.command)
    } else {
        action
    };
    // The engaged tool is the only control whose glyph sits on the accent fill,
    // so it is the only one that needs the contrasting page color.
    let on_accent = engaged && control.kind == ControlKind::Tool;
    let glyph_color = if on_accent {
        tokens.background
    } else if unavailable || !engaged {
        tokens.text_muted
    } else {
        tokens.text
    };
    let label_color = if unavailable {
        tokens.text_muted
    } else if on_accent {
        tokens.background
    } else {
        tokens.text
    };

    let mut node = UiNode::new(control.id(), UiNodeKind::Button)
        .with_class(CLASS_BUTTON)
        .with_layout(if labelled {
            UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                justify_content: UiJustify::Center,
                padding: if compact_mode {
                    UiSpacing::xy(1.0, 2.0)
                } else {
                    UiSpacing::xy(8.0, 2.0)
                },
                // The horizontal minimum must stay at zero for a content-sized
                // row child: any non-zero `min_size`/`basis` makes the parent
                // advance this node by that constant while the node still paints
                // at its measured text width, so the next sibling lands on top
                // of it. The atlas-driven intrinsic is what both agree on.
                min_size: [0.0, height],
                height_mode: UiSizeMode::Fixed,
                basis: [0.0, height],
                width_mode: UiSizeMode::FitContent,
                ..UiLayout::default()
            }
            .with_text_safe_area(true)
        } else {
            UiLayout::fixed(TOOLBAR_ICON_BUTTON_WIDTH, height)
        })
        .with_tooltip_key(tooltip)
        .with_accessibility_label_key(action)
        .with_accessibility_role(match control.kind {
            ControlKind::Option => UiAccessibilityRole::Checkbox,
            ControlKind::Tool | ControlKind::Segment | ControlKind::Command => {
                UiAccessibilityRole::Button
            }
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, control.command));

    if labelled {
        node = node
            .with_text_key(control.label.expect("a labelled control has a label"))
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_text_style(UiTextStyle {
                role: UiTextRole::Button,
                size_px: if compact_mode { 9.0 } else { 11.0 },
                line_height_px: if compact_mode { 12.0 } else { 15.0 },
                // Medium keeps the labelled controls readable without letting
                // the whole band carry the same weight as its one accent fill.
                weight: UiFontWeight::Medium,
                color: label_color,
                inherit_color: false,
            });
    } else {
        // The glyph carries the state when there is no label: muted at rest, the
        // contrasting page color on the engaged tool and the body color on an
        // engaged option, so the state is never color-only.
        node = node.with_icon(
            UiIcon::new(control.icon)
                .with_size(UiIconSize::Small)
                .with_tint(glyph_color),
        );
    }

    if let Some(island_class) = tool_island_class(control.command) {
        node = node.with_class(island_class);
    }

    if engaged {
        node = node.with_class(match control.kind {
            ControlKind::Tool => CLASS_TOOL_ACTIVE,
            ControlKind::Segment | ControlKind::Option | ControlKind::Command => CLASS_OPTION_ON,
        });
    }
    if unavailable {
        // A disabled node is skipped by hit testing, so it could neither show
        // the reason nor be reached by keyboard. The control therefore stays
        // interactive, states its prerequisite in its tooltip and accessible
        // description, and every unavailable command is a no-op in the
        // controller.
        node = node
            .with_class(CLASS_UNAVAILABLE)
            .with_accessibility_description_key(unavailable_reason_key(control.command));
    } else if control.danger {
        node = node.with_class(CLASS_DANGER);
    }

    match control.kind {
        ControlKind::Option => {
            node = node.with_accessibility_checked(engaged);
        }
        ControlKind::Tool | ControlKind::Segment => {
            node = node.with_accessibility_selected(engaged);
        }
        ControlKind::Command => {}
    }
    node
}

/// True when `command` is the tool currently in the user's hand.
pub(super) fn tool_is_engaged(command: &str, tool: ElectronicsTool) -> bool {
    matches!(
        (command, tool),
        ("electronics.select", ElectronicsTool::Select)
            | ("electronics.pan", ElectronicsTool::Pan)
            | ("electronics.wire", ElectronicsTool::Wire)
            | ("electronics.route", ElectronicsTool::Route)
            | ("electronics.place", ElectronicsTool::Place)
            | ("electronics.board-outline", ElectronicsTool::BoardOutline)
    )
}

pub(super) fn tool_island(command: &str) -> Option<ToolIsland> {
    match command {
        "electronics.select" | "electronics.pan" => Some(ToolIsland::Selection),
        "electronics.wire" | "electronics.route" | "electronics.board-outline" | "electronics.pcb.via" => {
            Some(ToolIsland::Connection)
        }
        "electronics.place" => Some(ToolIsland::Placement),
        "electronics.pcb.sync" | "electronics.rotate" | "edit.delete" | "electronics.pcb.menu:rules" => {
            Some(ToolIsland::Operations)
        }
        _ => None,
    }
}

fn tool_island_class(command: &str) -> Option<&'static str> {
    match tool_island(command)? {
        ToolIsland::Selection => Some(CLASS_TOOL_ISLAND_SELECT),
        ToolIsland::Connection => Some(CLASS_TOOL_ISLAND_CONNECT),
        ToolIsland::Placement => Some(CLASS_TOOL_ISLAND_PLACE),
        ToolIsland::Operations => None,
    }
}

/// Localized description of what a control does.
pub(super) fn tooltip_key(command: &str) -> &'static str {
    match command {
        "electronics.mode.schematic" => "electronics.tooltip.mode.schematic",
        "electronics.mode.pcb" => "electronics.tooltip.mode.pcb",
        "electronics.select" => "electronics.tooltip.select",
        "electronics.pan" => "electronics.tooltip.pan",
        "electronics.wire" => "electronics.tooltip.wire",
        "electronics.place" => "electronics.tooltip.place",
        "electronics.route" => "electronics.tooltip.route",
        "electronics.pcb.via" => "electronics.pcb.via_help",
        "electronics.pcb.menu:rules" => "electronics.pcb.rules_title",
        "electronics.board-outline" => "electronics.tooltip.board_outline",
        "electronics.pcb.sync" => "electronics.tooltip.pcb_sync",
        "electronics.fit" => "electronics.tooltip.fit",
        "electronics.zoom-out" => "electronics.tooltip.zoom_out",
        "electronics.zoom-in" => "electronics.tooltip.zoom_in",
        "electronics.grid.toggle" => "electronics.tooltip.grid",
        "electronics.snap.toggle" => "electronics.tooltip.snap",
        "electronics.labels.toggle" => "electronics.tooltip.labels",
        "electronics.rotate" => "electronics.tooltip.rotate",
        "edit.delete" => "electronics.tooltip.delete",
        "edit.undo" => "electronics.tooltip.undo",
        "edit.redo" => "electronics.tooltip.redo",
        _ => "electronics.tooltip.command",
    }
}

/// Localized reason a control cannot run yet.
///
/// An unavailable control keeps the action description as its accessible name
/// and states the missing prerequisite here, so the tooltip is self-contained
/// and assistive technology announces both.
fn unavailable_reason_key(command: &str) -> &'static str {
    match command {
        "electronics.rotate" => "electronics.tooltip.rotate.unavailable",
        "edit.delete" => "electronics.tooltip.delete.unavailable",
        "edit.undo" => "electronics.tooltip.undo.unavailable",
        "edit.redo" => "electronics.tooltip.redo.unavailable",
        _ => "electronics.tooltip.command",
    }
}
