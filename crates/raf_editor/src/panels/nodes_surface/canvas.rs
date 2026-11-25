//! Graph canvas and node card rendering for the Nodes authoring surface.

use super::links::pin_button;
use super::toolbar::{build_graph_header, stat_label};
use super::*;

// ----------------------------------------------------------------------------
// CENTER PANEL: Graph Canvas + Floating HUD + Wire Rendering
// ----------------------------------------------------------------------------

pub(super) fn build_graph_panel(
    palette: StudioUiPalette,
    graph: &NodeGraph,
    selected: Option<NodeId>,
    zoom: f32,
    pan: [f32; 2],
    viewport: [f32; 2],
    host: &NodesSurfaceHost,
    validation: Option<&NodeGraphValidation>,
    language: raf_core::Language,
) -> UiNode {
    let tokens = palette.tokens();
    let mut panel = UiNode::new("nodes.graph.panel", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 0.0,
            grow: 1.0,
            ..UiLayout::default()
        })
        .with_style(panel_style(tokens.background, tokens.border, tokens.text));

    // Top Floating Navigation Bar / HUD Island
    panel = panel.with_child(build_graph_header(palette, graph, zoom, host));

    // Active Wire / Pending Pin Notice Banner
    if host.pending_pin().is_some() {
        let banner = UiNode::new("nodes.wiring.banner", UiNodeKind::Panel)
            .with_class("nodes-wiring-banner")
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 8.0,
                padding: UiSpacing::xy(12.0, 4.0),
                ..UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)
            })
            .with_style(UiStyle {
                fill: [tokens.accent_hot[0], tokens.accent_hot[1], tokens.accent_hot[2], 40],
                border: tokens.accent_hot,
                text: tokens.text,
                border_width: 1.0,
                radius: 0.0,
                opacity: 1.0,
            })
            .with_icon(
                UiIcon::new(UiIconId::Wire)
                    .with_size(UiIconSize::Small)
                    .with_tint(tokens.accent_hot),
            )
            .with_child(
                UiNode::new("nodes.wiring.text", UiNodeKind::Label)
                    .with_text_value(
                        "WIRING IN PROGRESS: Click a compatible target connector to link wire • Click canvas or Cancel to abort",
                    )
                    .with_text_style(UiTextStyle {
                        role: UiTextRole::Label,
                        size_px: 10.5,
                        line_height_px: 18.0,
                        weight: UiFontWeight::Bold,
                        color: tokens.text,
                        inherit_color: false,
                    })
                    .with_layout(UiLayout {
                        grow: 1.0,
                        ..UiLayout::fit_content()
                    }),
            )
            .with_child(
                UiNode::new("nodes.wiring.cancel", UiNodeKind::Button)
                    .with_class("nodes-small-btn")
                    .with_layout(UiLayout {
                        padding: UiSpacing::xy(8.0, 2.0),
                        ..UiLayout::fixed(0.0, 20.0)
                    })
                    .with_style(UiStyle {
                        fill: tokens.surface_raised,
                        border: tokens.border,
                        text: tokens.text,
                        border_width: 1.0,
                        radius: 3.0,
                        opacity: 1.0,
                    })
                    .with_child(
                        UiNode::new("nodes.wiring.cancel_txt", UiNodeKind::Label)
                            .with_text_value("Cancel")
                            .with_text_style(UiTextStyle {
                                role: UiTextRole::Button,
                                size_px: 9.5,
                                line_height_px: 14.0,
                                weight: UiFontWeight::Bold,
                                color: tokens.text,
                                inherit_color: false,
                            }),
                    )
                    .focusable()
                    .with_event(UiEventBinding::command(
                        UiEventKind::Click,
                        "nodes.clear-pending-pin",
                    )),
            );
        panel = panel.with_child(banner);
    }

    if graph.nodes.is_empty() {
        return panel.with_child(empty_graph(palette));
    }

    if let Some(validation) = validation {
        panel = panel.with_child(validation_summary(palette, validation, language));
    }

    // Canvas viewport. The surface never grows with the graph: the canvas is
    // exactly the visible area and the graph is panned inside it, so the
    // background stays a constant plate instead of a growing slab.
    let canvas_size = [
        viewport[0].max(160.0),
        (viewport[1] - NODES_TOOLBAR_HEIGHT).max(120.0),
    ];
    let mut canvas = UiNode::new("nodes.canvas", UiNodeKind::Canvas)
        .with_class("nodes-canvas")
        .with_layout(UiLayout::absolute(UiRect::new(
            0.0,
            0.0,
            canvas_size[0],
            canvas_size[1],
        )))
        .with_style(canvas_style(tokens))
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            "nodes.clear-selection",
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::DoubleClick,
            "nodes.palette.open",
        ))
        .with_event(UiEventBinding {
            event: UiEventKind::ContextMenu,
            action: raf_ui::UiAction::OpenMenu {
                id: "nodes.palette.open".to_string(),
            },
        })
        .with_event(UiEventBinding::command(
            UiEventKind::DragStart,
            "nodes.canvas.pan.start",
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::DragMove,
            "nodes.canvas.pan.move",
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::DragEnd,
            "nodes.canvas.pan.end",
        ));

    // 1. Grid
    for child in nodes_canvas::build_grid(palette, canvas_size) {
        canvas = canvas.with_child(child);
    }

    // 2. Cables (persisted + in-flight elastic wire)
    for child in nodes_canvas::build_connections(
        palette,
        graph,
        zoom,
        pan,
        canvas_size,
        host.wire_drag().as_ref(),
    ) {
        canvas = canvas.with_child(child);
    }

    // 3. Node cards
    for (index, node) in graph.nodes.iter().enumerate() {
        let geometry = nodes_canvas::node_geometry(node, index, zoom);
        canvas = canvas.with_child(
            build_node_card(
                palette,
                graph,
                node,
                selected == Some(node.id),
                host.pending_pin(),
                language,
                zoom,
                host.selection_glow(),
            )
            .with_layout(UiLayout::absolute(UiRect::new(
                geometry.position[0] - pan[0],
                geometry.position[1] - pan[1],
                geometry.size[0],
                geometry.size[1],
            ))),
        );
    }

    panel.with_child(
        UiNode::new("nodes.canvas.viewport", UiNodeKind::Panel)
            .with_layout(UiLayout {
                grow: 1.0,
                overflow: UiOverflow::Clip,
                ..UiLayout::default()
            })
            .with_child(canvas),
    )
}
fn empty_graph(palette: StudioUiPalette) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("nodes.graph.empty", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            gap: 6.0,
            grow: 1.0,
            ..UiLayout::default()
        })
        // An empty graph has no canvas, so the empty state itself owns the
        // quick-add gesture.
        .with_event(UiEventBinding::command(
            UiEventKind::DoubleClick,
            "nodes.palette.open",
        ))
        .with_event(UiEventBinding {
            event: UiEventKind::ContextMenu,
            action: raf_ui::UiAction::OpenMenu {
                id: "nodes.palette.open".to_string(),
            },
        })
        .with_child(
            UiNode::new("nodes.graph.empty.icon", UiNodeKind::Label)
                .with_icon(
                    UiIcon::new(UiIconId::Node)
                        .with_size(UiIconSize::Custom(44))
                        .with_tint([
                            tokens.text_muted[0],
                            tokens.text_muted[1],
                            tokens.text_muted[2],
                            160,
                        ]),
                )
                .with_layout(UiLayout::fixed(44.0, 44.0)),
        )
        .with_child(
            UiNode::new("nodes.graph.empty.title", UiNodeKind::Label)
                .with_text_key("nodes.empty")
                .with_text_style(UiTextStyle::panel_title(tokens.text)),
        )
        .with_child(
            UiNode::new("nodes.graph.empty.hint", UiNodeKind::Label)
                .with_text_key("nodes.empty_hint")
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        )
        .with_child(
            UiNode::new("nodes.graph.empty.cta", UiNodeKind::Button)
                .with_class("nodes-empty-cta")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 6.0,
                    padding: UiSpacing::xy(12.0, 0.0),
                    ..UiLayout::fixed(0.0, 30.0)
                })
                .with_icon(UiIcon::new(UiIconId::Add).with_size(UiIconSize::Small))
                .with_child(
                    UiNode::new("nodes.graph.empty.cta.label", UiNodeKind::Label)
                        .with_text_key("nodes.empty_cta")
                        .with_text_style(UiTextStyle::button(tokens.accent)),
                )
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "nodes.add.on-start",
                )),
        )
}

fn validation_summary(
    palette: StudioUiPalette,
    validation: &NodeGraphValidation,
    language: raf_core::Language,
) -> UiNode {
    let tokens = palette.tokens();
    let mut panel = UiNode::new("nodes.validation", UiNodeKind::Panel)
        .with_class("nodes-validation")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 3.0,
            padding: UiSpacing::xy(8.0, 5.0),
            ..UiLayout::fixed(0.0, 94.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_style(UiStyle {
            fill: if validation.errors.is_empty() {
                tokens.surface_alt
            } else {
                [72, 38, 34, 255]
            },
            border: if validation.errors.is_empty() {
                tokens.border
            } else {
                tokens.danger
            },
            text: tokens.text,
            border_width: 1.0,
            radius: 3.0,
            opacity: 1.0,
        });
    if validation.errors.is_empty() && validation.warnings.is_empty() {
        return panel.with_child(
            UiNode::new("nodes.validation.ok", UiNodeKind::Label)
                .with_text_key("nodes.validation.ok")
                .with_text_style(UiTextStyle::body(tokens.positive)),
        );
    }
    panel = panel.with_child(
        UiNode::new("nodes.validation.summary", UiNodeKind::Panel)
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 12.0,
                ..UiLayout::fit_content()
            })
            .with_child(stat_label(
                "nodes.validation.errors-count",
                "nodes.validation.errors",
                validation.errors.len(),
                tokens.danger,
            ))
            .with_child(stat_label(
                "nodes.validation.warnings-count",
                "nodes.validation.warnings",
                validation.warnings.len(),
                tokens.warning,
            )),
    );
    let mut messages = UiNode::scroll_view("nodes.validation.messages", UiScrollAxis::Vertical)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 2.0,
            grow: 1.0,
            overflow: UiOverflow::ScrollY,
            ..UiLayout::default()
        });
    for (index, message) in validation
        .errors
        .iter()
        .chain(validation.warnings.iter())
        .enumerate()
    {
        messages = messages.with_child(
            UiNode::new(
                format!("nodes.validation.message.{index}"),
                UiNodeKind::Label,
            )
            .with_text_value(diagnostic_message(message, language))
            .with_text_style(UiTextStyle::body(tokens.text_muted))
            .with_text_overflow(UiTextOverflow::Ellipsis),
        );
    }
    panel.with_child(messages)
}

fn diagnostic_message(
    diagnostic: &raf_nodes::graph::GraphDiagnostic,
    language: raf_core::Language,
) -> String {
    let key = format!("nodes.validation.{}", diagnostic.code);
    let mut message = raf_core::i18n::t(&key, language);
    if message == key {
        message = diagnostic.message.clone();
    }
    diagnostic
        .detail
        .as_ref()
        .map(|detail| format!("{message}: {detail}"))
        .unwrap_or(message)
}
// ----------------------------------------------------------------------------
// NODE CARD: AAA Aesthetics + Interactive Pin Highlights
// ----------------------------------------------------------------------------

fn build_node_card(
    palette: StudioUiPalette,
    graph: &NodeGraph,
    node: &Node,
    selected: bool,
    pending_pin: Option<PendingPin>,
    language: raf_core::Language,
    zoom: f32,
    glow: f32,
) -> UiNode {
    let tokens = palette.tokens();
    let scale = zoom.max(0.55);
    let id = node.id.0.to_string();
    let size = nodes_canvas::node_size(node, zoom);
    let width = size[0];
    let height = size[1];
    let radius = nodes_canvas::NODE_RADIUS * scale;
    let category_color = category_color(node.category);
    let descriptor = raf_nodes::catalog::descriptor_for_node(node);
    let title_key = descriptor
        .map(|descriptor| descriptor.label_key)
        .unwrap_or("nodes.node.unknown");
    let description_key = descriptor
        .map(|descriptor| descriptor.description_key)
        .unwrap_or("nodes.description.unknown");

    let mut card = UiNode::new(format!("nodes.card.{id}"), UiNodeKind::Panel)
        .with_class("nodes-card")
        .with_layout(UiLayout::fixed(width, height))
        .with_style(UiStyle {
            // Neutral body for every category. The category reads through the
            // stripe and the icon, so the canvas stops looking like confetti.
            fill: if selected {
                tokens.surface_raised
            } else {
                tokens.surface
            },
            border: [tokens.border[0], tokens.border[1], tokens.border[2], 210],
            text: tokens.text,
            border_width: 1.0,
            radius,
            opacity: 1.0,
        })
        .with_accessibility_role(UiAccessibilityRole::Generic)
        .with_accessibility_label_key(title_key)
        .with_accessibility_description_key(description_key)
        .with_accessibility_selected(selected)
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            format!("nodes.select.{id}"),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("enter".to_string()),
            format!("nodes.select.{id}"),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("delete".to_string()),
            format!("nodes.delete.{id}"),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::DragStart,
            format!("nodes.drag.start.{id}"),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::DragMove,
            format!("nodes.drag.move.{id}"),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::DragEnd,
            format!("nodes.drag.end.{id}"),
        ))
        .with_event(UiEventBinding {
            event: UiEventKind::ContextMenu,
            action: raf_ui::UiAction::OpenMenu {
                id: format!("nodes.context.{id}"),
            },
        });

    // Drop shadow. RafUI has no blur, so the elevation is a second plate
    // painted one pixel below the card instead of a shadow map.
    card = card.with_child(
        UiNode::new(format!("nodes.card.{id}.shadow"), UiNodeKind::Panel)
            .with_layout(UiLayout::absolute(UiRect::new(0.0, 3.0, width, height)).with_z_index(-1))
            .with_style(UiStyle {
                fill: [0, 0, 0, 110],
                border: [0, 0, 0, 0],
                text: tokens.text,
                border_width: 0.0,
                radius,
                opacity: 1.0,
            }),
    );

    // Selection glow ring. `glow` is the host reveal tween so the ring
    // animates in instead of popping between rebuilds; it only paints on the
    // selected card because the tween value is shared across the surface.
    let ring_alpha = if selected {
        (235.0 * glow.clamp(0.0, 1.0)) as u8
    } else {
        0
    };
    card = card.with_child(
        UiNode::new(format!("nodes.card.{id}.ring"), UiNodeKind::Panel)
            .with_class("nodes-card-glow")
            .with_layout(UiLayout::absolute(UiRect::new(0.0, 0.0, width, height)).with_z_index(0))
            .with_style(UiStyle {
                fill: [0, 0, 0, 0],
                border: [
                    category_color[0],
                    category_color[1],
                    category_color[2],
                    ring_alpha,
                ],
                text: tokens.text,
                border_width: if selected {
                    ((1.5 + 1.5 * glow.clamp(0.0, 1.0)) * scale).max(1.0)
                } else {
                    0.0
                },
                radius,
                opacity: 0.7 + 0.3 * glow.clamp(0.0, 1.0),
            }),
    );

    // Category stripe anchors the card to its palette group at a glance.
    let stripe_alpha = if selected { 255 } else { 190 };
    card = card.with_child(
        UiNode::new(format!("nodes.card.{id}.stripe"), UiNodeKind::Panel)
            .with_class("nodes-card-stripe")
            .with_layout(UiLayout::absolute(UiRect::new(0.0, 0.0, 3.0, height)).with_z_index(2))
            .with_style(UiStyle {
                fill: [
                    category_color[0],
                    category_color[1],
                    category_color[2],
                    stripe_alpha,
                ],
                border: [0, 0, 0, 0],
                text: tokens.text,
                border_width: 0.0,
                radius,
                opacity: 1.0,
            }),
    );

    let header_height = nodes_canvas::NODE_HEADER_HEIGHT * scale;

    // Header bar background with category accent tint and subtle gradient
    card = card.with_child(
        UiNode::new(format!("nodes.card.{id}.header"), UiNodeKind::Panel)
            .with_class("nodes-card-header")
            .with_layout(
                UiLayout::absolute(UiRect::new(0.0, 0.0, width, header_height)).with_z_index(1),
            )
            .with_style(UiStyle {
                fill: [category_color[0], category_color[1], category_color[2], 34],
                border: [category_color[0], category_color[1], category_color[2], 70],
                text: tokens.text,
                border_width: 1.0,
                radius,
                opacity: 1.0,
            }),
    );

    let icon_size = (14.0 * scale).round().clamp(11.0, 22.0) as u16;
    card = card.with_child(
        UiNode::new(format!("nodes.card.{id}.icon"), UiNodeKind::Label)
            .with_icon(
                UiIcon::new(
                    descriptor
                        .map(|descriptor| nodes_catalog::icon_for_slug(descriptor.slug))
                        .unwrap_or(UiIconId::Node),
                )
                .with_size(UiIconSize::Custom(icon_size))
                .with_tint(category_color),
            )
            .with_layout(
                UiLayout::absolute(UiRect::new(
                    9.0 * scale,
                    (header_height - f32::from(icon_size)) * 0.5,
                    f32::from(icon_size),
                    f32::from(icon_size),
                ))
                .with_z_index(3),
            ),
    );

    // The delete affordance only exists on the selected card, so an unselected
    // card shows nothing but its identity and its connectors.
    let title_reserve = if selected { 22.0 * scale } else { 10.0 * scale };
    let title_x = 9.0 * scale + f32::from(icon_size) + 6.0 * scale;
    let title_width = (width - title_x - title_reserve).max(20.0);
    card = card.with_child(
        UiNode::new(format!("nodes.card.{id}.name"), UiNodeKind::Label)
            .with_text_key(title_key)
            .with_text_style(UiTextStyle {
                role: UiTextRole::Button,
                size_px: 12.0 * scale,
                line_height_px: header_height,
                weight: UiFontWeight::Medium,
                color: tokens.text,
                inherit_color: false,
            })
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_layout(
                UiLayout::absolute(UiRect::new(title_x, 0.0, title_width, header_height))
                    .with_z_index(4),
            ),
    );

    if selected {
        let delete_size = (14.0 * scale).round().clamp(12.0, 20.0);
        card = card.with_child(
            UiNode::new(format!("nodes.card.{id}.delete"), UiNodeKind::Button)
                .with_icon(
                    UiIcon::new(UiIconId::Close)
                        .with_size(UiIconSize::Custom(delete_size as u16))
                        .with_tint(tokens.text_muted),
                )
                .with_layout(
                    UiLayout::absolute(UiRect::new(
                        width - delete_size - 5.0 * scale,
                        (header_height - delete_size) * 0.5,
                        delete_size,
                        delete_size,
                    ))
                    .with_z_index(8),
                )
                .with_style(UiStyle {
                    fill: [0, 0, 0, 0],
                    border: [0, 0, 0, 0],
                    text: tokens.text_muted,
                    border_width: 0.0,
                    radius: 3.0,
                    opacity: 0.8,
                })
                .with_tooltip_key("nodes.delete")
                .with_accessibility_label_key("nodes.delete")
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("nodes.delete.{id}"),
                )),
        );
    }

    // Resolve pending pin info for magnetic affordances
    let pending_info = pending_pin.and_then(|pending| {
        let source_node = graph.node(pending.node_id)?;
        let source_pin = source_node.pins.iter().find(|p| p.id == pending.pin_id)?;
        Some((
            pending.node_id,
            pending.pin_id,
            source_pin.kind,
            source_pin.data_type,
        ))
    });

    for pin in &node.pins {
        let label = nodes_catalog::localized_pin_label(node, &pin.name, language);
        let rect = nodes_canvas::pin_rect(node, pin, zoom);
        let output = matches!(pin.kind, PinKind::Output);
        // Inputs and outputs are laid out in two independent stacks, so the
        // same row can hold one of each. Each label therefore owns half of the
        // body: a full width label made both sides overlap.
        let inset = nodes_canvas::NODE_CONNECTOR_SIZE * scale + 6.0 * scale;
        let label_height = 13.0 * scale;
        let label_width = ((width - inset * 2.0) * 0.5).max(20.0);
        let (label_x, label_y) = if output {
            (
                width - inset - label_width,
                rect.y + (rect.height - label_height) * 0.5,
            )
        } else {
            (inset, rect.y + (rect.height - label_height) * 0.5)
        };

        // Check if this pin is compatible with the in-flight/pending pin
        let (is_self_pending, is_compatible) = match pending_info {
            Some((p_node, p_pin, p_kind, p_type)) => {
                if p_node == node.id && p_pin == pin.id {
                    (true, false)
                } else if p_node != node.id
                    && p_kind != pin.kind
                    && raf_nodes::types_compatible(p_type, pin.data_type)
                {
                    (false, true)
                } else {
                    (false, false)
                }
            }
            None => (false, false),
        };

        let is_dimmed = pending_info.is_some() && !is_self_pending && !is_compatible;

        let pin_label_node = UiNode::new(
            format!("nodes.pin.{}.{}.label", node.id.0, pin.id),
            UiNodeKind::Label,
        )
        .with_text_value(label)
        .with_text_style(UiTextStyle {
            role: UiTextRole::Label,
            size_px: 10.5 * scale,
            line_height_px: 14.0 * scale,
            weight: if is_self_pending || is_compatible {
                UiFontWeight::Bold
            } else {
                UiFontWeight::Regular
            },
            color: if is_dimmed {
                [
                    tokens.text_muted[0],
                    tokens.text_muted[1],
                    tokens.text_muted[2],
                    75,
                ]
            } else if output {
                tokens.text
            } else {
                tokens.text_muted
            },
            inherit_color: false,
        })
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .with_layout(
            UiLayout::absolute(UiRect::new(label_x, label_y, label_width, label_height))
                .with_z_index(nodes_canvas::Z_PIN_LABEL),
        );
        card = card.with_child(pin_label_node);

        card = card.with_child(pin_button(
            palette,
            node,
            pin,
            is_self_pending,
            is_compatible,
            is_dimmed,
            language,
            zoom,
        ));
    }
    card
}
fn canvas_style(tokens: raf_render::api_graphic_basic::ui_surface::UiTokens) -> UiStyle {
    UiStyle {
        fill: tokens.surface_alt,
        border: tokens.border,
        text: tokens.text,
        border_width: 1.0,
        radius: 0.0,
        opacity: 1.0,
    }
}
