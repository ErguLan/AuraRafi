//! CPU scene pixels uploaded into a reusable GPU texture before presentation.

use std::sync::Arc;

use crate::api_graphic_basic::device::SceneFrameOutput;

use super::{CanvasTargetRect, DirectCanvasPresenter, PreparedCanvasSource};

impl DirectCanvasPresenter {
    /// Drop retained scene output without recreating presentation pipelines.
    pub fn release_source(&mut self) {
        self.source_bind_group = None;
        self.source_key = 0;
        self.cpu_view = None;
        self.cpu_texture = None;
        self.cpu_size = [0, 0];
        self.last_cpu_source = None;
    }

    /// Presents a borrowed CPU RGBA buffer without allocating an intermediate
    /// `SceneFrameOutput`. This is used by retained UI recovery rendering.
    pub fn present_cpu_pixels(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        pixels: &[u8],
        source_size: [u32; 2],
        clear_color: [u8; 4],
    ) {
        let Some(source) = self.upload_cpu_pixels(device, queue, pixels, source_size) else {
            return;
        };
        self.present_texture(
            device,
            queue,
            target,
            PreparedCanvasSource { view: source },
            source_size,
            CanvasTargetRect::full(source_size),
            clear_color,
        );
    }

    /// A retained CPU canvas does not upload its pixels again when only UI
    /// chrome changes. Retaining the Arc also prevents pointer-identity reuse.
    pub(crate) fn prepare_shared_output(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        output: &Arc<SceneFrameOutput>,
        source_size: [u32; 2],
    ) -> Option<PreparedCanvasSource> {
        if matches!(output.as_ref(), SceneFrameOutput::CpuPixels(_)) {
            if self
                .last_cpu_source
                .as_ref()
                .is_some_and(|cached| Arc::ptr_eq(cached, output))
                && self.cpu_size == source_size
            {
                return self
                    .cpu_view
                    .clone()
                    .map(|view| PreparedCanvasSource { view });
            }
            let source = self.prepare_output(device, queue, output.as_ref(), source_size)?;
            self.last_cpu_source = Some(output.clone());
            Some(source)
        } else {
            self.last_cpu_source = None;
            self.prepare_output(device, queue, output.as_ref(), source_size)
        }
    }

    pub(crate) fn prepare_output(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        output: &SceneFrameOutput,
        source_size: [u32; 2],
    ) -> Option<PreparedCanvasSource> {
        self.last_cpu_source = None;
        let view = match output {
            SceneFrameOutput::GpuTexture { view, .. } => view.arc(),
            SceneFrameOutput::CpuPixels(pixels) => {
                self.upload_cpu_pixels(device, queue, pixels, source_size)?
            }
        };
        Some(PreparedCanvasSource { view })
    }

    fn upload_cpu_pixels(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pixels: &[u8],
        source_size: [u32; 2],
    ) -> Option<Arc<wgpu::TextureView>> {
        let width = source_size[0].max(1);
        let height = source_size[1].max(1);
        if pixels.len() < width as usize * height as usize * 4 {
            return None;
        }
        if self.cpu_texture.is_none() || self.cpu_size != [width, height] {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("ApiGraphicBasic.CpuCanvasUpload"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            self.cpu_view = Some(Arc::new(
                texture.create_view(&wgpu::TextureViewDescriptor::default()),
            ));
            self.cpu_texture = Some(texture);
            self.cpu_size = [width, height];
        }
        let texture = self.cpu_texture.as_ref()?;
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.cpu_view.clone()
    }
}
