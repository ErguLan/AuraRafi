//! CAD picking, object bounds and canvas coordinate mapping.

use glam::{Mat4, Vec2};
use raf_electronics::{CadObject, CadObjectKind, CadRect};

use super::{CadSurfaceHitRegion, CadSurfaceOptions};

/// Reject CAD objects that cannot contribute to the current presentation.
///
/// Schematic and PCB documents can contain many off-screen objects. Keeping
/// them in the model is required for editing, but recording their geometry and
/// hit regions every frame is wasted work on low-end machines. The test is
/// intentionally conservative: a polyline uses its full bounds, so a cable
/// crossing the viewport is never culled accidentally.
pub(super) fn cad_object_intersects_bounds(object: &CadObject, bounds: [f32; 4]) -> bool {
    let [left, right, top, bottom] = bounds;
    if right <= left || bottom <= top {
        return false;
    }

    let mut bounds: Option<(glam::Vec2, glam::Vec2)> = object.rect.map(|rect| {
        let half = rotated_half_extent(rect.size, object.rotation_degrees.to_radians());
        (rect.center - half, rect.center + half)
    });

    let mut include_point = |point: glam::Vec2| {
        if let Some((min, max)) = &mut bounds {
            *min = min.min(point);
            *max = max.max(point);
        } else {
            bounds = Some((point, point));
        }
    };
    for point in &object.points {
        include_point(*point);
    }
    for path in &object.line_paths {
        for point in path {
            include_point(*point);
        }
    }

    let Some((mut min, mut max)) = bounds else {
        return false;
    };

    let pad = match object.kind {
        CadObjectKind::Trace => 3.0,
        CadObjectKind::Airwire => 6.0,
        CadObjectKind::Pin | CadObjectKind::Pad => 6.0,
        _ => 2.0,
    };
    min -= glam::Vec2::splat(pad);
    max += glam::Vec2::splat(pad);
    max.x >= left && min.x <= right && max.y >= top && min.y <= bottom
}

fn rotated_half_extent(size: Vec2, rotation: f32) -> Vec2 {
    let half = size.abs() * 0.5;
    let (sin, cos) = rotation.sin_cos();
    Vec2::new(cos.abs() * half.x + sin.abs() * half.y, sin.abs() * half.x + cos.abs() * half.y)
}

pub(super) fn cad_hit_region(object: &CadObject, options: &CadSurfaceOptions) -> Option<CadSurfaceHitRegion> {
    let bounds = if let Some(rect) = object.rect {
        CadRect::new(
            rect.center,
            rotated_half_extent(rect.size, object.rotation_degrees.to_radians()) * 2.0,
        )
    } else {
        polyline_bounds(object, object_pick_width(object, options))?
    };
    Some(CadSurfaceHitRegion {
        id: object.id.clone(),
        source_id: object.source_id.map(|id| id.to_string()),
        kind: object.kind,
        layer: object.layer,
        pick_priority: object.pick_priority,
        bounds,
        label: object.label.clone(),
        net: object.net.clone(),
    })
}

fn polyline_bounds(object: &CadObject, width: f32) -> Option<CadRect> {
    let first = object.points.first().copied()?;
    let mut min = first;
    let mut max = first;
    for point in &object.points {
        min.x = min.x.min(point.x);
        min.y = min.y.min(point.y);
        max.x = max.x.max(point.x);
        max.y = max.y.max(point.y);
    }
    let pad = width.max(6.0);
    let size = (max - min).abs() + Vec2::splat(pad * 2.0);
    Some(CadRect::new((min + max) * 0.5, size))
}

fn object_pick_width(object: &CadObject, options: &CadSurfaceOptions) -> f32 {
    if let Some(width)=object.stroke_width_world.filter(|w|w.is_finite() && *w>0.0) {return width;}
    match object.kind {
        CadObjectKind::Trace => options.trace_width,
        CadObjectKind::Airwire => 4.0,
        CadObjectKind::BoardOutline => 6.0,
        _ => options.wire_width,
    }
}

pub(super) fn canvas_view_projection(left: f32, right: f32, top: f32, bottom: f32) -> Mat4 {
    let width = (right - left).max(0.001);
    let height = (bottom - top).max(0.001);
    let scale_x = 2.0 / width;
    let scale_y = -2.0 / height;
    let translate_x = -(right + left) / width;
    let translate_y = (bottom + top) / height;

    Mat4::from_cols_array(&[
        scale_x,
        0.0,
        0.0,
        0.0,
        0.0,
        scale_y,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
        translate_x,
        translate_y,
        0.0,
        1.0,
    ])
}
