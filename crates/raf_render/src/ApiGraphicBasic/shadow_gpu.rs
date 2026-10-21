//! Private GPU implementation: one depth-only instanced pass, retained until its geometry key changes.
use super::*;
use crate::shadow_frame::ShadowFrame;

const SHADOW_DECLARATIONS: &str = r#"
struct ShadowUniforms { vp: mat4x4<f32>, camera_distance: vec4<f32>, params: vec4<f32> };
@group(2) @binding(0) var<uniform> shadow: ShadowUniforms;
@group(2) @binding(1) var shadow_depth: texture_depth_2d;
@group(2) @binding(2) var shadow_sampler: sampler_comparison;
"#;
const SHADOW_VISIBILITY: &str = r#"
fn shadow_tap(uv: vec2<f32>, depth: f32) -> f32 {
    if (any(uv < vec2<f32>(0.0)) || any(uv >= vec2<f32>(1.0))) { return 1.0; }
    return textureSampleCompareLevel(shadow_depth, shadow_sampler, uv, depth);
}
fn sun_visibility(world: vec3<f32>, ndotl: f32) -> f32 {
    // Branch, not select: non-receivers must not pay for comparison taps.
    if (mesh_uniforms.params.w < 0.5) { return 1.0; }
    let p = (shadow.vp * vec4<f32>(world, 1.0)).xyz;
    if (any(abs(p.xy) >= vec2<f32>(1.0)) || p.z <= 0.0 || p.z >= 1.0) { return 1.0; }
    let uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    let cosine = clamp(ndotl, 0.05, 1.0);
    let slope = min(sqrt(max(0.0,1.0-cosine*cosine))/cosine,8.0);
    let footprint = select(0.75,1.5,shadow.params.y > 0.5);
    let depth = p.z - shadow.params.x - footprint * shadow.params.w * slope;
    var visible = 1.0;
    if (shadow.params.y > 1.5) {
        let texel = 1.0 / f32(textureDimensions(shadow_depth).x);
        visible = 0.0;
        for (var y=-1; y<=1; y=y+1) { for (var x=-1; x<=1; x=x+1) {
            visible += shadow_tap(uv+vec2<f32>(f32(x),f32(y))*texel,depth);
        } }
        visible /= 9.0;
    } else if (shadow.params.y > 0.5) {
        let half_texel = 0.5 / f32(textureDimensions(shadow_depth).x);
        // Exactly four comparisons. No variable kernel, temporal history or extra targets.
        visible = (shadow_tap(uv + vec2<f32>(-half_texel,-half_texel),depth)
            + shadow_tap(uv + vec2<f32>(half_texel,-half_texel),depth)
            + shadow_tap(uv + vec2<f32>(-half_texel,half_texel),depth)
            + shadow_tap(uv + vec2<f32>(half_texel,half_texel),depth)) * 0.25;
    } else { visible = shadow_tap(uv,depth); }
    let fade = clamp((shadow.camera_distance.w-distance(world,shadow.camera_distance.xyz))/(shadow.camera_distance.w*0.2),0.0,1.0);
    return 1.0-(1.0-visible)*fade*shadow.params.z;
}
"#;
const DEPTH_SHADER: &str = r#"
struct Input {
    @location(0) position: vec3<f32>,
    @location(3) m0: vec4<f32>, @location(4) m1: vec4<f32>,
    @location(5) m2: vec4<f32>, @location(6) m3: vec4<f32>,
};
@vertex fn depth_vs(v: Input) -> @builtin(position) vec4<f32> {
    return mat4x4<f32>(v.m0,v.m1,v.m2,v.m3)*vec4<f32>(v.position,1.0);
}
"#;

pub(super) fn receiver_shader() -> String {
    crate::shaders::basic_scene_shader()
        .replace("// SHADOW_DECLARATIONS", SHADOW_DECLARATIONS)
        .replace(
            "fn sun_visibility(world: vec3<f32>, ndotl: f32) -> f32 { return 1.0; }",
            SHADOW_VISIBILITY,
        )
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Pod, Zeroable)]
struct ShadowUniforms {
    vp: [[f32; 4]; 4],
    camera_distance: [f32; 4],
    params: [f32; 4],
}

pub(super) struct ShadowGpuState {
    pub pipelines: [wgpu::RenderPipeline; 4],
    pub binding: wgpu::BindGroup,
    depth_pipeline: wgpu::RenderPipeline,
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    uniform: wgpu::Buffer,
    last_uniform: Option<ShadowUniforms>,
    pub(super) resolution: u32,
    key: Option<u64>,
    mesh_buffers: HashMap<usize, (Arc<BasicMesh>, GpuMeshBuffers)>,
    instances: wgpu::Buffer,
    instance_capacity: usize,
    groups: Vec<(usize, std::ops::Range<u32>)>,
}

impl ShadowGpuState {
    pub fn matches_geometry(&self, key: u64) -> bool {
        self.key == Some(key)
    }
    pub fn new(
        device: &wgpu::Device,
        frame: &ShadowFrame,
        mesh_layout: &wgpu::BindGroupLayout,
        texture_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
    ) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ApiGraphicBasic.ShadowLayout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Shadow.Uniform"),
            size: std::mem::size_of::<ShadowUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let resolution = frame.geometry.resolution;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Shadow.Depth"),
            size: wgpu::Extent3d {
                width: resolution,
                height: resolution,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Shadow.NearestCompare"),
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Shadow.Binding"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let receiver = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shadow.ReceiverShader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(receiver_shader())),
        });
        let receiver_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Shadow.ReceiverLayout"),
            bind_group_layouts: &[mesh_layout, texture_layout, &layout],
            push_constant_ranges: &[],
        });
        let pipelines = [
            create_scene_mesh_pipeline(device, &receiver, &receiver_layout, format, false, false),
            create_scene_mesh_pipeline(device, &receiver, &receiver_layout, format, false, true),
            create_scene_mesh_pipeline(device, &receiver, &receiver_layout, format, true, false),
            create_scene_mesh_pipeline(device, &receiver, &receiver_layout, format, true, true),
        ];
        let depth_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shadow.DepthShader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(DEPTH_SHADER)),
        });
        let depth_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Shadow.DepthLayout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        let depth_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Shadow.DepthPipeline"),
            layout: Some(&depth_layout),
            vertex: wgpu::VertexState {
                module: &depth_shader,
                entry_point: Some("depth_vs"),
                buffers: &[GpuMeshVertex::desc(), GpuMeshInstance::desc()],
                compilation_options: Default::default(),
            },
            fragment: None,
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Shadow.Instances"),
            size: 80,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipelines,
            binding,
            depth_pipeline,
            _texture: texture,
            view,
            uniform,
            last_uniform: None,
            resolution,
            key: None,
            mesh_buffers: HashMap::new(),
            instances,
            instance_capacity: 1,
            groups: Vec::new(),
        }
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        frame: &ShadowFrame,
        metrics: &mut SceneFrameMetrics,
    ) {
        let start = Instant::now();
        let uniforms = ShadowUniforms {
            vp: frame.geometry.view_projection.to_cols_array_2d(),
            camera_distance: frame.camera.extend(frame.settings.distance).to_array(),
            params: [
                frame.settings.bias / (8.0 * frame.settings.distance),
                if frame.settings.soft_filter {
                    if frame.improved {
                        2.0
                    } else {
                        1.0
                    }
                } else {
                    0.0
                },
                frame.settings.strength,
                if frame.improved {
                    frame
                        .geometry
                        .normalized_texel_bias(frame.geometry.resolution)
                } else {
                    1.0 / (4.0 * frame.geometry.resolution as f32)
                },
            ],
        };
        if self.last_uniform != Some(uniforms) {
            queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniforms));
            metrics.uniform_upload_bytes += std::mem::size_of::<ShadowUniforms>() as u64;
            self.last_uniform = Some(uniforms);
        }
        metrics.shadow_active = true;
        metrics.shadow_resolution = self.resolution;
        metrics.shadow_casters = frame.geometry.casters.len() as u32;
        if self.key == Some(frame.geometry.key) {
            metrics.shadow_cache_hits = 1;
        } else {
            let mut grouped: HashMap<usize, Vec<GpuMeshInstance>> = HashMap::new();
            let used: std::collections::HashSet<_> = frame
                .geometry
                .casters
                .iter()
                .map(|c| Arc::as_ptr(&c.mesh) as usize)
                .collect();
            // Release obsolete geometry before admitting replacements, not after uploading them.
            self.mesh_buffers.retain(|key, _| used.contains(key));
            for caster in &frame.geometry.casters {
                let key = Arc::as_ptr(&caster.mesh) as usize;
                self.mesh_buffers.entry(key).or_insert_with(|| {
                    let buffers = create_gpu_mesh_buffers(device, &caster.mesh);
                    metrics.mesh_upload_bytes += buffers.vertex_bytes + buffers.index_bytes;
                    (Arc::clone(&caster.mesh), buffers)
                });
                grouped.entry(key).or_default().push(GpuMeshInstance {
                    model: (frame.geometry.view_projection * caster.model).to_cols_array_2d(),
                    color: [1.0; 4],
                });
            }
            let mut instances = Vec::with_capacity(frame.geometry.casters.len());
            self.groups.clear();
            for (key, group) in grouped {
                let first = instances.len() as u32;
                instances.extend(group);
                self.groups.push((key, first..instances.len() as u32));
            }
            if instances.len() > self.instance_capacity {
                self.instance_capacity = instances.len().next_power_of_two();
                self.instances = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Shadow.Instances"),
                    size: (self.instance_capacity * std::mem::size_of::<GpuMeshInstance>()) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
            }
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&instances));
            metrics.mesh_instance_upload_bytes +=
                std::mem::size_of_val(instances.as_slice()) as u64;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ApiGraphicBasic.DirectionalShadowPass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.depth_pipeline);
            pass.set_vertex_buffer(1, self.instances.slice(..));
            for (key, range) in &self.groups {
                let buffers = &self.mesh_buffers[key].1;
                pass.set_vertex_buffer(0, buffers.vertex_buffer.slice(..));
                pass.set_index_buffer(buffers.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..buffers.index_count, 0, range.clone());
                metrics.shadow_draw_calls += 1;
            }
            self.key = Some(frame.geometry.key);
            metrics.shadow_map_rebuilds = 1;
        }
        metrics.shadow_resident_bytes = self.resolution as u64 * self.resolution as u64 * 4
            + self
                .mesh_buffers
                .values()
                .map(|(_, b)| b.vertex_bytes + b.index_bytes)
                .sum::<u64>()
            + (self.instance_capacity * std::mem::size_of::<GpuMeshInstance>()) as u64
            + std::mem::size_of::<ShadowUniforms>() as u64;
        metrics.shadow_prepare_cpu_ms = start.elapsed().as_secs_f32() * 1000.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shadows_wgsl_validates_depth_and_receiver_entrypoints() {
        for source in [receiver_shader(), DEPTH_SHADER.to_owned()] {
            let module = wgpu::naga::front::wgsl::parse_str(&source).unwrap();
            wgpu::naga::valid::Validator::new(
                wgpu::naga::valid::ValidationFlags::all(),
                wgpu::naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap();
        }
    }
}
