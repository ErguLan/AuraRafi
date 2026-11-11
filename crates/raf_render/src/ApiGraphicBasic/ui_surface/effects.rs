//! Shared, backend-neutral paint metadata. Legacy quads remain compact.
use raf_ui::{UiImageOptions, UiLinearGradient, UiRect};
use std::collections::HashMap;

pub const MAX_ROUNDED_CLIPS: usize = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiPaintKey {
    Solid(usize),
    Stroke(usize),
    Text(usize),
    Image(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiRoundedClip {
    pub rect: UiRect,
    pub corners: [f32; 4],
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct UiPaintEffects {
    pub corners: Option<[f32; 4]>,
    pub border: [u8; 4],
    pub border_widths: [f32; 4],
    pub gradient: Option<UiLinearGradient>,
    pub clips: Vec<UiRoundedClip>,
    pub softness: f32,
    pub image: Option<UiImageOptions>,
    pub image_scale: Option<[f32; 2]>,
}

pub type UiSurfaceEffects = HashMap<UiPaintKey, UiPaintEffects>;

impl UiPaintEffects {
    pub fn scaled(&self, x: f32, y: f32) -> Self {
        let mut result = self.clone();
        let scale = x.min(y);
        result.corners = result.corners.map(|r| r.map(|v| v * scale));
        result.border_widths = [
            self.border_widths[0] * y,
            self.border_widths[1] * x,
            self.border_widths[2] * y,
            self.border_widths[3] * x,
        ];
        result.softness *= scale;
        result.image_scale = Some([x, y]);
        for clip in &mut result.clips {
            clip.rect = UiRect::new(
                clip.rect.x * x,
                clip.rect.y * y,
                clip.rect.width * x,
                clip.rect.height * y,
            );
            clip.corners = clip.corners.map(|r| r * scale);
        }
        result
    }
}

/// Corner normalization prevents adjacent radii from overlapping.
pub fn normalized_corners(rect: UiRect, corners: [f32; 4]) -> [f32; 4] {
    let r = corners.map(|v| if v.is_finite() { v.max(0.0) } else { 0.0 });
    let mut factor = 1.0_f32;
    for (available, sum) in [
        (rect.width, r[0] + r[1]),
        (rect.width, r[3] + r[2]),
        (rect.height, r[0] + r[3]),
        (rect.height, r[1] + r[2]),
    ] {
        if sum > 0.0 {
            factor = factor.min(available.max(0.0) / sum);
        }
    }
    r.map(|v| v * factor)
}

/// Signed distance in physical or logical pixels. Negative is inside.
pub fn rounded_distance(rect: UiRect, corners: [f32; 4], p: [f32; 2]) -> f32 {
    let r = normalized_corners(rect, corners);
    let center = [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5];
    let local = [p[0] - center[0], p[1] - center[1]];
    let radius = if local[1] < 0.0 {
        if local[0] < 0.0 {
            r[0]
        } else {
            r[1]
        }
    } else if local[0] < 0.0 {
        r[3]
    } else {
        r[2]
    };
    let q = [
        local[0].abs() - rect.width * 0.5 + radius,
        local[1].abs() - rect.height * 0.5 + radius,
    ];
    q[0].max(0.0).hypot(q[1].max(0.0)) + q[0].max(q[1]).min(0.0) - radius
}

pub fn clip_coverage(clips: &[UiRoundedClip], point: [f32; 2]) -> f32 {
    if clips.len() > MAX_ROUNDED_CLIPS {
        return 0.0;
    }
    clips.iter().fold(1.0_f32, |alpha, clip| {
        alpha.min((0.5 - rounded_distance(clip.rect, clip.corners, point)).clamp(0.0, 1.0))
    })
}
