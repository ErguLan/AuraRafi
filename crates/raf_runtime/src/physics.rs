//! Initial fixed-step physics: world-space translation and swept AABB contacts.
//! No angular dynamics, hull/mesh narrow phase or implicit character controller.
use raf_core::scene::{Aabb, ColliderType, RigidBodyType, SceneGraph, SceneNodeId};

#[derive(Default)]
pub struct PhysicsWorld {
    dynamic: Vec<SceneNodeId>,
    colliders: Vec<SceneNodeId>,
}
impl PhysicsWorld {
    pub fn prepare(scene: &SceneGraph) -> Result<Self, Vec<String>> {
        let mut world = Self::default();
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
                world.colliders.push(id);
            }
            if node.rigid_body.enabled && node.rigid_body.body_type == RigidBodyType::Dynamic {
                if !node.rigid_body.velocity.is_finite()
                    || !node.rigid_body.damping.is_finite()
                    || node.rigid_body.damping < 0.0
                {
                    errors.push(format!(
                        "{}: invalid rigid-body velocity or damping",
                        node.name
                    ));
                }
                world.dynamic.push(id);
            }
            if errors.len() >= 32 {
                break;
            }
        }
        if world.dynamic.len() > 64
            || world.colliders.len() > 1024
            || world.dynamic.len().saturating_mul(world.colliders.len()) > 4096
        {
            errors.push("Initial local physics budget exceeded (64 dynamic bodies, 1024 colliders, 4096 body/collider pairs)".into());
        }
        if errors.is_empty() {
            Ok(world)
        } else {
            Err(errors)
        }
    }
    pub fn has_dynamic_bodies(&self) -> bool {
        !self.dynamic.is_empty()
    }
    pub fn step(&mut self, scene: &mut SceneGraph, dt: f32) -> Result<(), String> {
        // The initial body roster is explicit. Spawned primitives do not acquire physics.
        for &id in &self.dynamic {
            if !scene.is_valid_node(id) {
                continue;
            }
            let node = scene.get(id).ok_or("physics body missing")?;
            if !node.rigid_body.enabled {
                continue;
            }
            let mut velocity = node.rigid_body.velocity;
            if node.rigid_body.use_gravity {
                velocity.y -= 9.81 * dt;
            }
            velocity *= (1.0 - node.rigid_body.damping * dt).clamp(0.0, 1.0);
            let mut delta = velocity * dt;
            if !delta.is_finite() {
                return Err("physics produced a non-finite displacement".into());
            }
            let trigger = node.rigid_body.is_trigger;
            let body_bounds = world_bounds(scene, id);
            if !trigger {
                if let Some(mut body) = body_bounds {
                    for axis in 0..3 {
                        let mut movement = delta[axis];
                        for &other in &self.colliders {
                            if other == id || !scene.is_valid_node(other) {
                                continue;
                            }
                            let obstacle = scene.get(other).ok_or("collider missing")?;
                            if obstacle.rigid_body.is_trigger {
                                continue;
                            }
                            let Some(bounds) = world_bounds(scene, other) else {
                                continue;
                            };
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
            node.position += local_delta;
            node.rigid_body.velocity = velocity;
        }
        Ok(())
    }
}
fn world_bounds(scene: &SceneGraph, id: SceneNodeId) -> Option<Aabb> {
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
    Some(Aabb::from_points(&points))
}
