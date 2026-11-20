//! Converts embedded GLB transform clips into the engine's authored tracks.

use glam::{Quat, Vec3};
use gltf::animation::{util::ReadOutputs, Interpolation};
use raf_core::scene::animation::{degrees, AnimationChannel, AnimationKey, AnimationLayer};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_TRACKS: usize = 512;
const MAX_KEYS: usize = 32_768;

pub fn import_clip(
    file: &Path,
    clip: usize,
    bindings: &BTreeMap<usize, Vec<String>>,
) -> Result<AnimationLayer, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(file)
        .map_err(|err| err.to_string())?
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|err| err.to_string())?;
    if bytes.len() as u64 > MAX_FILE_BYTES || !bytes.starts_with(b"glTF") {
        return Err("Animation import requires an embedded GLB of at most 32 MiB".into());
    }
    let asset = gltf::Gltf::from_slice(&bytes).map_err(|err| err.to_string())?;
    let blob = asset
        .blob
        .as_deref()
        .ok_or("GLB has no embedded binary data")?;
    let animation = asset
        .animations()
        .nth(clip)
        .ok_or("GLB clip index does not exist")?;
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
        let stepped = match sampler.interpolation() {
            Interpolation::Step => true,
            Interpolation::Linear => false,
            Interpolation::CubicSpline => {
                return Err("Bake cubic GLB curves to linear keys before import".into())
            }
        };
        let reader = channel.reader(|buffer| match buffer.source() {
            gltf::buffer::Source::Bin => Some(blob),
            gltf::buffer::Source::Uri(_) => None,
        });
        let times = reader
            .read_inputs()
            .ok_or("Missing embedded GLB key times")?;
        let count = sampler.input().count();
        if sampler.output().count() != count {
            return Err("GLB output is not a transform key per timestamp".into());
        }
        let outputs = reader
            .read_outputs()
            .ok_or("Missing embedded GLB key values")?;
        let (channel, keys) = match outputs {
            ReadOutputs::Translations(values) => (
                AnimationChannel::Position,
                zip_keys(times, values.map(Vec3::from)),
            ),
            ReadOutputs::Scales(values) => (
                AnimationChannel::Scale,
                zip_keys(times, values.map(Vec3::from)),
            ),
            ReadOutputs::Rotations(values) => {
                let rotations = values.into_f32().map(|q| {
                    let q = Quat::from_array(q);
                    if q.is_finite() && q.length_squared() > f32::EPSILON {
                        degrees(q.normalize())
                    } else {
                        Vec3::splat(f32::NAN)
                    }
                });
                (AnimationChannel::Rotation, zip_keys(times, rotations))
            }
            ReadOutputs::MorphTargetWeights(_) => {
                return Err("GLB morph animation tracks are not supported by transform rigs".into())
            }
        };
        let end = keys.last().ok_or("GLB animation track has no keys")?;
        if keys.len() != count {
            return Err("GLB key values are truncated".into());
        }
        duration = duration.max(end.time);
        remaining -= count;
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
            .unwrap_or_else(|| format!("Clip {clip}")),
        duration: duration.max(1.0 / 120.0),
        looping: false,
        weight: 1.0,
        start: 0.0,
        fade_in: 0.0,
        fade_out: 0.0,
        motion_speed: None,
        additive: false,
        tracks,
    };
    Ok(layer)
}

fn zip_keys(
    times: impl Iterator<Item = f32>,
    values: impl Iterator<Item = Vec3>,
) -> Vec<AnimationKey> {
    times
        .zip(values)
        .map(|(time, value)| AnimationKey { time, value })
        .collect()
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
