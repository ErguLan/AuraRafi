//! Bounded, demand-driven thumbnails. Temporary scenes never touch the document/camera.
use raf_assets::{
    authoring_catalog::{AssetClass, AuthoringAsset},
    DecodedImage,
};
use raf_core::scene::{geometry::subtree_geometry_bounds, prefab::ScenePrefab, SceneGraph};
use raf_render::api_graphic_basic::device::{BasicDevice, BasicDeviceConfig};
use raf_render::api_graphic_basic::ui_surface::UiSurfaceImageStore;
use raf_render::{
    camera::{Camera, CameraMode},
    scene_renderer::{RenderOptions, SceneRenderer},
};
use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::mpsc,
    time::SystemTime,
};

pub const MAX_THUMBNAILS: usize = 256;
pub const MAX_THUMBNAIL_BYTES: usize = 16 * 1024 * 1024;
#[derive(Debug, Clone)]
pub enum PreviewState {
    Loading,
    Ready {
        key: String,
        dimensions: Option<[f32; 3]>,
    },
    /// The catalog row exists but its file is gone.
    Missing,
    Error(String),
    Unavailable(&'static str),
}
/// Cheap per-asset identity used to invalidate a single cache entry without
/// flushing every thumbnail when the catalog revision changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileStamp {
    modified: Option<SystemTime>,
    len: u64,
}
struct Entry {
    state: PreviewState,
    bytes: usize,
    stamp: Option<FileStamp>,
}
struct Reply {
    epoch: u64,
    path: String,
    stamp: Option<FileStamp>,
    result: Result<(DecodedImage, Option<[f32; 3]>), String>,
}
struct Job {
    epoch: u64,
    root: Option<PathBuf>,
    stamp: Option<FileStamp>,
    entry: AuthoringAsset,
}
pub struct AssetPreviews {
    entries: HashMap<String, Entry>,
    lru: VecDeque<String>,
    bytes: usize,
    epoch: u64,
    root: Option<PathBuf>,
    revision: u64,
    sender: Option<mpsc::SyncSender<Job>>,
    receiver: Option<mpsc::Receiver<Reply>>,
    pub wakeup: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authoring_thumbnail_is_real_agb_capture_and_cache_is_bounded() {
        let entry = AuthoringAsset::from_path("builtin://primitive/capsule");
        let mut renderer = SceneRenderer::new(128, 128);
        let mut device = BasicDevice::new(BasicDeviceConfig {
            allow_gpu: false,
            force_cpu: true,
            ..Default::default()
        });
        let (image, dimensions) = render_preview(
            &entry,
            Some(Path::new("unused")),
            &mut renderer,
            &mut device,
        )
        .unwrap();
        assert_eq!([image.width, image.height], [128, 128]);
        assert_eq!(dimensions, Some([1.0, 2.0, 1.0]));
        let first = &image.rgba8[..4];
        assert!(image.rgba8.chunks_exact(4).any(|pixel| pixel != first));
        if let Ok(directory) = std::env::var("RAF_AUTHORING_QA_DIR") {
            std::fs::create_dir_all(&directory).unwrap();
            image::save_buffer(
                Path::new(&directory).join("capsule-thumbnail.png"),
                &image.rgba8,
                128,
                128,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
        let mut previews = AssetPreviews::default();
        let mut images = UiSurfaceImageStore::default();
        for index in 0..300 {
            let path = index.to_string();
            let key = format!("test-thumbnail-{index}");
            let rgba = vec![index as u8; 128 * 128 * 4];
            images.insert_rgba(key.clone(), [128, 128], rgba).unwrap();
            previews.bytes += 128 * 128 * 4;
            previews.lru.push_back(path.clone());
            previews.entries.insert(
                path,
                Entry {
                    state: PreviewState::Ready {
                        key,
                        dimensions: None,
                    },
                    bytes: 128 * 128 * 4,
                    stamp: None,
                },
            );
        }
        previews.evict(&mut images);
        assert!(previews.entries.len() <= MAX_THUMBNAILS);
        assert!(previews.bytes <= MAX_THUMBNAIL_BYTES);
        assert_eq!(images.resident_bytes() as usize, previews.bytes);
        previews.sync(Some(Path::new("another-project")), 1, &mut images);
        assert!(previews.entries.is_empty());
        assert_eq!(images.resident_bytes(), 0);
        let missing = AuthoringAsset::from_path("missing-preview-test.png");
        let root = std::env::temp_dir().join(format!("raf-absent-preview-{}", std::process::id()));
        assert!(render_preview(&missing, Some(&root), &mut renderer, &mut device).is_err());
    }

    #[test]
    fn a_revision_only_invalidates_assets_whose_file_stamp_changed() {
        let root = std::env::temp_dir().join(format!(
            "raf-preview-invalidate-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|time| time.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(root.join("assets/keep.png"), [1u8; 8]).unwrap();
        std::fs::write(root.join("assets/edit.png"), [2u8; 8]).unwrap();

        let mut previews = AssetPreviews::default();
        let mut images = UiSurfaceImageStore::default();
        previews.sync(Some(&root), 1, &mut images);
        for (path, byte) in [("keep.png", 1u8), ("edit.png", 2u8)] {
            let key = format!("test-{path}");
            let stamp = previews.asset_stamp(path);
            images
                .insert_rgba(key.clone(), [1, 1], vec![byte; 4])
                .unwrap();
            previews.bytes += 4;
            previews.lru.push_back(path.to_string());
            previews.entries.insert(
                path.to_string(),
                Entry {
                    state: PreviewState::Ready {
                        key,
                        dimensions: None,
                    },
                    bytes: 4,
                    stamp,
                },
            );
        }

        // Same revision is a no-op; a later revision revalidates per asset.
        assert!(!previews.sync(Some(&root), 1, &mut images));
        std::fs::write(root.join("assets/edit.png"), [2u8; 32]).unwrap();
        assert!(previews.sync(Some(&root), 2, &mut images));

        assert!(previews.entries.contains_key("keep.png"));
        assert!(!previews.entries.contains_key("edit.png"));
        assert!(images.get("test-keep.png").is_some());
        assert!(images.get("test-edit.png").is_none());
        assert_eq!(previews.bytes, 4);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn models_are_missing_unavailable_and_then_injectable() {
        let root = std::env::temp_dir().join(format!(
            "raf-preview-model-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|time| time.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(root.join("assets/models")).unwrap();
        let mut previews = AssetPreviews::default();
        let mut images = UiSurfaceImageStore::default();
        previews.sync(Some(&root), 1, &mut images);
        let model = AuthoringAsset::from_path("models/chair.glb");

        assert!(matches!(
            previews.request(&model, &mut images),
            PreviewState::Missing
        ));

        std::fs::write(root.join("assets/models/chair.glb"), b"glTF").unwrap();
        let state = previews.request(&model, &mut images);
        assert!(
            matches!(state, PreviewState::Loading | PreviewState::Error(_)),
            "an existing model must enter the preview worker, got {state:?}"
        );

        previews
            .inject_model_preview(
                "models/chair.glb",
                DecodedImage {
                    width: 2,
                    height: 2,
                    rgba8: vec![255; 16],
                },
                Some([1.0, 2.0, 3.0]),
                &mut images,
            )
            .unwrap();
        match previews.request(&model, &mut images) {
            PreviewState::Ready { dimensions, .. } => assert_eq!(dimensions, Some([1.0, 2.0, 3.0])),
            other => panic!("expected an injected model preview, got {other:?}"),
        }
        std::fs::remove_dir_all(&root).ok();
    }
}
impl Default for AssetPreviews {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            lru: VecDeque::new(),
            bytes: 0,
            epoch: 0,
            root: None,
            revision: u64::MAX,
            sender: None,
            receiver: None,
            wakeup: None,
        }
    }
}
impl AssetPreviews {
    pub fn sync(
        &mut self,
        root: Option<&Path>,
        revision: u64,
        images: &mut UiSurfaceImageStore,
    ) -> bool {
        let mut changed = false;
        if self.root.as_deref() != root {
            // A different project means every stored key is meaningless.
            self.clear(images);
            self.epoch = self.epoch.wrapping_add(1);
            self.root = root.map(Path::to_path_buf);
            self.revision = revision;
            changed = true;
        } else if self.revision != revision {
            // Same project: revalidate each cached asset against its own
            // path+size+mtime instead of flushing the whole cache.
            self.revision = revision;
            let paths = self.entries.keys().cloned().collect::<Vec<_>>();
            for path in paths {
                let stamp = self.asset_stamp(&path);
                let stale = self
                    .entries
                    .get(&path)
                    .is_some_and(|entry| entry.stamp != stamp);
                if stale {
                    self.remove_entry(&path, images);
                    changed = true;
                }
            }
        }
        while let Some(reply) = self.receiver.as_ref().and_then(|rx| rx.try_recv().ok()) {
            if reply.epoch != self.epoch || !self.entries.contains_key(&reply.path) {
                continue;
            }
            // The file changed while the worker rendered it: drop the result
            // and let the next request enqueue a fresh job.
            if reply.stamp != self.asset_stamp(&reply.path) {
                self.remove_entry(&reply.path, images);
                changed = true;
                continue;
            }
            let mut bytes = 0;
            let state = match reply.result {
                Ok((image, dimensions)) => {
                    bytes = image.rgba8.len();
                    let key = format!("asset-preview:{}:{}", self.epoch, reply.path);
                    match images.insert_rgba(key.clone(), [image.width, image.height], image.rgba8)
                    {
                        Ok(()) => PreviewState::Ready { key, dimensions },
                        Err(error) => {
                            bytes = 0;
                            PreviewState::Error(error)
                        }
                    }
                }
                Err(error) if error == "preview_budget" => {
                    PreviewState::Unavailable("assets.preview_budget")
                }
                Err(error) => PreviewState::Error(error),
            };
            if let Some(previous) = self.entries.insert(
                reply.path,
                Entry {
                    state,
                    bytes,
                    stamp: reply.stamp,
                },
            ) {
                self.bytes = self.bytes.saturating_sub(previous.bytes);
            }
            self.bytes += bytes;
            changed = true;
            self.evict(images);
        }
        changed
    }

    /// Drops one cached asset. Hosts can call this after an import/reimport
    /// without invalidating unrelated thumbnails.
    #[allow(dead_code)] // wired by the import backend when it lands
    pub fn invalidate_asset(&mut self, path: &str, images: &mut UiSurfaceImageStore) -> bool {
        self.remove_entry(path, images)
    }

    /// Installs a model thumbnail produced by the future model importer.
    ///
    /// The Assets preview path never decodes models on its own yet, so model
    /// rows stay `Unavailable("assets.model_import_pending")` until the lead
    /// calls this method (or a worker equivalent) with real pixels.
    #[allow(dead_code)] // wired by the model importer when it lands
    pub fn inject_model_preview(
        &mut self,
        path: &str,
        image: DecodedImage,
        dimensions: Option<[f32; 3]>,
        images: &mut UiSurfaceImageStore,
    ) -> Result<(), String> {
        self.remove_entry(path, images);
        let bytes = image.rgba8.len();
        let key = format!("asset-preview:{}:{}", self.epoch, path);
        images.insert_rgba(key.clone(), [image.width, image.height], image.rgba8)?;
        self.entries.insert(
            path.to_string(),
            Entry {
                state: PreviewState::Ready { key, dimensions },
                bytes,
                stamp: self.asset_stamp(path),
            },
        );
        self.lru.retain(|cached| cached != path);
        self.lru.push_back(path.to_string());
        self.bytes += bytes;
        self.evict(images);
        Ok(())
    }

    fn clear(&mut self, images: &mut UiSurfaceImageStore) {
        for entry in self.entries.values() {
            if let PreviewState::Ready { key, .. } = &entry.state {
                images.remove(key);
            }
        }
        self.entries.clear();
        self.lru.clear();
        self.bytes = 0;
    }

    /// Identity of the file backing a row. Built-in rows have no file and
    /// return `None`; same for a row whose file disappeared.
    fn asset_stamp(&self, path: &str) -> Option<FileStamp> {
        if path.starts_with("builtin://") {
            return None;
        }
        let metadata = std::fs::metadata(self.root.as_deref()?.join("assets").join(path)).ok()?;
        Some(FileStamp {
            modified: metadata.modified().ok(),
            len: metadata.len(),
        })
    }

    fn remove_entry(&mut self, path: &str, images: &mut UiSurfaceImageStore) -> bool {
        self.lru.retain(|cached| cached != path);
        let Some(entry) = self.entries.remove(path) else {
            return false;
        };
        self.bytes = self.bytes.saturating_sub(entry.bytes);
        if let PreviewState::Ready { key, .. } = entry.state {
            images.remove(&key);
        }
        true
    }

    fn evict(&mut self, images: &mut UiSurfaceImageStore) {
        while self.entries.len() > MAX_THUMBNAILS || self.bytes > MAX_THUMBNAIL_BYTES {
            let Some(path) = self.lru.pop_front() else {
                break;
            };
            if let Some(entry) = self.entries.remove(&path) {
                self.bytes = self.bytes.saturating_sub(entry.bytes);
                if let PreviewState::Ready { key, .. } = entry.state {
                    images.remove(&key);
                }
            }
        }
    }
    fn start_worker(&mut self) {
        if self.sender.is_some() {
            return;
        }
        let (tx, rx) = mpsc::sync_channel::<Job>(16);
        let (result_tx, result_rx) = mpsc::sync_channel(16);
        let wakeup = self.wakeup.clone();
        std::thread::spawn(move || {
            let mut renderer = SceneRenderer::new(128, 128);
            let mut device = BasicDevice::new(BasicDeviceConfig {
                allow_gpu: false,
                force_cpu: true,
                ..Default::default()
            });
            while let Ok(job) = rx.recv() {
                let result =
                    render_preview(&job.entry, job.root.as_deref(), &mut renderer, &mut device);
                if result_tx
                    .send(Reply {
                        epoch: job.epoch,
                        path: job.entry.path,
                        stamp: job.stamp,
                        result,
                    })
                    .is_err()
                {
                    break;
                }
                if let Some(wakeup) = &wakeup {
                    wakeup();
                }
            }
        });
        self.sender = Some(tx);
        self.receiver = Some(result_rx);
    }
    pub fn request(
        &mut self,
        entry: &AuthoringAsset,
        images: &mut UiSurfaceImageStore,
    ) -> PreviewState {
        let stamp = self.asset_stamp(&entry.path);
        if let Some(cached) = self.entries.get(&entry.path) {
            let image_missing = matches!(
                &cached.state,
                PreviewState::Ready { key, .. } if images.get(key).is_none()
            );
            if cached.stamp == stamp && !image_missing {
                self.lru.retain(|path| path != &entry.path);
                self.lru.push_back(entry.path.clone());
                return cached.state.clone();
            }
        }
        self.remove_entry(&entry.path, images);
        self.lru.retain(|path| path != &entry.path);
        self.lru.push_back(entry.path.clone());

        let expects_file = matches!(
            entry.class,
            AssetClass::Image | AssetClass::Prefab | AssetClass::Model | AssetClass::Particle
        );
        let missing = expects_file
            && self.root.is_some()
            && !entry.path.starts_with("builtin://")
            && stamp.is_none();
        let state = if missing {
            PreviewState::Missing
        } else {
            match entry.class {
                AssetClass::Shape(_)
                | AssetClass::Image
                | AssetClass::Prefab
                | AssetClass::Model => {
                    self.start_worker();
                    let job = Job {
                        epoch: self.epoch,
                        root: self.root.clone(),
                        stamp,
                        entry: entry.clone(),
                    };
                    if self.sender.as_ref().unwrap().try_send(job).is_err() {
                        self.lru.retain(|path| path != &entry.path);
                        return PreviewState::Loading;
                    }
                    PreviewState::Loading
                }
                _ => PreviewState::Unavailable("assets.preview_unavailable"),
            }
        };
        self.entries.insert(
            entry.path.clone(),
            Entry {
                state: state.clone(),
                bytes: 0,
                stamp,
            },
        );
        self.evict(images);
        state
    }
}

fn render_preview(
    entry: &AuthoringAsset,
    root: Option<&Path>,
    renderer: &mut SceneRenderer,
    device: &mut BasicDevice,
) -> Result<(DecodedImage, Option<[f32; 3]>), String> {
    let asset_root = root.ok_or("Missing project")?.join("assets");
    let path = asset_root.join(&entry.path);
    if entry.class == AssetClass::Image {
        return raf_assets::decode_image(&path, 128).map(|image| (image, None));
    }
    let mut scene = SceneGraph::new();
    let id = match entry.class {
        AssetClass::Shape(primitive) => scene.add_root_with_primitive("Preview", primitive),
        AssetClass::Prefab => {
            if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > 2 * 1024 * 1024 {
                return Err("preview_budget".into());
            }
            let prefab: ScenePrefab =
                ron::from_str(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            if prefab.nodes.len() > 128 {
                return Err("preview_budget".into());
            }
            prefab.instantiate(&mut scene, None)?
        }
        AssetClass::Model => {
            let mut reference = raf_core::scene::MeshAssetRef::new(entry.path.clone());
            if let Some(metadata) =
                raf_assets::importer::ModelImportMetadata::read_for_asset_file(&path)
            {
                reference.bounds = Some([
                    metadata.bounds.min[0],
                    metadata.bounds.min[1],
                    metadata.bounds.min[2],
                    metadata.bounds.max[0],
                    metadata.bounds.max[1],
                    metadata.bounds.max[2],
                ]);
                reference.content_hash = Some(metadata.content_hash);
                reference.import_scale = metadata.import_scale;
            } else {
                let model = raf_assets::model_asset::load_static_model(&path)?;
                reference.bounds = Some([
                    model.bounds.min[0],
                    model.bounds.min[1],
                    model.bounds.min[2],
                    model.bounds.max[0],
                    model.bounds.max[1],
                    model.bounds.max[2],
                ]);
            }
            let id = scene.add_root("Preview");
            if let Some(node) = scene.get_mut(id) {
                node.mesh_asset = Some(reference);
            }
            id
        }
        _ => return Err("No geometry".into()),
    };
    let (min, max) = subtree_geometry_bounds(&scene, id).ok_or("No renderable geometry")?;
    let size = max - min;
    let center = (min + max) * 0.5;
    let radius = size.length().max(0.5);
    let camera = Camera {
        position: center + glam::Vec3::new(1.0, 0.75, 1.3).normalize() * radius * 2.0,
        target: center,
        mode: CameraMode::Orthographic,
        ortho_scale: radius * 1.3,
        near: 0.01,
        far: radius * 8.0 + 10.0,
        ..Default::default()
    };
    renderer.set_asset_root(Some(asset_root));
    let frame = renderer.build_frame(
        &scene,
        &camera,
        128.0,
        128.0,
        &[],
        [22, 27, 35, 255],
        raf_render::scene_renderer::SCENE_LIGHT_DIRECTION.normalize(),
        RenderOptions {
            show_grid_3d: false,
            selection_outline: false,
            ..Default::default()
        },
        None,
    );
    device.execute_scene_frame(&frame);
    let capture = device.capture_last_scene_rgba()?;
    Ok((
        DecodedImage {
            width: 128,
            height: 128,
            rgba8: capture.rgba8,
        },
        Some(size.to_array()),
    ))
}
