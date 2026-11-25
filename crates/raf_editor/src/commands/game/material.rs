//! Entity color and material texture commands.

use super::query::{node_detail_lines, parse_json_argument, resolve_target, select_ids};
use super::*;

pub(super) fn validate_component_texture(
    value: &str,
    label: &str,
) -> Result<Option<String>, String> {
    let normalized = value.trim().replace('\\', "/");
    if normalized.is_empty() {
        return Ok(None);
    }
    validate_component_asset_path(
        &normalized,
        label,
        &["png", "jpg", "jpeg", "bmp", "tga", "webp"],
    )
    .map(Some)
}

pub(super) fn validate_component_asset_path(
    value: &str,
    label: &str,
    extensions: &[&str],
) -> Result<String, String> {
    let normalized = value.trim().replace('\\', "/");
    if normalized.starts_with('/')
        || normalized.contains(':')
        || normalized
            .split('/')
            .any(|component| component.is_empty() || matches!(component, "." | ".."))
    {
        return Err(format!(
            "{label} path '{value}' must be project-relative without '.' or '..' components."
        ));
    }
    let extension = std::path::Path::new(&normalized)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .ok_or_else(|| format!("{label} path '{value}' requires a file extension."))?;
    if !extensions.iter().any(|allowed| *allowed == extension) {
        return Err(format!(
            "{label} path '{value}' has an unsupported extension; expected one of {}.",
            extensions.join(", ")
        ));
    }
    Ok(normalized)
}

pub(super) fn color_entity(
    command: &ParsedCommand,
    ctx: &mut GameCommandContext<'_>,
) -> CommandOutput {
    let Some(id) = resolve_target(command, ctx.scene, ctx.selection) else {
        return CommandOutput::error("Color entity", "Target not found.");
    };
    let color = match color_argument(command, NodeColor::rgb(255, 255, 255)) {
        Ok(Some(color)) => color,
        Ok(None) => {
            return CommandOutput::error(
                "Color entity",
                "A color value is required (for example color=#44AAFF).",
            )
        }
        Err(error) => return CommandOutput::error("Color entity", error),
    };
    if let Some(node) = ctx.scene.get_mut(id) {
        node.color = color;
    }
    select_ids(ctx, vec![id]);
    let node = ctx.scene.get(id).expect("valid node");
    CommandOutput::changed(
        format!("Color updated {}", node.name),
        node_detail_lines(id, node, ctx.scene),
        node_json(id, node, ctx.scene),
    )
}

pub(super) fn color_argument(
    command: &ParsedCommand,
    default: NodeColor,
) -> Result<Option<NodeColor>, String> {
    if let Some(value) = command
        .structured_arg("color")
        .or_else(|| command.structured_arg("color_rgba"))
    {
        return parse_color_value(value, "color").map(Some);
    }
    if let Some(raw) = command.arg("color").or_else(|| command.arg("color_rgba")) {
        return parse_color(raw).map(Some);
    }

    let has_channels = ["r", "g", "b", "a"]
        .iter()
        .any(|key| command.arg(key).is_some());
    if !has_channels {
        return Ok(None);
    }

    Ok(Some(NodeColor::rgba(
        parse_u8_component(command, "r", default.r)?,
        parse_u8_component(command, "g", default.g)?,
        parse_u8_component(command, "b", default.b)?,
        parse_u8_component(command, "a", default.a)?,
    )))
}

fn parse_u8_component(command: &ParsedCommand, key: &str, default: u8) -> Result<u8, String> {
    command
        .arg(key)
        .map(|raw| parse_u8_literal(raw, key))
        .unwrap_or(Ok(default))
}

fn parse_color(raw: &str) -> Result<NodeColor, String> {
    let raw = raw.trim();
    if let Some(hex) = raw.strip_prefix('#') {
        if hex.len() != 6 && hex.len() != 8 {
            return Err("color hex must use #RRGGBB or #RRGGBBAA.".to_string());
        }
        let r = u8::from_str_radix(&hex[0..2], 16)
            .map_err(|_| "color contains an invalid red channel.".to_string())?;
        let g = u8::from_str_radix(&hex[2..4], 16)
            .map_err(|_| "color contains an invalid green channel.".to_string())?;
        let b = u8::from_str_radix(&hex[4..6], 16)
            .map_err(|_| "color contains an invalid blue channel.".to_string())?;
        let a = if hex.len() == 8 {
            u8::from_str_radix(&hex[6..8], 16)
                .map_err(|_| "color contains an invalid alpha channel.".to_string())?
        } else {
            255
        };
        return Ok(NodeColor::rgba(r, g, b, a));
    }

    if raw.starts_with('[') || raw.starts_with('{') || raw.starts_with('"') {
        let value = parse_json_argument(raw, "color")?;
        return parse_color_value(&value, "color");
    }

    let parts = raw.split(',').map(str::trim).collect::<Vec<_>>();
    if parts.len() != 3 && parts.len() != 4 {
        return Err(
            "color must be #RRGGBB, #RRGGBBAA, or 3/4 comma-separated channels.".to_string(),
        );
    }
    let r = parse_u8_literal(parts[0], "color.r")?;
    let g = parse_u8_literal(parts[1], "color.g")?;
    let b = parse_u8_literal(parts[2], "color.b")?;
    let a = if parts.len() == 4 {
        parse_u8_literal(parts[3], "color.a")?
    } else {
        255
    };
    Ok(NodeColor::rgba(r, g, b, a))
}

fn parse_color_value(value: &Value, label: &str) -> Result<NodeColor, String> {
    match value {
        Value::Array(values) if values.len() == 3 || values.len() == 4 => {
            let r = parse_json_u8(&values[0], &format!("{label}[0]"))?;
            let g = parse_json_u8(&values[1], &format!("{label}[1]"))?;
            let b = parse_json_u8(&values[2], &format!("{label}[2]"))?;
            let a = values
                .get(3)
                .map(|value| parse_json_u8(value, &format!("{label}[3]")))
                .transpose()?
                .unwrap_or(255);
            Ok(NodeColor::rgba(r, g, b, a))
        }
        Value::Array(_) => Err(format!("{label} must contain 3 or 4 channels.")),
        Value::Object(object) => {
            if let Some(values) = object
                .get("values")
                .or_else(|| object.get("channels"))
                .or_else(|| object.get("items"))
            {
                return parse_color_value(values, label);
            }
            Ok(NodeColor::rgba(
                parse_json_object_u8(object, "r", &format!("{label}.r"))?,
                parse_json_object_u8(object, "g", &format!("{label}.g"))?,
                parse_json_object_u8(object, "b", &format!("{label}.b"))?,
                object
                    .get("a")
                    .map(|value| parse_json_u8(value, &format!("{label}.a")))
                    .transpose()?
                    .unwrap_or(255),
            ))
        }
        Value::String(raw) => parse_color(raw),
        _ => Err(format!(
            "{label} must be a color array, object, or literal."
        )),
    }
}

fn parse_json_u8(value: &Value, label: &str) -> Result<u8, String> {
    match value {
        Value::Number(number) => number
            .as_u64()
            .and_then(|value| u8::try_from(value).ok())
            .ok_or_else(|| format!("{label} must be an integer from 0 to 255.")),
        Value::String(raw) => parse_u8_literal(raw, label),
        _ => Err(format!("{label} must be an integer from 0 to 255.")),
    }
}

fn parse_json_object_u8(
    object: &serde_json::Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<u8, String> {
    object
        .get(key)
        .ok_or_else(|| format!("{label} is required."))
        .and_then(|value| parse_json_u8(value, label))
}

fn parse_u8_literal(raw: &str, label: &str) -> Result<u8, String> {
    raw.trim()
        .parse::<u8>()
        .map_err(|_| format!("{label} must be an integer from 0 to 255."))
}
