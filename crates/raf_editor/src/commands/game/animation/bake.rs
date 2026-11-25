//! Compile a sampled transform take into a reusable, independent clip.

use raf_core::scene::animation::{resolve_path, AnimationChannel, AnimationKey, AnimationRig};
use raf_core::scene::{SceneGraph, SceneNodeId};
use raf_runtime::animation::AnimationPlayback;
use std::collections::{HashMap, HashSet};

const MAX_KEYS: usize = 32_768;

pub(crate) fn bake_rig(
    scene: &SceneGraph,
    owner: SceneNodeId,
    rig: &AnimationRig,
    fps: u32,
) -> Result<AnimationRig, String> {
    if !(12..=120).contains(&fps) {
        return Err("Bake rate must be in 12–120 fps".into());
    }
    let mut snapshot = scene.clone();
    snapshot
        .get_mut(owner)
        .ok_or("The bake target was removed")?
        .animation = Some(rig.clone());
    let mut playback = AnimationPlayback::prepare(&snapshot)?;
    let duration = rig
        .layers
        .iter()
        .map(|layer| layer.start + layer.duration)
        .reduce(f32::max)
        .ok_or("There are no transform clips to bake")?;
    let mut seen = HashSet::new();
    let mut maps = Vec::new();
    let mut tracks = Vec::new();
    let frames = (duration * fps as f32).ceil() as usize;
    for track in rig.layers.iter().flat_map(|layer| &layer.tracks) {
        if seen.insert((&track.path, track.channel)) {
            let id = resolve_path(scene, owner, &track.path)?;
            maps.push(id);
            let mut output = track.clone();
            output.keys.clear();
            output.stepped = rig
                .layers
                .iter()
                .flat_map(|layer| &layer.tracks)
                .filter(|item| item.path == track.path && item.channel == track.channel)
                .all(|item| item.stepped);
            tracks.push(output);
        }
    }
    if (frames + 1).saturating_mul(tracks.len()) > MAX_KEYS {
        return Err("Bake exceeds the clip key budget".into());
    }
    for frame in 0..=frames {
        let time = (frame as f32 / fps as f32).min(duration);
        let samples: HashMap<_, _> = playback
            .sample(f64::from(time), 0.0)?
            .filter(|pose| pose.rig == owner)
            .map(|pose| (pose.node, pose.transform))
            .collect();
        for (track, id) in tracks.iter_mut().zip(&maps) {
            let transform = samples
                .get(id)
                .ok_or("Baked pose is missing a required track")?;
            let value = match track.channel {
                AnimationChannel::Position => transform.position,
                AnimationChannel::Rotation => transform.rotation,
                AnimationChannel::Scale => transform.scale,
            };
            if track.keys.last().is_none_or(|key| key.time < time) {
                track.keys.push(AnimationKey { time, value });
            }
        }
    }
    let mut layer = rig.layers[0].clone();
    layer.name = "Baked".into();
    layer.duration = duration;
    layer.start = 0.0;
    layer.fade_in = 0.0;
    layer.fade_out = 0.0;
    layer.weight = 1.0;
    layer.looping = false;
    layer.additive = false;
    layer.motion_speed = None;
    layer.action = None;
    layer.tracks = tracks;
    let baked = AnimationRig {
        layers: vec![layer],
        impact: None,
        contacts: Vec::new(),
        motion_match: None,
        physics_follow: Vec::new(),
    };
    baked.validate()?;
    Ok(baked)
}
