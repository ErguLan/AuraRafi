//! Small native overview image for the Electronics viewport.
//!
//! The minimap is an overview, not a second schematic renderer. It intentionally
//! draws only abstract component bodies, pins, connectivity and the viewport
//! frame. Component artwork remains owned by the native SVG/PNG catalog used by
//! the main RafUI canvas overlay.

use glam::Vec2;
use raf_electronics::{CadObject, CadObjectKind, CadScene};

use crate::electronics_controller::{CadCamera, ElectronicsSelection};

pub const IMAGE_KEY: &str = "electronics://minimap";
pub const IMAGE_SIZE: [u32; 2] = [440, 280];

const BACKGROUND: [u8; 4] = [10, 12, 16, 246];
const BORDER: [u8; 4] = [74, 82, 96, 255];
const VIEWPORT: [u8; 4] = [255, 172, 64, 255];
const COMPONENT: [u8; 4] = [190, 112, 34, 230];
const COMPONENT_SELECTED: [u8; 4] = [255, 196, 96, 255];
const PIN: [u8; 4] = [154, 222, 170, 235];
const WIRE: [u8; 4] = [112, 224, 136, 235];
const TRACE: [u8; 4] = [212, 119, 26, 245];
const AIRWIRE: [u8; 4] = [198, 204, 216, 180];
const DRC: [u8; 4] = [255, 112, 112, 255];

pub fn build_rgba(
    scene: &CadScene,
    camera: CadCamera,
    canvas_size: Vec2,
    selection: Option<ElectronicsSelection>,
) -> ([u32; 2], Vec<u8>) {
    let width = IMAGE_SIZE[0] as usize;
    let height = IMAGE_SIZE[1] as usize;
    let mut pixels = vec![0_u8; width * height * 4];
    fill(&mut pixels, BACKGROUND);

    let (min, max) = overview_bounds(scene).unwrap_or_else(|| {
        let bounds = camera.world_bounds(canvas_size.max(Vec2::ONE));
        (
            Vec2::new(bounds[0], bounds[2]),
            Vec2::new(bounds[1], bounds[3]),
        )
    });
    let map = Map::new(min, max, width, height);

    for object in &scene.objects {
        draw_object(&mut pixels, &map, object, selection);
    }

    let viewport = camera.world_bounds(canvas_size.max(Vec2::ONE));
    map.border_rect(
        &mut pixels,
        Vec2::new(viewport[0], viewport[2]),
        Vec2::new(viewport[1], viewport[3]),
        VIEWPORT,
        2,
    );
    draw_frame(&mut pixels, width, height, BORDER);

    (IMAGE_SIZE, pixels)
}

#[derive(Clone, Copy)]
struct Map {
    min: Vec2,
    max: Vec2,
    width: usize,
    height: usize,
}

impl Map {
    fn new(min: Vec2, max: Vec2, width: usize, height: usize) -> Self {
        let mut min = min;
        let mut max = max;
        let span = (max - min).abs().max(Vec2::splat(1.0));
        let padding = span * 0.08 + Vec2::splat(4.0);
        min -= padding;
        max += padding;
        Self {
            min,
            max,
            width,
            height,
        }
    }

    fn point(self, point: Vec2) -> [f32; 2] {
        let span = (self.max - self.min).max(Vec2::splat(1.0));
        [
            (point.x - self.min.x) / span.x * (self.width.saturating_sub(1) as f32),
            (point.y - self.min.y) / span.y * (self.height.saturating_sub(1) as f32),
        ]
    }

    fn line(self, pixels: &mut [u8], start: Vec2, end: Vec2, color: [u8; 4], radius: i32) {
        let start = self.point(start);
        let end = self.point(end);
        let distance = Vec2::new(end[0] - start[0], end[1] - start[1]).length();
        let steps = distance.ceil().max(1.0) as usize;
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            let x = start[0] + (end[0] - start[0]) * t;
            let y = start[1] + (end[1] - start[1]) * t;
            disc(pixels, self.width, self.height, x, y, radius, color);
        }
    }

    fn border_rect(self, pixels: &mut [u8], min: Vec2, max: Vec2, color: [u8; 4], radius: i32) {
        self.line(pixels, min, Vec2::new(max.x, min.y), color, radius);
        self.line(pixels, Vec2::new(max.x, min.y), max, color, radius);
        self.line(pixels, max, Vec2::new(min.x, max.y), color, radius);
        self.line(pixels, Vec2::new(min.x, max.y), min, color, radius);
    }

    fn fill_rect(&self, pixels: &mut [u8], min: Vec2, max: Vec2, color: [u8; 4]) {
        let a = self.point(min);
        let b = self.point(max);
        let left = a[0].min(b[0]).floor() as i32;
        let right = a[0].max(b[0]).ceil() as i32;
        let top = a[1].min(b[1]).floor() as i32;
        let bottom = a[1].max(b[1]).ceil() as i32;
        for y in top..=bottom {
            for x in left..=right {
                put(pixels, self.width, self.height, x, y, color);
            }
        }
    }
}

fn draw_object(
    pixels: &mut [u8],
    map: &Map,
    object: &CadObject,
    selection: Option<ElectronicsSelection>,
) {
    let selected = selection.is_some_and(|selection| {
        object.source_id == Some(selection.source_id)
            && matches!(
                selection.kind,
                crate::electronics_controller::ElectronicsSelectionKind::Component
                    | crate::electronics_controller::ElectronicsSelectionKind::Pin
            )
    });
    match object.kind {
        CadObjectKind::Component => {
            if let Some(rect) = object.rect {
                let color = if selected {
                    COMPONENT_SELECTED
                } else {
                    COMPONENT
                };
                map.fill_rect(
                    pixels,
                    rect.center - rect.size.abs() * 0.5,
                    rect.center + rect.size.abs() * 0.5,
                    color,
                );
            }
        }
        CadObjectKind::Pin | CadObjectKind::Pad => {
            if let Some(rect) = object.rect {
                map.fill_rect(
                    pixels,
                    rect.center - rect.size.abs() * 0.5,
                    rect.center + rect.size.abs() * 0.5,
                    if selected { COMPONENT_SELECTED } else { PIN },
                );
            }
        }
        CadObjectKind::Wire => draw_paths(pixels, map, object, WIRE, 2),
        CadObjectKind::Trace => draw_paths(pixels, map, object, TRACE, 3),
        CadObjectKind::Airwire => draw_paths(pixels, map, object, AIRWIRE, 1),
        CadObjectKind::BoardOutline => draw_paths(pixels, map, object, BORDER, 2),
        CadObjectKind::DrcMarker => {
            if let Some(rect) = object.rect {
                let center = map.point(rect.center);
                disc(pixels, map.width, map.height, center[0], center[1], 4, DRC);
            }
        }
        CadObjectKind::NetLabel => {}
    }
}

fn draw_paths(pixels: &mut [u8], map: &Map, object: &CadObject, color: [u8; 4], radius: i32) {
    for path in object
        .points
        .windows(2)
        .map(|segment| (segment[0], segment[1]))
        .chain(
            object
                .line_paths
                .iter()
                .flat_map(|path| path.windows(2).map(|segment| (segment[0], segment[1]))),
        )
    {
        map.line(pixels, path.0, path.1, color, radius);
    }
}

fn overview_bounds(scene: &CadScene) -> Option<(Vec2, Vec2)> {
    let mut bounds: Option<(Vec2, Vec2)> = None;
    for object in &scene.objects {
        if let Some(rect) = object.rect {
            include(&mut bounds, rect.center - rect.size.abs() * 0.5);
            include(&mut bounds, rect.center + rect.size.abs() * 0.5);
        }
        for point in &object.points {
            include(&mut bounds, *point);
        }
        for path in &object.line_paths {
            for point in path {
                include(&mut bounds, *point);
            }
        }
    }
    bounds
}

fn include(bounds: &mut Option<(Vec2, Vec2)>, point: Vec2) {
    if let Some((min, max)) = bounds {
        *min = min.min(point);
        *max = max.max(point);
    } else {
        *bounds = Some((point, point));
    }
}

fn fill(pixels: &mut [u8], color: [u8; 4]) {
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.copy_from_slice(&color);
    }
}

fn draw_frame(pixels: &mut [u8], width: usize, height: usize, color: [u8; 4]) {
    for x in 0..width {
        put(pixels, width, height, x as i32, 0, color);
        put(pixels, width, height, x as i32, height as i32 - 1, color);
    }
    for y in 0..height {
        put(pixels, width, height, 0, y as i32, color);
        put(pixels, width, height, width as i32 - 1, y as i32, color);
    }
}

fn disc(
    pixels: &mut [u8],
    width: usize,
    height: usize,
    center_x: f32,
    center_y: f32,
    radius: i32,
    color: [u8; 4],
) {
    let radius = radius.max(0);
    for y in -radius..=radius {
        for x in -radius..=radius {
            if x * x + y * y <= radius * radius {
                put(
                    pixels,
                    width,
                    height,
                    center_x.round() as i32 + x,
                    center_y.round() as i32 + y,
                    color,
                );
            }
        }
    }
}

fn put(pixels: &mut [u8], width: usize, height: usize, x: i32, y: i32, color: [u8; 4]) {
    if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
        return;
    }
    let index = (y as usize * width + x as usize) * 4;
    let source_alpha = f32::from(color[3]) / 255.0;
    let destination_alpha = f32::from(pixels[index + 3]) / 255.0;
    let output_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
    if output_alpha <= f32::EPSILON {
        return;
    }
    for channel in 0..3 {
        let source = f32::from(color[channel]) * source_alpha;
        let destination =
            f32::from(pixels[index + channel]) * destination_alpha * (1.0 - source_alpha);
        pixels[index + channel] = ((source + destination) / output_alpha).round() as u8;
    }
    pixels[index + 3] = (output_alpha * 255.0).round() as u8;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_scene_still_produces_a_framed_overview() {
        let (size, pixels) = build_rgba(
            &CadScene::from_schematic(&raf_electronics::Schematic::new("test")),
            CadCamera::default(),
            Vec2::new(640.0, 480.0),
            None,
        );
        assert_eq!(size, IMAGE_SIZE);
        assert_eq!(
            pixels.len(),
            IMAGE_SIZE[0] as usize * IMAGE_SIZE[1] as usize * 4
        );
        assert_eq!(&pixels[..4], &BORDER);
    }
}
