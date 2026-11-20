//! Native editor frame composition owned by ApiGraphicBasic.
//!
//! Scene/CAD canvases and retained RafUI chrome are encoded into one final
//! presentation command buffer. Upper editor layers provide owned outputs and
//! logical rectangles without importing WGPU types.

mod compose;
mod layers;
mod metrics;

pub use compose::NativeEditorCompositor;
pub use layers::{EditorCanvasLayer, EditorUiLayer};
pub use metrics::{EditorComposedFrame, EditorCompositorMetrics};
