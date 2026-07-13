//! Native Winit and WGPU presentation host for retained UI.
//!
//! The current editor may keep an `eframe` adapter during migration, but this
//! host owns a real surface and can present `raf_ui` without Egui.

use std::sync::Arc;

use winit::window::Window;

use super::{CpuUiSurfaceHost, DirectUiSurfaceFrame, DirectUiSurfaceHost, UiSurfaceCpuMetrics};
use crate::api_graphic_basic::canvas_presenter::DirectCanvasPresenter;
use crate::api_graphic_basic::canvas_presenter::DirectSceneSurfaceHost;
use crate::api_graphic_basic::device::BasicDevice;
use crate::api_graphic_basic::device::SceneFrameOutput;
use crate::scene_renderer::SceneRenderFrame;

pub struct NativeUiWindowHost {
    window: Arc<Window>,
    _instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    configuration: wgpu::SurfaceConfiguration,
}

impl NativeUiWindowHost {
    pub async fn create(window: Arc<Window>) -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..wgpu::InstanceDescriptor::default()
        });
        let surface = instance
            .create_surface(window.clone())
            .map_err(|error| format!("native UI surface: {error}"))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .ok_or_else(|| "native UI adapter was not found".to_string())?;
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("ApiGraphicBasic.NativeUiDevice"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::downlevel_defaults(),
                    memory_hints: wgpu::MemoryHints::Performance,
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
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &configuration);

        Ok(Self {
            window,
            _instance: instance,
            surface,
            adapter,
            device,
            queue,
            configuration,
        })
    }

    pub fn window(&self) -> &Arc<Window> {
        &self.window
    }

    pub fn adapter(&self) -> &wgpu::Adapter {
        &self.adapter
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    pub fn color_format(&self) -> wgpu::TextureFormat {
        self.configuration.format
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
