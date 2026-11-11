//! Read-only projection shared by authoring surfaces. Resources remain catalog-owned.
use raf_core::scene::{Primitive, SceneGraph};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetClass {
    Shape(Primitive),
    Prefab,
    Image,
    Model,
    Script,
    Audio,
    Camera,
    Interface,
    File,
}

#[derive(Debug, Clone)]
pub struct AuthoringAsset {
    pub path: String,
    pub name: String,
    pub class: AssetClass,
    pub usages: usize,
    pub dimensions: Option<[f32; 3]>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authoring_catalog_exposes_only_real_capabilities_and_usage() {
        let mut scene = SceneGraph::new();
        let id = scene.add_root_with_primitive("Part", Primitive::Cube);
        scene.get_mut(id).unwrap().base_color_texture = Some("textures/wood.png".into());
        let rows = vec![
            "textures/wood.png".into(),
            "models/store.glb".into(),
            "shelf.prefab.ron".into(),
        ];
        let entries = project_assets(&rows, &scene);
        assert_eq!(entries[&rows[0]].usages, 1);
        assert!(!entries[&rows[1]].insertable());
        assert!(entries[&rows[2]].insertable());
    }
}

pub fn normalized_reference(path: &str) -> String {
    path.replace('\\', "/")
        .trim_start_matches("assets/")
        .to_string()
}

impl AuthoringAsset {
    pub fn from_path(path: &str) -> Self {
        let lower = path.to_ascii_lowercase();
        let class = if let Some(slug) = lower.strip_prefix("builtin://primitive/") {
            Primitive::from_slug(slug)
                .map(AssetClass::Shape)
                .unwrap_or(AssetClass::File)
        } else if lower == "builtin://camera" {
            AssetClass::Camera
        } else if lower.starts_with("builtin://interface/") || lower.ends_with(".ui.ron") {
            AssetClass::Interface
        } else if lower.ends_with(".prefab.ron") {
            AssetClass::Prefab
        } else {
            match lower.rsplit('.').next().unwrap_or("") {
                "png" | "jpg" | "jpeg" | "bmp" | "webp" => AssetClass::Image,
                "glb" | "gltf" | "obj" | "fbx" => AssetClass::Model,
                "rhai" | "lua" | "rs" | "cpp" | "h" | "js" | "ts" | "wasm" => AssetClass::Script,
                "wav" | "ogg" | "mp3" | "flac" => AssetClass::Audio,
                _ => AssetClass::File,
            }
        };
        let dimensions = match class {
            AssetClass::Shape(p) => Some(p.dimensions(glam::Vec3::ONE).to_array()),
            _ => None,
        };
        Self {
            path: path.into(),
            name: path
                .replace('\\', "/")
                .rsplit('/')
                .next()
                .unwrap_or(path)
                .into(),
            class,
            usages: 0,
            dimensions,
        }
    }
    pub fn insertable(&self) -> bool {
        matches!(
            self.class,
            AssetClass::Shape(_) | AssetClass::Prefab | AssetClass::Camera | AssetClass::Interface
        )
    }
}

pub fn project_assets(rows: &[String], scene: &SceneGraph) -> HashMap<String, AuthoringAsset> {
    let mut usages: HashMap<String, usize> = HashMap::new();
    for (_, node) in scene.iter() {
        for path in node
            .source_asset
            .iter()
            .chain(node.base_color_texture.iter())
            .chain(node.scripts.iter())
            .chain(std::iter::once(&node.audio_source.clip).filter(|clip|!clip.is_empty()))
        {
            *usages.entry(normalized_reference(path)).or_default() += 1;
        }
    }
    rows.iter()
        .map(|path| {
            let mut entry = AuthoringAsset::from_path(path);
            entry.usages = usages
                .get(&normalized_reference(path))
                .copied()
                .unwrap_or_default();
            (path.clone(), entry)
        })
        .collect()
}
