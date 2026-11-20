//! Image residency, upload, mip handling, samplers, and image geometry for the
//! retained UI compositor.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use raf_ui::{UiImagePlaceholder, UiImageResourceStatus, UiImageSampling};

use crate::api_graphic_basic::device::GpuTextureView;

use super::super::effects::{UiPaintEffects, UiPaintKey};
use super::super::image_layout::{image_patches, ImagePatch};
use super::super::{UiRect, UiSurfaceDrawList, UiSurfaceImageStore};
use super::buffers::{padded_rgba_rows, GpuEffectTable, VertexBatch};
use super::pipelines::{create_icon_atlas_bind_groups, create_image_bind_groups};
use super::{
    color_to_f32, ndc_quad, physical_snap_rect, TextVertex, UiSurfaceGpuMetrics,
    UiSurfaceGpuRenderer,
};

pub(super) struct UiImageGpuTexture {
    _texture: wgpu::Texture,
    pub(super) linear_bind_group: wgpu::BindGroup,
    pub(super) nearest_bind_group: wgpu::BindGroup,
    pub(super) revision: u64,
    pub(super) size: [u32; 2],
    pub(super) bytes: u64,
    pub(super) last_used_frame: u64,
}

pub(super) struct UiExternalImage {
    _view: GpuTextureView,
    pub(super) linear_bind_group: wgpu::BindGroup,
    pub(super) nearest_bind_group: wgpu::BindGroup,
    pub(super) size: [u32; 2],
    pub(super) revision: u64,
    pub(super) bytes: u64,
}

pub(super) struct UiIconAtlasPage {
    _texture: wgpu::Texture,
    pub(super) linear_bind_group: wgpu::BindGroup,
    pub(super) nearest_bind_group: wgpu::BindGroup,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct UiIconAtlasEntry {
    pub(super) page: usize,
    pub(super) uv: UiRect,
}

#[derive(Default)]
pub(super) struct UiBuiltinIconAtlas {
    pub(super) signature: Option<u64>,
    pub(super) revision: u64,
    pub(super) pages: Vec<UiIconAtlasPage>,
    pub(super) entries: HashMap<String, UiIconAtlasEntry>,
    pub(super) resident_bytes: u64,
}

impl UiSurfaceGpuRenderer {
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
}

impl UiSurfaceGpuRenderer {
    pub(super) fn sync_image(
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

    pub(super) fn sync_icon_atlas(
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

    pub(super) fn reserve_image_budget(
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
}

pub(super) fn image_mip_level_count(size: [u32; 2]) -> u32 {
    let mut largest = size[0].max(size[1]).max(1);
    let mut levels = 1;
    while largest > 1 {
        largest = (largest / 2).max(1);
        levels += 1;
    }
    levels
}

pub(super) fn image_mip_chain_bytes(size: [u32; 2]) -> u64 {
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

pub(super) fn image_resources_signature(
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

pub(super) fn resolved_sampling(sampling: UiImageSampling) -> UiImageSampling {
    match sampling {
        UiImageSampling::Auto => UiImageSampling::Linear,
        explicit => explicit,
    }
}

pub(super) fn downsample_rgba_premultiplied(
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

pub(super) fn image_vertices(
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

pub(super) fn missing_placeholder_mode(
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
