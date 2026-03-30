//! Collider system for physics / collision detection.
//!
//! Three types ordered by cost:
//! - AABB: cheapest, axis-aligned bounding box auto-fitted from mesh
//! - ConvexHull: medium cost, tighter fit than AABB
//! - MeshCollider: most expensive, uses exact mesh geometry
//!
//! All can be auto-generated from an EditableMesh.

use glam::Vec3;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Collider types
// ---------------------------------------------------------------------------

/// Type of collision shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColliderType {
    /// No collider.
    None,
    /// Axis-Aligned Bounding Box (cheapest).
    Aabb,
    /// Convex hull (medium cost, tighter fit).
    ConvexHull,
    /// Exact mesh geometry (most expensive, most precise).
    MeshCollider,
}

impl Default for ColliderType {
    fn default() -> Self {
        Self::None
    }
}

/// Axis-Aligned Bounding Box: min and max corners.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    /// Create from a set of vertex positions.
    pub fn from_points(points: &[Vec3]) -> Self {
        if points.is_empty() {
            return Self { min: Vec3::ZERO, max: Vec3::ZERO };
        }
        let mut min = points[0];
        let mut max = points[0];
        for p in &points[1..] {
            min = min.min(*p);
            max = max.max(*p);
        }
        Self { min, max }
    }

    /// Center of the AABB.
    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    /// Half-extents (dimensions / 2).
    pub fn half_extents(&self) -> Vec3 {
        (self.max - self.min) * 0.5
    }

    /// Size (dimensions).
    pub fn size(&self) -> Vec3 {
        self.max - self.min
    }

    /// Check if a point is inside this AABB.
    pub fn contains_point(&self, point: Vec3) -> bool {
        point.x >= self.min.x && point.x <= self.max.x
            && point.y >= self.min.y && point.y <= self.max.y
            && point.z >= self.min.z && point.z <= self.max.z
    }

    /// Check if two AABBs overlap.
    pub fn intersects(&self, other: &Aabb) -> bool {
        self.min.x <= other.max.x && self.max.x >= other.min.x
            && self.min.y <= other.max.y && self.max.y >= other.min.y
            && self.min.z <= other.max.z && self.max.z >= other.min.z
    }

    /// Get the 8 corner points of this AABB (for wireframe rendering).
    pub fn corners(&self) -> [Vec3; 8] {
        [
            Vec3::new(self.min.x, self.min.y, self.min.z),
            Vec3::new(self.max.x, self.min.y, self.min.z),
            Vec3::new(self.max.x, self.max.y, self.min.z),
            Vec3::new(self.min.x, self.max.y, self.min.z),
            Vec3::new(self.min.x, self.min.y, self.max.z),
            Vec3::new(self.max.x, self.min.y, self.max.z),
            Vec3::new(self.max.x, self.max.y, self.max.z),
            Vec3::new(self.min.x, self.max.y, self.max.z),
        ]
    }

    /// Get 12 wireframe edges for AABB visualization.
    pub fn edges(&self) -> [[Vec3; 2]; 12] {
        let c = self.corners();
        [
            [c[0], c[1]], [c[1], c[2]], [c[2], c[3]], [c[3], c[0]], // front
            [c[4], c[5]], [c[5], c[6]], [c[6], c[7]], [c[7], c[4]], // back
            [c[0], c[4]], [c[1], c[5]], [c[2], c[6]], [c[3], c[7]], // sides
        ]
    }
}

/// Convex hull stored as a set of boundary points.
/// Tighter fit than AABB, cheaper than mesh collider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvexHull {
    /// Hull boundary vertices (subset of original mesh vertices).
    pub points: Vec<Vec3>,
}

impl ConvexHull {
    /// Generate a convex hull from a set of points.
    /// Uses a simple incremental approach (good enough for low-poly game meshes).
    pub fn from_points(points: &[Vec3]) -> Self {
        if points.len() <= 4 {
            return Self { points: points.to_vec() };
        }

        // Simple approach: keep only points that are on the bounding "surface".
        // For proper convex hull we'd use gift wrapping or quickhull,
        // but for game meshes with <100 vertices this is fast enough.
        let aabb = Aabb::from_points(points);
        let center = aabb.center();
        let mut hull_points: Vec<Vec3> = Vec::new();

        // Keep points that are furthest from center in each octant.
        for point in points {
            let dir = (*point - center).normalize_or_zero();
            let dist = (*point - center).length();
            // Check if there's already a hull point in a similar direction.
            let mut is_extreme = true;
            for hp in &hull_points {
                let hp_dir = (*hp - center).normalize_or_zero();
                let hp_dist = (*hp - center).length();
                // If another point is in a similar direction but further, skip this one.
                if dir.dot(hp_dir) > 0.95 && hp_dist >= dist {
                    is_extreme = false;
                    break;
                }
            }
            if is_extreme {
                // Remove any existing hull point that this new point dominates.
                hull_points.retain(|hp| {
                    let hp_dir = (*hp - center).normalize_or_zero();
                    let hp_dist = (*hp - center).length();
                    !(dir.dot(hp_dir) > 0.95 && dist > hp_dist)
                });
                hull_points.push(*point);
            }
        }

        Self { points: hull_points }
    }

    /// Quick point-in-hull test (approximate: uses AABB of hull points).
    pub fn contains_point_approx(&self, point: Vec3) -> bool {
        Aabb::from_points(&self.points).contains_point(point)
    }
}

/// Mesh collider - uses exact mesh triangles.
/// Most precise but most expensive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshColliderData {
    /// Vertices used for collision.
    pub vertices: Vec<Vec3>,
    /// Triangle indices (groups of 3).
    pub indices: Vec<[usize; 3]>,
}

impl MeshColliderData {
    /// Create from vertex positions and triangle indices.
    pub fn new(vertices: Vec<Vec3>, indices: Vec<[usize; 3]>) -> Self {
        Self { vertices, indices }
    }

    /// Get the AABB of this mesh collider (cheap early-out test).
    pub fn aabb(&self) -> Aabb {
        Aabb::from_points(&self.vertices)
    }

    /// Triangle count.
    pub fn triangle_count(&self) -> usize {
        self.indices.len()
    }
}

// ---------------------------------------------------------------------------
// Collider component
// ---------------------------------------------------------------------------

/// Collider component attached to a scene entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collider {
    /// Type of collider.
    pub collider_type: ColliderType,
    /// AABB data (always computed, used as broad-phase).
    pub aabb: Aabb,
    /// Convex hull data (only if type is ConvexHull).
    pub convex_hull: Option<ConvexHull>,
    /// Mesh collider data (only if type is MeshCollider).
    pub mesh_collider: Option<MeshColliderData>,
    /// Whether the collider is visible in the editor.
    pub visible_in_editor: bool,
    /// Offset from entity position.
    pub offset: Vec3,
}

impl Default for Collider {
    fn default() -> Self {
        Self {
            collider_type: ColliderType::None,
            aabb: Aabb { min: Vec3::ZERO, max: Vec3::ZERO },
            convex_hull: None,
            mesh_collider: None,
            visible_in_editor: true,
            offset: Vec3::ZERO,
        }
    }
}

impl Collider {
    /// Auto-generate collider from a set of vertex positions.
    pub fn auto_fit(points: &[Vec3], collider_type: ColliderType) -> Self {
        let aabb = Aabb::from_points(points);

        let convex_hull = match collider_type {
            ColliderType::ConvexHull => Some(ConvexHull::from_points(points)),
            _ => None,
        };

        Self {
            collider_type,
            aabb,
            convex_hull,
            mesh_collider: None,
            visible_in_editor: true,
            offset: Vec3::ZERO,
        }
    }

    /// Auto-generate from mesh vertices and faces.
    pub fn auto_fit_mesh(
        vertices: &[Vec3],
        indices: &[[usize; 3]],
        collider_type: ColliderType,
    ) -> Self {
        let aabb = Aabb::from_points(vertices);

        let convex_hull = match collider_type {
            ColliderType::ConvexHull => Some(ConvexHull::from_points(vertices)),
            _ => None,
        };

        let mesh_collider = match collider_type {
            ColliderType::MeshCollider => {
                Some(MeshColliderData::new(vertices.to_vec(), indices.to_vec()))
            }
            _ => None,
        };

        Self {
            collider_type,
            aabb,
            convex_hull,
            mesh_collider,
            visible_in_editor: true,
            offset: Vec3::ZERO,
        }
    }

    /// Label for UI display.
    pub fn type_label(&self) -> &'static str {
        match self.collider_type {
            ColliderType::None => "None",
            ColliderType::Aabb => "AABB",
            ColliderType::ConvexHull => "Convex Hull",
            ColliderType::MeshCollider => "Mesh",
        }
    }

    /// Label in Spanish.
    pub fn type_label_es(&self) -> &'static str {
        match self.collider_type {
            ColliderType::None => "Ninguno",
            ColliderType::Aabb => "AABB",
            ColliderType::ConvexHull => "Hull Convexo",
            ColliderType::MeshCollider => "Malla",
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aabb_from_cube() {
        let points = vec![
            Vec3::new(-0.5, -0.5, -0.5),
            Vec3::new(0.5, 0.5, 0.5),
        ];
        let aabb = Aabb::from_points(&points);
        assert!((aabb.center() - Vec3::ZERO).length() < 0.001);
        assert!((aabb.half_extents() - Vec3::splat(0.5)).length() < 0.001);
    }

    #[test]
    fn aabb_intersection() {
        let a = Aabb::from_points(&[Vec3::ZERO, Vec3::ONE]);
        let b = Aabb::from_points(&[Vec3::splat(0.5), Vec3::splat(1.5)]);
        assert!(a.intersects(&b));

        let c = Aabb::from_points(&[Vec3::splat(5.0), Vec3::splat(6.0)]);
        assert!(!a.intersects(&c));
    }

    #[test]
    fn collider_auto_fit() {
        let points = vec![
            Vec3::new(-1.0, 0.0, -1.0),
            Vec3::new(1.0, 2.0, 1.0),
        ];
        let col = Collider::auto_fit(&points, ColliderType::Aabb);
        assert_eq!(col.collider_type, ColliderType::Aabb);
        assert!(col.aabb.contains_point(Vec3::new(0.0, 1.0, 0.0)));
    }
}
