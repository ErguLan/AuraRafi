//! World environment authoring shared by native UI, Console, Agent, CLI and MCP.
use super::{CommandOutput, ParsedCommand};
use raf_core::{
    environment::{EnvironmentPropertyKind, EnvironmentSettings, ENVIRONMENT_PROPERTIES},
    scene::SceneGraph,
};
use serde_json::{json, Map, Value};
pub fn execute(command: &ParsedCommand, scene: &mut SceneGraph) -> CommandOutput {
    match execute_inner(command, scene) {
        Ok(output) => output,
        Err(error) => CommandOutput::error(&command.name, error),
    }
}
fn execute_inner(command: &ParsedCommand, scene: &mut SceneGraph) -> Result<CommandOutput, String> {
    let before = *scene.environment();
    if command.name == "lighting.inspect" {
        let properties: Vec<_> = ENVIRONMENT_PROPERTIES
            .iter()
            .map(|p| {
                let kind = match p.kind {
                    EnvironmentPropertyKind::Bool => json!({"type":"boolean"}),
                    EnvironmentPropertyKind::Number { min, max, .. } => {
                        json!({"type":"number","min":min,"max":max})
                    }
                    EnvironmentPropertyKind::Color => {
                        json!({"type":"color","format":"[r,g,b] sRGB bytes"})
                    }
                    EnvironmentPropertyKind::Choice(choices) => {
                        json!({"type":"string","enum":choices})
                    }
                };
                json!({"property":p.path,"schema":kind})
            })
            .collect();
        return Ok(CommandOutput::info("Lighting", vec!["World environment; angles in radians, time in seconds. One directional shadow map; GPU 256/512/1024, CPU capped at 256; hard or four-tap filter. Shadow distance/bias use world units.".into()],
            json!({"environment":before,"properties":properties,"scope":"world","shadows_supported":true,"shadow_resolutions":[256,512,1024],"shadow_cpu_max_resolution":256,"shadow_transparent_casters":false})));
    }
    let next = match command.name.as_str() {
        "lighting.set" => {
            let patch = if let Some(patch) = command.structured_arg("patch") {
                patch
                    .as_object()
                    .cloned()
                    .ok_or("patch must be an object")?
            } else if let Some(patch) = command.arg("patch") {
                serde_json::from_str::<Map<String, Value>>(patch)
                    .map_err(|e| format!("Invalid patch JSON: {e}"))?
            } else {
                let path = command.arg("property").ok_or("Missing property or patch")?;
                let value = command
                    .structured_arg("value")
                    .cloned()
                    .or_else(|| {
                        command
                            .arg("value")
                            .and_then(|v| serde_json::from_str(v).ok())
                    })
                    .ok_or(
                        "value must be valid JSON: number, boolean, quoted choice, or [r,g,b]",
                    )?;
                Map::from_iter([(path.to_string(), value)])
            };
            before.patched(&patch)?
        }
        "lighting.reset" => {
            let defaults = EnvironmentSettings::default();
            if let Some(path) = command.arg("property") {
                before.patched(&Map::from_iter([(
                    path.to_string(),
                    defaults.get_property(path)?,
                )]))?
            } else {
                defaults
            }
        }
        _ => return Err(format!("Unknown lighting command: {}", command.name)),
    };
    let changed = scene.set_environment(next);
    let mut output = CommandOutput::info(
        &command.name,
        vec![if changed {
            "World lighting updated."
        } else {
            "Lighting unchanged."
        }
        .into()],
        json!({"before":before,"after":next,"scope":"world","shadows_supported":true}),
    );
    output.changed = changed;
    Ok(output)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{parse_console_input, ParsedInput};
    #[test]
    fn lighting_commands_are_atomic_persistent_and_do_not_touch_geometry() {
        let mut scene = SceneGraph::new();
        let geometry = scene.geometry_revision();
        let ParsedInput::Command(cmd) =
            parse_console_input("/lighting.set patch='{\"mode\":\"scene\",\"sun.intensity\":1.5,\"shadows.enabled\":true,\"shadows.resolution\":512}'")
                .unwrap()
        else {
            panic!()
        };
        assert!(execute(&cmd, &mut scene).changed);
        assert_eq!(scene.geometry_revision(), geometry);
        let roundtrip: SceneGraph = ron::from_str(&ron::to_string(&scene).unwrap()).unwrap();
        assert_eq!(scene.environment(), roundtrip.environment());
        let before = *scene.environment();
        let ParsedInput::Command(cmd) = parse_console_input(
            "/lighting.set patch='{\"sun.enabled\":false,\"sun.intensity\":-1}'",
        )
        .unwrap() else {
            panic!()
        };
        assert!(!execute(&cmd, &mut scene).changed);
        assert_eq!(scene.environment(), &before);
    }
    #[test]
    fn lighting_gateway_preview_confirm_diff_and_history_are_real() {
        use crate::commands::gateway::{CommandGateway, ParsedCommandExecutor};
        use raf_core::{CommandEndpoint, CommandSource, EngineCommandRequest};
        struct Executor {
            scene: SceneGraph,
        }
        impl ParsedCommandExecutor for Executor {
            fn execute_parsed(&mut self, c: &ParsedCommand) -> CommandOutput {
                execute(c, &mut self.scene)
            }
            fn preview_parsed(&mut self, c: &ParsedCommand) -> CommandOutput {
                execute(c, &mut self.scene.clone())
            }
        }
        let before = SceneGraph::new();
        let mut gateway = CommandGateway::new(Executor {
            scene: before.clone(),
        });
        let mut request = EngineCommandRequest::new(
            "lighting.set",
            json!({"patch":{"mode":"scene"}}),
            CommandSource::Mcp,
        );
        assert!(!gateway.execute(request.clone()).ok);
        request.dry_run = true;
        assert!(gateway.execute(request.clone()).ok);
        assert_eq!(gateway.executor.scene.environment(), before.environment());
        request.dry_run = false;
        request.confirm = true;
        let response = gateway.execute(request);
        assert!(response.ok && response.changed);
        let diff = raf_core::agent_context::scene_diff(
            &raf_core::agent_context::scene_fingerprint(&before),
            &raf_core::agent_context::scene_fingerprint(&gateway.executor.scene),
        )
        .unwrap();
        assert_eq!(diff["environment_changed"], json!(true));
        assert_eq!(diff["created"], json!([]));
        let mut history = crate::scene_history::SceneHistory::default();
        history.record_if_changed(before.clone(), &gateway.executor.scene);
        assert!(history.undo(&mut gateway.executor.scene));
        assert_eq!(gateway.executor.scene.environment(), before.environment());
        assert!(history.redo(&mut gateway.executor.scene));
        assert_ne!(gateway.executor.scene.environment(), before.environment());
    }
}
