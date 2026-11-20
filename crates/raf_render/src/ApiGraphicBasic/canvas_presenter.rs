//! Native presentation of ApiGraphicBasic scene textures.
//!
//! A viewport, schematic, or PCB surface may render off-screen and use this
//! presenter to reach a native WGPU swapchain without becoming a foreign UI
//! image.

mod blit;
mod cpu;
mod metrics;

pub use metrics::CanvasTargetRect;

use std::sync::Arc;

use crate::api_graphic_basic::device::BasicDevice;
use crate::api_graphic_basic::device::SceneFrameOutput;
use crate::scene_renderer::SceneRenderFrame;

/// Renderer-neutral host for any ApiGraphicBasic scene frame.
///
/// The caller may use it for a 3D viewport, schematic, PCB canvas, or a
/// future runtime canvas. In GPU mode the BasicDevice must share the same WGPU
/// device as the presentation host; CPU output is uploaded only as fallback.
pub struct DirectSceneSurfaceHost {
    presenter: DirectCanvasPresenter,
    clear_color: [u8; 4],
}

impl DirectSceneSurfaceHost {
    pub fn new(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        clear_color: [u8; 4],
    ) -> Self {
        Self {
            presenter: DirectCanvasPresenter::new(device, color_format),
            clear_color,
        }
    }

    pub fn render_frame(
        &mut self,
        basic_device: &mut BasicDevice,
        presentation_device: &wgpu::Device,
        presentation_queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        frame: &SceneRenderFrame,
    ) {
        let output = basic_device.execute_scene_frame(frame);
        self.presenter.present(
            presentation_device,
            presentation_queue,
            target,
            output,
            [frame.width, frame.height],
            self.clear_color,
        );
    }

    /// Presents an already-rendered ApiGraphicBasic output. Native editor
    /// hosts use this when the shared `RenderRuntime` owns device selection
    /// and frame scheduling, so CAD/Electronics does not need to reach into a
    /// concrete `BasicDevice`.
    pub fn present_output(
        &mut self,
        presentation_device: &wgpu::Device,
        presentation_queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        output: SceneFrameOutput,
        source_size: [u32; 2],
    ) {
        self.presenter.present(
            presentation_device,
            presentation_queue,
            target,
            output,
            source_size,
            self.clear_color,
        );
    }

    pub fn presenter_mut(&mut self) -> &mut DirectCanvasPresenter {
        &mut self.presenter
    }
}

pub struct DirectCanvasPresenter {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    cpu_texture: Option<wgpu::Texture>,
    cpu_view: Option<Arc<wgpu::TextureView>>,
    cpu_size: [u32; 2],
    source_key: usize,
    source_bind_group: Option<wgpu::BindGroup>,
    last_cpu_source: Option<Arc<SceneFrameOutput>>,
}

pub(crate) struct PreparedCanvasSource {
    view: Arc<wgpu::TextureView>,
}
