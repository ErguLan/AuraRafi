//! Direct WGPU compositor for retained UI surfaces.
//!
//! It renders into any caller-owned texture view. Window ownership stays
//! outside this type, which keeps it usable by a native Winit host, tests, or
//! an off-screen editor surface without coupling it to a window framework.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};

use crate::api_graphic_basic::canvas_presenter::CanvasTargetRect;
use crate::api_graphic_basic::capabilities::GraphicsMemoryBudget;
use crate::api_graphic_basic::device::GpuTextureView;
use raf_ui::{UiImagePlaceholder, UiImageResourceStatus, UiImageSampling};

use super::effects::{
    normalized_corners, UiPaintEffects, UiPaintKey, UiRoundedClip, MAX_ROUNDED_CLIPS,
};
use super::image_layout::{image_patches, ImagePatch};
use super::{
    UiRect, UiSurfaceDrawList, UiSurfaceImageStore, UiSurfacePaintCommand, UiTextAtlas,
    UiTextAtlasRect,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiSurfaceGpuMetrics {
    pub solid_vertices: u32,
    pub stroke_vertices: u32,
    pub text_vertices: u32,
    pub image_vertices: u32,
    pub atlas_uploads: u32,
    pub image_uploads: u32,
    pub image_upload_bytes: u64,
    pub image_evictions: u32,
    pub image_resident_bytes: u64,
    pub image_budget_exceeded: bool,
    pub geometry_cache_hit: bool,
    pub buffer_upload_bytes: u64,
    pub paint_runs: u32,
    pub draw_calls: u32,
    /// Number of submitted solid quads that require alpha blending.
    pub translucent_solid_quads: u32,
    /// Approximate physical-pixel coverage submitted by translucent solids.
    /// Overlapping quads are intentionally counted independently.
    pub estimated_translucent_pixels: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SolidVertex {
    position: [f32; 2],
    local_position: [f32; 2],
    color: [f32; 4],
    effect_index: u32,
}

impl SolidVertex {
    fn layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        const ATTRIBUTES: [wgpu::VertexAttribute; 4] =
            wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4, 3 => Uint32];
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
    local_position: [f32; 2],
    color: [f32; 4],
    effect_index: u32,
}

struct VertexBatch<T> {
    vertices: Vec<T>,
    ranges: Vec<Range<u32>>,
}

struct UiSurfaceGpuGeometryCache {
    key: UiSurfaceGpuGeometryKey,
    effect_table: GpuEffectTable,
    solid_batch: VertexBatch<SolidVertex>,
    stroke_batch: VertexBatch<SolidVertex>,
    text_batch: VertexBatch<TextVertex>,
    image_batch: VertexBatch<TextVertex>,
    paint_runs: Vec<UiSurfacePaintRun>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct UiSurfaceGpuGeometryKey {
    draw_list_identity: usize,
    draw_list_revision: u64,
    logical_size: [u32; 2],
    target_size: [u32; 2],
    atlas_revision: u64,
    atlas_size: [u16; 2],
    image_signature: u64,
    icon_atlas_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum UiSurfacePaintKind {
    Solid,
    Stroke,
    Text,
    Image(String, UiImageSampling),
    IconAtlas(usize, UiImageSampling),
    ExternalImage(String, UiImageSampling),
    MissingImage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UiSurfacePaintRun {
    kind: UiSurfacePaintKind,
    range: Range<u32>,
    scissor: (u32, u32, u32, u32),
}

struct UiImageGpuTexture {
    _texture: wgpu::Texture,
    linear_bind_group: wgpu::BindGroup,
    nearest_bind_group: wgpu::BindGroup,
    revision: u64,
    size: [u32; 2],
    bytes: u64,
    last_used_frame: u64,
}

struct UiExternalImage {
    _view: GpuTextureView,
    linear_bind_group: wgpu::BindGroup,
    nearest_bind_group: wgpu::BindGroup,
    size: [u32; 2],
    revision: u64,
    bytes: u64,
}

struct UiIconAtlasPage {
    _texture: wgpu::Texture,
    linear_bind_group: wgpu::BindGroup,
    nearest_bind_group: wgpu::BindGroup,
}

#[derive(Debug, Clone, Copy)]
struct UiIconAtlasEntry {
    page: usize,
    uv: UiRect,
}

#[derive(Default)]
struct UiBuiltinIconAtlas {
    signature: Option<u64>,
    revision: u64,
    pages: Vec<UiIconAtlasPage>,
    entries: HashMap<String, UiIconAtlasEntry>,
    resident_bytes: u64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct GpuPaintEffect {
    rect: [f32; 4],
    corners: [f32; 4],
    border_color: [f32; 4],
    border_widths: [f32; 4],
    gradient_start: [f32; 4],
    gradient_end: [f32; 4],
    gradient_direction_softness: [f32; 4],
    clip_range_flags: [u32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct GpuRoundedClip {
    rect: [f32; 4],
    corners: [f32; 4],
}

#[derive(Default)]
struct GpuEffectTable {
    paints: Vec<GpuPaintEffect>,
    clips: Vec<GpuRoundedClip>,
    indices: HashMap<UiPaintKey, u32>,
}

impl GpuPaintEffect {
    const HAS_SHAPE: u32 = 1;
    const HAS_BORDER: u32 = 2;
    const HAS_GRADIENT: u32 = 4;
    const SOFT_FALLOFF: u32 = 8;
    const INVALID_CLIPS: u32 = 16;
}

impl Default for GpuPaintEffect {
    fn default() -> Self {
        Self::zeroed()
    }
}

impl TextVertex {
    fn layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        const ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
            0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32x4, 4 => Uint32
        ];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRIBUTES,
        }
    }
}

pub(crate) struct UiSurfaceGpuSharedResources {
    solid_pipeline: wgpu::RenderPipeline,
    text_pipeline: wgpu::RenderPipeline,
    image_pipeline: wgpu::RenderPipeline,
    external_image_pipeline: wgpu::RenderPipeline,
    missing_image_pipeline: wgpu::RenderPipeline,
    effects_layout: wgpu::BindGroupLayout,
    atlas_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    image_sampler: wgpu::Sampler,
    nearest_sampler: wgpu::Sampler,
    icon_linear_sampler: wgpu::Sampler,
    icon_nearest_sampler: wgpu::Sampler,
}

pub struct UiSurfaceGpuRenderer {
    shared: Arc<UiSurfaceGpuSharedResources>,
    atlas_texture: Option<wgpu::Texture>,
    atlas_bind_group: Option<wgpu::BindGroup>,
    atlas_size: [u16; 2],
    solid_buffer: Option<wgpu::Buffer>,
    solid_capacity: u64,
    stroke_buffer: Option<wgpu::Buffer>,
    stroke_capacity: u64,
    text_buffer: Option<wgpu::Buffer>,
    text_capacity: u64,
    image_buffer: Option<wgpu::Buffer>,
    image_capacity: u64,
    image_textures: HashMap<String, UiImageGpuTexture>,
    external_images: HashMap<String, UiExternalImage>,
    external_image_resident_bytes: u64,
    external_image_revision: u64,
    icon_atlas: UiBuiltinIconAtlas,
    image_resident_bytes: u64,
    image_budget_bytes: u64,
    image_entry_budget: usize,
    image_frame_index: u64,
    effect_buffer: Option<wgpu::Buffer>,
    clip_buffer: Option<wgpu::Buffer>,
    effect_bind_group: Option<wgpu::BindGroup>,
    effect_capacity: u64,
    clip_capacity: u64,
    geometry_cache: Option<UiSurfaceGpuGeometryCache>,
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

impl UiSurfaceGpuRenderer {
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
        Self::with_shared_and_budget(
            Arc::new(UiSurfaceGpuSharedResources::new(device, color_format)),
            GraphicsMemoryBudget::default(),
        )
    }

    pub(crate) fn with_shared_and_budget(
        shared: Arc<UiSurfaceGpuSharedResources>,
        memory_budget: GraphicsMemoryBudget,
    ) -> Self {
        Self {
            shared,
            atlas_texture: None,
            atlas_bind_group: None,
            atlas_size: [0, 0],
            solid_buffer: None,
            solid_capacity: 0,
            stroke_buffer: None,
            stroke_capacity: 0,
            text_buffer: None,
            text_capacity: 0,
            image_buffer: None,
            image_capacity: 0,
            image_textures: HashMap::new(),
            external_images: HashMap::new(),
            external_image_resident_bytes: 0,
            external_image_revision: 0,
            icon_atlas: UiBuiltinIconAtlas::default(),
            image_resident_bytes: 0,
            image_budget_bytes: memory_budget
                .gpu_bytes
                .saturating_div(16)
                .max(4 * 1024 * 1024),
            image_entry_budget: memory_budget.texture_cache_entries.max(1) as usize,
            image_frame_index: 0,
            effect_buffer: None,
            clip_buffer: None,
            effect_bind_group: None,
            effect_capacity: 0,
            clip_capacity: 0,
            geometry_cache: None,
        }
    }

    /// Registers a GPU-rendered image without copying it through CPU memory.
    /// The renderer retains the view and its filtering bind groups. External
    /// residency is bounded by the renderer's image cache budget. The view
    /// must be a filterable 2D render target whose sampled RGB is already
    /// premultiplied.
    pub(crate) fn register_external_image(
        &mut self,
        device: &wgpu::Device,
        key: impl Into<String>,
        view: GpuTextureView,
        size: [u32; 2],
        revision: u64,
    ) -> Result<(), String> {
        if size[0] == 0 || size[1] == 0 {
            return Err("external image size must be non-zero".to_string());
        }
        let key = key.into();
        if let Some(existing) = self.external_images.get_mut(&key) {
            if existing._view.handle() == view.handle() && existing.size == size {
                existing.revision = revision;
                return Ok(());
            }
        }
        let bytes = u64::from(size[0])
            .saturating_mul(u64::from(size[1]))
            .saturating_mul(4);
        let replacing = self
            .external_images
            .get(&key)
            .map_or(0, |image| image.bytes);
        let next_bytes = self
            .external_image_resident_bytes
            .saturating_sub(replacing)
            .saturating_add(bytes);
        if !self.external_images.contains_key(&key)
            && self.external_images.len() >= self.image_entry_budget
        {
            return Err("external image entry budget is full".to_string());
        }
        let replaced_cpu_bytes = self.image_textures.get(&key).map_or(0, |image| image.bytes);
        if next_bytes
            .saturating_add(self.icon_atlas.resident_bytes)
            .saturating_add(self.image_resident_bytes.saturating_sub(replaced_cpu_bytes))
            > self.image_budget_bytes
        {
            return Err("external image byte budget exceeded".to_string());
        }
        if let Some(previous) = self.image_textures.remove(&key) {
            self.image_resident_bytes = self.image_resident_bytes.saturating_sub(previous.bytes);
        }
        let (linear_bind_group, nearest_bind_group) =
            create_image_bind_groups(device, &self.shared, view.as_wgpu(), false);
        self.external_images.insert(
            key,
            UiExternalImage {
                _view: view,
                linear_bind_group,
                nearest_bind_group,
                size,
                revision,
                bytes,
            },
        );
        self.external_image_resident_bytes = next_bytes;
        self.external_image_revision = self.external_image_revision.wrapping_add(1);
        self.geometry_cache = None;
        Ok(())
    }

    /// Releases a previously registered GPU image and its retained view.
    pub(crate) fn remove_external_image(&mut self, key: &str) -> bool {
        let Some(image) = self.external_images.remove(key) else {
            return false;
        };
        self.external_image_resident_bytes = self
            .external_image_resident_bytes
            .saturating_sub(image.bytes);
        self.external_image_revision = self.external_image_revision.wrapping_add(1);
        self.geometry_cache = None;
        true
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
        self.render_at_scale_with_revision(
            device,
            queue,
            target,
            target_size,
            logical_size,
            0,
            draw_list,
            atlas,
            images,
            clear_color,
        )
    }

    pub(crate) fn render_at_scale_with_revision(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        target_size: [u32; 2],
        logical_size: [u32; 2],
        draw_list_revision: u64,
        draw_list: &UiSurfaceDrawList,
        atlas: &mut UiTextAtlas,
        images: &UiSurfaceImageStore,
        clear_color: [u8; 4],
    ) -> UiSurfaceGpuMetrics {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ApiGraphicBasic.UiSurfaceEncoder"),
        });
        let metrics = self.encode_at_scale_with_revision(
            device,
            queue,
            &mut encoder,
            target,
            target_size,
            logical_size,
            draw_list_revision,
            draw_list,
            atlas,
            images,
            wgpu::LoadOp::Clear(color_from_bytes(clear_color, true)),
        );
        queue.submit(std::iter::once(encoder.finish()));
        metrics
    }

    pub(crate) fn encode_at_scale_with_revision(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        target_size: [u32; 2],
        logical_size: [u32; 2],
        draw_list_revision: u64,
        draw_list: &UiSurfaceDrawList,
        atlas: &mut UiTextAtlas,
        images: &UiSurfaceImageStore,
        load: wgpu::LoadOp<wgpu::Color>,
    ) -> UiSurfaceGpuMetrics {
        self.encode_in_rect_with_revision(
            device,
            queue,
            encoder,
            target,
            target_size,
            CanvasTargetRect::full(target_size),
            logical_size,
            draw_list_revision,
            draw_list,
            atlas,
            images,
            load,
        )
    }

    pub(crate) fn encode_in_rect_with_revision(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        target_size: [u32; 2],
        target_rect: CanvasTargetRect,
        logical_size: [u32; 2],
        draw_list_revision: u64,
        draw_list: &UiSurfaceDrawList,
        atlas: &mut UiTextAtlas,
        images: &UiSurfaceImageStore,
        load: wgpu::LoadOp<wgpu::Color>,
    ) -> UiSurfaceGpuMetrics {
        let target_rect = target_rect.clipped(target_size);
        if target_rect.width == 0 || target_rect.height == 0 {
            return UiSurfaceGpuMetrics::default();
        }
        let width = target_rect.width;
        let height = target_rect.height;
        let logical_width = logical_size[0].max(1);
        let logical_height = logical_size[1].max(1);
        self.image_frame_index = self.image_frame_index.wrapping_add(1).max(1);
        let mut metrics = UiSurfaceGpuMetrics::default();
        (
            metrics.translucent_solid_quads,
            metrics.estimated_translucent_pixels,
        ) = estimate_translucent_solids(
            draw_list,
            [logical_width, logical_height],
            [width, height],
        );
        metrics.atlas_uploads = u32::from(self.sync_atlas(device, queue, atlas));
        let icon_atlas_upload_bytes = self.sync_icon_atlas(device, queue, images, draw_list);
        if icon_atlas_upload_bytes > 0 {
            metrics.atlas_uploads = metrics.atlas_uploads.saturating_add(1);
            metrics.image_upload_bytes = metrics
                .image_upload_bytes
                .saturating_add(icon_atlas_upload_bytes);
        }
        let mut uploaded_image_keys = HashSet::new();
        for quad in &draw_list.images {
            let atlas_resident = self.icon_atlas.entries.contains_key(&quad.source_key);
            if !atlas_resident
                && !self.external_images.contains_key(&quad.source_key)
                && uploaded_image_keys.insert(quad.source_key.as_str())
            {
                metrics.image_uploads += u32::from(self.sync_image(
                    device,
                    queue,
                    images,
                    &quad.source_key,
                    &mut metrics,
                ));
            }
        }
        metrics.image_resident_bytes = self
            .image_resident_bytes
            .saturating_add(self.icon_atlas.resident_bytes)
            .saturating_add(self.external_image_resident_bytes);
        metrics.image_budget_exceeded = metrics.image_resident_bytes > self.image_budget_bytes;

        let image_signature = image_resources_signature(
            draw_list,
            images,
            &self.image_textures,
            &self.external_images,
            self.external_image_revision,
        );
        let atlas_size = atlas.size();
        let geometry_key = UiSurfaceGpuGeometryKey {
            draw_list_identity: if draw_list_revision == 0 {
                std::ptr::from_ref(draw_list) as usize
            } else {
                0
            },
            draw_list_revision,
            logical_size: [logical_width, logical_height],
            target_size: [width, height],
            atlas_revision: atlas.revision(),
            atlas_size,
            image_signature,
            icon_atlas_revision: self.icon_atlas.revision,
        };
        let geometry_rebuilt = self
            .geometry_cache
            .as_ref()
            .map_or(true, |cached| cached.key != geometry_key);
        metrics.geometry_cache_hit = !geometry_rebuilt;
        if geometry_rebuilt {
            // UiStyle colors and registered PNGs are stored as sRGB values.
            // The compositor operates in linear light even when its target is
            // Unorm because the native compositor presents it into an sRGB target.
            let effect_table = build_gpu_effect_table(
                draw_list,
                logical_width,
                logical_height,
                width,
                height,
                true,
            );
            let solid_batch = solid_vertices(
                draw_list,
                logical_width,
                logical_height,
                width,
                height,
                true,
                &effect_table,
            );
            let stroke_batch = stroke_vertices(
                draw_list,
                logical_width,
                logical_height,
                width,
                height,
                true,
                &effect_table,
            );
            let text_batch = text_vertices(
                draw_list,
                logical_width,
                logical_height,
                width,
                height,
                true,
                &effect_table,
            );
            let image_batch = image_vertices(
                draw_list,
                logical_width,
                logical_height,
                width,
                height,
                images,
                true,
                &draw_list.effects,
                &effect_table,
                &self.icon_atlas.entries,
                &self.image_textures,
                &self.external_images,
            );
            let mut geometry = UiSurfaceGpuGeometryCache {
                key: geometry_key,
                effect_table,
                solid_batch,
                stroke_batch,
                text_batch,
                image_batch,
                paint_runs: Vec::new(),
            };
            self.ensure_effect_buffers(device, queue, &geometry.effect_table, &mut metrics);
            geometry.paint_runs = build_paint_runs(
                draw_list,
                &geometry,
                [width, height],
                [logical_width, logical_height],
                &self.icon_atlas.entries,
                &self.image_textures,
                &self.external_images,
            );
            metrics.solid_vertices = geometry.solid_batch.vertices.len() as u32;
            metrics.stroke_vertices = geometry.stroke_batch.vertices.len() as u32;
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
            if !geometry.stroke_batch.vertices.is_empty() {
                self.ensure_stroke_buffer(device, geometry.stroke_batch.vertices.len());
                if let Some(buffer) = self.stroke_buffer.as_ref() {
                    let bytes = bytemuck::cast_slice(&geometry.stroke_batch.vertices);
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
            metrics.stroke_vertices = geometry.stroke_batch.vertices.len() as u32;
            metrics.text_vertices = geometry.text_batch.vertices.len() as u32;
            metrics.image_vertices = geometry.image_batch.vertices.len() as u32;
        }
        let geometry = self
            .geometry_cache
            .as_ref()
            .expect("GPU UI geometry cache is initialized before rendering");
        let paint_runs = &geometry.paint_runs;
        metrics.paint_runs = paint_runs.len() as u32;
        metrics.draw_calls = metrics.paint_runs;

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ApiGraphicBasic.UiSurfacePass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_viewport(
                target_rect.x as f32,
                target_rect.y as f32,
                target_rect.width as f32,
                target_rect.height as f32,
                0.0,
                1.0,
            );
            let solid_buffer = self
                .solid_buffer
                .as_ref()
                .filter(|_| !geometry.solid_batch.vertices.is_empty());
            let stroke_buffer = self
                .stroke_buffer
                .as_ref()
                .filter(|_| !geometry.stroke_batch.vertices.is_empty());
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
            for run in paint_runs {
                match &run.kind {
                    UiSurfacePaintKind::Solid => {
                        let (Some(buffer), Some(effects)) =
                            (solid_buffer, self.effect_bind_group.as_ref())
                        else {
                            continue;
                        };
                        if active_pipeline != Some(&run.kind) {
                            pass.set_pipeline(&self.shared.solid_pipeline);
                            pass.set_vertex_buffer(0, buffer.slice(..));
                            pass.set_bind_group(0, effects, &[]);
                            active_pipeline = Some(&run.kind);
                        }
                    }
                    UiSurfacePaintKind::Stroke => {
                        let (Some(buffer), Some(effects)) =
                            (stroke_buffer, self.effect_bind_group.as_ref())
                        else {
                            continue;
                        };
                        if active_pipeline != Some(&run.kind) {
                            pass.set_pipeline(&self.shared.solid_pipeline);
                            pass.set_vertex_buffer(0, buffer.slice(..));
                            pass.set_bind_group(0, effects, &[]);
                            active_pipeline = Some(&run.kind);
                        }
                    }
                    UiSurfacePaintKind::Text => {
                        let (Some(buffer), Some(effects), Some(bind_group)) = (
                            text_buffer,
                            self.effect_bind_group.as_ref(),
                            self.atlas_bind_group.as_ref(),
                        ) else {
                            continue;
                        };
                        if active_pipeline != Some(&run.kind) {
                            pass.set_pipeline(&self.shared.text_pipeline);
                            pass.set_vertex_buffer(0, buffer.slice(..));
                            pass.set_bind_group(0, effects, &[]);
                            pass.set_bind_group(1, bind_group, &[]);
                            active_pipeline = Some(&run.kind);
                        }
                    }
                    UiSurfacePaintKind::Image(source_key, sampling) => {
                        let (Some(buffer), Some(effects), Some(texture)) = (
                            image_buffer,
                            self.effect_bind_group.as_ref(),
                            self.image_textures.get(source_key),
                        ) else {
                            continue;
                        };
                        if active_pipeline != Some(&run.kind) {
                            pass.set_pipeline(&self.shared.image_pipeline);
                            pass.set_vertex_buffer(0, buffer.slice(..));
                            pass.set_bind_group(0, effects, &[]);
                            pass.set_bind_group(1, image_bind_group(texture, *sampling), &[]);
                            active_pipeline = Some(&run.kind);
                        }
                    }
                    UiSurfacePaintKind::IconAtlas(page, sampling) => {
                        let (Some(buffer), Some(effects), Some(atlas_page)) = (
                            image_buffer,
                            self.effect_bind_group.as_ref(),
                            self.icon_atlas.pages.get(*page),
                        ) else {
                            continue;
                        };
                        if active_pipeline != Some(&run.kind) {
                            pass.set_pipeline(&self.shared.image_pipeline);
                            pass.set_vertex_buffer(0, buffer.slice(..));
                            pass.set_bind_group(0, effects, &[]);
                            pass.set_bind_group(1, icon_bind_group(atlas_page, *sampling), &[]);
                            active_pipeline = Some(&run.kind);
                        }
                    }
                    UiSurfacePaintKind::ExternalImage(source_key, sampling) => {
                        let (Some(buffer), Some(effects), Some(texture)) = (
                            image_buffer,
                            self.effect_bind_group.as_ref(),
                            self.external_images.get(source_key),
                        ) else {
                            continue;
                        };
                        if active_pipeline != Some(&run.kind) {
                            pass.set_pipeline(&self.shared.external_image_pipeline);
                            pass.set_vertex_buffer(0, buffer.slice(..));
                            pass.set_bind_group(0, effects, &[]);
                            pass.set_bind_group(
                                1,
                                external_image_bind_group(texture, *sampling),
                                &[],
                            );
                            active_pipeline = Some(&run.kind);
                        }
                    }
                    UiSurfacePaintKind::MissingImage => {
                        let (Some(buffer), Some(effects)) =
                            (image_buffer, self.effect_bind_group.as_ref())
                        else {
                            continue;
                        };
                        if active_pipeline != Some(&run.kind) {
                            pass.set_pipeline(&self.shared.missing_image_pipeline);
                            pass.set_vertex_buffer(0, buffer.slice(..));
                            pass.set_bind_group(0, effects, &[]);
                            active_pipeline = Some(&run.kind);
                        }
                    }
                }
                if active_scissor != Some(run.scissor) {
                    pass.set_scissor_rect(
                        target_rect.x.saturating_add(run.scissor.0),
                        target_rect.y.saturating_add(run.scissor.1),
                        run.scissor.2,
                        run.scissor.3,
                    );
                    active_scissor = Some(run.scissor);
                }
                pass.draw(run.range.clone(), 0..1);
            }
        }
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
                layout: &self.shared.atlas_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.shared.sampler),
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
        metrics: &mut UiSurfaceGpuMetrics,
    ) -> bool {
        if images.resource_status(key) != UiImageResourceStatus::Ready {
            if let Some(previous) = self.image_textures.remove(key) {
                self.image_resident_bytes =
                    self.image_resident_bytes.saturating_sub(previous.bytes);
            }
            return false;
        }
        let Some(image) = images.get(key) else {
            if let Some(previous) = self.image_textures.remove(key) {
                self.image_resident_bytes =
                    self.image_resident_bytes.saturating_sub(previous.bytes);
            }
            return false;
        };
        if let Some(cached) = self.image_textures.get_mut(key) {
            cached.last_used_frame = self.image_frame_index;
            if cached.revision == image.revision {
                return false;
            }
            let previous = self
                .image_textures
                .remove(key)
                .expect("image cache entry existed before replacement");
            self.image_resident_bytes = self.image_resident_bytes.saturating_sub(previous.bytes);
        }
        let bytes = image_mip_chain_bytes(image.size);
        self.reserve_image_budget(bytes, key, metrics);
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
        let mut mip_pixels = premultiply_rgba_linear(&image.pixels);
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
                mip_pixels = downsample_premultiplied_linear(&mip_pixels, mip_size, next_size);
                mip_size = next_size;
            }
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let (linear_bind_group, nearest_bind_group) =
            create_image_bind_groups(device, &self.shared, &view, false);
        self.image_textures.insert(
            key.to_string(),
            UiImageGpuTexture {
                _texture: texture,
                linear_bind_group,
                nearest_bind_group,
                revision: image.revision,
                size: image.size,
                bytes,
                last_used_frame: self.image_frame_index,
            },
        );
        self.image_resident_bytes = self.image_resident_bytes.saturating_add(bytes);
        metrics.image_upload_bytes = metrics.image_upload_bytes.saturating_add(bytes);
        true
    }

    fn sync_icon_atlas(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        images: &UiSurfaceImageStore,
        draw_list: &UiSurfaceDrawList,
    ) -> u64 {
        const PAGE_SIZE: u32 = 512;
        const MIP_LEVELS: u32 = 3;
        const CELL_SIZE: u32 = 72;
        const GUTTER: u32 = 4;
        const ICON_SIZE: u32 = 64;
        const CELLS_PER_AXIS: usize = (PAGE_SIZE / CELL_SIZE) as usize;
        const CELLS_PER_PAGE: usize = CELLS_PER_AXIS * CELLS_PER_AXIS;
        const MAX_ENTRIES: usize = CELLS_PER_PAGE * 2;

        let mut keys = draw_list
            .images
            .iter()
            .map(|quad| quad.source_key.as_str())
            .filter(|key| key.starts_with("builtin://icon/"))
            .filter(|key| images.resource_status(key) == UiImageResourceStatus::Ready)
            .filter(|key| {
                images
                    .get(key)
                    .is_some_and(|image| image.size == [ICON_SIZE, ICON_SIZE])
            })
            .map(str::to_string)
            .collect::<Vec<_>>();
        keys.sort();
        keys.dedup();
        let page_bytes = (0..MIP_LEVELS)
            .map(|level| u64::from(PAGE_SIZE >> level).pow(2) * 4)
            .sum::<u64>();
        let available_pages = self
            .image_budget_bytes
            .saturating_sub(self.external_image_resident_bytes)
            / page_bytes;
        keys.truncate(MAX_ENTRIES.min(available_pages as usize * CELLS_PER_PAGE));

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for key in &keys {
            key.hash(&mut hasher);
            if let Some(image) = images.get(key) {
                image.revision.hash(&mut hasher);
                image.size.hash(&mut hasher);
            }
        }
        let signature = hasher.finish();
        if self.icon_atlas.signature == Some(signature) {
            return 0;
        }

        let page_count = if keys.is_empty() {
            0
        } else {
            (keys.len() + CELLS_PER_PAGE - 1) / CELLS_PER_PAGE
        };
        if page_count == 0 {
            let had_entries = !self.icon_atlas.entries.is_empty();
            self.icon_atlas.pages.clear();
            self.icon_atlas.entries.clear();
            self.icon_atlas.resident_bytes = 0;
            self.icon_atlas.signature = Some(signature);
            if had_entries {
                self.icon_atlas.revision = self.icon_atlas.revision.wrapping_add(1);
            }
            return 0;
        }
        let mut pages = Vec::with_capacity(page_count);
        let mut entries = HashMap::with_capacity(keys.len());
        for page_index in 0..page_count {
            let start = page_index * CELLS_PER_PAGE;
            let end = (start + CELLS_PER_PAGE).min(keys.len());
            let mut mip_pages = (0..MIP_LEVELS)
                .map(|level| {
                    let side = (PAGE_SIZE >> level) as usize;
                    vec![0_u8; side * side * 4]
                })
                .collect::<Vec<_>>();

            for (cell_index, key) in keys[start..end].iter().enumerate() {
                let Some(image) = images.get(key) else {
                    continue;
                };
                let cell_x = (cell_index % CELLS_PER_AXIS) as u32 * CELL_SIZE;
                let cell_y = (cell_index / CELLS_PER_AXIS) as u32 * CELL_SIZE;
                let mut source_mips = vec![premultiply_rgba_linear(&image.pixels)];
                for level in 1..MIP_LEVELS {
                    let previous_size = [ICON_SIZE >> (level - 1), ICON_SIZE >> (level - 1)];
                    let current_size = [ICON_SIZE >> level, ICON_SIZE >> level];
                    let next = downsample_premultiplied_linear(
                        source_mips.last().expect("base icon mip exists"),
                        previous_size,
                        current_size,
                    );
                    source_mips.push(next);
                }

                for level in 0..MIP_LEVELS {
                    let level = level as usize;
                    let page_side = (PAGE_SIZE >> level) as usize;
                    let cell_x = (cell_x >> level) as usize;
                    let cell_y = (cell_y >> level) as usize;
                    let gutter = (GUTTER >> level) as usize;
                    let icon_side = (ICON_SIZE >> level) as usize;
                    let cell_side = (CELL_SIZE >> level) as usize;
                    let source = &source_mips[level];
                    let page = &mut mip_pages[level];
                    for y in 0..cell_side {
                        let source_y = y.saturating_sub(gutter).min(icon_side - 1);
                        for x in 0..cell_side {
                            let source_x = x.saturating_sub(gutter).min(icon_side - 1);
                            let source_index = (source_y * icon_side + source_x) * 4;
                            let target_index = ((cell_y + y) * page_side + cell_x + x) * 4;
                            page[target_index..target_index + 4]
                                .copy_from_slice(&source[source_index..source_index + 4]);
                        }
                    }
                }

                let cell_x = (cell_index % CELLS_PER_AXIS) as u32 * CELL_SIZE;
                let cell_y = (cell_index / CELLS_PER_AXIS) as u32 * CELL_SIZE;
                entries.insert(
                    key.clone(),
                    UiIconAtlasEntry {
                        page: page_index,
                        uv: UiRect::new(
                            (cell_x + GUTTER) as f32 / PAGE_SIZE as f32,
                            (cell_y + GUTTER) as f32 / PAGE_SIZE as f32,
                            ICON_SIZE as f32 / PAGE_SIZE as f32,
                            ICON_SIZE as f32 / PAGE_SIZE as f32,
                        ),
                    },
                );
            }

            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("ApiGraphicBasic.UiSurfaceIconAtlas"),
                size: wgpu::Extent3d {
                    width: PAGE_SIZE,
                    height: PAGE_SIZE,
                    depth_or_array_layers: 1,
                },
                mip_level_count: MIP_LEVELS,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            for level in 0..MIP_LEVELS {
                let size = [PAGE_SIZE >> level, PAGE_SIZE >> level];
                let (pixels, row_bytes) = padded_rgba_rows(&mip_pages[level as usize], size);
                queue.write_texture(
                    wgpu::ImageCopyTexture {
                        texture: &texture,
                        mip_level: level,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &pixels,
                    wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(row_bytes),
                        rows_per_image: Some(size[1]),
                    },
                    wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                );
            }
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let (linear_bind_group, nearest_bind_group) =
                create_icon_atlas_bind_groups(device, &self.shared, &view);
            pages.push(UiIconAtlasPage {
                _texture: texture,
                linear_bind_group,
                nearest_bind_group,
            });
        }

        self.icon_atlas.pages = pages;
        self.icon_atlas.entries = entries;
        self.icon_atlas.resident_bytes = self.icon_atlas.pages.len() as u64
            * (u64::from(PAGE_SIZE * PAGE_SIZE)
                + u64::from((PAGE_SIZE / 2) * (PAGE_SIZE / 2))
                + u64::from((PAGE_SIZE / 4) * (PAGE_SIZE / 4)))
            * 4;
        self.icon_atlas.signature = Some(signature);
        self.icon_atlas.revision = self.icon_atlas.revision.wrapping_add(1);
        for key in self.icon_atlas.entries.keys() {
            if let Some(previous) = self.image_textures.remove(key) {
                self.image_resident_bytes =
                    self.image_resident_bytes.saturating_sub(previous.bytes);
            }
        }
        self.icon_atlas.resident_bytes
    }

    fn reserve_image_budget(
        &mut self,
        incoming_bytes: u64,
        protected_key: &str,
        metrics: &mut UiSurfaceGpuMetrics,
    ) {
        while self.image_textures.len() >= self.image_entry_budget
            || self
                .image_resident_bytes
                .saturating_add(self.external_image_resident_bytes)
                .saturating_add(self.icon_atlas.resident_bytes)
                .saturating_add(incoming_bytes)
                > self.image_budget_bytes
        {
            let candidate = self
                .image_textures
                .iter()
                .filter(|(key, texture)| {
                    key.as_str() != protected_key
                        && texture.last_used_frame < self.image_frame_index
                })
                .min_by_key(|(_, texture)| texture.last_used_frame)
                .map(|(key, _)| key.clone());
            let Some(candidate) = candidate else {
                // Keep all images used by the current draw list resident. If
                // that set is larger than the budget, allow the frame to
                // exceed it instead of evicting a texture and immediately
                // uploading it again later in the same frame.
                break;
            };
            if let Some(evicted) = self.image_textures.remove(&candidate) {
                self.image_resident_bytes = self.image_resident_bytes.saturating_sub(evicted.bytes);
                metrics.image_evictions = metrics.image_evictions.saturating_add(1);
            }
        }
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

    fn ensure_stroke_buffer(&mut self, device: &wgpu::Device, vertex_count: usize) {
        let required = (vertex_count * std::mem::size_of::<SolidVertex>()) as u64;
        if self.stroke_buffer.is_some() && self.stroke_capacity >= required {
            return;
        }
        self.stroke_capacity = required.next_power_of_two().max(256);
        self.stroke_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ApiGraphicBasic.UiSurfaceStrokeVertices"),
            size: self.stroke_capacity,
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

    fn ensure_effect_buffers(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        table: &GpuEffectTable,
        metrics: &mut UiSurfaceGpuMetrics,
    ) {
        let fallback_effect = GpuPaintEffect::default();
        let fallback_clip = GpuRoundedClip::zeroed();
        let paints = if table.paints.is_empty() {
            std::slice::from_ref(&fallback_effect)
        } else {
            &table.paints
        };
        let clips = if table.clips.is_empty() {
            std::slice::from_ref(&fallback_clip)
        } else {
            &table.clips
        };
        let paint_bytes = bytemuck::cast_slice(paints);
        let clip_bytes = bytemuck::cast_slice(clips);
        let paint_required = paint_bytes.len() as u64;
        let clip_required = clip_bytes.len() as u64;
        let mut bind_group_changed = false;
        if self.effect_buffer.is_none() || self.effect_capacity < paint_required {
            self.effect_capacity = paint_required.next_power_of_two().max(256);
            self.effect_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ApiGraphicBasic.UiSurfacePaintEffects"),
                size: self.effect_capacity,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            bind_group_changed = true;
        }
        if self.clip_buffer.is_none() || self.clip_capacity < clip_required {
            self.clip_capacity = clip_required.next_power_of_two().max(256);
            self.clip_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ApiGraphicBasic.UiSurfaceRoundedClips"),
                size: self.clip_capacity,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            bind_group_changed = true;
        }
        let effect_buffer = self
            .effect_buffer
            .as_ref()
            .expect("effect buffer allocated");
        let clip_buffer = self.clip_buffer.as_ref().expect("clip buffer allocated");
        queue.write_buffer(effect_buffer, 0, paint_bytes);
        queue.write_buffer(clip_buffer, 0, clip_bytes);
        metrics.buffer_upload_bytes = metrics
            .buffer_upload_bytes
            .saturating_add(paint_required)
            .saturating_add(clip_required);
        if bind_group_changed || self.effect_bind_group.is_none() {
            self.effect_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ApiGraphicBasic.UiSurfaceEffectsBindGroup"),
                layout: &self.shared.effects_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: effect_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: clip_buffer.as_entire_binding(),
                    },
                ],
            }));
        }
    }
}

fn estimate_translucent_solids(
    draw_list: &UiSurfaceDrawList,
    logical_size: [u32; 2],
    physical_size: [u32; 2],
) -> (u32, u64) {
    let bounds = UiRect::new(0.0, 0.0, logical_size[0] as f32, logical_size[1] as f32);
    let scale_x = physical_size[0] as f64 / f64::from(logical_size[0].max(1));
    let scale_y = physical_size[1] as f64 / f64::from(logical_size[1].max(1));
    let mut quads = 0_u32;
    let mut pixels = 0_u64;
    for quad in &draw_list.solids {
        if quad.color[3] == 0 || quad.color[3] == 255 {
            continue;
        }
        let clipped = quad.rect.intersection(quad.clip_rect).intersection(bounds);
        if clipped.is_empty() {
            continue;
        }
        quads = quads.saturating_add(1);
        let area = f64::from(clipped.width.max(0.0))
            * f64::from(clipped.height.max(0.0))
            * scale_x
            * scale_y;
        pixels = pixels.saturating_add(area.round().max(0.0) as u64);
    }
    (quads, pixels)
}

fn build_paint_runs(
    draw_list: &UiSurfaceDrawList,
    geometry: &UiSurfaceGpuGeometryCache,
    target_size: [u32; 2],
    logical_size: [u32; 2],
    icon_entries: &HashMap<String, UiIconAtlasEntry>,
    image_textures: &HashMap<String, UiImageGpuTexture>,
    external_images: &HashMap<String, UiExternalImage>,
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
            UiSurfacePaintCommand::Stroke { index, .. } => {
                let (Some(stroke), Some(range)) = (
                    draw_list.strokes.get(index),
                    geometry.stroke_batch.ranges.get(index),
                ) else {
                    continue;
                };
                ui_scissor_rect(Some(stroke.clip_rect), target_size, logical_size).map(|scissor| {
                    UiSurfacePaintRun {
                        kind: UiSurfacePaintKind::Stroke,
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
                let options = draw_list
                    .effects
                    .get(&UiPaintKey::Image(index))
                    .and_then(|effect| effect.image.as_ref())
                    .cloned()
                    .unwrap_or_default();
                let sampling = resolved_sampling(options.sampling);
                let kind = if external_images.contains_key(&quad.source_key) {
                    UiSurfacePaintKind::ExternalImage(quad.source_key.clone(), sampling)
                } else if let Some(entry) = icon_entries.get(&quad.source_key) {
                    UiSurfacePaintKind::IconAtlas(entry.page, sampling)
                } else if image_textures.contains_key(&quad.source_key) {
                    UiSurfacePaintKind::Image(quad.source_key.clone(), sampling)
                } else {
                    UiSurfacePaintKind::MissingImage
                };
                ui_scissor_rect(
                    Some(quad.rect.intersection(quad.clip_rect)),
                    target_size,
                    logical_size,
                )
                .map(|scissor| UiSurfacePaintRun {
                    kind,
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

fn image_mip_level_count(size: [u32; 2]) -> u32 {
    let mut largest = size[0].max(size[1]).max(1);
    let mut levels = 1;
    while largest > 1 {
        largest = (largest / 2).max(1);
        levels += 1;
    }
    levels
}

fn image_mip_chain_bytes(size: [u32; 2]) -> u64 {
    let mut width = size[0].max(1);
    let mut height = size[1].max(1);
    let mut bytes = 0_u64;
    loop {
        bytes = bytes.saturating_add(
            u64::from(width)
                .saturating_mul(u64::from(height))
                .saturating_mul(4),
        );
        if width == 1 && height == 1 {
            break;
        }
        width = (width / 2).max(1);
        height = (height / 2).max(1);
    }
    bytes
}

fn image_resources_signature(
    draw_list: &UiSurfaceDrawList,
    images: &UiSurfaceImageStore,
    gpu_images: &HashMap<String, UiImageGpuTexture>,
    external_images: &HashMap<String, UiExternalImage>,
    external_revision: u64,
) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    external_revision.hash(&mut hasher);
    for quad in &draw_list.images {
        quad.source_key.hash(&mut hasher);
        (images.resource_status(&quad.source_key) as u8).hash(&mut hasher);
        if let Some(image) = gpu_images.get(&quad.source_key) {
            image.revision.hash(&mut hasher);
            image.size.hash(&mut hasher);
        } else {
            0_u64.hash(&mut hasher);
        }
        if let Some(image) = external_images.get(&quad.source_key) {
            image.size.hash(&mut hasher);
            image.revision.hash(&mut hasher);
        }
    }
    hasher.finish()
}

fn resolved_sampling(sampling: UiImageSampling) -> UiImageSampling {
    match sampling {
        UiImageSampling::Auto => UiImageSampling::Linear,
        explicit => explicit,
    }
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
    downsample_premultiplied_linear(&premultiply_rgba_linear(pixels), source_size, target_size)
}

fn premultiply_rgba_linear(pixels: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(pixels.len());
    for pixel in pixels.chunks_exact(4) {
        let alpha = f32::from(pixel[3]) / 255.0;
        for channel in &pixel[..3] {
            let linear = srgb_to_linear(f32::from(*channel) / 255.0) * alpha;
            output.push((linear_to_srgb(linear) * 255.0).round().clamp(0.0, 255.0) as u8);
        }
        output.push(pixel[3]);
    }
    output
}

fn downsample_premultiplied_linear(
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
                        premultiplied[channel] +=
                            srgb_to_linear(f32::from(pixels[index + channel]) / 255.0);
                    }
                    samples += 1.0;
                }
            }

            let output_index = (target_y * target_width + target_x) * 4;
            let alpha = (alpha_sum / samples.max(1.0)).clamp(0.0, 1.0);
            output[output_index + 3] = (alpha * 255.0).round() as u8;
            for channel in 0..3 {
                let linear = premultiplied[channel] / samples.max(1.0);
                output[output_index + channel] =
                    (linear_to_srgb(linear) * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    output
}

fn srgb_to_linear(encoded: f32) -> f32 {
    if encoded <= 0.040_45 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(linear: f32) -> f32 {
    let linear = linear.clamp(0.0, 1.0);
    if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
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

fn create_image_bind_groups(
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

fn create_icon_atlas_bind_groups(
    device: &wgpu::Device,
    shared: &UiSurfaceGpuSharedResources,
    view: &wgpu::TextureView,
) -> (wgpu::BindGroup, wgpu::BindGroup) {
    create_image_bind_groups(device, shared, view, true)
}

fn image_bind_group(texture: &UiImageGpuTexture, sampling: UiImageSampling) -> &wgpu::BindGroup {
    match sampling {
        UiImageSampling::Nearest => &texture.nearest_bind_group,
        UiImageSampling::Auto | UiImageSampling::Linear => &texture.linear_bind_group,
    }
}

fn icon_bind_group(page: &UiIconAtlasPage, sampling: UiImageSampling) -> &wgpu::BindGroup {
    match sampling {
        UiImageSampling::Nearest => &page.nearest_bind_group,
        UiImageSampling::Auto | UiImageSampling::Linear => &page.linear_bind_group,
    }
}

fn external_image_bind_group(
    image: &UiExternalImage,
    sampling: UiImageSampling,
) -> &wgpu::BindGroup {
    match sampling {
        UiImageSampling::Nearest => &image.nearest_bind_group,
        UiImageSampling::Auto | UiImageSampling::Linear => &image.linear_bind_group,
    }
}

fn build_gpu_effect_table(
    draw_list: &UiSurfaceDrawList,
    logical_width: u32,
    logical_height: u32,
    target_width: u32,
    target_height: u32,
    output_is_srgb: bool,
) -> GpuEffectTable {
    let scale_x = target_width.max(1) as f32 / logical_width.max(1) as f32;
    let scale_y = target_height.max(1) as f32 / logical_height.max(1) as f32;
    let mut table = GpuEffectTable::default();
    let mut clip_chains = HashMap::<Vec<u32>, (u32, u32)>::new();

    for (index, quad) in draw_list.solids.iter().enumerate() {
        append_gpu_paint_effect(
            &mut table,
            &mut clip_chains,
            UiPaintKey::Solid(index),
            draw_list.effects.get(&UiPaintKey::Solid(index)),
            physical_snap_rect(quad.rect, scale_x, scale_y),
            [quad.radius; 4],
            true,
            scale_x,
            scale_y,
            output_is_srgb,
        );
    }
    for index in 0..draw_list.strokes.len() {
        append_gpu_paint_effect(
            &mut table,
            &mut clip_chains,
            UiPaintKey::Stroke(index),
            draw_list.effects.get(&UiPaintKey::Stroke(index)),
            UiRect::new(0.0, 0.0, 0.0, 0.0),
            [0.0; 4],
            false,
            scale_x,
            scale_y,
            output_is_srgb,
        );
    }
    for (index, quad) in draw_list.text.iter().enumerate() {
        append_gpu_paint_effect(
            &mut table,
            &mut clip_chains,
            UiPaintKey::Text(index),
            draw_list.effects.get(&UiPaintKey::Text(index)),
            physical_snap_rect(quad.rect, scale_x, scale_y),
            [0.0; 4],
            false,
            scale_x,
            scale_y,
            output_is_srgb,
        );
    }
    for (index, quad) in draw_list.images.iter().enumerate() {
        append_gpu_paint_effect(
            &mut table,
            &mut clip_chains,
            UiPaintKey::Image(index),
            draw_list.effects.get(&UiPaintKey::Image(index)),
            physical_snap_rect(quad.rect, scale_x, scale_y),
            [0.0; 4],
            false,
            scale_x,
            scale_y,
            output_is_srgb,
        );
    }
    if table.paints.is_empty() {
        table.paints.push(GpuPaintEffect::default());
    }
    table
}

#[allow(clippy::too_many_arguments)]
fn append_gpu_paint_effect(
    table: &mut GpuEffectTable,
    clip_chains: &mut HashMap<Vec<u32>, (u32, u32)>,
    key: UiPaintKey,
    effects: Option<&UiPaintEffects>,
    rect: UiRect,
    default_corners: [f32; 4],
    has_shape: bool,
    scale_x: f32,
    scale_y: f32,
    output_is_srgb: bool,
) {
    let effects = effects.cloned().unwrap_or_default();
    let scale = scale_x.min(scale_y);
    let corners = normalized_corners(
        rect,
        effects
            .corners
            .unwrap_or(default_corners)
            .map(|radius| radius * scale),
    );
    let mut flags = if has_shape {
        GpuPaintEffect::HAS_SHAPE
    } else {
        0
    };
    let border_widths = [
        effects.border_widths[0].max(0.0) * scale_y,
        effects.border_widths[1].max(0.0) * scale_x,
        effects.border_widths[2].max(0.0) * scale_y,
        effects.border_widths[3].max(0.0) * scale_x,
    ];
    if effects.border[3] > 0 && border_widths.iter().any(|width| *width > 0.0) {
        flags |= GpuPaintEffect::HAS_BORDER;
    }
    if effects.gradient.is_some() {
        flags |= GpuPaintEffect::HAS_GRADIENT;
    }
    let softness = if effects.softness.is_finite() {
        effects.softness.max(0.0) * scale
    } else {
        0.0
    };
    if softness > 0.0 {
        flags |= GpuPaintEffect::SOFT_FALLOFF;
    }

    let mut clip_key = Vec::new();
    let mut scaled_clips = Vec::new();
    let invalid_clips = effects.clips.len() > MAX_ROUNDED_CLIPS;
    if !invalid_clips {
        for clip in &effects.clips {
            let scaled = scale_rounded_clip(clip, scale_x, scale_y);
            clip_key.extend(
                [
                    scaled.rect.x,
                    scaled.rect.y,
                    scaled.rect.width,
                    scaled.rect.height,
                ]
                .into_iter()
                .chain(scaled.corners)
                .map(f32::to_bits),
            );
            scaled_clips.push(scaled);
        }
    }
    let (clip_start, clip_count) = if invalid_clips {
        flags |= GpuPaintEffect::INVALID_CLIPS;
        (0, 0)
    } else if scaled_clips.is_empty() {
        (0, 0)
    } else if let Some(range) = clip_chains.get(&clip_key) {
        *range
    } else {
        let start = table.clips.len() as u32;
        table.clips.extend(scaled_clips.iter().map(gpu_clip));
        let range = (start, scaled_clips.len() as u32);
        clip_chains.insert(clip_key, range);
        range
    };

    let gradient = effects.gradient;
    let gradient_start = gradient.map_or([0.0; 4], |gradient| {
        color_to_f32(gradient.start, output_is_srgb)
    });
    let gradient_end = gradient.map_or([0.0; 4], |gradient| {
        color_to_f32(gradient.end, output_is_srgb)
    });
    let direction = gradient.map_or([0.0; 2], |gradient| gradient.direction);
    let effect = GpuPaintEffect {
        rect: [rect.x, rect.y, rect.width, rect.height],
        corners,
        border_color: color_to_f32(effects.border, output_is_srgb),
        border_widths,
        gradient_start,
        gradient_end,
        gradient_direction_softness: [direction[0], direction[1], softness, 0.0],
        clip_range_flags: [clip_start, clip_count, flags, 0],
    };
    let index = table.paints.len() as u32;
    table.paints.push(effect);
    table.indices.insert(key, index);
}

fn scale_rounded_clip(clip: &UiRoundedClip, scale_x: f32, scale_y: f32) -> UiRoundedClip {
    let rect = UiRect::new(
        clip.rect.x * scale_x,
        clip.rect.y * scale_y,
        clip.rect.width * scale_x,
        clip.rect.height * scale_y,
    );
    UiRoundedClip {
        rect,
        corners: normalized_corners(
            rect,
            clip.corners.map(|radius| radius * scale_x.min(scale_y)),
        ),
    }
}

fn gpu_clip(clip: &UiRoundedClip) -> GpuRoundedClip {
    GpuRoundedClip {
        rect: [clip.rect.x, clip.rect.y, clip.rect.width, clip.rect.height],
        corners: clip.corners,
    }
}

fn solid_vertices(
    draw_list: &UiSurfaceDrawList,
    logical_width: u32,
    logical_height: u32,
    target_width: u32,
    target_height: u32,
    output_is_srgb: bool,
    effect_table: &GpuEffectTable,
) -> VertexBatch<SolidVertex> {
    let scale_x = target_width.max(1) as f32 / logical_width.max(1) as f32;
    let scale_y = target_height.max(1) as f32 / logical_height.max(1) as f32;
    let mut vertices = Vec::with_capacity(draw_list.solids.len() * 6);
    let mut ranges = Vec::with_capacity(draw_list.solids.len());
    for (index, quad) in draw_list.solids.iter().enumerate() {
        let start = vertices.len() as u32;
        let rect = physical_snap_rect(quad.rect, scale_x, scale_y);
        let effect_index = effect_table
            .indices
            .get(&UiPaintKey::Solid(index))
            .copied()
            .unwrap_or(0);
        let softness = effect_table
            .paints
            .get(effect_index as usize)
            .map_or(0.0, |effect| effect.gradient_direction_softness[2]);
        let padding = (softness * 0.5).max(0.0) + 1.0;
        let outer = UiRect::new(
            rect.x - padding,
            rect.y - padding,
            rect.width + padding * 2.0,
            rect.height + padding * 2.0,
        );
        let positions = ndc_quad(
            outer.x,
            outer.y,
            outer.width,
            outer.height,
            target_width,
            target_height,
        );
        let local_positions = [
            [outer.x, outer.y],
            [outer.right(), outer.y],
            [outer.right(), outer.bottom()],
            [outer.x, outer.y],
            [outer.right(), outer.bottom()],
            [outer.x, outer.bottom()],
        ];
        let color = color_to_f32(quad.color, output_is_srgb);
        for (position, local_position) in positions.into_iter().zip(local_positions) {
            vertices.push(SolidVertex {
                position,
                local_position,
                color,
                effect_index,
            });
        }
        ranges.push(start..vertices.len() as u32);
    }
    VertexBatch { vertices, ranges }
}

fn stroke_vertices(
    draw_list: &UiSurfaceDrawList,
    logical_width: u32,
    logical_height: u32,
    target_width: u32,
    target_height: u32,
    output_is_srgb: bool,
    effect_table: &GpuEffectTable,
) -> VertexBatch<SolidVertex> {
    let scale_x = target_width.max(1) as f32 / logical_width.max(1) as f32;
    let scale_y = target_height.max(1) as f32 / logical_height.max(1) as f32;
    let scale = scale_x.min(scale_y);
    let mut vertices = Vec::with_capacity(draw_list.strokes.len() * 66);
    let mut ranges = Vec::with_capacity(draw_list.strokes.len());

    for (index, stroke) in draw_list.strokes.iter().enumerate() {
        let start = vertices.len() as u32;
        let start_point = [stroke.start[0] * scale_x, stroke.start[1] * scale_y];
        let end_point = [stroke.end[0] * scale_x, stroke.end[1] * scale_y];
        append_stroke_vertices(
            &mut vertices,
            start_point,
            end_point,
            stroke.width * scale,
            target_width,
            target_height,
            color_to_f32(stroke.color, output_is_srgb),
            effect_table
                .indices
                .get(&UiPaintKey::Stroke(index))
                .copied()
                .unwrap_or(0),
        );
        ranges.push(start..vertices.len() as u32);
    }

    VertexBatch { vertices, ranges }
}

fn append_stroke_vertices(
    vertices: &mut Vec<SolidVertex>,
    start: [f32; 2],
    end: [f32; 2],
    width: f32,
    target_width: u32,
    target_height: u32,
    color: [f32; 4],
    effect_index: u32,
) {
    let half_width = width.max(1.0) * 0.5;
    let dx = end[0] - start[0];
    let dy = end[1] - start[1];
    let length = (dx * dx + dy * dy).sqrt();
    if length <= f32::EPSILON {
        append_round_cap(
            vertices,
            start,
            half_width,
            target_width,
            target_height,
            color,
            effect_index,
        );
        return;
    }

    let normal = [-dy / length * half_width, dx / length * half_width];
    let a = [start[0] + normal[0], start[1] + normal[1]];
    let b = [end[0] + normal[0], end[1] + normal[1]];
    let c = [end[0] - normal[0], end[1] - normal[1]];
    let d = [start[0] - normal[0], start[1] - normal[1]];
    for point in [a, b, c, a, c, d] {
        vertices.push(SolidVertex {
            position: ndc_point(point[0], point[1], target_width, target_height),
            local_position: point,
            color,
            effect_index,
        });
    }
    append_round_cap(
        vertices,
        start,
        half_width,
        target_width,
        target_height,
        color,
        effect_index,
    );
    append_round_cap(
        vertices,
        end,
        half_width,
        target_width,
        target_height,
        color,
        effect_index,
    );
}

fn append_round_cap(
    vertices: &mut Vec<SolidVertex>,
    center: [f32; 2],
    radius: f32,
    target_width: u32,
    target_height: u32,
    color: [f32; 4],
    effect_index: u32,
) {
    const SEGMENTS: usize = 10;
    let center_position = ndc_point(center[0], center[1], target_width, target_height);
    for segment in 0..SEGMENTS {
        let start_angle = std::f32::consts::TAU * segment as f32 / SEGMENTS as f32;
        let end_angle = std::f32::consts::TAU * (segment + 1) as f32 / SEGMENTS as f32;
        vertices.extend([
            SolidVertex {
                position: center_position,
                local_position: center,
                color,
                effect_index,
            },
            SolidVertex {
                position: ndc_point(
                    center[0] + start_angle.cos() * radius,
                    center[1] + start_angle.sin() * radius,
                    target_width,
                    target_height,
                ),
                local_position: [
                    center[0] + start_angle.cos() * radius,
                    center[1] + start_angle.sin() * radius,
                ],
                color,
                effect_index,
            },
            SolidVertex {
                position: ndc_point(
                    center[0] + end_angle.cos() * radius,
                    center[1] + end_angle.sin() * radius,
                    target_width,
                    target_height,
                ),
                local_position: [
                    center[0] + end_angle.cos() * radius,
                    center[1] + end_angle.sin() * radius,
                ],
                color,
                effect_index,
            },
        ]);
    }
}

fn text_vertices(
    draw_list: &UiSurfaceDrawList,
    logical_width: u32,
    logical_height: u32,
    target_width: u32,
    target_height: u32,
    output_is_srgb: bool,
    effect_table: &GpuEffectTable,
) -> VertexBatch<TextVertex> {
    let scale_x = target_width.max(1) as f32 / logical_width.max(1) as f32;
    let scale_y = target_height.max(1) as f32 / logical_height.max(1) as f32;
    let atlas_width = f32::from(draw_list.atlas_size[0].max(1));
    let atlas_height = f32::from(draw_list.atlas_size[1].max(1));
    let mut vertices = Vec::with_capacity(draw_list.text.len() * 6);
    let mut ranges = Vec::with_capacity(draw_list.text.len());
    for (index, quad) in draw_list.text.iter().enumerate() {
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
        let local_positions = [
            [rect.x, rect.y],
            [rect.right(), rect.y],
            [rect.right(), rect.bottom()],
            [rect.x, rect.y],
            [rect.right(), rect.bottom()],
            [rect.x, rect.bottom()],
        ];
        let effect_index = effect_table
            .indices
            .get(&UiPaintKey::Text(index))
            .copied()
            .unwrap_or(0);
        for ((position, uv), local_position) in positions.into_iter().zip(uvs).zip(local_positions)
        {
            vertices.push(TextVertex {
                position,
                uv,
                local_position,
                color,
                effect_index,
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
    effects: &HashMap<UiPaintKey, UiPaintEffects>,
    effect_table: &GpuEffectTable,
    icon_entries: &HashMap<String, UiIconAtlasEntry>,
    image_textures: &HashMap<String, UiImageGpuTexture>,
    external_images: &HashMap<String, UiExternalImage>,
) -> VertexBatch<TextVertex> {
    let scale_x = target_width.max(1) as f32 / logical_width.max(1) as f32;
    let scale_y = target_height.max(1) as f32 / logical_height.max(1) as f32;
    let mut vertices = Vec::with_capacity(draw_list.images.len() * 6);
    let mut ranges = Vec::with_capacity(draw_list.images.len());
    for (index, quad) in draw_list.images.iter().enumerate() {
        let start = vertices.len() as u32;
        let effect_index = effect_table
            .indices
            .get(&UiPaintKey::Image(index))
            .copied()
            .unwrap_or(0);
        let options = effects
            .get(&UiPaintKey::Image(index))
            .and_then(|effect| effect.image.as_ref())
            .cloned()
            .unwrap_or_default();
        let external_image = external_images.get(&quad.source_key);
        let icon_entry = external_image
            .is_none()
            .then(|| icon_entries.get(&quad.source_key))
            .flatten();
        let source_size = external_image
            .map(|image| image.size)
            .or_else(|| icon_entry.map(|_| [64, 64]))
            .or_else(|| image_textures.get(&quad.source_key).map(|image| image.size));
        let color = color_to_f32(quad.tint, output_is_srgb);
        if let Some(source_size) = source_size {
            for patch in image_patches(quad.rect, source_size, quad.fit, &options) {
                append_image_patch(
                    &mut vertices,
                    patch,
                    icon_entry,
                    scale_x,
                    scale_y,
                    target_width,
                    target_height,
                    color,
                    effect_index,
                );
            }
        } else if let Some(mode) = missing_placeholder_mode(
            options.placeholder,
            images.resource_status(&quad.source_key),
        ) {
            let rect = physical_snap_rect(quad.rect, scale_x, scale_y);
            append_image_patch(
                &mut vertices,
                ImagePatch {
                    destination: rect,
                    uv: UiRect::new(mode, 0.0, 0.0, 0.0),
                },
                None,
                1.0,
                1.0,
                target_width,
                target_height,
                color,
                effect_index,
            );
        }
        ranges.push(start..vertices.len() as u32);
    }
    VertexBatch { vertices, ranges }
}

fn append_image_patch(
    vertices: &mut Vec<TextVertex>,
    patch: ImagePatch,
    atlas_entry: Option<&UiIconAtlasEntry>,
    scale_x: f32,
    scale_y: f32,
    target_width: u32,
    target_height: u32,
    color: [f32; 4],
    effect_index: u32,
) {
    let rect = physical_snap_rect(patch.destination, scale_x, scale_y);
    let positions = ndc_quad(
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        target_width,
        target_height,
    );
    let local_positions = [
        [rect.x, rect.y],
        [rect.right(), rect.y],
        [rect.right(), rect.bottom()],
        [rect.x, rect.y],
        [rect.right(), rect.bottom()],
        [rect.x, rect.bottom()],
    ];
    let uv_rect = atlas_entry.map_or(patch.uv, |entry| {
        UiRect::new(
            entry.uv.x + patch.uv.x * entry.uv.width,
            entry.uv.y + patch.uv.y * entry.uv.height,
            patch.uv.width * entry.uv.width,
            patch.uv.height * entry.uv.height,
        )
    });
    let uvs = [
        [uv_rect.x, uv_rect.y],
        [uv_rect.right(), uv_rect.y],
        [uv_rect.right(), uv_rect.bottom()],
        [uv_rect.x, uv_rect.y],
        [uv_rect.right(), uv_rect.bottom()],
        [uv_rect.x, uv_rect.bottom()],
    ];
    for ((position, uv), local_position) in positions.into_iter().zip(uvs).zip(local_positions) {
        vertices.push(TextVertex {
            position,
            uv,
            local_position,
            color,
            effect_index,
        });
    }
}

fn missing_placeholder_mode(
    placeholder: UiImagePlaceholder,
    status: UiImageResourceStatus,
) -> Option<f32> {
    match placeholder {
        UiImagePlaceholder::Hidden => None,
        UiImagePlaceholder::Skeleton => Some(0.0),
        UiImagePlaceholder::Checkerboard => Some(1.0),
        UiImagePlaceholder::Automatic => Some(if status == UiImageResourceStatus::Pending {
            0.0
        } else {
            2.0
        }),
    }
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
    fn ui_surface_shader_parses_and_validates_without_a_gpu() {
        let module = wgpu::naga::front::wgsl::parse_str(UI_SURFACE_WGSL)
            .expect("UI surface WGSL should parse");
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("UI surface WGSL should validate");
    }

    #[test]
    fn translucent_coverage_metrics_scale_to_the_physical_target() {
        let list = UiSurfaceDrawList {
            solids: vec![super::super::UiSurfaceQuad {
                rect: super::super::UiRect::new(5.0, 5.0, 10.0, 8.0),
                clip_rect: super::super::UiRect::new(0.0, 0.0, 20.0, 20.0),
                color: [25, 31, 38, 220],
                z_index: 0,
                radius: 0.0,
            }],
            ..UiSurfaceDrawList::default()
        };

        assert_eq!(
            estimate_translucent_solids(&list, [20, 20], [40, 40]),
            (1, 320)
        );
    }

    #[test]
    fn rounded_solid_uses_one_quad_and_carries_analytic_corner_data() {
        let list = UiSurfaceDrawList {
            solids: vec![super::super::UiSurfaceQuad {
                rect: UiRect::new(0.0, 0.0, 120.0, 40.0),
                clip_rect: UiRect::new(0.0, 0.0, 120.0, 40.0),
                color: [255, 255, 255, 255],
                z_index: 0,
                radius: 12.0,
            }],
            ..UiSurfaceDrawList::default()
        };
        let table = build_gpu_effect_table(&list, 120, 40, 120, 40, true);
        let batch = solid_vertices(&list, 120, 40, 120, 40, true, &table);

        assert_eq!(batch.ranges[0], 0..6);
        assert_eq!(table.paints[0].corners, [12.0; 4]);
    }

    #[test]
    fn stroke_batch_uses_connected_caps_and_one_range_per_stroke() {
        let list = UiSurfaceDrawList {
            strokes: vec![super::super::UiSurfaceStroke {
                start: [4.0, 5.0],
                end: [20.0, 17.0],
                clip_rect: super::super::UiRect::new(0.0, 0.0, 40.0, 30.0),
                color: [237, 239, 242, 255],
                width: 3.0,
                z_index: 0,
            }],
            ..UiSurfaceDrawList::default()
        };

        let table = build_gpu_effect_table(&list, 40, 30, 80, 60, true);
        let batch = stroke_vertices(&list, 40, 30, 80, 60, true, &table);

        assert_eq!(batch.ranges.len(), 1);
        assert_eq!(batch.ranges[0].start, 0);
        assert_eq!(batch.ranges[0].end - batch.ranges[0].start, 66);
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
        let table = build_gpu_effect_table(&list, 160, 80, 160, 80, true);
        let batch = solid_vertices(&list, 160, 80, 160, 80, true, &table);

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

        let table = build_gpu_effect_table(&list, 160, 80, 320, 160, true);
        let batch = solid_vertices(&list, 160, 80, 320, 160, true, &table);

        // One physical pixel of raster padding contains the analytic AA edge;
        // the effect's shape remains on the original snapped rectangle.
        assert_eq!(batch.vertices[0].position, ndc_point(79.0, 39.0, 320, 160));
        assert_eq!(batch.vertices[1].position, ndc_point(161.0, 39.0, 320, 160));
        assert_eq!(table.paints[0].rect, [80.0, 40.0, 80.0, 40.0]);
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
    fn rounded_clip_chains_are_deduplicated_and_bounded() {
        let clip = UiRoundedClip {
            rect: UiRect::new(2.0, 3.0, 20.0, 12.0),
            corners: [5.0; 4],
        };
        let mut list = UiSurfaceDrawList {
            solids: vec![
                super::super::UiSurfaceQuad {
                    rect: UiRect::new(0.0, 0.0, 20.0, 12.0),
                    clip_rect: UiRect::new(0.0, 0.0, 30.0, 20.0),
                    color: [20, 30, 40, 255],
                    z_index: 0,
                    radius: 0.0,
                },
                super::super::UiSurfaceQuad {
                    rect: UiRect::new(0.0, 0.0, 20.0, 12.0),
                    clip_rect: UiRect::new(0.0, 0.0, 30.0, 20.0),
                    color: [20, 30, 40, 255],
                    z_index: 1,
                    radius: 0.0,
                },
            ],
            ..UiSurfaceDrawList::default()
        };
        for index in 0..2 {
            list.effects.insert(
                UiPaintKey::Solid(index),
                UiPaintEffects {
                    clips: vec![clip.clone()],
                    ..UiPaintEffects::default()
                },
            );
        }
        let table = build_gpu_effect_table(&list, 30, 20, 30, 20, true);

        assert_eq!(table.clips.len(), 1);
        assert_eq!(
            table.paints[table.indices[&UiPaintKey::Solid(0)] as usize].clip_range_flags[0..2],
            [0, 1]
        );
        assert_eq!(
            table.paints[table.indices[&UiPaintKey::Solid(1)] as usize].clip_range_flags[0..2],
            [0, 1]
        );

        list.effects.get_mut(&UiPaintKey::Solid(0)).unwrap().clips =
            vec![clip; MAX_ROUNDED_CLIPS + 1];
        let table = build_gpu_effect_table(&list, 30, 20, 30, 20, true);
        let flags = table.paints[table.indices[&UiPaintKey::Solid(0)] as usize].clip_range_flags[2];
        assert_ne!(flags & GpuPaintEffect::INVALID_CLIPS, 0);
    }

    #[test]
    fn automatic_missing_placeholders_distinguish_pending_from_error() {
        assert_eq!(
            missing_placeholder_mode(
                UiImagePlaceholder::Automatic,
                UiImageResourceStatus::Pending,
            ),
            Some(0.0)
        );
        assert_eq!(
            missing_placeholder_mode(
                UiImagePlaceholder::Automatic,
                UiImageResourceStatus::Missing,
            ),
            Some(2.0)
        );
        assert_eq!(
            missing_placeholder_mode(UiImagePlaceholder::Automatic, UiImageResourceStatus::Error,),
            Some(2.0)
        );
        assert_eq!(
            missing_placeholder_mode(UiImagePlaceholder::Hidden, UiImageResourceStatus::Missing,),
            None
        );
    }

    #[test]
    fn automatic_image_sampling_preserves_the_linear_gpu_default() {
        assert_eq!(
            resolved_sampling(UiImageSampling::Auto),
            UiImageSampling::Linear
        );
        assert_eq!(
            resolved_sampling(UiImageSampling::Nearest),
            UiImageSampling::Nearest
        );
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
        let image_store = UiSurfaceImageStore::default();
        let effect_table = build_gpu_effect_table(&list, 160, 80, 160, 80, true);
        let geometry = UiSurfaceGpuGeometryCache {
            key: UiSurfaceGpuGeometryKey {
                draw_list_identity: std::ptr::from_ref(&list) as usize,
                draw_list_revision: 0,
                logical_size: [160, 80],
                target_size: [160, 80],
                atlas_revision: 0,
                atlas_size: [0, 0],
                image_signature: 0,
                icon_atlas_revision: 0,
            },
            solid_batch: solid_vertices(&list, 160, 80, 160, 80, true, &effect_table),
            stroke_batch: stroke_vertices(&list, 160, 80, 160, 80, true, &effect_table),
            text_batch: text_vertices(&list, 160, 80, 160, 80, true, &effect_table),
            image_batch: image_vertices(
                &list,
                160,
                80,
                160,
                80,
                &image_store,
                true,
                &list.effects,
                &effect_table,
                &HashMap::new(),
                &HashMap::new(),
                &HashMap::new(),
            ),
            effect_table,
            paint_runs: Vec::new(),
        };

        let runs = build_paint_runs(
            &list,
            &geometry,
            [160, 80],
            [160, 80],
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
        );

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
        assert_eq!(mip, vec![188, 0, 0, 128]);
    }

    #[test]
    fn image_budget_counts_the_complete_mip_chain() {
        assert_eq!(image_mip_chain_bytes([1, 1]), 4);
        assert_eq!(image_mip_chain_bytes([2, 2]), 20);
        assert_eq!(image_mip_chain_bytes([4, 2]), 44);
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
