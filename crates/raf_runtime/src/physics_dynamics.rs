//! Rigid-body integration and contact impulses for the advanced mode.
use crate::physics::{
    shapes::{self, BoxShape},
    spatial::SpatialIndex,
};
use glam::{EulerRot, Mat3, Quat, Vec3};
use raf_core::scene::{Aabb, ColliderType, RigidBodyType, SceneGraph, SceneNodeId};

const EPSILON: f32 = 1e-7;
const MAX_CONTACT_PASSES: usize = 4;
const SLEEP_SPEED: f32 = 0.025;
const SLEEP_DELAY: f32 = 0.8;
const MAX_RECORDED_IMPACTS: usize = 1024;

#[derive(Clone, Copy, Debug)]
pub struct PhysicsImpact {
    pub body: uuid::Uuid,
    pub other: uuid::Uuid,
    pub point: Vec3,
    pub normal: Vec3,
    pub normal_impulse: f32,
}

#[derive(Clone, Copy)]
struct BodyState {
    inverse_mass: f32,
    inverse_inertia: Mat3,
    center: Vec3,
    linear: Vec3,
    angular: Vec3,
    translation_locks: [bool; 3],
    rotation_locks: [bool; 3],
    friction: f32,
    restitution: f32,
}

fn unlocked(mut value: Vec3, locks: [bool; 3]) -> Vec3 {
    for axis in 0..3 {
        if locks[axis] {
            value[axis] = 0.0;
        }
    }
    value
}

fn state(scene: &SceneGraph, id: SceneNodeId) -> Result<BodyState, String> {
    let node = scene.get(id).ok_or("Physics body disappeared")?;
    let body = &node.rigid_body;
    let dynamic = body.enabled
        && body.body_type == RigidBodyType::Dynamic
        && !body.is_character
        && node.character_controller.is_none();
    let matrix = scene.world_matrix(id);
    let (scale, rotation, origin) = matrix.to_scale_rotation_translation();
    if !scale.is_finite()
        || !rotation.is_finite()
        || !origin.is_finite()
        || !body.mass.is_finite()
        || body.mass <= 0.0
    {
        return Err("Invalid rigid body transform or mass".into());
    }
    let (center, tensor) = mass_properties(scene, id, body.mass, origin);
    let inverse_mass = if dynamic { 1.0 / body.mass } else { 0.0 };
    let inverse_inertia = if dynamic { tensor } else { Mat3::ZERO };
    Ok(BodyState {
        inverse_mass,
        inverse_inertia,
        center,
        linear: if body.enabled && body.body_type != RigidBodyType::Static {
            body.velocity
        } else {
            Vec3::ZERO
        },
        angular: if body.enabled && body.body_type != RigidBodyType::Static {
            body.angular_velocity
        } else {
            Vec3::ZERO
        },
        translation_locks: body.translation_locks,
        rotation_locks: body.rotation_locks,
        friction: body.friction.max(0.0),
        restitution: body.restitution.clamp(0.0, 1.0),
    })
}

fn outer(a: Vec3) -> Mat3 {
    Mat3::from_cols(a * a.x, a * a.y, a * a.z)
}

fn volume(shape: BoxShape) -> f32 {
    (shape.edges[0].dot(shape.edges[1].cross(shape.edges[2])) * 8.0).abs()
}

pub(crate) fn mass_properties(
    scene: &SceneGraph,
    id: SceneNodeId,
    mass: f32,
    origin: Vec3,
) -> (Vec3, Mat3) {
    let (weighted, total) =
        body_shapes(scene, id).fold((Vec3::ZERO, 0.0), |(sum, total), (_, shape)| {
            let weight = volume(shape);
            (sum + shape.center * weight, total + weight)
        });
    if !total.is_finite() || total <= 0.0 {
        return (origin, Mat3::ZERO);
    }
    let center = weighted / total;
    let inertia = body_shapes(scene, id).fold(Mat3::ZERO, |sum, (_, shape)| {
        let part_mass = mass * (volume(shape) / total);
        let offset = shape.center - center;
        let mut tensor = Mat3::from_diagonal(Vec3::splat(offset.length_squared())) - outer(offset);
        for edge in shape.edges {
            tensor += (Mat3::from_diagonal(Vec3::splat(edge.length_squared())) - outer(edge))
                * (1.0 / 3.0);
        }
        sum + tensor * part_mass
    });
    let inverse = if inertia.determinant() > 0.0 {
        inertia.inverse()
    } else {
        Mat3::ZERO
    };
    (
        center,
        if inverse.is_finite() {
            inverse
        } else {
            Mat3::ZERO
        },
    )
}

fn linear_inverse(body: &BodyState, impulse: Vec3) -> Vec3 {
    unlocked(impulse * body.inverse_mass, body.translation_locks)
}
fn angular_inverse(body: &BodyState, impulse: Vec3) -> Vec3 {
    unlocked(
        body.inverse_inertia * unlocked(impulse, body.rotation_locks),
        body.rotation_locks,
    )
}
fn velocity_at(body: &BodyState, arm: Vec3) -> Vec3 {
    body.linear + body.angular.cross(arm)
}
fn denominator(a: &BodyState, b: &BodyState, ra: Vec3, rb: Vec3, axis: Vec3) -> f32 {
    axis.dot(linear_inverse(a, axis) + linear_inverse(b, axis))
        + axis.dot(
            angular_inverse(a, ra.cross(axis)).cross(ra)
                + angular_inverse(b, rb.cross(axis)).cross(rb),
        )
}

fn apply_impulse(
    scene: &mut SceneGraph,
    id: SceneNodeId,
    body: &BodyState,
    arm: Vec3,
    impulse: Vec3,
) {
    if body.inverse_mass == 0.0 {
        return;
    }
    let node = scene.get_mut(id).unwrap();
    node.rigid_body.velocity += linear_inverse(body, impulse);
    node.rigid_body.angular_velocity += angular_inverse(body, arm.cross(impulse));
    if node.rigid_body.sleeping && impulse.length_squared() > 1e-8 {
        node.rigid_body.sleeping = false;
        node.rigid_body.sleep_timer = 0.0;
    }
}

fn contact_point(a: BoxShape, b: BoxShape, normal: Vec3) -> Vec3 {
    // Clamp the moving center onto the obstacle bounds, then align to the contact plane.
    let bounds = b.bounds();
    let projected = a.center.clamp(bounds.min, bounds.max);
    let ra: f32 = a.edges.iter().map(|e| e.dot(normal).abs()).sum();
    let rb: f32 = b.edges.iter().map(|e| e.dot(normal).abs()).sum();
    let plane = ((a.center.dot(normal) - ra) + (b.center.dot(normal) + rb)) * 0.5;
    projected + normal * (plane - projected.dot(normal))
}

fn collide(
    scene: &mut SceneGraph,
    id: SceneNodeId,
    other: SceneNodeId,
    a_shape: BoxShape,
    b_shape: BoxShape,
    normal: Vec3,
    impacts: &mut Vec<PhysicsImpact>,
) -> Result<(), String> {
    let a = state(scene, id)?;
    let b = state(scene, other)?;
    let point = contact_point(a_shape, b_shape, normal);
    let ra = point - a.center;
    let rb = point - b.center;
    let relative = velocity_at(&a, ra) - velocity_at(&b, rb);
    let incoming = relative.dot(normal);
    if incoming >= 0.0 {
        return Ok(());
    }
    let divisor = denominator(&a, &b, ra, rb, normal);
    if divisor <= EPSILON {
        return Ok(());
    }
    let restitution = if incoming < -0.5 {
        a.restitution.max(b.restitution)
    } else {
        0.0
    };
    let magnitude = -(1.0 + restitution) * incoming / divisor;
    if impacts.len() < MAX_RECORDED_IMPACTS {
        impacts.push(PhysicsImpact {
            body: scene.get(id).unwrap().uuid,
            other: scene.get(other).unwrap().uuid,
            point,
            normal,
            normal_impulse: magnitude,
        });
    }
    let normal_impulse = normal * magnitude;
    apply_impulse(scene, id, &a, ra, normal_impulse);
    apply_impulse(scene, other, &b, rb, -normal_impulse);
    let a = state(scene, id)?;
    let b = state(scene, other)?;
    let relative = velocity_at(&a, ra) - velocity_at(&b, rb);
    let tangent = relative - normal * relative.dot(normal);
    if let Some(direction) = tangent.try_normalize() {
        let divisor = denominator(&a, &b, ra, rb, direction);
        if divisor > EPSILON {
            let limit = magnitude * (a.friction * b.friction).sqrt();
            let friction = (-relative.dot(direction) / divisor).clamp(-limit, limit);
            apply_impulse(scene, id, &a, ra, direction * friction);
            apply_impulse(scene, other, &b, rb, -direction * friction);
        }
    }
    Ok(())
}

fn translate(scene: &mut SceneGraph, id: SceneNodeId, delta: Vec3) -> Result<(), String> {
    let parent = scene.get(id).ok_or("Physics body disappeared")?.parent;
    let local = if let Some(parent) = parent {
        let matrix = scene.world_matrix(parent);
        if matrix.determinant().abs() < 1e-8 {
            return Err("physics parent transform is singular".into());
        }
        matrix.inverse().transform_vector3(delta)
    } else {
        delta
    };
    if !local.is_finite() {
        return Err("Invalid physics translation".into());
    }
    scene.get_mut(id).unwrap().position += local;
    Ok(())
}

fn rotate(scene: &mut SceneGraph, id: SceneNodeId, angular: Vec3, dt: f32) -> Result<(), String> {
    if angular.length_squared() < 1e-14 {
        return Ok(());
    }
    let center_before = state(scene, id)?.center;
    let node = scene.get(id).ok_or("Physics body disappeared")?;
    let local = Quat::from_euler(
        EulerRot::YXZ,
        node.rotation.y.to_radians(),
        node.rotation.x.to_radians(),
        node.rotation.z.to_radians(),
    );
    let parent = node.parent;
    let parent_rotation = parent
        .map(|p| {
            scene
                .world_matrix(p)
                .to_scale_rotation_translation()
                .1
                .normalize()
        })
        .unwrap_or(Quat::IDENTITY);
    let world = parent_rotation * local;
    let next = (Quat::from_scaled_axis(angular * dt) * world).normalize();
    let local = parent_rotation.inverse() * next;
    let (yaw, pitch, roll) = local.to_euler(EulerRot::YXZ);
    let value = Vec3::new(pitch.to_degrees(), yaw.to_degrees(), roll.to_degrees());
    if !value.is_finite() {
        return Err("Invalid physics orientation".into());
    }
    scene.get_mut(id).unwrap().rotation = value;
    let center_after = state(scene, id)?.center;
    translate(scene, id, center_before - center_after)?;
    Ok(())
}

pub(crate) fn collider_owner(scene: &SceneGraph, id: SceneNodeId) -> SceneNodeId {
    scene
        .get(id)
        .and_then(|node| {
            let owner = node.collider.body_owner?;
            let parent = node.parent?;
            scene
                .get(parent)
                .filter(|body| body.uuid == owner && body.rigid_body.enabled)
                .map(|_| parent)
        })
        .unwrap_or(id)
}

fn body_shapes(
    scene: &SceneGraph,
    id: SceneNodeId,
) -> impl Iterator<Item = (SceneNodeId, BoxShape)> + '_ {
    let node = scene.get(id);
    let children = node
        .into_iter()
        .flat_map(|node| node.children.iter().copied());
    std::iter::once(id)
        .chain(children.filter(move |&child| collider_owner(scene, child) == id))
        .filter_map(move |id| {
            let node = scene.get(id)?;
            (node.collider.collider_type == ColliderType::Aabb)
                .then_some((id, BoxShape::from_scene(scene, id)?))
        })
}

fn eligible(scene: &SceneGraph, id: SceneNodeId, shape: SceneNodeId, other: SceneNodeId) -> bool {
    collider_owner(scene, other) != id
        && scene
            .get(shape)
            .zip(scene.get(other))
            .is_some_and(|(a, b)| {
                !(b.rigid_body.enabled && b.rigid_body.is_trigger)
                    && a.collider.interacts_with(&b.collider)
            })
}

fn capsule_of(
    scene: &SceneGraph,
    id: SceneNodeId,
) -> Option<&raf_core::scene::CharacterController> {
    let node = scene.get(id)?;
    (node.rigid_body.enabled && node.rigid_body.is_character)
        .then_some(node.character_controller.as_ref()?)
}

fn penetration_against(
    scene: &SceneGraph,
    id: SceneNodeId,
    body: BoxShape,
    other: BoxShape,
) -> Result<Option<Vec3>, String> {
    let Some(capsule) = capsule_of(scene, id) else {
        return Ok(shapes::penetration(body, other));
    };
    let up = capsule.state.up.try_normalize().unwrap_or(Vec3::Y);
    let (distance, normal) = crate::physics_character::capsule_box_separation(
        other.center,
        up,
        capsule.radius,
        capsule.height,
        body,
    )?;
    Ok((distance < -1e-5).then_some(normal * distance))
}

fn sweep_against(
    scene: &SceneGraph,
    id: SceneNodeId,
    body: BoxShape,
    other: BoxShape,
    delta: Vec3,
) -> Result<Option<(f32, Vec3)>, String> {
    let Some(capsule) = capsule_of(scene, id) else {
        return Ok(shapes::sweep(body, other, delta));
    };
    let up = capsule.state.up.try_normalize().unwrap_or(Vec3::Y);
    Ok(crate::physics_character::sweep_capsule_box(
        other.center,
        up,
        capsule.radius,
        capsule.height,
        -delta,
        body,
    )?
    .map(|(time, normal)| (time, -normal)))
}

pub(crate) fn step_body(
    scene: &mut SceneGraph,
    spatial: &mut SpatialIndex,
    id: SceneNodeId,
    gravity: Vec3,
    dt: f32,
    candidates: &mut Vec<SceneNodeId>,
    impacts: &mut Vec<PhysicsImpact>,
) -> Result<(), String> {
    let initial = state(scene, id)?;
    let body = &scene.get(id).unwrap().rigid_body;
    if !body.angular_velocity.is_finite()
        || !body.force.is_finite()
        || !body.torque.is_finite()
        || !body.impulse.is_finite()
        || !body.angular_impulse.is_finite()
        || !body.gravity_scale.is_finite()
        || !body.angular_damping.is_finite()
        || body.angular_damping < 0.0
        || !body.friction.is_finite()
        || body.friction < 0.0
        || !body.restitution.is_finite()
        || !(0.0..=1.0).contains(&body.restitution)
    {
        return Err("Invalid rigid-body dynamics settings".into());
    }
    let impulse = body.impulse + body.force * dt;
    let angular_impulse = body.angular_impulse + body.torque * dt;
    let acceleration = if body.use_gravity {
        gravity * body.gravity_scale
    } else {
        Vec3::ZERO
    };
    let up = (-acceleration).try_normalize().unwrap_or(Vec3::Y);
    if body.sleeping
        && body.sleep_gravity == acceleration
        && body.velocity == Vec3::ZERO
        && body.angular_velocity == Vec3::ZERO
        && impulse == Vec3::ZERO
        && angular_impulse == Vec3::ZERO
    {
        for (shape_id, shape) in body_shapes(scene, id) {
            let bounds = shape.bounds();
            let margin = Vec3::splat(0.001);
            spatial.query(
                Aabb {
                    min: bounds.min - margin,
                    max: bounds.max + margin,
                },
                candidates,
            );
            let supported = candidates
                .iter()
                .copied()
                .filter(|&other| eligible(scene, id, shape_id, other))
                .filter_map(|other| spatial.shape(other))
                .any(|obstacle| {
                    shapes::contact_normal(shape, obstacle, 0.001)
                        .is_some_and(|normal| normal.dot(up) > 0.5)
                });
            if supported {
                return Ok(());
            }
        }
    }
    let mut linear = unlocked(
        initial.linear + acceleration * dt + linear_inverse(&initial, impulse),
        initial.translation_locks,
    );
    let mut angular = unlocked(
        initial.angular + angular_inverse(&initial, angular_impulse),
        initial.rotation_locks,
    );
    linear *= (1.0 - body.damping * dt).clamp(0.0, 1.0);
    angular *= (1.0 - body.angular_damping * dt).clamp(0.0, 1.0);
    if !linear.is_finite() || !angular.is_finite() {
        return Err("Rigid body integration exceeded finite velocity range".into());
    }
    {
        let body = &mut scene.get_mut(id).unwrap().rigid_body;
        body.velocity = linear;
        body.angular_velocity = angular;
        body.force = Vec3::ZERO;
        body.torque = Vec3::ZERO;
        body.impulse = Vec3::ZERO;
        body.angular_impulse = Vec3::ZERO;
    }
    rotate(scene, id, angular, dt)?;
    if scene.get(id).unwrap().rigid_body.is_trigger || body_shapes(scene, id).next().is_none() {
        return translate(scene, id, linear * dt);
    }
    let mut grounded = false;
    for _ in 0..MAX_CONTACT_PASSES {
        let mut penetration: Option<(Vec3, BoxShape, SceneNodeId, BoxShape)> = None;
        for (shape_id, shape) in body_shapes(scene, id) {
            spatial.query(shape.bounds(), candidates);
            for &other in candidates.iter() {
                if !eligible(scene, id, shape_id, other) {
                    continue;
                }
                let Some(obstacle) = spatial.shape(other) else {
                    continue;
                };
                let Some(correction) = penetration_against(scene, other, shape, obstacle)? else {
                    continue;
                };
                if penetration
                    .is_none_or(|hit| hit.0.length_squared() < correction.length_squared())
                {
                    penetration = Some((correction, shape, other, obstacle));
                }
            }
        }
        let Some((correction, shape, other, obstacle)) = penetration else {
            break;
        };
        let normal = correction.normalize_or_zero();
        let correction = unlocked(correction, initial.translation_locks);
        if correction.length_squared() <= EPSILON * EPSILON {
            break;
        }
        translate(scene, id, correction)?;
        collide(
            scene,
            id,
            collider_owner(scene, other),
            shape.translated(correction),
            obstacle,
            normal,
            impacts,
        )?;
        grounded |= normal.dot(-acceleration.normalize_or_zero()) > 0.5;
    }
    let mut time_left = dt;
    let mut remaining = scene.get(id).unwrap().rigid_body.velocity * time_left;
    for _ in 0..MAX_CONTACT_PASSES {
        if remaining.length_squared() < 1e-14 {
            break;
        }
        let mut earliest: Option<(f32, Vec3, SceneNodeId, BoxShape, BoxShape)> = None;
        for (shape_id, shape) in body_shapes(scene, id) {
            let start = shape.bounds();
            let end = shape.translated(remaining).bounds();
            spatial.query(
                Aabb {
                    min: start.min.min(end.min),
                    max: start.max.max(end.max),
                },
                candidates,
            );
            for &other in candidates.iter() {
                if !eligible(scene, id, shape_id, other) {
                    continue;
                }
                let Some(obstacle) = spatial.shape(other) else {
                    continue;
                };
                if let Some((time, normal)) =
                    sweep_against(scene, other, shape, obstacle, remaining)?
                {
                    if earliest.is_none_or(|hit| time < hit.0) {
                        earliest = Some((time, normal, other, shape, obstacle));
                    }
                }
            }
        }
        let Some((time, normal, other, shape, obstacle)) = earliest else {
            translate(scene, id, remaining)?;
            break;
        };
        let motion = remaining * time;
        translate(scene, id, motion)?;
        collide(
            scene,
            id,
            collider_owner(scene, other),
            shape.translated(motion),
            obstacle,
            normal,
            impacts,
        )?;
        grounded |= normal.dot(-acceleration.normalize_or_zero()) > 0.5;
        time_left *= 1.0 - time;
        remaining = scene.get(id).unwrap().rigid_body.velocity * time_left;
        if remaining.dot(normal) < 0.0 {
            remaining -= normal * remaining.dot(normal);
        }
        remaining = unlocked(remaining, initial.translation_locks);
    }
    let body = &mut scene.get_mut(id).unwrap().rigid_body;
    if grounded
        && body.velocity.length_squared() < SLEEP_SPEED * SLEEP_SPEED
        && body.angular_velocity.length_squared() < SLEEP_SPEED * SLEEP_SPEED
    {
        body.sleep_timer += dt;
        if body.sleep_timer >= SLEEP_DELAY {
            body.sleeping = true;
            body.sleep_gravity = acceleration;
            body.velocity = Vec3::ZERO;
            body.angular_velocity = Vec3::ZERO;
        }
    } else {
        body.sleep_timer = 0.0;
        body.sleeping = false;
    }
    Ok(())
}
