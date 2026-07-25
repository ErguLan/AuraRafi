//! Renderer-ready retained UI draw data.
//!
//! This module has no `eframe` dependency. A native WGPU host and the legacy
//! editor bridge can consume the same draw list while migration happens.

use std::borrow::Cow;
use std::collections::HashMap;

use super::{UiControl, UiImageFit, UiRect, UiSkeletonShape, UiStyle, UiSurfaceFrame, UiTextAtlas};

#[derive(Debug, Clone, PartialEq)]
pub struct UiSurfaceQuad {
    pub rect: UiRect,
    pub clip_rect: UiRect,
    pub color: [u8; 4],
    pub z_index: i16,
    pub radius: f32,
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
            | Self::Text { z_index, .. }
            | Self::Image { z_index, .. } => z_index,
        }
    }

    fn sequence(self) -> u32 {
        match self {
            Self::Solid { sequence, .. }
            | Self::Text { sequence, .. }
            | Self::Image { sequence, .. } => sequence,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct UiSurfaceDrawList {
    pub solids: Vec<UiSurfaceQuad>,
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
        let mut text = Vec::new();
        let mut images = Vec::new();
        let mut paint_order = Vec::with_capacity(frame.layout_boxes.len() * 3);
        let mut sequence = 0_u32;
        let text_request_index = frame
            .text_requests
            .iter()
            .enumerate()
            .map(|(index, request)| (request.node_id.as_str(), index))
            .collect::<HashMap<_, _>>();

        for layout in &frame.layout_boxes {
            let first_solid = solids.len();
            let style = skeleton_style(&layout.control, &layout.style, layout.rect);
            append_style_quads(
                &mut solids,
                layout.rect,
                layout.clip_rect,
                &style,
                layout.z_index,
            );
            for index in first_solid..solids.len() {
                paint_order.push(UiSurfacePaintCommand::Solid {
                    index,
                    z_index: solids[index].z_index,
                    sequence,
                });
                sequence = sequence.wrapping_add(1);
            }

            if let UiControl::Image(image) = &layout.control {
                let index = images.len();
                images.push(UiSurfaceImageQuad {
                    rect: layout.rect,
                    clip_rect: layout.clip_rect,
                    source_key: image.source.key.clone(),
                    fit: image.fit,
                    tint: image.tint.unwrap_or([255, 255, 255, 255]),
                    z_index: layout.z_index,
                });
                paint_order.push(UiSurfacePaintCommand::Image {
                    index,
                    z_index: layout.z_index,
                    sequence,
                });
                sequence = sequence.wrapping_add(1);
            }

            let Some(&request_index) = text_request_index.get(layout.id.as_str()) else {
                continue;
            };
            let Some((request, resolved)) = frame
                .text_requests
                .get(request_index)
                .zip(resolved_text.get(request_index))
            else {
                continue;
            };
            let Some(slot) = atlas.slot_for(request, resolved.as_str()) else {
                continue;
            };
            let text_width = f32::from(slot.rect.width) / raster_scale;
            let text_height = f32::from(slot.rect.height) / raster_scale;
            let text_rect = UiRect::new(
                layout.rect.x + 4.0,
                layout.rect.y + ((layout.rect.height - text_height) * 0.5).max(2.0),
                text_width,
                text_height,
            );
            let index = text.len();
            text.push(UiSurfaceTextQuad {
                rect: text_rect,
                clip_rect: layout.clip_rect,
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
        paint_order.sort_by_key(|command| (command.z_index(), command.sequence()));

        Self {
            solids,
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
        let mut commands =
            Vec::with_capacity(self.solids.len() + self.text.len() + self.images.len());
        for (index, quad) in self.solids.iter().enumerate() {
            commands.push(UiSurfacePaintCommand::Solid {
                index,
                z_index: quad.z_index,
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

fn append_style_quads(
    solids: &mut Vec<UiSurfaceQuad>,
    rect: UiRect,
    clip_rect: UiRect,
    style: &UiStyle,
    z_index: i16,
) {
    if rect.width <= 0.0 || rect.height <= 0.0 || style.opacity <= 0.0 {
        return;
    }
    let fill = apply_opacity(style.fill, style.opacity);
    let border = apply_opacity(style.border, style.opacity);
    let border_width = style
        .border_width
        .max(0.0)
        .min(rect.width * 0.5)
        .min(rect.height * 0.5);
    if border[3] == 0 || border_width <= 0.0 {
        if fill[3] != 0 {
            solids.push(UiSurfaceQuad {
                rect,
                clip_rect,
                color: fill,
                z_index,
                radius: style.radius.min(rect.width * 0.5).min(rect.height * 0.5),
            });
        }
        return;
    }

    if fill[3] != 0 {
        let outer_radius = style.radius.min(rect.width * 0.5).min(rect.height * 0.5);
        solids.push(UiSurfaceQuad {
            rect,
            clip_rect,
            color: border,
            z_index,
            radius: outer_radius,
        });
        let inner_rect = UiRect::new(
            rect.x + border_width,
            rect.y + border_width,
            (rect.width - border_width * 2.0).max(0.0),
            (rect.height - border_width * 2.0).max(0.0),
        );
        if inner_rect.width > 0.0 && inner_rect.height > 0.0 {
            solids.push(UiSurfaceQuad {
                rect: inner_rect,
                clip_rect,
                color: fill,
                z_index,
                radius: (outer_radius - border_width).max(0.0),
            });
        }
        return;
    }

    let border_z = z_index;
    solids.extend([
        UiSurfaceQuad {
            rect: UiRect::new(rect.x, rect.y, rect.width, border_width),
            clip_rect,
            color: border,
            z_index: border_z,
            radius: 0.0,
        },
        UiSurfaceQuad {
            rect: UiRect::new(
                rect.x,
                rect.bottom() - border_width,
                rect.width,
                border_width,
            ),
            clip_rect,
            color: border,
            z_index: border_z,
            radius: 0.0,
        },
        UiSurfaceQuad {
            rect: UiRect::new(rect.x, rect.y, border_width, rect.height),
            clip_rect,
            color: border,
            z_index: border_z,
            radius: 0.0,
        },
        UiSurfaceQuad {
            rect: UiRect::new(
                rect.right() - border_width,
                rect.y,
                border_width,
                rect.height,
            ),
            clip_rect,
            color: border,
            z_index: border_z,
            radius: 0.0,
        },
    ]);
}

fn apply_opacity(mut color: [u8; 4], opacity: f32) -> [u8; 4] {
    color[3] = ((f32::from(color[3]) * opacity.clamp(0.0, 1.0)).round()) as u8;
    color
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_graphic_basic::ui_surface::UiSurface;
    use raf_ui::{StudioUiPalette, UiNode, UiNodeKind, UiTextStyle};

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
    fn direct_draw_list_emits_fill_and_border_geometry() {
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

        assert_eq!(list.solids.len(), 2);
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
