//! Toolbar island and shared buttons for the Nodes authoring surface.

use super::*;

/// Height of the graph toolbar island above the canvas. The compositor needs
/// it to convert window pointers into canvas coordinates for zoom and pan.
pub const NODES_TOOLBAR_HEIGHT: f32 = 36.0;

/// Builds the floating graph toolbar island above the canvas.
pub(super) fn build_graph_header(
    palette: StudioUiPalette,
    graph: &NodeGraph,
    zoom: f32,
    host: &NodesSurfaceHost,
) -> UiNode {
    let tokens = palette.tokens();
    // Top Floating Navigation Bar / HUD Island
    let active_tool = host.active_tool();
    let header = UiNode::new("nodes.graph.header", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            // SpaceBetween instead of a `grow` spacer: a growing sibling in
            // this row consumed the free width and collapsed every control
            // that came after it, which is why pills and buttons rendered as
            // empty plates.
            justify_content: UiJustify::SpaceBetween,
            gap: 7.0,
            padding: UiSpacing::xy(10.0, 0.0),
            ..UiLayout::fixed(0.0, 36.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_style(UiStyle {
            fill: tokens.surface,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 0.0,
            opacity: 0.98,
        })
        // Tool group (Select, Pan, Fit)
        .with_child(hud_tool_button(
            "nodes.tool.select",
            UiIconId::Select,
            "Select Tool (V)",
            active_tool == "select",
            palette,
        ))
        .with_child(hud_tool_button(
            "nodes.tool.pan",
            UiIconId::Move,
            "Pan Canvas (H)",
            active_tool == "pan",
            palette,
        ))
        .with_child(hud_tool_button(
            "nodes.tool.fit",
            UiIconId::Focus,
            "Fit Graph (F)",
            false,
            palette,
        ))
        .with_child(
            UiNode::new("nodes.tool.add", UiNodeKind::Button)
                .with_class("nodes-small-btn")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 4.0,
                    padding: UiSpacing::xy(8.0, 4.0),
                    ..UiLayout::fit_content().fixed_height(26.0)
                })
                .with_style(UiStyle {
                    fill: [tokens.accent[0], tokens.accent[1], tokens.accent[2], 40],
                    border: tokens.accent,
                    text: tokens.text,
                    border_width: 1.0,
                    radius: 4.0,
                    opacity: 1.0,
                })
                .with_icon(
                    UiIcon::new(UiIconId::Add)
                        .with_size(UiIconSize::Small)
                        .with_tint(tokens.accent),
                )
                .with_child(
                    UiNode::new("nodes.tool.add.label", UiNodeKind::Label)
                        .with_text_key("nodes.add_node")
                        .with_text_style(UiTextStyle::button(tokens.text)),
                )
                .with_tooltip_key("nodes.palette_popup_hint")
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "nodes.palette.open",
                )),
        )
        .with_child(
            UiNode::new("nodes.graph.vsep", UiNodeKind::Panel)
                .with_layout(UiLayout::fixed(1.0, 18.0))
                .with_style(UiStyle {
                    fill: tokens.border,
                    border: [0, 0, 0, 0],
                    text: tokens.text,
                    border_width: 0.0,
                    radius: 0.0,
                    opacity: 0.8,
                }),
        )
        // Graph Title and Stats
        .with_child(
            UiNode::new("nodes.graph.name", UiNodeKind::Label)
                .with_text_value(graph.name.clone())
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_text_overflow(UiTextOverflow::Ellipsis),
        )
        .with_child(stat_pill(
            "nodes.graph.node-count",
            format!("{} nodes", graph.nodes.len()),
            tokens.accent,
        ))
        .with_child(stat_pill(
            "nodes.graph.wire-count",
            format!("{} wires", graph.connections.len()),
            tokens.text_muted,
        ))
        // Zoom percentage badge
        .with_child(
            UiNode::new("nodes.zoom.badge", UiNodeKind::Button)
                .with_class("nodes-small-btn")
                .with_layout(UiLayout::fixed(50.0, 24.0))
                .with_child(
                    UiNode::new("nodes.zoom.val", UiNodeKind::Label)
                        .with_text_value(format!("{:.0}%", zoom * 100.0))
                        .with_text_style(UiTextStyle {
                            role: UiTextRole::Button,
                            size_px: 10.5,
                            line_height_px: 24.0,
                            weight: UiFontWeight::Bold,
                            color: tokens.text_muted,
                            inherit_color: false,
                        }),
                )
                .with_tooltip_value("Reset zoom (100%)")
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "nodes.zoom.reset",
                )),
        )
        .with_child(icon_button(
            "nodes.zoom.out",
            UiIconId::ChevronLeft,
            "nodes.zoom_out",
            palette,
        ))
        .with_child(icon_button(
            "nodes.zoom.in",
            UiIconId::ChevronRight,
            "nodes.zoom_in",
            palette,
        ))
        .with_child(
            UiNode::new("nodes.graph.compile", UiNodeKind::Button)
                .with_class("nodes-small-btn")
                // Hug the label: a zero-width basis inside a row that also
                // holds a growing spacer paints the plate but drops the text.
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 4.0,
                    padding: UiSpacing::xy(8.0, 4.0),
                    ..UiLayout::fit_content().fixed_height(26.0)
                })
                .with_style(UiStyle {
                    fill: [
                        tokens.accent_hot[0],
                        tokens.accent_hot[1],
                        tokens.accent_hot[2],
                        40,
                    ],
                    border: tokens.accent_hot,
                    text: tokens.text,
                    border_width: 1.0,
                    radius: 4.0,
                    opacity: 1.0,
                })
                .with_icon(
                    UiIcon::new(UiIconId::Success)
                        .with_size(UiIconSize::Small)
                        .with_tint(tokens.accent_hot),
                )
                .with_child(
                    UiNode::new("nodes.graph.compile_label", UiNodeKind::Label)
                        .with_text_key("nodes.validate")
                        .with_text_style(UiTextStyle::button(tokens.accent_hot)),
                )
                .focusable()
                .with_event(UiEventBinding::command(UiEventKind::Click, "nodes.compile")),
        );
    header
}

fn hud_tool_button(
    id: &str,
    icon: UiIconId,
    tooltip: &str,
    active: bool,
    palette: StudioUiPalette,
) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(id.to_string(), UiNodeKind::Button)
        .with_class("nodes-tool-btn")
        .with_layout(UiLayout::fixed(26.0, 26.0))
        .with_style(UiStyle {
            fill: if active {
                tokens.surface_raised
            } else {
                [0, 0, 0, 0]
            },
            border: if active { tokens.accent } else { [0, 0, 0, 0] },
            text: tokens.text,
            border_width: if active { 1.5 } else { 0.0 },
            radius: 4.0,
            opacity: if active { 1.0 } else { 0.8 },
        })
        .with_icon(
            UiIcon::new(icon)
                .with_size(UiIconSize::Small)
                .with_tint(if active {
                    tokens.accent
                } else {
                    tokens.text_muted
                }),
        )
        .with_tooltip_value(tooltip)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, id.to_string()))
}

fn stat_pill(id: &str, text: String, color: [u8; 4]) -> UiNode {
    UiNode::new(id.to_string(), UiNodeKind::Panel)
        .with_layout(UiLayout {
            padding: UiSpacing::xy(6.0, 2.0),
            ..UiLayout::fit_content()
        })
        .with_style(UiStyle {
            fill: [color[0], color[1], color[2], 26],
            border: [color[0], color[1], color[2], 75],
            text: [255, 255, 255, 255],
            border_width: 1.0,
            radius: 4.0,
            opacity: 1.0,
        })
        .with_child(
            UiNode::new(format!("{id}.txt"), UiNodeKind::Label)
                .with_text_value(text)
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Label,
                    size_px: 9.5,
                    line_height_px: 13.0,
                    weight: UiFontWeight::Bold,
                    color,
                    inherit_color: false,
                }),
        )
}
pub(super) fn icon_button(
    id: impl Into<String>,
    icon: UiIconId,
    tooltip_key: impl Into<String>,
    palette: StudioUiPalette,
) -> UiNode {
    let tokens = palette.tokens();
    let id = id.into();
    let tooltip_key = tooltip_key.into();
    UiNode::new(id.clone(), UiNodeKind::Button)
        .with_class("nodes-icon-button")
        .with_icon(
            UiIcon::new(icon)
                .with_size(UiIconSize::Small)
                .with_tint(tokens.text_muted),
        )
        .with_layout(UiLayout::fixed(26.0, 26.0))
        .with_tooltip_key(tooltip_key.clone())
        .with_accessibility_label_key(tooltip_key)
        .with_accessibility_role(UiAccessibilityRole::Button)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, id))
}

pub(super) fn stat_label(
    id: impl Into<String>,
    label_key: impl Into<String>,
    value: usize,
    color: [u8; 4],
) -> UiNode {
    let id = id.into();
    UiNode::new(id.clone(), UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 4.0,
            ..UiLayout::fit_content()
        })
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::body(color)),
        )
        .with_child(
            UiNode::new(format!("{id}.value"), UiNodeKind::Label)
                .with_text_value(value.to_string())
                .with_text_style(UiTextStyle::body(color)),
        )
}
