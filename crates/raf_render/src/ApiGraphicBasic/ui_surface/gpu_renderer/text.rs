//! Text atlas upload and glyph geometry for the retained UI compositor.

use super::super::effects::UiPaintKey;
use super::super::{UiSurfaceDrawList, UiTextAtlas, UiTextAtlasRect};
use super::buffers::{padded_rgba_rows, GpuEffectTable, VertexBatch};
use super::{color_to_f32, ndc_quad, physical_snap_rect, TextVertex, UiSurfaceGpuRenderer};

impl UiSurfaceGpuRenderer {
    pub(super) fn sync_atlas(
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
}

pub(super) fn text_vertices(
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
