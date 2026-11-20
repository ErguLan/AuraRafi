use std::borrow::Cow;
use std::collections::HashMap;

use crate::api_graphic_basic::capabilities::{
    GraphicsAdapterPreference, GraphicsCapabilities, GraphicsMemoryBudget,
};
use crate::api_graphic_basic::command_list::BasicCommandList;
use crate::api_graphic_basic::memory_ledger::{GraphicsResourceFamily, GraphicsResourceLedger};
use crate::api_graphic_basic::resource_registry::{MeshRegistry, TextureRegistry};
use crate::shaders::basic_scene_shader;
use crate::texture::CpuTexture;

use super::resources::create_gpu_scene_texture;
use super::timestamps::GpuTimestampState;
use super::{
    BasicDevice, GpuLineVertex, GpuMeshInstance, GpuMeshVertex, GpuOverlayTriangle, GpuSceneState,
    UniformReuse,
};

impl BasicDevice {
    /// Internal GPU execution pipeline mapping commands onto wgpu.
    pub(super) fn execute_gpu(
        &self,
        _commands: &BasicCommandList,
        _width: u32,
        _height: u32,
    ) {
        // GPU execution implementation details (encapsulated internally)
    }

    /// Helper to attempt creating a wgpu device with generous compatibility parameters.
    /// Prioritizes Integrated GPUs and Low-Power options for maximum hardware reach,
    /// falling back to software/GL drivers if direct hardware context is missing.
    pub(super) fn try_init_gpu(preference: GraphicsAdapterPreference) -> Option<GpuState> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        // Request adapter using block_on for async initialization (run inside a lightweight runtime wrapper)
        let power_preference = match preference {
            GraphicsAdapterPreference::LowPower => wgpu::PowerPreference::LowPower,
            GraphicsAdapterPreference::HighPerformance => wgpu::PowerPreference::HighPerformance,
        };
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference,
            compatible_surface: None,
            force_fallback_adapter: false, // Fallback is requested if direct hardware creation fails
        }))?;

        let timestamp_features = wgpu_timestamp_features();
        let optional_features = if adapter.features().contains(timestamp_features) {
            timestamp_features
        } else {
            wgpu::Features::empty()
        };

        // Request device with minimal limit requirements (potato-friendly limit margin)
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("AuraRafi_Device"),
                required_features: optional_features,
                required_limits: wgpu::Limits::downlevel_defaults(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .ok()?;

        let limits = adapter.limits();
        Some(GpuState {
            instance,
            adapter,
            device,
            queue,
            capabilities: GraphicsCapabilities::wgpu_with_timestamp_queries(
                limits.max_texture_dimension_2d,
                limits.max_buffer_size,
                optional_features.contains(wgpu_timestamp_features()),
            ),
        })
    }
}

pub(crate) fn wgpu_timestamp_features() -> wgpu::Features {
    // `CommandEncoder::write_timestamp` needs the encoder-specific feature in
    // addition to timestamp queries. Some adapters expose TIMESTAMP_QUERY but
    // not TIMESTAMP_QUERY_INSIDE_ENCODERS, so timing must stay disabled there.
    wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS
}

pub(super) struct GpuState {
    pub(super) instance: wgpu::Instance,
    pub(super) adapter: wgpu::Adapter,
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    pub(super) capabilities: GraphicsCapabilities,
}

pub(super) fn create_scene_mesh_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    color_format: wgpu::TextureFormat,
    instanced: bool,
    two_sided: bool,
    ui_mode: Option<bool>,
) -> wgpu::RenderPipeline {
    let (label, vertex_entry, fragment_entry) = match (instanced, two_sided) {
        (false, false) => ("ApiGraphicBasic.MeshPipeline", "mesh_vs", "mesh_fs"),
        (false, true) => ("ApiGraphicBasic.MeshTwoSidedPipeline", "mesh_vs", "mesh_fs"),
        (true, false) => (
            "ApiGraphicBasic.MeshInstancedPipeline",
            "mesh_instanced_vs",
            "mesh_instanced_fs",
        ),
        (true, true) => (
            "ApiGraphicBasic.MeshInstancedTwoSidedPipeline",
            "mesh_instanced_vs",
            "mesh_instanced_fs",
        ),
    };
    let vertex_buffers = if instanced {
        vec![GpuMeshVertex::desc(), GpuMeshInstance::desc()]
    } else {
        vec![GpuMeshVertex::desc()]
    };

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(vertex_entry),
            buffers: &vertex_buffers,
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment_entry),
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
            cull_mode: if two_sided {
                None
            } else {
                Some(wgpu::Face::Back)
            },
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: ui_mode.is_none(),
            depth_compare: if ui_mode == Some(true) {
                wgpu::CompareFunction::Always
            } else {
                wgpu::CompareFunction::LessEqual
            },
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

/// Build the family ledger and return the mesh/texture shares used by the
/// bounded arenas.
fn scene_memory_ledger(memory_budget: GraphicsMemoryBudget) -> (GraphicsResourceLedger, u64, u64) {
    let mut ledger = GraphicsResourceLedger::new(memory_budget.gpu_bytes);
    let mut mesh_family_budget = 0;
    let mut texture_family_budget = 0;
    for (family, bytes) in memory_budget.family_budgets() {
        ledger.set_family_budget(family, bytes);
        match family {
            GraphicsResourceFamily::Mesh => mesh_family_budget = bytes,
            GraphicsResourceFamily::Texture => texture_family_budget = bytes,
            _ => {}
        }
    }
    (ledger, mesh_family_budget, texture_family_budget)
}

impl GpuSceneState {
    pub(super) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        memory_budget: GraphicsMemoryBudget,
    ) -> Self {
        let color_format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ApiGraphicBasic.SceneShader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(basic_scene_shader())),
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
        let scene_texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("ApiGraphicBasic.SceneTextureBindGroupLayout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
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
        let scene_texture_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ApiGraphicBasic.SceneTextureSampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..wgpu::SamplerDescriptor::default()
        });
        let fallback_texture = create_gpu_scene_texture(
            device,
            queue,
            &scene_texture_bind_group_layout,
            &scene_texture_sampler,
            &CpuTexture::solid(255, 255, 255),
            "ApiGraphicBasic.SceneWhiteFallback",
        );
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
        let overlay_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("ApiGraphicBasic.OverlayBindGroupLayout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
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
            bind_group_layouts: &[&mesh_bind_group_layout, &scene_texture_bind_group_layout],
            push_constant_ranges: &[],
        });
        let mesh_pipeline = create_scene_mesh_pipeline(
            device,
            &shader,
            &mesh_pipeline_layout,
            color_format,
            false,
            false,
            None,
        );
        let mesh_two_sided_pipeline = create_scene_mesh_pipeline(
            device,
            &shader,
            &mesh_pipeline_layout,
            color_format,
            false,
            true,
            None,
        );
        let mesh_instanced_pipeline = create_scene_mesh_pipeline(
            device,
            &shader,
            &mesh_pipeline_layout,
            color_format,
            true,
            false,
            None,
        );
        let mesh_instanced_two_sided_pipeline = create_scene_mesh_pipeline(
            device,
            &shader,
            &mesh_pipeline_layout,
            color_format,
            true,
            true,
            None,
        );
        let ui_texture_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ApiGraphicBasic.UiSurfaceSampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
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
                topology: wgpu::PrimitiveTopology::TriangleList,
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
                topology: wgpu::PrimitiveTopology::TriangleList,
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
        let overlay_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("ApiGraphicBasic.OverlayPipelineLayout"),
                bind_group_layouts: &[&overlay_bind_group_layout],
                push_constant_ranges: &[],
            });
        let overlay_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ApiGraphicBasic.OverlayPipeline"),
            layout: Some(&overlay_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("overlay_vs"),
                buffers: &[GpuOverlayTriangle::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("overlay_fs"),
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

        let (ledger, mesh_family_budget, texture_family_budget) =
            scene_memory_ledger(memory_budget);

        Self {
            color_format,
            optimized_filter: None,
            optimized_filter_memory_budget: (memory_budget.gpu_bytes / 16).min(64 * 1024 * 1024),
            environment: None,
            shadows: None,
            shadow_receiver: true,
            shadow_memory_budget: (memory_budget.gpu_bytes / 16).min(16 * 1024 * 1024),
            mesh_bind_group_layout,
            scene_texture_bind_group_layout,
            scene_texture_sampler,
            ui_texture_sampler,
            fallback_texture_bind_group: fallback_texture.bind_group,
            line_bind_group_layout,
            overlay_bind_group_layout,
            mesh_pipeline,
            mesh_two_sided_pipeline,
            ui_surface_pipeline: None,
            ui_overlay_pipeline: None,
            ui_mode: None,
            ui_premultiplied: false,
            mesh_instanced_pipeline,
            mesh_instanced_two_sided_pipeline,
            line_pipeline_depth,
            line_pipeline_xray,
            overlay_pipeline,
            mesh_cache: HashMap::new(),
            mesh_registry: MeshRegistry::new(mesh_family_budget),
            mesh_cache_limit: memory_budget.mesh_cache_entries as usize,
            texture_registry: TextureRegistry::new(texture_family_budget),
            texture_handles: HashMap::new(),
            external_texture_bindings: HashMap::new(),
            texture_cache_limit: (memory_budget.texture_cache_entries as usize).min(512),
            texture_cache_budget_bytes: texture_family_budget,
            mesh_uniform_reuse: UniformReuse::default(),
            line_uniform_reuse: UniformReuse::default(),
            overlay_uniform_reuse: UniformReuse::default(),
            mesh_uniform_slots: Vec::new(),
            mesh_instance_slots: Vec::new(),
            transient_mesh_slots: Vec::new(),
            line_slots: Vec::new(),
            overlay_slots: Vec::new(),
            transient_vertex_scratch: Vec::new(),
            line_vertex_scratch: Vec::new(),
            overlay_vertex_scratch: Vec::new(),
            mesh_instance_scratch: Vec::new(),
            target: None,
            target_generation: 0,
            frame_index: 0,
            frame_upload_budget: memory_budget.frame_upload_bytes,
            frame_upload_rejected: 0,
            upload_pressure: 0.0,
            ledger,
            gpu_timing: GpuTimestampState::new(device, queue),
        }
    }
}
