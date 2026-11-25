//! Camera component commands.

use super::query::*;
use super::*;

pub(super) fn camera_entity(
    command: &ParsedCommand,
    ctx: &mut GameCommandContext<'_>,
) -> CommandOutput {
    let result = (|| -> Result<CommandOutput, String> {
        let action = text_argument(command, "action").unwrap_or_else(|| "set".into());
        if !matches!(
            action.as_str(),
            "create" | "add" | "remove" | "set" | "activate" | "look_through" | "editor_view"
        ) {
            return Err(
                "Camera action must be create, add, remove, set, activate, look_through or editor_view."
                    .into(),
            );
        }
        let target = if action == "create" || action == "editor_view" {
            None
        } else {
            let id = match text_argument(command, "target") {
                Some(target) if target != "selected" => resolve_text_target(&target, ctx.scene),
                _ => ctx
                    .selection
                    .selected_node
                    .filter(|id| ctx.scene.is_valid_node(*id)),
            };
            Some(id.ok_or("Target not found.")?)
        };
        if action == "remove" {
            let id = target.expect("resolved target");
            if ctx
                .scene
                .get(id)
                .expect("live target")
                .game_camera
                .is_none()
            {
                return Ok(CommandOutput::info(
                    "Camera already absent",
                    vec![],
                    serde_json::json!({"id":id.0}),
                ));
            }
            ctx.scene.get_mut(id).expect("live target").game_camera = None;
            return Ok(CommandOutput::changed(
                "Camera removed",
                vec![],
                serde_json::json!({"id":id.0}),
            ));
        }
        if action == "editor_view" {
            ctx.viewport.set_look_through(ctx.scene, None);
            return Ok(CommandOutput::changed(
                "Editor camera restored",
                vec!["The editor navigation camera is active again.".into()],
                serde_json::json!({ "look_through": false }),
            ));
        }
        if action == "look_through" {
            let id = target.ok_or("Target not found.")?;
            let node = ctx.scene.get(id).ok_or("Target not found.")?;
            let lens = node
                .game_camera
                .clone()
                .ok_or("Target has no camera component.")?;
            let uuid = node.uuid;
            ctx.viewport.set_look_through(ctx.scene, Some(id));
            return Ok(CommandOutput::changed(
                "Camera look-through enabled",
                vec![
                    format!("uuid: {uuid}"),
                    "Press Escape in the viewport to return to the editor camera.".into(),
                ],
                serde_json::json!({
                    "id": id.0,
                    "uuid": uuid,
                    "camera": lens,
                    "look_through": true,
                    "escape_restores_editor_camera": true
                }),
            ));
        }
        let mut lens = target
            .and_then(|id| ctx.scene.get(id))
            .and_then(|n| n.game_camera.clone())
            .unwrap_or_default();
        if action == "activate" {
            let id = target.ok_or("Target not found.")?;
            let (uuid, camera) = {
                let node = ctx.scene.get_mut(id).ok_or("Target not found.")?;
                if node.game_camera.is_none() {
                    node.game_camera = Some(lens.clone());
                }
                (node.uuid, node.game_camera.clone())
            };
            select_ids(ctx, vec![id]);
            return Ok(CommandOutput::changed(
                "Camera activated",
                vec![format!("uuid: {uuid}")],
                serde_json::json!({
                    "id": id.0,
                    "uuid": uuid,
                    "active_camera": uuid,
                    "camera": camera
                }),
            ));
        }
        if action == "set"
            && target
                .and_then(|id| ctx.scene.get(id))
                .is_some_and(|n| n.game_camera.is_none())
        {
            return Err("Target has no camera component; use action=add.".into());
        }
        for (key, value) in [
            ("fov", &mut lens.fov_degrees),
            ("near", &mut lens.near),
            ("far", &mut lens.far),
            ("ortho_scale", &mut lens.ortho_scale),
        ] {
            if let Some(raw) = text_argument(command, key) {
                *value = raw
                    .parse::<f32>()
                    .map_err(|_| format!("{key} must be a finite number."))?;
            }
        }
        lens.orthographic = bool_argument(command, "orthographic", lens.orthographic)?;
        lens.validate().map_err(str::to_string)?;
        if target
            .and_then(|id| ctx.scene.get(id))
            .is_some_and(|n| n.game_camera.as_ref() == Some(&lens))
        {
            return Ok(CommandOutput::info(
                "Camera unchanged",
                vec![],
                serde_json::json!({"camera":lens}),
            ));
        }
        let parent = if action == "create" {
            text_argument(command, "parent")
                .filter(|p| p != "root")
                .map(|p| {
                    resolve_text_target(&p, ctx.scene)
                        .ok_or_else(|| "Parent not found.".to_string())
                })
                .transpose()?
        } else {
            None
        };
        let name = text_argument(command, "name")
            .unwrap_or_else(|| format!("Camera {}", ctx.scene.len() + 1));
        if action == "create" && (name.trim().is_empty() || name.len() > 256) {
            return Err("Camera name requires 1..256 bytes.".into());
        }
        let id = target.unwrap_or_else(|| {
            if let Some(parent) = parent {
                ctx.scene.add_child(parent, &name)
            } else {
                ctx.scene.add_root(&name)
            }
        });
        ctx.scene.get_mut(id).expect("live camera").game_camera = Some(lens);
        select_ids(ctx, vec![id]);
        let node = ctx.scene.get(id).expect("live camera");
        Ok(CommandOutput::changed(
            "Camera configured",
            vec![],
            serde_json::json!({
                "id":id.0,"uuid":node.uuid,"camera":node.game_camera
            }),
        ))
    })();
    result.unwrap_or_else(|error| CommandOutput::error("Camera", error))
}
