use super::query::{resolve_target, structured_or_json_value};
use super::{CommandOutput, GameCommandContext, ParsedCommand};
use raf_core::scene::animation::AnimationRig;
use raf_core::scene::{SceneNode, SceneNodeId};
use raf_runtime::animation::AnimationPlayback;
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Component, Path};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportClip {
    asset: String,
    clip: usize,
    bindings: BTreeMap<usize, Vec<String>>,
    name: Option<String>,
    #[serde(default)]
    looping: bool,
}

pub(super) fn update(
    command: &ParsedCommand,
    ctx: &GameCommandContext<'_>,
    id: SceneNodeId,
    staged: &SceneNode,
) -> Result<Option<Option<AnimationRig>>, String> {
    let has_rig =
        command.structured_arg("animation").is_some() || command.arg("animation").is_some();
    let clip = structured_or_json_value(command, "animation_clip", "game.update.animation_clip")?;
    if !has_rig && clip.is_none() {
        return Ok(None);
    }
    let mut rig: Option<AnimationRig> = if has_rig {
        structured_or_json_value(command, "animation", "game.update.animation")?
            .map(serde_json::from_value)
            .transpose()
            .map_err(|error| error.to_string())?
            .flatten()
    } else {
        staged.animation.clone()
    };
    if let Some(clip) = clip {
        let import: ImportClip = serde_json::from_value(clip).map_err(|error| error.to_string())?;
        let asset = asset_path(Path::new(&import.asset))?;
        let mut layer =
            raf_assets::animation_import::import_clip(&asset, import.clip, &import.bindings)?;
        layer.looping = import.looping;
        if let Some(name) = import.name {
            layer.name = name;
        }
        rig.get_or_insert_with(|| AnimationRig {
            layers: Vec::new(),
            impact: None,
        })
        .layers
        .push(layer);
    }
    let mut scene = ctx.scene.clone();
    let node = scene.get_mut(id).ok_or("Animation target not found")?;
    node.rigid_body = staged.rigid_body.clone();
    node.collider = staged.collider.clone();
    node.animation = rig.clone();
    AnimationPlayback::prepare(&scene)?;
    Ok(Some(rig))
}

fn asset_path(file: &Path) -> Result<std::path::PathBuf, String> {
    if !file
        .components()
        .all(|part| matches!(part, Component::Normal(_)))
        || !file
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("glb"))
    {
        return Err("Animation clip must be a GLB path relative to project assets".into());
    }
    let root = super::world::active_project_root()
        .ok_or("Animation import requires an open project")?
        .canonicalize()
        .map_err(|err| err.to_string())?;
    let assets = root
        .join("assets")
        .canonicalize()
        .map_err(|err| err.to_string())?;
    let file = assets
        .join(file)
        .canonicalize()
        .map_err(|err| err.to_string())?;
    if !assets.starts_with(&root) || !file.starts_with(&assets) || !file.is_file() {
        return Err("Animation clip resolves outside project assets or is not a file".into());
    }
    Ok(file)
}

pub(super) fn sample(command: &ParsedCommand, ctx: &GameCommandContext<'_>) -> CommandOutput {
    let result = (|| -> Result<_, String> {
        let target = resolve_target(command, ctx.scene, ctx.selection)
            .ok_or("Animation target not found")?;
        let rig = ctx
            .scene
            .get(target)
            .and_then(|node| node.animation.as_ref())
            .ok_or("Target has no animation rig")?;
        let seconds = command
            .arg("time")
            .unwrap_or("0")
            .parse::<f64>()
            .map_err(|error| error.to_string())?;
        let speed = command
            .arg("speed")
            .unwrap_or("0")
            .parse::<f32>()
            .map_err(|error| error.to_string())?;
        let mut playback = AnimationPlayback::prepare(ctx.scene)?;
        let poses: Vec<_> = playback.sample(seconds, speed)?.filter(|pose| pose.rig == target).filter_map(|pose| {
            let entry = ctx.scene.get(pose.node)?;
            Some(json!({ "uuid": entry.uuid, "path": ctx.scene.node_path(pose.node), "transform": pose.transform }))
        }).collect();
        Ok(json!({"time": seconds, "speed": speed, "rig": rig, "poses": poses}))
    })();
    match result {
        Ok(data) => CommandOutput::info("Animation sample", Vec::new(), data),
        Err(error) => CommandOutput::error("Animation sample", error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::game::{
        execute, test_support::parsed_command, HeadlessGameViewportPort, SceneSelectionState,
    };
    use raf_core::SceneGraph;

    #[test]
    fn animation_command_authoring_sampling_and_removal_use_the_scene_kernel() {
        let mut scene = SceneGraph::new();
        let owner = scene.add_root("Puppet");
        let head = scene.add_child(owner, "Head");
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };
        let command = |name, value| parsed_command(name, value);
        let rig = |path| {
            json!({"target":"/Puppet","animation":{"layers":[{"name":"Lift","duration":1,
            "tracks":[{"path":[path],"channel":"position","keys":[{"time":0,"value":[0,0,0]},
                {"time":1,"value":[0,2,0]}]}]}]}})
        };
        let invalid = execute(
            "game.update",
            &command("game.update", rig("Missing")),
            &mut ctx,
        );
        assert!(!invalid.changed);
        assert!(ctx.scene.get(owner).unwrap().animation.is_none());
        let added = execute(
            "game.update",
            &command("game.update", rig("Head")),
            &mut ctx,
        );
        assert!(added.changed, "{added:?}");
        let sampled = execute(
            "game.animation.sample",
            &command(
                "game.animation.sample",
                json!({"target":"/Puppet","time":0.5}),
            ),
            &mut ctx,
        );
        assert!(!sampled.changed);
        assert_eq!(
            sampled.json["poses"][0]["transform"]["position"],
            json!([0.0, 1.0, 0.0])
        );
        assert_eq!(ctx.scene.get(head).unwrap().position, glam::Vec3::ZERO);
        assert!(
            execute(
                "game.update",
                &command("game.update", json!({"target":"/Puppet","animation":null})),
                &mut ctx
            )
            .changed
        );
        assert!(ctx.scene.get(owner).unwrap().animation.is_none());
    }
}
