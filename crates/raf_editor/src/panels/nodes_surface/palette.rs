//! Node palette sidebar and quick-add popup for the Nodes authoring surface.

use super::toolbar::icon_button;
use super::*;

pub(super) fn build_palette(
    palette: StudioUiPalette,
    host: &NodesSurfaceHost,
    language: raf_core::Language,
    can_undo: bool,
    can_redo: bool,
) -> UiNode {
    let tokens = palette.tokens();
    let query = host.query().trim().to_ascii_lowercase();

    let mut panel = UiNode::new("nodes.palette", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 6.0,
            padding: UiSpacing::same(8.0),
            ..UiLayout::fixed(260.0, 0.0).with_height_mode(UiSizeMode::Fill)
        })
        .with_style(panel_style(tokens.surface, tokens.border, tokens.text));

    // Section 1: CONTEXT ROW. The flow switcher is context, not a tool, so it
    // stays a single compact row and gives the library the rest of the panel.
    // Active Flow Pill Card. The flow switcher is context, not a tool, so it
    // stays a single compact row at the top and gives the library the rest.
    panel = panel.with_child(
        UiNode::new("nodes.flows.active_card", UiNodeKind::Panel)
            .with_class("nodes-flow-card")
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 7.0,
                padding: UiSpacing::xy(8.0, 4.0),
                ..UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)
            })
            .with_style(UiStyle {
                fill: tokens.surface_alt,
                border: [tokens.border[0], tokens.border[1], tokens.border[2], 180],
                text: tokens.text,
                border_width: 1.0,
                radius: 5.0,
                opacity: 1.0,
            })
            .with_icon(
                UiIcon::new(UiIconId::Route)
                    .with_size(UiIconSize::Small)
                    .with_tint(tokens.accent),
            )
            // The new-flow action sits before the growing label: an empty
            // `grow` sibling placed before a control collapses that control.
            .with_child(
                UiNode::new("nodes.flows.new_btn", UiNodeKind::Button)
                    .with_class("nodes-icon-btn")
                    .with_icon(
                        UiIcon::new(UiIconId::Add)
                            .with_size(UiIconSize::Small)
                            .with_tint(tokens.accent),
                    )
                    .with_layout(UiLayout::fixed(22.0, 22.0))
                    .with_style(UiStyle {
                        fill: [tokens.accent[0], tokens.accent[1], tokens.accent[2], 34],
                        border: [tokens.accent[0], tokens.accent[1], tokens.accent[2], 120],
                        text: tokens.text,
                        border_width: 1.0,
                        radius: 4.0,
                        opacity: 1.0,
                    })
                    .with_tooltip_key("nodes.new_graph")
                    .with_accessibility_label_key("nodes.new_graph")
                    .focusable()
                    .with_event(UiEventBinding::command(
                        UiEventKind::Click,
                        "nodes.new-graph",
                    )),
            )
            .with_child(
                UiNode::new("nodes.flows.active_name", UiNodeKind::Label)
                    .with_text_value(host.active_flow().to_string())
                    .with_text_style(UiTextStyle {
                        role: UiTextRole::Button,
                        size_px: 11.5,
                        line_height_px: 16.0,
                        weight: UiFontWeight::Bold,
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
                UiNode::new("nodes.flows.active_badge", UiNodeKind::Label)
                    .with_text_value("ACTIVE")
                    .with_text_style(UiTextStyle {
                        role: UiTextRole::Label,
                        size_px: 8.5,
                        line_height_px: 12.0,
                        weight: UiFontWeight::Bold,
                        color: tokens.positive,
                        inherit_color: false,
                    }),
            ),
    );

    // Section 2: SEARCH FIRST. It is the primary entry point of the library,
    // so it sits directly under the context row instead of below a title.
    panel = panel.with_child(
        UiNode::text_input(
            "nodes.search",
            UiTextInput {
                value_key: "nodes.search".to_string(),
                placeholder_key: Some("nodes.search.placeholder".to_string()),
                max_length: 128,
                multiline: false,
                password: false,
                submit_command: None,
            },
        )
        .with_class("nodes-search")
        .with_layout(UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill))
        .with_text_style(UiTextStyle::body(tokens.text))
        .with_accessibility_role(UiAccessibilityRole::Textbox),
    );

    // Section 3: LIBRARY LABEL + COUNT, kept as a quiet caption for the list.
    panel = panel.with_child(
        UiNode::new("nodes.library.header", UiNodeKind::Panel)
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 5.0,
                padding: UiSpacing::xy(2.0, 1.0),
                ..UiLayout::fixed(0.0, 16.0).with_width_mode(UiSizeMode::Fill)
            })
            .with_child(
                UiNode::new("nodes.library.title", UiNodeKind::Label)
                    .with_text_value("NODE LIBRARY")
                    .with_text_style(UiTextStyle {
                        role: UiTextRole::Label,
                        size_px: 9.0,
                        line_height_px: 13.0,
                        weight: UiFontWeight::Bold,
                        color: tokens.text_muted,
                        inherit_color: false,
                    })
                    .with_layout(UiLayout {
                        grow: 1.0,
                        ..UiLayout::fit_content()
                    }),
            )
            .with_child(
                UiNode::new("nodes.library.count", UiNodeKind::Label)
                    .with_text_value(format!("{} nodes", raf_nodes::catalog::descriptors().len()))
                    .with_text_style(UiTextStyle {
                        role: UiTextRole::Label,
                        size_px: 9.0,
                        line_height_px: 12.0,
                        weight: UiFontWeight::Regular,
                        color: tokens.text_muted,
                        inherit_color: false,
                    }),
            ),
    );

    // Section 4: ACCORDION LIST OF CATEGORIES AND NODES
    let mut list =
        UiNode::scroll_view("nodes.palette.list", UiScrollAxis::Vertical).with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            grow: 1.0,
            overflow: UiOverflow::ScrollY,
            ..UiLayout::default()
        });

    let categories = [
        NodeCategory::Event,
        NodeCategory::Action,
        NodeCategory::Logic,
        NodeCategory::Math,
        NodeCategory::Electronics,
        NodeCategory::Variable,
    ];

    let mut total_matches = 0;

    for category in categories {
        let cat_slug = nodes_catalog::category_slug(category);
        let cat_color = category_color(category);
        let cat_name = raf_core::i18n::t(nodes_catalog::category_key(category), language);
        let is_collapsed = host.is_category_collapsed(cat_slug) && query.is_empty();

        // Collect matching descriptors for this category
        let mut matching_nodes = Vec::new();
        for descriptor in raf_nodes::catalog::descriptors() {
            if descriptor.category != category {
                continue;
            }
            let localized_label =
                raf_core::i18n::t(descriptor.label_key, language).to_ascii_lowercase();
            let localized_cat = cat_name.to_ascii_lowercase();
            if !query.is_empty()
                && !descriptor.slug.contains(&query)
                && !localized_label.contains(&query)
                && !localized_cat.contains(&query)
            {
                continue;
            }
            matching_nodes.push(descriptor);
        }

        if matching_nodes.is_empty() && !query.is_empty() {
            continue;
        }

        total_matches += matching_nodes.len();

        // Accordion Category Header Button
        let chevron_icon = if is_collapsed {
            UiIconId::ChevronRight
        } else {
            UiIconId::ChevronDown
        };

        let cat_header = UiNode::new(format!("nodes.cat.btn.{cat_slug}"), UiNodeKind::Button)
            .with_class("nodes-cat-header")
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 6.0,
                padding: UiSpacing::xy(6.0, 0.0),
                ..UiLayout::fixed(0.0, 22.0).with_width_mode(UiSizeMode::Fill)
            })
            .with_style(UiStyle {
                fill: [cat_color[0], cat_color[1], cat_color[2], 28],
                border: [cat_color[0], cat_color[1], cat_color[2], 75],
                text: tokens.text,
                border_width: 1.0,
                radius: 4.0,
                opacity: 1.0,
            })
            .with_icon(
                UiIcon::new(nodes_catalog::category_icon(category))
                    .with_size(UiIconSize::Small)
                    .with_tint(cat_color),
            )
            .with_child(
                UiNode::new(format!("nodes.cat.title.{cat_slug}"), UiNodeKind::Label)
                    .with_text_value(cat_name)
                    .with_text_style(UiTextStyle {
                        role: UiTextRole::Button,
                        size_px: 10.5,
                        line_height_px: 14.0,
                        weight: UiFontWeight::Bold,
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
                UiNode::new(format!("nodes.cat.count.{cat_slug}"), UiNodeKind::Label)
                    .with_text_value(format!("({})", matching_nodes.len()))
                    .with_text_style(UiTextStyle {
                        role: UiTextRole::Label,
                        size_px: 9.0,
                        line_height_px: 12.0,
                        weight: UiFontWeight::Regular,
                        color: cat_color,
                        inherit_color: false,
                    }),
            )
            .with_child(
                UiNode::new(format!("nodes.cat.chev.{cat_slug}"), UiNodeKind::Label)
                    .with_icon(
                        UiIcon::new(chevron_icon)
                            .with_size(UiIconSize::Small)
                            .with_tint(tokens.text_muted),
                    )
                    .with_layout(UiLayout::fixed(14.0, 14.0)),
            )
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("nodes.toggle_category.{cat_slug}"),
            ));

        list = list.with_child(cat_header);

        // If category is open, render nodes inside
        if !is_collapsed {
            for descriptor in matching_nodes {
                let node_slug = descriptor.slug;
                let localized_node_title = raf_core::i18n::t(descriptor.label_key, language);
                let icon_id = nodes_catalog::icon_for_slug(node_slug);

                // One line per node. The description moved to the tooltip: two
                // lines per row meant only six nodes fit on screen.
                let item_btn =
                    UiNode::new(format!("nodes.palette.{node_slug}"), UiNodeKind::Button)
                        .with_class("nodes-palette-card")
                        .with_layout(UiLayout {
                            flow: UiFlow::Row,
                            align_items: UiAlign::Center,
                            gap: 8.0,
                            padding: UiSpacing::xy(8.0, 0.0),
                            ..UiLayout::fixed(0.0, 26.0).with_width_mode(UiSizeMode::Fill)
                        })
                        .with_style(UiStyle {
                            fill: tokens.surface_alt,
                            border: tokens.border,
                            text: tokens.text,
                            border_width: 1.0,
                            radius: 4.0,
                            opacity: 1.0,
                        })
                        .with_icon(
                            UiIcon::new(icon_id)
                                .with_size(UiIconSize::Small)
                                .with_tint(cat_color),
                        )
                        .with_child(
                            UiNode::new(
                                format!("nodes.palette.{node_slug}.name"),
                                UiNodeKind::Label,
                            )
                            .with_text_value(localized_node_title)
                            .with_text_style(UiTextStyle {
                                role: UiTextRole::Button,
                                size_px: 11.0,
                                line_height_px: 26.0,
                                weight: UiFontWeight::Regular,
                                color: tokens.text,
                                inherit_color: false,
                            })
                            .with_text_overflow(UiTextOverflow::Ellipsis)
                            .with_layout(UiLayout {
                                grow: 1.0,
                                ..UiLayout::fixed(0.0, 26.0).with_width_mode(UiSizeMode::Fill)
                            }),
                        )
                        .with_child(
                            UiNode::new(
                                format!("nodes.palette.{node_slug}.add_icon"),
                                UiNodeKind::Label,
                            )
                            .with_icon(
                                UiIcon::new(UiIconId::Add)
                                    .with_size(UiIconSize::Small)
                                    .with_tint(tokens.text_muted),
                            )
                            .with_layout(UiLayout::fixed(12.0, 12.0)),
                        )
                        .with_tooltip_key(descriptor.description_key)
                        .focusable()
                        .with_event(UiEventBinding::command(
                            UiEventKind::Click,
                            format!("nodes.add.{node_slug}"),
                        ));

                list = list.with_child(item_btn);
            }
        }
    }

    if total_matches == 0 {
        list = list.with_child(
            UiNode::new("nodes.palette.no-results", UiNodeKind::Label)
                .with_text_key("nodes.palette.no_results")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        );
    }

    panel = panel.with_child(list);

    // Section 5: BOTTOM TOOLBAR (History & Compile)
    panel = panel.with_child(
        UiNode::new("nodes.palette.bottom_bar", UiNodeKind::Panel)
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                justify_content: UiJustify::SpaceBetween,
                gap: 5.0,
                padding: UiSpacing::xy(2.0, 4.0),
                ..UiLayout::fixed(0.0, 32.0).with_width_mode(UiSizeMode::Fill)
            })
            .with_child(disabled_icon_button(
                "nodes.undo",
                UiIconId::Undo,
                "nodes.undo",
                palette,
                !can_undo,
            ))
            .with_child(disabled_icon_button(
                "nodes.redo",
                UiIconId::Redo,
                "nodes.redo",
                palette,
                !can_redo,
            ))
            .with_child(
                UiNode::new("nodes.palette.compile_btn", UiNodeKind::Button)
                    .with_class("nodes-small-btn")
                    .with_layout(UiLayout {
                        flow: UiFlow::Row,
                        align_items: UiAlign::Center,
                        gap: 4.0,
                        padding: UiSpacing::xy(8.0, 3.0),
                        ..UiLayout::fixed(0.0, 26.0)
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
                        UiIcon::new(UiIconId::Success)
                            .with_size(UiIconSize::Small)
                            .with_tint(tokens.positive),
                    )
                    .with_child(
                        UiNode::new("nodes.palette.compile_lbl", UiNodeKind::Label)
                            .with_text_key("nodes.validate")
                            .with_text_style(UiTextStyle {
                                role: UiTextRole::Button,
                                size_px: 10.0,
                                line_height_px: 14.0,
                                weight: UiFontWeight::Bold,
                                color: tokens.text,
                                inherit_color: false,
                            }),
                    )
                    .focusable()
                    .with_event(UiEventBinding::command(UiEventKind::Click, "nodes.compile")),
            ),
    );

    panel
}
fn disabled_icon_button(
    id: &str,
    icon: UiIconId,
    tooltip_key: &str,
    palette: StudioUiPalette,
    disabled: bool,
) -> UiNode {
    icon_button(id, icon, tooltip_key, palette).disabled(disabled)
}

/// Quick-add palette shown at the pointer. This is the primary way to author a
/// graph: double click or secondary click on empty canvas, type, pick a node.
/// Each row spawns the node exactly under the cursor, snapped to the grid.
pub(super) fn build_palette_popup(
    palette: StudioUiPalette,
    host: &NodesSurfaceHost,
    language: raf_core::Language,
) -> Option<UiSurface> {
    let popup = host.palette_popup()?;
    let tokens = palette.tokens();
    let query = host.palette_query().trim().to_ascii_lowercase();

    let mut root = UiNode::new("nodes.palette-popup", UiNodeKind::Menu)
        .with_class("nodes-palette-popup")
        .with_material(raf_ui::UiSurfaceMaterial::TranslucentRaised)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            padding: UiSpacing::same(6.0),
            ..UiLayout::fixed(NODES_PALETTE_POPUP_WIDTH - 12.0, 0.0)
                .with_height_mode(UiSizeMode::FitContent)
        });

    root = root.with_child(
        UiNode::text_input(
            "nodes.palette-popup.search",
            UiTextInput {
                value_key: "nodes.palette-popup.search".to_string(),
                placeholder_key: Some("nodes.search.placeholder".to_string()),
                max_length: 64,
                multiline: false,
                password: false,
                submit_command: None,
            },
        )
        .with_class("nodes-search")
        .with_layout(UiLayout::fixed(0.0, 26.0).with_width_mode(UiSizeMode::Fill))
        .with_text_style(UiTextStyle::body(tokens.text))
        .with_accessibility_role(UiAccessibilityRole::Textbox),
    );

    let categories = [
        NodeCategory::Event,
        NodeCategory::Action,
        NodeCategory::Logic,
        NodeCategory::Math,
        NodeCategory::Electronics,
        NodeCategory::Variable,
    ];

    let mut list =
        UiNode::new("nodes.palette-popup.list", UiNodeKind::Panel).with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 2.0,
            ..UiLayout::fixed(0.0, 0.0)
                .with_width_mode(UiSizeMode::Fill)
                .with_height_mode(UiSizeMode::FitContent)
        });

    let mut matches = 0usize;
    for category in categories {
        let cat_color = category_color(category);
        let mut rows = Vec::new();
        for descriptor in raf_nodes::catalog::descriptors() {
            if descriptor.category != category {
                continue;
            }
            if !query.is_empty() {
                let label = raf_core::i18n::t(descriptor.label_key, language).to_ascii_lowercase();
                if !descriptor.slug.contains(&query) && !label.contains(&query) {
                    continue;
                }
            }
            rows.push(descriptor);
        }
        if rows.is_empty() {
            continue;
        }
        matches += rows.len();
        // Cap the popup so it never becomes a second palette sidebar.
        if matches > 40 {
            break;
        }

        list = list.with_child(
            UiNode::new(
                format!(
                    "nodes.palette-popup.cat.{}",
                    nodes_catalog::category_slug(category)
                ),
                UiNodeKind::Label,
            )
            .with_text_key(nodes_catalog::category_key(category))
            .with_text_style(UiTextStyle {
                role: UiTextRole::Label,
                size_px: 8.5,
                line_height_px: 12.0,
                weight: UiFontWeight::Bold,
                color: cat_color,
                inherit_color: false,
            })
            .with_layout(UiLayout {
                padding: UiSpacing::xy(6.0, 2.0),
                ..UiLayout::fixed(0.0, 14.0).with_width_mode(UiSizeMode::Fill)
            }),
        );

        for descriptor in rows {
            list = list.with_child(
                UiNode::new(
                    format!("nodes.palette-popup.item.{}", descriptor.slug),
                    UiNodeKind::Button,
                )
                .with_class("nodes-palette-popup-item")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 7.0,
                    padding: UiSpacing::xy(6.0, 0.0),
                    ..UiLayout::fixed(0.0, 24.0).with_width_mode(UiSizeMode::Fill)
                })
                .with_icon(
                    UiIcon::new(nodes_catalog::icon_for_slug(descriptor.slug))
                        .with_size(UiIconSize::Small)
                        .with_tint(cat_color),
                )
                .with_child(
                    UiNode::new(
                        format!("nodes.palette-popup.item.{}.label", descriptor.slug),
                        UiNodeKind::Label,
                    )
                    .with_text_key(descriptor.label_key)
                    .with_text_style(UiTextStyle {
                        role: UiTextRole::Label,
                        size_px: 11.0,
                        line_height_px: 24.0,
                        weight: UiFontWeight::Regular,
                        color: tokens.text,
                        inherit_color: false,
                    })
                    .with_text_overflow(UiTextOverflow::Ellipsis)
                    .with_layout(UiLayout {
                        grow: 1.0,
                        ..UiLayout::fixed(0.0, 24.0).with_width_mode(UiSizeMode::Fill)
                    }),
                )
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!(
                        "nodes.add.at:{}:{:.1}:{:.1}",
                        descriptor.slug, popup.spawn[0], popup.spawn[1]
                    ),
                )),
            );
        }
    }

    if matches == 0 {
        list = list.with_child(
            UiNode::new("nodes.palette-popup.empty", UiNodeKind::Label)
                .with_text_value(raf_core::i18n::t("nodes.palette_popup_empty", language))
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout {
                    padding: UiSpacing::xy(6.0, 8.0),
                    ..UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)
                }),
        );
    }

    root = root.with_child(list);

    let mut surface = UiSurface::new("editor.nodes.palette-popup", palette, root);
    surface.style_sheet = UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-palette-popup".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    text: Some(tokens.text),
                    border_width: Some(1.0),
                    radius: Some(6.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-palette-popup-item".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-palette-popup-item".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
        ],
    };
    Some(surface)
}

/// Window size of the quick-add node palette popup.
pub const NODES_PALETTE_POPUP_WIDTH: f32 = 268.0;
pub const NODES_PALETTE_POPUP_HEIGHT: f32 = 340.0;
