//! Bounded, spatial-only antialiasing for scene color buffers.
//!
//! This is a small edge-sensitive five-tap filter. It is not an FXAA
//! implementation: only corners with meaningful contrast in both image axes
//! are softened, while uniform fields and axis-aligned edges remain unchanged.

use crate::texture::{linear_to_srgb, srgb_to_linear};

const MAX_BLEND_STRENGTH: f32 = 0.45;
const CONTRAST_FLOOR: f32 = 0.035;
const CONTRAST_FULL_STRENGTH: f32 = 0.25;

/// Filters one RGBA8 image into another without allocating temporary storage.
///
/// The byte slices must each contain exactly `width * height * 4` bytes. Zero
/// dimensions, arithmetic overflow, or mismatched lengths leave `target`
/// untouched. RGB is interpreted as sRGB and filtered in linear light; each
/// output alpha byte is copied exactly from the center pixel.
pub(crate) fn filter_rgba(source: &[u8], target: &mut [u8], width: u32, height: u32) {
    if width == 0 || height == 0 {
        return;
    }

    let Some(pixel_count) = (width as usize).checked_mul(height as usize) else {
        return;
    };
    let Some(byte_count) = pixel_count.checked_mul(4) else {
        return;
    };
    if source.len() != byte_count || target.len() != byte_count {
        return;
    }

    let width_usize = width as usize;
    let height_usize = height as usize;

    for y in 0..height_usize {
        let north_y = y.saturating_sub(1);
        let south_y = (y + 1).min(height_usize - 1);

        for x in 0..width_usize {
            let west_x = x.saturating_sub(1);
            let east_x = (x + 1).min(width_usize - 1);

            let center_index = (y * width_usize + x) * 4;
            let north_index = (north_y * width_usize + x) * 4;
            let south_index = (south_y * width_usize + x) * 4;
            let west_index = (y * width_usize + west_x) * 4;
            let east_index = (y * width_usize + east_x) * 4;

            // Equal opposing RGB values guarantee one zero luma gradient and
            // therefore exactly zero filter strength. Skip all linear-light
            // decoding on flat areas, straight edges and vertical sky gradients.
            // Alpha is intentionally ignored: taps never filter center alpha.
            if source[north_index..north_index + 3] == source[south_index..south_index + 3]
                || source[west_index..west_index + 3] == source[east_index..east_index + 3]
            {
                target[center_index..center_index + 4]
                    .copy_from_slice(&source[center_index..center_index + 4]);
                continue;
            }

            let center = linear_rgb(source, center_index);
            let north = linear_rgb(source, north_index);
            let south = linear_rgb(source, south_index);
            let west = linear_rgb(source, west_index);
            let east = linear_rgb(source, east_index);

            let center_luma = luma(center);
            let north_luma = luma(north);
            let south_luma = luma(south);
            let west_luma = luma(west);
            let east_luma = luma(east);

            let local_min = center_luma
                .min(north_luma)
                .min(south_luma)
                .min(west_luma)
                .min(east_luma);
            let local_max = center_luma
                .max(north_luma)
                .max(south_luma)
                .max(west_luma)
                .max(east_luma);
            let contrast = local_max - local_min;

            let gradient_x = (east_luma - west_luma).abs();
            let gradient_y = (south_luma - north_luma).abs();
            let gradient_sum = gradient_x + gradient_y;
            let corner_measure = if gradient_sum > 0.0 {
                (2.0 * gradient_x.min(gradient_y) / gradient_sum).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let contrast_weight = ((contrast - CONTRAST_FLOOR)
                / (CONTRAST_FULL_STRENGTH - CONTRAST_FLOOR))
                .clamp(0.0, 1.0);
            let strength = (MAX_BLEND_STRENGTH * contrast_weight * corner_measure)
                .clamp(0.0, MAX_BLEND_STRENGTH);

            if strength == 0.0 {
                target[center_index..center_index + 4]
                    .copy_from_slice(&source[center_index..center_index + 4]);
                continue;
            }

            let neighbor_average = [
                (north[0] + south[0] + west[0] + east[0]) * 0.25,
                (north[1] + south[1] + west[1] + east[1]) * 0.25,
                (north[2] + south[2] + west[2] + east[2]) * 0.25,
            ];
            for channel in 0..3 {
                let filtered =
                    center[channel] + (neighbor_average[channel] - center[channel]) * strength;
                target[center_index + channel] = linear_to_srgb(filtered);
            }
            target[center_index + 3] = source[center_index + 3];
        }
    }
}

#[inline]
fn linear_rgb(bytes: &[u8], index: usize) -> [f32; 3] {
    [
        srgb_to_linear(bytes[index]),
        srgb_to_linear(bytes[index + 1]),
        srgb_to_linear(bytes[index + 2]),
    ]
}

#[inline]
fn luma(color: [f32; 3]) -> f32 {
    color[0] * 0.2126 + color[1] * 0.7152 + color[2] * 0.0722
}

#[cfg(test)]
mod tests {
    use super::{filter_rgba, srgb_to_linear};

    fn run_filter(source: &[u8], width: u32, height: u32) -> Vec<u8> {
        let mut target = vec![0; source.len()];
        filter_rgba(source, &mut target, width, height);
        target
    }

    fn grayscale_image(width: usize, height: usize, pixel: impl Fn(usize, usize) -> u8) -> Vec<u8> {
        let mut image = Vec::with_capacity(width * height * 4);
        for y in 0..height {
            for x in 0..width {
                let gray = pixel(x, y);
                image.extend_from_slice(&[gray, gray, gray, 255]);
            }
        }
        image
    }

    #[test]
    fn constant_fields_are_bit_exact_and_keep_alpha() {
        let mut source = Vec::new();
        for alpha in 0..16u8 {
            source.extend_from_slice(&[38, 129, 217, alpha.wrapping_mul(17)]);
        }

        assert_eq!(run_filter(&source, 4, 4), source);
    }

    #[test]
    fn axis_aligned_boundaries_remain_bit_exact() {
        let vertical = grayscale_image(7, 5, |x, _| if x < 3 { 0 } else { 255 });
        let horizontal = grayscale_image(5, 7, |_, y| if y < 3 { 0 } else { 255 });

        assert_eq!(run_filter(&vertical, 7, 5), vertical);
        assert_eq!(run_filter(&horizontal, 5, 7), horizontal);
    }

    #[test]
    fn staircase_corners_move_toward_local_coverage() {
        let thresholds = [3, 3, 2, 1, 1, 0];
        let source = grayscale_image(6, 6, |x, y| if x < thresholds[y] { 0 } else { 255 });
        let target = run_filter(&source, 6, 6);

        let black_corner_index = (2 * 6 + 1) * 4;
        let white_corner_index = (2 * 6 + 2) * 4;
        let black_before = srgb_to_linear(source[black_corner_index]);
        let black_after = srgb_to_linear(target[black_corner_index]);
        let white_before = srgb_to_linear(source[white_corner_index]);
        let white_after = srgb_to_linear(target[white_corner_index]);

        assert!(black_before < black_after && black_after < 0.5);
        assert!(white_before > white_after && white_after > 0.5);
        assert!((0.5 - black_after) < (0.5 - black_before));
        assert!((white_after - 0.5) < (white_before - 0.5));
    }

    #[test]
    fn clamped_border_taps_do_not_wrap_and_alpha_is_copied_exactly() {
        let source = [
            255, 255, 255, 7, 0, 0, 0, 61, // top row
            0, 0, 0, 129, 255, 255, 255, 243, // bottom row
        ];
        let target = run_filter(&source, 2, 2);

        for pixel in 0..4 {
            assert_eq!(target[pixel * 4 + 3], source[pixel * 4 + 3]);
        }
        let expected_white_corner = super::super::texture::linear_to_srgb(0.775);
        assert_eq!(target[0], expected_white_corner);
    }

    #[test]
    fn malformed_dimensions_or_lengths_leave_target_untouched() {
        let source = [10, 20, 30, 40];
        let mut target = [91, 92, 93, 94];

        filter_rgba(&source, &mut target, 0, 1);
        filter_rgba(&source, &mut target, 2, 1);
        filter_rgba(&source, &mut target, u32::MAX, u32::MAX);

        assert_eq!(target, [91, 92, 93, 94]);
    }
}
