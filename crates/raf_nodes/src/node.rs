//! Node definition - the building block of visual scripts.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a node instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub Uuid);

impl NodeId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for NodeId {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether a pin is input or output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PinKind {
    Input,
    Output,
}

/// Data type a pin carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PinDataType {
    Flow,
    Bool,
    Int,
    Float,
    String,
    Vec3,
    Any,
}

/// A connection point on a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodePin {
    pub id: Uuid,
    pub name: String,
    pub kind: PinKind,
    pub data_type: PinDataType,
}

/// Category of node for the palette/toolbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeCategory {
    Event,
    Logic,
    Action,
    Math,
    Electronics,
    Variable,
}

impl NodeCategory {
    pub fn display_name(&self) -> &str {
        match self {
            Self::Event => "Events",
            Self::Logic => "Logic",
            Self::Action => "Actions",
            Self::Math => "Math",
            Self::Electronics => "Electronics",
            Self::Variable => "Variables",
        }
    }

    /// Accent color as RGBA for the node header.
    pub fn color(&self) -> [f32; 4] {
        match self {
            Self::Event => [0.83, 0.33, 0.10, 1.0],  // Orange-ish
            Self::Logic => [0.40, 0.60, 0.80, 1.0],   // Blue
            Self::Action => [0.30, 0.70, 0.40, 1.0],   // Green
            Self::Math => [0.70, 0.50, 0.80, 1.0],     // Purple
            Self::Electronics => [0.80, 0.70, 0.20, 1.0], // Yellow
            Self::Variable => [0.50, 0.50, 0.50, 1.0],  // Gray
        }
    }
}

/// A visual scripting node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub name: String,
    pub category: NodeCategory,
    pub description: String,
    pub pins: Vec<NodePin>,
    /// Position on the node editor canvas.
    pub position: [f32; 2],
}

impl Node {
    /// Create an "On Start" event node.
    pub fn on_start() -> Self {
        Self {
            id: NodeId::new(),
            name: "On Start".to_string(),
            category: NodeCategory::Event,
            description: "Fires when the scene starts".to_string(),
            pins: vec![NodePin {
                id: Uuid::new_v4(),
                name: "Out".to_string(),
                kind: PinKind::Output,
                data_type: PinDataType::Flow,
            }],
            position: [100.0, 100.0],
        }
    }

    /// Create an "On Update" event node.
    pub fn on_update() -> Self {
        Self {
            id: NodeId::new(),
            name: "On Update".to_string(),
            category: NodeCategory::Event,
            description: "Fires every frame".to_string(),
            pins: vec![
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Out".to_string(),
                    kind: PinKind::Output,
                    data_type: PinDataType::Flow,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Delta Time".to_string(),
                    kind: PinKind::Output,
                    data_type: PinDataType::Float,
                },
            ],
            position: [100.0, 200.0],
        }
    }

    /// Create a "Print" action node.
    pub fn print_action() -> Self {
        Self {
            id: NodeId::new(),
            name: "Print".to_string(),
            category: NodeCategory::Action,
            description: "Print a message to console".to_string(),
            pins: vec![
                NodePin {
                    id: Uuid::new_v4(),
                    name: "In".to_string(),
                    kind: PinKind::Input,
                    data_type: PinDataType::Flow,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Message".to_string(),
                    kind: PinKind::Input,
                    data_type: PinDataType::String,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Out".to_string(),
                    kind: PinKind::Output,
                    data_type: PinDataType::Flow,
                },
            ],
            position: [300.0, 100.0],
        }
    }

    /// Create an "If" logic branch node.
    pub fn if_branch() -> Self {
        Self {
            id: NodeId::new(),
            name: "If".to_string(),
            category: NodeCategory::Logic,
            description: "Conditional branch".to_string(),
            pins: vec![
                NodePin {
                    id: Uuid::new_v4(),
                    name: "In".to_string(),
                    kind: PinKind::Input,
                    data_type: PinDataType::Flow,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Condition".to_string(),
                    kind: PinKind::Input,
                    data_type: PinDataType::Bool,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "True".to_string(),
                    kind: PinKind::Output,
                    data_type: PinDataType::Flow,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "False".to_string(),
                    kind: PinKind::Output,
                    data_type: PinDataType::Flow,
                },
            ],
            position: [300.0, 300.0],
        }
    }

    /// Create an "Add" math node.
    pub fn add_math() -> Self {
        Self {
            id: NodeId::new(),
            name: "Add".to_string(),
            category: NodeCategory::Math,
            description: "Add two numbers".to_string(),
            pins: vec![
                NodePin {
                    id: Uuid::new_v4(),
                    name: "A".to_string(),
                    kind: PinKind::Input,
                    data_type: PinDataType::Float,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "B".to_string(),
                    kind: PinKind::Input,
                    data_type: PinDataType::Float,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Result".to_string(),
                    kind: PinKind::Output,
                    data_type: PinDataType::Float,
                },
            ],
            position: [500.0, 200.0],
        }
    }
}
