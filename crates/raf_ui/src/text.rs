use std::collections::HashMap;
use std::sync::OnceLock;

use ab_glyph::{point, Font, FontArc, ScaleFont};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UiFontWeight {
    Regular,
    Medium,
    Bold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UiTextRole {
    Body,
    Label,
    /// Distinct product wordmark text. RafUI renders this with the bundled
    /// Tajawal ExtraLight face and deliberate tracking instead of a UI body face.
    Brand,
    Button,
    PanelTitle,
    Toolbar,
    Tooltip,
    Monospace,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UiTextStyle {
    pub role: UiTextRole,
    pub size_px: f32,
    pub line_height_px: f32,
    pub weight: UiFontWeight,
    pub color: [u8; 4],
}

impl UiTextStyle {
    pub fn body(color: [u8; 4]) -> Self {
        Self {
            role: UiTextRole::Body,
            size_px: 13.0,
            line_height_px: 18.0,
            weight: UiFontWeight::Regular,
            color,
        }
    }

    pub fn button(color: [u8; 4]) -> Self {
        Self {
            role: UiTextRole::Button,
            size_px: 12.0,
            line_height_px: 16.0,
            weight: UiFontWeight::Medium,
            color,
        }
    }

    pub fn panel_title(color: [u8; 4]) -> Self {
        Self {
            role: UiTextRole::PanelTitle,
            size_px: 12.0,
            line_height_px: 16.0,
            weight: UiFontWeight::Bold,
            color,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiTextAtlasRequest {
    pub node_id: String,
    pub text_key: String,
    pub style: UiTextStyle,
    pub max_width: f32,
}

impl UiTextAtlasRequest {
    pub fn new(
        node_id: impl Into<String>,
        text_key: impl Into<String>,
        style: UiTextStyle,
        max_width: f32,
    ) -> Self {
        Self {
            node_id: node_id.into(),
            text_key: text_key.into(),
            style,
            max_width: max_width.max(0.0),
        }
    }

    /// Returns a request sized for a physical output density while preserving
    /// the document's logical layout units. Hosts use this when a retained
    /// surface is painted into a HiDPI target texture.
    pub fn scaled_for_raster(&self, scale: f32) -> Self {
        let scale = scale.clamp(1.0, 4.0);
        let mut request = self.clone();
        request.style.size_px = (request.style.size_px * scale).max(1.0);
        request.style.line_height_px = (request.style.line_height_px * scale).max(1.0);
        request.max_width = (request.max_width * scale).max(0.0);
        request
    }
}

/// A stable rectangle inside a renderer-owned text atlas texture.
///
/// `raf_ui` remains renderer-agnostic: it allocates and caches slots, while a
/// presentation backend rasterizes glyphs into those slots when available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiTextAtlasRect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiTextAtlasSlot {
    pub rect: UiTextAtlasRect,
    pub estimated_size: [u16; 2],
    pub last_used_frame: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiTextAtlasSyncStats {
    pub allocated: usize,
    pub reused: usize,
    pub overflowed: usize,
}

/// Lightweight shelf allocator and vector-font rasterizer for cached UI text.
/// The atlas stores alpha coverage only, so the renderer can apply colors
/// without duplicating a glyph for every palette change.
#[derive(Debug, Clone)]
pub struct UiTextAtlas {
    size: [u16; 2],
    cursor: [u16; 2],
    row_height: u16,
    frame: u64,
    slots: HashMap<UiTextAtlasKey, UiTextAtlasSlot>,
    pixels: Vec<u8>,
    dirty: bool,
    revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct UiTextAtlasKey {
    node_id: String,
    text_hash: u64,
    role: UiTextRole,
    weight: UiFontWeight,
    size_px: u16,
    line_height_px: u16,
    max_width_px: u16,
}

impl UiTextAtlas {
    pub fn new(size: [u16; 2]) -> Self {
        Self {
            size: [size[0].max(1), size[1].max(1)],
            cursor: [0, 0],
            row_height: 0,
            frame: 0,
            slots: HashMap::new(),
            pixels: vec![0; usize::from(size[0].max(1)) * usize::from(size[1].max(1))],
            dirty: true,
            revision: 1,
        }
    }

    pub fn with_capacity(size: [u16; 2], expected_slots: usize) -> Self {
        let mut atlas = Self::new(size);
        atlas.slots = HashMap::with_capacity(expected_slots);
        atlas
    }

    pub fn size(&self) -> [u16; 2] {
        self.size
    }

    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    pub fn clear(&mut self) {
        self.cursor = [0, 0];
        self.row_height = 0;
        self.slots.clear();
        self.pixels.fill(0);
        self.dirty = true;
        self.revision = self.revision.wrapping_add(1).max(1);
    }

    /// Alpha coverage bytes in row-major atlas order.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Returns whether a presentation backend must upload the atlas again.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Changes whenever slot geometry or pixels are invalidated. Presentation
    /// hosts use this to invalidate cached text quads without rebuilding the
    /// retained layout.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Marks the current bitmap as uploaded by a presentation backend.
    pub fn mark_uploaded(&mut self) {
        self.dirty = false;
    }

    /// Syncs text keys as a conservative fallback when no localization layer
    /// has resolved their visible strings yet.
    pub fn sync(&mut self, requests: &[UiTextAtlasRequest]) -> UiTextAtlasSyncStats {
        self.sync_resolved(
            requests
                .iter()
                .map(|request| (request, request.text_key.as_str())),
        )
    }

    /// Syncs slots using resolved text. Callers should clear the atlas on a
    /// language or font change so stale rasterized glyphs are never reused.
    pub fn sync_resolved<'a, I>(&mut self, requests: I) -> UiTextAtlasSyncStats
    where
        I: IntoIterator<Item = (&'a UiTextAtlasRequest, &'a str)>,
    {
        self.frame = self.frame.wrapping_add(1);
        let frame = self.frame;
        let mut stats = UiTextAtlasSyncStats::default();

        for (request, resolved_text) in requests {
            let key = UiTextAtlasKey::from_request(request, resolved_text);
            if let Some(slot) = self.slots.get_mut(&key) {
                slot.last_used_frame = frame;
                stats.reused += 1;
                continue;
            }

            let raster = rasterize_text(request, resolved_text);
            let estimated_size = [
                raster.width.try_into().unwrap_or(u16::MAX),
                raster.height.try_into().unwrap_or(u16::MAX),
            ];
            let Some(rect) = self.allocate(estimated_size) else {
                stats.overflowed += 1;
                continue;
            };

            self.write_raster(rect, &raster);

            self.slots.insert(
                key,
                UiTextAtlasSlot {
                    rect,
                    estimated_size,
                    last_used_frame: frame,
                },
            );
            stats.allocated += 1;
        }

        stats
    }

    pub fn slot_for(
        &self,
        request: &UiTextAtlasRequest,
        resolved_text: &str,
    ) -> Option<&UiTextAtlasSlot> {
        self.slots
            .get(&UiTextAtlasKey::from_request(request, resolved_text))
    }

    fn allocate(&mut self, estimated_size: [u16; 2]) -> Option<UiTextAtlasRect> {
        const PADDING: u16 = 1;

        let width = estimated_size[0].saturating_add(PADDING * 2);
        let height = estimated_size[1].saturating_add(PADDING * 2);
        if width > self.size[0] || height > self.size[1] {
            return None;
        }

        if self.cursor[0].saturating_add(width) > self.size[0] {
            self.cursor[0] = 0;
            self.cursor[1] = self.cursor[1].saturating_add(self.row_height);
            self.row_height = 0;
        }
        if self.cursor[1].saturating_add(height) > self.size[1] {
            return None;
        }

        let rect = UiTextAtlasRect {
            x: self.cursor[0].saturating_add(PADDING),
            y: self.cursor[1].saturating_add(PADDING),
            width: estimated_size[0],
            height: estimated_size[1],
        };
        self.cursor[0] = self.cursor[0].saturating_add(width);
        self.row_height = self.row_height.max(height);
        Some(rect)
    }

    fn write_raster(&mut self, rect: UiTextAtlasRect, raster: &RasterizedText) {
        let atlas_width = usize::from(self.size[0]);
        let copy_width = usize::from(rect.width).min(raster.width);
        let copy_height = usize::from(rect.height).min(raster.height);

        for row in 0..copy_height {
            let source_start = row * raster.width;
            let target_start = (usize::from(rect.y) + row) * atlas_width + usize::from(rect.x);
            self.pixels[target_start..target_start + copy_width]
                .copy_from_slice(&raster.pixels[source_start..source_start + copy_width]);
        }
        self.dirty = true;
        self.revision = self.revision.wrapping_add(1).max(1);
    }
}

impl Default for UiTextAtlas {
    fn default() -> Self {
        Self::with_capacity([1024, 1024], 256)
    }
}

impl UiTextAtlasKey {
    fn from_request(request: &UiTextAtlasRequest, resolved_text: &str) -> Self {
        Self {
            node_id: request.node_id.clone(),
            text_hash: stable_text_hash(resolved_text),
            role: request.style.role,
            weight: request.style.weight,
            size_px: pixels_to_u16(request.style.size_px),
            line_height_px: pixels_to_u16(request.style.line_height_px),
            max_width_px: pixels_to_u16(request.max_width),
        }
    }
}

fn pixels_to_u16(value: f32) -> u16 {
    value.ceil().clamp(1.0, u16::MAX as f32) as u16
}

struct RasterizedText {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

fn rasterize_text(request: &UiTextAtlasRequest, text: &str) -> RasterizedText {
    const PADDING: f32 = 2.0;

    let font = atlas_font(request.style.role);
    let scale = font.as_scaled(request.style.size_px.max(6.0));
    let tracking = brand_tracking(request.style.role);
    let line_height = request
        .style
        .line_height_px
        .max(scale.height().ceil() + 2.0)
        .ceil();
    let lines = layout_lines(&scale, text, request.max_width.max(1.0), tracking);
    let width = lines
        .iter()
        .map(|line| line.width)
        .fold(1.0f32, f32::max)
        .ceil()
        .max(1.0) as usize
        + (PADDING * 2.0) as usize;
    let height = ((lines.len().max(1) as f32 * line_height) + PADDING * 2.0)
        .ceil()
        .max(1.0) as usize;
    let mut pixels = vec![0; width * height];

    for (line_index, line) in lines.iter().enumerate() {
        let baseline = PADDING + scale.ascent() + line_index as f32 * line_height;
        for glyph in &line.glyphs {
            let glyph = glyph
                .id
                .with_scale_and_position(scale.scale(), point(PADDING + glyph.x, baseline));
            let Some(outlined) = font.outline_glyph(glyph) else {
                continue;
            };
            let bounds = outlined.px_bounds();
            outlined.draw(|x, y, coverage| {
                let pixel_x = (bounds.min.x + x as f32).floor() as i32;
                let pixel_y = (bounds.min.y + y as f32).floor() as i32;
                if pixel_x < 0 || pixel_y < 0 {
                    return;
                }
                let pixel_x = pixel_x as usize;
                let pixel_y = pixel_y as usize;
                if pixel_x >= width || pixel_y >= height {
                    return;
                }
                let index = pixel_y * width + pixel_x;
                let coverage = weighted_coverage(coverage, request.style.weight);
                pixels[index] = pixels[index].max((coverage * 255.0).round() as u8);
            });
        }
    }

    RasterizedText {
        width,
        height,
        pixels,
    }
}

/// The bundled Ubuntu face is intentionally light. RafUI still exposes
/// regular, medium, and bold semantic weights, so the atlas must honour them
/// instead of rasterizing every role with the same faint coverage.
///
/// This adjusts coverage only; it does not dilate glyph geometry, avoiding
/// the soft halo that a blur-like synthetic bold would introduce at 12-13px.
fn weighted_coverage(coverage: f32, weight: UiFontWeight) -> f32 {
    let exponent = match weight {
        UiFontWeight::Regular => 0.92,
        UiFontWeight::Medium => 0.80,
        UiFontWeight::Bold => 0.68,
    };
    coverage.clamp(0.0, 1.0).powf(exponent)
}

struct RasterLine {
    width: f32,
    glyphs: Vec<RasterGlyph>,
}

struct RasterGlyph {
    id: ab_glyph::GlyphId,
    x: f32,
}

fn layout_lines<F: Font>(
    scale: &impl ScaleFont<F>,
    text: &str,
    max_width: f32,
    tracking: f32,
) -> Vec<RasterLine> {
    let mut lines = vec![RasterLine {
        width: 0.0,
        glyphs: Vec::new(),
    }];
    let mut previous = None;

    for (paragraph_index, paragraph) in text.split('\n').enumerate() {
        if paragraph_index > 0 {
            lines.push(RasterLine {
                width: 0.0,
                glyphs: Vec::new(),
            });
            previous = None;
        }

        for word in paragraph.split_whitespace() {
            let line = lines.last().expect("text layout begins with a line");
            if !line.glyphs.is_empty() {
                let width_with_space =
                    appended_text_width(scale, line.width, previous, " ", tracking);
                let width_with_word =
                    appended_text_width(scale, width_with_space, None, word, tracking);
                if width_with_word > max_width {
                    lines.push(RasterLine {
                        width: 0.0,
                        glyphs: Vec::new(),
                    });
                    previous = None;
                } else {
                    append_text_to_line(
                        scale,
                        lines.last_mut().expect("text layout line exists"),
                        &mut previous,
                        " ",
                        tracking,
                    );
                }
            }

            for character in word.chars() {
                let line = lines.last().expect("text layout line exists");
                let next_width =
                    appended_character_width(scale, line.width, previous, character, tracking);
                if !line.glyphs.is_empty() && next_width > max_width {
                    lines.push(RasterLine {
                        width: 0.0,
                        glyphs: Vec::new(),
                    });
                    previous = None;
                }
                append_character_to_line(
                    scale,
                    lines.last_mut().expect("text layout line exists"),
                    &mut previous,
                    character,
                    tracking,
                );
            }
        }
    }

    lines
}

fn appended_text_width<F: Font>(
    scale: &impl ScaleFont<F>,
    mut width: f32,
    mut previous: Option<ab_glyph::GlyphId>,
    text: &str,
    tracking: f32,
) -> f32 {
    for character in text.chars() {
        let id = scale.glyph_id(character);
        if previous.is_some() {
            width += tracking;
        }
        width = (width + previous.map(|glyph| scale.kern(glyph, id)).unwrap_or(0.0)).max(0.0);
        width += scale.h_advance(id);
        previous = Some(id);
    }
    width
}

fn appended_character_width<F: Font>(
    scale: &impl ScaleFont<F>,
    width: f32,
    previous: Option<ab_glyph::GlyphId>,
    character: char,
    tracking: f32,
) -> f32 {
    let id = scale.glyph_id(character);
    (width
        + if previous.is_some() { tracking } else { 0.0 }
        + previous.map(|glyph| scale.kern(glyph, id)).unwrap_or(0.0))
    .max(0.0)
        + scale.h_advance(id)
}

fn append_text_to_line<F: Font>(
    scale: &impl ScaleFont<F>,
    line: &mut RasterLine,
    previous: &mut Option<ab_glyph::GlyphId>,
    text: &str,
    tracking: f32,
) {
    for character in text.chars() {
        append_character_to_line(scale, line, previous, character, tracking);
    }
}

fn append_character_to_line<F: Font>(
    scale: &impl ScaleFont<F>,
    line: &mut RasterLine,
    previous: &mut Option<ab_glyph::GlyphId>,
    character: char,
    tracking: f32,
) {
    let id = scale.glyph_id(character);
    let tracking = if previous.is_some() { tracking } else { 0.0 };
    line.width =
        (line.width + tracking + previous.map(|glyph| scale.kern(glyph, id)).unwrap_or(0.0))
            .max(0.0);
    line.glyphs.push(RasterGlyph { id, x: line.width });
    line.width += scale.h_advance(id);
    *previous = Some(id);
}

fn brand_tracking(role: UiTextRole) -> f32 {
    matches!(role, UiTextRole::Brand)
        .then_some(14.0)
        .unwrap_or(0.0)
}

fn atlas_font(role: UiTextRole) -> &'static FontArc {
    static UI_FONT: OnceLock<FontArc> = OnceLock::new();
    static BRAND_FONT: OnceLock<FontArc> = OnceLock::new();
    match role {
        UiTextRole::Brand => BRAND_FONT.get_or_init(|| {
            FontArc::try_from_slice(include_bytes!("../assets/fonts/Tajawal-ExtraLight.ttf"))
                .expect("the bundled Tajawal ExtraLight font must be a valid OpenType font")
        }),
        _ => UI_FONT.get_or_init(|| {
            FontArc::try_from_slice(epaint_default_fonts::UBUNTU_LIGHT)
                .expect("the bundled RafUI font must be a valid OpenType font")
        }),
    }
}

fn stable_text_hash(text: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> UiTextAtlasRequest {
        UiTextAtlasRequest::new(
            "panel.title",
            "app.project_settings",
            UiTextStyle::panel_title([255, 255, 255, 255]),
            180.0,
        )
    }

    #[test]
    fn repeated_text_reuses_the_same_slot() {
        let request = request();
        let mut atlas = UiTextAtlas::new([256, 128]);

        assert_eq!(atlas.sync(&[request.clone()]).allocated, 1);
        let first = atlas
            .slot_for(&request, "app.project_settings")
            .expect("slot allocated")
            .rect;
        let stats = atlas.sync(&[request.clone()]);

        assert_eq!(stats.reused, 1);
        assert_eq!(stats.allocated, 0);
        assert_eq!(
            atlas
                .slot_for(&request, "app.project_settings")
                .expect("slot retained")
                .rect,
            first
        );
    }

    #[test]
    fn resolved_language_text_gets_its_own_slot() {
        let request = request();
        let mut atlas = UiTextAtlas::new([256, 128]);

        atlas.sync_resolved([(&request, "Project settings")]);
        let stats = atlas.sync_resolved([(&request, "Ajustes del proyecto")]);

        assert_eq!(stats.allocated, 1);
        assert_eq!(atlas.slot_count(), 2);
    }

    #[test]
    fn rasterized_text_marks_the_atlas_dirty_and_contains_coverage() {
        let request = request();
        let mut atlas = UiTextAtlas::new([256, 128]);
        atlas.mark_uploaded();
        atlas.sync_resolved([(&request, "Ajustes del proyecto")]);

        assert!(atlas.is_dirty());
        assert!(atlas.pixels().iter().any(|pixel| *pixel > 0));
    }

    #[test]
    fn semantic_weight_strengthens_coverage_without_expanding_glyph_bounds() {
        let mut regular = request();
        regular.style.weight = UiFontWeight::Regular;
        let mut medium = regular.clone();
        medium.style.weight = UiFontWeight::Medium;
        let mut bold = regular.clone();
        bold.style.weight = UiFontWeight::Bold;

        let regular = rasterize_text(&regular, "Properties");
        let medium = rasterize_text(&medium, "Properties");
        let bold = rasterize_text(&bold, "Properties");

        assert_eq!(
            (regular.width, regular.height),
            (medium.width, medium.height)
        );
        assert_eq!((medium.width, medium.height), (bold.width, bold.height));
        let total = |pixels: &[u8]| pixels.iter().map(|value| u64::from(*value)).sum::<u64>();
        assert!(total(&regular.pixels) < total(&medium.pixels));
        assert!(total(&medium.pixels) < total(&bold.pixels));
    }

    #[test]
    fn raster_scale_preserves_identity_but_grows_glyph_detail() {
        let request = request();
        let scaled = request.scaled_for_raster(1.5);

        assert_eq!(scaled.node_id, request.node_id);
        assert_eq!(scaled.text_key, request.text_key);
        assert_eq!(scaled.style.size_px, request.style.size_px * 1.5);
        assert_eq!(scaled.max_width, request.max_width * 1.5);
    }

    #[test]
    fn text_layout_prefers_word_boundaries_before_character_breaks() {
        let font = atlas_font(UiTextRole::Body);
        let scale = font.as_scaled(14.0);
        let alpha_width = appended_text_width(&scale, 0.0, None, "alpha", 0.0);
        let beta_width = appended_text_width(&scale, 0.0, None, "beta", 0.0);
        let max_width = alpha_width + beta_width * 0.5;

        let lines = layout_lines(&scale, "alpha beta", max_width, 0.0);

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].glyphs.len(), "alpha".chars().count());
    }

    #[test]
    fn brand_role_uses_wider_tracked_wordmark_layout() {
        let body_font = atlas_font(UiTextRole::Body);
        let brand_font = atlas_font(UiTextRole::Brand);
        let body_scale = body_font.as_scaled(48.0);
        let brand_scale = brand_font.as_scaled(48.0);
        let body_width = appended_text_width(&body_scale, 0.0, None, "RAFI", 0.0);
        let brand_width = appended_text_width(
            &brand_scale,
            0.0,
            None,
            "RAFI",
            brand_tracking(UiTextRole::Brand),
        );

        assert!(brand_width > body_width);
    }
}
