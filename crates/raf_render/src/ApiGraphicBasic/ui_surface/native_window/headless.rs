//! Headless target ownership for windowless ApiGraphicBasic presentation.

use std::sync::Arc;

use super::super::headless::{self, HeadlessSurface};
use super::super::presentation_lifecycle::PresentationRecovery;
use super::super::UiSurfaceGpuSharedResources;
use super::{NativeUiWindowConfig, NativeUiWindowHost};
use crate::api_graphic_basic::capabilities::GraphicsAdapterPreference;
use crate::api_graphic_basic::device::wgpu_timestamp_features;

impl NativeUiWindowHost {
    /// Creates a fixed-size, windowless presentation host.
    ///
    /// The host owns one render target instead of a swapchain and uses the
    /// same device, queue, UI shared resources, and frame composition path as
    /// the native window host. It never creates a Winit window or event loop.
    pub fn create_headless(
        size: [u32; 2],
        host_config: NativeUiWindowConfig,
    ) -> Result<Self, String> {
        let size = headless::validate_size(size)?;
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..wgpu::InstanceDescriptor::default()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: match host_config.adapter_preference {
                GraphicsAdapterPreference::LowPower => wgpu::PowerPreference::LowPower,
                GraphicsAdapterPreference::HighPerformance => {
                    wgpu::PowerPreference::HighPerformance
                }
            },
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .ok_or_else(|| "headless UI adapter was not found".to_string())?;
        let timestamp_features = wgpu_timestamp_features();
        let optional_features = if adapter.features().contains(timestamp_features) {
            timestamp_features
        } else {
            wgpu::Features::empty()
        };
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("ApiGraphicBasic.HeadlessUiDevice"),
                required_features: optional_features,
                required_limits: wgpu::Limits::downlevel_defaults(),
                memory_hints: match host_config.adapter_preference {
                    GraphicsAdapterPreference::LowPower => wgpu::MemoryHints::MemoryUsage,
                    GraphicsAdapterPreference::HighPerformance => wgpu::MemoryHints::Performance,
                },
            },
            None,
        ))
        .map_err(|error| format!("headless UI device: {error}"))?;
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let target = HeadlessSurface::new(&device, format, size)?;
        let configuration = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size[0],
            height: size[1],
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            desired_maximum_frame_latency: host_config.desired_maximum_frame_latency.clamp(1, 3),
        };
        let ui_shared = Arc::new(UiSurfaceGpuSharedResources::new(&device, format));

        Ok(Self {
            window: None,
            _instance: instance,
            surface: None,
            adapter,
            device: Arc::new(device),
            queue: Arc::new(queue),
            configuration,
            headless: Some(target),
            host_config,
            ui_shared,
            presentation: PresentationRecovery::default(),
        })
    }

    /// True when this host renders into an owned target without a swapchain.
    pub fn is_headless(&self) -> bool {
        self.headless.is_some()
    }
}
