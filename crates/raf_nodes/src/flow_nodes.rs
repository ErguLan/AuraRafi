use crate::node::{Node, NodeCategory, NodeId, NodePin, PinDataType, PinKind};
use uuid::Uuid;

/// Flow control nodes for the visual scripting engine (v0.4.0)
pub struct FlowNodes;

impl FlowNodes {
    /// Create a "For Loop" node.
    pub fn for_loop() -> Node {
        Node {
            id: NodeId::new(),
            name: "For Loop".to_string(),
            category: NodeCategory::Logic,
            description: "Execute a loop multiple times".to_string(),
            pins: vec![
                NodePin {
                    id: Uuid::new_v4(),
                    name: "In".to_string(),
                    kind: PinKind::Input,
                    data_type: PinDataType::Flow,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Start".to_string(),
                    kind: PinKind::Input,
                    data_type: PinDataType::Int,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "End".to_string(),
                    kind: PinKind::Input,
                    data_type: PinDataType::Int,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Loop Body".to_string(),
                    kind: PinKind::Output,
                    data_type: PinDataType::Flow,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Index".to_string(),
                    kind: PinKind::Output,
                    data_type: PinDataType::Int,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Completed".to_string(),
                    kind: PinKind::Output,
                    data_type: PinDataType::Flow,
                },
            ],
            position: [400.0, 300.0],
        }
    }

    /// Create a "While Loop" node.
    pub fn while_loop() -> Node {
        Node {
            id: NodeId::new(),
            name: "While".to_string(),
            category: NodeCategory::Logic,
            description: "Execute while condition is true".to_string(),
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
                    name: "Loop Body".to_string(),
                    kind: PinKind::Output,
                    data_type: PinDataType::Flow,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Completed".to_string(),
                    kind: PinKind::Output,
                    data_type: PinDataType::Flow,
                },
            ],
            position: [400.0, 450.0],
        }
    }
}
