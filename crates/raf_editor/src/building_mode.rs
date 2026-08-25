//! Organized building constraints for the Game viewport editor.
//!
//! The Free building style keeps the historical unconstrained authoring
//! behavior. The Organized style quantizes every transform gesture to a fixed
//! metric step and resolves entity overlap by clamping flush against the
//! blocking surface instead of allowing pass-through.
//!
//! All functions here are pure scene queries: they never mutate the graph and
//! never touch renderer state, so the viewport controller stays the single
//! owner of when constraints apply.

use glam::Vec3;

use raf_core::scene::graph::{SceneGraph, SceneNodeId};

/// Contact tolerance in meters. Surfaces closer than this are considered
/// touching (allowed) rather than overlapping (blocked), so an object resting
/// flush against a wall can keep sliding along that wall.
pub const CONTACT_EPSILON: f32 = 1e-3;

/// Half thickness assigned to Plane primitives along Y, which otherwise have
/// no volume.
const PLANE_HALF_THICKNESS: f32 = 0.005;

/// Axis-aligned bounding box in world space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn from_min_max(min: Vec3, max: Vec3) -> Self {
        Self {
            min: min.min(max),
            max: max.max(min),
        }
    }

    pub fn unite(self, other: Aabb) -> Aabb {
        Aabb {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    /// Signed gap between this box and `wall` measured along `axis` when this
    /// box advances by positive values. Positive means free space, negative
    /// means already interpenetrating on that axis.
    fn leading_gap(&self, wall: &Aabb, axis: usize) -> f32 {
        match axis {
            0 => wall.min.x - self.max.x,
            1 => wall.min.y - self.max.y,
            _ => wall.min.z - self.max.z,
        }
    }

    /// Signed gap when advancing in the negative direction of `axis`.
    fn trailing_gap(&self, wall: &Aabb, axis: usize) -> f32 {
        match axis {
            0 => self.min.x - wall.max.x,
            1 => self.min.y - wall.max.y,
            _ => self.min.z - wall.max.z,
        }
    }

    /// True when both boxes overlap on the two axes perpendicular to `axis`
    /// by more than the contact tolerance. Only such overlaps can block
    /// motion along `axis`; lateral contact must stay permitted so boxes can
    /// slide while flush.
    fn laterally_overlaps(&self, wall: &Aabb, axis: usize) -> bool {
        (0..3)
            .filter(|candidate| *candidate != axis)
            .all(|candidate| {
                let a_min = self.axis_min(candidate);
                let a_max = self.axis_max(candidate);
                let b_min = wall.axis_min(candidate);
                let b_max = wall.axis_max(candidate);
                (a_max - b_min > CONTACT_EPSILON) && (b_max - a_min > CONTACT_EPSILON)
            })
    }

    fn axis_min(&self, axis: usize) -> f32 {
        match axis {
            0 => self.min.x,
            1 => self.min.y,
            _ => self.min.z,
        }
    }

    fn axis_max(&self, axis: usize) -> f32 {
        match axis {
            0 => self.max.x,
            1 => self.max.y,
            _ => self.max.z,
        }
    }
}

/// World-space AABB of a node's own geometry, including its descendants, from
/// its world transform and unit primitive extents.
///
/// Returns `None` for organizational folders and empty nodes without solid
/// descendants because those carry no blocking geometry.
pub fn node_aabb(scene: &SceneGraph, id: SceneNodeId) -> Option<Aabb> {
    subtree_aabb(scene, id)
}

fn subtree_aabb(scene: &SceneGraph, id: SceneNodeId) -> Option<Aabb> {
    let mut acc: Option<Aabb> = None;
    visit_solids(scene, id, &mut |solid| {
        let bounds = solid_aabb(scene, solid);
        acc = Some(match acc {
            Some(current) => current.unite(bounds),
            None => bounds,
        });
    });
    acc
}

/// Visits every solid node in the subtree rooted at `id` (including itself).
pub fn visit_solids(scene: &SceneGraph, id: SceneNodeId, visit: &mut dyn FnMut(SceneNodeId)) {
    let Some(node) = scene.get(id) else {
        return;
    };
    if node.is_folder {
        for child in node.children.clone() {
            visit_solids(scene, child, visit);
        }
        return;
    }
    if is_solid(scene, id) {
        visit(id);
    }
    for child in node.children.clone() {
        visit_solids(scene, child, visit);
    }
}

/// A node contributes blocking geometry when it is visible, has a visible
/// primitive shape, and is not an organizational folder. Locked nodes still
/// block: locking protects them from edits, not from being walls.
pub fn is_solid(scene: &SceneGraph, id: SceneNodeId) -> bool {
    let Some(node) = scene.get(id) else {
        return false;
    };
    !node.is_folder && node.visible && node.primitive != raf_core::scene::Primitive::Empty
}

/// Unit half-extents contributed by each primitive before node scale.
fn primitive_half_extents(scene: &SceneGraph, id: SceneNodeId) -> Option<Vec3> {
    let node = scene.get(id)?;
    let scale = node.scale.abs().max(Vec3::splat(0.01));
    let base = match node.primitive {
        raf_core::scene::Primitive::Empty => return None,
        raf_core::scene::Primitive::Cube | raf_core::scene::Primitive::Sphere => Vec3::splat(0.5),
        raf_core::scene::Primitive::Plane => Vec3::new(0.5, PLANE_HALF_THICKNESS, 0.5),
        raf_core::scene::Primitive::Cylinder => Vec3::new(0.5, 0.5, 0.5),
    };
    Some(base * scale)
}

/// Conservative world AABB for one node using its world transform corners.
fn solid_aabb(scene: &SceneGraph, id: SceneNodeId) -> Aabb {
    let world = scene.world_matrix(id);
    let half = primitive_half_extents(scene, id).unwrap_or(Vec3::ZERO);
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for sign_x in [-1.0, 1.0] {
        for sign_y in [-1.0, 1.0] {
            for sign_z in [-1.0, 1.0] {
                let corner_world = world.transform_point3(half * Vec3::new(sign_x, sign_y, sign_z));
                min = min.min(corner_world);
                max = max.max(corner_world);
            }
        }
    }
    Aabb { min, max }
}

/// Collects the union AABB of everything a mover subtree occupies at its
/// current transforms.
pub fn mover_aabb(scene: &SceneGraph, mover_root: SceneNodeId) -> Option<Aabb> {
    subtree_aabb(scene, mover_root)
}

/// True when `id` descends from any node in `roots` or is itself one of them.
pub fn is_descendant_of_any(scene: &SceneGraph, id: SceneNodeId, roots: &[SceneNodeId]) -> bool {
    let mut current = Some(id);
    while let Some(node_id) = current {
        if roots.contains(&node_id) {
            return true;
        }
        current = scene.get(node_id).and_then(|node| node.parent);
    }
    false
}

/// World AABBs of every solid node that is not part of the moving set.
pub fn wall_aabbs(scene: &SceneGraph, movers: &[SceneNodeId]) -> Vec<Aabb> {
    let mut walls = Vec::new();
    for (id, _node) in scene.iter() {
        if !is_solid(scene, id) || is_descendant_of_any(scene, id, movers) {
            continue;
        }
        walls.push(solid_aabb(scene, id));
    }
    walls
}

/// Quantizes `value` to multiples of `step`. A non-positive step disables
/// snapping.
pub fn snap_scalar(value: f32, step: f32) -> f32 {
    if step <= f32::EPSILON || !value.is_finite() {
        return value;
    }
    (value / step).round() * step
}

/// Clamps a signed translation advance along `axis` so the moving union never
/// penetrates any wall. Returns the allowed advance with the same sign as the
/// desired value, stopping exactly flush with the closest blocker.
///
/// Walls already deep inside the mover (gap below minus tolerance) are ignored:
/// the resolver must not lock objects that were placed overlapping while Free
/// was active.
pub fn clamp_translation(start_union: Aabb, walls: &[Aabb], axis: usize, desired: f32) -> f32 {
    if desired.abs() <= f32::EPSILON {
        return desired;
    }
    let mut allowed = desired.abs();
    for wall in walls {
        if !start_union.laterally_overlaps(wall, axis) {
            continue;
        }
        let gap = if desired > 0.0 {
            start_union.leading_gap(wall, axis)
        } else {
            start_union.trailing_gap(wall, axis)
        };
        if gap < -CONTACT_EPSILON {
            continue;
        }
        allowed = allowed.min(gap.max(0.0));
    }
    if allowed < desired.abs() {
        allowed * desired.signum()
    } else {
        desired
    }
}

/// True when the mover union at `proposed` penetrates `wall` beyond the
/// contact tolerance.
pub fn penetrates_one(proposed: Aabb, wall: &Aabb) -> bool {
    (0..3).all(|axis| {
        proposed.axis_min(axis) < wall.axis_max(axis) - CONTACT_EPSILON
            && wall.axis_min(axis) < proposed.axis_max(axis) - CONTACT_EPSILON
    })
}

/// True when the mover union at `proposed` penetrates any wall beyond the
/// contact tolerance.
pub fn penetrating(proposed: Aabb, walls: &[Aabb]) -> bool {
    walls.iter().any(|wall| penetrates_one(proposed, wall))
}

/// Grows `start_union` along `axis` by independent min/max shifts and clamps
/// each side against walls. Returns `(min_shift, max_shift)` actually allowed.
///
/// Used by scale gestures: dragging the +X face only extends `max`, while
/// uniform growth extends both sides symmetrically.
pub fn clamp_growth(
    start_union: Aabb,
    walls: &[Aabb],
    axis: usize,
    min_shift: f32,
    max_shift: f32,
) -> (f32, f32) {
    let mut resolved_min = min_shift;
    let mut resolved_max = max_shift;
    for wall in walls {
        if !start_union.laterally_overlaps(wall, axis) {
            continue;
        }
        if max_shift > 0.0 {
            let gap = start_union.leading_gap(wall, axis);
            if gap >= -CONTACT_EPSILON {
                resolved_max = resolved_max.min(gap.max(0.0));
            }
        }
        if min_shift < 0.0 {
            let gap = start_union.trailing_gap(wall, axis);
            if gap >= -CONTACT_EPSILON {
                resolved_min = resolved_min.max(-gap.max(0.0));
            }
        }
    }
    (resolved_min, resolved_max)
}

/// Rotation search budget: binary refinement steps used to find the largest
/// collision-free angle below the requested one.
pub const ROTATION_REFINE_STEPS: usize = 10;

/// Builds the rotated AABB of `start_union` after rotating it around `pivot`
/// by `theta` radians on `axis`.
pub fn rotated_aabb(start_union: Aabb, pivot: Vec3, theta: f32, axis: usize) -> Aabb {
    let rotation = match axis {
        0 => glam::Quat::from_rotation_x(theta),
        1 => glam::Quat::from_rotation_y(theta),
        _ => glam::Quat::from_rotation_z(theta),
    };
    let corners = [
        Vec3::new(start_union.min.x, start_union.min.y, start_union.min.z),
        Vec3::new(start_union.max.x, start_union.min.y, start_union.min.z),
        Vec3::new(start_union.min.x, start_union.max.y, start_union.min.z),
        Vec3::new(start_union.min.x, start_union.min.y, start_union.max.z),
        Vec3::new(start_union.max.x, start_union.max.y, start_union.min.z),
        Vec3::new(start_union.max.x, start_union.min.y, start_union.max.z),
        Vec3::new(start_union.min.x, start_union.max.y, start_union.max.z),
        Vec3::new(start_union.max.x, start_union.max.y, start_union.max.z),
    ];
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for corner in corners {
        let rotated = pivot + rotation * (corner - pivot);
        min = min.min(rotated);
        max = max.max(rotated);
    }
    Aabb { min, max }
}

/// Finds the largest magnitude rotation not exceeding `desired_theta` that
/// keeps the mover collision-free. The result is exact when the full angle is
/// already valid and approaches the blocking surface otherwise.
///
/// Walls the mover already interpenetrates at rest are ignored: objects
/// resting on a floor plane or placed overlapping while Free was active must
/// keep rotating instead of being permanently locked.
pub fn clamp_rotation(
    start_union: Aabb,
    pivot: Vec3,
    axis: usize,
    desired_theta: f32,
    walls: &[Aabb],
) -> f32 {
    if desired_theta.abs() <= f32::EPSILON {
        return desired_theta;
    }
    let live: Vec<Aabb> = walls
        .iter()
        .copied()
        .filter(|wall| !penetrates_one(start_union, wall))
        .collect();
    if live.is_empty() {
        return desired_theta;
    }
    if !penetrating(rotated_aabb(start_union, pivot, desired_theta, axis), &live) {
        return desired_theta;
    }
    let mut low = 0.0f32;
    let mut high = desired_theta.abs();
    for _ in 0..ROTATION_REFINE_STEPS {
        let mid = (low + high) * 0.5;
        if penetrating(rotated_aabb(start_union, pivot, mid, axis), &live) {
            high = mid;
        } else {
            low = mid;
        }
    }
    if low <= f32::EPSILON {
        // Even the smallest refinement step collides. Hard-locking the handle
        // feels broken inside the editor, so the requested rotation wins and
        // the (visible) overlap becomes the user's decision.
        return desired_theta;
    }
    low.copysign(desired_theta.signum())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_at(center: [f32; 3], size: [f32; 3]) -> Aabb {
        let half = Vec3::from_array(size) * 0.5;
        let center = Vec3::from_array(center);
        Aabb {
            min: center - half,
            max: center + half,
        }
    }

    #[test]
    fn snap_scalar_quantizes_to_step() {
        assert!((snap_scalar(2.37, 1.0) - 2.0).abs() < 1e-5);
        assert!((snap_scalar(2.6, 1.0) - 3.0).abs() < 1e-5);
        assert!((snap_scalar(0.63, 0.25) - 0.75).abs() < 1e-5);
        assert!((snap_scalar(1.234, 0.0) - 1.234).abs() < 1e-5);
    }

    #[test]
    fn translation_stops_flush_before_wall() {
        let mover = box_at([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let wall = box_at([3.0, 0.0, 0.0], [1.0, 4.0, 4.0]);
        let clamped = clamp_translation(mover, &[wall], 0, 5.0);
        assert!(
            (clamped - 2.0).abs() < 1e-4,
            "expected flush stop, got {clamped}"
        );
    }

    #[test]
    fn translation_negative_direction_clamps_against_wall() {
        let mover = box_at([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        // Wall face at x = -3.5; mover face starts at x = -0.5 -> 3 m of room.
        let wall = box_at([-4.0, 0.0, 0.0], [1.0, 4.0, 4.0]);
        let clamped = clamp_translation(mover, &[wall], 0, -5.0);
        assert!(
            (clamped + 3.0).abs() < 1e-4,
            "expected flush stop, got {clamped}"
        );
    }

    #[test]
    fn translation_passes_when_laterally_clear() {
        let mover = box_at([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let wall = box_at([3.0, 5.0, 0.0], [1.0, 1.0, 4.0]);
        let clamped = clamp_translation(mover, &[wall], 0, 10.0);
        assert!((clamped - 10.0).abs() < 1e-5);
    }

    #[test]
    fn translation_ignores_walls_already_swallowed() {
        let mover = box_at([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        let swallowed = box_at([0.2, 0.0, 0.0], [0.5, 0.5, 0.5]);
        let clamped = clamp_translation(mover, &[swallowed], 0, 1.0);
        assert!((clamped - 1.0).abs() < 1e-5);
    }

    #[test]
    fn growth_extending_toward_wall_is_clamped() {
        let mover = box_at([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let wall = box_at([2.0, 0.0, 0.0], [1.0, 4.0, 4.0]);
        let (min_shift, max_shift) = clamp_growth(mover, &[wall], 0, 0.0, 4.0);
        assert!(min_shift.abs() < 1e-5);
        assert!(
            (max_shift - 1.0).abs() < 1e-4,
            "expected 1m growth, got {max_shift}"
        );
    }

    #[test]
    fn uniform_growth_clamps_both_sides_independently() {
        let mover = box_at([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let right_wall = box_at([2.0, 0.0, 0.0], [1.0, 4.0, 4.0]);
        // Left wall face at x = -0.9 -> only 0.4 m of room on that side.
        let left_wall = box_at([-1.4, 0.0, 0.0], [1.0, 4.0, 4.0]);
        let (min_shift, max_shift) = clamp_growth(mover, &[right_wall, left_wall], 0, -2.0, 2.0);
        assert!((min_shift + 0.4).abs() < 1e-4);
        assert!((max_shift - 1.0).abs() < 1e-4);
    }

    #[test]
    fn rotation_full_angle_allowed_when_free() {
        let mover = box_at([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let result = clamp_rotation(mover, Vec3::ZERO, 1, std::f32::consts::FRAC_PI_2, &[]);
        assert!((result - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
    }

    #[test]
    fn rotation_reduces_until_flush_with_wall() {
        // Rectangular footprint (3 m along X) so rotating about Y changes the
        // AABB; a square would sweep onto itself and never block.
        let mover = box_at([0.0, 0.0, 0.0], [3.0, 0.2, 1.0]);
        // Wall band just past the corner sweep radius (~1.58 m on Z).
        let wall = box_at([0.0, 0.0, -1.55], [8.0, 4.0, 0.2]);
        let requested = std::f32::consts::FRAC_PI_2;
        let result = clamp_rotation(mover, Vec3::ZERO, 1, requested, &[wall]);
        assert!(
            result.abs() < requested - 0.05,
            "rotation should back off, got {result}"
        );
        let rotated = rotated_aabb(mover, Vec3::ZERO, result, 1);
        assert!(!penetrating(rotated, &[wall]));
    }

    #[test]
    fn rotated_aabb_matches_input_when_zero() {
        let mover = box_at([1.0, 2.0, 3.0], [2.0, 1.0, 4.0]);
        let rotated = rotated_aabb(mover, Vec3::ONE, 0.0, 2);
        assert!((rotated.min - mover.min).length() < 1e-5);
        assert!((rotated.max - mover.max).length() < 1e-5);
    }
}
