//! View frustum extraction and culling.
//!
//! Extracts the 6 planes of the view frustum from the VP matrix,
//! then tests points, spheres, and AABBs against them for visibility.
//! Objects fully outside the frustum are culled before any vertex
//! processing, saving significant CPU time in large scenes.

use glam::{Mat4, Vec3, Vec4};

/// A plane in 3D space represented by the equation: ax + by + cz + d = 0.
/// The normal is (a, b, c) and d is the signed distance from the origin.
#[derive(Debug, Clone, Copy)]
pub struct Plane {
    pub normal: Vec3,
    pub d: f32,
}

impl Plane {
    /// Signed distance from the plane to a point.
    /// Positive = in front (normal side), negative = behind.
    pub fn distance_to(&self, point: Vec3) -> f32 {
        self.normal.dot(point) + self.d
    }
}

/// The 6 planes of a view frustum: left, right, bottom, top, near, far.
#[derive(Debug, Clone)]
pub struct Frustum {
    pub planes: [Plane; 6],
}

impl Frustum {
    /// Extract frustum planes from a combined View-Projection matrix.
    ///
    /// Uses the Gribb-Hartmann method: each plane is derived from
    /// rows of the VP matrix.
    pub fn from_matrix(vp: &Mat4) -> Self {
        let m = vp.to_cols_array_2d();
        // Row vectors of the VP matrix (column-major storage)
        let row = |r: usize| -> Vec4 { Vec4::new(m[0][r], m[1][r], m[2][r], m[3][r]) };

        let r0 = row(0);
        let r1 = row(1);
        let r2 = row(2);
        let r3 = row(3);

        let extract = |sum: Vec4| -> Plane {
            let len = Vec3::new(sum.x, sum.y, sum.z).length();
            if len > f32::EPSILON {
                Plane {
                    normal: Vec3::new(sum.x, sum.y, sum.z) / len,
                    d: sum.w / len,
                }
            } else {
                Plane {
                    normal: Vec3::Y,
                    d: 0.0,
                }
            }
        };

        Frustum {
            planes: [
                extract(r3 + r0), // Left
                extract(r3 - r0), // Right
                extract(r3 + r1), // Bottom
                extract(r3 - r1), // Top
                extract(r3 + r2), // Near
                extract(r3 - r2), // Far
            ],
        }
    }

    /// Test if a point is inside the frustum.
    pub fn contains_point(&self, point: Vec3) -> bool {
        for plane in &self.planes {
            if plane.distance_to(point) < 0.0 {
                return false;
            }
        }
        true
    }

    /// Test if a sphere is at least partially inside the frustum.
    ///
    /// Uses bounding sphere test: fast, no false negatives, rare false positives.
    pub fn intersects_sphere(&self, center: Vec3, radius: f32) -> bool {
        for plane in &self.planes {
            if plane.distance_to(center) < -radius {
                return false;
            }
        }
        true
    }

    /// Test if an AABB is at least partially inside the frustum.
    ///
    /// Tests the positive vertex (the corner most in the direction of the
    /// plane normal) against each plane. If the positive vertex is behind
    /// any plane, the entire AABB is outside.
    pub fn intersects_aabb(&self, aabb_min: Vec3, aabb_max: Vec3) -> bool {
        for plane in &self.planes {
            // Find the positive vertex (furthest along the normal direction)
            let positive = Vec3::new(
                if plane.normal.x >= 0.0 {
                    aabb_max.x
                } else {
                    aabb_min.x
                },
                if plane.normal.y >= 0.0 {
                    aabb_max.y
                } else {
                    aabb_min.y
                },
                if plane.normal.z >= 0.0 {
                    aabb_max.z
                } else {
                    aabb_min.z
                },
            );

            if plane.distance_to(positive) < 0.0 {
                return false; // Entirely outside this plane
            }
        }
        true
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_frustum() -> Frustum {
        let view = Mat4::look_at_rh(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
        let proj = Mat4::perspective_rh(60.0f32.to_radians(), 1.0, 0.1, 100.0);
        Frustum::from_matrix(&(proj * view))
    }

    #[test]
    fn origin_visible() {
        let frustum = test_frustum();
        assert!(frustum.contains_point(Vec3::ZERO));
    }

    #[test]
    fn behind_camera_not_visible() {
        let frustum = test_frustum();
        assert!(!frustum.contains_point(Vec3::new(0.0, 0.0, 10.0)));
    }

    #[test]
    fn sphere_visible() {
        let frustum = test_frustum();
        assert!(frustum.intersects_sphere(Vec3::ZERO, 1.0));
    }

    #[test]
    fn sphere_behind_invisible() {
        let frustum = test_frustum();
        assert!(!frustum.intersects_sphere(Vec3::new(0.0, 0.0, 200.0), 1.0));
    }

    #[test]
    fn aabb_visible() {
        let frustum = test_frustum();
        assert!(frustum.intersects_aabb(Vec3::splat(-1.0), Vec3::splat(1.0)));
    }

    #[test]
    fn aabb_far_invisible() {
        let frustum = test_frustum();
        assert!(!frustum.intersects_aabb(Vec3::new(-1.0, -1.0, 200.0), Vec3::new(1.0, 1.0, 201.0),));
    }
}
