//! Styled RafUI paint: one command in, shared overlay geometry out.
//!
//! The producer owns paint order and alpha. This encoder resolves corners,
//! per-side borders, feather, gradients, clip nesting and image patches, then
//! submits one overlay stream that every native executor rasterizes identically.

mod border;
mod feather;
mod image;
mod path;

use glam::Vec2;

use crate::product::paint_style::MAX_DECLARED_CLIPS;
use crate::product::{ImageFit, Paint, PaintStyle, RoundedClip};
use crate::{Error, Result};

use super::{unsupported, validate_color, Encoder};
use path::{Layers, PolygonPainter, Shape, ShapeStack};

/// Upper bound for a feather in normalized units. Keeps the bounded kernel and
/// the generated geometry inside a predictable budget.
const MAX_SOFTNESS: f32 = 64.;

/// Reusable paint buffers. One instance lives on the encoder and is borrowed by
/// every styled command, so a retained repaint does not grow the heap per quad.
#[derive(Default)]
pub(super) struct PaintScratch {
    current: Vec<path::Sample>,
    scratch: Vec<path::Sample>,
    triangles: Vec<crate::Vertex>,
}

impl PaintScratch {
    pub(super) fn new() -> Self {
        Self {
            current: Vec::with_capacity(path::OUTLINE_LEN + 1),
            scratch: Vec::with_capacity(path::OUTLINE_LEN + 1),
            triangles: Vec::with_capacity(path::OUTLINE_LEN * 3),
        }
    }
}

impl Encoder<'_> {
    pub(super) fn styled_paint(&mut self, paint: Paint, style: &PaintStyle) -> Result<()> {
        validate_style(style)?;
        let mut scratch = std::mem::take(&mut self.paint_scratch);
        let result = self.draw_styled(paint, style, &mut scratch);
        self.paint_scratch = scratch;
        result
    }

    fn draw_styled(
        &mut self,
        paint: Paint,
        style: &PaintStyle,
        scratch: &mut PaintScratch,
    ) -> Result<()> {
        let white = self.white;
        match paint {
            Paint::Solid(quad) => {
                reject_image_layout(style)?;
                let shapes = declared_shapes(style)?;
                PolygonPainter::new(self, scratch, quad.clip, shapes.as_slice())?
                    .panel(quad, style, white)
            }
            Paint::Text { quad, atlas } => {
                reject_image_layout(style)?;
                let atlas = self.texture(atlas)?;
                let shapes = declared_shapes(style)?;
                PolygonPainter::new(self, scratch, quad.clip, shapes.as_slice())?
                    .panel(quad, style, atlas)
            }
            Paint::Stroke(stroke) => {
                reject_image_layout(style)?;
                if style.radii != [[0.; 2]; 4] || style.border.is_some() {
                    return Err(Error::Unsupported(
                        "stroke corner radii or per-side borders",
                    ));
                }
                validate_color(stroke.color)?;
                let start = Vec2::from_array(stroke.start);
                let end = Vec2::from_array(stroke.end);
                let width = Vec2::from_array(stroke.width);
                if !start.is_finite()
                    || !end.is_finite()
                    || !width.is_finite()
                    || width.min_element() <= 0.
                {
                    return Err(Error::Invalid("invalid styled stroke"));
                }
                let Some(axis) = ((end - start) / width).try_normalize() else {
                    return Ok(());
                };
                let normal = Vec2::new(-axis.y, axis.x);
                let side = normal * width * 0.5;
                let inner = [start - side, end - side, end + side, start + side];
                let outer = (style.softness != [0.; 2]).then(|| {
                    let reach = Vec2::from_array(style.softness) * 0.5;
                    let direction = side.normalize_or_zero();
                    let distance = direction.x.abs() * reach.x + direction.y.abs() * reach.y;
                    let cap = axis.normalize_or_zero() * reach.x.max(reach.y);
                    let (start, end) = (start - cap, end + cap);
                    let widened = side + direction * distance;
                    [
                        start - widened,
                        end - widened,
                        end + widened,
                        start + widened,
                    ]
                });
                let shapes = declared_shapes(style)?;
                let mut painter =
                    PolygonPainter::new(self, scratch, stroke.clip, shapes.as_slice())?;
                painter.stroke(inner, outer, stroke.color, style)
            }
            Paint::Image { quad, texture } => {
                validate_color(quad.color)?;
                let texture = self.texture(texture)?;
                let Some(layout) = style.image else {
                    let shapes = declared_shapes(style)?;
                    return PolygonPainter::new(self, scratch, quad.clip, shapes.as_slice())?
                        .panel(quad, style, texture);
                };
                if style.softness != [0.; 2]
                    && (layout.nine_slice.is_some() || layout.fit != ImageFit::Stretch)
                {
                    return Err(Error::Unsupported(
                        "feather on a fitted or sliced image",
                    ));
                }
                let size = self.workload.textures[texture as usize].size;
                let Some(layers) = Layers::new(quad.rect, style)? else {
                    return Ok(());
                };
                let mut shapes = declared_shapes(style)?;
                // The feather halo grows outside the image rectangle, so it is
                // painted before the image's own rounded edge joins the clip
                // stack.
                PolygonPainter::new(self, scratch, quad.clip, shapes.as_slice())?
                    .feather(quad, style, &layers, texture)?;
                if style.radii.iter().flatten().any(|radius| *radius > 0.) {
                    shapes.push(Shape::try_from(RoundedClip {
                        rect: quad.rect,
                        radii: style.radii,
                    })?)?;
                }
                let mut painter = PolygonPainter::new(self, scratch, quad.clip, shapes.as_slice())?;
                if style.gradient.is_some() {
                    painter.fill(quad, style, &layers, layers.core, white)?;
                }
                image::emit(quad, size, layout, |patch| {
                    painter.polygon(&patch.corners(quad.color), texture)
                })?;
                if let Some(border) = layers.border {
                    painter.border(layers.core, border)?;
                }
                Ok(())
            }
            Paint::Effect(feature) => unsupported(feature),
        }
    }
}

fn declared_shapes(style: &PaintStyle) -> Result<ShapeStack> {
    let mut shapes = ShapeStack::new();
    for clip in &style.clips {
        shapes.push(Shape::try_from(*clip)?)?;
    }
    Ok(shapes)
}

fn validate_style(style: &PaintStyle) -> Result<()> {
    if style.clips.len() > MAX_DECLARED_CLIPS {
        return Err(Error::Invalid("too many nested rounded clips"));
    }
    if !style
        .radii
        .iter()
        .flatten()
        .all(|radius| radius.is_finite() && *radius >= 0.)
    {
        return Err(Error::Invalid("invalid corner radii"));
    }
    let softness = Vec2::from_array(style.softness);
    if !softness.is_finite() || softness.min_element() < 0. || softness.max_element() > MAX_SOFTNESS
    {
        return Err(Error::Invalid("invalid paint feather"));
    }
    for clip in &style.clips {
        Shape::try_from(*clip)?;
    }
    if let Some(gradient) = style.gradient {
        validate_color(gradient.start)?;
        validate_color(gradient.end)?;
        if !Vec2::from_array(gradient.direction).is_finite() {
            return Err(Error::Invalid("invalid gradient direction"));
        }
    }
    if let Some(border) = style.border {
        validate_color(border.color)?;
        for color in border.sides.iter().flatten() {
            validate_color(*color)?;
        }
        if !border
            .widths
            .iter()
            .all(|width| width.is_finite() && *width >= 0.)
        {
            return Err(Error::Invalid("invalid border widths"));
        }
    }
    if let Some(layout) = style.image {
        if layout.target_extent.contains(&0) && layout.fit != ImageFit::Stretch {
            return Err(Error::Invalid(
                "image fit requires the physical target extent",
            ));
        }
        if !layout
            .focal_point
            .into_iter()
            .all(|value| value.is_finite() && (0. ..=1.).contains(&value))
        {
            return Err(Error::Invalid("invalid image focal point"));
        }
        if let Some(slices) = layout.nine_slice {
            if !slices
                .source
                .into_iter()
                .chain(slices.target)
                .all(|value| value.is_finite() && value >= 0.)
            {
                return Err(Error::Invalid("invalid nine-slice insets"));
            }
        }
    }
    Ok(())
}

fn reject_image_layout(style: &PaintStyle) -> Result<()> {
    if style.image.is_some() {
        return Err(Error::Invalid("image layout requires image paint"));
    }
    Ok(())
}
