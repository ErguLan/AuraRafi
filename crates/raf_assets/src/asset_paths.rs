//! Canonical project asset paths and bounded import copying.
//!
//! Asset references are always relative to the project `assets` directory and
//! never include the `assets/` prefix. This module owns the normalization and
//! confinement rules so importers, commands and catalogs cannot disagree about
//! what a reference means.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use uuid::Uuid;

/// Hard cap for one imported file (model, buffer or texture).
pub const MAX_IMPORT_FILE_BYTES: u64 = 64 * 1024 * 1024;
/// Estimated cap for the whole set of files that belong to one asset package.
pub const MAX_IMPORT_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
/// Cap for the number of files in an asset package.
pub const MAX_PROJECT_ASSETS: usize = 2048;

/// Limits applied while a package is copied into the project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportLimits {
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
    pub max_assets: usize,
}

impl Default for ImportLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: MAX_IMPORT_FILE_BYTES,
            max_total_bytes: MAX_IMPORT_TOTAL_BYTES,
            max_assets: MAX_PROJECT_ASSETS,
        }
    }
}

/// Counters for one import operation. The budget is intentionally explicit:
/// callers can reuse it across a model plus its dependencies.
#[derive(Debug, Clone)]
pub struct ImportBudget {
    limits: ImportLimits,
    used_bytes: u64,
    asset_count: usize,
}

impl Default for ImportBudget {
    fn default() -> Self {
        Self::new(ImportLimits::default())
    }
}

impl ImportBudget {
    pub fn new(limits: ImportLimits) -> Self {
        Self {
            limits,
            used_bytes: 0,
            asset_count: 0,
        }
    }

    pub fn limits(&self) -> ImportLimits {
        self.limits
    }

    pub fn used_bytes(&self) -> u64 {
        self.used_bytes
    }

    pub fn asset_count(&self) -> usize {
        self.asset_count
    }

    pub fn remaining_bytes(&self) -> u64 {
        self.limits
            .max_total_bytes
            .saturating_sub(self.used_bytes)
    }

    /// Validate one addition without recording it.
    pub fn check_addition(&self, bytes: u64) -> Result<(), String> {
        if bytes > self.limits.max_file_bytes {
            return Err(format!(
                "Asset file is {} bytes; the per-file limit is {} bytes.",
                bytes, self.limits.max_file_bytes
            ));
        }
        if self.used_bytes.saturating_add(bytes) > self.limits.max_total_bytes {
            return Err(format!(
                "Asset package would exceed the {} byte total limit.",
                self.limits.max_total_bytes
            ));
        }
        if self.asset_count.saturating_add(1) > self.limits.max_assets {
            return Err(format!(
                "Asset package would exceed the {} file limit.",
                self.limits.max_assets
            ));
        }
        Ok(())
    }

    pub fn record(&mut self, bytes: u64) {
        self.used_bytes = self.used_bytes.saturating_add(bytes);
        self.asset_count = self.asset_count.saturating_add(1);
    }

    /// Validate and record one addition atomically.
    pub fn reserve(&mut self, bytes: u64) -> Result<(), String> {
        self.check_addition(bytes)?;
        self.record(bytes);
        Ok(())
    }
}

/// A file copied into a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopiedFile {
    pub path: PathBuf,
    pub bytes: u64,
}

/// Normalize a project asset reference to the form stored in scene nodes.
///
/// Backslashes become `/`, an optional `assets/` prefix and `./` prefixes are
/// removed, and absolute paths, drive letters, URI schemes and traversal
/// components are rejected.
pub fn normalize_reference(reference: &str) -> Result<String, String> {
    normalize(reference, true)
}

/// Normalize a directory reference below `assets/` (for example `models/props`).
pub fn normalize_directory_reference(reference: &str) -> Result<String, String> {
    let trimmed = reference.trim().trim_end_matches(['/', '\\']);
    if trimmed.is_empty() || trimmed == "." {
        return Err("Asset directory reference cannot be empty.".to_string());
    }
    normalize(trimmed, true)
}

/// Normalize a reference relative to an arbitrary base without asset-prefix
/// semantics. Used for glTF external buffer/image URIs.
pub fn normalize_relative(reference: &str) -> Result<String, String> {
    normalize(reference, false)
}

fn normalize(reference: &str, strip_assets_prefix: bool) -> Result<String, String> {
    let replaced = reference.trim().replace('\\', "/");
    if replaced.is_empty() {
        return Err("Asset reference cannot be empty.".to_string());
    }
    if replaced.chars().any(char::is_control) {
        return Err("Asset reference contains control characters.".to_string());
    }

    let mut value = replaced.as_str();
    while let Some(rest) = value.strip_prefix("./") {
        value = rest;
    }
    if strip_assets_prefix && value.len() >= 7 && value[..7].eq_ignore_ascii_case("assets/") {
        value = &value[7..];
    }
    if value.is_empty() {
        return Err("Asset reference cannot be empty.".to_string());
    }
    if value.starts_with('/') {
        return Err(format!(
            "Asset reference '{reference}' must be relative, not absolute."
        ));
    }
    if value.contains(':') {
        return Err(format!(
            "Asset reference '{reference}' must not contain a drive letter or URI scheme."
        ));
    }

    let mut parts = Vec::new();
    for part in value.split('/') {
        if part.is_empty() {
            return Err(format!(
                "Asset reference '{reference}' contains an empty path component."
            ));
        }
        if part == "." {
            continue;
        }
        if part == ".." {
            return Err(format!(
                "Asset reference '{reference}' must not contain '..' traversal."
            ));
        }
        parts.push(part);
    }
    if parts.is_empty() {
        return Err("Asset reference cannot be empty.".to_string());
    }
    Ok(parts.join("/"))
}

/// Resolve a reference below `base_dir`, rejecting traversal and escaping
/// symlinks. The base directory must exist.
pub fn resolve_confined(base_dir: &Path, reference: &str) -> Result<PathBuf, String> {
    let normalized = normalize_relative(reference)?;
    let base = base_dir.canonicalize().map_err(|error| {
        format!(
            "Cannot resolve asset base '{}': {error}",
            base_dir.display()
        )
    })?;
    let mut candidate = base.clone();
    for part in normalized.split('/') {
        candidate.push(part);
    }
    if !candidate.exists() {
        return Ok(candidate);
    }
    let canonical = candidate.canonicalize().map_err(|error| {
        format!(
            "Cannot resolve asset '{}': {error}",
            candidate.display()
        )
    })?;
    if !canonical.starts_with(&base) {
        return Err(format!(
            "Asset reference '{reference}' escapes the asset directory."
        ));
    }
    Ok(canonical)
}

/// Resolve `reference` below `<project_root>/assets`.
pub fn resolve_under_project(project_root: &Path, reference: &str) -> Result<PathBuf, String> {
    let normalized = normalize_reference(reference)?;
    let assets = project_root.join("assets");
    if !assets.is_dir() {
        return Err(format!(
            "Project assets directory '{}' does not exist.",
            assets.display()
        ));
    }
    resolve_confined(&assets, &normalized)
}

/// Produce a lowercase, filesystem-safe directory slug.
pub fn unique_slug(value: &str) -> String {
    let mut slug = String::with_capacity(value.len());
    let mut last_dash = false;
    for character in value.chars() {
        let normalized = character.to_ascii_lowercase();
        if normalized.is_ascii_alphanumeric() || normalized == '_' {
            slug.push(normalized);
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    let slug = slug.trim_matches(['-', '_']).trim_matches('-');
    let slug = slug.chars().take(64).collect::<String>();
    if slug.is_empty() {
        "model".to_string()
    } else {
        slug
    }
}

/// Create a fresh directory `parent/<slug>`, then `parent/<slug>_1`, ...
pub fn unique_directory(parent: &Path, slug: &str) -> Result<PathBuf, String> {
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "Cannot create asset directory '{}': {error}",
            parent.display()
        )
    })?;
    for index in 0..10_000u32 {
        let name = if index == 0 {
            slug.to_string()
        } else {
            format!("{slug}_{index}")
        };
        let candidate = parent.join(name);
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "Cannot create asset package directory '{}': {error}",
                    candidate.display()
                ))
            }
        }
    }
    Err(format!(
        "Could not allocate a unique directory for '{slug}' in '{}'.",
        parent.display()
    ))
}

/// Return a file name that does not collide with an existing file in
/// `directory`, adding `_1`, `_2`, ... before the extension.
pub fn unique_file_name(directory: &Path, file_name: &str) -> PathBuf {
    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("asset");
    let extension = path.extension().and_then(|extension| extension.to_str());
    let first = directory.join(file_name);
    if !first.exists() {
        return first;
    }
    for index in 1..10_000u32 {
        let candidate_name = match extension {
            Some(extension) => format!("{stem}_{index}.{extension}"),
            None => format!("{stem}_{index}"),
        };
        let candidate = directory.join(candidate_name);
        if !candidate.exists() {
            return candidate;
        }
    }
    let fallback = format!("{stem}_{}", Uuid::new_v4());
    match extension {
        Some(extension) => directory.join(format!("{fallback}.{extension}")),
        None => directory.join(fallback),
    }
}

/// Sanitize a source file name while preserving its extension.
pub fn sanitize_file_name(file_name: &str) -> Result<String, String> {
    let path = Path::new(file_name);
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .ok_or_else(|| format!("Asset file '{file_name}' requires an extension."))?;
    if extension.is_empty() || extension.len() > 16 {
        return Err(format!(
            "Asset file '{file_name}' has an invalid extension."
        ));
    }
    let raw_stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("asset");
    let mut stem = String::with_capacity(raw_stem.len());
    for character in raw_stem.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
            stem.push(character);
        } else {
            stem.push('_');
        }
    }
    let stem = stem.trim_matches(['.', ' ']).chars().take(120).collect::<String>();
    if stem.is_empty() {
        return Err(format!(
            "Asset file '{file_name}' has no usable file name."
        ));
    }
    Ok(format!("{stem}.{extension}"))
}

/// Copy one file into `destination_dir` under a unique, sanitized name.
///
/// The copy is written to a temporary file in the destination directory and
/// renamed into place only after the byte count matches.
pub fn copy_imported_file(
    source: &Path,
    destination_dir: &Path,
    budget: &mut ImportBudget,
) -> Result<CopiedFile, String> {
    let bytes = checked_source_bytes(source)?;
    budget.check_addition(bytes)?;
    fs::create_dir_all(destination_dir).map_err(|error| {
        format!(
            "Cannot create import directory '{}': {error}",
            destination_dir.display()
        )
    })?;
    let source_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("Asset source '{}' has no file name.", source.display()))?;
    let sanitized = sanitize_file_name(source_name)?;
    let destination = unique_file_name(destination_dir, &sanitized);
    copy_to_exact_path(source, &destination, bytes)?;
    budget.record(bytes);
    Ok(CopiedFile {
        path: destination,
        bytes,
    })
}

/// Copy one dependency to an exact destination path. Existing files are never
/// overwritten; callers use a fresh package directory.
pub fn copy_imported_file_exact(
    source: &Path,
    destination: &Path,
    budget: &mut ImportBudget,
) -> Result<CopiedFile, String> {
    let bytes = checked_source_bytes(source)?;
    budget.check_addition(bytes)?;
    if destination.exists() {
        return Err(format!(
            "Imported dependency '{}' already exists.",
            destination.display()
        ));
    }
    copy_to_exact_path(source, destination, bytes)?;
    budget.record(bytes);
    Ok(CopiedFile {
        path: destination.to_path_buf(),
        bytes,
    })
}

fn checked_source_bytes(source: &Path) -> Result<u64, String> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|error| format!("Cannot inspect '{}': {error}", source.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "Asset source '{}' is a symlink; copy the real file instead.",
            source.display()
        ));
    }
    if !metadata.is_file() {
        return Err(format!(
            "Asset source '{}' is not a regular file.",
            source.display()
        ));
    }
    Ok(metadata.len())
}

fn copy_to_exact_path(source: &Path, destination: &Path, bytes: u64) -> Result<(), String> {
    let parent = destination.parent().ok_or_else(|| {
        format!(
            "Destination '{}' has no parent directory.",
            destination.display()
        )
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "Cannot create destination directory '{}': {error}",
            parent.display()
        )
    })?;
    let temporary = parent.join(format!(".raf-import-{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let copied = fs::copy(source, &temporary).map_err(|error| {
            format!(
                "Cannot copy '{}' into the project: {error}",
                source.display()
            )
        })?;
        if copied != bytes {
            return Err(format!(
                "Copy of '{}' wrote {copied} bytes; expected {bytes}.",
                source.display()
            ));
        }
        fs::rename(&temporary, destination).map_err(|error| {
            format!(
                "Cannot finalize imported file '{}': {error}",
                destination.display()
            )
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Write a file atomically through a temporary sibling and a rename.
pub fn write_atomic(path: &Path, contents: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| {
        format!("Destination '{}' has no parent directory.", path.display())
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "Cannot create output directory '{}': {error}",
            parent.display()
        )
    })?;
    let temporary = parent.join(format!(".raf-write-{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut file = File::create(&temporary).map_err(|error| {
            format!(
                "Cannot create temporary file '{}': {error}",
                temporary.display()
            )
        })?;
        file.write_all(contents).map_err(|error| {
            format!(
                "Cannot write temporary file '{}': {error}",
                temporary.display()
            )
        })?;
        file.sync_all().map_err(|error| {
            format!(
                "Cannot flush temporary file '{}': {error}",
                temporary.display()
            )
        })?;
        drop(file);
        fs::rename(&temporary, path).map_err(|error| {
            format!(
                "Cannot finalize file '{}': {error}",
                path.display()
            )
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Stable content hash used for mesh cache invalidation. FNV-1a is deliberately
/// cheap and deterministic across platforms; it is not a security hash.
pub fn content_hash_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("Cannot hash '{}': {error}", path.display()))?;
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("Cannot hash '{}': {error}", path.display()))?;
        if read == 0 {
            break;
        }
        for byte in &buffer[..read] {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    Ok(format!("fnv1a64:{hash:016x}"))
}

/// Build a path below `root` from an already normalized reference.
pub fn join_normalized(root: &Path, normalized: &str) -> PathBuf {
    let mut path = root.to_path_buf();
    for part in normalized.split('/') {
        path.push(part);
    }
    path
}

/// True when every component of `path` is a normal child of `root`.
pub fn is_confined_path(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    relative
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
}
