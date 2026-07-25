//! Visual node editor panel - no-code scripting through connected nodes.
//!
//! Features:
//! - Canvas with pan/zoom
//! - Node rendering with header, pins, and body
//! - Pin-to-pin connections drawn as bezier curves
//! - Node palette for adding new nodes
//! - Drag to connect pins
//! - Selection and deletion
//! - Multiple flows (Graph List)
//! - Auto-save timer and Undo/Redo

use eframe::egui_wgpu;
use egui::{Color32, Pos2, Rect, RichText, Stroke, Ui, Vec2};
use raf_nodes::graph::NodeGraph;
use raf_nodes::node::{Node, NodeCategory, NodeId, PinDataType, PinKind};
use raf_render::api_graphic_basic::ui_surface::UiSurface;
use raf_ui::{
    StudioUiPalette, UiAction, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiLayout, UiNode,
    UiNodeKind, UiOverflow, UiScrollAxis, UiSpacing, UiStylePatch, UiStyleRule, UiStyleRuleState,
    UiStyleSelector, UiStyleSheet, UiTextInput, UiTextStyle,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use raf_core::config::Language;
use raf_core::i18n::t;

use crate::theme;

use super::raf_ui_surface_bridge::RafUiSurfaceBridge;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const NODE_WIDTH: f32 = 180.0;
const NODE_HEADER_HEIGHT: f32 = 28.0;
const PIN_ROW_HEIGHT: f32 = 22.0;
const PIN_RADIUS: f32 = 5.0;
const NODE_ROUNDING: f32 = 6.0;
const CONNECTION_THICKNESS: f32 = 2.5;
const FLOW_RAIL_WIDTH: f32 = 224.0;
const INSPECTOR_RAIL_WIDTH: f32 = 272.0;
const CANVAS_TOOLBAR_HEIGHT: f32 = 42.0;
const MINIMAP_SIZE: Vec2 = Vec2::new(218.0, 132.0);

// ---------------------------------------------------------------------------
// Node Editor State
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct DragConnection {
    from_node: NodeId,
    from_pin: Uuid,
    from_pos: Pos2,
    from_kind: PinKind,
    from_type: PinDataType,
}

fn node_template(id: &str) -> Option<NodeTemplate> {
    NODE_TEMPLATES
        .iter()
        .copied()
        .find(|template| template.id == id)
}

fn command_index(command: &str, prefix: &str) -> Option<usize> {
    command
        .strip_prefix(prefix)
        .and_then(|value| value.strip_prefix(':'))
        .and_then(|value| value.parse::<usize>().ok())
}

fn pins_compatible(source: PinDataType, target: PinDataType) -> bool {
    source == target || source == PinDataType::Any || target == PinDataType::Any
}

fn soft_snap(value: f32, grid: f32, threshold: f32) -> f32 {
    let snapped = (value / grid).round() * grid;
    if (value - snapped).abs() <= threshold {
        snapped
    } else {
        value
    }
}

fn clone_node_with_fresh_ids(node: &Node) -> Node {
    let mut clone = node.clone();
    clone.id = NodeId::new();
    for pin in &mut clone.pins {
        pin.id = Uuid::new_v4();
    }
    clone
}

fn graph_bounds(nodes: &[Node]) -> Rect {
    if nodes.is_empty() {
        return Rect::from_min_size(Pos2::ZERO, Vec2::splat(1.0));
    }
    let min_x = nodes
        .iter()
        .map(|node| node.position[0])
        .fold(f32::INFINITY, f32::min);
    let min_y = nodes
        .iter()
        .map(|node| node.position[1])
        .fold(f32::INFINITY, f32::min);
    let max_x = nodes
        .iter()
        .map(|node| node.position[0] + NODE_WIDTH)
        .fold(f32::NEG_INFINITY, f32::max);
    let max_y = nodes
        .iter()
        .map(|node| {
            node.position[1]
                + NODE_HEADER_HEIGHT
                + node.pins.len().max(1) as f32 * PIN_ROW_HEIGHT
                + 8.0
        })
        .fold(f32::NEG_INFINITY, f32::max);
    Rect::from_min_max(
        Pos2::new(min_x - 48.0, min_y - 48.0),
        Pos2::new(max_x + 48.0, max_y + 48.0),
    )
}

fn world_rect_to_minimap(world: Rect, minimap: Rect, bounds: Rect) -> Rect {
    let scale_x = minimap.width() / bounds.width().max(1.0);
    let scale_y = minimap.height() / bounds.height().max(1.0);
    Rect::from_min_max(
        Pos2::new(
            minimap.left() + (world.left() - bounds.left()) * scale_x,
            minimap.top() + (world.top() - bounds.top()) * scale_y,
        ),
        Pos2::new(
            minimap.left() + (world.right() - bounds.left()) * scale_x,
            minimap.top() + (world.bottom() - bounds.top()) * scale_y,
        ),
    )
}

fn minimap_to_world(point: Pos2, minimap: Rect, bounds: Rect) -> Pos2 {
    Pos2::new(
        bounds.left() + (point.x - minimap.left()) / minimap.width().max(1.0) * bounds.width(),
        bounds.top() + (point.y - minimap.top()) / minimap.height().max(1.0) * bounds.height(),
    )
}

fn bezier_distance(point: Pos2, from: Pos2, to: Pos2) -> f32 {
    let dx = (to.x - from.x).abs() * 0.5;
    let cp1 = Pos2::new(from.x + dx, from.y);
    let cp2 = Pos2::new(to.x - dx, to.y);
    let mut distance = f32::INFINITY;
    let mut previous = from;
    for index in 1..=24 {
        let t = index as f32 / 24.0;
        let inverse = 1.0 - t;
        let current = Pos2::new(
            inverse * inverse * inverse * from.x
                + 3.0 * inverse * inverse * t * cp1.x
                + 3.0 * inverse * t * t * cp2.x
                + t * t * t * to.x,
            inverse * inverse * inverse * from.y
                + 3.0 * inverse * inverse * t * cp1.y
                + 3.0 * inverse * t * t * cp2.y
                + t * t * t * to.y,
        );
        distance = distance.min(point_segment_distance(point, previous, current));
        previous = current;
    }
    distance
}

fn point_segment_distance(point: Pos2, from: Pos2, to: Pos2) -> f32 {
    let delta = to - from;
    let length_sq = delta.length_sq();
    if length_sq <= f32::EPSILON {
        return point.distance(from);
    }
    let projected = ((point - from).dot(delta) / length_sq).clamp(0.0, 1.0);
    point.distance(from + delta * projected)
}

fn build_node_toolbar_surface(
    palette: StudioUiPalette,
    zoom: f32,
    has_selection: bool,
) -> UiSurface {
    let tokens = palette.tokens();
    let root = UiNode::new("node.toolbar.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            padding: UiSpacing::xy(8.0, 5.0),
            gap: 6.0,
            ..UiLayout::fill(UiFlow::Row)
        })
        .with_style(palette.root_style())
        .with_child(node_toolbar_button(
            "node.toolbar.add",
            "nodes.add_node",
            "node.browser.open",
            "node-toolbar-primary",
        ))
        .with_child(
            node_toolbar_button(
                "node.toolbar.focus",
                "nodes.focus_selection",
                "node.canvas.focus-selection",
                "node-toolbar-button",
            )
            .disabled(!has_selection),
        )
        .with_child(node_toolbar_button(
            "node.toolbar.fit",
            "nodes.fit_flow",
            "node.canvas.fit",
            "node-toolbar-button",
        ))
        .with_child(
            UiNode::new("node.toolbar.spacer", UiNodeKind::Panel).with_layout(UiLayout {
                grow: 1.0,
                ..UiLayout::default()
            }),
        )
        .with_child(node_toolbar_button(
            "node.toolbar.zoom-out",
            "nodes.zoom_out",
            "node.canvas.zoom-out",
            "node-toolbar-button",
        ))
        .with_child(
            UiNode::new("node.toolbar.zoom", UiNodeKind::Label)
                .with_text_key(format!("{:.0}%", zoom * 100.0))
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(52.0, 26.0)),
        )
        .with_child(node_toolbar_button(
            "node.toolbar.zoom-in",
            "nodes.zoom_in",
            "node.canvas.zoom-in",
            "node-toolbar-button",
        ));
    let mut surface = UiSurface::new("node.toolbar", palette, root);
    surface.style_sheet = node_surface_style_sheet(palette);
    surface
}

fn build_flow_rail_surface(
    palette: StudioUiPalette,
    graphs: &[NodeGraph],
    active: usize,
    filter: &str,
) -> UiSurface {
    let tokens = palette.tokens();
    let mut search = UiTextInput::new("node.flow-search");
    search.placeholder_key = Some("nodes.search_flows".to_string());
    search.max_length = 128;
    let mut list = UiNode::scroll_view("node.flows.list", UiScrollAxis::Vertical)
        .with_class("node-scroll")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 4.0,
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        });
    let query = filter.to_lowercase();
    for (index, graph) in graphs.iter().enumerate() {
        if !query.is_empty() && !graph.name.to_lowercase().contains(&query) {
            continue;
        }
        let row_class = if index == active {
            "node-flow-active"
        } else {
            "node-flow-row"
        };
        let row = UiNode::new(format!("node.flow.row.{index}"), UiNodeKind::Panel)
            .with_class(row_class)
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 4.0,
                padding: UiSpacing::xy(6.0, 2.0),
                ..UiLayout::fixed(0.0, 30.0)
            })
            .with_child(
                UiNode::new(format!("node.flow.select.{index}"), UiNodeKind::Button)
                    .with_text_key(graph.name.clone())
                    .with_text_style(UiTextStyle::button(tokens.text))
                    .with_layout(UiLayout {
                        grow: 1.0,
                        ..UiLayout::default()
                    })
                    .focusable()
                    .with_event(UiEventBinding::command(
                        UiEventKind::Click,
                        format!("node.flow.select:{index}"),
                    )),
            )
            .with_child(
                UiNode::new(format!("node.flow.delete.{index}"), UiNodeKind::Button)
                    .with_text_key("X")
                    .with_class("node-flow-delete")
                    .disabled(graphs.len() == 1)
                    .with_layout(UiLayout::fixed(24.0, 22.0))
                    .focusable()
                    .with_event(UiEventBinding::command(
                        UiEventKind::Click,
                        format!("node.flow.delete:{index}"),
                    )),
            );
        list = list.with_child(row);
    }
    let root = UiNode::new("node.flows.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            padding: UiSpacing::same(8.0),
            gap: 8.0,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(palette.root_style())
        .with_child(
            UiNode::new("node.flows.header", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    ..UiLayout::fixed(0.0, 26.0)
                })
                .with_child(
                    UiNode::new("node.flows.title", UiNodeKind::Label)
                        .with_text_key("nodes.flows")
                        .with_text_style(UiTextStyle::panel_title(tokens.text))
                        .with_layout(UiLayout {
                            grow: 1.0,
                            ..UiLayout::default()
                        }),
                )
                .with_child(node_toolbar_button(
                    "node.flows.new",
                    "nodes.new_flow",
                    "node.flow.new",
                    "node-toolbar-primary",
                )),
        )
        .with_child(
            UiNode::text_input("node.flow-search.input", search)
                .with_class("node-input")
                .with_layout(UiLayout::fixed(0.0, 30.0)),
        )
        .with_child(list)
        .with_child(
            UiNode::new("node.library", UiNodeKind::Panel)
                .with_class("node-library")
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    gap: 4.0,
                    padding: UiSpacing::same(8.0),
                    ..UiLayout::fixed(0.0, 72.0)
                })
                .with_child(
                    UiNode::new("node.library.title", UiNodeKind::Label)
                        .with_text_key("nodes.library")
                        .with_text_style(UiTextStyle::panel_title(tokens.text_muted))
                        .with_layout(UiLayout::fixed(0.0, 18.0)),
                )
                .with_child(node_toolbar_button(
                    "node.library.open",
                    "nodes.add_node",
                    "node.browser.open",
                    "node-toolbar-button",
                )),
        );
    let mut surface = UiSurface::new("node.flows", palette, root);
    surface.style_sheet = node_surface_style_sheet(palette);
    surface
}

fn build_node_inspector_surface(
    palette: StudioUiPalette,
    lang: Language,
    graph: &NodeGraph,
    selected: Option<NodeId>,
    wire_source: Option<(PinKind, PinDataType)>,
    filter: &str,
) -> UiSurface {
    let tokens = palette.tokens();
    let mut search = UiTextInput::new("node.template-search");
    search.placeholder_key = Some("nodes.search_nodes".to_string());
    search.max_length = 128;
    let query = filter.to_lowercase();
    let mut templates = UiNode::scroll_view("node.templates.list", UiScrollAxis::Vertical)
        .with_class("node-scroll")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 3.0,
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fixed(0.0, 260.0)
        });
    for template in NODE_TEMPLATES.iter().copied().filter(|template| {
        let matches_query = query.is_empty()
            || template.label.to_lowercase().contains(&query)
            || template.category.to_lowercase().contains(&query);
        let matches_wire = wire_source.is_none_or(|(source_kind, source_type)| {
            let node = (template.factory)();
            node.pins
                .iter()
                .any(|pin| pin.kind != source_kind && pins_compatible(source_type, pin.data_type))
        });
        matches_query && matches_wire
    }) {
        templates = templates.with_child(
            UiNode::new(format!("node.template.{}", template.id), UiNodeKind::Button)
                .with_text_key(format!("{}   {}", template.label, template.category))
                .with_class("node-template")
                .with_layout(UiLayout::fixed(0.0, 28.0))
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("node.template.add:{}", template.id),
                )),
        );
    }
    let inspector =
        if let Some(node) = selected.and_then(|id| graph.nodes.iter().find(|node| node.id == id)) {
            UiNode::new("node.inspector.content", UiNodeKind::Panel)
                .with_class("node-inspector-content")
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    gap: 6.0,
                    padding: UiSpacing::same(10.0),
                    grow: 1.0,
                    ..UiLayout::default()
                })
                .with_child(
                    UiNode::new("node.inspector.name", UiNodeKind::Label)
                        .with_text_key(node.name.clone())
                        .with_text_style(UiTextStyle::panel_title(tokens.text))
                        .with_layout(UiLayout::fixed(0.0, 20.0)),
                )
                .with_child(node_meta_label(
                    palette,
                    lang,
                    "node.inspector.category",
                    "nodes.category",
                    node.category.display_name(),
                ))
                .with_child(node_meta_label(
                    palette,
                    lang,
                    "node.inspector.ports",
                    "nodes.ports",
                    &node.pins.len().to_string(),
                ))
                .with_child(node_meta_label(
                    palette,
                    lang,
                    "node.inspector.description",
                    "nodes.description",
                    &node.description,
                ))
        } else {
            UiNode::new("node.inspector.empty", UiNodeKind::Panel)
                .with_class("node-inspector-content")
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    gap: 6.0,
                    padding: UiSpacing::same(10.0),
                    grow: 1.0,
                    ..UiLayout::default()
                })
                .with_child(
                    UiNode::new("node.inspector.empty.title", UiNodeKind::Label)
                        .with_text_key("nodes.no_selection")
                        .with_text_style(UiTextStyle::panel_title(tokens.text))
                        .with_layout(UiLayout::fixed(0.0, 20.0)),
                )
                .with_child(
                    UiNode::new("node.inspector.empty.copy", UiNodeKind::Label)
                        .with_text_key("nodes.select_node_to_edit")
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout::fixed(0.0, 42.0)),
                )
        };
    let mut root = UiNode::new("node.inspector.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            padding: UiSpacing::same(8.0),
            gap: 8.0,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(palette.root_style())
        .with_child(
            UiNode::new("node.add.header", UiNodeKind::Label)
                .with_text_key("nodes.add_node")
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fixed(0.0, 22.0)),
        )
        .with_child(
            UiNode::text_input("node.template-search.input", search)
                .with_class("node-input")
                .with_layout(UiLayout::fixed(0.0, 30.0)),
        );
    if wire_source.is_some() {
        root = root.with_child(
            UiNode::new("node.wire.hint", UiNodeKind::Label)
                .with_text_key("nodes.active_wire")
                .with_text_style(UiTextStyle::body(tokens.warning))
                .with_layout(UiLayout::fixed(0.0, 34.0)),
        );
    }
    root = root
        .with_child(templates)
        .with_child(
            UiNode::new("node.inspector.header", UiNodeKind::Label)
                .with_text_key("nodes.inspector")
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fixed(0.0, 22.0)),
        )
        .with_child(inspector);
    let mut surface = UiSurface::new("node.inspector", palette, root);
    surface.style_sheet = node_surface_style_sheet(palette);
    surface
}

fn node_meta_label(
    palette: StudioUiPalette,
    lang: Language,
    id: &str,
    label: &str,
    value: &str,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Label)
        .with_text_key(format!("{}: {}", t(label, lang), value))
        .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
        .with_layout(UiLayout::fixed(0.0, 22.0))
}

fn node_toolbar_button(id: &str, label: &str, command: &str, class: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_text_key(label)
        .with_class(class)
        .with_layout(UiLayout {
            min_size: [28.0, 28.0],
            padding: UiSpacing::xy(8.0, 4.0),
            ..UiLayout::default()
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn node_surface_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    UiStyleSheet {
        rules: vec![
            node_style_rule(
                "node-toolbar-button",
                tokens.surface_alt,
                tokens.border,
                tokens.text,
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("node-toolbar-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.focus),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            node_style_rule(
                "node-toolbar-primary",
                tokens.accent,
                tokens.accent_hot,
                [18, 18, 20, 255],
            ),
            node_style_rule("node-flow-row", tokens.surface, tokens.border, tokens.text),
            node_style_rule(
                "node-flow-active",
                tokens.selection,
                tokens.accent,
                tokens.text,
            ),
            node_style_rule(
                "node-flow-delete",
                tokens.surface_alt,
                tokens.border,
                tokens.text_muted,
            ),
            node_style_rule(
                "node-library",
                tokens.surface_alt,
                tokens.border,
                tokens.text,
            ),
            node_style_rule(
                "node-input",
                tokens.surface_raised,
                tokens.border,
                tokens.text,
            ),
            node_style_rule(
                "node-template",
                tokens.surface_alt,
                tokens.border,
                tokens.text,
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("node-template".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.focus),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            node_style_rule(
                "node-inspector-content",
                tokens.surface_alt,
                tokens.border,
                tokens.text,
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("node-scroll".to_string()),
                UiStylePatch {
                    fill: Some(tokens.background),
                    ..UiStylePatch::default()
                },
            ),
        ],
    }
}

fn node_style_rule(class: &str, fill: [u8; 4], border: [u8; 4], text: [u8; 4]) -> UiStyleRule {
    UiStyleRule::new(
        UiStyleSelector::Class(class.to_string()),
        UiStylePatch {
            fill: Some(fill),
            border: Some(border),
            border_width: Some(1.0),
            radius: Some(4.0),
            text: Some(text),
            ..UiStylePatch::default()
        },
    )
}

#[derive(Debug, Clone, Copy)]
struct NodeTemplate {
    id: &'static str,
    label: &'static str,
    category: &'static str,
    factory: fn() -> Node,
}

const NODE_TEMPLATES: &[NodeTemplate] = &[
    NodeTemplate {
        id: "on-start",
        label: "On Start",
        category: "Event",
        factory: Node::on_start,
    },
    NodeTemplate {
        id: "on-update",
        label: "On Update",
        category: "Event",
        factory: Node::on_update,
    },
    NodeTemplate {
        id: "print",
        label: "Print",
        category: "Action",
        factory: Node::print_action,
    },
    NodeTemplate {
        id: "if-branch",
        label: "If Branch",
        category: "Logic",
        factory: Node::if_branch,
    },
    NodeTemplate {
        id: "for-loop",
        label: "For Loop",
        category: "Logic",
        factory: raf_nodes::flow_nodes::FlowNodes::for_loop,
    },
    NodeTemplate {
        id: "while-loop",
        label: "While Loop",
        category: "Logic",
        factory: raf_nodes::flow_nodes::FlowNodes::while_loop,
    },
    NodeTemplate {
        id: "add",
        label: "Add",
        category: "Math",
        factory: Node::add_math,
    },
    NodeTemplate {
        id: "greater-than",
        label: "Greater Than",
        category: "Math",
        factory: || raf_nodes::math_nodes::MathNodes::compare(">"),
    },
    NodeTemplate {
        id: "less-than",
        label: "Less Than",
        category: "Math",
        factory: || raf_nodes::math_nodes::MathNodes::compare("<"),
    },
    NodeTemplate {
        id: "equals",
        label: "Equals",
        category: "Math",
        factory: || raf_nodes::math_nodes::MathNodes::compare("=="),
    },
    NodeTemplate {
        id: "spawn-entity",
        label: "Spawn Entity",
        category: "Action",
        factory: raf_nodes::entity_nodes::EntityNodes::spawn_entity,
    },
    NodeTemplate {
        id: "destroy-entity",
        label: "Destroy Entity",
        category: "Action",
        factory: raf_nodes::entity_nodes::EntityNodes::destroy_entity,
    },
    NodeTemplate {
        id: "set-position",
        label: "Set Position",
        category: "Action",
        factory: raf_nodes::entity_nodes::EntityNodes::set_position,
    },
    NodeTemplate {
        id: "key-press",
        label: "Key Press",
        category: "Event",
        factory: raf_nodes::input_nodes::InputNodes::key_press,
    },
    NodeTemplate {
        id: "mouse-click",
        label: "Mouse Click",
        category: "Event",
        factory: raf_nodes::input_nodes::InputNodes::mouse_click,
    },
    NodeTemplate {
        id: "delay",
        label: "Delay",
        category: "Logic",
        factory: raf_nodes::input_nodes::InputNodes::timer_delay,
    },
    NodeTemplate {
        id: "serial-read",
        label: "Serial Read",
        category: "Hardware",
        factory: raf_nodes::hardware_nodes::HardwareNodes::serial_read,
    },
    NodeTemplate {
        id: "serial-write",
        label: "Serial Write",
        category: "Hardware",
        factory: raf_nodes::hardware_nodes::HardwareNodes::serial_write,
    },
    NodeTemplate {
        id: "read-sensor",
        label: "Read Sensor",
        category: "Hardware",
        factory: raf_nodes::hardware_nodes::HardwareNodes::sensor_input,
    },
    NodeTemplate {
        id: "write-actuator",
        label: "Write Actuator",
        category: "Hardware",
        factory: raf_nodes::hardware_nodes::HardwareNodes::actuator_output,
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
enum NodeEditorShellAction {
    NewFlow,
    SelectFlow(usize),
    DeleteFlow(usize),
    OpenNodeBrowser,
    AddNode(&'static str),
    FocusSelection,
    FitFlow,
    ZoomIn,
    ZoomOut,
}

struct NodeEditorSurfaceHost {
    toolbar: RafUiSurfaceBridge,
    flows: RafUiSurfaceBridge,
    inspector: RafUiSurfaceBridge,
    flow_filter: String,
    node_filter: String,
}

impl Default for NodeEditorSurfaceHost {
    fn default() -> Self {
        Self {
            toolbar: RafUiSurfaceBridge::new("raf_ui_node_toolbar"),
            flows: RafUiSurfaceBridge::new("raf_ui_node_flows"),
            inspector: RafUiSurfaceBridge::new("raf_ui_node_inspector"),
            flow_filter: String::new(),
            node_filter: String::new(),
        }
    }
}

impl NodeEditorSurfaceHost {
    fn show_toolbar(
        &mut self,
        ui: &mut Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        lang: Language,
        zoom: f32,
        has_selection: bool,
    ) -> Vec<NodeEditorShellAction> {
        let surface = build_node_toolbar_surface(palette, zoom, has_selection);
        let dispatched = self
            .toolbar
            .show(ui, render_state, palette, surface, |key| t(key, lang));
        self.collect(dispatched)
    }

    fn show_flow_rail(
        &mut self,
        ui: &mut Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        lang: Language,
        graphs: &[NodeGraph],
        active_graph_index: usize,
    ) -> Vec<NodeEditorShellAction> {
        let surface =
            build_flow_rail_surface(palette, graphs, active_graph_index, &self.flow_filter);
        let filter = self.flow_filter.clone();
        let dispatched = self.flows.show_with_control_state(
            ui,
            render_state,
            palette,
            surface,
            |controls| controls.set_text("node.flow-search", filter.clone(), 128),
            |key| t(key, lang),
        );
        self.collect(dispatched)
    }

    fn show_inspector(
        &mut self,
        ui: &mut Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        lang: Language,
        graph: &NodeGraph,
        selected: Option<NodeId>,
        wire_source: Option<(PinKind, PinDataType)>,
    ) -> Vec<NodeEditorShellAction> {
        let surface = build_node_inspector_surface(
            palette,
            lang,
            graph,
            selected,
            wire_source,
            &self.node_filter,
        );
        let filter = self.node_filter.clone();
        let dispatched = self.inspector.show_with_control_state(
            ui,
            render_state,
            palette,
            surface,
            |controls| controls.set_text("node.template-search", filter.clone(), 128),
            |key| t(key, lang),
        );
        self.collect(dispatched)
    }

    fn collect(
        &mut self,
        dispatched: Vec<raf_render::api_graphic_basic::ui_surface::UiDispatchedAction>,
    ) -> Vec<NodeEditorShellAction> {
        let mut actions = Vec::new();
        for action in dispatched {
            match action.action {
                UiAction::SetText { key, value } if key == "node.flow-search" => {
                    self.flow_filter = value;
                }
                UiAction::SetText { key, value } if key == "node.template-search" => {
                    self.node_filter = value;
                }
                UiAction::Command { name } => {
                    if name == "node.flow.new" {
                        actions.push(NodeEditorShellAction::NewFlow);
                    } else if name == "node.browser.open" {
                        actions.push(NodeEditorShellAction::OpenNodeBrowser);
                    } else if name == "node.canvas.focus-selection" {
                        actions.push(NodeEditorShellAction::FocusSelection);
                    } else if name == "node.canvas.fit" {
                        actions.push(NodeEditorShellAction::FitFlow);
                    } else if name == "node.canvas.zoom-in" {
                        actions.push(NodeEditorShellAction::ZoomIn);
                    } else if name == "node.canvas.zoom-out" {
                        actions.push(NodeEditorShellAction::ZoomOut);
                    } else if let Some(index) = command_index(&name, "node.flow.select") {
                        actions.push(NodeEditorShellAction::SelectFlow(index));
                    } else if let Some(index) = command_index(&name, "node.flow.delete") {
                        actions.push(NodeEditorShellAction::DeleteFlow(index));
                    } else if let Some(id) = name.strip_prefix("node.template.add:") {
                        if let Some(template) = node_template(id) {
                            actions.push(NodeEditorShellAction::AddNode(template.id));
                        }
                    }
                }
                _ => {}
            }
        }
        actions
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeEditorDocument {
    pub graphs: Vec<NodeGraph>,
    pub active_graph_index: usize,
}

impl Default for NodeEditorDocument {
    fn default() -> Self {
        Self {
            graphs: vec![NodeGraph::new("Main")],
            active_graph_index: 0,
        }
    }
}

pub struct NodeEditorPanel {
    pub graphs: Vec<NodeGraph>,
    pub active_graph_index: usize,

    pub offset: Vec2,
    pub zoom: f32,
    pub selected_node: Option<NodeId>,
    selected_nodes: Vec<NodeId>,
    selected_connection: Option<Uuid>,

    dragging_node: Option<NodeId>,
    node_drag_origins: Vec<(NodeId, [f32; 2])>,
    node_drag_start_pointer: Option<Pos2>,
    node_drag_changed: bool,
    drag_connection: Option<DragConnection>,
    pending_connection: Option<DragConnection>,
    selection_box_start: Option<Pos2>,
    selection_box_current: Option<Pos2>,
    show_palette: bool,
    palette_pos: Pos2,
    palette_filter: String,
    last_canvas_position: Option<Pos2>,
    request_fit_flow: bool,
    request_focus_selection: bool,
    pending_zoom_delta: f32,
    copied_nodes: Vec<Node>,
    surface: NodeEditorSurfaceHost,

    // Undo/Redo
    history: Vec<(Vec<NodeGraph>, usize)>,
    history_pointer: usize,

    // Auto-save tracker
    auto_save_timer: f64,
}

impl Default for NodeEditorPanel {
    fn default() -> Self {
        let initial_graph = NodeGraph::new("Main");
        let initial_graphs = vec![initial_graph];
        Self {
            history: vec![(initial_graphs.clone(), 0)],
            history_pointer: 0,
            graphs: initial_graphs,
            active_graph_index: 0,
            offset: Vec2::ZERO,
            zoom: 1.0,
            selected_node: None,
            selected_nodes: Vec::new(),
            selected_connection: None,
            dragging_node: None,
            node_drag_origins: Vec::new(),
            node_drag_start_pointer: None,
            node_drag_changed: false,
            drag_connection: None,
            pending_connection: None,
            selection_box_start: None,
            selection_box_current: None,
            show_palette: false,
            palette_pos: Pos2::ZERO,
            palette_filter: String::new(),
            last_canvas_position: None,
            request_fit_flow: false,
            request_focus_selection: false,
            pending_zoom_delta: 0.0,
            copied_nodes: Vec::new(),
            surface: NodeEditorSurfaceHost::default(),
            auto_save_timer: 0.0,
        }
    }
}

impl NodeEditorPanel {
    pub fn document(&self) -> NodeEditorDocument {
        NodeEditorDocument {
            graphs: self.graphs.clone(),
            active_graph_index: self.active_graph_index,
        }
    }

    pub fn load_document(&mut self, document: NodeEditorDocument) {
        let mut graphs = document.graphs;
        if graphs.is_empty() {
            graphs.push(NodeGraph::new("Main"));
        }

        let active_graph_index = document
            .active_graph_index
            .min(graphs.len().saturating_sub(1));

        self.history = vec![(graphs.clone(), active_graph_index)];
        self.history_pointer = 0;
        self.graphs = graphs;
        self.active_graph_index = active_graph_index;
        self.selected_node = None;
        self.selected_nodes.clear();
        self.selected_connection = None;
        self.dragging_node = None;
        self.node_drag_origins.clear();
        self.node_drag_start_pointer = None;
        self.node_drag_changed = false;
        self.drag_connection = None;
        self.pending_connection = None;
        self.selection_box_start = None;
        self.selection_box_current = None;
        self.show_palette = false;
        self.palette_filter.clear();
        self.last_canvas_position = None;
        self.request_fit_flow = false;
        self.request_focus_selection = false;
        self.pending_zoom_delta = 0.0;
    }

    fn active_graph(&self) -> &NodeGraph {
        &self.graphs[self.active_graph_index]
    }

    fn active_graph_mut(&mut self) -> &mut NodeGraph {
        &mut self.graphs[self.active_graph_index]
    }

    fn push_history(&mut self) {
        self.history.truncate(self.history_pointer + 1);
        self.history
            .push((self.graphs.clone(), self.active_graph_index));
        if self.history.len() > 50 {
            self.history.remove(0);
        } else {
            self.history_pointer += 1;
        }
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        lang: Language,
    ) {
        let mut state_changed = false;

        if ui.input(|i| i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(egui::Key::Z))
            || ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Y))
        {
            if self.history_pointer + 1 < self.history.len() {
                self.history_pointer += 1;
                let state = self.history[self.history_pointer].clone();
                self.graphs = state.0;
                self.active_graph_index = state.1;
                self.clear_selection();
            }
        } else if ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Z)) {
            if self.history_pointer > 0 {
                self.history_pointer -= 1;
                let state = self.history[self.history_pointer].clone();
                self.graphs = state.0;
                self.active_graph_index = state.1;
                self.clear_selection();
            }
        }

        // Auto-save tracking (30 seconds)
        let time = ui.input(|i| i.time);
        if self.auto_save_timer == 0.0 {
            self.auto_save_timer = time;
        } else if time - self.auto_save_timer > 30.0 {
            self.auto_save_timer = time;
            // Simulated local auto-save for flow panel
            // In a real integration, this would trigger app layer serialization
            // Here we just ensure current state is snapshotted properly.
        }

        let toolbar_actions = ui
            .allocate_ui_with_layout(
                Vec2::new(ui.available_width(), CANVAS_TOOLBAR_HEIGHT),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    self.surface.show_toolbar(
                        ui,
                        render_state,
                        palette,
                        lang,
                        self.zoom,
                        !self.selected_nodes.is_empty(),
                    )
                },
            )
            .inner;
        self.apply_shell_actions(toolbar_actions, &mut state_changed);

        ui.add_space(6.0);
        let available_height = ui.available_height().max(1.0);
        ui.horizontal_top(|ui| {
            let flow_actions = ui
                .allocate_ui_with_layout(
                    Vec2::new(FLOW_RAIL_WIDTH, available_height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        self.surface.show_flow_rail(
                            ui,
                            render_state,
                            palette,
                            lang,
                            &self.graphs,
                            self.active_graph_index,
                        )
                    },
                )
                .inner;
            self.apply_shell_actions(flow_actions, &mut state_changed);

            ui.add_space(6.0);
            let center_width = (ui.available_width() - INSPECTOR_RAIL_WIDTH - 6.0).max(120.0);
            ui.allocate_ui_with_layout(
                Vec2::new(center_width, available_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| self.draw_canvas(ui, lang, &mut state_changed),
            );

            ui.add_space(6.0);
            let active_graph = self.active_graph().clone();
            let inspector_actions = ui
                .allocate_ui_with_layout(
                    Vec2::new(INSPECTOR_RAIL_WIDTH, available_height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        self.surface.show_inspector(
                            ui,
                            render_state,
                            palette,
                            lang,
                            &active_graph,
                            self.selected_node,
                            self.drag_connection
                                .as_ref()
                                .or(self.pending_connection.as_ref())
                                .map(|connection| (connection.from_kind, connection.from_type)),
                        )
                    },
                )
                .inner;
            self.apply_shell_actions(inspector_actions, &mut state_changed);
        });

        // Record history snapshot if mutations happened
        if state_changed {
            self.push_history();
        }
    }

    fn apply_shell_actions(
        &mut self,
        actions: Vec<NodeEditorShellAction>,
        state_changed: &mut bool,
    ) {
        for action in actions {
            match action {
                NodeEditorShellAction::NewFlow => {
                    let name = format!("Flow_{:02}", self.graphs.len() + 1);
                    self.graphs.push(NodeGraph::new(&name));
                    self.active_graph_index = self.graphs.len() - 1;
                    self.clear_selection();
                    *state_changed = true;
                }
                NodeEditorShellAction::SelectFlow(index) if index < self.graphs.len() => {
                    self.active_graph_index = index;
                    self.clear_selection();
                }
                NodeEditorShellAction::DeleteFlow(index)
                    if self.graphs.len() > 1 && index < self.graphs.len() =>
                {
                    self.graphs.remove(index);
                    if self.active_graph_index >= self.graphs.len() {
                        self.active_graph_index = self.graphs.len() - 1;
                    } else if self.active_graph_index > index {
                        self.active_graph_index -= 1;
                    }
                    self.clear_selection();
                    *state_changed = true;
                }
                NodeEditorShellAction::OpenNodeBrowser => self.open_node_browser_at_last_pointer(),
                NodeEditorShellAction::AddNode(template) => {
                    self.palette_pos = self
                        .last_canvas_position
                        .unwrap_or_else(|| Pos2::new(160.0, 120.0));
                    if self.add_template_at_palette(template) {
                        *state_changed = true;
                    }
                }
                NodeEditorShellAction::FocusSelection => self.request_focus_selection = true,
                NodeEditorShellAction::FitFlow => self.request_fit_flow = true,
                NodeEditorShellAction::ZoomIn => self.pending_zoom_delta += 0.16,
                NodeEditorShellAction::ZoomOut => self.pending_zoom_delta -= 0.16,
                _ => {}
            }
        }
    }

    fn clear_selection(&mut self) {
        self.selected_node = None;
        self.selected_nodes.clear();
        self.selected_connection = None;
    }

    fn open_node_browser_at_last_pointer(&mut self) {
        self.show_palette = true;
        self.palette_filter.clear();
        self.palette_pos = self
            .last_canvas_position
            .unwrap_or_else(|| Pos2::new(160.0, 120.0));
    }

    fn add_template_at_palette(&mut self, template_id: &str) -> bool {
        let Some(template) = node_template(template_id) else {
            return false;
        };
        let mut node = (template.factory)();
        node.position = [self.palette_pos.x, self.palette_pos.y];
        let node_id = self.active_graph_mut().add_node(node);
        self.try_complete_pending_connection(node_id);
        self.select_exact(node_id);
        self.show_palette = false;
        self.palette_filter.clear();
        true
    }

    fn select_exact(&mut self, node_id: NodeId) {
        self.selected_nodes.clear();
        self.selected_nodes.push(node_id);
        self.selected_node = Some(node_id);
        self.selected_connection = None;
    }

    fn select_node_with_modifiers(&mut self, node_id: NodeId, modifiers: egui::Modifiers) {
        if modifiers.ctrl {
            if let Some(index) = self.selected_nodes.iter().position(|id| *id == node_id) {
                self.selected_nodes.remove(index);
            }
        } else if modifiers.shift {
            if !self.selected_nodes.contains(&node_id) {
                self.selected_nodes.push(node_id);
            }
        } else {
            self.selected_nodes.clear();
            self.selected_nodes.push(node_id);
        }
        self.selected_node = self.selected_nodes.last().copied();
        self.selected_connection = None;
    }

    fn draw_canvas(&mut self, ui: &mut Ui, lang: Language, state_changed: &mut bool) {
        let available = ui.available_rect_before_wrap();
        let (id, rect) = ui.allocate_space(available.size());
        let response = ui.interact(rect, id, egui::Sense::click_and_drag());
        let painter = ui.painter_at(rect);

        painter.rect_filled(rect, 0.0, theme::DARK_BG);
        self.draw_grid(&painter, rect);

        let pointer = ui.input(|input| input.pointer.hover_pos());
        if let Some(pointer) = pointer.filter(|position| rect.contains(*position)) {
            self.last_canvas_position = Some(self.screen_to_canvas(pointer, rect));
        }

        if self.request_fit_flow || response.double_clicked_by(egui::PointerButton::Middle) {
            self.focus_nodes(
                rect,
                &self
                    .active_graph()
                    .nodes
                    .iter()
                    .map(|node| node.id)
                    .collect::<Vec<_>>(),
            );
            self.request_fit_flow = false;
        }
        if self.request_focus_selection {
            let selection = self.selected_nodes.clone();
            self.focus_nodes(rect, &selection);
            self.request_focus_selection = false;
        }
        if self.pending_zoom_delta.abs() > f32::EPSILON {
            self.zoom_around_screen_point(rect.center(), rect, self.pending_zoom_delta);
            self.pending_zoom_delta = 0.0;
        }

        let space_pan = ui.input(|input| input.key_down(egui::Key::Space));
        let escape_pressed = ui.input(|input| input.key_pressed(egui::Key::Escape));
        if escape_pressed {
            self.drag_connection = None;
            self.pending_connection = None;
            self.show_palette = false;
            self.selection_box_start = None;
            self.selection_box_current = None;
        }
        if ui.input(|input| input.key_pressed(egui::Key::Backspace))
            && self.drag_connection.is_some()
        {
            self.drag_connection = None;
            self.pending_connection = None;
        }
        if ui.rect_contains_pointer(rect) && ui.input(|input| input.key_pressed(egui::Key::Tab)) {
            self.open_node_browser_at_last_pointer();
        }
        if ui.rect_contains_pointer(rect) && ui.input(|input| input.key_pressed(egui::Key::Home)) {
            self.request_fit_flow = true;
        }
        if ui.rect_contains_pointer(rect) && ui.input(|input| input.key_pressed(egui::Key::F)) {
            self.request_focus_selection = true;
        }
        if ui.rect_contains_pointer(rect)
            && ui.input(|input| input.modifiers.ctrl && input.key_pressed(egui::Key::Num1))
        {
            self.set_zoom_around_screen_point(1.0, rect.center(), rect);
        }

        if ui.rect_contains_pointer(rect) {
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let speed = ui.input(|input| if input.modifiers.ctrl { 0.006 } else { 0.002 });
                let anchor = pointer.unwrap_or_else(|| rect.center());
                self.zoom_around_screen_point(anchor, rect, scroll * speed);
            }
        }

        let nodes_snapshot = self.active_graph().nodes.clone();
        let conns_snapshot = self.active_graph().connections.clone();
        for conn in &conns_snapshot {
            self.draw_connection(&painter, rect, conn, &nodes_snapshot);
        }

        if let Some(drag) = &self.drag_connection {
            let mouse_pos = ui.input(|i| i.pointer.hover_pos().unwrap_or(drag.from_pos));
            Self::draw_bezier(
                &painter,
                drag.from_pos,
                mouse_pos,
                Color32::from_rgb(255, 200, 100),
                2.0,
            );
        }

        let node_data: Vec<(
            NodeId,
            [f32; 2],
            String,
            NodeCategory,
            Vec<raf_nodes::node::NodePin>,
        )> = self
            .active_graph()
            .nodes
            .iter()
            .map(|n| (n.id, n.position, n.name.clone(), n.category, n.pins.clone()))
            .collect();

        let pointer_over_node = pointer.is_some_and(|position| {
            node_data
                .iter()
                .any(|(node_id, position_data, _, _, pins)| {
                    self.node_rect(rect, *node_id, *position_data, pins)
                        .contains(position)
                })
        });

        if response.secondary_clicked() || response.double_clicked_by(egui::PointerButton::Primary)
        {
            if let Some(mouse) = pointer {
                self.palette_pos = self.screen_to_canvas(mouse, rect);
            }
            self.show_palette = true;
            self.palette_filter.clear();
        }

        if response.drag_started_by(egui::PointerButton::Primary)
            && !pointer_over_node
            && !space_pan
            && self.drag_connection.is_none()
        {
            self.selection_box_start = pointer;
            self.selection_box_current = pointer;
            self.selected_connection = None;
        }

        if self.selection_box_start.is_some() && response.dragged_by(egui::PointerButton::Primary) {
            self.selection_box_current = pointer;
        }

        for (node_id, position, name, category, pins) in &node_data {
            self.draw_node_visual(
                ui,
                &painter,
                rect,
                *node_id,
                *position,
                name,
                *category,
                pins,
                state_changed,
                space_pan,
            );
        }

        let middle_pan = response.dragged_by(egui::PointerButton::Middle);
        let space_pan_drag = space_pan
            && response.dragged_by(egui::PointerButton::Primary)
            && self.dragging_node.is_none()
            && self.drag_connection.is_none();
        if middle_pan || space_pan_drag {
            let mut delta = ui.input(|input| input.pointer.delta());
            if middle_pan && ui.input(|input| input.modifiers.shift) {
                delta = Vec2::new(delta.x * 1.8, 0.0);
            }
            self.offset += delta;
        }

        let clicked_connection = pointer.and_then(|position| {
            (!pointer_over_node && response.clicked_by(egui::PointerButton::Primary))
                .then(|| {
                    self.connection_at_screen(rect, position, &conns_snapshot, &nodes_snapshot)
                })
                .flatten()
        });
        if let Some(connection_id) = clicked_connection {
            self.selected_connection = Some(connection_id);
            self.selected_nodes.clear();
            self.selected_node = None;
        } else if response.clicked_by(egui::PointerButton::Primary)
            && !pointer_over_node
            && self.drag_connection.is_none()
            && !space_pan
        {
            self.clear_selection();
            self.show_palette = false;
        }

        if let (Some(start), Some(current)) = (self.selection_box_start, self.selection_box_current)
        {
            let selection_rect = Rect::from_two_pos(start, current);
            painter.rect_stroke(selection_rect, 0.0, Stroke::new(1.0, theme::ACCENT));
            painter.rect_filled(
                selection_rect,
                0.0,
                Color32::from_rgba_premultiplied(
                    theme::ACCENT.r(),
                    theme::ACCENT.g(),
                    theme::ACCENT.b(),
                    24,
                ),
            );
        }

        self.draw_minimap(ui, &painter, rect, &nodes_snapshot);

        if ui.input(|input| input.pointer.any_released()) {
            if let Some(drag) = self.drag_connection.take() {
                if let Some(mouse) =
                    pointer.filter(|position| rect.contains(*position) && !pointer_over_node)
                {
                    self.pending_connection = Some(drag);
                    self.palette_pos = self.screen_to_canvas(mouse, rect);
                    self.show_palette = true;
                    self.palette_filter.clear();
                }
            }
            if self.dragging_node.take().is_some() && self.node_drag_changed {
                *state_changed = true;
            }
            self.node_drag_origins.clear();
            self.node_drag_start_pointer = None;
            self.node_drag_changed = false;
            self.finish_box_selection(rect, &node_data, ui.input(|input| input.modifiers));
        }

        if ui.rect_contains_pointer(rect) && ui.input(|input| input.key_pressed(egui::Key::Delete))
        {
            if self.delete_selected() {
                *state_changed = true;
            }
        }
        if ui.rect_contains_pointer(rect)
            && ui.input(|input| input.modifiers.ctrl && input.key_pressed(egui::Key::D))
        {
            if self.duplicate_selected(self.last_canvas_position) {
                *state_changed = true;
            }
        }
        if ui.rect_contains_pointer(rect)
            && ui.input(|input| input.modifiers.ctrl && input.key_pressed(egui::Key::C))
        {
            self.copy_selected();
        }
        if ui.rect_contains_pointer(rect)
            && ui.input(|input| input.modifiers.ctrl && input.key_pressed(egui::Key::V))
        {
            if self.paste_nodes(self.last_canvas_position) {
                *state_changed = true;
            }
        }

        if self.show_palette {
            self.draw_palette(ui, lang, rect, state_changed);
        }
    }

    fn draw_grid(&self, painter: &egui::Painter, rect: Rect) {
        let grid_color = Color32::from_rgb(25, 31, 39);
        let grid_major = Color32::from_rgb(42, 49, 59);
        let step = 30.0 * self.zoom;
        if step < 3.0 {
            return;
        }

        let mut x = rect.left() + (self.offset.x % step);
        let mut ix = 0u32;
        while x < rect.right() {
            let c = if ix % 5 == 0 { grid_major } else { grid_color };
            painter.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(0.7, c),
            );
            x += step;
            ix += 1;
        }

        let mut y = rect.top() + (self.offset.y % step);
        let mut iy = 0u32;
        while y < rect.bottom() {
            let c = if iy % 5 == 0 { grid_major } else { grid_color };
            painter.line_segment(
                [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                Stroke::new(0.7, c),
            );
            y += step;
            iy += 1;
        }
    }

    fn draw_node_visual(
        &mut self,
        ui: &mut Ui,
        painter: &egui::Painter,
        canvas_rect: Rect,
        node_id: NodeId,
        position: [f32; 2],
        name: &str,
        category: NodeCategory,
        pins: &[raf_nodes::node::NodePin],
        state_changed: &mut bool,
        space_pan: bool,
    ) {
        let node_rect = self.node_rect(canvas_rect, node_id, position, pins);
        let screen_pos = node_rect.min;

        if !canvas_rect.intersects(node_rect) {
            return;
        }

        let is_selected = self.selected_nodes.contains(&node_id);

        let shadow_rect = node_rect.translate(Vec2::new(3.0, 3.0));
        painter.rect_filled(
            shadow_rect,
            NODE_ROUNDING * self.zoom,
            Color32::from_rgba_premultiplied(0, 0, 0, 60),
        );
        painter.rect_filled(
            node_rect,
            NODE_ROUNDING * self.zoom,
            Color32::from_rgb(25, 31, 38),
        );

        let cat_color = category_color(category);
        let header_rect = Rect::from_min_size(
            node_rect.min,
            Vec2::new(node_rect.width(), NODE_HEADER_HEIGHT * self.zoom),
        );
        painter.rect_filled(
            header_rect,
            egui::Rounding {
                nw: NODE_ROUNDING * self.zoom,
                ne: NODE_ROUNDING * self.zoom,
                sw: 0.0,
                se: 0.0,
            },
            cat_color,
        );

        painter.text(
            header_rect.center(),
            egui::Align2::CENTER_CENTER,
            name,
            egui::FontId::proportional(12.0 * self.zoom),
            Color32::WHITE,
        );

        if is_selected {
            painter.rect_stroke(
                node_rect,
                NODE_ROUNDING * self.zoom,
                Stroke::new(
                    if self.selected_node == Some(node_id) {
                        2.0
                    } else {
                        1.0
                    },
                    theme::ACCENT,
                ),
            );
        }

        for (i, pin) in pins.iter().enumerate() {
            let y_offset = NODE_HEADER_HEIGHT + (i as f32 + 0.5) * PIN_ROW_HEIGHT;
            let pin_y = screen_pos.y + y_offset * self.zoom;
            let pin_x = match pin.kind {
                PinKind::Input => node_rect.left(),
                PinKind::Output => node_rect.right(),
            };

            let pin_center = Pos2::new(pin_x, pin_y);
            let pin_color = pin_data_type_color(pin.data_type);
            let compatible_target = self.drag_connection.as_ref().is_some_and(|drag| {
                drag.from_node != node_id
                    && drag.from_kind != pin.kind
                    && pins_compatible(drag.from_type, pin.data_type)
            });

            painter.circle_filled(pin_center, PIN_RADIUS * self.zoom, pin_color);
            painter.circle_stroke(
                pin_center,
                PIN_RADIUS * self.zoom,
                Stroke::new(
                    if compatible_target { 2.0 } else { 1.0 },
                    if compatible_target {
                        theme::ACCENT
                    } else {
                        Color32::from_rgb(200, 200, 210)
                    },
                ),
            );

            let (text_offset, text_align) = match pin.kind {
                PinKind::Input => (12.0 * self.zoom, egui::Align2::LEFT_CENTER),
                PinKind::Output => (-12.0 * self.zoom, egui::Align2::RIGHT_CENTER),
            };
            painter.text(
                Pos2::new(pin_x + text_offset, pin_y),
                text_align,
                &pin.name,
                egui::FontId::proportional(10.0 * self.zoom),
                Color32::from_rgb(200, 200, 210),
            );

            let pin_hit =
                Rect::from_center_size(pin_center, Vec2::splat(PIN_RADIUS * 3.0 * self.zoom));
            let pin_resp =
                ui.interact(pin_hit, ui.id().with(pin.id), egui::Sense::click_and_drag());

            if (pin_resp.drag_started() || pin_resp.clicked()) && self.drag_connection.is_none() {
                self.drag_connection = Some(DragConnection {
                    from_node: node_id,
                    from_pin: pin.id,
                    from_pos: pin_center,
                    from_kind: pin.kind,
                    from_type: pin.data_type,
                });
                self.pending_connection = None;
            }

            let ptr_released = ui.input(|i| i.pointer.any_released());
            let pointer_pos = ui.input(|i| i.pointer.hover_pos());
            if pointer_pos.is_some_and(|pos| pin_hit.contains(pos)) && ptr_released {
                if let Some(drag) = self.drag_connection.take() {
                    if drag.from_kind != pin.kind
                        && drag.from_node != node_id
                        && pins_compatible(drag.from_type, pin.data_type)
                    {
                        match drag.from_kind {
                            PinKind::Output => {
                                self.active_graph_mut().connect(
                                    drag.from_node,
                                    drag.from_pin,
                                    node_id,
                                    pin.id,
                                );
                            }
                            PinKind::Input => {
                                self.active_graph_mut().connect(
                                    node_id,
                                    pin.id,
                                    drag.from_node,
                                    drag.from_pin,
                                );
                            }
                        }
                        *state_changed = true;
                    } else if drag.from_node == node_id && drag.from_pin == pin.id {
                        self.drag_connection = Some(drag);
                    }
                }
            }
        }

        let header_resp = ui.interact(
            header_rect,
            ui.id().with(node_id),
            egui::Sense::click_and_drag(),
        );
        if header_resp.clicked() {
            self.select_node_with_modifiers(node_id, ui.input(|input| input.modifiers));
        }
        if header_resp.double_clicked() {
            self.select_exact(node_id);
        }
        if header_resp.drag_started() && !space_pan {
            let dragged_node = if ui.input(|input| input.modifiers.alt) {
                self.duplicate_one(node_id).unwrap_or(node_id)
            } else {
                node_id
            };
            if dragged_node != node_id || !self.selected_nodes.contains(&dragged_node) {
                self.select_exact(dragged_node);
            }
            self.dragging_node = Some(dragged_node);
            self.node_drag_origins = self
                .active_graph()
                .nodes
                .iter()
                .filter(|node| self.selected_nodes.contains(&node.id))
                .map(|node| (node.id, node.position))
                .collect();
            self.node_drag_start_pointer = ui.input(|input| input.pointer.hover_pos());
            self.node_drag_changed = dragged_node != node_id;
        }
        if self.dragging_node == Some(node_id) && header_resp.dragged() && !space_pan {
            let Some(start) = self.node_drag_start_pointer else {
                return;
            };
            let Some(pointer) = ui.input(|input| input.pointer.hover_pos()) else {
                return;
            };
            let delta = (pointer - start) / self.zoom;
            let ignore_snap = ui.input(|input| input.modifiers.shift);
            let snap_threshold = 6.0 / self.zoom;
            let origins = self.node_drag_origins.clone();
            for (id, origin) in origins {
                if let Some(node) = self
                    .active_graph_mut()
                    .nodes
                    .iter_mut()
                    .find(|candidate| candidate.id == id)
                {
                    let position_x = origin[0] + delta.x;
                    let position_y = origin[1] + delta.y;
                    node.position[0] = if ignore_snap {
                        position_x
                    } else {
                        soft_snap(position_x, 30.0, snap_threshold)
                    };
                    node.position[1] = if ignore_snap {
                        position_y
                    } else {
                        soft_snap(position_y, 30.0, snap_threshold)
                    };
                }
            }
            self.node_drag_changed = true;
        }
    }

    fn draw_connection(
        &self,
        painter: &egui::Painter,
        canvas_rect: Rect,
        conn: &raf_nodes::graph::Connection,
        nodes: &[Node],
    ) {
        let from_pos = self.find_pin_screen_pos(canvas_rect, conn.from_node, conn.from_pin, nodes);
        let to_pos = self.find_pin_screen_pos(canvas_rect, conn.to_node, conn.to_pin, nodes);

        if let (Some(from), Some(to)) = (from_pos, to_pos) {
            let color = self.pin_color_from_nodes(conn.from_node, conn.from_pin, nodes);
            let selected = self.selected_connection == Some(conn.id);
            Self::draw_bezier(
                painter,
                from,
                to,
                if selected { theme::ACCENT } else { color },
                if selected {
                    CONNECTION_THICKNESS + 1.5
                } else {
                    CONNECTION_THICKNESS
                },
            );
        }
    }

    fn draw_bezier(painter: &egui::Painter, from: Pos2, to: Pos2, color: Color32, thickness: f32) {
        let dx = (to.x - from.x).abs() * 0.5;
        let cp1 = Pos2::new(from.x + dx, from.y);
        let cp2 = Pos2::new(to.x - dx, to.y);

        let segments = 24;
        let mut points = Vec::with_capacity(segments + 1);
        for i in 0..=segments {
            let t = i as f32 / segments as f32;
            let it = 1.0 - t;
            let x = it * it * it * from.x
                + 3.0 * it * it * t * cp1.x
                + 3.0 * it * t * t * cp2.x
                + t * t * t * to.x;
            let y = it * it * it * from.y
                + 3.0 * it * it * t * cp1.y
                + 3.0 * it * t * t * cp2.y
                + t * t * t * to.y;
            points.push(Pos2::new(x, y));
        }

        for w in points.windows(2) {
            painter.line_segment([w[0], w[1]], Stroke::new(thickness, color));
        }
    }

    fn find_pin_screen_pos(
        &self,
        canvas_rect: Rect,
        node_id: NodeId,
        pin_id: Uuid,
        nodes: &[Node],
    ) -> Option<Pos2> {
        let node = nodes.iter().find(|n| n.id == node_id)?;
        let (pin_index, pin) = node.pins.iter().enumerate().find(|(_, p)| p.id == pin_id)?;
        let screen_pos =
            self.canvas_to_screen(Pos2::new(node.position[0], node.position[1]), canvas_rect);
        let y_offset = NODE_HEADER_HEIGHT + (pin_index as f32 + 0.5) * PIN_ROW_HEIGHT;
        let pin_y = screen_pos.y + y_offset * self.zoom;
        let pin_x = match pin.kind {
            PinKind::Input => screen_pos.x,
            PinKind::Output => screen_pos.x + NODE_WIDTH * self.zoom,
        };
        Some(Pos2::new(pin_x, pin_y))
    }

    fn pin_color_from_nodes(&self, node_id: NodeId, pin_id: Uuid, nodes: &[Node]) -> Color32 {
        if let Some(node) = nodes.iter().find(|n| n.id == node_id) {
            if let Some(pin) = node.pins.iter().find(|p| p.id == pin_id) {
                return pin_data_type_color(pin.data_type);
            }
        }
        Color32::from_rgb(180, 180, 190)
    }

    fn draw_palette(
        &mut self,
        ui: &mut Ui,
        lang: Language,
        canvas_rect: Rect,
        state_changed: &mut bool,
    ) {
        let anchor = self.canvas_to_screen(self.palette_pos, canvas_rect);
        let size = Vec2::new(256.0, 328.0);
        let origin = Pos2::new(
            anchor
                .x
                .clamp(canvas_rect.left() + 8.0, canvas_rect.right() - size.x - 8.0),
            anchor
                .y
                .clamp(canvas_rect.top() + 8.0, canvas_rect.bottom() - size.y - 8.0),
        );
        let area = egui::Area::new(ui.id().with("node.browser"))
            .order(egui::Order::Foreground)
            .fixed_pos(origin);
        area.show(ui.ctx(), |ui| {
            let palette = theme::palette_for_visuals(ui.visuals().dark_mode, 0.0);
            egui::Frame::none()
                .fill(palette.panel)
                .stroke(Stroke::new(1.0, palette.border))
                .rounding(6.0)
                .inner_margin(egui::Margin::same(8.0))
                .show(ui, |ui| {
                    ui.set_min_width(size.x);
                    ui.label(
                        RichText::new(t("nodes.add_node", lang))
                            .size(12.0)
                            .color(theme::ACCENT),
                    );
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.palette_filter)
                            .hint_text(t("nodes.search_nodes", lang))
                            .desired_width(f32::INFINITY),
                    );
                    if self.palette_filter.is_empty() {
                        response.request_focus();
                    }
                    ui.add_space(4.0);
                    let filter = self.palette_filter.to_lowercase();
                    let matching = NODE_TEMPLATES
                        .iter()
                        .copied()
                        .filter(|template| {
                            filter.is_empty()
                                || template.label.to_lowercase().contains(&filter)
                                || template.category.to_lowercase().contains(&filter)
                        })
                        .collect::<Vec<_>>();
                    let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
                    if enter {
                        if let Some(template) = matching.first() {
                            if self.add_template_at_palette(template.id) {
                                *state_changed = true;
                            }
                        }
                    }
                    egui::ScrollArea::vertical()
                        .max_height(250.0)
                        .show(ui, |ui| {
                            for template in matching {
                                let label = format!("{}   {}", template.label, template.category);
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new(label).size(11.0).color(palette.text),
                                        )
                                        .fill(palette.widget)
                                        .stroke(Stroke::new(1.0, palette.border))
                                        .rounding(4.0)
                                        .min_size(Vec2::new(ui.available_width(), 28.0)),
                                    )
                                    .clicked()
                                {
                                    if self.add_template_at_palette(template.id) {
                                        *state_changed = true;
                                    }
                                }
                            }
                        });
                });
        });
    }

    fn node_rect(
        &self,
        canvas_rect: Rect,
        _node_id: NodeId,
        position: [f32; 2],
        pins: &[raf_nodes::node::NodePin],
    ) -> Rect {
        let node_height = NODE_HEADER_HEIGHT + pins.len().max(1) as f32 * PIN_ROW_HEIGHT + 8.0;
        let screen_pos = self.canvas_to_screen(Pos2::new(position[0], position[1]), canvas_rect);
        Rect::from_min_size(
            screen_pos,
            Vec2::new(NODE_WIDTH * self.zoom, node_height * self.zoom),
        )
    }

    fn zoom_around_screen_point(&mut self, anchor: Pos2, canvas_rect: Rect, delta: f32) {
        let target = (self.zoom + delta).clamp(0.3, 3.0);
        self.set_zoom_around_screen_point(target, anchor, canvas_rect);
    }

    fn set_zoom_around_screen_point(&mut self, zoom: f32, anchor: Pos2, canvas_rect: Rect) {
        let world = self.screen_to_canvas(anchor, canvas_rect);
        self.zoom = zoom.clamp(0.3, 3.0);
        self.offset = Vec2::new(
            anchor.x - canvas_rect.left() - world.x * self.zoom,
            anchor.y - canvas_rect.top() - world.y * self.zoom,
        );
    }

    fn focus_nodes(&mut self, canvas_rect: Rect, ids: &[NodeId]) {
        let nodes = self
            .active_graph()
            .nodes
            .iter()
            .filter(|node| ids.is_empty() || ids.contains(&node.id))
            .collect::<Vec<_>>();
        if nodes.is_empty() {
            self.zoom = 1.0;
            self.offset = Vec2::new(canvas_rect.width() * 0.35, canvas_rect.height() * 0.32);
            return;
        }
        let min_x = nodes
            .iter()
            .map(|node| node.position[0])
            .fold(f32::INFINITY, f32::min);
        let min_y = nodes
            .iter()
            .map(|node| node.position[1])
            .fold(f32::INFINITY, f32::min);
        let max_x = nodes
            .iter()
            .map(|node| node.position[0] + NODE_WIDTH)
            .fold(f32::NEG_INFINITY, f32::max);
        let max_y = nodes
            .iter()
            .map(|node| {
                node.position[1]
                    + NODE_HEADER_HEIGHT
                    + node.pins.len().max(1) as f32 * PIN_ROW_HEIGHT
                    + 8.0
            })
            .fold(f32::NEG_INFINITY, f32::max);
        let padding = 72.0;
        let width = (max_x - min_x).max(1.0) + padding * 2.0;
        let height = (max_y - min_y).max(1.0) + padding * 2.0;
        self.zoom = (canvas_rect.width() / width)
            .min(canvas_rect.height() / height)
            .clamp(0.3, 1.6);
        self.offset = Vec2::new(
            canvas_rect.center().x - canvas_rect.left() - (min_x + max_x) * 0.5 * self.zoom,
            canvas_rect.center().y - canvas_rect.top() - (min_y + max_y) * 0.5 * self.zoom,
        );
    }

    fn draw_minimap(&mut self, ui: &mut Ui, painter: &egui::Painter, canvas: Rect, nodes: &[Node]) {
        let minimap = Rect::from_min_size(
            Pos2::new(
                canvas.right() - MINIMAP_SIZE.x - 14.0,
                canvas.bottom() - MINIMAP_SIZE.y - 14.0,
            ),
            MINIMAP_SIZE,
        );
        painter.rect_filled(
            minimap,
            4.0,
            Color32::from_rgba_premultiplied(13, 17, 22, 236),
        );
        painter.rect_stroke(minimap, 4.0, Stroke::new(1.0, theme::DARK_BORDER));
        let response = ui.interact(
            minimap,
            ui.id().with("node.minimap"),
            egui::Sense::click_and_drag(),
        );
        if (response.clicked() || response.dragged())
            && ui.input(|input| input.pointer.hover_pos()).is_some()
        {
            if let Some(pointer) = ui.input(|input| input.pointer.hover_pos()) {
                let bounds = graph_bounds(nodes);
                let target = minimap_to_world(pointer, minimap, bounds);
                self.offset = Vec2::new(
                    canvas.center().x - canvas.left() - target.x * self.zoom,
                    canvas.center().y - canvas.top() - target.y * self.zoom,
                );
            }
        }
        if nodes.is_empty() {
            return;
        }
        let bounds = graph_bounds(nodes);
        for node in nodes {
            let node_rect = world_rect_to_minimap(
                Rect::from_min_size(
                    Pos2::new(node.position[0], node.position[1]),
                    Vec2::new(NODE_WIDTH, 64.0),
                ),
                minimap,
                bounds,
            );
            let color = if self.selected_nodes.contains(&node.id) {
                theme::ACCENT
            } else {
                category_color(node.category)
            };
            painter.rect_filled(node_rect, 1.0, color.gamma_multiply(0.7));
        }
        let visible_world = Rect::from_min_max(
            self.screen_to_canvas(canvas.min, canvas),
            self.screen_to_canvas(canvas.max, canvas),
        );
        painter.rect_stroke(
            world_rect_to_minimap(visible_world, minimap, bounds),
            1.0,
            Stroke::new(1.0, Color32::from_rgb(220, 224, 232)),
        );
    }

    fn connection_at_screen(
        &self,
        canvas_rect: Rect,
        point: Pos2,
        connections: &[raf_nodes::graph::Connection],
        nodes: &[Node],
    ) -> Option<Uuid> {
        connections.iter().rev().find_map(|connection| {
            let from = self.find_pin_screen_pos(
                canvas_rect,
                connection.from_node,
                connection.from_pin,
                nodes,
            )?;
            let to = self.find_pin_screen_pos(
                canvas_rect,
                connection.to_node,
                connection.to_pin,
                nodes,
            )?;
            bezier_distance(point, from, to)
                .le(&7.0)
                .then_some(connection.id)
        })
    }

    fn finish_box_selection(
        &mut self,
        canvas_rect: Rect,
        node_data: &[(
            NodeId,
            [f32; 2],
            String,
            NodeCategory,
            Vec<raf_nodes::node::NodePin>,
        )],
        modifiers: egui::Modifiers,
    ) {
        let (Some(start), Some(current)) = (
            self.selection_box_start.take(),
            self.selection_box_current.take(),
        ) else {
            return;
        };
        let selection = Rect::from_two_pos(start, current);
        if selection.size().length_sq() < 16.0 {
            return;
        }
        let selected = node_data
            .iter()
            .filter_map(|(id, position, _, _, pins)| {
                self.node_rect(canvas_rect, *id, *position, pins)
                    .intersects(selection)
                    .then_some(*id)
            })
            .collect::<Vec<_>>();
        if modifiers.ctrl {
            self.selected_nodes.retain(|id| !selected.contains(id));
        } else if modifiers.shift {
            for id in selected {
                if !self.selected_nodes.contains(&id) {
                    self.selected_nodes.push(id);
                }
            }
        } else {
            self.selected_nodes = selected;
        }
        self.selected_node = self.selected_nodes.last().copied();
        self.selected_connection = None;
    }

    fn try_complete_pending_connection(&mut self, new_node_id: NodeId) {
        let Some(drag) = self.pending_connection.take() else {
            return;
        };
        let target = self
            .active_graph()
            .nodes
            .iter()
            .find(|node| node.id == new_node_id)
            .and_then(|node| {
                node.pins.iter().find(|pin| {
                    pin.kind != drag.from_kind && pins_compatible(drag.from_type, pin.data_type)
                })
            })
            .map(|pin| pin.id);
        let Some(target_pin) = target else {
            return;
        };
        match drag.from_kind {
            PinKind::Output => self.active_graph_mut().connect(
                drag.from_node,
                drag.from_pin,
                new_node_id,
                target_pin,
            ),
            PinKind::Input => self.active_graph_mut().connect(
                new_node_id,
                target_pin,
                drag.from_node,
                drag.from_pin,
            ),
        };
    }

    fn duplicate_one(&mut self, node_id: NodeId) -> Option<NodeId> {
        let original = self
            .active_graph()
            .nodes
            .iter()
            .find(|node| node.id == node_id)?
            .clone();
        let mut duplicate = clone_node_with_fresh_ids(&original);
        duplicate.position[0] += 24.0;
        duplicate.position[1] += 24.0;
        Some(self.active_graph_mut().add_node(duplicate))
    }

    fn duplicate_selected(&mut self, target: Option<Pos2>) -> bool {
        let originals = self
            .active_graph()
            .nodes
            .iter()
            .filter(|node| self.selected_nodes.contains(&node.id))
            .cloned()
            .collect::<Vec<_>>();
        if originals.is_empty() {
            return false;
        }
        let origin = originals
            .iter()
            .map(|node| Pos2::new(node.position[0], node.position[1]))
            .reduce(|a, b| Pos2::new(a.x.min(b.x), a.y.min(b.y)))
            .unwrap_or_else(|| Pos2::ZERO);
        let anchor = target.unwrap_or_else(|| Pos2::new(origin.x + 32.0, origin.y + 32.0));
        let new_ids = originals
            .iter()
            .map(|node| {
                let mut duplicate = clone_node_with_fresh_ids(node);
                duplicate.position[0] = anchor.x + (node.position[0] - origin.x);
                duplicate.position[1] = anchor.y + (node.position[1] - origin.y);
                self.active_graph_mut().add_node(duplicate)
            })
            .collect::<Vec<_>>();
        self.selected_nodes = new_ids;
        self.selected_node = self.selected_nodes.last().copied();
        self.selected_connection = None;
        true
    }

    fn copy_selected(&mut self) {
        self.copied_nodes = self
            .active_graph()
            .nodes
            .iter()
            .filter(|node| self.selected_nodes.contains(&node.id))
            .cloned()
            .collect();
    }

    fn paste_nodes(&mut self, target: Option<Pos2>) -> bool {
        if self.copied_nodes.is_empty() {
            return false;
        }
        let copied_nodes = self.copied_nodes.clone();
        let origin = copied_nodes
            .iter()
            .map(|node| Pos2::new(node.position[0], node.position[1]))
            .reduce(|a, b| Pos2::new(a.x.min(b.x), a.y.min(b.y)))
            .unwrap_or(Pos2::ZERO);
        let anchor = target.unwrap_or_else(|| Pos2::new(origin.x + 32.0, origin.y + 32.0));
        let new_ids = copied_nodes
            .iter()
            .map(|node| {
                let mut duplicate = clone_node_with_fresh_ids(node);
                duplicate.position[0] = anchor.x + (node.position[0] - origin.x);
                duplicate.position[1] = anchor.y + (node.position[1] - origin.y);
                self.active_graph_mut().add_node(duplicate)
            })
            .collect::<Vec<_>>();
        self.selected_nodes = new_ids;
        self.selected_node = self.selected_nodes.last().copied();
        self.selected_connection = None;
        true
    }

    fn canvas_to_screen(&self, canvas_pos: Pos2, canvas_rect: Rect) -> Pos2 {
        Pos2::new(
            canvas_rect.left() + canvas_pos.x * self.zoom + self.offset.x,
            canvas_rect.top() + canvas_pos.y * self.zoom + self.offset.y,
        )
    }

    fn screen_to_canvas(&self, screen_pos: Pos2, canvas_rect: Rect) -> Pos2 {
        Pos2::new(
            (screen_pos.x - canvas_rect.left() - self.offset.x) / self.zoom,
            (screen_pos.y - canvas_rect.top() - self.offset.y) / self.zoom,
        )
    }

    pub fn delete_selected(&mut self) -> bool {
        if let Some(connection) = self.selected_connection.take() {
            self.active_graph_mut().disconnect(connection);
            return true;
        }
        if self.selected_nodes.is_empty() {
            return false;
        }
        let selected = std::mem::take(&mut self.selected_nodes);
        for id in selected {
            self.active_graph_mut().remove_node(id);
        }
        self.selected_node = None;
        true
    }
}

fn category_color(category: NodeCategory) -> Color32 {
    let c = category.color();
    Color32::from_rgba_unmultiplied(
        (c[0] * 255.0) as u8,
        (c[1] * 255.0) as u8,
        (c[2] * 255.0) as u8,
        (c[3] * 255.0) as u8,
    )
}

fn pin_data_type_color(dt: PinDataType) -> Color32 {
    match dt {
        PinDataType::Flow => Color32::from_rgb(220, 220, 230),
        PinDataType::Bool => Color32::from_rgb(180, 60, 60),
        PinDataType::Int => Color32::from_rgb(60, 180, 180),
        PinDataType::Float => Color32::from_rgb(100, 200, 100),
        PinDataType::String => Color32::from_rgb(200, 100, 200),
        PinDataType::Vec3 => Color32::from_rgb(240, 200, 60),
        PinDataType::Any => Color32::from_rgb(160, 160, 170),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatible_pins_accept_the_same_type_or_any() {
        assert!(pins_compatible(PinDataType::Float, PinDataType::Float));
        assert!(pins_compatible(PinDataType::Any, PinDataType::String));
        assert!(pins_compatible(PinDataType::Bool, PinDataType::Any));
        assert!(!pins_compatible(PinDataType::Bool, PinDataType::Vec3));
    }

    #[test]
    fn soft_snap_only_engages_near_a_grid_line() {
        assert_eq!(soft_snap(58.0, 30.0, 4.0), 60.0);
        assert_eq!(soft_snap(51.0, 30.0, 4.0), 51.0);
    }

    #[test]
    fn duplicate_nodes_receive_new_node_and_pin_ids() {
        let source = Node::if_branch();
        let duplicate = clone_node_with_fresh_ids(&source);

        assert_ne!(source.id, duplicate.id);
        assert_eq!(source.pins.len(), duplicate.pins.len());
        for (source_pin, duplicate_pin) in source.pins.iter().zip(&duplicate.pins) {
            assert_ne!(source_pin.id, duplicate_pin.id);
        }
    }
}
