//! Native scene handoff.
//!
//! In native mode the device retains the frame and returns an `Arc` handle. The
//! AGB compositor owns the graphics device, allocates targets, uploads resources
//! and presents. Nothing in this module touches a graphics API.

use std::sync::Arc;

use crate::scene_renderer::SceneRenderFrame;

use super::backend_cpu::{capture_framebuffer_rgba, render_scene_frame_cpu};
use super::{BasicDevice, SceneFrameCapture, SceneFrameMetrics, SceneFrameOutput};

impl BasicDevice {
    pub(super) fn execute_native_scene_frame(
        &mut self,
        frame: &SceneRenderFrame,
    ) -> SceneFrameOutput {
        let retained = Arc::new(frame.clone());
        self.last_frame_metrics = native_handoff_metrics(frame);
        self.last_scene_frame = Some(Arc::clone(&retained));
        SceneFrameOutput::Scene(retained)
    }

    /// Inspect the retained frame with the CPU rasterizer.
    ///
    /// The capture never reads a graphics target and never mutates the retained
    /// frame; it is the same recovery rasterizer the product uses when no
    /// native API is available.
    pub(super) fn capture_native_scene_rgba(&self) -> Result<SceneFrameCapture, String> {
        let frame = self.last_scene_frame.as_ref().ok_or_else(|| {
            "The graphics device has not retained a native scene frame yet.".to_string()
        })?;
        let mut scratch = self.native_capture_scratch.borrow_mut();
        let scratch = &mut *scratch;
        // Capture must not revise the metrics of the frame the compositor drew.
        let _ = render_scene_frame_cpu(
            frame,
            &mut scratch.framebuffer,
            &mut scratch.filter_scratch,
            self.memory_budget,
        );
        capture_framebuffer_rgba(&scratch.framebuffer)
    }
}

/// Diagnostics for a frame the compositor will draw.
///
/// The scene device allocates, uploads and submits nothing in native mode, so
/// only the recorded command counts are reported.
pub fn native_handoff_metrics(frame: &SceneRenderFrame) -> SceneFrameMetrics {
    let stats = frame.commands.stats();
    SceneFrameMetrics {
        command_count: stats.command_count,
        mesh_instances: stats.mesh_instances,
        line_instances: stats.line_instances,
        overlay_triangles: stats.overlay_triangles,
        shadow_budget_exceeded: frame.commands.shadow_budget_exceeded,
        ..SceneFrameMetrics::default()
    }
}
