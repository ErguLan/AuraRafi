//! Joint constraints solved as impulses at explicit anchors.
use glam::{Mat3, Quat, Vec3};
use raf_core::scene::joint::{JointKind, PhysicsJoint};
use raf_core::scene::{RigidBodyType, SceneGraph, SceneNodeId};
use std::collections::HashMap;
use uuid::Uuid;

const ITERATIONS: usize = 8;
const BIAS: f32 = 0.2;
const MAX_CORRECTION_SPEED: f32 = 10.0;
const MIN_EFFECTIVE_MASS: f32 = 1e-8;
const MAX_JOINTS: usize = 16_384;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JointBreak {
    pub owner: Uuid,
    pub target: Uuid,
    pub index: usize,
    pub impulse: f32,
}

#[derive(Default)]
pub(crate) struct JointSolver {
    ids: HashMap<Uuid, SceneNodeId>,
    constraints: Vec<(SceneNodeId, SceneNodeId, usize)>,
    broken: HashMap<(Uuid, Uuid, usize), PhysicsJoint>,
    pub breaks: Vec<JointBreak>,
}

struct Endpoint {
    anchor: Vec3,
    arm: Vec3,
    orientation: Quat,
    linear: Vec3,
    angular: Vec3,
    inv_mass: f32,
    inv_inertia: Mat3,
    linear_mask: Vec3,
}

impl Endpoint {
    fn new(scene: &SceneGraph, id: SceneNodeId, local_anchor: Vec3) -> Self {
        let node = scene.get(id).unwrap();
        let matrix = scene.world_matrix(id);
        let (_, orientation, origin) = matrix.to_scale_rotation_translation();
        let orientation = orientation.normalize();
        let anchor = matrix.transform_point3(local_anchor);
        let body = &node.rigid_body;
        let dynamic =
            body.enabled && body.body_type == RigidBodyType::Dynamic && !body.is_character;
        let moving = body.enabled && body.body_type != RigidBodyType::Static;
        let (center, tensor) =
            crate::physics_dynamics::mass_properties(scene, id, body.mass, origin);
        let inv_mass = if dynamic { body.mass.recip() } else { 0.0 };
        let linear_mask = axis_mask(body.translation_locks);
        let angular_mask = Mat3::from_diagonal(axis_mask(body.rotation_locks));
        let inv_inertia = if dynamic {
            angular_mask * tensor * angular_mask
        } else {
            Mat3::ZERO
        };
        Self {
            anchor,
            arm: anchor - center,
            orientation,
            inv_mass,
            inv_inertia,
            linear_mask,
            linear: if moving {
                node.rigid_body.velocity
            } else {
                Vec3::ZERO
            },
            angular: if moving {
                node.rigid_body.angular_velocity
            } else {
                Vec3::ZERO
            },
        }
    }

    fn velocity(&self) -> Vec3 {
        self.linear + self.angular.cross(self.arm)
    }
}

impl JointSolver {
    pub fn solve(&mut self, scene: &mut SceneGraph, dt: f32) -> Result<(), String> {
        self.ids.clear();
        self.constraints.clear();
        self.breaks.clear();
        if !scene
            .iter()
            .any(|(id, node)| scene.is_valid_node(id) && !node.physics_joints.is_empty())
        {
            self.broken.clear();
            return Ok(());
        }
        self.ids.extend(
            scene
                .iter()
                .filter(|(id, _)| scene.is_valid_node(*id))
                .map(|(id, node)| (node.uuid, id)),
        );
        for (owner, node) in scene.iter().filter(|(id, _)| scene.is_valid_node(*id)) {
            for (index, joint) in node.physics_joints.iter().enumerate() {
                if !joint.is_valid() {
                    return Err(format!("Invalid physics joint on {}", node.uuid));
                }
                let Some(&target) = self.ids.get(&joint.target) else {
                    continue;
                };
                if target == owner {
                    return Err(format!("Physics joint on {} references itself", node.uuid));
                }
                self.constraints.push((owner, target, index));
                if self.constraints.len() > MAX_JOINTS {
                    return Err("Physics joint budget exceeded".into());
                }
            }
        }
        self.broken.retain(|&(owner, target, index), broken| {
            self.ids
                .get(&owner)
                .and_then(|&id| scene.get(id))
                .and_then(|node| node.physics_joints.get(index))
                .is_some_and(|joint| joint.target == target && joint == broken)
        });
        for _ in 0..ITERATIONS {
            for &(owner, target, index) in &self.constraints {
                let node = scene.get(owner).unwrap();
                let joint = &node.physics_joints[index];
                let key = (node.uuid, joint.target, index);
                if self.broken.contains_key(&key) {
                    continue;
                }
                let a = Endpoint::new(scene, owner, joint.local_anchor);
                let b = Endpoint::new(scene, target, joint.target_anchor);
                if a.inv_mass + b.inv_mass == 0.0 {
                    continue;
                }
                let (linear, angular) = solve_joint(joint, &a, &b, dt);
                if !linear.is_finite() || !angular.is_finite() {
                    return Err(format!("Non-finite physics joint impulse on {}", node.uuid));
                }
                let impulse = linear.length().max(angular.length());
                if joint.break_impulse.is_some_and(|limit| impulse > limit) {
                    self.broken.insert(key, joint.clone());
                    self.breaks.push(JointBreak {
                        owner: key.0,
                        target: key.1,
                        index,
                        impulse,
                    });
                    continue;
                }
                apply(scene, owner, &a, linear, angular);
                apply(scene, target, &b, -linear, -angular);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_core::scene::{Aabb, ColliderType, Primitive};

    #[test]
    fn physics_slider_motor_brakes_at_its_limit() {
        use raf_core::scene::joint::JointMotor;
        let mut joint = PhysicsJoint::new(Uuid::nil(), JointKind::Slider, Vec3::ZERO, Vec3::ZERO);
        joint.limits = Some([0.0, 1.0]);
        joint.motor = Some(JointMotor {
            velocity: 4.0,
            max_force: 100.0,
        });
        assert_eq!(
            constrained_speed(1.0, 2.0, &joint, 0.1, |speed| speed - 2.0),
            -2.0
        );
        assert_eq!(constrained_speed(1.0, 0.0, &joint, 0.1, |speed| speed), 0.0);
        assert!((constrained_speed(0.99, 0.0, &joint, 0.1, |speed| speed) - 0.1).abs() < 1e-5);
    }

    #[test]
    fn physics_ball_joint_stops_anchor_separation_and_reports_breaks() {
        let mut scene = SceneGraph::new();
        let a = scene.add_root_with_primitive("Fixed", Primitive::Cube);
        let b = scene.add_root_with_primitive("Dynamic", Primitive::Cube);
        let target = scene.get(a).unwrap().uuid;
        let body = scene.get_mut(b).unwrap();
        body.collider.collider_type = ColliderType::Aabb;
        body.collider.aabb = Aabb {
            min: Vec3::splat(-0.5),
            max: Vec3::splat(0.5),
        };
        body.rigid_body.enabled = true;
        body.rigid_body.body_type = RigidBodyType::Dynamic;
        body.rigid_body.velocity = Vec3::Y;
        body.physics_joints.push(PhysicsJoint::new(
            target,
            JointKind::Ball,
            Vec3::ZERO,
            Vec3::ZERO,
        ));
        let mut solver = JointSolver::default();
        solver.solve(&mut scene, 1.0 / 60.0).unwrap();
        assert!(scene.get(b).unwrap().rigid_body.velocity.length() < 1e-5);

        let body = scene.get_mut(b).unwrap();
        body.rigid_body.velocity = Vec3::Y;
        body.physics_joints[0].break_impulse = Some(0.5);
        solver.solve(&mut scene, 1.0 / 60.0).unwrap();
        assert_eq!(solver.breaks.len(), 1);
        assert_eq!(scene.get(b).unwrap().rigid_body.velocity, Vec3::Y);
        solver.solve(&mut scene, 1.0 / 60.0).unwrap();
        assert!(solver.breaks.is_empty());
    }
}

fn solve_joint(joint: &PhysicsJoint, a: &Endpoint, b: &Endpoint, dt: f32) -> (Vec3, Vec3) {
    let axis_a = a.orientation * joint.local_axis.normalize();
    let axis_b = b.orientation * joint.target_axis.normalize();
    let tangent_a = a.orientation * joint.local_axis.normalize().any_orthonormal_vector();
    let tangent_b = b.orientation * joint.target_axis.normalize().any_orthonormal_vector();
    let binormal = axis_a.cross(tangent_a);
    let error = b.anchor - a.anchor;
    let relative = a.velocity() - b.velocity();
    let relative_angular = a.angular - b.angular;
    let bias = |e: f32| (e * BIAS / dt).clamp(-MAX_CORRECTION_SPEED, MAX_CORRECTION_SPEED);
    let linear_impulse = |axis: Vec3, speed: f32| {
        let arm_a = a.arm.cross(axis);
        let arm_b = b.arm.cross(axis);
        let denom = axis.dot(axis * (a.linear_mask * a.inv_mass + b.linear_mask * b.inv_mass))
            + arm_a.dot(a.inv_inertia * arm_a)
            + arm_b.dot(b.inv_inertia * arm_b);
        if denom > MIN_EFFECTIVE_MASS {
            axis * ((speed - relative.dot(axis)) / denom)
        } else {
            Vec3::ZERO
        }
    };
    let angular_impulse = |axis: Vec3, speed: f32| {
        let denom = axis.dot((a.inv_inertia + b.inv_inertia) * axis);
        if denom > MIN_EFFECTIVE_MASS {
            axis * ((speed - relative_angular.dot(axis)) / denom)
        } else {
            Vec3::ZERO
        }
    };
    if joint.kind == JointKind::Spring {
        let Some(axis) = error.try_normalize() else {
            return (Vec3::ZERO, Vec3::ZERO);
        };
        let force = joint.stiffness * (error.length() - joint.rest_length)
            - joint.damping * relative.dot(axis);
        return (axis * (force * dt / ITERATIONS as f32), Vec3::ZERO);
    }
    let mut linear = linear_impulse(tangent_a, bias(error.dot(tangent_a)))
        + linear_impulse(binormal, bias(error.dot(binormal)));
    if joint.kind == JointKind::Slider {
        let along = -error.dot(axis_a);
        linear += axis_a
            * constrained_speed(along, relative.dot(axis_a), joint, dt, |speed| {
                linear_impulse(axis_a, speed).dot(axis_a)
            });
    } else {
        linear += linear_impulse(axis_a, bias(error.dot(axis_a)));
    }
    if joint.kind == JointKind::Ball {
        return (linear, Vec3::ZERO);
    }

    let alignment = axis_a.cross(axis_b);
    let mut angular = angular_impulse(tangent_a, bias(alignment.dot(tangent_a)))
        + angular_impulse(binormal, bias(alignment.dot(binormal)));
    let angle = axis_a
        .dot(tangent_b.cross(tangent_a))
        .atan2(tangent_b.dot(tangent_a));
    if joint.kind == JointKind::Hinge {
        angular += axis_a
            * constrained_speed(angle, relative_angular.dot(axis_a), joint, dt, |speed| {
                angular_impulse(axis_a, speed).dot(axis_a)
            });
    } else {
        angular += angular_impulse(axis_a, bias(-angle));
    }
    (linear, angular)
}

fn apply(
    scene: &mut SceneGraph,
    id: SceneNodeId,
    endpoint: &Endpoint,
    linear: Vec3,
    angular: Vec3,
) {
    if endpoint.inv_mass == 0.0 || (linear == Vec3::ZERO && angular == Vec3::ZERO) {
        return;
    }
    let body = &mut scene.get_mut(id).unwrap().rigid_body;
    let dv = endpoint.linear_mask * linear * endpoint.inv_mass;
    let dw = endpoint.inv_inertia * (angular + endpoint.arm.cross(linear));
    body.velocity += dv;
    body.angular_velocity += dw;
    if dv.length_squared().max(dw.length_squared()) > 1e-6 {
        body.sleeping = false;
        body.sleep_timer = 0.0;
    }
}

fn axis_mask(locked: [bool; 3]) -> Vec3 {
    Vec3::from_array(locked.map(|locked| if locked { 0.0 } else { 1.0 }))
}

fn constrained_speed(
    position: f32,
    velocity: f32,
    joint: &PhysicsJoint,
    dt: f32,
    impulse: impl Fn(f32) -> f32,
) -> f32 {
    let (min_speed, max_speed) =
        joint
            .limits
            .map_or((f32::NEG_INFINITY, f32::INFINITY), |[min, max]| {
                if position < min {
                    (
                        ((min - position) * BIAS / dt).min(MAX_CORRECTION_SPEED),
                        f32::INFINITY,
                    )
                } else if position > max {
                    (
                        f32::NEG_INFINITY,
                        ((max - position) * BIAS / dt).max(-MAX_CORRECTION_SPEED),
                    )
                } else {
                    ((min - position) / dt, (max - position) / dt)
                }
            });
    let safe = velocity.clamp(min_speed, max_speed);
    if safe != velocity {
        return impulse(safe);
    }
    let Some(motor) = joint.motor else {
        return 0.0;
    };
    let budget = motor.max_force * dt / ITERATIONS as f32;
    impulse(motor.velocity.clamp(min_speed, max_speed)).clamp(-budget, budget)
}
