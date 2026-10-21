//! Private WGPU execution for the bounded scene spatial filter.

use std::borrow::Cow;
use std::sync::Arc;

pub(super) struct OptimizedFilterGpu {
    pub texture: wgpu::Texture,
    pub view: Arc<wgpu::TextureView>,
    pub size: [u32; 2],
    pipeline: wgpu::RenderPipeline,
    source_bind_group: wgpu::BindGroup,
}

impl OptimizedFilterGpu {
    pub(super) fn new(
        device: &wgpu::Device,
        source: &wgpu::TextureView,
        size: [u32; 2],
        format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ApiGraphicBasic.OptimizedFilter.Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(WGSL)),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ApiGraphicBasic.OptimizedFilter.BindGroupLayout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ApiGraphicBasic.OptimizedFilter.PipelineLayout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ApiGraphicBasic.OptimizedFilter.Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
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
        let source_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ApiGraphicBasic.OptimizedFilter.SourceBindGroup"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(source),
            }],
        });
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ApiGraphicBasic.OptimizedFilter.Target"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = Arc::new(texture.create_view(&wgpu::TextureViewDescriptor::default()));

        Self {
            texture,
            view,
            size,
            pipeline,
            source_bind_group,
        }
    }

    pub(super) fn encode(&self, encoder: &mut wgpu::CommandEncoder) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ApiGraphicBasic.OptimizedFilter.Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.source_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// Five-tap scene-only filter. Source and target should use the same sRGB
/// format; WGPU decodes source samples to linear RGB and encodes the render
/// target on write. Keep thresholds and weights aligned with optimized_filter.rs.
pub(crate) const WGSL: &str = r#"
@group(0) @binding(0)
var source_texture: texture_2d<f32>;

const MAX_BLEND_STRENGTH: f32 = 0.45;
const CONTRAST_FLOOR: f32 = 0.035;
const CONTRAST_FULL_STRENGTH: f32 = 0.25;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let corner = vec2<f32>(
        f32((vertex_index << 1u) & 2u),
        f32(vertex_index & 2u),
    );
    var output: VertexOutput;
    output.position = vec4<f32>(corner * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    return output;
}

fn luma(color: vec3<f32>) -> f32 {
    return color.r * 0.2126 + color.g * 0.7152 + color.b * 0.0722;
}

@fragment
fn fs_main(@builtin(position) fragment_position: vec4<f32>) -> @location(0) vec4<f32> {
    let dimensions = vec2<i32>(textureDimensions(source_texture, 0));
    let last_coordinate = max(dimensions - vec2<i32>(1, 1), vec2<i32>(0, 0));
    let center_coordinate = clamp(vec2<i32>(fragment_position.xy), vec2<i32>(0, 0), last_coordinate);

    let north_coordinate = max(center_coordinate + vec2<i32>(0, -1), vec2<i32>(0, 0));
    let south_coordinate = min(center_coordinate + vec2<i32>(0, 1), last_coordinate);
    let west_coordinate = max(center_coordinate + vec2<i32>(-1, 0), vec2<i32>(0, 0));
    let east_coordinate = min(center_coordinate + vec2<i32>(1, 0), last_coordinate);

    let center = textureLoad(source_texture, center_coordinate, 0);
    let north = textureLoad(source_texture, north_coordinate, 0);
    let south = textureLoad(source_texture, south_coordinate, 0);
    let west = textureLoad(source_texture, west_coordinate, 0);
    let east = textureLoad(source_texture, east_coordinate, 0);

    let center_luma = luma(center.rgb);
    let north_luma = luma(north.rgb);
    let south_luma = luma(south.rgb);
    let west_luma = luma(west.rgb);
    let east_luma = luma(east.rgb);

    let local_min = min(center_luma, min(min(north_luma, south_luma), min(west_luma, east_luma)));
    let local_max = max(center_luma, max(max(north_luma, south_luma), max(west_luma, east_luma)));
    let contrast = local_max - local_min;

    let gradient_x = abs(east_luma - west_luma);
    let gradient_y = abs(south_luma - north_luma);
    let gradient_sum = gradient_x + gradient_y;
    var corner_measure = 0.0;
    if gradient_sum > 0.0 {
        corner_measure = clamp(2.0 * min(gradient_x, gradient_y) / gradient_sum, 0.0, 1.0);
    }
    let contrast_weight = clamp(
        (contrast - CONTRAST_FLOOR) / (CONTRAST_FULL_STRENGTH - CONTRAST_FLOOR),
        0.0,
        1.0,
    );
    let strength = clamp(MAX_BLEND_STRENGTH * contrast_weight * corner_measure, 0.0, MAX_BLEND_STRENGTH);

    let neighbor_average = (north.rgb + south.rgb + west.rgb + east.rgb) * 0.25;
    return vec4<f32>(mix(center.rgb, neighbor_average, strength), center.a);
}
"#;

#[cfg(test)]
mod tests {
    use super::WGSL;

    #[test]
    fn shader_parses_and_validates_with_wgpu_naga() {
        let module =
            wgpu::naga::front::wgsl::parse_str(WGSL).expect("optimized filter WGSL must parse");
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("optimized filter WGSL must validate");
    }
}
