//! Bounded edge falloff shared by solid panels, borders and strokes.
//!
//! A feather is a short stack of concentric rings between an outer outline and
//! an inner outline. Both outlines use the same point ordering, so every band
//! interpolates corner by corner without gaps or overlaps. The band count is
//! fixed: the kernel never grows with the feather width or the frame size.

use glam::Vec2;

use crate::Result;

use super::path::{PolygonPainter, Sample};

/// Four bands keep a linear ramp smooth while bounding the triangle count.
pub(super) const BANDS: usize = 4;

/// Emits `BANDS` rings from `outer` (transparent) to `inner` (opaque).
/// `sample` receives an outline point and the coverage of that point.
pub(super) fn emit(
    painter: &mut PolygonPainter<'_, '_>,
    outer: &[Vec2],
    inner: &[Vec2],
    texture: u32,
    mut sample: impl FnMut(Vec2, f32) -> Sample,
) -> Result<()> {
    let len = outer.len().min(inner.len());
    if len < 3 {
        return Ok(());
    }
    for band in 0..BANDS {
        let low = band as f32 / BANDS as f32;
        let high = (band + 1) as f32 / BANDS as f32;
        for index in 0..len {
            let next = (index + 1) % len;
            let outer_a = outer[index].lerp(inner[index], low);
            let outer_b = outer[next].lerp(inner[next], low);
            let inner_a = outer[index].lerp(inner[index], high);
            let inner_b = outer[next].lerp(inner[next], high);
            if outer_a == outer_b && inner_a == inner_b {
                continue;
            }
            painter.polygon(
                &[
                    sample(outer_a, low),
                    sample(outer_b, low),
                    sample(inner_b, high),
                    sample(inner_a, high),
                ],
                texture,
            )?;
        }
    }
    Ok(())
}
