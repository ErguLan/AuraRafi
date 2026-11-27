//! Stable texture identities for retained RafUI paint.
//!
//! The compositor rebuilds a `ProductPacket` for every source revision, so the
//! texture table is re-appended. This cache keeps the expensive part: it owns
//! the produced RGBA bytes for the stable atlas page and each image revision, so
//! an unchanged glyph page or image is never rasterized, decoded, hashed or
//! expanded twice. Glyph writes refresh only the atlas dirty rectangle.

use std::collections::HashMap;
use std::sync::Arc;

use raf_agb::Texture;

use super::super::{UiTextAtlas, UiTextAtlasRect};
use crate::api_graphic_basic::capabilities::GraphicsMemoryBudget;

/// Share of the graphics budget retained UI textures may keep resident.
const TEXTURE_BUDGET_DIVISOR: u64 = 16;
/// Floor so a potato profile can still hold the glyph page and a few icons.
const MIN_TEXTURE_BUDGET_BYTES: u64 = 4 * 1024 * 1024;

/// Stable identity of one uploaded texture.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum UiTextureKey {
    /// Text atlas page. The identity stays stable across glyph writes so the
    /// page can be refreshed in place instead of converted and retained once
    /// per revision.
    Atlas { size: [u32; 2], height: u32 },
    /// Image residency revision reported by the CPU image store.
    Image { source: String, revision: u64 },
}

#[derive(Debug)]
struct UiTextureEntry {
    texture: Texture,
    last_used: u64,
    /// Atlas revision already reflected by `texture`. `None` for images and
    /// for a page that was just converted in full.
    atlas_revision: Option<u64>,
}

/// Bounded texture table shared by every retained UI paint append.
#[derive(Debug)]
pub(crate) struct UiTextureCache {
    entries: HashMap<UiTextureKey, UiTextureEntry>,
    resident_bytes: u64,
    budget_bytes: u64,
    entry_budget: usize,
    clock: u64,
    conversions: u64,
}

impl UiTextureCache {
    pub(crate) fn with_budget(budget: GraphicsMemoryBudget) -> Self {
        Self {
            entries: HashMap::new(),
            resident_bytes: 0,
            budget_bytes: budget
                .gpu_bytes
                .saturating_div(TEXTURE_BUDGET_DIVISOR)
                .max(MIN_TEXTURE_BUDGET_BYTES),
            entry_budget: budget.texture_cache_entries.max(1) as usize,
            clock: 0,
            conversions: 0,
        }
    }

    /// Returns the RGBA atlas texture for the used glyph page. The atlas is
    /// single-channel alpha; conversion to white-with-alpha RGBA happens on
    /// first use. Later glyph writes patch only the atlas dirty rectangle into
    /// the retained page, so adding a label does not rescale or expand the
    /// whole glyph sheet.
    pub(crate) fn atlas(
        &mut self,
        atlas: &UiTextAtlas,
        height: u32,
    ) -> (&Texture, Option<UiTextAtlasRect>) {
        let size = atlas.size();
        let height = height.clamp(1, u32::from(size[1].max(1)));
        let key = UiTextureKey::Atlas {
            size: [u32::from(size[0]), u32::from(size[1])],
            height,
        };
        let revision = atlas.revision();
        let dirty = atlas.dirty_region();
        let mut converted = false;
        let mut updated = None;
        if !self.entries.contains_key(&key) {
            let texture = text_atlas_texture(atlas, height);
            self.insert(key.clone(), texture, Some(revision));
            converted = true;
        } else {
            let patch = {
                let entry = self.entries.get(&key).expect("atlas entry is resident");
                entry.atlas_revision != Some(revision)
            };
            if patch {
                let rebuild = dirty.is_none();
                if rebuild {
                    let texture = text_atlas_texture(atlas, height);
                    self.insert(key.clone(), texture, Some(revision));
                    updated = Some(UiTextAtlasRect {
                        x: 0,
                        y: 0,
                        width: size[0],
                        height: u16::try_from(height).unwrap_or(size[1]),
                    });
                } else if let Some(entry) = self.entries.get_mut(&key) {
                    if let Some(rect) = dirty {
                        patch_atlas_texture(&mut entry.texture, atlas, rect);
                        updated = Some(rect);
                    }
                    entry.atlas_revision = Some(revision);
                }
                converted = true;
            }
        }
        if converted {
            self.conversions = self.conversions.saturating_add(1);
        }
        self.touch(&key);
        (
            &self
                .entries
                .get(&key)
                .expect("atlas entry is resident")
                .texture,
            updated,
        )
    }

    /// Returns the straight-alpha RGBA texture for one resolved image revision.
    pub(crate) fn image(
        &mut self,
        source: &str,
        size: [u32; 2],
        revision: u64,
        pixels: &[u8],
    ) -> &Texture {
        let key = UiTextureKey::Image {
            source: source.to_string(),
            revision,
        };
        if !self.entries.contains_key(&key) {
            let texture = Texture {
                size,
                rgba: pixels.to_vec().into(),
                mipmaps: true,
            };
            self.conversions = self.conversions.saturating_add(1);
            self.insert(key.clone(), texture, None);
        }
        self.touch(&key);
        &self
            .entries
            .get(&key)
            .expect("image entry is resident")
            .texture
    }

    pub(crate) fn resident_bytes(&self) -> u64 {
        self.resident_bytes
    }

    pub(crate) fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Number of source-to-RGBA conversions performed. Reuse keeps this flat.
    pub(crate) fn conversions(&self) -> u64 {
        self.conversions
    }

    fn insert(&mut self, key: UiTextureKey, texture: Texture, atlas_revision: Option<u64>) {
        let bytes = texture.rgba.len() as u64;
        if let Some(previous) = self.entries.remove(&key) {
            self.resident_bytes = self
                .resident_bytes
                .saturating_sub(previous.texture.rgba.len() as u64);
        }
        self.resident_bytes = self.resident_bytes.saturating_add(bytes);
        let last_used = self.next_clock();
        self.entries.insert(
            key.clone(),
            UiTextureEntry {
                texture,
                last_used,
                atlas_revision,
            },
        );
        while self.resident_bytes > self.budget_bytes || self.entries.len() > self.entry_budget {
            if !self.evict_oldest(&key) {
                break;
            }
        }
    }

    fn touch(&mut self, key: &UiTextureKey) {
        let sequence = self.next_clock();
        if let Some(entry) = self.entries.get_mut(key) {
            entry.last_used = sequence;
        }
    }

    fn next_clock(&mut self) -> u64 {
        self.clock = self.clock.wrapping_add(1).max(1);
        self.clock
    }

    fn evict_oldest(&mut self, protect: &UiTextureKey) -> bool {
        let Some(key) = self
            .entries
            .iter()
            .filter(|(key, _)| *key != protect)
            .min_by_key(|(_, entry)| entry.last_used)
            .map(|(key, _)| key.clone())
        else {
            return false;
        };
        if let Some(entry) = self.entries.remove(&key) {
            self.resident_bytes = self
                .resident_bytes
                .saturating_sub(entry.texture.rgba.len() as u64);
        }
        true
    }
}

/// Expands the used atlas rows into straight-alpha RGBA. The atlas stores
/// single-channel coverage, so RGB is white and the alpha channel is coverage;
/// glyph tint comes from the paint quad color.
fn text_atlas_texture(atlas: &UiTextAtlas, height: u32) -> Texture {
    let size = atlas.size();
    let width = usize::from(size[0].max(1));
    let height = usize::try_from(height)
        .unwrap_or(usize::from(size[1]))
        .clamp(1, usize::from(size[1].max(1)));
    let pixels = atlas.pixels();
    let mut rgba = Vec::with_capacity(width * height * 4);
    for row in 0..height {
        let start = row * width;
        let end = (start + width).min(pixels.len());
        for coverage in &pixels[start..end] {
            rgba.extend_from_slice(&[255, 255, 255, *coverage]);
        }
    }
    Texture {
        size: [width as u32, height as u32],
        rgba: rgba.into(),
        mipmaps: false,
    }
}

/// Refreshes only the glyph rectangle that changed since the last page update.
fn patch_atlas_texture(texture: &mut Texture, atlas: &UiTextAtlas, rect: UiTextAtlasRect) {
    let atlas_size = atlas.size();
    let atlas_width = usize::from(atlas_size[0].max(1));
    let page_width = texture.size[0] as usize;
    let page_height = texture.size[1] as usize;
    let x0 = usize::from(rect.x).min(atlas_width).min(page_width);
    let y0 = usize::from(rect.y).min(page_height);
    let x1 = usize::from(rect.x)
        .saturating_add(usize::from(rect.width))
        .min(atlas_width)
        .min(page_width);
    let y1 = usize::from(rect.y)
        .saturating_add(usize::from(rect.height))
        .min(usize::from(atlas_size[1]))
        .min(page_height);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let pixels = atlas.pixels();
    let data = Arc::make_mut(&mut texture.rgba);
    for row in y0..y1 {
        let source_start = row * atlas_width + x0;
        let target_start = (row * page_width + x0) * 4;
        for (offset, coverage) in pixels[source_start..source_start + (x1 - x0)]
            .iter()
            .enumerate()
        {
            let pixel = target_start + offset * 4;
            data[pixel..pixel + 4].copy_from_slice(&[255, 255, 255, *coverage]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The atlas is stored as coverage, so one conversion must expand it into
    /// the alpha channel of a white RGBA page.
    #[test]
    fn atlas_coverage_becomes_white_rgba() {
        let atlas = UiTextAtlas::new([4, 2]);
        let texture = text_atlas_texture(&atlas, 1);
        assert_eq!(texture.size, [4, 1]);
        assert_eq!(texture.rgba.len(), 16);
        for pixel in texture.rgba.chunks_exact(4) {
            assert_eq!(&pixel[..3], &[255, 255, 255]);
        }
    }

    /// Re-appending the same atlas generation must not rasterize it again.
    #[test]
    fn atlas_generation_is_converted_once() {
        let atlas = UiTextAtlas::new([8, 4]);
        let mut cache = UiTextureCache::with_budget(GraphicsMemoryBudget::potato());
        let first = cache.atlas(&atlas, 2).0.rgba.clone();
        let second = cache.atlas(&atlas, 2).0.rgba.clone();
        assert_eq!(first, second);
        assert_eq!(cache.conversions(), 1);
        assert_eq!(cache.entry_count(), 1);
        assert_eq!(cache.resident_bytes(), 8 * 2 * 4);
    }

    /// A new glyph keeps the same resident page and refreshes it in place.
    /// Allocating one page per atlas revision retained old full pages until
    /// the budget evicted them and repeatedly expanded the entire atlas.
    #[test]
    fn new_glyphs_patch_the_resident_atlas_page() {
        let mut atlas = UiTextAtlas::new([128, 64]);
        let first = raf_ui::UiTextAtlasRequest::new(
            "first",
            "First",
            raf_ui::UiTextStyle::panel_title([255; 4]),
            120.0,
        );
        atlas.sync(std::slice::from_ref(&first));
        let mut cache = UiTextureCache::with_budget(GraphicsMemoryBudget::potato());
        let (size, bytes) = {
            let before = cache.atlas(&atlas, 64).0;
            (before.size, before.rgba.len() as u64)
        };
        let conversions = cache.conversions();

        let second = raf_ui::UiTextAtlasRequest::new(
            "second",
            "Second label",
            raf_ui::UiTextStyle::panel_title([255; 4]),
            120.0,
        );
        atlas.sync(std::slice::from_ref(&second));
        let after = cache.atlas(&atlas, 64).0;

        assert_eq!(after.size, size);
        assert_eq!(after.rgba.len() as u64, bytes);
        assert_eq!(cache.entry_count(), 1);
        assert_eq!(cache.resident_bytes(), bytes);
        assert_eq!(cache.conversions(), conversions + 1);
    }

    #[test]
    fn atlas_patches_only_the_reported_dirty_rectangle() {
        let mut atlas = UiTextAtlas::new([64, 64]);
        let first = raf_ui::UiTextAtlasRequest::new(
            "first",
            "First",
            raf_ui::UiTextStyle::panel_title([255; 4]),
            60.0,
        );
        atlas.sync(std::slice::from_ref(&first));
        let mut cache = UiTextureCache::with_budget(GraphicsMemoryBudget::potato());
        let before = cache.atlas(&atlas, 64).0.rgba.clone();

        let second = raf_ui::UiTextAtlasRequest::new(
            "second",
            "Second",
            raf_ui::UiTextStyle::panel_title([255; 4]),
            60.0,
        );
        atlas.sync(std::slice::from_ref(&second));
        let rect = atlas.dirty_region().expect("the new glyph is dirty");
        let after = cache.atlas(&atlas, 64).0.rgba.clone();
        let width = 64usize;
        for y in 0..64usize {
            for x in 0..width {
                let index = (y * width + x) * 4;
                let inside = x >= usize::from(rect.x)
                    && x < usize::from(rect.x) + usize::from(rect.width)
                    && y >= usize::from(rect.y)
                    && y < usize::from(rect.y) + usize::from(rect.height);
                if inside {
                    let coverage = atlas.pixels()[y * width + x];
                    assert_eq!(&after[index..index + 4], &[255, 255, 255, coverage]);
                } else {
                    assert_eq!(&after[index..index + 4], &before[index..index + 4]);
                }
            }
        }
    }
}
