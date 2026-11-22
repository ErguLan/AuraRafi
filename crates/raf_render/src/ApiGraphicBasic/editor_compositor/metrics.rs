//! Metrics for a composed native editor frame.

use crate::api_graphic_basic::ui_surface::DirectUiSurfaceFrame;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EditorCompositorMetrics {
    pub command_submits: u32,
    pub canvas_layers: u32,
    pub ui_layers: u32,
    pub presents: u32,
    /// True when the frame used two retained packets and one layered draw.
    pub layered: bool,
    /// Number of retained packets recorded into the target (1 or 2).
    pub layer_count: u32,
    /// Latest prepared scene packet revision. Zero before the first scene.
    pub scene_packet_revision: u64,
    /// Latest prepared UI packet revision. Zero before the first UI build.
    pub ui_packet_revision: u64,
    /// Scene packet rebuilds performed by this frame (0 or 1).
    pub scene_packet_rebuilds: u32,
    /// UI packet rebuilds performed by this frame (0 or 1).
    pub ui_packet_rebuilds: u32,
    pub shadow_resolution: u32,
    pub shadow_resident_bytes: u64,
    pub shadow_draw_calls: u32,
    pub shadows_unsupported: bool,
    pub shadow_budget_denied: bool,
}

impl EditorCompositorMetrics {
    /// Resets the fields that describe only the frame being composed. Packet
    /// revisions, layer counts and shadow residency stay as the latest truth.
    pub(super) fn begin_frame(&mut self) {
        self.command_submits = 0;
        self.scene_packet_rebuilds = 0;
        self.ui_packet_rebuilds = 0;
    }

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
