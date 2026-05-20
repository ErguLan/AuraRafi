//! Scanline triangle rasterizer with edge-walking.
//!
//! Rasterizes triangles defined in screen-space coordinates into the
//! framebuffer using the classic scanline algorithm with edge walking.
//!
//! Why scanline instead of the old barycentric brute-force:
//! - The old approach iterated every pixel in the bounding box and tested
//!   3 edge functions per pixel. For a triangle covering 10% of its bbox,
//!   90% of those tests were wasted.
//! - Scanline edge-walking only visits pixels actually inside the triangle.
//!   The slopes are computed once per edge, then incremented per scanline.
//!   This is 5-20x fewer operations for typical triangles.
//!
//! Algorithm:
//! 1. Sort triangle vertices by Y (top, mid, bottom)
//! 2. Split into upper and lower halves at the middle vertex
//! 3. For each scanline Y, compute left/right X boundaries by edge walking
//! 4. Fill pixels between left and right, interpolating Z for depth test

use super::framebuffer::Framebuffer;

/// A screen-space vertex ready for rasterization.
#[derive(Debug, Clone, Copy)]
pub struct ScreenVertex {
    /// Screen X (pixels, 0 = left).
    pub x: f32,
    /// Screen Y (pixels, 0 = top).
    pub y: f32,
    /// Normalized depth (0.0 = near, 1.0 = far). Used for Z-buffer.
    pub z: f32,
    /// Per-vertex lighting factor for Gouraud shading.
    pub shade: f32,
}

/// Rasterize a single triangle into the framebuffer.
///
/// Vertices must be in screen space (pixels, top-left origin).
/// The triangle is filled with a flat color and Z values are interpolated
/// linearly for depth testing.
///
/// Backface culling: if the signed area is <= 0 (clockwise in screen space),
/// the triangle is skipped. Front faces are CCW in screen space.
pub fn rasterize_triangle(
    fb: &mut Framebuffer,
    v0: ScreenVertex,
    v1: ScreenVertex,
    v2: ScreenVertex,
    r: u8, g: u8, b: u8, a: u8,
) {
    // Backface cull: compute signed area (2x cross product in screen space).
    // In screen space Y points DOWN, so the sign is inverted compared to
    // math convention. Negative area = CCW visually = front face.
    let area = (v1.x - v0.x) * (v2.y - v0.y) - (v2.x - v0.x) * (v1.y - v0.y);
    if area >= 0.0 {
        return; // Backface or degenerate
    }

    // Sort vertices by Y (top to bottom: smallest Y first).
    let mut sorted = [v0, v1, v2];
    if sorted[0].y > sorted[1].y { sorted.swap(0, 1); }
    if sorted[1].y > sorted[2].y { sorted.swap(1, 2); }
    if sorted[0].y > sorted[1].y { sorted.swap(0, 1); }

    let [top, mid, bot] = sorted;

    let fb_w = fb.width() as f32;
    let fb_h = fb.height() as f32;

    // Upper half: top -> mid
    let height_upper = mid.y - top.y;
    if height_upper > 0.5 {
        fill_scanlines(fb, &top, &mid, &bot, height_upper, true, fb_w, fb_h, r, g, b, a);
    }

    // Lower half: mid -> bot
    let height_lower = bot.y - mid.y;
    if height_lower > 0.5 {
        fill_scanlines(fb, &top, &mid, &bot, height_lower, false, fb_w, fb_h, r, g, b, a);
    }
}

/// Fill scanlines for one half of the triangle (upper or lower).
fn fill_scanlines(
    fb: &mut Framebuffer,
    top: &ScreenVertex,
    mid: &ScreenVertex,
    bot: &ScreenVertex,
    _half_height: f32,
    is_upper: bool,
    fb_w: f32,
    fb_h: f32,
    r: u8, g: u8, b: u8, a: u8,
) {
    let total_height = bot.y - top.y;
    if total_height < 0.5 {
        return;
    }
    let inv_total = 1.0 / total_height;

    let (y_start, y_end) = if is_upper {
        (top.y.ceil().max(0.0) as u32, mid.y.ceil().min(fb_h) as u32)
    } else {
        (mid.y.ceil().max(0.0) as u32, bot.y.ceil().min(fb_h) as u32)
    };

    for y in y_start..y_end {
        let yf = y as f32 + 0.5;

        // Long edge: top -> bot (spans the full triangle height)
        let t_long = (yf - top.y) * inv_total;
        let x_long = top.x + (bot.x - top.x) * t_long;
        let z_long = top.z + (bot.z - top.z) * t_long;
        let shade_long = top.shade + (bot.shade - top.shade) * t_long;

        // Short edge: depends on upper/lower half
        let (x_short, z_short, shade_short) = if is_upper {
            let seg_h = mid.y - top.y;
            if seg_h < 0.5 {
                continue;
            }
            let t_short = (yf - top.y) / seg_h;
            (
                top.x + (mid.x - top.x) * t_short,
                top.z + (mid.z - top.z) * t_short,
                top.shade + (mid.shade - top.shade) * t_short,
            )
        } else {
            let seg_h = bot.y - mid.y;
            if seg_h < 0.5 {
                continue;
            }
            let t_short = (yf - mid.y) / seg_h;
            (
                mid.x + (bot.x - mid.x) * t_short,
                mid.z + (bot.z - mid.z) * t_short,
                mid.shade + (bot.shade - mid.shade) * t_short,
            )
        };

        // Determine left and right X
        let (xl, xr, zl, zr, shade_l, shade_r) = if x_long < x_short {
            (x_long, x_short, z_long, z_short, shade_long, shade_short)
        } else {
            (x_short, x_long, z_short, z_long, shade_short, shade_long)
        };

        let x_start = xl.ceil().max(0.0) as u32;
        let x_end = xr.ceil().min(fb_w) as u32;

        if x_start >= x_end {
            continue;
        }

        let span = xr - xl;
        if span < 0.5 {
            // Single pixel span
            let z = (zl + zr) * 0.5;
            let shade = ((shade_l + shade_r) * 0.5).clamp(0.0, 1.0);
            let packed = pack_rgba_inline(
                (r as f32 * shade).min(255.0) as u8,
                (g as f32 * shade).min(255.0) as u8,
                (b as f32 * shade).min(255.0) as u8,
                a,
            );
            let idx = (y * fb.width() + x_start) as usize;
            fb.write_pixel_unchecked(idx, z, packed);
            continue;
        }

        let inv_span = 1.0 / span;
        let rf = r as f32;
        let gf = g as f32;
        let bf = b as f32;
        let rl = rf * shade_l;
        let gl = gf * shade_l;
        let bl = bf * shade_l;
        let dr = rf * (shade_r - shade_l);
        let dg = gf * (shade_r - shade_l);
        let db = bf * (shade_r - shade_l);
        let row_base = (y * fb.width()) as usize;

        for x in x_start..x_end {
            let t = ((x as f32 + 0.5) - xl) * inv_span;
            let z = zl + (zr - zl) * t;
            let packed = pack_rgba_inline(
                (rl + dr * t).min(255.0) as u8,
                (gl + dg * t).min(255.0) as u8,
                (bl + db * t).min(255.0) as u8,
                a,
            );
            fb.write_pixel_unchecked(row_base + x as usize, z, packed);
        }
    }
}

/// Rasterize a triangle with alpha blending (for transparent objects).
///
/// Same algorithm as rasterize_triangle but uses blend_pixel for compositing.
/// Kept as a separate function to avoid branching overhead in the opaque path.
pub fn rasterize_triangle_blended(
    fb: &mut Framebuffer,
    v0: ScreenVertex,
    v1: ScreenVertex,
    v2: ScreenVertex,
    r: u8, g: u8, b: u8, a: u8,
) {
    // Backface cull (same Y-down convention)
    let area = (v1.x - v0.x) * (v2.y - v0.y) - (v2.x - v0.x) * (v1.y - v0.y);
    if area >= 0.0 {
        return;
    }

    let mut sorted = [v0, v1, v2];
    if sorted[0].y > sorted[1].y { sorted.swap(0, 1); }
    if sorted[1].y > sorted[2].y { sorted.swap(1, 2); }
    if sorted[0].y > sorted[1].y { sorted.swap(0, 1); }

    let [top, mid, bot] = sorted;
    let fb_w = fb.width() as f32;
    let fb_h = fb.height() as f32;

    let total_height = bot.y - top.y;
    if total_height < 0.5 {
        return;
    }
    let inv_total = 1.0 / total_height;

    // Upper half
    let y_start = top.y.ceil().max(0.0) as u32;
    let y_mid = mid.y.ceil().min(fb_h) as u32;
    let y_end = bot.y.ceil().min(fb_h) as u32;

    for y in y_start..y_mid {
        let yf = y as f32 + 0.5;
        let t_long = (yf - top.y) * inv_total;
        let x_long = top.x + (bot.x - top.x) * t_long;
        let z_long = top.z + (bot.z - top.z) * t_long;
        let shade_long = top.shade + (bot.shade - top.shade) * t_long;

        let seg_h = mid.y - top.y;
        if seg_h < 0.5 { continue; }
        let t_short = (yf - top.y) / seg_h;
        let x_short = top.x + (mid.x - top.x) * t_short;
        let z_short = top.z + (mid.z - top.z) * t_short;
        let shade_short = top.shade + (mid.shade - top.shade) * t_short;

        fill_span_blended(
            fb,
            y,
            x_long,
            z_long,
            shade_long,
            x_short,
            z_short,
            shade_short,
            fb_w,
            r,
            g,
            b,
            a,
        );
    }

    // Lower half
    for y in y_mid..y_end {
        let yf = y as f32 + 0.5;
        let t_long = (yf - top.y) * inv_total;
        let x_long = top.x + (bot.x - top.x) * t_long;
        let z_long = top.z + (bot.z - top.z) * t_long;
        let shade_long = top.shade + (bot.shade - top.shade) * t_long;

        let seg_h = bot.y - mid.y;
        if seg_h < 0.5 { continue; }
        let t_short = (yf - mid.y) / seg_h;
        let x_short = mid.x + (bot.x - mid.x) * t_short;
        let z_short = mid.z + (bot.z - mid.z) * t_short;
        let shade_short = mid.shade + (bot.shade - mid.shade) * t_short;

        fill_span_blended(
            fb,
            y,
            x_long,
            z_long,
            shade_long,
            x_short,
            z_short,
            shade_short,
            fb_w,
            r,
            g,
            b,
            a,
        );
    }
}

/// Fill a single scanline span with alpha blending.
#[inline]
fn fill_span_blended(
    fb: &mut Framebuffer,
    y: u32,
    x_a: f32, z_a: f32, shade_a: f32,
    x_b: f32, z_b: f32, shade_b: f32,
    fb_w: f32,
    r: u8, g: u8, b: u8, a: u8,
) {
    let (xl, xr, zl, zr, shade_l, shade_r) = if x_a < x_b {
        (x_a, x_b, z_a, z_b, shade_a, shade_b)
    } else {
        (x_b, x_a, z_b, z_a, shade_b, shade_a)
    };

    let x_start = xl.ceil().max(0.0) as u32;
    let x_end = xr.ceil().min(fb_w) as u32;
    if x_start >= x_end { return; }

    let span = xr - xl;
    if span < 0.5 {
        let shade = ((shade_l + shade_r) * 0.5).clamp(0.0, 1.0);
        let [pr, pg, pb] = apply_shade([r, g, b], shade);
        fb.blend_pixel(x_start, y, (zl + zr) * 0.5, pr, pg, pb, a);
        return;
    }

    let inv_span = 1.0 / span;
    for x in x_start..x_end {
        let t = ((x as f32 + 0.5) - xl) * inv_span;
        let z = zl + (zr - zl) * t;
        let shade = (shade_l + (shade_r - shade_l) * t).clamp(0.0, 1.0);
        let [pr, pg, pb] = apply_shade([r, g, b], shade);
        fb.blend_pixel(x, y, z, pr, pg, pb, a);
    }
}

#[inline]
fn apply_shade(color: [u8; 3], shade: f32) -> [u8; 3] {
    [
        (color[0] as f32 * shade).min(255.0) as u8,
        (color[1] as f32 * shade).min(255.0) as u8,
        (color[2] as f32 * shade).min(255.0) as u8,
    ]
}

#[inline(always)]
fn pack_rgba_inline(r: u8, g: u8, b: u8, a: u8) -> u32 {
    #[cfg(target_endian = "little")]
    {
        u32::from_ne_bytes([r, g, b, a])
    }
    #[cfg(target_endian = "big")]
    {
        u32::from_ne_bytes([a, b, g, r])
    }
}

/// Rasterize an edge (line) into the framebuffer with a given color.
///
/// Uses Bresenham's line algorithm for clean 1-pixel-wide lines.
/// Depth is linearly interpolated along the line.
pub fn rasterize_line(
    fb: &mut Framebuffer,
    mut x0: f32, mut y0: f32, mut z0: f32,
    mut x1: f32, mut y1: f32, mut z1: f32,
    r: u8, g: u8, b: u8, a: u8,
) {
    let w = fb.width() as f32;
    let h = fb.height() as f32;

    const INSIDE: u8 = 0; // 0000
    const LEFT: u8 = 1;   // 0001
    const RIGHT: u8 = 2;  // 0010
    const BOTTOM: u8 = 4; // 0100
    const TOP: u8 = 8;    // 1000

    let compute_code = |x: f32, y: f32| -> u8 {
        let mut code = INSIDE;
        if x < 0.0 {
            code |= LEFT;
        } else if x >= w {
            code |= RIGHT;
        }
        if y < 0.0 {
            code |= TOP;
        } else if y >= h {
            code |= BOTTOM;
        }
        code
    };

    let mut code0 = compute_code(x0, y0);
    let mut code1 = compute_code(x1, y1);

    let orig_x0 = x0;
    let orig_y0 = y0;
    let orig_x1 = x1;
    let orig_y1 = y1;

    let mut accept = false;

    for _ in 0..8 {
        if (code0 | code1) == 0 {
            accept = true;
            break;
        } else if (code0 & code1) != 0 {
            break;
        } else {
            let outcode = if code0 != 0 { code0 } else { code1 };
            let mut x = 0.0;
            let mut y = 0.0;

            if (outcode & TOP) != 0 {
                x = x0 + (x1 - x0) * (0.0 - y0) / (y1 - y0);
                y = 0.0;
            } else if (outcode & BOTTOM) != 0 {
                let max_y = h - 0.001;
                x = x0 + (x1 - x0) * (max_y - y0) / (y1 - y0);
                y = max_y;
            } else if (outcode & RIGHT) != 0 {
                let max_x = w - 0.001;
                y = y0 + (y1 - y0) * (max_x - x0) / (x1 - x0);
                x = max_x;
            } else if (outcode & LEFT) != 0 {
                y = y0 + (y1 - y0) * (0.0 - x0) / (x1 - x0);
                x = 0.0;
            }

            if outcode == code0 {
                x0 = x;
                y0 = y;
                code0 = compute_code(x0, y0);
            } else {
                x1 = x;
                y1 = y;
                code1 = compute_code(x1, y1);
            }
        }
    }

    if !accept {
        return;
    }

    // Interpolate z0 and z1 based on the clipped x0, y0 and x1, y1
    let dx_orig = orig_x1 - orig_x0;
    let dy_orig = orig_y1 - orig_y0;
    let len_sq = dx_orig * dx_orig + dy_orig * dy_orig;
    if len_sq > 1e-6 {
        let t0 = ((x0 - orig_x0) * dx_orig + (y0 - orig_y0) * dy_orig) / len_sq;
        let t1 = ((x1 - orig_x0) * dx_orig + (y1 - orig_y0) * dy_orig) / len_sq;
        let orig_z0 = z0;
        z0 = orig_z0 + (z1 - orig_z0) * t0.clamp(0.0, 1.0);
        z1 = orig_z0 + (z1 - orig_z0) * t1.clamp(0.0, 1.0);
    }

    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let steps = dx.max(dy).ceil() as u32;

    if steps == 0 {
        fb.write_pixel(x0 as u32, y0 as u32, z0, r, g, b, a);
        return;
    }

    let inv_steps = 1.0 / steps as f32;
    for i in 0..=steps {
        let t = i as f32 * inv_steps;
        let x = x0 + (x1 - x0) * t;
        let y = y0 + (y1 - y0) * t;
        let z = z0 + (z1 - z0) * t;
        let xi = x as u32;
        let yi = y as u32;
        fb.write_pixel(xi, yi, z, r, g, b, a);
    }
}

pub fn rasterize_line_no_depth(
    fb: &mut Framebuffer,
    mut x0: f32, mut y0: f32,
    mut x1: f32, mut y1: f32,
    r: u8, g: u8, b: u8, a: u8,
) {
    let w = fb.width() as f32;
    let h = fb.height() as f32;

    const INSIDE: u8 = 0; // 0000
    const LEFT: u8 = 1;   // 0001
    const RIGHT: u8 = 2;  // 0010
    const BOTTOM: u8 = 4; // 0100
    const TOP: u8 = 8;    // 1000

    let compute_code = |x: f32, y: f32| -> u8 {
        let mut code = INSIDE;
        if x < 0.0 {
            code |= LEFT;
        } else if x >= w {
            code |= RIGHT;
        }
        if y < 0.0 {
            code |= TOP;
        } else if y >= h {
            code |= BOTTOM;
        }
        code
    };

    let mut code0 = compute_code(x0, y0);
    let mut code1 = compute_code(x1, y1);

    let mut accept = false;

    for _ in 0..8 {
        if (code0 | code1) == 0 {
            accept = true;
            break;
        } else if (code0 & code1) != 0 {
            break;
        } else {
            let outcode = if code0 != 0 { code0 } else { code1 };
            let mut x = 0.0;
            let mut y = 0.0;

            if (outcode & TOP) != 0 {
                x = x0 + (x1 - x0) * (0.0 - y0) / (y1 - y0);
                y = 0.0;
            } else if (outcode & BOTTOM) != 0 {
                let max_y = h - 0.001;
                x = x0 + (x1 - x0) * (max_y - y0) / (y1 - y0);
                y = max_y;
            } else if (outcode & RIGHT) != 0 {
                let max_x = w - 0.001;
                y = y0 + (y1 - y0) * (max_x - x0) / (x1 - x0);
                x = max_x;
            } else if (outcode & LEFT) != 0 {
                y = y0 + (y1 - y0) * (0.0 - x0) / (x1 - x0);
                x = 0.0;
            }

            if outcode == code0 {
                x0 = x;
                y0 = y;
                code0 = compute_code(x0, y0);
            } else {
                x1 = x;
                y1 = y;
                code1 = compute_code(x1, y1);
            }
        }
    }

    if !accept {
        return;
    }

    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let steps = dx.max(dy).ceil() as u32;

    if steps == 0 {
        fb.write_pixel_no_depth(x0 as u32, y0 as u32, r, g, b, a);
        return;
    }

    let inv_steps = 1.0 / steps as f32;
    for i in 0..=steps {
        let t = i as f32 * inv_steps;
        let x = x0 + (x1 - x0) * t;
        let y = y0 + (y1 - y0) * t;
        let xi = x as u32;
        let yi = y as u32;
        fb.write_pixel_no_depth(xi, yi, r, g, b, a);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cull_backface() {
        let mut fb = Framebuffer::new(100, 100);
        fb.clear(0, 0, 0, 255);

        // CW-visual triangle in screen space (backface) -- should be culled.
        // v0=(50,10), v1=(90,90), v2=(10,90) in CW order visually.
        let v0 = ScreenVertex { x: 50.0, y: 10.0, z: 0.5, shade: 1.0 };
        let v1 = ScreenVertex { x: 90.0, y: 90.0, z: 0.5, shade: 1.0 };
        let v2 = ScreenVertex { x: 10.0, y: 90.0, z: 0.5, shade: 1.0 };
        rasterize_triangle(&mut fb, v0, v1, v2, 255, 0, 0, 255);

        // Check a pixel that would be inside -- should still be black
        let check_idx = (60 * 100 + 50) * 4;
        assert_eq!(fb.pixels()[check_idx], 0, "backface should be culled");
    }

    #[test]
    fn render_front_face() {
        let mut fb = Framebuffer::new(100, 100);
        fb.clear(0, 0, 0, 255);

        // CCW triangle (front face) - large, centered
        let v0 = ScreenVertex { x: 50.0, y: 10.0, z: 0.5, shade: 1.0 };
        let v1 = ScreenVertex { x: 10.0, y: 90.0, z: 0.5, shade: 1.0 };
        let v2 = ScreenVertex { x: 90.0, y: 90.0, z: 0.5, shade: 1.0 };
        rasterize_triangle(&mut fb, v0, v1, v2, 255, 0, 0, 255);

        // Check a pixel well inside the triangle (y=60 is safely between top and bottom)
        let check_idx = (60 * 100 + 50) * 4;
        assert_eq!(fb.pixels()[check_idx], 255, "interior pixel should be red");
    }

    #[test]
    fn depth_ordering() {
        let mut fb = Framebuffer::new(100, 100);
        fb.clear(0, 0, 0, 255);

        // Red triangle at z=0.8 (far)
        let v0 = ScreenVertex { x: 50.0, y: 10.0, z: 0.8, shade: 1.0 };
        let v1 = ScreenVertex { x: 10.0, y: 90.0, z: 0.8, shade: 1.0 };
        let v2 = ScreenVertex { x: 90.0, y: 90.0, z: 0.8, shade: 1.0 };
        rasterize_triangle(&mut fb, v0, v1, v2, 255, 0, 0, 255);

        // Green triangle at z=0.3 (near) -- should overwrite red
        let v3 = ScreenVertex { x: 50.0, y: 10.0, z: 0.3, shade: 1.0 };
        let v4 = ScreenVertex { x: 10.0, y: 90.0, z: 0.3, shade: 1.0 };
        let v5 = ScreenVertex { x: 90.0, y: 90.0, z: 0.3, shade: 1.0 };
        rasterize_triangle(&mut fb, v3, v4, v5, 0, 255, 0, 255);

        // Check at y=60 (well inside both triangles)
        let check_idx = (60 * 100 + 50) * 4;
        assert_eq!(fb.pixels()[check_idx], 0, "red should be overwritten");
        assert_eq!(fb.pixels()[check_idx + 1], 255, "green should win");
    }

    #[test]
    fn line_rasterization() {
        let mut fb = Framebuffer::new(100, 100);
        fb.clear(0, 0, 0, 255);

        rasterize_line(&mut fb, 10.0, 50.0, 0.5, 90.0, 50.0, 0.5, 255, 255, 255, 255);

        // Check midpoint
        let mid_idx = (50 * 100 + 50) * 4;
        assert_eq!(fb.pixels()[mid_idx], 255, "midpoint should be white");
    }

    #[test]
    fn line_rasterization_outside() {
        let mut fb = Framebuffer::new(100, 100);
        fb.clear(0, 0, 0, 255);

        // Entirely off-screen line: from (-10, -10) to (-50, -50)
        rasterize_line(&mut fb, -10.0, -10.0, 0.5, -50.0, -50.0, 0.5, 255, 255, 255, 255);

        // Framebuffer should remain completely black
        for pixel in fb.pixels().chunks_exact(4) {
            assert_eq!(pixel, &[0, 0, 0, 255]);
        }
    }

    #[test]
    fn line_rasterization_clipped() {
        let mut fb = Framebuffer::new(100, 100);
        fb.clear(0, 0, 0, 255);

        // A line from (-50, 50) to (150, 50).
        // It crosses the screen horizontally at y = 50.
        // It should be clipped to x in [0, 99].
        rasterize_line(&mut fb, -50.0, 50.0, 0.5, 150.0, 50.0, 0.5, 255, 255, 255, 255);

        // Leftmost pixel (x=0) and rightmost pixel (x=99) should be white.
        let left_idx = (50 * 100 + 0) * 4;
        let right_idx = (50 * 100 + 99) * 4;
        assert_eq!(fb.pixels()[left_idx], 255, "left clipped endpoint should be white");
        assert_eq!(fb.pixels()[right_idx], 255, "right clipped endpoint should be white");
    }

    #[test]
    fn line_no_depth_test() {
        let mut fb = Framebuffer::new(100, 100);
        fb.clear(0, 0, 0, 255);

        // 1. Draw a pixel at (50, 50) with depth 0.1 and red color
        fb.write_pixel(50, 50, 0.1, 255, 0, 0, 255);

        // 2. Draw a line with rasterize_line at depth 0.9 (further away) crossing (50, 50) with green color
        rasterize_line(&mut fb, 10.0, 50.0, 0.9, 90.0, 50.0, 0.9, 0, 255, 0, 255);

        // The pixel at (50, 50) should still be red (since 0.9 >= 0.1, it fails the depth test)
        let idx = (50 * 100 + 50) * 4;
        assert_eq!(fb.pixels()[idx], 255, "should remain red");
        assert_eq!(fb.pixels()[idx + 1], 0, "should not have green");

        // 3. Draw a line with rasterize_line_no_depth crossing (50, 50) with blue color
        rasterize_line_no_depth(&mut fb, 10.0, 50.0, 90.0, 50.0, 0, 0, 255, 255);

        // The pixel at (50, 50) should now be blue (since depth test is bypassed)
        assert_eq!(fb.pixels()[idx], 0, "should not be red anymore");
        assert_eq!(fb.pixels()[idx + 2], 255, "should be blue");

        // 4. Verify that the depth buffer was NOT overwritten by the no_depth_test line, i.e. it remains 0.1
        // We can check this by drawing a new pixel with depth 0.2 (which is further than 0.1 but closer than any default or 0.9).
        // If the depth buffer is still 0.1, drawing with depth 0.2 should FAIL.
        // If the depth buffer was overwritten (or set to 0.0), drawing with depth 0.2 would fail too, but drawing with 0.05 would pass.
        // Let's draw with depth 0.05 (closer than 0.1) and check if it passes:
        let pass = fb.write_pixel(50, 50, 0.05, 0, 255, 0, 255);
        assert!(pass, "writing closer than 0.1 should pass");
    }
}
