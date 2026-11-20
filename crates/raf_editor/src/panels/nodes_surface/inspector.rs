//! Node inspector panel for the Nodes authoring surface.

use super::*;

pub(super) fn build_empty_inspector(palette: StudioUiPalette) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("nodes.inspector.empty", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 8.0,
            padding: UiSpacing::same(16.0),
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(panel_style(tokens.surface, tokens.border, tokens.text))
        .with_child(
            UiNode::new("nodes.inspector.empty.title", UiNodeKind::Label)
                .with_text_key("nodes.inspector")
                .with_text_style(UiTextStyle::panel_title(tokens.text)),
        )
        .with_child(
            UiNode::new("nodes.inspector.empty.hint", UiNodeKind::Label)
                .with_text_key("nodes.inspector.select_hint")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_text_overflow(UiTextOverflow::Wrap)
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        )
}

pub(super) fn build_inspector(
    palette: StudioUiPalette,
    graph: &NodeGraph,
    node: &Node,
    language: raf_core::Language,
) -> UiNode {
    let tokens = palette.tokens();
    let id = node.id.0.to_string();
    let category_color = category_color(node.category);
    let descriptor = raf_nodes::catalog::descriptor_for_node(node);
    let title_key = descriptor
        .map(|descriptor| descriptor.label_key)
        .unwrap_or("nodes.node.unknown");
    let description_key = descriptor
        .map(|descriptor| descriptor.description_key)
        .unwrap_or("nodes.description.unknown");

    let mut content = UiNode::new("nodes.inspector.content", UiNodeKind::ScrollView)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 10.0,
            padding: UiSpacing::same(12.0),
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_control(raf_ui::UiControl::ScrollView {
            axis: raf_ui::UiScrollAxis::Vertical,
        });

    // 1. Identity Header Banner
    content = content.with_child(
        UiNode::new("nodes.inspector.header_box", UiNodeKind::Panel)
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                gap: 5.0,
                padding: UiSpacing::xy(10.0, 8.0),
                ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
            })
            .with_style(UiStyle {
                fill: tokens.surface_alt,
                border: [category_color[0], category_color[1], category_color[2], 120],
                text: tokens.text,
                border_width: 1.0,
                radius: 6.0,
                opacity: 1.0,
            })
            .with_child(
                UiNode::new("nodes.inspector.top_row", UiNodeKind::Panel)
                    .with_layout(UiLayout {
                        flow: UiFlow::Row,
                        align_items: UiAlign::Center,
                        gap: 6.0,
                        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                    })
                    .with_child(
                        UiNode::new("nodes.inspector.cat_pill", UiNodeKind::Label)
                            .with_text_key(nodes_catalog::category_key(node.category))
                            .with_text_style(UiTextStyle {
                                role: UiTextRole::Label,
                                size_px: 9.0,
                                line_height_px: 12.0,
                                weight: UiFontWeight::Bold,
                                color: category_color,
                                inherit_color: false,
                            }),
                    )
                    .with_child(
                        UiNode::new("nodes.inspector.header_spacer", UiNodeKind::Panel)
                            .with_layout(UiLayout {
                                grow: 1.0,
                                ..UiLayout::fit_content()
                            }),
                    )
                    .with_child(
                        UiNode::new("nodes.inspector.status_badge", UiNodeKind::Label)
                            .with_text_value("READY")
                            .with_text_style(UiTextStyle {
                                role: UiTextRole::Label,
                                size_px: 8.5,
                                line_height_px: 12.0,
                                weight: UiFontWeight::Bold,
                                color: tokens.positive,
                                inherit_color: false,
                            }),
                    ),
            )
            .with_child(
                UiNode::new("nodes.inspector.node-name", UiNodeKind::Label)
                    .with_text_key(title_key)
                    .with_text_style(UiTextStyle {
                        role: UiTextRole::PanelTitle,
                        size_px: 14.0,
                        line_height_px: 18.0,
                        weight: UiFontWeight::Bold,
                        color: tokens.text,
                        inherit_color: false,
                    })
                    .with_text_overflow(UiTextOverflow::Ellipsis),
            )
            .with_child(
                UiNode::new("nodes.inspector.id_label", UiNodeKind::Label)
                    .with_text_value(format!("ID: #{}", &id[..8]))
                    .with_text_style(UiTextStyle {
                        role: UiTextRole::Label,
                        size_px: 9.0,
                        line_height_px: 12.0,
                        weight: UiFontWeight::Regular,
                        color: tokens.text_muted,
                        inherit_color: false,
                    }),
            )
            // Description lives inside the header box so its wrapped text
            // participates in the same measured column as the identity rows.
            .with_child(
                UiNode::new("nodes.inspector.description", UiNodeKind::Label)
                    .with_text_key(description_key)
                    .with_text_style(UiTextStyle {
                        role: UiTextRole::Body,
                        size_px: 10.5,
                        line_height_px: 15.0,
                        weight: UiFontWeight::Regular,
                        color: tokens.text_muted,
                        inherit_color: false,
                    })
                    .with_text_overflow(UiTextOverflow::Wrap)
                    .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
            ),
    );

    // 2. Properties Section
    content = content.with_child(section_heading(
        "nodes.inspector.properties-title",
        format!("PROPERTIES ({})", node.properties.len()),
        tokens.text_muted,
    ));

    if node.properties.is_empty() {
        content = content.with_child(
            UiNode::new("nodes.inspector.properties.empty", UiNodeKind::Label)
                .with_text_key("nodes.properties.empty")
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        );
    } else {
        for property in &node.properties {
            content = content.with_child(property_field(palette, node, property));
        }
    }

    // 3. Pin Mapping Section
    let connection_count = graph.connections_for(node.id).len();
    content = content.with_child(section_heading(
        "nodes.inspector.pins-title",
        format!("CONNECTORS & PINS ({connection_count} active)"),
        tokens.text_muted,
    ));

    // Group pins into Inputs and Outputs
    let input_pins: Vec<&NodePin> = node
        .pins
        .iter()
        .filter(|p| matches!(p.kind, PinKind::Input))
        .collect();
    let output_pins: Vec<&NodePin> = node
        .pins
        .iter()
        .filter(|p| matches!(p.kind, PinKind::Output))
        .collect();

    if !input_pins.is_empty() {
        content = content.with_child(
            UiNode::new("nodes.inspector.inputs_sub", UiNodeKind::Label)
                .with_text_value("INPUTS")
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Label,
                    size_px: 9.0,
                    line_height_px: 13.0,
                    weight: UiFontWeight::Bold,
                    color: tokens.text_muted,
                    inherit_color: false,
                }),
        );
        for (index, pin) in input_pins.iter().enumerate() {
            let is_linked = graph
                .connections
                .iter()
                .any(|c| c.to_node == node.id && c.to_pin == pin.id);
            content = content.with_child(inspector_pin_row(
                palette, node, pin, index, is_linked, language,
            ));
        }
    }

    if !output_pins.is_empty() {
        content = content.with_child(
            UiNode::new("nodes.inspector.outputs_sub", UiNodeKind::Label)
                .with_text_value("OUTPUTS")
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Label,
                    size_px: 9.0,
                    line_height_px: 13.0,
                    weight: UiFontWeight::Bold,
                    color: tokens.text_muted,
                    inherit_color: false,
                }),
        );
        for (index, pin) in output_pins.iter().enumerate() {
            let is_linked = graph
                .connections
                .iter()
                .any(|c| c.from_node == node.id && c.from_pin == pin.id);
            content = content.with_child(inspector_pin_row(
                palette,
                node,
                pin,
                index + 100,
                is_linked,
                language,
            ));
        }
    }

    // 4. Actions Footer
    content = content.with_child(
        UiNode::new("nodes.inspector.footer_actions", UiNodeKind::Panel)
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                gap: 6.0,
                padding: UiSpacing::xy(0.0, 8.0),
                ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
            })
            .with_child(
                UiNode::new(
                    format!("nodes.inspector.disconnect_all.{id}"),
                    UiNodeKind::Button,
                )
                .with_class("nodes-small-btn")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 4.0,
                    padding: UiSpacing::xy(8.0, 4.0),
                    grow: 1.0,
                    ..UiLayout::fit_content().fixed_height(26.0)
                })
                .with_style(UiStyle {
                    fill: tokens.surface_alt,
                    border: tokens.border,
                    text: tokens.text,
                    border_width: 1.0,
                    radius: 4.0,
                    opacity: 1.0,
                })
                .with_child(
                    UiNode::new("nodes.inspector.disconnect_lbl", UiNodeKind::Label)
                        .with_text_value("Disconnect All")
                        .with_text_style(UiTextStyle {
                            role: UiTextRole::Button,
                            size_px: 9.5,
                            line_height_px: 14.0,
                            weight: UiFontWeight::Regular,
                            color: tokens.text_muted,
                            inherit_color: false,
                        }),
                )
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("nodes.disconnect_all.{id}"),
                )),
            )
            .with_child(
                UiNode::new(
                    format!("nodes.inspector.delete_btn.{id}"),
                    UiNodeKind::Button,
                )
                .with_class("nodes-small-btn")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 4.0,
                    padding: UiSpacing::xy(8.0, 4.0),
                    ..UiLayout::fit_content().fixed_height(26.0)
                })
                .with_style(UiStyle {
                    fill: [tokens.danger[0], tokens.danger[1], tokens.danger[2], 30],
                    border: tokens.danger,
                    text: tokens.danger,
                    border_width: 1.0,
                    radius: 4.0,
                    opacity: 1.0,
                })
                .with_icon(
                    UiIcon::new(UiIconId::Trash)
                        .with_size(UiIconSize::Small)
                        .with_tint(tokens.danger),
                )
                .with_child(
                    UiNode::new("nodes.inspector.delete_lbl", UiNodeKind::Label)
                        .with_text_value("Delete")
                        .with_text_style(UiTextStyle {
                            role: UiTextRole::Button,
                            size_px: 9.5,
                            line_height_px: 14.0,
                            weight: UiFontWeight::Bold,
                            color: tokens.danger,
                            inherit_color: false,
                        }),
                )
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("nodes.delete.{id}"),
                )),
            ),
    );

    content
}

fn property_field(
    palette: StudioUiPalette,
    node: &Node,
    property: &raf_nodes::NodeProperty,
) -> UiNode {
    let tokens = palette.tokens();
    let node_id = node.id.0;
    let key = property.key.as_str();
    let value_key = format!("nodes.property.{node_id}.{key}");
    let label_key = if property.label_key.is_empty() {
        nodes_catalog::property_label_key(key)
    } else {
        property.label_key.clone()
    };
    let input = UiNode::text_input(
        format!("nodes.property.control.{node_id}.{key}"),
        UiTextInput {
            value_key: value_key.clone(),
            placeholder_key: Some("nodes.property.value_placeholder".to_string()),
            max_length: 512,
            multiline: false,
            password: false,
            submit_command: Some(format!("nodes.property.commit:{node_id}:{key}")),
        },
    )
    .with_class("nodes-property-input")
    .with_layout(UiLayout::fixed(0.0, 26.0).with_width_mode(UiSizeMode::Fill))
    .with_text_style(UiTextStyle::body(tokens.text))
    .with_accessibility_role(UiAccessibilityRole::Textbox)
    .with_accessibility_label_key(label_key.clone())
    .with_text_overflow(UiTextOverflow::Clip);

    UiNode::new(
        format!("nodes.inspector.property.{node_id}.{key}"),
        UiNodeKind::Panel,
    )
    .with_class("nodes-property-field")
    .with_layout(UiLayout {
        flow: UiFlow::Column,
        gap: 3.0,
        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
    })
    .with_child(
        UiNode::new(
            format!("nodes.inspector.property.{node_id}.{key}.label"),
            UiNodeKind::Label,
        )
        .with_text_key(label_key)
        .with_text_style(UiTextStyle {
            role: UiTextRole::Label,
            size_px: 10.0,
            line_height_px: 14.0,
            weight: UiFontWeight::Bold,
            color: tokens.text_muted,
            inherit_color: false,
        }),
    )
    .with_child(input)
}

fn inspector_pin_row(
    palette: StudioUiPalette,
    node: &Node,
    pin: &NodePin,
    index: usize,
    is_linked: bool,
    language: raf_core::Language,
) -> UiNode {
    let tokens = palette.tokens();
    let name_str = nodes_catalog::localized_pin_label(node, &pin.name, language);
    let type_key = nodes_catalog::pin_type_key(pin.data_type);
    let type_str = raf_core::i18n::t(type_key, language);
    let pin_color = nodes_canvas::pin_color(pin.data_type, tokens.accent);

    UiNode::new(format!("nodes.inspector.pin.{index}"), UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            padding: UiSpacing::xy(6.0, 3.0),
            ..UiLayout::fixed(0.0, 24.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_style(UiStyle {
            fill: tokens.surface_alt,
            border: [tokens.border[0], tokens.border[1], tokens.border[2], 80],
            text: tokens.text,
            border_width: 1.0,
            radius: 3.0,
            opacity: 1.0,
        })
        .with_child(
            UiNode::new(
                format!("nodes.inspector.pin.{index}.dot"),
                UiNodeKind::Panel,
            )
            .with_layout(UiLayout::fixed(10.0, 10.0))
            .with_style(UiStyle {
                fill: pin_color,
                border: tokens.text,
                text: tokens.text,
                border_width: 1.0,
                radius: 5.0,
                opacity: 1.0,
            }),
        )
        .with_child(
            UiNode::new(
                format!("nodes.inspector.pin.{index}.name"),
                UiNodeKind::Label,
            )
            .with_text_value(name_str)
            .with_text_style(UiTextStyle {
                role: UiTextRole::Body,
                size_px: 10.5,
                line_height_px: 14.0,
                weight: UiFontWeight::Regular,
                color: tokens.text,
                inherit_color: false,
            })
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_layout(UiLayout {
                grow: 1.0,
                ..UiLayout::fit_content()
            }),
        )
        .with_child(
            UiNode::new(
                format!("nodes.inspector.pin.{index}.type"),
                UiNodeKind::Label,
            )
            .with_text_value(type_str)
            .with_text_style(UiTextStyle {
                role: UiTextRole::Label,
                size_px: 9.0,
                line_height_px: 12.0,
                weight: UiFontWeight::Regular,
                color: tokens.text_muted,
                inherit_color: false,
            }),
        )
        .with_child(
            UiNode::new(
                format!("nodes.inspector.pin.{index}.linked"),
                UiNodeKind::Label,
            )
            .with_text_value(if is_linked { "Linked" } else { "Unlinked" })
            .with_text_style(UiTextStyle {
                role: UiTextRole::Label,
                size_px: 8.5,
                line_height_px: 12.0,
                weight: UiFontWeight::Bold,
                color: if is_linked {
                    tokens.accent
                } else {
                    tokens.text_muted
                },
                inherit_color: false,
            }),
        )
}
fn section_heading(id: &str, title: String, color: [u8; 4]) -> UiNode {
    UiNode::new(id.to_string(), UiNodeKind::Label)
        .with_class("nodes-section-heading")
        .with_text_value(title)
        .with_text_style(UiTextStyle {
            role: UiTextRole::Label,
            size_px: 9.5,
            line_height_px: 14.0,
            weight: UiFontWeight::Bold,
            color,
            inherit_color: false,
        })
        .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill))
}
