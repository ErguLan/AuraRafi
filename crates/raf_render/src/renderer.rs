//! Core renderer abstraction.
//!
//! The MVP renderer is intentionally minimal: it sets up wgpu, clears the
//! viewport with the configured background color, and provides hooks for
//! the editor viewport to draw into.

use raf_core::config::RenderQuality;

use crate::pipeline::RenderPipeline;

/// Viewport clear color (matches AuraRafi dark theme).
pub const CLEAR_COLOR: wgpu::Color = wgpu::Color {
    r: 0.08,
    g: 0.08,
    b: 0.12,
    a: 1.0,
};

/// High-level renderer holding wgpu state and pipeline config.
pub struct Renderer {
    /// Current render pipeline configuration.
    pub pipeline: RenderPipeline,
}

impl Renderer {
    /// Create a new renderer with the specified quality level.
    pub fn new(quality: RenderQuality) -> Self {
        Self {
            pipeline: RenderPipeline::for_quality(quality),
        }
    }

    /// Update the quality level at runtime (e.g., from settings panel).
    pub fn set_quality(&mut self, quality: RenderQuality) {
        self.pipeline = RenderPipeline::for_quality(quality);
        tracing::info!("Render quality changed to {:?}", quality);
    }
}
