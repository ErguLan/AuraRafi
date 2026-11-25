//! Static imported-model cache shared by the scene and asset previews.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use glam::Vec3;
use raf_core::scene::MeshAssetRef;

use crate::geometry::mesh_data::MeshData;

const MAX_CACHED_MODELS: usize = 64;

#[derive(Default)]
pub struct ModelMeshCache {
    entries: HashMap<String, Option<Arc<MeshData>>>,
}

impl ModelMeshCache {
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn get(
        &mut self,
        asset_root: Option<&Path>,
        reference: &MeshAssetRef,
    ) -> Result<Arc<MeshData>, String> {
        reference.validate()?;
        let key = format!(
            "{}|{}|{}|{}",
            reference.path,
            reference.content_hash.as_deref().unwrap_or(""),
            reference.node.map(|node| node as i64).unwrap_or(-1),
            reference.import_scale.to_bits()
        );
        if let Some(cached) = self.entries.get(&key) {
            return cached
                .clone()
                .ok_or_else(|| "Model could not be decoded.".to_string());
        }
        if self.entries.len() >= MAX_CACHED_MODELS {
            self.entries.clear();
        }
        let result = match asset_root {
            Some(root) => load_model(root, reference),
            None => Err("No project asset root is configured.".to_string()),
        };
        if let Ok(mesh) = &result {
            self.entries.insert(key, Some(Arc::clone(mesh)));
        }
        result
    }
}

fn load_model(root: &Path, reference: &MeshAssetRef) -> Result<Arc<MeshData>, String> {
    let path = confined_model_path(root, &reference.path)?;
    let model = raf_assets::model_asset::load_static_model(&path)?;
    let mut mesh = MeshData::new();
    for source in model
        .meshes
        .iter()
        .filter(|source| reference.node.is_none_or(|node| source.node == Some(node)))
    {
        let base = mesh.positions.len() as u32;
        for position in &source.positions {
            mesh.positions.push(
                Vec3::from_array(*position) * reference.import_scale.max(f32::EPSILON),
            );
        }
        for normal in &source.normals {
            mesh.normals
                .push(Vec3::from_array(*normal).normalize_or_zero());
        }
        for uv in &source.uvs {
            mesh.uvs.push(*uv);
        }
        mesh.indices.extend(source.indices.iter().map(|index| base + *index));
    }
    if mesh.indices.is_empty() || mesh.positions.is_empty() {
        return Err("The selected model node contains no renderable triangles.".to_string());
    }
    if mesh.normals.len() != mesh.positions.len() {
        mesh.normals.resize(mesh.positions.len(), Vec3::Y);
    }
    if mesh.uvs.len() != mesh.positions.len() {
        mesh.uvs.resize(mesh.positions.len(), [0.0, 0.0]);
    }
    Ok(Arc::new(mesh))
}

fn confined_model_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let normalized = relative.replace('\\', "/");
    let mut path = PathBuf::new();
    for component in Path::new(&normalized).components() {
        if !matches!(component, Component::Normal(_)) {
            return Err("Model path is outside the project assets directory.".to_string());
        }
        path.push(component.as_os_str());
    }
    if path.as_os_str().is_empty() {
        return Err("Model path is empty.".to_string());
    }
    Ok(root.join(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal_paths() {
        let root = Path::new("D:/project/assets");
        assert!(confined_model_path(root, "../secret.glb").is_err());
        assert!(confined_model_path(root, "C:/secret.glb").is_err());
        assert!(confined_model_path(root, "models/a.glb").is_ok());
    }
}
