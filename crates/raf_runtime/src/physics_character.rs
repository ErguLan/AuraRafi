//! Optional capsule movement against the runtime's authored box colliders.
//!
//! The capsule follows the world gravity axis. Translations are swept against
//! oriented colliders, with support normals returned to the caller.
use glam::Vec3;
use raf_core::scene::{Aabb, ColliderType, SceneGraph, SceneNodeId};

use crate::physics::shapes::BoxShape;
use crate::physics::spatial::SpatialIndex;

const EPSILON: f32 = 1.0e-4;
const MAX_CAST_STEPS: usize = 48;
const MAX_SLIDES: usize = 4;

#[derive(Clone, Copy, Debug)]
pub(crate) struct CharacterSettings {
    pub up: Vec3,
    pub radius: f32,
    /// Full capsule height including both rounded ends.
    pub height: f32,
    pub max_slope_degrees: f32,
    pub support_distance: f32,
    pub max_step_height: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CharacterMovement {
    pub translation: Vec3,
    pub grounded: bool,
    pub support_normal: Vec3,
    pub support: Option<SceneNodeId>,
    push: Option<(SceneNodeId, Vec3)>,
}

pub(crate) fn character_up(gravity: Vec3, inverted: bool) -> Vec3 {
    (if inverted { gravity } else { -gravity })
        .try_normalize()
        .unwrap_or(Vec3::Y)
}

/// Integrate one opt-in character within the existing fixed physics step.
pub(crate) fn step_character(
    scene: &mut SceneGraph,
    spatial: &SpatialIndex,
    owner: SceneNodeId,
    gravity: Vec3,
    dt: f32,
    advanced: bool,
    candidates: &mut Vec<SceneNodeId>,
) -> Result<(), String> {
    if !gravity.is_finite() || !dt.is_finite() || dt <= 0.0 {
        return Err("Invalid character timestep or gravity".into());
    }
    let node = scene.get(owner).ok_or("Character is missing")?;
    let gravity_scale = if node.rigid_body.use_gravity {
        if advanced {
            node.rigid_body.gravity_scale
        } else {
            1.0
        }
    } else {
        0.0
    };
    if !gravity_scale.is_finite() {
        return Err("Invalid character gravity scale".into());
    }
    let mut controller = node
        .character_controller
        .clone()
        .ok_or("Character controller is missing")?;
    controller.validate().map_err(str::to_owned)?;
    let up = character_up(gravity, gravity_scale < 0.0);
    let center = scene.world_matrix(owner).transform_point3(Vec3::ZERO);
    if !center.is_finite() {
        return Err("Invalid character transform".into());
    }
    let state = &mut controller.state;
    if state.up != Vec3::ZERO && state.up.dot(up) < 0.999 {
        state.grounded = false;
        state.vertical_velocity *= state.up.dot(up);
    }
    state.up = up;
    let support_delta = if state.grounded {
        state
            .support_uuid
            .and_then(|uuid| {
                let (support_id, _) = scene
                    .iter()
                    .find(|(id, node)| scene.is_valid_node(*id) && node.uuid == uuid)?;
                let position = scene.world_matrix(support_id).transform_point3(Vec3::ZERO);
                state.support_position.map(|previous| position - previous)
            })
            .unwrap_or(Vec3::ZERO)
    } else {
        Vec3::ZERO
    };
    if !support_delta.is_finite() {
        return Err("Invalid platform movement".into());
    }
    if state.jump_requested && state.grounded {
        state.vertical_velocity =
            (2.0 * gravity.length() * gravity_scale.abs() * controller.jump_height).sqrt();
        state.grounded = false;
        state.support_uuid = None;
        state.support_position = None;
    }
    state.jump_requested = false;
    state.vertical_velocity += gravity.dot(up) * gravity_scale * dt;
    let desired_velocity = state.desired_velocity - up * state.desired_velocity.dot(up);
    let desired = (desired_velocity + up * state.vertical_velocity) * dt + support_delta;
    let movement = move_character(
        scene,
        spatial,
        owner,
        center,
        desired,
        CharacterSettings {
            up,
            radius: controller.radius,
            height: controller.height,
            max_slope_degrees: controller.max_slope_degrees,
            support_distance: controller.support_distance,
            max_step_height: controller.max_step_height,
        },
        candidates,
    )
    .map_err(str::to_owned)?;
    let support = movement
        .support
        .and_then(|id| scene.get(id).map(|node| (id, node.uuid)));
    let support_position =
        support.map(|(id, _)| scene.world_matrix(id).transform_point3(Vec3::ZERO));
    let parent = scene.get(owner).and_then(|node| node.parent);
    let local_delta = if let Some(parent) = parent {
        let matrix = scene.world_matrix(parent);
        if matrix.determinant().abs() < 1e-8 {
            return Err("Character parent transform is singular".into());
        }
        matrix.inverse().transform_vector3(movement.translation)
    } else {
        movement.translation
    };
    if !local_delta.is_finite() {
        return Err("Character displacement is non-finite".into());
    }
    let state = &mut controller.state;
    state.grounded = movement.grounded;
    state.ground_normal = movement.support_normal;
    state.support_uuid = support.map(|(_, uuid)| uuid);
    state.support_position = support_position;
    if movement.grounded && state.vertical_velocity < 0.0 {
        state.vertical_velocity = 0.0;
    }
    if desired.dot(up) > 0.0 && movement.translation.dot(up) < desired.dot(up) - EPSILON {
        state.vertical_velocity = 0.0;
    }
    let node = scene.get_mut(owner).ok_or("Character disappeared")?;
    if !(node.position + local_delta).is_finite() {
        return Err("Character position overflow".into());
    }
    node.position += local_delta;
    node.rigid_body.velocity = movement.translation / dt;
    let character_mass = node.rigid_body.mass;
    node.character_controller = Some(controller);
    if let (true, Some((collider, normal))) = (advanced, movement.push) {
        let target = crate::physics_dynamics::collider_owner(scene, collider);
        if let Some(node) = scene.get_mut(target) {
            let body = &mut node.rigid_body;
            if body.enabled
                && body.body_type == raf_core::scene::RigidBodyType::Dynamic
                && !body.is_character
            {
                let approach = (desired_velocity - body.velocity).dot(-normal).max(0.0);
                let change = -normal * (approach * character_mass / (character_mass + body.mass));
                body.velocity += Vec3::from_array(std::array::from_fn(|i| {
                    if body.translation_locks[i] {
                        0.0
                    } else {
                        change[i]
                    }
                }));
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct Capsule {
    center: Vec3,
    up: Vec3,
    radius: f32,
    segment_half: f32,
}

impl Capsule {
    fn translated(self, offset: Vec3) -> Self {
        Self {
            center: self.center + offset,
            ..self
        }
    }

    fn bounds(self) -> Aabb {
        let extent = self.up.abs() * self.segment_half + Vec3::splat(self.radius);
        Aabb {
            min: self.center - extent,
            max: self.center + extent,
        }
    }
}

pub(crate) fn capsule_bounds(center: Vec3, up: Vec3, radius: f32, height: f32) -> BoxShape {
    let extent = up.abs() * (height * 0.5 - radius) + Vec3::splat(radius);
    BoxShape {
        center,
        edges: [Vec3::X * extent.x, Vec3::Y * extent.y, Vec3::Z * extent.z],
    }
}

#[derive(Clone, Copy)]
struct Hit {
    time: f32,
    normal: Vec3,
    entity: SceneNodeId,
}

/// Move and slide with a bounded downward support probe.
/// The caller owns gravity, jump state and applying the returned translation.
pub(crate) fn move_character(
    scene: &SceneGraph,
    spatial: &SpatialIndex,
    owner: SceneNodeId,
    center: Vec3,
    desired: Vec3,
    settings: CharacterSettings,
    candidates: &mut Vec<SceneNodeId>,
) -> Result<CharacterMovement, &'static str> {
    if !center.is_finite()
        || !desired.is_finite()
        || !settings.up.is_finite()
        || (settings.up.length_squared() - 1.0).abs() > 1e-3
        || !settings.radius.is_finite()
        || settings.radius <= 0.0
        || !settings.height.is_finite()
        || settings.height < settings.radius * 2.0
        || !settings.max_slope_degrees.is_finite()
        || !(0.0..90.0).contains(&settings.max_slope_degrees)
        || !settings.support_distance.is_finite()
        || settings.support_distance < 0.0
        || !settings.max_step_height.is_finite()
        || settings.max_step_height < 0.0
    {
        return Err("Invalid capsule controller settings");
    }
    let owner_node = scene.get(owner).ok_or("Character is missing")?;
    let capsule = Capsule {
        center,
        up: settings.up,
        radius: settings.radius,
        segment_half: settings.height * 0.5 - settings.radius,
    };
    let mut position = capsule;
    let mut translation = Vec3::ZERO;
    let mut remaining = desired;
    let mut push = None;
    for _ in 0..MAX_SLIDES {
        spatial.query(position.bounds(), candidates);
        let mut deepest: Option<(f32, Vec3)> = None;
        for &id in candidates.iter() {
            if id == owner {
                continue;
            }
            let Some(node) = scene.get(id) else {
                continue;
            };
            if node.collider.collider_type != ColliderType::Aabb
                || (node.rigid_body.enabled && node.rigid_body.is_trigger)
                || !owner_node.collider.interacts_with(&node.collider)
            {
                continue;
            }
            let shape = spatial.shape(id).ok_or("Invalid character obstacle")?;
            let (distance, normal) = capsule_box_distance(position, shape)?;
            if distance < -EPSILON && deepest.is_none_or(|old| distance < old.0) {
                deepest = Some((distance, normal));
            }
        }
        let Some((distance, normal)) = deepest else {
            break;
        };
        let correction = normal * (-distance + EPSILON);
        position = position.translated(correction);
        translation += correction;
    }
    spatial.query(position.bounds(), candidates);
    for &id in candidates.iter() {
        if id == owner {
            continue;
        }
        let Some(node) = scene.get(id) else {
            continue;
        };
        if node.collider.collider_type != ColliderType::Aabb
            || (node.rigid_body.enabled && node.rigid_body.is_trigger)
            || !owner_node.collider.interacts_with(&node.collider)
        {
            continue;
        }
        let shape = spatial.shape(id).ok_or("Invalid character obstacle")?;
        if capsule_box_distance(position, shape)?.0 < -EPSILON {
            return Err("Character penetration could not be resolved");
        }
    }
    let min_up = settings.max_slope_degrees.to_radians().cos();
    let was_grounded = if settings.support_distance > 0.0 {
        cast(
            scene,
            spatial,
            owner,
            owner_node,
            position,
            -settings.up * settings.support_distance,
            candidates,
        )?
        .is_some_and(|hit| hit.normal.dot(settings.up) >= min_up)
    } else {
        false
    };
    for _ in 0..MAX_SLIDES {
        if remaining.length_squared() <= EPSILON * EPSILON {
            break;
        }
        let Some(hit) = cast(
            scene, spatial, owner, owner_node, position, remaining, candidates,
        )?
        else {
            translation += remaining;
            position = position.translated(remaining);
            break;
        };
        if was_grounded
            && desired.dot(settings.up) <= 0.0
            && hit.normal.dot(settings.up) >= -EPSILON
            && hit.normal.dot(settings.up) < min_up
            && settings.max_step_height > 0.0
        {
            let horizontal = remaining - settings.up * remaining.dot(settings.up);
            let up = settings.up * settings.max_step_height;
            if horizontal.length_squared() > EPSILON * EPSILON
                && cast(scene, spatial, owner, owner_node, position, up, candidates)?.is_none()
            {
                let raised = position.translated(up);
                if cast(
                    scene, spatial, owner, owner_node, raised, horizontal, candidates,
                )?
                .is_none()
                {
                    let down =
                        -settings.up * (settings.max_step_height + settings.support_distance);
                    if let Some(landing) = cast(
                        scene,
                        spatial,
                        owner,
                        owner_node,
                        raised.translated(horizontal),
                        down,
                        candidates,
                    )? {
                        if landing.normal.dot(settings.up) >= min_up {
                            let step = up
                                + horizontal
                                + down * (landing.time - EPSILON / down.length()).max(0.0);
                            translation += step;
                            position = position.translated(step);
                            break;
                        }
                    }
                }
            }
        }
        if push.is_none()
            && hit.normal.dot(settings.up).abs() < 0.5
            && scene
                .get(crate::physics_dynamics::collider_owner(scene, hit.entity))
                .is_some_and(|node| {
                    node.rigid_body.enabled
                        && node.rigid_body.body_type == raf_core::scene::RigidBodyType::Dynamic
                        && !node.rigid_body.is_character
                })
        {
            push = Some((hit.entity, hit.normal));
        }
        let distance = remaining.length();
        let safe_time = (hit.time - EPSILON / distance).max(0.0);
        let traveled = remaining * safe_time;
        translation += traveled;
        position = position.translated(traveled);
        remaining *= 1.0 - safe_time;
        let into = remaining.dot(hit.normal).min(0.0);
        remaining -= hit.normal * into;
        // Steep surfaces must not accelerate the character upwards by sliding.
        let normal_up = hit.normal.dot(settings.up);
        if normal_up > 0.0 && normal_up < min_up && remaining.dot(settings.up) > 0.0 {
            remaining -= settings.up * remaining.dot(settings.up);
        }
    }
    let support = if desired.dot(settings.up) <= 0.0 && settings.support_distance > 0.0 {
        cast(
            scene,
            spatial,
            owner,
            owner_node,
            position,
            -settings.up * settings.support_distance,
            candidates,
        )?
        .filter(|hit| hit.normal.dot(settings.up) >= settings.max_slope_degrees.to_radians().cos())
    } else {
        None
    };
    Ok(CharacterMovement {
        translation,
        grounded: support.is_some(),
        support_normal: support.map_or(Vec3::ZERO, |hit| hit.normal),
        support: support.map(|hit| hit.entity),
        push,
    })
}

fn cast(
    scene: &SceneGraph,
    spatial: &SpatialIndex,
    owner: SceneNodeId,
    owner_node: &raf_core::scene::SceneNode,
    capsule: Capsule,
    delta: Vec3,
    candidates: &mut Vec<SceneNodeId>,
) -> Result<Option<Hit>, &'static str> {
    let start = capsule.bounds();
    let end = capsule.translated(delta).bounds();
    spatial.query(
        Aabb {
            min: start.min.min(end.min) - Vec3::splat(EPSILON),
            max: start.max.max(end.max) + Vec3::splat(EPSILON),
        },
        candidates,
    );
    let mut nearest: Option<Hit> = None;
    for &id in candidates.iter() {
        if id == owner {
            continue;
        }
        let Some(node) = scene.get(id) else {
            continue;
        };
        if node.collider.collider_type != ColliderType::Aabb
            || (node.rigid_body.enabled && node.rigid_body.is_trigger)
            || !owner_node.collider.interacts_with(&node.collider)
        {
            continue;
        }
        let shape = spatial.shape(id).ok_or("Invalid character obstacle")?;
        if let Some((time, normal)) = cast_box(capsule, delta, shape)? {
            if nearest.is_none_or(|old| time < old.time) {
                nearest = Some(Hit {
                    time,
                    normal,
                    entity: id,
                });
            }
        }
    }
    Ok(nearest)
}

/// Exact closest segment/box distance for orthogonal box axes. The scalar
/// distance function is piecewise quadratic, so each interval has one minimum.
fn capsule_box_distance(capsule: Capsule, shape: BoxShape) -> Result<(f32, Vec3), &'static str> {
    let extents = Vec3::new(
        shape.edges[0].length(),
        shape.edges[1].length(),
        shape.edges[2].length(),
    );
    if extents.min_element() <= 1e-8 {
        return Err("Degenerate character obstacle");
    }
    let basis = [
        shape.edges[0] / extents.x,
        shape.edges[1] / extents.y,
        shape.edges[2] / extents.z,
    ];
    if basis[0].dot(basis[1]).abs() > 1e-4
        || basis[0].dot(basis[2]).abs() > 1e-4
        || basis[1].dot(basis[2]).abs() > 1e-4
    {
        return Err("Sheared character obstacle is unsupported");
    }
    let start_world = capsule.center - capsule.up * capsule.segment_half - shape.center;
    let segment_world = capsule.up * (capsule.segment_half * 2.0);
    let start = Vec3::new(
        start_world.dot(basis[0]),
        start_world.dot(basis[1]),
        start_world.dot(basis[2]),
    );
    let segment = Vec3::new(
        segment_world.dot(basis[0]),
        segment_world.dot(basis[1]),
        segment_world.dot(basis[2]),
    );
    let mut breaks = [0.0_f32; 8];
    let mut count = 2;
    breaks[1] = 1.0;
    for axis in 0..3 {
        if segment[axis].abs() <= 1e-8 {
            continue;
        }
        for side in [-extents[axis], extents[axis]] {
            let t = (side - start[axis]) / segment[axis];
            if t > 0.0 && t < 1.0 {
                breaks[count] = t;
                count += 1;
            }
        }
    }
    breaks[..count].sort_unstable_by(f32::total_cmp);
    let mut best_t = 0.0;
    let mut best = f32::INFINITY;
    for pair in breaks[..count].windows(2) {
        let midpoint = (pair[0] + pair[1]) * 0.5;
        let mut numerator = 0.0;
        let mut denominator = 0.0;
        for axis in 0..3 {
            let value = start[axis] + segment[axis] * midpoint;
            let boundary = if value < -extents[axis] {
                -extents[axis]
            } else if value > extents[axis] {
                extents[axis]
            } else {
                continue;
            };
            numerator += segment[axis] * (start[axis] - boundary);
            denominator += segment[axis] * segment[axis];
        }
        let t = if denominator > 0.0 {
            (-numerator / denominator).clamp(pair[0], pair[1])
        } else {
            pair[0]
        };
        let point = start + segment * t;
        let closest = point.clamp(-extents, extents);
        let distance = (point - closest).length_squared();
        if distance < best {
            best = distance;
            best_t = t;
        }
    }
    let point = start + segment * best_t;
    let closest = point.clamp(-extents, extents);
    let local_delta = point - closest;
    let distance = local_delta.length();
    if distance <= 1e-8 {
        // The segment is inside the box. Return a separating normal and a
        // negative distance, allowing overlap queries to report the contact.
        let clearance = extents - point.abs();
        let axis = if clearance.x <= clearance.y && clearance.x <= clearance.z {
            0
        } else if clearance.y <= clearance.z {
            1
        } else {
            2
        };
        let sign = if point[axis] < 0.0 { -1.0 } else { 1.0 };
        return Ok((-capsule.radius - clearance[axis], basis[axis] * sign));
    }
    let local_normal = local_delta / distance;
    let normal = basis[0] * local_normal.x + basis[1] * local_normal.y + basis[2] * local_normal.z;
    Ok((distance - capsule.radius, normal))
}

/// Test a capsule against an oriented box using its real curved boundary.
pub(crate) fn capsule_overlaps_box(
    center: Vec3,
    up: Vec3,
    radius: f32,
    height: f32,
    shape: BoxShape,
) -> Result<bool, &'static str> {
    if !center.is_finite()
        || !up.is_finite()
        || (up.length_squared() - 1.0).abs() > 1e-3
        || !radius.is_finite()
        || !height.is_finite()
        || radius <= 0.0
        || height < 2.0 * radius
    {
        return Err("Invalid capsule");
    }
    let capsule = Capsule {
        center,
        up,
        radius,
        segment_half: height * 0.5 - radius,
    };
    Ok(capsule_box_distance(capsule, shape)?.0 <= 0.0)
}

/// Return the impact fraction and normal along a capsule translation.
pub(crate) fn capsule_box_separation(
    center: Vec3,
    up: Vec3,
    radius: f32,
    height: f32,
    shape: BoxShape,
) -> Result<(f32, Vec3), &'static str> {
    capsule_box_distance(
        Capsule {
            center,
            up,
            radius,
            segment_half: height * 0.5 - radius,
        },
        shape,
    )
}

pub(crate) fn sweep_capsule_box(
    center: Vec3,
    up: Vec3,
    radius: f32,
    height: f32,
    delta: Vec3,
    shape: BoxShape,
) -> Result<Option<(f32, Vec3)>, &'static str> {
    if !center.is_finite()
        || !up.is_finite()
        || (up.length_squared() - 1.0).abs() > 1e-3
        || !delta.is_finite()
        || !radius.is_finite()
        || !height.is_finite()
        || radius <= 0.0
        || height < 2.0 * radius
    {
        return Err("Invalid capsule");
    }
    cast_box(
        Capsule {
            center,
            up,
            radius,
            segment_half: height * 0.5 - radius,
        },
        delta,
        shape,
    )
}

fn cast_box(
    capsule: Capsule,
    delta: Vec3,
    shape: BoxShape,
) -> Result<Option<(f32, Vec3)>, &'static str> {
    let mut time = 0.0_f32;
    for _ in 0..MAX_CAST_STEPS {
        let (distance, normal) = capsule_box_distance(capsule.translated(delta * time), shape)?;
        if distance <= EPSILON {
            return if delta.dot(normal) < -1e-7 {
                Ok(Some((time, normal)))
            } else {
                Ok(None)
            };
        }
        let closing = -delta.dot(normal);
        if closing <= 1e-8 {
            return Ok(None);
        }
        time += distance / closing;
        if time > 1.0 {
            return Ok(None);
        }
    }
    Err("Capsule cast did not converge")
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_core::scene::{Primitive, RigidBodyType};

    fn make_box(scene: &mut SceneGraph, position: Vec3, scale: Vec3) -> SceneNodeId {
        let id = scene.add_root_with_primitive("box", Primitive::Cube);
        let node = scene.get_mut(id).unwrap();
        node.position = position;
        node.scale = scale;
        node.collider.collider_type = ColliderType::Aabb;
        node.collider.aabb = Aabb {
            min: Vec3::splat(-0.5),
            max: Vec3::splat(0.5),
        };
        id
    }

    fn settings() -> CharacterSettings {
        CharacterSettings {
            up: Vec3::Y,
            radius: 0.5,
            height: 2.0,
            max_slope_degrees: 45.0,
            support_distance: 0.1,
            max_step_height: 0.3,
        }
    }

    #[test]
    fn capsule_ground_support_ends_after_leaving_the_platform() {
        let mut scene = SceneGraph::new();
        let floor = make_box(&mut scene, Vec3::ZERO, Vec3::new(4.0, 1.0, 4.0));
        let owner = make_box(&mut scene, Vec3::new(0.0, 1.5002, 0.0), Vec3::ONE);
        scene.get_mut(owner).unwrap().rigid_body.body_type = RigidBodyType::Dynamic;
        let mut spatial = SpatialIndex::default();
        spatial.rebuild(&scene, &[floor, owner]);
        let mut candidates = Vec::new();
        let standing = move_character(
            &scene,
            &spatial,
            owner,
            Vec3::new(0.0, 1.5002, 0.0),
            Vec3::ZERO,
            settings(),
            &mut candidates,
        )
        .unwrap();
        assert!(standing.grounded);
        assert_eq!(standing.support, Some(floor));
        assert!(standing.support_normal.y > 0.99);
        let air = move_character(
            &scene,
            &spatial,
            owner,
            Vec3::new(3.0, 1.5002, 0.0),
            Vec3::new(0.0, -0.1, 0.0),
            settings(),
            &mut candidates,
        )
        .unwrap();
        assert!(!air.grounded);
        assert!(air.translation.y < -0.09);
    }

    #[test]
    fn capsule_rounded_corner_avoids_box_broadphase_false_hit() {
        let shape = BoxShape {
            center: Vec3::ZERO,
            edges: [Vec3::X * 0.5, Vec3::Y * 0.5, Vec3::Z * 0.5],
        };
        let capsule = Capsule {
            center: Vec3::new(0.95, 0.95, 0.0),
            up: Vec3::Y,
            radius: 0.5,
            segment_half: 0.0,
        };
        let (gap, normal) = capsule_box_distance(capsule, shape).unwrap();
        assert!(gap > 0.13 && gap < 0.14);
        assert!(normal.x > 0.7 && normal.y > 0.7);
        assert!(cast_box(capsule, Vec3::X, shape).unwrap().is_none());
    }

    #[test]
    fn capsule_cast_finds_thin_floor_at_high_speed() {
        let shape = BoxShape {
            center: Vec3::ZERO,
            edges: [Vec3::X * 5.0, Vec3::Y * 0.05, Vec3::Z * 5.0],
        };
        let capsule = Capsule {
            center: Vec3::Y * 5.0,
            up: Vec3::Y,
            radius: 0.5,
            segment_half: 0.5,
        };
        let (time, normal) = cast_box(capsule, Vec3::Y * -10.0, shape).unwrap().unwrap();
        assert!((time - 0.395).abs() < 0.001);
        assert!(normal.y > 0.99);
    }

    #[test]
    fn capsule_climbs_small_step_and_stays_on_its_top() {
        let mut scene = SceneGraph::new();
        let floor = make_box(
            &mut scene,
            Vec3::new(0.0, -0.5, 0.0),
            Vec3::new(10.0, 1.0, 10.0),
        );
        let step = make_box(
            &mut scene,
            Vec3::new(1.0, 0.1, 0.0),
            Vec3::new(1.0, 0.2, 2.0),
        );
        let owner = make_box(&mut scene, Vec3::new(0.0, 1.0002, 0.0), Vec3::ONE);
        let mut spatial = SpatialIndex::default();
        spatial.rebuild(&scene, &[floor, step, owner]);
        let mut candidates = Vec::new();
        let result = move_character(
            &scene,
            &spatial,
            owner,
            Vec3::new(0.0, 1.0002, 0.0),
            Vec3::new(1.0, -0.05, 0.0),
            settings(),
            &mut candidates,
        )
        .unwrap();
        assert!(result.translation.x > 0.9);
        assert!(result.translation.y > 0.15);
        assert!(result.grounded);
        assert_eq!(result.support, Some(step));
    }

    #[test]
    fn capsule_overlap_respects_rounded_corners() {
        let shape = BoxShape {
            center: Vec3::ZERO,
            edges: [Vec3::X * 0.5, Vec3::Y * 0.5, Vec3::Z * 0.5],
        };
        assert!(
            !capsule_overlaps_box(Vec3::new(0.95, 0.95, 0.0), Vec3::Y, 0.5, 1.0, shape).unwrap()
        );
        assert!(capsule_overlaps_box(Vec3::new(0.7, 0.7, 0.0), Vec3::Y, 0.5, 1.0, shape).unwrap());
    }

    #[test]
    fn character_support_works_with_sideways_gravity() {
        let mut scene = SceneGraph::new();
        let wall = make_box(&mut scene, Vec3::ZERO, Vec3::new(1.0, 10.0, 10.0));
        let owner = make_box(&mut scene, Vec3::new(1.5002, 0.0, 0.0), Vec3::ONE);
        scene.get_mut(owner).unwrap().character_controller = Some(Default::default());
        let mut spatial = SpatialIndex::default();
        spatial.rebuild(&scene, &[wall, owner]);
        let mut candidates = Vec::new();
        step_character(
            &mut scene,
            &spatial,
            owner,
            Vec3::new(-9.81, 0.0, 0.0),
            1.0 / 60.0,
            false,
            &mut candidates,
        )
        .unwrap();
        let state = &scene
            .get(owner)
            .unwrap()
            .character_controller
            .as_ref()
            .unwrap()
            .state;
        assert!(state.grounded);
        assert!(state.ground_normal.x > 0.99);
        assert_eq!(state.support_uuid, Some(scene.get(wall).unwrap().uuid));
    }

    #[test]
    fn character_follows_translating_support_and_jumps() {
        let mut scene = SceneGraph::new();
        let floor = make_box(&mut scene, Vec3::ZERO, Vec3::new(10.0, 1.0, 10.0));
        let owner = make_box(&mut scene, Vec3::new(0.0, 1.5002, 0.0), Vec3::ONE);
        scene.get_mut(owner).unwrap().character_controller = Some(Default::default());
        let mut spatial = SpatialIndex::default();
        let mut candidates = Vec::new();
        spatial.rebuild(&scene, &[floor, owner]);
        step_character(
            &mut scene,
            &spatial,
            owner,
            Vec3::new(0.0, -9.81, 0.0),
            1.0 / 60.0,
            false,
            &mut candidates,
        )
        .unwrap();
        assert!(
            scene
                .get(owner)
                .unwrap()
                .character_controller
                .as_ref()
                .unwrap()
                .state
                .grounded
        );
        scene.get_mut(floor).unwrap().position.x = 1.0;
        spatial.rebuild(&scene, &[floor, owner]);
        step_character(
            &mut scene,
            &spatial,
            owner,
            Vec3::new(0.0, -9.81, 0.0),
            1.0 / 60.0,
            false,
            &mut candidates,
        )
        .unwrap();
        assert!(scene.get(owner).unwrap().position.x > 0.99);
        scene
            .get_mut(owner)
            .unwrap()
            .character_controller
            .as_mut()
            .unwrap()
            .state
            .jump_requested = true;
        spatial.rebuild(&scene, &[floor, owner]);
        let before = scene.get(owner).unwrap().position.y;
        step_character(
            &mut scene,
            &spatial,
            owner,
            Vec3::new(0.0, -9.81, 0.0),
            1.0 / 60.0,
            false,
            &mut candidates,
        )
        .unwrap();
        assert!(scene.get(owner).unwrap().position.y > before);
        assert!(
            !scene
                .get(owner)
                .unwrap()
                .character_controller
                .as_ref()
                .unwrap()
                .state
                .grounded
        );
    }

    #[test]
    fn character_capsule_crosses_sensor_and_emits_enter_and_exit() {
        use crate::physics::PhysicsWorld;
        use raf_script::physics_api::PhysicsEventKind;
        let mut scene = SceneGraph::new();
        let sensor = make_box(&mut scene, Vec3::ZERO, Vec3::ONE);
        let body = &mut scene.get_mut(sensor).unwrap().rigid_body;
        body.enabled = true;
        body.is_trigger = true;
        let owner = make_box(&mut scene, Vec3::new(-4.0, 0.0, 0.0), Vec3::ONE);
        let node = scene.get_mut(owner).unwrap();
        node.collider.aabb = Aabb {
            min: Vec3::splat(-0.2),
            max: Vec3::splat(0.2),
        };
        node.rigid_body.enabled = true;
        node.rigid_body.is_character = true;
        node.rigid_body.body_type = RigidBodyType::Dynamic;
        let mut controller = raf_core::scene::CharacterController {
            radius: 0.2,
            height: 0.4,
            ..Default::default()
        };
        controller.state.desired_velocity = Vec3::X * 100.0;
        node.character_controller = Some(controller);
        let mut physics = PhysicsWorld::prepare(&scene).unwrap();
        physics
            .step_with_settings(&mut scene, 0.1, Vec3::ZERO, true)
            .unwrap();
        assert!(scene.get(owner).unwrap().position.x > 5.0);
        let events = physics.events();
        assert_eq!(
            events
                .iter()
                .filter(|e| e.kind == PhysicsEventKind::TriggerEnter)
                .count(),
            2
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| e.kind == PhysicsEventKind::TriggerExit)
                .count(),
            2
        );
    }
}
