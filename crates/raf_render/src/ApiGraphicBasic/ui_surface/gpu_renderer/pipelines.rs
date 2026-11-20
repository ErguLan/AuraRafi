//! Shader modules, pipeline layouts, render pipelines, and texture bind-group
//! layouts for the retained UI compositor.

use std::borrow::Cow;

use raf_ui::UiImageSampling;

use super::images::{UiExternalImage, UiIconAtlasPage, UiImageGpuTexture};
use super::{SolidVertex, TextVertex};

pub(crate) struct UiSurfaceGpuSharedResources {
    pub(super) solid_pipeline: wgpu::RenderPipeline,
    pub(super) text_pipeline: wgpu::RenderPipeline,
    pub(super) image_pipeline: wgpu::RenderPipeline,
    pub(super) external_image_pipeline: wgpu::RenderPipeline,
    pub(super) missing_image_pipeline: wgpu::RenderPipeline,
    pub(super) effects_layout: wgpu::BindGroupLayout,
    pub(super) atlas_layout: wgpu::BindGroupLayout,
    pub(super) sampler: wgpu::Sampler,
    pub(super) image_sampler: wgpu::Sampler,
    pub(super) nearest_sampler: wgpu::Sampler,
    pub(super) icon_linear_sampler: wgpu::Sampler,
    pub(super) icon_nearest_sampler: wgpu::Sampler,
}

impl UiSurfaceGpuSharedResources {
    pub(crate) fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ApiGraphicBasic.UiSurfaceShader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(UI_SURFACE_WGSL)),
        });
        let atlas_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ApiGraphicBasic.UiSurfaceAtlasLayout"),
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
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ApiGraphicBasic.UiSurfaceAtlasSampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..wgpu::SamplerDescriptor::default()
        });
        let image_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ApiGraphicBasic.UiSurfaceImageSampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            // Images use alpha-correct mipmaps generated at upload time. Pick
            // one complete mip instead of blending adjacent levels, which
            // removes icon detail and produces the washed-out look at 12-18px.
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..wgpu::SamplerDescriptor::default()
        });
        let nearest_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ApiGraphicBasic.UiSurfaceNearestSampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..wgpu::SamplerDescriptor::default()
        });
        let icon_linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ApiGraphicBasic.UiSurfaceIconLinearSampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            lod_max_clamp: 2.0,
            ..wgpu::SamplerDescriptor::default()
        });
        let icon_nearest_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ApiGraphicBasic.UiSurfaceIconNearestSampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            lod_max_clamp: 2.0,
            ..wgpu::SamplerDescriptor::default()
        });
        let effects_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ApiGraphicBasic.UiSurfaceEffectsLayout"),
            entries: &[
                storage_buffer_layout_entry(0),
                storage_buffer_layout_entry(1),
            ],
        });
        let solid_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ApiGraphicBasic.UiSurfaceSolidLayout"),
            bind_group_layouts: &[&effects_layout],
            push_constant_ranges: &[],
        });
        let text_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ApiGraphicBasic.UiSurfaceTextLayout"),
            bind_group_layouts: &[&effects_layout, &atlas_layout],
            push_constant_ranges: &[],
        });

        let solid_pipeline = create_pipeline(
            device,
            &shader,
            &solid_layout,
            color_format,
            "solid_vs",
            "solid_fs",
            &[SolidVertex::layout()],
            "ApiGraphicBasic.UiSurfaceSolidPipeline",
        );
        let text_pipeline = create_pipeline(
            device,
            &shader,
            &text_layout,
            color_format,
            "text_vs",
            "text_fs",
            &[TextVertex::layout()],
            "ApiGraphicBasic.UiSurfaceTextPipeline",
        );
        let image_pipeline = create_premultiplied_pipeline(
            device,
            &shader,
            &text_layout,
            color_format,
            "text_vs",
            "image_fs",
            &[TextVertex::layout()],
            "ApiGraphicBasic.UiSurfaceImagePipeline",
        );
        let external_image_pipeline = create_premultiplied_pipeline(
            device,
            &shader,
            &text_layout,
            color_format,
            "text_vs",
            "external_image_fs",
            &[TextVertex::layout()],
            "ApiGraphicBasic.UiSurfaceExternalImagePipeline",
        );
        let missing_image_pipeline = create_pipeline(
            device,
            &shader,
            &text_layout,
            color_format,
            "text_vs",
            "missing_image_fs",
            &[TextVertex::layout()],
            "ApiGraphicBasic.UiSurfaceMissingImagePipeline",
        );

        Self {
            solid_pipeline,
            text_pipeline,
            image_pipeline,
            external_image_pipeline,
            missing_image_pipeline,
            effects_layout,
            atlas_layout,
            sampler,
            image_sampler,
            nearest_sampler,
            icon_linear_sampler,
            icon_nearest_sampler,
        }
    }
}

fn create_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    color_format: wgpu::TextureFormat,
    vertex_entry: &str,
    fragment_entry: &str,
    buffers: &[wgpu::VertexBufferLayout<'_>],
    label: &'static str,
) -> wgpu::RenderPipeline {
    create_pipeline_with_blend(
        device,
        shader,
        layout,
        color_format,
        vertex_entry,
        fragment_entry,
        buffers,
        label,
        wgpu::BlendState::ALPHA_BLENDING,
    )
}

fn create_premultiplied_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    color_format: wgpu::TextureFormat,
    vertex_entry: &str,
    fragment_entry: &str,
    buffers: &[wgpu::VertexBufferLayout<'_>],
    label: &'static str,
) -> wgpu::RenderPipeline {
    create_pipeline_with_blend(
        device,
        shader,
        layout,
        color_format,
        vertex_entry,
        fragment_entry,
        buffers,
        label,
        wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        },
    )
}

fn create_pipeline_with_blend(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    color_format: wgpu::TextureFormat,
    vertex_entry: &str,
    fragment_entry: &str,
    buffers: &[wgpu::VertexBufferLayout<'_>],
    label: &'static str,
    blend: wgpu::BlendState,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(vertex_entry),
            buffers,
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment_entry),
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: Some(blend),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
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
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

fn storage_buffer_layout_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

pub(super) fn create_image_bind_groups(
    device: &wgpu::Device,
    shared: &UiSurfaceGpuSharedResources,
    view: &wgpu::TextureView,
    icon_atlas: bool,
) -> (wgpu::BindGroup, wgpu::BindGroup) {
    let (linear_sampler, nearest_sampler) = if icon_atlas {
        (&shared.icon_linear_sampler, &shared.icon_nearest_sampler)
    } else {
        (&shared.image_sampler, &shared.nearest_sampler)
    };
    let create = |label, sampler: &wgpu::Sampler| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: &shared.atlas_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    };
    (
        create(
            "ApiGraphicBasic.UiSurfaceImageLinearBindGroup",
            linear_sampler,
        ),
        create(
            "ApiGraphicBasic.UiSurfaceImageNearestBindGroup",
            nearest_sampler,
        ),
    )
}

pub(super) fn create_icon_atlas_bind_groups(
    device: &wgpu::Device,
    shared: &UiSurfaceGpuSharedResources,
    view: &wgpu::TextureView,
) -> (wgpu::BindGroup, wgpu::BindGroup) {
    create_image_bind_groups(device, shared, view, true)
}

pub(super) fn image_bind_group(
    texture: &UiImageGpuTexture,
    sampling: UiImageSampling,
) -> &wgpu::BindGroup {
    match sampling {
        UiImageSampling::Nearest => &texture.nearest_bind_group,
        UiImageSampling::Auto | UiImageSampling::Linear => &texture.linear_bind_group,
    }
}

pub(super) fn icon_bind_group(
    page: &UiIconAtlasPage,
    sampling: UiImageSampling,
) -> &wgpu::BindGroup {
    match sampling {
        UiImageSampling::Nearest => &page.nearest_bind_group,
        UiImageSampling::Auto | UiImageSampling::Linear => &page.linear_bind_group,
    }
}

pub(super) fn external_image_bind_group(
    image: &UiExternalImage,
    sampling: UiImageSampling,
) -> &wgpu::BindGroup {
    match sampling {
        UiImageSampling::Nearest => &image.nearest_bind_group,
        UiImageSampling::Auto | UiImageSampling::Linear => &image.linear_bind_group,
    }
}

pub(super) const UI_SURFACE_WGSL: &str = r#"
const HAS_SHAPE: u32 = 1u;
const HAS_BORDER: u32 = 2u;
const HAS_GRADIENT: u32 = 4u;
const SOFT_FALLOFF: u32 = 8u;
const INVALID_CLIPS: u32 = 16u;
const MAX_ROUNDED_CLIPS: u32 = 9u;
const ANALYTIC_AA_WIDTH: f32 = 1.0;

struct PaintEffect {
    rect: vec4<f32>,
    corners: vec4<f32>,
    border_color: vec4<f32>,
    border_widths: vec4<f32>,
    gradient_start: vec4<f32>,
    gradient_end: vec4<f32>,
    gradient_direction_softness: vec4<f32>,
    clip_range_flags: vec4<u32>,
};

struct RoundedClip {
    rect: vec4<f32>,
    corners: vec4<f32>,
};

@group(0) @binding(0) var<storage, read> paint_effects: array<PaintEffect>;
@group(0) @binding(1) var<storage, read> rounded_clips: array<RoundedClip>;
@group(1) @binding(0) var source_texture: texture_2d<f32>;
@group(1) @binding(1) var source_sampler: sampler;

fn rounded_distance(rect: vec4<f32>, corners: vec4<f32>, point: vec2<f32>) -> f32 {
    let local = point - (rect.xy + rect.zw * 0.5);
    var radius = corners.w;
    if (local.y < 0.0) {
        radius = select(corners.x, corners.y, local.x >= 0.0);
    } else {
        radius = select(corners.w, corners.z, local.x >= 0.0);
    }
    let q = abs(local) - rect.zw * 0.5 + vec2<f32>(radius, radius);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - radius;
}

fn analytic_coverage(distance: f32) -> f32 {
    let aa_width = ANALYTIC_AA_WIDTH;
    return clamp(0.5 - distance / aa_width, 0.0, 1.0);
}

fn clip_coverage(effect: PaintEffect, point: vec2<f32>) -> f32 {
    let start = effect.clip_range_flags.x;
    let count = effect.clip_range_flags.y;
    let flags = effect.clip_range_flags.z;
    if ((flags & INVALID_CLIPS) != 0u || count > MAX_ROUNDED_CLIPS) {
        return 0.0;
    }
    var coverage = 1.0;
    for (var index = 0u; index < MAX_ROUNDED_CLIPS; index = index + 1u) {
        if (index >= count) {
            break;
        }
        let clip = rounded_clips[start + index];
        coverage = min(coverage, analytic_coverage(rounded_distance(clip.rect, clip.corners, point)));
    }
    return coverage;
}

struct SolidOut {
    @builtin(position) position: vec4<f32>,
    @location(0) local_position: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) @interpolate(flat) effect_index: u32,
};

@vertex
fn solid_vs(
    @location(0) position: vec2<f32>,
    @location(1) local_position: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) effect_index: u32,
) -> SolidOut {
    var output: SolidOut;
    output.position = vec4<f32>(position, 0.0, 1.0);
    output.local_position = local_position;
    output.color = color;
    output.effect_index = effect_index;
    return output;
}

@fragment
fn solid_fs(input: SolidOut) -> @location(0) vec4<f32> {
    let effect = paint_effects[input.effect_index];
    let flags = effect.clip_range_flags.z;
    var coverage = clip_coverage(effect, input.local_position);
    var fill = input.color;
    if ((flags & HAS_SHAPE) != 0u) {
        let distance = rounded_distance(effect.rect, effect.corners, input.local_position);
        let softness = effect.gradient_direction_softness.z;
        let aa_width = max(ANALYTIC_AA_WIDTH, select(0.0, softness, (flags & SOFT_FALLOFF) != 0u));
        coverage *= clamp(0.5 - distance / aa_width, 0.0, 1.0);

        if ((flags & HAS_GRADIENT) != 0u) {
            let direction = effect.gradient_direction_softness.xy;
            let denominator = abs(direction.x) + abs(direction.y);
            var amount = 0.0;
            if (denominator > 0.0) {
                let normalized = (input.local_position - effect.rect.xy) / max(effect.rect.zw, vec2<f32>(1.0));
                amount = clamp(((normalized.x - 0.5) * direction.x + (normalized.y - 0.5) * direction.y) / denominator + 0.5, 0.0, 1.0);
            }
            fill = mix(effect.gradient_start, effect.gradient_end, amount);
        }

        if ((flags & HAS_BORDER) != 0u) {
            let widths = effect.border_widths;
            let inner_rect = vec4<f32>(
                effect.rect.x + widths.w,
                effect.rect.y + widths.x,
                max(effect.rect.z - widths.y - widths.w, 0.0),
                max(effect.rect.w - widths.x - widths.z, 0.0),
            );
            let inner_corners = max(effect.corners - vec4<f32>(
                max(widths.x, widths.w),
                max(widths.x, widths.y),
                max(widths.z, widths.y),
                max(widths.z, widths.w),
            ), vec4<f32>(0.0));
            var inner_coverage = 0.0;
            if (inner_rect.z > 0.0 && inner_rect.w > 0.0) {
                inner_coverage = analytic_coverage(rounded_distance(inner_rect, inner_corners, input.local_position));
            }
            let border_weight = 1.0 - inner_coverage;
            let fill_weight = inner_coverage;
            let alpha = effect.border_color.a * border_weight + fill.a * fill_weight;
            var mixed_rgb = vec3<f32>(0.0);
            if (alpha > 0.00001) {
                mixed_rgb = (effect.border_color.rgb * effect.border_color.a * border_weight + fill.rgb * fill.a * fill_weight) / alpha;
            }
            fill = vec4<f32>(mixed_rgb, alpha);
        }
    }
    return vec4<f32>(fill.rgb, fill.a * coverage);
}

struct TextOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) local_position: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) @interpolate(flat) effect_index: u32,
};

@vertex
fn text_vs(
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) local_position: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) effect_index: u32,
) -> TextOut {
    var output: TextOut;
    output.position = vec4<f32>(position, 0.0, 1.0);
    output.uv = uv;
    output.local_position = local_position;
    output.color = color;
    output.effect_index = effect_index;
    return output;
}

@fragment
fn text_fs(input: TextOut) -> @location(0) vec4<f32> {
    let effect = paint_effects[input.effect_index];
    let glyph = textureSample(source_texture, source_sampler, input.uv).a;
    let coverage = glyph * clip_coverage(effect, input.local_position);
    return vec4<f32>(input.color.rgb, input.color.a * coverage);
}

@fragment
fn image_fs(input: TextOut) -> @location(0) vec4<f32> {
    let effect = paint_effects[input.effect_index];
    let sample = textureSample(source_texture, source_sampler, input.uv);
    let coverage = clip_coverage(effect, input.local_position);
    return vec4<f32>(sample.rgb * input.color.rgb * input.color.a * coverage, sample.a * input.color.a * coverage);
}

@fragment
fn external_image_fs(input: TextOut) -> @location(0) vec4<f32> {
    let effect = paint_effects[input.effect_index];
    let sample = textureSample(source_texture, source_sampler, input.uv);
    let coverage = clip_coverage(effect, input.local_position);
    return vec4<f32>(sample.rgb * input.color.rgb * input.color.a * coverage, sample.a * input.color.a * coverage);
}

@fragment
fn missing_image_fs(input: TextOut) -> @location(0) vec4<f32> {
    let effect = paint_effects[input.effect_index];
    let mode = i32(round(input.uv.x));
    let point = input.local_position;
    let parity = i32(floor(point.x / 8.0) + floor(point.y / 8.0));
    var color = select(vec3<f32>(0.208, 0.224, 0.251), vec3<f32>(0.325, 0.341, 0.369), (parity & 1) == 0);
    if (mode == 0) {
        let normalized = (point - effect.rect.xy) / max(effect.rect.zw, vec2<f32>(1.0));
        let row1 = normalized.y >= 0.30 && normalized.y <= 0.42 && normalized.x >= 0.14 && normalized.x <= 0.86;
        let row2 = normalized.y >= 0.48 && normalized.y <= 0.60 && normalized.x >= 0.14 && normalized.x <= 0.64;
        let row3 = normalized.y >= 0.66 && normalized.y <= 0.78 && normalized.x >= 0.14 && normalized.x <= 0.78;
        color = select(vec3<f32>(0.235, 0.255, 0.282), vec3<f32>(0.420, 0.439, 0.471), row1 || row2 || row3);
    } else if (mode == 2) {
        let normalized = (point - effect.rect.xy) / max(effect.rect.zw, vec2<f32>(1.0));
        if (abs(normalized.x - normalized.y) < 0.045 || abs(normalized.x + normalized.y - 1.0) < 0.045) {
            color = vec3<f32>(0.796, 0.286, 0.286);
        }
    }
    let coverage = clip_coverage(effect, point);
    return vec4<f32>(color, input.color.a * coverage);
}
"#;
