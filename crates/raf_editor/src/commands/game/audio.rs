//! Entity-scoped audio source commands.

use super::material::validate_component_asset_path;
use super::query::{resolve_text_target, select_ids, text_argument};
use super::*;

pub(super) fn execute(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let result = (|| -> Result<CommandOutput, String> {
        let action = text_argument(command, "action")
            .unwrap_or_else(|| "update".into())
            .trim()
            .to_ascii_lowercase();
        let target_raw = text_argument(command, "target");
        let target = match action.as_str() {
            "remove" | "update" | "attach" | "set" => {
                let raw = target_raw.as_deref().unwrap_or("selected");
                let id = if raw == "selected" {
                    ctx.selection.selected_node
                } else {
                    resolve_text_target(raw, ctx.scene)
                };
                Some(id.ok_or("Audio target not found.")?)
            }
            "preview" | "stop" => None,
            other => {
                return Err(format!(
                    "Unknown audio action '{other}'. Use attach, update, remove, preview or stop."
                ))
            }
        };
        if action == "preview" || action == "stop" {
            let path = text_argument(command, "clip")
                .or_else(|| text_argument(command, "path"))
                .unwrap_or_default();
            return Ok(CommandOutput::info(
                if action == "preview" {
                    "Audio preview requested"
                } else {
                    "Audio preview stopped"
                },
                vec![format!("clip: {path}")],
                serde_json::json!({
                    "action": action,
                    "clip": path,
                    "volume": text_argument(command, "volume")
                        .and_then(|value| value.parse::<f32>().ok())
                        .unwrap_or(1.0),
                    "looping": false
                }),
            ));
        }
        let id = target.expect("audio target");
        if action == "remove" {
            if let Some(node) = ctx.scene.get_mut(id) {
                node.audio_source = raf_core::scene::AudioSource::default();
            }
            let node = ctx.scene.get(id).expect("live node");
            return Ok(CommandOutput::changed(
                "Audio source removed",
                vec![format!("id: {}", id.0)],
                node_json(id, node, ctx.scene),
            ));
        }
        let spatial_explicit =
            command.arg("spatial").is_some() || command.structured_arg("audio").is_some();
        let mut source = ctx
            .scene
            .get(id)
            .map(|node| node.audio_source.clone())
            .unwrap_or_default();
        if let Some(value) = command.structured_arg("audio") {
            source = serde_json::from_value(value.clone())
                .map_err(|error| format!("audio is invalid: {error}"))?;
        }
        if let Some(clip) =
            text_argument(command, "clip").or_else(|| text_argument(command, "path"))
        {
            source.clip = raf_core::scene::normalize_reference(validate_component_asset_path(
                &clip,
                "audio.clip",
                &["wav", "mp3", "ogg", "flac"],
            )?);
            source.enabled = true;
        }
        if let Some(value) = text_argument(command, "volume") {
            source.volume = value
                .parse::<f32>()
                .map_err(|_| "audio.volume must be a finite number.".to_string())?;
        }
        if let Some(value) = text_argument(command, "bus") {
            source.bus = raf_core::scene::AudioBus::from_slug(&value).ok_or_else(|| {
                "audio.bus must be master, music, sfx, voice or ambient.".to_string()
            })?;
        }
        if let Some(raw) = text_argument(command, "looping") {
            source.looping = audio_bool(&raw);
        }
        if let Some(raw) = text_argument(command, "autoplay") {
            source.autoplay = audio_bool(&raw);
        }
        if let Some(raw) = text_argument(command, "spatial") {
            source.spatial = audio_bool(&raw);
        }
        if let Some(raw) = text_argument(command, "enabled") {
            source.enabled = audio_bool(&raw);
        }
        for (key, slot) in [
            ("min_distance", &mut source.min_distance),
            ("max_distance", &mut source.max_distance),
            ("rolloff", &mut source.rolloff),
        ] {
            if let Some(raw) = text_argument(command, key) {
                *slot = raw
                    .parse::<f32>()
                    .map_err(|_| format!("audio.{key} must be a finite number."))?;
            }
        }
        if action == "attach" && !spatial_explicit {
            // Newly attached sounds are spatial by default because they are
            // bound to the entity transform. Explicit spatial=false preserves
            // a 2D UI/music cue.
            source.spatial = true;
        }
        source.validate().map_err(str::to_string)?;
        if action == "attach" && source.clip.trim().is_empty() {
            return Err("Audio attach requires clip=<project audio path>.".to_string());
        }
        if let Some(node) = ctx.scene.get_mut(id) {
            node.audio_source = source.clone();
        }
        select_ids(ctx, vec![id]);
        let node = ctx.scene.get(id).expect("live audio node");
        Ok(CommandOutput::changed(
            "Audio source configured",
            vec![
                format!("id: {}", id.0),
                format!("clip: {}", source.clip),
                format!("bus: {}", source.bus.slug()),
                format!("volume: {:.2}", source.volume),
                format!("spatial: {}", source.spatial),
            ],
            node_json(id, node, ctx.scene),
        ))
    })();
    result.unwrap_or_else(|error| CommandOutput::error("Audio", error))
}

fn audio_bool(raw: &str) -> bool {
    !matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "false" | "0" | "off" | "no"
    )
}
