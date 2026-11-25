//! Read-only projection shared by authoring surfaces. Resources remain catalog-owned.
use crate::importer::ModelImportMetadata;
use raf_core::scene::{Primitive, SceneGraph};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
struct CachedModelInfo {
    /// Full path of the located model, so later lookups stay root-aware without
    /// widening the public catalog row type.
    file: Option<PathBuf>,
}

thread_local! {
    /// Model rows are built on the UI thread and read immediately by previews
    /// and the inspector. Keep the located path per reference without changing
    /// the public row shape.
    static MODEL_INFO_CACHE: RefCell<HashMap<String, CachedModelInfo>> =
        RefCell::new(HashMap::new());
}

fn model_cache_key(path: &str) -> String {
    crate::asset_paths::normalize_reference(path)
        .unwrap_or_else(|_| path.replace('\\', "/"))
        .to_ascii_lowercase()
}

fn cache_model_info(path: &str, info: CachedModelInfo) {
    MODEL_INFO_CACHE.with(|cache| {
        cache.borrow_mut().insert(model_cache_key(path), info);
    });
}

fn cached_model_info(path: &str) -> Option<CachedModelInfo> {
    MODEL_INFO_CACHE.with(|cache| cache.borrow().get(&model_cache_key(path)).cloned())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetClass {
    Shape(Primitive),
    Prefab,
    Image,
    Model,
    Script,
    Audio,
    Camera,
    Light,
    Interface,
    Animation,
    Particle,
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
        scene.get_mut(id).unwrap().animation_asset = Some("motion/lift.animation.json".into());
        scene.get_mut(id).unwrap().particle_system = Some(
            raf_core::scene::ParticleSystemComponent::new("particles/fire.rafparticle"),
        );
        let rows = vec![
            "textures/wood.png".into(),
            "models/store.glb".into(),
            "shelf.prefab.ron".into(),
            "motion/lift.animation.json".into(),
            "particles/fire.rafparticle".into(),
        ];
        let entries = project_assets(&rows, &scene);
        assert_eq!(entries[&rows[0]].usages, 1);
        assert!(!entries[&rows[1]].insertable());
        assert!(entries[&rows[2]].insertable());
        assert_eq!(entries[&rows[3]].class, AssetClass::Animation);
        assert_eq!(entries[&rows[3]].usages, 1);
        assert_eq!(entries[&rows[4]].class, AssetClass::Particle);
        assert_eq!(entries[&rows[4]].usages, 1);
        assert!(entries[&rows[4]].insertable());
    }
}

pub fn normalized_reference(path: &str) -> String {
    crate::asset_paths::normalize_reference(path).unwrap_or_else(|_| path.replace('\\', "/"))
}

impl AuthoringAsset {
    pub fn from_path(path: &str) -> Self {
        Self::from_path_with_root(path, None)
    }

    /// Build a catalog row with an explicit project assets root so model
    /// existence, dimensions and cached metadata can be resolved without
    /// guessing from the current working directory.
    pub fn from_path_with_root(path: &str, assets_root: Option<&Path>) -> Self {
        let lower = path.to_ascii_lowercase();
        let class = if let Some(slug) = lower.strip_prefix("builtin://primitive/") {
            Primitive::from_slug(slug)
                .map(AssetClass::Shape)
                .unwrap_or(AssetClass::File)
        } else if lower == "builtin://camera" {
            AssetClass::Camera
        } else if lower.starts_with("builtin://light/") {
            AssetClass::Light
        } else if lower.starts_with("builtin://interface/") || lower.ends_with(".ui.ron") {
            AssetClass::Interface
        } else if lower.ends_with(".prefab.ron") {
            AssetClass::Prefab
        } else if crate::animation_asset::is_animation_asset(&lower) {
            AssetClass::Animation
        } else if lower.ends_with(".rafparticle") {
            AssetClass::Particle
        } else {
            match lower.rsplit('.').next().unwrap_or("") {
                "png" | "jpg" | "jpeg" | "bmp" | "webp" => AssetClass::Image,
                "glb" | "gltf" | "obj" | "fbx" => AssetClass::Model,
                "rhai" | "lua" | "rs" | "cpp" | "h" | "js" | "ts" | "wasm" => AssetClass::Script,
                "wav" | "ogg" | "mp3" | "flac" => AssetClass::Audio,
                _ => AssetClass::File,
            }
        };
        let is_builtin = lower.starts_with("builtin://");
        let located = if is_builtin {
            None
        } else {
            locate_asset_file(path, assets_root)
        };
        let exists = located.as_ref().is_some_and(|file| file.is_file());
        let metadata = if class == AssetClass::Model && exists {
            located
                .as_deref()
                .and_then(ModelImportMetadata::read_for_asset_file)
        } else {
            None
        };
        if class == AssetClass::Model && !is_builtin {
            cache_model_info(
                path,
                CachedModelInfo {
                    file: located.filter(|file| file.is_file()),
                },
            );
        }
        let dimensions = match class {
            AssetClass::Shape(p) => Some(p.dimensions(glam::Vec3::ONE).to_array()),
            AssetClass::Model => metadata.as_ref().map(ModelImportMetadata::dimensions),
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

    /// Cached metadata for an imported static model, when a sidecar exists.
    pub fn model_metadata(&self) -> Option<ModelImportMetadata> {
        if self.class != AssetClass::Model {
            return None;
        }
        if let Some(file) = locate_asset_file(&self.path, None) {
            return ModelImportMetadata::read_for_asset_file(&file);
        }
        cached_model_info(&self.path)
            .and_then(|info| info.file)
            .filter(|file| file.is_file())
            .and_then(|file| ModelImportMetadata::read_for_asset_file(&file))
    }

    /// Cached triangle count for an imported static model.
    pub fn triangles(&self) -> Option<usize> {
        self.model_metadata().map(|metadata| metadata.triangles)
    }

    /// Whether the referenced file currently exists in the project assets.
    pub fn exists(&self) -> bool {
        if self.path.to_ascii_lowercase().starts_with("builtin://") {
            return false;
        }
        if locate_asset_file(&self.path, None).is_some() {
            return true;
        }
        cached_model_info(&self.path)
            .and_then(|info| info.file)
            .is_some_and(|file| file.is_file())
    }

    /// WAV duration, channels, sample rate and waveform peaks when available.
    pub fn audio_metadata(&self) -> Option<crate::audio_asset::AudioAssetMeta> {
        if self.class != AssetClass::Audio {
            return None;
        }
        let file = locate_asset_file(&self.path, None)?;
        crate::audio_asset::AudioAssetMeta::read_for_asset_file(&file)
    }

    pub fn insertable(&self) -> bool {
        match self.class {
            AssetClass::Model => self.exists() && has_static_model_extension(&self.path),
            class => matches!(
                class,
                AssetClass::Shape(_)
                    | AssetClass::Prefab
                    | AssetClass::Camera
                    | AssetClass::Light
                    | AssetClass::Interface
                    | AssetClass::Particle
            ),
        }
    }
}

fn has_static_model_extension(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".glb") || lower.ends_with(".gltf")
}

fn locate_asset_file(path: &str, assets_root: Option<&Path>) -> Option<PathBuf> {
    let normalized = crate::asset_paths::normalize_reference(path).ok()?;
    let mut candidates = Vec::new();
    if let Some(root) = assets_root {
        candidates.push(crate::asset_paths::join_normalized(root, &normalized));
    }
    candidates.push(PathBuf::from(&normalized));
    candidates.push(Path::new("assets").join(&normalized));
    candidates.into_iter().find(|candidate| candidate.is_file())
}

pub fn project_assets(rows: &[String], scene: &SceneGraph) -> HashMap<String, AuthoringAsset> {
    project_assets_with_root(None, rows, scene)
}

/// Project assets with an explicit assets root. Prefer this overload in editor
/// surfaces that already know the active project.
pub fn project_assets_with_root(
    assets_root: Option<&Path>,
    rows: &[String],
    scene: &SceneGraph,
) -> HashMap<String, AuthoringAsset> {
    let mut usages: HashMap<String, usize> = HashMap::new();
    for (_, node) in scene.iter() {
        for path in node
            .source_asset
            .iter()
            .chain(node.base_color_texture.iter())
            .chain(node.animation_asset.iter())
            .chain(node.scripts.iter())
            .chain(node.mesh_asset.iter().map(|asset| &asset.path))
            .chain(std::iter::once(&node.audio_source.clip).filter(|clip| !clip.is_empty()))
            .chain(
                node.particle_system
                    .as_ref()
                    .map(|component| &component.asset)
                    .filter(|asset| !asset.is_empty()),
            )
        {
            *usages.entry(normalized_reference(path)).or_default() += 1;
        }
    }
    rows.iter()
        .map(|path| {
            let mut entry = AuthoringAsset::from_path_with_root(path, assets_root);
            entry.usages = usages
                .get(&normalized_reference(path))
                .copied()
                .unwrap_or_default();
            (path.clone(), entry)
        })
        .collect()
}
