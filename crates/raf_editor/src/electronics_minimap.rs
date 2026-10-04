//! Fine overview reconstruction of the live Electronics CAD document.
//!
//! The minimap is a raster preview of the same `CadScene` and `CadCamera`
//! used by the main canvas. It reconstructs the full board/design extent,
//! layers connectivity under component bodies, and overlays the current
//! viewport rectangle so navigation always has spatial context. It never
//! mutates the document and never becomes a second schematic renderer;
//! component artwork remains owned by the native PNG catalog on the main
//! RafUI canvas overlay.

use glam::Vec2;
use raf_electronics::{CadObject, CadObjectKind, CadScene, CadSurfaceKind};
use raf_ui::{StudioUiPalette, UiTokens};

use crate::electronics_controller::{CadCamera, ElectronicsSelection, ElectronicsSelectionKind};

pub const IMAGE_KEY: &str = "electronics://minimap";
// Render at ~3.5x the presented image width so thin nets, outlines and the
// viewport frame stay crisp on high-density displays. The retained image is
// still presented inside a compact panel, so this does not change layout.
pub const IMAGE_SIZE: [u32; 2] = [880, 560];

/// Chrome header above the raster preview inside the minimap panel.
pub const HEADER_HEIGHT: f32 = 28.0;
const PANEL_WIDTH: f32 = 240.0;
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

/// Builds the minimap RGBA preview from the live CAD scene and camera.
///
/// Bounds are the union of document content and the visible world rectangle,
/// so the whole design stays visible while the viewport frame remains a
/// meaningful navigation indicator even when the camera leaves the board.
pub fn build_rgba(
    colors: MinimapColors,
    scene: &CadScene,
    camera: CadCamera,
    canvas_size: Vec2,
    selection: Option<ElectronicsSelection>,
) -> ([u32; 2], Vec<u8>) {
    let width = IMAGE_SIZE[0] as usize;
    let height = IMAGE_SIZE[1] as usize;
    let mut pixels = vec![0_u8; width * height * 4];
    fill(&mut pixels, colors.background);

    let canvas_size = canvas_size.max(Vec2::ONE);
    let (min, max) = overview_bounds(scene, camera, canvas_size);
    let map = Map::new(min, max, width, height);

    draw_background_grid(&mut pixels, &map, colors.grid);

    // Connectivity first so component bodies sit above nets, matching the
    // canvas paint order without inventing geometry the document lacks.
    for object in &scene.objects {
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
        if matches!(
            object.kind,
            CadObjectKind::Component | CadObjectKind::NetLabel
        ) {
            draw_object(&mut pixels, &map, colors, object, selection);
        }
    }
    for object in &scene.objects {
        if matches!(
            object.kind,
            CadObjectKind::Pin | CadObjectKind::Pad | CadObjectKind::DrcMarker
        ) {
            draw_object(&mut pixels, &map, colors, object, selection);
        }
    }

    let viewport = camera.world_bounds(canvas_size);
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
    draw_frame(&mut pixels, width, height, colors.border);

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

    fn stroke_rect(self, pixels: &mut [u8], min: Vec2, max: Vec2, color: [u8; 4], radius: i32) {
        self.border_rect(pixels, min, max, color, radius);
    }
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
            if let Some(rect) = object.rect {
                let min = rect.center - rect.size.abs() * 0.5;
                let max = rect.center + rect.size.abs() * 0.5;
                let mut fill_color = object.color_rgba;
                fill_color[3] = fill_color[3].saturating_sub(40).max(160);
                if selected {
                    fill_color = colors.selection_fill;
                }
                map.fill_rect(pixels, min, max, fill_color);
                let stroke = if selected {
                    colors.selected_stroke
                } else {
                    colors.component_stroke
                };
                map.stroke_rect(pixels, min, max, stroke, 1);
            }
        }
        CadObjectKind::Pin | CadObjectKind::Pad => {
            if let Some(rect) = object.rect {
                let min = rect.center - rect.size.abs() * 0.5;
                let max = rect.center + rect.size.abs() * 0.5;
                let color = if selected {
                    colors.selected_stroke
                } else {
                    pin_color(object, colors)
                };
                map.fill_rect(pixels, min, max, color);
                if selected {
                    map.stroke_rect(
                        pixels,
                        min - Vec2::splat(1.0),
                        max + Vec2::splat(1.0),
                        colors.selected_stroke,
                        1,
                    );
                }
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
            draw_paths(pixels, map, object, color, 3);
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

/// Union of content bounds and the camera's visible world rectangle.
///
/// Content keeps the whole board stable while zoomed in. The viewport is
/// folded in so panning away from the design never drops the navigation
/// indicator off the minimap. An empty document falls back to the viewport.
fn overview_bounds(scene: &CadScene, camera: CadCamera, canvas_size: Vec2) -> (Vec2, Vec2) {
    let viewport = camera.world_bounds(canvas_size.max(Vec2::ONE));
    let viewport_min = Vec2::new(viewport[0], viewport[2]);
    let viewport_max = Vec2::new(viewport[1], viewport[3]);
    match content_bounds(scene) {
        Some((min, max)) => (min.min(viewport_min), max.max(viewport_max)),
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

/// Full panel rectangle including the chrome header. Used for pointer
/// routing so clicks on the header never fall through to the CAD canvas.
pub fn overlay_rect(size: Vec2) -> raf_ui::UiRect {
    let max_width = (size.x - PANEL_MARGIN * 2.0).max(1.0);
    let max_height = (size.y - PANEL_MARGIN * 2.0).max(1.0);
    let width = PANEL_WIDTH.min(max_width);
    let image_h = (width - PANEL_PAD * 2.0) * IMAGE_SIZE[1] as f32 / IMAGE_SIZE[0] as f32;
    let height = (HEADER_HEIGHT + image_h + PANEL_PAD).min(max_height);
    raf_ui::UiRect::new(
        (size.x - width - PANEL_MARGIN).max(0.0),
        (size.y - height - PANEL_MARGIN).max(0.0),
        width,
        height,
    )
}

/// Inner raster image rectangle below the chrome header. World mapping and
/// image placement both use this rect so drag navigation stays pixel-true.
pub fn image_rect(size: Vec2) -> raf_ui::UiRect {
    let panel = overlay_rect(size);
    let width = (panel.width - PANEL_PAD * 2.0).max(1.0);
    let height = (width * IMAGE_SIZE[1] as f32 / IMAGE_SIZE[0] as f32)
        .min((panel.height - HEADER_HEIGHT - PANEL_PAD).max(1.0));
    raf_ui::UiRect::new(panel.x + PANEL_PAD, panel.y + HEADER_HEIGHT, width, height)
}

/// Maps a canvas-local pointer position to a world point on the document.
///
/// Uses the same content-union-viewport bounds as [`build_rgba`], so a drag
/// on the minimap lands exactly where the overview paints that pixel.
pub fn world_at(
    scene: &CadScene,
    camera: CadCamera,
    canvas_size: Vec2,
    local: Vec2,
) -> Option<Vec2> {
    let canvas_size = canvas_size.max(Vec2::ONE);
    let rect = image_rect(canvas_size);
    if rect.width <= f32::EPSILON || rect.height <= f32::EPSILON {
        return None;
    }
    let fraction = ((local - Vec2::new(rect.x, rect.y)) / Vec2::new(rect.width, rect.height))
        .clamp(Vec2::ZERO, Vec2::ONE);
    let (min, max) = overview_bounds(scene, camera, canvas_size);
    Some(min + fraction * (max - min))
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
    use raf_electronics::{ElectronicComponent, Schematic};
    use std::collections::HashMap;

    fn colors() -> MinimapColors {
        MinimapColors::from_palette(StudioUiPalette::IndustrialDark)
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
    fn world_at_center_of_empty_minimap_maps_to_camera_center() {
        let camera = CadCamera {
            center: Vec2::new(50.0, 30.0),
            zoom: 1.0,
        };
        let size = Vec2::new(640.0, 480.0);
        let image = image_rect(size);
        let local = Vec2::new(image.x + image.width * 0.5, image.y + image.height * 0.5);
        let world = world_at(
            &CadScene::from_schematic(&Schematic::new("t")),
            camera,
            size,
            local,
        )
        .expect("empty minimap still maps");
        assert!(
            (world - camera.center).length() < 4.0,
            "center pixel should resolve near the camera center, got {world:?}"
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
