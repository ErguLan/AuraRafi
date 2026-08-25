//! Shared graphics runtime state for editor surfaces.
//!
//! Owns the lightweight `ApiGraphicBasic` device lifecycle so interactive
//! surfaces can lazily request GPU-first execution without forcing GPU startup
//! on the hub or settings screens.

use raf_core::config::RenderExecutionPolicy;

use crate::api_graphic_basic::device::{
    BasicBackendType, BasicDevice, BasicDeviceConfig, SceneFrameMetrics, SceneFrameOutput,
    SharedGraphicsContext,
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
    pub fn is_gpu_active(&self) -> bool {
        matches!(self.active_backend, Some(BasicBackendType::GpuHardware))
    }

    pub fn status_badge(&self) -> String {
        match self.active_backend {
            Some(BasicBackendType::GpuHardware) if self.advanced_gpu_features_allowed => {
                "GFX GPU+".to_string()
            }
            Some(BasicBackendType::GpuHardware) => "GFX GPU".to_string(),
            Some(BasicBackendType::CpuSoftware) => "GFX CPU".to_string(),
            None if self.surface.requires_graphics_device() => "GFX init".to_string(),
            None => "GFX idle".to_string(),
        }
    }
}

pub struct RenderRuntime {
    policy: RenderExecutionPolicy,
    advanced_gpu_features_allowed: bool,
    shared_graphics_context: Option<SharedGraphicsContext>,
    surface: GraphicsSurfaceKind,
    surface_generation: u64,
    device: Option<BasicDevice>,
    device_generation: u64,
    scheduler: FrameScheduler,
}

impl Default for RenderRuntime {
    fn default() -> Self {
        Self {
            policy: RenderExecutionPolicy::Auto,
            advanced_gpu_features_allowed: false,
            shared_graphics_context: None,
            surface: GraphicsSurfaceKind::None,
            surface_generation: 0,
            device: None,
            device_generation: 0,
            scheduler: FrameScheduler::default(),
        }
    }
}

impl RenderRuntime {
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
        self.scheduler.request(FrameInvalidation::EXPLICIT);
    }

    pub fn set_shared_graphics_context(
        &mut self,
        shared_graphics_context: Option<SharedGraphicsContext>,
    ) {
        self.shared_graphics_context = shared_graphics_context;
        self.device = None;
        self.scheduler.request(FrameInvalidation::WINDOW);
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
        RenderRuntimeSnapshot {
            surface: self.surface,
            policy: self.policy,
            active_backend: self.device.as_ref().map(|device| device.backend()),
            backend_id: self
                .device
                .as_ref()
                .map(|device| device.capabilities().backend),
            capabilities: self.device.as_ref().map(|device| device.capabilities()),
            memory_budget: self
                .device
                .as_ref()
                .map(|device| device.memory_budget())
                .unwrap_or_default(),
            device_generation: self.device_generation,
            surface_generation: self.surface_generation,
            advanced_gpu_features_allowed: self.advanced_gpu_features_allowed,
            last_frame_metrics: self
                .device
                .as_ref()
                .map(|device| device.last_frame_metrics())
                .unwrap_or_default(),
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
    ) {
        self.scheduler
            .finish_frame(permit, presented_at_seconds, frame_cpu_ms, frame_gpu_ms);
    }

    pub fn render_scene_frame(&mut self, frame: &SceneRenderFrame) -> SceneFrameOutput {
        self.ensure_device();
        self.device
            .as_mut()
            .map(|device| device.execute_scene_frame(frame))
            .unwrap_or_else(|| SceneFrameOutput::CpuPixels(Vec::new()))
    }

    fn ensure_device(&mut self) {
        if self.device.is_none() {
            let mut config = BasicDeviceConfig::from_render_policy(self.policy);
            config.shared_graphics_context = self.shared_graphics_context.clone();
            self.device = Some(BasicDevice::new(config));
            self.device_generation = self.device_generation.wrapping_add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_only_surface_forces_cpu_backend() {
        let mut runtime = RenderRuntime::default();
        runtime.configure(RenderExecutionPolicy::CpuOnly, false);
        runtime.activate_surface(GraphicsSurfaceKind::SceneViewport);

        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.active_backend, Some(BasicBackendType::CpuSoftware));
        assert_eq!(snapshot.device_generation, 1);
        assert_eq!(snapshot.status_badge(), "GFX CPU");
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

        runtime.set_shared_graphics_context(None);
        runtime.activate_surface(GraphicsSurfaceKind::SchematicCanvas);

        assert!(runtime.snapshot().device_generation > first_generation);
    }
}
