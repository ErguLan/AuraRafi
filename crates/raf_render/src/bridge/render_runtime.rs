//! Shared graphics runtime state for editor surfaces.
//!
//! Owns the lightweight `ApiGraphicBasic` scene device lifecycle so interactive
//! surfaces can lazily request recovery rasterization without forcing any
//! graphics startup on the hub or settings screens. Native policy never opens a
//! scene device: the AGB compositor owns the device and consumes the retained
//! frame this runtime hands over.

use std::sync::Arc;

use raf_core::config::RenderExecutionPolicy;

use crate::api_graphic_basic::device::{
    capture_scene_frame_cpu, native_handoff_metrics, BasicBackendType, BasicDevice,
    BasicDeviceConfig, SceneFrameCapture, SceneFrameMetrics, SceneFrameOutput,
};
use crate::api_graphic_basic::{
    FrameInvalidation, FramePacingProfile, FramePermit, FrameScheduler, FrameSchedulerMetrics,
    GraphicsBackendId, GraphicsCapabilities, GraphicsMemoryBudget,
};
use crate::scene_renderer::SceneRenderFrame;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphicsSurfaceKind {
    None,
    SceneViewport,
    SchematicCanvas,
    PcbCanvas,
}

impl Default for GraphicsSurfaceKind {
    fn default() -> Self {
        Self::None
    }
}

impl GraphicsSurfaceKind {
    pub fn requires_graphics_device(&self) -> bool {
        !matches!(self, Self::None)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderRuntimeSnapshot {
    pub surface: GraphicsSurfaceKind,
    pub policy: RenderExecutionPolicy,
    pub active_backend: Option<BasicBackendType>,
    pub backend_id: Option<GraphicsBackendId>,
    pub capabilities: Option<GraphicsCapabilities>,
    pub memory_budget: GraphicsMemoryBudget,
    /// Changes whenever the backing ApiGraphicBasic device is recreated.
    pub device_generation: u64,
    /// Changes whenever ownership of the shared render target moves between
    /// editor surfaces. The device can stay alive while its target contains
    /// another tab's frame, so caches must not key presentation on the device
    /// generation alone.
    pub surface_generation: u64,
    pub advanced_gpu_features_allowed: bool,
    pub last_frame_metrics: SceneFrameMetrics,
    pub pacing_profile: FramePacingProfile,
    pub scheduler_metrics: FrameSchedulerMetrics,
}

impl Default for RenderRuntimeSnapshot {
    fn default() -> Self {
        Self {
            surface: GraphicsSurfaceKind::None,
            policy: RenderExecutionPolicy::Auto,
            active_backend: None,
            backend_id: None,
            capabilities: None,
            memory_budget: GraphicsMemoryBudget::default(),
            device_generation: 0,
            surface_generation: 0,
            advanced_gpu_features_allowed: false,
            last_frame_metrics: SceneFrameMetrics::default(),
            pacing_profile: FramePacingProfile::Eco,
            scheduler_metrics: FrameSchedulerMetrics::default(),
        }
    }
}

impl RenderRuntimeSnapshot {
    /// Whether a graphics API draws the active surface.
    pub fn is_gpu_active(&self) -> bool {
        self.active_backend
            .is_some_and(|backend| backend.is_gpu_active())
    }

    /// Compact diagnostic badge naming the active AGB API.
    pub fn status_badge(&self) -> String {
        match self.active_backend {
            Some(BasicBackendType::Native(api)) if self.advanced_gpu_features_allowed => {
                format!("GFX AGB {api}+")
            }
            Some(BasicBackendType::Native(api)) => format!("GFX AGB {api}"),
            Some(BasicBackendType::CpuSoftware) => "GFX CPU".to_string(),
            None if self.surface.requires_graphics_device() => "GFX init".to_string(),
            None => "GFX idle".to_string(),
        }
    }
}

pub struct RenderRuntime {
    policy: RenderExecutionPolicy,
    advanced_gpu_features_allowed: bool,
    surface: GraphicsSurfaceKind,
    surface_generation: u64,
    device: Option<BasicDevice>,
    device_generation: u64,
    /// Most recent frame handed to the compositor in native mode.
    last_native_frame: Option<Arc<SceneRenderFrame>>,
    /// Diagnostics for the most recent native handoff.
    native_metrics: SceneFrameMetrics,
    scheduler: FrameScheduler,
    memory_budget: Option<GraphicsMemoryBudget>,
}

impl Default for RenderRuntime {
    fn default() -> Self {
        Self {
            policy: RenderExecutionPolicy::Auto,
            advanced_gpu_features_allowed: false,
            surface: GraphicsSurfaceKind::None,
            surface_generation: 0,
            device: None,
            device_generation: 0,
            last_native_frame: None,
            native_metrics: SceneFrameMetrics::default(),
            scheduler: FrameScheduler::default(),
            memory_budget: None,
        }
    }
}

impl RenderRuntime {
    /// Optional surface-specific cap, still enforced by ApiGraphicBasic.
    pub fn set_memory_budget(&mut self, budget: GraphicsMemoryBudget) {
        if self.memory_budget == Some(budget) {
            return;
        }
        self.memory_budget = Some(budget);
        self.device = None;
    }

    pub fn configure(
        &mut self,
        policy: RenderExecutionPolicy,
        advanced_gpu_features_allowed: bool,
    ) {
        let policy_changed = self.policy != policy;
        let features_changed = self.advanced_gpu_features_allowed != advanced_gpu_features_allowed;
        if !policy_changed && !features_changed {
            return;
        }

        self.policy = policy;
        self.advanced_gpu_features_allowed = advanced_gpu_features_allowed;
        self.device = None;
        self.last_native_frame = None;
        self.native_metrics = SceneFrameMetrics::default();
        self.scheduler.request(FrameInvalidation::EXPLICIT);
    }

    pub fn activate_surface(&mut self, surface: GraphicsSurfaceKind) {
        if self.surface == surface {
            if surface.requires_graphics_device() {
                self.ensure_device();
            } else {
                self.device = None;
            }
            return;
        }

        self.surface = surface;
        self.surface_generation = self.surface_generation.wrapping_add(1);
        self.scheduler.request(FrameInvalidation::WINDOW);
        if surface.requires_graphics_device() {
            self.ensure_device();
        } else {
            self.device = None;
        }
    }

    pub fn snapshot(&self) -> RenderRuntimeSnapshot {
        let active_backend = self.reported_backend();
        RenderRuntimeSnapshot {
            surface: self.surface,
            policy: self.policy,
            active_backend,
            backend_id: active_backend.map(|backend| backend.id()),
            capabilities: active_backend.map(|backend| backend.capabilities()),
            memory_budget: self
                .device
                .as_ref()
                .map(|device| device.memory_budget())
                .unwrap_or_else(|| self.device_config().memory_budget),
            device_generation: self.device_generation,
            surface_generation: self.surface_generation,
            advanced_gpu_features_allowed: self.advanced_gpu_features_allowed,
            last_frame_metrics: self
                .device
                .as_ref()
                .map(|device| device.last_frame_metrics())
                .unwrap_or(self.native_metrics),
            pacing_profile: self.scheduler.profile(),
            scheduler_metrics: self.scheduler.metrics(),
        }
    }

    pub fn scheduler(&self) -> &FrameScheduler {
        &self.scheduler
    }

    pub fn scheduler_mut(&mut self) -> &mut FrameScheduler {
        &mut self.scheduler
    }

    pub fn set_frame_pacing_profile(&mut self, profile: FramePacingProfile) {
        self.scheduler.set_profile(profile);
    }

    pub fn set_frame_limit(&mut self, fps_limit: u32) {
        self.scheduler.set_frame_limit(fps_limit);
    }

    pub fn set_present_refresh_hz(&mut self, refresh_hz: Option<u16>) {
        self.scheduler.set_present_refresh_hz(refresh_hz);
    }

    pub fn request_frame(&mut self, reason: FrameInvalidation) {
        self.scheduler.request(reason);
    }

    pub fn set_continuous_frame_reason(&mut self, reason: FrameInvalidation, active: bool) {
        self.scheduler.set_continuous(reason, active);
    }

    pub fn next_frame(&mut self, now_seconds: f64) -> Option<FramePermit> {
        self.scheduler.request_frame(now_seconds)
    }

    pub fn finish_frame(
        &mut self,
        permit: FramePermit,
        presented_at_seconds: f64,
        frame_cpu_ms: f32,
        frame_gpu_ms: f32,
        total_cpu_ms: f32,
    ) {
        self.scheduler.finish_frame(
            permit,
            presented_at_seconds,
            frame_cpu_ms,
            frame_gpu_ms,
            total_cpu_ms,
        );
    }

    pub fn render_scene_frame(&mut self, frame: &SceneRenderFrame) -> SceneFrameOutput {
        match self.device_config().resolved_backend() {
            BasicBackendType::Native(_) => {
                // The compositor owns the device, its targets and presentation.
                // This runtime only retains and returns the frame.
                self.device = None;
                let retained = Arc::new(frame.clone());
                self.native_metrics = native_handoff_metrics(frame);
                self.last_native_frame = Some(Arc::clone(&retained));
                SceneFrameOutput::Scene(retained)
            }
            BasicBackendType::CpuSoftware => {
                self.last_native_frame = None;
                self.native_metrics = SceneFrameMetrics::default();
                self.ensure_device();
                self.device
                    .as_mut()
                    .map(|device| device.execute_scene_frame(frame))
                    .unwrap_or_else(|| SceneFrameOutput::CpuPixels(Vec::new()))
            }
        }
    }

    /// Capture the last rendered scene frame for Agent perception or an
    /// explicit evidence artifact. It never presents or mutates the scene.
    ///
    /// Native handoffs are re-rasterized with the CPU recovery path, which is
    /// the same pixel contract the product uses when no native API is present.
    pub fn capture_last_scene_rgba(&self) -> Result<SceneFrameCapture, String> {
        if let Some(device) = self.device.as_ref() {
            return device.capture_last_scene_rgba();
        }
        let frame = self.last_native_frame.as_ref().ok_or_else(|| {
            "The graphics device has not rendered a scene frame yet.".to_string()
        })?;
        capture_scene_frame_cpu(frame, self.device_config().memory_budget)
    }

    fn device_config(&self) -> BasicDeviceConfig {
        let mut config = BasicDeviceConfig::from_render_policy(self.policy);
        if let Some(budget) = self.memory_budget {
            config.memory_budget = budget;
        }
        config
    }

    /// Backend reported to the editor for the active surface.
    ///
    /// A native handoff has no scene device, so its backend comes from policy.
    /// A CPU surface still reports nothing until its device is opened, which
    /// keeps the "GFX init" state meaningful.
    fn reported_backend(&self) -> Option<BasicBackendType> {
        if let Some(device) = self.device.as_ref() {
            return Some(device.backend());
        }
        if !self.surface.requires_graphics_device() {
            return None;
        }
        let backend = self.device_config().resolved_backend();
        backend.is_native().then_some(backend)
    }

    fn ensure_device(&mut self) {
        if self.device.is_some() {
            return;
        }
        let config = self.device_config();
        if config.resolved_backend().is_native() {
            // The compositor owns the native device. Never open a second one
            // for the scene handoff.
            return;
        }
        self.device = Some(BasicDevice::new(config));
        self.device_generation = self.device_generation.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_graphic_basic::command_list::BasicCommandList;
    use crate::api_graphic_basic::device::preferred_native_api;
    use crate::scene_renderer::FrameStats;
    use glam::{Mat4, Vec3};

    fn scene_frame(width: u32, height: u32, clear: [u8; 4]) -> SceneRenderFrame {
        let mut commands = BasicCommandList::new();
        commands.clear(clear);
        SceneRenderFrame {
            commands,
            view_proj: Mat4::IDENTITY,
            light_dir: Vec3::Y,
            width,
            height,
            texture_cache_budget_bytes: 0,
            stats: FrameStats::default(),
        }
    }

    #[test]
    fn agent_workspace_preview_surface_budget_is_applied_without_changing_other_runtimes() {
        let mut runtime = RenderRuntime::default();
        let budget = GraphicsMemoryBudget {
            gpu_bytes: 64 * 1024 * 1024,
            mesh_cache_entries: 512,
            texture_cache_entries: 64,
            ..GraphicsMemoryBudget::potato()
        };
        runtime.set_memory_budget(budget);
        runtime.configure(RenderExecutionPolicy::CpuOnly, false);
        runtime.activate_surface(GraphicsSurfaceKind::SceneViewport);
        assert_eq!(runtime.snapshot().memory_budget, budget);
        let mut other = RenderRuntime::default();
        other.configure(RenderExecutionPolicy::CpuOnly, false);
        other.activate_surface(GraphicsSurfaceKind::SceneViewport);
        assert_eq!(
            other.snapshot().memory_budget,
            GraphicsMemoryBudget::potato()
        );
    }

    #[test]
    fn cpu_only_surface_forces_cpu_backend() {
        let mut runtime = RenderRuntime::default();
        runtime.configure(RenderExecutionPolicy::CpuOnly, false);
        runtime.activate_surface(GraphicsSurfaceKind::SceneViewport);

        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.active_backend, Some(BasicBackendType::CpuSoftware));
        assert!(!snapshot.is_gpu_active());
        assert_eq!(snapshot.device_generation, 1);
        assert_eq!(snapshot.status_badge(), "GFX CPU");
    }

    #[test]
    fn cpu_policy_renders_and_captures_pixels() {
        let mut runtime = RenderRuntime::default();
        runtime.configure(RenderExecutionPolicy::CpuOnly, false);
        runtime.activate_surface(GraphicsSurfaceKind::SceneViewport);

        let frame = scene_frame(2, 1, [12, 34, 56, 255]);
        let output = runtime.render_scene_frame(&frame);
        let SceneFrameOutput::CpuPixels(pixels) = output else {
            panic!("CPU policy must return rasterized pixels");
        };
        assert_eq!(pixels, vec![12, 34, 56, 255, 12, 34, 56, 255]);

        let capture = runtime.capture_last_scene_rgba().unwrap();
        assert_eq!((capture.width, capture.height), (2, 1));
        assert_eq!(capture.rgba8, pixels);
    }

    #[test]
    fn idle_surface_releases_device() {
        let mut runtime = RenderRuntime::default();
        runtime.configure(RenderExecutionPolicy::CpuOnly, false);
        runtime.activate_surface(GraphicsSurfaceKind::SceneViewport);
        runtime.activate_surface(GraphicsSurfaceKind::None);

        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.active_backend, None);
        assert_eq!(snapshot.device_generation, 1);
        assert_eq!(snapshot.status_badge(), "GFX idle");
    }

    #[test]
    fn device_generation_changes_after_device_reset() {
        let mut runtime = RenderRuntime::default();
        runtime.configure(RenderExecutionPolicy::CpuOnly, false);
        runtime.activate_surface(GraphicsSurfaceKind::SchematicCanvas);
        let first_generation = runtime.snapshot().device_generation;

        runtime.set_memory_budget(GraphicsMemoryBudget::desktop());
        runtime.activate_surface(GraphicsSurfaceKind::SchematicCanvas);

        assert!(runtime.snapshot().device_generation > first_generation);
    }

    #[test]
    fn native_policy_hands_the_frame_over_without_opening_a_device() {
        let Some(api) = preferred_native_api() else {
            eprintln!("Native handoff test skipped: no native executor in this build.");
            return;
        };
        let mut runtime = RenderRuntime::default();
        runtime.configure(RenderExecutionPolicy::Auto, false);
        runtime.activate_surface(GraphicsSurfaceKind::SceneViewport);
        assert!(runtime.capture_last_scene_rgba().is_err());

        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.active_backend, Some(BasicBackendType::Native(api)));
        assert!(snapshot.is_gpu_active());
        assert_eq!(snapshot.device_generation, 0);
        assert_eq!(snapshot.status_badge(), format!("GFX AGB {api}"));

        let frame = scene_frame(2, 1, [9, 8, 7, 255]);
        let output = runtime.render_scene_frame(&frame);
        let SceneFrameOutput::Scene(retained) = output else {
            panic!("native policy must hand over the retained frame");
        };
        assert_eq!(retained.width, 2);
        assert_eq!(retained.height, 1);

        let metrics = runtime.snapshot().last_frame_metrics;
        assert_eq!(metrics.frame_upload_bytes, 0);
        assert_eq!(metrics.total_upload_bytes(), 0);
        assert_eq!(metrics.mesh_draw_calls, 0);

        let capture = runtime.capture_last_scene_rgba().unwrap();
        assert_eq!(capture.rgba8, vec![9, 8, 7, 255, 9, 8, 7, 255]);
    }
}
