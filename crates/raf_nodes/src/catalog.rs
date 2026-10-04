//! Stable built-in Nodes catalog.
//!
//! The editor consumes this catalog instead of keeping a second hardcoded
//! preset list. Slugs and translation keys are stable authoring identifiers;
//! the visible text may change with the selected language.

use crate::entity_nodes::EntityNodes;
use crate::flow_nodes::FlowNodes;
use crate::graph::NodeGraph;
use crate::hardware_nodes::HardwareNodes;
use crate::input_nodes::InputNodes;
use crate::math_nodes::MathNodes;
use crate::node::{Node, NodeCategory};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeDescriptor {
    pub slug: &'static str,
    pub label_key: &'static str,
    pub description_key: &'static str,
    pub category: NodeCategory,
}

const DESCRIPTORS: &[NodeDescriptor] = &[
    NodeDescriptor {
        slug: "on-start",
        label_key: "nodes.preset.on_start",
        description_key: "nodes.description.on_start",
        category: NodeCategory::Event,
    },
    NodeDescriptor {
        slug: "on-update",
        label_key: "nodes.preset.on_update",
        description_key: "nodes.description.on_update",
        category: NodeCategory::Event,
    },
    NodeDescriptor {
        slug: "key-press",
        label_key: "nodes.preset.key_press",
        description_key: "nodes.description.key_press",
        category: NodeCategory::Event,
    },
    NodeDescriptor {
        slug: "mouse-click",
        label_key: "nodes.preset.mouse_click",
        description_key: "nodes.description.mouse_click",
        category: NodeCategory::Event,
    },
    NodeDescriptor {
        slug: "if",
        label_key: "nodes.preset.if",
        description_key: "nodes.description.if",
        category: NodeCategory::Logic,
    },
    NodeDescriptor {
        slug: "for-loop",
        label_key: "nodes.preset.for_loop",
        description_key: "nodes.description.for_loop",
        category: NodeCategory::Logic,
    },
    NodeDescriptor {
        slug: "while-loop",
        label_key: "nodes.preset.while_loop",
        description_key: "nodes.description.while_loop",
        category: NodeCategory::Logic,
    },
    NodeDescriptor {
        slug: "delay",
        label_key: "nodes.preset.delay",
        description_key: "nodes.description.delay",
        category: NodeCategory::Logic,
    },
    NodeDescriptor {
        slug: "print",
        label_key: "nodes.preset.print",
        description_key: "nodes.description.print",
        category: NodeCategory::Action,
    },
    NodeDescriptor {
        slug: "spawn-entity",
        label_key: "nodes.preset.spawn_entity",
        description_key: "nodes.description.spawn_entity",
        category: NodeCategory::Action,
    },
    NodeDescriptor {
        slug: "destroy-entity",
        label_key: "nodes.preset.destroy_entity",
        description_key: "nodes.description.destroy_entity",
        category: NodeCategory::Action,
    },
    NodeDescriptor {
        slug: "set-position",
        label_key: "nodes.preset.set_position",
        description_key: "nodes.description.set_position",
        category: NodeCategory::Action,
    },
    NodeDescriptor {
        slug: "add",
        label_key: "nodes.preset.add",
        description_key: "nodes.description.add",
        category: NodeCategory::Math,
    },
    NodeDescriptor {
        slug: "greater-than",
        label_key: "nodes.preset.greater_than",
        description_key: "nodes.description.greater_than",
        category: NodeCategory::Math,
    },
    NodeDescriptor {
        slug: "less-than",
        label_key: "nodes.preset.less_than",
        description_key: "nodes.description.less_than",
        category: NodeCategory::Math,
    },
    NodeDescriptor {
        slug: "equals",
        label_key: "nodes.preset.equals",
        description_key: "nodes.description.equals",
        category: NodeCategory::Math,
    },
    NodeDescriptor {
        slug: "not-equals",
        label_key: "nodes.preset.not_equals",
        description_key: "nodes.description.not_equals",
        category: NodeCategory::Math,
    },
    NodeDescriptor {
        slug: "serial-read",
        label_key: "nodes.preset.serial_read",
        description_key: "nodes.description.serial_read",
        category: NodeCategory::Electronics,
    },
    NodeDescriptor {
        slug: "serial-write",
        label_key: "nodes.preset.serial_write",
        description_key: "nodes.description.serial_write",
        category: NodeCategory::Electronics,
    },
    NodeDescriptor {
        slug: "read-sensor",
        label_key: "nodes.preset.read_sensor",
        description_key: "nodes.description.read_sensor",
        category: NodeCategory::Electronics,
    },
    NodeDescriptor {
        slug: "write-actuator",
        label_key: "nodes.preset.write_actuator",
        description_key: "nodes.description.write_actuator",
        category: NodeCategory::Electronics,
    },
];

pub fn descriptors() -> &'static [NodeDescriptor] {
    static ALL: std::sync::OnceLock<Vec<NodeDescriptor>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        let mut descriptors = DESCRIPTORS.to_vec();
        descriptors.extend(
            crate::scene_nodes::SCENE_NODES
                .iter()
                .map(|(slug, _, action)| NodeDescriptor {
                    slug,
                    label_key: Box::leak(
                        format!("nodes.preset.{}", slug.replace('-', "_")).into_boxed_str(),
                    ),
                    description_key: Box::leak(
                        format!("nodes.description.{}", slug.replace('-', "_")).into_boxed_str(),
                    ),
                    category: if slug.starts_with("on-") {
                        NodeCategory::Event
                    } else if *action {
                        NodeCategory::Action
                    } else {
                        NodeCategory::Variable
                    },
                }),
        );
        descriptors
    })
}

pub fn descriptor_for_slug(slug: &str) -> Option<&'static NodeDescriptor> {
    descriptors()
        .iter()
        .find(|descriptor| descriptor.slug == slug)
}

pub fn descriptor_for_node(node: &Node) -> Option<&'static NodeDescriptor> {
    descriptors()
        .iter()
        .find(|descriptor| node_name_matches(node, descriptor.slug))
}

pub fn create(slug: &str) -> Option<Node> {
    let mut node = match slug {
        "on-start" => Node::on_start(),
        "on-update" => Node::on_update(),
        "print" => Node::print_action(),
        "if" => Node::if_branch(),
        "add" => Node::add_math(),
        "for-loop" => FlowNodes::for_loop(),
        "while-loop" => FlowNodes::while_loop(),
        "greater-than" => MathNodes::compare(">"),
        "less-than" => MathNodes::compare("<"),
        "equals" => MathNodes::compare("=="),
        "not-equals" => MathNodes::compare("!="),
        "spawn-entity" => EntityNodes::spawn_entity(),
        "destroy-entity" => EntityNodes::destroy_entity(),
        "set-position" => EntityNodes::set_position(),
        "key-press" => InputNodes::key_press(),
        "mouse-click" => InputNodes::mouse_click(),
        "delay" => InputNodes::timer_delay(),
        "serial-read" => HardwareNodes::serial_read(),
        "serial-write" => HardwareNodes::serial_write(),
        "read-sensor" => HardwareNodes::sensor_input(),
        "write-actuator" => HardwareNodes::actuator_output(),
        _ => return crate::scene_nodes::create(slug),
    };
    let mut add = |key: &str, value: &str| {
        if !node.properties.iter().any(|p| p.key == key) {
            node.properties.push(crate::NodeProperty::new(
                key,
                format!("nodes.property.{key}"),
                value,
            ));
        }
    };
    match slug {
        "destroy-entity" => add("entity", ""),
        "set-position" => {
            add("entity", "");
            add("position", "0,0,0");
        }
        "spawn-entity" => {
            add("position", "0,0,0");
            add("primitive", "empty");
        }
        "add" => {
            add("a", "0");
            add("b", "0");
        }
        "if" | "while-loop" => add("condition", "false"),
        _ => {}
    }
    Some(node)
}

/// Backfill properties introduced after older `nodes.ron` documents were
/// written. Existing values always win; unknown/custom nodes are untouched.
pub fn hydrate_properties(graph: &mut NodeGraph) {
    for node in &mut graph.nodes {
        let Some(descriptor) = descriptor_for_node(node) else {
            continue;
        };
        let Some(template) = create(descriptor.slug) else {
            continue;
        };
        for property in template.properties {
            if !node
                .properties
                .iter()
                .any(|existing| existing.key == property.key)
            {
                node.properties.push(property);
            }
        }
    }
}

fn node_name_matches(node: &Node, slug: &str) -> bool {
    match slug {
        "on-start" => node.name == "On Start",
        "on-update" => node.name == "On Update",
        "print" => node.name == "Print",
        "if" => node.name == "If",
        "add" => node.name == "Add",
        "for-loop" => node.name == "For Loop",
        "while-loop" => node.name == "While",
        "greater-than" => node.name == "Greater Than",
        "less-than" => node.name == "Less Than",
        "equals" => node.name == "Equals",
        "not-equals" => node.name == "Not Equals",
        "spawn-entity" => node.name == "Spawn Entity",
        "destroy-entity" => node.name == "Destroy Entity",
        "set-position" => node.name == "Set Position",
        "key-press" => node.name == "Key Press",
        "mouse-click" => node.name == "Mouse Click",
        "delay" => node.name == "Delay",
        "serial-read" => node.name == "Serial Read",
        "serial-write" => node.name == "Serial Write",
        "read-sensor" => node.name == "Read Sensor",
        "write-actuator" => node.name == "Write Actuator",
        _ => crate::scene_nodes::SCENE_NODES
            .iter()
            .any(|(s, name, _)| *s == slug && node.name == *name),
    }
}
