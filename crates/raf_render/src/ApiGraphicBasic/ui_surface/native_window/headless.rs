//! Headless target ownership for windowless AGB presentation.

use raf_agb::backends::native::{NativeBackend, NativeRequest};
use raf_agb::product::ProductDevice;
use raf_agb::Output;

use super::{NativeUiWindowConfig, NativeUiWindowHost};

/// Largest axis accepted by a headless target.
pub const MAX_HEADLESS_AXIS: u32 = 16_384;

fn validate_size(size: [u32; 2]) -> Result<[u32; 2], String> {
    if size[0] == 0 || size[1] == 0 {
        return Err("headless target needs a non-zero size".to_string());
    }
    if size[0] > MAX_HEADLESS_AXIS || size[1] > MAX_HEADLESS_AXIS {
        return Err(format!(
            "headless target {}x{} exceeds {MAX_HEADLESS_AXIS} per axis",
            size[0], size[1]
        ));
    }
    Ok(size)
}

impl NativeUiWindowHost {
    /// Creates a fixed-size, windowless presentation host.
    ///
    /// The host owns one capture target instead of a swapchain and uses the
    /// same product composition path as the window host. It never creates a
    /// Winit window or event loop.
    pub fn create_headless(
        size: [u32; 2],
        host_config: NativeUiWindowConfig,
    ) -> Result<Self, String> {
        let size = validate_size(size)?;
        let backend = NativeBackend::headless(NativeRequest::Auto)
            .map_err(|error| format!("headless native executor: {error}"))?;
        let api = backend.api();
        let mut device = ProductDevice::new(backend, host_config.memory_budget)
            .map_err(|error| format!("headless product device: {error}"))?;
        device
            .configure(size, Output::Capture)
            .map_err(|error| format!("headless target configure: {error}"))?;
        Ok(Self {
            window: None,
            device,
            api,
            extent: size,
            headless: true,
            host_config,
            presentation:
                super::super::presentation_lifecycle::PresentationRecovery::default(),
        })
    }

    /// True when this host renders into an owned target without a swapchain.
    pub fn is_headless(&self) -> bool {
        self.headless
    }

    /// Reads the last drawn frame as RGBA8.
    ///
    /// Blocking; call it away from the interactive event loop.
    pub fn capture(&mut self) -> Result<Vec<u8>, String> {
        self.device
            .capture()
            .map_err(|error| format!("native capture failed: {error}"))
    }
}
