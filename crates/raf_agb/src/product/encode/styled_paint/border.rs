//! Per-side border rings.
//!
//! A border is the area between the outer shape and the same shape inset by one
//! width per side. The ring is emitted segment by segment; each outline segment
//! belongs to the side it touches, and corner arcs split at their midpoint so a
//! per-side color never bleeds across a whole corner.

use crate::product::{PaintBorder, Side};
use crate::Result;

use super::path::{PolygonPainter, Sample, Shape, ARC_POINTS, OUTLINE_LEN, SEGMENTS};

/// Side that owns one segment of a shape outline.
pub(super) fn segment_side(index: usize) -> Side {
    let corner = index / ARC_POINTS;
    let step = index % ARC_POINTS;
    // Corner arcs run from the previous side to the current side in
    // `top, right, bottom, left` order.
    let side = if step * 2 < SEGMENTS {
        (corner + 3) % 4
    } else {
        corner % 4
    };
    Side::ALL[side]
}

/// Emits the ring between `outer` and `inner`. Both shapes must share the same
/// bounds ordering, which every shape produced by `Shape` does.
pub(super) fn ring(
    painter: &mut PolygonPainter<'_, '_>,
    outer: Shape,
    inner: Shape,
    border: PaintBorder,
) -> Result<()> {
    let white = painter.white();
    let outer = outer.outline();
    let inner = inner.outline();
    for index in 0..OUTLINE_LEN {
        let next = (index + 1) % OUTLINE_LEN;
        if outer[index] == outer[next] && inner[index] == inner[next] {
            continue;
        }
        let color = border.side_color(segment_side(index));
        painter.polygon(
            &[
                Sample::solid(outer[index], color),
                Sample::solid(outer[next], color),
                Sample::solid(inner[next], color),
                Sample::solid(inner[index], color),
            ],
            white,
        )?;
    }
    Ok(())
}
