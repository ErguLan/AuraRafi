//! Resource cache for raster images referenced by retained UI documents.
//!
//! Documents store only `UiImageSource` keys. This cache owns decoded pixels
//! at the surface-host boundary, so a project can replace or unload icons
//! without modifying serialized layout data.
use std::collections::BTreeMap;
use std::path::Path;

use raf_ui::{UiColorPicker, UiIcon, UiIconId};

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
/// and evicts the least-recently inserted/updated entry when admission would
/// cross the budget. Oversized single images are rejected instead of
/// ballooning process memory.
#[derive(Debug, Clone)]
pub struct UiSurfaceImageStore {
    images: BTreeMap<String, UiSurfaceImageData>,
    next_revision: u64,
    budget_bytes: u64,
    resident_bytes: u64,
    evictions: u64,
}

impl Default for UiSurfaceImageStore {
    fn default() -> Self {
        Self {
            images: BTreeMap::new(),
            next_revision: 0,
            budget_bytes: DEFAULT_UI_IMAGE_BUDGET_BYTES,
            resident_bytes: 0,
            evictions: 0,
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
            key,
            UiSurfaceImageData {
                size: [width, height],
                pixels,
                revision: self.next_revision,
            },
        );
        Ok(())
    }

    pub fn load_png(&mut self, key: impl Into<String>, path: &Path) -> Result<(), String> {
        let image = image::open(path)
            .map_err(|error| format!("Unable to read UI image '{}': {error}", path.display()))?
            .to_rgba8();
        self.insert_rgba(key, [image.width(), image.height()], image.into_raw())
    }

    pub fn remove(&mut self, key: &str) -> Option<UiSurfaceImageData> {
        let removed = self.images.remove(key)?;
        self.resident_bytes = self
            .resident_bytes
            .saturating_sub(removed.pixels.len() as u64);
        Some(removed)
    }

    pub fn get(&self, key: &str) -> Option<&UiSurfaceImageData> {
        self.images.get(key)
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
        self.budget_bytes = budget_bytes;
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
        let key = self
            .images
            .iter()
            .filter(|(key, _)| protect_key.is_none_or(|protected| key.as_str() != protected))
            .min_by_key(|(_, image)| image.revision)
            .map(|(key, _)| key.clone())?;
        let removed = self.images.remove(&key)?;
        self.resident_bytes = self
            .resident_bytes
            .saturating_sub(removed.pixels.len() as u64);
        self.evictions = self.evictions.saturating_add(1);
        Some(key)
    }

    /// Ensures that a semantic RafUI icon has a renderer-owned source. The
    /// generated PNG is the runtime representation of the editable SVG master;
    /// the procedural rasterizer remains a compatibility fallback.
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
                // A bad or accidentally transparent generated asset must
                // never turn a toolbar control into an empty box at runtime.
                // Keep the renderer-owned procedural outline as a safe
                // compatibility fallback.
                _ => ([64, 64], builtin_icon_pixels(icon.id)),
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

fn builtin_icon_png(id: UiIconId) -> Option<&'static [u8]> {
    match id {
        UiIconId::Select => Some(include_bytes!("../../../assets/ui_icons/png/select.png")),
        UiIconId::Move => Some(include_bytes!("../../../assets/ui_icons/png/move.png")),
        UiIconId::Rotate => Some(include_bytes!("../../../assets/ui_icons/png/rotate.png")),
        UiIconId::Scale => Some(include_bytes!("../../../assets/ui_icons/png/scale.png")),
        UiIconId::Focus => Some(include_bytes!("../../../assets/ui_icons/png/focus.png")),
        UiIconId::Undo => Some(include_bytes!("../../../assets/ui_icons/png/undo.png")),
        UiIconId::Redo => Some(include_bytes!("../../../assets/ui_icons/png/redo.png")),
        UiIconId::Refresh => Some(include_bytes!("../../../assets/ui_icons/png/refresh.png")),
        UiIconId::Grid => Some(include_bytes!("../../../assets/ui_icons/png/grid.png")),
        UiIconId::View2d => Some(include_bytes!("../../../assets/ui_icons/png/view-2d.png")),
        UiIconId::View3d => Some(include_bytes!("../../../assets/ui_icons/png/view-3d.png")),
        UiIconId::Shaded => Some(include_bytes!("../../../assets/ui_icons/png/shaded.png")),
        UiIconId::Wireframe => Some(include_bytes!("../../../assets/ui_icons/png/wireframe.png")),
        UiIconId::Folder => Some(include_bytes!("../../../assets/ui_icons/png/folder.png")),
        UiIconId::Scene => Some(include_bytes!("../../../assets/ui_icons/png/scene.png")),
        UiIconId::Entity => Some(include_bytes!("../../../assets/ui_icons/png/entity.png")),
        UiIconId::Cube => Some(include_bytes!("../../../assets/ui_icons/png/cube.png")),
        UiIconId::Sphere => Some(include_bytes!("../../../assets/ui_icons/png/sphere.png")),
        UiIconId::Plane => Some(include_bytes!("../../../assets/ui_icons/png/plane.png")),
        UiIconId::Cylinder => Some(include_bytes!("../../../assets/ui_icons/png/cylinder.png")),
        UiIconId::Eye => Some(include_bytes!("../../../assets/ui_icons/png/eye.png")),
        UiIconId::EyeOff => Some(include_bytes!("../../../assets/ui_icons/png/eye-off.png")),
        UiIconId::Lock => Some(include_bytes!("../../../assets/ui_icons/png/lock.png")),
        UiIconId::Unlock => Some(include_bytes!("../../../assets/ui_icons/png/unlock.png")),
        UiIconId::ChevronLeft => Some(include_bytes!(
            "../../../assets/ui_icons/png/chevron-left.png"
        )),
        UiIconId::ChevronRight => Some(include_bytes!(
            "../../../assets/ui_icons/png/chevron-right.png"
        )),
        UiIconId::ChevronDown => Some(include_bytes!(
            "../../../assets/ui_icons/png/chevron-down.png"
        )),
        UiIconId::More => Some(include_bytes!("../../../assets/ui_icons/png/more.png")),
        UiIconId::Search => Some(include_bytes!("../../../assets/ui_icons/png/search.png")),
        UiIconId::Filter => Some(include_bytes!("../../../assets/ui_icons/png/filter.png")),
        UiIconId::Add => Some(include_bytes!("../../../assets/ui_icons/png/add.png")),
        UiIconId::Close => Some(include_bytes!("../../../assets/ui_icons/png/close.png")),
        UiIconId::Play => Some(include_bytes!("../../../assets/ui_icons/png/play.png")),
        UiIconId::Pause | UiIconId::StepForward | UiIconId::Camera => None,
        UiIconId::Stop => Some(include_bytes!("../../../assets/ui_icons/png/stop.png")),
        UiIconId::Console => Some(include_bytes!("../../../assets/ui_icons/png/console.png")),
        UiIconId::Assets => Some(include_bytes!("../../../assets/ui_icons/png/assets.png")),
        UiIconId::Project => Some(include_bytes!("../../../assets/ui_icons/png/project.png")),
        UiIconId::Node => Some(include_bytes!("../../../assets/ui_icons/png/node.png")),
        UiIconId::Agent => Some(include_bytes!("../../../assets/ui_icons/png/agent.png")),
        UiIconId::Schematic => Some(include_bytes!("../../../assets/ui_icons/png/schematic.png")),
        UiIconId::Pcb => Some(include_bytes!("../../../assets/ui_icons/png/pcb.png")),
        UiIconId::Wire => Some(include_bytes!("../../../assets/ui_icons/png/wire.png")),
        UiIconId::Route => Some(include_bytes!("../../../assets/ui_icons/png/route.png")),
        UiIconId::BoardOutline => Some(include_bytes!(
            "../../../assets/ui_icons/png/board-outline.png"
        )),
        UiIconId::ZoomIn => Some(include_bytes!("../../../assets/ui_icons/png/zoom-in.png")),
        UiIconId::ZoomOut => Some(include_bytes!("../../../assets/ui_icons/png/zoom-out.png")),
        UiIconId::Trash => Some(include_bytes!("../../../assets/ui_icons/png/trash.png")),
        UiIconId::Script => Some(include_bytes!("../../../assets/ui_icons/png/script.png")),
        UiIconId::File => Some(include_bytes!("../../../assets/ui_icons/png/file.png")),
        UiIconId::ExternalLink => Some(include_bytes!(
            "../../../assets/ui_icons/png/external-link.png"
        )),
        UiIconId::Pencil => Some(include_bytes!("../../../assets/ui_icons/png/pencil.png")),
        UiIconId::Copy => Some(include_bytes!("../../../assets/ui_icons/png/copy.png")),
        UiIconId::Settings => Some(include_bytes!("../../../assets/ui_icons/png/settings.png")),
        UiIconId::Menu => Some(include_bytes!("../../../assets/ui_icons/png/menu.png")),
        UiIconId::Warning => Some(include_bytes!("../../../assets/ui_icons/png/warning.png")),
        UiIconId::Error => Some(include_bytes!("../../../assets/ui_icons/png/error.png")),
        UiIconId::Success => Some(include_bytes!("../../../assets/ui_icons/png/success.png")),
    }
}
// HARDCODEED!!!

fn icon_id_from_key(key: &str) -> Option<UiIconId> {
    [
        UiIconId::Select,
        UiIconId::Move,
        UiIconId::Rotate,
        UiIconId::Scale,
        UiIconId::Focus,
        UiIconId::Undo,
        UiIconId::Redo,
        UiIconId::Refresh,
        UiIconId::Grid,
        UiIconId::View2d,
        UiIconId::View3d,
        UiIconId::Shaded,
        UiIconId::Wireframe,
        UiIconId::Folder,
        UiIconId::Scene,
        UiIconId::Entity,
        UiIconId::Camera,
        UiIconId::Cube,
        UiIconId::Sphere,
        UiIconId::Plane,
        UiIconId::Cylinder,
        UiIconId::Eye,
        UiIconId::EyeOff,
        UiIconId::Lock,
        UiIconId::Unlock,
        UiIconId::ChevronLeft,
        UiIconId::ChevronRight,
        UiIconId::ChevronDown,
        UiIconId::More,
        UiIconId::Search,
        UiIconId::Filter,
        UiIconId::Add,
        UiIconId::Close,
        UiIconId::Play,
        UiIconId::Pause,
        UiIconId::StepForward,
        UiIconId::Stop,
        UiIconId::Console,
        UiIconId::Assets,
        UiIconId::Project,
        UiIconId::Node,
        UiIconId::Agent,
        UiIconId::Schematic,
        UiIconId::Pcb,
        UiIconId::Wire,
        UiIconId::Route,
        UiIconId::BoardOutline,
        UiIconId::ZoomIn,
        UiIconId::ZoomOut,
        UiIconId::Trash,
        UiIconId::Script,
        UiIconId::File,
        UiIconId::ExternalLink,
        UiIconId::Pencil,
        UiIconId::Copy,
        UiIconId::Settings,
        UiIconId::Menu,
        UiIconId::Warning,
        UiIconId::Error,
        UiIconId::Success,
    ]
    .into_iter()
    .find(|id| id.key() == key)
}

fn builtin_icon_pixels(id: UiIconId) -> Vec<u8> {
    const SIZE: usize = 64;
    let mut pixels = vec![0_u8; SIZE * SIZE * 4];
    let stroke = 4.0;
    let line = |pixels: &mut [u8], a: [f32; 2], b: [f32; 2], width: f32| {
        for y in 0..SIZE {
            for x in 0..SIZE {
                let distance = distance_to_segment([x as f32 + 0.5, y as f32 + 0.5], a, b);
                if distance <= width {
                    write_icon_pixel(pixels, x, y, ((1.0 - distance / width) * 255.0) as u8);
                }
            }
        }
    };
    let circle = |pixels: &mut [u8], center: [f32; 2], radius: f32, width: f32| {
        for y in 0..SIZE {
            for x in 0..SIZE {
                let dx = x as f32 + 0.5 - center[0];
                let dy = y as f32 + 0.5 - center[1];
                let distance = (dx * dx + dy * dy).sqrt();
                let edge = (distance - radius).abs();
                if edge <= width {
                    write_icon_pixel(pixels, x, y, ((1.0 - edge / width) * 255.0) as u8);
                }
            }
        }
    };
    let filled_rect = |pixels: &mut [u8], left: usize, top: usize, right: usize, bottom: usize| {
        for y in top.min(SIZE)..=bottom.min(SIZE.saturating_sub(1)) {
            for x in left.min(SIZE)..=right.min(SIZE.saturating_sub(1)) {
                write_icon_pixel(pixels, x, y, 255);
            }
        }
    };
    let filled_triangle = |pixels: &mut [u8], points: [[f32; 2]; 3]| {
        let min_x = points
            .iter()
            .map(|point| point[0])
            .fold(SIZE as f32, f32::min)
            .floor()
            .max(0.0) as usize;
        let max_x = points
            .iter()
            .map(|point| point[0])
            .fold(0.0, f32::max)
            .ceil()
            .min(SIZE.saturating_sub(1) as f32) as usize;
        let min_y = points
            .iter()
            .map(|point| point[1])
            .fold(SIZE as f32, f32::min)
            .floor()
            .max(0.0) as usize;
        let max_y = points
            .iter()
            .map(|point| point[1])
            .fold(0.0, f32::max)
            .ceil()
            .min(SIZE.saturating_sub(1) as f32) as usize;
        let area = |a: [f32; 2], b: [f32; 2], c: [f32; 2]| {
            (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
        };
        let signed_area = area(points[0], points[1], points[2]);
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let point = [x as f32 + 0.5, y as f32 + 0.5];
                let a = area(points[0], points[1], point);
                let b = area(points[1], points[2], point);
                let c = area(points[2], points[0], point);
                if (a >= 0.0 && b >= 0.0 && c >= 0.0) || (a <= 0.0 && b <= 0.0 && c <= 0.0) {
                    let coverage = ((a.abs() + b.abs() + c.abs()) / signed_area.abs().max(1.0))
                        .clamp(0.0, 1.0);
                    write_icon_pixel(pixels, x, y, (coverage * 255.0) as u8);
                }
            }
        }
    };
    // Shared document outline used by the file-like procedural icon fallbacks.
    let file_outline = |pixels: &mut [u8], width: f32| {
        let segments = [
            ([16.0, 10.0], [40.0, 10.0]),
            ([40.0, 10.0], [50.0, 20.0]),
            ([50.0, 20.0], [50.0, 54.0]),
            ([50.0, 54.0], [16.0, 54.0]),
            ([16.0, 54.0], [16.0, 10.0]),
            ([40.0, 10.0], [40.0, 20.0]),
            ([40.0, 20.0], [50.0, 20.0]),
        ];
        for (a, b) in segments {
            line(pixels, a, b, width);
        }
    };
    // I HATE THESE And No one should Replicated it!
    match id {
        UiIconId::Select => {
            line(&mut pixels, [18.0, 10.0], [18.0, 52.0], stroke);
            line(&mut pixels, [18.0, 10.0], [47.0, 39.0], stroke);
            line(&mut pixels, [18.0, 52.0], [28.0, 42.0], stroke);
            line(&mut pixels, [28.0, 42.0], [38.0, 54.0], stroke);
            line(&mut pixels, [38.0, 54.0], [44.0, 49.0], stroke);
        }
        UiIconId::Move => {
            line(&mut pixels, [32.0, 10.0], [32.0, 54.0], stroke);
            line(&mut pixels, [10.0, 32.0], [54.0, 32.0], stroke);
            line(&mut pixels, [32.0, 10.0], [25.0, 18.0], stroke);
            line(&mut pixels, [32.0, 10.0], [39.0, 18.0], stroke);
            line(&mut pixels, [32.0, 54.0], [25.0, 46.0], stroke);
            line(&mut pixels, [32.0, 54.0], [39.0, 46.0], stroke);
            line(&mut pixels, [10.0, 32.0], [18.0, 25.0], stroke);
            line(&mut pixels, [10.0, 32.0], [18.0, 39.0], stroke);
            line(&mut pixels, [54.0, 32.0], [46.0, 25.0], stroke);
            line(&mut pixels, [54.0, 32.0], [46.0, 39.0], stroke);
        }
        UiIconId::Rotate => {
            circle(&mut pixels, [32.0, 32.0], 18.0, stroke);
            line(&mut pixels, [47.0, 14.0], [51.0, 26.0], stroke);
            line(&mut pixels, [47.0, 14.0], [36.0, 17.0], stroke);
        }
        UiIconId::Scale => {
            line(&mut pixels, [12.0, 26.0], [12.0, 12.0], stroke);
            line(&mut pixels, [12.0, 12.0], [26.0, 12.0], stroke);
            line(&mut pixels, [38.0, 12.0], [52.0, 12.0], stroke);
            line(&mut pixels, [52.0, 12.0], [52.0, 26.0], stroke);
            line(&mut pixels, [12.0, 38.0], [12.0, 52.0], stroke);
            line(&mut pixels, [12.0, 52.0], [26.0, 52.0], stroke);
            line(&mut pixels, [38.0, 52.0], [52.0, 52.0], stroke);
            line(&mut pixels, [52.0, 52.0], [52.0, 38.0], stroke);
        }
        UiIconId::Undo => {
            line(&mut pixels, [49.0, 20.0], [18.0, 20.0], stroke);
            line(&mut pixels, [18.0, 20.0], [30.0, 10.0], stroke);
            line(&mut pixels, [18.0, 20.0], [30.0, 30.0], stroke);
            circle(&mut pixels, [39.0, 37.0], 13.0, stroke);
        }
        UiIconId::Redo => {
            line(&mut pixels, [15.0, 20.0], [46.0, 20.0], stroke);
            line(&mut pixels, [46.0, 20.0], [34.0, 10.0], stroke);
            line(&mut pixels, [46.0, 20.0], [34.0, 30.0], stroke);
            circle(&mut pixels, [25.0, 37.0], 13.0, stroke);
        }
        UiIconId::Refresh => {
            circle(&mut pixels, [32.0, 32.0], 19.0, stroke);
            line(&mut pixels, [44.0, 14.0], [52.0, 15.0], stroke);
            line(&mut pixels, [52.0, 15.0], [50.0, 24.0], stroke);
            line(&mut pixels, [20.0, 50.0], [12.0, 49.0], stroke);
            line(&mut pixels, [12.0, 49.0], [14.0, 40.0], stroke);
        }
        UiIconId::Close => {
            line(&mut pixels, [16.0, 16.0], [48.0, 48.0], stroke);
            line(&mut pixels, [48.0, 16.0], [16.0, 48.0], stroke);
        }
        UiIconId::Add => {
            line(&mut pixels, [32.0, 14.0], [32.0, 50.0], stroke);
            line(&mut pixels, [14.0, 32.0], [50.0, 32.0], stroke);
        }
        UiIconId::Eye | UiIconId::EyeOff => {
            line(&mut pixels, [12.0, 32.0], [24.0, 20.0], stroke);
            line(&mut pixels, [24.0, 20.0], [40.0, 20.0], stroke);
            line(&mut pixels, [40.0, 20.0], [52.0, 32.0], stroke);
            line(&mut pixels, [52.0, 32.0], [40.0, 44.0], stroke);
            line(&mut pixels, [40.0, 44.0], [24.0, 44.0], stroke);
            line(&mut pixels, [24.0, 44.0], [12.0, 32.0], stroke);
            circle(&mut pixels, [32.0, 32.0], 6.0, stroke);
            if id == UiIconId::EyeOff {
                line(&mut pixels, [14.0, 14.0], [50.0, 50.0], stroke);
            }
        }
        UiIconId::Lock | UiIconId::Unlock => {
            line(&mut pixels, [20.0, 28.0], [44.0, 28.0], stroke);
            line(&mut pixels, [20.0, 28.0], [20.0, 50.0], stroke);
            line(&mut pixels, [44.0, 28.0], [44.0, 50.0], stroke);
            line(&mut pixels, [20.0, 50.0], [44.0, 50.0], stroke);
            line(&mut pixels, [24.0, 28.0], [24.0, 18.0], stroke);
            line(&mut pixels, [24.0, 18.0], [40.0, 18.0], stroke);
            if id == UiIconId::Lock {
                line(&mut pixels, [40.0, 18.0], [40.0, 28.0], stroke);
            } else {
                line(&mut pixels, [40.0, 18.0], [40.0, 12.0], stroke);
                line(&mut pixels, [40.0, 12.0], [48.0, 12.0], stroke);
            }
            circle(&mut pixels, [32.0, 38.0], 2.0, 2.0);
        }
        UiIconId::ChevronLeft => {
            line(&mut pixels, [40.0, 14.0], [22.0, 32.0], stroke);
            line(&mut pixels, [22.0, 32.0], [40.0, 50.0], stroke);
        }
        UiIconId::ChevronRight => {
            line(&mut pixels, [24.0, 14.0], [42.0, 32.0], stroke);
            line(&mut pixels, [42.0, 32.0], [24.0, 50.0], stroke);
        }
        UiIconId::ChevronDown => {
            line(&mut pixels, [14.0, 24.0], [32.0, 42.0], stroke);
            line(&mut pixels, [32.0, 42.0], [50.0, 24.0], stroke);
        }
        UiIconId::Folder => {
            line(&mut pixels, [10.0, 20.0], [27.0, 20.0], stroke);
            line(&mut pixels, [27.0, 20.0], [32.0, 25.0], stroke);
            line(&mut pixels, [32.0, 25.0], [54.0, 25.0], stroke);
            line(&mut pixels, [54.0, 25.0], [50.0, 48.0], stroke);
            line(&mut pixels, [50.0, 48.0], [12.0, 48.0], stroke);
            line(&mut pixels, [12.0, 48.0], [10.0, 20.0], stroke);
        }
        UiIconId::Grid => {
            for offset in [16.0, 32.0, 48.0] {
                line(&mut pixels, [offset, 12.0], [offset, 52.0], 2.5);
                line(&mut pixels, [12.0, offset], [52.0, offset], 2.5);
            }
            line(&mut pixels, [12.0, 12.0], [52.0, 12.0], stroke);
            line(&mut pixels, [52.0, 12.0], [52.0, 52.0], stroke);
            line(&mut pixels, [52.0, 52.0], [12.0, 52.0], stroke);
            line(&mut pixels, [12.0, 52.0], [12.0, 12.0], stroke);
        }
        UiIconId::View2d => {
            line(&mut pixels, [12.0, 14.0], [52.0, 14.0], stroke);
            line(&mut pixels, [52.0, 14.0], [52.0, 50.0], stroke);
            line(&mut pixels, [52.0, 50.0], [12.0, 50.0], stroke);
            line(&mut pixels, [12.0, 50.0], [12.0, 14.0], stroke);
            line(&mut pixels, [20.0, 32.0], [44.0, 32.0], 2.5);
            line(&mut pixels, [32.0, 20.0], [32.0, 44.0], 2.5);
        }
        UiIconId::View3d | UiIconId::Wireframe | UiIconId::Shaded => {
            line(&mut pixels, [20.0, 14.0], [44.0, 14.0], stroke);
            line(&mut pixels, [44.0, 14.0], [54.0, 24.0], stroke);
            line(&mut pixels, [54.0, 24.0], [54.0, 46.0], stroke);
            line(&mut pixels, [54.0, 46.0], [32.0, 54.0], stroke);
            line(&mut pixels, [32.0, 54.0], [10.0, 46.0], stroke);
            line(&mut pixels, [10.0, 46.0], [10.0, 24.0], stroke);
            line(&mut pixels, [10.0, 24.0], [20.0, 14.0], stroke);
            line(&mut pixels, [10.0, 24.0], [32.0, 32.0], stroke);
            line(&mut pixels, [32.0, 32.0], [54.0, 24.0], stroke);
            line(&mut pixels, [32.0, 32.0], [32.0, 54.0], stroke);
            if id == UiIconId::Shaded {
                filled_triangle(&mut pixels, [[12.0, 25.0], [32.0, 33.0], [32.0, 51.0]]);
            }
        }
        UiIconId::Search => {
            circle(&mut pixels, [28.0, 28.0], 14.0, stroke);
            line(&mut pixels, [39.0, 39.0], [53.0, 53.0], stroke);
        }
        UiIconId::Focus => {
            circle(&mut pixels, [32.0, 32.0], 13.0, stroke);
            line(&mut pixels, [32.0, 8.0], [32.0, 20.0], stroke);
            line(&mut pixels, [32.0, 44.0], [32.0, 56.0], stroke);
            line(&mut pixels, [8.0, 32.0], [20.0, 32.0], stroke);
            line(&mut pixels, [44.0, 32.0], [56.0, 32.0], stroke);
        }
        UiIconId::Warning => {
            line(&mut pixels, [32.0, 10.0], [54.0, 50.0], stroke);
            line(&mut pixels, [54.0, 50.0], [10.0, 50.0], stroke);
            line(&mut pixels, [10.0, 50.0], [32.0, 10.0], stroke);
            line(&mut pixels, [32.0, 23.0], [32.0, 38.0], stroke);
            circle(&mut pixels, [32.0, 44.0], 1.5, 2.0);
        }
        UiIconId::Scene | UiIconId::Assets | UiIconId::Project => {
            line(&mut pixels, [10.0, 20.0], [27.0, 20.0], stroke);
            line(&mut pixels, [27.0, 20.0], [32.0, 25.0], stroke);
            line(&mut pixels, [32.0, 25.0], [54.0, 25.0], stroke);
            line(&mut pixels, [54.0, 25.0], [50.0, 48.0], stroke);
            line(&mut pixels, [50.0, 48.0], [12.0, 48.0], stroke);
            line(&mut pixels, [12.0, 48.0], [10.0, 20.0], stroke);
            if id == UiIconId::Scene {
                line(&mut pixels, [18.0, 32.0], [45.0, 32.0], 2.5);
                line(&mut pixels, [18.0, 39.0], [39.0, 39.0], 2.5);
            }
        }
        UiIconId::Camera => {
            for (a, b) in [
                ([10.0, 22.0], [42.0, 22.0]),
                ([42.0, 22.0], [42.0, 46.0]),
                ([42.0, 46.0], [10.0, 46.0]),
                ([10.0, 46.0], [10.0, 22.0]),
                ([42.0, 28.0], [54.0, 20.0]),
                ([54.0, 20.0], [54.0, 48.0]),
                ([54.0, 48.0], [42.0, 40.0]),
            ] {
                line(&mut pixels, a, b, stroke);
            }
            circle(&mut pixels, [22.0, 16.0], 6.0, stroke);
            circle(&mut pixels, [35.0, 16.0], 6.0, stroke);
        }
        UiIconId::Entity | UiIconId::Cube => {
            line(&mut pixels, [32.0, 10.0], [52.0, 21.0], stroke);
            line(&mut pixels, [52.0, 21.0], [52.0, 44.0], stroke);
            line(&mut pixels, [52.0, 44.0], [32.0, 54.0], stroke);
            line(&mut pixels, [32.0, 54.0], [12.0, 44.0], stroke);
            line(&mut pixels, [12.0, 44.0], [12.0, 21.0], stroke);
            line(&mut pixels, [12.0, 21.0], [32.0, 10.0], stroke);
            line(&mut pixels, [12.0, 21.0], [32.0, 32.0], stroke);
            line(&mut pixels, [32.0, 32.0], [52.0, 21.0], stroke);
            line(&mut pixels, [32.0, 32.0], [32.0, 54.0], stroke);
        }
        UiIconId::Sphere => {
            circle(&mut pixels, [32.0, 32.0], 21.0, stroke);
            line(&mut pixels, [13.0, 32.0], [51.0, 32.0], 2.5);
            line(&mut pixels, [20.0, 18.0], [20.0, 46.0], 2.5);
            line(&mut pixels, [44.0, 18.0], [44.0, 46.0], 2.5);
        }
        UiIconId::Plane => {
            line(&mut pixels, [12.0, 14.0], [52.0, 14.0], stroke);
            line(&mut pixels, [52.0, 14.0], [52.0, 50.0], stroke);
            line(&mut pixels, [52.0, 50.0], [12.0, 50.0], stroke);
            line(&mut pixels, [12.0, 50.0], [12.0, 14.0], stroke);
            line(&mut pixels, [16.0, 46.0], [48.0, 18.0], 2.5);
        }
        UiIconId::Cylinder => {
            circle(&mut pixels, [32.0, 16.0], 20.0, stroke);
            circle(&mut pixels, [32.0, 48.0], 20.0, stroke);
            line(&mut pixels, [12.0, 16.0], [12.0, 48.0], stroke);
            line(&mut pixels, [52.0, 16.0], [52.0, 48.0], stroke);
        }
        UiIconId::More => {
            circle(&mut pixels, [16.0, 32.0], 2.5, 2.5);
            circle(&mut pixels, [32.0, 32.0], 2.5, 2.5);
            circle(&mut pixels, [48.0, 32.0], 2.5, 2.5);
        }
        UiIconId::Filter => {
            line(&mut pixels, [10.0, 14.0], [54.0, 14.0], stroke);
            line(&mut pixels, [10.0, 14.0], [28.0, 34.0], stroke);
            line(&mut pixels, [54.0, 14.0], [36.0, 34.0], stroke);
            line(&mut pixels, [28.0, 34.0], [28.0, 50.0], stroke);
            line(&mut pixels, [36.0, 34.0], [36.0, 50.0], stroke);
            line(&mut pixels, [28.0, 50.0], [36.0, 50.0], stroke);
        }
        UiIconId::Play => {
            filled_triangle(&mut pixels, [[20.0, 12.0], [50.0, 32.0], [20.0, 52.0]]);
        }
        UiIconId::Stop => filled_rect(&mut pixels, 15, 15, 49, 49),
        UiIconId::Pause => {
            filled_rect(&mut pixels, 17, 14, 27, 50);
            filled_rect(&mut pixels, 37, 14, 47, 50);
        }
        UiIconId::StepForward => {
            filled_triangle(&mut pixels, [[16.0, 14.0], [16.0, 50.0], [40.0, 32.0]]);
            filled_rect(&mut pixels, 44, 14, 50, 50);
        }
        UiIconId::Console => {
            line(&mut pixels, [10.0, 15.0], [54.0, 15.0], stroke);
            line(&mut pixels, [54.0, 15.0], [54.0, 49.0], stroke);
            line(&mut pixels, [54.0, 49.0], [10.0, 49.0], stroke);
            line(&mut pixels, [10.0, 49.0], [10.0, 15.0], stroke);
            line(&mut pixels, [18.0, 25.0], [28.0, 32.0], stroke);
            line(&mut pixels, [28.0, 32.0], [18.0, 39.0], stroke);
            line(&mut pixels, [34.0, 40.0], [46.0, 40.0], stroke);
        }
        UiIconId::Node => {
            line(&mut pixels, [20.0, 20.0], [44.0, 44.0], stroke);
            line(&mut pixels, [44.0, 20.0], [20.0, 44.0], stroke);
            circle(&mut pixels, [20.0, 20.0], 7.0, stroke);
            circle(&mut pixels, [44.0, 20.0], 7.0, stroke);
            circle(&mut pixels, [20.0, 44.0], 7.0, stroke);
            circle(&mut pixels, [44.0, 44.0], 7.0, stroke);
        }
        UiIconId::Schematic => {
            line(&mut pixels, [12.0, 18.0], [52.0, 18.0], stroke);
            line(&mut pixels, [12.0, 46.0], [52.0, 46.0], stroke);
            line(&mut pixels, [20.0, 18.0], [20.0, 46.0], stroke);
            line(&mut pixels, [40.0, 18.0], [40.0, 46.0], stroke);
            circle(&mut pixels, [12.0, 18.0], 3.0, 2.0);
            circle(&mut pixels, [52.0, 46.0], 3.0, 2.0);
        }
        UiIconId::Pcb => {
            line(&mut pixels, [12.0, 12.0], [52.0, 12.0], stroke);
            line(&mut pixels, [52.0, 12.0], [52.0, 52.0], stroke);
            line(&mut pixels, [52.0, 52.0], [12.0, 52.0], stroke);
            line(&mut pixels, [12.0, 52.0], [12.0, 12.0], stroke);
            line(&mut pixels, [16.0, 40.0], [30.0, 40.0], stroke);
            line(&mut pixels, [30.0, 40.0], [30.0, 22.0], stroke);
            circle(&mut pixels, [16.0, 40.0], 3.0, 2.0);
            circle(&mut pixels, [46.0, 22.0], 3.0, 2.0);
        }
        UiIconId::Wire => {
            line(&mut pixels, [10.0, 32.0], [22.0, 32.0], stroke);
            line(&mut pixels, [22.0, 32.0], [32.0, 20.0], stroke);
            line(&mut pixels, [32.0, 20.0], [42.0, 20.0], stroke);
            line(&mut pixels, [42.0, 20.0], [54.0, 32.0], stroke);
            circle(&mut pixels, [10.0, 32.0], 3.0, 2.0);
            circle(&mut pixels, [54.0, 32.0], 3.0, 2.0);
        }
        UiIconId::Route => {
            line(&mut pixels, [10.0, 46.0], [24.0, 46.0], stroke);
            line(&mut pixels, [24.0, 46.0], [24.0, 20.0], stroke);
            line(&mut pixels, [24.0, 20.0], [52.0, 20.0], stroke);
            circle(&mut pixels, [10.0, 46.0], 3.0, 2.0);
            circle(&mut pixels, [52.0, 20.0], 3.0, 2.0);
        }
        UiIconId::BoardOutline => {
            line(&mut pixels, [16.0, 12.0], [48.0, 12.0], stroke);
            line(&mut pixels, [48.0, 12.0], [52.0, 16.0], stroke);
            line(&mut pixels, [52.0, 16.0], [52.0, 48.0], stroke);
            line(&mut pixels, [52.0, 48.0], [48.0, 52.0], stroke);
            line(&mut pixels, [48.0, 52.0], [16.0, 52.0], stroke);
            line(&mut pixels, [16.0, 52.0], [12.0, 48.0], stroke);
            line(&mut pixels, [12.0, 48.0], [12.0, 16.0], stroke);
            line(&mut pixels, [12.0, 16.0], [16.0, 12.0], stroke);
        }
        UiIconId::ZoomIn => {
            circle(&mut pixels, [28.0, 28.0], 15.0, stroke);
            line(&mut pixels, [39.0, 39.0], [53.0, 53.0], stroke);
            line(&mut pixels, [20.0, 28.0], [36.0, 28.0], stroke);
            line(&mut pixels, [28.0, 20.0], [28.0, 36.0], stroke);
        }
        UiIconId::ZoomOut => {
            circle(&mut pixels, [28.0, 28.0], 15.0, stroke);
            line(&mut pixels, [39.0, 39.0], [53.0, 53.0], stroke);
            line(&mut pixels, [20.0, 28.0], [36.0, 28.0], stroke);
        }
        UiIconId::Trash => {
            line(&mut pixels, [18.0, 20.0], [46.0, 20.0], stroke);
            line(&mut pixels, [24.0, 20.0], [24.0, 14.0], stroke);
            line(&mut pixels, [24.0, 14.0], [40.0, 14.0], stroke);
            line(&mut pixels, [40.0, 14.0], [40.0, 20.0], stroke);
            line(&mut pixels, [21.0, 20.0], [24.0, 51.0], stroke);
            line(&mut pixels, [24.0, 51.0], [40.0, 51.0], stroke);
            line(&mut pixels, [40.0, 51.0], [43.0, 20.0], stroke);
            line(&mut pixels, [29.0, 27.0], [30.0, 44.0], 2.5);
            line(&mut pixels, [35.0, 27.0], [34.0, 44.0], 2.5);
        }
        UiIconId::Script => {
            file_outline(&mut pixels, stroke);
            line(&mut pixels, [31.0, 30.0], [25.0, 36.0], 2.8);
            line(&mut pixels, [25.0, 36.0], [31.0, 42.0], 2.8);
            line(&mut pixels, [33.0, 43.0], [39.0, 29.0], 2.8);
            line(&mut pixels, [35.0, 30.0], [41.0, 36.0], 2.8);
            line(&mut pixels, [41.0, 36.0], [35.0, 42.0], 2.8);
        }
        UiIconId::File => {
            file_outline(&mut pixels, stroke);
            line(&mut pixels, [23.0, 30.0], [43.0, 30.0], 2.8);
            line(&mut pixels, [23.0, 38.0], [43.0, 38.0], 2.8);
            line(&mut pixels, [23.0, 46.0], [35.0, 46.0], 2.8);
        }
        UiIconId::ExternalLink => {
            line(&mut pixels, [30.0, 34.0], [12.0, 34.0], stroke);
            line(&mut pixels, [12.0, 34.0], [12.0, 52.0], stroke);
            line(&mut pixels, [12.0, 52.0], [46.0, 52.0], stroke);
            line(&mut pixels, [46.0, 52.0], [46.0, 38.0], stroke);
            line(&mut pixels, [34.0, 30.0], [52.0, 12.0], stroke);
            line(&mut pixels, [40.0, 12.0], [52.0, 12.0], stroke);
            line(&mut pixels, [52.0, 12.0], [52.0, 24.0], stroke);
        }
        UiIconId::Pencil => {
            line(&mut pixels, [16.0, 48.0], [19.0, 38.0], stroke);
            line(&mut pixels, [19.0, 38.0], [42.0, 15.0], stroke);
            line(&mut pixels, [42.0, 15.0], [50.0, 23.0], stroke);
            line(&mut pixels, [50.0, 23.0], [27.0, 46.0], stroke);
            line(&mut pixels, [27.0, 46.0], [16.0, 48.0], stroke);
            line(&mut pixels, [19.0, 38.0], [27.0, 46.0], stroke);
            line(&mut pixels, [42.0, 15.0], [50.0, 23.0], stroke);
        }
        UiIconId::Copy => {
            line(&mut pixels, [10.0, 24.0], [40.0, 24.0], stroke);
            line(&mut pixels, [40.0, 24.0], [40.0, 54.0], stroke);
            line(&mut pixels, [40.0, 54.0], [10.0, 54.0], stroke);
            line(&mut pixels, [10.0, 54.0], [10.0, 24.0], stroke);
            line(&mut pixels, [18.0, 24.0], [18.0, 10.0], stroke);
            line(&mut pixels, [18.0, 10.0], [48.0, 10.0], stroke);
            line(&mut pixels, [48.0, 10.0], [48.0, 40.0], stroke);
            line(&mut pixels, [48.0, 40.0], [40.0, 40.0], stroke);
        }
        UiIconId::Settings => {
            circle(&mut pixels, [32.0, 32.0], 9.0, stroke);
            for angle in [0.0_f32, 1.047, 2.094, 3.141, 4.188, 5.235] {
                let a = [32.0 + angle.cos() * 14.0, 32.0 + angle.sin() * 14.0];
                let b = [32.0 + angle.cos() * 24.0, 32.0 + angle.sin() * 24.0];
                line(&mut pixels, a, b, stroke);
            }
        }
        UiIconId::Menu => {
            line(&mut pixels, [12.0, 18.0], [52.0, 18.0], stroke);
            line(&mut pixels, [12.0, 32.0], [52.0, 32.0], stroke);
            line(&mut pixels, [12.0, 46.0], [52.0, 46.0], stroke);
        }
        UiIconId::Error => {
            circle(&mut pixels, [32.0, 32.0], 20.0, stroke);
            line(&mut pixels, [23.0, 23.0], [41.0, 41.0], stroke);
            line(&mut pixels, [41.0, 23.0], [23.0, 41.0], stroke);
        }
        UiIconId::Success => {
            circle(&mut pixels, [32.0, 32.0], 20.0, stroke);
            line(&mut pixels, [20.0, 32.0], [29.0, 41.0], stroke);
            line(&mut pixels, [29.0, 41.0], [46.0, 22.0], stroke);
        }
        _ => {
            line(&mut pixels, [16.0, 32.0], [48.0, 32.0], stroke);
            line(&mut pixels, [32.0, 16.0], [32.0, 48.0], stroke);
            circle(&mut pixels, [32.0, 32.0], 20.0, 2.5);
        }
    }
    pixels
}

fn write_icon_pixel(pixels: &mut [u8], x: usize, y: usize, alpha: u8) {
    let index = (y * 64 + x) * 4;
    pixels[index..index + 4].copy_from_slice(&[255, 255, 255, alpha]);
}

fn distance_to_segment(point: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let ap = [point[0] - a[0], point[1] - a[1]];
    let denominator = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if denominator <= f32::EPSILON {
        0.0
    } else {
        (ap[0] * ab[0] + ap[1] * ab[1]) / denominator
    }
    .clamp(0.0, 1.0);
    let closest = [a[0] + ab[0] * t, a[1] + ab[1] * t];
    ((point[0] - closest[0]).powi(2) + (point[1] - closest[1]).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn budget_evicts_oldest_revision_to_admit_new_image() {
        let mut store = UiSurfaceImageStore::default();
        store.set_budget_bytes(32);
        assert!(store.insert_rgba("first", [2, 2], vec![1; 16]).is_ok());
        assert!(store.insert_rgba("second", [2, 2], vec![2; 16]).is_ok());
        assert!(store.insert_rgba("third", [2, 2], vec![3; 16]).is_ok());
        assert!(store.get("first").is_none(), "oldest revision must evict");
        assert!(store.get("second").is_some());
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
                        UiIconId::Camera | UiIconId::Pause | UiIconId::StepForward
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
