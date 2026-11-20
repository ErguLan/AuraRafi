use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

use super::parser::{parse_kicad_footprint, FootprintImportError};
use super::types::{FootprintAsset, FootprintProvenance};
use super::{
    MAX_ASSET_ID_BYTES, MAX_FOOTPRINT_SOURCE_BYTES, MAX_LIBRARY_ASSETS,
    MAX_LIBRARY_TOTAL_BYTES, MAX_STORED_ASSET_BYTES,
};

/// Errors while reading or writing the local footprint collection.
#[derive(Debug)]
pub enum FootprintLibraryError {
    Io(io::Error),
    Parse(FootprintImportError),
    Serialization(String),
    InvalidName(String),
    DuplicateName(String),
    CorruptAsset { path: PathBuf, message: String },
    ResourceLimit { path: PathBuf, limit_bytes: usize },
    TooManyAssets { directory: PathBuf, limit: usize },
}

impl fmt::Display for FootprintLibraryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "footprint library I/O error: {error}"),
            Self::Parse(error) => write!(formatter, "could not import footprint: {error}"),
            Self::Serialization(error) => {
                write!(formatter, "could not serialize footprint: {error}")
            }
            Self::InvalidName(name) => write!(formatter, "invalid footprint name '{name}'"),
            Self::DuplicateName(name) => write!(formatter, "footprint '{name}' already exists"),
            Self::CorruptAsset { path, message } => {
                write!(
                    formatter,
                    "invalid saved footprint '{}': {message}",
                    path.display()
                )
            }
            Self::ResourceLimit { path, limit_bytes } => write!(
                formatter,
                "footprint data '{}' exceeds the {limit_bytes}-byte resource limit",
                path.display()
            ),
            Self::TooManyAssets { directory, limit } => write!(
                formatter,
                "footprint library '{}' exceeds the {limit}-asset limit",
                directory.display()
            ),
        }
    }
}

impl Error for FootprintLibraryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Parse(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for FootprintLibraryError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<FootprintImportError> for FootprintLibraryError {
    fn from(value: FootprintImportError) -> Self {
        Self::Parse(value)
    }
}

/// Project-local collection under `ElectricalAssets/Footprints`.
pub struct FootprintLibrary {
    directory: PathBuf,
}

impl FootprintLibrary {
    /// Import downloaded source while retaining its URL and library license.
    /// Parsing and collision checks are identical to a local import.
    pub fn import_kicad_source_with_provenance(
        &self,
        source: &str,
        provenance: FootprintProvenance,
    ) -> Result<FootprintAsset, FootprintLibraryError> {
        let mut asset = parse_kicad_footprint(source)?;
        asset.provenance.source_path = provenance.source_path;
        asset.provenance.library = provenance.library;
        asset.provenance.license = asset.provenance.license.or(provenance.license);
        let path = self.asset_path(&asset.id);
        if self.name_exists(&asset.name)? || path.exists() {
            return Err(FootprintLibraryError::DuplicateName(asset.name));
        }
        let serialized = ron::ser::to_string_pretty(&asset, ron::ser::PrettyConfig::default())
            .map_err(|e| FootprintLibraryError::Serialization(e.to_string()))?;
        write_new_atomic(&path, serialized.as_bytes())?;
        Ok(asset)
    }
    /// Opens (and creates when needed) the project-local collection.
    pub fn open(project_root: impl AsRef<Path>) -> Result<Self, FootprintLibraryError> {
        let directory = project_root
            .as_ref()
            .join(crate::library::ELECTRICAL_ASSETS_DIR)
            .join("Footprints");
        fs::create_dir_all(&directory)?;
        Ok(Self { directory })
    }

    /// Returns the on-disk collection directory.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Imports one footprint string and atomically stores it as a `.ron` asset.
    /// Existing names are rejected so an import never silently replaces data.
    pub fn import_kicad_footprint(
        &self,
        source: &str,
        name_override: Option<&str>,
    ) -> Result<FootprintAsset, FootprintLibraryError> {
        let mut asset = parse_kicad_footprint(source)?;
        if let Some(name) = name_override {
            if name.trim().is_empty() {
                return Err(FootprintLibraryError::InvalidName(name.to_string()));
            }
            asset.name = name.trim().to_string();
        }
        asset.id = safe_asset_stem(&asset.name)
            .ok_or_else(|| FootprintLibraryError::InvalidName(asset.name.clone()))?;
        let path = self.asset_path(&asset.id);
        if self.name_exists(&asset.name)? || path.exists() {
            return Err(FootprintLibraryError::DuplicateName(asset.name));
        }
        let serialized = ron::ser::to_string_pretty(&asset, ron::ser::PrettyConfig::default())
            .map_err(|error| FootprintLibraryError::Serialization(error.to_string()))?;
        write_new_atomic(&path, serialized.as_bytes())?;
        Ok(asset)
    }

    /// Imports a `.kicad_mod` file and records its source path and `.pretty`
    /// folder name as provenance.
    pub fn import_kicad_file(
        &self,
        source_path: impl AsRef<Path>,
        name_override: Option<&str>,
    ) -> Result<FootprintAsset, FootprintLibraryError> {
        let source_path = source_path.as_ref();
        let source = read_limited_text(source_path, MAX_FOOTPRINT_SOURCE_BYTES)?;
        let mut asset = parse_kicad_footprint(&source)?;
        if let Some(name) = name_override {
            if name.trim().is_empty() {
                return Err(FootprintLibraryError::InvalidName(name.to_string()));
            }
            asset.name = name.trim().to_string();
        }
        asset.id = safe_asset_stem(&asset.name)
            .ok_or_else(|| FootprintLibraryError::InvalidName(asset.name.clone()))?;
        asset.provenance.source_path = Some(source_path.to_string_lossy().into_owned());
        asset.provenance.library = source_path
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .map(str::to_owned);
        let path = self.asset_path(&asset.id);
        if self.name_exists(&asset.name)? || path.exists() {
            return Err(FootprintLibraryError::DuplicateName(asset.name));
        }
        let serialized = ron::ser::to_string_pretty(&asset, ron::ser::PrettyConfig::default())
            .map_err(|error| FootprintLibraryError::Serialization(error.to_string()))?;
        write_new_atomic(&path, serialized.as_bytes())?;
        Ok(asset)
    }

    /// Loads one asset by its display name or stable local id.
    pub fn load(&self, name_or_id: &str) -> Result<Option<FootprintAsset>, FootprintLibraryError> {
        for asset in self.load_all()? {
            if asset.name.eq_ignore_ascii_case(name_or_id)
                || asset.id.eq_ignore_ascii_case(name_or_id)
            {
                return Ok(Some(asset));
            }
        }
        Ok(None)
    }

    /// Loads every saved footprint in stable name order.
    pub fn load_all(&self) -> Result<Vec<FootprintAsset>, FootprintLibraryError> {
        let mut assets = Vec::new();
        let mut total_bytes = 0usize;
        let mut seen_names = BTreeSet::new();
        let mut seen_ids = BTreeSet::new();
        for entry in fs::read_dir(&self.directory)? {
            let entry = entry?;
            let path = entry.path();
            if !entry.file_type()?.is_file()
                || path.extension().and_then(|ext| ext.to_str()) != Some("ron")
            {
                continue;
            }
            if assets.len() >= MAX_LIBRARY_ASSETS {
                return Err(FootprintLibraryError::TooManyAssets {
                    directory: self.directory.clone(),
                    limit: MAX_LIBRARY_ASSETS,
                });
            }
            let contents = read_limited_text(&path, MAX_STORED_ASSET_BYTES)?;
            total_bytes = total_bytes.saturating_add(contents.len());
            if total_bytes > MAX_LIBRARY_TOTAL_BYTES {
                return Err(FootprintLibraryError::ResourceLimit {
                    path: self.directory.clone(),
                    limit_bytes: MAX_LIBRARY_TOTAL_BYTES,
                });
            }
            let asset: FootprintAsset =
                ron::from_str(&contents).map_err(|error| FootprintLibraryError::CorruptAsset {
                    path: path.clone(),
                    message: error.to_string(),
                })?;
            let issues = asset.validate();
            if !issues.is_empty() {
                return Err(FootprintLibraryError::CorruptAsset {
                    path,
                    message: issues
                        .iter()
                        .map(|issue| format!("{}: {}", issue.location, issue.message))
                        .collect::<Vec<_>>()
                        .join("; "),
                });
            }
            let file_id = path.file_stem().and_then(|stem| stem.to_str());
            if file_id != Some(asset.id.as_str())
                || safe_asset_stem(&asset.name).as_deref() != Some(asset.id.as_str())
            {
                return Err(FootprintLibraryError::CorruptAsset {
                    path,
                    message: "saved asset name, id, and filename do not match".into(),
                });
            }
            if !seen_names.insert(asset.name.to_ascii_lowercase())
                || !seen_ids.insert(asset.id.to_ascii_lowercase())
            {
                return Err(FootprintLibraryError::CorruptAsset {
                    path,
                    message: "duplicate saved footprint name or id".into(),
                });
            }
            assets.push(asset);
        }
        assets.sort_by(|left, right| {
            left.name
                .to_ascii_lowercase()
                .cmp(&right.name.to_ascii_lowercase())
        });
        Ok(assets)
    }

    /// Case-insensitive search across name, description, tags, and pad numbers.
    pub fn search(&self, query: &str) -> Result<Vec<FootprintAsset>, FootprintLibraryError> {
        let query = query.trim().to_ascii_lowercase();
        let mut matches = self
            .load_all()?
            .into_iter()
            .filter(|asset| {
                query.is_empty()
                    || asset.name.to_ascii_lowercase().contains(&query)
                    || asset.description.as_deref().is_some_and(|description| {
                        description.to_ascii_lowercase().contains(&query)
                    })
                    || asset
                        .tags
                        .iter()
                        .any(|tag| tag.to_ascii_lowercase().contains(&query))
                    || asset
                        .pads
                        .iter()
                        .any(|pad| pad.number.to_ascii_lowercase().contains(&query))
            })
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| {
            left.name
                .to_ascii_lowercase()
                .cmp(&right.name.to_ascii_lowercase())
        });
        Ok(matches)
    }

    fn asset_path(&self, id: &str) -> PathBuf {
        self.directory.join(format!("{id}.ron"))
    }

    fn name_exists(&self, name: &str) -> Result<bool, FootprintLibraryError> {
        Ok(self
            .load_all()?
            .iter()
            .any(|asset| asset.name.eq_ignore_ascii_case(name)))
    }
}

pub(super) fn safe_asset_stem(name: &str) -> Option<String> {
    let mut output = String::with_capacity(name.len());
    let mut previous_separator = false;
    for character in name.trim().chars() {
        if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
            output.push(character.to_ascii_lowercase());
            previous_separator = false;
        } else if character.is_whitespace()
            || character == '.'
            || character == '/'
            || character == '\\'
        {
            if !previous_separator && !output.is_empty() {
                output.push('_');
                previous_separator = true;
            }
        } else {
            if !previous_separator && !output.is_empty() {
                output.push('_');
                previous_separator = true;
            }
        }
    }
    while output.ends_with('_') {
        output.pop();
    }
    if output.is_empty() || output == "." || output == ".." || output.len() > MAX_ASSET_ID_BYTES {
        None
    } else {
        Some(output)
    }
}

fn read_limited_text(path: &Path, limit_bytes: usize) -> Result<String, FootprintLibraryError> {
    let file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(limit_bytes as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit_bytes {
        return Err(FootprintLibraryError::ResourceLimit {
            path: path.to_path_buf(),
            limit_bytes,
        });
    }
    String::from_utf8(bytes).map_err(|error| {
        FootprintLibraryError::Io(io::Error::new(io::ErrorKind::InvalidData, error))
    })
}

fn write_new_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut temporary_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "footprint".to_string());
    temporary_name.push_str(&format!(".{}.tmp", Uuid::new_v4()));
    let temporary = path.with_file_name(temporary_name);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let write_result = (|| -> io::Result<()> {
        file.write_all(contents)?;
        file.flush()?;
        file.sync_all()?;
        Ok(())
    })();
    drop(file);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }

    // A same-directory hard link publishes the complete file atomically and
    // fails if another importer has already claimed the destination.
    let publish_result = fs::hard_link(&temporary, path);
    let _ = fs::remove_file(&temporary);
    match publish_result {
        Ok(()) => Ok(()),
        Err(error) => Err(error),
    }
}
