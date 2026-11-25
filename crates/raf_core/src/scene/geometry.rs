//! Shared authoring geometry. Transforms are applied exactly once.
use super::{PivotMode, Primitive, SceneGraph, SceneNode, SceneNodeId};
use glam::{Mat4, Vec3};
use std::collections::HashSet;

pub const CREATABLE_PRIMITIVES: [Primitive; 9] = [
    Primitive::Cube,
    Primitive::Sphere,
    Primitive::Cylinder,
    Primitive::Plane,
    Primitive::Capsule,
    Primitive::Wedge,
    Primitive::Torus,
    Primitive::Prism,
    Primitive::Sphere2D,
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
            Self::Wedge => (
                "wedge",
                "app.primitive_wedge",
                "builtin://primitive/wedge",
                Vec3::splat(0.5),
            ),
            Self::Torus => (
                "torus",
                "app.primitive_torus",
                "builtin://primitive/torus",
                Vec3::new(0.5, 0.2, 0.5),
            ),
            Self::Prism => (
                "prism",
                "app.primitive_prism",
                "builtin://primitive/prism",
                Vec3::splat(0.5),
            ),
            Self::Sphere2D => (
                "sphere2d",
                "app.primitive_sphere2d",
                "builtin://primitive/sphere2d",
                Vec3::splat(0.5),
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
            "billboard_sphere" | "sphere_2d" | "circle2d" => "sphere2d",
            "ramp" => "wedge",
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
        if self == Self::Empty
            || !size.is_finite()
            || !previous.is_finite()
            || size.min_element() < 0.0
        {
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

/// Local point that becomes the node transform origin for the authored pivot.
pub fn node_pivot_offset(node: &SceneNode) -> Vec3 {
    let (min, max) = if let Some(mesh) = &node.editable_mesh {
        mesh.bounds()
            .unwrap_or((-Vec3::splat(0.5), Vec3::splat(0.5)))
    } else if let Some(mesh) = &node.mesh_asset {
        // Imported models keep their authored origin for `Center`. Their local
        // AABB is often far from zero, so using its center would silently move
        // every imported entity when it uses the default pivot. `Base` and
        // `Corner` still use the cached import bounds. This is the documented
        // limitation of the current `PivotMode` set; an explicit `Origin`
        // variant would remove the special case.
        if node.pivot == PivotMode::Center {
            return Vec3::ZERO;
        }
        let bounds = mesh.bounds.unwrap_or([-0.5, -0.5, -0.5, 0.5, 0.5, 0.5]);
        let scale = mesh.import_scale.max(f32::EPSILON);
        (
            Vec3::new(bounds[0], bounds[1], bounds[2]) * scale,
            Vec3::new(bounds[3], bounds[4], bounds[5]) * scale,
        )
    } else if node.primitive == Primitive::Empty {
        (-Vec3::splat(0.5), Vec3::splat(0.5))
    } else {
        let half = node.primitive.descriptor().half_extents;
        (-half, half)
    };
    node.pivot.offset(min, max)
}

/// Render-space geometry matrix. The authored node transform (and every gizmo,
/// script or camera anchored to it) keeps the node origin; only the drawn mesh
/// is shifted so the authored pivot point lands on that origin.
pub fn node_geometry_matrix(world: Mat4, node: &SceneNode) -> Mat4 {
    world * Mat4::from_translation(-node_pivot_offset(node))
}

fn transformed_aabb(min: Vec3, max: Vec3, matrix: Mat4) -> Option<(Vec3, Vec3)> {
    let mut out_min = Vec3::splat(f32::INFINITY);
    let mut out_max = Vec3::splat(f32::NEG_INFINITY);
    for x in [min.x, max.x] {
        for y in [min.y, max.y] {
            for z in [min.z, max.z] {
                let point = matrix.transform_point3(Vec3::new(x, y, z));
                if !point.is_finite() {
                    return None;
                }
                out_min = out_min.min(point);
                out_max = out_max.max(point);
            }
        }
    }
    Some((out_min, out_max))
}

pub fn node_geometry_bounds(scene: &SceneGraph, id: SceneNodeId) -> Option<(Vec3, Vec3)> {
    let node = scene.get(id)?;
    if node.is_folder {
        return None;
    }
    let world = node_geometry_matrix(scene.world_matrix(id), node);
    if let Some(mesh) = &node.editable_mesh {
        let (min, max) = mesh.bounds()?;
        return transformed_aabb(min, max, world);
    }
    if let Some(mesh) = &node.mesh_asset {
        let bounds = mesh.bounds.unwrap_or([-0.5, -0.5, -0.5, 0.5, 0.5, 0.5]);
        let scale = mesh.import_scale.max(f32::EPSILON);
        let min = Vec3::new(bounds[0], bounds[1], bounds[2]) * scale;
        let max = Vec3::new(bounds[3], bounds[4], bounds[5]) * scale;
        return transformed_aabb(min, max, world);
    }
    if node.primitive == Primitive::Empty {
        return None;
    }
    let half = node.primitive.descriptor().half_extents;
    transformed_aabb(-half, half, world)
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

    #[test]
    fn render_geometry_applies_base_and_corner_pivots_without_moving_the_origin() {
        let mut scene = SceneGraph::new();
        let cube = scene.add_root_with_primitive("Cube", Primitive::Cube);
        scene.get_mut(cube).unwrap().position = Vec3::new(10.0, 5.0, 0.0);
        scene.get_mut(cube).unwrap().pivot = PivotMode::Base;

        let (min, max) = node_geometry_bounds(&scene, cube).unwrap();
        assert!((min.y - 5.0).abs() < 1e-5);
        assert!((max.y - 6.0).abs() < 1e-5);
        assert!(
            (node_geometry_matrix(scene.world_matrix(cube), scene.get(cube).unwrap())
                .transform_point3(Vec3::ZERO)
                - Vec3::new(10.0, 5.5, 0.0))
            .length()
                < 1e-5
        );

        scene.get_mut(cube).unwrap().pivot = PivotMode::Corner;
        let (min, max) = node_geometry_bounds(&scene, cube).unwrap();
        assert!((min - Vec3::new(10.0, 5.0, 0.0)).length() < 1e-5);
        assert!((max - Vec3::new(11.0, 6.0, 1.0)).length() < 1e-5);
    }

    #[test]
    fn imported_center_pivot_preserves_the_authored_origin() {
        let mut scene = SceneGraph::new();
        let node = scene.add_root("Imported");
        let mut reference = crate::scene::MeshAssetRef::new("models/a.glb");
        reference.bounds = Some([1.0, 2.0, 3.0, 3.0, 6.0, 9.0]);
        let node_ref = scene.get_mut(node).unwrap();
        node_ref.mesh_asset = Some(reference);
        node_ref.pivot = PivotMode::Center;

        let (min, max) = node_geometry_bounds(&scene, node).unwrap();
        assert!((min - Vec3::new(1.0, 2.0, 3.0)).length() < 1e-5);
        assert!((max - Vec3::new(3.0, 6.0, 9.0)).length() < 1e-5);
        assert_eq!(node_pivot_offset(scene.get(node).unwrap()), Vec3::ZERO);

        scene.get_mut(node).unwrap().pivot = PivotMode::Base;
        let (min, max) = node_geometry_bounds(&scene, node).unwrap();
        assert!((min - Vec3::new(-1.0, 0.0, -3.0)).length() < 1e-5);
        assert!((max - Vec3::new(1.0, 4.0, 3.0)).length() < 1e-5);
    }
}
