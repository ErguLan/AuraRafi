//! Render-facing texture ingress: path confinement, decode policy, residency
//! budget, LRU eviction, bounded thumbnails, and offscreen prefetch.
//!
//! `raf_assets` owns source validation and RGBA8 decoding. This module owns the
//! renderer-facing lifetime of every decoded texture and sits behind the
//! ApiGraphicBasic boundary. GPU upload remains in the device executor, which
//! consumes the returned [`CpuTexture`] exactly as before.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use raf_assets::image_output_dimensions;
use raf_assets::ingress::{decode_texture, DecodedTexture, TextureDecodeLimits};

#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

use crate::texture::CpuTexture;

/// Hard default CPU residency for renderer-owned textures.
pub const DEFAULT_TEXTURE_INGRESS_BUDGET_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum thumbnail dimension requested by the v0.11 asset ingress.
pub const TEXTURE_THUMBNAIL_MAX_DIMENSION: u32 = raf_assets::ingress::MAX_THUMBNAIL_DIMENSION;

const MAX_RESIDENT_TEXTURE_ENTRIES: usize = 512;
const MAX_REJECTED_TEXTURE_PATHS: usize = 256;
/// Frames before a failed path is probed again. A missing or corrupt texture
/// used by a visible mesh must not cause a filesystem read every frame.
const REJECTED_TEXTURE_RETRY_FRAMES: u64 = 120;
/// Speculative decodes allowed in flight. Four bounded 1024px textures keep
/// the transient CPU charge proportional to the smallest quality profile.
const MAX_PREFETCH_IN_FLIGHT: usize = 4;
const PREFETCH_REQUEST_QUEUE: usize = 4;
const PREFETCH_COMPLETION_QUEUE: usize = 2;
const FRAME_BUDGET_REJECTION: &str = "texture exceeds the remaining frame texture budget";
const RESIDENCY_BUDGET_REJECTION: &str = "texture is larger than the entire texture budget";

#[cfg(not(target_arch = "wasm32"))]
struct PrefetchRequest {
    key: TextureIngressKey,
    path: Arc<Path>,
    limits: TextureDecodeLimits,
}

#[cfg(not(target_arch = "wasm32"))]
struct PrefetchCompletion {
    key: TextureIngressKey,
    result: Result<raf_assets::ingress::DecodedTexture, String>,
}

/// One named worker that decodes textures which are outside the current
/// frustum. Visible misses keep the synchronous path, so presentation never
/// has to wait for a placeholder or a speculative job.
#[cfg(not(target_arch = "wasm32"))]
struct PrefetchWorker {
    requests: SyncSender<PrefetchRequest>,
    completions: Receiver<PrefetchCompletion>,
    cancelled: Arc<AtomicBool>,
}

#[cfg(not(target_arch = "wasm32"))]
impl PrefetchWorker {
    fn spawn() -> Result<Self, String> {
        let (requests, request_receiver): (SyncSender<PrefetchRequest>, Receiver<PrefetchRequest>) =
            mpsc::sync_channel(PREFETCH_REQUEST_QUEUE);
        let (completion_sender, completions): (
            SyncSender<PrefetchCompletion>,
            Receiver<PrefetchCompletion>,
        ) = mpsc::sync_channel(PREFETCH_COMPLETION_QUEUE);
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        std::thread::Builder::new()
            .name("raf-texture-prefetch".into())
            .spawn(move || {
                while !worker_cancelled.load(Ordering::Acquire) {
                    let request = match request_receiver.recv_timeout(Duration::from_millis(50)) {
                        Ok(request) => request,
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    };
                    if worker_cancelled.load(Ordering::Acquire) {
                        break;
                    }
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        decode_texture(request.path.as_ref(), &request.limits)
                            .map_err(|error| error.to_string())
                    }))
                    .unwrap_or_else(|_| Err("texture prefetch panicked".to_string()));
                    if worker_cancelled.load(Ordering::Acquire) {
                        break;
                    }
                    if completion_sender
                        .send(PrefetchCompletion {
                            key: request.key,
                            result,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|error| format!("Unable to start texture prefetch worker: {error}"))?;
        Ok(Self {
            requests,
            completions,
            cancelled,
        })
    }

    fn try_queue(&self, request: PrefetchRequest) -> PrefetchQueue {
        match self.requests.try_send(request) {
            Ok(()) => PrefetchQueue::Queued,
            Err(TrySendError::Full(_)) => PrefetchQueue::Full,
            Err(TrySendError::Disconnected(_)) => PrefetchQueue::Disconnected,
        }
    }

    fn try_completion(&self) -> PrefetchPoll {
        match self.completions.try_recv() {
            Ok(completion) => PrefetchPoll::Completion(completion),
            Err(TryRecvError::Empty) => PrefetchPoll::Empty,
            Err(TryRecvError::Disconnected) => PrefetchPoll::Disconnected,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
enum PrefetchQueue {
    Queued,
    Full,
    Disconnected,
}

#[cfg(not(target_arch = "wasm32"))]
enum PrefetchPoll {
    Completion(PrefetchCompletion),
    Empty,
    Disconnected,
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for PrefetchWorker {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

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
    /// Speculative decodes admitted before the texture became visible.
    pub prefetched: u64,
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
    /// Speculative decode identities already requested or resident.
    pending_prefetch: HashSet<TextureIngressKey>,
    /// Paths whose speculative decode failed, with their retry frame.
    prefetch_failed: HashMap<PathBuf, u64>,
    #[cfg(not(target_arch = "wasm32"))]
    prefetch: Option<PrefetchWorker>,
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
            pending_prefetch: HashSet::new(),
            prefetch_failed: HashMap::new(),
            #[cfg(not(target_arch = "wasm32"))]
            prefetch: None,
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
        self.poll_prefetch(frame);
        if self.was_recently_rejected(path, frame) {
            return Err("texture is waiting for its rejection retry window".to_string());
        }
        if let Err(error) = limits.validate() {
            return Err(self.reject(path, frame, &error.to_string()));
        }

        let cached_canonical = self.resolutions.get(path).map(Arc::clone);
        if let Some(canonical) = cached_canonical.as_ref() {
            match self.lookup_resident(canonical.as_ref(), limits.max_dimension, frame) {
                ResidentLookup::Hit(texture) => {
                    self.rejected_paths.remove(path);
                    return Ok(texture);
                }
                ResidentLookup::Missing => {}
            }
        }

        let canonical = match self.canonicalize_texture_path(path, asset_root) {
            Ok(canonical) => canonical,
            Err(message) => return Err(self.reject(path, frame, &message)),
        };
        match self.lookup_resident(canonical.as_ref(), limits.max_dimension, frame) {
            ResidentLookup::Hit(texture) => {
                self.rejected_paths.remove(path);
                return Ok(texture);
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
            && self.frame_admitted_bytes.saturating_add(estimated_bytes) > self.frame_budget_bytes
        {
            return Err(self.reject(path, frame, FRAME_BUDGET_REJECTION));
        }

        let decode_started = std::time::Instant::now();
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
        if bytes > 0 {
            let decode_ms = decode_started.elapsed().as_secs_f64() * 1000.0;
            if decode_ms >= 100.0 {
                tracing::info!(
                    path = %canonical.display(),
                    width = decoded.width,
                    height = decoded.height,
                    bytes,
                    decode_ms,
                    "slow renderer texture decode"
                );
            }
        }
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
        self.rejected_paths.remove(path);
        self.rejected_paths.remove(canonical.as_ref());
        self.insert_resident(
            Arc::clone(&canonical),
            limits.max_dimension,
            Arc::clone(&texture),
            bytes,
            frame,
        );
        Ok(texture)
    }

    /// Admits a texture that was decoded ahead of the first visible frame.
    ///
    /// Loading-time warmup decodes the project textures on worker threads so
    /// the first city frame does not pay for them. Admission still enforces the
    /// residency budget and canonical path confinement.
    pub fn admit_decoded(
        &mut self,
        path: &Path,
        asset_root: &Path,
        limits: TextureDecodeLimits,
        decoded: DecodedTexture,
        frame: u64,
    ) -> Result<Arc<CpuTexture>, String> {
        self.begin_frame(frame);
        if let Err(error) = limits.validate() {
            return Err(self.reject(path, frame, &error.to_string()));
        }
        let canonical = match self.canonicalize_texture_path(path, asset_root) {
            Ok(canonical) => canonical,
            Err(message) => return Err(self.reject(path, frame, &message)),
        };
        if let ResidentLookup::Hit(texture) =
            self.lookup_resident(canonical.as_ref(), limits.max_dimension, frame)
        {
            return Ok(texture);
        }
        let bytes = decoded.rgba8.len() as u64;
        if bytes == 0 {
            return Err(self.reject(path, frame, "texture decoded to an empty pixel buffer"));
        }
        if bytes > self.budget_bytes {
            return Err(self.reject(path, frame, RESIDENCY_BUDGET_REJECTION));
        }
        let texture = Arc::new(CpuTexture {
            data: decoded.rgba8,
            width: decoded.width,
            height: decoded.height,
            source: canonical.to_path_buf(),
        });
        self.metrics.misses = self.metrics.misses.saturating_add(1);
        self.rejected_paths.remove(path);
        self.rejected_paths.remove(canonical.as_ref());
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

    /// Speculatively decode a texture that is not yet visible.
    ///
    /// Visible frames keep the synchronous path, so this never substitutes a
    /// placeholder. A prefetched entry is admitted only when it fits the
    /// remaining residency budget without evicting a texture already in use.
    pub fn prefetch(
        &mut self,
        path: &Path,
        asset_root: &Path,
        limits: TextureDecodeLimits,
        frame: u64,
    ) -> bool {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (path, asset_root, limits, frame);
            false
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            if limits.validate().is_err() || self.pending_prefetch.len() >= MAX_PREFETCH_IN_FLIGHT {
                return false;
            }
            if self
                .prefetch_failed
                .get(path)
                .is_some_and(|retry_at| *retry_at >= frame)
            {
                return false;
            }
            let cached_canonical = self.resolutions.get(path).map(Arc::clone);
            let canonical = match cached_canonical {
                Some(canonical) => canonical,
                None => match self.canonicalize_texture_path(path, asset_root) {
                    Ok(canonical) => canonical,
                    Err(_) => {
                        self.remember_prefetch_failure(path.to_path_buf(), frame);
                        return false;
                    }
                },
            };
            if self.has_resident(canonical.as_ref(), limits.max_dimension) {
                return false;
            }
            if self
                .prefetch_failed
                .get(canonical.as_ref())
                .is_some_and(|retry_at| *retry_at >= frame)
            {
                return false;
            }
            let key = TextureIngressKey {
                path: canonical.to_path_buf(),
                max_dimension: limits.max_dimension,
            };
            if self.pending_prefetch.contains(&key) {
                return false;
            }
            if self.prefetch.is_none() {
                match PrefetchWorker::spawn() {
                    Ok(worker) => self.prefetch = Some(worker),
                    Err(error) => {
                        tracing::warn!(%error, "Texture prefetch worker is unavailable");
                        self.remember_prefetch_failure(canonical.to_path_buf(), frame);
                        return false;
                    }
                }
            }
            let worker = self.prefetch.as_ref().expect("prefetch worker was created");
            match worker.try_queue(PrefetchRequest {
                key: key.clone(),
                path: Arc::clone(&canonical),
                limits,
            }) {
                PrefetchQueue::Queued => {
                    self.pending_prefetch.insert(key);
                    true
                }
                PrefetchQueue::Full => false,
                PrefetchQueue::Disconnected => {
                    self.prefetch = None;
                    false
                }
            }
        }
    }

    /// Admit completed speculative decodes. Call once per frame before the
    /// visible texture lookups.
    pub fn poll_prefetch(&mut self, frame: u64) {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = frame;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            for _ in 0..PREFETCH_COMPLETION_QUEUE {
                let poll = match self.prefetch.as_ref() {
                    Some(worker) => worker.try_completion(),
                    None => return,
                };
                match poll {
                    PrefetchPoll::Completion(completion) => {
                        self.admit_prefetch(completion, frame);
                    }
                    PrefetchPoll::Empty => break,
                    PrefetchPoll::Disconnected => {
                        self.prefetch = None;
                        self.pending_prefetch.clear();
                        break;
                    }
                }
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn admit_prefetch(&mut self, completion: PrefetchCompletion, frame: u64) {
        if !self.pending_prefetch.remove(&completion.key) {
            // Released or cleared while the worker was decoding.
            return;
        }
        let decoded = match completion.result {
            Ok(decoded) => decoded,
            Err(_) => {
                self.remember_prefetch_failure(completion.key.path, frame);
                return;
            }
        };
        let bytes = decoded.rgba8.len() as u64;
        let fits = bytes > 0
            && bytes <= self.budget_bytes
            && self.resident_bytes.saturating_add(bytes) <= self.budget_bytes
            && self.entry_count < MAX_RESIDENT_TEXTURE_ENTRIES;
        if !fits || self.has_resident(&completion.key.path, completion.key.max_dimension) {
            self.remember_prefetch_failure(completion.key.path, frame);
            return;
        }
        let texture = Arc::new(CpuTexture {
            data: decoded.rgba8,
            width: decoded.width,
            height: decoded.height,
            source: completion.key.path.clone(),
        });
        let canonical: Arc<Path> = Arc::from(completion.key.path.clone().into_boxed_path());
        self.prefetch_failed.remove(&completion.key.path);
        self.metrics.prefetched = self.metrics.prefetched.saturating_add(1);
        self.insert_resident(
            canonical,
            completion.key.max_dimension,
            texture,
            bytes,
            frame,
        );
    }

    fn remember_prefetch_failure(&mut self, path: PathBuf, frame: u64) {
        if self.prefetch_failed.len() >= MAX_REJECTED_TEXTURE_PATHS {
            self.prefetch_failed
                .retain(|_, retry_at| *retry_at >= frame);
            if self.prefetch_failed.len() >= MAX_REJECTED_TEXTURE_PATHS {
                self.prefetch_failed.clear();
            }
        }
        self.prefetch_failed
            .insert(path, frame.saturating_add(REJECTED_TEXTURE_RETRY_FRAMES));
    }

    fn has_resident(&self, canonical: &Path, max_dimension: u32) -> bool {
        self.entries
            .get(canonical)
            .is_some_and(|slots| slots.iter().any(|slot| slot.max_dimension == max_dimension))
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
        self.prefetch_failed.remove(path);
        self.pending_prefetch.retain(|key| {
            key.path.as_path() != path
                && canonical
                    .as_ref()
                    .is_none_or(|canonical| key.path.as_path() != canonical.as_ref())
        });
        if let Some(canonical) = canonical.as_ref() {
            self.rejected_paths.remove(canonical.as_ref());
            self.prefetch_failed.remove(canonical.as_ref());
        }
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
        self.pending_prefetch.clear();
        self.prefetch_failed.clear();
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.prefetch = None;
        }
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
            prefetched: self.metrics.prefetched,
        }
    }

    fn begin_frame(&mut self, frame: u64) {
        if self.admitted_frame != frame {
            self.admitted_frame = frame;
            self.frame_admitted_bytes = 0;
        }
    }

    fn was_recently_rejected(&self, path: &Path, frame: u64) -> bool {
        self.rejected_paths
            .get(path)
            .is_some_and(|retry_at| *retry_at >= frame)
    }

    fn reject(&mut self, path: &Path, frame: u64, message: &str) -> String {
        self.metrics.rejected = self.metrics.rejected.saturating_add(1);
        if self.rejected_paths.len() >= MAX_REJECTED_TEXTURE_PATHS {
            self.rejected_paths.retain(|_, retry_at| *retry_at >= frame);
            if self.rejected_paths.len() >= MAX_REJECTED_TEXTURE_PATHS {
                self.rejected_paths.clear();
            }
        }
        self.rejected_paths.insert(
            path.to_path_buf(),
            frame.saturating_add(REJECTED_TEXTURE_RETRY_FRAMES),
        );
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
                if first_use {
                    // Residency hits touch the LRU but never consume the
                    // per-frame decode budget. The old accounting rejected
                    // already-decoded textures whenever a scene used more than
                    // half of the resident budget, which hid textures and
                    // caused decode/evict thrash on camera movement.
                    entry.last_used = frame;
                }
                outcome = ResidentLookup::Hit(Arc::clone(&entry.texture));
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
        let position = self
            .lru
            .iter()
            .position(|key| key.max_dimension == max_dimension && key.path.as_path() == canonical);
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
        assert_eq!(
            first.source,
            path.canonicalize().expect("canonical test path")
        );
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

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn prefetched_textures_are_admitted_on_a_later_frame() {
        let root = temporary_root("prefetch");
        let path = write_texture(&root, "texture.png", 8, 8, [10, 20, 30, 255]);
        let mut ingress = TextureIngress::new(1024 * 1024);
        let limits = TextureDecodeLimits {
            max_dimension: 64,
            ..TextureDecodeLimits::default()
        };

        assert!(ingress.prefetch(&path, &root, limits, 1));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while ingress.metrics().prefetched == 0 && std::time::Instant::now() < deadline {
            ingress.poll_prefetch(2);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        assert_eq!(ingress.metrics().prefetched, 1);
        let texture = ingress
            .get_or_decode(&path, &root, limits, 3)
            .expect("the prefetched texture is resident");
        assert_eq!((texture.width, texture.height), (8, 8));
        assert_eq!(ingress.metrics().misses, 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn decoded_textures_can_be_admitted_before_the_first_frame() {
        let root = temporary_root("admit-decoded");
        let path = write_texture(&root, "texture.png", 8, 8, [200, 10, 20, 255]);
        let limits = TextureDecodeLimits::default();
        let decoded = decode_texture(&path, &limits).expect("decode ahead of the first frame");
        let mut ingress = TextureIngress::new(DEFAULT_TEXTURE_INGRESS_BUDGET_BYTES);

        let admitted = ingress
            .admit_decoded(&path, &root, limits, decoded, 1)
            .expect("admit the predecoded texture");
        assert_eq!((admitted.width, admitted.height), (8, 8));
        assert_eq!(ingress.metrics().entries, 1);

        // Overwriting the source must not be read while the admitted texture
        // is resident; the first frame has to reuse the predecoded pixels.
        image::RgbaImage::from_pixel(8, 8, image::Rgba([1, 2, 3, 255]))
            .save(&path)
            .expect("overwrite the temporary texture");
        let reused = ingress
            .get_or_decode(&path, &root, limits, 2)
            .expect("the predecoded texture is resident");
        assert!(Arc::ptr_eq(&admitted, &reused));
        assert_eq!(&reused.data[..4], &[200, 10, 20, 255]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn predecoded_textures_outside_the_root_are_rejected() {
        let root = temporary_root("admit-root");
        let outside = temporary_root("admit-outside");
        let path = write_texture(&outside, "texture.png", 8, 8, [10, 20, 30, 255]);
        let limits = TextureDecodeLimits::default();
        let decoded = decode_texture(&path, &limits).expect("decode the outside texture");
        let mut ingress = TextureIngress::new(DEFAULT_TEXTURE_INGRESS_BUDGET_BYTES);

        let error = ingress
            .admit_decoded(&path, &root, limits, decoded, 1)
            .expect_err("predecoded textures outside the asset root must be rejected");

        assert!(error.contains("escapes"));
        assert_eq!(ingress.metrics().entries, 0);
        assert_eq!(ingress.metrics().resident_bytes, 0);
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }

    #[test]
    fn rejected_textures_wait_before_probing_the_source_again() {
        let root = temporary_root("reject-cooldown");
        let path = write_texture(&root, "texture.png", 8, 8, [10, 20, 30, 255]);
        let mut ingress = TextureIngress::new(16);

        ingress
            .get_or_decode(&path, &root, TextureDecodeLimits::default(), 1)
            .expect_err("oversized textures are rejected");
        ingress
            .get_or_decode(&path, &root, TextureDecodeLimits::default(), 2)
            .expect_err("the rejection cooldown keeps the same error");
        assert_eq!(ingress.metrics().rejected, 1);

        ingress
            .get_or_decode(&path, &root, TextureDecodeLimits::default(), 122)
            .expect_err("the path is probed again after the cooldown");
        assert_eq!(ingress.metrics().rejected, 2);
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

    #[test]
    fn resident_hits_do_not_consume_the_per_frame_decode_budget() {
        let root = temporary_root("resident-free");
        let paths = [
            write_texture(&root, "a.png", 4, 4, [10, 20, 30, 255]),
            write_texture(&root, "b.png", 4, 4, [40, 50, 60, 255]),
            write_texture(&root, "c.png", 4, 4, [70, 80, 90, 255]),
        ];
        let mut ingress = TextureIngress::new(1024 * 1024);
        ingress.set_frame_budget(100);
        let limits = TextureDecodeLimits::default();
        for (index, path) in paths.iter().enumerate() {
            ingress
                .get_or_decode(path, &root, limits, index as u64 + 1)
                .expect("each texture decodes on its own frame");
        }

        // One frame requests all three resident textures. The decode budget
        // must not reject already-decoded entries.
        for path in &paths {
            ingress
                .get_or_decode(path, &root, limits, 10)
                .expect("resident texture stays available");
        }
        assert_eq!(ingress.metrics().entries, 3);
        let _ = fs::remove_dir_all(root);
    }
}
