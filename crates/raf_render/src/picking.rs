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

/// Result of a ray-based pick query.
#[derive(Debug, Clone)]
pub struct RayPickResult {
    /// Entity index (matches scene graph iteration order).
    pub entity_index: usize,
    /// Distance along the ray to the first hit.
    pub hit_distance: f32,
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

/// Find the entity closest to a cursor-derived world ray.
pub fn pick_entity_ray(
    click_screen: [f32; 2],
    entities: &[(Vec3, f32)],
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
) -> Option<RayPickResult> {
    let (ray_origin, ray_dir) = screen_to_world_ray(click_screen, view_proj, vp_w, vp_h)?;
    let mut best: Option<RayPickResult> = None;

    for (i, (world_pos, radius)) in entities.iter().enumerate() {
        let expanded_radius = radius.max(0.45) * 1.15;
        let Some(hit_distance) =
            intersect_ray_sphere(ray_origin, ray_dir, *world_pos, expanded_radius)
        else {
            continue;
        };

        let is_better = match &best {
            None => true,
            Some(prev) => hit_distance < prev.hit_distance,
        };

        if is_better {
            best = Some(RayPickResult {
                entity_index: i,
                hit_distance,
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
    GizmoArrow {
        axis: Vec3::X,
        color: [220, 70, 70, 255],
        label: "X",
    },
    GizmoArrow {
        axis: Vec3::Y,
        color: [70, 200, 70, 255],
        label: "Y",
    },
    GizmoArrow {
        axis: Vec3::Z,
        color: [70, 100, 220, 255],
        label: "Z",
    },
];

/// Length of gizmo arrows in world units.
pub const GIZMO_LENGTH: f32 = 1.2;

/// Radius of the rotation rings in world units.
pub const GIZMO_ROTATION_RADIUS: f32 = 1.35;

/// Base radius of scale handles in screen pixels.
pub const GIZMO_SCALE_HANDLE_RADIUS: f32 = 8.0;

/// World-space gap between a scale handle and the selected object's face.
///
/// Scale handles are deliberately outside the object silhouette. The gap is
/// relative to the object, with a small floor and cap so tiny and very large
/// primitives remain easy to target without making the gizmo drift away.
pub const GIZMO_SCALE_HANDLE_OUTSET: f32 = 0.08;

/// Extra screen-space forgiveness around scale handles. The visible handle is
/// intentionally small, but its interaction target should remain comfortable
/// on high-DPI displays and while the viewport is being rendered adaptively.
pub const GIZMO_SCALE_HANDLE_PICK_PADDING: f32 = 12.0;

/// Resolve the visible and interactive scale-handle radius for the current
/// camera distance. The lower bound keeps handles comfortable up close, while
/// the cap prevents a distant camera from producing oversized controls.
pub fn gizmo_scale_handle_radius(presentation_scale: f32) -> f32 {
    let distance_growth = (presentation_scale.max(1.0) * 0.32).clamp(1.0, 1.8);
    GIZMO_SCALE_HANDLE_RADIUS * distance_growth
}

/// Invisible screen-space radius for translate-arrow picking. The shaft stays
/// visually thin; this larger target makes it reliable on high-DPI displays
/// and when the pointer moves a few pixels between hover and press frames.
pub const GIZMO_ARROW_PICK_RADIUS: f32 = 14.0;

/// Segments used for projected rotation ring hit-testing.
pub const GIZMO_ROTATION_SEGMENTS: usize = 48;

/// Denser sampling used only when a rotation plane is nearly parallel to the
/// camera ray and the exact ray/plane parameter cannot be resolved reliably.
const GIZMO_ROTATION_PARAMETER_SEGMENTS: usize = 192;

/// Arrowhead size (fraction of arrow length).
pub const GIZMO_HEAD_SIZE: f32 = 0.15;

/// Line thickness for gizmo.
pub const GIZMO_LINE_WIDTH: f32 = 2.5;

/// A scale handle projected to screen coordinates.
#[derive(Debug, Clone)]
pub struct GizmoScaleHandle {
    pub center: [f32; 2],
    pub axis_index: usize,
    pub sign: f32,
}

/// Project the 6 scale handles placed at the center of each face.
pub fn project_gizmo_scale_handles(
    entity_pos: Vec3,
    entity_scale: Vec3,
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
) -> Vec<GizmoScaleHandle> {
    project_gizmo_scale_handles_oriented(
        entity_pos,
        entity_scale,
        [Vec3::X, Vec3::Y, Vec3::Z],
        view_proj,
        vp_w,
        vp_h,
    )
}

/// Resolve the world directions and full world-space axis lengths represented
/// by a model matrix. The normalized columns are the actual transformed local
/// X/Y/Z axes, including parent transforms and mirrored scales.
pub fn gizmo_scale_basis(world: Mat4) -> ([Vec3; 3], Vec3) {
    let columns = [
        world.x_axis.truncate(),
        world.y_axis.truncate(),
        world.z_axis.truncate(),
    ];
    let mut axes = [Vec3::X, Vec3::Y, Vec3::Z];
    let mut scale = Vec3::ZERO;
    for (index, column) in columns.into_iter().enumerate() {
        let length = column.length();
        if length.is_finite() && length > 1e-5 {
            axes[index] = column / length;
            scale[index] = length;
        }
    }
    (axes, scale.max(Vec3::splat(0.01)))
}

/// World position of one scale handle. This is shared by rendering, picking,
/// and drag math so a rotated face cannot display one handle while interaction
/// follows a different, world-aligned axis.
pub fn gizmo_scale_handle_world_position(
    entity_pos: Vec3,
    entity_scale: Vec3,
    entity_axes: [Vec3; 3],
    axis_index: usize,
    sign: f32,
) -> Vec3 {
    let extents = entity_scale.abs().max(Vec3::splat(0.1)) * 0.5;
    let outset = (extents.max_element() * 0.12)
        .max(GIZMO_SCALE_HANDLE_OUTSET)
        .min(0.5);
    let axis_index = axis_index.min(2);
    let fallback = [Vec3::X, Vec3::Y, Vec3::Z][axis_index];
    let axis = normalized_axis_or(entity_axes[axis_index], fallback);
    entity_pos + axis * (extents[axis_index] + outset) * normalized_sign(sign)
}

/// Project scale handles along the selected object's transformed local axes.
pub fn project_gizmo_scale_handles_oriented(
    entity_pos: Vec3,
    entity_scale: Vec3,
    entity_axes: [Vec3; 3],
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
) -> Vec<GizmoScaleHandle> {
    let handles = [
        (0usize, 1.0f32),
        (0usize, -1.0f32),
        (1usize, 1.0f32),
        (1usize, -1.0f32),
        (2usize, 1.0f32),
        (2usize, -1.0f32),
    ];

    handles
        .iter()
        .filter_map(|(axis_index, sign)| {
            let world_position = gizmo_scale_handle_world_position(
                entity_pos,
                entity_scale,
                entity_axes,
                *axis_index,
                *sign,
            );
            project_to_screen(world_position, view_proj, vp_w, vp_h).map(|center| {
                GizmoScaleHandle {
                    center,
                    axis_index: *axis_index,
                    sign: *sign,
                }
            })
        })
        .collect()
}

/// Project a gizmo arrow from entity position to screen.
/// Returns (start_screen, end_screen, head_points) or None if behind camera.
pub fn project_gizmo_arrow(
    entity_pos: Vec3,
    arrow: &GizmoArrow,
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
) -> Option<GizmoScreenArrow> {
    project_gizmo_arrow_scaled(entity_pos, arrow, 1.0, view_proj, vp_w, vp_h)
}

/// Project a gizmo arrow with a presentation-only world scale.
///
/// The same scale is consumed by rendering and hit-testing so a larger gizmo
/// never becomes visually detached from its selectable geometry.
pub fn project_gizmo_arrow_scaled(
    entity_pos: Vec3,
    arrow: &GizmoArrow,
    presentation_scale: f32,
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
) -> Option<GizmoScreenArrow> {
    let tip = entity_pos + arrow.axis * GIZMO_LENGTH * presentation_scale.max(0.1);

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
    pick_gizmo_arrow_scaled(click, entity_pos, 1.0, view_proj, vp_w, vp_h)
}

/// Hit-test transform arrows using the same presentation scale as drawing.
pub fn pick_gizmo_arrow_scaled(
    click: [f32; 2],
    entity_pos: Vec3,
    presentation_scale: f32,
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
) -> Option<(usize, f32)> {
    let mut best: Option<(usize, f32)> = None;

    for (i, arrow) in GIZMO_ARROWS.iter().enumerate() {
        if let Some(screen) =
            project_gizmo_arrow_scaled(entity_pos, arrow, presentation_scale, view_proj, vp_w, vp_h)
        {
            let dist = point_to_segment_distance(click, screen.start, screen.end);
            if dist <= GIZMO_ARROW_PICK_RADIUS {
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

/// Hit-test projected scale handles at the default presentation scale.
pub fn pick_gizmo_scale_handle(
    click: [f32; 2],
    entity_pos: Vec3,
    entity_scale: Vec3,
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
) -> Option<(usize, f32, f32)> {
    pick_gizmo_scale_handle_scaled(click, entity_pos, entity_scale, 1.0, view_proj, vp_w, vp_h)
}

/// Hit-test projected scale handles using the same distance-aware radius used
/// by the renderer.
pub fn pick_gizmo_scale_handle_scaled(
    click: [f32; 2],
    entity_pos: Vec3,
    entity_scale: Vec3,
    presentation_scale: f32,
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
) -> Option<(usize, f32, f32)> {
    pick_gizmo_scale_handle_scaled_oriented(
        click,
        entity_pos,
        entity_scale,
        [Vec3::X, Vec3::Y, Vec3::Z],
        presentation_scale,
        view_proj,
        vp_w,
        vp_h,
    )
}

/// Hit-test scale handles against the same transformed local axes used by the
/// renderer.
pub fn pick_gizmo_scale_handle_scaled_oriented(
    click: [f32; 2],
    entity_pos: Vec3,
    entity_scale: Vec3,
    entity_axes: [Vec3; 3],
    presentation_scale: f32,
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
) -> Option<(usize, f32, f32)> {
    let pick_radius =
        gizmo_scale_handle_radius(presentation_scale) + GIZMO_SCALE_HANDLE_PICK_PADDING;
    let mut best: Option<(usize, f32, f32)> = None;

    for handle in project_gizmo_scale_handles_oriented(
        entity_pos,
        entity_scale,
        entity_axes,
        view_proj,
        vp_w,
        vp_h,
    ) {
        let dx = click[0] - handle.center[0];
        let dy = click[1] - handle.center[1];
        let distance = (dx * dx + dy * dy).sqrt();
        if distance <= pick_radius {
            let is_better = match best {
                None => true,
                Some((_, best_distance, _)) => distance < best_distance,
            };
            if is_better {
                best = Some((handle.axis_index, distance, handle.sign));
            }
        }
    }

    best
}

/// Hit-test a rotation ring projected in screen-space.
/// Returns axis index (0=X, 1=Y, 2=Z) and nearest distance.
pub fn pick_gizmo_rotation_ring(
    click: [f32; 2],
    entity_pos: Vec3,
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
) -> Option<(usize, f32)> {
    pick_gizmo_rotation_ring_scaled(click, entity_pos, 1.0, view_proj, vp_w, vp_h)
}

/// Hit-test a rotation ring with a presentation-only world scale.
pub fn pick_gizmo_rotation_ring_scaled(
    click: [f32; 2],
    entity_pos: Vec3,
    presentation_scale: f32,
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
) -> Option<(usize, f32)> {
    let axis_planes = [(Vec3::Y, Vec3::Z), (Vec3::X, Vec3::Z), (Vec3::X, Vec3::Y)];
    let radius = GIZMO_ROTATION_RADIUS * presentation_scale.max(0.1);

    let mut best: Option<(usize, f32)> = None;

    for (axis_idx, (axis_a, axis_b)) in axis_planes.iter().enumerate() {
        let mut previous: Option<[f32; 2]> = None;
        let mut best_distance_for_ring: Option<f32> = None;

        for step in 0..=GIZMO_ROTATION_SEGMENTS {
            let angle = (step as f32 / GIZMO_ROTATION_SEGMENTS as f32) * std::f32::consts::TAU;
            let world_point =
                entity_pos + *axis_a * (angle.cos() * radius) + *axis_b * (angle.sin() * radius);

            if let Some(screen_point) = project_to_screen(world_point, view_proj, vp_w, vp_h) {
                if let Some(prev) = previous {
                    let distance = point_to_segment_distance(click, prev, screen_point);
                    best_distance_for_ring = Some(match best_distance_for_ring {
                        Some(current) => current.min(distance),
                        None => distance,
                    });
                }
                previous = Some(screen_point);
            }
        }

        if let Some(distance) = best_distance_for_ring {
            if distance <= 12.0 {
                let is_better = match best {
                    None => true,
                    Some((_, best_distance)) => distance < best_distance,
                };

                if is_better {
                    best = Some((axis_idx, distance));
                }
            }
        }
    }

    best
}

/// Resolve the angular parameter of a pointer around one projected rotation
/// ring. The exact path intersects the pointer ray with the ring plane; a
/// continuity-aware projected fallback handles nearly edge-on rings.
pub fn rotation_ring_parameter_scaled(
    pointer: [f32; 2],
    entity_pos: Vec3,
    axis_index: usize,
    presentation_scale: f32,
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
    reference_angle: Option<f32>,
) -> Option<f32> {
    let (axis_a, axis_b) = rotation_axis_plane(axis_index)?;
    let normal = axis_a.cross(axis_b).normalize_or_zero();

    if let Some((ray_origin, ray_dir)) = screen_to_world_ray(pointer, view_proj, vp_w, vp_h) {
        let denominator = ray_dir.dot(normal);
        if denominator.abs() > 1e-4 {
            let distance = (entity_pos - ray_origin).dot(normal) / denominator;
            if distance.is_finite() && distance >= 0.0 {
                let relative = ray_origin + ray_dir * distance - entity_pos;
                let x = relative.dot(axis_a);
                let y = relative.dot(axis_b);
                if x.is_finite() && y.is_finite() && x * x + y * y > 1e-8 {
                    return Some(y.atan2(x));
                }
            }
        }
    }

    projected_rotation_ring_parameter(
        pointer,
        entity_pos,
        axis_a,
        axis_b,
        presentation_scale,
        view_proj,
        vp_w,
        vp_h,
        reference_angle,
    )
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
    Some([(ndc_x + 1.0) * 0.5 * vp_w, (1.0 - ndc_y) * 0.5 * vp_h])
}

fn rotation_axis_plane(axis_index: usize) -> Option<(Vec3, Vec3)> {
    match axis_index {
        0 => Some((Vec3::Y, Vec3::Z)),
        // Positive Y rotation maps +X toward -Z in the engine's right-handed
        // transform convention, so the parameter basis mirrors Z here.
        1 => Some((Vec3::X, -Vec3::Z)),
        2 => Some((Vec3::X, Vec3::Y)),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn projected_rotation_ring_parameter(
    pointer: [f32; 2],
    entity_pos: Vec3,
    axis_a: Vec3,
    axis_b: Vec3,
    presentation_scale: f32,
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
    reference_angle: Option<f32>,
) -> Option<f32> {
    let radius = GIZMO_ROTATION_RADIUS * presentation_scale.max(0.1);
    let mut samples = Vec::with_capacity(GIZMO_ROTATION_PARAMETER_SEGMENTS);
    let mut best_distance_sq = f32::INFINITY;
    for step in 0..GIZMO_ROTATION_PARAMETER_SEGMENTS {
        let angle =
            (step as f32 / GIZMO_ROTATION_PARAMETER_SEGMENTS as f32) * std::f32::consts::TAU;
        let world_point =
            entity_pos + axis_a * (angle.cos() * radius) + axis_b * (angle.sin() * radius);
        let Some(screen) = project_to_screen(world_point, view_proj, vp_w, vp_h) else {
            continue;
        };
        let dx = pointer[0] - screen[0];
        let dy = pointer[1] - screen[1];
        let distance_sq = dx * dx + dy * dy;
        best_distance_sq = best_distance_sq.min(distance_sq);
        samples.push((angle, distance_sq));
    }
    if samples.is_empty() || !best_distance_sq.is_finite() {
        return None;
    }

    // Opposite ring points can project to the same pixel when the plane is
    // edge-on. Keep candidates inside a two-pixel band and choose the one
    // nearest the previous angular parameter to prevent branch flipping.
    let distance_band = best_distance_sq + 4.0;
    samples
        .into_iter()
        .filter(|(_, distance_sq)| *distance_sq <= distance_band)
        .min_by(|(angle_a, distance_a), (angle_b, distance_b)| {
            let score_a = reference_angle
                .map(|reference| wrapped_angle_delta(reference, *angle_a).abs())
                .unwrap_or(*distance_a);
            let score_b = reference_angle
                .map(|reference| wrapped_angle_delta(reference, *angle_b).abs())
                .unwrap_or(*distance_b);
            score_a.total_cmp(&score_b)
        })
        .map(|(angle, _)| angle)
}

fn wrapped_angle_delta(from: f32, to: f32) -> f32 {
    (to - from + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

fn normalized_axis_or(axis: Vec3, fallback: Vec3) -> Vec3 {
    let length = axis.length();
    if length.is_finite() && length > 1e-5 {
        axis / length
    } else {
        fallback
    }
}

fn normalized_sign(sign: f32) -> f32 {
    if sign < 0.0 {
        -1.0
    } else {
        1.0
    }
}

fn screen_to_world_ray(
    click_screen: [f32; 2],
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
) -> Option<(Vec3, Vec3)> {
    let inv_view_proj = view_proj.inverse();
    let ndc_x = (click_screen[0] / vp_w) * 2.0 - 1.0;
    let ndc_y = 1.0 - (click_screen[1] / vp_h) * 2.0;

    let near_clip = inv_view_proj * Vec4::new(ndc_x, ndc_y, -1.0, 1.0);
    let far_clip = inv_view_proj * Vec4::new(ndc_x, ndc_y, 1.0, 1.0);
    if near_clip.w.abs() <= 0.0001 || far_clip.w.abs() <= 0.0001 {
        return None;
    }

    let near_world = near_clip.truncate() / near_clip.w;
    let far_world = far_clip.truncate() / far_clip.w;
    let ray_dir = (far_world - near_world).normalize_or_zero();
    if ray_dir.length_squared() <= 0.0001 {
        return None;
    }

    Some((near_world, ray_dir))
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

fn intersect_ray_sphere(origin: Vec3, dir: Vec3, center: Vec3, radius: f32) -> Option<f32> {
    let oc = origin - center;
    let a = dir.dot(dir);
    let b = 2.0 * oc.dot(dir);
    let c = oc.dot(oc) - radius * radius;
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return None;
    }

    let sqrt_disc = discriminant.sqrt();
    let t0 = (-b - sqrt_disc) / (2.0 * a);
    let t1 = (-b + sqrt_disc) / (2.0 * a);

    if t0 >= 0.0 {
        Some(t0)
    } else if t1 >= 0.0 {
        Some(t1)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Quat;

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

    #[test]
    fn translate_arrow_pick_has_invisible_forgiveness() {
        let picked = pick_gizmo_arrow_scaled(
            [400.0, 313.0],
            Vec3::ZERO,
            1.0,
            &Mat4::IDENTITY,
            800.0,
            600.0,
        );
        assert!(picked.is_some());
    }

    #[test]
    fn scale_face_pick_has_invisible_forgiveness() {
        let picked = pick_gizmo_scale_handle(
            [517.0, 300.0],
            Vec3::ZERO,
            Vec3::splat(0.5),
            &Mat4::IDENTITY,
            800.0,
            600.0,
        );
        assert!(picked.is_some());
    }

    #[test]
    fn scale_handles_project_outside_each_face() {
        let handles = project_gizmo_scale_handles(
            Vec3::ZERO,
            Vec3::splat(0.5),
            &Mat4::IDENTITY,
            800.0,
            600.0,
        );

        assert_eq!(handles.len(), 6);
        assert!(handles[0].center[0] > 500.0);
        assert!(handles[1].center[0] < 300.0);
        assert!(handles[2].center[1] < 225.0);
        assert!(handles[3].center[1] > 375.0);
    }

    #[test]
    fn scale_basis_tracks_rotated_world_axes_and_lengths() {
        let world = Mat4::from_scale_rotation_translation(
            Vec3::new(2.0, 3.0, 4.0),
            Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            Vec3::ZERO,
        );
        let (axes, scale) = gizmo_scale_basis(world);

        assert!((axes[0] - Vec3::Y).length() < 1e-5);
        assert!((axes[1] + Vec3::X).length() < 1e-5);
        assert!((axes[2] - Vec3::Z).length() < 1e-5);
        assert!((scale - Vec3::new(2.0, 3.0, 4.0)).length() < 1e-5);
    }

    #[test]
    fn scale_handles_follow_rotated_local_faces() {
        let handles = project_gizmo_scale_handles_oriented(
            Vec3::ZERO,
            Vec3::splat(0.5),
            [Vec3::Y, -Vec3::X, Vec3::Z],
            &Mat4::IDENTITY,
            800.0,
            600.0,
        );

        assert_eq!(handles.len(), 6);
        assert!(handles[0].center[1] < 225.0);
        assert!(handles[1].center[1] > 375.0);
        assert!(handles[2].center[0] < 300.0);
        assert!(handles[3].center[0] > 500.0);
    }

    #[test]
    fn rotation_parameter_crosses_pi_without_reversing_direction() {
        let first_expected = 179.0f32.to_radians();
        let second_expected = -179.0f32.to_radians();
        let screen_point = |angle: f32| {
            let world = Vec3::new(
                angle.cos() * GIZMO_ROTATION_RADIUS,
                angle.sin() * GIZMO_ROTATION_RADIUS,
                0.0,
            );
            project_to_screen(world, &Mat4::IDENTITY, 800.0, 600.0).unwrap()
        };
        let first = rotation_ring_parameter_scaled(
            screen_point(first_expected),
            Vec3::ZERO,
            2,
            1.0,
            &Mat4::IDENTITY,
            800.0,
            600.0,
            None,
        )
        .unwrap();
        let second = rotation_ring_parameter_scaled(
            screen_point(second_expected),
            Vec3::ZERO,
            2,
            1.0,
            &Mat4::IDENTITY,
            800.0,
            600.0,
            Some(first),
        )
        .unwrap();

        let delta = wrapped_angle_delta(first, second);
        assert!((delta - 2.0f32.to_radians()).abs() < 1e-4);
    }
}
