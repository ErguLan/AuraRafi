//! Presentation mapping for the shared `raf_nodes` catalog.
//!
//! Node identity and factories live in `raf_nodes`; iconography and UI
//! semantics stay here so the domain crate does not depend on RafUI.

use raf_nodes::{Node, NodeCategory};
use raf_render::api_graphic_basic::ui_surface::UiIconId;

pub fn icon_for_slug(slug: &str) -> UiIconId {
    match slug {
        "on-start" => UiIconId::Play,
        "on-update" => UiIconId::Refresh,
        "print" => UiIconId::Console,
        "if" | "greater-than" | "less-than" | "equals" => UiIconId::Node,
        "add" => UiIconId::Add,
        "for-loop" | "while-loop" | "delay" => UiIconId::Refresh,
        "spawn-entity" => UiIconId::Cube,
        "destroy-entity" => UiIconId::Close,
        "set-position" => UiIconId::Move,
        "key-press" | "serial-read" | "serial-write" => UiIconId::Node,
        "mouse-click" => UiIconId::Entity,
        "read-sensor" | "write-actuator" => UiIconId::BoardOutline,
        _ => UiIconId::Node,
    }
}

pub fn category_icon(category: NodeCategory) -> UiIconId {
    match category {
        NodeCategory::Event => UiIconId::Play,
        NodeCategory::Logic => UiIconId::Route,
        NodeCategory::Action => UiIconId::Settings,
        NodeCategory::Math => UiIconId::Add,
        NodeCategory::Electronics => UiIconId::Schematic,
        NodeCategory::Variable => UiIconId::Folder,
    }
}

pub fn category_key(category: NodeCategory) -> &'static str {
    match category {
        NodeCategory::Event => "nodes.category.events",
        NodeCategory::Logic => "nodes.category.logic",
        NodeCategory::Action => "nodes.category.actions",
        NodeCategory::Math => "nodes.category.math",
        NodeCategory::Electronics => "nodes.category.electronics",
        NodeCategory::Variable => "nodes.category.variables",
    }
}

pub fn category_slug(category: NodeCategory) -> &'static str {
    match category {
        NodeCategory::Event => "events",
        NodeCategory::Logic => "logic",
        NodeCategory::Action => "actions",
        NodeCategory::Math => "math",
        NodeCategory::Electronics => "electronics",
        NodeCategory::Variable => "variables",
    }
}

pub fn slug_for_node(node: &Node) -> Option<&'static str> {
    raf_nodes::catalog::descriptor_for_node(node).map(|descriptor| descriptor.slug)
}

pub fn label_key_for_node(node: &Node) -> Option<&'static str> {
    raf_nodes::catalog::descriptor_for_node(node).map(|descriptor| descriptor.label_key)
}

pub fn description_key_for_node(node: &Node) -> Option<&'static str> {
    raf_nodes::catalog::descriptor_for_node(node).map(|descriptor| descriptor.description_key)
}

pub fn pin_label_key(node: &Node, pin_name: &str) -> Option<String> {
    let slug = match slug_for_node(node)? {
        // The event pin locale keys predate the hyphenated node slugs used by
        // the catalog. Keep that legacy key shape stable instead of leaking a
        // missing translation key into the canvas.
        "on-start" => "on_start",
        "on-update" => "on_update",
        slug => slug,
    };
    let normalized = pin_name
        .chars()
        .filter_map(|character| {
            if character.is_ascii_alphanumeric() {
                Some(character.to_ascii_lowercase())
            } else if character.is_ascii_whitespace() || character == '-' {
                Some('_')
            } else {
                None
            }
        })
        .collect::<String>();
    Some(format!("nodes.pin.{slug}.{normalized}"))
}

/// Resolve a pin label for visible UI text without ever rendering an i18n key
/// when a custom or older node has no catalog entry.
pub fn localized_pin_label(node: &Node, pin_name: &str, language: raf_core::Language) -> String {
    let Some(key) = pin_label_key(node, pin_name) else {
        return pin_name.to_string();
    };
    let translated = raf_core::i18n::t(&key, language);
    if translated == key {
        pin_name.to_string()
    } else {
        translated
    }
}

pub fn property_label_key(key: &str) -> String {
    format!("nodes.property.{key}")
}

pub fn pin_type_key(data_type: raf_nodes::PinDataType) -> &'static str {
    match data_type {
        raf_nodes::PinDataType::Flow => "nodes.type.flow",
        raf_nodes::PinDataType::Bool => "nodes.type.bool",
        raf_nodes::PinDataType::Int => "nodes.type.int",
        raf_nodes::PinDataType::Float => "nodes.type.float",
        raf_nodes::PinDataType::String => "nodes.type.string",
        raf_nodes::PinDataType::Vec3 => "nodes.type.vec3",
        raf_nodes::PinDataType::Any => "nodes.type.any",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_pin_keys_match_the_locale_contract() {
        let node = Node::on_update();
        let pin = node
            .pins
            .iter()
            .find(|pin| pin.name == "Delta Time")
            .expect("On Update must expose Delta Time");

        assert_eq!(
            pin_label_key(&node, &pin.name).as_deref(),
            Some("nodes.pin.on_update.delta_time")
        );
        assert_eq!(
            localized_pin_label(&node, &pin.name, raf_core::Language::English),
            "Delta time"
        );
        assert_eq!(
            localized_pin_label(&node, &pin.name, raf_core::Language::Spanish),
            "Tiempo delta"
        );
    }
}
