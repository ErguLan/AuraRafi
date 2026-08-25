//! Native presentation of ApiGraphicBasic scene textures.
//!
//! A viewport, schematic, or PCB surface may render off-screen and use this
//! presenter to reach a native WGPU swapchain without becoming a foreign UI
//! image.

use std::borrow::Cow;
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanvasTargetRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl CanvasTargetRect {
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub const fn full(size: [u32; 2]) -> Self {
        Self::new(0, 0, size[0], size[1])
    }

    pub(crate) fn clipped(self, target_size: [u32; 2]) -> Self {
        let x = self.x.min(target_size[0]);
        let y = self.y.min(target_size[1]);
        Self {
            x,
            y,
            width: self.width.min(target_size[0].saturating_sub(x)),
            height: self.height.min(target_size[1].saturating_sub(y)),
        }
    }
}

pub(crate) struct PreparedCanvasSource {
    view: Arc<wgpu::TextureView>,
}

impl DirectCanvasPresenter {
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ApiGraphicBasic.CanvasPresenterShader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(CANVAS_PRESENTER_WGSL)),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ApiGraphicBasic.CanvasPresenterLayout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ApiGraphicBasic.CanvasPresenterPipelineLayout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ApiGraphicBasic.CanvasPresenterPipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ApiGraphicBasic.CanvasPresenterSampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..wgpu::SamplerDescriptor::default()
        });

        Self {
            pipeline,
            bind_group_layout,
            sampler,
            cpu_texture: None,
            cpu_view: None,
            cpu_size: [0, 0],
            source_key: 0,
            source_bind_group: None,
        }
    }

    pub fn present(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        output: SceneFrameOutput,
        source_size: [u32; 2],
        clear_color: [u8; 4],
    ) {
        let source = self.prepare_output(device, queue, output, source_size);
        let Some(source) = source else {
            return;
        };
        self.present_texture(
            device,
            queue,
            target,
            source,
            source_size,
            CanvasTargetRect::full(source_size),
            clear_color,
        );
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

    pub(crate) fn prepare_output(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        output: SceneFrameOutput,
        source_size: [u32; 2],
    ) -> Option<PreparedCanvasSource> {
        let view = match output {
            SceneFrameOutput::GpuTexture { view, .. } => view.arc(),
            SceneFrameOutput::CpuPixels(pixels) => {
                self.upload_cpu_pixels(device, queue, &pixels, source_size)?
            }
        };
        Some(PreparedCanvasSource { view })
    }

    fn present_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        source: PreparedCanvasSource,
        target_size: [u32; 2],
        target_rect: CanvasTargetRect,
        clear_color: [u8; 4],
    ) {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ApiGraphicBasic.CanvasPresenterEncoder"),
        });
        self.encode_prepared(
            device,
            &mut encoder,
            target,
            target_size,
            target_rect,
            source,
            wgpu::LoadOp::Clear(color_from_bytes(clear_color)),
        );
        queue.submit(std::iter::once(encoder.finish()));
    }

    pub(crate) fn encode_prepared(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        target_size: [u32; 2],
        target_rect: CanvasTargetRect,
        source: PreparedCanvasSource,
        load: wgpu::LoadOp<wgpu::Color>,
    ) {
        let rect = target_rect.clipped(target_size);
        if rect.width == 0 || rect.height == 0 {
            return;
        }
        let key = Arc::as_ptr(&source.view) as usize;
        if self.source_key != key || self.source_bind_group.is_none() {
            self.source_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ApiGraphicBasic.CanvasPresenterBindGroup"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(source.view.as_ref()),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            }));
            self.source_key = key;
        }
        let Some(bind_group) = self.source_bind_group.as_ref() else {
            return;
        };

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ApiGraphicBasic.CanvasPresenterPass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_viewport(
            rect.x as f32,
            rect.y as f32,
            rect.width as f32,
            rect.height as f32,
            0.0,
            1.0,
        );
        pass.set_scissor_rect(rect.x, rect.y, rect.width, rect.height);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..6, 0..1);
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

fn color_from_bytes(color: [u8; 4]) -> wgpu::Color {
    wgpu::Color {
        r: f64::from(color[0]) / 255.0,
        g: f64::from(color[1]) / 255.0,
        b: f64::from(color[2]) / 255.0,
        a: f64::from(color[3]) / 255.0,
    }
}

const CANVAS_PRESENTER_WGSL: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0) var source_texture: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

@vertex
fn vs(@builtin(vertex_index) index: u32) -> VertexOutput {
    let positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, 1.0), vec2<f32>(1.0, -1.0),
        vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, -1.0)
    );
    let uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0)
    );
    var output: VertexOutput;
    output.position = vec4<f32>(positions[index], 0.0, 1.0);
    output.uv = uvs[index];
    return output;
}

@fragment
fn fs(input: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(source_texture, source_sampler, input.uv);
}
"#;
