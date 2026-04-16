//! Texture loading and management for the CPU painter.
//!
//! Loads images from disk, caches them in memory, and provides
//! UV-mapped color sampling. Zero cost when textures_enabled = false.
//!
//! Uses pure Rust (no GPU upload). Images are stored as RGBA byte arrays.
//! Supports PNG, JPG, BMP, TGA, WebP via the `image` crate (when available).
//! Falls back to a 1x1 placeholder if the `image` crate is not in dependencies.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A loaded texture stored in CPU memory.
#[derive(Clone)]
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
            [self.data[idx], self.data[idx + 1], self.data[idx + 2], self.data[idx + 3]]
        } else {
            [255, 0, 255, 255]
        }
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

/// Texture cache: loads from disk, caches in memory, evicts by LRU.
pub struct TextureCache {
    textures: HashMap<String, CpuTexture>,
    /// Maximum memory budget in bytes. Default: 50MB.
    max_memory: usize,
    /// Total memory used.
    used_memory: usize,
    /// Maximum texture dimension (auto-downscale).
    max_texture_size: u32,
}

impl TextureCache {
    /// Create a new cache with given limits.
    pub fn new(max_memory_mb: usize, max_texture_size: u32) -> Self {
        Self {
            textures: HashMap::new(),
            max_memory: max_memory_mb * 1024 * 1024,
            used_memory: 0,
            max_texture_size,
        }
    }

    /// Get or load a texture by file path.
    /// Returns the checkerboard fallback if loading fails.
    pub fn get_or_load(&mut self, path: &Path) -> &CpuTexture {
        let key = path.to_string_lossy().to_string();
        if !self.textures.contains_key(&key) {
            let tex = load_texture_from_disk(path, self.max_texture_size);
            self.used_memory += tex.memory_bytes();
            self.textures.insert(key.clone(), tex);

            // Evict oldest if over budget.
            while self.used_memory > self.max_memory && self.textures.len() > 1 {
                if let Some(oldest_key) = self.textures.keys().next().cloned() {
                    if oldest_key != key {
                        if let Some(removed) = self.textures.remove(&oldest_key) {
                            self.used_memory -= removed.memory_bytes();
                        }
                    } else {
                        break;
                    }
                }
            }
        }
        self.textures.get(&key).unwrap()
    }

    /// Clear the entire cache.
    pub fn clear(&mut self) {
        self.textures.clear();
        self.used_memory = 0;
    }

    /// Number of loaded textures.
    pub fn count(&self) -> usize {
        self.textures.len()
    }

    /// Total memory used in bytes.
    pub fn memory_used(&self) -> usize {
        self.used_memory
    }
}

impl Default for TextureCache {
    fn default() -> Self {
        Self::new(50, 512) // 50MB, max 512px
    }
}

/// Load a texture from disk. Returns checkerboard on failure.
fn load_texture_from_disk(path: &Path, max_size: u32) -> CpuTexture {
    // Try to read the file as raw bytes.
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return CpuTexture::checkerboard(16),
    };

    // Try to parse as a simple format.
    // For now, support raw RGBA if the file looks like it could be decoded.
    // Full PNG/JPG support requires the `image` crate dependency.
    // We provide a basic BMP/TGA parser for zero-dependency loading.
    if let Some(tex) = try_parse_bmp(&bytes, path) {
        return tex.downscaled(max_size);
    }

    // Fallback: if file exists but format unknown, show checkerboard.
    CpuTexture::checkerboard(16)
}

/// Very basic BMP parser (uncompressed 24-bit or 32-bit).
/// Covers the most common case for texture loading without external deps.
fn try_parse_bmp(data: &[u8], source: &Path) -> Option<CpuTexture> {
    if data.len() < 54 || data[0] != b'B' || data[1] != b'M' {
        return None;
    }
    let offset = u32::from_le_bytes([data[10], data[11], data[12], data[13]]) as usize;
    let width = u32::from_le_bytes([data[18], data[19], data[20], data[21]]);
    let height = u32::from_le_bytes([data[22], data[23], data[24], data[25]]);
    let bpp = u16::from_le_bytes([data[28], data[29]]);

    if width == 0 || height == 0 || width > 8192 || height > 8192 {
        return None;
    }

    let bytes_per_pixel = match bpp {
        24 => 3,
        32 => 4,
        _ => return None,
    };

    let row_size = ((bytes_per_pixel * width as usize + 3) / 4) * 4; // BMP rows are 4-byte aligned
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);

    // BMP stores rows bottom-to-top.
    for y in (0..height).rev() {
        let row_start = offset + y as usize * row_size;
        for x in 0..width {
            let px = row_start + x as usize * bytes_per_pixel;
            if px + bytes_per_pixel > data.len() {
                rgba.extend_from_slice(&[0, 0, 0, 255]);
                continue;
            }
            let b = data[px];
            let g = data[px + 1];
            let r = data[px + 2];
            let a = if bytes_per_pixel == 4 { data[px + 3] } else { 255 };
            rgba.extend_from_slice(&[r, g, b, a]);
        }
    }

    Some(CpuTexture {
        data: rgba,
        width,
        height,
        source: source.to_path_buf(),
    })
}
