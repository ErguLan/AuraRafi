//! Explicit rest-pose mapping for transform clips between scene hierarchies.

use raf_core::scene::animation::{degrees, resolve_path, to_quat, AnimationChannel, AnimationRig};
use raf_core::scene::{SceneGraph, SceneNodeId};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Binding {
    source: Vec<String>,
    target: Vec<String>,
    translation_ratio: Option<f32>,
}

pub(super) fn retarget(
    scene: &SceneGraph,
    source: SceneNodeId,
    destination: SceneNodeId,
    rig: &AnimationRig,
    mappings: &[Binding],
) -> Result<AnimationRig, String> {
    if mappings.is_empty() || mappings.len() > 512 {
        return Err("Retarget bindings are outside the allowed count".into());
    }
    let mut lookup = HashMap::new();
    let mut targets = HashSet::new();
    for mapping in mappings {
        let original = resolve_path(scene, source, &mapping.source)?;
        let mapped = resolve_path(scene, destination, &mapping.target)?;
        if lookup
            .insert(&mapping.source, (mapping, original, mapped))
            .is_some()
            || !targets.insert(mapped)
        {
            return Err("Retarget bindings must be one-to-one".into());
        }
    }
    let mut result = rig.clone();
    result.impact = None;
    result.contacts.clear();
    result.physics_follow.clear();
    if let Some(search) = &mut result.motion_match {
        search.pose_path = lookup
            .get(&search.pose_path)
            .ok_or("Missing retarget path for the motion feature")?
            .0
            .target
            .clone();
    }
    for track in result.layers.iter_mut().flat_map(|layer| &mut layer.tracks) {
        let (binding, original, mapped) = *lookup
            .get(&track.path)
            .ok_or_else(|| format!("Missing retarget binding for {:?}", track.path))?;
        let from = scene.get(original).ok_or("Source joint was removed")?;
        let to = scene.get(mapped).ok_or("Target joint was removed")?;
        let length = from.position.length();
        let ratio = binding.translation_ratio.unwrap_or_else(|| {
            if !binding.source.is_empty() && length > 1e-4 {
                to.position.length() / length
            } else {
                1.0
            }
        });
        if !(0.0..=100.0).contains(&ratio) {
            return Err("Invalid retarget translation ratio".into());
        }
        if track.channel == AnimationChannel::Scale && from.scale.abs().min_element() <= 1e-6 {
            return Err("Retarget source has singular scale".into());
        }
        for key in &mut track.keys {
            key.value = match track.channel {
                AnimationChannel::Position => to.position + (key.value - from.position) * ratio,
                AnimationChannel::Rotation => degrees(
                    to_quat(to.rotation) * to_quat(from.rotation).inverse() * to_quat(key.value),
                ),
                AnimationChannel::Scale => to.scale * key.value / from.scale,
            };
        }
        track.path.clone_from(&binding.target);
    }
    result.validate()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn animation_retarget_scales_motion_relative_to_the_two_rest_poses() {
        let mut scene = SceneGraph::new();
        let source = scene.add_root("Source");
        let foot = scene.add_child(source, "Foot");
        scene.get_mut(foot).unwrap().position = Vec3::Y;
        let destination = scene.add_root("Destination");
        let target = scene.add_child(destination, "Shoe");
        scene.get_mut(target).unwrap().position = 2.0 * Vec3::Y;
        let rig = serde_json::from_value(serde_json::json!({"layers":[{"name":"Walk","duration":1.0,"tracks":[{
            "path":["Foot"],"channel":"position","keys":[{"time":0.0,"value":[0,1,0]},{"time":1.0,"value":[0,2,0]}]
        }]}]})).unwrap();
        let mappings = vec![Binding {
            source: vec!["Foot".into()],
            target: vec!["Shoe".into()],
            translation_ratio: None,
        }];
        let result = retarget(&scene, source, destination, &rig, &mappings).unwrap();
        assert_eq!(result.layers[0].tracks[0].path, ["Shoe"]);
        assert_eq!(result.layers[0].tracks[0].sample(0.5), 3.0 * Vec3::Y);
        let baked =
            crate::commands::game::animation::bake_rig(&scene, destination, &result, 30).unwrap();
        assert_eq!(baked.layers[0].tracks[0].keys.len(), 31);
        assert_eq!(baked.layers[0].tracks[0].sample(0.5), 3.0 * Vec3::Y);
    }
}
