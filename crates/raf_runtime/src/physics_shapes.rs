//! World-space parallelepipeds and continuous translation tests.
use glam::{Mat4, Vec3};
use raf_core::scene::{Aabb, SceneGraph, SceneNodeId};

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct BoxShape {
    pub center: Vec3,
    pub edges: [Vec3; 3],
}

impl BoxShape {
    pub fn from_scene(scene: &SceneGraph, id: SceneNodeId) -> Option<Self> {
        let node = scene.get(id)?;
        Self::from_collider(&node.collider, scene.world_matrix(id))
    }

    pub fn from_collider(collider: &raf_core::scene::Collider, matrix: Mat4) -> Option<Self> {
        let bounds = collider.aabb;
        let center = matrix.transform_point3((bounds.min + bounds.max) * 0.5 + collider.offset);
        let half = (bounds.max - bounds.min) * 0.5;
        let edges = [
            matrix.transform_vector3(Vec3::X * half.x),
            matrix.transform_vector3(Vec3::Y * half.y),
            matrix.transform_vector3(Vec3::Z * half.z),
        ];
        if !center.is_finite() || edges.iter().any(|e| !e.is_finite()) {
            return None;
        }
        Some(Self { center, edges })
    }

    pub fn bounds(self) -> Aabb {
        let extent = self.edges[0].abs() + self.edges[1].abs() + self.edges[2].abs();
        Aabb {
            min: self.center - extent,
            max: self.center + extent,
        }
    }

    pub fn translated(self, delta: Vec3) -> Self {
        Self {
            center: self.center + delta,
            ..self
        }
    }
}

fn axes(a: BoxShape, b: BoxShape) -> impl Iterator<Item = Vec3> {
    let mut axes = [Vec3::ZERO; 15];
    axes[0] = a.edges[1].cross(a.edges[2]);
    axes[1] = a.edges[2].cross(a.edges[0]);
    axes[2] = a.edges[0].cross(a.edges[1]);
    axes[3] = b.edges[1].cross(b.edges[2]);
    axes[4] = b.edges[2].cross(b.edges[0]);
    axes[5] = b.edges[0].cross(b.edges[1]);
    for i in 0..3 {
        for j in 0..3 {
            axes[6 + i * 3 + j] = a.edges[i].cross(b.edges[j]);
        }
    }
    axes.into_iter().filter_map(|axis| axis.try_normalize())
}

fn radius(shape: BoxShape, axis: Vec3) -> f32 {
    shape.edges.iter().map(|edge| edge.dot(axis).abs()).sum()
}

/// Return the minimum translation needed to separate two intersecting boxes.
pub(crate) fn penetration(a: BoxShape, b: BoxShape) -> Option<Vec3> {
    let offset = a.center - b.center;
    let mut best = f32::INFINITY;
    let mut normal = Vec3::ZERO;
    for axis in axes(a, b) {
        let distance = offset.dot(axis);
        let overlap = radius(a, axis) + radius(b, axis) - distance.abs();
        if overlap <= 1e-6 {
            return None;
        }
        if overlap < best {
            best = overlap;
            normal = if distance < 0.0 { -axis } else { axis };
        }
    }
    best.is_finite().then_some(normal * best)
}

pub(crate) fn contact_normal(a: BoxShape, b: BoxShape, tolerance: f32) -> Option<Vec3> {
    let offset = a.center - b.center;
    let mut smallest_overlap = f32::INFINITY;
    let mut normal = None;
    for axis in axes(a, b) {
        let distance = offset.dot(axis);
        let overlap = radius(a, axis) + radius(b, axis) - distance.abs();
        if overlap < -tolerance {
            return None;
        }
        if overlap < smallest_overlap {
            smallest_overlap = overlap;
            normal = Some(if distance < 0.0 { -axis } else { axis });
        }
    }
    normal
}

pub(crate) fn intersects(a: BoxShape, b: BoxShape, tolerance: f32) -> bool {
    let offset = a.center - b.center;
    axes(a, b).all(|axis| offset.dot(axis).abs() <= radius(a, axis) + radius(b, axis) + tolerance)
}

/// Earliest time of impact for translation with both shapes' orientations fixed.
pub(crate) fn sweep(a: BoxShape, b: BoxShape, delta: Vec3) -> Option<(f32, Vec3)> {
    let offset = a.center - b.center;
    let mut enter = 0.0_f32;
    let mut exit = 1.0_f32;
    let mut normal = Vec3::ZERO;
    for axis in axes(a, b) {
        let distance = offset.dot(axis);
        let extent = radius(a, axis) + radius(b, axis);
        let speed = delta.dot(axis);
        if speed.abs() < 1e-8 {
            if distance.abs() > extent {
                return None;
            }
            continue;
        }
        let t1 = (-extent - distance) / speed;
        let t2 = (extent - distance) / speed;
        let near = t1.min(t2);
        let far = t1.max(t2);
        if near >= enter {
            enter = near;
            normal = if speed > 0.0 { -axis } else { axis };
        }
        exit = exit.min(far);
        if enter > exit {
            return None;
        }
    }
    if exit < 0.0 || enter > 1.0 || normal == Vec3::ZERO || delta.dot(normal) >= -1e-8 {
        return None;
    }
    Some((enter.max(0.0), normal))
}
