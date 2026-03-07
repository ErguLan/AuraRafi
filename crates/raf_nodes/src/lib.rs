//! # raf_nodes
//!
//! Visual node-based scripting system (no-code) for AuraRafi.
//! Allows users to create logic by connecting nodes visually.

pub mod compiler;
pub mod graph;
pub mod node;

pub use graph::NodeGraph;
pub use node::{Node, NodeCategory, NodeId, NodePin, PinKind};
