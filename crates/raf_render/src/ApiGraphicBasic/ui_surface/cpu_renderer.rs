//! Software compositor for retained UI surfaces.
//!
//! The normal editor path presents through WGPU. This renderer is a compact
//! recovery path for the same retained draw list, so low-end or incompatible
//! systems do not need a separate UI implementation.

use super::{UiRect, UiSurfaceDrawList, UiSurfacePaintCommand, UiTextAtlas};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiSurfaceCpuMetrics {
    pub solid_pixels: u64,
    pub text_pixels: u64,
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
                        metrics.solid_pixels += self.draw_solid(quad.rect, quad.radius, quad.color);
                    }
                }
                UiSurfacePaintCommand::Text { index, .. } => {
                    if let Some(quad) = draw_list.text.get(index) {
                        metrics.text_pixels += self.draw_text(quad, atlas);
                    }
                }
            }
        }
        metrics
    }

    fn draw_solid(&mut self, rect: UiRect, radius: f32, color: [u8; 4]) -> u64 {
        let Some(bounds) = pixel_bounds(rect, self.size) else {
            return 0;
        };
        let mut written = 0;
        for y in bounds.y_start..bounds.y_end {
            for x in bounds.x_start..bounds.x_end {
                if !inside_rounded_rect(rect, radius, x as f32 + 0.5, y as f32 + 0.5) {
                    continue;
                }
                if self.blend_pixel(x, y, color, 255) {
                    written += 1;
                }
            }
        }
        written
    }

    fn draw_text(&mut self, quad: &super::UiSurfaceTextQuad, atlas: &UiTextAtlas) -> u64 {
        let Some(bounds) = pixel_bounds(quad.rect, self.size) else {
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
            let source_y =
                quad.atlas_rect.y as usize + (local_y * quad.atlas_rect.height.max(1.0)) as usize;
            if source_y >= atlas_height {
                continue;
            }
            for x in bounds.x_start..bounds.x_end {
                let local_x =
                    ((x as f32 + 0.5 - quad.rect.x) / quad.rect.width).clamp(0.0, 0.999_999);
                let source_x = quad.atlas_rect.x as usize
                    + (local_x * quad.atlas_rect.width.max(1.0)) as usize;
                if source_x >= atlas_width {
                    continue;
                }
                let coverage = atlas_pixels[source_y * atlas_width + source_x];
                if self.blend_pixel(x, y, quad.color, coverage) {
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

fn inside_rounded_rect(rect: UiRect, radius: f32, x: f32, y: f32) -> bool {
    let radius = radius.max(0.0).min(rect.width * 0.5).min(rect.height * 0.5);
    if radius <= 0.5 {
        return true;
    }
    let center_x = x.clamp(rect.x + radius, rect.right() - radius);
    let center_y = y.clamp(rect.y + radius, rect.bottom() - radius);
    let dx = x - center_x;
    let dy = y - center_y;
    dx * dx + dy * dy <= radius * radius
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_graphic_basic::ui_surface::UiSurfaceQuad;

    #[test]
    fn software_compositor_blends_a_solid_over_the_clear_color() {
        let list = UiSurfaceDrawList {
            solids: vec![UiSurfaceQuad {
                rect: UiRect::new(1.0, 1.0, 2.0, 2.0),
                color: [255, 0, 0, 128],
                z_index: 0,
                radius: 0.0,
            }],
            ..UiSurfaceDrawList::default()
        };
        let mut renderer = UiSurfaceCpuRenderer::new();
        renderer.render(&list, &UiTextAtlas::default(), [4, 4], [0, 0, 0, 255]);

        let pixel = &renderer.pixels()[(1 * 4 + 1) * 4..(1 * 4 + 2) * 4];
        assert!((126..=129).contains(&pixel[0]));
        assert_eq!(pixel[1], 0);
        assert_eq!(pixel[3], 255);
    }

    #[test]
    fn rounded_solid_keeps_the_outside_corner_clear() {
        let list = UiSurfaceDrawList {
            solids: vec![UiSurfaceQuad {
                rect: UiRect::new(0.0, 0.0, 8.0, 8.0),
                color: [255, 128, 0, 255],
                z_index: 0,
                radius: 3.0,
            }],
            ..UiSurfaceDrawList::default()
        };
        let mut renderer = UiSurfaceCpuRenderer::new();
        renderer.render(&list, &UiTextAtlas::default(), [8, 8], [0, 0, 0, 255]);

        assert_eq!(&renderer.pixels()[0..4], &[0, 0, 0, 255]);
        assert_eq!(
            &renderer.pixels()[(4 * 4)..(4 * 4 + 4)],
            &[255, 128, 0, 255]
        );
    }
}
