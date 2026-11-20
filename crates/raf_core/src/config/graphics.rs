//! Rendering quality, viewport mode, and render execution enums.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Viewport Render Mode
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewportRenderMode {
    Solid,
    /// Removed from the editor UI. Kept only so older RON settings can still
    /// be read and migrated to `Solid` without resetting the full config.
    Wireframe,
    /// Removed from the editor UI. Kept only for the same migration path as
    /// [`ViewportRenderMode::Wireframe`].
    Preview,
}

impl ViewportRenderMode {
    /// Returns the only viewport mode currently supported by the editor.
    pub const fn normalized(self) -> Self {
        Self::Solid
    }
}

impl Default for ViewportRenderMode {
    fn default() -> Self {
        Self::Solid
    }
}

// ---------------------------------------------------------------------------
// Render quality
// ---------------------------------------------------------------------------

/// Render quality presets (0 = potato, 3 = high-end future RTX).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderQuality {
    /// No shadows, no post-processing. Maximum performance.
    Potato = 0,
    /// Basic shadows, simple ambient occlusion.
    Low = 1,
    /// Improved shadows, bloom, anti-aliasing.
    Medium = 2,
    /// Full quality. Future: RTX/ray tracing.
    High = 3,
}

impl Default for RenderQuality {
    fn default() -> Self {
        Self::Low
    }
}

// ---------------------------------------------------------------------------
// Render execution policy
// ---------------------------------------------------------------------------

/// Global render execution policy for the current machine.
///
/// This selects how the engine prefers to run render work in editor and
/// runtime surfaces. Project settings may still enable heavier GPU-only
/// features, but the base backend preference lives here so the whole app
/// follows one consistent policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderExecutionPolicy {
    /// Prefer GPU when possible and fall back to CPU automatically.
    Auto,
    /// Force CPU rendering for maximum compatibility.
    CpuOnly,
    /// Prefer GPU-backed rendering for interactive surfaces.
    GpuPreferred,
}

impl Default for RenderExecutionPolicy {
    fn default() -> Self {
        Self::Auto
    }
}

/// v0.7.0: Render quality presets that map to RenderConfig defaults.
/// These are stored in EngineSettings. The actual feature toggles
/// live in raf_render::RenderConfig and are per-project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderPreset {
    /// Everything off. Maximum compatibility. Default.
    Potato,
    /// Specular + fog + basic textures.
    Low,
    /// GPU + shadows + bloom + FXAA.
    Medium,
    /// Everything on (except raytracing).
    High,
}

impl Default for RenderPreset {
    fn default() -> Self {
        Self::Potato
    }
}
