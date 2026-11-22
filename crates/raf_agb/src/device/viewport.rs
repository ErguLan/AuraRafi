//! Normalized scene viewport validation and pixel conversion shared by every
//! executor. Rectangles use a top-left origin inside the full target.

use crate::{DrawMode, Error, Result};

/// Float error tolerated on a viewport edge produced by layout, DPI or camera
/// math. Anything beyond it is rejected instead of silently rescaled.
const EDGE_TOLERANCE: f32 = 1.0e-5;

/// Smallest fractional extent kept when a hand-built frame carries a degenerate
/// rectangle. The product contract rejects those before submission.
const MIN_SPAN: f32 = 1.0e-6;

/// The scene viewport resolved against the full target in physical pixels.
///
/// The fractional fields keep the exact pixel edges for viewport transforms;
/// `pixels` is the integer rectangle used by scissor and clipping tests.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ViewportRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// `[x, y, width, height]` in whole target pixels.
    pub pixels: [u32; 4],
}

impl ViewportRect {
    pub(crate) fn full([width, height]: [u32; 2]) -> Self {
        Self::from_pixels([0, 0, width.max(1), height.max(1)])
    }

    pub(crate) fn from_normalized(viewport: [f32; 4], extent: [u32; 2]) -> Self {
        let [x, y, width, height] = clamp_unit(viewport);
        let [left, across] = span(x, x + width, extent[0]);
        let [top, down] = span(y, y + height, extent[1]);
        Self::from_pixels([left, top, across, down])
    }

    /// Scene groups keep the canvas rectangle. Overlay groups always cover the
    /// target so RafUI paint and CAD are never clipped by the canvas.
    pub(crate) fn for_mode(scene: Self, mode: DrawMode, extent: [u32; 2]) -> Self {
        if mode.is_scene() {
            scene
        } else {
            Self::full(extent)
        }
    }

    fn from_pixels([x, y, width, height]: [u32; 4]) -> Self {
        Self {
            x: x as f32,
            y: y as f32,
            width: width as f32,
            height: height as f32,
            pixels: [x, y, width, height],
        }
    }
}

/// Validates the product-facing normalized rectangle and returns it clamped to
/// the unit square. A few ULPs outside the boundary are folded back; a
/// rectangle that is meaningfully outside is an explicit error.
pub(crate) fn validate_viewport([x, y, width, height]: [f32; 4]) -> Result<[f32; 4]> {
    if ![x, y, width, height].into_iter().all(f32::is_finite) {
        return Err(Error::Invalid("invalid scene viewport rectangle"));
    }
    if x < -EDGE_TOLERANCE || y < -EDGE_TOLERANCE {
        return Err(Error::Invalid("scene viewport starts outside the target"));
    }
    if width <= 0. || height <= 0. {
        return Err(Error::Invalid("scene viewport has no area"));
    }
    if x + width > 1. + EDGE_TOLERANCE || y + height > 1. + EDGE_TOLERANCE {
        return Err(Error::Invalid("scene viewport exceeds the target"));
    }
    Ok(clamp_unit([x, y, width, height]))
}

fn clamp_unit([x, y, width, height]: [f32; 4]) -> [f32; 4] {
    // Non-finite values can only arrive through a frame built by hand; the
    // product contract rejects them before submission.
    let x = finite(x).clamp(0., 1.);
    let y = finite(y).clamp(0., 1.);
    let width = finite(width).min(1. - x).max(MIN_SPAN);
    let height = finite(height).min(1. - y).max(MIN_SPAN);
    [x, y, width, height]
}

fn finite(value: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        0.
    }
}

/// Maps a normalized span to whole target pixels with at least one pixel of
/// coverage, clamped to the target edge.
fn span(start: f32, end: f32, limit: u32) -> [u32; 2] {
    let extent = limit.max(1) as f32;
    let first = (start * extent).round().clamp(0., extent);
    let last = (end * extent).round().clamp(0., extent);
    let first = (first as u32).min(limit.max(1) - 1);
    let last = (last as u32).max(first + 1).min(limit.max(1));
    [first, last - first]
}

#[cfg(test)]
mod tests {
    use super::*;

    const TARGET: [u32; 2] = [1280, 720];

    #[test]
    fn normalized_rect_maps_to_target_pixels() {
        let rect = ViewportRect::from_normalized([0.25, 0.1, 0.5, 0.6], TARGET);
        assert_eq!(rect.pixels, [320, 72, 640, 432]);
        assert_eq!([rect.x, rect.y, rect.width, rect.height], [320., 72., 640., 432.]);
    }

    #[test]
    fn default_viewport_covers_the_full_target() {
        assert_eq!(validate_viewport([0., 0., 1., 1.]).unwrap(), [0., 0., 1., 1.]);
        let rect = ViewportRect::from_normalized([0., 0., 1., 1.], [640, 360]);
        assert_eq!(rect.pixels, [0, 0, 640, 360]);
        assert_eq!(ViewportRect::for_mode(rect, DrawMode::Overlay, [640, 360]), rect);
    }

    #[test]
    fn overlay_groups_keep_the_full_target() {
        let scene = ViewportRect::from_normalized([0.25, 0.1, 0.5, 0.6], TARGET);
        let full = ViewportRect::full(TARGET);
        for mode in [
            DrawMode::Scene,
            DrawMode::DepthReadOnly,
            DrawMode::SceneNoDepth,
        ] {
            assert_eq!(ViewportRect::for_mode(scene, mode, TARGET), scene);
        }
        assert_eq!(ViewportRect::for_mode(scene, DrawMode::Overlay, TARGET), full);
    }

    #[test]
    fn tiny_float_error_is_clamped_and_invalid_rects_are_rejected() {
        assert_eq!(
            validate_viewport([-0.000_000_5, 0., 1.000_000_5, 1.]).unwrap(),
            [0., 0., 1., 1.]
        );
        assert_eq!(
            validate_viewport([0.5, 0., 0.500_000_1, 1.]).unwrap(),
            [0.5, 0., 0.5, 1.]
        );
        for rect in [
            [f32::NAN, 0., 0.5, 1.],
            [0., f32::INFINITY, 0.5, 1.],
            [0., 0., 0., 1.],
            [0., 0., 0.5, -1.],
            [-0.25, 0., 0.5, 1.],
            [0., 0., 1.5, 1.],
            [0.5, 0., 0.6, 1.],
            [0., 0.5, 1., 0.6],
        ] {
            assert!(
                matches!(validate_viewport(rect), Err(Error::Invalid(_))),
                "{rect:?} was admitted"
            );
        }
    }

    #[test]
    fn pixel_rects_stay_inside_the_target() {
        let rect = ViewportRect::from_normalized([0.9, 0.9, 0.2, 0.2], [100, 100]);
        assert_eq!(rect.pixels, [90, 90, 10, 10]);
        let sliver = ViewportRect::from_normalized([0.999_9, 0., 0.000_1, 1.], [100, 50]);
        assert_eq!(sliver.pixels, [99, 0, 1, 50]);
    }
}
