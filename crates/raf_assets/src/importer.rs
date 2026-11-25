//! Asset importing and validation.
//!
//! Image/audio/scene metadata stays in the compact [`AssetMeta`] shape used by
//! the asset browser. Static 3D models use [`import_model_package`], which
//! validates the document before copying it and records a cached sidecar next
//! to the model.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use raf_core::scene::MeshAssetRef;

use crate::asset_paths::{
    self, content_hash_file, copy_imported_file, copy_imported_file_exact, join_normalized,
    write_atomic, ImportBudget,
};
use crate::model_asset::{
    load_static_model, ImportedModel, ModelBounds, ModelDependency, ModelMaterial,
    MAX_MODEL_FILE_BYTES,
};

/// Supported asset types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetType {
    Image,
    Model3D,
    Audio,
    Scene,
    Unknown,
}

impl AssetType {
    /// Detect asset type from file extension.
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "png" | "jpg" | "jpeg" | "bmp" | "webp" | "svg" => Self::Image,
            "gltf" | "glb" | "obj" | "fbx" => Self::Model3D,
            "wav" | "mp3" | "ogg" | "flac" => Self::Audio,
            "ron" | "json" => Self::Scene,
            _ => Self::Unknown,
        }
    }
}

/// Metadata for an imported asset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetMeta {
    pub id: Uuid,
    pub name: String,
    pub asset_type: AssetType,
    pub source_path: PathBuf,
    pub file_size_bytes: u64,
}

/// Handles asset importing into the project.
pub struct AssetImporter;

impl AssetImporter {
    /// Import a file into the project's asset directory.
    /// Returns metadata about the imported asset.
    pub fn import(
        source: &Path,
        project_assets_dir: &Path,
    ) -> Result<AssetMeta, Box<dyn std::error::Error>> {
        let file_name = source
            .file_name()
            .ok_or("No file name")?
            .to_str()
            .ok_or("Invalid file name")?;

        let ext = source
            .extension()
            .unwrap_or_default()
            .to_str()
            .unwrap_or("");

        let asset_type = AssetType::from_extension(ext);
        let dest = project_assets_dir.join(file_name);

        // Copy the file into the project directory.
        if source != dest {
            std::fs::copy(source, &dest)?;
        }

        let file_size = std::fs::metadata(&dest)?.len();

        let meta = AssetMeta {
            id: Uuid::new_v4(),
            name: file_name.to_string(),
            asset_type,
            source_path: dest,
            file_size_bytes: file_size,
        };

        tracing::info!("Imported asset: {} ({:?})", meta.name, meta.asset_type);
        Ok(meta)
    }
}

/// Cached metadata written next to an imported model as `<stem>.meta.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelImportMetadata {
    pub version: u32,
    /// Project-relative path below `assets/`, for example `models/store/store.glb`.
    pub path: String,
    pub bytes: u64,
    /// FNV-1a content hash used to invalidate decoded meshes.
    pub content_hash: String,
    pub bounds: ModelBounds,
    pub triangles: usize,
    pub vertices: usize,
    pub meshes: usize,
    pub materials: Vec<ModelMaterial>,
    /// Project-relative dependency paths below `assets/`.
    pub dependencies: Vec<ModelDependency>,
    #[serde(default = "default_import_scale")]
    pub import_scale: f32,
}

fn default_import_scale() -> f32 {
    1.0
}

impl ModelImportMetadata {
    pub const VERSION: u32 = 1;
    pub const SIDECAR_SUFFIX: &'static str = ".meta.json";
    const MAX_SIDECAR_BYTES: u64 = 4 * 1024 * 1024;

    /// Sidecar path for an imported model file.
    pub fn sidecar_path(asset_file: &Path) -> PathBuf {
        let stem = asset_file
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("model");
        asset_file.with_file_name(format!("{stem}{}", Self::SIDECAR_SUFFIX))
    }

    /// Read a sidecar if it exists and is valid. Missing or corrupt caches are
    /// treated as "no metadata" so the catalog can still list the file.
    pub fn read_for_asset_file(asset_file: &Path) -> Option<Self> {
        let path = Self::sidecar_path(asset_file);
        let metadata = fs::metadata(&path).ok()?;
        if !metadata.is_file() || metadata.len() > Self::MAX_SIDECAR_BYTES {
            return None;
        }
        let bytes = fs::read(&path).ok()?;
        let parsed: Self = serde_json::from_slice(&bytes).ok()?;
        (parsed.version == Self::VERSION).then_some(parsed)
    }

    /// Read cached metadata for a project-relative reference below `assets/`.
    pub fn read(
        project_assets_dir: &Path,
        reference: &str,
    ) -> Result<Option<Self>, String> {
        let asset_file = asset_paths::resolve_confined(project_assets_dir, reference)?;
        Ok(Self::read_for_asset_file(&asset_file))
    }

    pub fn dimensions(&self) -> [f32; 3] {
        self.bounds.dimensions()
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != Self::VERSION {
            return Err(format!(
                "Unsupported model metadata version {}.",
                self.version
            ));
        }
        if !self.bounds.min.iter().all(|value| value.is_finite())
            || !self.bounds.max.iter().all(|value| value.is_finite())
        {
            return Err("Model metadata bounds must be finite.".to_string());
        }
        if self.triangles > crate::model_asset::MAX_MODEL_TRIANGLES {
            return Err("Model metadata exceeds the triangle limit.".to_string());
        }
        if !self.import_scale.is_finite() || self.import_scale <= 0.0 {
            return Err("Model import scale must be finite and greater than zero.".to_string());
        }
        Ok(())
    }
}

/// Result of importing one static model package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportReport {
    pub mesh_asset: MeshAssetRef,
    /// Project-relative package directory below `assets/` (e.g. `models/store`).
    pub package: String,
    /// Every project-relative file written by the import.
    pub copied_files: Vec<String>,
    pub metadata: ModelImportMetadata,
    pub warnings: Vec<String>,
}

/// Import a static model into `<project_assets_dir>/models/<slug>/`.
pub fn import_model_package(
    source_file: &Path,
    project_assets_dir: &Path,
) -> Result<ImportReport, String> {
    import_model_package_with_destination(source_file, project_assets_dir, None)
}

/// Import a static model below an explicit destination directory.
///
/// `destination` is relative to `assets/` (`models`, `models/props`, ...).
/// `None` uses `models`.
pub fn import_model_package_with_destination(
    source_file: &Path,
    project_assets_dir: &Path,
    destination: Option<&str>,
) -> Result<ImportReport, String> {
    let extension = source_file
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .ok_or_else(|| "Static model import requires a .glb or .gltf file.".to_string())?;
    if extension != "glb" && extension != "gltf" {
        return Err(format!(
            "Static model import requires .glb or .gltf; received '.{extension}'."
        ));
    }
    let source_metadata = fs::symlink_metadata(source_file)
        .map_err(|error| format!("Cannot inspect '{}': {error}", source_file.display()))?;
    if source_metadata.file_type().is_symlink() {
        return Err(
            "Static model import does not accept symlinks; choose the real file.".to_string(),
        );
    }
    if !source_metadata.is_file() {
        return Err(format!(
            "Static model source '{}' is not a regular file.",
            source_file.display()
        ));
    }
    if source_metadata.len() > MAX_MODEL_FILE_BYTES {
        return Err(format!(
            "Static model '{}' is {} bytes; the import limit is {} bytes.",
            source_file.display(),
            source_metadata.len(),
            MAX_MODEL_FILE_BYTES
        ));
    }

    // Validate fully before touching the project directory.
    let imported = load_static_model(source_file)?;
    let destination = match destination {
        Some(destination) => asset_paths::normalize_directory_reference(destination)?,
        None => "models".to_string(),
    };

    fs::create_dir_all(project_assets_dir).map_err(|error| {
        format!(
            "Cannot create project assets directory '{}': {error}",
            project_assets_dir.display()
        )
    })?;
    let assets_root = project_assets_dir.canonicalize().map_err(|error| {
        format!(
            "Cannot resolve project assets directory '{}': {error}",
            project_assets_dir.display()
        )
    })?;
    let destination_parent = join_normalized(&assets_root, &destination);
    fs::create_dir_all(&destination_parent).map_err(|error| {
        format!(
            "Cannot create import destination '{}': {error}",
            destination_parent.display()
        )
    })?;
    let destination_parent = destination_parent.canonicalize().map_err(|error| {
        format!(
            "Cannot resolve import destination '{}': {error}",
            destination_parent.display()
        )
    })?;
    if !destination_parent.starts_with(&assets_root) {
        return Err(format!(
            "Import destination '{destination}' escapes the project assets directory."
        ));
    }

    let source_stem = source_file
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("model");
    let slug = asset_paths::unique_slug(source_stem);
    let package_dir = asset_paths::unique_directory(&destination_parent, &slug)?;

    let result = (|| -> Result<ImportReport, String> {
        let mut budget = ImportBudget::default();
        let copied_model = copy_imported_file(source_file, &package_dir, &mut budget)?;
        let model_name = copied_model
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "Imported model has no file name.".to_string())?
            .to_string();
        let package_reference = relative_reference(&assets_root, &package_dir)?;
        let model_reference = format!("{package_reference}/{model_name}");
        let content_hash = content_hash_file(&copied_model.path)?;

        let mut copied_files = vec![model_reference.clone()];
        for dependency in &imported.dependencies {
            let source = dependency_source_path(source_file, &dependency.path)?;
            let destination = dependency_destination_path(&package_dir, &dependency.path)?;
            copy_imported_file_exact(&source, &destination, &mut budget)?;
            copied_files.push(format!(
                "{package_reference}/{}",
                dependency.path
            ));
        }

        let metadata = build_metadata(
            &model_reference,
            copied_model.bytes,
            content_hash.clone(),
            &imported,
            &package_reference,
        );
        metadata.validate()?;
        let sidecar = ModelImportMetadata::sidecar_path(&copied_model.path);
        let sidecar_bytes = serde_json::to_vec_pretty(&metadata)
            .map_err(|error| format!("Cannot serialize model metadata: {error}"))?;
        write_atomic(&sidecar, &sidecar_bytes)?;

        let mut mesh_asset = MeshAssetRef::new(model_reference);
        mesh_asset.content_hash = Some(content_hash);
        mesh_asset.validate().map_err(str::to_string)?;

        Ok(ImportReport {
            mesh_asset,
            package: package_reference,
            copied_files,
            metadata,
            warnings: imported.warnings.clone(),
        })
    })();

    if result.is_err() {
        // The package directory was created by this call and is still private
        // to the failed import, so removing it cannot delete user data.
        let _ = fs::remove_dir_all(&package_dir);
    }
    result
}

fn build_metadata(
    model_reference: &str,
    bytes: u64,
    content_hash: String,
    imported: &ImportedModel,
    package_reference: &str,
) -> ModelImportMetadata {
    let mut materials = imported.materials.clone();
    for material in &mut materials {
        if let Some(texture) = material.base_color_texture.as_mut() {
            if !texture.starts_with("embedded:") {
                *texture = format!("{package_reference}/{texture}");
            }
        }
    }
    let dependencies = imported
        .dependencies
        .iter()
        .map(|dependency| ModelDependency {
            path: format!("{package_reference}/{}", dependency.path),
            kind: dependency.kind,
            bytes: dependency.bytes,
        })
        .collect();
    ModelImportMetadata {
        version: ModelImportMetadata::VERSION,
        path: model_reference.to_string(),
        bytes,
        content_hash,
        bounds: imported.bounds,
        triangles: imported.triangles,
        vertices: imported.vertex_count(),
        meshes: imported.meshes.len(),
        materials,
        dependencies,
        import_scale: 1.0,
    }
}

fn dependency_source_path(source_file: &Path, relative: &str) -> Result<PathBuf, String> {
    let base = source_file.parent().unwrap_or_else(|| Path::new("."));
    let resolved = asset_paths::resolve_confined(base, relative)?;
    if !resolved.is_file() {
        return Err(format!(
            "Model dependency '{}' is missing or not a file.",
            resolved.display()
        ));
    }
    Ok(resolved)
}

fn dependency_destination_path(package_dir: &Path, relative: &str) -> Result<PathBuf, String> {
    let normalized = asset_paths::normalize_relative(relative)?;
    let destination = join_normalized(package_dir, &normalized);
    if !asset_paths::is_confined_path(package_dir, &destination) {
        return Err(format!(
            "Model dependency '{relative}' escapes its package directory."
        ));
    }
    Ok(destination)
}

fn relative_reference(assets_root: &Path, path: &Path) -> Result<String, String> {
    let relative = path.strip_prefix(assets_root).map_err(|_| {
        format!(
            "Imported path '{}' is outside the assets directory.",
            path.display()
        )
    })?;
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            std::path::Component::Normal(value) => {
                parts.push(value.to_string_lossy().to_string())
            }
            _ => {
                return Err(format!(
                    "Imported path '{}' contains unsupported components.",
                    path.display()
                ))
            }
        }
    }
    if parts.is_empty() {
        return Err("Imported path has no project-relative components.".to_string());
    }
    Ok(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecar_names_follow_the_model_stem() {
        assert_eq!(
            ModelImportMetadata::sidecar_path(Path::new("models/store/store.glb")),
            PathBuf::from("models/store/store.meta.json")
        );
        assert_eq!(
            ModelImportMetadata::sidecar_path(Path::new("models/store/store_1.gltf")),
            PathBuf::from("models/store/store_1.meta.json")
        );
    }
}
