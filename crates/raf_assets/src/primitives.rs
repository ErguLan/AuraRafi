//! Editable 3D primitives - cubes, spheres, cylinders that users can
//! stretch, scale, and modify per-axis.

use glam::Vec3;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Primitive shape type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrimitiveShape {
    Cube,
    Sphere,
    Cylinder,
    Plane,
}

impl PrimitiveShape {
    pub fn display_name(&self) -> &str {
        match self {
            Self::Cube => "Cube",
            Self::Sphere => "Sphere",
            Self::Cylinder => "Cylinder",
            Self::Plane => "Plane",
        }
    }
}

/// An editable 3D primitive with per-axis dimensions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Primitive3D {
    pub id: Uuid,
    pub shape: PrimitiveShape,
    pub name: String,
    /// Per-axis dimensions (width, height, depth).
    pub dimensions: Vec3,
    /// Color as RGBA [0..1].
    pub color: [f32; 4],
    /// Number of segments (for spheres/cylinders).
    pub segments: u32,
}

impl Primitive3D {
    /// Create a default primitive of the given shape.
    pub fn new(shape: PrimitiveShape) -> Self {
        Self {
            id: Uuid::new_v4(),
            shape,
            name: shape.display_name().to_string(),
            dimensions: Vec3::ONE,
            color: [0.8, 0.8, 0.8, 1.0],
            segments: 16,
        }
    }

    /// Create a cube with uniform size.
    pub fn cube(size: f32) -> Self {
        let mut p = Self::new(PrimitiveShape::Cube);
        p.dimensions = Vec3::splat(size);
        p
    }

    /// Create a sphere with radius.
    pub fn sphere(radius: f32) -> Self {
        let mut p = Self::new(PrimitiveShape::Sphere);
        p.dimensions = Vec3::splat(radius * 2.0);
        p
    }
}
