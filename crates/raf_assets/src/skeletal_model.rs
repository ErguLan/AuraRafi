//! Bounded, render-backend independent glTF skeletal assets.

use crate::model_asset::{decode_data_uri, read_bounded, MAX_MODEL_FILE_BYTES};
use glam::{Mat4, Quat, Vec3};
use raf_core::scene::animation::{degrees, AnimationLayer};
use std::collections::{BTreeMap, VecDeque};
use std::path::Path;

const MAX_NODES: usize = 512;
const MAX_JOINTS: usize = 256;
const MAX_VERTICES: usize = 100_000;
const MAX_INDICES: usize = 600_000;

pub struct RigNode {
    pub source: usize,
    pub parent: Option<usize>,
    pub name: String,
    pub translation: Vec3,
    pub rotation: Vec3,
    pub scale: Vec3,
}

pub struct SkinVertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: [f32; 2],
    pub joints: [u16; 4],
    pub weights: [f32; 4],
}

pub struct SkinMesh {
    pub source: usize,
    pub joints: Vec<usize>,
    pub inverse_bind: Vec<Mat4>,
    pub vertices: Vec<SkinVertex>,
    pub indices: Vec<u32>,
}

pub struct SkeletalModel {
    pub nodes: Vec<RigNode>,
    pub paths: BTreeMap<usize, Vec<String>>,
    pub meshes: Vec<SkinMesh>,
    pub clips: Vec<AnimationLayer>,
}

pub fn load(file: &Path, read_clips: bool) -> Result<SkeletalModel, String> {
    let asset = gltf::Gltf::from_slice(&read_bounded(file, MAX_MODEL_FILE_BYTES)?)
        .map_err(|error| error.to_string())?;
    if asset.skins().next().is_none() {
        return Err("The model contains no skin".into());
    }
    let buffers = read_buffers(&asset, file)?;
    let (nodes, paths) = hierarchy(&asset.document)?;
    let mut meshes = Vec::new();
    let mut vertex_count = 0;
    let mut index_count = 0;
    for node in asset
        .nodes()
        .filter(|node| node.skin().is_some() && node.mesh().is_some())
    {
        let skin = node.skin().ok_or("The mesh skin is missing")?;
        let joints: Vec<_> = skin.joints().map(|joint| joint.index()).collect();
        if joints.is_empty() || joints.len() > MAX_JOINTS {
            return Err("The skin exceeds the joint budget".into());
        }
        let inverse_bind: Vec<_> = skin
            .reader(|buffer| buffers.get(buffer.index()).map(Vec::as_slice))
            .read_inverse_bind_matrices()
            .map(|matrices| {
                matrices
                    .map(|values| Mat4::from_cols_array_2d(&values))
                    .collect()
            })
            .unwrap_or_else(|| vec![Mat4::IDENTITY; joints.len()]);
        if inverse_bind.len() != joints.len()
            || inverse_bind.iter().any(|matrix| !matrix.is_finite())
        {
            return Err("The skin has invalid inverse bind matrices".into());
        }
        let mut mesh = SkinMesh {
            source: node.index(),
            joints,
            inverse_bind,
            vertices: Vec::new(),
            indices: Vec::new(),
        };
        for primitive in node.mesh().ok_or("The skin mesh is missing")?.primitives() {
            if primitive.mode() != gltf::mesh::Mode::Triangles
                || primitive.get(&gltf::Semantic::Joints(1)).is_some()
                || primitive.get(&gltf::Semantic::Weights(1)).is_some()
            {
                return Err("A skinned primitive needs triangles and at most four joint influences per vertex".into());
            }
            let count = primitive
                .get(&gltf::Semantic::Positions)
                .ok_or("Skin primitive has no positions")?
                .count();
            let indices = primitive
                .indices()
                .map_or(count, |accessor| accessor.count());
            if count == 0
                || indices % 3 != 0
                || count > MAX_VERTICES - vertex_count
                || indices > MAX_INDICES - index_count
            {
                return Err("The skin exceeds its triangle or vertex budget".into());
            }
            vertex_count += count;
            index_count += indices;
            let reader = primitive.reader(|buffer| buffers.get(buffer.index()).map(Vec::as_slice));
            let positions = reader
                .read_positions()
                .ok_or("Skin positions cannot be decoded")?;
            let influences = reader
                .read_joints(0)
                .ok_or("Skin vertex joints are missing")?
                .into_u16();
            let weights = reader
                .read_weights(0)
                .ok_or("Skin vertex weights are missing")?
                .into_f32();
            if influences.len() != count || weights.len() != count {
                return Err("Skin vertex arrays have different lengths".into());
            }
            let mut normals = reader.read_normals();
            let mut uvs = reader.read_tex_coords(0).map(|uvs| uvs.into_f32());
            let first = mesh.vertices.len() as u32;
            for ((position, joints), mut weights) in positions.zip(influences).zip(weights) {
                if joints
                    .iter()
                    .any(|&joint| usize::from(joint) >= mesh.joints.len())
                    || weights
                        .iter()
                        .any(|weight| !weight.is_finite() || *weight < 0.0)
                    || !position.iter().all(|value| value.is_finite())
                {
                    return Err("Skin vertex influence or position is invalid".into());
                }
                let total: f32 = weights.iter().sum();
                if !total.is_finite() || total <= f32::EPSILON {
                    return Err("Skin vertex has zero influence".into());
                }
                for value in &mut weights {
                    *value /= total;
                }
                let normal = normals
                    .as_mut()
                    .and_then(Iterator::next)
                    .map(Vec3::from_array)
                    .unwrap_or(Vec3::Y);
                let uv = uvs.as_mut().and_then(Iterator::next).unwrap_or([0.0; 2]);
                if !normal.is_finite() || !uv.iter().all(|value| value.is_finite()) {
                    return Err("Skin normal or texture coordinate is invalid".into());
                }
                mesh.vertices.push(SkinVertex {
                    position: Vec3::from_array(position),
                    normal,
                    uv,
                    joints,
                    weights,
                });
            }
            if mesh.vertices.len() - first as usize != count {
                return Err("Skin vertex arrays are truncated".into());
            }
            let start = mesh.indices.len();
            if let Some(indices) = reader.read_indices() {
                mesh.indices.extend(indices.into_u32());
            } else if primitive.indices().is_none() {
                mesh.indices.extend(0..count as u32);
            }
            if mesh.indices.len() - start != indices
                || mesh.indices[start..]
                    .iter()
                    .any(|&index| index as usize >= count)
            {
                return Err("Skin triangle indices are invalid".into());
            }
            for index in &mut mesh.indices[start..] {
                *index += first;
            }
        }
        if !mesh.indices.is_empty() {
            meshes.push(mesh);
        }
    }
    if meshes.is_empty() {
        return Err("The model contains no renderable skinned mesh".into());
    }
    let clips = if read_clips {
        if asset.animations().count() > 32 {
            return Err("The model exceeds the clip budget".into());
        }
        asset
            .animations()
            .map(|animation| {
                crate::animation_import::read_layer(animation, &paths, |buffer| {
                    buffers.get(buffer.index()).map(Vec::as_slice)
                })
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };
    Ok(SkeletalModel {
        nodes,
        paths,
        meshes,
        clips,
    })
}

pub(crate) fn read_buffers(asset: &gltf::Gltf, file: &Path) -> Result<Vec<Vec<u8>>, String> {
    let mut bytes = 0u64;
    let parent = file.parent().ok_or("glTF asset has no parent directory")?;
    if asset.buffers().count() > 32 {
        return Err("The model exceeds the buffer count budget".into());
    }
    asset
        .buffers()
        .map(|buffer| {
            let limit = MAX_MODEL_FILE_BYTES - bytes;
            if buffer.length() as u64 > limit {
                return Err("The model exceeds the combined buffer budget".into());
            }
            let data = match buffer.source() {
                gltf::buffer::Source::Bin => {
                    asset.blob.clone().ok_or("The GLB BIN chunk is missing")?
                }
                gltf::buffer::Source::Uri(uri) if uri.starts_with("data:") => decode_data_uri(uri)?,
                gltf::buffer::Source::Uri(uri) => {
                    read_bounded(&crate::asset_paths::resolve_confined(parent, uri)?, limit)?
                }
            };
            if data.len() < buffer.length() || data.len() as u64 > limit {
                return Err("Invalid glTF buffer size".into());
            }
            bytes += data.len() as u64;
            Ok(data)
        })
        .collect()
}

type NodePaths = BTreeMap<usize, Vec<String>>;

fn hierarchy(document: &gltf::Document) -> Result<(Vec<RigNode>, NodePaths), String> {
    let nodes: Vec<_> = document.nodes().collect();
    if nodes.len() > MAX_NODES {
        return Err("The model exceeds the skeleton node budget".into());
    }
    let mut parents = vec![None; nodes.len()];
    for node in &nodes {
        for child in node.children() {
            if parents[child.index()].replace(node.index()).is_some() {
                return Err("A skeleton node has more than one parent".into());
            }
        }
    }
    let mut ready: VecDeque<_> = (0..nodes.len())
        .filter(|&index| parents[index].is_none())
        .collect();
    let mut ordered = Vec::with_capacity(nodes.len());
    let mut paths = NodePaths::new();
    while let Some(index) = ready.pop_front() {
        let node = &nodes[index];
        let (translation, r, scale) = node.transform().decomposed();
        let rotation = Quat::from_array(r);
        let translation = Vec3::from_array(translation);
        let scale = Vec3::from_array(scale);
        if !translation.is_finite()
            || !rotation.is_finite()
            || rotation.length_squared() <= f32::EPSILON
            || !scale.is_finite()
            || scale.abs().min_element() < f32::EPSILON
        {
            return Err("Invalid skeleton node transform".into());
        }
        let label: String = node
            .name()
            .unwrap_or("Node")
            .chars()
            .filter(|character| *character != '/' && !character.is_control())
            .take(48)
            .collect();
        let name = format!("{label} [{index}]");
        let parent = parents[index];
        let mut path = parent
            .and_then(|id| paths.get(&id).cloned())
            .unwrap_or_default();
        path.push(name.clone());
        if path.len() > 64 {
            return Err("Skeleton hierarchy is deeper than 64 nodes".into());
        }
        paths.insert(index, path);
        ordered.push(RigNode {
            source: index,
            parent,
            name,
            translation,
            rotation: degrees(rotation.normalize()),
            scale,
        });
        ready.extend(node.children().map(|child| child.index()));
    }
    if ordered.len() != nodes.len() {
        return Err("Skeleton hierarchy contains a cycle".into());
    }
    Ok((ordered, paths))
}

/// Joint matrices already include inverse binds; points and normals share the same weights.
pub fn deform_vertex(vertex: &SkinVertex, joints: &[Mat4], normals: &[glam::Mat3]) -> (Vec3, Vec3) {
    let mut position = Vec3::ZERO;
    let mut normal = Vec3::ZERO;
    for (&joint, &weight) in vertex.joints.iter().zip(&vertex.weights) {
        if weight == 0.0 {
            continue;
        }
        position += joints[usize::from(joint)].transform_point3(vertex.position) * weight;
        normal += normals[usize::from(joint)] * vertex.normal * weight;
    }
    (position, normal.try_normalize().unwrap_or(Vec3::Y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skeletal_glb_loads_joint_hierarchy_weights_and_animation() {
        let document = serde_json::json!({
            "asset":{"version":"2.0"}, "scene":0,
            "scenes":[{"nodes":[0,1]}],
            "nodes":[{"mesh":0,"skin":0},{"name":"Bone"}],
            "skins":[{"joints":[1],"inverseBindMatrices":3}],
            "meshes":[{"primitives":[{"attributes":{"POSITION":0,"JOINTS_0":1,"WEIGHTS_0":2}}]}],
            "buffers":[{"byteLength":192}],
            "bufferViews": [
                {"buffer":0,"byteOffset":0,"byteLength":36}, {"buffer":0,"byteOffset":36,"byteLength":12},
                {"buffer":0,"byteOffset":48,"byteLength":48},{"buffer":0,"byteOffset":96,"byteLength":64},
                {"buffer":0,"byteOffset":160,"byteLength":8},{"buffer":0,"byteOffset":168,"byteLength":24}
            ],
            "accessors":[
                {"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]},
                {"bufferView":1,"componentType":5121,"count":3,"type":"VEC4"},
                {"bufferView":2,"componentType":5126,"count":3,"type":"VEC4"},
                {"bufferView":3,"componentType":5126,"count":1,"type":"MAT4"},
                {"bufferView":4,"componentType":5126,"count":2,"type":"SCALAR","min":[0],"max":[1]},
                {"bufferView":5,"componentType":5126,"count":2,"type":"VEC3"}
            ],
            "animations":[{"name":"Lift","samplers":[{"input":4,"output":5}],
                "channels":[{"sampler":0,"target":{"node":1,"path":"translation"}}]}]
        });
        let mut metadata = serde_json::to_vec(&document).unwrap();
        metadata.resize(metadata.len().next_multiple_of(4), b' ');
        let mut bin = Vec::new();
        for value in [0.0_f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
            bin.extend(value.to_le_bytes());
        }
        bin.extend([0; 12]);
        for value in [1.0_f32, 0.0, 0.0, 0.0]
            .repeat(3)
            .into_iter()
            .chain(Mat4::IDENTITY.to_cols_array())
            .chain([0.0_f32, 1.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0])
        {
            bin.extend(value.to_le_bytes());
        }
        assert_eq!(bin.len(), 192);
        let mut file = b"glTF".to_vec();
        file.extend(2u32.to_le_bytes());
        file.extend(((28 + metadata.len() + bin.len()) as u32).to_le_bytes());
        file.extend((metadata.len() as u32).to_le_bytes());
        file.extend(b"JSON");
        file.extend(metadata);
        file.extend((bin.len() as u32).to_le_bytes());
        file.extend(b"BIN\0");
        file.extend(bin);
        let path = std::env::temp_dir().join(format!("raf-skin-{}.glb", uuid::Uuid::new_v4()));
        std::fs::write(&path, file).unwrap();
        let model = load(&path, true).unwrap();
        std::fs::remove_file(path).unwrap();
        let mesh = &model.meshes[0];
        assert_eq!(mesh.indices, [0, 1, 2]);
        assert_eq!(mesh.joints, [1]);
        let track = &model.clips[0].tracks[0];
        assert_eq!(track.path, model.paths[&1]);
        let joint = Mat4::from_translation(track.sample(0.5)) * mesh.inverse_bind[0];
        assert_eq!(
            deform_vertex(&mesh.vertices[1], &[joint], &[glam::Mat3::IDENTITY]).0,
            Vec3::new(1.0, 1.0, 0.0)
        );
    }
}
