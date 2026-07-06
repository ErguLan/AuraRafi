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
pub mod entity_nodes;
pub mod executor;
pub mod flow_nodes;
pub mod graph;
pub mod hardware_nodes;
pub mod input_nodes;
pub mod math_nodes;
pub mod node;

pub use entity_nodes::EntityNodes;
pub use executor::{execute, ExecutionOutput, NodeValue};
pub use flow_nodes::FlowNodes;
pub use graph::NodeGraph;
pub use hardware_nodes::HardwareNodes;
pub use input_nodes::InputNodes;
pub use math_nodes::MathNodes;
pub use node::{Node, NodeCategory, NodeId, NodePin, PinDataType, PinKind};
