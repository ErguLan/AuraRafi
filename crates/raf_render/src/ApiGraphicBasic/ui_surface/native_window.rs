//! Native Winit and WGPU presentation host for retained UI.
//!
//! This host owns a real surface and presents `raf_ui` directly.

mod capture;
mod headless;
mod surface;
mod window;

pub use window::{
    NativeCursorLease, NativeGraphicsContext, NativePresentMode, NativeUiWindowConfig,
    NativeWindowCommandResult,
};

use std::sync::Arc;

use winit::window::Window;

use super::headless::HeadlessSurface;
use super::presentation_lifecycle::PresentationRecovery;
use super::UiSurfaceGpuSharedResources;

pub struct NativeUiWindowHost {
    window: Option<Arc<Window>>,
    _instance: wgpu::Instance,
    surface: Option<wgpu::Surface<'static>>,
    adapter: wgpu::Adapter,
    pub(super) device: Arc<wgpu::Device>,
    pub(super) queue: Arc<wgpu::Queue>,
    pub(super) configuration: wgpu::SurfaceConfiguration,
    pub(super) headless: Option<HeadlessSurface>,
    host_config: NativeUiWindowConfig,
    ui_shared: Arc<UiSurfaceGpuSharedResources>,
    presentation: PresentationRecovery,
}

impl NativeUiWindowHost {
    /// Presentation color format shared by the surface, UI compositor, and
    /// headless render target. It stays below the ApiGraphicBasic boundary.
    pub(super) fn format(&self) -> wgpu::TextureFormat {
        self.configuration.format
    }
}
