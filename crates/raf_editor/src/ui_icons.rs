use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui::{self, Color32, Rect, TextureHandle};
use image::RgbaImage;

const FALLBACK_ICON: &str = "NoImage.png";

struct PendingIconUpload {
    icon_name: &'static str,
    size: [usize; 2],
    pixels: Vec<u8>,
}

enum IconWorkerResult {
    Ready(PendingIconUpload),
    Failed(&'static str),
}

pub struct UiIconAtlas {
    textures: HashMap<&'static str, TextureHandle>,
    ready_uploads: VecDeque<PendingIconUpload>,
    queued: HashSet<&'static str>,
    failed: HashSet<&'static str>,
    request_tx: Sender<&'static str>,
    result_rx: Receiver<IconWorkerResult>,
}

impl Default for UiIconAtlas {
    fn default() -> Self {
        let (request_tx, request_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        spawn_icon_loader(request_rx, result_tx);

        Self {
            textures: HashMap::new(),
            ready_uploads: VecDeque::new(),
            queued: HashSet::new(),
            failed: HashSet::new(),
            request_tx,
            result_rx,
        }
    }
}

impl UiIconAtlas {
    pub fn request_icons(&mut self, icon_names: &[&'static str]) {
        self.queue_icon(FALLBACK_ICON);
        for icon_name in icon_names {
            self.queue_icon(*icon_name);
        }
    }

    pub fn process_load_budget(&mut self, ctx: &egui::Context, budget: usize) {
        self.drain_worker_results();

        for _ in 0..budget.max(1) {
            let Some(icon) = self.ready_uploads.pop_front() else {
                break;
            };
            self.upload_icon(ctx, icon);
        }

        if !self.ready_uploads.is_empty() || !self.queued.is_empty() {
            ctx.request_repaint();
        }
    }

    pub fn get(&self, icon_name: &'static str) -> Option<&TextureHandle> {
        self.textures
            .get(icon_name)
            .or_else(|| self.textures.get(FALLBACK_ICON))
    }

    pub fn paint(&self, painter: &egui::Painter, icon_name: &'static str, rect: Rect, tint: Color32) -> bool {
        let Some(texture) = self.get(icon_name) else {
            return false;
        };

        let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        if rect.width() <= 20.0 && rect.height() <= 20.0 {
            let soft_tint = Color32::from_rgba_premultiplied(
                tint.r(),
                tint.g(),
                tint.b(),
                ((tint.a() as f32) * 0.18) as u8,
            );
            painter.image(texture.id(), rect.expand(0.9), uv, soft_tint);
        }

        painter.image(
            texture.id(),
            rect,
            uv,
            tint,
        );
        true
    }

    fn queue_icon(&mut self, icon_name: &'static str) {
        if self.textures.contains_key(icon_name)
            || self.queued.contains(icon_name)
            || self.failed.contains(icon_name)
        {
            return;
        }
        self.queued.insert(icon_name);
        if self.request_tx.send(icon_name).is_err() {
            self.queued.remove(icon_name);
            self.failed.insert(icon_name);
        }
    }

    fn drain_worker_results(&mut self) {
        while let Ok(result) = self.result_rx.try_recv() {
            match result {
                IconWorkerResult::Ready(icon) => {
                    self.queued.remove(icon.icon_name);
                    self.ready_uploads.push_back(icon);
                }
                IconWorkerResult::Failed(icon_name) => {
                    self.queued.remove(icon_name);
                    self.failed.insert(icon_name);
                }
            }
        }
    }

    fn upload_icon(&mut self, ctx: &egui::Context, icon: PendingIconUpload) {
        let color_image = egui::ColorImage::from_rgba_unmultiplied(icon.size, &icon.pixels);
        let texture = ctx.load_texture(
            format!("ui-icon-{}", icon.icon_name),
            color_image,
            egui::TextureOptions::LINEAR,
        );
        self.textures.insert(icon.icon_name, texture);
    }
}

fn spawn_icon_loader(request_rx: Receiver<&'static str>, result_tx: Sender<IconWorkerResult>) {
    let icon_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../editor/assets/ui_icons");

    let _ = std::thread::Builder::new()
        .name("raf-ui-icon-loader".to_string())
        .spawn(move || {
            while let Ok(icon_name) = request_rx.recv() {
                let result = load_icon_pixels(&icon_root, icon_name)
                    .map(IconWorkerResult::Ready)
                    .unwrap_or(IconWorkerResult::Failed(icon_name));
                let _ = result_tx.send(result);
            }
        });
}

fn load_icon_pixels(icon_root: &std::path::Path, icon_name: &'static str) -> Option<PendingIconUpload> {
    let icon_path = [
        icon_root.join("unos").join(icon_name),
        icon_root.join(icon_name),
    ]
    .into_iter()
    .find(|path| path.exists())?;

    let bytes = std::fs::read(&icon_path).ok()?;
    let image = image::load_from_memory(&bytes).ok()?;
    let rgba = prepare_icon_rgba(image.to_rgba8());

    Some(PendingIconUpload {
        icon_name,
        size: [rgba.width() as usize, rgba.height() as usize],
        pixels: rgba.into_vec(),
    })
}

fn prepare_icon_rgba(rgba: RgbaImage) -> RgbaImage {
    let padded = pad_icon(rgba, 2);
    let width = padded.width();
    let height = padded.height();

    if width <= 40 && height <= 40 {
        image::imageops::resize(
            &padded,
            width * 2,
            height * 2,
            image::imageops::FilterType::CatmullRom,
        )
    } else {
        padded
    }
}

fn pad_icon(rgba: RgbaImage, padding: u32) -> RgbaImage {
    let width = rgba.width() + (padding * 2);
    let height = rgba.height() + (padding * 2);
    let mut padded = RgbaImage::from_pixel(width, height, image::Rgba([0, 0, 0, 0]));
    image::imageops::overlay(&mut padded, &rgba, padding as i64, padding as i64);
    padded
}
