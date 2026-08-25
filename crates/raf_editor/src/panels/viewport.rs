//! Public native Game viewport boundary.
//!
//! The renderer owns the canvas; this module preserves the panel-level names
//! used by older editor integrations while delegating interaction to the
//! backend-neutral controller.

pub use super::viewport_controller::{
    NativeGameViewportController as ViewportPanel, NativeGameViewportController,
    NativeViewportRenderStyle, NativeViewportUpdate,
};
pub use super::viewport_surface_host::{
    frame_key, ViewportFrameKey, ViewportHostMode, ViewportPresentationBackend,
    ViewportPresentationStats, ViewportSurfaceCacheStats, ViewportSurfaceHost, ViewportSurfacePlan,
};
