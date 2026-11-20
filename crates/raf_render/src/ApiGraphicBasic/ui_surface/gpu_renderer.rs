//! Direct WGPU compositor for retained UI surfaces.
//!
//! It renders into any caller-owned texture view. Window ownership stays
//! outside this type, which keeps it usable by a native Winit host, tests, or
//! an off-screen editor surface without coupling it to a window framework.
//!
//! Implementation modules: `pipelines` owns shader and pipeline objects,
//! `buffers` owns retained GPU buffers and paint effects, `text` owns atlas
//! upload and glyph geometry, `images` owns image residency and image
//! geometry, `draw` owns frame submission, and `diagnostics` owns metrics.

mod buffers;
mod diagnostics;
mod draw;
mod images;
mod pipelines;
mod text;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};

use crate::api_graphic_basic::capabilities::GraphicsMemoryBudget;

pub use diagnostics::UiSurfaceGpuMetrics;
pub(crate) use pipelines::UiSurfaceGpuSharedResources;

use buffers::UiSurfaceGpuGeometryCache;
use images::{UiBuiltinIconAtlas, UiExternalImage, UiImageGpuTexture};

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

impl TextVertex {
    fn layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        const ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
            0 => Float32x2,
            1 => Float32x2,
            2 => Float32x2,
            3 => Float32x4,
            4 => Uint32
        ];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRIBUTES,
        }
    }
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
