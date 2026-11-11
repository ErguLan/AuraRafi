//! Resource cache for raster images referenced by retained UI documents.
//!
//! Documents store only `UiImageSource` keys. This cache owns decoded pixels
//! at the surface-host boundary, so a project can replace or unload icons
//! without modifying serialized layout data.
#[path = "image_resource_loader.rs"]
mod image_resource_loader;

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use image_resource_loader::{ProjectImageCompletion, ProjectImageResourceLoader};
use raf_ui::{UiColorPicker, UiControl, UiIcon, UiIconId, UiImageResourceStatus, UiNode};

const COLOR_PICKER_SOURCE_PREFIX: &str = "builtin://color-picker/hsv/";
const COLOR_PICKER_IMAGE_SIZE: usize = UiColorPicker::CANVAS_SIZE as usize;
const DEFAULT_UI_IMAGE_BUDGET_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct UiSurfaceImageData {
    pub size: [u32; 2],
    pub pixels: Vec<u8>,
    pub revision: u64,
}

/// CPU-side pixel residency for retained UI images.
///
/// The store is a work budget, not an FPS cap: it bounds decoded RGBA bytes
/// and evicts the least-recently-used entry when admission would cross the
/// budget. Oversized single images are rejected instead of ballooning memory.
#[derive(Debug)]
pub struct UiSurfaceImageStore {
    images: BTreeMap<String, UiSurfaceImageData>,
    next_revision: u64,
    budget_bytes: u64,
    resident_bytes: u64,
    evictions: u64,
    use_clock: AtomicU64,
    last_used: Mutex<BTreeMap<String, u64>>,
    project_epoch: Arc<AtomicU64>,
    project_keys: BTreeSet<String>,
    project_status: BTreeMap<String, UiImageResourceStatus>,
    project_loader: Option<ProjectImageResourceLoader>,
    deferred_completion: Option<ProjectImageCompletion>,
    work_limits: raf_ui::UiWorkLimits,
    performance: raf_ui::UiPerformanceMonitor,
}

impl Clone for UiSurfaceImageStore {
    fn clone(&self) -> Self {
        let images = self.images.clone();
        let project_status = self
            .project_status
            .iter()
            .map(|(key, status)| {
                let snapshot_status = if images.contains_key(key) {
                    UiImageResourceStatus::Ready
                } else if *status == UiImageResourceStatus::Pending {
                    UiImageResourceStatus::Missing
                } else {
                    *status
                };
                (key.clone(), snapshot_status)
            })
            .collect();
        let last_used = self
            .last_used
            .lock()
            .map(|last_used| last_used.clone())
            .unwrap_or_default();
        Self {
            images,
            next_revision: self.next_revision,
            budget_bytes: self.budget_bytes,
            resident_bytes: self.resident_bytes,
            evictions: self.evictions,
            use_clock: AtomicU64::new(self.use_clock.load(Ordering::Relaxed)),
            last_used: Mutex::new(last_used),
            project_epoch: Arc::new(AtomicU64::new(self.project_epoch.load(Ordering::Acquire))),
            project_keys: self.project_keys.clone(),
            project_status,
            // A clone is a pixel-residency snapshot, not a second owner of the
            // live project's asynchronous decode lifecycle.
            project_loader: None,
            deferred_completion: None,
            work_limits: self.work_limits,
            performance: self.performance.clone(),
        }
    }
}

impl Default for UiSurfaceImageStore {
    fn default() -> Self {
        Self {
            images: BTreeMap::new(),
            next_revision: 0,
            budget_bytes: DEFAULT_UI_IMAGE_BUDGET_BYTES,
            resident_bytes: 0,
            evictions: 0,
            use_clock: AtomicU64::new(0),
            last_used: Mutex::new(BTreeMap::new()),
            project_epoch: Arc::new(AtomicU64::new(0)),
            project_keys: BTreeSet::new(),
            project_status: BTreeMap::new(),
            project_loader: None,
            deferred_completion: None,
            work_limits: raf_ui::UiWorkLimits::default(),
            performance: raf_ui::UiPerformanceMonitor::default(),
        }
    }
}

impl UiSurfaceImageStore {
    pub fn insert_rgba(
        &mut self,
        key: impl Into<String>,
        size: [u32; 2],
        pixels: Vec<u8>,
    ) -> Result<(), String> {
        let width = size[0].max(1);
        let height = size[1].max(1);
        let expected = width as usize * height as usize * 4;
        if pixels.len() != expected {
            return Err(format!(
                "UI image has {} bytes; expected {} for {}x{} RGBA.",
                pixels.len(),
                expected,
                width,
                height
            ));
        }
        let key = key.into();
        let key = canonical_builtin_icon_uri(&key).into_owned();
        let incoming_bytes = pixels.len() as u64;
        if incoming_bytes > self.budget_bytes {
            return Err(format!(
                "UI image '{}' needs {} bytes; budget is {} bytes.",
                key, incoming_bytes, self.budget_bytes
            ));
        }
        loop {
            let replaced_bytes = self
                .images
                .get(&key)
                .map(|image| image.pixels.len() as u64)
                .unwrap_or(0);
            let prospective = self
                .resident_bytes
                .saturating_sub(replaced_bytes)
                .saturating_add(incoming_bytes);
            if prospective <= self.budget_bytes {
                break;
            }
            if self.evict_oldest_except(Some(key.as_str())).is_none() {
                return Err(format!(
                    "UI image '{}' needs {} resident bytes; budget is {} bytes.",
                    key, prospective, self.budget_bytes
                ));
            }
        }
        if let Some(previous) = self.images.remove(&key) {
            self.resident_bytes = self
                .resident_bytes
                .saturating_sub(previous.pixels.len() as u64);
        }
        self.next_revision = self.next_revision.wrapping_add(1).max(1);
        self.resident_bytes = self.resident_bytes.saturating_add(incoming_bytes);
        self.images.insert(
            key.clone(),
            UiSurfaceImageData {
                size: [width, height],
                pixels,
                revision: self.next_revision,
            },
        );
        if self.project_keys.contains(&key) {
            self.project_status
                .insert(key.clone(), UiImageResourceStatus::Ready);
        }
        self.touch(&key);
        Ok(())
    }

    pub fn load_png(&mut self, key: impl Into<String>, path: &Path) -> Result<(), String> {
        let image = image::open(path)
            .map_err(|error| format!("Unable to read UI image '{}': {error}", path.display()))?
            .to_rgba8();
        self.insert_rgba(key, [image.width(), image.height()], image.into_raw())
    }

    pub fn remove(&mut self, key: &str) -> Option<UiSurfaceImageData> {
        let canonical_key = canonical_builtin_icon_uri(key);
        let key = canonical_key.as_ref();
        let removed = self.images.remove(key)?;
        self.resident_bytes = self
            .resident_bytes
            .saturating_sub(removed.pixels.len() as u64);
        if self.project_keys.contains(key) {
            self.project_status
                .insert(key.to_string(), UiImageResourceStatus::Missing);
        }
        if let Ok(mut last_used) = self.last_used.lock() {
            last_used.remove(key);
        }
        Some(removed)
    }

    pub fn get(&self, key: &str) -> Option<&UiSurfaceImageData> {
        let canonical_key = canonical_builtin_icon_uri(key);
        let key = canonical_key.as_ref();
        if let Some(image) = self.images.get(key) {
            self.touch(key);
            return Some(image);
        }
        if self.project_keys.contains(key) {
            self.touch(key);
        }
        if self.project_keys.contains(key)
            && self.project_status.get(key) == Some(&UiImageResourceStatus::Missing)
        {
            if let Some(loader) = &self.project_loader {
                loader.request_project_image(key);
            }
        }
        None
    }

    /// Returns the runtime state for an image key. The document itself keeps
    /// only its portable key; status and decoded pixels remain host-owned.
    pub fn resource_status(&self, key: &str) -> UiImageResourceStatus {
        let canonical_key = canonical_builtin_icon_uri(key);
        let key = canonical_key.as_ref();
        if self.images.contains_key(key) {
            return UiImageResourceStatus::Ready;
        }
        let status = self
            .project_status
            .get(key)
            .copied()
            .unwrap_or(UiImageResourceStatus::Missing);
        if status == UiImageResourceStatus::Missing
            && self
                .project_loader
                .as_ref()
                .is_some_and(|loader| loader.is_pending(key))
        {
            UiImageResourceStatus::Pending
        } else {
            status
        }
    }

    /// Registers project-relative image references from an authored UI tree.
    /// Supported source keys include `project://...`, `assets://...`, and
    /// relative file-like keys; other URI namespaces remain host-owned.
    pub fn register_project_images(&mut self, project_root: &Path, root: &UiNode) {
        self.register_project_images_with_wakeup(project_root, root, None);
    }

    /// Registers project images and optionally wakes the native presentation
    /// once a bounded decode completion is available.
    pub fn register_project_images_with_wakeup(
        &mut self,
        project_root: &Path,
        root: &UiNode,
        wakeup: Option<Arc<dyn Fn() + Send + Sync>>,
    ) {
        self.cancel_project_images();
        for key in std::mem::take(&mut self.project_keys) {
            self.remove(&key);
        }
        self.project_status.clear();

        let mut keys = BTreeSet::new();
        collect_project_image_keys(root, &mut keys);
        if keys.is_empty() {
            return;
        }

        let mut accepted = Vec::new();
        for key in keys {
            if image_resource_loader::project_image_relative_path(&key).is_none() {
                continue;
            }
            if accepted.len() >= image_resource_loader::MAX_PROJECT_IMAGE_KEYS {
                self.project_status
                    .insert(key, UiImageResourceStatus::Error);
                continue;
            }
            self.project_keys.insert(key.clone());
            self.project_status
                .insert(key.clone(), UiImageResourceStatus::Pending);
            accepted.push(key);
        }
        if accepted.is_empty() {
            return;
        }

        let wakeup = wakeup.map(|wakeup| {
            let current_epoch = Arc::clone(&self.project_epoch);
            let registered_epoch = current_epoch.load(Ordering::Acquire);
            Arc::new(move || {
                if current_epoch.load(Ordering::Acquire) == registered_epoch {
                    wakeup();
                }
            }) as Arc<dyn Fn() + Send + Sync>
        });

        match ProjectImageResourceLoader::new(
            project_root,
            accepted.clone(),
            self.budget_bytes,
            wakeup,
        ) {
            Ok(mut loader) => {
                // Start bounded work immediately so its first completion can
                // request the one redraw needed by a paused presentation.
                loader.dispatch_queued();
                self.project_loader = Some(loader);
            }
            Err(error) => {
                for key in accepted {
                    self.project_status
                        .insert(key, UiImageResourceStatus::Error);
                }
                tracing::warn!("Unable to start project UI image loader: {error}");
            }
        }
    }

    /// Drains a bounded number of finished decodes and dispatches queued work.
    /// An optional worker wakeup requests a single redraw after completion;
    /// it does not create a polling loop or raise the idle frame rate.
    pub fn pump_project_images(&mut self, max_completions: usize) -> bool {
        let started = std::time::Instant::now();
        let mut admitted = 0_usize;
        let mut admitted_bytes = 0_u64;
        let mut allowance = raf_ui::UiWorkAllowance::new(self.work_limits);
        let mut changed = false;
        for _ in 0..max_completions {
            if !allowance.can_poll() {
                break;
            }
            let completion = self
                .deferred_completion
                .take()
                .or_else(|| self.project_loader.as_mut()?.try_completion());
            let Some(completion) = completion else {
                break;
            };
            let bytes = completion
                .result
                .as_ref()
                .map_or(0, |image| image.rgba8.len() as u64);
            if !allowance.admit(bytes) {
                self.deferred_completion = Some(completion);
                break;
            }
            self.apply_project_completion(completion);
            admitted += 1;
            admitted_bytes = admitted_bytes.saturating_add(bytes);
            changed = true;
        }
        if let Some(loader) = self.project_loader.as_mut() {
            if max_completions > 0
                && self.deferred_completion.is_none()
                && (admitted == max_completions || !allowance.can_poll())
            {
                self.deferred_completion = loader.try_completion();
            }
            loader.dispatch_queued();
        }
        if changed || self.has_deferred_images() {
            super::cost_observation::observe(
                &mut self.performance,
                "resources.project_images",
                raf_ui::UiWorkPhase::ResourceAdmission,
                raf_ui::UiWorkCause::Resources,
                started.elapsed(),
                admitted,
                admitted_bytes,
                false,
            );
        }
        changed
    }

    pub fn set_work_limits(&mut self, limits: raf_ui::UiWorkLimits) {
        self.work_limits = limits;
        self.performance.set_limits(limits);
    }
    pub fn has_deferred_images(&self) -> bool {
        self.deferred_completion.is_some()
    }
    pub fn performance_snapshot(&self) -> raf_ui::UiPerformanceSnapshot {
        self.performance.snapshot()
    }
    pub fn clear_performance(&mut self) {
        self.performance.clear();
    }

    /// Stops project decodes and drops queued work. Safe to call repeatedly.
    pub fn cancel_project_images(&mut self) {
        self.deferred_completion = None;
        self.project_epoch.fetch_add(1, Ordering::AcqRel);
        self.project_loader.take();
        for status in self.project_status.values_mut() {
            if *status == UiImageResourceStatus::Pending {
                *status = UiImageResourceStatus::Missing;
            }
        }
    }

    pub fn len(&self) -> usize {
        self.images.len()
    }

    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }

    pub fn budget_bytes(&self) -> u64 {
        self.budget_bytes
    }

    pub fn resident_bytes(&self) -> u64 {
        self.resident_bytes
    }

    pub fn evictions(&self) -> u64 {
        self.evictions
    }

    pub fn set_budget_bytes(&mut self, budget_bytes: u64) {
        let increased = budget_bytes > self.budget_bytes;
        self.budget_bytes = budget_bytes;
        if let Some(loader) = &self.project_loader {
            loader.set_max_image_bytes(budget_bytes);
        }
        if increased {
            for key in &self.project_keys {
                if self.project_status.get(key) == Some(&UiImageResourceStatus::Error) {
                    self.project_status
                        .insert(key.clone(), UiImageResourceStatus::Missing);
                }
            }
        }
        while self.resident_bytes > self.budget_bytes {
            if self.evict_oldest().is_none() {
                break;
            }
        }
    }

    fn evict_oldest(&mut self) -> Option<String> {
        self.evict_oldest_except(None)
    }

    fn evict_oldest_except(&mut self, protect_key: Option<&str>) -> Option<String> {
        let key = {
            let last_used = self.last_used.lock().ok();
            self.images
                .iter()
                .filter(|(key, _)| protect_key.is_none_or(|protected| key.as_str() != protected))
                .min_by_key(|(key, image)| {
                    last_used
                        .as_ref()
                        .and_then(|last_used| last_used.get(*key))
                        .copied()
                        .unwrap_or(image.revision)
                })
                .map(|(key, _)| key.clone())?
        };
        let removed = self.images.remove(&key)?;
        self.resident_bytes = self
            .resident_bytes
            .saturating_sub(removed.pixels.len() as u64);
        self.evictions = self.evictions.saturating_add(1);
        if self.project_keys.contains(&key) {
            self.project_status
                .insert(key.clone(), UiImageResourceStatus::Missing);
        }
        if let Ok(mut last_used) = self.last_used.lock() {
            last_used.remove(&key);
        }
        Some(key)
    }

    fn touch(&self, key: &str) {
        let sequence = self
            .use_clock
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        if let Ok(mut last_used) = self.last_used.lock() {
            last_used.insert(key.to_string(), sequence);
        }
    }

    fn apply_project_completion(&mut self, completion: ProjectImageCompletion) {
        match completion.result {
            Ok(image) => {
                let result = self.insert_rgba(
                    completion.key.clone(),
                    [image.width, image.height],
                    image.rgba8,
                );
                if result.is_err() {
                    self.project_status
                        .insert(completion.key, UiImageResourceStatus::Error);
                }
            }
            Err(image_resource_loader::ProjectImageFailure::Missing) => {
                self.project_status
                    .insert(completion.key, UiImageResourceStatus::Missing);
            }
            Err(image_resource_loader::ProjectImageFailure::Error) => {
                self.project_status
                    .insert(completion.key, UiImageResourceStatus::Error);
            }
        }
    }

    /// Ensures that a semantic RafUI icon has a renderer-owned source. The
    /// generated PNG is the runtime representation of the registered master.
    /// Compile-time inclusion guarantees catalog coverage. Asset tests check
    /// decoding; a corrupt embedded image uses one generic diagnostic marker.
    pub fn ensure_builtin_icon(&mut self, icon: UiIcon) -> String {
        let key = builtin_icon_key(icon.id);
        if !self.images.contains_key(&key) {
            let decoded = builtin_icon_png(icon.id)
                .and_then(|bytes| image::load_from_memory(bytes).ok())
                .map(|image| {
                    let image = image.to_rgba8();
                    ([image.width(), image.height()], image.into_raw())
                });
            let pixels = match decoded {
                Some((size, pixels)) if pixels.chunks_exact(4).any(|pixel| pixel[3] > 0) => {
                    (size, pixels)
                }
                _ => {
                    tracing::error!(icon = icon.id.key(), "Invalid embedded RafUI icon");
                    (
                        [2, 2],
                        vec![
                            203, 73, 73, 255, 30, 30, 30, 255, 30, 30, 30, 255, 203, 73, 73, 255,
                        ],
                    )
                }
            };
            let _ = self.insert_rgba(key.clone(), pixels.0, pixels.1);
        }
        key
    }

    /// Resolves generated UI sources without making product surfaces understand
    /// how renderer-owned pixels are rasterized.
    pub fn ensure_builtin_key(&mut self, key: &str) {
        if let Some(hue) = key
            .strip_prefix(COLOR_PICKER_SOURCE_PREFIX)
            .and_then(|value| value.parse::<f32>().ok())
            .filter(|hue| hue.is_finite())
        {
            if !self.images.contains_key(key) {
                let _ = self.insert_rgba(
                    key.to_string(),
                    [
                        COLOR_PICKER_IMAGE_SIZE as u32,
                        COLOR_PICKER_IMAGE_SIZE as u32,
                    ],
                    color_picker_pixels(hue),
                );
            }
            return;
        }
        let Some(id) = key.strip_prefix("builtin://icon/") else {
            return;
        };
        let Some(id) = icon_id_from_key(id) else {
            return;
        };
        let _ = self.ensure_builtin_icon(UiIcon::new(id));
    }
}

fn collect_project_image_keys(node: &UiNode, keys: &mut BTreeSet<String>) {
    if keys.len() > image_resource_loader::MAX_PROJECT_IMAGE_KEYS {
        return;
    }
    if let UiControl::Image(image) = &node.control {
        let key = image.source.key.clone();
        if image_resource_loader::project_image_relative_path(&key).is_some() {
            keys.insert(key);
        }
    }
    for child in &node.children {
        if keys.len() > image_resource_loader::MAX_PROJECT_IMAGE_KEYS {
            break;
        }
        collect_project_image_keys(child, keys);
    }
}

/// Generates the static part of the HSV editor used by the Inspector.
///
/// The image is keyed by hue, so a surface rebuild only uploads a new texture
/// when the hue changes. Saturation/value remain interactive retained ranges
/// layered above this image; the GPU and CPU hosts therefore share the same
/// pixels without embedding editor-specific painting in either backend.
fn color_picker_pixels(hue: f32) -> Vec<u8> {
    const SQUARE_START: usize = UiColorPicker::SQUARE_START as usize;
    const SQUARE_SIDE: usize = UiColorPicker::SQUARE_SIDE as usize;
    let mut pixels = vec![0_u8; COLOR_PICKER_IMAGE_SIZE * COLOR_PICKER_IMAGE_SIZE * 4];

    for y in 0..COLOR_PICKER_IMAGE_SIZE {
        for x in 0..COLOR_PICKER_IMAGE_SIZE {
            let dx = x as f32 + 0.5 - COLOR_PICKER_IMAGE_SIZE as f32 * 0.5;
            let dy = y as f32 + 0.5 - COLOR_PICKER_IMAGE_SIZE as f32 * 0.5;
            let distance = (dx * dx + dy * dy).sqrt();
            let pixel = &mut pixels[(y * COLOR_PICKER_IMAGE_SIZE + x) * 4..][..4];

            if (UiColorPicker::RING_INNER_RADIUS..=UiColorPicker::RING_OUTER_RADIUS)
                .contains(&distance)
            {
                let ring_hue = (dy.atan2(dx).to_degrees() + 90.0).rem_euclid(360.0);
                let rgb = UiColorPicker::hsv_to_rgb_bytes(ring_hue, 1.0, 1.0);
                let edge = (distance - UiColorPicker::RING_INNER_RADIUS)
                    .min(UiColorPicker::RING_OUTER_RADIUS - distance);
                let alpha = (edge.clamp(0.0, 1.0) * 255.0).round() as u8;
                pixel.copy_from_slice(&[rgb[0], rgb[1], rgb[2], alpha]);
            } else if (SQUARE_START..SQUARE_START + SQUARE_SIDE).contains(&x)
                && (SQUARE_START..SQUARE_START + SQUARE_SIDE).contains(&y)
            {
                let saturation = (x - SQUARE_START) as f32 / (SQUARE_SIDE - 1) as f32;
                let value = 1.0 - (y - SQUARE_START) as f32 / (SQUARE_SIDE - 1) as f32;
                let rgb = UiColorPicker::hsv_to_rgb_bytes(hue, saturation, value);
                pixel.copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
            }
        }
    }

    pixels
}

pub fn builtin_icon_key(id: UiIconId) -> String {
    format!("builtin://icon/{}", id.key())
}

pub(super) fn canonical_builtin_icon_uri(key: &str) -> Cow<'_, str> {
    let Some(icon_key) = key.strip_prefix("builtin://icon/") else {
        return Cow::Borrowed(key);
    };
    let Some(id) = UiIconId::from_key(icon_key) else {
        return Cow::Borrowed(key);
    };
    if id.key() == icon_key {
        Cow::Borrowed(key)
    } else {
        Cow::Owned(builtin_icon_key(id))
    }
}

macro_rules! declare_builtin_icon_sources {
    ($($variant:ident => $key:literal,)*) => {
        fn builtin_icon_png(id: UiIconId) -> Option<&'static [u8]> {
            Some(match id { $(UiIconId::$variant => include_bytes!(concat!("../../../assets/ui_icons/png/", $key, ".png")).as_slice(),)* })
        }
    };
}
raf_ui::ui_icon_catalog!(declare_builtin_icon_sources);

fn icon_id_from_key(key: &str) -> Option<UiIconId> {
    UiIconId::from_key(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_cost_guard_image_admission_makes_bounded_progress_and_records_bytes() {
        let mut store = UiSurfaceImageStore::default();
        store.set_work_limits(raf_ui::UiWorkLimits {
            optional_bytes: 4,
            ..Default::default()
        });
        store.deferred_completion = Some(ProjectImageCompletion {
            key: "project://test.png".into(),
            result: Ok(raf_assets::DecodedImage {
                width: 2,
                height: 2,
                rgba8: vec![255; 16],
            }),
        });
        assert!(store.pump_project_images(4));
        assert!(!store.has_deferred_images());
        assert!(store.get("project://test.png").is_some());
        assert_eq!(
            store.performance_snapshot().samples.last().unwrap().bytes,
            16
        );
        assert!(!store.pump_project_images(4));
    }

    #[test]
    fn rgba_images_require_exact_pixel_storage() {
        let mut store = UiSurfaceImageStore::default();
        assert!(store.insert_rgba("icon", [2, 2], vec![255; 16]).is_ok());
        assert!(store.insert_rgba("bad", [2, 2], vec![255; 12]).is_err());
        assert_eq!(store.get("icon").unwrap().size, [2, 2]);
        assert_eq!(store.resident_bytes(), 16);
    }

    #[test]
    fn budget_rejects_oversized_single_image() {
        let mut store = UiSurfaceImageStore::default();
        store.set_budget_bytes(64);
        assert!(store.insert_rgba("big", [8, 8], vec![0; 256]).is_err());
        assert_eq!(store.len(), 0);
        assert_eq!(store.resident_bytes(), 0);
        assert_eq!(store.evictions(), 0);
    }

    #[test]
    fn budget_evicts_least_recently_used_image_to_admit_new_image() {
        let mut store = UiSurfaceImageStore::default();
        store.set_budget_bytes(32);
        assert!(store.insert_rgba("first", [2, 2], vec![1; 16]).is_ok());
        assert!(store.insert_rgba("second", [2, 2], vec![2; 16]).is_ok());
        assert!(store.get("first").is_some());
        assert!(store.insert_rgba("third", [2, 2], vec![3; 16]).is_ok());
        assert!(store.get("first").is_some(), "recently used image survives");
        assert!(
            store.get("second").is_none(),
            "least-recently-used image evicts"
        );
        assert!(store.get("third").is_some());
        assert_eq!(store.resident_bytes(), 32);
        assert_eq!(store.evictions(), 1);
    }

    #[test]
    fn replace_updates_resident_bytes_without_double_counting() {
        let mut store = UiSurfaceImageStore::default();
        assert!(store.insert_rgba("icon", [2, 2], vec![1; 16]).is_ok());
        assert!(store.insert_rgba("icon", [4, 4], vec![2; 64]).is_ok());
        assert_eq!(store.len(), 1);
        assert_eq!(store.resident_bytes(), 64);
        assert_eq!(store.evictions(), 0);
        assert!(store.remove("icon").is_some());
        assert_eq!(store.resident_bytes(), 0);
    }

    #[test]
    fn lowering_budget_evicts_until_resident_fits() {
        let mut store = UiSurfaceImageStore::default();
        assert!(store.insert_rgba("a", [2, 2], vec![1; 16]).is_ok());
        assert!(store.insert_rgba("b", [2, 2], vec![2; 16]).is_ok());
        assert!(store.insert_rgba("c", [2, 2], vec![3; 16]).is_ok());
        store.set_budget_bytes(32);
        assert!(store.resident_bytes() <= 32);
        assert!(store.evictions() >= 1);
        assert!(store.get("c").is_some(), "newest revision must survive");
    }

    #[test]
    fn semantic_icons_are_generated_once_at_high_density() {
        let mut store = UiSurfaceImageStore::default();
        let key = store.ensure_builtin_icon(UiIcon::new(UiIconId::Undo));
        let image = store.get(&key).expect("built-in icon image");

        assert_eq!(image.size, [64, 64]);
        assert!(image.pixels.chunks_exact(4).any(|pixel| pixel[3] > 0));
        let revision = image.revision;
        store.ensure_builtin_key(&key);
        assert_eq!(store.get(&key).unwrap().revision, revision);
    }

    #[test]
    fn builtin_icon_alias_uris_resolve_to_single_canonical_residency() {
        let mut store = UiSurfaceImageStore::default();
        let canonical_key = store.ensure_builtin_icon(UiIcon::new(UiIconId::Undo));
        let alias_key = "builtin://icon/Undo";
        let revision = store.get(&canonical_key).unwrap().revision;

        store.ensure_builtin_key(alias_key);

        assert_eq!(store.len(), 1);
        assert_eq!(store.resident_bytes(), 64 * 64 * 4);
        assert_eq!(store.get(alias_key).unwrap().revision, revision);
        assert_eq!(
            store.resource_status(alias_key),
            UiImageResourceStatus::Ready
        );
        assert!(store.remove(alias_key).is_some());
        assert!(store.get(&canonical_key).is_none());
    }

    #[test]
    fn color_picker_asset_contains_hue_ring_and_hsv_square() {
        let mut store = UiSurfaceImageStore::default();
        let key = "builtin://color-picker/hsv/0";
        store.ensure_builtin_key(key);
        let image = store.get(key).expect("color picker image");

        assert_eq!(image.size, [256, 256]);
        let pixel = |x: usize, y: usize| {
            let index = (y * 256 + x) * 4;
            &image.pixels[index..index + 4]
        };
        assert!(pixel(128, 20)[3] > 0, "hue ring should be visible");
        assert!(pixel(63, 63)[3] > 0, "HSV square should be visible");
        assert_eq!(pixel(128, 20)[0], 255, "zero hue should start at red");
    }

    #[test]
    fn every_semantic_icon_has_a_generated_runtime_asset() {
        for icon in UiIconId::ALL {
            if let Some(png) = builtin_icon_png(icon) {
                assert!(!png.is_empty(), "{} PNG must not be empty", icon.key());
            } else {
                assert!(
                    matches!(
                        icon,
                        UiIconId::Camera
                            | UiIconId::Pause
                            | UiIconId::StepForward
                            | UiIconId::ScenePreview
                    ),
                    "{} must ship a PNG master or an explicit procedural icon",
                    icon.key()
                );
            }
            assert_eq!(
                icon_id_from_key(icon.key()),
                Some(icon),
                "icon key mapping must be reversible"
            );

            let mut store = UiSurfaceImageStore::default();
            let key = store.ensure_builtin_icon(UiIcon::new(icon));
            let image = store.get(&key).expect("generated icon asset");
            assert_eq!(image.size, [64, 64], "{}", icon.key());
            assert!(
                image.pixels.chunks_exact(4).any(|pixel| pixel[3] > 0),
                "{} must not be transparent",
                icon.key()
            );
        }
    }
}
