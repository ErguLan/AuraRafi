//! Resource cache for raster images referenced by retained UI documents.
//!
//! Documents store only `UiImageSource` keys. This cache owns decoded pixels
//! at the surface-host boundary, so a project can replace or unload icons
//! without modifying serialized layout data.
//! AI SLOP!!! But used like Fallback TO BE profesionals🥀🥀🥀
use std::collections::BTreeMap;
use std::path::Path;

use raf_ui::{UiIcon, UiIconId};

#[derive(Debug, Clone)]
pub struct UiSurfaceImageData {
    pub size: [u32; 2],
    pub pixels: Vec<u8>,
    pub revision: u64,
}

#[derive(Debug, Clone, Default)]
pub struct UiSurfaceImageStore {
    images: BTreeMap<String, UiSurfaceImageData>,
    next_revision: u64,
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
        self.next_revision = self.next_revision.wrapping_add(1).max(1);
        self.images.insert(
            key.into(),
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
        self.images.remove(key)
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

    /// Resolves a draw-list source without making product surfaces understand
    /// how built-in icons are rasterized.
    pub fn ensure_builtin_key(&mut self, key: &str) {
        let Some(id) = key.strip_prefix("builtin://icon/") else {
            return;
        };
        let Some(id) = icon_id_from_key(id) else {
            return;
        };
        let _ = self.ensure_builtin_icon(UiIcon::new(id));
    }
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
        UiIconId::Stop => Some(include_bytes!("../../../assets/ui_icons/png/stop.png")),
        UiIconId::Console => Some(include_bytes!("../../../assets/ui_icons/png/console.png")),
        UiIconId::Assets => Some(include_bytes!("../../../assets/ui_icons/png/assets.png")),
        UiIconId::Project => Some(include_bytes!("../../../assets/ui_icons/png/project.png")),
        UiIconId::Node => Some(include_bytes!("../../../assets/ui_icons/png/node.png")),
        UiIconId::Agent => Some(include_bytes!("../../../assets/ui_icons/png/agent.png")),
        UiIconId::Schematic => Some(include_bytes!("../../../assets/ui_icons/png/schematic.png")),
        UiIconId::Pcb => Some(include_bytes!("../../../assets/ui_icons/png/pcb.png")),
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
        UiIconId::Grid,
        UiIconId::View2d,
        UiIconId::View3d,
        UiIconId::Shaded,
        UiIconId::Wireframe,
        UiIconId::Folder,
        UiIconId::Scene,
        UiIconId::Entity,
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
        UiIconId::Stop,
        UiIconId::Console,
        UiIconId::Assets,
        UiIconId::Project,
        UiIconId::Node,
        UiIconId::Agent,
        UiIconId::Schematic,
        UiIconId::Pcb,
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
    fn every_semantic_icon_has_a_generated_runtime_asset() {
        let icons = [
            UiIconId::Select,
            UiIconId::Move,
            UiIconId::Rotate,
            UiIconId::Scale,
            UiIconId::Focus,
            UiIconId::Undo,
            UiIconId::Redo,
            UiIconId::Grid,
            UiIconId::View2d,
            UiIconId::View3d,
            UiIconId::Shaded,
            UiIconId::Wireframe,
            UiIconId::Folder,
            UiIconId::Scene,
            UiIconId::Entity,
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
            UiIconId::Stop,
            UiIconId::Console,
            UiIconId::Assets,
            UiIconId::Project,
            UiIconId::Node,
            UiIconId::Agent,
            UiIconId::Schematic,
            UiIconId::Pcb,
            UiIconId::Settings,
            UiIconId::Menu,
            UiIconId::Warning,
            UiIconId::Error,
            UiIconId::Success,
        ];

        for icon in icons {
            let bytes = builtin_icon_png(icon).expect("generated icon PNG");
            let image = image::load_from_memory(bytes)
                .expect("generated icon PNG decodes")
                .to_rgba8();
            assert_eq!(image.dimensions(), (64, 64), "{}", icon.key());
        }
    }
}
