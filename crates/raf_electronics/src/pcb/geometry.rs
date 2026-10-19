//! Copper geometry shared by connectivity, routing checks and PCB DRC.

use super::{footprint_definition, PcbLayer, PcbLayout};
use glam::Vec2;
use uuid::Uuid;

pub(crate) const CONTACT_EPSILON: f32 = 0.0001;

#[derive(Clone)]
pub(crate) enum Shape {
    Segment(Vec2, Vec2, f32),
    Pad([Vec2; 4]),
}

pub(crate) struct Copper {
    pub source: Uuid,
    pub net: String,
    pub layer: PcbLayer,
    pub shape: Shape,
    pub pad: Option<(usize, usize)>,
}

impl Shape {
    pub fn bounds(&self) -> (Vec2, Vec2) {
        match self {
            Self::Segment(a, b, radius) => (
                a.min(*b) - Vec2::splat(*radius),
                a.max(*b) + Vec2::splat(*radius),
            ),
            Self::Pad(points) => points
                .iter()
                .copied()
                .fold((points[0], points[0]), |(lo, hi), p| (lo.min(p), hi.max(p))),
        }
    }

    pub fn distance(&self, other: &Self) -> f32 {
        match (self, other) {
            (Self::Segment(a, b, r), Self::Segment(c, d, s)) => {
                (segment_distance(*a, *b, *c, *d) - r - s).max(0.0)
            }
            (Self::Segment(a, b, r), Self::Pad(p)) | (Self::Pad(p), Self::Segment(a, b, r)) => {
                (segment_polygon_distance(*a, *b, p) - r).max(0.0)
            }
            (Self::Pad(a), Self::Pad(b)) => {
                if point_in_polygon(a[0], b) || point_in_polygon(b[0], a) {
                    0.0
                } else {
                    (0..4)
                        .map(|i| segment_polygon_distance(a[i], a[(i + 1) % 4], b))
                        .fold(f32::INFINITY, f32::min)
                }
            }
        }
    }

    pub fn inside_outline(&self, outline: &[Vec2], clearance: f32) -> bool {
        let inside_segment = |a: Vec2, b: Vec2, radius: f32| {
            point_in_polygon(a, outline)
                && point_in_polygon(b, outline)
                && outline.windows(2).all(|edge| {
                    segment_distance(a, b, edge[0], edge[1]) + CONTACT_EPSILON >= radius + clearance
                })
        };
        match self {
            Self::Segment(a, b, radius) => inside_segment(*a, *b, *radius),
            Self::Pad(p) => (0..4).all(|i| inside_segment(p[i], p[(i + 1) % 4], 0.0)),
        }
    }
}

pub(crate) fn copper_items(layout: &PcbLayout) -> Vec<Copper> {
    let mut items = Vec::new();
    for (ci, component) in layout.components.iter().enumerate() {
        if !component.position.is_finite() || !component.rotation.is_finite() {
            continue;
        }
        let footprint = footprint_definition(&component.footprint, component.pad_nets.len().max(1));
        let (sin, cos) = component.rotation.to_radians().sin_cos();
        let rotate =
            |p: Vec2| component.position + Vec2::new(p.x * cos - p.y * sin, p.x * sin + p.y * cos);
        for (pi, pad) in footprint.pads.iter().enumerate() {
            let half = pad.size * 0.5;
            let corners = [
                Vec2::new(-half.x, -half.y),
                Vec2::new(half.x, -half.y),
                half,
                Vec2::new(-half.x, half.y),
            ]
            .map(|p| rotate(pad.offset + p));
            items.push(Copper {
                source: component.component_id,
                net: component.pad_nets.get(pi).cloned().unwrap_or_default(),
                layer: component.layer,
                shape: Shape::Pad(corners),
                pad: Some((ci, pi)),
            });
        }
    }
    for trace in &layout.traces {
        if !trace.width.is_finite()
            || trace.width <= 0.0
            || trace.points.iter().any(|p| !p.is_finite())
        {
            continue;
        }
        for pair in trace.points.windows(2) {
            items.push(Copper {
                source: trace.id,
                net: trace.net.clone(),
                layer: trace.layer,
                shape: Shape::Segment(pair[0], pair[1], trace.width * 0.5),
                pad: None,
            });
        }
    }
    items
}

/// Sweep bounding boxes before exact copper comparisons. A long track does not
/// require populating thousands of spatial cells, nor scanning every pair.
pub(crate) fn nearby_pairs(items: &[Copper], gap: f32, mut visit: impl FnMut(usize, usize)) {
    nearby_pairs_cancellable(
        items,
        gap,
        &std::sync::atomic::AtomicBool::new(false),
        &mut visit,
    );
}

pub(crate) fn nearby_pairs_cancellable(
    items: &[Copper],
    gap: f32,
    cancel: &std::sync::atomic::AtomicBool,
    mut visit: impl FnMut(usize, usize),
) {
    let bounds: Vec<_> = items.iter().map(|item| item.shape.bounds()).collect();
    let mut order: Vec<_> = (0..items.len()).collect();
    order.sort_by(|&a, &b| bounds[a].0.x.total_cmp(&bounds[b].0.x));
    for (offset, &a) in order.iter().enumerate() {
        for &b in &order[offset + 1..] {
            if cancel.load(std::sync::atomic::Ordering::Acquire) {
                return;
            }
            if bounds[b].0.x > bounds[a].1.x + gap {
                break;
            }
            if items[a].layer == items[b].layer
                && bounds[a].0.y <= bounds[b].1.y + gap
                && bounds[b].0.y <= bounds[a].1.y + gap
            {
                visit(a, b);
            }
        }
    }
}

pub(crate) fn point_in_polygon(point: Vec2, polygon: &[Vec2]) -> bool {
    if !point.is_finite() || polygon.len() < 3 {
        return false;
    }
    let mut inside = false;
    for (a, b) in polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .take(polygon.len())
    {
        if point_segment_distance(point, *a, *b) <= CONTACT_EPSILON {
            return true;
        }
        if (a.y > point.y) != (b.y > point.y)
            && point.x < (b.x - a.x) * (point.y - a.y) / (b.y - a.y) + a.x
        {
            inside = !inside;
        }
    }
    inside
}

fn point_segment_distance(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let delta = b - a;
    let length = delta.length_squared();
    if length <= f32::EPSILON {
        return p.distance(a);
    }
    p.distance(a + delta * ((p - a).dot(delta) / length).clamp(0.0, 1.0))
}

pub(crate) fn segment_distance(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> f32 {
    let ab = b - a;
    let cd = d - c;
    let cross = ab.perp_dot(cd);
    if cross.abs() > f32::EPSILON {
        let t = (c - a).perp_dot(cd) / cross;
        let u = (c - a).perp_dot(ab) / cross;
        if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
            return 0.0;
        }
    }
    [
        point_segment_distance(a, c, d),
        point_segment_distance(b, c, d),
        point_segment_distance(c, a, b),
        point_segment_distance(d, a, b),
    ]
    .into_iter()
    .fold(f32::INFINITY, f32::min)
}

fn segment_polygon_distance(a: Vec2, b: Vec2, polygon: &[Vec2; 4]) -> f32 {
    if point_in_polygon(a, polygon) || point_in_polygon(b, polygon) {
        return 0.0;
    }
    (0..4)
        .map(|i| segment_distance(a, b, polygon[i], polygon[(i + 1) % 4]))
        .fold(f32::INFINITY, f32::min)
}
