//! Service schemas only. Generated graphs call the same runtime Host API as text scripts.
use crate::{Node, NodeCategory, NodeId, NodePin, NodeProperty, PinDataType as T, PinKind as K};
pub fn create(slug: &str) -> Option<Node> {
    if !is_service(slug) {
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
        properties: vec![],
        position: [300.0, 200.0],
    };
    if *action {
        pin(&mut node, "In", K::Input, T::Flow);
        pin(&mut node, "Out", K::Output, T::Flow);
    }
    let fields: &[(&str, &str, T, &str)] = match slug {
        "ui-text" | "ui-image" => &[("ui_instance","Instance",T::String,"main"),("ui_node","Element",T::String,""),("ui_text","Value",T::String,"")],
        "ui-visible" | "ui-enabled" => &[("ui_instance","Instance",T::String,"main"),("ui_node","Element",T::String,""),("ui_visible","Visible",T::Bool,"true")],
        "ui-show" => &[("ui_instance","Instance",T::String,"main"),("ui_visible","Visible",T::Bool,"true")],
        "ui-value" => &[("ui_key","Key",T::String,""),("ui_text","Value",T::String,"")],
        "input-axis" => &[("axis_name", "Name", T::String, "")],
        "pointer-motion" => &[
            ("pointer_source", "Source", T::String, "relative"),
            ("pointer_axis", "Axis", T::Int, "0"),
        ],
        "pointer-button" => &[
            ("pointer_button", "Button", T::Int, "0"),
            ("pointer_phase", "Phase", T::String, "pressed"),
        ],
        "set-cursor" => &[
            ("cursor_mode", "Mode", T::String, "free"),
            ("cursor_visible", "Visible", T::Bool, "true"),
        ],
        "progress-set" => &[
            ("progress_key", "Key", T::String, ""),
            ("value", "Value", T::Any, ""),
        ],
        "progress-get" | "progress-loaded-get" => &[
            ("progress_key", "Key", T::String, ""),
            ("fallback", "Fallback", T::Any, ""),
        ],
        "progress-remove" => &[("progress_key", "Key", T::String, "")],
        "progress-profile" => &[("progress_profile", "Profile", T::String, "main")],
        "progress-save" | "progress-load" | "progress-load-backup" => {
            &[("progress_slot", "Slot", T::String, "manual")]
        }
        "request-session" => &[("session_id", "Session", T::String, "")],
        "runtime-field" => &[
            ("record", "Record", T::Any, ""),
            ("field_name", "Field", T::String, "status"),
            ("fallback", "Fallback", T::Any, ""),
        ],
        "raycast" => &[
            ("ray_origin", "Origin", T::Vec3, "0,0,0"),
            ("ray_direction", "Direction", T::Vec3, "0,0,-1"),
            ("ray_distance", "Distance", T::Float, "100"),
            ("query_ignore", "Ignore", T::Any, ""),
            ("query_tag", "Tag", T::String, ""),
            ("query_triggers", "Triggers", T::Bool, "false"),
        ],
        "overlap-aabb" => &[
            ("query_min", "Min", T::Vec3, "-1,-1,-1"),
            ("query_max", "Max", T::Vec3, "1,1,1"),
            ("query_ignore", "Ignore", T::Any, ""),
            ("query_tag", "Tag", T::String, ""),
            ("query_triggers", "Triggers", T::Bool, "false"),
        ],
        _ => &[],
    };
    for (key, name, kind, value) in fields {
        pin(&mut node, name, K::Input, *kind);
        node.properties.push(NodeProperty::new(
            *key,
            format!("nodes.property.{key}"),
            *value,
        ));
    }
    if !action {
        let kind = match slug {
            "input-axis" | "pointer-motion" => T::Float,
            "pointer-button" => T::Bool,
            _ => T::Any,
        };
        pin(&mut node, "Value", K::Output, kind);
    }
    Some(node)
}
pub(crate) fn is_service(slug: &str) -> bool {
    matches!(
        slug,
        "ui-text"|"ui-image"|"ui-visible"|"ui-enabled"|"ui-show"|"ui-value"|
        "input-axis"
            | "pointer-motion"
            | "pointer-button"
            | "set-cursor"
            | "progress-set"
            | "progress-get"
            | "progress-loaded-get"
            | "progress-remove"
            | "progress-clear"
            | "progress-discard"
            | "progress-profile"
            | "progress-save"
            | "progress-load"
            | "progress-load-backup"
            | "progress-apply-loaded"
            | "progress-status"
            | "request-session"
            | "session-status"
            | "runtime-field"
            | "raycast"
            | "overlap-aabb"
    )
}
fn pin(node: &mut Node, name: &str, kind: K, data_type: T) {
    node.pins.push(NodePin {
        id: uuid::Uuid::new_v4(),
        name: name.into(),
        kind,
        data_type,
    });
}
