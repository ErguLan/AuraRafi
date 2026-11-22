//! Image placement: source crop, focal point, contain/cover/stretch and
//! nine-slice. Patches are clipped against the destination quad so a `Cover`
//! image never paints outside its authored rectangle.

use glam::Vec2;

use crate::product::{ImageFit, ImageLayout, PaintQuad};
use crate::{Error, Result};

#[derive(Clone, Copy, Debug)]
pub(super) struct Patch {
    pub rect: [f32; 4],
    pub uv: [f32; 4],
}

impl Patch {
    /// Four corners in top-left, top-right, bottom-right, bottom-left order and
    /// their sampled coordinates. The UV map is linear inside one patch.
    pub fn corners(self, tint: [f32; 4]) -> [super::path::Sample; 4] {
        let [x, y, w, h] = self.rect;
        let [u, v, uw, vh] = self.uv;
        [
            ([x, y], [u, v]),
            ([x + w, y], [u + uw, v]),
            ([x + w, y + h], [u + uw, v + vh]),
            ([x, y + h], [u, v + vh]),
        ]
        .map(|(point, uv)| super::path::Sample {
            point: Vec2::from_array(point),
            uv: Vec2::from_array(uv),
            color: tint,
        })
    }
}

pub(super) fn emit(
    quad: PaintQuad,
    texture: [u32; 2],
    layout: ImageLayout,
    mut emit: impl FnMut(Patch) -> Result<()>,
) -> Result<()> {
    let [x, y, w, h] = quad.rect;
    let [u, v, uw, vh] = quad.uv_rect;
    let size = texture.map(|dimension| dimension as f32);
    if size.contains(&0.)
        || !quad
            .rect
            .into_iter()
            .chain(quad.uv_rect)
            .all(f32::is_finite)
        || !layout
            .focal_point
            .into_iter()
            .all(|value| (0. ..=1.).contains(&value))
        || w < 0.
        || h < 0.
        || uw < 0.
        || vh < 0.
    {
        return Err(Error::Invalid("invalid image layout"));
    }
    if w == 0. || h == 0. || uw == 0. || vh == 0. {
        return Ok(());
    }
    let bounds = [x, y, x + w, y + h];
    if let Some(slices) = layout.nine_slice {
        if !slices
            .source
            .into_iter()
            .chain(slices.target)
            .all(|value| value.is_finite() && value >= 0.)
        {
            return Err(Error::Invalid("invalid nine-slice insets"));
        }
        let [top, right, bottom, left] = slices.source;
        let [to_top, to_right, to_bottom, to_left] = slices.target;
        let xs = axis(x, w, to_left, to_right);
        let ys = axis(y, h, to_top, to_bottom);
        let us = axis(u, uw, left / size[0], right / size[0]);
        let vs = axis(v, vh, top / size[1], bottom / size[1]);
        for row in 0..3 {
            for column in 0..3 {
                let patch = Patch {
                    rect: [
                        xs[column],
                        ys[row],
                        xs[column + 1] - xs[column],
                        ys[row + 1] - ys[row],
                    ],
                    uv: [
                        us[column],
                        vs[row],
                        us[column + 1] - us[column],
                        vs[row + 1] - vs[row],
                    ],
                };
                if patch.rect[2] <= 0. || patch.rect[3] <= 0. || patch.uv[2] <= 0. || patch.uv[3] <= 0.
                {
                    continue;
                }
                if let Some(patch) = clipped(patch, bounds) {
                    emit(patch)?;
                }
            }
        }
        return Ok(());
    }
    if layout.fit == ImageFit::Stretch {
        if let Some(patch) = clipped(
            Patch {
                rect: quad.rect,
                uv: quad.uv_rect,
            },
            bounds,
        ) {
            emit(patch)?;
        }
        return Ok(());
    }
    let [target_width, target_height] = layout.target_extent.map(|dimension| dimension as f32);
    if target_width <= 0. || target_height <= 0. {
        return Err(Error::Invalid(
            "image fit requires the physical target extent",
        ));
    }
    let source = [size[0] * uw, size[1] * vh];
    let scale_x = w * target_width / source[0];
    let scale_y = h * target_height / source[1];
    let scale = if layout.fit == ImageFit::Cover {
        scale_x.max(scale_y)
    } else {
        scale_x.min(scale_y)
    };
    let extent = [
        source[0] * scale / target_width,
        source[1] * scale / target_height,
    ];
    if let Some(patch) = clipped(
        Patch {
            rect: [
                x + (w - extent[0]) * layout.focal_point[0],
                y + (h - extent[1]) * layout.focal_point[1],
                extent[0],
                extent[1],
            ],
            uv: quad.uv_rect,
        },
        bounds,
    ) {
        emit(patch)?;
    }
    Ok(())
}

fn clipped(patch: Patch, bounds: [f32; 4]) -> Option<Patch> {
    let [x, y, w, h] = patch.rect;
    let [u, v, uw, vh] = patch.uv;
    if w <= 0. || h <= 0. {
        return None;
    }
    let low = [x.max(bounds[0]), y.max(bounds[1])];
    let high = [(x + w).min(bounds[2]), (y + h).min(bounds[3])];
    if high[0] <= low[0] || high[1] <= low[1] {
        return None;
    }
    let scale = [uw / w, vh / h];
    let size = [high[0] - low[0], high[1] - low[1]];
    Some(Patch {
        rect: [low[0], low[1], size[0], size[1]],
        uv: [
            u + (low[0] - x) * scale[0],
            v + (low[1] - y) * scale[1],
            size[0] * scale[0],
            size[1] * scale[1],
        ],
    })
}

/// Places two insets inside one axis. Insets larger than the axis are scaled
/// down together so opposite edges never cross.
fn axis(start: f32, length: f32, low: f32, high: f32) -> [f32; 4] {
    let scale = length / length.max(low + high);
    [
        start,
        start + low * scale,
        (start + length - high * scale).max(start + low * scale),
        start + length,
    ]
}
