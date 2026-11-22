//! Native Winit and AGB presentation host for retained UI.
//!
//! The host owns the window, one native executor and its `ProductDevice`.
//! Scene, CAD and RafUI reach the target as product packets composed by
//! `NativeEditorCompositor`; this type only opens, resizes, draws and reports
//! presentation health.

mod headless;
mod window;

pub use window::{
    NativeCursorLease, NativeGraphicsContext, NativePresentMode, NativeUiWindowConfig,
    NativeWindowCommandResult,
};

use std::sync::Arc;

use raf_agb::backends::native::NativeApi;
use raf_agb::backends::native::NativeBackend;
use raf_agb::product::ProductDevice;
use winit::window::Window;

use super::presentation_lifecycle::PresentationRecovery;

pub struct NativeUiWindowHost {
    // The native executor owns the surface, so it must drop before the Winit
    // window whose raw handle it holds.
    pub(super) device: ProductDevice<NativeBackend>,
    window: Option<Arc<Window>>,
    pub(super) api: NativeApi,
    pub(super) extent: [u32; 2],
    pub(super) headless: bool,
    pub(super) host_config: NativeUiWindowConfig,
    pub(super) presentation: PresentationRecovery,
}

impl NativeUiWindowHost {
    /// Selected native executor. Reported to editor diagnostics and never
    /// changed after the device opens.
    pub fn api(&self) -> NativeApi {
        self.api
    }

    pub fn api_label(&self) -> &'static str {
        match self.api {
            NativeApi::Vulkan => "vulkan",
            NativeApi::Dx12 => "dx12",
            NativeApi::Metal => "metal",
        }
    }
}
