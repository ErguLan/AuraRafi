//! Win32/Winit window creation, native device ownership, and OS window commands.

use std::marker::PhantomData;
use std::sync::Arc;

use raf_agb::backends::native::{NativeBackend, NativeRequest};
use raf_agb::product::ProductDevice;
use raf_agb::Output;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::{ResizeDirection, Window};

use super::super::presentation_lifecycle::{PresentationDecision, PresentationFault, PresentationRecovery};
use super::super::{DirectUiSurfaceHost, NativeApplicationMenuAdapter, UiSurface};
use super::NativeUiWindowHost;
use crate::api_graphic_basic::cad_surface_host::DirectCadSurfaceHost;
use crate::api_graphic_basic::capabilities::{
    GraphicsAdapterPreference, GraphicsMemoryBudget,
};
use crate::api_graphic_basic::editor_compositor::{
    EditorCanvasLayer, EditorComposedFrame, EditorUiLayer, NativeEditorCompositor,
};
use crate::engine_time::Instant;
use raf_ui::{UiApplicationMenu, UiResizeEdge, UiWindowCommand};

/// Result of a window command that needs to cross from RafUI into the native
/// event loop instead of being executed directly on the Winit window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeWindowCommandResult {
    Applied,
    RequestClose,
    ShowSystemMenu,
}

/// Backend-neutral presentation mode reported to editor diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativePresentMode {
    Vsync,
    Immediate,
    Mailbox,
    Other,
}

impl NativePresentMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Vsync => "VSync",
            Self::Immediate => "Immediate",
            Self::Mailbox => "Mailbox",
            Self::Other => "Present",
        }
    }
}

/// Opaque graphics construction context exposed to editor hosts.
///
/// The native device and target format stay inside ApiGraphicBasic. Editor
/// surfaces request owned UI/CAD producers without importing adapter types.
pub struct NativeGraphicsContext<'a> {
    memory_budget: GraphicsMemoryBudget,
    _lifetime: PhantomData<&'a ()>,
}

impl NativeGraphicsContext<'_> {
    pub fn create_ui_host(&self, surface: UiSurface, clear_color: [u8; 4]) -> DirectUiSurfaceHost {
        DirectUiSurfaceHost::with_budget(surface, clear_color, self.memory_budget)
    }

    pub fn create_cad_host(&self, _clear_color: [u8; 4]) -> DirectCadSurfaceHost {
        DirectCadSurfaceHost::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NativeUiWindowConfig {
    /// Retained for host configuration compatibility. The native executor
    /// chooses its own adapter; AGB does not accept a power hint.
    pub adapter_preference: GraphicsAdapterPreference,
    pub memory_budget: GraphicsMemoryBudget,
    /// Retained for compatibility. AGB owns swapchain latency.
    pub desired_maximum_frame_latency: u32,
}

impl Default for NativeUiWindowConfig {
    fn default() -> Self {
        Self {
            adapter_preference: GraphicsAdapterPreference::LowPower,
            memory_budget: GraphicsMemoryBudget::potato(),
            desired_maximum_frame_latency: 2,
        }
    }
}

/// Runtime-owned cursor lease. Dropping Play never leaves the editor cursor captured.
///
/// A headless host has no window, so applying and releasing a capture become
/// virtual bookkeeping instead of operating-system calls.
pub struct NativeCursorLease {
    window: Option<Arc<Window>>,
    requested: Option<(raf_core::runtime_config::CursorCapture, bool)>,
    effective: raf_core::runtime_config::CursorCapture,
}
impl NativeCursorLease {
    pub fn apply(
        &mut self,
        capture: raf_core::runtime_config::CursorCapture,
        visible: bool,
    ) -> Result<raf_core::runtime_config::CursorCapture, String> {
        use raf_core::runtime_config::CursorCapture;
        use winit::window::CursorGrabMode;
        if self.requested == Some((capture, visible)) {
            return Ok(self.effective);
        }
        self.requested = Some((capture, visible));
        let Some(window) = self.window.as_ref() else {
            self.effective = capture;
            return Ok(capture);
        };
        let result = match capture {
            CursorCapture::Free => window
                .set_cursor_grab(CursorGrabMode::None)
                .map(|_| CursorCapture::Free),
            CursorCapture::Confined => window
                .set_cursor_grab(CursorGrabMode::Confined)
                .map(|_| CursorCapture::Confined),
            CursorCapture::Locked => window
                .set_cursor_grab(CursorGrabMode::Locked)
                .map(|_| CursorCapture::Locked)
                .or_else(|_| {
                    window
                        .set_cursor_grab(CursorGrabMode::Confined)
                        .map(|_| CursorCapture::Confined)
                }),
        };
        match result {
            Ok(effective) => {
                self.effective = effective;
                window.set_cursor_visible(visible);
                Ok(effective)
            }
            Err(error) => {
                self.release();
                self.requested = Some((capture, visible));
                Err(format!("input.cursor_unavailable: {error}"))
            }
        }
    }
    pub fn release(&mut self) {
        if let Some(window) = self.window.as_ref() {
            let _ = window.set_cursor_grab(winit::window::CursorGrabMode::None);
            window.set_cursor_visible(true);
        }
        self.requested = None;
        self.effective = raf_core::runtime_config::CursorCapture::Free;
    }
}
impl Drop for NativeCursorLease {
    fn drop(&mut self) {
        self.release();
    }
}

impl NativeUiWindowHost {
    pub fn cursor_lease(&self) -> NativeCursorLease {
        NativeCursorLease {
            window: self.window.clone(),
            requested: None,
            effective: raf_core::runtime_config::CursorCapture::Free,
        }
    }

    /// Opens the window on the compiled native executor chosen by AGB.
    ///
    /// Kept `async` for call-site compatibility; the native open path does not
    /// await. There is no WGPU fallback: if no native executor can serve the
    /// window, the host fails with the backend error.
    pub async fn create(window: Arc<Window>) -> Result<Self, String> {
        Self::create_with_config(window, NativeUiWindowConfig::default()).await
    }

    pub async fn create_with_config(
        window: Arc<Window>,
        host_config: NativeUiWindowConfig,
    ) -> Result<Self, String> {
        let size = window.inner_size();
        let extent = [size.width.max(1), size.height.max(1)];
        let backend = open_window_backend(&window)?;
        let api = backend.api();
        let mut device = ProductDevice::new(backend, host_config.memory_budget)
            .map_err(|error| format!("native product device: {error}"))?;
        device
            .configure(extent, Output::Present)
            .map_err(|error| format!("native window configure: {error}"))?;
        Ok(Self {
            window: Some(window),
            device,
            api,
            extent,
            headless: false,
            host_config,
            presentation: PresentationRecovery::default(),
        })
    }

    /// Logical-to-physical scale reported by the native window, or `1.0`
    /// headless where logical and physical pixels are identical.
    pub fn scale_factor(&self) -> f32 {
        self.window
            .as_ref()
            .map_or(1.0, |window| window.scale_factor() as f32)
    }

    /// Headless-safe window access for hosts that may run without a window.
    pub fn window_opt(&self) -> Option<&Arc<Window>> {
        self.window.as_ref()
    }

    /// Returns the native window.
    ///
    /// Existing editor call sites depend on this borrowed handle. Headless
    /// hosts have no window, so use [`Self::window_opt`] in code that may run
    /// without one; calling this in headless mode is a programming error.
    pub fn window(&self) -> &Arc<Window> {
        self.window.as_ref().expect(
            "NativeUiWindowHost::window() requires a windowed host; use window_opt() for headless-capable code",
        )
    }

    /// Executes the OS-owned part of RafUI's window contract.
    pub fn execute_window_command(
        &self,
        command: UiWindowCommand,
    ) -> Result<NativeWindowCommandResult, String> {
        let Some(window) = self.window.as_ref() else {
            return Err("window commands are unavailable in headless presentation".to_string());
        };
        let result = match command {
            UiWindowCommand::BeginDrag => window
                .drag_window()
                .map_err(|error| format!("native window drag: {error}"))
                .map(|_| NativeWindowCommandResult::Applied),
            UiWindowCommand::BeginResize(edge) => window
                .drag_resize_window(resize_direction(edge))
                .map_err(|error| format!("native window resize: {error}"))
                .map(|_| NativeWindowCommandResult::Applied),
            UiWindowCommand::Minimize => {
                window.set_minimized(true);
                Ok(NativeWindowCommandResult::Applied)
            }
            UiWindowCommand::ToggleMaximize => {
                window.set_maximized(!window.is_maximized());
                Ok(NativeWindowCommandResult::Applied)
            }
            UiWindowCommand::Close => Ok(NativeWindowCommandResult::RequestClose),
            UiWindowCommand::ShowSystemMenu => Ok(NativeWindowCommandResult::ShowSystemMenu),
        }?;
        Ok(result)
    }

    pub fn graphics_context(&self) -> NativeGraphicsContext<'_> {
        NativeGraphicsContext {
            memory_budget: self.host_config.memory_budget,
            _lifetime: PhantomData,
        }
    }

    pub fn size(&self) -> [u32; 2] {
        self.extent
    }

    pub fn width(&self) -> u32 {
        self.extent[0]
    }

    pub fn height(&self) -> u32 {
        self.extent[1]
    }

    /// AGB presents with FIFO, so the reported mode is always VSync.
    pub fn presentation_mode(&self) -> NativePresentMode {
        if self.is_headless() {
            return NativePresentMode::Other;
        }
        NativePresentMode::Vsync
    }

    /// Returns the refresh rate of the monitor currently carrying the
    /// native window. Winit exposes this in milli-Hz; the scheduler only
    /// needs a conservative whole-Hz cadence. A headless host has no monitor.
    pub fn display_refresh_hz(&self) -> Option<u16> {
        self.window.as_ref().and_then(|window| {
            window.current_monitor().and_then(|monitor| {
                let refresh_millihz = monitor.refresh_rate_millihertz()?;
                (refresh_millihz > 0).then(|| {
                    ((refresh_millihz.saturating_add(500)) / 1000).clamp(1, u32::from(u16::MAX))
                        as u16
                })
            })
        })
    }

    /// Returns the presentation cadence that should constrain CPU/GPU work.
    pub fn effective_present_refresh_hz(&self) -> Option<u16> {
        matches!(self.presentation_mode(), NativePresentMode::Vsync)
            .then(|| self.display_refresh_hz())
            .flatten()
    }

    /// AGB currently owns the swapchain present mode and uses FIFO. The
    /// preference is retained for diagnostics; it does not silently claim a
    /// mode the executor did not apply.
    pub fn set_vsync(&mut self, _enabled: bool) {}

    /// Installs the shared application command tree through a platform-owned
    /// menu adapter.
    pub fn install_application_menu<A>(
        &self,
        adapter: &mut A,
        menu: &UiApplicationMenu,
    ) -> Result<(), String>
    where
        A: NativeApplicationMenuAdapter,
    {
        let Some(window) = self.window.as_ref() else {
            return Err("native application menus require a windowed host".to_string());
        };
        adapter.install(window.as_ref(), menu)
    }

    pub fn install_application_menu_localized<A>(
        &self,
        adapter: &mut A,
        menu: &UiApplicationMenu,
        resolve: &mut dyn FnMut(&str) -> String,
    ) -> Result<(), String>
    where
        A: NativeApplicationMenuAdapter,
    {
        let Some(window) = self.window.as_ref() else {
            return Err("native application menus require a windowed host".to_string());
        };
        adapter.install_localized(window.as_ref(), menu, resolve)
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        if self.extent == [width, height] {
            return;
        }
        self.extent = [width, height];
        let output = if self.headless {
            Output::Capture
        } else {
            Output::Present
        };
        let _ = self.device.configure(self.extent, output);
    }

    pub fn presentation_health(&self) -> super::super::presentation_lifecycle::PresentationHealth {
        self.presentation.health()
    }

    pub fn last_presentation_fault(
        &self,
    ) -> Option<super::super::presentation_lifecycle::PresentationFault> {
        self.presentation.last_fault()
    }

    /// Reconfigures the swapchain after an external surface change or a fault.
    pub fn recover_surface(&mut self) -> Result<(), String> {
        let output = if self.headless {
            Output::Capture
        } else {
            Output::Present
        };
        self.device
            .configure(self.extent, output)
            .map_err(|error| format!("native surface reconfigure: {error}"))?;
        self.presentation.record_success(Instant::now());
        Ok(())
    }

    pub fn render_editor_frame<F>(
        &mut self,
        compositor: &mut NativeEditorCompositor,
        canvas_layer: Option<EditorCanvasLayer>,
        ui: &mut DirectUiSurfaceHost,
        logical_size: [u32; 2],
        raster_scale: f32,
        resolve: F,
    ) -> Result<EditorComposedFrame, String>
    where
        F: FnMut(&str) -> String,
    {
        let mut ui_layers = [EditorUiLayer {
            host: ui,
            target_rect: crate::api_graphic_basic::CanvasTargetRect::full(self.extent),
            logical_size,
            raster_scale,
        }];
        self.render_editor_layers(compositor, canvas_layer, &mut ui_layers, resolve)
    }

    pub fn render_editor_layers<F>(
        &mut self,
        compositor: &mut NativeEditorCompositor,
        canvas_layer: Option<EditorCanvasLayer>,
        ui_layers: &mut [EditorUiLayer<'_>],
        mut resolve: F,
    ) -> Result<EditorComposedFrame, String>
    where
        F: FnMut(&str) -> String,
    {
        let mut attempt = 0;
        loop {
            attempt += 1;
            let result = compositor.compose_layers(
                &mut self.device,
                self.extent,
                canvas_layer.clone(),
                ui_layers,
                &mut resolve,
            );
            match result {
                Ok(mut frame) => {
                    self.presentation.record_success(Instant::now());
                    frame.metrics.presents = if self.headless { 0 } else { 1 };
                    return Ok(frame);
                }
                Err(error) => {
                    let Some(fault) = self.device_fault(&error) else {
                        tracing::error!(%error, "native editor composition failed");
                        return Err(error);
                    };
                    let decision = self.presentation.record_fault(fault, Instant::now());
                    if matches!(decision, PresentationDecision::Fatal) {
                        tracing::error!(%error, fault = fault.label(), "native editor presentation failed");
                        return Err(error);
                    }
                    if fault == PresentationFault::Timeout {
                        // Swapchain acquisition timed out. Skipping the frame
                        // is cheaper and more truthful than retrying inside the
                        // same tick; the scheduler will call again.
                        let mut metrics = compositor.metrics();
                        metrics.presents = 0;
                        return Ok(EditorComposedFrame {
                            ui_layers: Vec::new(),
                            metrics,
                        });
                    }
                    if fault == PresentationFault::Lost {
                        self.reopen_device()?;
                        compositor.release_canvas();
                    } else {
                        self.recover_surface()?;
                    }
                    if attempt >= 2 {
                        return Err(error);
                    }
                }
            }
        }
    }

    /// Reopens the native executor after a device loss. The window handle is
    /// still valid; the compositor replays its retained sources on the next
    /// frame.
    fn reopen_device(&mut self) -> Result<(), String> {
        let output = if self.headless {
            Output::Capture
        } else {
            Output::Present
        };
        let backend = match self.window.as_ref() {
            Some(window) => open_window_backend(window)?,
            None => NativeBackend::headless(NativeRequest::Auto)
                .map_err(|error| format!("headless native executor: {error}"))?,
        };
        self.api = backend.api();
        let mut device = ProductDevice::new(backend, self.host_config.memory_budget)
            .map_err(|error| format!("native product device: {error}"))?;
        device
            .configure(self.extent, output)
            .map_err(|error| format!("native target configure: {error}"))?;
        self.device = device;
        self.presentation.record_success(Instant::now());
        Ok(())
    }

    fn device_fault(&self, error: &str) -> Option<PresentationFault> {
        let lowered = error.to_ascii_lowercase();
        if lowered.contains("surface lost") || lowered.contains("device lost") {
            Some(PresentationFault::Lost)
        } else if lowered.contains("surface must be reconfigured")
            || lowered.contains("outdated")
        {
            Some(PresentationFault::Outdated)
        } else if lowered.contains("queue is busy") {
            Some(PresentationFault::Timeout)
        } else {
            None
        }
    }
}

fn open_window_backend(window: &Arc<Window>) -> Result<NativeBackend, String> {
    let display = window
        .display_handle()
        .map_err(|error| format!("native display handle: {error}"))?
        .as_raw();
    let handle = window
        .window_handle()
        .map_err(|error| format!("native window handle: {error}"))?
        .as_raw();
    // SAFETY: the host keeps the Winit window alive for the whole lifetime of
    // the backend and drops the backend before the window.
    unsafe { NativeBackend::window(NativeRequest::Auto, display, handle) }
        .map_err(|error| format!("native window executor: {error}"))
}

const fn resize_direction(edge: UiResizeEdge) -> ResizeDirection {
    match edge {
        UiResizeEdge::North => ResizeDirection::North,
        UiResizeEdge::South => ResizeDirection::South,
        UiResizeEdge::East => ResizeDirection::East,
        UiResizeEdge::West => ResizeDirection::West,
        UiResizeEdge::NorthEast => ResizeDirection::NorthEast,
        UiResizeEdge::NorthWest => ResizeDirection::NorthWest,
        UiResizeEdge::SouthEast => ResizeDirection::SouthEast,
        UiResizeEdge::SouthWest => ResizeDirection::SouthWest,
    }
}
