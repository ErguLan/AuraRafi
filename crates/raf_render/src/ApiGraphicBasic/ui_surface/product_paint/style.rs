//! Retained RafUI paint mapped onto the shared ApiGraphicBasic paint description.
//!
//! The retained draw list keeps logical-point geometry and effect data. This
//! module owns the only translation between that data and
//! `raf_agb::product::{PaintStyle, PaintQuad, PaintStroke}`: per-corner radii,
//! per-side border widths, linear gradients, rounded clip stacks, feather,
//! image fit and nine-slice.
//!
//! Every value leaves this module in normalized full-target units, with
//! horizontal and vertical axes converted separately because the normalized
//! space is not square.

use raf_agb::product::{
    ImageFit as AgbImageFit, ImageLayout, LinearGradient, NineSlice, PaintBorder, PaintStyle,
    RoundedClip, MAX_DECLARED_CLIPS,
};
use raf_ui::{UiImageFit, UiImageOptions, UiRect};

use super::super::effects::{UiPaintEffects, UiRoundedClip};

/// Rectangles closer than this in logical points describe the same edge.
const EDGE_TOLERANCE: f32 = 0.01;

/// What kind of retained paint is being described.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PaintKind {
    Solid,
    Stroke,
    Text,
    Image,
}

impl PaintKind {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Solid => "solid",
            Self::Stroke => "stroke",
            Self::Text => "text",
            Self::Image => "image",
        }
    }
}

/// Identifies one paint command for actionable diagnostics.
#[derive(Debug, Clone, Copy)]
pub(super) struct PaintRef<'a> {
    pub(super) surface: &'a str,
    pub(super) kind: PaintKind,
    pub(super) index: usize,
}

impl PaintRef<'_> {
    pub(super) fn error(&self, message: impl std::fmt::Display) -> String {
        format!(
            "RafUI surface '{}': {} paint #{} {message}",
            self.surface,
            self.kind.label(),
            self.index
        )
    }
}

/// One paint request in logical coordinates.
#[derive(Debug, Clone, Copy)]
pub(super) struct StyleRequest<'a> {
    pub(super) reference: PaintRef<'a>,
    /// Shape rectangle in logical points. Used for corner and border geometry.
    pub(super) rect: UiRect,
    /// Uniform fallback radius from the compact quad.
    pub(super) fallback_radius: f32,
    pub(super) effects: Option<&'a UiPaintEffects>,
    /// Image fitting policy from the draw list entry.
    pub(super) image_fit: Option<UiImageFit>,
}

/// Converts logical layer geometry into normalized full-target coordinates.
#[derive(Debug, Clone, Copy)]
pub(super) struct TargetMap {
    target: [f32; 2],
    target_extent: [u32; 2],
    origin: [f32; 2],
    scale: [f32; 2],
}

impl TargetMap {
    pub(super) fn new(
        target_size: [u32; 2],
        target_rect: [f32; 4],
        logical_size: [u32; 2],
    ) -> Result<Self, String> {
        if target_size.contains(&0) {
            return Err("RafUI paint target has a zero axis".to_string());
        }
        if !target_rect.iter().all(|value| value.is_finite())
            || target_rect[2] < 0.0
            || target_rect[3] < 0.0
        {
            return Err(format!(
                "RafUI paint layer rect {target_rect:?} is not a finite positive rectangle"
            ));
        }
        let logical = [
            logical_size[0].max(1) as f32,
            logical_size[1].max(1) as f32,
        ];
        Ok(Self {
            target: [target_size[0] as f32, target_size[1] as f32],
            target_extent: target_size,
            origin: [target_rect[0], target_rect[1]],
            scale: [target_rect[2] / logical[0], target_rect[3] / logical[1]],
        })
    }

    /// Normalized top-left `[x, y, width, height]`.
    pub(super) fn rect(&self, rect: UiRect) -> [f32; 4] {
        [
            (self.origin[0] + rect.x * self.scale[0]) / self.target[0],
            (self.origin[1] + rect.y * self.scale[1]) / self.target[1],
            rect.width * self.scale[0] / self.target[0],
            rect.height * self.scale[1] / self.target[1],
        ]
    }

    pub(super) fn point(&self, point: [f32; 2]) -> [f32; 2] {
        [
            (self.origin[0] + point[0] * self.scale[0]) / self.target[0],
            (self.origin[1] + point[1] * self.scale[1]) / self.target[1],
        ]
    }

    /// Horizontal extent in normalized target units.
    pub(super) fn nx(&self, points: f32) -> f32 {
        points * self.scale[0] / self.target[0]
    }

    /// Vertical extent in normalized target units.
    pub(super) fn ny(&self, points: f32) -> f32 {
        points * self.scale[1] / self.target[1]
    }

    pub(super) fn target_extent(&self) -> [u32; 2] {
        self.target_extent
    }
}

/// Builds the shared paint style for one draw-list entry.
///
/// Returns `None` when the effect data adds nothing beyond the plain quad, so
/// the caller can emit `Paint` instead of `StyledPaint`.
pub(super) fn build_paint_style(
    map: &TargetMap,
    request: &StyleRequest<'_>,
) -> Result<Option<PaintStyle>, String> {
    let empty = UiPaintEffects::default();
    let effects = request.effects.unwrap_or(&empty);
    let (declared, corners) = split_clips(effects, request.rect, request.reference.kind, request.fallback_radius);
    if declared.len() > MAX_DECLARED_CLIPS {
        return Err(request.reference.error(format!(
            "declares {} nested rounded clips; ApiGraphicBasic accepts {MAX_DECLARED_CLIPS} declared clips plus the painted edge",
            declared.len()
        )));
    }
    let radii = corner_radii(map, corners);
    let border = border_geometry(map, effects);
    if request.reference.kind == PaintKind::Stroke && (radii != [[0.0; 2]; 4] || border.is_some()) {
        return Err(request.reference.error(
            "cannot carry corner radii or per-side borders; ApiGraphicBasic rejects them on strokes",
        ));
    }
    if request.reference.kind != PaintKind::Image && effects.image.is_some() {
        return Err(request.reference.error(
            "carries an image layout on a non-image paint",
        ));
    }
    let gradient = match effects.gradient {
        Some(gradient) => {
            if !gradient.direction.iter().all(|value| value.is_finite()) {
                return Err(request.reference.error("has a non-finite gradient direction"));
            }
            Some(LinearGradient {
                direction: gradient.direction,
                start: rgba(gradient.start),
                end: rgba(gradient.end),
            })
        }
        None => None,
    };
    let softness = if effects.softness.is_finite() && effects.softness > 0.0 {
        let horizontal = map.nx(effects.softness);
        let vertical = map.ny(effects.softness);
        if horizontal > 0.0 || vertical > 0.0 {
            [horizontal, vertical]
        } else {
            [0.0; 2]
        }
    } else {
        [0.0; 2]
    };
    let image = if request.reference.kind == PaintKind::Image {
        Some(image_layout(map, effects.image.as_ref(), request.image_fit))
    } else {
        None
    };
    if request.reference.kind == PaintKind::Image && softness != [0.0; 2] {
        let fitted = request.image_fit.unwrap_or(UiImageFit::Stretch) != UiImageFit::Stretch
            || effects
                .image
                .as_ref()
                .is_some_and(|options| options.nine_slice.is_some());
        if fitted {
            return Err(request.reference.error(
                "combines feather with a fitted or sliced image; ApiGraphicBasic rejects that combination",
            ));
        }
    }
    let style = PaintStyle {
        radii,
        border,
        gradient,
        clips: declared
            .iter()
            .map(|clip| rounded_clip(map, clip))
            .collect(),
        softness,
        image,
    };
    Ok((!style.is_plain()).then_some(style))
}

/// Normalized crop (`uv_rect`) for an image paint.
pub(super) fn image_uv(options: Option<&UiImageOptions>) -> [f32; 4] {
    let Some(rect) = options.and_then(|options| options.source_rect) else {
        return [0.0, 0.0, 1.0, 1.0];
    };
    let x = finite(rect.x, 0.0).clamp(0.0, 1.0);
    let y = finite(rect.y, 0.0).clamp(0.0, 1.0);
    [
        x,
        y,
        finite(rect.width, 0.0).clamp(0.0, 1.0 - x),
        finite(rect.height, 0.0).clamp(0.0, 1.0 - y),
    ]
}

/// True when a style would paint nothing at all.
pub(super) fn style_has_visible_paint(style: Option<&PaintStyle>) -> bool {
    style.is_some_and(|style| {
        style.border.is_some_and(|border| border.is_visible())
            || style.gradient.is_some()
            || style.image.is_some()
            || style.softness != [0.0; 2]
    })
}

fn split_clips<'a>(
    effects: &'a UiPaintEffects,
    rect: UiRect,
    kind: PaintKind,
    fallback_radius: f32,
) -> (&'a [UiRoundedClip], [f32; 4]) {
    let mut corners = effects.corners.unwrap_or([fallback_radius; 4]);
    let mut clips = effects.clips.as_slice();
    if kind == PaintKind::Image {
        // The retained painter appends the image's own rounded edge after the
        // ancestor masks. It becomes the styled shape so the declared stack
        // stays inside the shared nine-shape limit.
        if let Some((own, ancestors)) = clips.split_last() {
            if rect_near(own.rect, rect) {
                if effects.corners.is_none() {
                    corners = own.corners;
                }
                clips = ancestors;
            }
        }
    }
    (clips, corners)
}

fn corner_radii(map: &TargetMap, corners: [f32; 4]) -> [[f32; 2]; 4] {
    corners.map(|radius| {
        let radius = finite(radius, 0.0).max(0.0);
        [map.nx(radius), map.ny(radius)]
    })
}

fn border_geometry(map: &TargetMap, effects: &UiPaintEffects) -> Option<PaintBorder> {
    if effects.border[3] == 0 || !effects.border_widths.iter().any(|width| *width > 0.0) {
        return None;
    }
    let [top, right, bottom, left] = effects.border_widths.map(|width| finite(width, 0.0).max(0.0));
    Some(PaintBorder {
        widths: [map.ny(top), map.nx(right), map.ny(bottom), map.nx(left)],
        color: rgba(effects.border),
        sides: [None; 4],
    })
}

fn rounded_clip(map: &TargetMap, clip: &UiRoundedClip) -> RoundedClip {
    RoundedClip {
        rect: map.rect(clip.rect),
        radii: corner_radii(map, clip.corners),
    }
}

fn image_layout(
    map: &TargetMap,
    options: Option<&UiImageOptions>,
    fit: Option<UiImageFit>,
) -> ImageLayout {
    let default = UiImageOptions::default();
    let options = options.unwrap_or(&default);
    let fit = match fit.unwrap_or(UiImageFit::Stretch) {
        UiImageFit::Contain => AgbImageFit::Contain,
        UiImageFit::Cover => AgbImageFit::Cover,
        UiImageFit::Stretch => AgbImageFit::Stretch,
    };
    let focal_point = [
        finite(options.focal_point[0], 0.5).clamp(0.0, 1.0),
        finite(options.focal_point[1], 0.5).clamp(0.0, 1.0),
    ];
    let nine_slice = options.nine_slice.map(|insets| NineSlice {
        source: [
            finite(insets.top, 0.0).max(0.0),
            finite(insets.right, 0.0).max(0.0),
            finite(insets.bottom, 0.0).max(0.0),
            finite(insets.left, 0.0).max(0.0),
        ],
        target: [
            map.ny(finite(insets.top, 0.0).max(0.0)),
            map.nx(finite(insets.right, 0.0).max(0.0)),
            map.ny(finite(insets.bottom, 0.0).max(0.0)),
            map.nx(finite(insets.left, 0.0).max(0.0)),
        ],
    });
    ImageLayout {
        fit,
        target_extent: map.target_extent(),
        focal_point,
        nine_slice,
    }
}

fn rect_near(left: UiRect, right: UiRect) -> bool {
    (left.x - right.x).abs() <= EDGE_TOLERANCE
        && (left.y - right.y).abs() <= EDGE_TOLERANCE
        && (left.width - right.width).abs() <= EDGE_TOLERANCE
        && (left.height - right.height).abs() <= EDGE_TOLERANCE
}

fn finite(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

/// Straight-alpha byte color. ApiGraphicBasic consumes the same normalized
/// values producers author, so no implicit color-space conversion happens here.
pub(super) fn rgba(color: [u8; 4]) -> [f32; 4] {
    color.map(|channel| f32::from(channel) / 255.0)
}
