//! Viewport bridge layer.
//!
//! Keeps editor-facing viewport orchestration out of the egui panel itself.

pub mod input_handler;
pub mod render_runtime;
pub mod transform_controller;
pub mod viewport_bridge;

pub use input_handler::{
    ProjectedEditEdge, ProjectedEditOverlay, ProjectedEditVertex, ViewportEditSession,
};
pub use render_runtime::{GraphicsSurfaceKind, RenderRuntime, RenderRuntimeSnapshot};
pub use transform_controller::ViewportTransformController;
pub use viewport_bridge::{ViewportBridge, ViewportNavigationConfig, ViewportPointerInput};
