//! Retained GPU buffers, paint effects, and upload helpers for the UI
//! compositor.

use std::collections::HashMap;
use std::ops::Range;

use bytemuck::{Pod, Zeroable};

use super::super::effects::{
    normalized_corners, UiPaintEffects, UiPaintKey, UiRoundedClip, MAX_ROUNDED_CLIPS,
};
use super::super::{UiRect, UiSurfaceDrawList, UiSurfaceGpuMetrics};
use super::draw::UiSurfacePaintRun;
use super::{
    color_to_f32, physical_snap_rect, SolidVertex, TextVertex, UiSurfaceGpuRenderer,
};

pub(super) struct VertexBatch<T> {
    pub(super) vertices: Vec<T>,
    pub(super) ranges: Vec<Range<u32>>,
}

pub(super) struct UiSurfaceGpuGeometryCache {
    pub(super) key: UiSurfaceGpuGeometryKey,
    pub(super) effect_table: GpuEffectTable,
    pub(super) solid_batch: VertexBatch<SolidVertex>,
    pub(super) stroke_batch: VertexBatch<SolidVertex>,
    pub(super) text_batch: VertexBatch<TextVertex>,
    pub(super) image_batch: VertexBatch<TextVertex>,
    pub(super) paint_runs: Vec<UiSurfacePaintRun>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct UiSurfaceGpuGeometryKey {
    pub(super) draw_list_identity: usize,
    pub(super) draw_list_revision: u64,
    pub(super) logical_size: [u32; 2],
    pub(super) target_size: [u32; 2],
    pub(super) atlas_revision: u64,
    pub(super) atlas_size: [u16; 2],
    pub(super) image_signature: u64,
    pub(super) icon_atlas_revision: u64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(super) struct GpuPaintEffect {
    pub(super) rect: [f32; 4],
    pub(super) corners: [f32; 4],
    pub(super) border_color: [f32; 4],
    pub(super) border_widths: [f32; 4],
    pub(super) gradient_start: [f32; 4],
    pub(super) gradient_end: [f32; 4],
    pub(super) gradient_direction_softness: [f32; 4],
    pub(super) clip_range_flags: [u32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(super) struct GpuRoundedClip {
    rect: [f32; 4],
    corners: [f32; 4],
}

#[derive(Default)]
pub(super) struct GpuEffectTable {
    pub(super) paints: Vec<GpuPaintEffect>,
    pub(super) clips: Vec<GpuRoundedClip>,
    pub(super) indices: HashMap<UiPaintKey, u32>,
}

impl GpuPaintEffect {
    pub(super) const HAS_SHAPE: u32 = 1;
    pub(super) const HAS_BORDER: u32 = 2;
    pub(super) const HAS_GRADIENT: u32 = 4;
    pub(super) const SOFT_FALLOFF: u32 = 8;
    pub(super) const INVALID_CLIPS: u32 = 16;
}

impl Default for GpuPaintEffect {
    fn default() -> Self {
        Self::zeroed()
    }
}

impl UiSurfaceGpuRenderer {
    pub(super) fn ensure_solid_buffer(&mut self, device: &wgpu::Device, vertex_count: usize) {
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

    pub(super) fn ensure_stroke_buffer(&mut self, device: &wgpu::Device, vertex_count: usize) {
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

    pub(super) fn ensure_text_buffer(&mut self, device: &wgpu::Device, vertex_count: usize) {
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

    pub(super) fn ensure_image_buffer(&mut self, device: &wgpu::Device, vertex_count: usize) {
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

    pub(super) fn ensure_effect_buffers(
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

pub(super) fn padded_rgba_rows(pixels: &[u8], size: [u32; 2]) -> (Vec<u8>, u32) {
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

pub(super) fn build_gpu_effect_table(
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
