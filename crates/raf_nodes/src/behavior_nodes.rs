//! Behavior primitives describe data and Host API calls, never predefined gameplay.
use crate::{Node, NodeCategory, NodeId, NodePin, NodeProperty, PinDataType as T, PinKind as K};
use uuid::Uuid;

pub fn create(slug: &str) -> Option<Node> {
    if !matches!(
        slug,
        "has-capability"
            | "set-capability"
            | "query-capability"
            | "query-nearby"
            | "get-entity-property"
            | "set-entity-property"
            | "event-record"
            | "entity-reference"
    ) {
        return None;
    }
    let (_, name, action) = crate::scene_nodes::SCENE_NODES
        .iter()
        .find(|(s, _, _)| *s == slug)?;
    let mut node = Node {
        id: NodeId::new(),
        name: (*name).into(),
        description: (*name).into(),
        category: if *action {
            NodeCategory::Action
        } else {
            NodeCategory::Variable
        },
        pins: vec![],
        position: [300.0, 200.0],
        properties: vec![],
    };
    let mut pin = |name: &str, kind: K, data_type: T| {
        node.pins.push(NodePin {
            id: Uuid::new_v4(),
            name: name.into(),
            kind,
            data_type,
        })
    };
    if *action {
        pin("In", K::Input, T::Flow);
        pin("Out", K::Output, T::Flow);
    }
    if matches!(
        slug,
        "has-capability"
            | "set-capability"
            | "get-entity-property"
            | "set-entity-property"
            | "entity-reference"
    ) {
        pin("Entity", K::Input, T::Any);
    }
    if slug.contains("capability") || slug == "query-nearby" {
        pin("Capability", K::Input, T::String);
    }
    match slug {
        "has-capability" => pin("Result", K::Output, T::Bool),
        "set-capability" => pin("Enabled", K::Input, T::Bool),
        "query-capability" | "query-nearby" => {
            pin("Limit", K::Input, T::Int);
            pin("Result", K::Output, T::Any);
            if slug == "query-nearby" {
                pin("Position", K::Input, T::Vec3);
                pin("Radius", K::Input, T::Float);
            }
        }
        "get-entity-property" | "entity-reference" => {
            pin("Property", K::Input, T::String);
            pin("Value", K::Output, T::Any);
        }
        "set-entity-property" => {
            pin("Property", K::Input, T::String);
            pin("Value", K::Input, T::Any);
        }
        "event-record" => pin("Value", K::Output, T::Any),
        _ => {}
    }
    let mut property = |key: &str, value: &str| {
        node.properties.push(NodeProperty::new(
            key,
            format!("nodes.property.{key}"),
            value,
        ))
    };
    if matches!(
        slug,
        "has-capability"
            | "set-capability"
            | "get-entity-property"
            | "set-entity-property"
            | "entity-reference"
    ) {
        property("entity", "");
    }
    if slug.contains("capability") || slug == "query-nearby" {
        property("capability", "");
    }
    match slug {
        "set-capability" => property("enabled", "true"),
        "query-capability" | "query-nearby" => {
            property("query_limit", "32");
            if slug == "query-nearby" {
                property("position", "0,0,0");
                property("query_radius", "10");
            }
        }
        "get-entity-property" | "set-entity-property" | "entity-reference" => {
            property("property_name", "");
            if slug == "set-entity-property" {
                property("value", "");
            }
        }
        "event-record" => property("event_data", "{}"),
        _ => {}
    }
    Some(node)
}
