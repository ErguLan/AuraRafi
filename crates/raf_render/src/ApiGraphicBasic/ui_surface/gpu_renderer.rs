//! Direct WGPU compositor for retained UI surfaces.
//!
//! It renders into any caller-owned texture view. Window ownership stays
//! outside this type, which keeps it usable by a native Winit host, tests, or
//! an off-screen editor surface without reintroducing an `eframe` dependency.

use std::borrow::Cow;
use std::collections::{hash_map::DefaultHasher, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::ops::Range;

use bytemuck::{Pod, Zeroable};

use super::{
    UiSurfaceDrawList, UiSurfaceImageStore, UiSurfacePaintCommand, UiTextAtlas, UiTextAtlasRect,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiSurfaceGpuMetrics {
    pub solid_vertices: u32,
    pub text_vertices: u32,
    pub image_vertices: u32,
    pub atlas_uploads: u32,
    pub image_uploads: u32,
    pub geometry_cache_hit: bool,
    pub buffer_upload_bytes: u64,
    pub paint_runs: u32,
    pub draw_calls: u32,
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

struct UiSurfaceGpuGeometryCache {
    fingerprint: u64,
    solid_batch: VertexBatch<SolidVertex>,
    text_batch: VertexBatch<TextVertex>,
    image_batch: VertexBatch<TextVertex>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum UiSurfacePaintKind {
    Solid,
    Text,
    Image(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UiSurfacePaintRun {
    kind: UiSurfacePaintKind,
    range: Range<u32>,
    scissor: (u32, u32, u32, u32),
}

struct UiImageGpuTexture {
    _texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    revision: u64,
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
    image_pipeline: wgpu::RenderPipeline,
    atlas_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    image_sampler: wgpu::Sampler,
    icon_sampler: wgpu::Sampler,
    atlas_texture: Option<wgpu::Texture>,
    atlas_bind_group: Option<wgpu::BindGroup>,
    atlas_size: [u16; 2],
    solid_buffer: Option<wgpu::Buffer>,
    solid_capacity: u64,
    text_buffer: Option<wgpu::Buffer>,
    text_capacity: u64,
    image_buffer: Option<wgpu::Buffer>,
    image_capacity: u64,
    image_textures: HashMap<String, UiImageGpuTexture>,
    geometry_cache: Option<UiSurfaceGpuGeometryCache>,
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
        let icon_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ApiGraphicBasic.UiSurfaceIconSampler"),
            // Built-in icons are authored at a higher resolution than their
            // small controls. Linear magnification preserves the white-line
            // artwork without the blocky pixels produced by nearest sampling.
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
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
        let image_pipeline = create_pipeline(
            device,
            &shader,
            &text_layout,
            color_format,
            "text_vs",
            "image_fs",
            &[TextVertex::layout()],
            "ApiGraphicBasic.UiSurfaceImagePipeline",
        );

        Self {
            solid_pipeline,
            text_pipeline,
            image_pipeline,
            atlas_layout,
            sampler,
            image_sampler,
            icon_sampler,
            atlas_texture: None,
            atlas_bind_group: None,
            atlas_size: [0, 0],
            solid_buffer: None,
            solid_capacity: 0,
            text_buffer: None,
            text_capacity: 0,
            image_buffer: None,
            image_capacity: 0,
            image_textures: HashMap::new(),
            geometry_cache: None,
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
        images: &UiSurfaceImageStore,
        clear_color: [u8; 4],
    ) -> UiSurfaceGpuMetrics {
        self.render_at_scale(
            device,
            queue,
            target,
            size,
            size,
            draw_list,
            atlas,
            images,
            clear_color,
        )
    }

    /// Composites logical UI coordinates into a physical target texture. The
    /// logical size controls geometry while the target size controls the GPU
    /// viewport and scissor rectangles.
    pub fn render_at_scale(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        target_size: [u32; 2],
        logical_size: [u32; 2],
        draw_list: &UiSurfaceDrawList,
        atlas: &mut UiTextAtlas,
        images: &UiSurfaceImageStore,
        clear_color: [u8; 4],
    ) -> UiSurfaceGpuMetrics {
        let width = target_size[0].max(1);
        let height = target_size[1].max(1);
        let logical_width = logical_size[0].max(1);
        let logical_height = logical_size[1].max(1);
        let mut metrics = UiSurfaceGpuMetrics::default();
        metrics.atlas_uploads = u32::from(self.sync_atlas(device, queue, atlas));
        let mut uploaded_image_keys = HashSet::new();
        for quad in &draw_list.images {
            if uploaded_image_keys.insert(quad.source_key.as_str()) {
                metrics.image_uploads +=
                    u32::from(self.sync_image(device, queue, images, &quad.source_key));
            }
        }
        // Keep GPU image memory proportional to the current surface. Images are
        // content-addressed by source key, so an image that leaves the draw list
        // is safe to release; it will be uploaded again only if it becomes
        // visible later. This prevents editor previews and asset browsers from
        // accumulating every thumbnail ever visited.
        self.image_textures
            .retain(|key, _| uploaded_image_keys.contains(key.as_str()));

        let geometry_fingerprint = retained_geometry_fingerprint(
            draw_list,
            images,
            logical_width,
            logical_height,
            width,
            height,
        );
        let geometry_rebuilt = self
            .geometry_cache
            .as_ref()
            .map_or(true, |cached| cached.fingerprint != geometry_fingerprint);
        metrics.geometry_cache_hit = !geometry_rebuilt;
        if geometry_rebuilt {
            // UiStyle colors and registered PNGs are stored as sRGB values.
            // The compositor operates in linear light even when its target is
            // Unorm because the eframe bridge presents it into an sRGB target.
            let geometry = UiSurfaceGpuGeometryCache {
                fingerprint: geometry_fingerprint,
                solid_batch: solid_vertices(
                    draw_list,
                    logical_width,
                    logical_height,
                    width,
                    height,
                    true,
                ),
                text_batch: text_vertices(
                    draw_list,
                    logical_width,
                    logical_height,
                    width,
                    height,
                    true,
                ),
                image_batch: image_vertices(
                    draw_list,
                    logical_width,
                    logical_height,
                    width,
                    height,
                    images,
                    true,
                ),
            };
            metrics.solid_vertices = geometry.solid_batch.vertices.len() as u32;
            metrics.text_vertices = geometry.text_batch.vertices.len() as u32;
            metrics.image_vertices = geometry.image_batch.vertices.len() as u32;
            if !geometry.solid_batch.vertices.is_empty() {
                self.ensure_solid_buffer(device, geometry.solid_batch.vertices.len());
                if let Some(buffer) = self.solid_buffer.as_ref() {
                    let bytes = bytemuck::cast_slice(&geometry.solid_batch.vertices);
                    queue.write_buffer(buffer, 0, bytes);
                    metrics.buffer_upload_bytes += bytes.len() as u64;
                }
            }
            if !geometry.text_batch.vertices.is_empty() {
                self.ensure_text_buffer(device, geometry.text_batch.vertices.len());
                if let Some(buffer) = self.text_buffer.as_ref() {
                    let bytes = bytemuck::cast_slice(&geometry.text_batch.vertices);
                    queue.write_buffer(buffer, 0, bytes);
                    metrics.buffer_upload_bytes += bytes.len() as u64;
                }
            }
            if !geometry.image_batch.vertices.is_empty() {
                self.ensure_image_buffer(device, geometry.image_batch.vertices.len());
                if let Some(buffer) = self.image_buffer.as_ref() {
                    let bytes = bytemuck::cast_slice(&geometry.image_batch.vertices);
                    queue.write_buffer(buffer, 0, bytes);
                    metrics.buffer_upload_bytes += bytes.len() as u64;
                }
            }
            self.geometry_cache = Some(geometry);
        } else if let Some(geometry) = self.geometry_cache.as_ref() {
            metrics.solid_vertices = geometry.solid_batch.vertices.len() as u32;
            metrics.text_vertices = geometry.text_batch.vertices.len() as u32;
            metrics.image_vertices = geometry.image_batch.vertices.len() as u32;
        }
        let geometry = self
            .geometry_cache
            .as_ref()
            .expect("GPU UI geometry cache is initialized before rendering");
        let paint_runs = build_paint_runs(
            draw_list,
            geometry,
            [width, height],
            [logical_width, logical_height],
        );
        metrics.paint_runs = paint_runs.len() as u32;
        metrics.draw_calls = metrics.paint_runs;

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
                        load: wgpu::LoadOp::Clear(color_from_bytes(clear_color, true)),
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
                .filter(|_| !geometry.solid_batch.vertices.is_empty());
            let text_buffer = self
                .text_buffer
                .as_ref()
                .filter(|_| !geometry.text_batch.vertices.is_empty());
            let image_buffer = self
                .image_buffer
                .as_ref()
                .filter(|_| !geometry.image_batch.vertices.is_empty());
            let mut active_pipeline: Option<&UiSurfacePaintKind> = None;
            let mut active_scissor = None;
            for run in &paint_runs {
                match &run.kind {
                    UiSurfacePaintKind::Solid => {
                        let Some(buffer) = solid_buffer else {
                            continue;
                        };
                        if active_pipeline != Some(&run.kind) {
                            pass.set_pipeline(&self.solid_pipeline);
                            pass.set_vertex_buffer(0, buffer.slice(..));
                            active_pipeline = Some(&run.kind);
                        }
                    }
                    UiSurfacePaintKind::Text => {
                        let (Some(buffer), Some(bind_group)) =
                            (text_buffer, self.atlas_bind_group.as_ref())
                        else {
                            continue;
                        };
                        if active_pipeline != Some(&run.kind) {
                            pass.set_pipeline(&self.text_pipeline);
                            pass.set_vertex_buffer(0, buffer.slice(..));
                            pass.set_bind_group(0, bind_group, &[]);
                            active_pipeline = Some(&run.kind);
                        }
                    }
                    UiSurfacePaintKind::Image(source_key) => {
                        let (Some(buffer), Some(texture)) =
                            (image_buffer, self.image_textures.get(source_key))
                        else {
                            continue;
                        };
                        if active_pipeline != Some(&run.kind) {
                            pass.set_pipeline(&self.image_pipeline);
                            pass.set_vertex_buffer(0, buffer.slice(..));
                            pass.set_bind_group(0, &texture.bind_group, &[]);
                            active_pipeline = Some(&run.kind);
                        }
                    }
                }
                if active_scissor != Some(run.scissor) {
                    pass.set_scissor_rect(
                        run.scissor.0,
                        run.scissor.1,
                        run.scissor.2,
                        run.scissor.3,
                    );
                    active_scissor = Some(run.scissor);
                }
                pass.draw(run.range.clone(), 0..1);
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
        let texture_recreated = self.atlas_size != size || self.atlas_texture.is_none();
        if texture_recreated {
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
        let region = if texture_recreated {
            UiTextAtlasRect {
                x: 0,
                y: 0,
                width: size[0],
                height: size[1],
            }
        } else {
            atlas.dirty_region().unwrap_or(UiTextAtlasRect {
                x: 0,
                y: 0,
                width: size[0],
                height: size[1],
            })
        };
        let pixels = atlas_rgba_region(atlas.pixels(), size, region);
        let (rgba, row_bytes) = padded_rgba_rows(
            &pixels,
            [
                u32::from(region.width).max(1),
                u32::from(region.height).max(1),
            ],
        );
        if let Some(texture) = self.atlas_texture.as_ref() {
            queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: u32::from(region.x),
                        y: u32::from(region.y),
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &rgba,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(row_bytes),
                    rows_per_image: Some(u32::from(region.height)),
                },
                wgpu::Extent3d {
                    width: u32::from(region.width),
                    height: u32::from(region.height),
                    depth_or_array_layers: 1,
                },
            );
        }
        atlas.mark_uploaded();
        true
    }

    fn sync_image(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        images: &UiSurfaceImageStore,
        key: &str,
    ) -> bool {
        let Some(image) = images.get(key) else {
            self.image_textures.remove(key);
            return false;
        };
        if self
            .image_textures
            .get(key)
            .is_some_and(|cached| cached.revision == image.revision)
        {
            return false;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ApiGraphicBasic.UiSurfaceImage"),
            size: wgpu::Extent3d {
                width: image.size[0].max(1),
                height: image.size[1].max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: image_mip_level_count(image.size),
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mip_level_count = image_mip_level_count(image.size);
        let mut mip_pixels = image.pixels.clone();
        let mut mip_size = [image.size[0].max(1), image.size[1].max(1)];
        for mip_level in 0..mip_level_count {
            let (upload_pixels, upload_row_bytes) = padded_rgba_rows(&mip_pixels, mip_size);
            queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: &texture,
                    mip_level,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &upload_pixels,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(upload_row_bytes),
                    rows_per_image: Some(mip_size[1]),
                },
                wgpu::Extent3d {
                    width: mip_size[0],
                    height: mip_size[1],
                    depth_or_array_layers: 1,
                },
            );

            if mip_level + 1 < mip_level_count {
                let next_size = [(mip_size[0] / 2).max(1), (mip_size[1] / 2).max(1)];
                mip_pixels = downsample_rgba_premultiplied(&mip_pixels, mip_size, next_size);
                mip_size = next_size;
            }
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = if key.starts_with("builtin://icon/") {
            &self.icon_sampler
        } else {
            &self.image_sampler
        };
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ApiGraphicBasic.UiSurfaceImageBindGroup"),
            layout: &self.atlas_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });
        self.image_textures.insert(
            key.to_string(),
            UiImageGpuTexture {
                _texture: texture,
                bind_group,
                revision: image.revision,
            },
        );
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

    fn ensure_image_buffer(&mut self, device: &wgpu::Device, vertex_count: usize) {
        let required = (vertex_count * std::mem::size_of::<TextVertex>()) as u64;
        if self.image_buffer.is_some() && self.image_capacity >= required {
            return;
        }
        self.image_capacity = required.next_power_of_two().max(256);
        self.image_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ApiGraphicBasic.UiSurfaceImageVertices"),
            size: self.image_capacity,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
    }
}

fn build_paint_runs(
    draw_list: &UiSurfaceDrawList,
    geometry: &UiSurfaceGpuGeometryCache,
    target_size: [u32; 2],
    logical_size: [u32; 2],
) -> Vec<UiSurfacePaintRun> {
    let mut runs: Vec<UiSurfacePaintRun> = Vec::new();
    for command in draw_list.paint_commands().iter().copied() {
        let candidate = match command {
            UiSurfacePaintCommand::Solid { index, .. } => {
                let (Some(quad), Some(range)) = (
                    draw_list.solids.get(index),
                    geometry.solid_batch.ranges.get(index),
                ) else {
                    continue;
                };
                ui_scissor_rect(Some(quad.clip_rect), target_size, logical_size).map(|scissor| {
                    UiSurfacePaintRun {
                        kind: UiSurfacePaintKind::Solid,
                        range: range.clone(),
                        scissor,
                    }
                })
            }
            UiSurfacePaintCommand::Text { index, .. } => {
                let (Some(quad), Some(range)) = (
                    draw_list.text.get(index),
                    geometry.text_batch.ranges.get(index),
                ) else {
                    continue;
                };
                ui_scissor_rect(Some(quad.clip_rect), target_size, logical_size).map(|scissor| {
                    UiSurfacePaintRun {
                        kind: UiSurfacePaintKind::Text,
                        range: range.clone(),
                        scissor,
                    }
                })
            }
            UiSurfacePaintCommand::Image { index, .. } => {
                let (Some(quad), Some(range)) = (
                    draw_list.images.get(index),
                    geometry.image_batch.ranges.get(index),
                ) else {
                    continue;
                };
                ui_scissor_rect(
                    Some(quad.rect.intersection(quad.clip_rect)),
                    target_size,
                    logical_size,
                )
                .map(|scissor| UiSurfacePaintRun {
                    kind: UiSurfacePaintKind::Image(quad.source_key.clone()),
                    range: range.clone(),
                    scissor,
                })
            }
        };
        let Some(candidate) = candidate.filter(|run| run.range.start < run.range.end) else {
            continue;
        };
        if let Some(previous) = runs.last_mut().filter(|previous| {
            previous.kind == candidate.kind
                && previous.scissor == candidate.scissor
                && previous.range.end == candidate.range.start
        }) {
            previous.range.end = candidate.range.end;
        } else {
            runs.push(candidate);
        }
    }
    runs
}

fn retained_geometry_fingerprint(
    draw_list: &UiSurfaceDrawList,
    images: &UiSurfaceImageStore,
    logical_width: u32,
    logical_height: u32,
    target_width: u32,
    target_height: u32,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    logical_width.hash(&mut hasher);
    logical_height.hash(&mut hasher);
    target_width.hash(&mut hasher);
    target_height.hash(&mut hasher);
    draw_list.atlas_size.hash(&mut hasher);
    for quad in &draw_list.solids {
        hash_rect(quad.rect, &mut hasher);
        hash_rect(quad.clip_rect, &mut hasher);
        quad.color.hash(&mut hasher);
        quad.z_index.hash(&mut hasher);
        quad.radius.to_bits().hash(&mut hasher);
    }
    for quad in &draw_list.text {
        hash_rect(quad.rect, &mut hasher);
        hash_rect(quad.clip_rect, &mut hasher);
        hash_rect(quad.atlas_rect, &mut hasher);
        quad.color.hash(&mut hasher);
        quad.z_index.hash(&mut hasher);
    }
    for quad in &draw_list.images {
        hash_rect(quad.rect, &mut hasher);
        hash_rect(quad.clip_rect, &mut hasher);
        quad.source_key.hash(&mut hasher);
        match quad.fit {
            super::UiImageFit::Contain => 0_u8,
            super::UiImageFit::Cover => 1_u8,
            super::UiImageFit::Stretch => 2_u8,
        }
        .hash(&mut hasher);
        quad.tint.hash(&mut hasher);
        quad.z_index.hash(&mut hasher);
        images
            .get(&quad.source_key)
            .map(|image| image.size)
            .unwrap_or([0, 0])
            .hash(&mut hasher);
    }
    for command in draw_list.paint_commands().iter().copied() {
        match command {
            UiSurfacePaintCommand::Solid {
                index,
                z_index,
                sequence,
            } => {
                0_u8.hash(&mut hasher);
                index.hash(&mut hasher);
                z_index.hash(&mut hasher);
                sequence.hash(&mut hasher);
            }
            UiSurfacePaintCommand::Text {
                index,
                z_index,
                sequence,
            } => {
                1_u8.hash(&mut hasher);
                index.hash(&mut hasher);
                z_index.hash(&mut hasher);
                sequence.hash(&mut hasher);
            }
            UiSurfacePaintCommand::Image {
                index,
                z_index,
                sequence,
            } => {
                2_u8.hash(&mut hasher);
                index.hash(&mut hasher);
                z_index.hash(&mut hasher);
                sequence.hash(&mut hasher);
            }
        }
    }
    hasher.finish()
}

fn hash_rect(rect: super::UiRect, hasher: &mut DefaultHasher) {
    rect.x.to_bits().hash(hasher);
    rect.y.to_bits().hash(hasher);
    rect.width.to_bits().hash(hasher);
    rect.height.to_bits().hash(hasher);
}

fn image_mip_level_count(size: [u32; 2]) -> u32 {
    let mut largest = size[0].max(size[1]).max(1);
    let mut levels = 1;
    while largest > 1 {
        largest = (largest / 2).max(1);
        levels += 1;
    }
    levels
}

fn padded_rgba_rows(pixels: &[u8], size: [u32; 2]) -> (Vec<u8>, u32) {
    const COPY_BYTES_PER_ROW_ALIGNMENT: usize = 256;
    let row_bytes = size[0].max(1) as usize * 4;
    let padded_row_bytes = (row_bytes + COPY_BYTES_PER_ROW_ALIGNMENT - 1)
        / COPY_BYTES_PER_ROW_ALIGNMENT
        * COPY_BYTES_PER_ROW_ALIGNMENT;
    if row_bytes == padded_row_bytes {
        return (pixels.to_vec(), row_bytes as u32);
    }

    let height = size[1].max(1) as usize;
    let mut padded = vec![0; padded_row_bytes * height];
    for row in 0..height {
        let source_start = row * row_bytes;
        let source_end = source_start + row_bytes;
        let target_start = row * padded_row_bytes;
        padded[target_start..target_start + row_bytes]
            .copy_from_slice(&pixels[source_start..source_end]);
    }
    (padded, padded_row_bytes as u32)
}

fn downsample_rgba_premultiplied(
    pixels: &[u8],
    source_size: [u32; 2],
    target_size: [u32; 2],
) -> Vec<u8> {
    let source_width = source_size[0].max(1) as usize;
    let source_height = source_size[1].max(1) as usize;
    let target_width = target_size[0].max(1) as usize;
    let target_height = target_size[1].max(1) as usize;
    let mut output = vec![0; target_width * target_height * 4];

    for target_y in 0..target_height {
        for target_x in 0..target_width {
            let source_left = target_x * source_width / target_width;
            let source_right = ((target_x + 1) * source_width / target_width).max(source_left + 1);
            let source_top = target_y * source_height / target_height;
            let source_bottom =
                ((target_y + 1) * source_height / target_height).max(source_top + 1);
            let mut alpha_sum = 0.0_f32;
            let mut premultiplied = [0.0_f32; 3];
            let mut samples = 0.0_f32;

            for source_y in source_top..source_bottom.min(source_height) {
                for source_x in source_left..source_right.min(source_width) {
                    let index = (source_y * source_width + source_x) * 4;
                    let alpha = f32::from(pixels[index + 3]) / 255.0;
                    alpha_sum += alpha;
                    for channel in 0..3 {
                        premultiplied[channel] += f32::from(pixels[index + channel]) * alpha;
                    }
                    samples += 1.0;
                }
            }

            let output_index = (target_y * target_width + target_x) * 4;
            let alpha = (alpha_sum / samples.max(1.0)).clamp(0.0, 1.0);
            output[output_index + 3] = (alpha * 255.0).round() as u8;
            if alpha > f32::EPSILON {
                for channel in 0..3 {
                    output[output_index + channel] =
                        (premultiplied[channel] / alpha / samples.max(1.0))
                            .round()
                            .clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
    output
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
    logical_width: u32,
    logical_height: u32,
    target_width: u32,
    target_height: u32,
    output_is_srgb: bool,
) -> VertexBatch<SolidVertex> {
    let scale_x = target_width.max(1) as f32 / logical_width.max(1) as f32;
    let scale_y = target_height.max(1) as f32 / logical_height.max(1) as f32;
    let mut vertices = Vec::with_capacity(draw_list.solids.len() * 12);
    let mut ranges = Vec::with_capacity(draw_list.solids.len());
    for quad in &draw_list.solids {
        let start = vertices.len() as u32;
        let rect = physical_snap_rect(quad.rect, scale_x, scale_y);
        let positions = rounded_ndc_quad(
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            quad.radius * scale_x.min(scale_y),
            target_width,
            target_height,
        );
        let color = color_to_f32(quad.color, output_is_srgb);
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

    // Rounded controls are prominent at small UI sizes. A denser adaptive
    // perimeter removes visible facets without introducing a heavy tessellator.
    let segments = (radius * 1.5).ceil().clamp(4.0, 12.0) as usize;
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
    logical_width: u32,
    logical_height: u32,
    target_width: u32,
    target_height: u32,
    output_is_srgb: bool,
) -> VertexBatch<TextVertex> {
    let scale_x = target_width.max(1) as f32 / logical_width.max(1) as f32;
    let scale_y = target_height.max(1) as f32 / logical_height.max(1) as f32;
    let atlas_width = f32::from(draw_list.atlas_size[0].max(1));
    let atlas_height = f32::from(draw_list.atlas_size[1].max(1));
    let mut vertices = Vec::with_capacity(draw_list.text.len() * 6);
    let mut ranges = Vec::with_capacity(draw_list.text.len());
    for quad in &draw_list.text {
        let start = vertices.len() as u32;
        let rect = physical_snap_rect(quad.rect, scale_x, scale_y);
        let positions = ndc_quad(
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            target_width,
            target_height,
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
        let color = color_to_f32(quad.color, output_is_srgb);
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

fn image_vertices(
    draw_list: &UiSurfaceDrawList,
    logical_width: u32,
    logical_height: u32,
    target_width: u32,
    target_height: u32,
    images: &UiSurfaceImageStore,
    output_is_srgb: bool,
) -> VertexBatch<TextVertex> {
    let scale_x = target_width.max(1) as f32 / logical_width.max(1) as f32;
    let scale_y = target_height.max(1) as f32 / logical_height.max(1) as f32;
    let mut vertices = Vec::with_capacity(draw_list.images.len() * 6);
    let mut ranges = Vec::with_capacity(draw_list.images.len());
    for quad in &draw_list.images {
        let start = vertices.len() as u32;
        if let Some(image) = images.get(&quad.source_key) {
            let rect = super::cpu_renderer::fitted_image_rect(quad.rect, image.size, quad.fit);
            let rect = physical_snap_rect(rect, scale_x, scale_y);
            let positions = ndc_quad(
                rect.x,
                rect.y,
                rect.width,
                rect.height,
                target_width,
                target_height,
            );
            let uvs = [
                [0.0, 0.0],
                [1.0, 0.0],
                [1.0, 1.0],
                [0.0, 0.0],
                [1.0, 1.0],
                [0.0, 1.0],
            ];
            let color = color_to_f32(quad.tint, output_is_srgb);
            for (position, uv) in positions.into_iter().zip(uvs) {
                vertices.push(TextVertex {
                    position,
                    uv,
                    color,
                });
            }
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

fn physical_snap_rect(rect: super::UiRect, scale_x: f32, scale_y: f32) -> super::UiRect {
    let left = (rect.x * scale_x).round();
    let top = (rect.y * scale_y).round();
    let right = (rect.right() * scale_x).round().max(left);
    let bottom = (rect.bottom() * scale_y).round().max(top);
    super::UiRect::new(left, top, right - left, bottom - top)
}

fn color_to_f32(color: [u8; 4], output_is_srgb: bool) -> [f32; 4] {
    [
        color_channel_to_linear(color[0], output_is_srgb),
        color_channel_to_linear(color[1], output_is_srgb),
        color_channel_to_linear(color[2], output_is_srgb),
        f32::from(color[3]) / 255.0,
    ]
}

fn color_from_bytes(color: [u8; 4], output_is_srgb: bool) -> wgpu::Color {
    wgpu::Color {
        r: f64::from(color_channel_to_linear(color[0], output_is_srgb)),
        g: f64::from(color_channel_to_linear(color[1], output_is_srgb)),
        b: f64::from(color_channel_to_linear(color[2], output_is_srgb)),
        a: f64::from(color[3]) / 255.0,
    }
}

fn color_channel_to_linear(channel: u8, output_is_srgb: bool) -> f32 {
    let encoded = f32::from(channel) / 255.0;
    if !output_is_srgb {
        return encoded;
    }
    if encoded <= 0.040_45 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    }
}

fn ui_scissor_rect(
    rect: Option<super::UiRect>,
    size: [u32; 2],
    logical_size: [u32; 2],
) -> Option<(u32, u32, u32, u32)> {
    let rect = rect?;
    let scale_x = size[0].max(1) as f32 / logical_size[0].max(1) as f32;
    let scale_y = size[1].max(1) as f32 / logical_size[1].max(1) as f32;
    let x = (rect.x * scale_x).floor().max(0.0).min(size[0] as f32) as u32;
    let y = (rect.y * scale_y).floor().max(0.0).min(size[1] as f32) as u32;
    let right = (rect.right() * scale_x).ceil().max(0.0).min(size[0] as f32) as u32;
    let bottom = (rect.bottom() * scale_y)
        .ceil()
        .max(0.0)
        .min(size[1] as f32) as u32;
    (right > x && bottom > y).then_some((x, y, right - x, bottom - y))
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
        assert!(rounded.len() <= 4 * (12 + 1) * 3);
    }

    #[test]
    fn vertex_batches_keep_one_draw_range_per_surface_item() {
        let list = UiSurfaceDrawList {
            solids: vec![
                super::super::UiSurfaceQuad {
                    rect: super::super::UiRect::new(0.0, 0.0, 40.0, 20.0),
                    clip_rect: super::super::UiRect::new(0.0, 0.0, 160.0, 80.0),
                    color: [255, 255, 255, 255],
                    z_index: 0,
                    radius: 0.0,
                },
                super::super::UiSurfaceQuad {
                    rect: super::super::UiRect::new(40.0, 0.0, 40.0, 20.0),
                    clip_rect: super::super::UiRect::new(0.0, 0.0, 160.0, 80.0),
                    color: [255, 128, 0, 255],
                    z_index: 1,
                    radius: 6.0,
                },
            ],
            ..UiSurfaceDrawList::default()
        };
        let batch = solid_vertices(&list, 160, 80, 160, 80, true);

        assert_eq!(batch.ranges.len(), 2);
        assert_eq!(batch.ranges[0], 0..6);
        assert!(batch.ranges[1].end > batch.ranges[1].start);
    }

    #[test]
    fn physical_target_scales_logical_geometry_before_ndc_conversion() {
        let list = UiSurfaceDrawList {
            solids: vec![super::super::UiSurfaceQuad {
                rect: super::super::UiRect::new(40.0, 20.0, 40.0, 20.0),
                clip_rect: super::super::UiRect::new(0.0, 0.0, 160.0, 80.0),
                color: [255, 255, 255, 255],
                z_index: 0,
                radius: 0.0,
            }],
            ..UiSurfaceDrawList::default()
        };

        let batch = solid_vertices(&list, 160, 80, 320, 160, true);

        assert_eq!(batch.vertices[0].position, ndc_point(80.0, 40.0, 320, 160));
        assert_eq!(batch.vertices[1].position, ndc_point(160.0, 40.0, 320, 160));
    }

    #[test]
    fn physical_geometry_snaps_small_controls_to_pixel_boundaries() {
        let snapped = physical_snap_rect(
            super::super::UiRect::new(10.24, 4.76, 13.52, 9.48),
            1.0,
            1.0,
        );

        assert_eq!(snapped, super::super::UiRect::new(10.0, 5.0, 14.0, 9.0));
    }

    #[test]
    fn adjacent_compatible_paint_commands_merge_into_one_gpu_draw() {
        let list = UiSurfaceDrawList {
            solids: vec![
                super::super::UiSurfaceQuad {
                    rect: super::super::UiRect::new(0.0, 0.0, 40.0, 20.0),
                    clip_rect: super::super::UiRect::new(0.0, 0.0, 160.0, 80.0),
                    color: [255, 255, 255, 255],
                    z_index: 0,
                    radius: 0.0,
                },
                super::super::UiSurfaceQuad {
                    rect: super::super::UiRect::new(40.0, 0.0, 40.0, 20.0),
                    clip_rect: super::super::UiRect::new(0.0, 0.0, 160.0, 80.0),
                    color: [255, 128, 0, 255],
                    z_index: 0,
                    radius: 0.0,
                },
            ],
            paint_order: vec![
                UiSurfacePaintCommand::Solid {
                    index: 0,
                    z_index: 0,
                    sequence: 0,
                },
                UiSurfacePaintCommand::Solid {
                    index: 1,
                    z_index: 0,
                    sequence: 1,
                },
            ],
            ..UiSurfaceDrawList::default()
        };
        let geometry = UiSurfaceGpuGeometryCache {
            fingerprint: 1,
            solid_batch: solid_vertices(&list, 160, 80, 160, 80, true),
            text_batch: text_vertices(&list, 160, 80, 160, 80, true),
            image_batch: image_vertices(
                &list,
                160,
                80,
                160,
                80,
                &UiSurfaceImageStore::default(),
                true,
            ),
        };

        let runs = build_paint_runs(&list, &geometry, [160, 80], [160, 80]);

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].range, 0..12);
    }

    #[test]
    fn srgb_surface_colors_are_linearized_before_gpu_compositing() {
        let dark = color_to_f32([8, 8, 8, 255], true);
        let raw = color_to_f32([8, 8, 8, 255], false);

        assert!(dark[0] < raw[0]);
        assert!((raw[0] - 8.0 / 255.0).abs() < f32::EPSILON);
        assert!((color_from_bytes([255, 255, 255, 255], true).r - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn scissor_uses_physical_pixels_for_a_hidpi_target() {
        let clip = super::super::UiRect::new(10.0, 20.0, 30.0, 40.0);

        assert_eq!(
            ui_scissor_rect(Some(clip), [200, 160], [100, 80]),
            Some((20, 40, 60, 80))
        );
    }

    #[test]
    fn image_mip_levels_cover_small_icon_minification() {
        assert_eq!(image_mip_level_count([1, 1]), 1);
        assert_eq!(image_mip_level_count([16, 8]), 5);
        assert_eq!(image_mip_level_count([64, 64]), 7);

        let (pixels, row_bytes) = padded_rgba_rows(&[255; 4 * 3 * 2], [3, 2]);
        assert_eq!(row_bytes, 256);
        assert_eq!(pixels.len(), 256 * 2);
        assert_eq!(&pixels[..12], &[255; 12]);

        let mip = downsample_rgba_premultiplied(&[255, 0, 0, 255, 0, 0, 0, 0], [2, 1], [1, 1]);
        assert_eq!(mip, vec![255, 0, 0, 128]);
    }
}

fn atlas_rgba_region(alpha: &[u8], size: [u16; 2], region: UiTextAtlasRect) -> Vec<u8> {
    let atlas_width = usize::from(size[0]);
    let width = usize::from(region.width);
    let height = usize::from(region.height);
    let mut rgba = Vec::with_capacity(width * height * 4);
    for row in 0..height {
        let start = (usize::from(region.y) + row) * atlas_width + usize::from(region.x);
        for value in &alpha[start..start + width] {
            rgba.extend_from_slice(&[255, 255, 255, *value]);
        }
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

@fragment
fn image_fs(input: TextOut) -> @location(0) vec4<f32> {
    return textureSample(atlas_texture, atlas_sampler, input.uv) * input.color;
}
"#;
