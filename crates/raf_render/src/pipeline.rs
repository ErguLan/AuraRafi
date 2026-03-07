//! Render pipeline configuration and quality levels.

use raf_core::config::RenderQuality;
use serde::{Deserialize, Serialize};

/// Configuration for the render pipeline at a given quality level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderPipeline {
    pub quality: RenderQuality,
    pub shadows_enabled: bool,
    pub shadow_resolution: u32,
    pub ambient_occlusion: bool,
    pub bloom: bool,
    pub anti_aliasing: AntiAliasing,
    pub max_draw_calls: u32,
}

/// Anti-aliasing modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AntiAliasing {
    None,
    FXAA,
    MSAA4x,
}

impl RenderPipeline {
    /// Create a pipeline configuration for the given quality level.
    pub fn for_quality(quality: RenderQuality) -> Self {
        match quality {
            RenderQuality::Potato => Self {
                quality,
                shadows_enabled: false,
                shadow_resolution: 0,
                ambient_occlusion: false,
                bloom: false,
                anti_aliasing: AntiAliasing::None,
                max_draw_calls: 500,
            },
            RenderQuality::Low => Self {
                quality,
                shadows_enabled: true,
                shadow_resolution: 512,
                ambient_occlusion: false,
                bloom: false,
                anti_aliasing: AntiAliasing::None,
                max_draw_calls: 2000,
            },
            RenderQuality::Medium => Self {
                quality,
                shadows_enabled: true,
                shadow_resolution: 1024,
                ambient_occlusion: true,
                bloom: true,
                anti_aliasing: AntiAliasing::FXAA,
                max_draw_calls: 5000,
            },
            RenderQuality::High => Self {
                quality,
                shadows_enabled: true,
                shadow_resolution: 2048,
                ambient_occlusion: true,
                bloom: true,
                anti_aliasing: AntiAliasing::MSAA4x,
                max_draw_calls: 10000,
            },
        }
    }
}
