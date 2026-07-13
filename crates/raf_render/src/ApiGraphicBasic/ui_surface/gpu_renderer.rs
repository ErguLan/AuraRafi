//! Direct WGPU compositor for retained UI surfaces.
//!
//! It renders into any caller-owned texture view. Window ownership stays
//! outside this type, which keeps it usable by a native Winit host, tests, or
//! an off-screen editor surface without reintroducing an `eframe` dependency.

use std::borrow::Cow;
use std::ops::Range;

use bytemuck::{Pod, Zeroable};

use super::{UiSurfaceDrawList, UiSurfacePaintCommand, UiTextAtlas};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiSurfaceGpuMetrics {
    pub solid_vertices: u32,
    pub text_vertices: u32,
    pub atlas_uploads: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SolidVertex {
    position: [f32; 2],
    color: [f32; 4],
}

impl SolidVertex {
    fn layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
            wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRIBUTES,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TextVertex {
    position: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

struct VertexBatch<T> {
    vertices: Vec<T>,
    ranges: Vec<Range<u32>>,
}

impl TextVertex {
    fn layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
            wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRIBUTES,
        }
    }
}

pub struct UiSurfaceGpuRenderer {
    solid_pipeline: wgpu::RenderPipeline,
    text_pipeline: wgpu::RenderPipeline,
    atlas_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    atlas_texture: Option<wgpu::Texture>,
    atlas_bind_group: Option<wgpu::BindGroup>,
    atlas_size: [u16; 2],
    solid_buffer: Option<wgpu::Buffer>,
    solid_capacity: u64,
    text_buffer: Option<wgpu::Buffer>,
    text_capacity: u64,
}

impl UiSurfaceGpuRenderer {
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
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
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..wgpu::SamplerDescriptor::default()
        });
        let solid_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ApiGraphicBasic.UiSurfaceSolidLayout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        let text_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ApiGraphicBasic.UiSurfaceTextLayout"),
            bind_group_layouts: &[&atlas_layout],
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

        Self {
            solid_pipeline,
            text_pipeline,
            atlas_layout,
            sampler,
            atlas_texture: None,
            atlas_bind_group: None,
            atlas_size: [0, 0],
            solid_buffer: None,
            solid_capacity: 0,
            text_buffer: None,
            text_capacity: 0,
        }
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        size: [u32; 2],
        draw_list: &UiSurfaceDrawList,
        atlas: &mut UiTextAtlas,
        clear_color: [u8; 4],
    ) -> UiSurfaceGpuMetrics {
        let width = size[0].max(1);
        let height = size[1].max(1);
        let mut metrics = UiSurfaceGpuMetrics::default();
        metrics.atlas_uploads = u32::from(self.sync_atlas(device, queue, atlas));

        let solid_batch = solid_vertices(draw_list, width, height);
        let text_batch = text_vertices(draw_list, width, height);
        metrics.solid_vertices = solid_batch.vertices.len() as u32;
        metrics.text_vertices = text_batch.vertices.len() as u32;

        if !solid_batch.vertices.is_empty() {
            self.ensure_solid_buffer(device, solid_batch.vertices.len());
            if let Some(buffer) = self.solid_buffer.as_ref() {
                queue.write_buffer(buffer, 0, bytemuck::cast_slice(&solid_batch.vertices));
            }
        }
        if !text_batch.vertices.is_empty() {
            self.ensure_text_buffer(device, text_batch.vertices.len());
            if let Some(buffer) = self.text_buffer.as_ref() {
                queue.write_buffer(buffer, 0, bytemuck::cast_slice(&text_batch.vertices));
            }
        }

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ApiGraphicBasic.UiSurfaceEncoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ApiGraphicBasic.UiSurfacePass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(color_from_bytes(clear_color)),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            let solid_buffer = self
                .solid_buffer
                .as_ref()
                .filter(|_| !solid_batch.vertices.is_empty());
            let text_buffer = self
                .text_buffer
                .as_ref()
                .filter(|_| !text_batch.vertices.is_empty());
            let paint_order = draw_list.paint_commands();
            for command in paint_order.iter() {
                match *command {
                    UiSurfacePaintCommand::Solid { index, .. } => {
                        let (Some(buffer), Some(range)) =
                            (solid_buffer, solid_batch.ranges.get(index))
                        else {
                            continue;
                        };
                        pass.set_pipeline(&self.solid_pipeline);
                        pass.set_vertex_buffer(0, buffer.slice(..));
                        pass.draw(range.clone(), 0..1);
                    }
                    UiSurfacePaintCommand::Text { index, .. } => {
                        let (Some(buffer), Some(bind_group), Some(range)) = (
                            text_buffer,
                            self.atlas_bind_group.as_ref(),
                            text_batch.ranges.get(index),
                        ) else {
                            continue;
                        };
                        pass.set_pipeline(&self.text_pipeline);
                        pass.set_bind_group(0, bind_group, &[]);
                        pass.set_vertex_buffer(0, buffer.slice(..));
                        pass.draw(range.clone(), 0..1);
                    }
                }
            }
        }
        queue.submit(std::iter::once(encoder.finish()));
        metrics
    }

    fn sync_atlas(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        atlas: &mut UiTextAtlas,
    ) -> bool {
        let size = atlas.size();
        if self.atlas_size != size || self.atlas_texture.is_none() {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("ApiGraphicBasic.UiSurfaceTextAtlas"),
                size: wgpu::Extent3d {
                    width: u32::from(size[0]),
                    height: u32::from(size[1]),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.atlas_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ApiGraphicBasic.UiSurfaceAtlasBindGroup"),
                layout: &self.atlas_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            }));
            self.atlas_texture = Some(texture);
            self.atlas_size = size;
        }

        if !atlas.is_dirty() {
            return false;
        }
        let rgba = atlas_rgba(atlas.pixels());
        if let Some(texture) = self.atlas_texture.as_ref() {
            queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &rgba,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(u32::from(size[0]) * 4),
                    rows_per_image: Some(u32::from(size[1])),
                },
                wgpu::Extent3d {
                    width: u32::from(size[0]),
                    height: u32::from(size[1]),
                    depth_or_array_layers: 1,
                },
            );
        }
        atlas.mark_uploaded();
        true
    }

    fn ensure_solid_buffer(&mut self, device: &wgpu::Device, vertex_count: usize) {
        let required = (vertex_count * std::mem::size_of::<SolidVertex>()) as u64;
        if self.solid_buffer.is_some() && self.solid_capacity >= required {
            return;
        }
        self.solid_capacity = required.next_power_of_two().max(256);
        self.solid_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ApiGraphicBasic.UiSurfaceSolidVertices"),
            size: self.solid_capacity,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
    }

    fn ensure_text_buffer(&mut self, device: &wgpu::Device, vertex_count: usize) {
        let required = (vertex_count * std::mem::size_of::<TextVertex>()) as u64;
        if self.text_buffer.is_some() && self.text_capacity >= required {
            return;
        }
        self.text_capacity = required.next_power_of_two().max(256);
        self.text_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ApiGraphicBasic.UiSurfaceTextVertices"),
            size: self.text_capacity,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
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
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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

fn solid_vertices(
    draw_list: &UiSurfaceDrawList,
    width: u32,
    height: u32,
) -> VertexBatch<SolidVertex> {
    let mut vertices = Vec::with_capacity(draw_list.solids.len() * 12);
    let mut ranges = Vec::with_capacity(draw_list.solids.len());
    for quad in &draw_list.solids {
        let start = vertices.len() as u32;
        let positions = rounded_ndc_quad(
            quad.rect.x,
            quad.rect.y,
            quad.rect.width,
            quad.rect.height,
            quad.radius,
            width,
            height,
        );
        let color = color_to_f32(quad.color);
        for position in positions {
            vertices.push(SolidVertex { position, color });
        }
        ranges.push(start..vertices.len() as u32);
    }
    VertexBatch { vertices, ranges }
}

fn rounded_ndc_quad(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    radius: f32,
    target_width: u32,
    target_height: u32,
) -> Vec<[f32; 2]> {
    let radius = radius.max(0.0).min(width * 0.5).min(height * 0.5);
    if radius <= 0.5 || width <= 0.0 || height <= 0.0 {
        return ndc_quad(x, y, width, height, target_width, target_height).to_vec();
    }

    let segments = (radius / 4.0).ceil().clamp(2.0, 6.0) as usize;
    let mut perimeter = Vec::with_capacity(4 * (segments + 1));
    let corners = [
        (
            x + radius,
            y + radius,
            std::f32::consts::PI,
            std::f32::consts::FRAC_PI_2 * 3.0,
        ),
        (
            x + width - radius,
            y + radius,
            std::f32::consts::FRAC_PI_2 * 3.0,
            std::f32::consts::TAU,
        ),
        (
            x + width - radius,
            y + height - radius,
            0.0,
            std::f32::consts::FRAC_PI_2,
        ),
        (
            x + radius,
            y + height - radius,
            std::f32::consts::FRAC_PI_2,
            std::f32::consts::PI,
        ),
    ];
    for (center_x, center_y, start, end) in corners {
        for step in 0..=segments {
            let progress = step as f32 / segments as f32;
            let angle = start + (end - start) * progress;
            perimeter.push([
                center_x + angle.cos() * radius,
                center_y + angle.sin() * radius,
            ]);
        }
    }

    let center = ndc_point(
        x + width * 0.5,
        y + height * 0.5,
        target_width,
        target_height,
    );
    let mut positions = Vec::with_capacity(perimeter.len() * 3);
    for index in 0..perimeter.len() {
        positions.push(center);
        positions.push(ndc_point(
            perimeter[index][0],
            perimeter[index][1],
            target_width,
            target_height,
        ));
        let next = perimeter[(index + 1) % perimeter.len()];
        positions.push(ndc_point(next[0], next[1], target_width, target_height));
    }
    positions
}

fn text_vertices(
    draw_list: &UiSurfaceDrawList,
    width: u32,
    height: u32,
) -> VertexBatch<TextVertex> {
    let atlas_width = f32::from(draw_list.atlas_size[0].max(1));
    let atlas_height = f32::from(draw_list.atlas_size[1].max(1));
    let mut vertices = Vec::with_capacity(draw_list.text.len() * 6);
    let mut ranges = Vec::with_capacity(draw_list.text.len());
    for quad in &draw_list.text {
        let start = vertices.len() as u32;
        let positions = ndc_quad(
            quad.rect.x,
            quad.rect.y,
            quad.rect.width,
            quad.rect.height,
            width,
            height,
        );
        let left = quad.atlas_rect.x / atlas_width;
        let right = quad.atlas_rect.right() / atlas_width;
        let top = quad.atlas_rect.y / atlas_height;
        let bottom = quad.atlas_rect.bottom() / atlas_height;
        let uvs = [
            [left, top],
            [right, top],
            [right, bottom],
            [left, top],
            [right, bottom],
            [left, bottom],
        ];
        let color = color_to_f32(quad.color);
        for (position, uv) in positions.into_iter().zip(uvs) {
            vertices.push(TextVertex {
                position,
                uv,
                color,
            });
        }
        ranges.push(start..vertices.len() as u32);
    }
    VertexBatch { vertices, ranges }
}

fn ndc_quad(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    target_width: u32,
    target_height: u32,
) -> [[f32; 2]; 6] {
    let [left, top] = ndc_point(x, y, target_width, target_height);
    let [right, bottom] = ndc_point(x + width, y + height, target_width, target_height);
    [
        [left, top],
        [right, top],
        [right, bottom],
        [left, top],
        [right, bottom],
        [left, bottom],
    ]
}

fn ndc_point(x: f32, y: f32, target_width: u32, target_height: u32) -> [f32; 2] {
    [
        (x / target_width.max(1) as f32) * 2.0 - 1.0,
        1.0 - (y / target_height.max(1) as f32) * 2.0,
    ]
}

fn color_to_f32(color: [u8; 4]) -> [f32; 4] {
    [
        f32::from(color[0]) / 255.0,
        f32::from(color[1]) / 255.0,
        f32::from(color[2]) / 255.0,
        f32::from(color[3]) / 255.0,
    ]
}

fn color_from_bytes(color: [u8; 4]) -> wgpu::Color {
    wgpu::Color {
        r: f64::from(color[0]) / 255.0,
        g: f64::from(color[1]) / 255.0,
        b: f64::from(color[2]) / 255.0,
        a: f64::from(color[3]) / 255.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounded_quad_uses_more_geometry_than_a_square_without_unbounded_segments() {
        let square = rounded_ndc_quad(0.0, 0.0, 120.0, 40.0, 0.0, 240, 120);
        let rounded = rounded_ndc_quad(0.0, 0.0, 120.0, 40.0, 12.0, 240, 120);

        assert_eq!(square.len(), 6);
        assert!(rounded.len() > square.len());
        assert!(rounded.len() <= 4 * (6 + 1) * 3);
    }

    #[test]
    fn vertex_batches_keep_one_draw_range_per_surface_item() {
        let list = UiSurfaceDrawList {
            solids: vec![
                super::super::UiSurfaceQuad {
                    rect: super::super::UiRect::new(0.0, 0.0, 40.0, 20.0),
                    color: [255, 255, 255, 255],
                    z_index: 0,
                    radius: 0.0,
                },
                super::super::UiSurfaceQuad {
                    rect: super::super::UiRect::new(40.0, 0.0, 40.0, 20.0),
                    color: [255, 128, 0, 255],
                    z_index: 1,
                    radius: 6.0,
                },
            ],
            ..UiSurfaceDrawList::default()
        };
        let batch = solid_vertices(&list, 160, 80);

        assert_eq!(batch.ranges.len(), 2);
        assert_eq!(batch.ranges[0], 0..6);
        assert!(batch.ranges[1].end > batch.ranges[1].start);
    }
}

fn atlas_rgba(alpha: &[u8]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(alpha.len() * 4);
    for value in alpha {
        rgba.extend_from_slice(&[255, 255, 255, *value]);
    }
    rgba
}

const UI_SURFACE_WGSL: &str = r#"
struct SolidOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn solid_vs(@location(0) position: vec2<f32>, @location(1) color: vec4<f32>) -> SolidOut {
    var output: SolidOut;
    output.position = vec4<f32>(position, 0.0, 1.0);
    output.color = color;
    return output;
}

@fragment
fn solid_fs(input: SolidOut) -> @location(0) vec4<f32> {
    return input.color;
}

struct TextOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@group(0) @binding(0) var atlas_texture: texture_2d<f32>;
@group(0) @binding(1) var atlas_sampler: sampler;

@vertex
fn text_vs(
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
) -> TextOut {
    var output: TextOut;
    output.position = vec4<f32>(position, 0.0, 1.0);
    output.uv = uv;
    output.color = color;
    return output;
}

@fragment
fn text_fs(input: TextOut) -> @location(0) vec4<f32> {
    let coverage = textureSample(atlas_texture, atlas_sampler, input.uv).a;
    return vec4<f32>(input.color.rgb, input.color.a * coverage);
}
"#;
