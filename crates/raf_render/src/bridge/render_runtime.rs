//! Shared graphics runtime state for editor surfaces.
//!
//! Owns the lightweight `ApiGraphicBasic` device lifecycle so interactive
//! surfaces can lazily request GPU-first execution without forcing GPU startup
//! on the hub or settings screens.

use raf_core::config::RenderExecutionPolicy;

use crate::api_graphic_basic::device::{
    BasicBackendType, BasicDevice, BasicDeviceConfig, SceneFrameOutput, SharedWgpuContext,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderRuntimeSnapshot {
    pub surface: GraphicsSurfaceKind,
    pub policy: RenderExecutionPolicy,
    pub active_backend: Option<BasicBackendType>,
    pub advanced_gpu_features_allowed: bool,
}

impl Default for RenderRuntimeSnapshot {
    fn default() -> Self {
        Self {
            surface: GraphicsSurfaceKind::None,
            policy: RenderExecutionPolicy::Auto,
            active_backend: None,
            advanced_gpu_features_allowed: false,
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
    shared_wgpu_context: Option<SharedWgpuContext>,
    surface: GraphicsSurfaceKind,
    device: Option<BasicDevice>,
}

impl Default for RenderRuntime {
    fn default() -> Self {
        Self {
            policy: RenderExecutionPolicy::Auto,
            advanced_gpu_features_allowed: false,
            shared_wgpu_context: None,
            surface: GraphicsSurfaceKind::None,
            device: None,
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
    }

    pub fn set_shared_wgpu_context(&mut self, shared_wgpu_context: Option<SharedWgpuContext>) {
        self.shared_wgpu_context = shared_wgpu_context;
        self.device = None;
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
            advanced_gpu_features_allowed: self.advanced_gpu_features_allowed,
        }
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
            config.shared_wgpu_context = self.shared_wgpu_context.clone();
            self.device = Some(BasicDevice::new(config));
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
        assert_eq!(snapshot.status_badge(), "GFX idle");
    }
}