//! Metrics for a composed native editor frame.

use crate::api_graphic_basic::ui_surface::DirectUiSurfaceFrame;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EditorCompositorMetrics {
    pub command_submits: u32,
    pub canvas_layers: u32,
    pub ui_layers: u32,
    pub presents: u32,
    pub shadow_resolution: u32,
    pub shadow_resident_bytes: u64,
    pub shadow_draw_calls: u32,
    pub shadows_unsupported: bool,
    pub shadow_budget_denied: bool,
}

impl EditorCompositorMetrics {
    pub(super) fn record_draw(&mut self, draw: raf_agb::FrameMetrics) {
        self.command_submits = draw.submits;
        self.shadow_resolution = draw.shadow_resolution;
        self.shadow_resident_bytes = draw.shadow_resident_bytes;
        self.shadow_draw_calls = draw.shadow_draw_calls;
        self.shadows_unsupported = draw.shadows_unsupported;
        self.shadow_budget_denied = draw.shadow_budget_denied;
    }
}

pub struct EditorComposedFrame {
    pub ui_layers: Vec<DirectUiSurfaceFrame>,
    pub metrics: EditorCompositorMetrics,
}
