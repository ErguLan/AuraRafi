//! Node graph executor.
//!
//! Walks a compiled node graph and evaluates each node in
//! topological order. Supports flow execution (event-driven chains)
//! and data evaluation (pull values through connections).
//!
//! This executor works for both game logic and circuit logic,
//! keeping the node system unified across project types.
//!
//! SISTEMA INSPIRADO DE YOLL AU de yoll.site

use crate::graph::{Connection, NodeGraph};
use crate::node::{NodeId, PinDataType, PinKind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Runtime values
// ---------------------------------------------------------------------------

/// A runtime value that flows through node pins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeValue {
    None,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Vec3([f64; 3]),
}

impl Default for NodeValue {
    fn default() -> Self {
        Self::None
    }
}

impl NodeValue {
    /// Coerce to bool.
    pub fn as_bool(&self) -> bool {
        match self {
            Self::Bool(b) => *b,
            Self::Int(i) => *i != 0,
            Self::Float(f) => *f != 0.0,
            Self::String(s) => !s.is_empty(),
            _ => false,
        }
    }

    /// Coerce to f64.
    pub fn as_float(&self) -> f64 {
        match self {
            Self::Float(f) => *f,
            Self::Int(i) => *i as f64,
            Self::Bool(b) => if *b { 1.0 } else { 0.0 },
            _ => 0.0,
        }
    }

    /// Coerce to i64.
    pub fn as_int(&self) -> i64 {
        match self {
            Self::Int(i) => *i,
            Self::Float(f) => *f as i64,
            Self::Bool(b) => if *b { 1 } else { 0 },
            _ => 0,
        }
    }

    /// Coerce to string.
    pub fn as_string(&self) -> String {
        match self {
            Self::String(s) => s.clone(),
            Self::Float(f) => format!("{}", f),
            Self::Int(i) => format!("{}", i),
            Self::Bool(b) => format!("{}", b),
            Self::Vec3(v) => format!("({}, {}, {})", v[0], v[1], v[2]),
            Self::None => String::new(),
        }
    }

    /// Default value for a given pin data type.
    pub fn default_for(dt: PinDataType) -> Self {
        match dt {
            PinDataType::Flow => Self::None,
            PinDataType::Bool => Self::Bool(false),
            PinDataType::Int => Self::Int(0),
            PinDataType::Float => Self::Float(0.0),
            PinDataType::String => Self::String(String::new()),
            PinDataType::Vec3 => Self::Vec3([0.0, 0.0, 0.0]),
            PinDataType::Any => Self::None,
        }
    }
}

// ---------------------------------------------------------------------------
// Execution context
// ---------------------------------------------------------------------------

/// Output produced by node execution (log messages, state changes).
#[derive(Debug, Clone)]
pub struct ExecutionOutput {
    /// Log messages generated during execution.
    pub logs: Vec<String>,
    /// Final pin values for all nodes after execution.
    pub values: HashMap<Uuid, NodeValue>,
    /// Whether execution completed without errors.
    pub success: bool,
    /// Error messages if any.
    pub errors: Vec<String>,
}

/// Execute a node graph starting from the given entry node.
///
/// This is a simple interpreter that:
/// 1. Finds the entry node (e.g. "On Start" or "On Update")
/// 2. Follows flow connections (pin type = Flow) in order
/// 3. Evaluates data inputs by pulling values from connected outputs
/// 4. Calls the built-in handler for each node type
///
/// Returns the execution output with logs and final values.
pub fn execute(graph: &NodeGraph, entry_node_id: NodeId) -> ExecutionOutput {
    let mut output = ExecutionOutput {
        logs: Vec::new(),
        values: HashMap::new(),
        success: true,
        errors: Vec::new(),
    };

    // Find the entry node.
    let entry = graph.nodes.iter().find(|n| n.id == entry_node_id);
    if entry.is_none() {
        output.success = false;
        output.errors.push("Entry node not found".to_string());
        return output;
    }

    // Build a connection lookup: to_pin -> (from_node, from_pin).
    let mut input_map: HashMap<Uuid, (NodeId, Uuid)> = HashMap::new();
    for conn in &graph.connections {
        input_map.insert(conn.to_pin, (conn.from_node, conn.from_pin));
    }

    // Build a flow-output lookup: from_pin -> (to_node, to_pin).
    let mut flow_map: HashMap<Uuid, (NodeId, Uuid)> = HashMap::new();
    for conn in &graph.connections {
        // Check if this connection is a flow connection.
        if is_flow_connection(graph, conn) {
            flow_map.insert(conn.from_pin, (conn.to_node, conn.to_pin));
        }
    }

    // Walk the flow chain starting from the entry node.
    let mut current_node_id = Some(entry_node_id);
    let mut steps = 0u32;
    let max_steps = 10_000; // Safety limit.

    while let Some(node_id) = current_node_id {
        steps += 1;
        if steps > max_steps {
            output.errors.push("Execution exceeded maximum step limit".to_string());
            output.success = false;
            break;
        }

        let node = match graph.nodes.iter().find(|n| n.id == node_id) {
            Some(n) => n,
            None => {
                output.errors.push(format!("Node not found during execution: {:?}", node_id));
                output.success = false;
                break;
            }
        };

        // Evaluate input data pins (pull values from connected outputs).
        let mut input_values: HashMap<Uuid, NodeValue> = HashMap::new();
        for pin in &node.pins {
            if pin.kind == PinKind::Input && pin.data_type != PinDataType::Flow {
                let value = if let Some((_src_node, src_pin)) = input_map.get(&pin.id) {
                    // Pull from the source node's output.
                    output
                        .values
                        .get(src_pin)
                        .cloned()
                        .unwrap_or_else(|| NodeValue::default_for(pin.data_type))
                } else {
                    NodeValue::default_for(pin.data_type)
                };
                input_values.insert(pin.id, value);
            }
        }

        // Execute the node based on its name (built-in handler).
        execute_node(node, &input_values, &mut output);

        // Follow the flow output to the next node.
        current_node_id = None;
        for pin in &node.pins {
            if pin.kind == PinKind::Output && pin.data_type == PinDataType::Flow {
                // For "If" nodes, choose branch based on condition.
                if node.name == "If" && pin.name == "True" {
                    let condition = input_values
                        .values()
                        .next()
                        .map(|v| v.as_bool())
                        .unwrap_or(false);
                    if condition {
                        if let Some((next_id, _)) = flow_map.get(&pin.id) {
                            current_node_id = Some(*next_id);
                        }
                    }
                    continue;
                }
                if node.name == "If" && pin.name == "False" {
                    let condition = input_values
                        .values()
                        .next()
                        .map(|v| v.as_bool())
                        .unwrap_or(false);
                    if !condition {
                        if let Some((next_id, _)) = flow_map.get(&pin.id) {
                            current_node_id = Some(*next_id);
                        }
                    }
                    continue;
                }

                // Default: follow the first flow output.
                if let Some((next_id, _)) = flow_map.get(&pin.id) {
                    current_node_id = Some(*next_id);
                    break;
                }
            }
        }
    }

    output
}

/// Execute a single node's logic.
fn execute_node(
    node: &crate::node::Node,
    inputs: &HashMap<Uuid, NodeValue>,
    output: &mut ExecutionOutput,
) {
    match node.name.as_str() {
        "On Start" | "On Update" | "For Loop" | "While" => {
            // Event/Flow nodes: just pass flow through, logic evaluated by flow walker.
        }
        "Print" => {
            // Print node: collect all input String values and log them.
            for (pin_id, value) in inputs {
                let msg = value.as_string();
                if !msg.is_empty() {
                    output.logs.push(msg);
                }
                output.values.insert(*pin_id, value.clone());
            }
        }
        "If" => {
            for (pin_id, value) in inputs {
                output.values.insert(*pin_id, value.clone());
            }
        }
        "Add" => {
            let mut sum = 0.0f64;
            for value in inputs.values() {
                sum += value.as_float();
            }
            for pin in &node.pins {
                if pin.kind == PinKind::Output && pin.data_type != PinDataType::Flow {
                    output.values.insert(pin.id, NodeValue::Float(sum));
                }
            }
        }
        "Greater Than" | "Less Than" | "Equals" | "Not Equals" => {
            let mut a = 0.0;
            let mut b = 0.0;
            for pin in &node.pins {
                if pin.name == "A" {
                    a = inputs.get(&pin.id).map(|v| v.as_float()).unwrap_or(0.0);
                } else if pin.name == "B" {
                    b = inputs.get(&pin.id).map(|v| v.as_float()).unwrap_or(0.0);
                }
            }
            let res = match node.name.as_str() {
                "Greater Than" => a > b,
                "Less Than" => a < b,
                "Equals" => (a - b).abs() < 1e-6,
                "Not Equals" => (a - b).abs() >= 1e-6,
                _ => false,
            };
            for pin in &node.pins {
                if pin.kind == PinKind::Output && pin.data_type == PinDataType::Bool {
                    output.values.insert(pin.id, NodeValue::Bool(res));
                }
            }
        }
        "Spawn Entity" | "Destroy Entity" | "Set Position" => {
            // Evaluated by the external system listener (ECS bridging).
            // We just log that it fired for now.
            output.logs.push(format!("Node {} executed - deferring to ECS Bridge", node.name));
            for (pin_id, value) in inputs {
                output.values.insert(*pin_id, value.clone());
            }
        }
        _ => {
            // Unknown node type: pass through.
            for (pin_id, value) in inputs {
                output.values.insert(*pin_id, value.clone());
            }
        }
    }
}

/// Check if a connection carries flow data.
fn is_flow_connection(graph: &NodeGraph, conn: &Connection) -> bool {
    // Find the source pin and check its data type.
    for node in &graph.nodes {
        if node.id == conn.from_node {
            for pin in &node.pins {
                if pin.id == conn.from_pin {
                    return pin.data_type == PinDataType::Flow;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::NodeGraph;
    use crate::node::Node;

    #[test]
    fn execute_on_start_print() {
        let mut graph = NodeGraph::new("Test");
        let start = Node::on_start();
        let print = Node::print_action();
        let start_id = start.id;
        let start_out = start.pins[0].id;
        let print_in = print.pins[0].id;
        graph.add_node(start);
        graph.add_node(print);
        graph.connect(start_id, start_out, print.id, print_in);

        let result = execute(&graph, start_id);
        assert!(result.success);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn execute_missing_entry() {
        let graph = NodeGraph::new("Empty");
        let fake_id = NodeId::new();
        let result = execute(&graph, fake_id);
        assert!(!result.success);
    }
}
