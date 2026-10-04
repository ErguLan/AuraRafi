//! Data-oriented scene operations; no dependency on editor or renderer.
use crate::{Node, NodeCategory, NodeId, NodePin, NodeProperty, PinDataType as T, PinKind as K};
use uuid::Uuid;

pub const SCENE_NODES: &[(&str, &str, bool)] = &[
    ("on-late-update", "On Late Update", false),
    ("on-event", "On Event", false),
    ("get-entity", "Get Entity", false),
    ("get-parent", "Get Parent", false),
    ("find-child", "Find Child", false),
    ("get-position", "Get Position", false),
    ("get-active-camera", "Get Active Camera", false),
    ("is-valid", "Is Valid", false),
    ("clear-camera", "Clear Camera", true),
    ("add-camera", "Add Camera", true),
    ("remove-camera", "Remove Camera", true),
    ("activate-camera", "Activate Camera", true),
    ("set-camera-fov", "Set Camera FOV", true),
    ("set-camera-clip", "Set Camera Clip", true),
    ("set-camera-projection", "Set Camera Projection", true),
    ("look-at", "Look At", true),
    ("follow-camera", "Follow Camera", true),
    ("set-rotation", "Set Rotation", true),
    ("set-local-position", "Set Local Position", true),
    ("set-parent", "Set Parent", true),
    ("detach-parent", "Detach Parent", true),
    ("send-event", "Send Event", true),
];

pub fn create(slug: &str) -> Option<Node> {
    let (_, name, action) = SCENE_NODES.iter().find(|(s, _, _)| *s == slug)?;
    let mut node = Node {
        id: NodeId::new(),
        name: (*name).into(),
        description: (*name).into(),
        category: if slug.starts_with("on-") {
            NodeCategory::Event
        } else if *action {
            NodeCategory::Action
        } else {
            NodeCategory::Variable
        },
        pins: Vec::new(),
        position: [300.0, 200.0],
        properties: Vec::new(),
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
    if slug.starts_with("on-") {
        pin("Out", K::Output, T::Flow);
        pin("Delta Time", K::Output, T::Float);
    }
    if slug == "on-event" {
        pin("Value", K::Output, T::Any);
    }
    if !slug.starts_with("on-")
        && !matches!(slug, "get-entity" | "get-active-camera" | "clear-camera")
    {
        pin("Entity", K::Input, T::Any);
    }
    if slug == "get-entity" {
        pin("Reference", K::Input, T::String);
    }
    if slug == "is-valid" {
        pin("Valid", K::Output, T::Bool);
    }
    if matches!(
        slug,
        "get-entity" | "get-parent" | "find-child" | "get-active-camera"
    ) {
        pin("Result", K::Output, T::Any);
    }
    if slug == "get-position" {
        pin("Position", K::Output, T::Vec3);
    }
    if matches!(slug, "look-at" | "follow-camera" | "set-parent") {
        pin("Target", K::Input, T::Any);
    }
    if matches!(slug, "set-local-position" | "set-rotation") {
        pin("Value", K::Input, T::Vec3);
    }
    match slug {
        "set-camera-fov" => pin("FOV", K::Input, T::Float),
        "set-camera-clip" => {
            pin("Near", K::Input, T::Float);
            pin("Far", K::Input, T::Float);
        }
        "set-camera-projection" => {
            pin("Orthographic", K::Input, T::Bool);
            pin("Scale", K::Input, T::Float);
        }
        "look-at" => pin("Aim Offset", K::Input, T::Vec3),
        "follow-camera" => {
            pin("Offset", K::Input, T::Vec3);
            pin("Aim Offset", K::Input, T::Vec3);
            pin("Sharpness", K::Input, T::Float);
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
    if slug == "get-entity" {
        property("reference", "");
    }
    if !slug.starts_with("on-")
        && !matches!(slug, "get-entity" | "get-active-camera" | "clear-camera")
    {
        property("entity", "");
    }
    match slug {
        "find-child" => property("child_name", ""),
        "look-at" | "follow-camera" | "set-parent" => property("target", ""),
        "on-event" | "send-event" => property("event_name", ""),
        _ => {}
    }
    match slug {
        "set-camera-fov" => property("fov_degrees", "60"),
        "set-camera-clip" => {
            property("near", "0.1");
            property("far", "1000");
        }
        "set-camera-projection" => {
            property("orthographic", "false");
            property("ortho_scale", "10");
        }
        "look-at" => property("aim_offset", "0,0,0"),
        "follow-camera" => {
            property("offset", "0,2,8");
            property("aim_offset", "0,1,0");
            property("sharpness", "8");
        }
        "set-parent" | "detach-parent" => property("keep_world", "true"),
        "set-local-position" | "set-rotation" => property("value", "0,0,0"),
        "send-event" => {
            property("value", "");
            property("broadcast", "false");
        }
        _ => {}
    }
    Some(node)
}
