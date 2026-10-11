//! Pure translation from Inspector intents to canonical authoring commands.
use crate::panels::inspector_surface::{
    behavior_surface::{parameter_key, parse_scope},
    variable_value_text,
};
use raf_core::scene::{SceneGraph, SceneNodeId, VariableValue};
use serde_json::{json, Value};

fn console(name: &str, args: Value) -> String {
    format!("console.submit:/{name} {args}")
}

pub(crate) fn parameter_action(
    scene: &SceneGraph,
    selected: &[SceneNodeId],
    raw: &str,
    text: impl Fn(&str) -> String,
) -> Option<String> {
    let mut parts = raw.split(':');
    let action = parts.next()?;
    let id = SceneNodeId(parts.next()?.parse().ok()?);
    if selected.first().copied() != Some(id) || !scene.is_valid_node(id) {
        return None;
    }
    let source = parse_scope(parts.next()?)?;
    let index: usize = parts.next()?.parse().ok()?;
    let node = scene.get(id)?;
    let source_path = source
        .map(|i| node.scripts.get(i).map(String::as_str))
        .unwrap_or(Some(""))?;
    let values = if source.is_some() {
        node.behavior_parameters
            .get(source_path)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    } else {
        node.variables.as_slice()
    };
    let mut args = json!({"target": node.uuid, "source": source_path});
    if action == "add" {
        let name = (1..=257)
            .map(|i| format!("parameter_{i}"))
            .find(|name| !values.iter().any(|p| &p.name == name))?;
        args["name"] = json!(name);
        args["type"] = json!("Number");
        args["value"] = json!("0");
    } else {
        let parameter = values.get(index)?;
        args["name"] = json!(parameter.name);
        match action {
            "remove" => args["action"] = json!("remove"),
            "rename" => {
                args["action"] = json!("rename");
                args["new_name"] = json!(text(&parameter_key(node.uuid, source, index, "name")));
            }
            "value" => {
                args["type"] = json!(parameter.value.type_label());
                args["value"] = json!(text(&parameter_key(node.uuid, source, index, "value")));
            }
            "type" => {
                let (kind, value) = match parameter.value {
                    VariableValue::Number(_) => ("Text", ""),
                    VariableValue::Text(_) => ("Bool", "false"),
                    VariableValue::Bool(_) => ("Entity", "none"),
                    VariableValue::Entity(_) => ("Number", "0"),
                };
                args["type"] = json!(kind);
                args["value"] = json!(value);
            }
            _ => return None,
        }
    }
    Some(console("behavior.parameter", args))
}

pub(crate) fn select_action(
    scene: &SceneGraph,
    selected: &[SceneNodeId],
    key: &str,
    value: &str,
) -> Option<String> {
    let id = *selected.first()?;
    if !scene.is_valid_node(id) {
        return None;
    }
    let node = scene.get(id)?;
    if key == "inspector.behavior-source" && !value.is_empty() {
        return Some(console(
            "behavior.attach",
            json!({"target": node.uuid, "source": value}),
        ));
    }
    if key == "inspector.prefab-source" && !value.is_empty() {
        return Some(console("prefab.instantiate", json!({"file": value})));
    }
    let (scope, index) = key
        .strip_prefix("inspector.parameter-ref.")?
        .split_once('.')?;
    let source = parse_scope(scope)?;
    let index: usize = index.parse().ok()?;
    let source_path = source
        .map(|i| node.scripts.get(i).map(String::as_str))
        .unwrap_or(Some(""))?;
    let values = if source.is_some() {
        node.behavior_parameters.get(source_path)?.as_slice()
    } else {
        node.variables.as_slice()
    };
    let parameter = values.get(index)?;
    if !matches!(parameter.value, VariableValue::Entity(_)) {
        return None;
    }
    Some(console(
        "behavior.parameter",
        json!({"target": node.uuid, "source": source_path, "name": parameter.name, "type": "Entity", "value": value}),
    ))
}

pub(crate) fn detach_action(
    scene: &SceneGraph,
    selected: &[SceneNodeId],
    raw: &str,
) -> Option<String> {
    let (id, index) = raw.split_once(':')?;
    let id = SceneNodeId(id.parse().ok()?);
    if selected.first().copied() != Some(id) {
        return None;
    }
    let node = scene.get(id)?;
    Some(console(
        "behavior.detach",
        json!({"target": node.uuid, "source": node.scripts.get(index.parse::<usize>().ok()?)?}),
    ))
}

pub(crate) fn capture_action(
    scene: &SceneGraph,
    selected: &[SceneNodeId],
    raw: &str,
    text: impl Fn(&str) -> String,
) -> Option<String> {
    let id = SceneNodeId(raw.parse().ok()?);
    if selected.first().copied() != Some(id) {
        return None;
    }
    let node = scene.get(id)?;
    let file = text(&format!("inspector.prefab.{}.file", node.uuid));
    Some(console(
        "prefab.capture",
        json!({"target": node.uuid, "file": file}),
    ))
}

pub(crate) fn seed_parameters(
    node: &raf_core::scene::SceneNode,
    controls: &mut raf_ui::UiControlState,
    focused: Option<&str>,
    shared_visible: bool,
    expanded: Option<usize>,
) {
    for source in shared_visible
        .then_some(None)
        .into_iter()
        .chain(expanded.filter(|i| *i < node.scripts.len()).map(Some))
    {
        let values = source
            .and_then(|i| node.behavior_parameters.get(&node.scripts[i]))
            .map(Vec::as_slice)
            .unwrap_or_else(|| {
                if source.is_none() {
                    &node.variables
                } else {
                    &[]
                }
            });
        for (index, parameter) in values.iter().enumerate().take(256) {
            for (field, value) in [
                ("name", parameter.name.clone()),
                ("value", variable_value_text(&parameter.value)),
            ] {
                let key = parameter_key(node.uuid, source, index, field);
                if focused != Some(key.as_str()) || !controls.has_text(&key) {
                    controls.set_text(key, value, 4096);
                }
            }
        }
    }
    let key = format!("inspector.prefab.{}.file", node.uuid);
    if focused != Some(key.as_str()) && !controls.has_text(&key) {
        controls.set_text(key, format!("{}.prefab.ron", node.uuid), 512);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inspector_entity_assignment_uses_canonical_command_and_stale_selection_is_ignored() {
        let mut scene = SceneGraph::new();
        let id = scene.add_root("Controller");
        let camera = scene.add_root("Camera");
        scene
            .get_mut(id)
            .unwrap()
            .scripts
            .push("scripts/camera.rhai".into());
        scene.get_mut(id).unwrap().behavior_parameters.insert(
            "scripts/camera.rhai".into(),
            vec![raf_core::scene::SceneVariable {
                name: "camera".into(),
                value: VariableValue::Entity(None),
            }],
        );
        let uuid = scene.get(camera).unwrap().uuid;
        let action = select_action(
            &scene,
            &[id],
            "inspector.parameter-ref.0.0",
            &uuid.to_string(),
        )
        .unwrap();
        let crate::commands::ParsedInput::Command(command) =
            crate::commands::parse_console_input(action.strip_prefix("console.submit:").unwrap())
                .unwrap()
        else {
            panic!("command expected");
        };
        assert_eq!(command.name, "behavior.parameter");
        assert_eq!(command.arg("source"), Some("scripts/camera.rhai"));
        assert_eq!(command.arg("value"), Some(uuid.to_string().as_str()));
        let before = raf_core::agent_context::scene_fingerprint(&scene);
        let mut context = crate::commands::script::ScriptCommandContext {
            scene: &mut scene,
            assets_root: None,
        };
        assert!(crate::commands::behavior::execute(&command, &mut context, false).changed);
        assert!(raf_core::agent_context::scene_diff(
            &before,
            &raf_core::agent_context::scene_fingerprint(&scene)
        )
        .is_some());
        assert!(
            parameter_action(&scene, &[camera], &format!("remove:{}:0:0", id.0), |_| {
                String::new()
            })
            .is_none()
        );
        let invalid =
            select_action(&scene, &[id], "inspector.parameter-ref.0.0", "not-present").unwrap();
        let crate::commands::ParsedInput::Command(command) =
            crate::commands::parse_console_input(invalid.strip_prefix("console.submit:").unwrap())
                .unwrap()
        else {
            panic!("command expected");
        };
        let before = ron::to_string(&scene).unwrap();
        let mut context = crate::commands::script::ScriptCommandContext {
            scene: &mut scene,
            assets_root: None,
        };
        assert!(!crate::commands::behavior::execute(&command, &mut context, false).changed);
        assert_eq!(ron::to_string(&scene).unwrap(), before);
    }
}
