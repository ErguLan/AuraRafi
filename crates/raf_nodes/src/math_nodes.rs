use crate::node::{Node, NodeCategory, NodeId, NodePin, PinDataType, PinKind};
use uuid::Uuid;

/// Math and Comparison nodes for v0.4.0 visual scripting
pub struct MathNodes;

impl MathNodes {
    pub fn compare(op: &str) -> Node {
        let name = match op {
            ">" => "Greater Than",
            "<" => "Less Than",
            "==" => "Equals",
            "!=" => "Not Equals",
            _ => "Compare",
        };

        Node {
            id: NodeId::new(),
            name: name.to_string(),
            category: NodeCategory::Math,
            description: format!("Compares A {} B", op),
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
                    data_type: PinDataType::Bool,
                },
            ],
            position: [500.0, 400.0],
        }
    }
}
