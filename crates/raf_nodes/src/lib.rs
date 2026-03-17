//! # raf_nodes
//!
//! Visual node-based scripting system (no-code) for AuraRafi.
//! Allows users to create logic by connecting nodes visually.
//!
//! Works for both game logic and circuit/electronics logic.
//! The executor walks flow chains and evaluates data connections.

pub mod compiler;
pub mod executor;
pub mod graph;
pub mod node;

pub use executor::{execute, ExecutionOutput, NodeValue};
pub use graph::NodeGraph;
pub use node::{Node, NodeCategory, NodeId, NodePin, PinKind};
