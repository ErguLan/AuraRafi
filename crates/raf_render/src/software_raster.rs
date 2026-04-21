//! Software rasterizer with per-pixel Z-buffer.
//!
//! Eliminates interpenetration artifacts that painter's algorithm cannot solve.
//! Runs entirely on CPU -- zero GPU. Opt-in via RenderConfig.depth_accurate.
//!
//! Resolution can be reduced (e.g. 0.5x) for potato hardware.
//! Default = OFF (painter sort remains the default path).
//!
//! Pipeline: triangles -> barycentric rasterization -> depth test -> framebuffer -> egui texture.

use glam::{Mat4, Vec3, Vec4};

// ---------------------------------------------------------------------------
// Framebuffer
// ---------------------------------------------------------------------------

/// CPU framebuffer with RGBA color and f32 depth per pixel.
pub struct SoftwareFramebuffer {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// RGBA color buffer (width * height * 4 bytes).
    color: Vec<u8>,
    /// Depth buffer (width * height floats). Lower = closer.
    depth: Vec<f32>,
}

impl SoftwareFramebuffer {
    /// Create a new framebuffer. Allocates once, reused across frames.
    pub fn new(width: u32, height: u32) -> Self {
        let size = (width * height) as usize;
        Self {
            width,
            height,
            color: vec![0u8; size * 4],
            depth: vec![f32::MAX; size],
        }
    }

    /// Resize if dimensions changed. Avoids reallocation if same size.
    pub fn resize(&mut self, width: u32, height: u32) {
        if self.width == width && self.height == height {
            return;
        }
        let size = (width * height) as usize;
        self.width = width;
        self.height = height;
        self.color.resize(size * 4, 0);
        self.depth.resize(size, f32::MAX);
    }

    /// Clear framebuffer with background color.
    pub fn clear(&mut self, bg_r: u8, bg_g: u8, bg_b: u8, bg_a: u8) {
        let len = (self.width * self.height) as usize;
        for i in 0..len {
            let base = i * 4;
            self.color[base] = bg_r;
            self.color[base + 1] = bg_g;
            self.color[base + 2] = bg_b;
            self.color[base + 3] = bg_a;
            self.depth[i] = f32::MAX;
        }
    }

    /// Write a single pixel if it passes the depth test.
    #[inline(always)]
    fn write_pixel(&mut self, x: u32, y: u32, z: f32, r: u8, g: u8, b: u8, a: u8) {
        if x >= self.width || y >= self.height {
            return;
        }
        let idx = (y * self.width + x) as usize;
        if z < self.depth[idx] {
            let base = idx * 4;
            if a >= 250 {
                // Opaque: update depth AND color.
                self.depth[idx] = z;
                self.color[base] = r;
                self.color[base + 1] = g;
                self.color[base + 2] = b;
                self.color[base + 3] = 255;
            } else if a > 0 {
                // Semi-transparent: blend color but DO NOT update depth.
                // This allows objects behind translucent faces to show through.
                let alpha = a as f32 / 255.0;
                let inv = 1.0 - alpha;
                self.color[base] = (r as f32 * alpha + self.color[base] as f32 * inv) as u8;
                self.color[base + 1] = (g as f32 * alpha + self.color[base + 1] as f32 * inv) as u8;
                self.color[base + 2] = (b as f32 * alpha + self.color[base + 2] as f32 * inv) as u8;
                self.color[base + 3] = a.max(self.color[base + 3]);
            }
        }
    }

    /// Get the raw RGBA pixel data for uploading to egui texture.
    pub fn pixels(&self) -> &[u8] {
        &self.color
    }

    /// Get dimensions as [width, height].
    pub fn dimensions(&self) -> [usize; 2] {
        [self.width as usize, self.height as usize]
    }
}

// ---------------------------------------------------------------------------
// Triangle rasterization
// ---------------------------------------------------------------------------

/// A triangle ready for software rasterization.
/// Screen-space coordinates + depth per vertex.
#[derive(Clone)]
pub struct RasterTriangle {
    /// Screen-space positions [x, y] for 3 vertices.
    pub screen: [[f32; 2]; 3],
    /// Clip-space depth (z/w) for 3 vertices (for interpolation).
    pub depth: [f32; 3],
    /// Face color RGBA.
    pub color: [u8; 4],
}

/// Rasterize a triangle into the framebuffer using barycentric coordinates.
///
/// Uses scanline with edge function for correct per-pixel depth interpolation.
/// Back-face culling via signed area (CCW = front-facing).
pub fn rasterize_triangle(fb: &mut SoftwareFramebuffer, tri: &RasterTriangle) {
    let [v0, v1, v2] = tri.screen;
    let [d0, d1, d2] = tri.depth;
    let [r, g, b, a] = tri.color;

    // Signed area * 2 (screen-space cross product).
    let area = edge_function(v0, v1, v2);

    // Back-face culling: negative area = CW winding = back face.
    if area <= 0.0 {
        return;
    }

    let inv_area = 1.0 / area;

    // Bounding box (clamped to framebuffer).
    let min_x = v0[0].min(v1[0]).min(v2[0]).max(0.0) as u32;
    let min_y = v0[1].min(v1[1]).min(v2[1]).max(0.0) as u32;
    let max_x = (v0[0].max(v1[0]).max(v2[0]) + 1.0).min(fb.width as f32) as u32;
    let max_y = (v0[1].max(v1[1]).max(v2[1]) + 1.0).min(fb.height as f32) as u32;

    // Early reject: triangle fully outside framebuffer.
    if max_x == 0 || max_y == 0 || min_x >= fb.width || min_y >= fb.height {
        return;
    }

    // Scanline rasterization with edge functions.
    for y in min_y..max_y {
        for x in min_x..max_x {
            let p = [x as f32 + 0.5, y as f32 + 0.5];

            // Barycentric coordinates via edge functions.
            let w0 = edge_function(v1, v2, p) * inv_area;
            let w1 = edge_function(v2, v0, p) * inv_area;
            let w2 = edge_function(v0, v1, p) * inv_area;

            // Point inside triangle if all weights >= 0.
            if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                // Interpolate depth.
                let z = w0 * d0 + w1 * d1 + w2 * d2;
                fb.write_pixel(x, y, z, r, g, b, a);
            }
        }
    }
}

/// Rasterize a quad as two triangles (0-1-2, 0-2-3).
pub fn rasterize_quad(
    fb: &mut SoftwareFramebuffer,
    screen: &[[f32; 2]; 4],
    depth: &[f32; 4],
    color: [u8; 4],
) {
    rasterize_triangle(fb, &RasterTriangle {
        screen: [screen[0], screen[1], screen[2]],
        depth: [depth[0], depth[1], depth[2]],
        color,
    });
    rasterize_triangle(fb, &RasterTriangle {
        screen: [screen[0], screen[2], screen[3]],
        depth: [depth[0], depth[2], depth[3]],
        color,
    });
}

/// Rasterize a wireframe line (Bresenham) with depth test.
/// Used for wireframe mode and selection outlines.
pub fn rasterize_line(
    fb: &mut SoftwareFramebuffer,
    p0: [f32; 2], d0: f32,
    p1: [f32; 2], d1: f32,
    color: [u8; 4],
    width: f32,
) {
    let dx = p1[0] - p0[0];
    let dy = p1[1] - p0[1];
    let steps = dx.abs().max(dy.abs()) as i32;

    if steps <= 0 {
        return;
    }

    let x_inc = dx / steps as f32;
    let y_inc = dy / steps as f32;
    let d_inc = (d1 - d0) / steps as f32;

    let half_w = (width * 0.5).max(0.5) as i32;

    let mut x = p0[0];
    let mut y = p0[1];
    let mut d = d0;

    for _ in 0..=steps {
        let px = x as i32;
        let py = y as i32;

        // Draw thick line via small rect.
        for wy in -half_w..=half_w {
            for wx in -half_w..=half_w {
                let fx = (px + wx) as u32;
                let fy = (py + wy) as u32;
                fb.write_pixel(fx, fy, d - 0.001, color[0], color[1], color[2], color[3]);
            }
        }

        x += x_inc;
        y += y_inc;
        d += d_inc;
    }
}

// ---------------------------------------------------------------------------
// Projection helpers (world -> screen + depth for rasterizer)
// ---------------------------------------------------------------------------

/// Project a world-space quad through model + view_proj matrices.
/// Returns screen coordinates + per-vertex depth for the rasterizer.
/// Performs near-plane clipping: if a vertex is behind the camera,
/// it is clamped to the near plane instead of discarding the whole quad.
/// Returns None only if ALL vertices are behind the camera.
pub fn project_quad_for_raster(
    corners: &[Vec3; 4],
    model: &Mat4,
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
) -> Option<([[f32; 2]; 4], [f32; 4])> {
    let mut screen = [[0.0f32; 2]; 4];
    let mut depth = [0.0f32; 4];
    let mut behind_count = 0;
    const NEAR_W: f32 = 0.01;

    for (i, corner) in corners.iter().enumerate() {
        let world = (*model * corner.extend(1.0)).truncate();
        let clip = *view_proj * Vec4::new(world.x, world.y, world.z, 1.0);

        // Clamp vertices behind camera to near plane instead of discarding.
        let w = if clip.w <= NEAR_W {
            behind_count += 1;
            NEAR_W
        } else {
            clip.w
        };

        let ndc_x = clip.x / w;
        let ndc_y = clip.y / w;

        screen[i] = [
            (ndc_x + 1.0) * 0.5 * vp_w,
            (1.0 - ndc_y) * 0.5 * vp_h,
        ];
        depth[i] = clip.z / w;
    }

    // Only discard if ALL 4 vertices are behind camera.
    if behind_count >= 4 {
        return None;
    }

    Some((screen, depth))
}

/// Project a single world-space point. Returns (screen_xy, depth).
pub fn project_point_for_raster(
    point: Vec3,
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
) -> Option<([f32; 2], f32)> {
    let clip = *view_proj * Vec4::new(point.x, point.y, point.z, 1.0);
    if clip.w <= 0.001 {
        return None;
    }
    let ndc_x = clip.x / clip.w;
    let ndc_y = clip.y / clip.w;
    Some((
        [(ndc_x + 1.0) * 0.5 * vp_w, (1.0 - ndc_y) * 0.5 * vp_h],
        clip.z / clip.w,
    ))
}

// ---------------------------------------------------------------------------
// Selection outline
// ---------------------------------------------------------------------------

/// Draw a selection highlight outline around screen-space quad edges.
/// Color is typically orange/yellow for selected entities.
pub fn rasterize_selection_outline(
    fb: &mut SoftwareFramebuffer,
    screen: &[[f32; 2]; 4],
    depth: &[f32; 4],
    color: [u8; 4],
    line_width: f32,
) {
    // Draw 4 edges of the quad.
    for i in 0..4 {
        let j = (i + 1) % 4;
        rasterize_line(fb, screen[i], depth[i], screen[j], depth[j], color, line_width);
    }
}

// ---------------------------------------------------------------------------
// Internal: edge function for barycentric coords
// ---------------------------------------------------------------------------

/// 2D edge function: returns signed area of the parallelogram formed by (b-a) x (c-a).
/// Positive = CCW (front-facing), Negative = CW (back-facing).
#[inline(always)]
fn edge_function(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framebuffer_clear() {
        let mut fb = SoftwareFramebuffer::new(4, 4);
        fb.clear(255, 128, 64, 255);
        assert_eq!(fb.pixels()[0], 255);
        assert_eq!(fb.pixels()[1], 128);
        assert_eq!(fb.pixels()[2], 64);
        assert_eq!(fb.pixels()[3], 255);
    }

    #[test]
    fn depth_test_closer_wins() {
        let mut fb = SoftwareFramebuffer::new(4, 4);
        fb.clear(0, 0, 0, 255);
        // Write far pixel.
        fb.write_pixel(1, 1, 0.9, 255, 0, 0, 255);
        let idx = (1 * 4 + 1) * 4;
        assert_eq!(fb.color[idx], 255); // Red.
        // Write closer pixel (should overwrite).
        fb.write_pixel(1, 1, 0.1, 0, 255, 0, 255);
        assert_eq!(fb.color[idx], 0);
        assert_eq!(fb.color[idx + 1], 255); // Green wins.
        // Write farther pixel (should NOT overwrite).
        fb.write_pixel(1, 1, 0.5, 0, 0, 255, 255);
        assert_eq!(fb.color[idx + 1], 255); // Still green.
    }

    #[test]
    fn edge_function_ccw() {
        let a = [0.0, 0.0];
        let b = [1.0, 0.0];
        let c = [0.0, 1.0];
        assert!(edge_function(a, b, c) > 0.0); // CCW = positive.
    }

    #[test]
    fn edge_function_cw() {
        let a = [0.0, 0.0];
        let b = [0.0, 1.0];
        let c = [1.0, 0.0];
        assert!(edge_function(a, b, c) < 0.0); // CW = negative.
    }

    #[test]
    fn rasterize_small_triangle() {
        let mut fb = SoftwareFramebuffer::new(10, 10);
        fb.clear(0, 0, 0, 255);
        let tri = RasterTriangle {
            screen: [[2.0, 2.0], [8.0, 2.0], [5.0, 8.0]],
            depth: [0.5, 0.5, 0.5],
            color: [255, 0, 0, 255],
        };
        rasterize_triangle(&mut fb, &tri);
        // Center pixel (5,4) should be red.
        let idx = (4 * 10 + 5) * 4;
        assert_eq!(fb.color[idx], 255);
    }
}
