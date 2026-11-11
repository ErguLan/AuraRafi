//! Bounded world-space AABB queries shared by all language adapters.
use crate::{NodeHandle, ScriptContext, ScriptError, ScriptResult};
use glam::Vec3;
use raf_core::scene::{Aabb, ColliderType, SceneGraph, SceneNodeId};

#[derive(Debug, Clone, Default)]
pub struct QueryFilter {
    pub ignore: Option<NodeHandle>,
    /// Reuse authored semantic tags instead of introducing a second category registry.
    pub tag: Option<String>,
    pub include_triggers: bool,
}
#[derive(Debug, Clone, Copy)]
pub struct RayHit {
    pub entity: NodeHandle,
    pub position: Vec3,
    /// Zero when the ray starts inside the box; no arbitrary surface is invented.
    pub normal: Vec3,
    pub distance: f32,
}

pub fn world_aabb(scene: &SceneGraph, id: SceneNodeId) -> Option<Aabb> {
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

fn invalid(message: &str) -> ScriptError {
    ScriptError::InvalidArgument(message.into())
}
impl QueryFilter {
    fn validate(&self, ctx: &ScriptContext<'_>) -> ScriptResult<Option<SceneNodeId>> {
        if self
            .tag
            .as_ref()
            .is_some_and(|s| s.is_empty() || s.len() > 128)
        {
            return Err(invalid("Invalid query tag"));
        }
        self.ignore.map(|h| h.resolve(ctx)).transpose()
    }
    fn accepts(&self, scene: &SceneGraph, id: SceneNodeId, ignore: Option<SceneNodeId>) -> bool {
        let Some(node) = scene.get(id) else {
            return false;
        };
        scene.is_valid_node(id)
            && Some(id) != ignore
            && node.collider.collider_type == ColliderType::Aabb
            && (self.include_triggers || !node.rigid_body.is_trigger)
            && self.tag.as_ref().is_none_or(|tag| node.tags.contains(tag))
    }
}
impl ScriptContext<'_> {
    pub fn raycast(
        &self,
        origin: Vec3,
        direction: Vec3,
        distance: f32,
        filter: &QueryFilter,
    ) -> ScriptResult<Option<RayHit>> {
        self.check_budget()?;
        let ignore = filter.validate(self)?;
        if !origin.is_finite()
            || !direction.is_finite()
            || direction.length_squared() < 1e-12
            || !distance.is_finite()
            || distance <= 0.0
            || distance > 100_000.0
        {
            return Err(invalid(
                "Raycast requires finite origin, nonzero direction and distance in (0,100000]",
            ));
        }
        let direction = direction
            .try_normalize()
            .ok_or_else(|| invalid("Invalid ray direction magnitude"))?;
        let mut closest = None;
        let mut limit = distance;
        for (id, _) in self.scene.iter() {
            self.check_budget()?;
            if !filter.accepts(self.scene, id, ignore) {
                continue;
            }
            let bounds =
                world_aabb(self.scene, id).ok_or_else(|| invalid("Invalid collider transform"))?;
            if let Some((t, normal)) = ray_box(origin, direction, limit, bounds) {
                limit = t;
                closest = Some(RayHit {
                    entity: NodeHandle::scoped(id, self),
                    position: origin + direction * t,
                    normal,
                    distance: t,
                });
            }
        }
        Ok(closest)
    }
    pub fn overlap_aabb(
        &self,
        min: Vec3,
        max: Vec3,
        filter: &QueryFilter,
    ) -> ScriptResult<Vec<NodeHandle>> {
        self.check_budget()?;
        let ignore = filter.validate(self)?;
        if !min.is_finite() || !max.is_finite() || min.cmpge(max).any() {
            return Err(invalid("Overlap requires finite min < max"));
        }
        let query = Aabb { min, max };
        let mut hits = Vec::new();
        for (id, _) in self.scene.iter() {
            self.check_budget()?;
            if !filter.accepts(self.scene, id, ignore) {
                continue;
            }
            let bounds =
                world_aabb(self.scene, id).ok_or_else(|| invalid("Invalid collider transform"))?;
            if query.intersects(&bounds) {
                if hits.len() == 256 {
                    return Err(invalid("Overlap exceeds 256 results; narrow the query"));
                }
                hits.push(NodeHandle::scoped(id, self));
            }
        }
        Ok(hits)
    }
}
pub fn ray_box(origin: Vec3, direction: Vec3, limit: f32, bounds: Aabb) -> Option<(f32, Vec3)> {
    let mut near = 0.0_f32;
    let mut far = limit;
    let mut normal = Vec3::ZERO;
    for axis in 0..3 {
        if direction[axis].abs() < 1e-8 {
            if origin[axis] < bounds.min[axis] || origin[axis] > bounds.max[axis] {
                return None;
            }
            continue;
        }
        let mut a = (bounds.min[axis] - origin[axis]) / direction[axis];
        let mut b = (bounds.max[axis] - origin[axis]) / direction[axis];
        let sign = -direction[axis].signum();
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        if a > near {
            near = a;
            normal = Vec3::ZERO;
            normal[axis] = sign;
        }
        far = far.min(b);
        if near > far {
            return None;
        }
    }
    Some((near, normal))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn runtime_services_slab_query_handles_parallel_inside_and_surface_normal() {
        let bounds = Aabb {
            min: -Vec3::ONE,
            max: Vec3::ONE,
        };
        assert_eq!(
            ray_box(Vec3::new(0.0, 0.0, 3.0), -Vec3::Z, 10.0, bounds),
            Some((2.0, Vec3::Z))
        );
        assert_eq!(
            ray_box(Vec3::ZERO, Vec3::X, 10.0, bounds),
            Some((0.0, Vec3::ZERO))
        );
        assert!(ray_box(Vec3::new(2.0, 0.0, 0.0), Vec3::Y, 10.0, bounds).is_none());
    }
}
