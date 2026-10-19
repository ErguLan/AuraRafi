//! Typed environment nodes. No implicit entity/character association.
use crate::{Node, NodeCategory, NodeId, NodePin, NodeProperty, PinDataType as T, PinKind as K};
use uuid::Uuid;
pub fn create(slug: &str) -> Option<Node> {
    if !slug.starts_with("environment-") {
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
    let (kind, path, value) = match slug {
        "environment-set-bool" => (T::Bool, "sun.enabled", "true"),
        "environment-set-choice" => (T::String, "mode", "scene"),
        "environment-set-color" => (T::Vec3, "sun.color", "255,250,242"),
        _ => (T::Float, "sun.intensity", "1.0"),
    };
    if slug == "environment-get" {
        pin("Value", K::Output, T::Any);
    } else if slug != "environment-clear-view" {
        pin("Value", K::Input, kind);
    }
    for (key, value) in [
        ("environment_property", path),
        ("environment_scope", "world"),
        ("value", value),
    ] {
        node.properties.push(NodeProperty::new(
            key,
            format!("nodes.property.{key}"),
            value,
        ));
    }
    Some(node)
}
