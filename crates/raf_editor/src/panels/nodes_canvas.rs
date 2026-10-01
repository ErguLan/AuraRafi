//! Geometry and retained paint helpers for the Nodes canvas.
//!
//! Emits clean, modern AAA visual scripting card geometries with smooth Bezier
//! connections, crisp pin alignment, and responsive zoom scaling.

use crate::panels::nodes_surface_host::NodesWireDrag;
use raf_nodes::{Node, NodeGraph, NodePin, PinDataType, PinKind};
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiEventBinding, UiEventKind, UiIcon, UiIconId, UiIconSize, UiLayout, UiNode,
    UiNodeKind, UiRect, UiStyle,
};

/// Width of a node card at 100% zoom.
pub const NODE_WIDTH: f32 = 180.0;
/// Backwards-compatibility alias for older references.
pub const NODE_DIAMETER: f32 = NODE_WIDTH;
pub const NODE_HEADER_HEIGHT: f32 = 26.0;
pub const NODE_PIN_ROW_HEIGHT: f32 = 22.0;
pub const NODE_PADDING_BOTTOM: f32 = 6.0;
pub const NODE_CONNECTOR_SIZE: f32 = 11.0;
pub const NODE_RADIUS: f32 = 6.0;

const GRID_STEP_MINOR: f32 = 24.0;
const GRID_STEP_MAJOR: f32 = 96.0;
/// Upper bound of minor dots so a huge canvas never explodes the retained
/// node count; the step doubles until the budget fits.
const MINOR_DOT_BUDGET: f32 = 1100.0;
const MAJOR_DOT_BUDGET: usize = 48;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeCanvasGeometry {
    pub position: [f32; 2],
    pub size: [f32; 2],
}

pub fn node_geometry(node: &Node, index: usize, zoom: f32) -> NodeCanvasGeometry {
    let zoom = zoom.max(0.55);
    let fallback = [
        40.0 + (index % 3) as f32 * 250.0,
        52.0 + (index / 3) as f32 * 210.0,
    ];
    let position = if node.position == [0.0, 0.0] {
        fallback
    } else {
        node.position
    };
    NodeCanvasGeometry {
        position: [position[0] * zoom, position[1] * zoom],
        size: node_size(node, zoom),
    }
}

pub fn node_size(node: &Node, zoom: f32) -> [f32; 2] {
    let scale = zoom.max(0.55);
    let input_count = node
        .pins
        .iter()
        .filter(|p| matches!(p.kind, PinKind::Input))
        .count();
    let output_count = node
        .pins
        .iter()
        .filter(|p| matches!(p.kind, PinKind::Output))
        .count();
    let max_rows = input_count.max(output_count).max(1);
    let width = NODE_WIDTH * scale;
    let height =
        (NODE_HEADER_HEIGHT + max_rows as f32 * NODE_PIN_ROW_HEIGHT + NODE_PADDING_BOTTOM) * scale;
    [width, height]
}

/// Extent of the authored graph in world units, ignoring the viewport. Used to
/// frame the graph (`fit`) without ever growing the canvas surface.
pub fn graph_extent(graph: &NodeGraph) -> [f32; 2] {
    let mut width: f32 = 0.0;
    let mut height: f32 = 0.0;
    for (index, node) in graph.nodes.iter().enumerate() {
        let geometry = node_geometry(node, index, 1.0);
        width = width.max(geometry.position[0] + geometry.size[0]);
        height = height.max(geometry.position[1] + geometry.size[1]);
    }
    [width, height]
}

/// Bounding box of every node in world units, including the origin.
pub fn graph_bounds(graph: &NodeGraph) -> [f32; 2] {
    let mut min = [f32::MAX, f32::MAX];
    let mut max = [f32::MIN, f32::MIN];
    for (index, node) in graph.nodes.iter().enumerate() {
        let geometry = node_geometry(node, index, 1.0);
        min[0] = min[0].min(geometry.position[0]);
        min[1] = min[1].min(geometry.position[1]);
        max[0] = max[0].max(geometry.position[0] + geometry.size[0]);
        max[1] = max[1].max(geometry.position[1] + geometry.size[1]);
    }
    if min[0] > max[0] {
        return [0.0, 0.0];
    }
    [max[0] - min[0], max[1] - min[1]]
}

pub fn build_grid(palette: StudioUiPalette, size: [f32; 2]) -> Vec<UiNode> {
    let tokens = palette.tokens();
    let mut dots = Vec::new();

    // Minor dots: engine-editor style point grid instead of full lines. The
    // step doubles on large canvases to keep the dot count bounded.
    let mut minor_step = GRID_STEP_MINOR;
    while minor_step < GRID_STEP_MAJOR
        && (size[0] / minor_step) * (size[1] / minor_step) > MINOR_DOT_BUDGET
    {
        minor_step *= 2.0;
    }
    if minor_step < GRID_STEP_MAJOR {
        let columns = (size[0] / minor_step).floor() as usize;
        let rows = (size[1] / minor_step).floor() as usize;
        for column in 0..=columns {
            let x = column as f32 * minor_step;
            if (x % GRID_STEP_MAJOR).abs() <= f32::EPSILON {
                continue;
            }
            for row in 0..=rows {
                let y = row as f32 * minor_step;
                if (y % GRID_STEP_MAJOR).abs() <= f32::EPSILON {
                    continue;
                }
                dots.push(quad(
                    format!("nodes.canvas.grid.dot.{column}.{row}"),
                    UiRect::new(x - 1.0, y - 1.0, 2.0, 2.0),
                    [tokens.border[0], tokens.border[1], tokens.border[2], 34],
                    -25,
                ));
            }
        }
    }

    // Major anchors on the 96px intersections carry the hierarchy.
    let major_columns = ((size[0] / GRID_STEP_MAJOR).floor() as usize).min(MAJOR_DOT_BUDGET);
    let major_rows = ((size[1] / GRID_STEP_MAJOR).floor() as usize).min(MAJOR_DOT_BUDGET);
    for column in 0..=major_columns {
        let x = column as f32 * GRID_STEP_MAJOR;
        for row in 0..=major_rows {
            let y = row as f32 * GRID_STEP_MAJOR;
            dots.push(quad(
                format!("nodes.canvas.grid.major.{column}.{row}"),
                UiRect::new(x - 1.5, y - 1.5, 3.0, 3.0),
                [tokens.border[0], tokens.border[1], tokens.border[2], 90],
                -20,
            ));
        }
    }

    dots
}

/// Z-order contract between cards, wires and pins on the Nodes canvas.
/// Cards paint first so the canvas never shows a growing background plate,
/// then wires, then connectors on top of both.
pub const Z_CARD: i16 = 0;
pub const Z_WIRE_HALO: i16 = 9;
pub const Z_WIRE: i16 = 10;
pub const Z_WIRE_KNOT: i16 = 13;
pub const Z_PIN_LABEL: i16 = 21;
pub const Z_PIN_DOT: i16 = 22;

pub fn build_connections(
    palette: StudioUiPalette,
    graph: &NodeGraph,
    zoom: f32,
    pan: [f32; 2],
    viewport: [f32; 2],
    wire_drag: Option<&NodesWireDrag>,
) -> Vec<UiNode> {
    let tokens = palette.tokens();
    let mut result = Vec::new();

    // 1. Persisted connections
    for connection in &graph.connections {
        let Some(source) = graph.node(connection.from_node) else {
            continue;
        };
        let Some(target) = graph.node(connection.to_node) else {
            continue;
        };
        let source_index = graph
            .nodes
            .iter()
            .position(|node| node.id == source.id)
            .unwrap_or_default();
        let target_index = graph
            .nodes
            .iter()
            .position(|node| node.id == target.id)
            .unwrap_or_default();
        let Some(source_pin) = source.pins.iter().find(|pin| pin.id == connection.from_pin) else {
            continue;
        };
        let Some(target_pin) = target.pins.iter().find(|pin| pin.id == connection.to_pin) else {
            continue;
        };
        let from = pin_point(source, source_index, source_pin, zoom, pan, true);
        let to = pin_point(target, target_index, target_pin, zoom, pan, false);
        let color = pin_color(source_pin.data_type, tokens.accent);
        let id = connection.id.to_string();

        let p0 = from;
        let p3 = to;
        let dx = (p3[0] - p0[0]).abs().max(40.0 * zoom) * 0.5;
        let p1 = [p0[0] + dx, p0[1]];
        let p2 = [p3[0] - dx, p3[1]];
        let mut mid_pt = [0.0, 0.0];

        // RafUI has no line primitive, so a wire is painted as a brush run:
        // small overlapping squares walked along the curve. Drawing each bezier
        // segment as its bounding box instead is what produced the chain of
        // blocks, because a diagonal segment is a filled rectangle, not a line.
        //
        // The brush is wider than the sample spacing even at the sample cap, so
        // consecutive squares always fuse and never show a seam.
        const WIRE_THICKNESS: f32 = 2.5;
        const WIRE_SAMPLE_SPACING: f32 = 1.2;
        const WIRE_MAX_SAMPLES: usize = 240;

        // Cheap bounds first: a wire outside the viewport costs nothing.
        let bounds_min_x = p0[0].min(p1[0]).min(p2[0]).min(p3[0]);
        let bounds_max_x = p0[0].max(p1[0]).max(p2[0]).max(p3[0]);
        let bounds_min_y = p0[1].min(p1[1]).min(p2[1]).min(p3[1]);
        let bounds_max_y = p0[1].max(p1[1]).max(p2[1]).max(p3[1]);
        let visible = bounds_max_x >= 0.0
            && bounds_min_x <= viewport[0]
            && bounds_max_y >= 0.0
            && bounds_min_y <= viewport[1];

        if visible {
            // Sample count follows the real curve length, so short hops stay
            // cheap and long sweeps stay smooth.
            let mut length = 0.0;
            let mut previous = p0;
            const PROBE: usize = 12;
            for step in 1..=PROBE {
                let point = eval_bezier(p0, p1, p2, p3, step as f32 / PROBE as f32);
                length +=
                    ((point[0] - previous[0]).powi(2) + (point[1] - previous[1]).powi(2)).sqrt();
                previous = point;
            }
            let samples =
                ((length / WIRE_SAMPLE_SPACING).ceil() as usize).clamp(6, WIRE_MAX_SAMPLES);
            let half = WIRE_THICKNESS * 0.5;
            for step in 0..=samples {
                let t = step as f32 / samples as f32;
                let point = eval_bezier(p0, p1, p2, p3, t);
                // Overlap the brush by half so consecutive squares fuse.
                result.push(quad(
                    format!("nodes.connection.{id}.dot.{step}"),
                    UiRect::new(
                        point[0] - half,
                        point[1] - half,
                        WIRE_THICKNESS,
                        WIRE_THICKNESS,
                    ),
                    color,
                    Z_WIRE,
                ));
                if step == samples / 2 {
                    mid_pt = point;
                }
            }
        } else {
            mid_pt = [(p0[0] + p3[0]) * 0.5, (p0[1] + p3[1]) * 0.5];
        }

        // Disconnect affordance. It sits on the wire, so it stays small and
        // translucent until the pointer reaches it.
        let knot_size = 11.0 * zoom.clamp(0.8, 1.2);
        let knot = UiNode::new(
            format!("nodes.connection.{id}.knot"),
            UiNodeKind::Button,
        )
        .with_class("nodes-connection-knot")
        .with_layout(
            UiLayout::absolute(UiRect::new(
                mid_pt[0] - knot_size * 0.5,
                mid_pt[1] - knot_size * 0.5,
                knot_size,
                knot_size,
            ))
            .with_z_index(Z_WIRE_KNOT),
        )
        .with_style(UiStyle {
            fill: [tokens.background[0], tokens.background[1], tokens.background[2], 190],
            border: color,
            text: tokens.text,
            border_width: 1.0,
            radius: knot_size * 0.5,
            opacity: 0.45,
        })
        .with_icon(
            UiIcon::new(UiIconId::Close)
                .with_size(UiIconSize::Custom((knot_size * 0.6) as u16))
                .with_tint(color),
        )
        .with_tooltip_value("Click to disconnect wire")
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            format!("nodes.disconnect.{id}"),
        ));
        result.push(knot);
    }

    // 2. In-flight elastic cable during drag or pending connection
    if let Some(drag) = wire_drag {
        if let Some(source) = graph.node(drag.from_node) {
            if let Some(source_pin) = source.pins.iter().find(|p| p.id == drag.from_pin) {
                let source_index = graph
                    .nodes
                    .iter()
                    .position(|node| node.id == source.id)
                    .unwrap_or_default();
                let from = pin_point(source, source_index, source_pin, zoom, pan, drag.is_output);
                let to = drag.pointer_canvas;
                let color = pin_color(source_pin.data_type, tokens.accent_hot);

                let p0 = from;
                let p3 = to;
                let dx = (p3[0] - p0[0]).abs().max(40.0 * zoom) * 0.5;
                let (p1, p2) = if drag.is_output {
                    ([p0[0] + dx, p0[1]], [p3[0] - dx, p3[1]])
                } else {
                    ([p0[0] - dx, p0[1]], [p3[0] + dx, p3[1]])
                };

                // Same brush technique as the persisted wires: sample spacing
                // stays under the brush thickness so the run has no seams.
                const DRAG_THICKNESS: f32 = 2.5;
                let mut length = 0.0;
                let mut previous = p0;
                const PROBE: usize = 8;
                for step in 1..=PROBE {
                    let point = eval_bezier(p0, p1, p2, p3, step as f32 / PROBE as f32);
                    length +=
                        ((point[0] - previous[0]).powi(2) + (point[1] - previous[1]).powi(2)).sqrt();
                    previous = point;
                }
                let samples = ((length / (DRAG_THICKNESS * 0.6)).ceil() as usize).clamp(6, 220);
                let half = DRAG_THICKNESS * 0.5;
                for step in 0..=samples {
                    let point = eval_bezier(p0, p1, p2, p3, step as f32 / samples as f32);
                    result.push(quad(
                        format!("nodes.wire.drag.dot.{step}"),
                        UiRect::new(
                            point[0] - half,
                            point[1] - half,
                            DRAG_THICKNESS,
                            DRAG_THICKNESS,
                        ),
                        color,
                        Z_WIRE + 1,
                    ));
                }

                // Glowing cursor tip
                let tip_size = 12.0 * zoom.clamp(0.8, 1.2);
                result.push(
                    UiNode::new("nodes.wire.drag.tip", UiNodeKind::Panel)
                        .with_layout(UiLayout::absolute(UiRect::new(
                            to[0] - tip_size * 0.5,
                            to[1] - tip_size * 0.5,
                            tip_size,
                            tip_size,
                        )).with_z_index(Z_WIRE_KNOT))
                        .with_style(UiStyle {
                            fill: color,
                            border: [255, 255, 255, 255],
                            text: tokens.text,
                            border_width: 2.0,
                            radius: tip_size * 0.5,
                            opacity: 1.0,
                        }),
                );
            }
        }
    }

    result
}

fn eval_bezier(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], p3: [f32; 2], t: f32) -> [f32; 2] {
    let u = 1.0 - t;
    let tt = t * t;
    let uu = u * u;
    let uuu = uu * u;
    let ttt = tt * t;

    let x = uuu * p0[0] + 3.0 * uu * t * p1[0] + 3.0 * u * tt * p2[0] + ttt * p3[0];
    let y = uuu * p0[1] + 3.0 * uu * t * p1[1] + 3.0 * u * tt * p2[1] + ttt * p3[1];
    [x, y]
}

pub fn pin_point(
    node: &Node,
    index: usize,
    pin: &NodePin,
    zoom: f32,
    pan: [f32; 2],
    output: bool,
) -> [f32; 2] {
    let geometry = node_geometry(node, index, zoom);
    let rect = pin_rect(node, pin, zoom);
    [
        if output {
            geometry.position[0] + geometry.size[0]
        } else {
            geometry.position[0]
        } - pan[0],
        geometry.position[1] + rect.y + rect.height * 0.5 - pan[1],
    ]
}

/// Returns the local connector hit/draw rectangle for a pin.
pub fn pin_rect(node: &Node, pin: &NodePin, zoom: f32) -> UiRect {
    let scale = zoom.max(0.55);
    let size = node_size(node, zoom);
    let connector_size = NODE_CONNECTOR_SIZE * scale;
    let output = matches!(pin.kind, PinKind::Output);
    let row_index = node
        .pins
        .iter()
        .filter(|candidate| matches!(candidate.kind, PinKind::Output) == output)
        .position(|candidate| candidate.id == pin.id)
        .unwrap_or_default();
    let row_y =
        (NODE_HEADER_HEIGHT + row_index as f32 * NODE_PIN_ROW_HEIGHT + NODE_PIN_ROW_HEIGHT * 0.5)
            * scale;
    let x = if output {
        size[0] - connector_size * 0.5
    } else {
        -connector_size * 0.5
    };
    UiRect::new(
        x,
        row_y - connector_size * 0.5,
        connector_size,
        connector_size,
    )
}

pub fn pin_color(data_type: PinDataType, accent: [u8; 4]) -> [u8; 4] {
    match data_type {
        PinDataType::Flow => [255, 255, 255, 255],     // Execution flow: solid white
        PinDataType::Bool => [132, 204, 22, 255],      // Boolean: vibrant lime
        PinDataType::Int => [6, 182, 212, 255],        // Integer: electric cyan
        PinDataType::Float => [14, 165, 233, 255],     // Float: sky cyan
        PinDataType::String => [245, 158, 11, 255],    // String: golden amber
        PinDataType::Vec3 => [249, 115, 22, 255],      // Vec3: bright coral/orange
        PinDataType::Any => accent,
    }
}

fn quad(id: String, rect: UiRect, fill: [u8; 4], z_index: i16) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_layout(UiLayout::absolute(rect).with_z_index(z_index))
        .with_style(UiStyle {
            fill,
            border: [0, 0, 0, 0],
            text: [255, 255, 255, 255],
            border_width: 0.0,
            radius: 0.0,
            opacity: 1.0,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nodes_keep_connector_dots_on_their_edges() {
        let node = Node::print_action();
        let input = node
            .pins
            .iter()
            .find(|pin| matches!(pin.kind, PinKind::Input))
            .expect("Print must expose an input pin");
        let output = node
            .pins
            .iter()
            .find(|pin| matches!(pin.kind, PinKind::Output))
            .expect("Print must expose an output pin");
        let geometry = node_geometry(&node, 0, 1.0);
        let size = node_size(&node, 1.0);
        let input_point = pin_point(&node, 0, input, 1.0, [0.0, 0.0], false);
        let output_point = pin_point(&node, 0, output, 1.0, [0.0, 0.0], true);

        assert_eq!(input_point[0], geometry.position[0]);
        assert_eq!(output_point[0], geometry.position[0] + size[0]);
        assert!(input_point[1] > geometry.position[1]);
        assert!(input_point[1] < geometry.position[1] + size[1]);
        assert!(output_point[1] > geometry.position[1]);
        assert!(output_point[1] < geometry.position[1] + size[1]);
    }

    #[test]
    fn dot_grid_stays_bounded_as_the_canvas_grows() {
        let palette = StudioUiPalette::IndustrialDark;
        let small = build_grid(palette, [860.0, 560.0]);
        let medium = build_grid(palette, [2000.0, 1200.0]);
        let huge = build_grid(palette, [8000.0, 6000.0]);

        assert!(small.len() <= 600, "default canvas: {}", small.len());
        assert!(medium.len() <= 700, "medium canvas: {}", medium.len());
        assert!(huge.len() <= 2500, "huge canvas: {}", huge.len());
        assert!(
            small.iter().any(|dot| dot.id.contains("grid.dot")),
            "minor dots render on the default canvas"
        );
        assert!(
            small.iter().any(|dot| dot.id.contains("grid.major")),
            "major anchors render on the default canvas"
        );
    }

    #[test]
    fn wires_paint_above_cards_and_pins_above_wires() {
        assert!(
            Z_WIRE > Z_CARD,
            "wires must paint over cards, otherwise dense graphs hide them"
        );
        assert!(
            Z_PIN_DOT > Z_WIRE,
            "connectors must stay visible on top of the wire they own"
        );
        assert!(Z_PIN_LABEL > Z_WIRE);
    }

    #[test]
    fn a_connected_graph_emits_wires_above_the_card_layer() {
        let mut graph = NodeGraph::new("Main Event Graph");
        let source = graph.add_node(Node::on_start());
        let target = graph.add_node(Node::print_action());
        let from = graph
            .node(source)
            .and_then(|node| {
                node.pins
                    .iter()
                    .find(|pin| matches!(pin.kind, PinKind::Output))
            })
            .map(|pin| pin.id)
            .expect("On Start exposes an output");
        let to = graph
            .node(target)
            .and_then(|node| {
                node.pins
                    .iter()
                    .find(|pin| matches!(pin.kind, PinKind::Input))
            })
            .map(|pin| pin.id)
            .expect("Print exposes an input");
        graph
            .try_connect(source, from, target, to)
            .expect("flow pins connect");

        let nodes = build_connections(
            StudioUiPalette::IndustrialDark,
            &graph,
            1.0,
            [0.0, 0.0],
            [4000.0, 4000.0],
            None,
        );
        assert!(
            nodes.iter().any(|node| node.id.contains(".dot.")),
            "a connected graph must emit a brush run"
        );
        assert!(nodes
            .iter()
            .filter(|node| node.id.contains(".dot."))
            .all(|node| { node.layout.z_index >= Z_WIRE }));
    }

    #[test]
    fn wires_outside_the_viewport_are_skipped() {
        let mut graph = NodeGraph::new("Main Event Graph");
        let source = graph.add_node(Node::on_start());
        let target = graph.add_node(Node::print_action());
        let from = graph
            .node(source)
            .and_then(|node| {
                node.pins
                    .iter()
                    .find(|pin| matches!(pin.kind, PinKind::Output))
            })
            .map(|pin| pin.id)
            .expect("On Start exposes an output");
        let to = graph
            .node(target)
            .and_then(|node| {
                node.pins
                    .iter()
                    .find(|pin| matches!(pin.kind, PinKind::Input))
            })
            .map(|pin| pin.id)
            .expect("Print exposes an input");
        graph
            .try_connect(source, from, target, to)
            .expect("flow pins connect");

        // Pan the whole graph far away from a small viewport.
        let visible = build_connections(
            StudioUiPalette::IndustrialDark,
            &graph,
            1.0,
            [0.0, 0.0],
            [4000.0, 4000.0],
            None,
        );
        let offscreen = build_connections(
            StudioUiPalette::IndustrialDark,
            &graph,
            1.0,
            [2000.0, 2000.0],
            [400.0, 300.0],
            None,
        );
        assert!(visible.iter().any(|node| node.id.contains(".dot.")));
        assert!(
            !offscreen.iter().any(|node| node.id.contains(".dot.")),
            "wires outside the viewport must not be painted"
        );
    }

    #[test]
    fn panning_shifts_the_whole_graph_without_changing_its_size() {
        let node = Node::print_action();
        let input = node
            .pins
            .iter()
            .find(|pin| matches!(pin.kind, PinKind::Input))
            .expect("Print exposes an input");
        let origin = pin_point(&node, 0, input, 1.0, [0.0, 0.0], false);
        let panned = pin_point(&node, 0, input, 1.0, [120.0, 80.0], false);
        assert_eq!(origin[0] - panned[0], 120.0);
        assert_eq!(origin[1] - panned[1], 80.0);
    }

    #[test]
    fn graph_bounds_cover_every_node() {
        let mut graph = NodeGraph::new("Main Event Graph");
        graph.add_node(Node::on_start());
        graph.add_node(Node::print_action());
        let bounds = graph_bounds(&graph);
        assert!(bounds[0] > 0.0, "bounds must have a real width");
        assert!(bounds[1] > 0.0, "bounds must have a real height");

        let empty = graph_bounds(&NodeGraph::new("Empty"));
        assert_eq!(empty, [0.0, 0.0]);
    }
}
