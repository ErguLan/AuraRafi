//! ApiGraphicBasic-owned typography backend for retained RafUI surfaces.
//!
//! The upper UI crate supplies semantic requests only. This module owns the
//! font registry, shaping fallback, rasterization, shelf atlas, dirty state,
//! and cache keys that presentation backends consume.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};

use ab_glyph::{point, Font, FontArc, ScaleFont};
use raf_ui::{UiFontWeight, UiTextAtlasRequest, UiTextOverflow, UiTextRole, UiTextStyle};

const TEXT_RASTER_PADDING: f32 = 2.0;
const MAX_LAYOUT_CACHE_ENTRIES: usize = 96;
const MAX_LAYOUT_CACHE_BYTES: usize = 384 * 1024;
const MAX_LAYOUT_CACHE_TEXT_BYTES: usize = 4 * 1024;

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
    size_px: u32,
    line_height_px: u32,
    max_width_px: u32,
    overflow: UiTextOverflow,
    single_line: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct UiTextLayoutCacheKey {
    text_hash: u64,
    role: UiTextRole,
    weight: UiFontWeight,
    size_px_bits: u32,
    width_px_bits: u32,
    tracking_bits: u32,
}

struct UiTextLayoutCacheEntry {
    text: String,
    lines: Arc<Vec<RasterLine>>,
    bytes: usize,
    last_used: u64,
}

#[derive(Default)]
struct UiTextLayoutCache {
    entries: HashMap<UiTextLayoutCacheKey, UiTextLayoutCacheEntry>,
    bytes: usize,
    clock: u64,
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
        let compacted = self.compact_stale_slots(&requests);
        let mut stats = self.sync_resolved_pass(&requests, frame);
        stats.evicted = stats.evicted.saturating_add(compacted);

        // Recovery is deliberately frame-atomic. Clearing while iterating the
        // requests invalidates slots that presentation still needs for this
        // frame, which makes labels disappear depending on their order in the
        // retained tree. Repack only after the first pass has finished, then
        // resolve the complete request set against the fresh atlas.
        if stats.overflowed > 0 {
            let evicted = stats.evicted.saturating_add(self.slots.len());
            self.clear();
            stats = self.sync_resolved_pass(&requests, frame);
            stats.evicted = stats.evicted.saturating_add(evicted);
        }
        stats
    }

    /// Reclaims atlas space left by text that no longer belongs to the current
    /// retained frame. Agent history virtualization changes a few large text
    /// blocks at a time; keeping every previously visited block made the atlas
    /// grow until scrolling itself became progressively more expensive.
    ///
    /// A small stale tail is intentionally preserved so a one-row scroll back
    /// can reuse its glyphs. Repacking only begins once stale content is both
    /// meaningful and large enough to justify one bounded rebuild.
    fn compact_stale_slots(&mut self, requests: &[(&UiTextAtlasRequest, &str)]) -> usize {
        const MIN_STALE_SLOTS: usize = 8;

        if self.slots.len() < MIN_STALE_SLOTS {
            return 0;
        }
        let active = requests
            .iter()
            .map(|(request, text)| UiTextAtlasKey::from_request(request, text))
            .collect::<HashSet<_>>();
        let stale = self
            .slots
            .iter()
            .filter(|(key, _)| !active.contains(*key))
            .collect::<Vec<_>>();
        if stale.len() < MIN_STALE_SLOTS {
            return 0;
        }
        let stale_area = stale
            .iter()
            .map(|(_, slot)| usize::from(slot.rect.width) * usize::from(slot.rect.height))
            .sum::<usize>();
        let atlas_area = usize::from(self.size[0]) * usize::from(self.size[1]);
        let stale_dominates_slots = stale.len().saturating_mul(2) >= self.slots.len();
        let stale_dominates_pixels = stale_area.saturating_mul(4) >= atlas_area;
        if !stale_dominates_slots && !stale_dominates_pixels {
            return 0;
        }

        let evicted = self.slots.len();
        self.clear();
        evicted
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
            if raster.pixels.is_empty() {
                stats.overflowed += 1;
                continue;
            }
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
        let prepared = (request.overflow == UiTextOverflow::Ellipsis)
            .then(|| prepare_text(request, resolved_text));
        let display_cursor = prepared
            .as_ref()
            .map(|prepared| prepared.display_cursor_for_source(cursor))
            .unwrap_or(cursor);
        let display_text = prepared
            .as_ref()
            .map(|prepared| prepared.display.as_str())
            .unwrap_or(resolved_text);
        let lines = layout_for_request(request, display_text);
        let mut visible_width = 0.0_f32;

        for line in lines.iter() {
            if line.end <= display_cursor {
                visible_width = visible_width.max(line.width);
            }
            if line.start <= display_cursor && display_cursor <= line.end {
                let partial_width = line
                    .glyphs
                    .iter()
                    .take_while(|glyph| glyph.character_index < display_cursor)
                    .last()
                    .map(|glyph| glyph.end_x)
                    .unwrap_or(0.0);
                visible_width = visible_width.max(partial_width);
            }
            if line.start > display_cursor {
                break;
            }
        }

        visible_width
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

/// Returns the baseline from the raster text-box top and the actual height of
/// one line at this style's scale. Layout callers should pass an already
/// `scaled_for_ui` style when measuring a scaled child.
pub(super) fn baseline_metrics(style: &UiTextStyle) -> [f32; 2] {
    let font = atlas_font(style.role, style.weight);
    let scale = font.as_scaled(style.size_px.max(6.0));
    let line_height = raster_line_height(style, &scale);
    let raster_height = (line_height + TEXT_RASTER_PADDING * 2.0).ceil().max(1.0);
    [TEXT_RASTER_PADDING + scale.ascent(), raster_height]
}

fn raster_line_height<F: Font>(style: &UiTextStyle, scale: &impl ScaleFont<F>) -> f32 {
    style
        .line_height_px
        .max(scale.height().ceil() + TEXT_RASTER_PADDING)
        .ceil()
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
            size_px: request.style.size_px.to_bits(),
            line_height_px: request.style.line_height_px.to_bits(),
            max_width_px: request.max_width.to_bits(),
            overflow: request.overflow,
            single_line: request.single_line,
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

struct RasterizedText {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

fn rasterize_text(request: &UiTextAtlasRequest, text: &str) -> RasterizedText {
    if text.len() > 64 * 1024 {
        tracing::warn!(node=%request.node_id,"RafUI text must be split into bounded tiles");
        return RasterizedText {
            width: 0,
            height: 0,
            pixels: Vec::new(),
        };
    }
    let font = atlas_font(request.style.role, request.style.weight);
    let scale = font.as_scaled(request.style.size_px.max(6.0));
    let line_height = raster_line_height(&request.style, &scale);
    let prepared =
        (request.overflow == UiTextOverflow::Ellipsis).then(|| prepare_text(request, text));
    let display_text = prepared
        .as_ref()
        .map(|prepared| prepared.display.as_str())
        .unwrap_or(text);
    let lines = layout_for_request(request, display_text);
    let width = lines
        .iter()
        .map(|line| line.width)
        .fold(1.0f32, f32::max)
        .ceil()
        .max(1.0) as usize
        + (TEXT_RASTER_PADDING * 2.0) as usize;
    let height = ((lines.len().max(1) as f32 * line_height) + TEXT_RASTER_PADDING * 2.0)
        .ceil()
        .max(1.0) as usize;
    if width > 2048 || height > 2048 {
        tracing::warn!(node=%request.node_id,width,height,"RafUI text tile exceeds atlas dimensions");
        return RasterizedText {
            width,
            height,
            pixels: Vec::new(),
        };
    }
    let mut pixels = vec![0; width * height];

    for (line_index, line) in lines.iter().enumerate() {
        let baseline = TEXT_RASTER_PADDING + scale.ascent() + line_index as f32 * line_height;
        for glyph in &line.glyphs {
            let glyph = glyph.id.with_scale_and_position(
                scale.scale(),
                point(TEXT_RASTER_PADDING + glyph.x, baseline),
            );
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

fn text_layout_width(
    role: UiTextRole,
    authored_width: f32,
    single_line: bool,
    overflow: UiTextOverflow,
) -> f32 {
    // Controls are single-line by contract. Wrapping a tab, row label, or
    // toolbar button can reduce its visible text to one glyph when a narrow
    // responsive layout temporarily compresses the flow track. The parent
    // clip remains responsible for truncation; the atlas must not create
    // extra lines for these compact controls.
    if single_line
        || matches!(role, UiTextRole::Button | UiTextRole::Toolbar)
        || matches!(overflow, UiTextOverflow::Clip)
    {
        authored_width.max(1024.0)
    } else if overflow == UiTextOverflow::Ellipsis {
        authored_width.max(1.0)
    } else {
        authored_width.max(1.0)
    }
}

fn ellipsize_text<F: Font>(
    scale: &impl ScaleFont<F>,
    text: &str,
    max_width: f32,
    tracking: f32,
) -> PreparedText {
    let mut result = PreparedText::new();
    let mut paragraph = Vec::new();
    let mut paragraph_start = 0;

    for (source_index, character) in text.chars().enumerate() {
        if character == '\n' {
            append_ellipsized_paragraph(
                scale,
                &paragraph,
                paragraph_start,
                max_width,
                tracking,
                &mut result,
            );
            paragraph.clear();
            result.push_source('\n', source_index);
            paragraph_start = source_index + 1;
        } else {
            paragraph.push((source_index, character));
        }
    }
    append_ellipsized_paragraph(
        scale,
        &paragraph,
        paragraph_start,
        max_width,
        tracking,
        &mut result,
    );
    result
}

struct PreparedText {
    display: String,
    /// Maps display-character boundaries back to source-character boundaries.
    /// A synthetic ellipsis maps to the end of the retained source prefix.
    source_boundaries: Vec<usize>,
}

impl PreparedText {
    fn new() -> Self {
        Self {
            display: String::new(),
            source_boundaries: vec![0],
        }
    }

    fn push_source(&mut self, character: char, source_index: usize) {
        self.display.push(character);
        self.source_boundaries.push(source_index + 1);
    }

    fn push_synthetic(&mut self, character: char, source_boundary: usize) {
        self.display.push(character);
        self.source_boundaries.push(source_boundary);
    }

    fn display_cursor_for_source(&self, source_cursor: usize) -> usize {
        self.source_boundaries
            .iter()
            .enumerate()
            .take_while(|(_, source_boundary)| **source_boundary <= source_cursor)
            .map(|(display_boundary, _)| display_boundary)
            .last()
            .unwrap_or(0)
    }
}

fn prepare_text(request: &UiTextAtlasRequest, text: &str) -> PreparedText {
    if request.overflow == UiTextOverflow::Ellipsis {
        let font = atlas_font(request.style.role, request.style.weight);
        let scale = font.as_scaled(request.style.size_px.max(6.0));
        return ellipsize_text(
            &scale,
            text,
            request.max_width.max(1.0),
            brand_tracking(request.style.role),
        );
    }

    let mut prepared = PreparedText::new();
    for (source_index, character) in text.chars().enumerate() {
        prepared.push_source(character, source_index);
    }
    prepared
}

fn append_ellipsized_paragraph<F: Font>(
    scale: &impl ScaleFont<F>,
    paragraph: &[(usize, char)],
    source_start: usize,
    max_width: f32,
    tracking: f32,
    output: &mut PreparedText,
) {
    let mut full_text = String::new();
    for (_, character) in paragraph {
        full_text.push(*character);
    }
    if appended_text_width(scale, 0.0, None, &full_text, tracking) <= max_width {
        for (source_index, character) in paragraph {
            output.push_source(*character, *source_index);
        }
        return;
    }

    let ellipsis_width = appended_character_width(scale, 0.0, None, '…', tracking);
    if ellipsis_width > max_width {
        return;
    }

    let mut prefix_width = 0.0;
    let mut previous = None;
    let mut source_boundary = source_start;
    for (source_index, character) in paragraph {
        let id = resolved_glyph_id(scale, *character);
        let next_width =
            appended_character_width(scale, prefix_width, previous, *character, tracking);
        let width_with_ellipsis =
            appended_character_width(scale, next_width, Some(id), '…', tracking);
        if width_with_ellipsis > max_width {
            break;
        }
        output.push_source(*character, *source_index);
        prefix_width = next_width;
        previous = Some(id);
        source_boundary = *source_index + 1;
    }
    output.push_synthetic('…', source_boundary);
}

/// Real Ubuntu outlines already carry their own weight. Do not darken or
/// dilate coverage after rasterization: that creates the soft halo that made
/// the previous editor chrome look blurry at small sizes.
fn weighted_coverage(coverage: f32, _weight: UiFontWeight) -> f32 {
    coverage.clamp(0.0, 1.0)
}

struct RasterLine {
    start: usize,
    end: usize,
    width: f32,
    glyphs: Vec<RasterGlyph>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct UiTextVisualLine {
    pub start: usize,
    pub end: usize,
    pub width: f32,
}

struct RasterGlyph {
    id: ab_glyph::GlyphId,
    x: f32,
    end_x: f32,
    character_index: usize,
}

fn layout_lines<F: Font>(
    scale: &impl ScaleFont<F>,
    text: &str,
    max_width: f32,
    tracking: f32,
) -> Vec<RasterLine> {
    let mut lines = vec![RasterLine {
        start: 0,
        end: 0,
        width: 0.0,
        glyphs: Vec::new(),
    }];
    let mut previous = None;
    let mut characters = text.chars().enumerate().peekable();
    while let Some((character_index, character)) = characters.next() {
        if character == '\n' {
            lines.last_mut().expect("text layout line exists").end = character_index;
            lines.push(RasterLine {
                start: character_index + 1,
                end: character_index + 1,
                width: 0.0,
                glyphs: Vec::new(),
            });
            previous = None;
            continue;
        }

        if character.is_whitespace() {
            let line = lines.last().expect("text layout begins with a line");
            let next_width =
                appended_character_width(scale, line.width, previous, character, tracking);
            if !line.glyphs.is_empty() && next_width > max_width {
                lines.push(RasterLine {
                    start: character_index,
                    end: character_index,
                    width: 0.0,
                    glyphs: Vec::new(),
                });
                previous = None;
            }
            let line = lines.last_mut().expect("text layout line exists");
            append_character_to_line(
                scale,
                line,
                &mut previous,
                character_index,
                character,
                tracking,
            );
            line.end = character_index + 1;
            continue;
        }

        let mut word = vec![(character_index, character)];
        while characters
            .peek()
            .map(|(_, next)| !next.is_whitespace() && *next != '\n')
            .unwrap_or(false)
        {
            word.push(characters.next().expect("peeked word character exists"));
        }

        let line = lines.last().expect("text layout begins with a line");
        let mut word_width = line.width;
        let mut word_previous = previous;
        for (_, word_character) in &word {
            word_width = appended_character_width(
                scale,
                word_width,
                word_previous,
                *word_character,
                tracking,
            );
            word_previous = Some(resolved_glyph_id(scale, *word_character));
        }
        if !line.glyphs.is_empty() && word_width > max_width {
            lines.push(RasterLine {
                start: word[0].0,
                end: word[0].0,
                width: 0.0,
                glyphs: Vec::new(),
            });
            previous = None;
        }

        for (word_index, word_character) in word {
            let line = lines.last().expect("text layout begins with a line");
            let next_width =
                appended_character_width(scale, line.width, previous, word_character, tracking);
            if !line.glyphs.is_empty() && next_width > max_width {
                lines.push(RasterLine {
                    start: word_index,
                    end: word_index,
                    width: 0.0,
                    glyphs: Vec::new(),
                });
                previous = None;
            }
            let line = lines.last_mut().expect("text layout line exists");
            append_character_to_line(
                scale,
                line,
                &mut previous,
                word_index,
                word_character,
                tracking,
            );
            line.end = word_index + 1;
        }
    }
    lines
}

fn layout_for_request(request: &UiTextAtlasRequest, text: &str) -> Arc<Vec<RasterLine>> {
    cached_layout(
        request,
        text,
        text_layout_width(
            request.style.role,
            request.max_width,
            request.single_line,
            request.overflow,
        ),
        brand_tracking(request.style.role),
    )
}

fn cached_layout(
    request: &UiTextAtlasRequest,
    text: &str,
    max_width: f32,
    tracking: f32,
) -> Arc<Vec<RasterLine>> {
    static CACHE: OnceLock<Mutex<UiTextLayoutCache>> = OnceLock::new();

    let key = UiTextLayoutCacheKey {
        text_hash: stable_text_hash(text),
        role: request.style.role,
        weight: request.style.weight,
        size_px_bits: request.style.size_px.max(6.0).to_bits(),
        width_px_bits: max_width.to_bits(),
        tracking_bits: tracking.to_bits(),
    };

    if text.len() <= MAX_LAYOUT_CACHE_TEXT_BYTES {
        let cache = CACHE.get_or_init(|| Mutex::new(UiTextLayoutCache::default()));
        let mut cache = cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.clock = cache.clock.wrapping_add(1).max(1);
        let last_used = cache.clock;
        if let Some(entry) = cache.entries.get_mut(&key) {
            if entry.text == text {
                entry.last_used = last_used;
                return Arc::clone(&entry.lines);
            }
        }
    }

    let font = atlas_font(request.style.role, request.style.weight);
    let scale = font.as_scaled(request.style.size_px.max(6.0));
    let lines = Arc::new(layout_lines(&scale, text, max_width, tracking));
    if text.len() > MAX_LAYOUT_CACHE_TEXT_BYTES {
        return lines;
    }

    let bytes = text
        .len()
        .saturating_add(
            lines
                .len()
                .saturating_mul(std::mem::size_of::<RasterLine>()),
        )
        .saturating_add(
            lines
                .iter()
                .map(|line| {
                    line.glyphs
                        .len()
                        .saturating_mul(std::mem::size_of::<RasterGlyph>())
                })
                .sum::<usize>(),
        )
        .saturating_add(std::mem::size_of::<UiTextLayoutCacheEntry>());
    if bytes > MAX_LAYOUT_CACHE_BYTES {
        return lines;
    }

    let cache = CACHE.get_or_init(|| Mutex::new(UiTextLayoutCache::default()));
    let mut cache = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(replaced) = cache.entries.remove(&key) {
        cache.bytes = cache.bytes.saturating_sub(replaced.bytes);
    }
    while cache.entries.len() >= MAX_LAYOUT_CACHE_ENTRIES
        || cache.bytes.saturating_add(bytes) > MAX_LAYOUT_CACHE_BYTES
    {
        let Some(oldest_key) = cache
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_used)
            .map(|(key, _)| key.clone())
        else {
            break;
        };
        if let Some(oldest) = cache.entries.remove(&oldest_key) {
            cache.bytes = cache.bytes.saturating_sub(oldest.bytes);
        }
    }
    cache.clock = cache.clock.wrapping_add(1).max(1);
    let last_used = cache.clock;
    cache.bytes = cache.bytes.saturating_add(bytes);
    cache.entries.insert(
        key,
        UiTextLayoutCacheEntry {
            text: text.to_string(),
            lines: Arc::clone(&lines),
            bytes,
            last_used,
        },
    );
    lines
}

/// Returns the exact character ranges occupied by the same visual lines that
/// the atlas rasterizes. Spaces are retained, and soft-wrap boundaries are
/// represented alongside explicit newline boundaries.
pub(crate) fn visual_line_ranges(
    request: &UiTextAtlasRequest,
    text: &str,
) -> Vec<UiTextVisualLine> {
    let prepared =
        (request.overflow == UiTextOverflow::Ellipsis).then(|| prepare_text(request, text));
    let display_text = prepared
        .as_ref()
        .map(|prepared| prepared.display.as_str())
        .unwrap_or(text);
    let lines = layout_for_request(request, display_text);
    lines
        .iter()
        .map(|line| UiTextVisualLine {
            start: source_boundary(prepared.as_ref(), line.start),
            end: source_boundary(prepared.as_ref(), line.end),
            width: line.width,
        })
        .collect()
}

fn source_boundary(prepared: Option<&PreparedText>, display_boundary: usize) -> usize {
    prepared
        .map(|prepared| prepared.source_boundaries[display_boundary])
        .unwrap_or(display_boundary)
}

fn appended_text_width<F: Font>(
    scale: &impl ScaleFont<F>,
    mut width: f32,
    mut previous: Option<ab_glyph::GlyphId>,
    text: &str,
    tracking: f32,
) -> f32 {
    for character in text.chars() {
        let id = resolved_glyph_id(scale, character);
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
    let id = resolved_glyph_id(scale, character);
    (width
        + if previous.is_some() { tracking } else { 0.0 }
        + previous.map(|glyph| scale.kern(glyph, id)).unwrap_or(0.0))
    .max(0.0)
        + scale.h_advance(id)
}

fn append_character_to_line<F: Font>(
    scale: &impl ScaleFont<F>,
    line: &mut RasterLine,
    previous: &mut Option<ab_glyph::GlyphId>,
    character_index: usize,
    character: char,
    tracking: f32,
) {
    let id = resolved_glyph_id(scale, character);
    let tracking = if previous.is_some() { tracking } else { 0.0 };
    let x = (line.width + tracking + previous.map(|glyph| scale.kern(glyph, id)).unwrap_or(0.0))
        .max(0.0);
    let end_x = x + scale.h_advance(id);
    line.glyphs.push(RasterGlyph {
        id,
        x,
        end_x,
        character_index,
    });
    line.width = end_x;
    *previous = Some(id);
}

fn resolved_glyph_id<F: Font>(scale: &impl ScaleFont<F>, character: char) -> ab_glyph::GlyphId {
    let glyph = scale.glyph_id(character);
    if glyph.0 != 0 {
        return glyph;
    }
    if character.is_whitespace() {
        let space = scale.glyph_id(' ');
        return if space.0 != 0 { space } else { glyph };
    }

    let replacement = scale.glyph_id('\u{fffd}');
    if replacement.0 != 0 {
        return replacement;
    }
    let question = scale.glyph_id('?');
    if question.0 != 0 {
        question
    } else {
        glyph
    }
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
    #[test]
    fn rafui_fractional_font_sizes_do_not_alias_and_oversized_tiles_are_bounded() {
        let mut first = request();
        first.style.size_px = 11.1;
        let mut second = first.clone();
        second.style.size_px = 11.8;
        assert_ne!(
            super::UiTextAtlasKey::from_request(&first, "same"),
            super::UiTextAtlasKey::from_request(&second, "same")
        );
        let raster = super::rasterize_text(&first, &"x".repeat(70 * 1024));
        assert!(raster.pixels.is_empty());
    }
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
    fn virtualized_text_reclaims_a_stale_atlas_window() {
        let mut atlas = UiTextAtlas::new([512, 512]);
        let mut previous = Vec::new();
        for index in 0..12 {
            let mut request = request();
            request.node_id = format!("previous.{index}");
            previous.push((request, format!("Previous Agent message {index}")));
        }
        atlas.sync_resolved(
            previous
                .iter()
                .map(|(request, text)| (request, text.as_str())),
        );
        assert_eq!(atlas.slot_count(), previous.len());

        let mut current = Vec::new();
        for index in 0..4 {
            let mut request = request();
            request.node_id = format!("current.{index}");
            current.push((request, format!("Current Agent message {index}")));
        }
        let stats = atlas.sync_resolved(
            current
                .iter()
                .map(|(request, text)| (request, text.as_str())),
        );

        assert!(stats.evicted >= previous.len());
        assert_eq!(atlas.slot_count(), current.len());
        assert!(current
            .iter()
            .all(|(request, text)| atlas.slot_for(request, text).is_some()));
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
    fn visual_lines_preserve_consecutive_spaces_and_character_ranges() {
        let mut request = request();
        request.max_width = 512.0;
        let lines = visual_line_ranges(&request, "a  b");

        assert_eq!(lines.len(), 1);
        assert_eq!((lines[0].start, lines[0].end), (0, 4));

        let mut atlas = UiTextAtlas::new([512, 128]);
        atlas.sync_resolved([(&request, "a  b"), (&request, "ab")]);
        assert!(
            atlas.measure_prefix_width(&request, "a  b", 4)
                > atlas.measure_prefix_width(&request, "ab", 2)
        );
    }

    #[test]
    fn wrapping_prefers_word_boundaries_and_hard_breaks_overlong_tokens() {
        let mut request = request();
        let font = atlas_font(request.style.role, request.style.weight);
        let scale = font.as_scaled(request.style.size_px.max(6.0));
        let tracking = brand_tracking(request.style.role);
        request.max_width = appended_text_width(&scale, 0.0, None, "alpha ", tracking) + 0.1;

        let word_lines = visual_line_ranges(&request, "alpha beta");
        assert_eq!(word_lines.len(), 2);
        assert_eq!((word_lines[0].start, word_lines[0].end), (0, 6));
        assert_eq!((word_lines[1].start, word_lines[1].end), (6, 10));

        request.max_width = appended_text_width(&scale, 0.0, None, "ab", tracking) + 0.1;
        let token = "unbreakable";
        let token_lines = visual_line_ranges(&request, token);
        assert!(token_lines.len() > 1);
        assert_eq!(token_lines.first().unwrap().start, 0);
        assert_eq!(token_lines.last().unwrap().end, token.chars().count());
        assert!(token_lines
            .windows(2)
            .all(|pair| pair[0].end == pair[1].start));
    }

    #[test]
    fn explicit_newlines_keep_empty_visual_lines_and_source_ranges() {
        let mut request = request();
        request.max_width = 512.0;
        let lines = visual_line_ranges(&request, "a\n\nb");

        assert_eq!(lines.len(), 3);
        assert_eq!((lines[0].start, lines[0].end), (0, 1));
        assert_eq!((lines[1].start, lines[1].end), (2, 2));
        assert_eq!((lines[2].start, lines[2].end), (3, 4));
    }

    #[test]
    fn ellipsis_is_measured_and_never_exceeds_a_narrow_width() {
        let mut request = request();
        request.overflow = UiTextOverflow::Ellipsis;
        let font = atlas_font(request.style.role, request.style.weight);
        let scale = font.as_scaled(request.style.size_px.max(6.0));
        let mark_width = appended_character_width(&scale, 0.0, None, '…', 0.0);

        request.max_width = (mark_width * 0.5).max(0.1);
        let too_narrow = prepare_text(&request, "A long label");
        assert!(too_narrow.display.is_empty());

        request.max_width = mark_width;
        let marker_only = prepare_text(&request, "A long label");
        assert_eq!(marker_only.display, "…");
        let line = visual_line_ranges(&request, "A long label");
        assert_eq!((line[0].start, line[0].end), (0, 0));
        assert!(line[0].width <= request.max_width + f32::EPSILON);
    }

    #[test]
    fn missing_non_whitespace_glyphs_resolve_to_a_visible_fallback() {
        let request = request();
        let font = atlas_font(request.style.role, request.style.weight);
        let scale = font.as_scaled(request.style.size_px.max(6.0));
        let missing = char::MAX;
        if scale.glyph_id(missing).0 != 0 {
            return;
        }

        assert_ne!(resolved_glyph_id(&scale, missing).0, 0);
        let raster = rasterize_text(&request, &missing.to_string());
        assert!(raster.pixels.iter().any(|pixel| *pixel > 0));
    }

    #[test]
    fn baseline_metrics_match_the_single_line_raster_box() {
        let request = request();
        let font = atlas_font(request.style.role, request.style.weight);
        let scale = font.as_scaled(request.style.size_px.max(6.0));
        let metrics = baseline_metrics(&request.style);

        assert_eq!(metrics[0], TEXT_RASTER_PADDING + scale.ascent());
        assert_eq!(metrics[1], raster_line_height(&request.style, &scale) + 4.0);
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
