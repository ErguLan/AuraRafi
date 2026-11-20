//! Render-facing texture ingress: path confinement, decode policy, residency
//! budget, LRU eviction, and bounded thumbnails.
//!
//! `raf_assets` owns source validation and RGBA8 decoding. This module owns the
//! renderer-facing lifetime of every decoded texture and sits behind the
//! ApiGraphicBasic boundary. GPU upload remains in the device executor, which
//! consumes the returned [`CpuTexture`] exactly as before.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use raf_assets::image_output_dimensions;
use raf_assets::ingress::{decode_texture, TextureDecodeLimits};

use crate::texture::CpuTexture;

/// Hard default CPU residency for renderer-owned textures.
pub const DEFAULT_TEXTURE_INGRESS_BUDGET_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum thumbnail dimension requested by the v0.11 asset ingress.
pub const TEXTURE_THUMBNAIL_MAX_DIMENSION: u32 = raf_assets::ingress::MAX_THUMBNAIL_DIMENSION;

const MAX_RESIDENT_TEXTURE_ENTRIES: usize = 512;
const MAX_REJECTED_TEXTURE_PATHS: usize = 256;
const FRAME_BUDGET_REJECTION: &str = "texture exceeds the remaining frame texture budget";
const RESIDENCY_BUDGET_REJECTION: &str = "texture is larger than the entire texture budget";

/// Identity of one resident texture variant.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TextureIngressKey {
    /// Canonical source path.
    pub path: PathBuf,
    /// Decoded output dimension cap used for this entry.
    pub max_dimension: u32,
}

/// Live and cumulative texture ingress diagnostics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextureIngressMetrics {
    /// Bytes currently held by resident textures and thumbnails.
    pub resident_bytes: u64,
    /// Resident texture variants, including bounded thumbnails.
    pub entries: usize,
    /// Cache hits, including repeated uses inside the same frame.
    pub hits: u64,
    /// Successful decodes admitted into the residency cache.
    pub misses: u64,
    /// Entries evicted by the LRU budget or the entry cap.
    pub evictions: u64,
    /// Path, decode, or budget rejections observed by the ingress.
    pub rejected: u64,
}

/// One resident texture and its optional bounded thumbnail.
#[derive(Debug, Clone)]
pub struct TextureIngressEntry {
    texture: Arc<CpuTexture>,
    thumbnail: Option<Arc<CpuTexture>>,
    bytes: u64,
    last_used: u64,
}

impl TextureIngressEntry {
    /// The full bounded texture returned to render commands.
    pub fn texture(&self) -> &Arc<CpuTexture> {
        &self.texture
    }

    /// The optional thumbnail variant cached with this entry.
    pub fn thumbnail(&self) -> Option<&Arc<CpuTexture>> {
        self.thumbnail.as_ref()
    }

    /// Bytes charged to the ingress budget for this entry.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Frame index of the last use.
    pub fn last_used(&self) -> u64 {
        self.last_used
    }
}

#[derive(Debug, Clone)]
struct TextureIngressSlot {
    max_dimension: u32,
    entry: TextureIngressEntry,
}

enum ResidentLookup {
    Hit(Arc<CpuTexture>),
    BudgetRejected,
    Missing,
}

/// Budgeted decode cache for renderer-owned textures.
///
/// The ingress canonicalizes every source path and requires it to stay under
/// the configured project asset root. Admission is charged against a hard
/// residency budget, a per-frame admission budget, and an entry-count cap.
pub struct TextureIngress {
    /// Canonical path -> per-dimension variants. Path-keyed storage lets a
    /// cached lookup borrow `&Path` instead of allocating a temporary key.
    entries: HashMap<PathBuf, Vec<TextureIngressSlot>>,
    lru: VecDeque<TextureIngressKey>,
    resolutions: HashMap<PathBuf, Arc<Path>>,
    rejected_paths: HashMap<PathBuf, u64>,
    canonical_root: Option<(PathBuf, Arc<Path>)>,
    budget_bytes: u64,
    resident_bytes: u64,
    entry_count: usize,
    frame_budget_bytes: u64,
    frame_admitted_bytes: u64,
    admitted_frame: u64,
    metrics: TextureIngressMetrics,
}

impl TextureIngress {
    /// Create an ingress with a hard residency budget.
    pub fn new(budget_bytes: u64) -> Self {
        Self {
            entries: HashMap::new(),
            lru: VecDeque::new(),
            resolutions: HashMap::new(),
            rejected_paths: HashMap::new(),
            canonical_root: None,
            budget_bytes,
            resident_bytes: 0,
            entry_count: 0,
            frame_budget_bytes: u64::MAX,
            frame_admitted_bytes: 0,
            admitted_frame: 0,
            metrics: TextureIngressMetrics::default(),
        }
    }

    /// Current residency budget in bytes.
    pub fn budget_bytes(&self) -> u64 {
        self.budget_bytes
    }

    /// Replace the residency budget and evict until it holds again.
    pub fn set_budget(&mut self, budget_bytes: u64) {
        self.budget_bytes = budget_bytes;
        self.evict_to_budget();
    }

    /// Limit how many new texture bytes each frame may admit.
    pub fn set_frame_budget(&mut self, frame_budget_bytes: u64) {
        self.frame_budget_bytes = frame_budget_bytes;
    }

    /// Get a resident texture or decode one through the bounded ingress.
    ///
    /// The returned texture always belongs to this ingress and carries the
    /// canonical source path, so command lists and the GPU executor share one
    /// cache key. A texture larger than the whole residency budget is rejected
    /// instead of admitting an entry that can never fit.
    pub fn get_or_decode(
        &mut self,
        path: &Path,
        asset_root: &Path,
        limits: TextureDecodeLimits,
        frame: u64,
    ) -> Result<Arc<CpuTexture>, String> {
        self.begin_frame(frame);
        if self.was_rejected_this_frame(path, frame) {
            return Err("texture was already rejected in this frame".to_string());
        }
        if let Err(error) = limits.validate() {
            return Err(self.reject(path, frame, &error.to_string()));
        }

        let cached_canonical = self.resolutions.get(path).map(Arc::clone);
        if let Some(canonical) = cached_canonical.as_ref() {
            match self.lookup_resident(canonical.as_ref(), limits.max_dimension, frame) {
                ResidentLookup::Hit(texture) => return Ok(texture),
                ResidentLookup::BudgetRejected => {
                    return Err(self.reject(path, frame, FRAME_BUDGET_REJECTION));
                }
                ResidentLookup::Missing => {}
            }
        }

        let canonical = match self.canonicalize_texture_path(path, asset_root) {
            Ok(canonical) => canonical,
            Err(message) => return Err(self.reject(path, frame, &message)),
        };
        match self.lookup_resident(canonical.as_ref(), limits.max_dimension, frame) {
            ResidentLookup::Hit(texture) => return Ok(texture),
            ResidentLookup::BudgetRejected => {
                return Err(self.reject(path, frame, FRAME_BUDGET_REJECTION));
            }
            ResidentLookup::Missing => {}
        }

        let estimated_bytes = image_output_dimensions(canonical.as_ref(), limits.max_dimension)
            .ok()
            .and_then(|(width, height)| {
                u64::from(width)
                    .checked_mul(u64::from(height))
                    .and_then(|pixels| pixels.checked_mul(4))
            })
            .unwrap_or(0);
        if estimated_bytes > self.budget_bytes {
            return Err(self.reject(path, frame, RESIDENCY_BUDGET_REJECTION));
        }
        if estimated_bytes > 0
            && self.frame_admitted_bytes.saturating_add(estimated_bytes)
                > self.frame_budget_bytes
        {
            return Err(self.reject(path, frame, FRAME_BUDGET_REJECTION));
        }

        let decoded = match decode_texture(canonical.as_ref(), &limits) {
            Ok(decoded) => decoded,
            Err(error) => {
                tracing::warn!(
                    path = %canonical.display(),
                    %error,
                    "Unable to decode a renderer-owned texture"
                );
                return Err(self.reject(path, frame, &error.to_string()));
            }
        };
        let bytes = decoded.rgba8.len() as u64;
        if bytes == 0 {
            return Err(self.reject(path, frame, "texture decoded to an empty pixel buffer"));
        }
        if bytes > self.budget_bytes {
            return Err(self.reject(path, frame, RESIDENCY_BUDGET_REJECTION));
        }
        if self.frame_admitted_bytes.saturating_add(bytes) > self.frame_budget_bytes {
            return Err(self.reject(path, frame, FRAME_BUDGET_REJECTION));
        }

        let texture = Arc::new(CpuTexture {
            data: decoded.rgba8,
            width: decoded.width,
            height: decoded.height,
            source: canonical.to_path_buf(),
        });
        self.frame_admitted_bytes = self.frame_admitted_bytes.saturating_add(bytes);
        self.metrics.misses = self.metrics.misses.saturating_add(1);
        self.insert_resident(
            Arc::clone(&canonical),
            limits.max_dimension,
            Arc::clone(&texture),
            bytes,
            frame,
        );
        Ok(texture)
    }

    /// Get a thumbnail bounded to 128 px, cached separately from the full view.
    ///
    /// The thumbnail is decoded at its own output dimension, so requesting a
    /// preview never has to allocate a full-resolution pixel buffer first.
    pub fn get_or_thumbnail(
        &mut self,
        path: &Path,
        asset_root: &Path,
        limits: TextureDecodeLimits,
        frame: u64,
    ) -> Result<Arc<CpuTexture>, String> {
        let thumbnail_limit = limits
            .max_dimension
            .min(TEXTURE_THUMBNAIL_MAX_DIMENSION)
            .max(1);
        let thumbnail_limits = limits.with_max_dimension(thumbnail_limit);
        let texture = self.get_or_decode(path, asset_root, thumbnail_limits, frame)?;

        if let Some(canonical) = self.resolutions.get(path).map(Arc::clone) {
            if let Some(slots) = self.entries.get_mut(canonical.as_ref()) {
                if let Some(slot) = slots
                    .iter_mut()
                    .find(|slot| slot.max_dimension == thumbnail_limit)
                {
                    slot.entry
                        .thumbnail
                        .get_or_insert_with(|| Arc::clone(&texture));
                }
            }
        }
        Ok(texture)
    }

    /// Drop every resident variant that resolves to `path`.
    pub fn release(&mut self, path: &Path) {
        let canonical = self.resolutions.get(path).map(Arc::clone);
        self.remove_resident_path(path);
        if let Some(canonical) = canonical.as_ref() {
            self.remove_resident_path(canonical.as_ref());
        }
        self.resolutions.remove(path);
        self.rejected_paths.remove(path);
    }

    /// Release all textures and path-resolution memory.
    ///
    /// Cumulative hit, miss, eviction, and rejection counters are retained for
    /// session diagnostics; live residency becomes zero.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.lru.clear();
        self.resolutions.clear();
        self.rejected_paths.clear();
        self.canonical_root = None;
        self.resident_bytes = 0;
        self.entry_count = 0;
        self.frame_admitted_bytes = 0;
    }

    /// Snapshot of live residency and cumulative ingress counters.
    pub fn metrics(&self) -> TextureIngressMetrics {
        TextureIngressMetrics {
            resident_bytes: self.resident_bytes,
            entries: self.entry_count,
            hits: self.metrics.hits,
            misses: self.metrics.misses,
            evictions: self.metrics.evictions,
            rejected: self.metrics.rejected,
        }
    }

    fn begin_frame(&mut self, frame: u64) {
        if self.admitted_frame != frame {
            self.admitted_frame = frame;
            self.frame_admitted_bytes = 0;
        }
    }

    fn was_rejected_this_frame(&self, path: &Path, frame: u64) -> bool {
        self.rejected_paths
            .get(path)
            .is_some_and(|rejected| *rejected == frame)
    }

    fn reject(&mut self, path: &Path, frame: u64, message: &str) -> String {
        self.metrics.rejected = self.metrics.rejected.saturating_add(1);
        if self.rejected_paths.len() >= MAX_REJECTED_TEXTURE_PATHS {
            self.rejected_paths
                .retain(|_, rejected| *rejected == frame);
            if self.rejected_paths.len() >= MAX_REJECTED_TEXTURE_PATHS {
                self.rejected_paths.clear();
            }
        }
        self.rejected_paths.insert(path.to_path_buf(), frame);
        message.to_string()
    }

    fn lookup_resident(
        &mut self,
        canonical: &Path,
        max_dimension: u32,
        frame: u64,
    ) -> ResidentLookup {
        let mut first_use = false;
        let mut outcome = ResidentLookup::Missing;
        if let Some(slots) = self.entries.get_mut(canonical) {
            if let Some(slot) = slots
                .iter_mut()
                .find(|slot| slot.max_dimension == max_dimension)
            {
                let entry = &mut slot.entry;
                first_use = entry.last_used != frame;
                if first_use
                    && self.frame_admitted_bytes.saturating_add(entry.bytes)
                        > self.frame_budget_bytes
                {
                    outcome = ResidentLookup::BudgetRejected;
                } else {
                    if first_use {
                        self.frame_admitted_bytes =
                            self.frame_admitted_bytes.saturating_add(entry.bytes);
                        entry.last_used = frame;
                    }
                    outcome = ResidentLookup::Hit(Arc::clone(&entry.texture));
                }
            }
        }

        if matches!(&outcome, ResidentLookup::Hit(_)) {
            self.metrics.hits = self.metrics.hits.saturating_add(1);
            if first_use {
                self.touch_lru(canonical, max_dimension);
            }
        }
        outcome
    }

    fn touch_lru(&mut self, canonical: &Path, max_dimension: u32) {
        let position = self.lru.iter().position(|key| {
            key.max_dimension == max_dimension && key.path.as_path() == canonical
        });
        if let Some(position) = position {
            if let Some(key) = self.lru.remove(position) {
                self.lru.push_back(key);
            }
        }
    }

    fn canonical_asset_root(&mut self, asset_root: &Path) -> Result<Arc<Path>, String> {
        if let Some((root, canonical)) = self.canonical_root.as_ref() {
            if root.as_path() == asset_root {
                return Ok(Arc::clone(canonical));
            }
        }
        let canonical = std::fs::canonicalize(asset_root)
            .map_err(|_| "project asset root is not available".to_string())?;
        let canonical: Arc<Path> = Arc::from(canonical.into_boxed_path());
        self.canonical_root = Some((asset_root.to_path_buf(), Arc::clone(&canonical)));
        Ok(canonical)
    }

    fn canonicalize_texture_path(
        &mut self,
        path: &Path,
        asset_root: &Path,
    ) -> Result<Arc<Path>, String> {
        let canonical_root = self.canonical_asset_root(asset_root)?;
        match std::fs::canonicalize(path) {
            Ok(canonical) => {
                let canonical: Arc<Path> = Arc::from(canonical.into_boxed_path());
                if !canonical.starts_with(canonical_root.as_ref()) {
                    self.resolutions.remove(path);
                    return Err("texture path escapes the project asset root".to_string());
                }
                self.resolutions
                    .insert(path.to_path_buf(), Arc::clone(&canonical));
                Ok(canonical)
            }
            Err(_) => {
                self.resolutions.remove(path);
                Err("texture source file was not found".to_string())
            }
        }
    }

    fn insert_resident(
        &mut self,
        canonical: Arc<Path>,
        max_dimension: u32,
        texture: Arc<CpuTexture>,
        bytes: u64,
        frame: u64,
    ) {
        self.remove_resident_slot(canonical.as_ref(), max_dimension);
        self.entries
            .entry(canonical.to_path_buf())
            .or_default()
            .push(TextureIngressSlot {
                max_dimension,
                entry: TextureIngressEntry {
                    texture,
                    thumbnail: None,
                    bytes,
                    last_used: frame,
                },
            });
        self.entry_count = self.entry_count.saturating_add(1);
        self.resident_bytes = self.resident_bytes.saturating_add(bytes);
        self.lru.retain(|key| {
            !(key.max_dimension == max_dimension && key.path.as_path() == canonical.as_ref())
        });
        self.lru.push_back(TextureIngressKey {
            path: canonical.to_path_buf(),
            max_dimension,
        });
        self.evict_to_budget();
    }

    fn remove_resident_slot(&mut self, canonical: &Path, max_dimension: u32) -> bool {
        let mut removed_bytes = 0_u64;
        let mut removed = false;
        let mut remove_map_entry = false;
        if let Some(slots) = self.entries.get_mut(canonical) {
            if let Some(position) = slots
                .iter()
                .position(|slot| slot.max_dimension == max_dimension)
            {
                removed_bytes = slots.remove(position).entry.bytes;
                removed = true;
            }
            remove_map_entry = slots.is_empty();
        }
        if remove_map_entry {
            self.entries.remove(canonical);
        }
        if removed {
            self.resident_bytes = self.resident_bytes.saturating_sub(removed_bytes);
            self.entry_count = self.entry_count.saturating_sub(1);
        }
        removed
    }

    fn remove_resident_path(&mut self, canonical: &Path) -> u64 {
        let Some(slots) = self.entries.remove(canonical) else {
            return 0;
        };
        let mut removed_bytes = 0_u64;
        for slot in slots {
            removed_bytes = removed_bytes.saturating_add(slot.entry.bytes);
            self.entry_count = self.entry_count.saturating_sub(1);
        }
        self.resident_bytes = self.resident_bytes.saturating_sub(removed_bytes);
        self.lru.retain(|key| key.path.as_path() != canonical);
        removed_bytes
    }

    fn evict_to_budget(&mut self) {
        while self.resident_bytes > self.budget_bytes
            || self.entry_count > MAX_RESIDENT_TEXTURE_ENTRIES
        {
            let Some(key) = self.lru.pop_front() else {
                break;
            };
            if self.remove_resident_slot(key.path.as_path(), key.max_dimension) {
                self.metrics.evictions = self.metrics.evictions.saturating_add(1);
            }
        }
    }
}

impl Default for TextureIngress {
    fn default() -> Self {
        Self::new(DEFAULT_TEXTURE_INGRESS_BUDGET_BYTES)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temporary_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "raf-texture-ingress-{label}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).expect("create temporary asset root");
        root
    }

    fn write_texture(root: &Path, name: &str, width: u32, height: u32, color: [u8; 4]) -> PathBuf {
        let path = root.join(name);
        image::RgbaImage::from_pixel(width, height, image::Rgba(color))
            .save(&path)
            .expect("write temporary texture");
        path
    }

    #[test]
    fn paths_outside_the_asset_root_are_rejected_before_decode() {
        let root = temporary_root("root");
        let outside = temporary_root("outside");
        let path = write_texture(&outside, "texture.png", 4, 4, [10, 20, 30, 255]);
        let mut ingress = TextureIngress::new(DEFAULT_TEXTURE_INGRESS_BUDGET_BYTES);

        let error = ingress
            .get_or_decode(&path, &root, TextureDecodeLimits::default(), 1)
            .expect_err("textures outside the asset root must be rejected");

        assert!(error.contains("escapes"));
        let metrics = ingress.metrics();
        assert_eq!(metrics.entries, 0);
        assert_eq!(metrics.resident_bytes, 0);
        assert_eq!(metrics.rejected, 1);
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }

    #[test]
    fn decoded_textures_are_reused_across_frames() {
        let root = temporary_root("cache");
        let path = write_texture(&root, "texture.png", 4, 4, [10, 20, 30, 255]);
        let mut ingress = TextureIngress::new(DEFAULT_TEXTURE_INGRESS_BUDGET_BYTES);
        let limits = TextureDecodeLimits::default();

        let first = ingress
            .get_or_decode(&path, &root, limits, 1)
            .expect("decode the first frame");
        let second = ingress
            .get_or_decode(&path, &root, limits, 2)
            .expect("reuse the resident texture");

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.source, path.canonicalize().expect("canonical test path"));
        let metrics = ingress.metrics();
        assert_eq!(metrics.misses, 1);
        assert_eq!(metrics.hits, 1);
        assert_eq!(metrics.entries, 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn lru_eviction_keeps_residency_inside_the_budget() {
        let root = temporary_root("evict");
        let first = write_texture(&root, "first.png", 8, 8, [255, 0, 0, 255]);
        let second = write_texture(&root, "second.png", 8, 8, [0, 0, 255, 255]);
        let texture_bytes = 8 * 8 * 4;
        let mut ingress = TextureIngress::new(texture_bytes as u64);
        let limits = TextureDecodeLimits::default();

        ingress
            .get_or_decode(&first, &root, limits, 1)
            .expect("decode the first texture");
        ingress
            .get_or_decode(&second, &root, limits, 2)
            .expect("decode the second texture");

        let metrics = ingress.metrics();
        assert_eq!(metrics.entries, 1);
        assert_eq!(metrics.evictions, 1);
        assert!(metrics.resident_bytes <= texture_bytes as u64);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn textures_larger_than_the_whole_budget_are_rejected() {
        let root = temporary_root("oversized");
        let path = write_texture(&root, "texture.png", 8, 8, [10, 20, 30, 255]);
        let mut ingress = TextureIngress::new(16);

        let error = ingress
            .get_or_decode(&path, &root, TextureDecodeLimits::default(), 1)
            .expect_err("oversized textures must not be admitted");

        assert!(error.contains("budget"));
        let metrics = ingress.metrics();
        assert_eq!(metrics.entries, 0);
        assert_eq!(metrics.resident_bytes, 0);
        assert_eq!(metrics.rejected, 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn thumbnails_are_bounded_and_reused() {
        let root = temporary_root("thumbnail");
        let path = write_texture(&root, "texture.png", 512, 256, [30, 60, 90, 255]);
        let mut ingress = TextureIngress::new(DEFAULT_TEXTURE_INGRESS_BUDGET_BYTES);
        let limits = TextureDecodeLimits::default();

        let first = ingress
            .get_or_thumbnail(&path, &root, limits, 1)
            .expect("decode a bounded thumbnail");
        let second = ingress
            .get_or_thumbnail(&path, &root, limits, 2)
            .expect("reuse the cached thumbnail");

        assert_eq!((first.width, first.height), (128, 64));
        assert!(first.width <= TEXTURE_THUMBNAIL_MAX_DIMENSION);
        assert!(first.height <= TEXTURE_THUMBNAIL_MAX_DIMENSION);
        assert!(Arc::ptr_eq(&first, &second));
        let metrics = ingress.metrics();
        assert_eq!(metrics.entries, 1);
        assert_eq!(metrics.misses, 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn release_and_clear_return_residency_to_zero() {
        let root = temporary_root("release");
        let path = write_texture(&root, "texture.png", 4, 4, [10, 20, 30, 255]);
        let mut ingress = TextureIngress::new(DEFAULT_TEXTURE_INGRESS_BUDGET_BYTES);
        let limits = TextureDecodeLimits::default();

        ingress
            .get_or_decode(&path, &root, limits, 1)
            .expect("decode the texture");
        ingress.release(&path);
        assert_eq!(ingress.metrics().entries, 0);
        assert_eq!(ingress.metrics().resident_bytes, 0);

        ingress
            .get_or_decode(&path, &root, limits, 2)
            .expect("decode the texture again");
        ingress.clear();
        assert_eq!(ingress.metrics().entries, 0);
        assert_eq!(ingress.metrics().resident_bytes, 0);
        let _ = fs::remove_dir_all(root);
    }
}
