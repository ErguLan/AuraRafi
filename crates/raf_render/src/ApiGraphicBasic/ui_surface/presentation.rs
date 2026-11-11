//! Renderer-ready retained UI draw data.
//!
//! This module has no window-framework dependency. Native WGPU hosts consume
//! the same draw list as off-screen and test presenters.

use std::borrow::Cow;
use std::collections::HashMap;

use super::effects::{UiPaintEffects, UiPaintKey, UiRoundedClip, UiSurfaceEffects};
use super::text_atlas::visual_line_ranges;
use super::{
    images::builtin_icon_key, UiControl, UiImageFit, UiRect, UiSkeletonShape, UiStyle,
    UiSurfaceFrame, UiTextAtlas, UiTextAtlasRequest, UiTextRole, UiTogglePresentation,
};

#[derive(Debug, Clone, PartialEq)]
pub struct UiSurfaceQuad {
    pub rect: UiRect,
    pub clip_rect: UiRect,
    pub color: [u8; 4],
    pub z_index: i16,
    pub radius: f32,
}

/// A native, resolution-independent solid stroke in the retained draw list.
///
/// Small marks such as checkbox ticks should not be assembled from a handful
/// of axis-aligned pixels. Keeping the stroke in the ApiGraphicBasic draw list
/// lets both the WGPU presenter and the CPU recovery path rasterize the same
/// geometry at the output density.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiSurfaceStroke {
    pub start: [f32; 2],
    pub end: [f32; 2],
    pub clip_rect: UiRect,
    pub color: [u8; 4],
    pub width: f32,
    pub z_index: i16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiSurfaceTextQuad {
    pub rect: UiRect,
    pub clip_rect: UiRect,
    pub atlas_rect: UiRect,
    pub color: [u8; 4],
    pub z_index: i16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiSurfaceImageQuad {
    pub rect: UiRect,
    pub clip_rect: UiRect,
    pub source_key: String,
    pub fit: UiImageFit,
    pub tint: [u8; 4],
    pub z_index: i16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiSurfacePaintCommand {
    Solid {
        index: usize,
        z_index: i16,
        sequence: u32,
    },
    Stroke {
        index: usize,
        z_index: i16,
        sequence: u32,
    },
    Text {
        index: usize,
        z_index: i16,
        sequence: u32,
    },
    Image {
        index: usize,
        z_index: i16,
        sequence: u32,
    },
}

impl UiSurfacePaintCommand {
    pub fn z_index(self) -> i16 {
        match self {
            Self::Solid { z_index, .. }
            | Self::Stroke { z_index, .. }
            | Self::Text { z_index, .. }
            | Self::Image { z_index, .. } => z_index,
        }
    }

    pub(super) fn sequence(self) -> u32 {
        match self {
            Self::Solid { sequence, .. }
            | Self::Stroke { sequence, .. }
            | Self::Text { sequence, .. }
            | Self::Image { sequence, .. } => sequence,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct UiSurfaceDrawList {
    pub effects: UiSurfaceEffects,
    pub solids: Vec<UiSurfaceQuad>,
    pub strokes: Vec<UiSurfaceStroke>,
    pub text: Vec<UiSurfaceTextQuad>,
    pub images: Vec<UiSurfaceImageQuad>,
    pub paint_order: Vec<UiSurfacePaintCommand>,
    pub atlas_size: [u16; 2],
}

impl UiSurfaceDrawList {
    pub fn build<F>(frame: &UiSurfaceFrame, atlas: &UiTextAtlas, mut resolve: F) -> Self
    where
        F: FnMut(&str) -> String,
    {
        let resolved_text = frame
            .text_requests
            .iter()
            .map(|request| resolve(&request.text_key))
            .collect::<Vec<_>>();
        Self::build_with_resolved_text_values(frame, atlas, &resolved_text)
    }

    /// Builds a draw list from text already resolved by the stateful surface
    /// session. This is required for control values and placeholders, whose
    /// visible text is not necessarily their declarative `text_key`.
    pub fn build_with_resolved_text<F>(
        frame: &UiSurfaceFrame,
        atlas: &UiTextAtlas,
        resolve: F,
    ) -> Self
    where
        F: FnMut(&super::UiTextAtlasRequest) -> String,
    {
        Self::build_with_resolved_text_at_scale(frame, atlas, 1.0, resolve)
    }

    /// Builds a draw list for a physical target whose text atlas has been
    /// rasterized above logical layout resolution. Text rectangles remain in
    /// logical points while their glyph coverage stays dense in the atlas.
    pub fn build_with_resolved_text_at_scale<F>(
        frame: &UiSurfaceFrame,
        atlas: &UiTextAtlas,
        raster_scale: f32,
        mut resolve: F,
    ) -> Self
    where
        F: FnMut(&super::UiTextAtlasRequest) -> String,
    {
        let resolved_text = frame
            .text_requests
            .iter()
            .map(|request| resolve(request))
            .collect::<Vec<_>>();
        Self::build_with_resolved_text_values_at_scale(frame, atlas, raster_scale, &resolved_text)
    }

    /// Builds from text resolved by `UiSurfaceSession`. This avoids resolving
    /// values once for atlas upload and then again while creating geometry.
    pub fn build_with_resolved_text_values(
        frame: &UiSurfaceFrame,
        atlas: &UiTextAtlas,
        resolved_text: &[String],
    ) -> Self {
        Self::build_with_resolved_text_values_at_scale(frame, atlas, 1.0, resolved_text)
    }

    /// HiDPI variant of `build_with_resolved_text_values`.
    pub fn build_with_resolved_text_values_at_scale(
        frame: &UiSurfaceFrame,
        atlas: &UiTextAtlas,
        raster_scale: f32,
        resolved_text: &[String],
    ) -> Self {
        let raster_scale = raster_scale.clamp(1.0, 4.0);
        let mut solids = Vec::with_capacity(frame.layout_boxes.len() * 3);
        let mut strokes = Vec::new();
        let mut text = Vec::new();
        let mut images = Vec::new();
        let mut effects = UiSurfaceEffects::new();
        let mut paint_order = Vec::with_capacity(frame.layout_boxes.len() * 3);
        let mut sequence = 0_u32;
        let text_request_index = frame
            .text_requests
            .iter()
            .enumerate()
            .map(|(index, request)| (request.node_id.as_str(), index))
            .collect::<HashMap<_, _>>();

        for layout in &frame.layout_boxes {
            let starts = [solids.len(), strokes.len(), text.len(), images.len()];
            'paint: {
                let first_solid = solids.len();
                let style = skeleton_style(&layout.control, &layout.style, layout.rect);
                append_visual_quads(&mut solids, &mut effects, layout, &style, frame.tokens);
                for index in first_solid..solids.len() {
                    paint_order.push(UiSurfacePaintCommand::Solid {
                        index,
                        z_index: solids[index].z_index,
                        sequence,
                    });
                    sequence = sequence.wrapping_add(1);
                }

                let first_control_solid = solids.len();
                let first_control_stroke = strokes.len();
                append_control_quads(
                    &mut solids,
                    &mut strokes,
                    layout.content_rect,
                    layout.clip_rect,
                    &layout.control,
                    &layout.style,
                    layout.z_index.saturating_add(1),
                    frame.tokens,
                );
                for index in first_control_solid..solids.len() {
                    paint_order.push(UiSurfacePaintCommand::Solid {
                        index,
                        z_index: solids[index].z_index,
                        sequence,
                    });
                    sequence = sequence.wrapping_add(1);
                }
                for index in first_control_stroke..strokes.len() {
                    paint_order.push(UiSurfacePaintCommand::Stroke {
                        index,
                        z_index: strokes[index].z_index,
                        sequence,
                    });
                    sequence = sequence.wrapping_add(1);
                }

                if let UiControl::Image(image) = &layout.control {
                    let index = images.len();
                    images.push(UiSurfaceImageQuad {
                        rect: layout.content_rect,
                        clip_rect: layout.clip_rect,
                        source_key: super::images::canonical_builtin_icon_uri(&image.source.key)
                            .into_owned(),
                        fit: image.fit,
                        tint: apply_opacity(
                            image.tint.unwrap_or([255, 255, 255, 255]),
                            layout.style.opacity,
                        ),
                        z_index: layout.z_index,
                    });
                    let mut clips = layout.rounded_clips.clone();
                    clips.push(UiRoundedClip {
                        rect: layout.content_rect,
                        corners: layout.visual.corners.unwrap_or([layout.style.radius; 4]),
                    });
                    effects.insert(
                        UiPaintKey::Image(index),
                        UiPaintEffects {
                            clips,
                            image: Some(image.options.clone()),
                            ..Default::default()
                        },
                    );
                    paint_order.push(UiSurfacePaintCommand::Image {
                        index,
                        z_index: layout.z_index,
                        sequence,
                    });
                    sequence = sequence.wrapping_add(1);
                }

                let has_text = text_request_index.contains_key(layout.id.as_str());
                let content_rect = layout.content_rect;
                let content_clip = layout.clip_rect.intersection(content_rect);
                if let Some(icon) = layout.icon {
                    let icon_size = f32::from(icon.size.logical_pixels());
                    let icon_x = if has_text {
                        let measured = text_request_index
                            .get(layout.id.as_str())
                            .and_then(|index| {
                                frame
                                    .text_requests
                                    .get(*index)
                                    .zip(resolved_text.get(*index))
                            })
                            .and_then(|(request, value)| atlas.slot_for(request, value))
                            .map(|slot| f32::from(slot.rect.width) / raster_scale)
                            .unwrap_or(0.0);
                        content_origins(layout, measured).0
                    } else {
                        content_rect.x + ((content_rect.width - icon_size) * 0.5).max(0.0)
                    };
                    let icon_rect = UiRect::new(
                        icon_x,
                        content_rect.y + ((content_rect.height - icon_size) * 0.5).max(0.0),
                        icon_size.min(content_rect.width),
                        icon_size.min(content_rect.height),
                    );
                    let index = images.len();
                    images.push(UiSurfaceImageQuad {
                        rect: icon_rect,
                        clip_rect: content_clip,
                        source_key: builtin_icon_key(icon.id),
                        fit: UiImageFit::Contain,
                        tint: apply_opacity(
                            icon.tint.unwrap_or_else(|| {
                                let colors = frame.tokens.chrome();
                                if icon.state == raf_ui::UiIconState::Disabled {
                                    return colors.icon_disabled;
                                }
                                if icon.state == raf_ui::UiIconState::Active {
                                    return colors.icon_accent;
                                }
                                if icon.state == raf_ui::UiIconState::Hovered {
                                    return colors.icon;
                                }
                                match icon.tone {
                                    raf_ui::UiIconTone::Inherit => layout.style.text,
                                    raf_ui::UiIconTone::Normal => colors.icon,
                                    raf_ui::UiIconTone::Muted => colors.icon_muted,
                                    raf_ui::UiIconTone::Accent => colors.icon_accent,
                                    raf_ui::UiIconTone::OnAccent => colors.icon_on_accent,
                                    raf_ui::UiIconTone::Disabled => colors.icon_disabled,
                                }
                            }),
                            layout.style.opacity,
                        ),
                        z_index: layout.z_index.saturating_add(1),
                    });
                    paint_order.push(UiSurfacePaintCommand::Image {
                        index,
                        z_index: layout.z_index.saturating_add(1),
                        sequence,
                    });
                    sequence = sequence.wrapping_add(1);
                }

                let Some(&request_index) = text_request_index.get(layout.id.as_str()) else {
                    break 'paint;
                };
                let Some((request, resolved)) = frame
                    .text_requests
                    .get(request_index)
                    .zip(resolved_text.get(request_index))
                else {
                    break 'paint;
                };
                if let Some(slot) = atlas.slot_for(request, resolved.as_str()) {
                    let text_width = f32::from(slot.rect.width) / raster_scale;
                    let text_height = f32::from(slot.rect.height) / raster_scale;
                    let symbol_button = request.style.role == UiTextRole::Button
                        && resolved.chars().count() <= 2
                        && layout.icon.is_none();
                    let text_origin_x = if symbol_button {
                        content_rect.x + ((content_rect.width - text_width) * 0.5).max(0.0)
                    } else {
                        text_origin_x_for_layout(layout, request, resolved, atlas, raster_scale)
                    };
                    let text_rect = UiRect::new(
                        text_origin_x,
                        content_rect.y + ((content_rect.height - text_height) * 0.5).max(0.0),
                        text_width,
                        text_height,
                    );
                    if let Some(edit) = layout.text_edit {
                        let selection = edit.selection();
                        if selection.start < selection.end {
                            let characters = resolved.chars().collect::<Vec<_>>();
                            let lines = visual_line_ranges(request, resolved);
                            let line_height =
                                (request.style.line_height_px / raster_scale).max(1.0);
                            let text_y = text_rect.y;
                            let selection_color =
                                apply_opacity([58, 121, 226, 150], layout.style.opacity);
                            for (line_index, line) in lines.iter().enumerate() {
                                let start = selection.start.max(line.start);
                                let end = selection.end.min(line.end);
                                if start >= end || start >= characters.len() {
                                    continue;
                                }
                                let line_text = characters
                                    [line.start..line.end.min(characters.len())]
                                    .iter()
                                    .collect::<String>();
                                let start_x = text_origin_x
                                    + atlas.measure_prefix_width(
                                        request,
                                        &line_text,
                                        start.saturating_sub(line.start),
                                    ) / raster_scale;
                                let end_x = text_origin_x
                                    + atlas.measure_prefix_width(
                                        request,
                                        &line_text,
                                        end.saturating_sub(line.start),
                                    ) / raster_scale;
                                let selection_rect = UiRect::new(
                                    start_x.min(end_x),
                                    text_y + line_index as f32 * line_height,
                                    (end_x - start_x).abs().max(1.0),
                                    line_height,
                                )
                                .intersection(content_clip);
                                if selection_rect.is_empty() {
                                    continue;
                                }
                                let index = solids.len();
                                solids.push(UiSurfaceQuad {
                                    rect: selection_rect,
                                    clip_rect: content_clip,
                                    color: selection_color,
                                    // Keep the highlight in the text layer: it is
                                    // emitted after the input fill and before the
                                    // glyph command below.
                                    z_index: layout.z_index,
                                    radius: 1.0,
                                });
                                paint_order.push(UiSurfacePaintCommand::Solid {
                                    index,
                                    z_index: layout.z_index,
                                    sequence,
                                });
                                sequence = sequence.wrapping_add(1);
                            }
                        }
                    }
                    let index = text.len();
                    text.push(UiSurfaceTextQuad {
                        rect: text_rect,
                        clip_rect: content_clip,
                        atlas_rect: UiRect::new(
                            f32::from(slot.rect.x),
                            f32::from(slot.rect.y),
                            f32::from(slot.rect.width),
                            f32::from(slot.rect.height),
                        ),
                        color: request.style.color,
                        z_index: layout.z_index,
                    });
                    paint_order.push(UiSurfacePaintCommand::Text {
                        index,
                        z_index: layout.z_index,
                        sequence,
                    });
                    sequence = sequence.wrapping_add(1);
                }

                if let Some(edit) = layout.text_edit {
                    let (caret_line, prefix_width) =
                        text_cursor_metrics(request, resolved, edit.cursor, atlas, raster_scale);
                    let text_origin_x =
                        text_origin_x_for_layout(layout, request, resolved, atlas, raster_scale);
                    let caret_width = 1.0;
                    let caret_x = (text_origin_x + prefix_width).clamp(
                        content_rect.x,
                        (content_rect.right() - caret_width).max(content_rect.x),
                    );
                    let line_height = (request.style.line_height_px / raster_scale).max(1.0);
                    let text_height = atlas
                        .slot_for(request, resolved)
                        .map(|slot| f32::from(slot.rect.height) / raster_scale)
                        .unwrap_or(line_height);
                    let text_y =
                        content_rect.y + ((content_rect.height - text_height) * 0.5).max(0.0);
                    let caret_y =
                        (text_y + caret_line as f32 * line_height + 2.0).min(content_rect.bottom());
                    let caret_rect = UiRect::new(
                        caret_x,
                        caret_y,
                        caret_width.min(content_rect.width),
                        (if request.single_line {
                            content_rect.height - 4.0
                        } else {
                            line_height - 3.0
                        })
                        .max(1.0)
                        .min((content_rect.bottom() - caret_y).max(1.0)),
                    );
                    if !content_clip.is_empty() && !caret_rect.is_empty() {
                        let mut caret_color = layout.style.border;
                        if caret_color[3] == 0 {
                            caret_color = layout.style.text;
                        }
                        caret_color = apply_opacity(caret_color, layout.style.opacity);
                        let index = solids.len();
                        solids.push(UiSurfaceQuad {
                            rect: caret_rect,
                            clip_rect: content_clip,
                            color: caret_color,
                            z_index: layout.z_index.saturating_add(2),
                            radius: 0.0,
                        });
                        paint_order.push(UiSurfacePaintCommand::Solid {
                            index,
                            z_index: layout.z_index.saturating_add(2),
                            sequence,
                        });
                        sequence = sequence.wrapping_add(1);
                    }
                    if layout
                        .ime_preedit
                        .as_deref()
                        .is_some_and(|value| !value.is_empty())
                    {
                        let preedit_width = atlas.measure_prefix_width(
                            request,
                            layout.ime_preedit.as_deref().unwrap_or_default(),
                            layout
                                .ime_preedit
                                .as_deref()
                                .unwrap_or_default()
                                .chars()
                                .count(),
                        ) / raster_scale;
                        let underline = UiRect::new(
                            (text_origin_x + prefix_width).max(content_rect.x),
                            (caret_y + line_height - 3.0).min(content_rect.bottom() - 1.0),
                            preedit_width.max(2.0),
                            1.0,
                        )
                        .intersection(content_clip);
                        if !underline.is_empty() {
                            let index = solids.len();
                            solids.push(UiSurfaceQuad {
                                rect: underline,
                                clip_rect: content_clip,
                                color: apply_opacity([232, 133, 28, 255], layout.style.opacity),
                                z_index: layout.z_index.saturating_add(2),
                                radius: 0.0,
                            });
                            paint_order.push(UiSurfacePaintCommand::Solid {
                                index,
                                z_index: layout.z_index.saturating_add(2),
                                sequence,
                            });
                            sequence = sequence.wrapping_add(1);
                        }
                    }
                }
            }
            for key in (starts[0]..solids.len())
                .map(UiPaintKey::Solid)
                .chain((starts[1]..strokes.len()).map(UiPaintKey::Stroke))
                .chain((starts[2]..text.len()).map(UiPaintKey::Text))
                .chain((starts[3]..images.len()).map(UiPaintKey::Image))
            {
                effects.entry(key).or_insert_with(|| UiPaintEffects {
                    clips: layout.rounded_clips.clone(),
                    ..Default::default()
                });
            }
            if matches!(layout.control, UiControl::Image(_))
                && layout.style.border[3] > 0
                && layout
                    .visual
                    .border_widths
                    .unwrap_or([layout.style.border_width; 4])
                    .iter()
                    .any(|width| *width > 0.0)
            {
                let index = solids.len();
                solids.push(UiSurfaceQuad {
                    rect: layout.rect,
                    clip_rect: layout.clip_rect,
                    color: [0; 4],
                    z_index: layout.z_index.saturating_add(3),
                    radius: layout.style.radius,
                });
                effects.insert(
                    UiPaintKey::Solid(index),
                    UiPaintEffects {
                        corners: layout.visual.corners,
                        border: apply_opacity(layout.style.border, layout.style.opacity),
                        border_widths: layout
                            .visual
                            .border_widths
                            .unwrap_or([layout.style.border_width; 4]),
                        clips: layout.rounded_clips.clone(),
                        ..Default::default()
                    },
                );
                paint_order.push(UiSurfacePaintCommand::Solid {
                    index,
                    z_index: layout.z_index.saturating_add(3),
                    sequence,
                });
                sequence = sequence.wrapping_add(1);
            }
        }
        paint_order.sort_by_key(|command| (command.z_index(), command.sequence()));

        Self {
            effects,
            solids,
            strokes,
            text,
            images,
            paint_order,
            atlas_size: atlas.size(),
        }
    }

    /// Returns the retained paint order, synthesizing a stable legacy order
    /// for lists constructed by older callers.
    pub fn paint_commands(&self) -> Cow<'_, [UiSurfacePaintCommand]> {
        if !self.paint_order.is_empty() {
            return Cow::Borrowed(&self.paint_order);
        }

        let mut sequence = 0_u32;
        let mut commands = Vec::with_capacity(
            self.solids.len() + self.strokes.len() + self.text.len() + self.images.len(),
        );
        for (index, quad) in self.solids.iter().enumerate() {
            commands.push(UiSurfacePaintCommand::Solid {
                index,
                z_index: quad.z_index,
                sequence,
            });
            sequence = sequence.wrapping_add(1);
        }
        for (index, stroke) in self.strokes.iter().enumerate() {
            commands.push(UiSurfacePaintCommand::Stroke {
                index,
                z_index: stroke.z_index,
                sequence,
            });
            sequence = sequence.wrapping_add(1);
        }
        for (index, quad) in self.text.iter().enumerate() {
            commands.push(UiSurfacePaintCommand::Text {
                index,
                z_index: quad.z_index,
                sequence,
            });
            sequence = sequence.wrapping_add(1);
        }
        for (index, quad) in self.images.iter().enumerate() {
            commands.push(UiSurfacePaintCommand::Image {
                index,
                z_index: quad.z_index,
                sequence,
            });
            sequence = sequence.wrapping_add(1);
        }
        commands.sort_by_key(|command| (command.z_index(), command.sequence()));
        Cow::Owned(commands)
    }

    /// Produces physical-pixel geometry for the CPU fallback while preserving
    /// the source atlas rectangles. GPU presentation keeps logical geometry
    /// and uses its viewport transform instead.
    pub fn scaled_for_output(&self, scale_x: f32, scale_y: f32) -> Self {
        let scale_x = scale_x.clamp(0.01, 4.0);
        let scale_y = scale_y.clamp(0.01, 4.0);
        let scale_rect = |rect: UiRect| {
            UiRect::new(
                rect.x * scale_x,
                rect.y * scale_y,
                rect.width * scale_x,
                rect.height * scale_y,
            )
        };
        Self {
            effects: self
                .effects
                .iter()
                .map(|(key, effect)| (*key, effect.scaled(scale_x, scale_y)))
                .collect(),
            solids: self
                .solids
                .iter()
                .map(|quad| UiSurfaceQuad {
                    rect: scale_rect(quad.rect),
                    clip_rect: scale_rect(quad.clip_rect),
                    color: quad.color,
                    z_index: quad.z_index,
                    radius: quad.radius * scale_x.min(scale_y),
                })
                .collect(),
            strokes: self
                .strokes
                .iter()
                .map(|stroke| UiSurfaceStroke {
                    start: [stroke.start[0] * scale_x, stroke.start[1] * scale_y],
                    end: [stroke.end[0] * scale_x, stroke.end[1] * scale_y],
                    clip_rect: scale_rect(stroke.clip_rect),
                    color: stroke.color,
                    width: stroke.width * scale_x.min(scale_y),
                    z_index: stroke.z_index,
                })
                .collect(),
            text: self
                .text
                .iter()
                .map(|quad| UiSurfaceTextQuad {
                    rect: scale_rect(quad.rect),
                    clip_rect: scale_rect(quad.clip_rect),
                    atlas_rect: quad.atlas_rect,
                    color: quad.color,
                    z_index: quad.z_index,
                })
                .collect(),
            images: self
                .images
                .iter()
                .map(|quad| UiSurfaceImageQuad {
                    rect: scale_rect(quad.rect),
                    clip_rect: scale_rect(quad.clip_rect),
                    source_key: quad.source_key.clone(),
                    fit: quad.fit,
                    tint: quad.tint,
                    z_index: quad.z_index,
                })
                .collect(),
            paint_order: self.paint_order.clone(),
            atlas_size: self.atlas_size,
        }
    }
}

fn append_visual_quads(
    solids: &mut Vec<UiSurfaceQuad>,
    effects: &mut UiSurfaceEffects,
    layout: &super::UiLayoutBox,
    style: &UiStyle,
    tokens: raf_ui::UiTokens,
) {
    let corners = layout.visual.corners.unwrap_or([style.radius; 4]);
    if let Some(shadow) = layout.visual.shadow {
        let spread = shadow.spread.clamp(0.0, 24.0);
        let index = solids.len();
        solids.push(UiSurfaceQuad {
            rect: UiRect::new(
                layout.rect.x + shadow.offset[0] - spread,
                layout.rect.y + shadow.offset[1] - spread,
                layout.rect.width + spread * 2.0,
                layout.rect.height + spread * 2.0,
            ),
            clip_rect: layout.clip_rect,
            color: apply_opacity(shadow.color, style.opacity),
            z_index: layout.z_index,
            radius: style.radius + spread,
        });
        effects.insert(
            UiPaintKey::Solid(index),
            UiPaintEffects {
                corners: Some(corners.map(|r| r + spread)),
                softness: shadow.softness.clamp(0.0, 24.0),
                clips: layout.rounded_clips.clone(),
                ..Default::default()
            },
        );
    }
    if style.fill[3] > 0 || style.border[3] > 0 || layout.visual.gradient.is_some() {
        let index = solids.len();
        solids.push(UiSurfaceQuad {
            rect: layout.rect,
            clip_rect: layout.clip_rect,
            color: apply_opacity(style.fill, style.opacity),
            z_index: layout.z_index,
            radius: style.radius,
        });
        let image = matches!(layout.control, UiControl::Image(_));
        effects.insert(
            UiPaintKey::Solid(index),
            UiPaintEffects {
                corners: Some(corners),
                border: if image {
                    [0; 4]
                } else {
                    apply_opacity(style.border, style.opacity)
                },
                border_widths: layout
                    .visual
                    .border_widths
                    .unwrap_or([style.border_width; 4]),
                gradient: layout.visual.gradient.map(|mut g| {
                    g.start = apply_opacity(g.start, style.opacity);
                    g.end = apply_opacity(g.end, style.opacity);
                    g
                }),
                clips: layout.rounded_clips.clone(),
                ..Default::default()
            },
        );
    }
    let focus_ring = layout.visual.focus_ring.or_else(|| {
        (layout.focused && layout.focusable).then_some(raf_ui::UiFocusRing {
            width: 1.5,
            offset: 2.0,
            color: tokens.chrome().focus_ring,
        })
    });
    if let Some(ring) = focus_ring.filter(|_| layout.focused) {
        let offset = ring.offset.max(0.0);
        let width = ring.width.clamp(0.0, 8.0);
        let index = solids.len();
        solids.push(UiSurfaceQuad {
            rect: UiRect::new(
                layout.rect.x - offset - width,
                layout.rect.y - offset - width,
                layout.rect.width + 2.0 * (offset + width),
                layout.rect.height + 2.0 * (offset + width),
            ),
            clip_rect: layout.clip_rect,
            color: [0; 4],
            z_index: layout.z_index.saturating_add(4),
            radius: style.radius + offset + width,
        });
        effects.insert(
            UiPaintKey::Solid(index),
            UiPaintEffects {
                corners: Some(corners.map(|r| r + offset + width)),
                border: apply_opacity(ring.color, style.opacity),
                border_widths: [width; 4],
                clips: layout.rounded_clips.clone(),
                ..Default::default()
            },
        );
    }
}

fn skeleton_style(control: &UiControl, style: &UiStyle, rect: UiRect) -> UiStyle {
    let UiControl::Skeleton(skeleton) = control else {
        return style.clone();
    };
    let phase = skeleton.phase.clamp(0.35, 1.0);
    let fill = if style.fill[3] == 0 {
        [86, 86, 90, (180.0 * phase).round() as u8]
    } else {
        let mut fill = style.fill;
        fill[3] = ((f32::from(fill[3]) * phase).round()) as u8;
        fill
    };
    UiStyle {
        fill,
        border: [0, 0, 0, 0],
        text: style.text,
        border_width: 0.0,
        radius: match skeleton.shape {
            UiSkeletonShape::Text => rect.height.min(4.0),
            UiSkeletonShape::Rectangle => style.radius.max(4.0),
            UiSkeletonShape::Circle => rect.width.min(rect.height) * 0.5,
        },
        opacity: style.opacity,
    }
}

/// Paints the visual body of retained controls that are not ordinary buttons.
/// The interaction layer already emits their typed actions; keeping their
/// geometry here means every host gets the same toggle/range affordance on
/// GPU and CPU without asking each settings surface to fake it with offsets.
fn append_control_quads(
    solids: &mut Vec<UiSurfaceQuad>,
    strokes: &mut Vec<UiSurfaceStroke>,
    rect: UiRect,
    clip_rect: UiRect,
    control: &UiControl,
    style: &UiStyle,
    z_index: i16,
    tokens: raf_ui::UiTokens,
) {
    let accent = tokens.accent;
    let track = tokens.surface_alt;
    let thumb = tokens.text;

    if rect.width <= 0.0 || rect.height <= 0.0 || style.opacity <= 0.0 {
        return;
    }

    let border = if style.border[3] == 0 {
        track
    } else {
        style.border
    };
    let text = if style.text[3] == 0 {
        thumb
    } else {
        style.text
    };
    let color = |value: [u8; 4]| apply_opacity(value, style.opacity);

    match control {
        UiControl::Toggle(toggle) => {
            if toggle.presentation == UiTogglePresentation::Checkbox {
                let size = rect.width.min(rect.height).clamp(16.0, 20.0);
                let box_rect = UiRect::new(
                    rect.x + (rect.width - size).max(0.0),
                    rect.y + (rect.height - size) * 0.5,
                    size,
                    size,
                );
                solids.push(UiSurfaceQuad {
                    rect: box_rect,
                    clip_rect,
                    // Checkbox outlines are an explicit foreground affordance:
                    // near-white in dark themes and near-black in light themes.
                    // `style.text` comes from the active RafUI theme token.
                    color: color(if style.text[3] == 0 {
                        thumb
                    } else {
                        style.text
                    }),
                    z_index,
                    radius: 2.0,
                });
                let inner_color = if toggle.value {
                    accent
                } else if style.fill[3] == 0 {
                    track
                } else {
                    style.fill
                };
                solids.push(UiSurfaceQuad {
                    rect: UiRect::new(
                        box_rect.x + 2.0,
                        box_rect.y + 2.0,
                        (box_rect.width - 4.0).max(1.0),
                        (box_rect.height - 4.0).max(1.0),
                    ),
                    clip_rect,
                    color: color(inner_color),
                    z_index: z_index.saturating_add(1),
                    radius: 1.0,
                });
                if toggle.value {
                    append_checkbox_checkmark(
                        strokes,
                        box_rect,
                        clip_rect,
                        color(text),
                        z_index.saturating_add(2),
                    );
                }
            } else {
                let width = rect.width.min(44.0).max(28.0);
                let height = rect.height.min(20.0).max(16.0);
                let track_rect = UiRect::new(
                    rect.x + (rect.width - width).max(0.0),
                    rect.y + (rect.height - height) * 0.5,
                    width,
                    height,
                );
                let track_color = if toggle.value { accent } else { border };
                solids.push(UiSurfaceQuad {
                    rect: track_rect,
                    clip_rect,
                    color: color(track_color),
                    z_index,
                    radius: height * 0.5,
                });
                let thumb_size = (height - 4.0).max(8.0);
                let thumb_x = if toggle.value {
                    track_rect.right() - thumb_size - 2.0
                } else {
                    track_rect.x + 2.0
                };
                solids.push(UiSurfaceQuad {
                    rect: UiRect::new(
                        thumb_x,
                        track_rect.y + (height - thumb_size) * 0.5,
                        thumb_size,
                        thumb_size,
                    ),
                    clip_rect,
                    color: color(text),
                    z_index: z_index.saturating_add(1),
                    radius: thumb_size * 0.5,
                });
            }
        }
        UiControl::Range(range) => {
            // Transparent ranges are useful as interaction-only hit areas,
            // for example over a color-picker image. They still dispatch the
            // typed value action but do not paint a second track on top.
            if style.fill[3] == 0 && style.border[3] == 0 && style.text[3] == 0 {
                return;
            }
            let fraction = range.fraction();
            match range.orientation {
                super::UiRangeOrientation::Horizontal => {
                    let track_height = 4.0_f32.min(rect.height).max(2.0);
                    let track_rect = UiRect::new(
                        rect.x,
                        rect.y + (rect.height - track_height) * 0.5,
                        rect.width,
                        track_height,
                    );
                    solids.push(UiSurfaceQuad {
                        rect: track_rect,
                        clip_rect,
                        color: color(border),
                        z_index,
                        radius: track_height * 0.5,
                    });
                    let progress = UiRect::new(
                        track_rect.x,
                        track_rect.y,
                        track_rect.width * fraction,
                        track_rect.height,
                    );
                    if progress.width > 0.0 {
                        solids.push(UiSurfaceQuad {
                            rect: progress,
                            clip_rect,
                            color: color(accent),
                            z_index: z_index.saturating_add(1),
                            radius: track_height * 0.5,
                        });
                    }
                    let thumb_size = rect.height.min(14.0).max(8.0);
                    let thumb_x = (track_rect.x + track_rect.width * fraction - thumb_size * 0.5)
                        .clamp(track_rect.x, track_rect.right() - thumb_size);
                    solids.push(UiSurfaceQuad {
                        rect: UiRect::new(
                            thumb_x,
                            rect.y + (rect.height - thumb_size) * 0.5,
                            thumb_size,
                            thumb_size,
                        ),
                        clip_rect,
                        color: color(text),
                        z_index: z_index.saturating_add(2),
                        radius: thumb_size * 0.5,
                    });
                }
                super::UiRangeOrientation::Vertical => {
                    let track_width = 4.0_f32.min(rect.width).max(2.0);
                    let track_rect = UiRect::new(
                        rect.x + (rect.width - track_width) * 0.5,
                        rect.y,
                        track_width,
                        rect.height,
                    );
                    solids.push(UiSurfaceQuad {
                        rect: track_rect,
                        clip_rect,
                        color: color(border),
                        z_index,
                        radius: track_width * 0.5,
                    });
                    let progress = UiRect::new(
                        track_rect.x,
                        track_rect.bottom() - track_rect.height * fraction,
                        track_rect.width,
                        track_rect.height * fraction,
                    );
                    if progress.height > 0.0 {
                        solids.push(UiSurfaceQuad {
                            rect: progress,
                            clip_rect,
                            color: color(accent),
                            z_index: z_index.saturating_add(1),
                            radius: track_width * 0.5,
                        });
                    }
                    let thumb_size = rect.width.min(14.0).max(8.0);
                    let thumb_y =
                        (track_rect.bottom() - track_rect.height * fraction - thumb_size * 0.5)
                            .clamp(track_rect.y, track_rect.bottom() - thumb_size);
                    solids.push(UiSurfaceQuad {
                        rect: UiRect::new(
                            rect.x + (rect.width - thumb_size) * 0.5,
                            thumb_y,
                            thumb_size,
                            thumb_size,
                        ),
                        clip_rect,
                        color: color(text),
                        z_index: z_index.saturating_add(2),
                        radius: thumb_size * 0.5,
                    });
                }
            }
        }
        _ => {}
    }
}

fn append_checkbox_checkmark(
    strokes: &mut Vec<UiSurfaceStroke>,
    box_rect: UiRect,
    clip_rect: UiRect,
    color: [u8; 4],
    z_index: i16,
) {
    let size = box_rect.width.min(box_rect.height);
    if size <= 0.0 {
        return;
    }

    // Two connected vector strokes produce a stable tick at 100% through
    // 200% DPI. The old implementation emitted five tiny rounded quads; at
    // distance those read as a dotted staircase instead of one mark.
    let width = (size * 0.17).clamp(2.4, 3.4);
    let joint = [box_rect.x + size * 0.42, box_rect.y + size * 0.70];
    strokes.extend([
        UiSurfaceStroke {
            start: [box_rect.x + size * 0.22, box_rect.y + size * 0.51],
            end: joint,
            clip_rect,
            color,
            width,
            z_index,
        },
        UiSurfaceStroke {
            start: joint,
            end: [box_rect.x + size * 0.78, box_rect.y + size * 0.29],
            clip_rect,
            color,
            width,
            z_index,
        },
    ]);
}

fn apply_opacity(mut color: [u8; 4], opacity: f32) -> [u8; 4] {
    color[3] = ((f32::from(color[3]) * opacity.clamp(0.0, 1.0)).round()) as u8;
    color
}

fn content_origins(layout: &super::UiLayoutBox, text_width: f32) -> (f32, f32) {
    let content = layout.content_rect;
    let icon_width = layout
        .icon
        .map(|i| f32::from(i.size.logical_pixels()))
        .unwrap_or(0.0);
    let gap = if icon_width > 0.0 && text_width > 0.0 {
        layout.visual.content.gap.max(0.0)
    } else {
        0.0
    };
    let width = icon_width + gap + text_width;
    let free = (content.width - width).max(0.0);
    let origin = content.x
        + match layout.visual.content.align {
            raf_ui::UiContentAlign::Start => 0.0,
            raf_ui::UiContentAlign::Center => free * 0.5,
            raf_ui::UiContentAlign::End => free,
        };
    if layout.visual.content.icon_placement == raf_ui::UiIconPlacement::End {
        (origin + text_width + gap, origin)
    } else {
        (origin, origin + icon_width + gap)
    }
}

pub(super) fn text_origin_x_for_layout(
    layout: &super::UiLayoutBox,
    request: &UiTextAtlasRequest,
    resolved: &str,
    atlas: &UiTextAtlas,
    raster_scale: f32,
) -> f32 {
    let measured = atlas
        .slot_for(request, resolved)
        .map(|slot| f32::from(slot.rect.width) / raster_scale)
        .unwrap_or(0.0);
    let base = content_origins(layout, measured).1;
    let Some(edit) = layout.text_edit else {
        return base;
    };
    if layout.control.text_input().is_none() {
        return base;
    }
    if !request.single_line
        && layout
            .control
            .text_input()
            .is_some_and(|input| input.multiline)
    {
        return base;
    }
    let caret_width =
        text_prefix_width_for_cursor(request, resolved, edit.cursor, atlas) / raster_scale;
    let right = layout.content_rect.right() - 2.0;
    let available = (right - base).max(1.0);
    let offset = if caret_width > available {
        (available - caret_width).min(0.0)
    } else {
        0.0
    };
    (base + offset).max(base.min(layout.content_rect.right()))
}

/// Measures the current visual line instead of the widest line in a
/// multiline value. `UiTextAtlas::measure_prefix_width` intentionally returns
/// the widest prefix width for general layout; a caret needs the width of the
/// line it is actually sitting on.
pub(super) fn text_prefix_width_for_cursor(
    request: &UiTextAtlasRequest,
    resolved: &str,
    cursor: usize,
    atlas: &UiTextAtlas,
) -> f32 {
    text_cursor_metrics(request, resolved, cursor, atlas, 1.0).1
}

fn text_cursor_metrics(
    request: &UiTextAtlasRequest,
    resolved: &str,
    cursor: usize,
    atlas: &UiTextAtlas,
    raster_scale: f32,
) -> (usize, f32) {
    let characters = resolved.chars().collect::<Vec<_>>();
    let cursor = cursor.min(characters.len());
    let lines = visual_line_ranges(request, resolved);
    let line_index = lines
        .iter()
        .enumerate()
        .position(|(index, line)| {
            cursor >= line.start
                && (cursor < line.end
                    || cursor == line.end
                        && lines.get(index + 1).is_none_or(|next| next.start != cursor))
        })
        .unwrap_or_else(|| lines.len().saturating_sub(1));
    let line = lines
        .get(line_index)
        .copied()
        .unwrap_or(super::text_atlas::UiTextVisualLine {
            start: cursor,
            end: cursor,
            width: 0.0,
        });
    let line_text = characters[line.start.min(characters.len())..line.end.min(characters.len())]
        .iter()
        .collect::<String>();
    let prefix = cursor
        .saturating_sub(line.start)
        .min(line.end.saturating_sub(line.start));
    (
        line_index,
        atlas.measure_prefix_width(request, &line_text, prefix) / raster_scale.max(1.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_graphic_basic::ui_surface::UiSurface;
    use raf_ui::{
        StudioUiPalette, UiFlow, UiLayout, UiNode, UiNodeKind, UiRange, UiSpacing, UiTextInput,
        UiTextStyle, UiToggle,
    };

    #[test]
    fn builtin_image_aliases_compile_to_one_canonical_resource_key() {
        let image = |id: &str, key: &str| {
            UiNode::image(
                id,
                raf_ui::UiImage {
                    source: raf_ui::UiImageSource::new(key),
                    fit: UiImageFit::Contain,
                    tint: None,
                    options: Default::default(),
                },
            )
            .with_layout(UiLayout::fixed(24.0, 24.0))
        };
        let surface = UiSurface::new(
            "aliases",
            StudioUiPalette::IndustrialDark,
            UiNode::row(
                "root",
                [
                    image("canonical", "builtin://icon/undo"),
                    image("alias", "builtin://icon/Undo"),
                ],
            ),
        );
        let mut session = super::super::UiSurfaceSession::default();
        let frame = session.build_frame(&surface, 100, 40, [0; 4]);
        let list = UiSurfaceDrawList::build(&frame, &session.text_atlas, str::to_owned);
        assert_eq!(list.images.len(), 2);
        assert!(list
            .images
            .iter()
            .all(|quad| quad.source_key == "builtin://icon/undo"));
    }

    #[test]
    fn draw_list_contains_rasterized_text_quad() {
        let palette = StudioUiPalette::IndustrialDark;
        let surface = UiSurface::new(
            "surface",
            palette,
            UiNode::new("root", UiNodeKind::Root)
                .with_style(palette.root_style())
                .with_child(
                    UiNode::new("title", UiNodeKind::Label)
                        .with_text_key("title")
                        .with_text_style(UiTextStyle::panel_title([255, 255, 255, 255])),
                ),
        );
        let mut session = super::super::UiSurfaceSession::default();
        let frame =
            session.build_frame_with_resolved_text(&surface, 320, 160, [0, 0, 0, 255], |key| {
                key.to_string()
            });
        let list = UiSurfaceDrawList::build(&frame, &session.text_atlas, |key| key.to_string());

        assert!(!list.solids.is_empty());
        assert_eq!(list.text.len(), 1);
    }

    #[test]
    fn nested_text_rows_honor_padding_and_keep_text_inside_their_tracks() {
        let palette = StudioUiPalette::IndustrialDark;
        let root = UiNode::new("root", UiNodeKind::Root)
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                padding: UiSpacing::same(8.0),
                gap: 2.0,
                ..UiLayout::fill(UiFlow::Column)
            })
            .with_child(
                UiNode::new("row", UiNodeKind::Toolbar)
                    .with_layout(UiLayout {
                        flow: UiFlow::Row,
                        gap: 6.0,
                        padding: UiSpacing::xy(4.0, 2.0),
                        ..UiLayout::fixed(0.0, 24.0)
                    })
                    .with_child(
                        UiNode::new("timestamp", UiNodeKind::Label)
                            .with_text_key("timestamp")
                            .with_layout(UiLayout::fixed(58.0, 18.0)),
                    )
                    .with_child(
                        UiNode::new("message", UiNodeKind::Label)
                            .with_text_key("message")
                            .with_layout(UiLayout {
                                grow: 1.0,
                                min_size: [1.0, 18.0],
                                ..UiLayout::fixed(0.0, 18.0)
                            }),
                    ),
            );
        let surface = UiSurface::new("nested-text", palette, root);
        let mut session = super::super::UiSurfaceSession::default();
        let frame = session
            .build_frame_with_resolved_text(&surface, 320, 80, [0; 4], |key| key.to_string());
        let row = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "row")
            .unwrap();
        let timestamp = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "timestamp")
            .unwrap();
        let message = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "message")
            .unwrap();

        assert_eq!(row.content_rect.x, row.rect.x + 4.0);
        assert!(timestamp.rect.x >= row.content_rect.x);
        assert!(timestamp.rect.right() <= message.rect.x);

        let list = UiSurfaceDrawList::build_with_resolved_text_values(
            &frame,
            &session.text_atlas,
            &["timestamp".to_string(), "message".to_string()],
        );
        let timestamp_text = list
            .text
            .iter()
            .find(|quad| quad.rect.width > 0.0 && quad.rect.x >= timestamp.content_rect.x)
            .unwrap();
        assert!(timestamp_text.rect.x >= timestamp.content_rect.x);
    }

    #[test]
    fn short_symbol_buttons_center_text_inside_the_safe_area() {
        let palette = StudioUiPalette::IndustrialDark;
        let root = UiNode::new("root", UiNodeKind::Root).with_child(
            UiNode::new("back", UiNodeKind::Button)
                .with_text_value("<")
                .with_text_style(UiTextStyle::button([255, 255, 255, 255]))
                .with_layout(UiLayout::fixed(28.0, 30.0)),
        );
        let surface = UiSurface::new("symbol-button", palette, root);
        let mut session = super::super::UiSurfaceSession::default();
        let frame =
            session.build_frame_with_resolved_text(&surface, 80, 48, [0, 0, 0, 255], |key| {
                key.to_string()
            });
        let button = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "back")
            .expect("symbol button");
        let list = UiSurfaceDrawList::build_with_resolved_text_values(
            &frame,
            &session.text_atlas,
            &["<".to_string()],
        );
        let text = list.text.first().expect("symbol text quad");
        let text_center = text.rect.x + text.rect.width * 0.5;
        let content_center = button.content_rect.x + button.content_rect.width * 0.5;

        assert!((text_center - content_center).abs() < 1.0);
    }

    #[test]
    fn retained_toggle_and_range_controls_emit_visual_geometry() {
        let palette = StudioUiPalette::IndustrialDark;
        let root = UiNode::new("root", UiNodeKind::Root)
            .with_layout(UiLayout::fill(UiFlow::Column))
            .with_child(
                UiNode::toggle("toggle", UiToggle::new("toggle.value", true))
                    .with_layout(UiLayout::fixed(48.0, 28.0)),
            )
            .with_child(
                UiNode::range("range", UiRange::new("range.value", 0.5, 0.0, 1.0, 0.1))
                    .with_layout(UiLayout::fixed(180.0, 28.0)),
            );
        let surface = UiSurface::new("control-geometry", palette, root);
        let mut session = super::super::UiSurfaceSession::default();
        let frame = session.build_frame(&surface, 240, 100, [0; 4]);
        let list = UiSurfaceDrawList::build(&frame, &session.text_atlas, |key| key.to_string());

        assert!(list.solids.iter().any(|quad| quad.radius >= 8.0));
        assert!(list.solids.iter().any(|quad| quad.rect.width > 80.0));
    }

    #[test]
    fn checked_checkbox_uses_theme_foreground_and_native_strokes() {
        for palette in [StudioUiPalette::IndustrialDark, StudioUiPalette::PaperLight] {
            let control = UiControl::Toggle(
                UiToggle::new("setting.value", true)
                    .with_presentation(UiTogglePresentation::Checkbox),
            );
            let mut solids = Vec::new();
            let mut strokes = Vec::new();
            append_control_quads(
                &mut solids,
                &mut strokes,
                UiRect::new(0.0, 0.0, 22.0, 22.0),
                UiRect::new(0.0, 0.0, 22.0, 22.0),
                &control,
                &palette.root_style(),
                1,
                palette.tokens(),
            );

            assert_eq!(solids[0].color, palette.tokens().text);
            assert_eq!(strokes.len(), 2);
            assert!(strokes.iter().all(|stroke| stroke.width >= 2.4));
            assert!(strokes[0].end == strokes[1].start);
        }
    }

    #[test]
    fn focused_text_input_emits_a_visible_caret_quad() {
        let palette = StudioUiPalette::IndustrialDark;
        let root = UiNode::new("root", UiNodeKind::Root).with_child(
            UiNode::text_input("query", UiTextInput::new("query.value"))
                .with_layout(UiLayout::fixed(180.0, 28.0))
                .focusable(),
        );
        let surface = UiSurface::new("caret", palette, root);
        let mut session = super::super::UiSurfaceSession::default();
        session
            .interaction
            .controls
            .set_text("query.value", "raf", 64);
        session.interaction.focus.request_focus("query");
        let frame = session
            .build_frame_with_resolved_text(&surface, 240, 80, [0; 4], |key| key.to_string());
        let input = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "query")
            .unwrap();
        assert_eq!(input.text_edit.unwrap().cursor, 3);

        let list = UiSurfaceDrawList::build_with_resolved_text_values(
            &frame,
            &session.text_atlas,
            &["raf".to_string()],
        );
        assert!(list.solids.iter().any(|quad| {
            quad.rect.width <= 1.0
                && quad.rect.height >= 14.0
                && quad.rect.x > input.content_rect.x
                && quad.clip_rect == input.content_rect
        }));
    }

    #[test]
    fn focused_text_input_emits_a_blue_selection_quad_before_glyphs() {
        let palette = StudioUiPalette::IndustrialDark;
        let root = UiNode::new("root", UiNodeKind::Root).with_child(
            UiNode::text_input("query", UiTextInput::new("query.value"))
                .with_layout(UiLayout::fixed(180.0, 28.0))
                .focusable(),
        );
        let surface = UiSurface::new("selection", palette, root);
        let mut session = super::super::UiSurfaceSession::default();
        session
            .interaction
            .controls
            .set_text("query.value", "rafui", 64);
        session
            .interaction
            .controls
            .set_selection("query.value", 1, 4);
        session.interaction.focus.request_focus("query");
        let frame = session
            .build_frame_with_resolved_text(&surface, 240, 80, [0; 4], |key| key.to_string());
        let list = UiSurfaceDrawList::build_with_resolved_text_values(
            &frame,
            &session.text_atlas,
            &["rafui".to_string()],
        );

        let selection_index = list
            .solids
            .iter()
            .position(|quad| quad.color == [58, 121, 226, 150])
            .expect("blue text selection quad");
        assert!(list.solids[selection_index].rect.width > 0.0);
        let selection_order = list
            .paint_order
            .iter()
            .position(|command| {
                matches!(
                    command,
                    UiSurfacePaintCommand::Solid { index, .. } if *index == selection_index
                )
            })
            .expect("selection paint command");
        let text_order = list
            .paint_order
            .iter()
            .position(|command| matches!(command, UiSurfacePaintCommand::Text { .. }))
            .expect("text paint command");
        assert!(selection_order < text_order);
    }

    #[test]
    fn direct_draw_list_encodes_fill_and_border_in_one_analytic_quad() {
        let mut style = StudioUiPalette::IndustrialDark.panel_style();
        style.border_width = 2.0;
        let surface = UiSurface::new(
            "borders",
            StudioUiPalette::IndustrialDark,
            UiNode::new("root", UiNodeKind::Root).with_style(style),
        );
        let mut session = super::super::UiSurfaceSession::default();
        let frame = session.build_frame(&surface, 160, 80, [0, 0, 0, 255]);
        let list = UiSurfaceDrawList::build(&frame, &session.text_atlas, |key| key.to_string());

        assert_eq!(list.solids.len(), 1);
        let effects = &list.effects[&UiPaintKey::Solid(0)];
        assert_eq!(effects.border_widths, [2.0; 4]);
        assert!(effects.border[3] > 0);
    }

    #[test]
    fn paint_order_keeps_base_text_below_an_elevated_overlay() {
        let palette = StudioUiPalette::IndustrialDark;
        let root = UiNode::new("root", UiNodeKind::Root)
            .with_child(
                UiNode::new("base-label", UiNodeKind::Label)
                    .with_text_key("base")
                    .with_style(palette.panel_style()),
            )
            .with_child(
                UiNode::new("overlay", UiNodeKind::Overlay)
                    .with_layout(
                        raf_ui::UiLayout::absolute(UiRect::new(0.0, 0.0, 120.0, 40.0))
                            .with_z_index(10),
                    )
                    .with_style(palette.panel_style()),
            );
        let surface = UiSurface::new("order", palette, root);
        let mut session = super::super::UiSurfaceSession::default();
        let frame =
            session.build_frame_with_resolved_text(&surface, 160, 80, [0, 0, 0, 255], |key| {
                key.to_string()
            });
        let list = UiSurfaceDrawList::build(&frame, &session.text_atlas, |key| key.to_string());

        assert!(list
            .paint_order
            .windows(2)
            .all(|pair| pair[0].z_index() <= pair[1].z_index()));
        assert_eq!(list.paint_order.last().unwrap().z_index(), 10);
    }
}
