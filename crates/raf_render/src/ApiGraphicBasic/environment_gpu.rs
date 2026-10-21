//! Private GPU executor for the single fullscreen environment background.
use crate::environment_frame::{linear_color, EnvironmentFrame};
use bytemuck::{Pod, Zeroable};
use raf_core::environment::SkyMode;
use std::sync::Arc;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Pod, Zeroable)]
struct Uniforms {
    inverse_vp: [[f32; 4]; 4],
    top: [f32; 4],
    horizon: [f32; 4],
    ground: [f32; 4],
    sun: [f32; 4],
    direction: [f32; 4],
    params: [f32; 4],
}
impl From<EnvironmentFrame> for Uniforms {
    fn from(env: EnvironmentFrame) -> Self {
        let sky = env.settings.sky;
        Self {
            inverse_vp: env.inverse_view_projection.to_cols_array_2d(),
            top: env.sky_top.extend(f32::from(env.improved())).to_array(),
            horizon: env.sky_horizon.extend(1.0).to_array(),
            ground: env.sky_ground.extend(1.0).to_array(),
            sun: (linear_color(env.settings.sun.color) * env.settings.sun.intensity)
                .extend(env.exposure())
                .to_array(),
            direction: env
                .sun_direction
                .extend(env.settings.sun.angular_radius.cos())
                .to_array(),
            params: [
                f32::from(sky.mode == SkyMode::Gradient),
                f32::from(
                    env.settings.sun.enabled
                        && env.settings.sun.disk_visible
                        && env.sun_direction.y > 0.0,
                ),
                if sky.stars_enabled {
                    sky.stars_density
                } else {
                    0.0
                },
                sky.stars_intensity,
            ],
        }
    }
}
pub(super) struct GpuEnvironment {
    pub pipeline: wgpu::RenderPipeline,
    pub binding: wgpu::BindGroup,
    buffer: wgpu::Buffer,
}
pub(super) struct EnvironmentGpuState {
    resources: Arc<GpuEnvironment>,
    last: Uniforms,
}
pub(super) fn prepare(
    state: &mut Option<EnvironmentGpuState>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
    env: Option<EnvironmentFrame>,
) -> Option<(Arc<GpuEnvironment>, u64)> {
    let uniform = Uniforms::from(env?);
    if let Some(state) = state {
        let changed = state.last != uniform;
        if changed {
            queue.write_buffer(&state.resources.buffer, 0, bytemuck::bytes_of(&uniform));
            state.last = uniform;
        }
        return Some((
            Arc::clone(&state.resources),
            if changed {
                std::mem::size_of::<Uniforms>() as u64
            } else {
                0
            },
        ));
    }
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Environment.Layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Environment.Uniform"),
        contents: bytemuck::bytes_of(&uniform),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Environment.Binding"),
        layout: &layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Environment.Shader"),
        source: wgpu::ShaderSource::Wgsl(sky_shader().into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Environment.PipelineLayout"),
        bind_group_layouts: &[&layout],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Environment.Background"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::Always,
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        multiview: None,
        cache: None,
    });
    let resources = Arc::new(GpuEnvironment {
        pipeline,
        binding,
        buffer,
    });
    *state = Some(EnvironmentGpuState {
        resources: Arc::clone(&resources),
        last: uniform,
    });
    Some((resources, std::mem::size_of::<Uniforms>() as u64))
}
const SKY_SHADER: &str = r#"
// STAR_FUNCTIONS
struct Environment { inverse_vp: mat4x4<f32>, top: vec4<f32>, horizon: vec4<f32>, ground: vec4<f32>, sun: vec4<f32>, direction: vec4<f32>, params: vec4<f32> };
@group(0) @binding(0) var<uniform> env: Environment;
struct Vertex { @builtin(position) position: vec4<f32>, @location(0) ndc: vec2<f32> };
@vertex fn vs_main(@builtin(vertex_index) index: u32) -> Vertex {
    let x = f32((index << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(index & 2u) * 2.0 - 1.0;
    return Vertex(vec4<f32>(x, y, 0.0, 1.0), vec2<f32>(x, y));
}
@fragment fn fs_main(input: Vertex) -> @location(0) vec4<f32> {
    let a = env.inverse_vp * vec4<f32>(input.ndc, 0.0, 1.0);
    let b = env.inverse_vp * vec4<f32>(input.ndc, 0.9, 1.0);
    let ray = normalize(b.xyz / b.w - a.xyz / a.w);
    var footprint=0.0;
    if env.top.w > 0.5 { footprint=length(fwidth(ray))*10.0; }
    var color = env.top.rgb;
    if env.params.x > 0.5 {
        color = mix(env.horizon.rgb, select(env.ground.rgb, env.top.rgb, ray.y >= 0.0), clamp(abs(ray.y), 0.0, 1.0));
    }
    if env.params.z > 0.0 && env.params.w > 0.0 {
        var stars=0.0;
        if env.top.w > 0.5 { stars=star_brightness_filtered(ray,env.params.z,footprint); }
        else { stars=star_brightness(ray,env.params.z); }
        color += vec3<f32>(0.72,0.86,1.0)*stars*env.params.w;
    }
    if env.params.y > 0.5 {
        let disk_base = (dot(ray,env.direction.xyz)-env.direction.w) / max(1.0-env.direction.w,0.0000001);
        var disk = clamp(disk_base,0.0,1.0);
        if env.top.w > 0.5 { disk = smoothstep(-max(fwidth(disk_base),0.0001),1.0,disk_base); }
        color = mix(color, env.sun.rgb, disk);
    }
    if env.top.w > 0.5 { color = improved_expose(color,env.sun.w); }
    return vec4<f32>(color, 1.0);
}
"#;
fn sky_shader() -> String {
    SKY_SHADER.replace(
        "// STAR_FUNCTIONS",
        &format!(
            "{}\n{}",
            crate::sky_stars::WGSL,
            crate::optimized_lighting::WGSL
        ),
    )
}
#[cfg(test)]
mod tests {
    #[test]
    fn environment_shader_validates() {
        let module = wgpu::naga::front::wgsl::parse_str(&super::sky_shader()).unwrap();
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }
}
