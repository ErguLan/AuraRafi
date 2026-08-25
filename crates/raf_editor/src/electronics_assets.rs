//! Background asset atlas for the native Electronics surfaces.
//!
//! The old atlas stored widget texture handles. Native AGB surfaces consume
//! immutable RGBA uploads instead, so this module keeps the useful part—the
//! worker, de-duplication and failure tracking—without owning a UI toolkit.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender};

use image::RgbaImage;

const FALLBACK_ASSET: &str = "symbols/generic.png";

#[derive(Debug, Clone)]
pub struct ElectronicsAssetImage {
    pub name: String,
    pub size: [u32; 2],
    pub pixels: Vec<u8>,
}

enum WorkerResult {
    Ready(ElectronicsAssetImage),
    Failed { path: String, error: String },
}

pub struct ElectronicsAssetAtlas {
    images: HashMap<String, ElectronicsAssetImage>,
    queued: HashSet<String>,
    failed: HashSet<String>,
    request_tx: Sender<String>,
    result_rx: Receiver<WorkerResult>,
}

impl Default for ElectronicsAssetAtlas {
    fn default() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<String>();
        let (result_tx, result_rx) = mpsc::channel::<WorkerResult>();
        std::thread::Builder::new()
            .name("raf-electronics-assets".to_string())
            .spawn(move || {
                while let Ok(name) = request_rx.recv() {
                    let result = std::fs::read(&name)
                        .map_err(|error| format!("{name}: {error}"))
                        .and_then(|bytes| {
                            image::load_from_memory(&bytes)
                                .map_err(|error| format!("{name}: {error}"))
                        })
                        .map(|image| to_asset_image(name.clone(), image.to_rgba8()));
                    let _ = result_tx.send(match result {
                        Ok(image) => WorkerResult::Ready(image),
                        Err(error) => WorkerResult::Failed { path: name, error },
                    });
                }
            })
            .expect("electronics asset worker must start");
        Self {
            images: HashMap::new(),
            queued: HashSet::new(),
            failed: HashSet::new(),
            request_tx,
            result_rx,
        }
    }
}

impl ElectronicsAssetAtlas {
    pub fn request(&mut self, path: impl Into<String>) {
        let path = path.into();
        if self.images.contains_key(&path) || !self.queued.insert(path.clone()) {
            return;
        }
        if self.request_tx.send(path).is_err() {
            self.queued.clear();
        }
    }

    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        while let Ok(result) = self.result_rx.try_recv() {
            match result {
                WorkerResult::Ready(image) => {
                    self.queued.remove(&image.name);
                    self.images.insert(image.name.clone(), image);
                    changed = true;
                }
                WorkerResult::Failed {
                    path,
                    error: _error,
                } => {
                    self.queued.remove(&path);
                    self.failed.insert(path);
                    changed = true;
                }
            }
        }
        changed
    }

    pub fn image(&self, path: &str) -> Option<&ElectronicsAssetImage> {
        self.images.get(path)
    }

    pub fn fallback_name() -> &'static str {
        FALLBACK_ASSET
    }

    pub fn failed(&self, path: &str) -> bool {
        self.failed.contains(path)
    }

    pub fn len(&self) -> usize {
        self.images.len()
    }
}

fn to_asset_image(name: String, image: RgbaImage) -> ElectronicsAssetImage {
    ElectronicsAssetImage {
        name,
        size: [image.width(), image.height()],
        pixels: image.into_raw(),
    }
}
