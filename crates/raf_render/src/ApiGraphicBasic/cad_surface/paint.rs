//! Backend-neutral CAD primitives and paths shared by the schematic and PCB passes.

use std::sync::{Arc, OnceLock};

use glam::{Mat4, Quat, Vec2, Vec3};
use raf_electronics::cad_scene::CadShape;
use raf_electronics::CadRect;

use crate::api_graphic_basic::command_list::BasicCommandList;
use crate::api_graphic_basic::mesh::BasicMesh;

use super::Density;

/// Airwire guide stroke width, in logical points.
pub(super) const AIRWIRE_LOGICAL_WIDTH: f32 = 1.25;

/// Airwire dash period, in physical target pixels, before the stroke width is
/// taken into account.
const AIRWIRE_DASH_PIXELS: f32 = 6.0;

/// Gap between two airwire dashes as a fraction of the dash itself.
const AIRWIRE_GAP_RATIO: f32 = 0.9;

/// Hard cap on the number of dashes one segment may emit.
///
/// A one-pixel zoom on a large board would otherwise ask for hundreds of
/// thousands of line instances in a single frame.
const AIRWIRE_MAX_DASHES: usize = 2048;

pub(super) fn with_alpha(color: [u8; 4], alpha: u8) -> [u8; 4] {
    [color[0], color[1], color[2], alpha]
}

/// A filled, optionally rotated quad. One unit mesh covers every box-like mark:
/// object bodies, pads, net label plates, junction dots and the square/diamond
/// DRC shapes.
pub(super) fn record_quad(
    commands: &mut BasicCommandList,
    quad_id: usize,
    center: Vec2,
    size: Vec2,
    z: f32,
    rotation: f32,
    color: [u8; 4],
) {
    if color[3] == 0 || size.x <= 0.0 || size.y <= 0.0 {
        return;
    }
    let origin = center - rotate_vec2(size * 0.5, rotation);
    let transform = Mat4::from_scale_rotation_translation(
        Vec3::new(size.x, size.y, 1.0),
        Quat::from_rotation_z(rotation),
        Vec3::new(origin.x, origin.y, z),
    );
    commands.draw_mesh(quad_id, transform, color);
}

/// A filled n-gon disc, used for the informational DRC ring and for junction
/// dots so a meeting point reads as a node rather than as a fourth wire.
pub(super) fn record_disc(
    commands: &mut BasicCommandList,
    disc_id: usize,
    center: Vec2,
    radius: f32,
    z: f32,
    color: [u8; 4],
) {
    if color[3] == 0 || radius <= 0.0 {
        return;
    }
    let size = Vec2::splat(radius * 2.0);
    let min = center - Vec2::splat(radius);
    let transform = Mat4::from_scale_rotation_translation(
        Vec3::new(size.x, size.y, 1.0),
        Quat::IDENTITY,
        Vec3::new(min.x, min.y, z),
    );
    commands.draw_mesh(disc_id, transform, color);
}

pub(super) fn record_border(
    commands: &mut BasicCommandList,
    rect: CadRect,
    z: f32,
    logical_width: f32,
    color: [u8; 4],
    density: Density,
    rotation: f32,
) {
    if color[3] == 0 || logical_width <= 0.0 || rect.size.x <= 0.0 || rect.size.y <= 0.0 {
        return;
    }
    let half = rect.size.abs() * 0.5;
    let corner = |offset: Vec2| rect.center + rotate_vec2(offset, rotation);
    let top_left = corner(Vec2::new(-half.x, -half.y));
    let top_right = corner(Vec2::new(half.x, -half.y));
    let bottom_right = corner(Vec2::new(half.x, half.y));
    let bottom_left = corner(Vec2::new(-half.x, half.y));
    record_line(
        commands,
        top_left,
        top_right,
        z,
        logical_width,
        color,
        density,
    );
    record_line(
        commands,
        top_right,
        bottom_right,
        z,
        logical_width,
        color,
        density,
    );
    record_line(
        commands,
        bottom_right,
        bottom_left,
        z,
        logical_width,
        color,
        density,
    );
    record_line(
        commands,
        bottom_left,
        top_left,
        z,
        logical_width,
        color,
        density,
    );
}

/// The one place a CAD stroke reaches the backend-neutral command list.
///
/// `BasicLine::width` is target pixels for both executors (see
/// `command_list::BasicLine`), so the logical widths authored in
/// `CadSurfaceOptions` are converted once, here. See [`Density::pixels`] for
/// why the width is a logical quantity and not a physical one.
pub(super) fn record_line(
    commands: &mut BasicCommandList,
    start: Vec2,
    end: Vec2,
    z: f32,
    logical_width: f32,
    color: [u8; 4],
    density: Density,
) {
    let width = density.pixels(logical_width);
    if color[3] == 0 || width <= 0.0 || start.distance(end) <= f32::EPSILON {
        return;
    }
    commands.draw_line(
        Vec3::new(start.x, start.y, z),
        Vec3::new(end.x, end.y, z),
        color,
        width,
        true,
        0.0,
    );
}

/// The axis-aligned footprint a rotated quad actually covers.
///
/// `record_quad` rotates about the rect center, so a caller that wants the
/// border of a diamond needs the rotated extent, not the original rect.
pub(super) fn rotated_rect(center: Vec2, size: Vec2, rotation: f32) -> CadRect {
    let half = rotate_vec2(size * 0.5, rotation).abs();
    CadRect::new(center, half * 2.0)
}

pub(super) fn rotate_vec2(value: Vec2, radians: f32) -> Vec2 {
    let (sin, cos) = radians.sin_cos();
    Vec2::new(value.x * cos - value.y * sin, value.x * sin + value.y * cos)
}

pub(super) fn record_shape_fill(
    commands: &mut BasicCommandList,
    quad_id: usize,
    disc_id: Option<usize>,
    center: Vec2,
    size: Vec2,
    shape: CadShape,
    z: f32,
    rotation: f32,
    color: [u8; 4],
) {
    if color[3] == 0 || size.min_element() <= 0.0 || !size.is_finite() {
        return;
    }
    let half = size.abs() * 0.5;
    match shape {
        CadShape::Rectangle => record_quad(commands, quad_id, center, size, z, rotation, color),
        CadShape::Circle => {
            if let Some(disc_id) = disc_id {
                record_disc(commands, disc_id, center, half.min_element(), z, color);
            } else {
                record_quad(commands, quad_id, center, size, z, rotation, color);
            }
        }
        CadShape::Oval => {
            let radius = half.min_element();
            if half.x >= half.y {
                let cap_offset = Vec2::new((half.x - radius).max(0.0), 0.0);
                let bar_size = Vec2::new((size.x - radius * 2.0).max(0.0), size.y);
                record_quad(commands, quad_id, center, bar_size, z, rotation, color);
                if let Some(disc_id) = disc_id {
                    for side in [-1.0, 1.0] {
                        record_disc(
                            commands,
                            disc_id,
                            center + rotate_vec2(cap_offset * side, rotation),
                            radius,
                            z,
                            color,
                        );
                    }
                }
            } else {
                let cap_offset = Vec2::new(0.0, (half.y - radius).max(0.0));
                let bar_size = Vec2::new(size.x, (size.y - radius * 2.0).max(0.0));
                record_quad(commands, quad_id, center, bar_size, z, rotation, color);
                if let Some(disc_id) = disc_id {
                    for side in [-1.0, 1.0] {
                        record_disc(
                            commands,
                            disc_id,
                            center + rotate_vec2(cap_offset * side, rotation),
                            radius,
                            z,
                            color,
                        );
                    }
                }
            }
        }
        CadShape::RoundRect { corner_radius } => {
            let radius = safe_corner_radius(corner_radius, half);
            if radius <= f32::EPSILON {
                record_quad(commands, quad_id, center, size, z, rotation, color);
                return;
            }
            record_quad(
                commands,
                quad_id,
                center,
                Vec2::new((size.x - 2.0 * radius).max(0.0), size.y),
                z,
                rotation,
                color,
            );
            record_quad(
                commands,
                quad_id,
                center,
                Vec2::new(size.x, (size.y - 2.0 * radius).max(0.0)),
                z,
                rotation,
                color,
            );
            if let Some(disc_id) = disc_id {
                let corner = Vec2::new(half.x - radius, half.y - radius);
                for x in [-1.0, 1.0] {
                    for y in [-1.0, 1.0] {
                        record_disc(
                            commands,
                            disc_id,
                            center + rotate_vec2(corner * Vec2::new(x, y), rotation),
                            radius,
                            z,
                            color,
                        );
                    }
                }
            }
        }
    }
}

pub(super) fn record_shape_outline(
    commands: &mut BasicCommandList,
    rect: CadRect,
    shape: CadShape,
    rotation: f32,
    z: f32,
    width: f32,
    color: [u8; 4],
    density: Density,
) {
    if color[3] == 0 || width <= 0.0 || rect.size.min_element() <= 0.0 {
        return;
    }
    let points = shape_outline_points(rect.size.abs(), shape);
    for segment in points.windows(2) {
        record_line(
            commands,
            rect.center + rotate_vec2(segment[0], rotation),
            rect.center + rotate_vec2(segment[1], rotation),
            z,
            width,
            color,
            density,
        );
    }
}

fn shape_outline_points(size: Vec2, shape: CadShape) -> Vec<Vec2> {
    const CIRCLE_SEGMENTS: usize = 48;
    const ARC_SEGMENTS: usize = 12;
    let half = size * 0.5;
    match shape {
        CadShape::Rectangle => vec![
            Vec2::new(-half.x, -half.y),
            Vec2::new(half.x, -half.y),
            Vec2::new(half.x, half.y),
            Vec2::new(-half.x, half.y),
            Vec2::new(-half.x, -half.y),
        ],
        CadShape::Circle => (0..=CIRCLE_SEGMENTS)
            .map(|step| {
                let angle = step as f32 / CIRCLE_SEGMENTS as f32 * std::f32::consts::TAU;
                Vec2::new(angle.cos(), angle.sin()) * half.min_element()
            })
            .collect(),
        CadShape::Oval => capsule_outline_points(size, ARC_SEGMENTS),
        CadShape::RoundRect { corner_radius } => {
            let radius = safe_corner_radius(corner_radius, half);
            if radius <= f32::EPSILON {
                return shape_outline_points(size, CadShape::Rectangle);
            }
            let centers = [
                (Vec2::new(half.x - radius, -half.y + radius), -90.0),
                (Vec2::new(half.x - radius, half.y - radius), 0.0),
                (Vec2::new(-half.x + radius, half.y - radius), 90.0),
                (Vec2::new(-half.x + radius, -half.y + radius), 180.0),
            ];
            let mut points = Vec::with_capacity(centers.len() * (ARC_SEGMENTS + 1) + 1);
            for (center, start_degrees) in centers {
                for step in 0..=ARC_SEGMENTS {
                    let angle = (start_degrees + 90.0 * step as f32 / ARC_SEGMENTS as f32)
                        .to_radians();
                    points.push(center + Vec2::new(angle.cos(), angle.sin()) * radius);
                }
            }
            points.push(points[0]);
            points
        }
    }
}

fn capsule_outline_points(size: Vec2, arc_segments: usize) -> Vec<Vec2> {
    let half = size.abs() * 0.5;
    let radius = half.min_element();
    let mut points = Vec::with_capacity(2 * (arc_segments + 1) + 1);
    if half.x >= half.y {
        let cap = (half.x - radius).max(0.0);
        append_arc(&mut points, Vec2::new(-cap, 0.0), radius, 90.0, 270.0, arc_segments);
        append_arc(&mut points, Vec2::new(cap, 0.0), radius, 270.0, 450.0, arc_segments);
    } else {
        let cap = (half.y - radius).max(0.0);
        append_arc(&mut points, Vec2::new(0.0, -cap), radius, 0.0, 180.0, arc_segments);
        append_arc(&mut points, Vec2::new(0.0, cap), radius, 180.0, 360.0, arc_segments);
    }
    points.push(points[0]);
    points
}

fn append_arc(
    points: &mut Vec<Vec2>,
    center: Vec2,
    radius: f32,
    start_degrees: f32,
    end_degrees: f32,
    segments: usize,
) {
    for step in 0..=segments {
        let angle = (start_degrees
            + (end_degrees - start_degrees) * step as f32 / segments.max(1) as f32)
            .to_radians();
        points.push(center + Vec2::new(angle.cos(), angle.sin()) * radius);
    }
}

pub(super) fn expanded_shape(shape: CadShape, amount: f32) -> CadShape {
    match shape {
        CadShape::RoundRect { corner_radius } => CadShape::RoundRect {
            corner_radius: corner_radius + amount,
        },
        other => other,
    }
}

fn safe_corner_radius(radius: f32, half: Vec2) -> f32 {
    if radius.is_finite() {
        radius.clamp(0.0, half.min_element())
    } else {
        0.0
    }
}

pub(super) fn outset_rect(rect: CadRect, margin: f32) -> CadRect {
    CadRect::new(rect.center, rect.size + Vec2::splat(margin * 2.0))
}

/// Renders a polyline as a dashed guide.
///
/// Airwires are connections that still have to be routed. Drawing them like a
/// trace with only a lower alpha meant "missing copper" and "copper" differed by
/// a channel that disappears on a bad display or for a color-blind reader. A
/// dash pattern makes the pending state legible on its own.
pub(super) fn record_dashed_path(
    commands: &mut BasicCommandList,
    points: &[Vec2],
    z: f32,
    logical_width: f32,
    color: [u8; 4],
    density: Density,
) {
    let stroke_px = density.pixels(logical_width);
    let period = (AIRWIRE_DASH_PIXELS.max(stroke_px * 2.0)) * density.world_per_pixel;
    if period <= f32::EPSILON {
        return;
    }
    let dash = period / (1.0 + AIRWIRE_GAP_RATIO);
    let gap = period - dash;
    if dash <= f32::EPSILON {
        return;
    }

    for segment in points.windows(2) {
        let (start, end) = (segment[0], segment[1]);
        let length = start.distance(end);
        if length <= f32::EPSILON {
            continue;
        }
        let direction = (end - start) / length;
        let mut travelled = 0.0;
        let mut dashes = 0usize;
        while travelled < length && dashes < AIRWIRE_MAX_DASHES {
            let dash_end = (travelled + dash).min(length);
            record_line(
                commands,
                start + direction * travelled,
                start + direction * dash_end,
                z,
                logical_width,
                color,
                density,
            );
            travelled = dash_end + gap;
            dashes += 1;
        }
    }
}

/// Shared unit quad, allocated once.
///
/// The GPU mesh cache is keyed by `Arc::as_ptr` (see `BasicDevice::draw_mesh`),
/// so a fresh `Arc` every frame makes residency depend on allocator address
/// reuse: a hit is possible for different geometry and an eviction is arbitrary.
/// A `OnceLock` keeps one strong reference alive for the process, which makes
/// both the frame-local dedup key and the GPU cache key stable.
pub(super) fn unit_quad_mesh() -> Arc<BasicMesh> {
    static UNIT_QUAD: OnceLock<Arc<BasicMesh>> = OnceLock::new();
    UNIT_QUAD
        .get_or_init(|| {
            Arc::new(BasicMesh::from_positions(
                &[
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(1.0, 0.0, 0.0),
                    Vec3::new(1.0, 1.0, 0.0),
                    Vec3::new(0.0, 1.0, 0.0),
                ],
                &[0, 1, 2, 0, 2, 3],
            ))
        })
        .clone()
}

/// Shared unit disc, allocated once, for the same cache-key reason as the quad.
///
/// A 16-gon fan wound counter-clockwise in local XY, matching the unit quad, so
/// back-face culling keeps behaving identically on both executors.
pub(super) fn disc_mesh() -> Arc<BasicMesh> {
    const SEGMENTS: usize = 48;
    static UNIT_DISC: OnceLock<Arc<BasicMesh>> = OnceLock::new();
    UNIT_DISC
        .get_or_init(|| {
            let mut positions = Vec::with_capacity(SEGMENTS + 1);
            positions.push(Vec3::new(0.5, 0.5, 0.0));
            for step in 0..SEGMENTS {
                let angle = (step as f32 / SEGMENTS as f32) * std::f32::consts::TAU;
                positions.push(Vec3::new(
                    0.5 + angle.cos() * 0.5,
                    0.5 + angle.sin() * 0.5,
                    0.0,
                ));
            }
            let mut indices = Vec::with_capacity(SEGMENTS * 3);
            for step in 0..SEGMENTS {
                let rim = step as u32 + 1;
                let next = ((step + 1) % SEGMENTS) as u32 + 1;
                indices.extend_from_slice(&[0, rim, next]);
            }
            Arc::new(BasicMesh::from_positions(&positions, &indices))
        })
        .clone()
}
