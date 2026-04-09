//! Render backend selector.
//!
//! Allows the engine to switch between:
//! - CPU painter (egui shapes, zero GPU, runs on anything)
//! - GPU pipeline (wgpu, for when scenes get heavy)
//!
//! Default is CPU. The switch is cheap: just a flag + different code paths.
//! No GPU resources are allocated until the user explicitly switches to GPU mode.
//! This prevents the "Halo Slipspace" problem: engine that cannot scale.

use serde::{Deserialize, Serialize};
use raf_core::config::Language;
use raf_core::i18n::t;

// ---------------------------------------------------------------------------
// Backend enum
// ---------------------------------------------------------------------------

/// Which rendering backend to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderBackend {
    /// CPU-based rendering via egui painter (projection math + shape drawing).
    /// Zero GPU memory, zero shaders, zero buffers.
    /// Default for potato PCs and low-resource mode.
    CpuPainter,

    /// GPU-based rendering via wgpu (vertex buffers, shaders, proper pipeline).
    /// Uses VRAM but handles large scenes (500+ entities) without CPU bottleneck.
    /// Only activates when user explicitly selects it or entity count is high.
    GpuWgpu,
}

impl Default for RenderBackend {
    fn default() -> Self {
        // Always default to CPU - lightest option, runs on anything.
        Self::CpuPainter
    }
}

impl RenderBackend {
    /// Display label for the UI.
    pub fn label(&self, lang: Language) -> String {
        match self {
            Self::CpuPainter => t("render.backend.cpu", lang),
            Self::GpuWgpu => t("render.backend.gpu", lang),
        }
    }

    /// Whether this backend uses GPU resources.
    pub fn uses_gpu(&self) -> bool {
        matches!(self, Self::GpuWgpu)
    }
}

// ---------------------------------------------------------------------------
// Backend config
// ---------------------------------------------------------------------------

/// Configuration for the render backend, controlling resource usage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendConfig {
    /// Active backend.
    pub backend: RenderBackend,

    /// Entity count threshold: auto-suggest GPU when scene exceeds this.
    /// Does NOT auto-switch - only shows a hint to the user.
    /// Set to 0 to disable the suggestion.
    pub gpu_suggest_threshold: usize,

    /// Maximum triangles per frame for CPU backend before dropping detail.
    /// Beyond this, LOD is forced to minimum and distant entities are culled.
    /// This prevents the CPU painter from choking on heavy scenes.
    pub cpu_max_triangles: usize,

    /// Whether to show the backend selector in the UI.
    /// When false, the engine just uses CPU silently.
    pub show_selector: bool,

    /// Target FPS floor. If framerate drops below this with CPU backend,
    /// show a performance warning (not auto-switch).
    pub target_fps_floor: f32,

    /// Frame time budget in milliseconds for rendering.
    /// CPU painter should finish within this budget per frame.
    /// If consistently exceeded, the engine reduces detail automatically.
    pub frame_budget_ms: f32,
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            backend: RenderBackend::CpuPainter,
            gpu_suggest_threshold: 300, // Suggest GPU at 300+ entities
            cpu_max_triangles: 5000,    // ~200 low-poly objects max on CPU
            show_selector: true,
            target_fps_floor: 30.0,
            frame_budget_ms: 16.0,      // 60fps budget
        }
    }
}

impl BackendConfig {
    /// Check if the current entity count suggests switching to GPU.
    /// Returns true if entity_count > threshold and we're on CPU.
    /// Does NOT switch automatically - just a hint for the UI.
    pub fn should_suggest_gpu(&self, entity_count: usize) -> bool {
        self.backend == RenderBackend::CpuPainter
            && self.gpu_suggest_threshold > 0
            && entity_count > self.gpu_suggest_threshold
    }

    /// Calculate effective max triangles based on backend.
    pub fn effective_max_tris(&self) -> usize {
        match self.backend {
            RenderBackend::CpuPainter => self.cpu_max_triangles,
            RenderBackend::GpuWgpu => self.cpu_max_triangles * 20, // GPU can handle 20x more
        }
    }

    /// Estimate memory usage for the current backend configuration.
    /// Returns approximate bytes of VRAM/RAM used by the renderer itself.
    pub fn estimated_mem_bytes(&self) -> usize {
        match self.backend {
            // CPU painter: only the shape list in RAM. ~64 bytes per shape.
            RenderBackend::CpuPainter => self.cpu_max_triangles * 64,
            // GPU: vertex buffer + index buffer + uniform buffer.
            // ~48 bytes/vertex, ~12 bytes/index, ~256 bytes uniforms.
            RenderBackend::GpuWgpu => {
                let verts = self.cpu_max_triangles * 3; // 3 verts per tri
                verts * 48 + self.cpu_max_triangles * 12 + 256
            }
        }
    }

    /// Potato-optimized config: absolute minimum resource usage.
    pub fn potato() -> Self {
        Self {
            backend: RenderBackend::CpuPainter,
            gpu_suggest_threshold: 0, // Never suggest GPU
            cpu_max_triangles: 2000,
            show_selector: false,
            target_fps_floor: 20.0,
            frame_budget_ms: 33.0, // 30fps budget
        }
    }
}

// ---------------------------------------------------------------------------
// Frame stats (for adaptive quality)
// ---------------------------------------------------------------------------

/// Lightweight per-frame render statistics.
/// Used to detect when the engine needs to reduce detail.
/// Only tracks what's needed - no profiling overhead.
#[derive(Debug, Clone, Default)]
pub struct FrameRenderStats {
    /// Triangles drawn this frame.
    pub triangles_drawn: usize,
    /// Entities rendered this frame (after culling).
    pub entities_rendered: usize,
    /// Entities culled (LOD or distance).
    pub entities_culled: usize,
    /// Time spent in rendering (milliseconds).
    pub render_time_ms: f32,
    /// Whether the frame exceeded the budget.
    pub over_budget: bool,
}

impl FrameRenderStats {
    /// Check if we should reduce detail next frame.
    pub fn should_reduce_detail(&self, config: &BackendConfig) -> bool {
        self.over_budget || self.render_time_ms > config.frame_budget_ms * 1.5
    }

    /// Check if we can increase detail next frame.
    pub fn can_increase_detail(&self, config: &BackendConfig) -> bool {
        self.render_time_ms < config.frame_budget_ms * 0.5
            && self.triangles_drawn < config.effective_max_tris() / 2
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
        let config = BackendConfig::default();
        assert_eq!(config.backend, RenderBackend::CpuPainter);
        assert!(!config.backend.uses_gpu());
    }

    #[test]
    fn suggests_gpu_correctly() {
        let config = BackendConfig::default();
        assert!(!config.should_suggest_gpu(100));
        assert!(config.should_suggest_gpu(400));
    }

    #[test]
    fn potato_config() {
        let config = BackendConfig::potato();
        assert_eq!(config.cpu_max_triangles, 2000);
        assert!(!config.show_selector);
    }

    #[test]
    fn gpu_handles_more_tris() {
        let mut config = BackendConfig::default();
        let cpu_tris = config.effective_max_tris();
        config.backend = RenderBackend::GpuWgpu;
        let gpu_tris = config.effective_max_tris();
        assert!(gpu_tris > cpu_tris);
    }
}
