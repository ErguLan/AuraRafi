//! Viewport bridge layer.
//!
//! Keeps editor-facing viewport orchestration out of UI surface builders.

pub mod editor_camera;
pub mod gizmo_renderer;
pub mod input_handler;
pub mod picking_policy;
pub mod render_runtime;
pub mod transform_controller;
pub mod viewport_bridge;
pub mod viewport_input;

pub use editor_camera::{EditorCameraBlock, EditorCameraBookmark, EditorCameraMode};
pub use gizmo_renderer::GizmoRenderSpec;
pub use input_handler::{
    ProjectedEditEdge, ProjectedEditOverlay, ProjectedEditVertex, ViewportEditSession,
};
pub use picking_policy::{
    IdBufferSpec, PickingLayer, PickingLayerMask, PickingPolicy, PickingPriority, PickingTier,
};
pub use render_runtime::{GraphicsSurfaceKind, RenderRuntime, RenderRuntimeSnapshot};
pub use transform_controller::{AxisDragOutcome, ViewportTransformController};
pub use viewport_bridge::{ViewportBridge, ViewportNavigationConfig, ViewportPointerInput};
pub use viewport_input::{
    try_capture_camera, try_capture_gizmo, try_capture_viewport_tool, ViewportInputFrame,
    ViewportInputRect,
};
