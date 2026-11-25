use super::query::{resolve_target, structured_or_json_value};
use super::{CommandOutput, GameCommandContext, ParsedCommand};
use raf_assets::animation_asset;
use raf_core::scene::animation::AnimationRig;
use raf_core::scene::{SceneGraph, SceneNode, SceneNodeId};
use raf_runtime::animation::AnimationPlayback;
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Component, Path};

mod bake;
mod retarget;
pub(crate) use bake::bake_rig;

pub(super) fn retarget(command: &ParsedCommand, ctx: &GameCommandContext<'_>) -> CommandOutput {
    let result = (|| -> Result<_, String> {
        let destination =
            resolve_target(command, ctx.scene, ctx.selection).ok_or("Destination rig not found")?;
        let source_name = command.arg("source").ok_or("Source rig is required")?;
        let source = super::query::resolve_text_target(source_name, ctx.scene)
            .ok_or("Source rig not found")?;
        let mappings =
            structured_or_json_value(command, "bindings", "animation retarget bindings")?
                .ok_or("Retarget bindings are required")?;
        let mappings: Vec<retarget::Binding> =
            serde_json::from_value(mappings).map_err(|error| error.to_string())?;
        let mut scene = ctx.scene.clone();
        if scene
            .get(source)
            .is_some_and(|node| node.animation_asset.is_some())
        {
            let root = super::world::active_project_root()
                .ok_or("Animation asset lookup requires an open project")?;
            animation_asset::resolve_scene(&root, &mut scene)?;
        }
        let rig = scene
            .get(source)
            .and_then(|node| node.animation.as_ref())
            .ok_or("Source rig has no animation")?;
        Ok(json!({"animation": retarget::retarget(&scene, source, destination, rig, &mappings)?}))
    })();
    match result {
        Ok(data) => CommandOutput::info("Retargeted transform clip", Vec::new(), data),
        Err(error) => CommandOutput::error("Retarget animation", error),
    }
}

pub(super) fn bake(command: &ParsedCommand, ctx: &GameCommandContext<'_>) -> CommandOutput {
    let result = (|| -> Result<_, String> {
        let owner = resolve_target(command, ctx.scene, ctx.selection)
            .ok_or("Animation target not found")?;
        let mut scene = ctx.scene.clone();
        if scene
            .get(owner)
            .is_some_and(|node| node.animation_asset.is_some())
        {
            let root = super::world::active_project_root()
                .ok_or("Animation asset lookup requires an open project")?;
            animation_asset::resolve_scene(&root, &mut scene)?;
        }
        let rig = scene
            .get(owner)
            .and_then(|node| node.animation.as_ref())
            .ok_or("The target has no animation rig")?;
        let fps = command
            .arg("fps")
            .unwrap_or("30")
            .parse::<u32>()
            .map_err(|error| error.to_string())?;
        let baked = bake_rig(&scene, owner, rig, fps)?;
        Ok(json!({"animation": baked, "fps": fps}))
    })();
    match result {
        Ok(data) => CommandOutput::info("Baked pose take", Vec::new(), data),
        Err(error) => CommandOutput::error("Bake pose take", error),
    }
}

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

pub(super) struct AnimationBinding {
    pub rig: Option<AnimationRig>,
    pub asset: Option<String>,
}

pub(super) fn update(
    command: &ParsedCommand,
    ctx: &GameCommandContext<'_>,
    id: SceneNodeId,
    staged: &SceneNode,
) -> Result<Option<AnimationBinding>, String> {
    let has_rig =
        command.structured_arg("animation").is_some() || command.arg("animation").is_some();
    let clip = structured_or_json_value(command, "animation_clip", "game.update.animation_clip")?;
    let link = command
        .structured_arg("animation_asset")
        .map(|value| {
            if value.is_null() {
                Ok(None)
            } else {
                value
                    .as_str()
                    .map(animation_asset::reference)
                    .map(Some)
                    .ok_or("Expected an animation asset path or null")
            }
        })
        .or_else(|| {
            command
                .arg("animation_asset")
                .map(|value| Ok((value != "null").then(|| animation_asset::reference(value))))
        })
        .transpose()?;
    if !has_rig && clip.is_none() && link.is_none() {
        return Ok(None);
    }
    if link.is_some() && (has_rig || clip.is_some()) {
        return Err("Set animation_asset or an inline animation in one operation".into());
    }
    if clip.is_some() && !has_rig && staged.animation_asset.is_some() {
        return Err("Choose an inline rig before appending a GLB clip".into());
    }
    let mut rig: Option<AnimationRig> = if has_rig {
        structured_or_json_value(command, "animation", "game.update.animation")?
            .map(serde_json::from_value)
            .transpose()
            .map_err(|error| error.to_string())?
            .flatten()
    } else if link.is_none() {
        staged.animation.clone()
    } else {
        None
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
            contacts: Vec::new(),
            motion_match: None,
            physics_follow: Vec::new(),
        })
        .layers
        .push(layer);
    }
    let mut scene = ctx.scene.clone();
    let node = scene.get_mut(id).ok_or("Animation target not found")?;
    node.rigid_body = staged.rigid_body.clone();
    node.collider = staged.collider.clone();
    node.animation = rig.clone();
    let asset = link.flatten();
    node.animation_asset = asset.clone();
    if rig.is_some() || asset.is_some() {
        prepare_snapshot(&mut scene, super::world::active_project_root().as_deref())?;
    }
    Ok(Some(AnimationBinding { rig, asset }))
}

fn prepare_snapshot(
    scene: &mut SceneGraph,
    project: Option<&Path>,
) -> Result<AnimationPlayback, String> {
    if scene
        .iter()
        .any(|(_, node)| !node.name.is_empty() && node.animation_asset.is_some())
    {
        animation_asset::resolve_scene(
            project.ok_or("Animation asset lookup requires an open project")?,
            scene,
        )?;
    }
    AnimationPlayback::prepare(scene)
}

pub(crate) fn validate_link(
    scene: &SceneGraph,
    id: SceneNodeId,
    project: &Path,
    asset: Option<&str>,
) -> Result<(), String> {
    if asset.is_none() {
        return Ok(());
    }
    let mut snapshot = scene.clone();
    let target = snapshot.get_mut(id).ok_or("Animation target was removed")?;
    target.animation = None;
    target.animation_asset = asset.map(animation_asset::reference);
    prepare_snapshot(&mut snapshot, Some(project))?;
    Ok(())
}

pub(crate) fn preview(
    scene: &SceneGraph,
    id: SceneNodeId,
    rig: &AnimationRig,
    seconds: f64,
) -> Result<SceneGraph, String> {
    let (mut snapshot, mut playback) = prepare_preview(scene, id, rig)?;
    sample_preview(&mut snapshot, &mut playback, id, seconds)?;
    Ok(snapshot)
}

pub(crate) fn prepare_preview(
    scene: &SceneGraph,
    id: SceneNodeId,
    rig: &AnimationRig,
) -> Result<(SceneGraph, AnimationPlayback), String> {
    let mut snapshot = scene.clone();
    let target = snapshot.get_mut(id).ok_or("Animation target was removed")?;
    target.animation = Some(rig.clone());
    let playback = AnimationPlayback::prepare(&snapshot)?;
    Ok((snapshot, playback))
}

pub(crate) fn sample_preview(
    snapshot: &mut SceneGraph,
    playback: &mut AnimationPlayback,
    id: SceneNodeId,
    seconds: f64,
) -> Result<(), String> {
    for pose in playback.sample(seconds, 0.0)?.filter(|pose| pose.rig == id) {
        if let Some(node) = snapshot.get_mut(pose.node) {
            node.position = pose.transform.position;
            node.rotation = pose.transform.rotation;
            node.scale = pose.transform.scale;
        }
    }
    Ok(())
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
        let mut scene = ctx.scene.clone();
        let mut playback =
            prepare_snapshot(&mut scene, super::world::active_project_root().as_deref())?;
        let rig = scene
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
        let poses: Vec<_> = playback.sample(seconds, speed)?.filter(|pose| pose.rig == target).filter_map(|pose| {
            let entry = ctx.scene.get(pose.node)?;
            Some(json!({ "uuid": entry.uuid, "path": ctx.scene.node_path(pose.node), "transform": pose.transform }))
        }).collect();
        Ok(json!({"time": seconds, "speed": speed, "rig": rig,
            "animation_asset": scene.get(target).and_then(|node| node.animation_asset.as_ref()), "poses": poses}))
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
