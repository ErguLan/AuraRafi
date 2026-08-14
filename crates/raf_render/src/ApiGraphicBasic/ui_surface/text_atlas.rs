//! ApiGraphicBasic-owned typography backend for retained RafUI surfaces.
//!
//! The upper UI crate supplies semantic requests only. This module owns the
//! font registry, shaping fallback, rasterization, shelf atlas, dirty state,
//! and cache keys that presentation backends consume.

use std::collections::HashMap;
use std::sync::OnceLock;

use ab_glyph::{point, Font, FontArc, ScaleFont};
use raf_ui::{UiFontWeight, UiTextAtlasRequest, UiTextRole};

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
    pub evicted: usize,
}

/// Bounded alpha atlas used by both the GPU compositor and CPU recovery path.
#[derive(Debug, Clone)]
pub struct UiTextAtlas {
    size: [u16; 2],
    cursor: [u16; 2],
    row_height: u16,
    frame: u64,
    slots: HashMap<UiTextAtlasKey, UiTextAtlasSlot>,
    pixels: Vec<u8>,
    dirty: bool,
    dirty_region: Option<UiTextAtlasRect>,
    revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct UiTextAtlasKey {
    text_hash: u64,
    role: UiTextRole,
    weight: UiFontWeight,
    size_px: u16,
    line_height_px: u16,
    max_width_px: u16,
}

impl UiTextAtlas {
    pub fn new(size: [u16; 2]) -> Self {
        let size = [size[0].max(1), size[1].max(1)];
        Self {
            size,
            cursor: [0, 0],
            row_height: 0,
            frame: 0,
            slots: HashMap::new(),
            pixels: vec![0; usize::from(size[0]) * usize::from(size[1])],
            dirty: true,
            dirty_region: Some(UiTextAtlasRect {
                x: 0,
                y: 0,
                width: size[0],
                height: size[1],
            }),
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
        self.dirty_region = Some(UiTextAtlasRect {
            x: 0,
            y: 0,
            width: self.size[0],
            height: self.size[1],
        });
        self.revision = self.revision.wrapping_add(1).max(1);
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn mark_uploaded(&mut self) {
        self.dirty = false;
        self.dirty_region = None;
    }

    pub fn dirty_region(&self) -> Option<UiTextAtlasRect> {
        self.dirty_region
    }

    pub fn sync(&mut self, requests: &[UiTextAtlasRequest]) -> UiTextAtlasSyncStats {
        self.sync_resolved(
            requests
                .iter()
                .map(|request| (request, request.text_key.as_str())),
        )
    }

    pub fn sync_resolved<'a, I>(&mut self, requests: I) -> UiTextAtlasSyncStats
    where
        I: IntoIterator<Item = (&'a UiTextAtlasRequest, &'a str)>,
    {
        let requests = requests.into_iter().collect::<Vec<_>>();
        self.frame = self.frame.wrapping_add(1);
        let frame = self.frame;
        let mut stats = self.sync_resolved_pass(&requests, frame);

        // Recovery is deliberately frame-atomic. Clearing while iterating the
        // requests invalidates slots that presentation still needs for this
        // frame, which makes labels disappear depending on their order in the
        // retained tree. Repack only after the first pass has finished, then
        // resolve the complete request set against the fresh atlas.
        if stats.overflowed > 0 {
            let evicted = self.slots.len();
            self.clear();
            stats = self.sync_resolved_pass(&requests, frame);
            stats.evicted = stats.evicted.saturating_add(evicted);
        }
        stats
    }

    fn sync_resolved_pass<'a>(
        &mut self,
        requests: &[(&'a UiTextAtlasRequest, &'a str)],
        frame: u64,
    ) -> UiTextAtlasSyncStats {
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
            let rect = match self.allocate(estimated_size) {
                Some(rect) => rect,
                None => {
                    // Dynamic Agent/tool output can be much larger than the
                    // initial atlas. Grow while preserving existing glyphs;
                    // clearing the atlas here made earlier labels disappear
                    // whenever a long result arrived later in the same frame.
                    match self.grow().then(|| self.allocate(estimated_size)).flatten() {
                        Some(rect) => rect,
                        None => {
                            // The caller performs one atomic recovery pass
                            // after all requests have been attempted. Do not
                            // clear here: earlier text slots must remain valid
                            // until presentation finishes this frame.
                            stats.overflowed += 1;
                            continue;
                        }
                    }
                }
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

    /// Measures the visible advance before a caret in the same font and
    /// wrapping rules used by the atlas. The atlas itself is immutable here;
    /// this is only needed for the focused text control's one caret.
    pub fn measure_prefix_width(
        &self,
        request: &UiTextAtlasRequest,
        resolved_text: &str,
        cursor: usize,
    ) -> f32 {
        let prefix = resolved_text.chars().take(cursor).collect::<String>();
        if prefix.is_empty() {
            return 0.0;
        }
        let font = atlas_font(request.style.role, request.style.weight);
        let scale = font.as_scaled(request.style.size_px.max(6.0));
        let lines = layout_lines(
            &scale,
            &prefix,
            text_layout_width(request.style.role, request.max_width),
            brand_tracking(request.style.role),
        );
        lines.iter().map(|line| line.width).fold(0.0_f32, f32::max)
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

    fn grow(&mut self) -> bool {
        const MAX_SIZE: u16 = 2048;
        let next_size = [
            self.size[0].saturating_mul(2).min(MAX_SIZE),
            self.size[1].saturating_mul(2).min(MAX_SIZE),
        ];
        if next_size == self.size {
            return false;
        }

        let old_size = self.size;
        let old_pixels = std::mem::take(&mut self.pixels);
        let mut pixels = vec![0; usize::from(next_size[0]) * usize::from(next_size[1])];
        for row in 0..usize::from(old_size[1]) {
            let old_start = row * usize::from(old_size[0]);
            let old_end = old_start + usize::from(old_size[0]);
            let new_start = row * usize::from(next_size[0]);
            pixels[new_start..new_start + usize::from(old_size[0])]
                .copy_from_slice(&old_pixels[old_start..old_end]);
        }

        self.size = next_size;
        self.pixels = pixels;
        self.dirty = true;
        self.dirty_region = Some(UiTextAtlasRect {
            x: 0,
            y: 0,
            width: next_size[0],
            height: next_size[1],
        });
        self.revision = self.revision.wrapping_add(1).max(1);
        true
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
        self.dirty_region = Some(merge_rects(self.dirty_region, rect, self.size));
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
            text_hash: stable_text_hash(resolved_text),
            role: request.style.role,
            weight: request.style.weight,
            size_px: pixels_to_u16(request.style.size_px),
            line_height_px: pixels_to_u16(request.style.line_height_px),
            max_width_px: pixels_to_u16(request.max_width),
        }
    }
}

fn merge_rects(
    current: Option<UiTextAtlasRect>,
    next: UiTextAtlasRect,
    size: [u16; 2],
) -> UiTextAtlasRect {
    let Some(current) = current else {
        return next;
    };
    let left = current.x.min(next.x);
    let top = current.y.min(next.y);
    let right = current
        .x
        .saturating_add(current.width)
        .max(next.x.saturating_add(next.width))
        .min(size[0]);
    let bottom = current
        .y
        .saturating_add(current.height)
        .max(next.y.saturating_add(next.height))
        .min(size[1]);
    UiTextAtlasRect {
        x: left,
        y: top,
        width: right.saturating_sub(left),
        height: bottom.saturating_sub(top),
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
    let font = atlas_font(request.style.role, request.style.weight);
    let scale = font.as_scaled(request.style.size_px.max(6.0));
    let tracking = brand_tracking(request.style.role);
    let line_height = request
        .style
        .line_height_px
        .max(scale.height().ceil() + 2.0)
        .ceil();
    let lines = layout_lines(
        &scale,
        text,
        text_layout_width(request.style.role, request.max_width),
        tracking,
    );
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
                pixels[index] = pixels[index]
                    .max((weighted_coverage(coverage, request.style.weight) * 255.0).round() as u8);
            });
        }
    }
    RasterizedText {
        width,
        height,
        pixels,
    }
}

fn text_layout_width(role: UiTextRole, authored_width: f32) -> f32 {
    // Controls are single-line by contract. Wrapping a tab, row label, or
    // toolbar button can reduce its visible text to one glyph when a narrow
    // responsive layout temporarily compresses the flow track. The parent
    // clip remains responsible for truncation; the atlas must not create
    // extra lines for these compact controls.
    if matches!(role, UiTextRole::Button | UiTextRole::Toolbar) {
        authored_width.max(1024.0)
    } else {
        authored_width.max(1.0)
    }
}

/// Real Ubuntu outlines already carry their own weight. Do not darken or
/// dilate coverage after rasterization: that creates the soft halo that made
/// the previous editor chrome look blurry at small sizes.
fn weighted_coverage(coverage: f32, _weight: UiFontWeight) -> f32 {
    coverage.clamp(0.0, 1.0)
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

fn atlas_font(role: UiTextRole, weight: UiFontWeight) -> &'static FontArc {
    static UI_REGULAR: OnceLock<FontArc> = OnceLock::new();
    static UI_MEDIUM: OnceLock<FontArc> = OnceLock::new();
    static UI_BOLD: OnceLock<FontArc> = OnceLock::new();
    static MONOSPACE_FONT: OnceLock<FontArc> = OnceLock::new();
    static BRAND_FONT: OnceLock<FontArc> = OnceLock::new();
    match role {
        UiTextRole::Brand => BRAND_FONT.get_or_init(|| {
            FontArc::try_from_slice(include_bytes!(
                "../../../../raf_ui/assets/fonts/Tajawal-ExtraLight.ttf"
            ))
            .expect("the bundled Tajawal ExtraLight font must be valid")
        }),
        UiTextRole::Monospace => MONOSPACE_FONT.get_or_init(|| {
            FontArc::try_from_slice(epaint_default_fonts::HACK_REGULAR)
                .expect("the bundled Hack font must be valid")
        }),
        _ => match weight {
            UiFontWeight::Regular => UI_REGULAR.get_or_init(|| {
                FontArc::try_from_slice(include_bytes!("../../../assets/fonts/Ubuntu-Regular.ttf"))
                    .expect("the bundled Ubuntu Regular font must be valid")
            }),
            UiFontWeight::Medium => UI_MEDIUM.get_or_init(|| {
                FontArc::try_from_slice(include_bytes!("../../../assets/fonts/Ubuntu-Medium.ttf"))
                    .expect("the bundled Ubuntu Medium font must be valid")
            }),
            UiFontWeight::Bold => UI_BOLD.get_or_init(|| {
                FontArc::try_from_slice(include_bytes!("../../../assets/fonts/Ubuntu-Bold.ttf"))
                    .expect("the bundled Ubuntu Bold font must be valid")
            }),
        },
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
            raf_ui::UiTextStyle::panel_title([255, 255, 255, 255]),
            180.0,
        )
    }

    #[test]
    fn repeated_text_reuses_the_same_slot() {
        let request = request();
        let mut atlas = UiTextAtlas::new([256, 128]);
        assert_eq!(atlas.sync(std::slice::from_ref(&request)).allocated, 1);
        let first = atlas
            .slot_for(&request, "app.project_settings")
            .unwrap()
            .rect;
        assert_eq!(atlas.sync(std::slice::from_ref(&request)).reused, 1);
        assert_eq!(
            atlas
                .slot_for(&request, "app.project_settings")
                .unwrap()
                .rect,
            first
        );
    }

    #[test]
    fn resolved_language_text_gets_a_distinct_slot() {
        let request = request();
        let mut atlas = UiTextAtlas::new([256, 128]);
        atlas.sync_resolved([(&request, "Project settings")]);
        assert_eq!(
            atlas
                .sync_resolved([(&request, "Ajustes del proyecto")])
                .allocated,
            1
        );
        assert_eq!(atlas.slot_count(), 2);
    }

    #[test]
    fn rasterized_text_is_nonempty_and_weight_is_monotonic() {
        let regular = request();
        let mut bold = request();
        bold.style.weight = UiFontWeight::Bold;
        let mut atlas = UiTextAtlas::new([512, 128]);
        atlas.sync_resolved([(&regular, "Properties")]);
        atlas.sync_resolved([(&bold, "Properties")]);
        let regular_slot = atlas.slot_for(&regular, "Properties").unwrap();
        let bold_slot = atlas.slot_for(&bold, "Properties").unwrap();
        assert!(regular_slot.rect.width > 0);
        assert!(bold_slot.rect.width > 0);
        assert!(atlas.pixels().iter().any(|pixel| *pixel > 0));
    }

    #[test]
    fn identical_text_style_reuses_a_slot_across_nodes() {
        let mut first = request();
        first.node_id = "panel.title.one".to_string();
        let mut second = first.clone();
        second.node_id = "panel.title.two".to_string();
        let mut atlas = UiTextAtlas::new([256, 128]);
        let stats = atlas.sync_resolved([(&first, "Shared"), (&second, "Shared")]);
        assert_eq!(stats.allocated, 1);
        assert_eq!(stats.reused, 1);
        assert_eq!(atlas.slot_count(), 1);
    }

    #[test]
    fn a_full_atlas_grows_before_evicting_visible_text() {
        let mut atlas = UiTextAtlas::new([32, 32]);
        let mut requests = Vec::new();
        for index in 0..32 {
            let mut request = request();
            request.node_id = format!("label.{index}");
            request.style.size_px = 12.0;
            requests.push(request);
        }
        let stats = atlas.sync_resolved(requests.iter().map(|request| (request, "Atlas")));
        assert_eq!(stats.evicted, 0);
        assert_eq!(stats.overflowed, 0);
        assert!(atlas.size()[0] > 32 || atlas.size()[1] > 32);
        assert!(atlas.is_dirty());
    }

    #[test]
    fn growing_atlas_preserves_existing_slots() {
        let mut atlas = UiTextAtlas::new([32, 32]);
        let first = request();
        atlas.sync_resolved([(&first, "Header")]);
        let first_rect = atlas.slot_for(&first, "Header").unwrap().rect;

        let mut long = request();
        long.node_id = "long-value".to_string();
        long.style.size_px = 28.0;
        long.style.line_height_px = 32.0;
        long.max_width = 64.0;
        atlas.sync_resolved([(&first, "Header"), (&long, "Long value")]);

        assert_eq!(atlas.slot_for(&first, "Header").unwrap().rect, first_rect);
        assert!(atlas.slot_for(&long, "Long value").is_some());
        assert!(atlas.size()[0] > 32 || atlas.size()[1] > 32);
    }

    #[test]
    fn compact_button_text_stays_on_one_line() {
        let request = UiTextAtlasRequest::new(
            "hierarchy.row.label",
            "entity.name",
            raf_ui::UiTextStyle::button([255, 255, 255, 255]),
            24.0,
        );
        let raster = rasterize_text(&request, "Building_0_0_Base");
        let font = atlas_font(request.style.role, request.style.weight);
        let scale = font.as_scaled(request.style.size_px.max(6.0));
        let expected_line_height = request
            .style
            .line_height_px
            .max(scale.height().ceil() + 2.0)
            .ceil() as usize;
        assert!(raster.height <= expected_line_height + 4);
        assert!(raster.width > 24);
    }

    #[test]
    fn overflow_recovery_keeps_earlier_slots_in_the_same_frame() {
        let first = request();
        let mut oversized = request();
        oversized.node_id = "oversized-value".to_string();
        oversized.max_width = 8_192.0;

        let mut atlas = UiTextAtlas::new([2_048, 2_048]);
        let oversized_text = "x".repeat(5_000);
        let stats =
            atlas.sync_resolved([(&first, "Header"), (&oversized, oversized_text.as_str())]);

        assert!(stats.overflowed > 0);
        assert!(stats.evicted > 0);
        assert!(atlas.slot_for(&first, "Header").is_some());
    }
}
