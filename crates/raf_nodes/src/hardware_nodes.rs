use crate::node::{Node, NodeCategory, NodeId, NodePin, PinDataType, PinKind};
use uuid::Uuid;

/// Hardware interaction nodes (Serial, Sensors, Actuators) mapped to Electronics.
pub struct HardwareNodes;

impl HardwareNodes {
    /// Create a "Serial Read" node.
    pub fn serial_read() -> Node {
        Node {
            id: NodeId::new(),
            name: "Serial Read".to_string(),
            category: NodeCategory::Electronics,
            description: "Reads a String from a COM port".to_string(),
            pins: vec![
                NodePin {
                    id: Uuid::new_v4(),
                    name: "In".to_string(),
                    kind: PinKind::Input,
                    data_type: PinDataType::Flow,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Port".to_string(),
                    kind: PinKind::Input,
                    data_type: PinDataType::String, // e.g., "COM3"
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Out".to_string(),
                    kind: PinKind::Output,
                    data_type: PinDataType::Flow,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Data".to_string(),
                    kind: PinKind::Output,
                    data_type: PinDataType::String,
                },
            ],
            position: [200.0, 600.0],
        }
    }

    /// Create a "Serial Write" node.
    pub fn serial_write() -> Node {
        Node {
            id: NodeId::new(),
            name: "Serial Write".to_string(),
            category: NodeCategory::Electronics,
            description: "Writes data to a COM port".to_string(),
            pins: vec![
                NodePin {
                    id: Uuid::new_v4(),
                    name: "In".to_string(),
                    kind: PinKind::Input,
                    data_type: PinDataType::Flow,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Port".to_string(),
                    kind: PinKind::Input,
                    data_type: PinDataType::String,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Data".to_string(),
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
            position: [400.0, 600.0],
        }
    }

    /// Create a "Sensor Input" node.
    pub fn sensor_input() -> Node {
        Node {
            id: NodeId::new(),
            name: "Read Sensor".to_string(),
            category: NodeCategory::Electronics,
            description: "Reads analog/digital value from external hardware pin".to_string(),
            pins: vec![
                NodePin {
                    id: Uuid::new_v4(),
                    name: "In".to_string(),
                    kind: PinKind::Input,
                    data_type: PinDataType::Flow,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Pin".to_string(),
                    kind: PinKind::Input,
                    data_type: PinDataType::String, // e.g. "A0"
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Out".to_string(),
                    kind: PinKind::Output,
                    data_type: PinDataType::Flow,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Value".to_string(),
                    kind: PinKind::Output,
                    data_type: PinDataType::Float,
                },
            ],
            position: [200.0, 750.0],
        }
    }

    /// Create an "Actuator Output" node.
    pub fn actuator_output() -> Node {
        Node {
            id: NodeId::new(),
            name: "Write Actuator".to_string(),
            category: NodeCategory::Electronics,
            description: "Writes value to external hardware actuator".to_string(),
            pins: vec![
                NodePin {
                    id: Uuid::new_v4(),
                    name: "In".to_string(),
                    kind: PinKind::Input,
                    data_type: PinDataType::Flow,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Pin".to_string(),
                    kind: PinKind::Input,
                    data_type: PinDataType::String,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Value".to_string(),
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
            position: [400.0, 750.0],
        }
    }
}
