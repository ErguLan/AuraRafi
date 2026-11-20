//! Metrics for a composed native editor frame.

use crate::api_graphic_basic::ui_surface::DirectUiSurfaceFrame;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EditorCompositorMetrics {
    pub command_submits: u32,
    pub canvas_layers: u32,
    pub ui_layers: u32,
    pub presents: u32,
}

pub struct EditorComposedFrame {
    pub ui_layers: Vec<DirectUiSurfaceFrame>,
    pub metrics: EditorCompositorMetrics,
}
