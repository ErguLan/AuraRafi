//! Per-project rendering configuration.
//!
//! ALL advanced features are OFF by default (potato mode).
//! Users opt-in via project settings. Zero cost when disabled:
//! no GPU memory, no extra draw calls, no shader compilation.
//!
//! The engine ALWAYS starts in CPU painter mode. GPU features
//! only activate when explicitly enabled AND the hardware supports them.

use serde::{Deserialize, Serialize};

/// Complete rendering configuration for a project.
/// Serialized alongside project settings.
/// DEFAULT = everything off = potato mode = fast startup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderConfig {
    // -- Backend selection --
    /// Use GPU-accelerated rendering (wgpu). If false, CPU painter only.
    /// Default: false (potato mode).
    pub use_gpu: bool,

    // -- Lighting --
    /// Enable specular highlights on surfaces.
    /// Cost: negligible (one dot product per face). Default: false.
    pub specular_enabled: bool,
    /// Maximum number of point lights evaluated per frame.
    /// 0 = only directional light. Default: 0.
    pub max_point_lights: u32,
    /// Ambient light intensity (0.0 = pitch black, 1.0 = fully lit).
    /// Default: 0.3.
    pub ambient_intensity: f32,

    // -- Shadows --
    /// Enable shadow mapping. Requires GPU backend.
    /// Default: false.
    pub shadows_enabled: bool,
    /// Shadow map resolution (256, 512, 1024, 2048).
    /// Higher = sharper shadows, more VRAM. Default: 512.
    pub shadow_resolution: u32,

    // -- Post-processing --
    /// Enable bloom/glow effect.
    /// Cost: low (additive blend pass). Default: false.
    pub bloom_enabled: bool,
    /// Bloom intensity (0.0 - 1.0). Default: 0.3.
    pub bloom_intensity: f32,
    /// Enable screen-space ambient occlusion.
    /// Cost: medium (requires depth buffer). Default: false.
    pub ssao_enabled: bool,
    /// SSAO radius. Default: 0.5.
    pub ssao_radius: f32,
    /// Enable fog (distance-based color blending).
    /// Cost: negligible. Default: false.
    pub fog_enabled: bool,
    /// Fog color [R, G, B]. Default: dark gray.
    pub fog_color: [f32; 3],
    /// Fog start distance. Default: 20.0.
    pub fog_start: f32,
    /// Fog end distance (fully opaque). Default: 50.0.
    pub fog_end: f32,

    // -- Anti-aliasing --
    /// Anti-aliasing mode. Default: None.
    pub anti_aliasing: AntiAliasingMode,

    // -- Textures --
    /// Enable texture loading and UV mapping.
    /// Cost: memory for loaded images. Default: false.
    pub textures_enabled: bool,
    /// Maximum texture resolution (auto-downscale if larger).
    /// Default: 512 (potato-friendly).
    pub max_texture_size: u32,

    // -- Advanced (future) --
    /// Enable PBR materials (metallic/roughness).
    /// Requires GPU backend. Default: false.
    pub pbr_enabled: bool,
    /// Enable real-time reflections.
    /// Heavy. Requires GPU backend. Default: false.
    pub reflections_enabled: bool,
    /// Enable ray-tracing features (Complement Trace).
    /// Very heavy. Requires RTX hardware. Default: false.
    pub raytrace_enabled: bool,
    /// Enable GPU vertex deformation (cloth, hair, vegetation).
    /// Requires GPU backend. Default: false.
    pub gpu_deform_enabled: bool,

    // -- Performance limits --
    /// Maximum triangles per frame before quality auto-reduction.
    /// Default: 10000 (generous for potato).
    pub max_triangles: u32,
    /// Frame budget in milliseconds. If exceeded, auto-reduce detail.
    /// Default: 33 (30fps minimum).
    pub frame_budget_ms: f32,
}

/// Anti-aliasing modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AntiAliasingMode {
    /// No anti-aliasing. Fastest. Default.
    None,
    /// Fast approximate anti-aliasing (post-process).
    /// Cost: low.
    Fxaa,
    /// Multi-sample anti-aliasing (4x).
    /// Cost: medium (4x framebuffer memory).
    Msaa4x,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            // Everything OFF = potato mode.
            use_gpu: false,
            specular_enabled: false,
            max_point_lights: 0,
            ambient_intensity: 0.3,
            shadows_enabled: false,
            shadow_resolution: 512,
            bloom_enabled: false,
            bloom_intensity: 0.3,
            ssao_enabled: false,
            ssao_radius: 0.5,
            fog_enabled: false,
            fog_color: [0.15, 0.15, 0.18],
            fog_start: 20.0,
            fog_end: 50.0,
            anti_aliasing: AntiAliasingMode::None,
            textures_enabled: false,
            max_texture_size: 512,
            pbr_enabled: false,
            reflections_enabled: false,
            raytrace_enabled: false,
            gpu_deform_enabled: false,
            max_triangles: 10_000,
            frame_budget_ms: 33.0,
        }
    }
}

impl RenderConfig {
    /// Potato preset: absolute minimum, everything off.
    pub fn potato() -> Self {
        Self {
            max_triangles: 2_000,
            frame_budget_ms: 50.0, // 20fps budget
            max_texture_size: 256,
            ambient_intensity: 0.4,
            ..Default::default()
        }
    }

    /// Low preset: specular lighting, fog, basic textures.
    pub fn low() -> Self {
        Self {
            specular_enabled: true,
            fog_enabled: true,
            textures_enabled: true,
            max_texture_size: 512,
            max_triangles: 20_000,
            frame_budget_ms: 33.0,
            ambient_intensity: 0.25,
            ..Default::default()
        }
    }

    /// Medium preset: GPU rendering, shadows, bloom, FXAA.
    pub fn medium() -> Self {
        Self {
            use_gpu: true,
            specular_enabled: true,
            max_point_lights: 4,
            shadows_enabled: true,
            shadow_resolution: 1024,
            bloom_enabled: true,
            bloom_intensity: 0.3,
            fog_enabled: true,
            anti_aliasing: AntiAliasingMode::Fxaa,
            textures_enabled: true,
            max_texture_size: 1024,
            pbr_enabled: true,
            max_triangles: 100_000,
            frame_budget_ms: 16.6,
            ambient_intensity: 0.15,
            ..Default::default()
        }
    }

    /// High preset: everything on, RTX-ready.
    pub fn high() -> Self {
        Self {
            use_gpu: true,
            specular_enabled: true,
            max_point_lights: 16,
            shadows_enabled: true,
            shadow_resolution: 2048,
            bloom_enabled: true,
            bloom_intensity: 0.4,
            ssao_enabled: true,
            ssao_radius: 0.5,
            fog_enabled: true,
            anti_aliasing: AntiAliasingMode::Msaa4x,
            textures_enabled: true,
            max_texture_size: 2048,
            pbr_enabled: true,
            reflections_enabled: true,
            raytrace_enabled: false, // Requires explicit RTX opt-in
            gpu_deform_enabled: true,
            max_triangles: 500_000,
            frame_budget_ms: 16.6,
            ambient_intensity: 0.1,
            ..Default::default()
        }
    }

    /// Check if any GPU-dependent feature is enabled.
    pub fn requires_gpu(&self) -> bool {
        self.use_gpu
            || self.shadows_enabled
            || self.ssao_enabled
            || self.pbr_enabled
            || self.reflections_enabled
            || self.raytrace_enabled
            || self.gpu_deform_enabled
    }

    /// Count how many expensive features are active (for status display).
    pub fn active_feature_count(&self) -> u32 {
        let mut count = 0;
        if self.use_gpu { count += 1; }
        if self.specular_enabled { count += 1; }
        if self.max_point_lights > 0 { count += 1; }
        if self.shadows_enabled { count += 1; }
        if self.bloom_enabled { count += 1; }
        if self.ssao_enabled { count += 1; }
        if self.fog_enabled { count += 1; }
        if self.anti_aliasing != AntiAliasingMode::None { count += 1; }
        if self.textures_enabled { count += 1; }
        if self.pbr_enabled { count += 1; }
        if self.reflections_enabled { count += 1; }
        if self.raytrace_enabled { count += 1; }
        if self.gpu_deform_enabled { count += 1; }
        count
    }
}
