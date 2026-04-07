//! Scene render data - the bridge between SceneGraph and any render backend.
//!
//! SceneRenderData is a flat, GPU-friendly snapshot of what needs to be drawn.
//! The SceneGraph builds it, the render backend consumes it.
//! This separation means the SceneGraph never knows about GPU APIs.
//!
//! Zero cost: just plain data structs. No allocations until populated.

use glam::{Mat4, Vec3};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Render mesh (what the backend draws)
// ---------------------------------------------------------------------------

/// A single mesh ready for rendering.
/// Flat arrays for GPU-friendliness (can be uploaded to vertex buffers directly).
#[derive(Debug, Clone)]
pub struct RenderMesh {
    /// Vertex positions [x, y, z, x, y, z, ...].
    pub positions: Vec<f32>,
    /// Vertex normals [nx, ny, nz, ...] (same count as positions).
    pub normals: Vec<f32>,
    /// UV coordinates [u, v, u, v, ...] (same vertex count). Empty if no UVs.
    pub uvs: Vec<f32>,
    /// Triangle indices (groups of 3).
    pub indices: Vec<u32>,
    /// Model-to-world transform matrix.
    pub transform: Mat4,
    /// Material index (indexes into SceneRenderData.materials).
    pub material_idx: usize,
    /// LOD level this mesh represents (0 = highest detail).
    pub lod_level: u8,
    /// Whether this mesh casts shadows.
    pub cast_shadow: bool,
    /// Whether this mesh receives shadows.
    pub receive_shadow: bool,
    /// Instance count (1 for normal, >1 for instanced rendering).
    pub instance_count: u32,
}

impl Default for RenderMesh {
    fn default() -> Self {
        Self {
            positions: Vec::new(),
            normals: Vec::new(),
            uvs: Vec::new(),
            indices: Vec::new(),
            transform: Mat4::IDENTITY,
            material_idx: 0,
            lod_level: 0,
            cast_shadow: true,
            receive_shadow: true,
            instance_count: 1,
        }
    }
}

impl RenderMesh {
    /// Vertex count.
    pub fn vertex_count(&self) -> usize {
        self.positions.len() / 3
    }

    /// Triangle count.
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Estimated GPU memory in bytes.
    pub fn gpu_bytes(&self) -> usize {
        let pos = self.positions.len() * 4;
        let nrm = self.normals.len() * 4;
        let uv = self.uvs.len() * 4;
        let idx = self.indices.len() * 4;
        pos + nrm + uv + idx
    }
}

// ---------------------------------------------------------------------------
// Lights
// ---------------------------------------------------------------------------

/// Type of light source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LightType {
    /// Infinite directional light (sun).
    Directional,
    /// Point light (omnidirectional, with falloff).
    Point,
    /// Spot light (cone, with falloff + angle).
    Spot,
    /// Area light (rectangle emitter, for soft shadows). Future.
    Area,
}

/// A light source for rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderLight {
    /// Light type.
    pub light_type: LightType,
    /// Position in world space (ignored for directional).
    pub position: Vec3,
    /// Direction (normalized). For directional and spot.
    pub direction: Vec3,
    /// Color (linear RGB, 0.0-1.0+). Values >1.0 for HDR.
    pub color: Vec3,
    /// Intensity multiplier.
    pub intensity: f32,
    /// Range/radius for point and spot lights.
    pub range: f32,
    /// Inner cone angle in radians (spot only).
    pub inner_cone: f32,
    /// Outer cone angle in radians (spot only).
    pub outer_cone: f32,
    /// Whether this light casts shadows.
    pub cast_shadow: bool,
    /// Shadow map resolution (e.g., 512, 1024, 2048).
    pub shadow_resolution: u32,
}

impl Default for RenderLight {
    fn default() -> Self {
        Self {
            light_type: LightType::Directional,
            position: Vec3::ZERO,
            direction: Vec3::new(0.0, -1.0, -0.5).normalize(),
            color: Vec3::ONE,
            intensity: 1.0,
            range: 50.0,
            inner_cone: 0.3,
            outer_cone: 0.5,
            cast_shadow: true,
            shadow_resolution: 1024,
        }
    }
}

// ---------------------------------------------------------------------------
// Camera data (for the render backend)
// ---------------------------------------------------------------------------

/// Camera data needed by the render backend.
#[derive(Debug, Clone)]
pub struct RenderCamera {
    /// View matrix (world-to-camera).
    pub view: Mat4,
    /// Projection matrix (camera-to-clip).
    pub projection: Mat4,
    /// Camera position in world space.
    pub position: Vec3,
    /// Forward direction.
    pub forward: Vec3,
    /// Near clip plane.
    pub near: f32,
    /// Far clip plane.
    pub far: f32,
    /// Field of view in radians (for perspective).
    pub fov: f32,
    /// Viewport width in pixels.
    pub viewport_width: u32,
    /// Viewport height in pixels.
    pub viewport_height: u32,
}

impl Default for RenderCamera {
    fn default() -> Self {
        Self {
            view: Mat4::IDENTITY,
            projection: Mat4::IDENTITY,
            position: Vec3::new(0.0, 5.0, 10.0),
            forward: Vec3::NEG_Z,
            near: 0.1,
            far: 1000.0,
            fov: std::f32::consts::FRAC_PI_4,
            viewport_width: 800,
            viewport_height: 600,
        }
    }
}

// ---------------------------------------------------------------------------
// Environment
// ---------------------------------------------------------------------------

/// Environment/sky data for rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderEnvironment {
    /// Ambient light color (fills all shadows).
    pub ambient_color: Vec3,
    /// Ambient intensity.
    pub ambient_intensity: f32,
    /// Fog color.
    pub fog_color: Vec3,
    /// Fog start distance.
    pub fog_start: f32,
    /// Fog end distance (objects beyond this are fully fogged).
    pub fog_end: f32,
    /// Whether fog is enabled.
    pub fog_enabled: bool,
    /// Sky color (top). For gradient sky.
    pub sky_color_top: Vec3,
    /// Sky color (bottom/horizon).
    pub sky_color_bottom: Vec3,
    /// Exposure for HDR tone mapping. 1.0 = neutral.
    pub exposure: f32,
}

impl Default for RenderEnvironment {
    fn default() -> Self {
        Self {
            ambient_color: Vec3::new(0.15, 0.15, 0.2),
            ambient_intensity: 0.3,
            fog_color: Vec3::new(0.7, 0.75, 0.8),
            fog_start: 100.0,
            fog_end: 500.0,
            fog_enabled: false,
            sky_color_top: Vec3::new(0.3, 0.5, 0.9),
            sky_color_bottom: Vec3::new(0.7, 0.8, 0.9),
            exposure: 1.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Scene render data (the full frame package)
// ---------------------------------------------------------------------------

/// Complete data package for rendering one frame.
/// Built by the scene system, consumed by the render backend.
/// This is the ONLY thing the render backend sees - it never touches SceneGraph.
#[derive(Debug, Clone, Default)]
pub struct SceneRenderData {
    /// All visible meshes (after culling and LOD selection).
    pub meshes: Vec<RenderMesh>,
    /// All active lights.
    pub lights: Vec<RenderLight>,
    /// Camera data.
    pub camera: RenderCamera,
    /// Environment settings.
    pub environment: RenderEnvironment,
    /// Total triangle count (pre-computed for stats).
    pub total_triangles: usize,
    /// Total vertex count.
    pub total_vertices: usize,
    /// Frame number (monotonically increasing).
    pub frame_number: u64,
    /// Delta time since last frame (seconds).
    pub delta_time: f32,
}

impl SceneRenderData {
    /// Recompute stats from mesh data.
    pub fn update_stats(&mut self) {
        self.total_triangles = self.meshes.iter().map(|m| m.triangle_count()).sum();
        self.total_vertices = self.meshes.iter().map(|m| m.vertex_count()).sum();
    }

    /// Estimated total GPU memory for all meshes.
    pub fn total_gpu_bytes(&self) -> usize {
        self.meshes.iter().map(|m| m.gpu_bytes()).sum()
    }

    /// Clear all data for the next frame.
    pub fn clear(&mut self) {
        self.meshes.clear();
        self.lights.clear();
        self.total_triangles = 0;
        self.total_vertices = 0;
    }
}

// ---------------------------------------------------------------------------
// Render output (what the backend produces)
// ---------------------------------------------------------------------------

/// Output from a render frame. Contains stats and diagnostics.
#[derive(Debug, Clone, Default)]
pub struct RenderOutput {
    /// Time spent rendering (milliseconds).
    pub render_time_ms: f32,
    /// Triangles actually drawn (after culling).
    pub triangles_drawn: usize,
    /// Draw calls issued.
    pub draw_calls: usize,
    /// Whether the frame exceeded the time budget.
    pub over_budget: bool,
    /// Any warnings from this frame.
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_scene_data() {
        let data = SceneRenderData::default();
        assert_eq!(data.total_triangles, 0);
        assert_eq!(data.meshes.len(), 0);
    }

    #[test]
    fn mesh_gpu_bytes() {
        let mesh = RenderMesh {
            positions: vec![0.0; 9],   // 3 vertices
            normals: vec![0.0; 9],
            uvs: vec![0.0; 6],
            indices: vec![0, 1, 2],
            ..Default::default()
        };
        assert_eq!(mesh.vertex_count(), 3);
        assert_eq!(mesh.triangle_count(), 1);
        assert!(mesh.gpu_bytes() > 0);
    }

    #[test]
    fn default_light_is_directional() {
        let light = RenderLight::default();
        assert_eq!(light.light_type, LightType::Directional);
    }
}
