//! # raf_nodes
//!
//! Visual node-based scripting system (no-code) for AuraRafi.
//! Allows users to create logic by connecting nodes visually.
//!
//! Works for both game logic and circuit/electronics logic.
//! The executor walks flow chains and evaluates data connections.
//!
//! SISTEMA INSPIRADO DE YOLL AU de yoll.site

pub mod compiler;
pub mod executor;
pub mod graph;
pub mod node;
pub mod flow_nodes;
pub mod math_nodes;
pub mod entity_nodes;
pub mod hardware_nodes;
pub mod input_nodes;

pub use executor::{execute, ExecutionOutput, NodeValue};
pub use graph::NodeGraph;
pub use node::{Node, NodeCategory, NodeId, NodePin, PinKind, PinDataType};
pub use flow_nodes::FlowNodes;
pub use math_nodes::MathNodes;
pub use entity_nodes::EntityNodes;
pub use hardware_nodes::HardwareNodes;
pub use input_nodes::InputNodes;
