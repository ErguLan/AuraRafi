//! Node graph - collection of nodes and connections.

use crate::node::{Node, NodeId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A connection between two node pins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub id: Uuid,
    pub from_node: NodeId,
    pub from_pin: Uuid,
    pub to_node: NodeId,
    pub to_pin: Uuid,
}

/// A complete node graph (visual script).
#[derive(Debug, Clone, Serialize, Deserialize)]
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
