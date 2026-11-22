//! Retained RafUI to ApiGraphicBasic product paint.
//!
//! `DirectUiSurfaceHost` is the retained RafUI producer for ApiGraphicBasic.
//! It compiles the retained document, synchronizes the text atlas, uploads the
//! texture pixels the paint needs, and appends `Command::Paint` and
//! `Command::StyledPaint` in the exact retained paint order.
//!
//! No graphics API type crosses this boundary. Geometry and effects are
//! normalized full-target data; the native executors rasterize the same
//! command stream.

mod style;
mod texture_cache;

#[cfg(test)]
mod tests;

use std::collections::HashMap;

use raf_agb::product::{Command, Paint, PaintQuad, PaintStroke, PaintStyle, ProductPacket};
use raf_agb::Texture;
use raf_ui::UiRect;

use super::effects::UiPaintKey;
use super::images::{canonical_builtin_icon_uri, UiSurfaceImageStore};
use super::{UiSurfaceDrawList, UiSurfacePaintCommand, UiTextAtlas};
use style::{
    build_paint_style, image_uv, rgba, style_has_visible_paint, PaintKind, PaintRef, StyleRequest,
    TargetMap,
};
use texture_cache::UiTextureKey;

pub(crate) use texture_cache::UiTextureCache;

/// Paint produced for one retained UI layer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiPaintMetrics {
    pub solids: u32,
    pub strokes: u32,
    pub text: u32,
    pub images: u32,
    /// Paint commands that needed the shared styled path.
    pub styles: u32,
    /// Rounded clip shapes emitted, including a rounded painted edge.
    pub clips: u32,
    /// RGBA bytes appended for the text atlas texture.
    pub atlas_bytes: u64,
    /// RGBA bytes appended for image textures.
    pub image_bytes: u64,
}

impl UiPaintMetrics {
    pub fn paint_commands(self) -> u32 {
        self.solids
            .saturating_add(self.strokes)
            .saturating_add(self.text)
            .saturating_add(self.images)
    }

    pub fn upload_bytes(self) -> u64 {
        self.atlas_bytes.saturating_add(self.image_bytes)
    }
}

/// Appends one retained draw list to a shared product packet.
///
/// `target_size` is the full physical target, `target_rect` is the physical
/// pixel rectangle where this layer draws, and `logical_size` is the retained
/// layout extent in points. `raster_scale` is the physical density already used
/// to rasterize the text atlas.
#[allow(clippy::too_many_arguments)]
pub(crate) fn append_paint(
    packet: &mut ProductPacket,
    list: &UiSurfaceDrawList,
    atlas: &UiTextAtlas,
    images: &UiSurfaceImageStore,
    cache: &mut UiTextureCache,
    surface: &str,
    target_size: [u32; 2],
    target_rect: [f32; 4],
    logical_size: [u32; 2],
    _raster_scale: f32,
) -> Result<UiPaintMetrics, String> {
    let map = TargetMap::new(target_size, target_rect, logical_size)?;
    let atlas_height = active_atlas_height(list, atlas);
    let mut context = PaintContext {
        packet,
        list,
        images,
        cache,
        map,
        atlas,
        atlas_height,
        surface,
        textures: HashMap::new(),
        metrics: UiPaintMetrics::default(),
    };
    for command in list.paint_commands().iter() {
        match *command {
            UiSurfacePaintCommand::Solid { index, .. } => context.solid(index)?,
            UiSurfacePaintCommand::Stroke { index, .. } => context.stroke(index)?,
            UiSurfacePaintCommand::Text { index, .. } => context.text(index)?,
            UiSurfacePaintCommand::Image { index, .. } => context.image(index)?,
        }
    }
    Ok(context.metrics)
}

struct PaintContext<'a, 'b> {
    packet: &'a mut ProductPacket,
    list: &'b UiSurfaceDrawList,
    images: &'b UiSurfaceImageStore,
    cache: &'a mut UiTextureCache,
    map: TargetMap,
    atlas: &'b UiTextAtlas,
    atlas_height: u32,
    surface: &'b str,
    /// Packet texture index for every key already appended to this packet.
    textures: HashMap<UiTextureKey, u32>,
    metrics: UiPaintMetrics,
}

impl PaintContext<'_, '_> {
    fn solid(&mut self, index: usize) -> Result<(), String> {
        let Some(quad) = self.list.solids.get(index) else {
            return Err(self.missing(PaintKind::Solid, index));
        };
        if quad.rect.is_empty() || quad.clip_rect.is_empty() {
            return Ok(());
        }
        if !valid_rect(quad.rect) || !valid_rect(quad.clip_rect) {
            return Err(self
                .reference(PaintKind::Solid, index)
                .error("has a non-finite rectangle"));
        }
        let effects = self.list.effects.get(&UiPaintKey::Solid(index));
        let style = build_paint_style(
            &self.map,
            &StyleRequest {
                reference: self.reference(PaintKind::Solid, index),
                rect: quad.rect,
                fallback_radius: quad.radius,
                effects,
                image_fit: None,
            },
        )?;
        if quad.color[3] == 0 && !style_has_visible_paint(style.as_ref()) {
            return Ok(());
        }
        let paint = Paint::Solid(PaintQuad {
            rect: self.map.rect(quad.rect),
            clip: self.map.rect(quad.clip_rect),
            uv_rect: [0.0, 0.0, 1.0, 1.0],
            color: rgba(quad.color),
        });
        self.push(paint, style);
        self.metrics.solids = self.metrics.solids.saturating_add(1);
        Ok(())
    }

    fn stroke(&mut self, index: usize) -> Result<(), String> {
        let Some(stroke) = self.list.strokes.get(index) else {
            return Err(self.missing(PaintKind::Stroke, index));
        };
        if stroke.clip_rect.is_empty() || stroke.color[3] == 0 || stroke.width <= 0.0 {
            return Ok(());
        }
        if !valid_rect(stroke.clip_rect)
            || !stroke.start.iter().chain(stroke.end.iter()).all(|v| v.is_finite())
            || !stroke.width.is_finite()
        {
            return Err(self
                .reference(PaintKind::Stroke, index)
                .error("has non-finite geometry"));
        }
        if stroke.start == stroke.end {
            return Ok(());
        }
        let effects = self.list.effects.get(&UiPaintKey::Stroke(index));
        let style = build_paint_style(
            &self.map,
            &StyleRequest {
                reference: self.reference(PaintKind::Stroke, index),
                rect: UiRect::new(stroke.start[0], stroke.start[1], 0.0, 0.0),
                fallback_radius: 0.0,
                effects,
                image_fit: None,
            },
        )?;
        let paint = Paint::Stroke(PaintStroke {
            start: self.map.point(stroke.start),
            end: self.map.point(stroke.end),
            width: [self.map.nx(stroke.width), self.map.ny(stroke.width)],
            clip: self.map.rect(stroke.clip_rect),
            color: rgba(stroke.color),
        });
        self.push(paint, style);
        self.metrics.strokes = self.metrics.strokes.saturating_add(1);
        Ok(())
    }

    fn text(&mut self, index: usize) -> Result<(), String> {
        let Some(quad) = self.list.text.get(index) else {
            return Err(self.missing(PaintKind::Text, index));
        };
        if quad.rect.is_empty()
            || quad.clip_rect.is_empty()
            || quad.atlas_rect.is_empty()
            || quad.color[3] == 0
        {
            return Ok(());
        }
        if !valid_rect(quad.rect) || !valid_rect(quad.clip_rect) || !valid_rect(quad.atlas_rect) {
            return Err(self
                .reference(PaintKind::Text, index)
                .error("has a non-finite rectangle"));
        }
        let atlas_size = self.atlas.size();
        let width = f32::from(atlas_size[0].max(1));
        let height = self.atlas_height as f32;
        if quad.atlas_rect.x < 0.0 || quad.atlas_rect.y < 0.0 {
            return Err(self
                .reference(PaintKind::Text, index)
                .error("has a glyph rectangle outside the live atlas generation"));
        }
        if quad.atlas_rect.bottom() > height + 1.0 || quad.atlas_rect.right() > width + 1.0 {
            return Err(self
                .reference(PaintKind::Text, index)
                .error("has a glyph rectangle outside the live atlas generation"));
        }
        let atlas_index = self.text_atlas_texture(atlas_size)?;
        let uv = [
            (quad.atlas_rect.x / width).clamp(0.0, 1.0),
            (quad.atlas_rect.y / height).clamp(0.0, 1.0),
            (quad.atlas_rect.width / width).clamp(0.0, 1.0),
            (quad.atlas_rect.height / height).clamp(0.0, 1.0),
        ];
        let effects = self.list.effects.get(&UiPaintKey::Text(index));
        let style = build_paint_style(
            &self.map,
            &StyleRequest {
                reference: self.reference(PaintKind::Text, index),
                rect: quad.rect,
                fallback_radius: 0.0,
                effects,
                image_fit: None,
            },
        )?;
        let paint = Paint::Text {
            quad: PaintQuad {
                rect: self.map.rect(quad.rect),
                clip: self.map.rect(quad.clip_rect),
                uv_rect: uv,
                color: rgba(quad.color),
            },
            atlas: atlas_index,
        };
        self.push(paint, style);
        self.metrics.text = self.metrics.text.saturating_add(1);
        Ok(())
    }

    fn image(&mut self, index: usize) -> Result<(), String> {
        let Some(quad) = self.list.images.get(index) else {
            return Err(self.missing(PaintKind::Image, index));
        };
        if quad.rect.is_empty() || quad.clip_rect.is_empty() || quad.tint[3] == 0 {
            return Ok(());
        }
        if !valid_rect(quad.rect) || !valid_rect(quad.clip_rect) {
            return Err(self
                .reference(PaintKind::Image, index)
                .error("has a non-finite rectangle"));
        }
        let source = canonical_builtin_icon_uri(&quad.source_key).into_owned();
        let Some((pixels, size, revision)) = self.images.resolved_pixels(&source) else {
            // An unresolved image has no pixels in this surface's store. The
            // retained placeholder already paints its own status affordance.
            return Ok(());
        };
        let effects = self.list.effects.get(&UiPaintKey::Image(index));
        let style = build_paint_style(
            &self.map,
            &StyleRequest {
                reference: self.reference(PaintKind::Image, index),
                rect: quad.rect,
                fallback_radius: 0.0,
                effects,
                image_fit: Some(quad.fit),
            },
        )?;
        let Some(style) = style else {
            return Err(self
                .reference(PaintKind::Image, index)
                .error("lost its image layout"));
        };
        let uv_rect = image_uv(effects.and_then(|effects| effects.image.as_ref()));
        let key = UiTextureKey::Image {
            source: source.clone(),
            revision,
        };
        let texture = match self.textures.get(&key) {
            Some(index) => *index,
            None => {
                let texture = self.cache.image(&source, size, revision, pixels).clone();
                let bytes = texture.rgba.len() as u64;
                let index = push_texture(&mut *self.packet, &key, texture, &mut self.textures)?;
                self.metrics.image_bytes = self.metrics.image_bytes.saturating_add(bytes);
                index
            }
        };
        let paint = Paint::Image {
            quad: PaintQuad {
                rect: self.map.rect(quad.rect),
                clip: self.map.rect(quad.clip_rect),
                uv_rect,
                color: rgba(quad.tint),
            },
            texture,
        };
        self.push(paint, Some(style));
        self.metrics.images = self.metrics.images.saturating_add(1);
        Ok(())
    }

    fn text_atlas_texture(&mut self, atlas_size: [u16; 2]) -> Result<u32, String> {
        let key = UiTextureKey::Atlas {
            revision: self.atlas.revision(),
            size: [u32::from(atlas_size[0]), u32::from(atlas_size[1])],
            height: self.atlas_height,
        };
        if let Some(index) = self.textures.get(&key) {
            return Ok(*index);
        }
        let texture = self.cache.atlas(self.atlas, self.atlas_height).clone();
        let bytes = texture.rgba.len() as u64;
        let index = push_texture(&mut *self.packet, &key, texture, &mut self.textures)?;
        self.metrics.atlas_bytes = self.metrics.atlas_bytes.saturating_add(bytes);
        Ok(index)
    }

    fn push(&mut self, paint: Paint, style: Option<PaintStyle>) {
        match style {
            Some(style) => {
                self.metrics.clips = self.metrics.clips.saturating_add(rounded_shapes(&style));
                self.metrics.styles = self.metrics.styles.saturating_add(1);
                self.packet.commands.push(Command::StyledPaint {
                    paint,
                    style: Box::new(style),
                });
            }
            None => self.packet.commands.push(Command::Paint(paint)),
        }
    }

    fn reference(&self, kind: PaintKind, index: usize) -> PaintRef<'_> {
        PaintRef {
            surface: self.surface,
            kind,
            index,
        }
    }

    fn missing(&self, kind: PaintKind, index: usize) -> String {
        self.reference(kind, index)
            .error("references a draw-list entry that does not exist")
    }
}

fn push_texture(
    packet: &mut ProductPacket,
    key: &UiTextureKey,
    texture: Texture,
    textures: &mut HashMap<UiTextureKey, u32>,
) -> Result<u32, String> {
    let index = u32::try_from(packet.textures.len())
        .map_err(|_| "RafUI paint needs more textures than ApiGraphicBasic can index".to_string())?;
    packet.textures.push(texture);
    textures.insert(key.clone(), index);
    Ok(index)
}

fn rounded_shapes(style: &PaintStyle) -> u32 {
    let own = style
        .radii
        .iter()
        .flatten()
        .any(|radius| *radius > 0.0);
    let declared = u32::try_from(style.clips.len()).unwrap_or(u32::MAX);
    declared.saturating_add(u32::from(own))
}

fn active_atlas_height(list: &UiSurfaceDrawList, atlas: &UiTextAtlas) -> u32 {
    let used = list
        .text
        .iter()
        .filter(|quad| valid_rect(quad.atlas_rect))
        .map(|quad| quad.atlas_rect.bottom())
        .fold(0.0_f32, f32::max)
        .ceil()
        .max(0.0) as u32;
    used.min(u32::from(atlas.size()[1].max(1))).max(1)
}

fn valid_rect(rect: UiRect) -> bool {
    rect.x.is_finite()
        && rect.y.is_finite()
        && rect.width.is_finite()
        && rect.height.is_finite()
        && rect.width >= 0.0
        && rect.height >= 0.0
}
