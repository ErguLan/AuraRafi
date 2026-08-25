//! Retained RafUI surface for Game visual scripting.
//!
//! This is the native replacement for the old node panel. The surface
//! only presents a `raf_nodes::NodeGraph` and emits commands; graph mutation
//! remains in the native editor command boundary.

use raf_nodes::{NodeCategory, NodeGraph, NodeId};
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiIcon, UiIconId, UiIconSize,
    UiJustify, UiLayout, UiNode, UiNodeKind, UiOverflow, UiScrollAxis, UiSizeMode, UiSpacing,
    UiStyle, UiSurface, UiTextStyle,
};
use raf_ui::{UiRect, UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet};

const NODE_PRESETS: &[(&str, &str, UiIconId)] = &[
    ("on-start", "On Start", UiIconId::Play),
    ("on-update", "On Update", UiIconId::Refresh),
    ("print", "Print", UiIconId::Console),
    ("if", "If", UiIconId::Node),
    ("add", "Add", UiIconId::Add),
    ("for-loop", "For Loop", UiIconId::Refresh),
    ("while-loop", "While Loop", UiIconId::Refresh),
    ("greater-than", "Greater Than", UiIconId::Node),
    ("less-than", "Less Than", UiIconId::Node),
    ("equals", "Equals", UiIconId::Node),
    ("spawn-entity", "Spawn Entity", UiIconId::Cube),
    ("destroy-entity", "Destroy Entity", UiIconId::Close),
    ("set-position", "Set Position", UiIconId::Move),
    ("key-press", "Key Press", UiIconId::Node),
    ("mouse-click", "Mouse Click", UiIconId::Entity),
    ("delay", "Delay", UiIconId::Refresh),
];

/// Builds the retained node editor from the actual graph document.
pub fn build_nodes_surface(
    palette: StudioUiPalette,
    graph: &NodeGraph,
    selected: Option<NodeId>,
) -> UiSurface {
    build_nodes_surface_with_zoom(palette, graph, selected, 1.0)
}

/// Native workbench variant with an explicit canvas zoom. The zoom belongs to
/// the workbench state, not to the graph document, so changing it never marks
/// a project dirty.
pub fn build_nodes_surface_with_zoom(
    palette: StudioUiPalette,
    graph: &NodeGraph,
    selected: Option<NodeId>,
    zoom: f32,
) -> UiSurface {
    let tokens = palette.tokens();
    let mut palette_panel = UiNode::new("nodes.palette", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 5.0,
            padding: UiSpacing::same(10.0),
            ..UiLayout::fixed(190.0, 0.0).with_height_mode(UiSizeMode::Fill)
        })
        .with_style(panel_style(tokens.surface, tokens.border, tokens.text));
    palette_panel = palette_panel.with_child(
        UiNode::new("nodes.palette.title", UiNodeKind::Label)
            .with_text_value("Node palette")
            .with_text_style(UiTextStyle::panel_title(tokens.text))
            .with_layout(UiLayout::fixed(0.0, 26.0).with_width_mode(UiSizeMode::Fill)),
    );
    palette_panel = palette_panel.with_child(
        UiNode::new("nodes.palette.hint", UiNodeKind::Label)
            .with_text_value("Add a node to the active graph")
            .with_text_style(UiTextStyle::body(tokens.text_muted))
            .with_layout(UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)),
    );
    let mut palette_list = UiNode::scroll_view("nodes.palette.list", UiScrollAxis::Vertical)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            grow: 1.0,
            overflow: UiOverflow::ScrollY,
            ..UiLayout::default()
        });
    let mut previous_group = "";
    for (slug, label, icon) in NODE_PRESETS {
        let group = node_preset_group(slug);
        if group != previous_group {
            palette_list = palette_list.with_child(
                UiNode::new(format!("nodes.palette.group.{group}"), UiNodeKind::Label)
                    .with_text_value(group)
                    .with_text_style(UiTextStyle::body(tokens.text_muted))
                    .with_layout(UiLayout::fixed(0.0, 20.0).with_width_mode(UiSizeMode::Fill)),
            );
            previous_group = group;
        }
        palette_list = palette_list.with_child(palette_button(
            format!("nodes.palette.{slug}"),
            label,
            *icon,
            format!("nodes.add.{slug}"),
        ));
    }
    palette_panel = palette_panel.with_child(palette_list);
    palette_panel = palette_panel.with_child(
        UiNode::new("nodes.palette.spacer", UiNodeKind::Panel).with_layout(UiLayout {
            grow: 1.0,
            ..UiLayout::fit_content()
        }),
    );
    palette_panel = palette_panel.with_child(palette_button(
        "nodes.palette.new-graph",
        "New graph",
        UiIconId::Add,
        "nodes.new-graph",
    ));
    palette_panel = palette_panel.with_child(palette_button(
        "nodes.palette.compile",
        "Validate graph",
        UiIconId::Success,
        "nodes.compile",
    ));

    let mut graph_list = UiNode::new("nodes.graph.panel", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 8.0,
            padding: UiSpacing::same(10.0),
            grow: 1.0,
            ..UiLayout::default()
        })
        .with_style(panel_style(tokens.background, tokens.border, tokens.text));
    graph_list = graph_list.with_child(
        UiNode::new("nodes.graph.header", UiNodeKind::Toolbar)
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 8.0,
                padding: UiSpacing::xy(4.0, 0.0),
                ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
            })
            .with_child(
                UiNode::new("nodes.graph.name", UiNodeKind::Label)
                    .with_text_value(graph.name.clone())
                    .with_text_style(UiTextStyle::panel_title(tokens.text)),
            )
            .with_child(
                UiNode::new("nodes.graph.count", UiNodeKind::Label)
                    .with_text_value(format!(
                        "{} nodes · {} links",
                        graph.nodes.len(),
                        graph.connections.len()
                    ))
                    .with_text_style(UiTextStyle::body(tokens.text_muted))
                    .with_layout(UiLayout {
                        grow: 1.0,
                        ..UiLayout::fit_content()
                    }),
            )
            .with_child(nodes_icon_button(
                "nodes.zoom.out",
                UiIconId::ChevronLeft,
                "Zoom out",
            ))
            .with_child(nodes_icon_button(
                "nodes.zoom.reset",
                UiIconId::Focus,
                "Reset zoom",
            ))
            .with_child(nodes_icon_button(
                "nodes.zoom.in",
                UiIconId::ChevronRight,
                "Zoom in",
            ))
            .with_child(
                UiNode::new("nodes.graph.compile", UiNodeKind::Button)
                    .with_text_value("Validate".to_string())
                    .with_text_style(UiTextStyle::button(tokens.accent_hot))
                    .focusable()
                    .with_event(UiEventBinding::command(UiEventKind::Click, "nodes.compile")),
            ),
    );
    if graph.nodes.is_empty() {
        graph_list = graph_list.with_child(
            UiNode::new("nodes.graph.empty", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    align_items: UiAlign::Center,
                    justify_content: UiJustify::Center,
                    gap: 6.0,
                    grow: 1.0,
                    ..UiLayout::default()
                })
                .with_child(
                    UiNode::new("nodes.graph.empty.title", UiNodeKind::Label)
                        .with_text_value("This graph is empty")
                        .with_text_style(UiTextStyle::panel_title(tokens.text)),
                )
                .with_child(
                    UiNode::new("nodes.graph.empty.hint", UiNodeKind::Label)
                        .with_text_value("Choose a node from the palette to begin")
                        .with_text_style(UiTextStyle::body(tokens.text_muted)),
                ),
        );
    } else {
        let canvas_size = graph_canvas_size(graph, zoom);
        let mut canvas = UiNode::new("nodes.canvas", UiNodeKind::Panel)
            .with_layout(UiLayout::absolute(UiRect::new(
                0.0,
                0.0,
                canvas_size[0],
                canvas_size[1],
            )))
            .with_style(UiStyle {
                fill: [15, 19, 26, 255],
                border: tokens.border,
                text: tokens.text,
                border_width: 1.0,
                radius: 2.0,
                opacity: 1.0,
            });
        for (index, node) in graph.nodes.iter().enumerate() {
            let position = node_canvas_position(node.position, index, zoom);
            canvas = canvas.with_child(
                node_card(palette, node, selected == Some(node.id)).with_layout(
                    UiLayout::absolute(UiRect::new(
                        position[0],
                        position[1],
                        204.0 * zoom,
                        96.0 * zoom,
                    )),
                ),
            );
        }
        graph_list = graph_list.with_child(
            UiNode::new("nodes.canvas.viewport", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    grow: 1.0,
                    overflow: UiOverflow::ScrollBoth,
                    ..UiLayout::default()
                })
                .with_child(canvas),
        );
    }

    let inspector = selected
        .and_then(|id| graph.nodes.iter().find(|node| node.id == id))
        .map(|node| node_inspector(palette, node))
        .unwrap_or_else(|| {
            UiNode::new("nodes.inspector.empty", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    gap: 6.0,
                    padding: UiSpacing::same(10.0),
                    ..UiLayout::fill(UiFlow::Column)
                })
                .with_style(panel_style(tokens.surface, tokens.border, tokens.text))
                .with_child(
                    UiNode::new("nodes.inspector.empty.label", UiNodeKind::Label)
                        .with_text_value("Select a node")
                        .with_text_style(UiTextStyle::panel_title(tokens.text)),
                )
                .with_child(
                    UiNode::new("nodes.inspector.empty.hint", UiNodeKind::Label)
                        .with_text_value("Node properties will appear here")
                        .with_text_style(UiTextStyle::body(tokens.text_muted)),
                )
        });

    let root = UiNode::new("nodes.root", UiNodeKind::Panel)
        .with_class("nodes-surface")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 1.0,
            ..UiLayout::fill(UiFlow::Row)
        })
        .with_style(panel_style(tokens.background, tokens.border, tokens.text))
        .with_child(palette_panel)
        .with_child(graph_list)
        .with_child(
            UiNode::new("nodes.inspector.slot", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    ..UiLayout::fixed(235.0, 0.0).with_height_mode(UiSizeMode::Fill)
                })
                .with_child(inspector),
        );

    let mut surface = UiSurface::new("editor.nodes", palette, root);
    surface.style_sheet = nodes_style_sheet(palette);
    surface
}

fn node_preset_group(slug: &str) -> &'static str {
    match slug {
        "on-start" | "on-update" | "key-press" | "mouse-click" => "Events",
        "if" | "for-loop" | "while-loop" | "delay" => "Logic",
        "print" | "spawn-entity" | "destroy-entity" | "set-position" => "Actions",
        "add" | "greater-than" | "less-than" | "equals" => "Math",
        _ => "Game",
    }
}

fn graph_canvas_size(graph: &NodeGraph, zoom: f32) -> [f32; 2] {
    let mut width: f32 = 720.0;
    let mut height: f32 = 420.0;
    for node in &graph.nodes {
        width = width.max(node.position[0] + 280.0);
        height = height.max(node.position[1] + 170.0);
    }
    [width * zoom.max(0.55), height * zoom.max(0.55)]
}

fn node_canvas_position(position: [f32; 2], index: usize, zoom: f32) -> [f32; 2] {
    let fallback = [
        28.0 + (index % 3) as f32 * 232.0,
        48.0 + (index / 3) as f32 * 128.0,
    ];
    let position = if position == [0.0, 0.0] {
        fallback
    } else {
        position
    };
    [position[0] * zoom.max(0.55), position[1] * zoom.max(0.55)]
}

fn palette_button(
    id: impl Into<String>,
    label: &str,
    icon: UiIconId,
    command: impl Into<String>,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("nodes-palette-button")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 7.0,
            padding: UiSpacing::xy(8.0, 0.0),
            ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Small))
        .with_text_value(label)
        .with_text_style(UiTextStyle::button([237, 239, 242, 255]))
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn nodes_icon_button(id: &str, icon: UiIconId, tooltip: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Small))
        .with_layout(UiLayout::fixed(26.0, 26.0))
        .with_accessibility_label_key(tooltip)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, id))
}

fn node_card(palette: StudioUiPalette, node: &raf_nodes::Node, selected: bool) -> UiNode {
    let tokens = palette.tokens();
    let category_color = category_color(node.category);
    let fill = if selected {
        [44, 52, 65, 255]
    } else {
        [25, 30, 39, 255]
    };
    let id = node.id.0.to_string();
    let mut card = UiNode::new(format!("nodes.card.{id}"), UiNodeKind::Panel)
        .with_class("nodes-card")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 5.0,
            padding: UiSpacing::same(8.0),
            ..UiLayout::fixed(0.0, 82.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_style(UiStyle {
            fill,
            border: category_color,
            text: tokens.text,
            border_width: if selected { 2.0 } else { 1.0 },
            radius: 4.0,
            opacity: 1.0,
        })
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            format!("nodes.select.{id}"),
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
        ));
    card = card.with_child(
        UiNode::new(format!("nodes.card.{id}.header"), UiNodeKind::Toolbar)
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 6.0,
                ..UiLayout::fixed(0.0, 22.0).with_width_mode(UiSizeMode::Fill)
            })
            .with_child(
                UiNode::new(format!("nodes.card.{id}.category"), UiNodeKind::Label)
                    .with_text_value(node.category.display_name())
                    .with_text_style(UiTextStyle::button(category_color)),
            )
            .with_child(
                UiNode::new(format!("nodes.card.{id}.name"), UiNodeKind::Label)
                    .with_text_value(node.name.clone())
                    .with_text_style(UiTextStyle::button(tokens.text)),
            )
            .with_child(
                UiNode::new(format!("nodes.card.{id}.delete"), UiNodeKind::Button)
                    .with_layout(UiLayout {
                        justify_content: UiJustify::End,
                        grow: 1.0,
                        ..UiLayout::fit_content()
                    })
                    .with_icon(UiIcon::new(UiIconId::Close).with_size(UiIconSize::Small))
                    .with_event(UiEventBinding::command(
                        UiEventKind::Click,
                        format!("nodes.delete.{id}"),
                    )),
            ),
    );
    card.with_child(
        UiNode::new(format!("nodes.card.{id}.pins"), UiNodeKind::Label)
            .with_text_value(format!(
                "{} pins · position {:.0}, {:.0}",
                node.pins.len(),
                node.position[0],
                node.position[1]
            ))
            .with_text_style(UiTextStyle::body(tokens.text_muted)),
    )
}

fn nodes_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-palette-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-palette-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-palette-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent_hot),
                    text: Some([255, 255, 255, 255]),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Active),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-card".to_string()),
                UiStylePatch {
                    fill: Some([36, 43, 55, 255]),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-card".to_string()),
                UiStylePatch {
                    fill: Some([82, 54, 24, 255]),
                    border: Some(tokens.accent_hot),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Active),
        ],
    }
}

fn node_inspector(palette: StudioUiPalette, node: &raf_nodes::Node) -> UiNode {
    let tokens = palette.tokens();
    let id = node.id.0.to_string();
    let mut root = UiNode::new("nodes.inspector", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 7.0,
            padding: UiSpacing::same(10.0),
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(panel_style(tokens.surface, tokens.border, tokens.text))
        .with_child(
            UiNode::new("nodes.inspector.title", UiNodeKind::Label)
                .with_text_value("Node properties")
                .with_text_style(UiTextStyle::panel_title(tokens.text)),
        )
        .with_child(label_value(
            "Name",
            &node.name,
            tokens.text,
            tokens.text_muted,
        ))
        .with_child(label_value(
            "Category",
            node.category.display_name(),
            tokens.text,
            tokens.text_muted,
        ))
        .with_child(label_value(
            "Description",
            &node.description,
            tokens.text,
            tokens.text_muted,
        ));
    root = root.with_child(
        UiNode::new("nodes.inspector.pins-title", UiNodeKind::Label)
            .with_text_value("Pins")
            .with_text_style(UiTextStyle::panel_title(tokens.text)),
    );
    for (index, pin) in node.pins.iter().enumerate() {
        root = root.with_child(
            UiNode::new(format!("nodes.inspector.pin.{index}"), UiNodeKind::Label)
                .with_text_value(format!(
                    "{} · {} · {:?}",
                    pin.name,
                    if matches!(pin.kind, raf_nodes::PinKind::Input) {
                        "Input"
                    } else {
                        "Output"
                    },
                    pin.data_type
                ))
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        );
    }
    root.with_child(
        UiNode::new("nodes.inspector.id", UiNodeKind::Label)
            .with_text_value(format!("ID {id}"))
            .with_text_style(UiTextStyle::body(tokens.text_muted)),
    )
}

fn label_value(label: &str, value: &str, text: [u8; 4], muted: [u8; 4]) -> UiNode {
    UiNode::new(format!("nodes.value.{label}"), UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 2.0,
            ..UiLayout::fit_content()
        })
        .with_child(
            UiNode::new(format!("nodes.value.{label}.label"), UiNodeKind::Label)
                .with_text_value(label)
                .with_text_style(UiTextStyle::body(muted)),
        )
        .with_child(
            UiNode::new(format!("nodes.value.{label}.value"), UiNodeKind::Label)
                .with_text_value(value)
                .with_text_style(UiTextStyle::body(text)),
        )
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
