//! Workbench frame metrics, capacity estimates and graphics ledger readout.

use super::*;

impl NativeGameWorkbench {
    pub fn set_performance_metrics(
        &mut self,
        presented_fps: f32,
        viewport_fps: f32,
        frame_cpu_ms: f32,
        capacity_cpu_ms: f32,
        target_fps: u16,
        requested_target_fps: u16,
        p95_frame_time_ms: f32,
        hitch_count: u64,
        scene_metrics: SceneFrameMetrics,
        presentation_label: &'static str,
        frame_activity_label: &'static str,
        redraw_reason_label: &'static str,
        canvas_status_label: &'static str,
        camera_navigation_active: bool,
    ) {
        // Reformatting FPS/CPU text every frame during camera movement forces
        // the retained workbench surface to resubmit its resources. Freeze the
        // readout for the gesture; it refreshes as soon as navigation settles.
        if camera_navigation_active && self.performance_initialized {
            return;
        }
        self.performance_initialized = true;
        self.presented_fps = presented_fps.max(0.0);
        self.estimated_capacity_fps = estimated_capacity_fps(
            capacity_cpu_ms,
            scene_metrics
                .gpu_timing_sampled
                .then_some(scene_metrics.gpu_frame_ms),
        );
        self.viewport_fps = viewport_fps.max(0.0);
        self.frame_cpu_ms = frame_cpu_ms.max(0.0);
        self.target_fps = target_fps;
        self.requested_target_fps = requested_target_fps;
        self.p95_frame_time_ms = p95_frame_time_ms.max(0.0);
        self.hitch_count = hitch_count;
        self.frame_gpu_ms = scene_metrics.gpu_frame_ms.max(0.0);
        self.frame_gpu_timing_sampled = scene_metrics.gpu_timing_sampled;
        self.scene_draw_calls = scene_metrics.total_draw_calls();
        self.scene_upload_bytes = scene_metrics.frame_upload_bytes;
        self.scene_upload_budget_exceeded = scene_metrics.upload_budget_exceeded;
        self.shadow_metrics = scene_metrics;
        self.sync_performance_readout(scene_metrics);
        self.presentation_label = presentation_label;
        self.frame_activity_label = frame_activity_label;
        self.redraw_reason_label = redraw_reason_label;
        self.canvas_status_label = canvas_status_label;
    }

    /// Projects the graphics ledger into the cached status segment.
    ///
    /// The projection is quantized to the displayed precision, so the compact
    /// string is rebuilt only when a visible value changes. Sub-visible byte
    /// drift cannot invalidate status text or trigger paint work.
    fn sync_performance_readout(&mut self, metrics: SceneFrameMetrics) {
        let readout = performance_pressure::from_metrics(
            &metrics,
            ledger_budget_bytes(self.agent_settings.render_execution_policy),
        );
        if readout == self.performance_readout && !self.last_performance_segment.is_empty() {
            return;
        }
        // The inspector consumes the whole readout, but the compact status
        // string only displays residency, budget and upload rejections. Do not
        // reformat it when a peak or historical rejection changes.
        let segment_changed = self.last_performance_segment.is_empty()
            || readout.resident_mb != self.performance_readout.resident_mb
            || readout.budget_mb != self.performance_readout.budget_mb
            || readout.upload_rejected != self.performance_readout.upload_rejected;
        self.performance_readout = readout;
        if segment_changed {
            self.last_performance_segment = performance_pressure::status_segment(&readout);
        }
        performance_pressure::publish(readout);
    }
}

/// Estimates raw frame capacity from measured work time, independently from
/// the FPS that the window actually presents through VSync.
///
/// A missing GPU sample is intentionally ignored. The CPU sample still gives
/// the HUD a useful estimate without pretending that the GPU was measured.
pub(super) fn estimated_capacity_fps(frame_cpu_ms: f32, frame_gpu_ms: Option<f32>) -> u32 {
    let cpu_ms = if frame_cpu_ms.is_finite() && frame_cpu_ms > f32::EPSILON {
        frame_cpu_ms
    } else {
        0.0
    };
    let gpu_ms = frame_gpu_ms
        .filter(|value| value.is_finite() && *value > f32::EPSILON)
        .unwrap_or(0.0);
    let bottleneck_ms = cpu_ms.max(gpu_ms);
    if bottleneck_ms <= f32::EPSILON {
        return 0;
    }
    (1000.0 / bottleneck_ms)
        .round()
        .clamp(1.0, MAX_ESTIMATED_CAPACITY_FPS) as u32
}

/// Total graphics ledger budget carried by the active render policy.
///
/// The render runtime resolves the same policy through
/// `BasicDeviceConfig::from_render_policy`, so reusing that mapping keeps the
/// editor readout honest instead of guessing a fixed budget.
pub(super) fn ledger_budget_bytes(policy: RenderExecutionPolicy) -> u64 {
    BasicDeviceConfig::from_render_policy(policy)
        .memory_budget
        .gpu_bytes
}
