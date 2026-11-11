//! Fine overview reconstruction of the live Electronics CAD document.
//!
//! The minimap is a raster preview of the same `CadScene` and `CadCamera`
//! used by the main canvas. It reconstructs the full board/design extent,
//! layers connectivity under component bodies, and overlays the current
//! viewport rectangle so navigation always has spatial context. It never
//! mutates the document and never becomes a second schematic renderer;
//! symbol strokes and physical geometry remain owned by the main CAD scene.

use glam::Vec2;
use raf_electronics::{
    cad_scene::cad_object_distance, CadObject, CadObjectKind, CadScene, CadSurfaceKind,
};
use raf_ui::{StudioUiPalette, UiTokens};

use crate::electronics_controller::{CadCamera, ElectronicsSelection, ElectronicsSelectionKind};

pub const IMAGE_KEY: &str = "electronics://minimap";
// Render at ~3.5x the presented image width so thin nets, outlines and the
// viewport frame stay crisp on high-density displays. The retained image is
// still presented inside a compact panel, so this does not change layout.
pub const IMAGE_SIZE: [u32; 2] = [880, 560];

/// Preview the same physical shapes used by the board and picking, preserving
/// pad rotations, oblong bores and offset origins. No box approximation.
pub fn footprint_preview_rgba(
    asset: &raf_electronics::pcb::FootprintAsset,
    palette: StudioUiPalette,
) -> ([u32; 2], Vec<u8>) {
    use raf_electronics::pcb::{PcbComponentPlacement, PcbLayer, PcbLayout};
    let size = [640, 300];
    let mut layout = PcbLayout::new("preview");
    layout.board_outline.points.clear();
    layout.components.push(PcbComponentPlacement {
        component_id: uuid::Uuid::nil(),
        designator: String::new(),
        value: String::new(),
        footprint: asset.id.clone(),
        position: Vec2::ZERO,
        rotation: 0.0,
        layer: PcbLayer::TopCopper,
        locked: false,
        image_asset: None,
        pad_nets: vec![String::new(); asset.pads.len()],
    });
    layout
        .footprint_assets
        .insert(asset.id.clone(), asset.clone());
    let scene = CadScene::from_pcb(&layout);
    let bounds = asset.bounds_mm();
    let (min, max) = bounds
        .map(|b| {
            (
                Vec2::new(b.min.x as f32, b.min.y as f32),
                Vec2::new(b.max.x as f32, b.max.y as f32),
            )
        })
        .unwrap_or((Vec2::splat(-1.0), Vec2::ONE));
    let center = (min + max) * 0.5;
    let span = (max - min).max(Vec2::splat(0.1)) * 1.85;
    let aspect = size[0] as f32 / size[1] as f32;
    let span = Vec2::new(span.x.max(span.y * aspect), span.y.max(span.x / aspect));
    let map = Map {
        min: center - span * 0.5,
        max: center + span * 0.5,
        width: size[0] as usize,
        height: size[1] as usize,
    };
    let colors = MinimapColors::from_palette(palette);
    let mut pixels = vec![0; size[0] as usize * size[1] as usize * 4];
    fill(&mut pixels, colors.background);
    draw_background_grid(&mut pixels, &map, with_alpha(colors.grid, 38));
    map.line(
        &mut pixels,
        Vec2::new(map.min.x, 0.0),
        Vec2::new(map.max.x, 0.0),
        with_alpha(colors.component_stroke, 64),
        0,
    );
    map.line(
        &mut pixels,
        Vec2::new(0.0, map.min.y),
        Vec2::new(0.0, map.max.y),
        with_alpha(colors.component_stroke, 64),
        0,
    );
    for object in &scene.objects {
        if matches!(
            object.kind,
            CadObjectKind::Pad | CadObjectKind::Trace | CadObjectKind::BoardOutline
        ) {
            draw_object(&mut pixels, &map, colors, object, None);
        } else if object.kind == CadObjectKind::Component && !object.line_paths.is_empty() {
            draw_paths(&mut pixels, &map, object, colors.component_stroke, 1);
        }
    }
    (size, pixels)
}

/// Chrome header above the raster preview inside the minimap panel.
pub const HEADER_HEIGHT: f32 = 28.0;
const PANEL_WIDTH: f32 = 184.0;
const PANEL_MARGIN: f32 = 12.0;
const PANEL_PAD: f32 = 6.0;

/// Raster palette of the overview, resolved from the active theme tokens.
///
/// The minimap used to own twelve literals derived from the dark theme, so the
/// panel stayed black in the light theme. Every entry is now a semantic token
/// (or a token with an explicit alpha), which keeps both themes readable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MinimapColors {
    pub background: [u8; 4],
    pub grid: [u8; 4],
    pub border: [u8; 4],
    pub viewport_fill: [u8; 4],
    pub viewport: [u8; 4],
    pub component_stroke: [u8; 4],
    pub pin: [u8; 4],
    pub wire: [u8; 4],
    pub airwire: [u8; 4],
    pub drc: [u8; 4],
    pub selected_stroke: [u8; 4],
    pub selection_fill: [u8; 4],
}

/// Alpha applied to the overview grid lines, in 0..255.
const GRID_ALPHA: u8 = 160;
/// Alpha applied to the translucent viewport rectangle, in 0..255.
const VIEWPORT_FILL_ALPHA: u8 = 28;
/// Alpha applied to the airwire stroke, in 0..255.
const AIRWIRE_ALPHA: u8 = 170;
/// Alpha applied to the component outline, in 0..255.
const COMPONENT_STROKE_ALPHA: u8 = 220;
/// Minimum alpha of a pin marker, in 0..255.
const PIN_MIN_ALPHA: u8 = 220;

fn with_alpha(color: [u8; 4], alpha: u8) -> [u8; 4] {
    [color[0], color[1], color[2], alpha]
}

impl MinimapColors {
    /// Resolves the overview palette from the semantic tokens of a palette.
    pub fn from_palette(palette: StudioUiPalette) -> Self {
        Self::from_tokens(&palette.tokens())
    }

    /// Resolves the overview palette from an explicit token set.
    pub fn from_tokens(tokens: &UiTokens) -> Self {
        Self {
            background: tokens.canvas,
            grid: with_alpha(tokens.border, GRID_ALPHA),
            border: tokens.border,
            viewport_fill: with_alpha(tokens.accent, VIEWPORT_FILL_ALPHA),
            viewport: tokens.accent,
            component_stroke: with_alpha(tokens.background, COMPONENT_STROKE_ALPHA),
            pin: tokens.text,
            wire: tokens.text,
            airwire: with_alpha(tokens.text_muted, AIRWIRE_ALPHA),
            drc: tokens.danger,
            selected_stroke: tokens.accent_hot,
            selection_fill: tokens.selection,
        }
    }
}

/// CPU convenience capture, including the camera frame. The live editor uses
/// cached `build_scene_rgba` ink and a cheap retained viewport rectangle.
pub fn build_rgba(
    colors: MinimapColors,
    scene: &CadScene,
    camera: CadCamera,
    canvas_size: Vec2,
    selection: Option<ElectronicsSelection>,
) -> ([u32; 2], Vec<u8>) {
    let (size, mut pixels) = build_scene_rgba(colors, scene, selection);
    let map = overview_map(scene, camera, canvas_size);
    let viewport = camera.world_bounds(canvas_size.max(Vec2::ONE));
    map.fill_rect(
        &mut pixels,
        Vec2::new(viewport[0], viewport[2]),
        Vec2::new(viewport[1], viewport[3]),
        colors.viewport_fill,
    );
    map.border_rect(
        &mut pixels,
        Vec2::new(viewport[0], viewport[2]),
        Vec2::new(viewport[1], viewport[3]),
        colors.viewport,
        2,
    );
    draw_frame(
        &mut pixels,
        size[0] as usize,
        size[1] as usize,
        colors.border,
    );
    (size, pixels)
}

/// Camera-independent document ink. The overview keeps one stable world map,
/// so a minimap drag cannot move its own coordinate system under the pointer.
pub fn build_scene_rgba(
    colors: MinimapColors,
    scene: &CadScene,
    selection: Option<ElectronicsSelection>,
) -> ([u32; 2], Vec<u8>) {
    let width = IMAGE_SIZE[0] as usize;
    let height = IMAGE_SIZE[1] as usize;
    let mut pixels = vec![0_u8; width * height * 4];
    fill(&mut pixels, colors.background);

    let map = overview_map(scene, CadCamera::default(), Vec2::new(640.0, 480.0));

    draw_background_grid(&mut pixels, &map, colors.grid);

    // Connectivity first so component bodies sit above nets, matching the
    // canvas paint order without inventing geometry the document lacks.
    for object in &scene.objects {
        if object.id.ends_with("preview") {
            continue;
        }
        if matches!(
            object.kind,
            CadObjectKind::BoardOutline
                | CadObjectKind::Airwire
                | CadObjectKind::Wire
                | CadObjectKind::Trace
        ) {
            draw_object(&mut pixels, &map, colors, object, selection);
        }
    }
    for object in &scene.objects {
        if object.id.ends_with("preview") {
            continue;
        }
        if matches!(
            object.kind,
            CadObjectKind::Component | CadObjectKind::NetLabel
        ) {
            draw_object(&mut pixels, &map, colors, object, selection);
        }
    }
    for object in &scene.objects {
        if object.id.ends_with("preview") {
            continue;
        }
        if matches!(
            object.kind,
            CadObjectKind::Pin | CadObjectKind::Pad | CadObjectKind::DrcMarker
        ) {
            draw_object(&mut pixels, &map, colors, object, selection);
        }
    }

    draw_frame(&mut pixels, width, height, colors.border);

    (IMAGE_SIZE, pixels)
}

/// The image store owns pixels; this cache owns only the identity of its ink.
/// Camera/hover updates return None and therefore cause no raster or upload.
#[derive(Default)]
pub struct RasterCache {
    key: Option<((CadSurfaceKind, u64, u64), StudioUiPalette)>,
}

impl RasterCache {
    pub fn invalidate(&mut self) {
        self.key = None;
    }

    pub fn update(
        &mut self,
        revision: (CadSurfaceKind, u64, u64),
        palette: StudioUiPalette,
        scene: &CadScene,
        selection: Option<ElectronicsSelection>,
    ) -> Option<([u32; 2], Vec<u8>)> {
        let key = (revision, palette);
        if self.key == Some(key) {
            return None;
        }
        let image = build_scene_rgba(MinimapColors::from_palette(palette), scene, selection);
        self.key = Some(key);
        Some(image)
    }
}

fn overview_map(scene: &CadScene, camera: CadCamera, canvas_size: Vec2) -> Map {
    let (min, max) = overview_bounds(scene, camera, canvas_size.max(Vec2::ONE));
    Map::new(min, max, IMAGE_SIZE[0] as usize, IMAGE_SIZE[1] as usize)
}

/// Canvas-local visible portion of the camera, clipped to the overview image.
pub fn viewport_rect(
    scene: &CadScene,
    camera: CadCamera,
    canvas_size: Vec2,
) -> Option<raf_ui::UiRect> {
    let image = visible_rects(scene, canvas_size)?.image;
    let map = overview_map(scene, camera, canvas_size);
    let view = camera.world_bounds(canvas_size.max(Vec2::ONE));
    let span = map.max - map.min;
    let min = ((Vec2::new(view[0], view[2]) - map.min) / span).max(Vec2::ZERO);
    let max = ((Vec2::new(view[1], view[3]) - map.min) / span).min(Vec2::ONE);
    if min.x >= max.x || min.y >= max.y {
        return None;
    }
    Some(raf_ui::UiRect::new(
        image.x + min.x * image.width,
        image.y + min.y * image.height,
        (max.x - min.x) * image.width,
        (max.y - min.y) * image.height,
    ))
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
        let center = (min + max) * 0.5;
        let span = max - min;
        let aspect = width.saturating_sub(1).max(1) as f32 / height.saturating_sub(1).max(1) as f32;
        let span = Vec2::new(span.x.max(span.y * aspect), span.y.max(span.x / aspect));
        min = center - span * 0.5;
        max = center + span * 0.5;
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

    fn world(self, pixel: Vec2) -> Vec2 {
        let span = (self.max - self.min).max(Vec2::splat(1.0));
        let extent = Vec2::new(
            self.width.saturating_sub(1).max(1) as f32,
            self.height.saturating_sub(1).max(1) as f32,
        );
        self.min + pixel / extent * span
    }

    fn fill_cad_geometry(
        self,
        pixels: &mut [u8],
        object: &CadObject,
        fill: [u8; 4],
        stroke: [u8; 4],
    ) {
        let Some(rect) = object.rect else {
            return;
        };
        let half = rect.size.abs() * 0.5;
        let rotation = object.rotation_degrees.to_radians();
        let (sin, cos) = rotation.sin_cos();
        let extent = Vec2::new(
            cos.abs() * half.x + sin.abs() * half.y,
            sin.abs() * half.x + cos.abs() * half.y,
        );
        let min = self.point(rect.center - extent);
        let max = self.point(rect.center + extent);
        let left = (min[0].min(max[0]).floor() as i32 - 1).max(0);
        let right = (min[0].max(max[0]).ceil() as i32 + 1).min(self.width as i32 - 1);
        let top = (min[1].min(max[1]).floor() as i32 - 1).max(0);
        let bottom = (min[1].max(max[1]).ceil() as i32 + 1).min(self.height as i32 - 1);
        let pixel_step = (self.max - self.min).abs()
            / Vec2::new(
                self.width.saturating_sub(1).max(1) as f32,
                self.height.saturating_sub(1).max(1) as f32,
            );

        // Rectangles at quarter turns need no per-sample rotation, distance
        // square roots or four further shape probes. Drills/curves keep the
        // authoritative CAD distance path below.
        if object.shape == raf_electronics::cad_scene::CadShape::Rectangle
            && object.drill.is_none()
            && object.rotation_degrees.is_finite()
            && object.rotation_degrees.rem_euclid(90.0) < 0.0001
        {
            let lo = rect.center - extent;
            let hi = rect.center + extent;
            for y in top..=bottom {
                let wy = self.min.y + (y as f32 + 0.5) * pixel_step.y;
                if wy < lo.y || wy > hi.y {
                    continue;
                }
                for x in left..=right {
                    let wx = self.min.x + (x as f32 + 0.5) * pixel_step.x;
                    if wx < lo.x || wx > hi.x {
                        continue;
                    }
                    let edge = wx - pixel_step.x < lo.x
                        || wx + pixel_step.x > hi.x
                        || wy - pixel_step.y < lo.y
                        || wy + pixel_step.y > hi.y;
                    put(
                        pixels,
                        self.width,
                        self.height,
                        x,
                        y,
                        if edge { stroke } else { fill },
                    );
                }
            }
            return;
        }

        for y in top..=bottom {
            for x in left..=right {
                let world = self.world(Vec2::new(x as f32 + 0.5, y as f32 + 0.5));
                if cad_object_distance(object, world) > 0.0 {
                    continue;
                }
                let edge = [
                    Vec2::new(pixel_step.x, 0.0),
                    Vec2::new(-pixel_step.x, 0.0),
                    Vec2::new(0.0, pixel_step.y),
                    Vec2::new(0.0, -pixel_step.y),
                ]
                .into_iter()
                .any(|offset| cad_object_distance(object, world + offset) > 0.0);
                let color = if edge { stroke } else { fill };
                if color[3] != 0 {
                    put(pixels, self.width, self.height, x, y, color);
                }
            }
        }
    }

    fn line(self, pixels: &mut [u8], start: Vec2, end: Vec2, color: [u8; 4], radius: i32) {
        let start = self.point(start);
        let end = self.point(end);
        let Some((start, end)) = clip_pixel_line(
            Vec2::from(start),
            Vec2::from(end),
            Vec2::splat(-(radius.max(0) as f32)),
            Vec2::new(
                self.width as f32 + radius.max(0) as f32,
                self.height as f32 + radius.max(0) as f32,
            ),
        ) else {
            return;
        };
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
        let left = (a[0].min(b[0]).floor() as i32).max(0);
        let right = (a[0].max(b[0]).ceil() as i32).min(self.width as i32 - 1);
        let top = (a[1].min(b[1]).floor() as i32).max(0);
        let bottom = (a[1].max(b[1]).ceil() as i32).min(self.height as i32 - 1);
        for y in top..=bottom {
            for x in left..=right {
                put(pixels, self.width, self.height, x, y, color);
            }
        }
    }

    fn stroke_rect(self, pixels: &mut [u8], min: Vec2, max: Vec2, color: [u8; 4], radius: i32) {
        self.border_rect(pixels, min, max, color, radius);
    }
}

/// Clip before stepping pixels: off-map geometry must not turn a pan into
/// millions of rejected samples. Expanded bounds preserve thick edge ink.
fn clip_pixel_line(start: Vec2, end: Vec2, min: Vec2, max: Vec2) -> Option<(Vec2, Vec2)> {
    if !start.is_finite() || !end.is_finite() {
        return None;
    }
    let delta = end - start;
    let (mut enter, mut exit) = (0.0_f32, 1.0_f32);
    for (p, q) in [
        (-delta.x, start.x - min.x),
        (delta.x, max.x - start.x),
        (-delta.y, start.y - min.y),
        (delta.y, max.y - start.y),
    ] {
        if p.abs() <= f32::EPSILON {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                enter = enter.max(t);
            } else {
                exit = exit.min(t);
            }
            if enter > exit {
                return None;
            }
        }
    }
    Some((start + delta * enter, start + delta * exit))
}

fn draw_object(
    pixels: &mut [u8],
    map: &Map,
    colors: MinimapColors,
    object: &CadObject,
    selection: Option<ElectronicsSelection>,
) {
    let selected = selection.is_some_and(|selection| {
        object.source_id == Some(selection.source_id)
            && matches!(
                (selection.kind, object.kind),
                (
                    ElectronicsSelectionKind::Component | ElectronicsSelectionKind::Pin,
                    CadObjectKind::Component | CadObjectKind::Pin | CadObjectKind::Pad
                ) | (ElectronicsSelectionKind::Wire, CadObjectKind::Wire)
                    | (ElectronicsSelectionKind::Trace, CadObjectKind::Trace)
            )
    });
    match object.kind {
        CadObjectKind::Component => {
            if object.rect.is_some() {
                let mut fill_color = object.color_rgba;
                fill_color[3] = fill_color[3].saturating_sub(40).max(160);
                if selected {
                    fill_color = colors.selection_fill;
                }
                let stroke = if selected {
                    colors.selected_stroke
                } else {
                    colors.component_stroke
                };
                map.fill_cad_geometry(pixels, object, fill_color, stroke);
            }
        }
        CadObjectKind::Pin | CadObjectKind::Pad => {
            if object.rect.is_some() {
                let (fill, stroke) = if selected {
                    (colors.selection_fill, colors.selected_stroke)
                } else if object.kind == CadObjectKind::Pad
                    && object.drill.is_some()
                    && object.color_rgba[3] == 0
                {
                    ([0, 0, 0, 0], colors.border)
                } else {
                    let fill = pin_color(object, colors);
                    (fill, copper_edge_color(fill))
                };
                map.fill_cad_geometry(pixels, object, fill, stroke);
            }
        }
        CadObjectKind::Wire => {
            let color = if selected {
                colors.selected_stroke
            } else {
                wire_color(object, colors)
            };
            draw_paths(pixels, map, object, color, 2);
        }
        CadObjectKind::Trace => {
            let color = if selected {
                colors.selected_stroke
            } else {
                object.color_rgba
            };
            let width = object
                .stroke_width_world
                .map(|width| {
                    (width * 0.5 * map.width as f32 / (map.max.x - map.min.x).max(0.001))
                        .round()
                        .clamp(0.0, 64.0) as i32
                })
                .unwrap_or(3);
            draw_paths(pixels, map, object, color, width);
        }
        CadObjectKind::Airwire => draw_paths(pixels, map, object, colors.airwire, 1),
        CadObjectKind::BoardOutline => {
            draw_paths(pixels, map, object, outline_color(object, colors), 2);
        }
        CadObjectKind::DrcMarker => {
            if let Some(rect) = object.rect {
                let center = map.point(rect.center);
                disc(
                    pixels,
                    map.width,
                    map.height,
                    center[0],
                    center[1],
                    4,
                    drc_color(object, colors),
                );
            }
        }
        CadObjectKind::NetLabel => {}
    }
}

fn pin_color(object: &CadObject, colors: MinimapColors) -> [u8; 4] {
    if object.color_rgba[3] == 0 {
        colors.pin
    } else {
        let mut color = object.color_rgba;
        if color[0] < 40 && color[1] < 40 && color[2] < 40 {
            return colors.pin;
        }
        color[3] = color[3].max(PIN_MIN_ALPHA);
        color
    }
}

fn wire_color(object: &CadObject, colors: MinimapColors) -> [u8; 4] {
    if object.color_rgba == [0, 0, 0, 0] {
        colors.wire
    } else {
        object.color_rgba
    }
}

fn outline_color(object: &CadObject, colors: MinimapColors) -> [u8; 4] {
    if object.color_rgba == [0, 0, 0, 0] {
        colors.border
    } else {
        object.color_rgba
    }
}

fn drc_color(object: &CadObject, colors: MinimapColors) -> [u8; 4] {
    if object.color_rgba == [0, 0, 0, 0] {
        colors.drc
    } else {
        object.color_rgba
    }
}

fn copper_edge_color(copper: [u8; 4]) -> [u8; 4] {
    [
        copper[0].saturating_add(34),
        copper[1].saturating_add(29),
        copper[2].saturating_add(22),
        copper[3],
    ]
}

fn draw_background_grid(pixels: &mut [u8], map: &Map, grid: [u8; 4]) {
    const CELLS_X: usize = 10;
    const CELLS_Y: usize = 7;
    for cell in 1..CELLS_X {
        let x = (cell as f32 / CELLS_X as f32 * map.width.saturating_sub(1) as f32).round() as i32;
        for y in 0..map.height as i32 {
            put(pixels, map.width, map.height, x, y, grid);
        }
    }
    for cell in 1..CELLS_Y {
        let y = (cell as f32 / CELLS_Y as f32 * map.height.saturating_sub(1) as f32).round() as i32;
        for x in 0..map.width as i32 {
            put(pixels, map.width, map.height, x, y, grid);
        }
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

/// Fixed document extent. Only an empty-document capture uses the viewport.
fn overview_bounds(scene: &CadScene, camera: CadCamera, canvas_size: Vec2) -> (Vec2, Vec2) {
    let viewport = camera.world_bounds(canvas_size.max(Vec2::ONE));
    let viewport_min = Vec2::new(viewport[0], viewport[2]);
    let viewport_max = Vec2::new(viewport[1], viewport[3]);
    match content_bounds(scene) {
        Some(bounds) => bounds,
        None => (viewport_min, viewport_max),
    }
}

/// Whether the document has any navigable content worth framing.
///
/// Previews, net labels and DRC markers alone never count as content; they
/// are transient chrome on top of an otherwise empty design.
pub fn has_document_content(scene: &CadScene) -> bool {
    content_bounds(scene).is_some()
}

fn content_bounds(scene: &CadScene) -> Option<(Vec2, Vec2)> {
    let mut bounds: Option<(Vec2, Vec2)> = None;
    for object in &scene.objects {
        if object.id.ends_with("preview")
            || matches!(
                object.kind,
                CadObjectKind::NetLabel | CadObjectKind::DrcMarker
            )
        {
            continue;
        }
        if let Some(rect) = object.rect {
            let half = rect.size.abs() * 0.5;
            let rotation = object.rotation_degrees.to_radians();
            let (sin, cos) = rotation.sin_cos();
            let extent = Vec2::new(
                cos.abs() * half.x + sin.abs() * half.y,
                sin.abs() * half.x + cos.abs() * half.y,
            );
            include(&mut bounds, rect.center - extent);
            include(&mut bounds, rect.center + extent);
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

/// The exact minimap bounds shared by painting and pointer routing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MinimapRects {
    pub panel: raf_ui::UiRect,
    pub image: raf_ui::UiRect,
}

/// Returns no bounds until the document has navigable content. The UI and
/// controller both consume this helper, so hidden empty maps cannot steal input.
pub fn visible_rects(scene: &CadScene, size: Vec2) -> Option<MinimapRects> {
    visible_rects_with_state(scene, size, false)
}

pub fn visible_rects_with_state(
    scene: &CadScene,
    size: Vec2,
    collapsed: bool,
) -> Option<MinimapRects> {
    if !has_document_content(scene)
        || size.x < PANEL_MARGIN * 2.0 + PANEL_PAD * 2.0 + 8.0
        || size.y < PANEL_MARGIN * 2.0 + HEADER_HEIGHT + PANEL_PAD + 8.0
    {
        return None;
    }
    let mut panel = overlay_rect(size);
    let mut image = image_rect(size);
    if collapsed {
        panel.height = HEADER_HEIGHT;
        image = raf_ui::UiRect::new(panel.x, panel.y + HEADER_HEIGHT, 0.0, 0.0);
    }
    Some(MinimapRects { panel, image })
}

/// Full panel rectangle including the chrome header.
fn overlay_rect(size: Vec2) -> raf_ui::UiRect {
    let max_width = (size.x - PANEL_MARGIN * 2.0).max(1.0);
    let max_height = (size.y - PANEL_MARGIN * 2.0).max(1.0);
    let image_height_limit = (max_height - HEADER_HEIGHT - PANEL_PAD).max(1.0);
    let width_from_height =
        image_height_limit * IMAGE_SIZE[0] as f32 / IMAGE_SIZE[1] as f32 + PANEL_PAD * 2.0;
    let width = PANEL_WIDTH.min(max_width).min(width_from_height).max(1.0);
    let image_h = (width - PANEL_PAD * 2.0).max(1.0) * IMAGE_SIZE[1] as f32 / IMAGE_SIZE[0] as f32;
    let height = HEADER_HEIGHT + image_h + PANEL_PAD;
    raf_ui::UiRect::new(
        (size.x - width - PANEL_MARGIN).max(0.0),
        PANEL_MARGIN.min((size.y - height).max(0.0)),
        width,
        height,
    )
}

/// Inner raster image rectangle below the chrome header. World mapping and
/// image placement both use this rect so drag navigation stays pixel-true.
fn image_rect(size: Vec2) -> raf_ui::UiRect {
    let panel = overlay_rect(size);
    let width = (panel.width - PANEL_PAD * 2.0).max(1.0);
    let height = width * IMAGE_SIZE[1] as f32 / IMAGE_SIZE[0] as f32;
    raf_ui::UiRect::new(panel.x + PANEL_PAD, panel.y + HEADER_HEIGHT, width, height)
}

/// Maps a canvas-local pointer position to a world point on the document.
///
/// Uses the same padded, aspect-correct map as the raster and camera frame.
pub fn world_at(
    scene: &CadScene,
    camera: CadCamera,
    canvas_size: Vec2,
    local: Vec2,
) -> Option<Vec2> {
    let canvas_size = canvas_size.max(Vec2::ONE);
    let rect = visible_rects(scene, canvas_size)?.image;
    if rect.width <= f32::EPSILON || rect.height <= f32::EPSILON {
        return None;
    }
    let fraction = ((local - Vec2::new(rect.x, rect.y)) / Vec2::new(rect.width, rect.height))
        .clamp(Vec2::ZERO, Vec2::ONE);
    let map = overview_map(scene, camera, canvas_size);
    Some(map.min + fraction * (map.max - map.min))
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
    if color[3] == 0 {
        return;
    }
    if color[3] == 255 {
        pixels[index..index + 4].copy_from_slice(&color);
        return;
    }
    if pixels[index + 3] == 255 {
        let alpha = u32::from(color[3]);
        for channel in 0..3 {
            pixels[index + channel] = ((u32::from(color[channel]) * alpha
                + u32::from(pixels[index + channel]) * (255 - alpha)
                + 127)
                / 255) as u8;
        }
        return;
    }
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

/// Surface label key for the minimap chrome header badge.
pub fn surface_label_key(surface: CadSurfaceKind) -> &'static str {
    match surface {
        CadSurfaceKind::Schematic => "electronics.toolbar.schematic",
        CadSurfaceKind::Pcb => "electronics.toolbar.pcb",
    }
}

/// Tooltip key of the minimap panel for one surface.
///
/// The panel is chrome, not a document list: a single "board overview" string
/// told a schematic user they were looking at a board.
pub fn minimap_tooltip_key(surface: CadSurfaceKind) -> &'static str {
    match surface {
        CadSurfaceKind::Schematic => "electronics.minimap.tooltip_schematic",
        CadSurfaceKind::Pcb => "electronics.minimap.tooltip_pcb",
    }
}

/// Empty-state key of the minimap panel for one surface.
pub fn minimap_empty_key(surface: CadSurfaceKind) -> &'static str {
    match surface {
        CadSurfaceKind::Schematic => "electronics.minimap.empty_schematic",
        CadSurfaceKind::Pcb => "electronics.minimap.empty_pcb",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_electronics::{
        cad_scene::{CadDrill, CadDrillShape, CadRect, CadShape},
        ElectronicComponent, Schematic,
    };
    use std::collections::HashMap;

    fn colors() -> MinimapColors {
        MinimapColors::from_palette(StudioUiPalette::IndustrialDark)
    }

    #[test]
    fn electronics_pan_zoom_and_hover_reuse_overview_pixels() {
        use crate::electronics_controller::NativeElectronicsEditor;
        let mut editor = NativeElectronicsEditor::empty("overview cache");
        editor
            .schematic
            .add_component(ElectronicComponent::resistor("10k"));
        editor.rebuild_scene();
        editor.select_component(0);
        let mut cache = RasterCache::default();
        let palette = StudioUiPalette::IndustrialDark;
        assert!(cache
            .update(
                editor.overview_revision(),
                palette,
                editor.scene(),
                editor.selection()
            )
            .is_some());
        let original = editor.overview_revision();
        for i in 0..120 {
            editor.zoom_in();
            editor.set_hovered(if i % 2 == 0 { editor.selection() } else { None });
            assert_eq!(editor.overview_revision(), original);
            assert!(cache
                .update(
                    editor.overview_revision(),
                    palette,
                    editor.scene(),
                    editor.selection()
                )
                .is_none());
        }
        editor.select_component(0);
        assert!(cache
            .update(
                editor.overview_revision(),
                palette,
                editor.scene(),
                editor.selection()
            )
            .is_some());
        assert!(cache
            .update(
                editor.overview_revision(),
                StudioUiPalette::PaperLight,
                editor.scene(),
                editor.selection()
            )
            .is_some());
        cache.invalidate();
        assert!(cache
            .update(
                editor.overview_revision(),
                StudioUiPalette::PaperLight,
                editor.scene(),
                editor.selection()
            )
            .is_some());
    }

    #[test]
    fn minimap_coordinates_stay_fixed_during_pan_and_include_raster_padding() {
        let mut schematic = Schematic::new("fixed overview");
        let mut resistor = ElectronicComponent::resistor("1k");
        resistor.position = Vec2::new(100.0, 50.0);
        schematic.add_component(resistor);
        let scene = CadScene::from_schematic(&schematic);
        let size = Vec2::new(900.0, 600.0);
        let image = visible_rects(&scene, size).unwrap().image;
        let map = overview_map(&scene, CadCamera::default(), size);
        let world = Vec2::new(120.0, 55.0);
        let pixel = Vec2::from(map.point(world));
        let fraction = pixel / Vec2::new((IMAGE_SIZE[0] - 1) as f32, (IMAGE_SIZE[1] - 1) as f32);
        let point = Vec2::new(image.x, image.y) + fraction * Vec2::new(image.width, image.height);
        for camera in [
            CadCamera::default(),
            CadCamera {
                center: Vec2::new(-1000.0, 2000.0),
                zoom: 10.0,
            },
        ] {
            assert!(
                world_at(&scene, camera, size, point)
                    .unwrap()
                    .distance(world)
                    < 0.001
            );
        }
    }

    #[test]
    fn offscreen_minimap_ink_is_clipped_before_raster_work() {
        let min = Vec2::ZERO;
        let max = Vec2::new(880.0, 560.0);
        let (a, b) = clip_pixel_line(
            Vec2::new(-1_000_000.0, 200.0),
            Vec2::new(1_000_000.0, 200.0),
            min,
            max,
        )
        .unwrap();
        assert!(a.x >= -0.1 && b.x <= 880.1);
        assert!(a.distance(b) <= 881.0);
        assert!(
            clip_pixel_line(Vec2::new(-1e6, -100.0), Vec2::new(1e6, -100.0), min, max).is_none()
        );
    }

    #[test]
    fn minimap_copy_follows_the_active_surface() {
        assert_eq!(
            surface_label_key(CadSurfaceKind::Schematic),
            "electronics.toolbar.schematic"
        );
        assert_eq!(
            surface_label_key(CadSurfaceKind::Pcb),
            "electronics.toolbar.pcb"
        );
        assert_ne!(
            minimap_tooltip_key(CadSurfaceKind::Schematic),
            minimap_tooltip_key(CadSurfaceKind::Pcb),
            "a schematic panel must not describe itself as a board"
        );
        assert_ne!(
            minimap_empty_key(CadSurfaceKind::Schematic),
            minimap_empty_key(CadSurfaceKind::Pcb)
        );
    }

    #[test]
    fn every_minimap_key_exists_in_both_catalogs() {
        let catalogs: [(&str, HashMap<String, String>); 2] = [
            (
                "en",
                serde_json::from_str(include_str!("../../raf_core/locales/en.json"))
                    .expect("the English catalog is valid JSON"),
            ),
            (
                "es",
                serde_json::from_str(include_str!("../../raf_core/locales/es.json"))
                    .expect("the Spanish catalog is valid JSON"),
            ),
        ];
        for surface in [CadSurfaceKind::Schematic, CadSurfaceKind::Pcb] {
            let keys = [
                surface_label_key(surface),
                minimap_tooltip_key(surface),
                minimap_empty_key(surface),
            ];
            for (language, catalog) in &catalogs {
                for key in keys {
                    assert!(
                        catalog.contains_key(key),
                        "{language}.json is missing {key}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_empty_scene_has_no_document_content() {
        let scene = CadScene::from_schematic(&Schematic::new("test"));
        assert!(!has_document_content(&scene));
    }

    #[test]
    fn the_overview_palette_follows_the_active_theme() {
        let dark = MinimapColors::from_palette(StudioUiPalette::IndustrialDark);
        let light = MinimapColors::from_palette(StudioUiPalette::PaperLight);
        assert_ne!(dark.background, light.background);
        assert_ne!(dark.pin, light.pin);
        assert_eq!(
            light.background,
            StudioUiPalette::PaperLight.tokens().canvas
        );
    }

    #[test]
    fn no_overview_color_is_a_hand_mixed_literal() {
        for palette in [StudioUiPalette::IndustrialDark, StudioUiPalette::PaperLight] {
            let tokens = palette.tokens();
            let colors = MinimapColors::from_tokens(&tokens);
            assert_eq!(colors.viewport, tokens.accent);
            assert_eq!(colors.selected_stroke, tokens.accent_hot);
            assert_eq!(colors.drc, tokens.danger);
            assert_eq!(colors.border, tokens.border);
            assert_eq!(colors.grid, with_alpha(tokens.border, GRID_ALPHA));
        }
    }

    #[test]
    fn empty_scene_still_produces_a_framed_overview() {
        let colors = colors();
        let (size, pixels) = build_rgba(
            colors,
            &CadScene::from_schematic(&Schematic::new("test")),
            CadCamera::default(),
            Vec2::new(640.0, 480.0),
            None,
        );
        assert_eq!(size, IMAGE_SIZE);
        assert_eq!(
            pixels.len(),
            IMAGE_SIZE[0] as usize * IMAGE_SIZE[1] as usize * 4
        );
        assert_eq!(&pixels[..4], &colors.border);
    }

    #[test]
    fn empty_minimap_has_no_pointer_target() {
        let camera = CadCamera {
            center: Vec2::new(50.0, 30.0),
            zoom: 1.0,
        };
        let size = Vec2::new(640.0, 480.0);
        let image = image_rect(size);
        let local = Vec2::new(image.x + image.width * 0.5, image.y + image.height * 0.5);
        assert!(world_at(
            &CadScene::from_schematic(&Schematic::new("t")),
            camera,
            size,
            local,
        )
        .is_none());
    }

    #[test]
    fn content_minimap_uses_shared_visible_image_bounds_for_mapping() {
        let mut schematic = Schematic::new("content");
        let mut resistor = ElectronicComponent::resistor("10k");
        resistor.position = Vec2::ZERO;
        schematic.add_component(resistor);
        let scene = CadScene::from_schematic(&schematic);
        let camera = CadCamera::default();
        let size = Vec2::new(640.0, 480.0);
        let rects = visible_rects(&scene, size).expect("content shows the minimap");
        assert_eq!(rects.panel, overlay_rect(size));
        assert_eq!(rects.image, image_rect(size));
        let center = Vec2::new(
            rects.image.x + rects.image.width * 0.5,
            rects.image.y + rects.image.height * 0.5,
        );
        let world = world_at(&scene, camera, size, center).expect("visible image maps");
        assert!(
            (world - camera.center).length() < 4.0,
            "image center should resolve near the camera center, got {world:?}"
        );
    }

    #[test]
    fn image_rect_sits_below_header_inside_panel_and_canvas() {
        let size = Vec2::new(400.0, 300.0);
        let panel = overlay_rect(size);
        let image = image_rect(size);
        assert!(image.x >= panel.x - f32::EPSILON);
        assert!(image.y >= panel.y + HEADER_HEIGHT - f32::EPSILON);
        assert!(image.x + image.width <= panel.x + panel.width + f32::EPSILON);
        assert!(image.y + image.height <= panel.y + panel.height + f32::EPSILON);
        assert!(panel.x + panel.width <= size.x);
        assert!(panel.y + panel.height <= size.y);
    }

    #[test]
    fn component_body_uses_document_color_in_overview() {
        let mut schematic = Schematic::new("colors");
        let mut resistor = ElectronicComponent::resistor("10k");
        resistor.position = Vec2::new(0.0, 0.0);
        resistor.appearance.color = [10, 200, 50, 255];
        schematic.add_component(resistor);

        let camera = CadCamera {
            center: Vec2::ZERO,
            zoom: 1.0,
        };
        let (_, pixels) = build_rgba(
            colors(),
            &CadScene::from_schematic(&schematic),
            camera,
            Vec2::new(640.0, 480.0),
            None,
        );
        // Alpha blending against the overview background shifts the
        // document color slightly, so assert the green-dominant signature of
        // [10, 200, 50] rather than an exact channel match.
        let hits = pixels
            .chunks_exact(4)
            .filter(|pixel| {
                pixel[1] > 80
                    && i32::from(pixel[1]) - i32::from(pixel[0]) > 40
                    && i32::from(pixel[1]) - i32::from(pixel[2]) > 40
                    && pixel[3] > 120
            })
            .count();
        assert!(
            hits > 40,
            "expected the component body color in the overview, found {hits} green-dominant pixels"
        );
    }

    #[test]
    fn minimap_uses_rotated_pad_shape_and_keeps_drill_as_a_gap() {
        let object = CadObject {
            stroke_width_world: None,
            id: "pad:physical".to_string(),
            source_id: None,
            kind: CadObjectKind::Pad,
            layer: raf_electronics::CadLayerKind::PcbTopCopper,
            pick_priority: raf_electronics::CadPickPriority::Pin,
            rect: Some(CadRect::new(Vec2::splat(50.0), Vec2::new(30.0, 10.0))),
            shape: CadShape::Oval,
            rotation_degrees: 45.0,
            drill: Some(CadDrill {
                shape: CadDrillShape::Oval,
                size: Vec2::new(4.0, 8.0),
                offset: Vec2::new(2.0, 0.0),
                plated: true,
            }),
            points: Vec::new(),
            line_paths: Vec::new(),
            label: Some("1".to_string()),
            net: Some("GND".to_string()),
            net_id: Some(0),
            color_rgba: [207, 150, 91, 255],
        };
        let map = Map {
            min: Vec2::ZERO,
            max: Vec2::splat(100.0),
            width: 101,
            height: 101,
        };
        let background = [9, 10, 12, 255];
        let mut pixels = vec![0; 101 * 101 * 4];
        fill(&mut pixels, background);
        map.fill_cad_geometry(
            &mut pixels,
            &object,
            object.color_rgba,
            copper_edge_color(object.color_rgba),
        );

        let at = |x: usize, y: usize| &pixels[(y * 101 + x) * 4..(y * 101 + x) * 4 + 4];
        // Sample the pad interior, away from the highlighted drill rim.
        assert_eq!(at(44, 46), object.color_rgba);
        assert_eq!(
            at(51, 51),
            background,
            "the offset oval drill remains empty"
        );
    }

    #[test]
    fn viewport_fill_is_present_when_camera_looks_at_content() {
        let mut schematic = Schematic::new("vp");
        schematic.add_component(ElectronicComponent::resistor("10k"));
        let camera = CadCamera::default();
        let colors = colors();
        let (_, pixels) = build_rgba(
            colors,
            &CadScene::from_schematic(&schematic),
            camera,
            Vec2::new(640.0, 480.0),
            None,
        );
        // The viewport border color is the accent token at full alpha on the
        // frame path; at least the translucent fill must lift some pixels above
        // the background.
        let warm = pixels
            .chunks_exact(4)
            .filter(|pixel| pixel[0] as i32 - pixel[2] as i32 > 10 && pixel[3] > 40)
            .count();
        assert!(
            warm > 100,
            "expected viewport/content warm pixels, found {warm}"
        );
    }
}
