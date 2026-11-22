use std::sync::mpsc;

use wgpu::util::DeviceExt;

use crate::{
    AdapterInfo, Backend, Batch, DrawMode, Error, Frame, FrameMetrics, FramePlan, Instance, Output,
    Result, Vertex, Workload,
};

const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const CAMERA_BYTES: u64 = size_of::<crate::RasterUniforms>() as u64;

pub struct Reference {
    device: wgpu::Device,
    queue: wgpu::Queue,
    scene: wgpu::RenderPipeline,
    read_depth: wgpu::RenderPipeline,
    overlay: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    camera: wgpu::Buffer,
    last_camera: Option<crate::RasterUniforms>,
    info: AdapterInfo,
    target: Option<Target>,
}

pub struct Resident {
    geometry: wgpu::Buffer,
    instance_offset: u64,
    index_offset: u64,
    indexed: bool,
    bindings: Vec<wgpu::BindGroup>,
    batches: Vec<Batch>,
}

struct Target {
    size: [u32; 2],
    color: wgpu::Texture,
    view: wgpu::TextureView,
    depth: wgpu::TextureView,
}

impl Reference {
    pub fn headless() -> Result<Self> {
        pollster::block_on(Self::open(wgpu::Backends::VULKAN))
    }

    #[cfg(windows)]
    pub fn headless_dx12() -> Result<Self> {
        pollster::block_on(Self::open(wgpu::Backends::DX12))
    }

    async fn open(api: wgpu::Backends) -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: api,
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                ..Default::default()
            })
            .await
            .ok_or_else(|| Error::Backend("reference graphics adapter unavailable".into()))?;
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    required_limits: wgpu::Limits::downlevel_defaults(),
                    memory_hints: wgpu::MemoryHints::MemoryUsage,
                    ..Default::default()
                },
                None,
            )
            .await
            .map_err(|error| Error::Backend(error.to_string()))?;
        let description = adapter.get_info();
        let limits = device.limits();
        let info = AdapterInfo {
            backend: match description.backend {
                wgpu::Backend::Dx12 => "WGPU/Direct3D 12",
                _ => "WGPU/Vulkan",
            },
            name: description.name,
            driver: format!("{} {}", description.driver, description.driver_info),
            hardware: description.device_type != wgpu::DeviceType::Cpu,
            max_texture: limits.max_texture_dimension_2d,
            max_buffer: limits.max_buffer_size,
            gpu_timestamps: false,
        };
        let camera = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: CAMERA_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    count: None,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    count: None,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    count: None,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(crate::SHADER.into()),
        });
        let scene = pipeline(&device, &pipeline_layout, &shader, true, true);
        let read_depth = pipeline(&device, &pipeline_layout, &shader, true, false);
        let overlay = pipeline(&device, &pipeline_layout, &shader, false, false);
        Ok(Self {
            device,
            queue,
            scene,
            read_depth,
            overlay,
            layout,
            sampler,
            camera,
            last_camera: None,
            info,
            target: None,
        })
    }
}

impl Backend for Reference {
    type Resident = Resident;
    fn info(&self) -> &AdapterInfo {
        &self.info
    }

    fn configure(&mut self, plan: &FramePlan) -> Result<()> {
        if plan.output != Output::Capture {
            return Err(Error::Invalid("reference benchmark needs a capture target"));
        }
        self.wait()?;
        let color = texture(
            &self.device,
            plan.extent,
            COLOR,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        let depth = texture(
            &self.device,
            plan.extent,
            DEPTH,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        )
        .create_view(&Default::default());
        let view = color.create_view(&Default::default());
        self.target = Some(Target {
            size: plan.extent,
            color,
            view,
            depth,
        });
        Ok(())
    }

    fn upload(&mut self, workload: &Workload) -> Result<Resident> {
        let vertices: &[u8] = bytemuck::cast_slice(&workload.vertices);
        let instances: &[u8] = bytemuck::cast_slice(&workload.instances);
        let indices: &[u8] = bytemuck::cast_slice(&workload.indices);
        let instance_offset = vertices.len() as u64;
        let index_offset = instance_offset + instances.len() as u64;
        let geometry = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: index_offset + indices.len() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::INDEX,
            mapped_at_creation: true,
        });
        {
            let mut view = geometry.slice(..).get_mapped_range_mut();
            view[..instance_offset as usize].copy_from_slice(vertices);
            view[instance_offset as usize..index_offset as usize].copy_from_slice(instances);
            view[index_offset as usize..].copy_from_slice(indices);
        }
        geometry.unmap();
        let bindings = workload
            .textures
            .iter()
            .map(|image| {
                let texture = self.device.create_texture_with_data(
                    &self.queue,
                    &wgpu::TextureDescriptor {
                        size: extent(image.size),
                        format: COLOR,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING,
                        dimension: wgpu::TextureDimension::D2,
                        label: None,
                        mip_level_count: 1,
                        sample_count: 1,
                        view_formats: &[],
                    },
                    wgpu::util::TextureDataOrder::LayerMajor,
                    &image.rgba,
                );
                let view = texture.create_view(&Default::default());
                self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &self.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: self.camera.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                    ],
                })
            })
            .collect();
        Ok(Resident {
            geometry,
            instance_offset,
            index_offset,
            indexed: !indices.is_empty(),
            bindings,
            batches: workload.batches.clone(),
        })
    }

    fn draw(
        &mut self,
        resident: &Resident,
        plan: &FramePlan,
        frame: &Frame,
    ) -> Result<FrameMetrics> {
        // The benchmark reference is not the product executor: it validates the
        // normalized scene rectangle but keeps drawing the full target so A/B
        // captures remain comparable with older reference runs.
        crate::device::validate_viewport(frame.viewport)?;
        let target = self.target.as_ref().ok_or(Error::SurfaceOutdated)?;
        let uniforms = frame.uniforms(plan.extent);
        let changed = self.last_camera != Some(uniforms);
        if changed {
            self.queue
                .write_buffer(&self.camera, 0, bytemuck::bytes_of(&uniforms));
            self.last_camera = Some(uniforms);
        }
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let [r, g, b, a] = frame.clear.map(f64::from);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_vertex_buffer(0, resident.geometry.slice(..resident.instance_offset));
            pass.set_vertex_buffer(
                1,
                resident
                    .geometry
                    .slice(resident.instance_offset..resident.index_offset),
            );
            if resident.indexed {
                pass.set_index_buffer(
                    resident.geometry.slice(resident.index_offset..),
                    wgpu::IndexFormat::Uint32,
                );
            }
            let scene_end = resident
                .batches
                .partition_point(|batch| batch.mode.is_scene());
            for index in &plan.graph.order {
                let batches = match index {
                    0 => &resident.batches[..scene_end],
                    1 => &resident.batches[scene_end..],
                    _ => break,
                };
                let mut mode = None;
                let mut bound = u32::MAX;
                for batch in batches {
                    if mode != Some(batch.mode) {
                        mode = Some(batch.mode);
                        let pipeline = match batch.mode {
                            DrawMode::Scene => &self.scene,
                            DrawMode::DepthReadOnly => &self.read_depth,
                            DrawMode::Overlay | DrawMode::SceneNoDepth => &self.overlay,
                        };
                        pass.set_pipeline(pipeline);
                    }
                    if bound != batch.texture {
                        bound = batch.texture;
                        pass.set_bind_group(0, &resident.bindings[bound as usize], &[]);
                    }
                    if let Some(indices) = &batch.indices {
                        pass.draw_indexed(indices.clone(), 0, batch.instances.clone());
                    } else {
                        pass.draw(batch.vertices.clone(), batch.instances.clone());
                    }
                }
            }
        }
        self.queue.submit([encoder.finish()]);
        Ok(FrameMetrics {
            draw_calls: resident.batches.len() as u32,
            submits: 1,
            upload_bytes: if changed { CAMERA_BYTES } else { 0 },
            gpu_ms: None,
        })
    }

    fn wait(&mut self) -> Result<()> {
        self.device.poll(wgpu::Maintain::Wait);
        Ok(())
    }

    fn capture(&mut self) -> Result<Vec<u8>> {
        let target = self.target.as_ref().ok_or(Error::SurfaceOutdated)?;
        let row = target.size[0] * 4;
        let pitch =
            row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            mapped_at_creation: false,
            size: u64::from(pitch) * u64::from(target.size[1]),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            target.color.as_image_copy(),
            wgpu::ImageCopyBuffer {
                buffer: &buffer,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(pitch),
                    rows_per_image: None,
                },
            },
            extent(target.size),
        );
        self.queue.submit([encoder.finish()]);
        let (send, recv) = mpsc::sync_channel(1);
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = send.send(result);
            });
        self.wait()?;
        recv.recv()
            .map_err(|error| Error::Backend(error.to_string()))?
            .map_err(|error| Error::Backend(error.to_string()))?;
        let pixels = buffer
            .slice(..)
            .get_mapped_range()
            .chunks_exact(pitch as usize)
            .flat_map(|data| data[..row as usize].iter().copied())
            .collect();
        Ok(pixels)
    }
}

fn texture(
    device: &wgpu::Device,
    size: [u32; 2],
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: extent(size),
        format,
        usage,
        dimension: wgpu::TextureDimension::D2,
        mip_level_count: 1,
        sample_count: 1,
        view_formats: &[],
    })
}

fn extent([width, height]: [u32; 2]) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    }
}

fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    scene: bool,
    write_depth: bool,
) -> wgpu::RenderPipeline {
    let vertices =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x4, 8 => Float32x3];
    let instances = wgpu::vertex_attr_array![3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4, 9 => Uint32];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(layout),
        cache: None,
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(if scene { "scene" } else { "overlay" }),
            compilation_options: Default::default(),
            buffers: &[
                wgpu::VertexBufferLayout {
                    array_stride: size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &vertices,
                },
                wgpu::VertexBufferLayout {
                    array_stride: size_of::<Instance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &instances,
                },
            ],
        },
        primitive: wgpu::PrimitiveState::default(),
        multisample: wgpu::MultisampleState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH,
            depth_write_enabled: write_depth,
            depth_compare: if scene {
                wgpu::CompareFunction::LessEqual
            } else {
                wgpu::CompareFunction::Always
            },
            stencil: Default::default(),
            bias: Default::default(),
        }),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("paint"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: COLOR,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview: None,
    })
}
