//! Entity-scoped point and spot light commands.

use super::query::{resolve_text_target, select_ids, text_argument};
use super::*;

pub(super) fn execute(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let result = (|| -> Result<CommandOutput, String> {
        let action = text_argument(command, "action")
            .unwrap_or_else(|| "create".into())
            .trim()
            .to_ascii_lowercase();
        let target = match action.as_str() {
            "create" | "add" => None,
            "update" | "set" | "remove" => {
                let raw = text_argument(command, "target").ok_or("Target is required.")?;
                Some(resolve_text_target(&raw, ctx.scene).ok_or("Target not found.")?)
            }
            other => {
                return Err(format!(
                    "Unknown light action '{other}'. Use create, update or remove."
                ))
            }
        };
        if action == "remove" {
            let id = target.expect("remove target");
            if ctx
                .scene
                .get(id)
                .and_then(|node| node.light.as_ref())
                .is_none()
            {
                return Ok(CommandOutput::info(
                    "Light already absent",
                    vec![],
                    serde_json::json!({"id": id.0}),
                ));
            }
            ctx.scene.get_mut(id).expect("live node").light = None;
            return Ok(CommandOutput::changed(
                "Light removed",
                vec![],
                serde_json::json!({"id": id.0}),
            ));
        }

        let mut light = target
            .and_then(|id| ctx.scene.get(id))
            .and_then(|node| node.light.clone())
            .unwrap_or_default();
        if let Some(value) = command.structured_arg("light") {
            light = serde_json::from_value(value.clone())
                .map_err(|error| format!("light is invalid: {error}"))?;
        }
        if let Some(kind) = text_argument(command, "kind") {
            light.kind = match kind.trim().to_ascii_lowercase().as_str() {
                "spot" => raf_core::scene::LightKind::Spot,
                "point" => raf_core::scene::LightKind::Point,
                other => return Err(format!("Unknown light kind '{other}'.")),
            };
        }
        for (key, slot) in [
            ("intensity", &mut light.intensity),
            ("range", &mut light.range),
            ("inner_cone", &mut light.inner_cone_degrees),
            ("outer_cone", &mut light.outer_cone_degrees),
        ] {
            if let Some(text) = text_argument(command, key) {
                *slot = text
                    .parse::<f32>()
                    .map_err(|_| format!("{key} must be a finite number."))?;
            }
        }
        if let Some(value) = command.structured_arg("color") {
            light.color = parse_light_color(value)?;
        }
        if let Some(value) = text_argument(command, "enabled") {
            light.enabled = !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "false" | "0" | "off" | "no"
            );
        }
        light.validate().map_err(str::to_string)?;

        let id = if let Some(id) = target {
            ctx.scene.get_mut(id).expect("live node").light = Some(light.clone());
            id
        } else {
            let name = text_argument(command, "name")
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| {
                    if light.kind == raf_core::scene::LightKind::Spot {
                        "Spot Light".to_string()
                    } else {
                        "Point Light".to_string()
                    }
                });
            let parent = text_argument(command, "parent")
                .filter(|parent| parent != "root")
                .map(|parent| {
                    resolve_text_target(&parent, ctx.scene)
                        .ok_or_else(|| "Parent not found.".to_string())
                })
                .transpose()?;
            let id = parent
                .map(|parent| ctx.scene.add_child(parent, &name))
                .unwrap_or_else(|| ctx.scene.add_root(&name));
            let node = ctx.scene.get_mut(id).expect("new light");
            node.light = Some(light.clone());
            node.position.y = 2.0;
            node.color =
                raf_core::scene::NodeColor::rgb(light.color[0], light.color[1], light.color[2]);
            id
        };
        select_ids(ctx, vec![id]);
        let node = ctx.scene.get(id).expect("live light");
        Ok(CommandOutput::changed(
            "Light configured",
            vec![
                format!("id: {}", id.0),
                format!("kind: {:?}", light.kind),
                format!("intensity: {:.2}", light.intensity),
                format!("range: {:.2}", light.range),
            ],
            serde_json::json!({
                "id": id.0,
                "uuid": node.uuid,
                "light": node.light,
                "active": true
            }),
        ))
    })();
    result.unwrap_or_else(|error| CommandOutput::error("Light", error))
}

fn parse_light_color(value: &Value) -> Result<[u8; 3], String> {
    let array = value
        .as_array()
        .or_else(|| value.get("values").and_then(Value::as_array))
        .ok_or_else(|| "light color must be an RGB array.".to_string())?;
    if array.len() != 3 {
        return Err("light color must contain exactly three channels.".to_string());
    }
    let mut color = [0u8; 3];
    for (index, channel) in array.iter().enumerate() {
        color[index] = channel
            .as_u64()
            .and_then(|value| u8::try_from(value).ok())
            .ok_or_else(|| "light color channels must be integers from 0 to 255.".to_string())?;
    }
    Ok(color)
}
