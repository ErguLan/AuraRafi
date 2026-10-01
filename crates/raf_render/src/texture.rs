//! Texture loading and management for the CPU painter.
//!
//! Loads images from disk, caches them in memory, and provides
//! UV-mapped color sampling. Zero cost when textures_enabled = false.
//!
//! Uses pure Rust (no GPU upload). Images are stored as RGBA byte arrays.
//! Supports bounded PNG, JPG, BMP, TGA, and WebP decoding through `raf_assets`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

/// A loaded texture stored in CPU memory.
#[derive(Debug, Clone)]
pub struct CpuTexture {
    /// RGBA pixel data, row-major.
    pub data: Vec<u8>,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Source file path (for cache key).
    pub source: PathBuf,
}

impl CpuTexture {
    /// Create a solid-color 1x1 texture (placeholder).
    pub fn solid(r: u8, g: u8, b: u8) -> Self {
        Self {
            data: vec![r, g, b, 255],
            width: 1,
            height: 1,
            source: PathBuf::from("__solid__"),
        }
    }

    /// Create a checkerboard pattern (fallback texture).
    pub fn checkerboard(size: u32) -> Self {
        let mut data = Vec::with_capacity((size * size * 4) as usize);
        for y in 0..size {
            for x in 0..size {
                let checker = ((x / 4) + (y / 4)) % 2 == 0;
                let c = if checker { 200 } else { 80 };
                data.extend_from_slice(&[c, c, c, 255]);
            }
        }
        Self {
            data,
            width: size,
            height: size,
            source: PathBuf::from("__checkerboard__"),
        }
    }

    /// Sample color at UV coordinates (0.0 - 1.0).
    /// Returns [R, G, B, A] as u8 values.
    /// UV wraps around (repeating texture).
    pub fn sample_uv(&self, u: f32, v: f32) -> [u8; 4] {
        if self.width == 0 || self.height == 0 {
            return [255, 0, 255, 255]; // Magenta = missing texture
        }
        // Wrap UVs.
        let u = u.fract();
        let v = v.fract();
        let u = if u < 0.0 { u + 1.0 } else { u };
        let v = if v < 0.0 { v + 1.0 } else { v };

        let px = ((u * self.width as f32) as u32).min(self.width - 1);
        let py = ((v * self.height as f32) as u32).min(self.height - 1);
        let idx = ((py * self.width + px) * 4) as usize;

        if idx + 3 < self.data.len() {
            [
                self.data[idx],
                self.data[idx + 1],
                self.data[idx + 2],
                self.data[idx + 3],
            ]
        } else {
            [255, 0, 255, 255]
        }
    }

    /// Bilinear sample converted from sRGB bytes to linear-light RGBA.
    pub fn sample_uv_linear(&self, u: f32, v: f32) -> [f32; 4] {
        if self.width == 0 || self.height == 0 {
            return [1.0, 0.0, 1.0, 1.0];
        }
        let u = u.rem_euclid(1.0);
        let v = v.rem_euclid(1.0);
        let x = u * self.width as f32 - 0.5;
        let y = v * self.height as f32 - 0.5;
        let x0 = x.floor() as i64;
        let y0 = y.floor() as i64;
        let tx = x - x.floor();
        let ty = y - y.floor();
        let wrap = |coordinate: i64, size: u32| coordinate.rem_euclid(size as i64) as u32;
        let sample = |px: i64, py: i64| {
            let index = ((wrap(py, self.height) * self.width + wrap(px, self.width)) * 4) as usize;
            if index.saturating_add(3) >= self.data.len() {
                return [1.0, 0.0, 1.0, 1.0];
            }
            [
                srgb_to_linear(self.data[index]),
                srgb_to_linear(self.data[index + 1]),
                srgb_to_linear(self.data[index + 2]),
                self.data[index + 3] as f32 / 255.0,
            ]
        };
        let top_left = sample(x0, y0);
        let top_right = sample(x0 + 1, y0);
        let bottom_left = sample(x0, y0 + 1);
        let bottom_right = sample(x0 + 1, y0 + 1);
        let mut result = [0.0; 4];
        for channel in 0..4 {
            let top = top_left[channel] + (top_right[channel] - top_left[channel]) * tx;
            let bottom = bottom_left[channel] + (bottom_right[channel] - bottom_left[channel]) * tx;
            result[channel] = top + (bottom - top) * ty;
        }
        result
    }

    /// Downscale to fit within max_size (preserving aspect ratio).
    /// Returns a new texture if downscaled, or self if already fits.
    pub fn downscaled(&self, max_size: u32) -> Self {
        if self.width <= max_size && self.height <= max_size {
            return self.clone();
        }
        let scale = max_size as f32 / self.width.max(self.height) as f32;
        let new_w = ((self.width as f32 * scale) as u32).max(1);
        let new_h = ((self.height as f32 * scale) as u32).max(1);
        let mut data = Vec::with_capacity((new_w * new_h * 4) as usize);
        for y in 0..new_h {
            for x in 0..new_w {
                let src_x = (x as f32 / new_w as f32 * self.width as f32) as u32;
                let src_y = (y as f32 / new_h as f32 * self.height as f32) as u32;
                let idx = ((src_y * self.width + src_x) * 4) as usize;
                if idx + 3 < self.data.len() {
                    data.extend_from_slice(&self.data[idx..idx + 4]);
                } else {
                    data.extend_from_slice(&[0, 0, 0, 255]);
                }
            }
        }
        Self {
            data,
            width: new_w,
            height: new_h,
            source: self.source.clone(),
        }
    }

    /// Memory usage in bytes.
    pub fn memory_bytes(&self) -> usize {
        self.data.len()
    }
}

pub(crate) fn srgb_to_linear(channel: u8) -> f32 {
    static TABLE: OnceLock<[f32; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        std::array::from_fn(|index| {
            let value = index as f32 / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        })
    })[channel as usize]
}

pub(crate) fn linear_to_srgb(channel: f32) -> u8 {
    static TABLE: OnceLock<[u8; 4097]> = OnceLock::new();
    let index = (channel.clamp(0.0, 1.0) * 4096.0).round() as usize;
    TABLE.get_or_init(|| {
        std::array::from_fn(|index| {
            let value = index as f32 / 4096.0;
            let srgb = if value <= 0.0031308 {
                value * 12.92
            } else {
                1.055 * value.powf(1.0 / 2.4) - 0.055
            };
            (srgb * 255.0).round().clamp(0.0, 255.0) as u8
        })
    })[index]
}

/// Bounded decoded-image cache with deterministic LRU eviction.
pub struct TextureCache {
    textures: HashMap<String, CachedTexture>,
    max_memory: usize,
    used_memory: usize,
    max_texture_size: u32,
    frame: u64,
}

struct CachedTexture {
    texture: Option<Arc<CpuTexture>>,
    bytes: usize,
    last_used: u64,
}

impl TextureCache {
    /// Create a new cache with given limits.
    pub fn new(max_memory_mb: usize, max_texture_size: u32) -> Self {
        Self {
            textures: HashMap::new(),
            max_memory: max_memory_mb.saturating_mul(1024 * 1024),
            used_memory: 0,
            max_texture_size: max_texture_size.clamp(1, 8192),
            frame: 0,
        }
    }

    /// Gets a cached image or decodes it once. Invalid files resolve to `None`.
    pub fn get_or_load(&mut self, path: &Path) -> Option<Arc<CpuTexture>> {
        self.get_or_load_with_budget(path, usize::MAX)
    }

    /// Loads a texture only when its bounded output fits the caller's
    /// remaining frame budget. Oversized assets are skipped before pixel
    /// decoding, preventing a large scene from repeatedly decoding textures
    /// that cannot be retained in the current frame.
    pub fn get_or_load_with_budget(
        &mut self,
        path: &Path,
        frame_budget_bytes: usize,
    ) -> Option<Arc<CpuTexture>> {
        let key = path.to_string_lossy().to_string();
        self.frame = self.frame.wrapping_add(1).max(1);
        if let Some(entry) = self.textures.get_mut(&key) {
            if entry.bytes > frame_budget_bytes {
                return None;
            }
            entry.last_used = self.frame;
            return entry.texture.clone();
        }

        if let Ok((width, height)) =
            raf_assets::image_output_dimensions(path, self.max_texture_size)
        {
            let estimated_bytes = (width as usize)
                .checked_mul(height as usize)
                .and_then(|pixels| pixels.checked_mul(4))?;
            if estimated_bytes > frame_budget_bytes {
                return None;
            }
        }

        let texture = load_texture_from_disk(path, self.max_texture_size).map(Arc::new);
        let bytes = texture.as_ref().map_or(0, |texture| texture.memory_bytes());
        if bytes > frame_budget_bytes {
            return None;
        }
        let result = texture.clone();
        if bytes <= self.max_memory {
            self.used_memory = self.used_memory.saturating_add(bytes);
            self.textures.insert(
                key.clone(),
                CachedTexture {
                    texture,
                    bytes,
                    last_used: self.frame,
                },
            );
            self.evict_to_limits(Some(&key));
        }
        result
    }

    /// Clear the entire cache.
    pub fn clear(&mut self) {
        self.textures.clear();
        self.used_memory = 0;
    }

    /// Number of loaded textures.
    pub fn count(&self) -> usize {
        self.textures
            .values()
            .filter(|entry| entry.texture.is_some())
            .count()
    }

    /// Total memory used in bytes.
    pub fn memory_used(&self) -> usize {
        self.used_memory
    }

    pub fn set_limits(&mut self, max_memory_bytes: usize, max_texture_size: u32) {
        let next_texture_size = max_texture_size.clamp(1, 8192);
        if next_texture_size != self.max_texture_size {
            self.clear();
            self.max_texture_size = next_texture_size;
        }
        self.max_memory = max_memory_bytes;
        self.evict_to_limits(None);
    }

    pub fn invalidate(&mut self, path: &Path) {
        let key = path.to_string_lossy().to_string();
        if let Some(removed) = self.textures.remove(&key) {
            self.used_memory = self.used_memory.saturating_sub(removed.bytes);
        }
    }

    fn evict_to_limits(&mut self, protected: Option<&str>) {
        while self.used_memory > self.max_memory || self.textures.len() > 512 {
            let oldest = self
                .textures
                .iter()
                .filter(|(key, _)| protected.is_none_or(|protected| key.as_str() != protected))
                .min_by_key(|(_, texture)| texture.last_used)
                .map(|(key, _)| key.clone());
            let Some(oldest) = oldest else {
                break;
            };
            if let Some(removed) = self.textures.remove(&oldest) {
                self.used_memory = self.used_memory.saturating_sub(removed.bytes);
            }
        }
    }
}

impl Default for TextureCache {
    fn default() -> Self {
        Self::new(12, 512)
    }
}

/// Load a bounded decoded image from disk. Failure leaves the node's flat tint intact.
fn load_texture_from_disk(path: &Path, max_size: u32) -> Option<CpuTexture> {
    let decoded = raf_assets::decode_image(path, max_size)
        .map_err(|error| {
            tracing::warn!(path = ?path, %error, "Unable to load scene base-color texture");
            error
        })
        .ok()?;
    Some(CpuTexture {
        data: decoded.rgba8,
        width: decoded.width,
        height: decoded.height,
        source: path.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_bilinear_sampling_wraps_uv_and_preserves_uniform_color() {
        let texture = CpuTexture {
            data: [128, 64, 32, 255].repeat(4),
            width: 2,
            height: 2,
            source: PathBuf::from("test-texture"),
        };

        let center = texture.sample_uv_linear(0.5, 0.5);
        let wrapped = texture.sample_uv_linear(1.5, -0.5);
        assert!((center[0] - srgb_to_linear(128)).abs() < 0.0001);
        assert!((center[1] - srgb_to_linear(64)).abs() < 0.0001);
        assert!((center[2] - srgb_to_linear(32)).abs() < 0.0001);
        assert_eq!(center, wrapped);
        assert_eq!(center[3], 1.0);
    }
}
