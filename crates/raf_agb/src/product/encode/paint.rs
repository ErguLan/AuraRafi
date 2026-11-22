use glam::Vec2;

use crate::{Error, Result};

use super::{unsupported, validate_color, vertex, Encoder};
use crate::product::{Paint, PaintQuad, PaintStroke};

impl Encoder<'_> {
    pub(super) fn paint(&mut self, paint: Paint) -> Result<()> {
        match paint {
            Paint::Solid(quad) => self.quad(quad, self.white),
            Paint::Stroke(stroke) => self.stroke(stroke),
            Paint::Text { quad, atlas } => self.quad(quad, self.texture(atlas)?),
            Paint::Image { quad, texture } => self.quad(quad, self.texture(texture)?),
            Paint::Effect(feature) => unsupported(feature),
        }
    }

    fn quad(&mut self, quad: PaintQuad, texture: u32) -> Result<()> {
        let rect = Bounds::from_rect(quad.rect)?;
        let clip = Bounds::from_rect(quad.clip)?;
        let uv = Bounds::from_rect(quad.uv_rect)?;
        validate_color(quad.color)?;
        let low = rect.low.max(clip.low);
        let high = rect.high.min(clip.high);
        if low.x >= high.x || low.y >= high.y {
            return Ok(());
        }
        let mapped = |xy: Vec2| {
            let relative = (xy - rect.low) / (rect.high - rect.low);
            let uv = uv.low + relative * (uv.high - uv.low);
            vertex(xy.to_array(), uv.to_array(), quad.color)
        };
        let [a, b, c, d] = [
            low,
            Vec2::new(high.x, low.y),
            high,
            Vec2::new(low.x, high.y),
        ]
        .map(mapped);
        self.overlay_triangles(&[a, b, c, a, c, d], texture)
    }

    fn stroke(&mut self, stroke: PaintStroke) -> Result<()> {
        let clip = Bounds::from_rect(stroke.clip)?;
        let start = Vec2::from(stroke.start);
        let end = Vec2::from(stroke.end);
        let width = Vec2::from(stroke.width);
        validate_color(stroke.color)?;
        if !start.is_finite() || !end.is_finite() || !width.is_finite() || width.min_element() <= 0.
        {
            return Err(Error::Invalid("invalid stroke geometry"));
        }
        let Some(direction) = ((end - start) / width).try_normalize() else {
            return Err(Error::Invalid("degenerate stroke"));
        };
        let side = Vec2::new(-direction.y, direction.x) * width * 0.5;
        let mut a = [Vec2::ZERO; 8];
        let mut b = [Vec2::ZERO; 8];
        a[..4].copy_from_slice(&[start - side, end - side, end + side, start + side]);
        let mut count = 4;
        for (axis, edge, lower) in [
            (0, clip.low.x, true),
            (0, clip.high.x, false),
            (1, clip.low.y, true),
            (1, clip.high.y, false),
        ] {
            count = clip_polygon(&a[..count], &mut b, axis, edge, lower);
            if count < 3 {
                return Ok(());
            }
            std::mem::swap(&mut a, &mut b);
        }
        let make_vertex = |point: Vec2| vertex(point.to_array(), [0.; 2], stroke.color);
        let anchor = make_vertex(a[0]);
        // At most six triangles for a clipped quad.
        let mut triangles = [anchor; 18];
        for (index, edge) in a[1..count].windows(2).enumerate() {
            triangles[index * 3 + 1] = make_vertex(edge[0]);
            triangles[index * 3 + 2] = make_vertex(edge[1]);
        }
        self.overlay_triangles(&triangles[..(count - 2) * 3], self.white)
    }
}

struct Bounds {
    low: Vec2,
    high: Vec2,
}

impl Bounds {
    fn from_rect([x, y, w, h]: [f32; 4]) -> Result<Self> {
        let low = Vec2::new(x, y);
        let high = low + Vec2::new(w, h);
        if !low.is_finite() || !high.is_finite() || w < 0. || h < 0. {
            return Err(Error::Invalid("invalid paint rectangle"));
        }
        Ok(Self { low, high })
    }
}

fn clip_polygon(
    input: &[Vec2],
    output: &mut [Vec2; 8],
    axis: usize,
    edge: f32,
    lower: bool,
) -> usize {
    let inside = |point: Vec2| {
        if lower {
            point[axis] >= edge
        } else {
            point[axis] <= edge
        }
    };
    let mut count = 0;
    let mut previous = input[input.len() - 1];
    for &current in input {
        let was_inside = inside(previous);
        let is_inside = inside(current);
        if was_inside != is_inside && previous[axis] != edge && current[axis] != edge {
            let t = (edge - previous[axis]) / (current[axis] - previous[axis]);
            let mut crossing = previous.lerp(current, t);
            crossing[axis] = edge;
            output[count] = crossing;
            count += 1;
        }
        if is_inside {
            output[count] = current;
            count += 1;
        }
        previous = current;
    }
    count
}
