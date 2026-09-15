//! Native Winit and WGPU presentation host for retained UI.
//!
//! This host owns a real surface and presents `raf_ui` directly.

use std::sync::Arc;

use winit::window::{ResizeDirection, Window};

use super::{
    CpuUiSurfaceHost, DirectUiSurfaceFrame, DirectUiSurfaceHost, NativeApplicationMenuAdapter,
    UiSurface, UiSurfaceCpuMetrics, UiSurfaceGpuSharedResources,
};
use crate::api_graphic_basic::cad_surface_host::DirectCadSurfaceHost;
use crate::api_graphic_basic::canvas_presenter::DirectCanvasPresenter;
use crate::api_graphic_basic::canvas_presenter::DirectSceneSurfaceHost;
use crate::api_graphic_basic::capabilities::{GraphicsAdapterPreference, GraphicsMemoryBudget};
use crate::api_graphic_basic::device::{
    wgpu_timestamp_features, BasicDevice, BasicDeviceConfig, SceneFrameOutput,
    SharedGraphicsContext,
};
use crate::api_graphic_basic::{
    EditorCanvasLayer, EditorComposedFrame, EditorUiLayer, NativeEditorCompositor,
};
use crate::scene_renderer::SceneRenderFrame;
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
/// The concrete WGPU device and target format stay inside ApiGraphicBasic.
/// Editor surfaces can request owned UI/CAD hosts without importing adapter
/// types into their public constructors.
pub struct NativeGraphicsContext<'a> {
    device: &'a wgpu::Device,
    color_format: wgpu::TextureFormat,
    ui_shared: &'a Arc<UiSurfaceGpuSharedResources>,
    memory_budget: GraphicsMemoryBudget,
}

impl NativeGraphicsContext<'_> {
    pub fn create_ui_host(&self, surface: UiSurface, clear_color: [u8; 4]) -> DirectUiSurfaceHost {
        DirectUiSurfaceHost::with_shared_and_budget(
            surface,
            self.ui_shared.clone(),
            clear_color,
            self.memory_budget,
        )
    }

    pub fn create_cad_host(&self, clear_color: [u8; 4]) -> DirectCadSurfaceHost {
        DirectCadSurfaceHost::new(self.device, self.color_format, clear_color)
    }

    pub(crate) fn device(&self) -> &wgpu::Device {
        self.device
    }

    pub(crate) fn color_format(&self) -> wgpu::TextureFormat {
        self.color_format
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NativeUiWindowConfig {
    pub adapter_preference: GraphicsAdapterPreference,
    pub memory_budget: GraphicsMemoryBudget,
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

pub struct NativeUiWindowHost {
    window: Arc<Window>,
    _instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    adapter: wgpu::Adapter,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    configuration: wgpu::SurfaceConfiguration,
    host_config: NativeUiWindowConfig,
    ui_shared: Arc<UiSurfaceGpuSharedResources>,
}

impl NativeUiWindowHost {
    pub async fn create(window: Arc<Window>) -> Result<Self, String> {
        Self::create_with_config(window, NativeUiWindowConfig::default()).await
    }

    pub async fn create_with_config(
        window: Arc<Window>,
        host_config: NativeUiWindowConfig,
    ) -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..wgpu::InstanceDescriptor::default()
        });
        let surface = instance
            .create_surface(window.clone())
            .map_err(|error| format!("native UI surface: {error}"))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: match host_config.adapter_preference {
                    GraphicsAdapterPreference::LowPower => wgpu::PowerPreference::LowPower,
                    GraphicsAdapterPreference::HighPerformance => {
                        wgpu::PowerPreference::HighPerformance
                    }
                },
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .ok_or_else(|| "native UI adapter was not found".to_string())?;
        let timestamp_features = wgpu_timestamp_features();
        let optional_features = if adapter.features().contains(timestamp_features) {
            timestamp_features
        } else {
            wgpu::Features::empty()
        };
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("ApiGraphicBasic.NativeUiDevice"),
                    required_features: optional_features,
                    required_limits: wgpu::Limits::downlevel_defaults(),
                    memory_hints: match host_config.adapter_preference {
                        GraphicsAdapterPreference::LowPower => wgpu::MemoryHints::MemoryUsage,
                        GraphicsAdapterPreference::HighPerformance => {
                            wgpu::MemoryHints::Performance
                        }
                    },
                },
                None,
            )
            .await
            .map_err(|error| format!("native UI device: {error}"))?;
        let size = window.inner_size();
        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| capabilities.formats.first().copied())
            .ok_or_else(|| "native UI surface has no supported format".to_string())?;
        let present_mode = capabilities
            .present_modes
            .iter()
            .copied()
            .find(|mode| *mode == wgpu::PresentMode::Fifo)
            .or_else(|| capabilities.present_modes.first().copied())
            .ok_or_else(|| "native UI surface has no present mode".to_string())?;
        let alpha_mode = capabilities
            .alpha_modes
            .first()
            .copied()
            .ok_or_else(|| "native UI surface has no alpha mode".to_string())?;
        let configuration = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: host_config.desired_maximum_frame_latency.clamp(1, 3),
        };
        surface.configure(&device, &configuration);
        let ui_shared = Arc::new(UiSurfaceGpuSharedResources::new(&device, format));

        Ok(Self {
            window,
            _instance: instance,
            surface,
            adapter,
            device: Arc::new(device),
            queue: Arc::new(queue),
            configuration,
            host_config,
            ui_shared,
        })
    }

    pub fn window(&self) -> &Arc<Window> {
        &self.window
    }

    /// Executes the OS-owned part of RafUI's window contract.
    ///
    /// Dragging, resizing, minimizing and maximizing are safe to execute
    /// immediately. Closing and opening the system menu remain event-loop
    /// requests because Winit intentionally exposes those operations through
    /// native events rather than a synthetic `Window` mutation.
    pub fn execute_window_command(
        &self,
        command: UiWindowCommand,
    ) -> Result<NativeWindowCommandResult, String> {
        let result = match command {
            UiWindowCommand::BeginDrag => self
                .window
                .drag_window()
                .map_err(|error| format!("native window drag: {error}"))
                .map(|_| NativeWindowCommandResult::Applied),
            UiWindowCommand::BeginResize(edge) => self
                .window
                .drag_resize_window(resize_direction(edge))
                .map_err(|error| format!("native window resize: {error}"))
                .map(|_| NativeWindowCommandResult::Applied),
            UiWindowCommand::Minimize => {
                self.window.set_minimized(true);
                Ok(NativeWindowCommandResult::Applied)
            }
            UiWindowCommand::ToggleMaximize => {
                self.window.set_maximized(!self.window.is_maximized());
                Ok(NativeWindowCommandResult::Applied)
            }
            UiWindowCommand::Close => Ok(NativeWindowCommandResult::RequestClose),
            UiWindowCommand::ShowSystemMenu => Ok(NativeWindowCommandResult::ShowSystemMenu),
        }?;

        Ok(result)
    }

    pub fn adapter(&self) -> &wgpu::Adapter {
        &self.adapter
    }

    pub fn graphics_context(&self) -> NativeGraphicsContext<'_> {
        NativeGraphicsContext {
            device: self.device.as_ref(),
            color_format: self.configuration.format,
            ui_shared: &self.ui_shared,
            memory_budget: self.host_config.memory_budget,
        }
    }

    pub fn shared_graphics_context(&self) -> SharedGraphicsContext {
        SharedGraphicsContext::from_host(self.device.clone(), self.queue.clone())
    }

    pub fn basic_device_config(&self) -> BasicDeviceConfig {
        BasicDeviceConfig {
            allow_gpu: true,
            force_cpu: false,
            shared_graphics_context: Some(self.shared_graphics_context()),
            memory_budget: self.host_config.memory_budget,
            adapter_preference: self.host_config.adapter_preference,
        }
    }

    pub fn size(&self) -> [u32; 2] {
        [self.configuration.width, self.configuration.height]
    }

    pub fn width(&self) -> u32 {
        self.configuration.width
    }

    pub fn height(&self) -> u32 {
        self.configuration.height
    }

    pub fn presentation_mode(&self) -> NativePresentMode {
        match self.configuration.present_mode {
            wgpu::PresentMode::Fifo | wgpu::PresentMode::AutoVsync => NativePresentMode::Vsync,
            wgpu::PresentMode::Immediate | wgpu::PresentMode::AutoNoVsync => {
                NativePresentMode::Immediate
            }
            wgpu::PresentMode::Mailbox => NativePresentMode::Mailbox,
            _ => NativePresentMode::Other,
        }
    }

    /// Returns the refresh rate of the monitor currently carrying the
    /// native window. Winit exposes this in milli-Hz; the scheduler only
    /// needs a conservative whole-Hz cadence.
    pub fn display_refresh_hz(&self) -> Option<u16> {
        self.window.current_monitor().and_then(|monitor| {
            let refresh_millihz = monitor.refresh_rate_millihertz()?;
            (refresh_millihz > 0).then(|| {
                ((refresh_millihz.saturating_add(500)) / 1000).clamp(1, u32::from(u16::MAX)) as u16
            })
        })
    }

    /// Returns the presentation cadence that should constrain CPU/GPU work.
    /// Immediate mode deliberately returns `None`: there is no swapchain
    /// refresh contract for the scheduler to mirror in that mode.
    pub fn effective_present_refresh_hz(&self) -> Option<u16> {
        matches!(self.presentation_mode(), NativePresentMode::Vsync)
            .then(|| self.display_refresh_hz())
            .flatten()
    }

    /// Applies the editor's VSync preference to the native swapchain. The
    /// surface capabilities remain authoritative because some platforms do
    /// not expose an immediate present mode.
    pub fn set_vsync(&mut self, enabled: bool) {
        let capabilities = self.surface.get_capabilities(&self.adapter);
        let desired = if enabled {
            wgpu::PresentMode::Fifo
        } else {
            wgpu::PresentMode::Immediate
        };
        let present_mode = capabilities
            .present_modes
            .iter()
            .copied()
            .find(|mode| *mode == desired)
            .or_else(|| {
                capabilities
                    .present_modes
                    .iter()
                    .copied()
                    .find(|mode| *mode == wgpu::PresentMode::Fifo)
            })
            .or_else(|| capabilities.present_modes.first().copied());
        let Some(present_mode) = present_mode else {
            return;
        };
        if self.configuration.present_mode == present_mode {
            return;
        }
        self.configuration.present_mode = present_mode;
        self.surface.configure(&self.device, &self.configuration);
    }

    /// Installs the shared application command tree through a platform-owned
    /// menu adapter. The native event loop later drains stable command IDs and
    /// dispatches them at the application boundary.
    pub fn install_application_menu<A>(
        &self,
        adapter: &mut A,
        menu: &UiApplicationMenu,
    ) -> Result<(), String>
    where
        A: NativeApplicationMenuAdapter,
    {
        adapter.install(self.window.as_ref(), menu)
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
        adapter.install_localized(self.window.as_ref(), menu, resolve)
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        if self.configuration.width == width && self.configuration.height == height {
            return;
        }
        self.configuration.width = width;
        self.configuration.height = height;
        self.surface.configure(&self.device, &self.configuration);
    }

    pub fn render<F>(
        &mut self,
        host: &mut DirectUiSurfaceHost,
        resolve: F,
    ) -> Result<DirectUiSurfaceFrame, wgpu::SurfaceError>
    where
        F: FnMut(&str) -> String,
    {
        let frame = self.surface.get_current_texture()?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let output = host.render(
            &self.device,
            &self.queue,
            &view,
            [self.configuration.width, self.configuration.height],
            resolve,
        );
        frame.present();
        Ok(output)
    }

    pub fn render_editor_frame<F>(
        &mut self,
        compositor: &mut NativeEditorCompositor,
        canvas_layer: Option<EditorCanvasLayer>,
        ui: &mut DirectUiSurfaceHost,
        logical_size: [u32; 2],
        raster_scale: f32,
        resolve: F,
    ) -> Result<EditorComposedFrame, wgpu::SurfaceError>
    where
        F: FnMut(&str) -> String,
    {
        let frame = self.surface.get_current_texture()?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut composed = compositor.compose(
            self.device.as_ref(),
            self.queue.as_ref(),
            &view,
            [self.configuration.width, self.configuration.height],
            canvas_layer,
            ui,
            logical_size,
            raster_scale,
            resolve,
        );
        frame.present();
        composed.metrics.presents = 1;
        Ok(composed)
    }

    pub fn render_editor_layers<F>(
        &mut self,
        compositor: &mut NativeEditorCompositor,
        canvas_layer: Option<EditorCanvasLayer>,
        ui_layers: &mut [EditorUiLayer<'_>],
        resolve: F,
    ) -> Result<EditorComposedFrame, wgpu::SurfaceError>
    where
        F: FnMut(&str) -> String,
    {
        let frame = self.surface.get_current_texture()?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut composed = compositor.compose_layers(
            self.device.as_ref(),
            self.queue.as_ref(),
            &view,
            [self.configuration.width, self.configuration.height],
            canvas_layer,
            ui_layers,
            resolve,
        );
        frame.present();
        composed.metrics.presents = 1;
        Ok(composed)
    }

    pub fn present_canvas(
        &mut self,
        presenter: &mut DirectCanvasPresenter,
        output: SceneFrameOutput,
        source_size: [u32; 2],
        clear_color: [u8; 4],
    ) -> Result<(), wgpu::SurfaceError> {
        let frame = self.surface.get_current_texture()?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        presenter.present(
            &self.device,
            &self.queue,
            &view,
            output,
            source_size,
            clear_color,
        );
        frame.present();
        Ok(())
    }

    /// Presents a retained surface through the CPU compositor and uploads the
    /// borrowed RGBA result only for the final native presentation step.
    pub fn render_cpu_ui<F>(
        &mut self,
        presenter: &mut DirectCanvasPresenter,
        host: &mut CpuUiSurfaceHost,
        resolve: F,
    ) -> Result<UiSurfaceCpuMetrics, wgpu::SurfaceError>
    where
        F: FnMut(&str) -> String,
    {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let clear_color = host.clear_color();
        let ui_frame = host.render(
            [self.configuration.width, self.configuration.height],
            resolve,
        );
        let metrics = ui_frame.metrics;
        presenter.present_cpu_pixels(
            &self.device,
            &self.queue,
            &view,
            ui_frame.pixels,
            ui_frame.size,
            clear_color,
        );
        output.present();
        Ok(metrics)
    }

    pub fn render_scene(
        &mut self,
        host: &mut DirectSceneSurfaceHost,
        basic_device: &mut BasicDevice,
        frame: &SceneRenderFrame,
    ) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        host.render_frame(basic_device, &self.device, &self.queue, &view, frame);
        output.present();
        Ok(())
    }
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
