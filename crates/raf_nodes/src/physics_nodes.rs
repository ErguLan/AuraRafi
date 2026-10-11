//! Physics node schema only; runtime_compiler emits calls to the shared Host API.
use crate::{Node, NodeCategory, NodeId, NodePin, NodeProperty, PinDataType as T, PinKind as K};
use uuid::Uuid;

pub(crate) fn is_event(slug: &str) -> bool {
    matches!(
        slug,
        "on-collision-enter"
            | "on-collision-stay"
            | "on-collision-exit"
            | "on-trigger-enter"
            | "on-trigger-stay"
            | "on-trigger-exit"
    )
}
pub fn create(slug: &str) -> Option<Node> {
    if !is_event(slug)
        && !matches!(
            slug,
            "get-velocity"
                | "set-velocity"
                | "add-velocity"
                | "apply-impulse"
                | "configure-body"
                | "set-physics-enabled"
                | "set-aabb-collider"
                | "remove-collider"
        )
    {
        return None;
    }
    let (_, name, action) = crate::scene_nodes::SCENE_NODES
        .iter()
        .find(|(s, _, _)| *s == slug)?;
    let event = is_event(slug);
    let mut node = Node {
        id: NodeId::new(),
        name: (*name).into(),
        description: (*name).into(),
        category: if event {
            NodeCategory::Event
        } else if *action {
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
    }
    if *action || event {
        pin("Out", K::Output, T::Flow);
    }
    if event {
        pin("Entity", K::Output, T::Any);
        pin("Other", K::Output, T::Any);
    } else {
        pin("Entity", K::Input, T::Any);
    }
    match slug {
        "get-velocity" => pin("Velocity", K::Output, T::Vec3),
        "set-velocity" | "add-velocity" => pin("Velocity", K::Input, T::Vec3),
        "apply-impulse" => pin("Impulse", K::Input, T::Vec3),
        "configure-body" => {
            pin("Body Type", K::Input, T::String);
            pin("Gravity", K::Input, T::Bool);
            pin("Trigger", K::Input, T::Bool);
            pin("Mass", K::Input, T::Float);
            pin("Damping", K::Input, T::Float);
        }
        "set-physics-enabled" => pin("Enabled", K::Input, T::Bool),
        "set-aabb-collider" => {
            pin("Min", K::Input, T::Vec3);
            pin("Max", K::Input, T::Vec3);
        }
        _ => {}
    }
    let mut property = |key: &str, value: &str| {
        node.properties.push(NodeProperty::new(
            key,
            format!("nodes.property.{key}"),
            value,
        ))
    };
    property("entity", ""); // Empty event filter explicitly observes all receivers.
    match slug {
        "set-velocity" | "add-velocity" => property("velocity", "0,0,0"),
        "apply-impulse" => property("impulse", "0,0,0"),
        "configure-body" => {
            property("body_type", "dynamic");
            property("gravity", "true");
            property("trigger", "false");
            property("mass", "1");
            property("damping", "0");
        }
        "set-physics-enabled" => property("enabled", "true"),
        "set-aabb-collider" => {
            property("collider_min", "-0.5,-0.5,-0.5");
            property("collider_max", "0.5,0.5,0.5");
        }
        _ => {}
    }
    Some(node)
}
