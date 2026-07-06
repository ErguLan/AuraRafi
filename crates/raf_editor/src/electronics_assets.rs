use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui::{self, Color32, Rect, TextureHandle};
use image::RgbaImage;

const FALLBACK_ASSET: &str = "symbols/generic.png";

struct PendingAssetUpload {
    name: &'static str,
    size: [usize; 2],
    pixels: Vec<u8>,
}

enum AssetWorkerResult {
    Ready(PendingAssetUpload),
    Failed(&'static str),
}

pub struct ElectronicsAssetAtlas {
    textures: HashMap<&'static str, TextureHandle>,
    queued: HashSet<&'static str>,
    failed: HashSet<&'static str>,
    request_tx: Sender<&'static str>,
    result_rx: Receiver<AssetWorkerResult>,
}

impl Default for ElectronicsAssetAtlas {
    fn default() -> Self {
        let (request_tx, request_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        spawn_asset_loader(request_rx, result_tx);

        Self {
            textures: HashMap::new(),
            queued: HashSet::new(),
            failed: HashSet::new(),
            request_tx,
            result_rx,
        }
    }
}

impl ElectronicsAssetAtlas {
    pub fn request_assets(&mut self, names: &[&'static str]) {
        self.queue(FALLBACK_ASSET);
        for name in names {
            self.queue(*name);
        }
    }

    pub fn process(&mut self, ctx: &egui::Context) {
        while let Ok(result) = self.result_rx.try_recv() {
            match result {
                AssetWorkerResult::Ready(asset) => {
                    self.queued.remove(asset.name);
                    let image = egui::ColorImage::from_rgba_unmultiplied(asset.size, &asset.pixels);
                    let texture = ctx.load_texture(
                        format!("electronics-asset-{}", asset.name),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.textures.insert(asset.name, texture);
                }
                AssetWorkerResult::Failed(name) => {
                    self.queued.remove(name);
                    self.failed.insert(name);
                }
            }
        }

        if !self.queued.is_empty() {
            ctx.request_repaint();
        }
    }

    pub fn paint(
        &self,
        painter: &egui::Painter,
        name: &'static str,
        rect: Rect,
        tint: Color32,
    ) -> bool {
        let Some(texture) = self
            .textures
            .get(name)
            .or_else(|| self.textures.get(FALLBACK_ASSET))
        else {
            return false;
        };
        let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        painter.image(texture.id(), rect, uv, tint);
        true
    }

    fn queue(&mut self, name: &'static str) {
        if self.textures.contains_key(name)
            || self.queued.contains(name)
            || self.failed.contains(name)
        {
            return;
        }
        self.queued.insert(name);
        if self.request_tx.send(name).is_err() {
            self.queued.remove(name);
            self.failed.insert(name);
        }
    }
}

fn spawn_asset_loader(request_rx: Receiver<&'static str>, result_tx: Sender<AssetWorkerResult>) {
    let asset_root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../editor/assets/electronics");

    let _ = std::thread::Builder::new()
        .name("raf-electronics-asset-loader".to_string())
        .spawn(move || {
            while let Ok(name) = request_rx.recv() {
                let result = load_asset_pixels(&asset_root, name)
                    .map(AssetWorkerResult::Ready)
                    .unwrap_or(AssetWorkerResult::Failed(name));
                let _ = result_tx.send(result);
            }
        });
}

fn load_asset_pixels(root: &std::path::Path, name: &'static str) -> Option<PendingAssetUpload> {
    let path = root.join(name);
    let bytes = std::fs::read(path).ok()?;
    let image = image::load_from_memory(&bytes).ok()?.to_rgba8();
    let rgba = prepare_asset_rgba(image);

    Some(PendingAssetUpload {
        name,
        size: [rgba.width() as usize, rgba.height() as usize],
        pixels: rgba.into_vec(),
    })
}

fn prepare_asset_rgba(rgba: RgbaImage) -> RgbaImage {
    if rgba.width() <= 128 && rgba.height() <= 128 {
        image::imageops::resize(
            &rgba,
            rgba.width() * 2,
            rgba.height() * 2,
            image::imageops::FilterType::CatmullRom,
        )
    } else {
        rgba
    }
}
