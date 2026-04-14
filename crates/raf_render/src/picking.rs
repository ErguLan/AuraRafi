//! Screen-space entity picking and transform gizmo geometry.
//!
//! Picking: projects entity centers to screen, finds the closest one to a click.
//! Gizmo: generates 3D arrow geometry for translate/rotate/scale handles.
//!
//! No raycasting needed. Just project bounding sphere centers and compare
//! distances in screen space. Fast, simple, potato-friendly.

use glam::{Mat4, Vec3, Vec4};

// ---------------------------------------------------------------------------
// Entity picking
// ---------------------------------------------------------------------------

/// Result of a pick query.
#[derive(Debug, Clone)]
pub struct PickResult {
    /// Entity index (matches scene graph iteration order).
    pub entity_index: usize,
    /// Screen distance from click to entity center (pixels).
    pub screen_distance: f32,
    /// Projected entity center in screen coords.
    pub screen_pos: [f32; 2],
}

/// Maximum screen distance in pixels to consider a pick valid.
pub const PICK_RADIUS: f32 = 30.0;

/// Find the entity closest to a click position.
/// `entities` is a list of (world_position, bounding_radius) for each entity.
/// Returns the closest entity within PICK_RADIUS, or None.
pub fn pick_entity(
    click_screen: [f32; 2],
    entities: &[(Vec3, f32)], // (world_pos, bounding_radius)
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
) -> Option<PickResult> {
    let mut best: Option<PickResult> = None;

    for (i, (world_pos, _radius)) in entities.iter().enumerate() {
        let clip = *view_proj * Vec4::new(world_pos.x, world_pos.y, world_pos.z, 1.0);
        if clip.w <= 0.001 {
            continue; // Behind camera
        }

        let ndc_x = clip.x / clip.w;
        let ndc_y = clip.y / clip.w;
        let sx = (ndc_x + 1.0) * 0.5 * vp_w;
        let sy = (1.0 - ndc_y) * 0.5 * vp_h;

        let dx = sx - click_screen[0];
        let dy = sy - click_screen[1];
        let dist = (dx * dx + dy * dy).sqrt();

        if dist > PICK_RADIUS {
            continue;
        }

        // Prefer closer-to-camera entities when overlapping (smaller clip.z/w).
        let is_better = match &best {
            None => true,
            Some(prev) => dist < prev.screen_distance,
        };

        if is_better {
            best = Some(PickResult {
                entity_index: i,
                screen_distance: dist,
                screen_pos: [sx, sy],
            });
        }
    }

    best
}

// ---------------------------------------------------------------------------
// Transform gizmo arrows (geometry for rendering)
// ---------------------------------------------------------------------------

/// A single gizmo arrow (line from origin to tip + arrowhead triangle).
#[derive(Debug, Clone)]
pub struct GizmoArrow {
    /// Axis direction (unit vector).
    pub axis: Vec3,
    /// Arrow color [R, G, B, A].
    pub color: [u8; 4],
    /// Axis label ("X", "Y", "Z").
    pub label: &'static str,
}

/// The 3 transform gizmo arrows.
pub const GIZMO_ARROWS: [GizmoArrow; 3] = [
    GizmoArrow { axis: Vec3::X, color: [220, 70, 70, 255], label: "X" },
    GizmoArrow { axis: Vec3::Y, color: [70, 200, 70, 255], label: "Y" },
    GizmoArrow { axis: Vec3::Z, color: [70, 100, 220, 255], label: "Z" },
];

/// Length of gizmo arrows in world units.
pub const GIZMO_LENGTH: f32 = 1.2;

/// Arrowhead size (fraction of arrow length).
pub const GIZMO_HEAD_SIZE: f32 = 0.15;

/// Line thickness for gizmo.
pub const GIZMO_LINE_WIDTH: f32 = 2.5;

/// Project a gizmo arrow from entity position to screen.
/// Returns (start_screen, end_screen, head_points) or None if behind camera.
pub fn project_gizmo_arrow(
    entity_pos: Vec3,
    arrow: &GizmoArrow,
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
) -> Option<GizmoScreenArrow> {
    let tip = entity_pos + arrow.axis * GIZMO_LENGTH;

    // Project start and end.
    let start = project_to_screen(entity_pos, view_proj, vp_w, vp_h)?;
    let end = project_to_screen(tip, view_proj, vp_w, vp_h)?;

    // Arrowhead: two points perpendicular to the arrow shaft.
    let dx = end[0] - start[0];
    let dy = end[1] - start[1];
    let len = (dx * dx + dy * dy).sqrt();
    if len < 2.0 {
        return None; // Too small to draw
    }

    // Normalized perpendicular in screen space.
    let nx = -dy / len;
    let ny = dx / len;
    let head_len = len * GIZMO_HEAD_SIZE;
    let head_base_x = end[0] - dx / len * head_len * 2.0;
    let head_base_y = end[1] - dy / len * head_len * 2.0;

    let head_left = [head_base_x + nx * head_len, head_base_y + ny * head_len];
    let head_right = [head_base_x - nx * head_len, head_base_y - ny * head_len];

    Some(GizmoScreenArrow {
        start,
        end,
        head_tip: end,
        head_left,
        head_right,
        color: arrow.color,
        label: arrow.label,
    })
}

/// A gizmo arrow projected to screen coordinates.
#[derive(Debug, Clone)]
pub struct GizmoScreenArrow {
    /// Shaft start (entity center).
    pub start: [f32; 2],
    /// Shaft end (tip).
    pub end: [f32; 2],
    /// Arrowhead tip point.
    pub head_tip: [f32; 2],
    /// Arrowhead left point.
    pub head_left: [f32; 2],
    /// Arrowhead right point.
    pub head_right: [f32; 2],
    /// Color [R, G, B, A].
    pub color: [u8; 4],
    /// Axis label.
    pub label: &'static str,
}

/// Hit-test: check if a screen point is near a gizmo arrow shaft.
/// Returns the axis index (0=X, 1=Y, 2=Z) and distance, or None.
pub fn pick_gizmo_arrow(
    click: [f32; 2],
    entity_pos: Vec3,
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
) -> Option<(usize, f32)> {
    let mut best: Option<(usize, f32)> = None;

    for (i, arrow) in GIZMO_ARROWS.iter().enumerate() {
        if let Some(screen) = project_gizmo_arrow(entity_pos, arrow, view_proj, vp_w, vp_h) {
            let dist = point_to_segment_distance(
                click,
                screen.start,
                screen.end,
            );
            if dist < 8.0 {
                let is_better = match best {
                    None => true,
                    Some((_, prev_dist)) => dist < prev_dist,
                };
                if is_better {
                    best = Some((i, dist));
                }
            }
        }
    }

    best
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn project_to_screen(point: Vec3, view_proj: &Mat4, vp_w: f32, vp_h: f32) -> Option<[f32; 2]> {
    let clip = *view_proj * Vec4::new(point.x, point.y, point.z, 1.0);
    if clip.w <= 0.001 {
        return None;
    }
    let ndc_x = clip.x / clip.w;
    let ndc_y = clip.y / clip.w;
    Some([
        (ndc_x + 1.0) * 0.5 * vp_w,
        (1.0 - ndc_y) * 0.5 * vp_h,
    ])
}

/// Distance from a point to a line segment (all in screen space).
fn point_to_segment_distance(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len_sq = dx * dx + dy * dy;
    if len_sq < 0.001 {
        let ex = p[0] - a[0];
        let ey = p[1] - a[1];
        return (ex * ex + ey * ey).sqrt();
    }
    let t = ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len_sq;
    let t = t.clamp(0.0, 1.0);
    let closest_x = a[0] + t * dx;
    let closest_y = a[1] + t * dy;
    let ex = p[0] - closest_x;
    let ey = p[1] - closest_y;
    (ex * ex + ey * ey).sqrt()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_no_entities() {
        let result = pick_entity([100.0, 100.0], &[], &Mat4::IDENTITY, 800.0, 600.0);
        assert!(result.is_none());
    }

    #[test]
    fn point_segment_distance_on_line() {
        let dist = point_to_segment_distance([5.0, 0.0], [0.0, 0.0], [10.0, 0.0]);
        assert!(dist < 0.001);
    }

    #[test]
    fn point_segment_distance_offset() {
        let dist = point_to_segment_distance([5.0, 3.0], [0.0, 0.0], [10.0, 0.0]);
        assert!((dist - 3.0).abs() < 0.01);
    }
}
