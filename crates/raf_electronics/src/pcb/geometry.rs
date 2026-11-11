//! Copper geometry shared by connectivity, routing checks and PCB DRC.

use super::footprint_asset::{FootprintLayer, FootprintPad, FootprintPadTechnology, PointMm};
use super::{PcbComponentPlacement, PcbLayer, PcbLayout};
use glam::Vec2;
use uuid::Uuid;

pub(crate) const CONTACT_EPSILON: f32 = 0.0001;
const PHYSICAL_CURVE_SEGMENTS: usize = 48;

/// Applies the physical footprint placement transform in one place: when
/// mirrored, reflect local X first, then rotate and translate to board space.
pub(crate) fn transform_footprint_point(
    component: &PcbComponentPlacement,
    point: PointMm,
    mirror_bottom: bool,
) -> Vec2 {
    let radians = component.rotation.to_radians();
    let (sin, cos) = radians.sin_cos();
    let local_x = if mirror_bottom && component.layer == PcbLayer::BottomCopper {
        -point.x
    } else {
        point.x
    };
    let local = Vec2::new(local_x as f32, point.y as f32);
    component.position + Vec2::new(local.x * cos - local.y * sin, local.x * sin + local.y * cos)
}

/// Transforms a local orientation through a bottom-side X reflection and the
/// component rotation. For symmetric pad shapes, the 180-degree component of
/// the reflected angle is visually equivalent modulo 180 degrees.
pub(crate) fn transform_footprint_rotation(
    component: &PcbComponentPlacement,
    local_degrees: f64,
    mirror_bottom: bool,
) -> f32 {
    let local_degrees = if mirror_bottom && component.layer == PcbLayer::BottomCopper {
        180.0 - local_degrees
    } else {
        local_degrees
    };
    component.rotation + local_degrees as f32
}

#[derive(Clone)]
pub(crate) enum Shape {
    Segment(Vec2, Vec2, f32),
    Pad {
        outline: Vec<Vec2>,
        holes: Vec<Vec<Vec2>>,
    },
}

pub(crate) struct Copper {
    pub source: Uuid,
    pub net: String,
    pub layer: PcbLayer,
    pub shape: Shape,
    pub contact_shape: Option<Shape>,
    pub pad: Option<(usize, usize)>,
    pub plated_through_hole: bool,
}

impl Copper {
    pub fn contact_shape(&self) -> &Shape {
        self.contact_shape.as_ref().unwrap_or(&self.shape)
    }
}

impl Shape {
    pub fn bounds(&self) -> (Vec2, Vec2) {
        match self {
            Self::Segment(a, b, radius) => (
                a.min(*b) - Vec2::splat(*radius),
                a.max(*b) + Vec2::splat(*radius),
            ),
            Self::Pad { outline, .. } => outline
                .first()
                .copied()
                .map(|first| {
                    outline
                        .iter()
                        .copied()
                        .skip(1)
                        .fold((first, first), |(lo, hi), point| {
                            (lo.min(point), hi.max(point))
                        })
                })
                .unwrap_or((Vec2::ZERO, Vec2::ZERO)),
        }
    }

    pub fn distance(&self, other: &Self) -> f32 {
        match (self, other) {
            (Self::Segment(a, b, r), Self::Segment(c, d, s)) => {
                (segment_distance(*a, *b, *c, *d) - r - s).max(0.0)
            }
            (Self::Segment(a, b, r), Self::Pad { outline, holes })
            | (Self::Pad { outline, holes }, Self::Segment(a, b, r)) => {
                (segment_pad_distance(*a, *b, outline, holes) - r).max(0.0)
            }
            (
                Self::Pad {
                    outline: a,
                    holes: holes_a,
                },
                Self::Pad {
                    outline: b,
                    holes: holes_b,
                },
            ) => pad_pad_distance(a, holes_a, b, holes_b),
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
            Self::Pad { outline, .. } => {
                outline.len() >= 3
                    && (0..outline.len()).all(|index| {
                        inside_segment(outline[index], outline[(index + 1) % outline.len()], 0.0)
                    })
            }
        }
    }
}

pub(crate) fn circle_polygon(center: Vec2, radius: f32) -> Vec<Vec2> {
    (0..64)
        .map(|i| {
            let angle = i as f32 * std::f32::consts::TAU / 64.0;
            center + Vec2::new(angle.cos(), angle.sin()) * radius
        })
        .collect()
}

pub(crate) fn copper_items(layout: &PcbLayout) -> Vec<Copper> {
    let mut items = Vec::new();
    for via in &layout.vias {
        if !via.position.is_finite()
            || !via.diameter.is_finite()
            || !via.drill.is_finite()
            || via.drill <= 0.0
            || via.diameter <= via.drill
        {
            continue;
        }
        let cosine = (std::f32::consts::PI / 64.0).cos();
        for layer in [PcbLayer::TopCopper, PcbLayer::BottomCopper] {
            items.push(Copper {
                source: via.id,
                net: if via.orphaned {
                    String::new()
                } else {
                    via.net.clone()
                },
                layer,
                pad: None,
                plated_through_hole: true,
                shape: Shape::Pad {
                    outline: circle_polygon(via.position, via.diameter * 0.5 / cosine),
                    holes: vec![circle_polygon(via.position, via.drill * 0.5)],
                },
                contact_shape: Some(Shape::Pad {
                    outline: circle_polygon(via.position, via.diameter * 0.5),
                    holes: vec![circle_polygon(via.position, via.drill * 0.5 / cosine)],
                }),
            });
        }
    }
    for (ci, component) in layout.components.iter().enumerate() {
        if !component.position.is_finite() || !component.rotation.is_finite() {
            continue;
        }
        if let Some(asset) = layout.footprint_assets.get(&component.footprint) {
            if !asset.is_fabrication_ready() {
                continue;
            }
            for (pi, pad) in asset.pads.iter().enumerate() {
                if matches!(
                    &pad.technology,
                    FootprintPadTechnology::NonPlatedThroughHole
                ) {
                    continue;
                }
                let Some(local_outline) = pad.outline_polygon_mm(PHYSICAL_CURVE_SEGMENTS) else {
                    continue;
                };
                if local_outline.len() < 3 || local_outline.iter().any(|point| !point.is_finite()) {
                    continue;
                }
                let outline = local_outline
                    .into_iter()
                    .map(|point| transform_footprint_point(component, point, true))
                    .collect::<Vec<_>>();
                let holes = pad
                    .drill_outline_polygon_mm(PHYSICAL_CURVE_SEGMENTS)
                    .map(|hole| {
                        hole.into_iter()
                            .map(|point| transform_footprint_point(component, point, true))
                            .collect()
                    })
                    .into_iter()
                    .collect::<Vec<Vec<Vec2>>>();
                if outline.iter().any(|point| !point.is_finite())
                    || holes.iter().flatten().any(|point| !point.is_finite())
                {
                    continue;
                }
                let net = component.pad_nets.get(pi).cloned().unwrap_or_default();
                let contact_outline = pad
                    .contact_outline_polygon_mm(PHYSICAL_CURVE_SEGMENTS)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|p| transform_footprint_point(component, p, true))
                    .collect::<Vec<_>>();
                let contact_holes = pad
                    .contact_drill_polygon_mm(PHYSICAL_CURVE_SEGMENTS)
                    .map(|hole| {
                        hole.into_iter()
                            .map(|p| transform_footprint_point(component, p, true))
                            .collect::<Vec<_>>()
                    })
                    .into_iter()
                    .collect::<Vec<_>>();
                for layer in copper_layers(pad, component.layer) {
                    items.push(Copper {
                        source: component.component_id,
                        net: net.clone(),
                        layer,
                        contact_shape: Some(Shape::Pad {
                            outline: contact_outline.clone(),
                            holes: contact_holes.clone(),
                        }),
                        shape: Shape::Pad {
                            outline: outline.clone(),
                            holes: holes.clone(),
                        },
                        pad: Some((ci, pi)),
                        plated_through_hole: matches!(
                            &pad.technology,
                            FootprintPadTechnology::PlatedThroughHole
                        ),
                    });
                }
            }
        } else {
            // Existing documents retain their legacy illustrative footprints.
            let footprint = layout.resolved_footprint(component);
            for (pi, pad) in footprint.pads.iter().enumerate() {
                let half = pad.size * 0.5;
                let points = [
                    Vec2::new(-half.x, -half.y),
                    Vec2::new(half.x, -half.y),
                    half,
                    Vec2::new(-half.x, half.y),
                ]
                .into_iter()
                .map(|point| {
                    let local = PointMm {
                        x: pad.offset.x as f64 + point.x as f64,
                        y: pad.offset.y as f64 + point.y as f64,
                    };
                    transform_footprint_point(component, local, false)
                })
                .collect();
                items.push(Copper {
                    contact_shape: None,
                    source: component.component_id,
                    net: component.pad_nets.get(pi).cloned().unwrap_or_default(),
                    layer: component.layer,
                    shape: Shape::Pad {
                        outline: points,
                        holes: Vec::new(),
                    },
                    pad: Some((ci, pi)),
                    plated_through_hole: false,
                });
            }
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
                contact_shape: None,
                source: trace.id,
                net: trace.net.clone(),
                layer: trace.layer,
                shape: Shape::Segment(pair[0], pair[1], trace.width * 0.5),
                pad: None,
                plated_through_hole: false,
            });
        }
    }
    items
}

pub(super) fn copper_layers(pad: &FootprintPad, placement_layer: PcbLayer) -> Vec<PcbLayer> {
    let opposite = match placement_layer {
        PcbLayer::TopCopper => PcbLayer::BottomCopper,
        PcbLayer::BottomCopper => PcbLayer::TopCopper,
    };
    let placed_front = placement_layer;
    let placed_back = opposite;
    let mut layers = Vec::new();
    for layer in &pad.layers {
        let selected = match layer {
            FootprintLayer::FrontCopper => vec![placed_front],
            FootprintLayer::BackCopper => vec![placed_back],
            FootprintLayer::AllCopper => vec![PcbLayer::TopCopper, PcbLayer::BottomCopper],
            FootprintLayer::InnerCopper(_) | FootprintLayer::Other(_) => Vec::new(),
            _ => Vec::new(),
        };
        for selected_layer in selected {
            if !layers.contains(&selected_layer) {
                layers.push(selected_layer);
            }
        }
    }
    if matches!(&pad.technology, FootprintPadTechnology::PlatedThroughHole) {
        for layer in [PcbLayer::TopCopper, PcbLayer::BottomCopper] {
            if !layers.contains(&layer) {
                layers.push(layer);
            }
        }
    }
    layers
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

fn segment_pad_distance(a: Vec2, b: Vec2, outline: &[Vec2], holes: &[Vec<Vec2>]) -> f32 {
    if point_in_copper_pad(a, outline, holes)
        || point_in_copper_pad(b, outline, holes)
        || point_in_copper_pad((a + b) * 0.5, outline, holes)
    {
        return 0.0;
    }
    polygon_edges(outline)
        .chain(holes.iter().flat_map(|hole| polygon_edges(hole)))
        .map(|(start, end)| segment_distance(a, b, start, end))
        .fold(f32::INFINITY, f32::min)
}

fn pad_pad_distance(
    outline_a: &[Vec2],
    holes_a: &[Vec<Vec2>],
    outline_b: &[Vec2],
    holes_b: &[Vec<Vec2>],
) -> f32 {
    if outline_a
        .iter()
        .any(|point| point_in_copper_pad(*point, outline_b, holes_b))
        || outline_b
            .iter()
            .any(|point| point_in_copper_pad(*point, outline_a, holes_a))
    {
        return 0.0;
    }
    let edges_a = polygon_edges(outline_a)
        .chain(holes_a.iter().flat_map(|hole| polygon_edges(hole)))
        .collect::<Vec<_>>();
    let edges_b = polygon_edges(outline_b)
        .chain(holes_b.iter().flat_map(|hole| polygon_edges(hole)))
        .collect::<Vec<_>>();
    edges_a
        .iter()
        .flat_map(|(a, b)| {
            edges_b
                .iter()
                .map(move |(c, d)| segment_distance(*a, *b, *c, *d))
        })
        .fold(f32::INFINITY, f32::min)
}

fn point_in_copper_pad(point: Vec2, outline: &[Vec2], holes: &[Vec<Vec2>]) -> bool {
    if !point_in_polygon(point, outline) {
        return false;
    }
    !holes.iter().any(|hole| {
        point_in_polygon(point, hole)
            && polygon_edges(hole)
                .all(|(start, end)| point_segment_distance(point, start, end) > CONTACT_EPSILON)
    })
}

fn polygon_edges(points: &[Vec2]) -> impl Iterator<Item = (Vec2, Vec2)> + '_ {
    points
        .iter()
        .copied()
        .zip(points.iter().copied().cycle().skip(1))
        .take(points.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pcb::footprint_asset::parse_kicad_footprint;
    use crate::pcb::{PcbComponentPlacement, PcbLayer};

    #[test]
    fn physical_round_smd_respects_pad_and_component_rotation() {
        let asset = parse_kicad_footprint(
            r#"(footprint "RotatedPads"
                (version 20240108)
                (generator pcbnew)
                (layer "F.Cu")
                (pad "1" smd roundrect (at 2 0 45) (size 2 1) (layers "F.Cu" "F.Mask" "F.Paste") (roundrect_rratio 0.25))
            )"#,
        )
        .unwrap();
        assert!(asset.is_fabrication_ready(), "{:?}", asset.validate());

        let mut layout = PcbLayout::new("Board");
        let asset_id = asset.id.clone();
        layout.footprint_assets.insert(asset_id.clone(), asset);
        layout.components.push(PcbComponentPlacement {
            component_id: Uuid::new_v4(),
            designator: "U1".into(),
            value: "test".into(),
            footprint: asset_id,
            position: Vec2::new(10.0, 20.0),
            rotation: 90.0,
            layer: PcbLayer::BottomCopper,
            locked: false,
            image_asset: None,
            pad_nets: vec!["N1".into()],
        });

        let items = copper_items(&layout);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].layer, PcbLayer::BottomCopper);
        let Shape::Pad { outline, .. } = &items[0].shape else {
            panic!("a physical footprint pad must produce a polygon");
        };
        assert!(outline.len() > 8, "round corners should remain polygonal");
        let (minimum, maximum) = items[0].shape.bounds();
        let center = (minimum + maximum) * 0.5;
        assert!((center.x - 10.0).abs() < 0.02);
        assert!((center.y - 18.0).abs() < 0.02);
    }

    #[test]
    fn plated_hole_reaches_both_copper_faces_and_np_through_hole_is_excluded() {
        let asset = parse_kicad_footprint(
            r#"(footprint "MixedPads"
                (version 20240108)
                (generator pcbnew)
                (layer "F.Cu")
                (pad "1" smd oval (at 2 0 45) (size 2 1) (layers "F.Cu" "F.Mask" "F.Paste"))
                (pad "2" thru_hole circle (at 0 0) (size 1.8 1.8) (drill 0.8 (offset 0.1 0)) (layers "*.Cu" "*.Mask"))
                (pad "" np_thru_hole circle (at 4 0) (size 1.5 1.5) (drill 0.8) (layers "*.Mask"))
            )"#,
        )
        .unwrap();
        assert!(asset.is_fabrication_ready(), "{:?}", asset.validate());

        let mut layout = PcbLayout::new("Board");
        let asset_id = asset.id.clone();
        layout.footprint_assets.insert(asset_id.clone(), asset);
        layout.components.push(PcbComponentPlacement {
            component_id: Uuid::new_v4(),
            designator: "J1".into(),
            value: "test".into(),
            footprint: asset_id,
            position: Vec2::new(10.0, 20.0),
            rotation: 90.0,
            layer: PcbLayer::TopCopper,
            locked: false,
            image_asset: None,
            pad_nets: vec!["N1".into(), "N2".into(), String::new()],
        });

        let items = copper_items(&layout);
        assert_eq!(items.len(), 3, "NPTH creates no copper item");
        let smd = items.iter().find(|item| item.pad == Some((0, 0))).unwrap();
        assert_eq!(smd.layer, PcbLayer::TopCopper);
        let Shape::Pad { outline, .. } = &smd.shape else {
            panic!("SMD oval must keep its physical outline");
        };
        assert!(outline.len() > 8);

        let plated: Vec<_> = items
            .iter()
            .filter(|item| item.pad == Some((0, 1)))
            .collect();
        assert_eq!(plated.len(), 2);
        assert!(plated.iter().any(|item| item.layer == PcbLayer::TopCopper));
        assert!(plated
            .iter()
            .any(|item| item.layer == PcbLayer::BottomCopper));
        assert!(plated.iter().all(|item| item.plated_through_hole));
        for item in plated {
            let Shape::Pad { holes, .. } = &item.shape else {
                panic!("PTH copper must retain its drill void");
            };
            assert_eq!(holes.len(), 1);
            assert!(holes[0].len() >= 8);
        }
        assert!(items.iter().all(|item| item.pad != Some((0, 2))));
    }
}
