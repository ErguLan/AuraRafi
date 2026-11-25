//! Spawn markers and trigger-volume authoring commands.

use super::query::{node_json, resolve_text_target, select_ids, text_argument};
use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::game::test_support::parsed_command;
    use crate::commands::game::{execute, HeadlessGameViewportPort, SceneSelectionState};
    use raf_core::scene::{ColliderType, Primitive, SceneGraph};
    use serde_json::json;

    fn context<'a>(
        scene: &'a mut SceneGraph,
        selection: &'a mut SceneSelectionState,
        viewport: &'a mut HeadlessGameViewportPort,
    ) -> GameCommandContext<'a> {
        GameCommandContext {
            scene,
            selection,
            viewport,
        }
    }

    #[test]
    fn spawn_marker_attach_update_and_remove_are_real_components() {
        let mut scene = SceneGraph::new();
        let id = scene.add_root_with_primitive("Spawn", Primitive::Cube);
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let mut ctx = context(&mut scene, &mut selection, &mut viewport);

        let attach = parsed_command(
            "game.spawn",
            json!({"action":"attach","target":"Spawn","tag":"player.start","radius":0.8}),
        );
        let output = execute("game.spawn", &attach, &mut ctx);
        assert!(output.changed, "{:?}", output.lines);
        let marker = ctx.scene.get(id).unwrap().spawn_marker.clone().unwrap();
        assert_eq!(marker.tag, "player.start");
        assert!((marker.radius - 0.8).abs() < 1e-6);

        let remove = parsed_command("game.spawn", json!({"action":"remove","target":"Spawn"}));
        let output = execute("game.spawn", &remove, &mut ctx);
        assert!(output.changed);
        assert!(ctx.scene.get(id).unwrap().spawn_marker.is_none());
    }

    #[test]
    fn trigger_attach_creates_a_static_aabb_trigger_without_touching_geometry() {
        let mut scene = SceneGraph::new();
        let id = scene.add_root_with_primitive("Zone", Primitive::Cube);
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let mut ctx = context(&mut scene, &mut selection, &mut viewport);

        let attach = parsed_command("game.trigger", json!({"action":"attach","target":"Zone"}));
        let output = execute("game.trigger", &attach, &mut ctx);
        assert!(output.changed, "{:?}", output.lines);
        let node = ctx.scene.get(id).unwrap();
        assert!(node.rigid_body.enabled);
        assert!(node.rigid_body.is_trigger);
        assert_eq!(
            node.rigid_body.body_type,
            raf_core::scene::RigidBodyType::Static
        );
        assert_eq!(node.collider.collider_type, ColliderType::Aabb);
        assert_eq!(node.primitive, Primitive::Cube);

        let remove = parsed_command("game.trigger", json!({"action":"remove","target":"Zone"}));
        let output = execute("game.trigger", &remove, &mut ctx);
        assert!(output.changed);
        assert!(!ctx.scene.get(id).unwrap().rigid_body.is_trigger);
    }
}

pub(super) fn spawn(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let result = (|| -> Result<CommandOutput, String> {
        let action = text_argument(command, "action")
            .unwrap_or_else(|| "attach".into())
            .trim()
            .to_ascii_lowercase();
        let target = text_argument(command, "target").unwrap_or_else(|| "selected".into());
        let id = if target == "selected" {
            ctx.selection.selected_node
        } else {
            resolve_text_target(&target, ctx.scene)
        }
        .ok_or_else(|| format!("Spawn target '{target}' not found."))?;
        let node = ctx
            .scene
            .get(id)
            .ok_or("Spawn target is not a live node.")?;
        match action.as_str() {
            "remove" => {
                if node.spawn_marker.is_none() {
                    return Ok(CommandOutput::info(
                        "Spawn marker already absent",
                        vec![format!("id: {}", id.0)],
                        node_json(id, node, ctx.scene),
                    ));
                }
                if let Some(node) = ctx.scene.get_mut(id) {
                    node.spawn_marker = None;
                }
                let node = ctx.scene.get(id).expect("live spawn node");
                Ok(CommandOutput::changed(
                    "Spawn marker removed",
                    vec![format!("id: {}", id.0)],
                    node_json(id, node, ctx.scene),
                ))
            }
            "attach" | "update" | "set" => {
                let mut marker = node.spawn_marker.clone().unwrap_or_default();
                if let Some(raw) = text_argument(command, "tag") {
                    marker.tag = raw;
                }
                if let Some(raw) = text_argument(command, "radius") {
                    marker.radius = raw
                        .parse::<f32>()
                        .map_err(|_| "spawn.radius must be a finite number.".to_string())?;
                }
                if let Some(raw) = text_argument(command, "enabled") {
                    marker.enabled = !matches!(
                        raw.trim().to_ascii_lowercase().as_str(),
                        "false" | "0" | "off" | "no"
                    );
                }
                marker.validate().map_err(str::to_string)?;
                if action == "update" && node.spawn_marker.is_none() {
                    return Err(
                        "Spawn update requires an existing marker; use action=attach.".into(),
                    );
                }
                let before = node.spawn_marker.clone();
                if before.as_ref() == Some(&marker) {
                    return Ok(CommandOutput::info(
                        "Spawn marker unchanged",
                        vec![format!("id: {}", id.0)],
                        node_json(id, node, ctx.scene),
                    ));
                }
                if let Some(node) = ctx.scene.get_mut(id) {
                    node.spawn_marker = Some(marker.clone());
                }
                select_ids(ctx, vec![id]);
                let node = ctx.scene.get(id).expect("live spawn node");
                Ok(CommandOutput::changed(
                    "Spawn marker configured",
                    vec![
                        format!("id: {}", id.0),
                        format!("tag: {}", marker.tag),
                        format!("radius: {:.2}", marker.radius),
                        format!("enabled: {}", marker.enabled),
                    ],
                    node_json(id, node, ctx.scene),
                ))
            }
            other => Err(format!(
                "Unknown spawn action '{other}'. Use attach, update or remove."
            )),
        }
    })();
    result.unwrap_or_else(|error| CommandOutput::error("Spawn", error))
}

pub(super) fn trigger(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let result = (|| -> Result<CommandOutput, String> {
        let action = text_argument(command, "action")
            .unwrap_or_else(|| "attach".into())
            .trim()
            .to_ascii_lowercase();
        let target = text_argument(command, "target").unwrap_or_else(|| "selected".into());
        let id = if target == "selected" {
            ctx.selection.selected_node
        } else {
            resolve_text_target(&target, ctx.scene)
        }
        .ok_or_else(|| format!("Trigger target '{target}' not found."))?;
        let node = ctx
            .scene
            .get(id)
            .ok_or("Trigger target is not a live node.")?;
        match action.as_str() {
            "remove" => {
                if !node.rigid_body.is_trigger {
                    return Ok(CommandOutput::info(
                        "Trigger already absent",
                        vec![format!("id: {}", id.0)],
                        node_json(id, node, ctx.scene),
                    ));
                }
                if let Some(node) = ctx.scene.get_mut(id) {
                    node.rigid_body.is_trigger = false;
                }
                let node = ctx.scene.get(id).expect("live trigger node");
                Ok(CommandOutput::changed(
                    "Trigger volume disabled",
                    vec![format!("id: {}", id.0)],
                    node_json(id, node, ctx.scene),
                ))
            }
            "attach" | "update" | "set" => {
                if action == "update" && !node.rigid_body.is_trigger {
                    return Err(
                        "Trigger update requires an existing trigger; use action=attach.".into(),
                    );
                }
                let half = node.primitive.descriptor().half_extents;
                let collider_type = node.collider.collider_type;
                if let Some(node) = ctx.scene.get_mut(id) {
                    node.rigid_body.enabled = true;
                    node.rigid_body.body_type = raf_core::scene::RigidBodyType::Static;
                    node.rigid_body.is_trigger = true;
                    node.rigid_body.use_gravity = false;
                    if collider_type == raf_core::scene::ColliderType::None {
                        let points = [
                            Vec3::new(-half.x, -half.y, -half.z),
                            Vec3::new(half.x, -half.y, -half.z),
                            Vec3::new(half.x, half.y, -half.z),
                            Vec3::new(-half.x, half.y, -half.z),
                            Vec3::new(-half.x, -half.y, half.z),
                            Vec3::new(half.x, -half.y, half.z),
                            Vec3::new(half.x, half.y, half.z),
                            Vec3::new(-half.x, half.y, half.z),
                        ];
                        node.collider = raf_core::scene::Collider::auto_fit(
                            &points,
                            raf_core::scene::ColliderType::Aabb,
                        );
                        node.collider.visible_in_editor = true;
                    }
                }
                select_ids(ctx, vec![id]);
                let node = ctx.scene.get(id).expect("live trigger node");
                Ok(CommandOutput::changed(
                    "Trigger volume configured",
                    vec![
                        format!("id: {}", id.0),
                        format!("collider: {:?}", node.collider.collider_type),
                        format!("enabled: {}", node.rigid_body.is_trigger),
                    ],
                    node_json(id, node, ctx.scene),
                ))
            }
            other => Err(format!(
                "Unknown trigger action '{other}'. Use attach, update or remove."
            )),
        }
    })();
    result.unwrap_or_else(|error| CommandOutput::error("Trigger", error))
}
