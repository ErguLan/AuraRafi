//! Rounded shape geometry, polygon clipping and the reusable paint scratch.
//!
//! Every surface is reduced to one polygon and clipped against the planar clip
//! rectangle plus the ancestor rounded clips. The result is triangulated as a
//! fan and submitted through the shared overlay stream, so every native
//! executor rasterizes the same CPU-produced geometry.

use glam::Vec2;

use crate::product::paint_style::MAX_ROUNDED_CLIPS;
use crate::product::{LinearGradient, PaintBorder, PaintQuad, PaintStyle, RoundedClip};
use crate::{Error, Result};

use super::super::{validate_color, vertex, Encoder};
use super::PaintScratch;

pub(super) const SEGMENTS: usize = 8;
pub(super) const ARC_POINTS: usize = SEGMENTS + 1;
pub(super) const OUTLINE_LEN: usize = ARC_POINTS * 4;

/// One shape for the paint quad's clip rectangle and one per declared clip or
/// surface edge.
pub(super) const MAX_SHAPES: usize = MAX_ROUNDED_CLIPS + 1;

#[derive(Clone, Copy, Debug)]
pub(super) struct Shape {
    pub low: Vec2,
    pub high: Vec2,
    radii: [Vec2; 4],
}

impl Shape {
    const ZERO: Self = Self {
        low: Vec2::ZERO,
        high: Vec2::ZERO,
        radii: [Vec2::ZERO; 4],
    };

    pub fn empty(self) -> bool {
        (self.high - self.low).min_element() <= 0.
    }

    pub fn size(self) -> Vec2 {
        self.high - self.low
    }

    /// Climbs radii until opposite corners can no longer overlap. This is the
    /// same normalization the retained shader applies.
    fn normalized(mut self) -> Self {
        let [a, b, c, d] = self.radii;
        let size = self.high - self.low;
        let mut scale = 1.0_f32;
        for (length, sum) in [
            (size.x, a.x + b.x),
            (size.x, c.x + d.x),
            (size.y, a.y + d.y),
            (size.y, b.y + c.y),
        ] {
            if sum > 0. {
                scale = scale.min(length.max(0.) / sum);
            }
        }
        self.radii = self.radii.map(|radius| (radius * scale).max(Vec2::ZERO));
        self
    }

    /// Pointwise aligned outline in top-left, top-right, bottom-right,
    /// bottom-left order. Two shapes with the same bounds produce outlines that
    /// can be interpolated corner by corner.
    pub fn outline(self) -> [Vec2; OUTLINE_LEN] {
        let corners = [
            self.low,
            Vec2::new(self.high.x, self.low.y),
            self.high,
            Vec2::new(self.low.x, self.high.y),
        ];
        let signs = [
            Vec2::ONE,
            Vec2::new(-1., 1.),
            -Vec2::ONE,
            Vec2::new(1., -1.),
        ];
        std::array::from_fn(|index| {
            let corner = index / ARC_POINTS;
            let radius = self.radii[corner];
            let step = index % ARC_POINTS;
            let angle =
                std::f32::consts::FRAC_PI_2 * (2. + corner as f32 + step as f32 / SEGMENTS as f32);
            corners[corner] + signs[corner] * radius + Vec2::from_angle(angle) * radius
        })
    }

    /// Grows by `by` on every side. A negative amount shrinks the shape and
    /// clamps corner radii at zero.
    pub fn expanded(self, by: Vec2) -> Self {
        Self {
            low: self.low - by,
            high: self.high + by,
            radii: self.radii.map(|radius| (radius + by).max(Vec2::ZERO)),
        }
        .normalized()
    }

    /// Inner border shape. Matches the retained shader: the inner corner radius
    /// loses the largest of its two adjacent border widths.
    pub fn inner(self, widths: [f32; 4]) -> Result<Option<Self>> {
        if !widths.iter().all(|width| width.is_finite() && *width >= 0.) {
            return Err(Error::Invalid("invalid border widths"));
        }
        let [top, right, bottom, left] = widths;
        let low = self.low + Vec2::new(left, top);
        let high = self.high - Vec2::new(right, bottom);
        if high.x <= low.x || high.y <= low.y {
            return Ok(None);
        }
        let inset = [
            top.max(left),
            top.max(right),
            bottom.max(right),
            bottom.max(left),
        ];
        Ok(Some(
            Self {
                low,
                high,
                radii: std::array::from_fn(|index| {
                    (self.radii[index] - Vec2::splat(inset[index])).max(Vec2::ZERO)
                }),
            }
            .normalized(),
        ))
    }

    /// Intersection with an axis-aligned screen rectangle. The result keeps the
    /// source radii, so callers only use it for rectangular clip bounds.
    pub fn intersect(self, screen: Self) -> Self {
        Self {
            low: self.low.max(screen.low),
            high: self.high.min(screen.high),
            radii: self.radii,
        }
    }
}

impl TryFrom<RoundedClip> for Shape {
    type Error = Error;

    fn try_from(value: RoundedClip) -> Result<Self> {
        let [x, y, w, h] = value.rect;
        let low = Vec2::new(x, y);
        let high = low + Vec2::new(w, h);
        let radii = value.radii.map(Vec2::from_array);
        if !low.is_finite()
            || !high.is_finite()
            || w < 0.
            || h < 0.
            || radii.iter().any(|r| !r.is_finite() || r.min_element() < 0.)
        {
            return Err(Error::Invalid("invalid rounded paint shape"));
        }
        Ok(Self { low, high, radii }.normalized())
    }
}

#[derive(Clone, Copy)]
pub(super) struct Sample {
    pub point: Vec2,
    pub uv: Vec2,
    pub color: [f32; 4],
}

impl Sample {
    pub fn solid(point: Vec2, color: [f32; 4]) -> Self {
        Self {
            point,
            uv: Vec2::new(0.5, 0.5),
            color,
        }
    }

    /// True when two samples would produce the same vertex. Consecutive
    /// identical points are collapsed before clipping so square shapes keep
    /// their four-corner triangle count.
    fn same_vertex(self, other: Self) -> bool {
        self.point == other.point && self.uv == other.uv && self.color == other.color
    }

    fn mix(self, other: Self, t: f32) -> Self {
        Self {
            point: self.point.lerp(other.point, t),
            uv: self.uv.lerp(other.uv, t),
            color: std::array::from_fn(|i| self.color[i] + (other.color[i] - self.color[i]) * t),
        }
    }
}

pub(super) fn sample_quad(
    point: Vec2,
    area: Shape,
    quad: PaintQuad,
    gradient: Option<LinearGradient>,
) -> Sample {
    let local = ((point - area.low) / area.size()).clamp(Vec2::ZERO, Vec2::ONE);
    let uv = Vec2::new(quad.uv_rect[0], quad.uv_rect[1])
        + local * Vec2::new(quad.uv_rect[2], quad.uv_rect[3]);
    let color = gradient.map_or(quad.color, |gradient| {
        let direction = Vec2::from_array(gradient.direction);
        let span = direction.abs().element_sum();
        let t = if span > 0. {
            ((local - Vec2::splat(0.5)).dot(direction) / span + 0.5).clamp(0., 1.)
        } else {
            0.
        };
        std::array::from_fn(|i| gradient.start[i] + (gradient.end[i] - gradient.start[i]) * t)
    });
    Sample { point, uv, color }
}

/// Fixed-capacity stack of declared clips plus one surface edge.
pub(super) struct ShapeStack {
    shapes: [Shape; MAX_ROUNDED_CLIPS + 1],
    len: usize,
}

impl ShapeStack {
    pub fn new() -> Self {
        Self {
            shapes: [Shape::ZERO; MAX_ROUNDED_CLIPS + 1],
            len: 0,
        }
    }

    pub fn push(&mut self, shape: Shape) -> Result<()> {
        let slot = self
            .shapes
            .get_mut(self.len)
            .ok_or(Error::Invalid("too many nested rounded clips"))?;
        *slot = shape;
        self.len += 1;
        Ok(())
    }

    pub fn as_slice(&self) -> &[Shape] {
        &self.shapes[..self.len]
    }

    pub fn len(&self) -> usize {
        self.len
    }
}

#[derive(Clone, Copy)]
pub(super) struct Layers {
    /// Authored quad shape. Local UV and gradient mapping stay relative to it.
    pub area: Shape,
    /// Core shape after the feather half width is removed.
    pub core: Shape,
    /// Outer feather shape when softness is present.
    pub halo: Option<Shape>,
    pub border: Option<PaintBorder>,
}

impl Layers {
    pub fn new(rect: [f32; 4], style: &PaintStyle) -> Result<Option<Self>> {
        let area = Shape::try_from(RoundedClip {
            rect,
            radii: style.radii,
        })?;
        if area.empty() {
            return Ok(None);
        }
        let softness = Vec2::from_array(style.softness);
        let half = (softness * 0.5).min(area.size() * 0.49);
        let (core, halo) = if half == Vec2::ZERO {
            (area, None)
        } else {
            (area.expanded(-half), Some(area.expanded(half)))
        };
        let border = style
            .border
            .filter(PaintBorder::is_visible)
            .map(|border| border.clamped([0., 0., core.size().x, core.size().y]));
        Ok(Some(Self {
            area,
            core,
            halo,
            border,
        }))
    }
}

pub(super) struct PolygonPainter<'a, 'b> {
    encoder: &'a mut Encoder<'b>,
    geo: &'a mut PaintScratch,
    shapes: [Shape; MAX_SHAPES],
    outlines: [[Vec2; OUTLINE_LEN]; MAX_SHAPES],
    count: usize,
    visible: bool,
    white: u32,
}

impl<'a, 'b> PolygonPainter<'a, 'b> {
    pub fn new(
        encoder: &'a mut Encoder<'b>,
        geo: &'a mut PaintScratch,
        clip: [f32; 4],
        clips: &[Shape],
    ) -> Result<Self> {
        let outer = Shape::try_from(RoundedClip::square(clip))?;
        let screen = Shape::try_from(RoundedClip::square([0., 0., 1., 1.]))?;
        let combined = outer.intersect(screen);
        let visible = !combined.empty() && clips.iter().all(|shape| !shape.empty());
        let mut painter = Self {
            white: encoder.white,
            encoder,
            geo,
            shapes: [Shape::ZERO; MAX_SHAPES],
            outlines: [[Vec2::ZERO; OUTLINE_LEN]; MAX_SHAPES],
            count: 0,
            visible,
        };
        painter.push(combined)?;
        for shape in clips {
            painter.push(*shape)?;
        }
        for index in 0..painter.count {
            painter.outlines[index] = painter.shapes[index].outline();
        }
        Ok(painter)
    }

    fn push(&mut self, shape: Shape) -> Result<()> {
        if self.count >= MAX_SHAPES {
            return Err(Error::Invalid("too many nested rounded clips"));
        }
        self.shapes[self.count] = shape;
        self.count += 1;
        Ok(())
    }

    pub fn white(&self) -> u32 {
        self.white
    }

    /// Fades from the outer outline (transparent) to the inner outline (opaque).
    /// A border supplies its fallback color and the shared white texture; a
    /// plain surface samples its own texture at the clamped edge.
    pub fn feather(
        &mut self,
        quad: PaintQuad,
        style: &PaintStyle,
        layers: &Layers,
        texture: u32,
    ) -> Result<()> {
        let Some(halo) = layers.halo else {
            return Ok(());
        };
        let texture = if layers.border.is_some() {
            self.white
        } else {
            texture
        };
        let outer = halo.outline();
        let inner = layers.core.outline();
        let border = layers.border;
        super::feather::emit(self, &outer, &inner, texture, |point, alpha| {
            let mut sample = border.map_or_else(
                || sample_quad(point, layers.area, quad, style.gradient),
                |border| Sample::solid(point, border.color),
            );
            sample.color[3] *= alpha;
            sample
        })
    }

    /// Paints the border ring and returns the inner shape to fill with the
    /// surface color. `None` means the border covered the whole shape, in which
    /// case the fallback color fills it.
    pub fn border(&mut self, outer: Shape, border: PaintBorder) -> Result<Option<Shape>> {
        let white = self.white;
        let Some(inner) = outer.inner(border.widths)? else {
            let samples = outer
                .outline()
                .map(|point| Sample::solid(point, border.color));
            self.polygon(&samples, white)?;
            return Ok(None);
        };
        super::border::ring(self, outer, inner, border)?;
        Ok(Some(inner))
    }

    /// Fills one shape with the quad color, gradient or sampled texture.
    pub fn fill(
        &mut self,
        quad: PaintQuad,
        style: &PaintStyle,
        layers: &Layers,
        shape: Shape,
        texture: u32,
    ) -> Result<()> {
        let samples = shape
            .outline()
            .map(|point| sample_quad(point, layers.area, quad, style.gradient));
        self.polygon(&samples, texture)
    }

    pub fn panel(&mut self, quad: PaintQuad, style: &PaintStyle, texture: u32) -> Result<()> {
        validate_color(quad.color)?;
        validate_uv(quad)?;
        let Some(layers) = Layers::new(quad.rect, style)? else {
            return Ok(());
        };
        if !self.visible {
            return Ok(());
        }
        let halo_texture = if layers.border.is_some() {
            self.white
        } else {
            texture
        };
        self.feather(quad, style, &layers, halo_texture)?;
        let fill = match layers.border {
            Some(border) => match self.border(layers.core, border)? {
                Some(inner) => inner,
                None => return Ok(()),
            },
            None => layers.core,
        };
        self.fill(quad, style, &layers, fill, texture)
    }

    /// Straight stroke between two centers. The nominal width is preserved; a
    /// feather expands only the outer soft edge.
    pub fn stroke(
        &mut self,
        inner: [Vec2; 4],
        outer: Option<[Vec2; 4]>,
        color: [f32; 4],
        style: &PaintStyle,
    ) -> Result<()> {
        validate_color(color)?;
        let quad = PaintQuad {
            rect: bbox(inner),
            uv_rect: [0., 0., 1., 1.],
            clip: [0.; 4],
            color,
        };
        let area = Shape {
            low: Vec2::new(quad.rect[0], quad.rect[1]),
            high: Vec2::new(quad.rect[0] + quad.rect[2], quad.rect[1] + quad.rect[3]),
            radii: [Vec2::ZERO; 4],
        };
        if area.empty() {
            return Ok(());
        }
        if let Some(outer) = outer {
            let white = self.white;
            super::feather::emit(self, &outer, &inner, white, |point, alpha| {
                let mut sample = sample_quad(point, area, quad, style.gradient);
                sample.color[3] *= alpha;
                sample
            })?;
        }
        let samples = inner.map(|point| sample_quad(point, area, quad, style.gradient));
        let white = self.white;
        self.polygon(&samples, white)
    }

    pub fn polygon(&mut self, vertices: &[Sample], texture: u32) -> Result<()> {
        if !self.visible || vertices.is_empty() {
            return Ok(());
        }
        if vertices
            .iter()
            .any(|sample| !sample.point.is_finite() || !sample.uv.is_finite())
        {
            return Err(Error::Invalid("non-finite paint geometry"));
        }
        self.geo.current.clear();
        for sample in vertices {
            if self
                .geo
                .current
                .last()
                .is_some_and(|last| last.same_vertex(*sample))
            {
                continue;
            }
            self.geo.current.push(*sample);
        }
        if self.geo.current.len() > 1 {
            let first = self.geo.current[0];
            let last = self.geo.current[self.geo.current.len() - 1];
            if first.same_vertex(last) {
                self.geo.current.pop();
            }
        }
        for index in 0..self.count {
            for (&a, &b) in self.outlines[index]
                .iter()
                .zip(self.outlines[index].iter().cycle().skip(1))
            {
                if a == b {
                    continue;
                }
                clip_edge(&self.geo.current, &mut self.geo.scratch, a, b);
                std::mem::swap(&mut self.geo.current, &mut self.geo.scratch);
                if self.geo.current.len() < 3 {
                    return Ok(());
                }
            }
        }
        self.geo.triangles.clear();
        let to_vertex =
            |sample: Sample| vertex(sample.point.to_array(), sample.uv.to_array(), sample.color);
        let first = to_vertex(self.geo.current[0]);
        for pair in self.geo.current[1..].windows(2) {
            self.geo
                .triangles
                .extend([first, to_vertex(pair[0]), to_vertex(pair[1])]);
        }
        self.encoder.overlay_triangles(&self.geo.triangles, texture)
    }
}

fn bbox(points: [Vec2; 4]) -> [f32; 4] {
    let (low, high) = points.into_iter().fold(
        (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)),
        |(low, high), point| (low.min(point), high.max(point)),
    );
    [low.x, low.y, high.x - low.x, high.y - low.y]
}

fn validate_uv(quad: PaintQuad) -> Result<()> {
    if !quad.uv_rect.into_iter().all(f32::is_finite) || quad.uv_rect[2] < 0. || quad.uv_rect[3] < 0.
    {
        return Err(Error::Invalid("invalid sampled paint rectangle"));
    }
    Ok(())
}

fn clip_edge(input: &[Sample], output: &mut Vec<Sample>, a: Vec2, b: Vec2) {
    output.clear();
    let distance = |point: Vec2| (b - a).perp_dot(point - a);
    let mut previous = input[input.len() - 1];
    let mut old = distance(previous.point);
    for &current in input {
        let new = distance(current.point);
        if (old < 0.) != (new < 0.) && old != 0. && new != 0. {
            output.push(previous.mix(current, (old / (old - new)).clamp(0., 1.)));
        }
        if new >= 0. {
            output.push(current);
        }
        previous = current;
        old = new;
    }
}
