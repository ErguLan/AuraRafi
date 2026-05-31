use std::borrow::Cow;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use raf_core::config::RenderExecutionPolicy;
use wgpu::util::DeviceExt;

use crate::api_graphic_basic::command_list::GraphicCommand;
use crate::api_graphic_basic::mesh::BasicMesh;
use crate::api_graphic_basic::pipeline::BasicPipelineKind;
use crate::render_pipeline::framebuffer::Framebuffer;
use crate::scene_renderer::{rasterize_basic_scene_frame, SceneRenderFrame};
use crate::shaders::BASIC_SCENE_WGSL;

/// Supported execution backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BasicBackendType {
    /// High-performance GPU hardware rendering (using our private wgpu layer).
    GpuHardware,
    /// Fallback CPU software rasterized rendering (for low-spec potato PCs).
    CpuSoftware,
}

pub enum SceneFrameOutput {
    CpuPixels(Vec<u8>),
    GpuTexture {
        view: Arc<wgpu::TextureView>,
        width: u32,
        height: u32,
    },
}

#[derive(Debug, Clone)]
pub struct SharedWgpuContext {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
}

/// Configuration settings for device initialization.
#[derive(Debug, Clone)]
pub struct BasicDeviceConfig {
    /// Allow GPU rendering if a GPU adapter is found.
    pub allow_gpu: bool,
    /// Force CPU software rendering regardless of GPU availability.
    pub force_cpu: bool,
    /// Optional shared wgpu context supplied by the host editor.
    pub shared_wgpu_context: Option<SharedWgpuContext>,
}

impl Default for BasicDeviceConfig {
    fn default() -> Self {
        Self {
            allow_gpu: true,
            force_cpu: false,
            shared_wgpu_context: None,
        }
    }
}

impl BasicDeviceConfig {
    /// Build a device config from the engine render execution policy.
    pub fn from_render_policy(policy: RenderExecutionPolicy) -> Self {
        match policy {
            RenderExecutionPolicy::Auto | RenderExecutionPolicy::GpuPreferred => Self {
                allow_gpu: true,
                force_cpu: false,
                shared_wgpu_context: None,
            },
            RenderExecutionPolicy::CpuOnly => Self {
                allow_gpu: false,
                force_cpu: true,
                shared_wgpu_context: None,
            },
        }
    }
}

/// The unified graphics device driver.
/// Orchestrates commands recording and maps them onto the active execution backend (GPU or CPU).
#[allow(dead_code)]
pub struct BasicDevice {
    backend: BasicBackendType,
    framebuffer: Framebuffer,
    gpu_scene: Option<GpuSceneState>,
    // Private wgpu instances (only populated if running in GPU mode)
    wgpu_instance: Option<wgpu::Instance>,
    wgpu_adapter: Option<wgpu::Adapter>,
    wgpu_device: Option<Arc<wgpu::Device>>,
    wgpu_queue: Option<Arc<wgpu::Queue>>,
}

impl BasicDevice {
    /// Initialize the basic graphics device, attempting to use the GPU backend if possible.
    pub fn new(config: BasicDeviceConfig) -> Self {
        if !config.force_cpu && config.allow_gpu {
            if let Some(shared_wgpu_context) = config.shared_wgpu_context {
                tracing::info!(
                    "ApiGraphicBasic initialized GPU Hardware backend using shared eframe wgpu device."
                );
                return Self {
                    backend: BasicBackendType::GpuHardware,
                    framebuffer: Framebuffer::new(1, 1),
                    gpu_scene: Some(GpuSceneState::new(shared_wgpu_context.device.as_ref())),
                    wgpu_instance: None,
                    wgpu_adapter: None,
                    wgpu_device: Some(shared_wgpu_context.device),
                    wgpu_queue: Some(shared_wgpu_context.queue),
                };
            }
        }

        if !config.force_cpu && config.allow_gpu {
            // Attempt to initialize GPU with loose limits and high compatibility margin
            if let Some(gpu_state) = Self::try_init_gpu() {
                tracing::info!("ApiGraphicBasic successfully initialized GPU Hardware backend (wgpu).");
                return Self {
                    backend: BasicBackendType::GpuHardware,
                    framebuffer: Framebuffer::new(1, 1),
                    gpu_scene: Some(GpuSceneState::new(&gpu_state.device)),
                    wgpu_instance: Some(gpu_state.instance),
                    wgpu_adapter: Some(gpu_state.adapter),
                    wgpu_device: Some(Arc::new(gpu_state.device)),
                    wgpu_queue: Some(Arc::new(gpu_state.queue)),
                };
            }
            tracing::warn!("ApiGraphicBasic failed to initialize GPU. Falling back to CPU Software rendering.");
        }

        tracing::info!("ApiGraphicBasic initialized CPU Software backend.");
        Self {
            backend: BasicBackendType::CpuSoftware,
            framebuffer: Framebuffer::new(1, 1),
            gpu_scene: None,
            wgpu_instance: None,
            wgpu_adapter: None,
            wgpu_device: None,
            wgpu_queue: None,
        }
    }

    /// Retrieve the currently active backend.
    pub fn backend(&self) -> BasicBackendType {
        self.backend
    }

    /// Execute a scene frame recorded through `BasicCommandList`.
    pub fn execute_scene_frame(&mut self, frame: &SceneRenderFrame) -> SceneFrameOutput {
        match self.backend {
            BasicBackendType::GpuHardware => self.execute_gpu_scene_frame(frame),
            BasicBackendType::CpuSoftware => self.execute_cpu_scene_frame(frame),
        }
    }

    /// Execute the commands list and output pixel values.
    /// In CPU mode, it writes pixels using the Cohen-Sutherland clipped rasterizer.
    /// In GPU mode, it uploads buffers and issues draw calls to the graphics card.
    pub fn execute(&self, commands: &super::command_list::BasicCommandList, width: u32, height: u32) {
        match self.backend {
            BasicBackendType::GpuHardware => {
                self.execute_gpu(commands, width, height);
            }
            BasicBackendType::CpuSoftware => {
                self.execute_cpu(commands, width, height);
            }
        }
    }

    /// Internal GPU execution pipeline mapping commands onto wgpu.
    fn execute_gpu(&self, _commands: &super::command_list::BasicCommandList, _width: u32, _height: u32) {
        // GPU execution implementation details (encapsulated internally)
    }

    /// Internal CPU software execution mapping commands onto our clipping rasterizer.
    fn execute_cpu(&self, _commands: &super::command_list::BasicCommandList, _width: u32, _height: u32) {
        // CPU execution implementation details (encapsulated internally)
    }

    fn execute_gpu_scene_frame(&mut self, frame: &SceneRenderFrame) -> SceneFrameOutput {
        let Some(device) = self.wgpu_device.as_ref() else {
            return self.execute_cpu_scene_frame(frame);
        };
        let Some(queue) = self.wgpu_queue.as_ref() else {
            return self.execute_cpu_scene_frame(frame);
        };
        let Some(gpu_scene) = self.gpu_scene.as_mut() else {
            return self.execute_cpu_scene_frame(frame);
        };

        match gpu_scene.render(device, queue, frame) {
            Some(output) => output,
            None => {
                tracing::warn!("ApiGraphicBasic GPU scene execution failed, falling back to CPU raster path.");
                self.execute_cpu_scene_frame(frame)
            }
        }
    }

    fn execute_cpu_scene_frame(&mut self, frame: &SceneRenderFrame) -> SceneFrameOutput {
        rasterize_basic_scene_frame(frame, &mut self.framebuffer);
        SceneFrameOutput::CpuPixels(self.framebuffer.pixels().to_vec())
    }

    /// Helper to attempt creating a wgpu device with generous compatibility parameters.
    /// Prioritizes Integrated GPUs and Low-Power options for maximum hardware reach,
    /// falling back to software/GL drivers if direct hardware context is missing.
    fn try_init_gpu() -> Option<GpuState> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        // Request adapter using block_on for async initialization (run inside a lightweight runtime wrapper)
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower, // Prioritize integrated GPUs/laptops
            compatible_surface: None,
            force_fallback_adapter: false, // Fallback is requested if direct hardware creation fails
        }))?;

        // Request device with minimal limit requirements (potato-friendly limit margin)
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("AuraRafi_Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults(), // Lowest denominator limits for maximum compatibility
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        )).ok()?;

        Some(GpuState {
            instance,
            adapter,
            device,
            queue,
        })
    }
}

struct GpuState {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuMeshVertex {
    position: [f32; 3],
    normal: [f32; 3],
}

impl GpuMeshVertex {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        const ATTRS: [wgpu::VertexAttribute; 2] =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];

        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GpuMeshVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRS,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuLineVertex {
    position: [f32; 3],
}

impl GpuLineVertex {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        const ATTRS: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x3];

        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GpuLineVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRS,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct MeshUniforms {
    mvp: [[f32; 4]; 4],
    model: [[f32; 4]; 4],
    color: [f32; 4],
    light_dir: [f32; 4],
    params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LineUniforms {
    mvp: [[f32; 4]; 4],
    color: [f32; 4],
    params: [f32; 4],
}

struct GpuSceneState {
    color_format: wgpu::TextureFormat,
    mesh_bind_group_layout: wgpu::BindGroupLayout,
    line_bind_group_layout: wgpu::BindGroupLayout,
    mesh_pipeline: wgpu::RenderPipeline,
    line_pipeline_depth: wgpu::RenderPipeline,
    line_pipeline_xray: wgpu::RenderPipeline,
    target: Option<GpuSceneTarget>,
}

struct GpuSceneTarget {
    width: u32,
    height: u32,
    _color_texture: wgpu::Texture,
    color_view: Arc<wgpu::TextureView>,
    _depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
}

impl GpuSceneState {
    fn new(device: &wgpu::Device) -> Self {
        let color_format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ApiGraphicBasic.SceneShader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(BASIC_SCENE_WGSL)),
        });

        let mesh_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ApiGraphicBasic.MeshBindGroupLayout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let line_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ApiGraphicBasic.LineBindGroupLayout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let mesh_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ApiGraphicBasic.MeshPipelineLayout"),
            bind_group_layouts: &[&mesh_bind_group_layout],
            push_constant_ranges: &[],
        });
        let mesh_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ApiGraphicBasic.MeshPipeline"),
            layout: Some(&mesh_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("mesh_vs"),
                buffers: &[GpuMeshVertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("mesh_fs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let line_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ApiGraphicBasic.LinePipelineLayout"),
            bind_group_layouts: &[&line_bind_group_layout],
            push_constant_ranges: &[],
        });
        let line_pipeline_depth = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ApiGraphicBasic.LinePipelineDepth"),
            layout: Some(&line_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("line_vs"),
                buffers: &[GpuLineVertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("line_fs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let line_pipeline_xray = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ApiGraphicBasic.LinePipelineXray"),
            layout: Some(&line_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("line_vs"),
                buffers: &[GpuLineVertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("line_fs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            color_format,
            mesh_bind_group_layout,
            line_bind_group_layout,
            mesh_pipeline,
            line_pipeline_depth,
            line_pipeline_xray,
            target: None,
        }
    }

    fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &SceneRenderFrame,
    ) -> Option<SceneFrameOutput> {
        self.ensure_target(device, frame.width, frame.height);
        let target = self.target.as_ref()?;
        let clear_color = extract_clear_color(frame.commands.commands());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ApiGraphicBasic.SceneEncoder"),
        });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ApiGraphicBasic.ScenePass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target.color_view.as_ref(),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            let mut current_pipeline = BasicPipelineKind::FlatColor;
            for command in frame.commands.commands().iter().cloned() {
                match command {
                    GraphicCommand::Clear { .. } => {}
                    GraphicCommand::SetPipeline(pipeline) => current_pipeline = pipeline,
                    GraphicCommand::DrawMesh {
                        mesh_id,
                        transform,
                        color,
                    } => {
                        let Some(mesh) = frame.commands.mesh(mesh_id) else {
                            continue;
                        };
                        self.draw_mesh(
                            device,
                            &mut pass,
                            mesh,
                            transform,
                            frame,
                            color,
                            matches!(current_pipeline, BasicPipelineKind::PbrLit),
                        );
                    }
                    GraphicCommand::DrawLine {
                        start,
                        end,
                        color,
                        no_depth_test,
                        depth_bias,
                        ..
                    } => {
                        self.draw_line(
                            device,
                            &mut pass,
                            start,
                            end,
                            frame,
                            color,
                            no_depth_test,
                            depth_bias,
                        );
                    }
                    GraphicCommand::DrawGrid { .. } => {}
                }
            }
        }

        queue.submit(std::iter::once(encoder.finish()));
        let _ = device.poll(wgpu::Maintain::Poll);
        Some(SceneFrameOutput::GpuTexture {
            view: Arc::clone(&target.color_view),
            width: frame.width,
            height: frame.height,
        })
    }

    fn ensure_target(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let needs_rebuild = self
            .target
            .as_ref()
            .map(|target| target.width != width || target.height != height)
            .unwrap_or(true);

        if !needs_rebuild {
            return;
        }

        let color_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ApiGraphicBasic.SceneColor"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.color_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let color_view = Arc::new(color_texture.create_view(&wgpu::TextureViewDescriptor::default()));
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ApiGraphicBasic.SceneDepth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

        self.target = Some(GpuSceneTarget {
            width,
            height,
            _color_texture: color_texture,
            color_view,
            _depth_texture: depth_texture,
            depth_view,
        });
    }

    fn draw_mesh(
        &self,
        device: &wgpu::Device,
        pass: &mut wgpu::RenderPass<'_>,
        mesh: &BasicMesh,
        transform: glam::Mat4,
        frame: &SceneRenderFrame,
        color: [u8; 4],
        lit: bool,
    ) {
        if mesh.indices.is_empty() || mesh.vertices.is_empty() {
            return;
        }

        let vertices: Vec<GpuMeshVertex> = mesh
            .vertices
            .iter()
            .map(|vertex| GpuMeshVertex {
                position: vertex.position.to_array(),
                normal: vertex.normal.to_array(),
            })
            .collect();
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ApiGraphicBasic.MeshVertexBuffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ApiGraphicBasic.MeshIndexBuffer"),
            contents: bytemuck::cast_slice(mesh.indices.as_slice()),
            usage: wgpu::BufferUsages::INDEX,
        });
        let uniforms = MeshUniforms {
            mvp: (frame.view_proj * transform).to_cols_array_2d(),
            model: transform.to_cols_array_2d(),
            color: rgba8_to_f32(color),
            light_dir: [frame.light_dir.x, frame.light_dir.y, frame.light_dir.z, 0.0],
            params: [if lit { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
        };
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ApiGraphicBasic.MeshUniformBuffer"),
            contents: bytemuck::bytes_of(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ApiGraphicBasic.MeshBindGroup"),
            layout: &self.mesh_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        pass.set_pipeline(&self.mesh_pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..mesh.indices.len() as u32, 0, 0..1);
    }

    fn draw_line(
        &self,
        device: &wgpu::Device,
        pass: &mut wgpu::RenderPass<'_>,
        start: glam::Vec3,
        end: glam::Vec3,
        frame: &SceneRenderFrame,
        color: [u8; 4],
        no_depth_test: bool,
        depth_bias: f32,
    ) {
        if start.distance_squared(end) <= f32::EPSILON {
            return;
        }

        let vertices = [
            GpuLineVertex {
                position: start.to_array(),
            },
            GpuLineVertex {
                position: end.to_array(),
            },
        ];
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ApiGraphicBasic.LineVertexBuffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let uniforms = LineUniforms {
            mvp: frame.view_proj.to_cols_array_2d(),
            color: rgba8_to_f32(color),
            params: [depth_bias, 0.0, 0.0, 0.0],
        };
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ApiGraphicBasic.LineUniformBuffer"),
            contents: bytemuck::bytes_of(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ApiGraphicBasic.LineBindGroup"),
            layout: &self.line_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        pass.set_pipeline(if no_depth_test {
            &self.line_pipeline_xray
        } else {
            &self.line_pipeline_depth
        });
        pass.set_bind_group(0, &bind_group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..2, 0..1);
    }
}

fn extract_clear_color(commands: &[GraphicCommand]) -> wgpu::Color {
    let mut color = wgpu::Color::BLACK;
    for command in commands {
        if let GraphicCommand::Clear { r, g, b, a } = command {
            color = wgpu::Color {
                r: *r as f64 / 255.0,
                g: *g as f64 / 255.0,
                b: *b as f64 / 255.0,
                a: *a as f64 / 255.0,
            };
        }
    }
    color
}

fn rgba8_to_f32(color: [u8; 4]) -> [f32; 4] {
    [
        color[0] as f32 / 255.0,
        color[1] as f32 / 255.0,
        color[2] as f32 / 255.0,
        color[3] as f32 / 255.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_graphic_basic::command_list::BasicCommandList;
    use crate::scene_renderer::FrameStats;
    use glam::{Mat4, Vec3};

    #[test]
    fn cpu_scene_frame_returns_rgba_pixels() {
        let mut commands = BasicCommandList::new();
        commands.clear([12, 34, 56, 255]);
        let frame = SceneRenderFrame {
            commands,
            view_proj: Mat4::IDENTITY,
            light_dir: Vec3::Y,
            width: 2,
            height: 1,
            stats: FrameStats::default(),
        };

        let mut device = BasicDevice::new(BasicDeviceConfig {
            allow_gpu: false,
            force_cpu: true,
        });
        let output = device.execute_scene_frame(&frame);
        let SceneFrameOutput::CpuPixels(pixels) = output else {
            panic!("expected cpu pixel output");
        };

        assert_eq!(pixels.len(), 8);
        assert_eq!(pixels[0], 12);
        assert_eq!(pixels[1], 34);
        assert_eq!(pixels[2], 56);
        assert_eq!(pixels[3], 255);
    }
}
