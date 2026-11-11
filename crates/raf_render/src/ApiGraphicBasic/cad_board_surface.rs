//! Board material follows a closed, simple contour. Open or invalid outlines
//! keep their editable edge strokes and never acquire an invented solid body.
use glam::Vec2;
use std::cell::RefCell;

use super::command_list::{BasicCommandList, BasicScreenTriangle};

type BoardTriangles = Vec<[Vec2; 3]>;
thread_local! {
    static LAST_CONTOUR: RefCell<Option<(Vec<Vec2>, Option<BoardTriangles>)>> = const { RefCell::new(None) };
}

pub(super) fn record_board_material(
    commands: &mut BasicCommandList,
    points: &[Vec2],
    bounds: [f32; 4],
    size: [u32; 2],
    clear: [u8; 4],
) {
    let [left, right, top, bottom] = bounds;
    if right <= left || bottom <= top || points.len() > 513 {
        return;
    }
    LAST_CONTOUR.with(|memo| {
        let mut memo = memo.borrow_mut();
        if memo.as_ref().is_none_or(|(previous, _)| previous != points) {
            *memo = Some((points.to_vec(), triangulate(points)));
        }
        let Some(triangles) = memo.as_ref().and_then(|(_, triangles)| triangles.as_ref()) else {
            return;
        };
        let color = if u16::from(clear[0]) + u16::from(clear[1]) + u16::from(clear[2]) > 384 {
            [214, 231, 221, 255]
        } else {
            [10, 58, 36, 255]
        };
        for triangle in triangles {
            commands.draw_screen_triangle(BasicScreenTriangle {
                points: triangle.map(|p| {
                    [
                        (p.x - left) / (right - left) * size[0] as f32,
                        (p.y - top) / (bottom - top) * size[1] as f32,
                    ]
                }),
                color,
            });
        }
    });
}

fn cross(a: Vec2, b: Vec2, c: Vec2) -> f32 {
    (b - a).perp_dot(c - a)
}

fn triangulate(closed: &[Vec2]) -> Option<BoardTriangles> {
    // Bound the optional fill work, including malformed contour input. The
    // original CAD edge geometry remains available above this display limit.
    if closed.len() < 4
        || closed.len() > 513
        || closed.iter().any(|p| !p.is_finite())
        || closed.first()?.distance(*closed.last()?) > 0.01
    {
        return None;
    }
    let mut points = closed[..closed.len() - 1].to_vec();
    points.dedup_by(|a, b| a.distance_squared(*b) < 0.000001);
    let n = points.len();
    if n < 3 {
        return None;
    }
    // Reject crossing/touching non-adjacent edges before ear clipping. A
    // self-intersecting outline must not be represented as a valid substrate.
    for i in 0..n {
        for j in i + 1..n {
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            let (a, b, c, d) = (
                points[i],
                points[(i + 1) % n],
                points[j],
                points[(j + 1) % n],
            );
            let bounds_overlap = a.min(b).cmple(c.max(d)).all() && c.min(d).cmple(a.max(b)).all();
            if bounds_overlap
                && cross(a, b, c) * cross(a, b, d) <= 0.0
                && cross(c, d, a) * cross(c, d, b) <= 0.0
            {
                return None;
            }
        }
    }
    let area = (0..n)
        .map(|i| points[i].perp_dot(points[(i + 1) % n]))
        .sum::<f32>();
    if area.abs() <= 0.000001 {
        return None;
    }
    let winding = area.signum();
    let mut remaining = (0..n).collect::<Vec<_>>();
    let mut triangles = Vec::with_capacity(n - 2);
    while remaining.len() > 3 {
        let mut ear = None;
        for k in 0..remaining.len() {
            let a = points[remaining[(k + remaining.len() - 1) % remaining.len()]];
            let b = points[remaining[k]];
            let c = points[remaining[(k + 1) % remaining.len()]];
            if cross(a, b, c) * winding <= 0.000001 {
                continue;
            }
            if remaining.iter().any(|&index| {
                let p = points[index];
                p != a
                    && p != b
                    && p != c
                    && cross(a, b, p) * winding >= -0.000001
                    && cross(b, c, p) * winding >= -0.000001
                    && cross(c, a, p) * winding >= -0.000001
            }) {
                continue;
            }
            ear = Some((k, [a, b, c]));
            break;
        }
        let (index, triangle) = ear?;
        triangles.push(triangle);
        remaining.remove(index);
    }
    triangles.push([
        points[remaining[0]],
        points[remaining[1]],
        points[remaining[2]],
    ]);
    Some(triangles)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn executed_cad_fill_leaves_a_concave_cutout_empty() {
        use super::super::cad_surface::{build_cad_surface_frame, CadSurfaceOptions};
        use super::super::device::{BasicDevice, BasicDeviceConfig};
        let mut pcb = raf_electronics::pcb::PcbLayout::new("Concave material QA");
        pcb.board_outline.points = vec![
            Vec2::new(0., 0.),
            Vec2::new(8., 0.),
            Vec2::new(8., 3.),
            Vec2::new(3., 3.),
            Vec2::new(3., 8.),
            Vec2::new(0., 8.),
            Vec2::new(0., 0.),
        ];
        let scene = raf_electronics::CadScene::from_pcb(&pcb);
        let frame = build_cad_surface_frame(
            &scene,
            100,
            100,
            CadSurfaceOptions {
                clear_color: [10, 10, 11, 255],
                show_grid: false,
                world_bounds: Some([-1., 9., -1., 9.]),
                ..Default::default()
            },
        );
        let mut device = BasicDevice::new(BasicDeviceConfig {
            allow_gpu: false,
            force_cpu: true,
            ..Default::default()
        });
        device.execute_scene_frame(&frame.frame);
        let capture = device.capture_last_scene_rgba().unwrap();
        let pixel = |x: usize, y: usize| &capture.rgba8[(y * 100 + x) * 4..(y * 100 + x) * 4 + 4];
        assert_eq!(pixel(20, 20), &[10, 58, 36, 255]);
        assert_eq!(pixel(70, 70), &[10, 10, 11, 255]);
    }
    #[test]
    fn concave_board_material_preserves_the_cutout_in_either_winding() {
        let contour = vec![
            Vec2::new(0., 0.),
            Vec2::new(8., 0.),
            Vec2::new(8., 3.),
            Vec2::new(3., 3.),
            Vec2::new(3., 8.),
            Vec2::new(0., 8.),
            Vec2::new(0., 0.),
        ];
        for points in [contour.clone(), contour.into_iter().rev().collect()] {
            let triangles = triangulate(&points).unwrap();
            let area: f32 = triangles
                .iter()
                .map(|t| cross(t[0], t[1], t[2]).abs() * 0.5)
                .sum();
            assert!((area - 39.).abs() < 0.001);
            let cutout = Vec2::new(6., 6.);
            assert!(!triangles.iter().any(|t| {
                let w = cross(t[0], t[1], t[2]).signum();
                cross(t[0], t[1], cutout) * w >= 0.
                    && cross(t[1], t[2], cutout) * w >= 0.
                    && cross(t[2], t[0], cutout) * w >= 0.
            }));
        }
    }
    #[test]
    fn open_and_self_crossing_boards_have_no_solid_material() {
        assert!(triangulate(&[Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y]).is_none());
        assert!(triangulate(&[Vec2::ZERO, Vec2::ONE, Vec2::Y, Vec2::X, Vec2::ZERO]).is_none());
    }
}
