//! Stable texture identities for retained RafUI paint.
//!
//! The compositor rebuilds a `ProductPacket` for every source revision, so the
//! texture table is re-appended. This cache keeps the expensive part: it owns
//! the produced RGBA bytes for one text-atlas generation and one image revision
//! so an unchanged glyph page or image is never rasterized, decoded, hashed or
//! expanded twice.

use std::collections::HashMap;

use raf_agb::Texture;

use super::super::UiTextAtlas;
use crate::api_graphic_basic::capabilities::GraphicsMemoryBudget;

/// Share of the graphics budget retained UI textures may keep resident.
const TEXTURE_BUDGET_DIVISOR: u64 = 16;
/// Floor so a potato profile can still hold the glyph page and a few icons.
const MIN_TEXTURE_BUDGET_BYTES: u64 = 4 * 1024 * 1024;

/// Stable identity of one uploaded texture.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum UiTextureKey {
    /// Text atlas pixels: revision changes whenever glyph coverage is written.
    Atlas {
        revision: u64,
        size: [u32; 2],
        height: u32,
    },
    /// Image residency revision reported by the CPU image store.
    Image { source: String, revision: u64 },
}

#[derive(Debug)]
struct UiTextureEntry {
    texture: Texture,
    last_used: u64,
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
    /// single-channel alpha; conversion to white-with-alpha RGBA happens once
    /// per generation and is reused verbatim afterwards.
    pub(crate) fn atlas(&mut self, atlas: &UiTextAtlas, height: u32) -> &Texture {
        let size = atlas.size();
        let height = height.clamp(1, u32::from(size[1].max(1)));
        let key = UiTextureKey::Atlas {
            revision: atlas.revision(),
            size: [u32::from(size[0]), u32::from(size[1])],
            height,
        };
        if !self.entries.contains_key(&key) {
            let texture = text_atlas_texture(atlas, height);
            self.conversions = self.conversions.saturating_add(1);
            self.insert(key.clone(), texture);
        }
        self.touch(&key);
        &self.entries.get(&key).expect("atlas entry is resident").texture
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
                rgba: pixels.to_vec(),
            };
            self.conversions = self.conversions.saturating_add(1);
            self.insert(key.clone(), texture);
        }
        self.touch(&key);
        &self.entries.get(&key).expect("image entry is resident").texture
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

    fn insert(&mut self, key: UiTextureKey, texture: Texture) {
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
            UiTextureEntry { texture, last_used },
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
        rgba,
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
        let first = cache.atlas(&atlas, 2).rgba.clone();
        let second = cache.atlas(&atlas, 2).rgba.clone();
        assert_eq!(first, second);
        assert_eq!(cache.conversions(), 1);
        assert_eq!(cache.entry_count(), 1);
        assert_eq!(cache.resident_bytes(), 8 * 2 * 4);
    }
}
