//! Offline adapters from retained UI and CAD data into ApiGraphicBasic data.
//!
//! `append_ui` produces the same packet-side paint descriptors the product host
//! emits. `append_ui_fixture` triangulates that retained UI for the serializable
//! `Workload` used by the isolated probe; it is benchmark input, not a second
//! product renderer.

mod paint;

use std::collections::HashMap;

use glam::{Mat4, Vec2, Vec3};
use raf_agb::product::ProductPacket;
use raf_agb::{Texture, Workload};
use raf_ui::{UiImageOptions, UiRect};

use crate::api_graphic_basic::capabilities::GraphicsMemoryBudget;
use crate::{
    api_graphic_basic::command_list::{BasicLine, GraphicCommand},
    scene_renderer::SceneRenderFrame,
};

use super::product_paint::{append_paint, UiPaintMetrics, UiTextureCache};
use super::{
    image_layout::image_patches, UiPaintKey, UiSurfaceDrawList, UiSurfaceImageStore,
    UiSurfacePaintCommand, UiTextAtlas,
};

use paint::Painter;

/// Appends retained UI as ApiGraphicBasic packet descriptors.
///
/// This is the offline counterpart of
/// [`super::DirectUiSurfaceHost::append_product_paint`]; both share the same
/// conversion, so the probe exercises the product command stream.
#[allow(clippy::too_many_arguments)]
pub fn append_ui(
    packet: &mut ProductPacket,
    list: &UiSurfaceDrawList,
    atlas: &UiTextAtlas,
    images: &mut UiSurfaceImageStore,
    surface: &str,
    target_size: [u32; 2],
    target_rect: [f32; 4],
    logical_size: [u32; 2],
    raster_scale: f32,
) -> Result<UiPaintMetrics, String> {
    for quad in &list.images {
        images.ensure_builtin_key(&quad.source_key);
    }
    let mut cache = UiTextureCache::with_budget(GraphicsMemoryBudget::potato());
    append_paint(
        packet,
        list,
        atlas,
        images,
        &mut cache,
        surface,
        target_size,
        target_rect,
        logical_size,
        raster_scale,
    )
}

/// Triangulates retained UI into the serializable probe `Workload`.
#[allow(clippy::too_many_arguments)]
pub fn append_ui_fixture(
    workload: &mut Workload,
    list: &UiSurfaceDrawList,
    atlas: &UiTextAtlas,
    images: &mut UiSurfaceImageStore,
    viewport: UiRect,
    output: [u32; 2],
) {
    let mut painter = Painter::new(workload, viewport, output);
    let [aw, ah] = list.atlas_size;
    let active_height = list
        .text
        .iter()
        .map(|text| text.atlas_rect.bottom().ceil() as u16)
        .max()
        .unwrap_or(0)
        .min(ah);
    let atlas_index = if active_height > 0 {
        let mut rgba = Vec::with_capacity(usize::from(aw) * usize::from(active_height) * 4);
        for alpha in atlas
            .pixels()
            .iter()
            .take(usize::from(aw) * usize::from(active_height))
        {
            rgba.extend_from_slice(&[255, 255, 255, *alpha]);
        }
        painter.texture(Texture {
            size: [aw.into(), active_height.into()],
            rgba,
            mipmaps: false,
        })
    } else {
        0
    };
    let mut texture_indices = HashMap::new();
    let unit = UiRect::new(0., 0., 1., 1.);
    for command in list.paint_commands().iter() {
        match *command {
            UiSurfacePaintCommand::Solid { index, .. } => {
                let quad = &list.solids[index];
                let effects = list.effects.get(&UiPaintKey::Solid(index));
                let corners = effects
                    .and_then(|effect| effect.corners)
                    .unwrap_or([quad.radius; 4]);
                let tint = paint::rgba(quad.color);
                if let Some(effects) = effects {
                    if effects.border[3] > 0
                        && effects.border_widths.into_iter().any(|width| width > 0.)
                    {
                        let [top, right, bottom, left] =
                            effects.border_widths.map(|width| width.max(0.));
                        let shape = UiRect::new(
                            quad.rect.x + left,
                            quad.rect.y + top,
                            quad.rect.width - left - right,
                            quad.rect.height - top - bottom,
                        );
                        let inset = [
                            top.max(left),
                            top.max(right),
                            bottom.max(right),
                            bottom.max(left),
                        ];
                        let inner_corners =
                            std::array::from_fn(|i| (corners[i] - inset[i]).max(0.));
                        let border = paint::rgba(effects.border);
                        if shape.is_empty() {
                            painter.rect(
                                quad.rect,
                                quad.clip_rect,
                                unit,
                                corners,
                                border,
                                None,
                                &effects.clips,
                                0,
                            );
                            continue;
                        }
                        painter.ring(
                            quad.rect,
                            corners,
                            shape,
                            inner_corners,
                            [border; 2],
                            quad.clip_rect,
                            &effects.clips,
                        );
                        painter.rect(
                            shape,
                            quad.clip_rect,
                            unit,
                            inner_corners,
                            tint,
                            effects.gradient,
                            &effects.clips,
                            0,
                        );
                        continue;
                    }
                    if effects.softness > 0. && effects.gradient.is_none() {
                        let half = (effects.softness * 0.5)
                            .min(quad.rect.width.min(quad.rect.height) * 0.49);
                        let inner = expand(quad.rect, -half);
                        let outer = expand(quad.rect, half);
                        let inner_corners = corners.map(|radius| (radius - half).max(0.));
                        let mut fade = tint;
                        fade[3] = 0.;
                        painter.ring(
                            outer,
                            corners.map(|radius| radius + half),
                            inner,
                            inner_corners,
                            [fade, tint],
                            quad.clip_rect,
                            &effects.clips,
                        );
                        painter.rect(
                            inner,
                            quad.clip_rect,
                            unit,
                            inner_corners,
                            tint,
                            None,
                            &effects.clips,
                            0,
                        );
                        continue;
                    }
                }
                painter.rect(
                    quad.rect,
                    quad.clip_rect,
                    unit,
                    corners,
                    tint,
                    effects.and_then(|effect| effect.gradient),
                    effects.map_or(&[][..], |effect| effect.clips.as_slice()),
                    0,
                );
            }
            UiSurfacePaintCommand::Stroke { index, .. } => {
                let stroke = &list.strokes[index];
                let clips = list
                    .effects
                    .get(&UiPaintKey::Stroke(index))
                    .map_or(&[][..], |effects| effects.clips.as_slice());
                painter.stroke(
                    Vec2::from_array(stroke.start),
                    Vec2::from_array(stroke.end),
                    stroke.width,
                    stroke.clip_rect,
                    clips,
                    stroke.color,
                );
            }
            UiSurfacePaintCommand::Text { index, .. } => {
                let quad = &list.text[index];
                let uv = quad.atlas_rect;
                let uv = UiRect::new(
                    uv.x / f32::from(aw),
                    uv.y / f32::from(active_height),
                    uv.width / f32::from(aw),
                    uv.height / f32::from(active_height),
                );
                let clips = list
                    .effects
                    .get(&UiPaintKey::Text(index))
                    .map_or(&[][..], |effects| effects.clips.as_slice());
                painter.rect(
                    quad.rect,
                    quad.clip_rect,
                    uv,
                    [0.; 4],
                    paint::rgba(quad.color),
                    None,
                    clips,
                    atlas_index,
                );
            }
            UiSurfacePaintCommand::Image { index, .. } => {
                let quad = &list.images[index];
                images.ensure_builtin_key(&quad.source_key);
                let Some(source) = images.get(&quad.source_key) else {
                    continue;
                };
                let texture = *texture_indices
                    .entry(quad.source_key.as_str())
                    .or_insert_with(|| {
                        painter.texture(Texture {
                            size: source.size,
                            rgba: source.pixels.clone(),
                            mipmaps: true,
                        })
                    });
                let effect = list.effects.get(&UiPaintKey::Image(index));
                let fallback = UiImageOptions::default();
                let options = effect
                    .and_then(|effect| effect.image.as_ref())
                    .unwrap_or(&fallback);
                for patch in image_patches(quad.rect, source.size, quad.fit, options) {
                    let mut clips = effect.map_or_else(Vec::new, |effect| effect.clips.clone());
                    if let Some(corners) = effect.and_then(|effect| effect.corners) {
                        clips.push(super::UiRoundedClip {
                            rect: quad.rect,
                            corners,
                        });
                    }
                    painter.rect(
                        patch.destination,
                        quad.clip_rect,
                        patch.uv,
                        [0.; 4],
                        paint::rgba(quad.tint),
                        None,
                        &clips,
                        texture,
                    );
                }
            }
        }
    }
}

fn expand(rect: UiRect, amount: f32) -> UiRect {
    UiRect::new(
        rect.x - amount,
        rect.y - amount,
        rect.width + amount * 2.,
        rect.height + amount * 2.,
    )
}

pub fn append_cad(
    workload: &mut Workload,
    frame: &SceneRenderFrame,
    viewport: UiRect,
    output: [u32; 2],
) {
    let mut painter = Painter::new(workload, viewport, output);
    let clip = UiRect::new(0., 0., frame.width as f32, frame.height as f32);
    let unit = UiRect::new(0., 0., 1., 1.);
    let project = |point: Vec3| {
        let homogeneous = frame.view_proj * point.extend(1.);
        let xy = homogeneous.truncate() / homogeneous.w;
        Vec2::new(
            (xy.x * 0.5 + 0.5) * clip.width,
            (-xy.y * 0.5 + 0.5) * clip.height,
        )
    };
    let line = |painter: &mut Painter<'_>, line: &BasicLine| {
        use crate::api_graphic_basic::line_projection::prepare_line;
        if let Some(line) = prepare_line(line, &frame.view_proj, [clip.width, clip.height]) {
            painter.stroke(
                project(line.start),
                project(line.end),
                line.width,
                clip,
                &[],
                line.color,
            );
        }
    };
    let mesh = |painter: &mut Painter<'_>, id: usize, transform: Mat4, color: [u8; 4]| {
        let Some(mesh) = frame.commands.mesh(id) else {
            return;
        };
        for triangle in mesh.indices.chunks_exact(3) {
            let positions = [triangle[0], triangle[1], triangle[2]].map(|index| {
                project(transform.transform_point3(mesh.vertices[index as usize].position))
            });
            painter.solid_polygon(&positions, color, clip);
        }
    };
    for command in frame.commands.commands() {
        match command {
            GraphicCommand::Clear { r, g, b, a } => painter.rect(
                clip,
                clip,
                unit,
                [0.; 4],
                paint::rgba([*r, *g, *b, *a]),
                None,
                &[],
                0,
            ),
            GraphicCommand::DrawMesh {
                mesh_id,
                transform,
                color,
                ..
            } => mesh(&mut painter, *mesh_id, *transform, *color),
            GraphicCommand::DrawMeshBatch {
                mesh_id, instances, ..
            } => {
                for instance in instances {
                    mesh(&mut painter, *mesh_id, instance.transform, instance.color);
                }
            }
            GraphicCommand::DrawLine {
                start,
                end,
                color,
                width,
                depth_bias,
                ..
            } => line(
                &mut painter,
                &BasicLine {
                    start: *start,
                    end: *end,
                    color: *color,
                    width: *width,
                    depth_bias: *depth_bias,
                },
            ),
            GraphicCommand::DrawLineBatch { lines, .. } => {
                for item in lines {
                    line(&mut painter, item);
                }
            }
            GraphicCommand::DrawScreenTriangleBatch { triangles } => {
                for item in triangles {
                    painter.solid_polygon(&item.points.map(Vec2::from_array), item.color, clip);
                }
            }
            GraphicCommand::SetPipeline(_)
            | GraphicCommand::SetShadowReceiver(_)
            | GraphicCommand::DrawGrid { .. } => {}
        }
    }
}
