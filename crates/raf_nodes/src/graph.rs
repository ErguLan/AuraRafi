//! Node graph - collection of nodes and connections.
//!
//! SISTEMA INSPIRADO DE YOLL AU de yoll.site

use crate::node::{Node, NodeId, NodePin, PinDataType, PinKind};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

/// A connection between two node pins.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Connection {
    pub id: Uuid,
    pub from_node: NodeId,
    pub from_pin: Uuid,
    pub to_node: NodeId,
    pub to_pin: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphDiagnosticSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphDiagnostic {
    pub severity: GraphDiagnosticSeverity,
    pub code: &'static str,
    pub message: String,
    /// Optional runtime detail kept separate from the translatable message.
    /// The editor can localize the diagnostic and append this value verbatim.
    pub detail: Option<String>,
    pub node_id: Option<NodeId>,
    pub pin_id: Option<Uuid>,
}

impl GraphDiagnostic {
    fn error(
        code: &'static str,
        message: impl Into<String>,
        node_id: Option<NodeId>,
        pin_id: Option<Uuid>,
    ) -> Self {
        Self {
            severity: GraphDiagnosticSeverity::Error,
            code,
            message: message.into(),
            detail: None,
            node_id,
            pin_id,
        }
    }

    fn warning(
        code: &'static str,
        message: impl Into<String>,
        node_id: Option<NodeId>,
        detail: Option<String>,
    ) -> Self {
        Self {
            severity: GraphDiagnosticSeverity::Warning,
            code,
            message: message.into(),
            detail,
            node_id,
            pin_id: None,
        }
    }
}

/// A complete node graph (visual script).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeGraph {
    pub name: String,
    pub nodes: Vec<Node>,
    pub connections: Vec<Connection>,
}

impl NodeGraph {
    /// Create an empty graph.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            nodes: Vec::new(),
            connections: Vec::new(),
        }
    }

    /// Add a node to the graph.
    pub fn add_node(&mut self, node: Node) -> NodeId {
        let id = node.id;
        self.nodes.push(node);
        id
    }

    pub fn node(&self, node_id: NodeId) -> Option<&Node> {
        self.nodes.iter().find(|node| node.id == node_id)
    }

    pub fn pin(&self, node_id: NodeId, pin_id: Uuid) -> Option<&NodePin> {
        self.node(node_id)
            .and_then(|node| node.pins.iter().find(|pin| pin.id == pin_id))
    }

    /// Update a persisted instance property without exposing the graph's
    /// internal storage to the UI host.
    pub fn set_node_property(
        &mut self,
        node_id: NodeId,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> bool {
        let key = key.into();
        let value = value.into();
        let Some(node) = self.nodes.iter_mut().find(|node| node.id == node_id) else {
            return false;
        };
        let Some(property) = node
            .properties
            .iter_mut()
            .find(|property| property.key == key)
        else {
            return false;
        };
        if property.value == value {
            return false;
        }
        property.value = value;
        true
    }

    /// Connect two pins after enforcing the graph's direction and type rules.
    ///
    /// The older `connect` method remains available for deserialization and
    /// compatibility with existing callers. New editor mutations should use
    /// this checked path.
    pub fn try_connect(
        &mut self,
        first_node: NodeId,
        first_pin: Uuid,
        second_node: NodeId,
        second_pin: Uuid,
    ) -> Result<Uuid, String> {
        let first = self
            .pin(first_node, first_pin)
            .ok_or_else(|| "Source or target pin does not exist".to_string())?;
        let second = self
            .pin(second_node, second_pin)
            .ok_or_else(|| "Source or target pin does not exist".to_string())?;
        let (from_node, from_pin, to_node, to_pin) = match (first.kind, second.kind) {
            (PinKind::Output, PinKind::Input) => (first_node, first_pin, second_node, second_pin),
            (PinKind::Input, PinKind::Output) => (second_node, second_pin, first_node, first_pin),
            _ => return Err("Connections require one output and one input".to_string()),
        };
        if from_node == to_node {
            return Err("A node cannot connect to itself".to_string());
        }
        let from_type = self
            .pin(from_node, from_pin)
            .map(|pin| pin.data_type)
            .expect("validated source pin");
        let to_type = self
            .pin(to_node, to_pin)
            .map(|pin| pin.data_type)
            .expect("validated target pin");
        if !types_compatible(from_type, to_type) {
            return Err(format!(
                "Incompatible pin types: {:?} cannot connect to {:?}",
                from_type, to_type
            ));
        }
        if self.connections.iter().any(|connection| {
            connection.from_node == from_node
                && connection.from_pin == from_pin
                && connection.to_node == to_node
                && connection.to_pin == to_pin
        }) {
            return Err("Those pins are already connected".to_string());
        }
        if self
            .connections
            .iter()
            .any(|connection| connection.to_node == to_node && connection.to_pin == to_pin)
        {
            return Err("An input pin can only have one connection".to_string());
        }
        Ok(self.connect(from_node, from_pin, to_node, to_pin))
    }

    /// Returns actionable errors and warnings without mutating the graph.
    pub fn diagnostics(&self) -> Vec<GraphDiagnostic> {
        let mut diagnostics = Vec::new();
        let mut seen = HashSet::new();
        for connection in &self.connections {
            if self.node(connection.from_node).is_none() {
                diagnostics.push(GraphDiagnostic::error(
                    "missing_source_node",
                    "Connection source node does not exist",
                    Some(connection.from_node),
                    Some(connection.from_pin),
                ));
                continue;
            }
            if self.node(connection.to_node).is_none() {
                diagnostics.push(GraphDiagnostic::error(
                    "missing_target_node",
                    "Connection target node does not exist",
                    Some(connection.to_node),
                    Some(connection.to_pin),
                ));
                continue;
            }
            let Some(from_pin) = self.pin(connection.from_node, connection.from_pin) else {
                diagnostics.push(GraphDiagnostic::error(
                    "missing_source_pin",
                    "Connection source pin does not exist",
                    Some(connection.from_node),
                    Some(connection.from_pin),
                ));
                continue;
            };
            let Some(to_pin) = self.pin(connection.to_node, connection.to_pin) else {
                diagnostics.push(GraphDiagnostic::error(
                    "missing_target_pin",
                    "Connection target pin does not exist",
                    Some(connection.to_node),
                    Some(connection.to_pin),
                ));
                continue;
            };
            if from_pin.kind != PinKind::Output || to_pin.kind != PinKind::Input {
                diagnostics.push(GraphDiagnostic::error(
                    "invalid_pin_direction",
                    "Connections must run from an output to an input",
                    Some(connection.from_node),
                    Some(connection.from_pin),
                ));
            }
            if !types_compatible(from_pin.data_type, to_pin.data_type) {
                diagnostics.push(GraphDiagnostic::error(
                    "incompatible_pin_types",
                    format!(
                        "Incompatible pin types: {:?} cannot connect to {:?}",
                        from_pin.data_type, to_pin.data_type
                    ),
                    Some(connection.to_node),
                    Some(connection.to_pin),
                ));
            }
            let key = (
                connection.from_node,
                connection.from_pin,
                connection.to_node,
                connection.to_pin,
            );
            if !seen.insert(key) {
                diagnostics.push(GraphDiagnostic::error(
                    "duplicate_connection",
                    "The same pins are connected more than once",
                    Some(connection.to_node),
                    Some(connection.to_pin),
                ));
            }
        }
        for node in &self.nodes {
            if self.connections_for(node.id).is_empty() && self.nodes.len() > 1 {
                diagnostics.push(GraphDiagnostic::warning(
                    "unconnected_node",
                    "Node has no connections",
                    Some(node.id),
                    Some(node.name.clone()),
                ));
            }
        }
        diagnostics
    }

    /// Connect two pins between nodes.
    pub fn connect(
        &mut self,
        from_node: NodeId,
        from_pin: Uuid,
        to_node: NodeId,
        to_pin: Uuid,
    ) -> Uuid {
        let conn = Connection {
            id: Uuid::new_v4(),
            from_node,
            from_pin,
            to_node,
            to_pin,
        };
        let id = conn.id;
        self.connections.push(conn);
        id
    }

    /// Remove a node and all its connections.
    pub fn remove_node(&mut self, node_id: NodeId) {
        self.nodes.retain(|n| n.id != node_id);
        self.connections
            .retain(|c| c.from_node != node_id && c.to_node != node_id);
    }

    /// Remove a connection.
    pub fn disconnect(&mut self, connection_id: Uuid) {
        self.connections.retain(|c| c.id != connection_id);
    }

    /// Find all connections for a given node.
    pub fn connections_for(&self, node_id: NodeId) -> Vec<&Connection> {
        self.connections
            .iter()
            .filter(|c| c.from_node == node_id || c.to_node == node_id)
            .collect()
    }
}

pub fn types_compatible(from: PinDataType, to: PinDataType) -> bool {
    from == to || from == PinDataType::Any || to == PinDataType::Any
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::Node;

    #[test]
    fn try_connect_normalizes_pin_order_and_rejects_incompatible_types() {
        let mut graph = NodeGraph::new("Test");
        let start = Node::on_start();
        let print = Node::print_action();
        let add = Node::add_math();
        let start_id = start.id;
        let start_out = start.pins[0].id;
        let print_id = print.id;
        let print_in = print.pins[0].id;
        let add_id = add.id;
        let add_a = add.pins[0].id;
        graph.add_node(start);
        graph.add_node(print);
        graph.add_node(add);

        graph
            .try_connect(print_id, print_in, start_id, start_out)
            .expect("reverse pin order should be normalized");
        assert_eq!(graph.connections.len(), 1);
        assert_eq!(graph.connections[0].from_node, start_id);
        assert_eq!(graph.connections[0].to_node, print_id);

        let error = graph
            .try_connect(start_id, start_out, add_id, add_a)
            .expect_err("flow cannot connect to a float input");
        assert!(error.contains("Incompatible pin types"));
    }

    #[test]
    fn catalog_hydrates_properties_without_replacing_existing_values() {
        let mut graph = NodeGraph::new("Test");
        let mut print = Node::print_action();
        print.properties.clear();
        graph.add_node(print);

        crate::catalog::hydrate_properties(&mut graph);

        assert_eq!(graph.nodes[0].properties.len(), 1);
        assert_eq!(graph.nodes[0].properties[0].key, "message");
    }
}
