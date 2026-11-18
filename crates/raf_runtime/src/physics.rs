//! Fixed-step physics with oriented box contacts and a retained spatial tree.
#[path = "physics_shapes.rs"]
pub(super) mod shapes;
#[path = "physics_spatial.rs"]
pub(super) mod spatial;

pub use crate::physics_dynamics::PhysicsImpact;
pub use crate::physics_joints::JointBreak;
use glam::Vec3;
use raf_core::scene::{Aabb, ColliderType, RigidBodyType, SceneGraph, SceneNodeId};
use shapes::BoxShape;
pub use spatial::SpatialMetrics;

#[derive(Default)]
pub struct PhysicsWorld {
    dynamic: Vec<SceneNodeId>,
    colliders: Vec<SceneNodeId>,
    active: Vec<SceneNodeId>,
    spatial: spatial::SpatialIndex,
    candidates: Vec<SceneNodeId>,
    contacts: super::physics_contacts::ContactHistory,
    joints: crate::physics_joints::JointSolver,
    impacts: Vec<PhysicsImpact>,
}
impl PhysicsWorld {
    pub fn prepare(scene: &SceneGraph) -> Result<Self, Vec<String>> {
        let mut world = Self::default();
        world.refresh(scene)?;
        Ok(world)
    }
    fn refresh(&mut self, scene: &SceneGraph) -> Result<(), Vec<String>> {
        self.dynamic.clear();
        self.colliders.clear();
        self.active.clear();
        let mut errors = Vec::new();
        for (id, node) in scene.iter().filter(|(id, _)| scene.is_valid_node(*id)) {
            if node.collider.collider_type != ColliderType::None {
                if node.collider.collider_type != ColliderType::Aabb {
                    errors.push(format!(
                        "{}: runtime physics currently supports AABB, not {}",
                        node.name,
                        node.collider.type_label()
                    ));
                }
                if !node.collider.aabb.min.is_finite()
                    || !node.collider.aabb.max.is_finite()
                    || node.collider.aabb.min.cmpgt(node.collider.aabb.max).any()
                    || !node.collider.offset.is_finite()
                {
                    errors.push(format!("{}: invalid collider bounds", node.name));
                }
                if node.collider.collider_type == ColliderType::Aabb
                    && world_bounds(scene, id).is_none()
                {
                    errors.push(format!(
                        "{}: non-finite collider world transform",
                        node.name
                    ));
                }
                self.colliders.push(id);
            }
            if node.rigid_body.enabled {
                if !node.rigid_body.velocity.is_finite()
                    || !node.rigid_body.damping.is_finite()
                    || node.rigid_body.damping < 0.0
                    || !node.rigid_body.mass.is_finite()
                    || node.rigid_body.mass <= 0.0
                {
                    errors.push(format!(
                        "{}: invalid rigid-body velocity, mass or damping",
                        node.name
                    ));
                }
                if node.rigid_body.body_type != RigidBodyType::Static {
                    self.dynamic.push(id);
                }
                if node.rigid_body.body_type != RigidBodyType::Static || node.rigid_body.is_trigger
                {
                    self.active.push(id);
                }
            }
            if errors.len() >= 32 {
                break;
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
    pub fn has_dynamic_bodies(&self) -> bool {
        !self.dynamic.is_empty()
    }
    pub fn step(&mut self, scene: &mut SceneGraph, dt: f32) -> Result<(), String> {
        self.step_with_gravity(scene, dt, Vec3::new(0.0, -9.81, 0.0))
    }
    pub fn step_with_gravity(
        &mut self,
        scene: &mut SceneGraph,
        dt: f32,
        gravity: Vec3,
    ) -> Result<(), String> {
        self.step_with_settings(scene, dt, gravity, true)
    }
    pub fn step_with_settings(
        &mut self,
        scene: &mut SceneGraph,
        dt: f32,
        gravity: Vec3,
        advanced: bool,
    ) -> Result<(), String> {
        self.contacts.events.clear();
        self.impacts.clear();
        if !dt.is_finite() || dt <= 0.0 || !gravity.is_finite() {
            return Err("Physics timestep must be finite and positive".into());
        }
        // Explicit script changes are picked up at the fixed boundary; reuse allocations.
        self.refresh(scene).map_err(|errors| errors.join("; "))?;
        if !advanced {
            self.dynamic.retain(|id| {
                scene.get(*id).is_some_and(|node| {
                    node.rigid_body.is_character
                        || node.rigid_body.body_type == RigidBodyType::Kinematic
                })
            });
            self.active.retain(|id| {
                scene.get(*id).is_some_and(|node| {
                    node.rigid_body.is_character
                        || node.rigid_body.is_trigger
                        || node.rigid_body.body_type == RigidBodyType::Kinematic
                })
            });
        }
        self.active
            .extend(self.colliders.iter().copied().filter(|&id| {
                let owner = crate::physics_dynamics::collider_owner(scene, id);
                owner != id
                    && self
                        .dynamic
                        .binary_search_by_key(&owner.0, |id| id.0)
                        .is_ok()
            }));
        // Platforms must publish their pose before characters read the support delta.
        self.dynamic.sort_unstable_by_key(|id| {
            let node = scene.get(*id).unwrap();
            let stage = if node.rigid_body.body_type == RigidBodyType::Kinematic {
                0
            } else if node.character_controller.is_some() {
                1
            } else {
                2
            };
            (stage, id.0)
        });
        self.spatial.rebuild(scene, &self.colliders);
        self.update_character_bounds(scene, gravity, advanced);
        self.contacts.begin_step(
            scene,
            &self.colliders,
            &self.active,
            &self.spatial,
            advanced,
        );
        for index in 0..self.dynamic.len() {
            let id = self.dynamic[index];
            if !scene.is_valid_node(id) {
                continue;
            }
            let node = scene.get(id).ok_or("physics body missing")?;
            if !node.rigid_body.enabled {
                continue;
            }
            if node.character_controller.is_some() {
                crate::physics_character::step_character(
                    scene,
                    &self.spatial,
                    id,
                    gravity,
                    dt,
                    advanced,
                    &mut self.candidates,
                )?;
                if scene.get(id).is_some_and(|node| !node.children.is_empty()) {
                    self.spatial.rebuild(scene, &self.colliders);
                }
                self.update_character_bounds(scene, gravity, advanced);
                continue;
            }
            if advanced
                && node.rigid_body.body_type == RigidBodyType::Dynamic
                && !node.rigid_body.is_character
            {
                crate::physics_dynamics::step_body(
                    scene,
                    &mut self.spatial,
                    id,
                    gravity,
                    dt,
                    &mut self.candidates,
                    &mut self.impacts,
                )?;
                if scene.get(id).is_some_and(|node| !node.children.is_empty()) {
                    self.spatial.rebuild(scene, &self.colliders);
                    self.update_character_bounds(scene, gravity, advanced);
                } else if let Some(shape) = BoxShape::from_scene(scene, id) {
                    self.spatial.update_shape(id, shape);
                }
                continue;
            }
            let mut velocity = node.rigid_body.velocity;
            let dynamic = node.rigid_body.body_type == RigidBodyType::Dynamic;
            if dynamic && node.rigid_body.use_gravity {
                velocity += gravity * dt;
            }
            if dynamic {
                velocity *= (1.0 - node.rigid_body.damping * dt).clamp(0.0, 1.0);
            }
            let mut delta = velocity * dt;
            if !delta.is_finite() {
                return Err("physics produced a non-finite displacement".into());
            }
            let trigger = node.rigid_body.is_trigger;
            let body_shape = if node.collider.collider_type == ColliderType::Aabb {
                BoxShape::from_scene(scene, id)
            } else {
                None
            };
            let mut correction = glam::Vec3::ZERO;
            // Kinematic velocity is prescribed: no gravity, damping or collision response.
            if dynamic && !trigger {
                if let Some(mut body) = body_shape {
                    for _ in 0..4 {
                        self.spatial.query(body.bounds(), &mut self.candidates);
                        let mut changed = false;
                        for &other in &self.candidates {
                            if skip_collision(scene, id, other) {
                                continue;
                            }
                            if let Some(shift) = BoxShape::from_scene(scene, other)
                                .and_then(|obstacle| shapes::penetration(body, obstacle))
                            {
                                let normal = shift.normalize_or_zero();
                                velocity -= normal * velocity.dot(normal).min(0.0);
                                body = body.translated(shift);
                                correction += shift;
                                changed = true;
                            }
                        }
                        if !changed {
                            break;
                        }
                    }
                    self.spatial.query(body.bounds(), &mut self.candidates);
                    for &other in &self.candidates {
                        if skip_collision(scene, id, other) {
                            continue;
                        }
                        if BoxShape::from_scene(scene, other)
                            .and_then(|obstacle| shapes::penetration(body, obstacle))
                            .is_some_and(|shift| shift.length_squared() > 1e-10)
                        {
                            return Err(
                                "Initial overlap cannot be resolved within four translation passes"
                                    .into(),
                            );
                        }
                    }
                    let mut remaining = velocity * dt;
                    delta = Vec3::ZERO;
                    for _ in 0..4 {
                        let start = body.bounds();
                        let end = body.translated(remaining).bounds();
                        self.spatial.query(
                            Aabb {
                                min: start.min.min(end.min),
                                max: start.max.max(end.max),
                            },
                            &mut self.candidates,
                        );
                        let mut hit: Option<(f32, Vec3)> = None;
                        for &other in &self.candidates {
                            if skip_collision(scene, id, other) {
                                continue;
                            }
                            let candidate = BoxShape::from_scene(scene, other)
                                .and_then(|obstacle| shapes::sweep(body, obstacle, remaining));
                            if let Some(candidate) = candidate {
                                if hit.is_none_or(|previous| candidate.0 < previous.0) {
                                    hit = Some(candidate);
                                }
                            }
                        }
                        let Some((time, normal)) = hit else {
                            delta += remaining;
                            break;
                        };
                        let traveled = remaining * time;
                        delta += traveled;
                        body = body.translated(traveled);
                        remaining *= 1.0 - time;
                        remaining -= normal * remaining.dot(normal).min(0.0);
                        velocity -= normal * velocity.dot(normal).min(0.0);
                        if remaining.length_squared() < 1e-12 {
                            break;
                        }
                    }
                }
            }
            delta += correction;
            let parent = scene.get(id).and_then(|node| node.parent);
            let local_delta = if let Some(parent) = parent {
                let matrix = scene.world_matrix(parent);
                if matrix.determinant().abs() < 1e-8 {
                    return Err("physics parent transform is singular".into());
                }
                matrix.inverse().transform_vector3(delta)
            } else {
                delta
            };
            if !local_delta.is_finite() {
                return Err("physics parent conversion is non-finite".into());
            }
            let node = scene.get_mut(id).ok_or("physics body disappeared")?;
            if !(node.position + local_delta).is_finite() {
                return Err("Physics position overflow".into());
            }
            node.position += local_delta;
            node.rigid_body.velocity = velocity;
            if scene.get(id).is_some_and(|node| !node.children.is_empty()) {
                // Parent movement affects every descendant collider, including statics.
                self.spatial.rebuild(scene, &self.colliders);
            } else if let Some(shape) = BoxShape::from_scene(scene, id) {
                self.spatial.update_shape(id, shape);
            }
        }
        if advanced {
            self.joints.solve(scene, dt)?;
        } else {
            self.joints.breaks.clear();
        }
        self.spatial.rebuild(scene, &self.colliders);
        self.update_character_bounds(scene, gravity, advanced);
        let result = self.contacts.update(scene, &self.active, &self.spatial);
        if result.is_err() {
            self.contacts.events.clear();
        }
        result
    }
    pub fn events(&self) -> &[raf_script::physics_api::PhysicsEvent] {
        &self.contacts.events
    }
    pub fn joint_breaks(&self) -> &[JointBreak] {
        &self.joints.breaks
    }

    pub fn impacts(&self) -> &[PhysicsImpact] {
        &self.impacts
    }
    pub fn spatial_metrics(&self) -> SpatialMetrics {
        self.spatial.metrics()
    }
    fn update_character_bounds(&mut self, scene: &SceneGraph, gravity: Vec3, advanced: bool) {
        for &id in &self.dynamic {
            let Some(node) = scene.get(id) else {
                continue;
            };
            let Some(controller) = &node.character_controller else {
                continue;
            };
            let up = crate::physics_character::character_up(
                gravity,
                advanced && node.rigid_body.use_gravity && node.rigid_body.gravity_scale < 0.0,
            );
            let center = scene.world_matrix(id).transform_point3(Vec3::ZERO);
            let bounds = crate::physics_character::capsule_bounds(
                center,
                up,
                controller.radius,
                controller.height,
            );
            self.spatial.update_shape(id, bounds);
        }
    }
}
fn skip_collision(scene: &SceneGraph, id: SceneNodeId, other: SceneNodeId) -> bool {
    id == other
        || is_trigger(scene, other)
        || !scene
            .get(id)
            .zip(scene.get(other))
            .is_some_and(|(a, b)| a.collider.interacts_with(&b.collider))
}
fn is_trigger(scene: &SceneGraph, id: SceneNodeId) -> bool {
    scene
        .get(id)
        .is_some_and(|node| node.rigid_body.enabled && node.rigid_body.is_trigger)
}
pub(super) fn world_bounds(scene: &SceneGraph, id: SceneNodeId) -> Option<Aabb> {
    raf_script::query_api::world_aabb(scene, id)
}
