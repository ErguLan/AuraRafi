//! Software compositor for retained UI surfaces.
//!
//! The normal editor path presents through WGPU. This renderer is a compact
//! recovery path for the same retained draw list, so low-end or incompatible
//! systems do not need a separate UI implementation.

use super::effects::{clip_coverage, rounded_distance, UiPaintEffects, UiPaintKey};
use super::{
    UiRect, UiSurfaceDrawList, UiSurfaceImageQuad, UiSurfaceImageStore, UiSurfacePaintCommand,
    UiSurfaceStroke, UiTextAtlas,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiSurfaceCpuMetrics {
    pub solid_pixels: u64,
    pub stroke_pixels: u64,
    pub text_pixels: u64,
    pub image_pixels: u64,
    /// Pixels written by translucent solid quads. This is measured from the
    /// same draw list used by the GPU presenter.
    pub translucent_solid_pixels: u64,
}

/// Reusable CPU RGBA compositor for `UiSurfaceDrawList`.
///
/// Pixels are stored as straight-alpha RGBA to match the existing
/// `ApiGraphicBasic` software framebuffer contract.
#[derive(Debug, Default)]
pub struct UiSurfaceCpuRenderer {
    pixels: Vec<u8>,
    size: [u32; 2],
}

impl UiSurfaceCpuRenderer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn size(&self) -> [u32; 2] {
        self.size
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn render(
        &mut self,
        draw_list: &UiSurfaceDrawList,
        atlas: &UiTextAtlas,
        images: &UiSurfaceImageStore,
        size: [u32; 2],
        clear_color: [u8; 4],
    ) -> UiSurfaceCpuMetrics {
        let width = size[0].max(1);
        let height = size[1].max(1);
        self.size = [width, height];
        self.pixels.resize(width as usize * height as usize * 4, 0);
        for pixel in self.pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&clear_color);
        }

        let mut metrics = UiSurfaceCpuMetrics::default();
        let commands = draw_list.paint_commands();
        for command in commands.iter() {
            match *command {
                UiSurfacePaintCommand::Solid { index, .. } => {
                    if let Some(quad) = draw_list.solids.get(index) {
                        let written = self.draw_solid(
                            quad.rect,
                            quad.clip_rect,
                            quad.radius,
                            quad.color,
                            draw_list.effects.get(&UiPaintKey::Solid(index)),
                        );
                        metrics.solid_pixels += written;
                        if quad.color[3] > 0 && quad.color[3] < 255 {
                            metrics.translucent_solid_pixels += written;
                        }
                    }
                }
                UiSurfacePaintCommand::Stroke { index, .. } => {
                    if let Some(stroke) = draw_list.strokes.get(index) {
                        metrics.stroke_pixels += self
                            .draw_stroke(stroke, draw_list.effects.get(&UiPaintKey::Stroke(index)));
                    }
                }
                UiSurfacePaintCommand::Text { index, .. } => {
                    if let Some(quad) = draw_list.text.get(index) {
                        metrics.text_pixels += self.draw_text(
                            quad,
                            atlas,
                            draw_list.effects.get(&UiPaintKey::Text(index)),
                        );
                    }
                }
                UiSurfacePaintCommand::Image { index, .. } => {
                    if let Some(quad) = draw_list.images.get(index) {
                        if let Some(image) = images.get(&quad.source_key) {
                            metrics.image_pixels += self.draw_image(
                                quad,
                                image,
                                draw_list.effects.get(&UiPaintKey::Image(index)),
                            );
                        } else {
                            metrics.image_pixels += self.draw_missing_image(
                                quad,
                                draw_list.effects.get(&UiPaintKey::Image(index)),
                                images.resource_status(&quad.source_key),
                            );
                        }
                    }
                }
            }
        }
        metrics
    }

    fn draw_solid(
        &mut self,
        rect: UiRect,
        clip_rect: UiRect,
        radius: f32,
        color: [u8; 4],
        effects: Option<&UiPaintEffects>,
    ) -> u64 {
        let padding = effects.map_or(0.0, |effect| effect.softness.max(0.0) * 0.5) + 1.0;
        let paint_bounds = UiRect::new(
            rect.x - padding,
            rect.y - padding,
            rect.width + 2.0 * padding,
            rect.height + 2.0 * padding,
        );
        let Some(bounds) = pixel_bounds(paint_bounds.intersection(clip_rect), self.size) else {
            return 0;
        };
        let mut written = 0;
        for y in bounds.y_start..bounds.y_end {
            for x in bounds.x_start..bounds.x_end {
                let point = [x as f32 + 0.5, y as f32 + 0.5];
                let corners = effects.and_then(|e| e.corners).unwrap_or([radius; 4]);
                let softness = effects.map_or(0.0, |e| e.softness).max(1.0);
                let coverage = (0.5 - rounded_distance(rect, corners, point) / softness)
                    .clamp(0.0, 1.0)
                    * effects.map_or(1.0, |e| clip_coverage(&e.clips, point));
                if coverage <= 0.0 {
                    continue;
                }
                let mut fill = color;
                if let Some(g) = effects.and_then(|e| e.gradient) {
                    let d = g.direction;
                    let denom = d[0].abs() + d[1].abs();
                    let p = [
                        (point[0] - rect.x) / rect.width.max(1.0),
                        (point[1] - rect.y) / rect.height.max(1.0),
                    ];
                    let t = if denom > 0.0 {
                        ((p[0] - 0.5) * d[0] + (p[1] - 0.5) * d[1]) / denom + 0.5
                    } else {
                        0.0
                    };
                    for c in 0..4 {
                        fill[c] = (f32::from(g.start[c]) * (1.0 - t.clamp(0.0, 1.0))
                            + f32::from(g.end[c]) * t.clamp(0.0, 1.0))
                        .round() as u8;
                    }
                }
                let mut wrote = false;
                if let Some(e) =
                    effects.filter(|e| e.border[3] > 0 && e.border_widths.iter().any(|w| *w > 0.0))
                {
                    let w = e.border_widths.map(|v| v.max(0.0));
                    let inner = UiRect::new(
                        rect.x + w[3],
                        rect.y + w[0],
                        (rect.width - w[1] - w[3]).max(0.0),
                        (rect.height - w[0] - w[2]).max(0.0),
                    );
                    let inner_r = [
                        (corners[0] - w[0].max(w[3])).max(0.0),
                        (corners[1] - w[0].max(w[1])).max(0.0),
                        (corners[2] - w[2].max(w[1])).max(0.0),
                        (corners[3] - w[2].max(w[3])).max(0.0),
                    ];
                    let inside = if inner.is_empty() {
                        0.0
                    } else {
                        (0.5 - rounded_distance(inner, inner_r, point)).clamp(0.0, 1.0)
                    };
                    let alpha =
                        f32::from(e.border[3]) * (1.0 - inside) + f32::from(fill[3]) * inside;
                    let mut mixed = [0; 4];
                    mixed[3] = alpha.round() as u8;
                    if alpha > 0.0 {
                        for c in 0..3 {
                            mixed[c] =
                                ((f32::from(e.border[c]) * f32::from(e.border[3]) * (1.0 - inside)
                                    + f32::from(fill[c]) * f32::from(fill[3]) * inside)
                                    / alpha)
                                    .round() as u8;
                        }
                    }
                    wrote |= self.blend_pixel(x, y, mixed, (coverage * 255.0).round() as u8);
                } else {
                    wrote |= self.blend_pixel(x, y, fill, (coverage * 255.0).round() as u8);
                }
                if wrote {
                    written += 1;
                }
            }
        }
        written
    }

    fn draw_stroke(&mut self, stroke: &UiSurfaceStroke, effects: Option<&UiPaintEffects>) -> u64 {
        let half_width = stroke.width.max(0.0) * 0.5;
        if half_width <= 0.0 {
            return 0;
        }
        let bounds_rect = UiRect::new(
            stroke.start[0].min(stroke.end[0]) - half_width - 1.0,
            stroke.start[1].min(stroke.end[1]) - half_width - 1.0,
            (stroke.start[0].max(stroke.end[0]) - stroke.start[0].min(stroke.end[0]))
                + (half_width + 1.0) * 2.0,
            (stroke.start[1].max(stroke.end[1]) - stroke.start[1].min(stroke.end[1]))
                + (half_width + 1.0) * 2.0,
        );
        let Some(bounds) = pixel_bounds(bounds_rect.intersection(stroke.clip_rect), self.size)
        else {
            return 0;
        };

        let dx = stroke.end[0] - stroke.start[0];
        let dy = stroke.end[1] - stroke.start[1];
        let length_squared = dx * dx + dy * dy;
        let mut written = 0;
        for y in bounds.y_start..bounds.y_end {
            for x in bounds.x_start..bounds.x_end {
                let point = [x as f32 + 0.5, y as f32 + 0.5];
                let t = if length_squared > f32::EPSILON {
                    ((point[0] - stroke.start[0]) * dx + (point[1] - stroke.start[1]) * dy)
                        / length_squared
                } else {
                    0.0
                }
                .clamp(0.0, 1.0);
                let closest = [stroke.start[0] + dx * t, stroke.start[1] + dy * t];
                let distance =
                    ((point[0] - closest[0]).powi(2) + (point[1] - closest[1]).powi(2)).sqrt();
                // A small analytic coverage ramp keeps the CPU recovery path
                // from turning diagonal ticks into hard stair steps.
                let coverage = ((half_width + 0.75 - distance) / 1.5).clamp(0.0, 1.0)
                    * effects.map_or(1.0, |e| clip_coverage(&e.clips, point));
                if coverage <= 0.0 {
                    continue;
                }
                if self.blend_pixel(x, y, stroke.color, (coverage * 255.0).round() as u8) {
                    written += 1;
                }
            }
        }
        written
    }

    fn draw_text(
        &mut self,
        quad: &super::UiSurfaceTextQuad,
        atlas: &UiTextAtlas,
        effects: Option<&UiPaintEffects>,
    ) -> u64 {
        let Some(bounds) = pixel_bounds(quad.rect.intersection(quad.clip_rect), self.size) else {
            return 0;
        };
        let atlas_size = atlas.size();
        let atlas_width = usize::from(atlas_size[0].max(1));
        let atlas_height = usize::from(atlas_size[1].max(1));
        let atlas_pixels = atlas.pixels();
        if quad.rect.width <= 0.0 || quad.rect.height <= 0.0 {
            return 0;
        }

        let mut written = 0;
        for y in bounds.y_start..bounds.y_end {
            let local_y = ((y as f32 + 0.5 - quad.rect.y) / quad.rect.height).clamp(0.0, 0.999_999);
            for x in bounds.x_start..bounds.x_end {
                let local_x =
                    ((x as f32 + 0.5 - quad.rect.x) / quad.rect.width).clamp(0.0, 0.999_999);
                let coverage = sample_alpha(
                    atlas_pixels,
                    atlas_width,
                    atlas_height,
                    quad.atlas_rect,
                    local_x,
                    local_y,
                );
                let coverage = (f32::from(coverage)
                    * effects.map_or(1.0, |e| {
                        clip_coverage(&e.clips, [x as f32 + 0.5, y as f32 + 0.5])
                    }))
                .round() as u8;
                if self.blend_pixel(x, y, quad.color, coverage) {
                    written += 1;
                }
            }
        }
        written
    }

    fn draw_image(
        &mut self,
        quad: &UiSurfaceImageQuad,
        image: &super::UiSurfaceImageData,
        effects: Option<&UiPaintEffects>,
    ) -> u64 {
        let options = effects
            .and_then(|e| e.image.as_ref())
            .cloned()
            .unwrap_or_default();
        let scale = effects.and_then(|e| e.image_scale).unwrap_or([1.0, 1.0]);
        let logical = UiRect::new(
            quad.rect.x / scale[0],
            quad.rect.y / scale[1],
            quad.rect.width / scale[0],
            quad.rect.height / scale[1],
        );
        let patches = super::image_layout::image_patches(logical, image.size, quad.fit, &options);
        let mut written = 0;
        for patch in patches {
            let destination = UiRect::new(
                patch.destination.x * scale[0],
                patch.destination.y * scale[1],
                patch.destination.width * scale[0],
                patch.destination.height * scale[1],
            );
            let Some(bounds) = pixel_bounds(
                destination
                    .intersection(quad.rect)
                    .intersection(quad.clip_rect),
                self.size,
            ) else {
                continue;
            };
            if destination.is_empty() || image.size[0] == 0 || image.size[1] == 0 {
                continue;
            }
            for y in bounds.y_start..bounds.y_end {
                let local_y =
                    ((y as f32 + 0.5 - destination.y) / destination.height).clamp(0.0, 0.999_999);
                for x in bounds.x_start..bounds.x_end {
                    let local_x = ((x as f32 + 0.5 - destination.x) / destination.width)
                        .clamp(0.0, 0.999_999);
                    let u = patch.uv.x + local_x * patch.uv.width;
                    let v = patch.uv.y + local_y * patch.uv.height;
                    let nearest = options.sampling == raf_ui::UiImageSampling::Nearest
                        || (options.sampling == raf_ui::UiImageSampling::Auto
                            && quad.source_key.starts_with("builtin://icon/"));
                    let mut color = if nearest {
                        sample_rgba_nearest(&image.pixels, image.size, u, v)
                    } else {
                        sample_rgba(&image.pixels, image.size, u, v)
                    };
                    for channel in 0..4 {
                        color[channel] = ((u16::from(color[channel])
                            * u16::from(quad.tint[channel]))
                            / 255) as u8;
                    }
                    let coverage = effects.map_or(1.0, |e| {
                        clip_coverage(&e.clips, [x as f32 + 0.5, y as f32 + 0.5])
                    });
                    if self.blend_pixel(x, y, color, (coverage * 255.0).round() as u8) {
                        written += 1;
                    }
                }
            }
        }
        written
    }

    fn draw_missing_image(
        &mut self,
        quad: &UiSurfaceImageQuad,
        effects: Option<&UiPaintEffects>,
        status: raf_ui::UiImageResourceStatus,
    ) -> u64 {
        let mut placeholder = effects
            .and_then(|e| e.image.as_ref())
            .map(|i| i.placeholder)
            .unwrap_or_default();
        if placeholder == raf_ui::UiImagePlaceholder::Automatic
            && status == raf_ui::UiImageResourceStatus::Pending
        {
            placeholder = raf_ui::UiImagePlaceholder::Skeleton;
        }
        if placeholder == raf_ui::UiImagePlaceholder::Hidden {
            return 0;
        }
        let Some(bounds) = pixel_bounds(quad.rect.intersection(quad.clip_rect), self.size) else {
            return 0;
        };
        let mut written = 0;
        for y in bounds.y_start..bounds.y_end {
            for x in bounds.x_start..bounds.x_end {
                let p = [x as f32 + 0.5, y as f32 + 0.5];
                let coverage = effects.map_or(1.0, |e| clip_coverage(&e.clips, p));
                let light = ((x / 8 + y / 8) % 2) == 0;
                let mut color = if light {
                    [83, 87, 94, 255]
                } else {
                    [53, 57, 64, 255]
                };
                if placeholder == raf_ui::UiImagePlaceholder::Skeleton {
                    color = [67, 71, 78, 255];
                }
                if placeholder == raf_ui::UiImagePlaceholder::Automatic {
                    let dx = (p[0] - quad.rect.x) / quad.rect.width.max(1.0);
                    let dy = (p[1] - quad.rect.y) / quad.rect.height.max(1.0);
                    if (dx - dy).abs() < 0.045 || (dx + dy - 1.0).abs() < 0.045 {
                        color = [203, 73, 73, 255];
                    }
                }
                color[3] = quad.tint[3];
                if self.blend_pixel(x, y, color, (coverage * 255.0).round() as u8) {
                    written += 1;
                }
            }
        }
        written
    }

    fn blend_pixel(&mut self, x: u32, y: u32, color: [u8; 4], coverage: u8) -> bool {
        let source_alpha = (u32::from(color[3]) * u32::from(coverage) + 127) / 255;
        if source_alpha == 0 {
            return false;
        }
        let index = (y as usize * self.size[0] as usize + x as usize) * 4;
        let source_alpha = source_alpha as f32 / 255.0;
        let destination_alpha = self.pixels[index + 3] as f32 / 255.0;
        let output_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
        if output_alpha <= f32::EPSILON {
            return false;
        }

        for channel in 0..3 {
            let source = color[channel] as f32 / 255.0;
            let destination = self.pixels[index + channel] as f32 / 255.0;
            let output = (source * source_alpha
                + destination * destination_alpha * (1.0 - source_alpha))
                / output_alpha;
            self.pixels[index + channel] = (output * 255.0).round().clamp(0.0, 255.0) as u8;
        }
        self.pixels[index + 3] = (output_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
        true
    }
}

fn sample_alpha(
    pixels: &[u8],
    atlas_width: usize,
    atlas_height: usize,
    rect: UiRect,
    u: f32,
    v: f32,
) -> u8 {
    let width = rect.width.round().max(1.0) as usize;
    let height = rect.height.round().max(1.0) as usize;
    sample_plane(
        pixels,
        atlas_width,
        atlas_height,
        rect.x.round().max(0.0) as usize,
        rect.y.round().max(0.0) as usize,
        width,
        height,
        u,
        v,
    )
}

fn sample_plane(
    pixels: &[u8],
    stride: usize,
    rows: usize,
    origin_x: usize,
    origin_y: usize,
    width: usize,
    height: usize,
    u: f32,
    v: f32,
) -> u8 {
    if stride == 0 || rows == 0 || pixels.is_empty() {
        return 0;
    }

    let max_x = width
        .saturating_sub(1)
        .min(stride.saturating_sub(origin_x).saturating_sub(1));
    let max_y = height
        .saturating_sub(1)
        .min(rows.saturating_sub(origin_y).saturating_sub(1));
    let x = (u.clamp(0.0, 1.0) * max_x as f32).max(0.0);
    let y = (v.clamp(0.0, 1.0) * max_y as f32).max(0.0);
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(max_x);
    let y1 = (y0 + 1).min(max_y);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let at = |sample_x: usize, sample_y: usize| {
        pixels[(origin_y + sample_y) * stride + origin_x + sample_x]
    };
    let top = f32::from(at(x0, y0)) * (1.0 - tx) + f32::from(at(x1, y0)) * tx;
    let bottom = f32::from(at(x0, y1)) * (1.0 - tx) + f32::from(at(x1, y1)) * tx;
    (top * (1.0 - ty) + bottom * ty).round().clamp(0.0, 255.0) as u8
}

fn sample_rgba_nearest(pixels: &[u8], size: [u32; 2], u: f32, v: f32) -> [u8; 4] {
    let x = (u.clamp(0.0, 0.999_999) * size[0] as f32) as usize;
    let y = (v.clamp(0.0, 0.999_999) * size[1] as f32) as usize;
    let i = (y * size[0] as usize + x) * 4;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

fn sample_rgba(pixels: &[u8], size: [u32; 2], u: f32, v: f32) -> [u8; 4] {
    let width = size[0].max(1) as usize;
    let height = size[1].max(1) as usize;
    let x = (u.clamp(0.0, 1.0) * width as f32 - 0.5).clamp(0.0, width.saturating_sub(1) as f32);
    let y = (v.clamp(0.0, 1.0) * height as f32 - 0.5).clamp(0.0, height.saturating_sub(1) as f32);
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let at = |sample_x: usize, sample_y: usize| {
        let index = (sample_y * width + sample_x) * 4;
        [
            pixels[index],
            pixels[index + 1],
            pixels[index + 2],
            pixels[index + 3],
        ]
    };
    let top = at(x0, y0);
    let top_right = at(x1, y0);
    let bottom = at(x0, y1);
    let bottom_right = at(x1, y1);
    let mut color = [0; 4];
    let samples = [
        (top, (1.0 - tx) * (1.0 - ty)),
        (top_right, tx * (1.0 - ty)),
        (bottom, (1.0 - tx) * ty),
        (bottom_right, tx * ty),
    ];
    let alpha = samples
        .iter()
        .map(|(p, w)| f32::from(p[3]) * w)
        .sum::<f32>();
    color[3] = alpha.round() as u8;
    if alpha > 0.0 {
        for c in 0..3 {
            color[c] = (samples
                .iter()
                .map(|(p, w)| f32::from(p[c]) * f32::from(p[3]) * w)
                .sum::<f32>()
                / alpha)
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }
    color
}

struct PixelBounds {
    x_start: u32,
    x_end: u32,
    y_start: u32,
    y_end: u32,
}

fn pixel_bounds(rect: UiRect, size: [u32; 2]) -> Option<PixelBounds> {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return None;
    }
    let x_start = rect.x.floor().max(0.0) as u32;
    let y_start = rect.y.floor().max(0.0) as u32;
    let x_end = rect.right().ceil().min(size[0] as f32).max(0.0) as u32;
    let y_end = rect.bottom().ceil().min(size[1] as f32).max(0.0) as u32;
    (x_start < x_end && y_start < y_end).then_some(PixelBounds {
        x_start,
        x_end,
        y_start,
        y_end,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn rafui_alpha_weighted_filter_does_not_bleed_transparent_rgb() {
        let sampled = super::sample_rgba(&[255, 0, 0, 255, 0, 0, 255, 0], [2, 1], 0.5, 0.5);
        assert_eq!(&sampled[..3], &[255, 0, 0]);
        assert_eq!(sampled[3], 128);
    }
    use super::*;
    use crate::api_graphic_basic::ui_surface::{UiImageFit, UiSurfaceQuad};

    #[test]
    fn software_compositor_blends_a_solid_over_the_clear_color() {
        let list = UiSurfaceDrawList {
            solids: vec![UiSurfaceQuad {
                rect: UiRect::new(1.0, 1.0, 2.0, 2.0),
                clip_rect: UiRect::new(0.0, 0.0, 4.0, 4.0),
                color: [255, 0, 0, 128],
                z_index: 0,
                radius: 0.0,
            }],
            ..UiSurfaceDrawList::default()
        };
        let mut renderer = UiSurfaceCpuRenderer::new();
        let metrics = renderer.render(
            &list,
            &UiTextAtlas::default(),
            &UiSurfaceImageStore::default(),
            [4, 4],
            [0, 0, 0, 255],
        );

        let pixel = &renderer.pixels()[(1 * 4 + 1) * 4..(1 * 4 + 2) * 4];
        assert!((126..=129).contains(&pixel[0]));
        assert_eq!(pixel[1], 0);
        assert_eq!(pixel[3], 255);
        assert_eq!(metrics.translucent_solid_pixels, 4);
    }

    #[test]
    fn rounded_solid_keeps_the_outside_corner_clear() {
        let list = UiSurfaceDrawList {
            solids: vec![UiSurfaceQuad {
                rect: UiRect::new(0.0, 0.0, 8.0, 8.0),
                clip_rect: UiRect::new(0.0, 0.0, 8.0, 8.0),
                color: [255, 128, 0, 255],
                z_index: 0,
                radius: 3.0,
            }],
            ..UiSurfaceDrawList::default()
        };
        let mut renderer = UiSurfaceCpuRenderer::new();
        renderer.render(
            &list,
            &UiTextAtlas::default(),
            &UiSurfaceImageStore::default(),
            [8, 8],
            [0, 0, 0, 255],
        );

        assert_eq!(&renderer.pixels()[0..4], &[0, 0, 0, 255]);
        assert_eq!(
            &renderer.pixels()[(4 * 4)..(4 * 4 + 4)],
            &[255, 128, 0, 255]
        );
    }

    #[test]
    fn software_compositor_rasterizes_a_vector_stroke_without_gaps() {
        let list = UiSurfaceDrawList {
            strokes: vec![UiSurfaceStroke {
                start: [3.0, 4.0],
                end: [16.0, 15.0],
                clip_rect: UiRect::new(0.0, 0.0, 20.0, 20.0),
                color: [237, 239, 242, 255],
                width: 3.0,
                z_index: 0,
            }],
            paint_order: vec![UiSurfacePaintCommand::Stroke {
                index: 0,
                z_index: 0,
                sequence: 0,
            }],
            ..UiSurfaceDrawList::default()
        };
        let mut renderer = UiSurfaceCpuRenderer::new();
        let metrics = renderer.render(
            &list,
            &UiTextAtlas::default(),
            &UiSurfaceImageStore::default(),
            [20, 20],
            [0, 0, 0, 255],
        );

        assert!(metrics.stroke_pixels > 0);
        assert!(renderer
            .pixels()
            .chunks_exact(4)
            .any(|pixel| pixel[0] > 0 && pixel[1] > 0 && pixel[2] > 0));
    }

    #[test]
    fn image_quad_uses_registered_rgba_pixels() {
        let list = UiSurfaceDrawList {
            images: vec![UiSurfaceImageQuad {
                rect: UiRect::new(0.0, 0.0, 2.0, 2.0),
                clip_rect: UiRect::new(0.0, 0.0, 2.0, 2.0),
                source_key: "mark".to_string(),
                fit: UiImageFit::Stretch,
                tint: [255, 255, 255, 255],
                z_index: 0,
            }],
            ..UiSurfaceDrawList::default()
        };
        let mut images = UiSurfaceImageStore::default();
        images
            .insert_rgba("mark", [1, 1], vec![10, 200, 30, 255])
            .unwrap();
        let mut renderer = UiSurfaceCpuRenderer::new();
        renderer.render(
            &list,
            &UiTextAtlas::default(),
            &images,
            [2, 2],
            [0, 0, 0, 255],
        );

        assert_eq!(&renderer.pixels()[0..4], &[10, 200, 30, 255]);
    }
}
