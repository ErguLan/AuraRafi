//! RGBA color buffer + depth buffer for CPU rendering.
//!
//! The framebuffer is the render target for the scanline rasterizer.
//! It manages a pixel grid with per-pixel color (RGBA u8) and depth (f32).
//!
//! Depth convention: values in [0.0, 1.0] after perspective divide,
//! where 0.0 = near plane, 1.0 = far plane. Closer pixels have smaller
//! depth values and pass the depth test.

use std::slice;

/// CPU framebuffer with color and depth.
pub struct Framebuffer {
    width: u32,
    height: u32,
    /// Packed RGBA pixels, row-major, top-left origin. Length = width * height.
    color: Vec<u32>,
    /// Per-pixel depth values. Length = width * height. Initialized to f32::MAX.
    depth: Vec<f32>,
}

impl Framebuffer {
    /// Create a new framebuffer with the given dimensions.
    pub fn new(width: u32, height: u32) -> Self {
        let pixel_count = (width * height) as usize;
        Self {
            width,
            height,
            color: vec![0; pixel_count],
            depth: vec![f32::MAX; pixel_count],
        }
    }

    /// Resize the framebuffer. Reuses allocation if capacity is sufficient.
    pub fn resize(&mut self, width: u32, height: u32) {
        if self.width == width && self.height == height {
            return;
        }
        self.width = width;
        self.height = height;
        let pixel_count = (width * height) as usize;
        self.color.resize(pixel_count, 0);
        self.depth.resize(pixel_count, f32::MAX);
    }

    /// Clear the framebuffer with a background color and reset depth.
    pub fn clear(&mut self, r: u8, g: u8, b: u8, a: u8) {
        self.color.fill(pack_rgba(r, g, b, a));
        self.depth.fill(f32::MAX);
    }

    /// Write a pixel with depth test.
    /// Returns true if the pixel was written (passed depth test).
    #[inline]
    pub fn write_pixel(&mut self, x: u32, y: u32, z: f32, r: u8, g: u8, b: u8, a: u8) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }

        let idx = (y * self.width + x) as usize;

        // Depth test: closer pixels (smaller z) pass
        if z >= self.depth[idx] {
            return false;
        }

        self.depth[idx] = z;
        self.color[idx] = pack_rgba(r, g, b, a);
        true
    }

    /// Write a pixel bypassing the depth test entirely, without modifying the depth buffer.
    #[inline]
    pub fn write_pixel_no_depth(&mut self, x: u32, y: u32, r: u8, g: u8, b: u8, a: u8) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }

        let idx = (y * self.width + x) as usize;
        self.color[idx] = pack_rgba(r, g, b, a);
        true
    }

    /// Write a pixel without bounds checks.
    /// Caller must guarantee idx is in range and that x/y were already clipped.
    #[inline(always)]
    pub fn write_pixel_unchecked(&mut self, idx: usize, z: f32, packed: u32) {
        unsafe {
            let depth = self.depth.get_unchecked_mut(idx);
            if z < *depth {
                *depth = z;
                *self.color.get_unchecked_mut(idx) = packed;
            }
        }
    }

    /// Get the raw RGBA pixel data (for uploading to egui texture).
    pub fn pixels(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.color.as_ptr() as *const u8, self.color.len() * 4) }
    }

    /// Framebuffer width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Framebuffer height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Write a pixel with alpha blending over the existing color.
    ///
    /// Uses standard source-over compositing: result = src * alpha + dst * (1 - alpha).
    /// Depth test still applies: the pixel is only blended if it passes.
    /// The depth buffer is NOT updated for transparent pixels (a < 255)
    /// so that other transparent objects behind can still blend through.
    #[inline]
    pub fn blend_pixel(&mut self, x: u32, y: u32, z: f32, r: u8, g: u8, b: u8, a: u8) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }

        let idx = (y * self.width + x) as usize;

        // Depth test: same as opaque, closer pixels pass
        if z >= self.depth[idx] {
            return false;
        }

        let alpha = a as f32 / 255.0;
        let inv_alpha = 1.0 - alpha;

        let [dst_r, dst_g, dst_b, _] = unpack_rgba(self.color[idx]);
        self.color[idx] = pack_rgba(
            (r as f32 * alpha + dst_r as f32 * inv_alpha) as u8,
            (g as f32 * alpha + dst_g as f32 * inv_alpha) as u8,
            (b as f32 * alpha + dst_b as f32 * inv_alpha) as u8,
            255,
        );

        // Only update depth for fully opaque pixels
        if a == 255 {
            self.depth[idx] = z;
        }

        true
    }

    /// Read the depth value at a pixel coordinate.
    /// Returns f32::MAX if out of bounds.
    #[inline]
    pub fn depth_at(&self, x: u32, y: u32) -> f32 {
        if x >= self.width || y >= self.height {
            return f32::MAX;
        }
        self.depth[(y * self.width + x) as usize]
    }
}

#[inline]
fn pack_rgba(r: u8, g: u8, b: u8, a: u8) -> u32 {
    #[cfg(target_endian = "little")]
    {
        u32::from_ne_bytes([r, g, b, a])
    }
    #[cfg(target_endian = "big")]
    {
        u32::from_ne_bytes([a, b, g, r])
    }
}

#[inline]
fn unpack_rgba(color: u32) -> [u8; 4] {
    #[cfg(target_endian = "little")]
    {
        color.to_ne_bytes()
    }
    #[cfg(target_endian = "big")]
    {
        let [a, b, g, r] = color.to_ne_bytes();
        [r, g, b, a]
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_sets_color() {
        let mut fb = Framebuffer::new(2, 2);
        fb.clear(255, 128, 64, 255);
        assert_eq!(fb.pixels()[0], 255);
        assert_eq!(fb.pixels()[1], 128);
        assert_eq!(fb.pixels()[2], 64);
        assert_eq!(fb.pixels()[3], 255);
    }

    #[test]
    fn depth_test_pass() {
        let mut fb = Framebuffer::new(2, 2);
        fb.clear(0, 0, 0, 255);
        assert!(fb.write_pixel(0, 0, 0.5, 255, 0, 0, 255));
        assert_eq!(fb.pixels()[0], 255); // Red written
    }

    #[test]
    fn depth_test_fail() {
        let mut fb = Framebuffer::new(2, 2);
        fb.clear(0, 0, 0, 255);
        fb.write_pixel(0, 0, 0.3, 255, 0, 0, 255); // Closer
        assert!(!fb.write_pixel(0, 0, 0.5, 0, 255, 0, 255)); // Farther, rejected
        assert_eq!(fb.pixels()[0], 255); // Still red
    }

    #[test]
    fn depth_test_closer_overwrites() {
        let mut fb = Framebuffer::new(2, 2);
        fb.clear(0, 0, 0, 255);
        fb.write_pixel(0, 0, 0.5, 255, 0, 0, 255); // Red at 0.5
        assert!(fb.write_pixel(0, 0, 0.3, 0, 255, 0, 255)); // Green closer
        assert_eq!(fb.pixels()[0], 0);
        assert_eq!(fb.pixels()[1], 255); // Green overwrote
    }

    #[test]
    fn resize_clears() {
        let mut fb = Framebuffer::new(2, 2);
        fb.resize(4, 4);
        assert_eq!(fb.width(), 4);
        assert_eq!(fb.height(), 4);
        assert_eq!(fb.pixels().len(), 4 * 4 * 4);
    }
}
