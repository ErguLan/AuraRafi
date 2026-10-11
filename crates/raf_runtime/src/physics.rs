//! Initial fixed-step physics: world-space translation and swept AABB contacts.
//! No angular dynamics, hull/mesh narrow phase or implicit character controller.
use raf_core::scene::{Aabb, ColliderType, RigidBodyType, SceneGraph, SceneNodeId};

#[derive(Default)]
pub struct PhysicsWorld {
    dynamic: Vec<SceneNodeId>,
    colliders: Vec<SceneNodeId>,
    active: Vec<SceneNodeId>,
    obstacles: Vec<Aabb>,
    contacts: super::physics_contacts::ContactHistory,
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
        if self.dynamic.len() > 64
            || self.colliders.len() > 1024
            || self.active.len().saturating_mul(self.colliders.len()) > 4096
        {
            errors.push("Local physics budget exceeded (64 moving bodies, 1024 colliders, 4096 active/collider pairs)".into());
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
        self.contacts.events.clear();
        if !dt.is_finite() || dt <= 0.0 {
            return Err("Physics timestep must be finite and positive".into());
        }
        // Explicit script changes are picked up at the fixed boundary; reuse allocations.
        self.refresh(scene).map_err(|errors| errors.join("; "))?;
        for &id in &self.dynamic {
            if !scene.is_valid_node(id) {
                continue;
            }
            let node = scene.get(id).ok_or("physics body missing")?;
            if !node.rigid_body.enabled {
                continue;
            }
            let mut velocity = node.rigid_body.velocity;
            let dynamic = node.rigid_body.body_type == RigidBodyType::Dynamic;
            if dynamic && node.rigid_body.use_gravity {
                velocity.y -= 9.81 * dt;
            }
            if dynamic {
                velocity *= (1.0 - node.rigid_body.damping * dt).clamp(0.0, 1.0);
            }
            let mut delta = velocity * dt;
            if !delta.is_finite() {
                return Err("physics produced a non-finite displacement".into());
            }
            let trigger = node.rigid_body.is_trigger;
            let body_bounds = world_bounds(scene, id);
            let mut correction = glam::Vec3::ZERO;
            // Kinematic velocity is prescribed: no gravity, damping or collision response.
            if dynamic && !trigger {
                if let Some(mut body) = body_bounds {
                    self.obstacles.clear();
                    self.obstacles.extend(
                        self.colliders
                            .iter()
                            .copied()
                            .filter(|other| *other != id)
                            .filter(|other| {
                                scene.get(*other).is_some_and(|n| {
                                    !(n.rigid_body.enabled && n.rigid_body.is_trigger)
                                })
                            })
                            .filter_map(|other| world_bounds(scene, other)),
                    );
                    (body, correction) =
                        super::physics_response::depenetrate(body, &self.obstacles, &mut velocity)?;
                    delta = velocity * dt;
                    for axis in 0..3 {
                        let mut movement = delta[axis];
                        for bounds in &self.obstacles {
                            let perpendicular_overlap = (0..3).filter(|a| *a != axis).all(|a| {
                                body.min[a] < bounds.max[a] && body.max[a] > bounds.min[a]
                            });
                            if !perpendicular_overlap {
                                continue;
                            }
                            if movement > 0.0 && body.max[axis] <= bounds.min[axis] {
                                movement =
                                    movement.min((bounds.min[axis] - body.max[axis]).max(0.0));
                            } else if movement < 0.0 && body.min[axis] >= bounds.max[axis] {
                                movement =
                                    movement.max((bounds.max[axis] - body.min[axis]).min(0.0));
                            }
                        }
                        if movement != delta[axis] {
                            velocity[axis] = 0.0;
                        }
                        delta[axis] = movement;
                        body.min[axis] += movement;
                        body.max[axis] += movement;
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
        }
        let result = self.contacts.update(scene, &self.colliders, &self.active);
        if result.is_err() {
            self.contacts.events.clear();
        }
        result
    }
    pub fn events(&self) -> &[raf_script::physics_api::PhysicsEvent] {
        &self.contacts.events
    }
}
pub(super) fn world_bounds(scene: &SceneGraph, id: SceneNodeId) -> Option<Aabb> {
    let node = scene.get(id)?;
    if node.collider.collider_type != ColliderType::Aabb {
        return None;
    }
    let matrix = scene.world_matrix(id);
    let points = node
        .collider
        .aabb
        .corners()
        .map(|p| matrix.transform_point3(p + node.collider.offset));
    points
        .iter()
        .all(|p| p.is_finite())
        .then(|| Aabb::from_points(&points))
}
