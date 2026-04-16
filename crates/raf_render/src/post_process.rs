//! Post-processing effects for the CPU painter.
//!
//! These effects operate on already-painted pixels or brightness values.
//! All are opt-in via RenderConfig toggles. When disabled, these functions
//! are never called (zero cost).
//!
//! Effects:
//! - FXAA (simplified edge detection for CPU painter)
//! - Bloom glow (additive brightness on faces > threshold)
//! - Vignette (darken edges of viewport)
//! - Color grading (tone mapping, saturation)

/// Simple FXAA-like edge smoothing for the CPU painter.
///
/// Given a grid of face colors, detect edges and blend.
/// This is a VERY simplified version - true FXAA operates on pixel buffers.
/// For the CPU painter, we apply edge softening to face edges.
///
/// Returns a blended color for drawing edges between adjacent faces.
pub fn fxaa_edge_blend(
    color_a: [u8; 3],
    color_b: [u8; 3],
    edge_factor: f32,
) -> [u8; 3] {
    let t = edge_factor.clamp(0.0, 1.0);
    [
        ((color_a[0] as f32 * (1.0 - t) + color_b[0] as f32 * t)) as u8,
        ((color_a[1] as f32 * (1.0 - t) + color_b[1] as f32 * t)) as u8,
        ((color_a[2] as f32 * (1.0 - t) + color_b[2] as f32 * t)) as u8,
    ]
}

/// Apply bloom to a face color.
///
/// If the face brightness exceeds the threshold, add a glow overlay.
/// `brightness`: computed lighting brightness of the face (0.0 - 2.0).
/// `base_color`: original face color [R, G, B] as u8.
/// `bloom_intensity`: from RenderConfig (0.0 - 1.0).
///
/// Returns the bloomed color.
pub fn apply_bloom(
    base_color: [u8; 3],
    brightness: f32,
    bloom_intensity: f32,
) -> [u8; 3] {
    let threshold = 0.85;
    if brightness <= threshold || bloom_intensity <= 0.0 {
        return base_color;
    }
    let glow = (brightness - threshold) * bloom_intensity * 2.0;
    [
        (base_color[0] as f32 + glow * 60.0).min(255.0) as u8,
        (base_color[1] as f32 + glow * 60.0).min(255.0) as u8,
        (base_color[2] as f32 + glow * 60.0).min(255.0) as u8,
    ]
}

/// Apply vignette darkening based on screen position.
///
/// `screen_uv`: normalized screen position (0.0 - 1.0 for both x and y).
/// `color`: input pixel color.
/// `strength`: vignette intensity (0.0 = none, 1.0 = strong).
pub fn apply_vignette(
    color: [u8; 3],
    screen_uv: [f32; 2],
    strength: f32,
) -> [u8; 3] {
    if strength <= 0.0 {
        return color;
    }
    let cx = screen_uv[0] - 0.5;
    let cy = screen_uv[1] - 0.5;
    let dist = (cx * cx + cy * cy).sqrt() * 2.0; // 0 at center, ~1.4 at corners
    let factor = 1.0 - (dist * strength).min(0.7);
    [
        (color[0] as f32 * factor) as u8,
        (color[1] as f32 * factor) as u8,
        (color[2] as f32 * factor) as u8,
    ]
}

/// Simple tone mapping (Reinhard operator).
/// Maps HDR brightness value to LDR range.
pub fn tonemap_reinhard(hdr: f32) -> f32 {
    hdr / (1.0 + hdr)
}

/// Adjust color saturation.
///
/// `saturation`: 0.0 = grayscale, 1.0 = original, 2.0 = hyper-saturated.
pub fn adjust_saturation(color: [u8; 3], saturation: f32) -> [u8; 3] {
    let r = color[0] as f32 / 255.0;
    let g = color[1] as f32 / 255.0;
    let b = color[2] as f32 / 255.0;
    let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let r_out = (luma + (r - luma) * saturation).clamp(0.0, 1.0);
    let g_out = (luma + (g - luma) * saturation).clamp(0.0, 1.0);
    let b_out = (luma + (b - luma) * saturation).clamp(0.0, 1.0);
    [
        (r_out * 255.0) as u8,
        (g_out * 255.0) as u8,
        (b_out * 255.0) as u8,
    ]
}

/// Convert linear color to sRGB gamma space.
pub fn linear_to_srgb(linear: f32) -> f32 {
    if linear <= 0.0031308 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

/// Convert sRGB gamma color to linear space.
pub fn srgb_to_linear(srgb: f32) -> f32 {
    if srgb <= 0.04045 {
        srgb / 12.92
    } else {
        ((srgb + 0.055) / 1.055).powf(2.4)
    }
}
