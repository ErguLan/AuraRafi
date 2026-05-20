//! Ray casting and intersection tests.
//!
//! Used for entity picking (click-to-select) and gizmo interaction.
//! Provides ray-sphere and ray-AABB tests for fast broad-phase,
//! plus ray-triangle for precise hit detection.

use glam::Vec3;

/// A ray in 3D space defined by an origin point and a direction vector.
#[derive(Debug, Clone, Copy)]
pub struct Ray {
    /// Starting point of the ray.
    pub origin: Vec3,
    /// Direction of the ray (should be normalized for correct distances).
    pub direction: Vec3,
}

impl Ray {
    /// Create a new ray.
    pub fn new(origin: Vec3, direction: Vec3) -> Self {
        Self { origin, direction: direction.normalize_or_zero() }
    }

    /// Get the point at parameter t along the ray.
    pub fn at(&self, t: f32) -> Vec3 {
        self.origin + self.direction * t
    }
}

/// Ray-sphere intersection test.
///
/// Returns the distance along the ray to the nearest intersection point,
/// or None if the ray misses the sphere.
pub fn ray_sphere(ray: &Ray, center: Vec3, radius: f32) -> Option<f32> {
    let oc = ray.origin - center;
    let a = ray.direction.dot(ray.direction);
    let b = 2.0 * oc.dot(ray.direction);
    let c = oc.dot(oc) - radius * radius;
    let discriminant = b * b - 4.0 * a * c;

    if discriminant < 0.0 {
        return None;
    }

    let sqrt_disc = discriminant.sqrt();
    let t0 = (-b - sqrt_disc) / (2.0 * a);
    let t1 = (-b + sqrt_disc) / (2.0 * a);

    if t0 >= 0.0 {
        Some(t0)
    } else if t1 >= 0.0 {
        Some(t1)
    } else {
        None // Both intersections behind the ray origin
    }
}

/// Ray-AABB (axis-aligned bounding box) intersection test.
///
/// Returns the distance to the nearest intersection, or None if missed.
/// Uses the slab method for efficient computation.
pub fn ray_aabb(ray: &Ray, aabb_min: Vec3, aabb_max: Vec3) -> Option<f32> {
    let inv_dir = Vec3::new(
        if ray.direction.x.abs() > f32::EPSILON { 1.0 / ray.direction.x } else { f32::MAX },
        if ray.direction.y.abs() > f32::EPSILON { 1.0 / ray.direction.y } else { f32::MAX },
        if ray.direction.z.abs() > f32::EPSILON { 1.0 / ray.direction.z } else { f32::MAX },
    );

    let t1 = (aabb_min.x - ray.origin.x) * inv_dir.x;
    let t2 = (aabb_max.x - ray.origin.x) * inv_dir.x;
    let t3 = (aabb_min.y - ray.origin.y) * inv_dir.y;
    let t4 = (aabb_max.y - ray.origin.y) * inv_dir.y;
    let t5 = (aabb_min.z - ray.origin.z) * inv_dir.z;
    let t6 = (aabb_max.z - ray.origin.z) * inv_dir.z;

    let tmin = t1.min(t2).max(t3.min(t4)).max(t5.min(t6));
    let tmax = t1.max(t2).min(t3.max(t4)).min(t5.max(t6));

    if tmax < 0.0 || tmin > tmax {
        return None;
    }

    Some(if tmin >= 0.0 { tmin } else { tmax })
}

/// Ray-triangle intersection using the Moller-Trumbore algorithm.
///
/// Returns the distance along the ray to the intersection point,
/// or None if the ray misses the triangle.
///
/// The triangle is defined by three vertices in CCW winding order.
/// Only front-face intersections are detected (ray hits the side
/// the normal points from).
pub fn ray_triangle(ray: &Ray, v0: Vec3, v1: Vec3, v2: Vec3) -> Option<f32> {
    let edge1 = v1 - v0;
    let edge2 = v2 - v0;
    let h = ray.direction.cross(edge2);
    let a = edge1.dot(h);

    // If a is near zero, the ray is parallel to the triangle.
    if a.abs() < f32::EPSILON {
        return None;
    }

    let f = 1.0 / a;
    let s = ray.origin - v0;
    let u = f * s.dot(h);

    if !(0.0..=1.0).contains(&u) {
        return None;
    }

    let q = s.cross(edge1);
    let v = f * ray.direction.dot(q);

    if v < 0.0 || u + v > 1.0 {
        return None;
    }

    let t = f * edge2.dot(q);

    if t > f32::EPSILON {
        Some(t)
    } else {
        None // Intersection behind the ray
    }
}

/// Ray-plane intersection.
///
/// The plane is defined by a point on the plane and its normal.
/// Returns the distance along the ray, or None if parallel.
pub fn ray_plane(ray: &Ray, plane_point: Vec3, plane_normal: Vec3) -> Option<f32> {
    let denom = plane_normal.dot(ray.direction);
    if denom.abs() < f32::EPSILON {
        return None; // Parallel
    }

    let t = (plane_point - ray.origin).dot(plane_normal) / denom;
    if t >= 0.0 {
        Some(t)
    } else {
        None // Behind ray
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ray_hits_sphere() {
        let ray = Ray::new(Vec3::new(0.0, 0.0, 5.0), Vec3::NEG_Z);
        let t = ray_sphere(&ray, Vec3::ZERO, 1.0);
        assert!(t.is_some());
        let dist = t.unwrap();
        assert!((dist - 4.0).abs() < 0.01, "expected ~4.0, got {}", dist);
    }

    #[test]
    fn ray_misses_sphere() {
        let ray = Ray::new(Vec3::new(0.0, 5.0, 5.0), Vec3::NEG_Z);
        assert!(ray_sphere(&ray, Vec3::ZERO, 1.0).is_none());
    }

    #[test]
    fn ray_hits_aabb() {
        let ray = Ray::new(Vec3::new(0.0, 0.0, 5.0), Vec3::NEG_Z);
        let t = ray_aabb(&ray, Vec3::splat(-1.0), Vec3::splat(1.0));
        assert!(t.is_some());
        assert!((t.unwrap() - 4.0).abs() < 0.01);
    }

    #[test]
    fn ray_hits_triangle() {
        let ray = Ray::new(Vec3::new(0.0, 0.0, 5.0), Vec3::NEG_Z);
        let v0 = Vec3::new(-1.0, -1.0, 0.0);
        let v1 = Vec3::new(1.0, -1.0, 0.0);
        let v2 = Vec3::new(0.0, 1.0, 0.0);
        let t = ray_triangle(&ray, v0, v1, v2);
        assert!(t.is_some());
        assert!((t.unwrap() - 5.0).abs() < 0.01);
    }

    #[test]
    fn ray_misses_triangle() {
        let ray = Ray::new(Vec3::new(5.0, 5.0, 5.0), Vec3::NEG_Z);
        let v0 = Vec3::new(-1.0, -1.0, 0.0);
        let v1 = Vec3::new(1.0, -1.0, 0.0);
        let v2 = Vec3::new(0.0, 1.0, 0.0);
        assert!(ray_triangle(&ray, v0, v1, v2).is_none());
    }

    #[test]
    fn ray_hits_plane() {
        let ray = Ray::new(Vec3::new(0.0, 5.0, 0.0), Vec3::NEG_Y);
        let t = ray_plane(&ray, Vec3::ZERO, Vec3::Y);
        assert!(t.is_some());
        assert!((t.unwrap() - 5.0).abs() < 0.01);
    }
}
