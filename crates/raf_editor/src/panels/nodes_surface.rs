//! Retained visual scripting workbench for the Game downbar.
//!
//! The node runtime remains in `raf_nodes`. This module restores the old
//! editor's graph, selection, add/remove and graph-switching behavior while
//! presenting it through RafUI instead of the retired Egui panel.

use raf_nodes::node::{PinDataType, PinKind};
use raf_nodes::{Node, NodeCategory, NodeGraph, NodeId};
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAction, UiIcon, UiIconId, UiIconSize, UiSurface,
};
use raf_ui::{
    UiAlign, UiEventBinding, UiEventKind, UiFlow, UiFontWeight, UiLayout, UiNode, UiNodeKind,
    UiOverflow, UiRect, UiScrollAxis, UiSizeMode, UiSpacing, UiStylePatch, UiStyleRule,
    UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiTextRole, UiTextStyle,
};
use serde::{Deserialize, Serialize};

use super::raf_ui_surface_bridge::RafUiSurfaceBridge;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodePreset {
    OnStart,
    OnUpdate,
    Print,
    If,
    Add,
    ForLoop,
    WhileLoop,
    GreaterThan,
    LessThan,
    Equals,
    SpawnEntity,
    DestroyEntity,
    SetPosition,
    KeyPress,
    MouseClick,
    Delay,
    SerialRead,
    SerialWrite,
    ReadSensor,
    WriteActuator,
}

impl NodePreset {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::OnStart => "On Start",
            Self::OnUpdate => "On Update",
            Self::Print => "Print",
            Self::If => "If",
            Self::Add => "Add",
            Self::ForLoop => "For Loop",
            Self::WhileLoop => "While Loop",
            Self::GreaterThan => "Greater Than",
            Self::LessThan => "Less Than",
            Self::Equals => "Equals",
            Self::SpawnEntity => "Spawn Entity",
            Self::DestroyEntity => "Destroy Entity",
            Self::SetPosition => "Set Position",
            Self::KeyPress => "Key Press",
            Self::MouseClick => "Mouse Click",
            Self::Delay => "Delay",
            Self::SerialRead => "Serial Read",
            Self::SerialWrite => "Serial Write",
            Self::ReadSensor => "Read Sensor",
            Self::WriteActuator => "Write Actuator",
        }
    }

    fn label_key(self) -> Option<&'static str> {
        match self {
            Self::OnStart => Some("nodes.preset.on_start"),
            Self::OnUpdate => Some("nodes.preset.on_update"),
            Self::Print => Some("nodes.preset.print"),
            Self::If => Some("nodes.preset.if"),
            Self::Add => Some("nodes.preset.add"),
            Self::ForLoop => Some("nodes.preset.for_loop"),
            Self::WhileLoop => Some("nodes.preset.while_loop"),
            Self::GreaterThan => Some("nodes.preset.greater_than"),
            Self::LessThan => Some("nodes.preset.less_than"),
            Self::Equals => Some("nodes.preset.equals"),
            Self::SpawnEntity => Some("nodes.preset.spawn_entity"),
            Self::DestroyEntity => Some("nodes.preset.destroy_entity"),
            Self::SetPosition => Some("nodes.preset.set_position"),
            Self::KeyPress => Some("nodes.preset.key_press"),
            Self::MouseClick => Some("nodes.preset.mouse_click"),
            Self::Delay => Some("nodes.preset.delay"),
            Self::SerialRead => Some("nodes.preset.serial_read"),
            Self::SerialWrite => Some("nodes.preset.serial_write"),
            Self::ReadSensor => Some("nodes.preset.read_sensor"),
            Self::WriteActuator => Some("nodes.preset.write_actuator"),
        }
    }

    fn command_id(self) -> &'static str {
        match self {
            Self::OnStart => "on-start",
            Self::OnUpdate => "on-update",
            Self::Print => "print",
            Self::If => "if",
            Self::Add => "add",
            Self::ForLoop => "for-loop",
            Self::WhileLoop => "while-loop",
            Self::GreaterThan => "greater-than",
            Self::LessThan => "less-than",
            Self::Equals => "equals",
            Self::SpawnEntity => "spawn-entity",
            Self::DestroyEntity => "destroy-entity",
            Self::SetPosition => "set-position",
            Self::KeyPress => "key-press",
            Self::MouseClick => "mouse-click",
            Self::Delay => "delay",
            Self::SerialRead => "serial-read",
            Self::SerialWrite => "serial-write",
            Self::ReadSensor => "read-sensor",
            Self::WriteActuator => "write-actuator",
        }
    }

    fn category_label(self) -> &'static str {
        match self {
            Self::OnStart | Self::OnUpdate | Self::KeyPress | Self::MouseClick => "Events",
            Self::If | Self::ForLoop | Self::WhileLoop | Self::Delay => "Logic",
            Self::Print | Self::SpawnEntity | Self::DestroyEntity | Self::SetPosition => "Actions",
            Self::Add | Self::GreaterThan | Self::LessThan | Self::Equals => "Math",
            Self::SerialRead | Self::SerialWrite | Self::ReadSensor | Self::WriteActuator => {
                "Electronics"
            }
        }
    }

    fn category(self) -> NodeCategory {
        match self {
            Self::OnStart | Self::OnUpdate | Self::KeyPress | Self::MouseClick => {
                NodeCategory::Event
            }
            Self::If | Self::ForLoop | Self::WhileLoop | Self::Delay => NodeCategory::Logic,
            Self::Print | Self::SpawnEntity | Self::DestroyEntity | Self::SetPosition => {
                NodeCategory::Action
            }
            Self::Add | Self::GreaterThan | Self::LessThan | Self::Equals => NodeCategory::Math,
            Self::SerialRead | Self::SerialWrite | Self::ReadSensor | Self::WriteActuator => {
                NodeCategory::Electronics
            }
        }
    }

    pub fn create(self) -> Node {
        match self {
            Self::OnStart => Node::on_start(),
            Self::OnUpdate => Node::on_update(),
            Self::Print => Node::print_action(),
            Self::If => Node::if_branch(),
            Self::Add => Node::add_math(),
            Self::ForLoop => raf_nodes::flow_nodes::FlowNodes::for_loop(),
            Self::WhileLoop => raf_nodes::flow_nodes::FlowNodes::while_loop(),
            Self::GreaterThan => raf_nodes::math_nodes::MathNodes::compare(">"),
            Self::LessThan => raf_nodes::math_nodes::MathNodes::compare("<"),
            Self::Equals => raf_nodes::math_nodes::MathNodes::compare("=="),
            Self::SpawnEntity => raf_nodes::entity_nodes::EntityNodes::spawn_entity(),
            Self::DestroyEntity => raf_nodes::entity_nodes::EntityNodes::destroy_entity(),
            Self::SetPosition => raf_nodes::entity_nodes::EntityNodes::set_position(),
            Self::KeyPress => raf_nodes::input_nodes::InputNodes::key_press(),
            Self::MouseClick => raf_nodes::input_nodes::InputNodes::mouse_click(),
            Self::Delay => raf_nodes::input_nodes::InputNodes::timer_delay(),
            Self::SerialRead => raf_nodes::hardware_nodes::HardwareNodes::serial_read(),
            Self::SerialWrite => raf_nodes::hardware_nodes::HardwareNodes::serial_write(),
            Self::ReadSensor => raf_nodes::hardware_nodes::HardwareNodes::sensor_input(),
            Self::WriteActuator => raf_nodes::hardware_nodes::HardwareNodes::actuator_output(),
        }
    }
}

const ALL_NODE_PRESETS: &[NodePreset] = &[
    NodePreset::OnStart,
    NodePreset::OnUpdate,
    NodePreset::Print,
    NodePreset::If,
    NodePreset::ForLoop,
    NodePreset::WhileLoop,
    NodePreset::Add,
    NodePreset::GreaterThan,
    NodePreset::LessThan,
    NodePreset::Equals,
    NodePreset::SpawnEntity,
    NodePreset::DestroyEntity,
    NodePreset::SetPosition,
    NodePreset::KeyPress,
    NodePreset::MouseClick,
    NodePreset::Delay,
    NodePreset::SerialRead,
    NodePreset::SerialWrite,
    NodePreset::ReadSensor,
    NodePreset::WriteActuator,
];

const QUICK_NODE_PRESETS: &[NodePreset] = &[
    NodePreset::OnStart,
    NodePreset::OnUpdate,
    NodePreset::Print,
    NodePreset::If,
    NodePreset::Add,
];

#[derive(Debug, Clone, PartialEq)]
pub enum NodesIntent {
    SelectGraph(usize),
    DeleteGraph(usize),
    NewGraph,
    AddNode(NodePreset),
    SelectNode(usize),
    DeleteNode(usize),
    CopyNode(usize),
    PasteNode,
    BeginNodeDrag(usize),
    MoveNode {
        index: usize,
        position: [f32; 2],
    },
    EndNodeDrag,
    SelectPin {
        node_index: usize,
        pin_index: usize,
    },
    ConnectPins {
        from_node: usize,
        from_pin: usize,
        to_node: usize,
        to_pin: usize,
    },
    ZoomIn,
    ZoomOut,
    ResetZoom,
    Undo,
    Redo,
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

pub struct NodesSurfaceHost {
    bridge: RafUiSurfaceBridge,
    selected_node: Option<NodeId>,
    pending_pin: Option<(usize, usize)>,
    palette_filter: String,
    zoom: f32,
    dragging_node: Option<usize>,
    drag_origin_scene: Option<[f32; 2]>,
    drag_origin_node: Option<[f32; 2]>,
    context_menu_position: Option<[f32; 2]>,
    revision: u64,
    cached_key: Option<(
        StudioUiPalette,
        u64,
        usize,
        Option<NodeId>,
        Option<(usize, usize)>,
        String,
        u32,
        Option<(u32, u32)>,
    )>,
    cached_surface: Option<UiSurface>,
}

impl Default for NodesSurfaceHost {
    fn default() -> Self {
        Self {
            bridge: RafUiSurfaceBridge::new("raf_ui_editor_nodes"),
            selected_node: None,
            pending_pin: None,
            palette_filter: String::new(),
            zoom: 1.0,
            dragging_node: None,
            drag_origin_scene: None,
            drag_origin_node: None,
            context_menu_position: None,
            revision: 0,
            cached_key: None,
            cached_surface: None,
        }
    }
}

impl NodesSurfaceHost {
    pub fn reset(&mut self) {
        self.selected_node = None;
        self.pending_pin = None;
        self.palette_filter.clear();
        self.zoom = 1.0;
        self.dragging_node = None;
        self.drag_origin_scene = None;
        self.drag_origin_node = None;
        self.context_menu_position = None;
        self.revision = self.revision.wrapping_add(1);
        self.cached_key = None;
        self.cached_surface = None;
        self.bridge.cancel_pointer_gesture();
    }

    pub fn mark_changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.cached_key = None;
    }

    fn canvas_scene_position(&self, point: [f32; 2]) -> Option<[f32; 2]> {
        let rect = self.bridge.layout_rect("nodes.canvas")?;
        let offset = self
            .bridge
            .with_control_state_read(|controls| controls.scroll_offset("nodes.canvas"))
            .unwrap_or([0.0, 0.0]);
        Some([
            ((point[0] - rect.x + offset[0] - 20.0) / self.zoom).max(-10_000.0),
            ((point[1] - rect.y + offset[1] - 20.0) / self.zoom).max(-10_000.0),
        ])
    }

    pub fn selected_node(&self) -> Option<NodeId> {
        self.selected_node
    }

    pub fn show(
        &mut self,
        ui: &mut eframe::egui::Ui,
        render_state: Option<&eframe::egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        language: raf_core::config::Language,
        document: &NodeEditorDocument,
    ) -> Vec<NodesIntent> {
        let active = document
            .active_graph_index
            .min(document.graphs.len().saturating_sub(1));
        let key = (
            palette,
            self.revision,
            active,
            self.selected_node,
            self.pending_pin,
            self.palette_filter.clone(),
            self.zoom.to_bits(),
            self.context_menu_position
                .map(|position| (position[0].to_bits(), position[1].to_bits())),
        );
        if self.cached_key.as_ref() != Some(&key) {
            self.cached_surface = Some(build_nodes_surface_with_filter(
                palette,
                document,
                self.selected_node,
                self.pending_pin,
                self.zoom,
                self.context_menu_position,
                &self.palette_filter,
            ));
            self.cached_key = Some(key);
        }
        let Some(surface) = self.cached_surface.as_ref() else {
            return Vec::new();
        };
        let palette_filter = self.palette_filter.clone();
        let actions = self.bridge.show_with_control_state_ref(
            ui,
            render_state,
            palette,
            surface,
            |controls| controls.set_text("nodes.palette.search", palette_filter.clone(), 128),
            |key| raf_core::i18n::t(key, language),
        );
        let mut intents = Vec::new();
        for action in actions {
            match action.action {
                UiAction::SetText { key, value } if key == "nodes.palette.search" => {
                    if self.palette_filter != value {
                        self.palette_filter = value;
                        self.revision = self.revision.wrapping_add(1);
                        self.cached_key = None;
                    }
                }
                UiAction::Command { name } => {
                    if let Some(value) = name.strip_prefix("nodes.graph:") {
                        if let Ok(index) = value.parse::<usize>() {
                            self.close_palette();
                            self.selected_node = None;
                            self.pending_pin = None;
                            self.revision = self.revision.wrapping_add(1);
                            self.cached_key = None;
                            intents.push(NodesIntent::SelectGraph(index));
                        }
                    } else if name == "nodes.graph.new" {
                        self.close_palette();
                        intents.push(NodesIntent::NewGraph);
                    } else if let Some(value) = name.strip_prefix("nodes.graph.delete:") {
                        if let Ok(index) = value.parse::<usize>() {
                            self.close_palette();
                            intents.push(NodesIntent::DeleteGraph(index));
                        }
                    } else if name == "nodes.palette.open" {
                        self.context_menu_position = Some(self.palette_anchor());
                        self.palette_filter.clear();
                        self.revision = self.revision.wrapping_add(1);
                        self.cached_key = None;
                        self.bridge.request_focus("nodes.palette.search");
                    } else if name == "nodes.palette.close" {
                        self.close_palette();
                    } else if name == "nodes.palette.clear" {
                        if !self.palette_filter.is_empty() {
                            self.palette_filter.clear();
                            self.revision = self.revision.wrapping_add(1);
                            self.cached_key = None;
                        }
                    } else if let Some(value) = name.strip_prefix("nodes.add:") {
                        self.close_palette();
                        let preset = ALL_NODE_PRESETS
                            .iter()
                            .copied()
                            .find(|preset| preset.command_id() == value);
                        if let Some(preset) = preset {
                            intents.push(NodesIntent::AddNode(preset));
                        }
                    } else if let Some(value) = name.strip_prefix("nodes.select:") {
                        if let Ok(index) = value.parse::<usize>() {
                            self.close_palette();
                            if let Some(node) = document
                                .graphs
                                .get(active)
                                .and_then(|graph| graph.nodes.get(index))
                            {
                                if self.selected_node != Some(node.id) {
                                    self.selected_node = Some(node.id);
                                    self.revision = self.revision.wrapping_add(1);
                                    self.cached_key = None;
                                }
                            }
                            intents.push(NodesIntent::SelectNode(index));
                        }
                    } else if let Some(value) = name.strip_prefix("nodes.delete:") {
                        if let Ok(index) = value.parse::<usize>() {
                            self.close_palette();
                            if selected_index(document, active, self.selected_node) == Some(index) {
                                self.selected_node = None;
                                self.revision = self.revision.wrapping_add(1);
                                self.cached_key = None;
                            }
                            intents.push(NodesIntent::DeleteNode(index));
                        }
                    } else if name == "nodes.delete-selected" {
                        if let Some(index) = selected_index(document, active, self.selected_node) {
                            self.close_palette();
                            intents.push(NodesIntent::DeleteNode(index));
                        }
                    } else if name == "nodes.copy" {
                        if let Some(index) = selected_index(document, active, self.selected_node) {
                            intents.push(NodesIntent::CopyNode(index));
                        }
                    } else if name == "nodes.paste" {
                        intents.push(NodesIntent::PasteNode);
                    } else if name == "nodes.canvas.context" {
                        self.context_menu_position = self
                            .bridge
                            .pointer_position()
                            .map(|position| self.clamp_palette_position(position))
                            .or_else(|| Some(self.palette_anchor()));
                        self.revision = self.revision.wrapping_add(1);
                        self.cached_key = None;
                    } else if name == "nodes.canvas.click" {
                        let selection_changed = self.selected_node.take().is_some();
                        let pin_changed = self.pending_pin.take().is_some();
                        if selection_changed || pin_changed {
                            self.revision = self.revision.wrapping_add(1);
                            self.cached_key = None;
                        }
                        self.close_palette();
                    } else if let Some(value) = name.strip_prefix("nodes.drag.start:") {
                        if let Ok(index) = value.parse::<usize>() {
                            if let Some(node) = document
                                .graphs
                                .get(active)
                                .and_then(|graph| graph.nodes.get(index))
                            {
                                if self.selected_node != Some(node.id) {
                                    self.selected_node = Some(node.id);
                                    self.revision = self.revision.wrapping_add(1);
                                    self.cached_key = None;
                                    intents.push(NodesIntent::SelectNode(index));
                                }
                            }
                            self.dragging_node = Some(index);
                            self.drag_origin_scene = self
                                .bridge
                                .pointer_position()
                                .and_then(|point| self.canvas_scene_position(point));
                            self.drag_origin_node = document
                                .graphs
                                .get(active)
                                .and_then(|graph| graph.nodes.get(index))
                                .map(|node| node.position);
                            intents.push(NodesIntent::BeginNodeDrag(index));
                        }
                    } else if let Some(value) = name.strip_prefix("nodes.drag.move:") {
                        if let Ok(index) = value.parse::<usize>() {
                            if self.dragging_node == Some(index) {
                                if let (Some(origin_scene), Some(origin_node), Some(point)) = (
                                    self.drag_origin_scene,
                                    self.drag_origin_node,
                                    self.bridge.pointer_position(),
                                ) {
                                    if let Some(current_scene) = self.canvas_scene_position(point) {
                                        intents.push(NodesIntent::MoveNode {
                                            index,
                                            position: [
                                                origin_node[0] + current_scene[0] - origin_scene[0],
                                                origin_node[1] + current_scene[1] - origin_scene[1],
                                            ],
                                        });
                                    }
                                }
                            }
                        }
                    } else if name.starts_with("nodes.drag.end:") {
                        self.dragging_node = None;
                        self.drag_origin_scene = None;
                        self.drag_origin_node = None;
                        intents.push(NodesIntent::EndNodeDrag);
                    } else if let Some(value) = name.strip_prefix("nodes.pin:") {
                        let mut parts = value.split(':');
                        if let (Some(node), Some(pin)) = (parts.next(), parts.next()) {
                            if let (Ok(node_index), Ok(pin_index)) = (node.parse(), pin.parse()) {
                                self.close_palette();
                                let node: Option<&Node> = document
                                    .graphs
                                    .get(active)
                                    .and_then(|graph: &NodeGraph| graph.nodes.get(node_index));
                                if let Some(node) = node {
                                    if self.selected_node != Some(node.id) {
                                        self.selected_node = Some(node.id);
                                        self.revision = self.revision.wrapping_add(1);
                                        self.cached_key = None;
                                        intents.push(NodesIntent::SelectNode(node_index));
                                    }
                                }
                                if let Some((from_node, from_pin)) = self.pending_pin.take() {
                                    self.revision = self.revision.wrapping_add(1);
                                    self.cached_key = None;
                                    if from_node != node_index || from_pin != pin_index {
                                        intents.push(NodesIntent::ConnectPins {
                                            from_node,
                                            from_pin,
                                            to_node: node_index,
                                            to_pin: pin_index,
                                        });
                                    }
                                } else {
                                    self.pending_pin = Some((node_index, pin_index));
                                    self.revision = self.revision.wrapping_add(1);
                                    self.cached_key = None;
                                    intents.push(NodesIntent::SelectPin {
                                        node_index,
                                        pin_index,
                                    });
                                }
                            }
                        }
                    } else if name == "nodes.zoom.in" {
                        self.zoom = (self.zoom * 1.12).clamp(0.55, 1.8);
                        self.revision = self.revision.wrapping_add(1);
                        self.cached_key = None;
                        intents.push(NodesIntent::ZoomIn);
                    } else if name == "nodes.zoom.out" {
                        self.zoom = (self.zoom / 1.12).clamp(0.55, 1.8);
                        self.revision = self.revision.wrapping_add(1);
                        self.cached_key = None;
                        intents.push(NodesIntent::ZoomOut);
                    } else if name == "nodes.zoom.reset" {
                        self.zoom = 1.0;
                        self.revision = self.revision.wrapping_add(1);
                        self.cached_key = None;
                        intents.push(NodesIntent::ResetZoom);
                    } else if name == "nodes.undo" {
                        intents.push(NodesIntent::Undo);
                    } else if name == "nodes.redo" {
                        intents.push(NodesIntent::Redo);
                    }
                }
                _ => {}
            }
        }
        let input = self.bridge.input_snapshot().clone();
        if input.modifiers.control && input.key_pressed("z") {
            intents.push(if input.modifiers.shift {
                NodesIntent::Redo
            } else {
                NodesIntent::Undo
            });
        } else if input.modifiers.control && input.key_pressed("y") {
            intents.push(NodesIntent::Redo);
        }
        if self.context_menu_position.is_none() && input.modifiers.control {
            if input.key_pressed("c") {
                if let Some(index) = selected_index(document, active, self.selected_node) {
                    intents.push(NodesIntent::CopyNode(index));
                }
            } else if input.key_pressed("v") {
                intents.push(NodesIntent::PasteNode);
            }
        }
        if input.button_down(raf_ui::UiPointerButton::Middle) && input.pointer_delta != [0.0, 0.0] {
            self.bridge.scroll_by(
                "nodes.canvas",
                [-input.pointer_delta[0], -input.pointer_delta[1]],
            );
            self.bridge.invalidate_render();
        }
        intents
    }

    fn close_palette(&mut self) {
        if self.context_menu_position.take().is_some() || !self.palette_filter.is_empty() {
            self.palette_filter.clear();
            self.revision = self.revision.wrapping_add(1);
            self.cached_key = None;
        }
    }

    fn palette_anchor(&self) -> [f32; 2] {
        let Some(canvas) = self.bridge.layout_rect("nodes.canvas") else {
            return [32.0, 32.0];
        };
        self.clamp_palette_position([
            canvas.x + canvas.width * 0.5 - 126.0,
            canvas.y + canvas.height * 0.5 - 156.0,
        ])
    }

    fn clamp_palette_position(&self, position: [f32; 2]) -> [f32; 2] {
        let Some(canvas) = self.bridge.layout_rect("nodes.canvas") else {
            return position;
        };
        let bounds = UiRect::new(
            canvas.x + 8.0,
            canvas.y + 8.0,
            (canvas.width - 16.0).max(0.0),
            (canvas.height - 16.0).max(0.0),
        );
        let menu = UiRect::new(position[0], position[1], 252.0, 312.0).clamp_inside(bounds);
        [menu.x, menu.y]
    }

    pub fn apply_selection(&mut self, selected: Option<NodeId>) {
        if self.selected_node != selected {
            self.selected_node = selected;
            self.revision = self.revision.wrapping_add(1);
            self.cached_key = None;
        }
    }
}

pub fn build_nodes_surface(
    palette: StudioUiPalette,
    document: &NodeEditorDocument,
    selected_node: Option<NodeId>,
    pending_pin: Option<(usize, usize)>,
    zoom: f32,
    context_menu_position: Option<[f32; 2]>,
) -> UiSurface {
    build_nodes_surface_with_filter(
        palette,
        document,
        selected_node,
        pending_pin,
        zoom,
        context_menu_position,
        "",
    )
}

fn build_nodes_surface_with_filter(
    palette: StudioUiPalette,
    document: &NodeEditorDocument,
    selected_node: Option<NodeId>,
    pending_pin: Option<(usize, usize)>,
    zoom: f32,
    context_menu_position: Option<[f32; 2]>,
    palette_filter: &str,
) -> UiSurface {
    let tokens = palette.tokens();
    let active = document
        .active_graph_index
        .min(document.graphs.len().saturating_sub(1));
    let graph = document.graphs.get(active);
    let canvas_size = canvas_dimensions(graph, zoom);

    let mut graph_list = UiNode::scroll_view("nodes.graphs", UiScrollAxis::Vertical)
        .with_class("nodes-graph-list")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 3.0,
            padding: UiSpacing::xy(6.0, 6.0),
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        });
    for (index, item) in document.graphs.iter().enumerate() {
        graph_list = graph_list.with_child(
            UiNode::new(format!("nodes.graph-row.{index}"), UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 6.0,
                    padding: UiSpacing::xy(2.0, 2.0),
                    align_self: Some(UiAlign::Stretch),
                    ..UiLayout::fixed(0.0, 30.0)
                        .with_width_mode(UiSizeMode::Fill)
                        .with_z_index(1)
                })
                .with_child(
                    UiNode::new(format!("nodes.graph.{index}"), UiNodeKind::Button)
                        .with_class("nodes-graph")
                        .with_class(if index == active {
                            "nodes-graph-active"
                        } else {
                            ""
                        })
                        .with_icon(UiIcon::new(UiIconId::Node).with_size(UiIconSize::Small))
                        .with_text_value(item.name.clone())
                        .with_layout(UiLayout {
                            grow: 1.0,
                            ..UiLayout::fixed(0.0, 26.0).with_width_mode(UiSizeMode::Fill)
                        })
                        .with_text_style(UiTextStyle::button(if index == active {
                            tokens.text
                        } else {
                            tokens.text_muted
                        }))
                        .with_accessibility_label_key("nodes.graphs")
                        .focusable()
                        .with_event(UiEventBinding::command(
                            UiEventKind::Click,
                            format!("nodes.graph:{index}"),
                        )),
                )
                .with_child(
                    UiNode::new(format!("nodes.graph-delete.{index}"), UiNodeKind::Button)
                        .with_class("nodes-graph-delete")
                        .with_icon(UiIcon::new(UiIconId::Close).with_size(UiIconSize::Small))
                        .with_tooltip_key("nodes.delete_graph")
                        .with_accessibility_label_key("nodes.delete_graph")
                        .disabled(document.graphs.len() <= 1)
                        .with_layout(UiLayout::fixed(24.0, 24.0).with_z_index(1))
                        .focusable()
                        .with_event(UiEventBinding::command(
                            UiEventKind::Click,
                            format!("nodes.graph.delete:{index}"),
                        )),
                ),
        );
    }
    graph_list = graph_list.with_child(
        UiNode::new("nodes.graph.new", UiNodeKind::Button)
            .with_class("nodes-new-graph")
            .with_icon(UiIcon::new(UiIconId::Add).with_size(UiIconSize::Small))
            .with_text_key("nodes.new_graph")
            .with_layout(
                UiLayout::fixed(0.0, 30.0)
                    .with_width_mode(UiSizeMode::Fill)
                    .with_z_index(1),
            )
            .with_accessibility_label_key("nodes.new_graph")
            .with_text_style(UiTextStyle::button(tokens.accent_hot))
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                "nodes.graph.new",
            )),
    );

    let mut node_list = UiNode::scroll_view("nodes.canvas", UiScrollAxis::Both)
        .with_class("nodes-canvas")
        .with_layout(UiLayout {
            flow: UiFlow::None,
            padding: UiSpacing::ZERO,
            overflow: UiOverflow::ScrollBoth,
            ..UiLayout::fill(UiFlow::None)
        });
    node_list = node_list.with_child(
        UiNode::new("nodes.canvas.background", UiNodeKind::Canvas)
            .with_class("nodes-canvas-background")
            .with_layout(
                UiLayout::absolute(UiRect::new(0.0, 0.0, canvas_size[0], canvas_size[1]))
                    .with_z_index(1),
            )
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                "nodes.canvas.click",
            ))
            .with_event(UiEventBinding::command(
                UiEventKind::ContextMenu,
                "nodes.canvas.context",
            ))
            .interactive()
            .focusable(),
    );
    for grid_line in canvas_grid(canvas_size, zoom) {
        node_list = node_list.with_child(grid_line);
    }
    if let Some(graph) = graph {
        let pending_source = pending_pin.and_then(|(node_index, pin_index)| {
            graph
                .nodes
                .get(node_index)
                .and_then(|node| node.pins.get(pin_index))
                .map(|pin| (pin.kind, pin.data_type))
        });
        for (connection_index, connection) in graph.connections.iter().enumerate() {
            for cable in connection_cables(graph, connection, connection_index, zoom) {
                node_list = node_list.with_child(cable);
            }
        }
        for (index, node) in graph.nodes.iter().enumerate() {
            let selected = selected_node == Some(node.id);
            node_list = node_list.with_child(node_canvas_card(
                palette,
                index,
                node,
                selected,
                pending_pin,
                pending_source,
                zoom,
            ));
        }
        if graph.nodes.is_empty() {
            node_list = node_list.with_child(
                UiNode::new("nodes.empty", UiNodeKind::Panel)
                    .with_class("nodes-empty")
                    .with_layout(
                        UiLayout::absolute(UiRect::new(24.0, 24.0, 320.0, 84.0)).with_z_index(2),
                    )
                    .with_child(
                        UiNode::new("nodes.empty.title", UiNodeKind::Label)
                            .with_text_key("nodes.empty")
                            .with_text_style(UiTextStyle::panel_title(tokens.text))
                            .with_layout(UiLayout::fit_content()),
                    )
                    .with_child(
                        UiNode::new("nodes.empty.hint", UiNodeKind::Label)
                            .with_text_key("nodes.empty_hint")
                            .with_text_style(UiTextStyle::body(tokens.text_muted))
                            .with_layout(UiLayout::fit_content()),
                    ),
            );
        }
    } else {
        node_list = node_list.with_child(
            UiNode::new("nodes.error", UiNodeKind::Panel)
                .with_class("nodes-error")
                .with_layout(
                    UiLayout::absolute(UiRect::new(24.0, 24.0, 340.0, 112.0)).with_z_index(2),
                )
                .with_child(
                    UiNode::new("nodes.error.icon", UiNodeKind::Label)
                        .with_icon(UiIcon::new(UiIconId::Error).with_size(UiIconSize::Small))
                        .with_layout(UiLayout::fixed(20.0, 20.0)),
                )
                .with_child(
                    UiNode::new("nodes.error.title", UiNodeKind::Label)
                        .with_text_key("app.error")
                        .with_layout(UiLayout::absolute(UiRect::new(28.0, 8.0, 300.0, 20.0)))
                        .with_text_style(UiTextStyle::panel_title(tokens.text)),
                )
                .with_child(
                    UiNode::new("nodes.error.hint", UiNodeKind::Label)
                        .with_text_key("nodes.empty_hint")
                        .with_layout(UiLayout::absolute(UiRect::new(28.0, 34.0, 296.0, 34.0)))
                        .with_text_style(UiTextStyle::body(tokens.text_muted)),
                )
                .with_child(
                    UiNode::new("nodes.error.new", UiNodeKind::Button)
                        .with_class("nodes-error-action")
                        .with_text_key("nodes.new_graph")
                        .with_layout(UiLayout::absolute(UiRect::new(28.0, 76.0, 132.0, 28.0)))
                        .with_accessibility_label_key("nodes.new_graph")
                        .focusable()
                        .with_event(UiEventBinding::command(
                            UiEventKind::Click,
                            "nodes.graph.new",
                        )),
                ),
        );
    }

    let active_graph_label = if let Some(graph) = graph {
        UiNode::new("nodes.active-graph", UiNodeKind::Label).with_text_value(format!(
            "{}  |  {} nodes  |  {} links  |  {:.0}%",
            graph.name,
            graph.nodes.len(),
            graph.connections.len(),
            zoom * 100.0
        ))
    } else {
        UiNode::new("nodes.active-graph", UiNodeKind::Label).with_text_key("app.error")
    }
    .with_layout(UiLayout {
        grow: 1.0,
        ..UiLayout::fit_content()
    })
    .with_text_style(UiTextStyle::body(tokens.text_muted));

    let mut root = UiNode::new("nodes.root", UiNodeKind::Panel)
        .with_class("bottom-panel")
        .with_class("nodes-root")
        .with_layout(UiLayout::fill(UiFlow::Column))
        .with_style(palette.panel_style())
        .with_child(
            UiNode::new("nodes.toolbar", UiNodeKind::Toolbar)
                .with_class("nodes-toolbar")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 5.0,
                    padding: UiSpacing::xy(8.0, 4.0),
                    overflow: UiOverflow::Clip,
                    ..UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)
                })
                .with_child(
                    UiNode::new("nodes.title", UiNodeKind::Label)
                        .with_icon(UiIcon::new(UiIconId::Node).with_size(UiIconSize::Small))
                        .with_text_key("nodes.title")
                        .with_layout(UiLayout::fit_content())
                        .with_text_style(UiTextStyle::panel_title(tokens.text)),
                )
                .with_child(active_graph_label)
                .with_child(nodes_zoom_button(
                    "nodes.zoom.out",
                    UiIconId::ChevronLeft,
                    "nodes.zoom.out",
                ))
                .with_child(nodes_zoom_button(
                    "nodes.zoom.reset",
                    UiIconId::Grid,
                    "nodes.zoom.reset",
                ))
                .with_child(nodes_zoom_button(
                    "nodes.zoom.in",
                    UiIconId::ChevronRight,
                    "nodes.zoom.in",
                ))
                .with_child(nodes_text_button("nodes.delete-selected", "nodes.delete"))
                .with_child(nodes_text_button("nodes.copy", "nodes.copy"))
                .with_child(nodes_text_button("nodes.paste", "nodes.paste"))
                .with_child(nodes_zoom_button(
                    "nodes.undo",
                    UiIconId::Undo,
                    "nodes.undo",
                ))
                .with_child(nodes_zoom_button(
                    "nodes.redo",
                    UiIconId::Redo,
                    "nodes.redo",
                )),
        )
        .with_child(quick_add_toolbar(tokens))
        .with_child(
            UiNode::new("nodes.body", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    gap: 6.0,
                    grow: 1.0,
                    ..UiLayout::fill(UiFlow::Row)
                })
                .with_child(
                    UiNode::new("nodes.sidebar", UiNodeKind::Panel)
                        .with_class("nodes-sidebar")
                        .with_layout(UiLayout {
                            flow: UiFlow::Column,
                            overflow: UiOverflow::Clip,
                            ..UiLayout::fixed(170.0, 0.0)
                        })
                        .with_child(
                            UiNode::new("nodes.sidebar.title", UiNodeKind::Label)
                                .with_text_key("nodes.graphs")
                                .with_layout(UiLayout::fixed(0.0, 24.0))
                                .with_text_style(UiTextStyle::button(tokens.text_muted)),
                        )
                        .with_child(graph_list)
                        .with_child(connection_list(palette, graph))
                        .with_child(node_minimap(palette, graph))
                        .with_child(
                            UiNode::new("nodes.sidebar.hint", UiNodeKind::Label)
                                .with_text_key(if pending_pin.is_some() {
                                    "nodes.pin_hint_pending"
                                } else {
                                    "nodes.pin_hint"
                                })
                                .with_layout(UiLayout::fit_content())
                                .with_text_style(UiTextStyle {
                                    role: UiTextRole::Label,
                                    size_px: 10.0,
                                    line_height_px: 13.0,
                                    weight: UiFontWeight::Regular,
                                    color: tokens.text_muted,
                                    inherit_color: false,
                                }),
                        ),
                )
                .with_child(node_list),
        );

    if let Some(position) = context_menu_position {
        root = root.with_child(nodes_palette_menu(palette, position, palette_filter));
    }

    let mut surface = UiSurface::new("editor.bottom.nodes", palette, root);
    surface.style_sheet = nodes_style_sheet(palette);
    surface
}

fn nodes_palette_menu(palette: StudioUiPalette, position: [f32; 2], filter: &str) -> UiNode {
    let tokens = palette.tokens();
    let mut menu = UiNode::new("nodes.palette", UiNodeKind::Menu)
        .with_class("nodes-palette-menu")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 5.0,
            padding: UiSpacing::same(8.0),
            overflow: UiOverflow::Clip,
            ..UiLayout::absolute(UiRect::new(position[0], position[1], 252.0, 312.0))
                .with_z_index(100)
        })
        .with_child(
            UiNode::new("nodes.palette.header", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 5.0,
                    ..UiLayout::fixed(0.0, 24.0).with_width_mode(UiSizeMode::Fill)
                })
                .with_child(
                    UiNode::new("nodes.palette.title", UiNodeKind::Label)
                        .with_icon(UiIcon::new(UiIconId::Node).with_size(UiIconSize::Small))
                        .with_text_key("nodes.add_node")
                        .with_layout(UiLayout {
                            grow: 1.0,
                            ..UiLayout::fit_content()
                        })
                        .with_text_style(UiTextStyle::panel_title(tokens.text)),
                )
                .with_child(
                    UiNode::new("nodes.palette.clear", UiNodeKind::Button)
                        .with_class("nodes-palette-icon-button")
                        .with_icon(UiIcon::new(UiIconId::Search).with_size(UiIconSize::Small))
                        .with_tooltip_key("nodes.search_nodes")
                        .with_accessibility_label_key("nodes.search_nodes")
                        .with_layout(UiLayout::fixed(24.0, 24.0))
                        .focusable()
                        .with_event(UiEventBinding::command(
                            UiEventKind::Click,
                            "nodes.palette.clear",
                        )),
                )
                .with_child(
                    UiNode::new("nodes.palette.close", UiNodeKind::Button)
                        .with_class("nodes-palette-icon-button")
                        .with_icon(UiIcon::new(UiIconId::Close).with_size(UiIconSize::Small))
                        .with_tooltip_key("app.cancel")
                        .with_accessibility_label_key("app.cancel")
                        .with_layout(UiLayout::fixed(24.0, 24.0))
                        .focusable()
                        .with_event(UiEventBinding::command(
                            UiEventKind::Click,
                            "nodes.palette.close",
                        )),
                ),
        )
        .with_child(
            UiNode::text_input(
                "nodes.palette.search",
                raf_ui::UiTextInput {
                    value_key: "nodes.palette.search".to_string(),
                    placeholder_key: Some("nodes.search_nodes".to_string()),
                    max_length: 128,
                    multiline: false,
                    password: false,
                    submit_command: None,
                },
            )
            .with_class("nodes-palette-search")
            .with_layout(UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill))
            .with_accessibility_label_key("nodes.search_nodes")
            .with_event(UiEventBinding::command(
                UiEventKind::KeyPress("escape".to_string()),
                "nodes.palette.close",
            )),
        );
    let query = filter.trim().to_lowercase();
    let mut list = UiNode::scroll_view("nodes.palette.list", UiScrollAxis::Vertical)
        .with_class("nodes-palette-list")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 3.0,
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fixed(0.0, 216.0).with_width_mode(UiSizeMode::Fill)
        });
    for preset in ALL_NODE_PRESETS.iter().copied().filter(|preset| {
        query.is_empty()
            || preset.label().to_lowercase().contains(&query)
            || preset.category_label().to_lowercase().contains(&query)
    }) {
        let mut button = UiNode::new(
            format!("nodes.palette.{}", preset.command_id()),
            UiNodeKind::Button,
        )
        .with_class("nodes-palette-button")
        .with_icon(UiIcon::new(node_icon(preset.category())).with_size(UiIconSize::Small));
        button = button.with_text_key(preset.label_key().unwrap_or("nodes.add_node"));
        list = list.with_child(
            button
                .with_layout(
                    UiLayout::fixed(0.0, 28.0)
                        .with_width_mode(UiSizeMode::Fill)
                        .with_z_index(1),
                )
                .with_text_style(UiTextStyle::button(tokens.text))
                .with_accessibility_label_key("nodes.add_node")
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("nodes.add:{}", preset.command_id()),
                )),
        );
    }
    menu = menu.with_child(list);
    if !ALL_NODE_PRESETS.iter().copied().any(|preset| {
        query.is_empty()
            || preset.label().to_lowercase().contains(&query)
            || preset.category_label().to_lowercase().contains(&query)
    }) {
        menu = menu.with_child(
            UiNode::new("nodes.palette.empty", UiNodeKind::Label)
                .with_text_key("app.search_no_results")
                .with_layout(UiLayout::fixed(0.0, 24.0).with_width_mode(UiSizeMode::Fill))
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        );
    }
    menu.with_event(UiEventBinding::command(
        UiEventKind::KeyPress("escape".to_string()),
        "nodes.palette.close",
    ))
}

fn connection_cables(
    graph: &NodeGraph,
    connection: &raf_nodes::graph::Connection,
    connection_index: usize,
    zoom: f32,
) -> Vec<UiNode> {
    let Some(from_node) = graph
        .nodes
        .iter()
        .find(|node| node.id == connection.from_node)
    else {
        return Vec::new();
    };
    let Some(to_node) = graph
        .nodes
        .iter()
        .find(|node| node.id == connection.to_node)
    else {
        return Vec::new();
    };
    let Some(from_pin) = from_node
        .pins
        .iter()
        .position(|pin| pin.id == connection.from_pin)
    else {
        return Vec::new();
    };
    let Some(to_pin) = to_node
        .pins
        .iter()
        .position(|pin| pin.id == connection.to_pin)
    else {
        return Vec::new();
    };
    let start = connection_pin_position(from_node, from_pin, from_node.pins[from_pin].kind, zoom);
    let end = connection_pin_position(to_node, to_pin, to_node.pins[to_pin].kind, zoom);
    let data_type = from_node.pins[from_pin].data_type;
    let middle_x = (start[0] + end[0]) * 0.5;
    vec![
        cable_segment(
            format!("nodes.cable.{connection_index}.horizontal-start"),
            start[0].min(middle_x),
            start[1] - 1.0,
            (middle_x - start[0]).abs().max(2.0),
            2.0,
            data_type,
        ),
        cable_segment(
            format!("nodes.cable.{connection_index}.vertical"),
            middle_x - 1.0,
            start[1].min(end[1]),
            2.0,
            (end[1] - start[1]).abs().max(2.0),
            data_type,
        ),
        cable_segment(
            format!("nodes.cable.{connection_index}.horizontal-end"),
            middle_x.min(end[0]),
            end[1] - 1.0,
            (end[0] - middle_x).abs().max(2.0),
            2.0,
            data_type,
        ),
    ]
}

const NODE_WIDTH: f32 = 224.0;
const NODE_HEADER_HEIGHT: f32 = 26.0;
const NODE_META_HEIGHT: f32 = 15.0;
const NODE_PIN_ROW_HEIGHT: f32 = 22.0;
const NODE_PADDING: f32 = 8.0;
const NODE_GAP: f32 = 2.0;

fn node_card_height(node: &Node, zoom: f32) -> f32 {
    let pin_count = node.pins.len().max(1) as f32;
    (NODE_PADDING * 2.0
        + NODE_HEADER_HEIGHT
        + NODE_META_HEIGHT
        + NODE_PIN_ROW_HEIGHT * pin_count
        + NODE_GAP * (pin_count + 1.0))
        * zoom
}

fn node_visual_width(zoom: f32) -> f32 {
    (NODE_WIDTH * zoom).max(170.0)
}

fn connection_pin_position(node: &Node, pin_index: usize, kind: PinKind, zoom: f32) -> [f32; 2] {
    let width = node_visual_width(zoom);
    [
        20.0 + node.position[0] * zoom
            + if matches!(kind, PinKind::Output) {
                width
            } else {
                0.0
            },
        20.0 + node.position[1] * zoom
            + (NODE_PADDING
                + NODE_HEADER_HEIGHT
                + NODE_GAP
                + NODE_META_HEIGHT
                + NODE_GAP
                + pin_index as f32 * NODE_PIN_ROW_HEIGHT
                + NODE_PIN_ROW_HEIGHT * 0.5)
                * zoom,
    ]
}

fn cable_segment(
    id: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    data_type: PinDataType,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_class("nodes-cable")
        .with_class(format!("nodes-cable-{}", pin_type_label(data_type)))
        .with_layout(
            UiLayout::absolute(UiRect::new(x, y, width.max(2.0), height.max(2.0))).with_z_index(2),
        )
}

fn canvas_dimensions(graph: Option<&NodeGraph>, zoom: f32) -> [f32; 2] {
    let zoom = zoom.clamp(0.55, 1.8);
    let (max_x, max_y) = graph
        .map(|graph| {
            graph
                .nodes
                .iter()
                .fold((1600.0_f32, 1100.0_f32), |(max_x, max_y), node| {
                    (
                        max_x.max(node.position[0] + NODE_WIDTH + 180.0),
                        max_y.max(node.position[1] + node_card_height(node, 1.0) + 180.0),
                    )
                })
        })
        .unwrap_or((1600.0, 1100.0));
    [
        (max_x * zoom + 40.0).max(720.0),
        (max_y * zoom + 40.0).max(520.0),
    ]
}

fn canvas_grid(size: [f32; 2], zoom: f32) -> Vec<UiNode> {
    let step = (32.0 * zoom.clamp(0.55, 1.8)).max(10.0);
    let mut lines = Vec::new();
    let mut x = 0.0;
    let mut column = 0_u32;
    while x <= size[0] {
        let major = column % 5 == 0;
        lines.push(
            UiNode::new(format!("nodes.grid.vertical.{column}"), UiNodeKind::Panel)
                .with_class(if major {
                    "nodes-grid-major"
                } else {
                    "nodes-grid-minor"
                })
                .with_layout(
                    UiLayout::absolute(UiRect::new(x, 0.0, if major { 1.0 } else { 0.5 }, size[1]))
                        .with_z_index(1),
                ),
        );
        x += step;
        column = column.saturating_add(1);
    }
    let mut y = 0.0;
    let mut row = 0_u32;
    while y <= size[1] {
        let major = row % 5 == 0;
        lines.push(
            UiNode::new(format!("nodes.grid.horizontal.{row}"), UiNodeKind::Panel)
                .with_class(if major {
                    "nodes-grid-major"
                } else {
                    "nodes-grid-minor"
                })
                .with_layout(
                    UiLayout::absolute(UiRect::new(0.0, y, size[0], if major { 1.0 } else { 0.5 }))
                        .with_z_index(1),
                ),
        );
        y += step;
        row = row.saturating_add(1);
    }
    lines
}

fn quick_add_toolbar(tokens: raf_ui::UiTokens) -> UiNode {
    let mut toolbar = UiNode::scroll_view("nodes.add-toolbar", UiScrollAxis::Horizontal)
        .with_class("nodes-add-toolbar")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 4.0,
            padding: UiSpacing::xy(8.0, 3.0),
            overflow: UiOverflow::ScrollX,
            ..UiLayout::fixed(0.0, 32.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(add_palette_button(tokens));
    for preset in QUICK_NODE_PRESETS.iter().copied() {
        toolbar = toolbar.with_child(add_button(preset, tokens));
    }
    toolbar
}

fn add_palette_button(tokens: raf_ui::UiTokens) -> UiNode {
    UiNode::new("nodes.add.palette", UiNodeKind::Button)
        .with_class("nodes-add-button")
        .with_class("nodes-add-primary")
        .with_icon(UiIcon::new(UiIconId::Add).with_size(UiIconSize::Small))
        .with_text_key("nodes.add_node")
        .with_layout(UiLayout::fit_content().with_z_index(1))
        .with_text_style(UiTextStyle::button(tokens.text))
        .with_accessibility_label_key("nodes.add_node")
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            "nodes.palette.open",
        ))
}

fn add_button(preset: NodePreset, tokens: raf_ui::UiTokens) -> UiNode {
    let mut button = UiNode::new(
        format!("nodes.add.{}", preset.command_id()),
        UiNodeKind::Button,
    )
    .with_class("nodes-add-button");
    button = button.with_text_key(preset.label_key().unwrap_or("nodes.add_node"));
    button
        .with_layout(UiLayout::fit_content().with_z_index(1))
        .with_text_style(UiTextStyle::button(tokens.text_muted))
        .with_accessibility_label_key(preset.label_key().unwrap_or("nodes.add_node"))
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            format!("nodes.add:{}", preset.command_id()),
        ))
}

fn connection_list(palette: StudioUiPalette, graph: Option<&NodeGraph>) -> UiNode {
    let tokens = palette.tokens();
    let mut list = UiNode::scroll_view("nodes.connections", UiScrollAxis::Vertical)
        .with_class("nodes-connections")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 2.0,
            padding: UiSpacing::xy(6.0, 4.0),
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fixed(0.0, 82.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("nodes.connections.title", UiNodeKind::Label)
                .with_text_key("nodes.connections")
                .with_layout(UiLayout::fit_content())
                .with_text_style(UiTextStyle::button(tokens.text_muted)),
        );
    if let Some(graph) = graph {
        for (index, connection) in graph.connections.iter().enumerate() {
            let from_node = graph
                .nodes
                .iter()
                .find(|node| node.id == connection.from_node);
            let to_node = graph
                .nodes
                .iter()
                .find(|node| node.id == connection.to_node);
            let valid = from_node
                .is_some_and(|node| node.pins.iter().any(|pin| pin.id == connection.from_pin))
                && to_node
                    .is_some_and(|node| node.pins.iter().any(|pin| pin.id == connection.to_pin));
            let from = from_node.map(|node| node.name.as_str()).unwrap_or("?");
            let to = to_node.map(|node| node.name.as_str()).unwrap_or("?");
            let mut row = UiNode::new(format!("nodes.connection.{index}"), UiNodeKind::Label)
                .with_class("nodes-connection")
                .with_text_value(if valid {
                    format!("{from} -> {to}")
                } else {
                    format!("! {from} -> {to}")
                })
                .with_layout(UiLayout::fit_content())
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Monospace,
                    size_px: 9.0,
                    line_height_px: 12.0,
                    weight: UiFontWeight::Regular,
                    color: tokens.text_muted,
                    inherit_color: false,
                });
            if !valid {
                row = row.with_class("nodes-connection-invalid");
            }
            list = list.with_child(row);
        }
    }
    list
}

fn node_minimap(palette: StudioUiPalette, graph: Option<&NodeGraph>) -> UiNode {
    let tokens = palette.tokens();
    let mut panel = UiNode::new("nodes.minimap", UiNodeKind::Panel)
        .with_class("nodes-minimap")
        .with_layout(
            UiLayout::fixed(0.0, 88.0)
                .with_width_mode(UiSizeMode::Fill)
                .with_z_index(2),
        )
        .with_child(
            UiNode::new("nodes.minimap.title", UiNodeKind::Label)
                .with_text_key("nodes.minimap")
                .with_layout(UiLayout::absolute(UiRect::new(6.0, 4.0, 100.0, 16.0)))
                .with_text_style(UiTextStyle::button(tokens.text_muted)),
        );
    let Some(graph) = graph else {
        return panel;
    };
    if graph.nodes.is_empty() {
        return panel.with_child(
            UiNode::new("nodes.minimap.empty", UiNodeKind::Label)
                .with_text_key("nodes.empty")
                .with_layout(UiLayout::absolute(UiRect::new(6.0, 32.0, 100.0, 18.0)))
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        );
    }
    let min_x = graph
        .nodes
        .iter()
        .map(|node| node.position[0])
        .fold(f32::INFINITY, f32::min);
    let max_x = graph
        .nodes
        .iter()
        .map(|node| node.position[0])
        .fold(f32::NEG_INFINITY, f32::max);
    let min_y = graph
        .nodes
        .iter()
        .map(|node| node.position[1])
        .fold(f32::INFINITY, f32::min);
    let max_y = graph
        .nodes
        .iter()
        .map(|node| node.position[1])
        .fold(f32::NEG_INFINITY, f32::max);
    let span_x = (max_x - min_x).max(1.0);
    let span_y = (max_y - min_y).max(1.0);
    for (index, node) in graph.nodes.iter().enumerate() {
        let x = 8.0 + ((node.position[0] - min_x) / span_x) * 144.0;
        let y = 24.0 + ((node.position[1] - min_y) / span_y) * 52.0;
        panel = panel.with_child(
            UiNode::new(format!("nodes.minimap.node.{index}"), UiNodeKind::Button)
                .with_class("nodes-minimap-node")
                .with_layout(UiLayout::absolute(UiRect::new(x, y, 8.0, 8.0)))
                .with_tooltip_key(node.name.clone())
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("nodes.select:{index}"),
                )),
        );
    }
    panel
}

fn nodes_zoom_button(id: &str, icon: UiIconId, command: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("nodes-zoom-button")
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Small))
        .with_tooltip_key(command)
        .with_accessibility_label_key(command)
        .with_layout(UiLayout::fixed(24.0, 24.0))
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn nodes_text_button(id: &str, command: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("nodes-text-button")
        .with_text_key(command)
        .with_accessibility_label_key(command)
        .with_layout(UiLayout::fit_content())
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn selected_index(
    document: &NodeEditorDocument,
    active: usize,
    selected_node: Option<NodeId>,
) -> Option<usize> {
    let selected_node = selected_node?;
    document
        .graphs
        .get(active)
        .and_then(|graph| graph.nodes.iter().position(|node| node.id == selected_node))
}

fn node_canvas_card(
    palette: StudioUiPalette,
    index: usize,
    node: &Node,
    selected: bool,
    pending_pin: Option<(usize, usize)>,
    pending_source: Option<(PinKind, PinDataType)>,
    zoom: f32,
) -> UiNode {
    let tokens = palette.tokens();
    let zoom = zoom.clamp(0.55, 1.8);
    let width = node_visual_width(zoom);
    let height = node_card_height(node, zoom);
    let mut content = UiNode::new(format!("nodes.card.{index}.content"), UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: NODE_GAP * zoom,
            padding: UiSpacing::xy(NODE_PADDING * zoom, NODE_PADDING * zoom),
            z_index: 1,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_child(
            UiNode::new(format!("nodes.card.{index}.header"), UiNodeKind::Toolbar)
                .with_class("nodes-card-header")
                .with_class(format!(
                    "nodes-card-header-{}",
                    node_category_class(node.category)
                ))
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 6.0 * zoom,
                    ..UiLayout::fixed(0.0, NODE_HEADER_HEIGHT * zoom)
                        .with_width_mode(UiSizeMode::Fill)
                })
                .with_child(
                    UiNode::new(format!("nodes.card.{index}.icon"), UiNodeKind::Label)
                        .with_icon(
                            UiIcon::new(node_icon(node.category)).with_size(UiIconSize::Custom(
                                (14.0 * zoom).round().max(10.0) as u16,
                            )),
                        )
                        .with_layout(UiLayout::fixed(18.0 * zoom, 18.0 * zoom)),
                )
                .with_child(
                    UiNode::new(format!("nodes.card.{index}.name"), UiNodeKind::Label)
                        .with_text_value(node.name.clone())
                        .with_layout(UiLayout {
                            grow: 1.0,
                            ..UiLayout::fit_content()
                        })
                        .with_text_style(UiTextStyle::button(tokens.text)),
                )
                .with_child(
                    UiNode::new(format!("nodes.card.{index}.delete"), UiNodeKind::Button)
                        .with_class("nodes-delete")
                        .with_icon(UiIcon::new(UiIconId::Close).with_size(UiIconSize::Small))
                        .with_layout(UiLayout::fixed(22.0 * zoom, 22.0 * zoom))
                        .with_tooltip_key("nodes.delete")
                        .with_accessibility_label_key("nodes.delete")
                        .focusable()
                        .with_event(UiEventBinding::command(
                            UiEventKind::Click,
                            format!("nodes.delete:{index}"),
                        )),
                ),
        )
        .with_child(
            UiNode::new(format!("nodes.card.{index}.category"), UiNodeKind::Toolbar)
                .with_layout(
                    UiLayout::fixed(0.0, NODE_META_HEIGHT * zoom).with_width_mode(UiSizeMode::Fill),
                )
                .with_child(
                    UiNode::new(
                        format!("nodes.card.{index}.category.label"),
                        UiNodeKind::Label,
                    )
                    .with_text_key(node_category_key(node.category))
                    .with_layout(UiLayout::fit_content())
                    .with_text_style(UiTextStyle {
                        role: UiTextRole::Label,
                        size_px: (10.0 * zoom).max(8.0),
                        line_height_px: (13.0 * zoom).max(10.0),
                        weight: UiFontWeight::Regular,
                        color: tokens.text_muted,
                        inherit_color: false,
                    }),
                )
                .with_child(
                    UiNode::new(
                        format!("nodes.card.{index}.category.count"),
                        UiNodeKind::Label,
                    )
                    .with_text_value(node.pins.len().to_string())
                    .with_layout(UiLayout::fit_content())
                    .with_text_style(UiTextStyle::body(tokens.text_muted)),
                )
                .with_child(
                    UiNode::new(
                        format!("nodes.card.{index}.category.pins"),
                        UiNodeKind::Label,
                    )
                    .with_text_key("nodes.pins")
                    .with_layout(UiLayout::fit_content())
                    .with_text_style(UiTextStyle::body(tokens.text_muted)),
                ),
        );
    for (pin_index, pin) in node.pins.iter().enumerate() {
        content = content.with_child(node_pin(
            palette,
            index,
            pin_index,
            pin.name.as_str(),
            pin.kind,
            pin.data_type,
            pending_pin == Some((index, pin_index)),
            pending_source.is_some_and(|(source_kind, source_type)| {
                source_kind != pin.kind && pins_compatible(source_type, pin.data_type)
            }),
            zoom,
        ));
    }

    UiNode::new(format!("nodes.card.{index}"), UiNodeKind::Panel)
        .with_class("nodes-card")
        .with_class(format!(
            "nodes-category-{}",
            node_category_class(node.category)
        ))
        .with_class(if selected { "nodes-card-selected" } else { "" })
        .with_layout(
            UiLayout::absolute(UiRect::new(
                20.0 + node.position[0] * zoom,
                20.0 + node.position[1] * zoom,
                width,
                height,
            ))
            .with_z_index(3),
        )
        .with_child(content)
        .with_event(UiEventBinding::command(
            UiEventKind::DragStart,
            format!("nodes.drag.start:{index}"),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::DragMove,
            format!("nodes.drag.move:{index}"),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::DragEnd,
            format!("nodes.drag.end:{index}"),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            format!("nodes.select:{index}"),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::ContextMenu,
            "nodes.canvas.context",
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("delete".to_string()),
            format!("nodes.delete:{index}"),
        ))
        .interactive()
        .focusable()
}

fn node_pin(
    palette: StudioUiPalette,
    node_index: usize,
    pin_index: usize,
    name: &str,
    kind: PinKind,
    data_type: PinDataType,
    pending: bool,
    compatible: bool,
    zoom: f32,
) -> UiNode {
    let tokens = palette.tokens();
    let zoom = zoom.clamp(0.55, 1.8);
    let direction = match kind {
        PinKind::Input => "IN",
        PinKind::Output => "OUT",
    };
    let mut pin = UiNode::new(
        format!("nodes.pin.{node_index}.{pin_index}"),
        UiNodeKind::Button,
    )
    .with_class("nodes-pin")
    .with_class(format!("nodes-pin-{}", pin_type_label(data_type)))
    .with_class(if compatible {
        "nodes-pin-compatible"
    } else {
        ""
    })
    .with_class(if pending { "nodes-pin-pending" } else { "" })
    .with_text_value(format!(
        "{direction}  {name}  [{}]",
        pin_type_label(data_type)
    ))
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Center,
        padding: UiSpacing::xy(5.0 * zoom, 2.0 * zoom),
        ..UiLayout::fixed(0.0, NODE_PIN_ROW_HEIGHT * zoom)
            .with_width_mode(UiSizeMode::Fill)
            .with_z_index(4)
    })
    .with_text_style(UiTextStyle {
        role: UiTextRole::Monospace,
        size_px: (9.0 * zoom).max(7.0),
        line_height_px: (13.0 * zoom).max(9.0),
        weight: UiFontWeight::Regular,
        color: if pending {
            tokens.accent_hot
        } else {
            tokens.text_muted
        },
        inherit_color: false,
    })
    .with_accessibility_label_key(if pending {
        "nodes.pin_hint_pending"
    } else {
        "nodes.pin_hint"
    })
    .focusable();
    pin = pin.with_event(UiEventBinding::command(
        UiEventKind::Click,
        format!("nodes.pin:{node_index}:{pin_index}"),
    ));
    pin
}

fn pin_type_label(data_type: PinDataType) -> &'static str {
    match data_type {
        PinDataType::Flow => "flow",
        PinDataType::Bool => "bool",
        PinDataType::Int => "int",
        PinDataType::Float => "float",
        PinDataType::String => "string",
        PinDataType::Vec3 => "vec3",
        PinDataType::Any => "any",
    }
}

fn pins_compatible(source: PinDataType, target: PinDataType) -> bool {
    source == target || source == PinDataType::Any || target == PinDataType::Any
}

fn node_category_class(category: NodeCategory) -> &'static str {
    match category {
        NodeCategory::Event => "event",
        NodeCategory::Logic => "logic",
        NodeCategory::Action => "action",
        NodeCategory::Math => "math",
        NodeCategory::Electronics => "electronics",
        NodeCategory::Variable => "variable",
    }
}

fn node_category_key(category: NodeCategory) -> &'static str {
    match category {
        NodeCategory::Event => "nodes.category.events",
        NodeCategory::Logic => "nodes.category.logic",
        NodeCategory::Action => "nodes.category.actions",
        NodeCategory::Math => "nodes.category.math",
        NodeCategory::Electronics => "nodes.category.electronics",
        NodeCategory::Variable => "nodes.category.variables",
    }
}

fn node_icon(category: NodeCategory) -> UiIconId {
    match category {
        NodeCategory::Event => UiIconId::Play,
        NodeCategory::Logic => UiIconId::Node,
        NodeCategory::Action => UiIconId::Success,
        NodeCategory::Math => UiIconId::Grid,
        NodeCategory::Electronics => UiIconId::Schematic,
        NodeCategory::Variable => UiIconId::Assets,
    }
}

fn node_category_color(category: NodeCategory) -> [u8; 4] {
    let color = category.color();
    [
        (color[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[2].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[3].clamp(0.0, 1.0) * 255.0).round() as u8,
    ]
}

fn category_header_style_rule(category: NodeCategory, tokens: raf_ui::UiTokens) -> UiStyleRule {
    UiStyleRule::new(
        UiStyleSelector::Class(format!(
            "nodes-card-header-{}",
            node_category_class(category)
        )),
        UiStylePatch {
            fill: Some(node_category_color(category)),
            border: Some(tokens.border),
            border_width: Some(1.0),
            radius: Some(3.0),
            text: Some(tokens.text),
            ..UiStylePatch::default()
        },
    )
}

fn pin_wire_style_rule(data_type: PinDataType, color: [u8; 4]) -> UiStyleRule {
    UiStyleRule::new(
        UiStyleSelector::Class(format!("nodes-cable-{}", pin_type_label(data_type))),
        UiStylePatch {
            fill: Some(color),
            ..UiStylePatch::default()
        },
    )
    .when(UiStyleRuleState::Always)
}

fn pin_style_rule(data_type: PinDataType, color: [u8; 4]) -> UiStyleRule {
    let mut fill = color;
    fill[3] = 64;
    UiStyleRule::new(
        UiStyleSelector::Class(format!("nodes-pin-{}", pin_type_label(data_type))),
        UiStylePatch {
            fill: Some(fill),
            border: Some(color),
            border_width: Some(1.0),
            ..UiStylePatch::default()
        },
    )
    .when(UiStyleRuleState::Always)
}

fn nodes_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-graph".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-graph-active".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-graph-delete".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(2.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-text-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(2.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-add-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-add-primary".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent_hot),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-add-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-card".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-card-selected".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-card".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-card-header".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            category_header_style_rule(NodeCategory::Event, tokens),
            category_header_style_rule(NodeCategory::Logic, tokens),
            category_header_style_rule(NodeCategory::Action, tokens),
            category_header_style_rule(NodeCategory::Math, tokens),
            category_header_style_rule(NodeCategory::Electronics, tokens),
            category_header_style_rule(NodeCategory::Variable, tokens),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-card".to_string()),
                UiStylePatch {
                    border: Some(tokens.focus),
                    border_width: Some(2.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-pin".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(2.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            pin_style_rule(PinDataType::Flow, [218, 221, 229, 255]),
            pin_style_rule(PinDataType::Bool, [203, 73, 73, 255]),
            pin_style_rule(PinDataType::Int, [72, 175, 184, 255]),
            pin_style_rule(PinDataType::Float, [88, 176, 103, 255]),
            pin_style_rule(PinDataType::String, [177, 101, 187, 255]),
            pin_style_rule(PinDataType::Vec3, [205, 164, 52, 255]),
            pin_style_rule(PinDataType::Any, [150, 157, 168, 255]),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-pin-compatible".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent_hot),
                    border_width: Some(2.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-pin-pending".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.accent),
                    text: Some(tokens.accent_hot),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-canvas".to_string()),
                UiStylePatch {
                    fill: Some(tokens.canvas),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-canvas-background".to_string()),
                UiStylePatch {
                    fill: Some(tokens.canvas),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-grid-minor".to_string()),
                UiStylePatch {
                    fill: Some(tokens.border),
                    opacity: Some(0.18),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-grid-major".to_string()),
                UiStylePatch {
                    fill: Some(tokens.border),
                    opacity: Some(0.34),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-cable".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    radius: Some(1.0),
                    opacity: Some(0.72),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            pin_wire_style_rule(PinDataType::Flow, [218, 221, 229, 255]),
            pin_wire_style_rule(PinDataType::Bool, [203, 73, 73, 255]),
            pin_wire_style_rule(PinDataType::Int, [72, 175, 184, 255]),
            pin_wire_style_rule(PinDataType::Float, [88, 176, 103, 255]),
            pin_wire_style_rule(PinDataType::String, [177, 101, 187, 255]),
            pin_wire_style_rule(PinDataType::Vec3, [205, 164, 52, 255]),
            pin_wire_style_rule(PinDataType::Any, [150, 157, 168, 255]),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-palette-menu".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    opacity: Some(0.98),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-palette-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-palette-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-palette-search".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-palette-icon-button".to_string()),
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
                UiStyleSelector::Class("nodes-empty".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(5.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-error".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.danger),
                    border_width: Some(1.0),
                    radius: Some(5.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-error-action".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent_hot),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-minimap".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-connection-invalid".to_string()),
                UiStylePatch {
                    text: Some(tokens.danger),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-minimap-node".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent_hot),
                    border_width: Some(1.0),
                    radius: Some(2.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("nodes-sidebar".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
        ],
    }
}
