//! Shared authoring geometry. Transforms are applied exactly once.
use super::{Primitive, SceneGraph, SceneNodeId};
use glam::Vec3;
use std::collections::HashSet;

pub const CREATABLE_PRIMITIVES: [Primitive; 5] = [
    Primitive::Cube,
    Primitive::Sphere,
    Primitive::Cylinder,
    Primitive::Plane,
    Primitive::Capsule,
];

#[derive(Debug, Clone, Copy)]
pub struct ShapeDescriptor {
    pub slug: &'static str,
    pub label_key: &'static str,
    pub source: &'static str,
    pub half_extents: Vec3,
}

impl Primitive {
    pub const fn descriptor(self) -> ShapeDescriptor {
        let (slug, label_key, source, half_extents) = match self {
            Self::Empty => ("empty", "app.primitive_empty", "", Vec3::ZERO),
            Self::Cube => (
                "cube",
                "app.primitive_cube",
                "builtin://primitive/cube",
                Vec3::splat(0.5),
            ),
            Self::Sphere => (
                "sphere",
                "app.primitive_sphere",
                "builtin://primitive/sphere",
                Vec3::splat(0.5),
            ),
            Self::Cylinder => (
                "cylinder",
                "app.primitive_cylinder",
                "builtin://primitive/cylinder",
                Vec3::splat(0.5),
            ),
            Self::Plane => (
                "plane",
                "app.primitive_plane",
                "builtin://primitive/plane",
                Vec3::new(0.5, 0.0, 0.5),
            ),
            Self::Capsule => (
                "capsule",
                "app.primitive_capsule",
                "builtin://primitive/capsule",
                Vec3::new(0.5, 1.0, 0.5),
            ),
        };
        ShapeDescriptor {
            slug,
            label_key,
            source,
            half_extents,
        }
    }

    pub fn from_slug(slug: &str) -> Option<Self> {
        let normalized = slug.trim().to_ascii_lowercase();
        let normalized = match normalized.as_str() {
            "box" | "block" => "cube",
            "ball" | "circle" | "uv_sphere" => "sphere",
            "floor" | "sprite" | "sprite2d" => "plane",
            "entity" => "empty",
            other => other,
        };
        if matches!(normalized, "empty" | "group" | "folder") {
            return Some(Self::Empty);
        }
        CREATABLE_PRIMITIVES
            .into_iter()
            .find(|shape| shape.descriptor().slug == normalized)
    }

    pub fn dimensions(self, scale: Vec3) -> Vec3 {
        self.descriptor().half_extents * 2.0 * scale.abs()
    }

    pub fn scale_for_dimensions(self, size: Vec3, previous: Vec3) -> Result<Vec3, &'static str> {
        if self == Self::Empty || !size.is_finite() || !previous.is_finite() || size.min_element() < 0.0 {
            return Err("commands.error.dimensions_invalid");
        }
        let base = self.descriptor().half_extents * 2.0;
        let mut scale = previous;
        for axis in 0..3 {
            if base[axis] == 0.0 {
                if size[axis] != 0.0 {
                    return Err("commands.error.dimensions_plane");
                }
            } else {
                if size[axis] < 0.001 {
                    return Err("commands.error.dimensions_minimum");
                }
                scale[axis] =
                    (size[axis] / base[axis]) * if previous[axis] < 0.0 { -1.0 } else { 1.0 };
            }
        }
        Ok(scale)
    }
}

pub fn node_geometry_bounds(scene: &SceneGraph, id: SceneNodeId) -> Option<(Vec3, Vec3)> {
    let node = scene.get(id)?;
    if node.is_folder || node.primitive == Primitive::Empty {
        return None;
    }
    let half = node.primitive.descriptor().half_extents;
    let world = scene.world_matrix(id);
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for x in [-half.x, half.x] {
        for y in [-half.y, half.y] {
            for z in [-half.z, half.z] {
                let point = world.transform_point3(Vec3::new(x, y, z));
                if !point.is_finite() {
                    return None;
                }
                min = min.min(point);
                max = max.max(point);
            }
        }
    }
    Some((min, max))
}

pub fn subtree_geometry_bounds(scene: &SceneGraph, root: SceneNodeId) -> Option<(Vec3, Vec3)> {
    let mut pending = vec![root];
    let mut visited = HashSet::new();
    let mut bounds: Option<(Vec3, Vec3)> = None;
    while let Some(id) = pending.pop() {
        if !visited.insert(id) {
            continue;
        }
        let Some(node) = scene.get(id) else {
            continue;
        };
        if let Some((min, max)) = node_geometry_bounds(scene, id) {
            bounds = Some(bounds.map_or((min, max), |(a, b)| (a.min(min), b.max(max))));
        }
        pending.extend(node.children.iter().copied());
    }
    bounds
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authoring_bounds_apply_parent_and_local_scale_once() {
        let mut scene = SceneGraph::new();
        let parent = scene.add_root("Group");
        scene.get_mut(parent).unwrap().scale = Vec3::splat(3.0);
        let child = scene.add_child_with_primitive(parent, "Part", Primitive::Cube);
        scene.get_mut(child).unwrap().scale = Vec3::splat(2.0);
        let (min, max) = node_geometry_bounds(&scene, child).unwrap();
        assert_eq!(max - min, Vec3::splat(6.0));
        assert_eq!(
            Primitive::Capsule.dimensions(Vec3::ONE),
            Vec3::new(1.0, 2.0, 1.0)
        );
        assert_eq!(
            Primitive::Capsule
                .scale_for_dimensions(Vec3::new(2.0, 6.0, 2.0), Vec3::ONE)
                .unwrap(),
            Vec3::new(2.0, 3.0, 2.0)
        );
    }
}
