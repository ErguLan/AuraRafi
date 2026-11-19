//! Platform present step for the browser canvas.
//!
//! ApiGraphicBasic renders the scene into an off-screen `Rgba8UnormSrgb`
//! texture. This module owns the only browser-specific graphics work: one
//! fullscreen triangle that samples that texture and writes it to the canvas
//! surface. It is not a second scene renderer and it never records scene
//! geometry.
//!
//! The shared `DirectCanvasPresenter` is deliberately not used here: it ties
//! its destination viewport to the source size, while the WASM host renders at
//! a bounded resolution that may be smaller than the canvas and must be
//! stretched by the browser present step.

use std::borrow::Cow;

use raf_render::api_graphic_basic::device::SceneFrameOutput;

/// Presents ApiGraphicBasic output to a browser surface.
pub(crate) struct CanvasBlit {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    source: Option<SourceBinding>,
    cpu_upload: Option<CpuUpload>,
}

struct SourceBinding {
    key: usize,
    bind_group: std::sync::Arc<wgpu::BindGroup>,
}

struct CpuUpload {
    texture: wgpu::Texture,
    view: std::sync::Arc<wgpu::TextureView>,
    width: u32,
    height: u32,
}

impl CanvasBlit {
    pub(crate) fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("raf_web.CanvasPresentShader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(CANVAS_PRESENT_WGSL)),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("raf_web.CanvasPresentLayout"),
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
            label: Some("raf_web.CanvasPresentPipelineLayout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // Sampling an `Rgba8UnormSrgb` source decodes to linear values. An
        // sRGB surface encodes them again in hardware; an 8-bit UNORM surface
        // needs the explicit transfer function.
        let fragment_entry = match color_format {
            format if format.is_srgb() => "fs_srgb_target",
            wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm => "fs_linear_target",
            _ => "fs_srgb_target",
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("raf_web.CanvasPresentPipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(fragment_entry),
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
            label: Some("raf_web.CanvasPresentSampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..wgpu::SamplerDescriptor::default()
        });

        Self {
            pipeline,
            bind_group_layout,
            sampler,
            source: None,
            cpu_upload: None,
        }
    }

    /// Present one scene output. `source_size` is the render size used for the
    /// scene frame and is required for the CPU recovery pixels.
    pub(crate) fn present(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        output: &SceneFrameOutput,
        source_size: [u32; 2],
    ) -> Result<(), String> {
        let source_view = match output {
            SceneFrameOutput::GpuTexture {
                view,
                width,
                height,
            } => {
                if *width == 0 || *height == 0 {
                    return Ok(());
                }
                self.present_view(device, queue, target, view.as_wgpu())?;
                return Ok(());
            }
            SceneFrameOutput::CpuPixels(pixels) => {
                self.upload_cpu_pixels(device, queue, pixels, source_size)?
            }
        };

        self.present_view(device, queue, target, source_view.as_ref())
    }

    fn present_view(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        source_view: &wgpu::TextureView,
    ) -> Result<(), String> {
        let Some(bind_group) = self.bind_group(device, source_view) else {
            return Ok(());
        };

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("raf_web.CanvasPresentEncoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("raf_web.CanvasPresentPass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // The fullscreen triangle covers the target; the clear
                        // only protects against a partially sized surface.
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, bind_group.as_ref(), &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit(std::iter::once(encoder.finish()));
        Ok(())
    }

    fn upload_cpu_pixels(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pixels: &[u8],
        source_size: [u32; 2],
    ) -> Result<std::sync::Arc<wgpu::TextureView>, String> {
        let width = source_size[0].max(1);
        let height = source_size[1].max(1);
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| "the CPU scene size overflowed".to_string())?;
        if pixels.len() < expected {
            return Err(format!(
                "the CPU scene buffer is too small: {} bytes, expected at least {expected}",
                pixels.len()
            ));
        }

        let needs_recreate = self
            .cpu_upload
            .as_ref()
            .map(|upload| upload.width != width || upload.height != height)
            .unwrap_or(true);
        if needs_recreate {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("raf_web.CpuCanvasUpload"),
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
            let view =
                std::sync::Arc::new(texture.create_view(&wgpu::TextureViewDescriptor::default()));
            self.cpu_upload = Some(CpuUpload {
                texture,
                view,
                width,
                height,
            });
        }

        let Some(upload) = self.cpu_upload.as_ref() else {
            return Err("the CPU canvas upload texture is unavailable".to_string());
        };
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &upload.texture,
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
        Ok(std::sync::Arc::clone(&upload.view))
    }

    fn bind_group(
        &mut self,
        device: &wgpu::Device,
        view: &wgpu::TextureView,
    ) -> Option<std::sync::Arc<wgpu::BindGroup>> {
        let key = view as *const wgpu::TextureView as usize;
        if let Some(source) = self.source.as_ref() {
            if source.key == key {
                return Some(std::sync::Arc::clone(&source.bind_group));
            }
        }

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("raf_web.CanvasPresentBindGroup"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        let bind_group = std::sync::Arc::new(bind_group);
        self.source = Some(SourceBinding {
            key,
            bind_group: std::sync::Arc::clone(&bind_group),
        });
        Some(bind_group)
    }
}

const CANVAS_PRESENT_WGSL: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0) var source_texture: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

@vertex
fn vs(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    var uvs = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(2.0, 1.0),
        vec2<f32>(0.0, -1.0)
    );
    var output: VertexOutput;
    output.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    output.uv = uvs[vertex_index];
    return output;
}

fn srgb_encode(color: vec3<f32>) -> vec3<f32> {
    let clamped = clamp(color, vec3<f32>(0.0), vec3<f32>(1.0));
    let low = clamped * 12.92;
    let high = 1.055 * pow(clamped, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(high, low, clamped <= vec3<f32>(0.0031308));
}

@fragment
fn fs_srgb_target(input: VertexOutput) -> @location(0) vec4<f32> {
    let sample = textureSample(source_texture, source_sampler, input.uv);
    return vec4<f32>(sample.rgb, 1.0);
}

@fragment
fn fs_linear_target(input: VertexOutput) -> @location(0) vec4<f32> {
    let sample = textureSample(source_texture, source_sampler, input.uv);
    return vec4<f32>(srgb_encode(sample.rgb), 1.0);
}
"#;
