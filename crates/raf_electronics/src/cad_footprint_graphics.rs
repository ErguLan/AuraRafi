//! Vector silkscreen projection for physical PCB footprints.
use crate::pcb::geometry::transform_footprint_point;
use crate::pcb::{FootprintGraphicGeometry as Graphic, PcbComponentPlacement, PointMm};
use glam::Vec2;

pub(super) fn graphic_path(
    placement: &PcbComponentPlacement,
    graphic: &Graphic,
) -> Option<(Vec<Vec2>, f32)> {
    let (local, width) = match graphic {
        Graphic::Line {
            start_mm,
            end_mm,
            width_mm,
        } => (vec![*start_mm, *end_mm], *width_mm),
        Graphic::Rectangle {
            start_mm,
            end_mm,
            width_mm,
            ..
        } => (
            vec![
                *start_mm,
                PointMm {
                    x: end_mm.x,
                    y: start_mm.y,
                },
                *end_mm,
                PointMm {
                    x: start_mm.x,
                    y: end_mm.y,
                },
                *start_mm,
            ],
            *width_mm,
        ),
        Graphic::Polygon {
            points_mm,
            width_mm,
            ..
        } => {
            let mut points = points_mm.clone();
            if let Some(first) = points.first().copied() {
                points.push(first);
            }
            (points, *width_mm)
        }
        Graphic::Circle {
            center_mm,
            radius_mm,
            width_mm,
            ..
        } => (
            (0..=96)
                .map(|i| {
                    let a = i as f64 * std::f64::consts::TAU / 96.0;
                    PointMm {
                        x: center_mm.x + radius_mm * a.cos(),
                        y: center_mm.y + radius_mm * a.sin(),
                    }
                })
                .collect(),
            *width_mm,
        ),
        Graphic::Arc {
            start_mm: a,
            mid_mm: b,
            end_mm: c,
            width_mm,
        } => {
            let determinant = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
            if determinant.abs() < 1e-12 {
                return None;
            }
            let (aa, bb, cc) = (
                a.x * a.x + a.y * a.y,
                b.x * b.x + b.y * b.y,
                c.x * c.x + c.y * c.y,
            );
            let center = PointMm {
                x: (aa * (b.y - c.y) + bb * (c.y - a.y) + cc * (a.y - b.y)) / determinant,
                y: (aa * (c.x - b.x) + bb * (a.x - c.x) + cc * (b.x - a.x)) / determinant,
            };
            let radius = (a.x - center.x).hypot(a.y - center.y);
            let start = (a.y - center.y).atan2(a.x - center.x);
            let end = (c.y - center.y).atan2(c.x - center.x);
            let ccw = (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x) > 0.0;
            let sweep = if ccw {
                (end - start).rem_euclid(std::f64::consts::TAU)
            } else {
                -((start - end).rem_euclid(std::f64::consts::TAU))
            };
            (
                (0..=96)
                    .map(|i| {
                        let angle = start + sweep * i as f64 / 96.0;
                        PointMm {
                            x: center.x + radius * angle.cos(),
                            y: center.y + radius * angle.sin(),
                        }
                    })
                    .collect(),
                *width_mm,
            )
        }
        Graphic::Text { .. } | Graphic::Unsupported { .. } => return None,
    };
    Some((
        local
            .into_iter()
            .map(|p| transform_footprint_point(placement, p, true))
            .collect(),
        width as f32,
    ))
}
