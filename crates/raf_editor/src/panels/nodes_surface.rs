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
        palette,
        graph,
        selected,
        zoom,
        pan,
        viewport,
        host,
        validation,
        language,
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

// ----------------------------------------------------------------------------
// LEFT PANEL: Flows Selector + Categorized Node Library
// ----------------------------------------------------------------------------

fn build_palette(
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
    let mut list = UiNode::scroll_view("nodes.palette.list", UiScrollAxis::Vertical)
        .with_layout(UiLayout {
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

        let cat_header = UiNode::new(
            format!("nodes.cat.btn.{cat_slug}"),
            UiNodeKind::Button,
        )
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
                let item_btn = UiNode::new(
                    format!("nodes.palette.{node_slug}"),
                    UiNodeKind::Button,
                )
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
                    UiNode::new(format!("nodes.palette.{node_slug}.name"), UiNodeKind::Label)
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

// ----------------------------------------------------------------------------
// CENTER PANEL: Graph Canvas + Floating HUD + Wire Rendering
// ----------------------------------------------------------------------------

fn build_graph_panel(
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
        .with_child(
            stat_pill(
                "nodes.graph.node-count",
                format!("{} nodes", graph.nodes.len()),
                tokens.accent,
            ),
        )
        .with_child(
            stat_pill(
                "nodes.graph.wire-count",
                format!("{} wires", graph.connections.len()),
                tokens.text_muted,
            ),
        )
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
                .with_event(UiEventBinding::command(UiEventKind::Click, "nodes.zoom.reset")),
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
                    fill: [tokens.accent_hot[0], tokens.accent_hot[1], tokens.accent_hot[2], 40],
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

    panel = panel.with_child(header);

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
    for child in
        nodes_canvas::build_connections(palette, graph, zoom, pan, canvas_size, host.wire_drag().as_ref())
    {
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
            border: if active {
                tokens.accent
            } else {
                [0, 0, 0, 0]
            },
            text: tokens.text,
            border_width: if active { 1.5 } else { 0.0 },
            radius: 4.0,
            opacity: if active { 1.0 } else { 0.8 },
        })
        .with_icon(
            UiIcon::new(icon)
                .with_size(UiIconSize::Small)
                .with_tint(if active { tokens.accent } else { tokens.text_muted }),
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
            .with_layout(
                UiLayout::absolute(UiRect::new(0.0, 3.0, width, height)).with_z_index(-1),
            )
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
        Some((pending.node_id, pending.pin_id, source_pin.kind, source_pin.data_type))
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
                } else if p_node != node.id && p_kind != pin.kind && raf_nodes::types_compatible(p_type, pin.data_type) {
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
                [tokens.text_muted[0], tokens.text_muted[1], tokens.text_muted[2], 75]
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

fn pin_button(
    palette: StudioUiPalette,
    node: &Node,
    pin: &NodePin,
    is_self_pending: bool,
    is_compatible: bool,
    is_dimmed: bool,
    language: raf_core::Language,
    zoom: f32,
) -> UiNode {
    let tokens = palette.tokens();
    let node_id = node.id.0;
    let pin_id = pin.id;
    let id = format!("nodes.pin.{node_id}.{pin_id}");
    let color = nodes_canvas::pin_color(pin.data_type, tokens.accent);
    let rect = nodes_canvas::pin_rect(node, pin, zoom);
    let label = nodes_catalog::localized_pin_label(node, &pin.name, language);
    let type_key = nodes_catalog::pin_type_key(pin.data_type);
    let type_label = raf_core::i18n::t(type_key, language);

    let tooltip = if is_self_pending {
        format!("{label} ({type_label}) · Active wire source")
    } else if is_compatible {
        format!("Connect to {label} ({type_label})")
    } else if is_dimmed {
        format!("{label} ({type_label}) · Incompatible")
    } else {
        format!("{label} · {type_label}")
    };

    let fill_color = if is_self_pending {
        tokens.accent_hot
    } else if is_compatible {
        [255, 255, 255, 255]
    } else if is_dimmed {
        [color[0], color[1], color[2], 70]
    } else {
        color
    };

    let border_color = if is_self_pending {
        [255, 255, 255, 255]
    } else if is_compatible {
        tokens.accent_hot
    } else {
        tokens.text
    };

    let dot = UiNode::new(id, UiNodeKind::Button)
        .with_class(if is_self_pending {
            "nodes-pin-dot-pending"
        } else if is_compatible {
            "nodes-pin-dot-compatible"
        } else {
            "nodes-pin-dot"
        })
        .with_layout(UiLayout::absolute(rect).with_z_index(nodes_canvas::Z_PIN_DOT))
        .with_style(UiStyle {
            fill: fill_color,
            border: border_color,
            text: tokens.text,
            border_width: if is_self_pending || is_compatible { 3.0 } else { 2.0 },
            radius: rect.width * 0.5,
            opacity: if is_dimmed { 0.35 } else { 1.0 },
        })
        .with_accessibility_role(UiAccessibilityRole::Button)
        .with_accessibility_label_key(
            nodes_catalog::pin_label_key(node, &pin.name)
                .unwrap_or_else(|| "nodes.pin.unknown".to_string()),
        )
        .with_accessibility_selected(is_self_pending)
        .with_tooltip_value(tooltip)
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            format!("nodes.pin.{node_id}:{pin_id}"),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::DragStart,
            format!(
                "nodes.wire.start.{node_id}:{pin_id}:{}",
                if matches!(pin.kind, PinKind::Output) { "out" } else { "in" }
            ),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::DragMove,
            format!("nodes.wire.move.{node_id}:{pin_id}"),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::DragEnd,
            format!("nodes.wire.end.{node_id}:{pin_id}"),
        ));

    dot
}

// ----------------------------------------------------------------------------
// RIGHT PANEL: AAA Structured Inspector
// ----------------------------------------------------------------------------

/// Height of the graph toolbar island above the canvas. The compositor needs
/// it to convert window pointers into canvas coordinates for zoom and pan.
pub const NODES_TOOLBAR_HEIGHT: f32 = 36.0;

/// Quick-add palette shown at the pointer. This is the primary way to author a
/// graph: double click or secondary click on empty canvas, type, pick a node.
/// Each row spawns the node exactly under the cursor, snapped to the grid.
pub fn build_nodes_palette_popup_surface(
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

    let mut list = UiNode::new("nodes.palette-popup.list", UiNodeKind::Panel).with_layout(
        UiLayout {
            flow: UiFlow::Column,
            gap: 2.0,
            ..UiLayout::fixed(0.0, 0.0)
                .with_width_mode(UiSizeMode::Fill)
                .with_height_mode(UiSizeMode::FitContent)
        },
    );

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
                format!("nodes.palette-popup.cat.{}", nodes_catalog::category_slug(category)),
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

/// Window size of the node context menu surface. The compositor clamps the
/// stored pointer position against these dimensions.
pub const NODES_CONTEXT_MENU_WIDTH: f32 = 210.0;
pub const NODES_CONTEXT_MENU_HEIGHT: f32 = 148.0;
/// Window size of the quick-add node palette popup.
pub const NODES_PALETTE_POPUP_WIDTH: f32 = 268.0;
pub const NODES_PALETTE_POPUP_HEIGHT: f32 = 340.0;

/// Builds the node context menu as a standalone window-level surface, mirroring
/// the Electronics and bottom-dock tab menus. Returns `None` when the target
/// node no longer exists (for example, deleted while the menu was open).
pub fn build_nodes_context_menu_surface(
    palette: StudioUiPalette,
    graph: &NodeGraph,
    node_id: NodeId,
    language: raf_core::Language,
) -> Option<UiSurface> {
    let node = graph.node(node_id)?;
    let tokens = palette.tokens();
    let descriptor = raf_nodes::catalog::descriptor_for_node(node);
    let title_key = descriptor
        .map(|descriptor| descriptor.label_key)
        .unwrap_or("nodes.node.unknown");
    let id = node_id.0.to_string();

    let mut root = UiNode::new("nodes.context-menu", UiNodeKind::Menu)
        .with_class("nodes-context-menu")
        .with_material(raf_ui::UiSurfaceMaterial::TranslucentRaised)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 2.0,
            padding: UiSpacing::same(6.0),
            ..UiLayout::fit_content()
        })
        .with_child(
            UiNode::new("nodes.context-menu.title", UiNodeKind::Label)
                .with_text_key(title_key)
                .with_text_style(UiTextStyle {
                    role: UiTextRole::PanelTitle,
                    size_px: 12.0,
                    line_height_px: 16.0,
                    weight: UiFontWeight::Bold,
                    color: tokens.text,
                    inherit_color: false,
                })
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    padding: UiSpacing::xy(6.0, 4.0),
                    ..UiLayout::fixed(190.0, 24.0).with_width_mode(UiSizeMode::Fill)
                }),
        );

    let items = [
        (
            "duplicate",
            raf_core::i18n::t("app.duplicate_menu", language),
            UiIconId::Add,
            format!("nodes.duplicate.{id}"),
            tokens.text,
        ),
        (
            "disconnect-all",
            raf_core::i18n::t("nodes.menu.disconnect_all", language),
            UiIconId::Wire,
            format!("nodes.disconnect_all.{id}"),
            tokens.text,
        ),
        (
            "delete",
            raf_core::i18n::t("nodes.delete", language),
            UiIconId::Close,
            format!("nodes.delete.{id}"),
            tokens.danger,
        ),
    ];

    // Visual separator between constructive and destructive actions.
    root = root.with_child(
        UiNode::new("nodes.context-menu.sep", UiNodeKind::Panel)
            .with_layout(UiLayout::fixed(190.0, 1.0).with_width_mode(UiSizeMode::Fill))
            .with_style(UiStyle {
                fill: tokens.border,
                border: [0, 0, 0, 0],
                text: tokens.text,
                border_width: 0.0,
                radius: 0.0,
                opacity: 1.0,
            }),
    );

    for (item_id, label, icon, command, color) in items {
        root = root.with_child(
            UiNode::new(format!("nodes.context-menu.{item_id}"), UiNodeKind::Button)
                .with_class("nodes-context-item")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 8.0,
                    padding: UiSpacing::xy(8.0, 5.0),
                    ..UiLayout::fixed(190.0, 30.0).with_width_mode(UiSizeMode::Fill)
                })
                .with_icon(
                    UiIcon::new(icon)
                        .with_size(UiIconSize::Small)
                        .with_tint(color),
                )
                .with_child(
                    UiNode::new(
                        format!("nodes.context-menu.{item_id}.label"),
                        UiNodeKind::Label,
                    )
                    .with_text_value(label)
                    .with_text_style(UiTextStyle {
                        role: UiTextRole::Button,
                        size_px: 12.0,
                        line_height_px: 16.0,
                        weight: UiFontWeight::Bold,
                        color,
                        inherit_color: false,
                    }),
                )
                .focusable()
                .with_event(UiEventBinding::command(UiEventKind::Click, command)),
        );
    }

    let mut surface = UiSurface::new("editor.nodes.context-menu", palette, root);
    surface.style_sheet = UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-context-menu".to_string()),
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
                UiStyleSelector::Class("nodes-context-item".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some([0, 0, 0, 0]),
                    text: Some(tokens.text),
                    border_width: Some(0.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-context-item".to_string()),
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

fn build_empty_inspector(palette: StudioUiPalette) -> UiNode {
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

fn build_inspector(
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
                        UiNode::new("nodes.inspector.header_spacer", UiNodeKind::Panel).with_layout(
                            UiLayout {
                                grow: 1.0,
                                ..UiLayout::fit_content()
                            },
                        ),
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
                palette,
                node,
                pin,
                index,
                is_linked,
                language,
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
                color: if is_linked { tokens.accent } else { tokens.text_muted },
                inherit_color: false,
            }),
        )
}

fn icon_button(
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

fn stat_label(
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
                    fill: Some([tokens.surface_raised[0], tokens.surface_raised[1], tokens.surface_raised[2], 180]),
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
                    fill: Some([tokens.accent_hot[0], tokens.accent_hot[1], tokens.accent_hot[2], 200]),
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
                    fill: Some([tokens.surface_raised[0], tokens.surface_raised[1], tokens.surface_raised[2], 255]),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find_node<'a>(node: &'a UiNode, id: &str) -> Option<&'a UiNode> {
        if node.id == id {
            return Some(node);
        }
        node.children.iter().find_map(|child| find_node(child, id))
    }

    fn collect_commands(node: &UiNode, out: &mut Vec<String>) {
        for binding in &node.event_handlers {
            if let raf_ui::UiAction::Command { name } = &binding.action {
                out.push(name.clone());
            }
        }
        for child in &node.children {
            collect_commands(child, out);
        }
    }

    /// True when a node answers both the double click and the secondary click
    /// with the quick-add palette, the way every engine node canvas does.
    fn opens_palette(node: &UiNode) -> bool {
        let double = node.event_handlers.iter().any(|binding| {
            binding.event == UiEventKind::DoubleClick
                && matches!(&binding.action, raf_ui::UiAction::Command { name } if name == "nodes.palette.open")
        });
        let secondary = node.event_handlers.iter().any(|binding| {
            binding.event == UiEventKind::ContextMenu
                && matches!(&binding.action, raf_ui::UiAction::OpenMenu { id } if id == "nodes.palette.open")
        });
        double && secondary
    }

    fn sample_graph() -> (NodeGraph, NodeId) {
        let mut graph = NodeGraph::new("Main Event Graph");
        let id = graph.add_node(Node::on_start());
        (graph, id)
    }

    fn build_sample_surface(graph: &NodeGraph, selected: Option<NodeId>) -> UiSurface {
        build_sample_view(graph, selected, [0.0, 0.0], [1200.0, 700.0]).0
    }

    fn build_sample_view(
        graph: &NodeGraph,
        selected: Option<NodeId>,
        pan: [f32; 2],
        viewport: [f32; 2],
    ) -> (UiSurface, NodesSurfaceHost) {
        let mut host = NodesSurfaceHost::default();
        host.open_palette_popup([400.0, 300.0], [240.0, 96.0]);
        let surface = build_nodes_surface_with_host(
            StudioUiPalette::IndustrialDark,
            graph,
            selected,
            1.0,
            pan,
            viewport,
            &host,
            false,
            false,
            None,
            raf_core::Language::English,
        );
        (surface, host)
    }

    fn card_rect(surface: &UiSurface, id: &str) -> UiRect {
        find_node(&surface.root, "nodes.canvas")
            .and_then(|canvas| {
                canvas
                    .children
                    .iter()
                    .find(|child| child.id == format!("nodes.card.{id}"))
            })
            .and_then(|card| card.layout.rect)
            .expect("card must render inside the canvas with an absolute rect")
    }

    #[test]
    fn canvas_is_viewport_sized_and_pans_the_graph() {
        let (graph, node_id) = sample_graph();
        let id = node_id.0.to_string();

        let (surface, _) = build_sample_view(&graph, None, [0.0, 0.0], [1200.0, 700.0]);
        let base = card_rect(&surface, &id);
        let (panned, _) = build_sample_view(&graph, None, [200.0, 100.0], [1200.0, 700.0]);
        let moved = card_rect(&panned, &id);

        // Panning moves the card without resizing it: the canvas never grows.
        assert_eq!(moved.width, base.width, "pan must not resize a card");
        assert_eq!(moved.height, base.height, "pan must not resize a card");
        assert_eq!(base.x - moved.x, 200.0);
        assert_eq!(base.y - moved.y, 100.0);

        let canvas = find_node(&surface.root, "nodes.canvas").expect("canvas must exist");
        let canvas_rect = canvas.layout.rect.expect("canvas is absolutely placed");
        assert_eq!(canvas_rect.width, 1200.0, "canvas fills the viewport");
        assert_eq!(
            canvas_rect.height,
            700.0 - NODES_TOOLBAR_HEIGHT,
            "canvas sits below the toolbar island"
        );
    }

    #[test]
    fn cards_keep_a_body_below_the_header() {
        let mut graph = NodeGraph::new("Main Event Graph");
        graph.add_node(Node::on_start());
        // A one-pin card must be clearly taller than its own header, otherwise
        // the body has nowhere to live and every node reads as a colored bar.
        let size = nodes_canvas::node_size(&graph.nodes[0], 1.0);
        assert!(
            size[1] >= nodes_canvas::NODE_HEADER_HEIGHT + nodes_canvas::NODE_PIN_ROW_HEIGHT,
            "card height {} leaves no body under a {} header",
            size[1],
            nodes_canvas::NODE_HEADER_HEIGHT
        );
    }

    #[test]
    fn quick_add_palette_spawns_nodes_at_the_cursor() {
        let (graph, _) = sample_graph();
        let (_, host) = build_sample_view(&graph, None, [0.0, 0.0], [1200.0, 700.0]);
        let popup = host.palette_popup().expect("popup is open");
        assert_eq!(popup.spawn, [240.0, 96.0]);

        let surface = build_nodes_palette_popup_surface(
            StudioUiPalette::IndustrialDark,
            &host,
            raf_core::Language::English,
        )
        .expect("open palette must render its own surface");
        let mut commands = Vec::new();
        collect_commands(&surface.root, &mut commands);
        assert!(
            commands
                .iter()
                .any(|command| command.starts_with("nodes.add.at:")
                    && command.ends_with(":240.0:96.0")),
            "palette rows must spawn at the pointer: {commands:?}"
        );
    }

    #[test]
    fn quick_add_palette_filters_out_unrelated_nodes() {
        let (_graph, _) = sample_graph();
        let mut host = NodesSurfaceHost::default();
        host.open_palette_popup([10.0, 10.0], [0.0, 0.0]);
        let all = build_nodes_palette_popup_surface(
            StudioUiPalette::IndustrialDark,
            &host,
            raf_core::Language::English,
        )
        .expect("palette surface");
        let mut all_commands = Vec::new();
        collect_commands(&all.root, &mut all_commands);

        host.set_palette_query("print");
        let filtered = build_nodes_palette_popup_surface(
            StudioUiPalette::IndustrialDark,
            &host,
            raf_core::Language::English,
        )
        .expect("filtered palette");
        let mut filtered_commands = Vec::new();
        collect_commands(&filtered.root, &mut filtered_commands);

        assert!(
            filtered_commands.len() < all_commands.len(),
            "a search must narrow the palette: {} vs {}",
            filtered_commands.len(),
            all_commands.len()
        );
    }

    #[test]
    fn empty_canvas_offers_double_click_and_secondary_click_palette() {
        // A populated graph answers on the canvas itself.
        let (graph, _) = sample_graph();
        let (surface, _) = build_sample_view(&graph, None, [0.0, 0.0], [1200.0, 700.0]);
        let canvas = find_node(&surface.root, "nodes.canvas").expect("canvas must exist");
        assert!(
            opens_palette(canvas),
            "canvas must open the quick-add palette"
        );

        // An empty graph has no canvas, so the empty state owns the gesture.
        let empty = NodeGraph::new("Main Event Graph");
        let (surface, _) = build_sample_view(&empty, None, [0.0, 0.0], [1200.0, 700.0]);
        let empty_state = find_node(&surface.root, "nodes.graph.empty").expect("empty state");
        assert!(
            opens_palette(empty_state),
            "empty state must open the quick-add palette"
        );
    }

    #[test]
    fn node_card_opens_its_own_context_menu_command() {
        let (graph, node_id) = sample_graph();
        let id = node_id.0.to_string();
        let surface = build_sample_surface(&graph, Some(node_id));
        let card = find_node(&surface.root, &format!("nodes.card.{id}"))
            .expect("selected graph must render its card");
        assert!(card.event_handlers.iter().any(|binding| {
            binding.event == UiEventKind::ContextMenu
                && matches!(
                    &binding.action,
                    raf_ui::UiAction::OpenMenu { id: menu_id }
                        if *menu_id == format!("nodes.context.{id}")
                )
        }));
    }

    #[test]
    fn empty_state_offers_the_on_start_shortcut() {
        let graph = NodeGraph::new("Main Event Graph");
        let surface = build_sample_surface(&graph, None);
        let cta = find_node(&surface.root, "nodes.graph.empty.cta")
            .expect("empty graph must render its call to action");
        assert!(cta.event_handlers.iter().any(|binding| {
            binding.event == UiEventKind::Click
                && matches!(
                    &binding.action,
                    raf_ui::UiAction::Command { name } if name == "nodes.add.on-start"
                )
        }));
    }

    #[test]
    fn context_menu_surface_lists_node_actions_and_handles_missing_nodes() {
        let (graph, node_id) = sample_graph();
        let id = node_id.0.to_string();
        let surface = build_nodes_context_menu_surface(
            StudioUiPalette::IndustrialDark,
            &graph,
            node_id,
            raf_core::Language::English,
        )
        .expect("existing node must produce a menu");

        let mut commands = Vec::new();
        collect_commands(&surface.root, &mut commands);
        assert!(commands.contains(&format!("nodes.duplicate.{id}")));
        assert!(commands.contains(&format!("nodes.disconnect_all.{id}")));
        assert!(commands.contains(&format!("nodes.delete.{id}")));

        let missing = NodeId::new();
        assert!(
            build_nodes_context_menu_surface(
                StudioUiPalette::IndustrialDark,
                &graph,
                missing,
                raf_core::Language::English,
            )
            .is_none(),
            "a deleted node must not render a stale menu"
        );
    }
}
