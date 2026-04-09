use crate::node::{Node, NodeCategory, NodeId, NodePin, PinDataType, PinKind};
use uuid::Uuid;

/// Input event nodes (Keyboard, Mouse, Timer) for v0.4.0
pub struct InputNodes;

impl InputNodes {
    /// Create a "Key Press" event node.
    pub fn key_press() -> Node {
        Node {
            id: NodeId::new(),
            name: "Key Press".to_string(),
            category: NodeCategory::Event,
            description: "Fires when a specific key is pressed".to_string(),
            pins: vec![
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Key".to_string(),
                    kind: PinKind::Input,
                    data_type: PinDataType::String,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Pressed".to_string(),
                    kind: PinKind::Output,
                    data_type: PinDataType::Flow,
                },
            ],
            position: [100.0, 300.0],
        }
    }

    /// Create a "Mouse Click" event node.
    pub fn mouse_click() -> Node {
        Node {
            id: NodeId::new(),
            name: "Mouse Click".to_string(),
            category: NodeCategory::Event,
            description: "Fires on mouse click".to_string(),
            pins: vec![
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Button".to_string(),
                    kind: PinKind::Input,
                    data_type: PinDataType::String, // "Left", "Right", "Middle"
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Clicked".to_string(),
                    kind: PinKind::Output,
                    data_type: PinDataType::Flow,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "X".to_string(),
                    kind: PinKind::Output,
                    data_type: PinDataType::Float,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Y".to_string(),
                    kind: PinKind::Output,
                    data_type: PinDataType::Float,
                },
            ],
            position: [100.0, 450.0],
        }
    }

    /// Create a "Timer Delay" logic node.
    pub fn timer_delay() -> Node {
        Node {
            id: NodeId::new(),
            name: "Delay".to_string(),
            category: NodeCategory::Logic,
            description: "Waits before continuing flow".to_string(),
            pins: vec![
                NodePin {
                    id: Uuid::new_v4(),
                    name: "In".to_string(),
                    kind: PinKind::Input,
                    data_type: PinDataType::Flow,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Seconds".to_string(),
                    kind: PinKind::Input,
                    data_type: PinDataType::Float,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Out".to_string(),
                    kind: PinKind::Output,
                    data_type: PinDataType::Flow,
                },
            ],
            position: [300.0, 450.0],
        }
    }
}
