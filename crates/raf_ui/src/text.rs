use std::collections::HashMap;

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

/// Lightweight shelf allocator and deterministic bitmap rasterizer for cached
/// UI text. The atlas stores alpha coverage only, so the renderer can apply
/// colors without duplicating a glyph for every palette change.
#[derive(Debug, Clone)]
pub struct UiTextAtlas {
    size: [u16; 2],
    cursor: [u16; 2],
    row_height: u16,
    frame: u64,
    slots: HashMap<UiTextAtlasKey, UiTextAtlasSlot>,
    pixels: Vec<u8>,
    dirty: bool,
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
    }

    /// Alpha coverage bytes in row-major atlas order.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Returns whether a presentation backend must upload the atlas again.
    pub fn is_dirty(&self) -> bool {
        self.dirty
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
    let scale = (request.style.size_px.max(6.0) / 8.0)
        .ceil()
        .clamp(1.0, 4.0) as usize;
    let glyph_width = 5 * scale;
    let glyph_height = 7 * scale;
    let advance = glyph_width + scale;
    let line_height = request
        .style
        .line_height_px
        .max(glyph_height as f32 + scale as f32)
        .ceil() as usize;
    let max_width = request
        .max_width
        .max(advance as f32)
        .floor()
        .max(advance as f32) as usize;

    let mut placements = Vec::new();
    let mut cursor_x = 0usize;
    let mut cursor_y = 0usize;
    let mut width = 1usize;

    for character in text.chars() {
        if character == '\n' {
            cursor_x = 0;
            cursor_y += line_height;
            continue;
        }
        if cursor_x > 0 && cursor_x + glyph_width > max_width {
            cursor_x = 0;
            cursor_y += line_height;
        }
        placements.push((cursor_x, cursor_y, glyph_pattern(character)));
        width = width.max(cursor_x + glyph_width);
        cursor_x += advance;
    }

    let height = (cursor_y + glyph_height).max(1);
    let mut pixels = vec![0; width * height];
    for (x, y, pattern) in placements {
        rasterize_glyph(
            &mut pixels,
            width,
            x,
            y,
            scale,
            pattern,
            request.style.weight,
        );
    }

    RasterizedText {
        width,
        height,
        pixels,
    }
}

fn rasterize_glyph(
    pixels: &mut [u8],
    atlas_width: usize,
    origin_x: usize,
    origin_y: usize,
    scale: usize,
    pattern: [u8; 7],
    weight: UiFontWeight,
) {
    for (row, bits) in pattern.iter().copied().enumerate() {
        for column in 0..5 {
            if bits & (1 << (4 - column)) == 0 {
                continue;
            }
            let extra_width = if weight == UiFontWeight::Bold { 1 } else { 0 };
            for py in 0..scale {
                for px in 0..(scale + extra_width) {
                    let x = origin_x + column * scale + px;
                    let y = origin_y + row * scale + py;
                    let index = y * atlas_width + x;
                    if index < pixels.len() {
                        pixels[index] = 255;
                    }
                }
            }
        }
    }
}

fn glyph_pattern(character: char) -> [u8; 7] {
    match normalize_glyph(character) {
        'A' => [0x0e, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'B' => [0x1e, 0x11, 0x11, 0x1e, 0x11, 0x11, 0x1e],
        'C' => [0x0f, 0x10, 0x10, 0x10, 0x10, 0x10, 0x0f],
        'D' => [0x1e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1e],
        'E' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x1f],
        'F' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x10],
        'G' => [0x0f, 0x10, 0x10, 0x17, 0x11, 0x11, 0x0f],
        'H' => [0x11, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'I' => [0x0e, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0e],
        'J' => [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0c],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1f],
        'M' => [0x11, 0x1b, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        'O' => [0x0e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        'P' => [0x1e, 0x11, 0x11, 0x1e, 0x10, 0x10, 0x10],
        'Q' => [0x0e, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0d],
        'R' => [0x1e, 0x11, 0x11, 0x1e, 0x14, 0x12, 0x11],
        'S' => [0x0f, 0x10, 0x10, 0x0e, 0x01, 0x01, 0x1e],
        'T' => [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0a, 0x04],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0a],
        'X' => [0x11, 0x11, 0x0a, 0x04, 0x0a, 0x11, 0x11],
        'Y' => [0x11, 0x11, 0x0a, 0x04, 0x04, 0x04, 0x04],
        'Z' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1f],
        '0' => [0x0e, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0e],
        '1' => [0x04, 0x0c, 0x04, 0x04, 0x04, 0x04, 0x0e],
        '2' => [0x0e, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1f],
        '3' => [0x1e, 0x01, 0x01, 0x0e, 0x01, 0x01, 0x1e],
        '4' => [0x02, 0x06, 0x0a, 0x12, 0x1f, 0x02, 0x02],
        '5' => [0x1f, 0x10, 0x10, 0x1e, 0x01, 0x01, 0x1e],
        '6' => [0x0e, 0x10, 0x10, 0x1e, 0x11, 0x11, 0x0e],
        '7' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0e, 0x11, 0x11, 0x0e, 0x11, 0x11, 0x0e],
        '9' => [0x0e, 0x11, 0x11, 0x0f, 0x01, 0x01, 0x0e],
        ' ' => [0; 7],
        '.' => [0, 0, 0, 0, 0, 0x06, 0x06],
        ',' => [0, 0, 0, 0, 0, 0x06, 0x04],
        ':' => [0, 0x06, 0x06, 0, 0x06, 0x06, 0],
        ';' => [0, 0x06, 0x06, 0, 0x06, 0x04, 0x08],
        '-' => [0, 0, 0, 0x1f, 0, 0, 0],
        '_' => [0, 0, 0, 0, 0, 0, 0x1f],
        '/' => [0x01, 0x02, 0x02, 0x04, 0x08, 0x08, 0x10],
        '\\' => [0x10, 0x08, 0x08, 0x04, 0x02, 0x02, 0x01],
        '(' => [0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02],
        ')' => [0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08],
        '[' => [0x0e, 0x08, 0x08, 0x08, 0x08, 0x08, 0x0e],
        ']' => [0x0e, 0x02, 0x02, 0x02, 0x02, 0x02, 0x0e],
        '!' => [0x04, 0x04, 0x04, 0x04, 0x04, 0, 0x04],
        '?' => [0x0e, 0x11, 0x01, 0x02, 0x04, 0, 0x04],
        '+' => [0, 0x04, 0x04, 0x1f, 0x04, 0x04, 0],
        '=' => [0, 0x1f, 0, 0x1f, 0, 0, 0],
        '#' => [0x0a, 0x1f, 0x0a, 0x0a, 0x1f, 0x0a, 0],
        '%' => [0x19, 0x1a, 0x04, 0x08, 0x16, 0x13, 0],
        _ => [0x0e, 0x11, 0x01, 0x02, 0x04, 0, 0x04],
    }
}

fn normalize_glyph(character: char) -> char {
    match character {
        '\u{00e1}' | '\u{00e0}' | '\u{00e4}' | '\u{00e2}' | '\u{00c1}' | '\u{00c0}'
        | '\u{00c4}' | '\u{00c2}' => 'A',
        '\u{00e9}' | '\u{00e8}' | '\u{00eb}' | '\u{00ea}' | '\u{00c9}' | '\u{00c8}'
        | '\u{00cb}' | '\u{00ca}' => 'E',
        '\u{00ed}' | '\u{00ec}' | '\u{00ef}' | '\u{00ee}' | '\u{00cd}' | '\u{00cc}'
        | '\u{00cf}' | '\u{00ce}' => 'I',
        '\u{00f3}' | '\u{00f2}' | '\u{00f6}' | '\u{00f4}' | '\u{00d3}' | '\u{00d2}'
        | '\u{00d6}' | '\u{00d4}' => 'O',
        '\u{00fa}' | '\u{00f9}' | '\u{00fc}' | '\u{00fb}' | '\u{00da}' | '\u{00d9}'
        | '\u{00dc}' | '\u{00db}' => 'U',
        '\u{00f1}' | '\u{00d1}' => 'N',
        _ if character.is_ascii_lowercase() => character.to_ascii_uppercase(),
        _ => character,
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
}
