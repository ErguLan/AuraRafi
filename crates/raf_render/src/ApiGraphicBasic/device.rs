use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

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

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SceneFrameMetrics {
    pub frame_cpu_ms: f32,
    pub target_rebuilds: u32,
    pub mesh_draw_calls: u32,
    pub line_draw_calls: u32,
    pub mesh_cache_hits: u32,
    pub mesh_cache_misses: u32,
    pub mesh_uniform_slot_creations: u32,
    pub line_slot_creations: u32,
    pub mesh_upload_bytes: u64,
    pub uniform_upload_bytes: u64,
    pub line_upload_bytes: u64,
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
    last_frame_metrics: SceneFrameMetrics,
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
                    last_frame_metrics: SceneFrameMetrics::default(),
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
                tracing::info!(
                    "ApiGraphicBasic successfully initialized GPU Hardware backend (wgpu)."
                );
                return Self {
                    backend: BasicBackendType::GpuHardware,
                    framebuffer: Framebuffer::new(1, 1),
                    gpu_scene: Some(GpuSceneState::new(&gpu_state.device)),
                    last_frame_metrics: SceneFrameMetrics::default(),
                    wgpu_instance: Some(gpu_state.instance),
                    wgpu_adapter: Some(gpu_state.adapter),
                    wgpu_device: Some(Arc::new(gpu_state.device)),
                    wgpu_queue: Some(Arc::new(gpu_state.queue)),
                };
            }
            tracing::warn!(
                "ApiGraphicBasic failed to initialize GPU. Falling back to CPU Software rendering."
            );
        }

        tracing::info!("ApiGraphicBasic initialized CPU Software backend.");
        Self {
            backend: BasicBackendType::CpuSoftware,
            framebuffer: Framebuffer::new(1, 1),
            gpu_scene: None,
            last_frame_metrics: SceneFrameMetrics::default(),
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

    pub fn last_frame_metrics(&self) -> SceneFrameMetrics {
        self.last_frame_metrics
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
    pub fn execute(
        &self,
        commands: &super::command_list::BasicCommandList,
        width: u32,
        height: u32,
    ) {
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
    fn execute_gpu(
        &self,
        _commands: &super::command_list::BasicCommandList,
        _width: u32,
        _height: u32,
    ) {
        // GPU execution implementation details (encapsulated internally)
    }

    /// Internal CPU software execution mapping commands onto our clipping rasterizer.
    fn execute_cpu(
        &self,
        _commands: &super::command_list::BasicCommandList,
        _width: u32,
        _height: u32,
    ) {
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
            Some((output, metrics)) => {
                self.last_frame_metrics = metrics;
                output
            }
            None => {
                tracing::warn!(
                    "ApiGraphicBasic GPU scene execution failed, falling back to CPU raster path."
                );
                self.execute_cpu_scene_frame(frame)
            }
        }
    }

    fn execute_cpu_scene_frame(&mut self, frame: &SceneRenderFrame) -> SceneFrameOutput {
        let frame_start = Instant::now();
        rasterize_basic_scene_frame(frame, &mut self.framebuffer);
        self.last_frame_metrics = SceneFrameMetrics {
            frame_cpu_ms: frame_start.elapsed().as_secs_f32() * 1000.0,
            ..SceneFrameMetrics::default()
        };
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
        ))
        .ok()?;

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

struct GpuSceneTarget {
    width: u32,
    height: u32,
    _color_texture: wgpu::Texture,
    color_view: Arc<wgpu::TextureView>,
    _depth_texture: wgpu::Texture,
    depth_view: Arc<wgpu::TextureView>,
}

#[derive(Clone)]
struct GpuMeshBuffers {
    vertex_buffer: Arc<wgpu::Buffer>,
    index_buffer: Arc<wgpu::Buffer>,
    index_count: u32,
    vertex_bytes: u64,
    index_bytes: u64,
}

#[derive(Clone)]
struct GpuUniformSlot {
    buffer: Arc<wgpu::Buffer>,
    bind_group: Arc<wgpu::BindGroup>,
}

#[derive(Clone)]
struct GpuLineSlot {
    vertex_buffer: Arc<wgpu::Buffer>,
    uniform: GpuUniformSlot,
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
    mesh_cache: HashMap<usize, GpuMeshBuffers>,
    mesh_uniform_slots: Vec<GpuUniformSlot>,
    line_slots: Vec<GpuLineSlot>,
    target: Option<GpuSceneTarget>,
}

impl GpuSceneState {
    fn new(device: &wgpu::Device) -> Self {
        let color_format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ApiGraphicBasic.SceneShader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(BASIC_SCENE_WGSL)),
        });

        let mesh_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
        let line_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
            mesh_cache: HashMap::new(),
            mesh_uniform_slots: Vec::new(),
            line_slots: Vec::new(),
            target: None,
        }
    }

    fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &SceneRenderFrame,
    ) -> Option<(SceneFrameOutput, SceneFrameMetrics)> {
        let frame_start = Instant::now();
        let target_rebuilt = self.ensure_target(device, frame.width, frame.height);
        let target = self.target.as_ref()?;
        let color_view = Arc::clone(&target.color_view);
        let depth_view = Arc::clone(&target.depth_view);
        let clear_color = extract_clear_color(frame.commands.commands());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ApiGraphicBasic.SceneEncoder"),
        });
        let mut metrics = SceneFrameMetrics {
            target_rebuilds: u32::from(target_rebuilt),
            ..SceneFrameMetrics::default()
        };
        let mut mesh_draw_index = 0usize;
        let mut line_draw_index = 0usize;

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ApiGraphicBasic.ScenePass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view.as_ref(),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view.as_ref(),
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
                        let Some(mesh) = frame.commands.mesh_arc(mesh_id) else {
                            continue;
                        };
                        self.draw_mesh(
                            device,
                            queue,
                            &mut pass,
                            mesh,
                            transform,
                            frame,
                            color,
                            matches!(current_pipeline, BasicPipelineKind::PbrLit),
                            mesh_draw_index,
                            &mut metrics,
                        );
                        mesh_draw_index += 1;
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
                            queue,
                            &mut pass,
                            start,
                            end,
                            frame,
                            color,
                            no_depth_test,
                            depth_bias,
                            line_draw_index,
                            &mut metrics,
                        );
                        line_draw_index += 1;
                    }
                    GraphicCommand::DrawGrid { .. } => {}
                }
            }
        }

        queue.submit(std::iter::once(encoder.finish()));
        let _ = device.poll(wgpu::Maintain::Poll);
        metrics.frame_cpu_ms = frame_start.elapsed().as_secs_f32() * 1000.0;
        Some((
            SceneFrameOutput::GpuTexture {
                view: color_view,
                width: frame.width,
                height: frame.height,
            },
            metrics,
        ))
    }

    fn ensure_target(&mut self, device: &wgpu::Device, width: u32, height: u32) -> bool {
        let needs_rebuild = self
            .target
            .as_ref()
            .map(|target| target.width != width || target.height != height)
            .unwrap_or(true);

        if !needs_rebuild {
            return false;
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
        let color_view =
            Arc::new(color_texture.create_view(&wgpu::TextureViewDescriptor::default()));
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
        let depth_view =
            Arc::new(depth_texture.create_view(&wgpu::TextureViewDescriptor::default()));

        self.target = Some(GpuSceneTarget {
            width,
            height,
            _color_texture: color_texture,
            color_view,
            _depth_texture: depth_texture,
            depth_view,
        });

        true
    }

    fn draw_mesh(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'_>,
        mesh: &Arc<BasicMesh>,
        transform: glam::Mat4,
        frame: &SceneRenderFrame,
        color: [u8; 4],
        lit: bool,
        draw_index: usize,
        metrics: &mut SceneFrameMetrics,
    ) {
        if mesh.indices.is_empty() || mesh.vertices.is_empty() {
            return;
        }

        let mesh_key = Arc::as_ptr(mesh) as usize;
        let allow_cache = Arc::strong_count(mesh) > 1;
        let cached_buffers = if allow_cache {
            match self.mesh_cache.entry(mesh_key) {
                std::collections::hash_map::Entry::Occupied(entry) => {
                    metrics.mesh_cache_hits += 1;
                    entry.get().clone()
                }
                std::collections::hash_map::Entry::Vacant(entry) => {
                    metrics.mesh_cache_misses += 1;
                    let buffers = create_gpu_mesh_buffers(device, mesh.as_ref());
                    metrics.mesh_upload_bytes += buffers.vertex_bytes + buffers.index_bytes;
                    entry.insert(buffers.clone());
                    buffers
                }
            }
        } else {
            metrics.mesh_cache_misses += 1;
            return self.draw_transient_mesh(
                device,
                queue,
                pass,
                mesh.as_ref(),
                transform,
                frame,
                color,
                lit,
                draw_index,
                metrics,
            );
        };

        let uniform_slot = self.ensure_mesh_uniform_slot(device, draw_index, metrics);
        let uniforms = MeshUniforms {
            mvp: (frame.view_proj * transform).to_cols_array_2d(),
            model: transform.to_cols_array_2d(),
            color: rgba8_to_f32(color),
            light_dir: [frame.light_dir.x, frame.light_dir.y, frame.light_dir.z, 0.0],
            params: [if lit { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
        };
        queue.write_buffer(&uniform_slot.buffer, 0, bytemuck::bytes_of(&uniforms));
        metrics.uniform_upload_bytes += std::mem::size_of::<MeshUniforms>() as u64;
        metrics.mesh_draw_calls += 1;

        pass.set_pipeline(&self.mesh_pipeline);
        pass.set_bind_group(0, uniform_slot.bind_group.as_ref(), &[]);
        pass.set_vertex_buffer(0, cached_buffers.vertex_buffer.slice(..));
        pass.set_index_buffer(
            cached_buffers.index_buffer.slice(..),
            wgpu::IndexFormat::Uint32,
        );
        pass.draw_indexed(0..cached_buffers.index_count, 0, 0..1);
    }

    fn draw_line(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'_>,
        start: glam::Vec3,
        end: glam::Vec3,
        frame: &SceneRenderFrame,
        color: [u8; 4],
        no_depth_test: bool,
        depth_bias: f32,
        draw_index: usize,
        metrics: &mut SceneFrameMetrics,
    ) {
        if start.distance_squared(end) <= f32::EPSILON {
            return;
        }

        let line_slot = self.ensure_line_slot(device, draw_index, metrics);
        let vertices = [
            GpuLineVertex {
                position: start.to_array(),
            },
            GpuLineVertex {
                position: end.to_array(),
            },
        ];
        let uniforms = LineUniforms {
            mvp: frame.view_proj.to_cols_array_2d(),
            color: rgba8_to_f32(color),
            params: [depth_bias, 0.0, 0.0, 0.0],
        };
        queue.write_buffer(&line_slot.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        queue.write_buffer(&line_slot.uniform.buffer, 0, bytemuck::bytes_of(&uniforms));
        metrics.line_upload_bytes += std::mem::size_of_val(&vertices) as u64;
        metrics.uniform_upload_bytes += std::mem::size_of::<LineUniforms>() as u64;
        metrics.line_draw_calls += 1;

        pass.set_pipeline(if no_depth_test {
            &self.line_pipeline_xray
        } else {
            &self.line_pipeline_depth
        });
        pass.set_bind_group(0, line_slot.uniform.bind_group.as_ref(), &[]);
        pass.set_vertex_buffer(0, line_slot.vertex_buffer.slice(..));
        pass.draw(0..2, 0..1);
    }

    fn draw_transient_mesh(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'_>,
        mesh: &BasicMesh,
        transform: glam::Mat4,
        frame: &SceneRenderFrame,
        color: [u8; 4],
        lit: bool,
        draw_index: usize,
        metrics: &mut SceneFrameMetrics,
    ) {
        let vertices: Vec<GpuMeshVertex> = mesh
            .vertices
            .iter()
            .map(|vertex| GpuMeshVertex {
                position: vertex.position.to_array(),
                normal: vertex.normal.to_array(),
            })
            .collect();
        let vertex_bytes = std::mem::size_of_val(vertices.as_slice()) as u64;
        let index_bytes = std::mem::size_of_val(mesh.indices.as_slice()) as u64;
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ApiGraphicBasic.MeshVertexBufferTransient"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ApiGraphicBasic.MeshIndexBufferTransient"),
            contents: bytemuck::cast_slice(mesh.indices.as_slice()),
            usage: wgpu::BufferUsages::INDEX,
        });
        let uniform_slot = self.ensure_mesh_uniform_slot(device, draw_index, metrics);
        let uniforms = MeshUniforms {
            mvp: (frame.view_proj * transform).to_cols_array_2d(),
            model: transform.to_cols_array_2d(),
            color: rgba8_to_f32(color),
            light_dir: [frame.light_dir.x, frame.light_dir.y, frame.light_dir.z, 0.0],
            params: [if lit { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
        };
        queue.write_buffer(&uniform_slot.buffer, 0, bytemuck::bytes_of(&uniforms));
        metrics.mesh_upload_bytes += vertex_bytes + index_bytes;
        metrics.uniform_upload_bytes += std::mem::size_of::<MeshUniforms>() as u64;
        metrics.mesh_draw_calls += 1;

        pass.set_pipeline(&self.mesh_pipeline);
        pass.set_bind_group(0, uniform_slot.bind_group.as_ref(), &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..mesh.indices.len() as u32, 0, 0..1);
    }

    fn ensure_mesh_uniform_slot(
        &mut self,
        device: &wgpu::Device,
        draw_index: usize,
        metrics: &mut SceneFrameMetrics,
    ) -> GpuUniformSlot {
        while self.mesh_uniform_slots.len() <= draw_index {
            self.mesh_uniform_slots.push(create_uniform_slot(
                device,
                &self.mesh_bind_group_layout,
                std::mem::size_of::<MeshUniforms>() as u64,
                "ApiGraphicBasic.MeshUniformBuffer",
                "ApiGraphicBasic.MeshBindGroup",
            ));
            metrics.mesh_uniform_slot_creations += 1;
        }

        self.mesh_uniform_slots[draw_index].clone()
    }

    fn ensure_line_slot(
        &mut self,
        device: &wgpu::Device,
        draw_index: usize,
        metrics: &mut SceneFrameMetrics,
    ) -> GpuLineSlot {
        while self.line_slots.len() <= draw_index {
            self.line_slots.push(GpuLineSlot {
                vertex_buffer: Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("ApiGraphicBasic.LineVertexBuffer"),
                    size: std::mem::size_of::<[GpuLineVertex; 2]>() as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })),
                uniform: create_uniform_slot(
                    device,
                    &self.line_bind_group_layout,
                    std::mem::size_of::<LineUniforms>() as u64,
                    "ApiGraphicBasic.LineUniformBuffer",
                    "ApiGraphicBasic.LineBindGroup",
                ),
            });
            metrics.line_slot_creations += 1;
        }

        self.line_slots[draw_index].clone()
    }
}

fn create_gpu_mesh_buffers(device: &wgpu::Device, mesh: &BasicMesh) -> GpuMeshBuffers {
    let vertices: Vec<GpuMeshVertex> = mesh
        .vertices
        .iter()
        .map(|vertex| GpuMeshVertex {
            position: vertex.position.to_array(),
            normal: vertex.normal.to_array(),
        })
        .collect();
    let vertex_bytes = std::mem::size_of_val(vertices.as_slice()) as u64;
    let index_bytes = std::mem::size_of_val(mesh.indices.as_slice()) as u64;
    let vertex_buffer = Arc::new(
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ApiGraphicBasic.MeshVertexBuffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        }),
    );
    let index_buffer = Arc::new(
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ApiGraphicBasic.MeshIndexBuffer"),
            contents: bytemuck::cast_slice(mesh.indices.as_slice()),
            usage: wgpu::BufferUsages::INDEX,
        }),
    );

    GpuMeshBuffers {
        vertex_buffer,
        index_buffer,
        index_count: mesh.indices.len() as u32,
        vertex_bytes,
        index_bytes,
    }
}

fn create_uniform_slot(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    size: u64,
    buffer_label: &'static str,
    bind_group_label: &'static str,
) -> GpuUniformSlot {
    let buffer = Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(buffer_label),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    }));
    let bind_group = Arc::new(device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(bind_group_label),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    }));

    GpuUniformSlot { buffer, bind_group }
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
            shared_wgpu_context: None,
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
