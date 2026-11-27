//! CPU pose deformation reused by all ApiGraphicBasic devices.

use crate::bounded_cache::BoundedCache;
use crate::geometry::mesh_data::MeshData;
use glam::{Mat3, Mat4};
use raf_assets::skeletal_model::{self, SkeletalModel};
use raf_core::scene::{
    animation::resolve_path, mesh_asset::SkinBinding, MeshAssetRef, SceneGraph, SceneNodeId,
    WorldTransformCache,
};
use std::path::Path;
use std::sync::Arc;

const MAX_SOURCES: usize = 8;
const MAX_POSES: usize = 16;

pub(super) struct SkinCache {
    sources: BoundedCache<(String, Option<String>), Arc<SkeletalModel>>,
    poses: BoundedCache<u128, SkinPose>,
}

impl Default for SkinCache {
    fn default() -> Self {
        Self {
            sources: BoundedCache::new(MAX_SOURCES),
            poses: BoundedCache::new(MAX_POSES),
        }
    }
}

struct SkinPose {
    palette: Vec<Mat4>,
    source: Arc<SkeletalModel>,
    node: usize,
    mesh: Arc<MeshData>,
}

impl SkinCache {
    pub(super) fn mesh(
        &mut self,
        scene: &SceneGraph,
        transforms: &WorldTransformCache,
        id: SceneNodeId,
        asset_root: &Path,
        reference: &MeshAssetRef,
        binding: &SkinBinding,
    ) -> Option<Arc<MeshData>> {
        reference.validate().ok()?;
        let key = (reference.path.clone(), reference.content_hash.clone());
        let source = if let Some(source) = self.sources.get(&key) {
            Arc::clone(source)
        } else {
            let file =
                raf_assets::asset_paths::resolve_confined(asset_root, &reference.path).ok()?;
            let source = Arc::new(skeletal_model::load(&file, false).ok()?);
            self.sources.insert(key, Arc::clone(&source));
            source
        };
        let node = scene.get(id)?;
        let identity = node.uuid.as_u128();
        let root = node.parent?;
        let source_node = reference.node?;
        let shape = source
            .meshes
            .iter()
            .find(|mesh| mesh.source == source_node)?;
        if binding.joints.len() != shape.inverse_bind.len() {
            return None;
        }
        let inverse_model = transforms.world_matrix(id)?.inverse();
        if !inverse_model.is_finite() {
            return None;
        }
        let palette = binding
            .joints
            .iter()
            .zip(&shape.inverse_bind)
            .map(|(path, bind)| {
                let joint = resolve_path(scene, root, path).ok()?;
                let matrix = inverse_model * transforms.world_matrix(joint)? * *bind;
                matrix.is_finite().then_some(matrix)
            })
            .collect::<Option<Vec<_>>>()?;
        if let Some(last) = self.poses.get(&identity) {
            if Arc::ptr_eq(&last.source, &source)
                && last.node == source_node
                && last.palette == palette
            {
                return Some(Arc::clone(&last.mesh));
            }
        }
        let normal_palette: Vec<_> = palette
            .iter()
            .map(|matrix| Mat3::from_mat4(*matrix).inverse().transpose())
            .collect();
        let mut mesh = MeshData::new();
        mesh.positions.reserve(shape.vertices.len());
        mesh.normals.reserve(shape.vertices.len());
        mesh.uvs.reserve(shape.vertices.len());
        for vertex in &shape.vertices {
            let (position, normal) =
                skeletal_model::deform_vertex(vertex, &palette, &normal_palette);
            if !position.is_finite() {
                return None;
            }
            mesh.positions.push(position);
            mesh.normals.push(normal);
            mesh.uvs.push(vertex.uv);
        }
        mesh.indices.clone_from(&shape.indices);
        let mesh = Arc::new(mesh);
        self.poses.insert(
            identity,
            SkinPose {
                palette,
                source,
                node: source_node,
                mesh: Arc::clone(&mesh),
            },
        );
        Some(mesh)
    }
}
