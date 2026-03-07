//! Asset importer - handles loading various file formats.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

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
