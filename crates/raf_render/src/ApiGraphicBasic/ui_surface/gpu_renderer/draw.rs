//! Frame submission, paint-run recording, scissors, and solid geometry for the
//! retained UI compositor.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use raf_ui::UiImageSampling;

use crate::api_graphic_basic::canvas_presenter::CanvasTargetRect;

use super::super::effects::UiPaintKey;
use super::super::{UiRect, UiSurfaceDrawList, UiSurfaceImageStore, UiSurfacePaintCommand, UiTextAtlas};
use super::buffers::{
    build_gpu_effect_table, GpuEffectTable, UiSurfaceGpuGeometryCache, UiSurfaceGpuGeometryKey,
    VertexBatch,
};
use super::diagnostics::estimate_translucent_solids;
use super::images::{
    image_resources_signature, image_vertices, resolved_sampling, UiExternalImage, UiIconAtlasEntry,
    UiImageGpuTexture,
};
use super::pipelines::{external_image_bind_group, icon_bind_group, image_bind_group};
use super::text::text_vertices;
use super::{
    color_from_bytes, color_to_f32, ndc_point, ndc_quad, physical_snap_rect, SolidVertex,
    UiSurfaceGpuMetrics, UiSurfaceGpuRenderer,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum UiSurfacePaintKind {
    Solid,
    Stroke,
    Text,
    Image(String, UiImageSampling),
    IconAtlas(usize, UiImageSampling),
    ExternalImage(String, UiImageSampling),
    MissingImage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UiSurfacePaintRun {
    pub(super) kind: UiSurfacePaintKind,
    pub(super) range: Range<u32>,
    pub(super) scissor: (u32, u32, u32, u32),
}

impl UiSurfaceGpuRenderer {
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
}

pub(super) fn build_paint_runs(
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

pub(super) fn solid_vertices(
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

pub(super) fn stroke_vertices(
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

pub(super) fn ui_scissor_rect(
    rect: Option<UiRect>,
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
