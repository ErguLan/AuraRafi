//! Complement Trace - ray tracing complement designed from day 1.
//!
//! Part of the Complements system: advanced rendering features that are
//! optional, zero-cost when disabled, and architecturally native (not patched).
//!
//! Defines the ray tracing abstraction so that the architecture supports RT
//! natively, like BlackSpace Engine. When RT is implemented, it plugs into
//! this interface without modifying any other part of the engine.
//!
//! Two paths prepared:
//! - Software RT: runs on CPU or compute shaders. Slow but works anywhere.
//! - Hardware RT: uses Vulkan RT / DXR extensions. Fast, requires RTX GPU.
//!
//! Zero cost: just traits and data types. No ray is traced until a
//! backend implementing the RT complement is activated by the user.

use glam::Vec3;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Ray
// ---------------------------------------------------------------------------

/// A ray in 3D space.
#[derive(Debug, Clone, Copy)]
pub struct Ray {
    /// Origin point.
    pub origin: Vec3,
    /// Direction (normalized).
    pub direction: Vec3,
    /// Maximum distance (prevents infinite rays).
    pub max_distance: f32,
}

impl Ray {
    pub fn new(origin: Vec3, direction: Vec3, max_distance: f32) -> Self {
        Self {
            origin,
            direction: direction.normalize(),
            max_distance,
        }
    }

    /// Point along the ray at parameter t.
    pub fn at(&self, t: f32) -> Vec3 {
        self.origin + self.direction * t
    }
}

// ---------------------------------------------------------------------------
// Hit result
// ---------------------------------------------------------------------------

/// Result of a ray intersection test.
#[derive(Debug, Clone)]
pub struct RayHit {
    /// Distance from ray origin to hit point.
    pub distance: f32,
    /// Hit position in world space.
    pub position: Vec3,
    /// Surface normal at hit point.
    pub normal: Vec3,
    /// UV coordinates at hit point (if available).
    pub uv: (f32, f32),
    /// Material index of the hit surface.
    pub material_idx: usize,
    /// Entity/mesh index that was hit.
    pub entity_idx: usize,
    /// Whether the ray hit the front face.
    pub front_face: bool,
}

// ---------------------------------------------------------------------------
// RT config
// ---------------------------------------------------------------------------

/// Configuration for the Complement Trace system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RayTraceConfig {
    /// Whether RT is enabled at all.
    pub enabled: bool,
    /// RT mode.
    pub mode: RayTraceMode,
    /// Samples per pixel (higher = less noise, more expensive).
    pub samples_per_pixel: u32,
    /// Maximum ray bounces (higher = more realistic GI, more expensive).
    pub max_bounces: u32,
    /// Resolution scale (0.5 = half res RT, 1.0 = full res).
    /// Lower = faster, then upscale with denoiser.
    pub resolution_scale: f32,
    /// Whether to use a denoiser on the RT output.
    pub denoise: bool,
    /// Which RT features to enable.
    pub features: RayTraceFeatures,
}

impl Default for RayTraceConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Off by default - user opts in
            mode: RayTraceMode::Disabled,
            samples_per_pixel: 1,
            max_bounces: 3,
            resolution_scale: 0.5, // Half res for performance
            denoise: true,
            features: RayTraceFeatures::default(),
        }
    }
}

/// RT mode selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RayTraceMode {
    /// RT completely disabled (zero cost).
    Disabled,
    /// Software RT on CPU (slow, works anywhere, for previewing).
    Software,
    /// Hardware RT via Vulkan/DXR (fast, requires RTX/RDNA2+).
    Hardware,
    /// Hybrid: rasterize primary, RT for reflections/shadows only.
    Hybrid,
}

impl Default for RayTraceMode {
    fn default() -> Self {
        Self::Disabled
    }
}

impl RayTraceMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Disabled => "Disabled",
            Self::Software => "Software (CPU)",
            Self::Hardware => "Hardware (RTX)",
            Self::Hybrid => "Hybrid (Raster + RT)",
        }
    }

    pub fn label_es(&self) -> &'static str {
        match self {
            Self::Disabled => "Desactivado",
            Self::Software => "Software (CPU)",
            Self::Hardware => "Hardware (RTX)",
            Self::Hybrid => "Hibrido (Raster + RT)",
        }
    }
}

/// Which RT features to enable individually.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RayTraceFeatures {
    /// RT shadows (sharp, accurate shadows).
    pub shadows: bool,
    /// RT reflections (accurate mirror/glossy reflections).
    pub reflections: bool,
    /// RT global illumination (indirect light bouncing).
    pub global_illumination: bool,
    /// RT ambient occlusion (accurate contact shadows).
    pub ambient_occlusion: bool,
    /// RT refractions (glass, water).
    pub refractions: bool,
    /// RT caustics (light patterns through glass/water). Very expensive.
    pub caustics: bool,
}

impl Default for RayTraceFeatures {
    fn default() -> Self {
        Self {
            shadows: true,
            reflections: true,
            global_illumination: true,
            ambient_occlusion: true,
            refractions: false,  // Expensive, off by default
            caustics: false,     // Very expensive, off by default
        }
    }
}

// ---------------------------------------------------------------------------
// Acceleration structure (BVH for RT)
// ---------------------------------------------------------------------------

/// Bounding Volume Hierarchy node for ray tracing acceleration.
/// Every RT engine needs this - it makes ray-triangle intersection O(log n)
/// instead of O(n).
#[derive(Debug, Clone)]
pub struct BvhNode {
    /// Axis-aligned bounding box min.
    pub aabb_min: Vec3,
    /// AABB max.
    pub aabb_max: Vec3,
    /// Left child index (in flat array). usize::MAX if leaf.
    pub left: usize,
    /// Right child index. usize::MAX if leaf.
    pub right: usize,
    /// For leaf nodes: first triangle index.
    pub first_tri: usize,
    /// For leaf nodes: number of triangles.
    pub tri_count: usize,
}

/// Top-level acceleration structure.
/// Contains the BVH tree for the entire scene.
#[derive(Debug, Clone, Default)]
pub struct AccelerationStructure {
    /// BVH nodes (flat array, root at index 0).
    pub nodes: Vec<BvhNode>,
    /// Whether the structure needs rebuilding (scene changed).
    pub dirty: bool,
    /// Total triangles in the structure.
    pub total_triangles: usize,
    /// Build time in milliseconds (for performance monitoring).
    pub build_time_ms: f32,
}

impl AccelerationStructure {
    /// Mark as needing rebuild (call when scene geometry changes).
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Clear the structure.
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.dirty = true;
        self.total_triangles = 0;
    }

    /// Memory usage estimate.
    pub fn mem_bytes(&self) -> usize {
        self.nodes.len() * std::mem::size_of::<BvhNode>()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ray_at_distance() {
        let ray = Ray::new(Vec3::ZERO, Vec3::X, 100.0);
        let point = ray.at(5.0);
        assert!((point.x - 5.0).abs() < 0.001);
    }

    #[test]
    fn default_config_disabled() {
        let config = RayTraceConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.mode, RayTraceMode::Disabled);
    }

    #[test]
    fn bvh_starts_dirty() {
        let accel = AccelerationStructure::default();
        assert!(accel.dirty);
    }
}
