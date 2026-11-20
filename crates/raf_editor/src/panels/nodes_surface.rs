//! Retained RafUI surface for the Nodes authoring editor.
//!
//! The surface is intentionally declarative. It reads the persisted
//! `raf_nodes::NodeGraph`, renders the graph through ordinary RafUI nodes, and
//! emits typed command names for the native editor boundary. Transient search,
//! pin-connection selection, focus, and scroll state live in
//! `nodes_surface_host.rs`; graph mutation remains in `NativeEditorRuntime`.

use super::{nodes_canvas, nodes_catalog};
use crate::native_editor_runtime::NodeGraphValidation;
use crate::panels::nodes_surface_host::{NodesSurfaceHost, PendingPin};
use raf_nodes::{Node, NodeCategory, NodeGraph, NodeId, NodePin, PinKind};
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAccessibilityRole, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiIcon,
    UiIconId, UiIconSize, UiJustify, UiLayout, UiNode, UiNodeKind, UiOverflow, UiScrollAxis,
    UiSizeMode, UiSpacing, UiStyle, UiSurface, UiTextInput, UiTextOverflow, UiTextStyle,
};
use raf_ui::{
    UiFontWeight, UiRect, UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector,
    UiStyleSheet, UiTextRole,
};

mod canvas;
mod inspector;
mod links;
mod palette;
mod selection;
mod toolbar;

#[cfg(test)]
mod tests;

use canvas::build_graph_panel;
use inspector::{build_empty_inspector, build_inspector};
use palette::build_palette;

pub use palette::{NODES_PALETTE_POPUP_HEIGHT, NODES_PALETTE_POPUP_WIDTH};
pub use selection::{NODES_CONTEXT_MENU_HEIGHT, NODES_CONTEXT_MENU_WIDTH};
pub use toolbar::NODES_TOOLBAR_HEIGHT;

/// Builds the retained Nodes surface at the default zoom.
pub fn build_nodes_surface(
    palette: StudioUiPalette,
    graph: &NodeGraph,
    selected: Option<NodeId>,
) -> UiSurface {
    build_nodes_surface_with_zoom_and_query(
        palette, graph, selected, 1.0, "", None, false, false, None,
    )
}

/// Compatibility entry point used by older workbench callers.
pub fn build_nodes_surface_with_zoom(
    palette: StudioUiPalette,
    graph: &NodeGraph,
    selected: Option<NodeId>,
    zoom: f32,
) -> UiSurface {
    build_nodes_surface_with_zoom_and_query(
        palette, graph, selected, zoom, "", None, false, false, None,
    )
}

/// Build the authoring surface with transient query and pending-pin state.
pub fn build_nodes_surface_with_zoom_and_query(
    palette: StudioUiPalette,
    graph: &NodeGraph,
    selected: Option<NodeId>,
    zoom: f32,
    query: &str,
    pending_pin: Option<PendingPin>,
    can_undo: bool,
    can_redo: bool,
    validation: Option<&NodeGraphValidation>,
) -> UiSurface {
    build_nodes_surface_with_zoom_query_and_language(
        palette,
        graph,
        selected,
        zoom,
        query,
        pending_pin,
        can_undo,
        can_redo,
        validation,
        raf_core::Language::English,
    )
}

/// Build the authoring surface using the active application language.
pub fn build_nodes_surface_with_zoom_query_and_language(
    palette: StudioUiPalette,
    graph: &NodeGraph,
    selected: Option<NodeId>,
    zoom: f32,
    query: &str,
    pending_pin: Option<PendingPin>,
    can_undo: bool,
    can_redo: bool,
    validation: Option<&NodeGraphValidation>,
    language: raf_core::Language,
) -> UiSurface {
    let mut host = NodesSurfaceHost::default();
    host.set_query(query);
    if let Some(pin) = pending_pin {
        host.set_pending_pin(pin);
    }
    // Legacy callers have no viewport, so fall back to a compact default
    // canvas that still frames a typical graph.
    let extent = nodes_canvas::graph_extent(graph);
    build_nodes_surface_with_host(
        palette,
        graph,
        selected,
        zoom,
        [0.0, 0.0],
        [extent[0].max(860.0), extent[1].max(560.0)],
        &host,
        can_undo,
        can_redo,
        validation,
        language,
    )
}

/// Master entry point for the AAA Nodes authoring workspace.
pub fn build_nodes_surface_with_host(
    palette: StudioUiPalette,
    graph: &NodeGraph,
    selected: Option<NodeId>,
    zoom: f32,
    pan: [f32; 2],
    viewport: [f32; 2],
    host: &NodesSurfaceHost,
    can_undo: bool,
    can_redo: bool,
    validation: Option<&NodeGraphValidation>,
    language: raf_core::Language,
) -> UiSurface {
    let tokens = palette.tokens();
    let palette_panel = build_palette(palette, host, language, can_undo, can_redo);
    let graph_panel = build_graph_panel(
        palette, graph, selected, zoom, pan, viewport, host, validation, language,
    );
    let inspector = selected
        .and_then(|id| graph.node(id))
        .map(|node| build_inspector(palette, graph, node, language))
        .unwrap_or_else(|| build_empty_inspector(palette));

    let root = UiNode::new("nodes.root", UiNodeKind::Panel)
        .with_class("nodes-surface")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 1.0,
            ..UiLayout::fill(UiFlow::Row)
        })
        .with_style(panel_style(tokens.background, tokens.border, tokens.text))
        .with_child(palette_panel)
        .with_child(graph_panel)
        .with_child(
            UiNode::new("nodes.inspector.slot", UiNodeKind::Panel)
                .with_layout(UiLayout::fixed(300.0, 0.0).with_height_mode(UiSizeMode::Fill))
                .with_child(inspector),
        );

    let mut surface = UiSurface::new("editor.nodes", palette, root);
    surface.style_sheet = nodes_style_sheet(palette);
    surface
}

/// Quick-add palette shown at the pointer. This is the primary way to author a
/// graph: double click or secondary click on empty canvas, type, pick a node.
/// Each row spawns the node exactly under the cursor, snapped to the grid.
pub fn build_nodes_palette_popup_surface(
    palette: StudioUiPalette,
    host: &NodesSurfaceHost,
    language: raf_core::Language,
) -> Option<UiSurface> {
    self::palette::build_palette_popup(palette, host, language)
}

/// Builds the node context menu as a standalone window-level surface, mirroring
/// the Electronics and bottom-dock tab menus. Returns `None` when the target
/// node no longer exists (for example, deleted while the menu was open).
pub fn build_nodes_context_menu_surface(
    palette: StudioUiPalette,
    graph: &NodeGraph,
    node_id: NodeId,
    language: raf_core::Language,
) -> Option<UiSurface> {
    self::selection::build_context_menu(palette, graph, node_id, language)
}

fn panel_style(fill: [u8; 4], border: [u8; 4], text: [u8; 4]) -> UiStyle {
    UiStyle {
        fill,
        border,
        text,
        border_width: 1.0,
        radius: 0.0,
        opacity: 1.0,
    }
}

fn category_color(category: NodeCategory) -> [u8; 4] {
    let color = category.color();
    [
        (color[0] * 255.0) as u8,
        (color[1] * 255.0) as u8,
        (color[2] * 255.0) as u8,
        255,
    ]
}

// ----------------------------------------------------------------------------
// STYLE SHEET: High-End Visual Transitions and Glows
// ----------------------------------------------------------------------------

fn nodes_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-search".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-cat-header".to_string()),
                UiStylePatch {
                    border_width: Some(1.5),
                    fill: Some([
                        tokens.surface_raised[0],
                        tokens.surface_raised[1],
                        tokens.surface_raised[2],
                        180,
                    ]),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-palette-card".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-palette-card".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent_hot),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Active),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-card".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent_hot),
                    border_width: Some(2.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-card".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent_hot),
                    border_width: Some(2.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-pin-dot".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent_hot),
                    border_width: Some(3.0),
                    fill: Some([
                        tokens.accent_hot[0],
                        tokens.accent_hot[1],
                        tokens.accent_hot[2],
                        200,
                    ]),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-pin-dot-pending".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent_hot),
                    border: Some([255, 255, 255, 255]),
                    border_width: Some(3.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-pin-dot-compatible".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent_hot),
                    border_width: Some(3.0),
                    fill: Some([255, 255, 255, 220]),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-connection-knot".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent_hot),
                    border_width: Some(1.5),
                    opacity: Some(1.0),
                    fill: Some([
                        tokens.surface_raised[0],
                        tokens.surface_raised[1],
                        tokens.surface_raised[2],
                        255,
                    ]),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
        ],
    }
}
