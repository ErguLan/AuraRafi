use crate::node::{Node, NodeCategory, NodeId, NodePin, PinDataType, PinKind};
use uuid::Uuid;

/// Entity manipulation nodes for v0.4.0 game logic
pub struct EntityNodes;

impl EntityNodes {
    pub fn spawn_entity() -> Node {
        Node {
            id: NodeId::new(),
            name: "Spawn Entity".to_string(),
            category: NodeCategory::Action,
            description: "Spawns a new entity in the scene".to_string(),
            pins: vec![
                NodePin {
                    id: Uuid::new_v4(),
                    name: "In".to_string(),
                    kind: PinKind::Input,
                    data_type: PinDataType::Flow,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Name".to_string(),
                    kind: PinKind::Input,
                    data_type: PinDataType::String,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Position".to_string(),
                    kind: PinKind::Input,
                    data_type: PinDataType::Vec3,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Out".to_string(),
                    kind: PinKind::Output,
                    data_type: PinDataType::Flow,
                },
            ],
            position: [600.0, 200.0],
        }
    }

    pub fn destroy_entity() -> Node {
        Node {
            id: NodeId::new(),
            name: "Destroy Entity".to_string(),
            category: NodeCategory::Action,
            description: "Destroys an entity".to_string(),
            pins: vec![
                NodePin {
                    id: Uuid::new_v4(),
                    name: "In".to_string(),
                    kind: PinKind::Input,
                    data_type: PinDataType::Flow,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Entity ID".to_string(),
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
            position: [600.0, 350.0],
        }
    }

    pub fn set_position() -> Node {
        Node {
            id: NodeId::new(),
            name: "Set Position".to_string(),
            category: NodeCategory::Action,
            description: "Sets the position of an entity".to_string(),
            pins: vec![
                NodePin {
                    id: Uuid::new_v4(),
                    name: "In".to_string(),
                    kind: PinKind::Input,
                    data_type: PinDataType::Flow,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Entity ID".to_string(),
                    kind: PinKind::Input,
                    data_type: PinDataType::String,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "New Position".to_string(),
                    kind: PinKind::Input,
                    data_type: PinDataType::Vec3,
                },
                NodePin {
                    id: Uuid::new_v4(),
                    name: "Out".to_string(),
                    kind: PinKind::Output,
                    data_type: PinDataType::Flow,
                },
            ],
            position: [600.0, 500.0],
        }
    }
}
