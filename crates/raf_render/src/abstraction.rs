//! Render abstraction layer.
//!
//! The trait that separates "what to render" from "how to render".
//! Today: CpuPainter implements it (egui shapes, zero GPU).
//! Tomorrow: WgpuPipeline implements it (vertex buffers, shaders).
//! Future: RayTracePipeline implements it (ray tracing, GI).
//!
//! The SceneGraph does NOT know which backend is active.
//! It gives SceneRenderData to whoever implements RenderBackendTrait.
//!
//! Zero cost when not used. The compiler eliminates dead implementations.

use crate::scene_data::{SceneRenderData, RenderOutput};

// ---------------------------------------------------------------------------
// Core render trait
// ---------------------------------------------------------------------------

/// The central abstraction: any render backend implements this.
/// CpuPainter, WgpuPipeline, RayTracePipeline - all share this interface.
///
/// This is the "plug" that lets Rafi scale from potato to 4K RT
/// without changing a single line in the scene graph or editor.
pub trait RenderBackendTrait {
    /// Name of this backend (for UI display).
    fn name(&self) -> &str;

    /// Initialize the backend. Called once at startup or when switching.
    /// Returns Err if the backend cannot initialize (missing GPU, etc).
    fn init(&mut self) -> Result<(), RenderError>;

    /// Shut down the backend. Release all GPU/system resources.
    fn shutdown(&mut self);

    /// Render a frame. Takes scene data in, produces render output.
    /// This is called every frame by the editor/game loop.
    fn render_frame(&mut self, scene: &SceneRenderData) -> RenderOutput;

    /// Resize the render target (window resized).
    fn resize(&mut self, width: u32, height: u32);

    /// Whether this backend uses GPU resources.
    fn uses_gpu(&self) -> bool;

    /// Estimated VRAM usage in bytes (0 for CPU backends).
    fn vram_usage(&self) -> usize;

    /// Maximum triangles this backend can handle per frame.
    fn max_triangles(&self) -> usize;

    /// Whether this backend supports the given capability.
    fn supports(&self, cap: RenderCapability) -> bool;
}

// ---------------------------------------------------------------------------
// Render capabilities (what a backend CAN do)
// ---------------------------------------------------------------------------

/// Capabilities that a render backend may or may not support.
/// Used to query at runtime what features are available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderCapability {
    /// Basic filled mesh rendering.
    FilledMeshes,
    /// Wireframe overlay.
    Wireframe,
    /// Flat shading with directional light.
    FlatShading,
    /// Per-pixel (smooth) shading.
    SmoothShading,
    /// PBR materials (metallic/roughness).
    PbrMaterials,
    /// Normal mapping.
    NormalMaps,
    /// Texture mapping (UV coordinates).
    TextureMapping,
    /// Shadow mapping (directional/point lights).
    ShadowMapping,
    /// Shadow cascades (for large worlds).
    ShadowCascades,
    /// Screen-space ambient occlusion.
    Ssao,
    /// Bloom post-processing.
    Bloom,
    /// Anti-aliasing (FXAA).
    Fxaa,
    /// Anti-aliasing (MSAA).
    Msaa,
    /// GPU-based vertex deformation (cloth, hair, vegetation).
    GpuDeformation,
    /// Compute shaders.
    ComputeShaders,
    /// Software ray tracing (CPU or compute shader).
    SoftwareRayTracing,
    /// Hardware ray tracing (RTX/DXR).
    HardwareRayTracing,
    /// Global illumination (any method).
    GlobalIllumination,
    /// HDR rendering.
    Hdr,
    /// 4K resolution support.
    Resolution4K,
    /// Instanced rendering (draw many copies efficiently).
    Instancing,
    /// Indirect draw (GPU-driven rendering).
    IndirectDraw,
}

// ---------------------------------------------------------------------------
// Render error
// ---------------------------------------------------------------------------

/// Errors that can occur during rendering backend operations.
#[derive(Debug, Clone)]
pub enum RenderError {
    /// GPU not available or not supported.
    GpuNotAvailable(String),
    /// Shader compilation failed.
    ShaderError(String),
    /// Out of VRAM.
    OutOfMemory,
    /// Backend-specific error.
    BackendError(String),
    /// Feature not supported by this backend.
    Unsupported(RenderCapability),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GpuNotAvailable(msg) => write!(f, "GPU not available: {}", msg),
            Self::ShaderError(msg) => write!(f, "Shader error: {}", msg),
            Self::OutOfMemory => write!(f, "Out of GPU memory"),
            Self::BackendError(msg) => write!(f, "Backend error: {}", msg),
            Self::Unsupported(cap) => write!(f, "Unsupported capability: {:?}", cap),
        }
    }
}

// ---------------------------------------------------------------------------
// Backend registry (manages which backend is active)
// ---------------------------------------------------------------------------

/// Which backend implementation to use.
/// This is the user-facing selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ActiveBackend {
    /// CPU painter via egui (default, zero GPU, potato-friendly).
    CpuPainter,
    /// GPU rendering via wgpu (opt-in, requires GPU).
    Wgpu,
    /// Software ray tracing (very future, CPU-heavy).
    SoftwareRT,
    /// Hardware ray tracing via Vulkan RT / DXR (very future, requires RTX).
    HardwareRT,
}

impl Default for ActiveBackend {
    fn default() -> Self {
        Self::CpuPainter
    }
}

impl ActiveBackend {
    /// Display label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::CpuPainter => "CPU Painter (Lightweight)",
            Self::Wgpu => "GPU (wgpu)",
            Self::SoftwareRT => "Software Ray Tracing",
            Self::HardwareRT => "Hardware Ray Tracing (RTX)",
        }
    }

    /// Spanish label.
    pub fn label_es(&self) -> &'static str {
        match self {
            Self::CpuPainter => "CPU Painter (Ligero)",
            Self::Wgpu => "GPU (wgpu)",
            Self::SoftwareRT => "Trazado de rayos por software",
            Self::HardwareRT => "Trazado de rayos por hardware (RTX)",
        }
    }

    /// Whether this backend requires a GPU.
    pub fn requires_gpu(&self) -> bool {
        !matches!(self, Self::CpuPainter)
    }

    /// All available backends (for UI selector).
    pub fn all() -> &'static [ActiveBackend] {
        &[
            Self::CpuPainter,
            Self::Wgpu,
            Self::SoftwareRT,
            Self::HardwareRT,
        ]
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_cpu() {
        assert_eq!(ActiveBackend::default(), ActiveBackend::CpuPainter);
        assert!(!ActiveBackend::CpuPainter.requires_gpu());
    }

    #[test]
    fn gpu_backends_require_gpu() {
        assert!(ActiveBackend::Wgpu.requires_gpu());
        assert!(ActiveBackend::HardwareRT.requires_gpu());
    }
}
