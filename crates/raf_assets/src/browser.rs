//! Asset browser - lists and filters project assets.

use crate::importer::{AssetMeta, AssetType};
use std::path::{Path, PathBuf};

/// In-memory asset browser for the editor.
pub struct AssetBrowser {
    /// All known assets in the project.
    pub assets: Vec<AssetMeta>,
    /// Current directory being browsed.
    pub current_dir: PathBuf,
    /// Active filter (None = show all).
    pub filter: Option<AssetType>,
    /// Text search query.
    pub search_query: String,
}

impl AssetBrowser {
    /// Create a new browser rooted at the project assets directory.
    pub fn new(assets_dir: &Path) -> Self {
        Self {
            assets: Vec::new(),
            current_dir: assets_dir.to_path_buf(),
            filter: None,
            search_query: String::new(),
        }
    }

    /// Scan the directory and populate the asset list (non-recursive for now).
    pub fn refresh(&mut self) {
        self.assets.clear();
        if let Ok(entries) = std::fs::read_dir(&self.current_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let ext = path
                        .extension()
                        .unwrap_or_default()
                        .to_str()
                        .unwrap_or("");
                    let name = path
                        .file_name()
                        .unwrap_or_default()
                        .to_str()
                        .unwrap_or("")
                        .to_string();
                    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

                    self.assets.push(AssetMeta {
                        id: uuid::Uuid::new_v4(),
                        name,
                        asset_type: AssetType::from_extension(ext),
                        source_path: path,
                        file_size_bytes: size,
                    });
                }
            }
        }
    }

    /// Return assets matching the current filter and search query.
    pub fn filtered(&self) -> Vec<&AssetMeta> {
        self.assets
            .iter()
            .filter(|a| {
                if let Some(filter) = &self.filter {
                    if a.asset_type != *filter {
                        return false;
                    }
                }
                if !self.search_query.is_empty() {
                    return a
                        .name
                        .to_lowercase()
                        .contains(&self.search_query.to_lowercase());
                }
                true
            })
            .collect()
    }
}
