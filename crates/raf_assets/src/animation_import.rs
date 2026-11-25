//! Converts glTF transform clips into the engine's authored tracks.

use glam::{Quat, Vec3, Vec4};
use gltf::animation::{util::ReadOutputs, Interpolation};
use raf_core::scene::animation::{degrees, AnimationChannel, AnimationKey, AnimationLayer};
use std::collections::BTreeMap;
use std::path::Path;

const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_TRACKS: usize = 512;
const MAX_KEYS: usize = 32_768;

pub fn import_clip(
    file: &Path,
    clip: usize,
    bindings: &BTreeMap<usize, Vec<String>>,
) -> Result<AnimationLayer, String> {
    let bytes = crate::model_asset::read_bounded(file, MAX_FILE_BYTES)?;
    let asset = gltf::Gltf::from_slice(&bytes).map_err(|err| err.to_string())?;
    let buffers = crate::skeletal_model::read_buffers(&asset, file)?;
    let animation = asset
        .animations()
        .nth(clip)
        .ok_or("glTF clip index does not exist")?;
    read_layer(animation, bindings, |buffer| {
        buffers.get(buffer.index()).map(Vec::as_slice)
    })
}

pub(crate) fn read_layer<'a, 's>(
    animation: gltf::Animation<'a>,
    bindings: &BTreeMap<usize, Vec<String>>,
    data: impl Clone + Fn(gltf::Buffer<'a>) -> Option<&'s [u8]>,
) -> Result<AnimationLayer, String> {
    let mut tracks = Vec::new();
    let mut duration: f32 = 0.0;
    let mut remaining = MAX_KEYS;
    for channel in animation.channels() {
        let source = channel.target().node().index();
        let Some(path) = bindings.get(&source) else {
            continue;
        };
        let sampler = channel.sampler();
        if tracks.len() == MAX_TRACKS || sampler.input().count() > remaining {
            return Err("GLB clip exceeds the animation track or key budget".into());
        }
        let cubic = sampler.interpolation() == Interpolation::CubicSpline;
        let stepped = sampler.interpolation() == Interpolation::Step;
        let reader = channel.reader(data.clone());
        let times: Vec<_> = reader
            .read_inputs()
            .ok_or("Missing glTF key times")?
            .collect();
        let count = sampler.input().count();
        if times.len() != count
            || times.is_empty()
            || times.iter().any(|&time| !(0.0..=3600.0).contains(&time))
            || times.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err("Invalid glTF key times".into());
        }
        let outputs = reader.read_outputs().ok_or("Missing glTF key values")?;
        if matches!(outputs, ReadOutputs::MorphTargetWeights(_)) {
            continue;
        }
        if sampler.output().count() != count * if cubic { 3 } else { 1 } {
            return Err("GLB output is not a transform key per timestamp".into());
        }
        let (channel, values): (_, Vec<_>) = match outputs {
            ReadOutputs::Translations(values) => (
                AnimationChannel::Position,
                values
                    .map(|value| Vec3::from_array(value).extend(0.0))
                    .collect(),
            ),
            ReadOutputs::Scales(values) => (
                AnimationChannel::Scale,
                values
                    .map(|value| Vec3::from_array(value).extend(0.0))
                    .collect(),
            ),
            ReadOutputs::Rotations(values) => (
                AnimationChannel::Rotation,
                values.into_f32().map(Vec4::from_array).collect(),
            ),
            ReadOutputs::MorphTargetWeights(_) => continue,
        };
        if values.len() != sampler.output().count() || values.iter().any(|value| !value.is_finite())
        {
            return Err("Invalid glTF key values".into());
        }
        let keys = bake_keys(&times, &values, channel, cubic, remaining)?;
        let end = keys.last().ok_or("GLB animation track has no keys")?;
        duration = duration.max(end.time);
        remaining -= keys.len();
        tracks.push(raf_core::scene::animation::AnimationTrack {
            path: path.clone(),
            channel,
            stepped,
            keys,
        });
    }
    if tracks.is_empty() {
        return Err("No transform tracks matched the GLB node bindings".into());
    }
    let layer = AnimationLayer {
        name: animation
            .name()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("Clip {}", animation.index())),
        duration: duration.max(1.0 / 120.0),
        looping: false,
        weight: 1.0,
        start: 0.0,
        fade_in: 0.0,
        fade_out: 0.0,
        motion_speed: None,
        motion_heading_degrees: 0.0,
        action: None,
        additive: false,
        tracks,
    };
    Ok(layer)
}

fn bake_keys(
    times: &[f32],
    values: &[Vec4],
    channel: AnimationChannel,
    cubic: bool,
    budget: usize,
) -> Result<Vec<AnimationKey>, String> {
    let mut keys = Vec::with_capacity(times.len());
    let mut push = |time, value: Vec4| -> Result<(), String> {
        if keys.len() >= budget {
            return Err("Baked glTF curve exceeds the key budget".into());
        }
        let value = if channel == AnimationChannel::Rotation {
            if value.length_squared() <= f32::EPSILON || !value.is_finite() {
                return Err("Invalid glTF rotation".into());
            }
            degrees(Quat::from_array(value.normalize().to_array()))
        } else {
            value.truncate()
        };
        if !value.is_finite() {
            return Err("Baked glTF curve has a nonfinite key".into());
        }
        keys.push(AnimationKey { time, value });
        Ok(())
    };
    if !cubic {
        for (&time, &value) in times.iter().zip(values) {
            push(time, value)?;
        }
        return Ok(keys);
    }
    for (index, pair) in times.windows(2).enumerate() {
        let duration = pair[1] - pair[0];
        let steps = (duration * 60.0).ceil().max(1.0) as usize;
        let a = values[index * 3 + 1];
        let b = values[(index + 1) * 3 + 1];
        let outgoing = values[index * 3 + 2] * duration;
        let incoming = values[(index + 1) * 3] * duration;
        for step in 0..steps {
            let t = step as f32 / steps as f32;
            let t2 = t * t;
            let t3 = t2 * t;
            let value = (2.0 * t3 - 3.0 * t2 + 1.0) * a
                + (t3 - 2.0 * t2 + t) * outgoing
                + (-2.0 * t3 + 3.0 * t2) * b
                + (t3 - t2) * incoming;
            push(pair[0] + duration * t, value)?;
        }
    }
    push(
        *times.last().ok_or("Empty glTF curve")?,
        values[values.len() - 2],
    )?;
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn embedded_glb_imports_stepped_translation_for_explicit_binding() {
        let mut metadata = serde_json::to_vec(&json!({
            "asset":{"version":"2.0"}, "buffers":[{"byteLength":32}],
            "bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":8},{"buffer":0,"byteOffset":8,"byteLength":24}],
            "accessors":[{"bufferView":0,"componentType":5126,"count":2,"type":"SCALAR","min":[0],"max":[1]},
                {"bufferView":1,"componentType":5126,"count":2,"type":"VEC3"}],
            "nodes":[{"name":"source"}],
            "animations":[{"name":"Lift","samplers":[{"input":0,"output":1,"interpolation":"STEP"}],
                "channels":[{"sampler":0,"target":{"node":0,"path":"translation"}}]}]
        })).unwrap();
        metadata.resize(metadata.len().next_multiple_of(4), b' ');
        let mut bytes = b"glTF".to_vec();
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&(28 + 32 + metadata.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
        bytes.extend_from_slice(b"JSON");
        bytes.extend_from_slice(&metadata);
        bytes.extend_from_slice(&32u32.to_le_bytes());
        bytes.extend_from_slice(b"BIN\0");
        for value in [0.0_f32, 1.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        let path = std::env::temp_dir().join(format!("raf-animation-{}.glb", uuid::Uuid::new_v4()));
        std::fs::write(&path, bytes).unwrap();
        let result = import_clip(&path, 0, &BTreeMap::from([(0, vec!["Arm".to_string()])]));
        std::fs::remove_file(path).unwrap();
        let layer = result.unwrap();
        assert_eq!(layer.name, "Lift");
        assert_eq!(layer.tracks[0].path, ["Arm"]);
        assert_eq!(layer.tracks[0].sample(0.99), Vec3::ZERO);
        assert_eq!(layer.tracks[0].sample(1.0), 2.0 * Vec3::Y);
    }
}
