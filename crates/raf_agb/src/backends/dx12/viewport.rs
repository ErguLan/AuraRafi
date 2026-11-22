//! Scene viewport conversion for the Direct3D 12 executor.
//!
//! The shared frame carries the scene viewport normalized inside the full
//! target (`[x, y, w, h]`). Scene batches render into that pixel rect, while
//! overlays and the fullscreen composite keep the full target so UI and CAD are
//! never clipped by the canvas.

use windows::Win32::{Foundation::RECT, Graphics::Direct3D12::D3D12_VIEWPORT};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl PixelRect {
    pub(super) fn full([width, height]: [u32; 2]) -> Self {
        Self {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    pub(super) fn viewport(self) -> D3D12_VIEWPORT {
        D3D12_VIEWPORT {
            TopLeftX: self.x as f32,
            TopLeftY: self.y as f32,
            Width: self.width as f32,
            Height: self.height as f32,
            MinDepth: 0.,
            MaxDepth: 1.,
        }
    }

    pub(super) fn scissor(self) -> RECT {
        RECT {
            left: self.x as i32,
            top: self.y as i32,
            right: (self.x + self.width) as i32,
            bottom: (self.y + self.height) as i32,
        }
    }
}

/// Converts a normalized scene viewport into a clamped pixel rect.
///
/// `x = round(nx * width)` and `y = round(ny * height)` are clamped to the last
/// target column or row. `w = round(nw * width)` and `h = round(nh * height)`
/// keep at least one pixel and never pass the target edge.
pub(super) fn scene_rect(normalized: [f32; 4], [width, height]: [u32; 2]) -> PixelRect {
    if width == 0 || height == 0 {
        return PixelRect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };
    }
    let [nx, ny, nw, nh] = normalized;
    let nx = finite(nx, 0.);
    let ny = finite(ny, 0.);
    let nw = finite(nw, 1.);
    let nh = finite(nh, 1.);
    let x = (nx * width as f32)
        .round()
        .clamp(0., (width - 1) as f32) as u32;
    let y = (ny * height as f32)
        .round()
        .clamp(0., (height - 1) as f32) as u32;
    let w = (nw * width as f32)
        .round()
        .clamp(1., (width - x) as f32) as u32;
    let h = (nh * height as f32)
        .round()
        .clamp(1., (height - y) as f32) as u32;
    PixelRect {
        x,
        y,
        width: w,
        height: h,
    }
}

fn finite(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_a_normalized_viewport_to_pixels() {
        assert_eq!(
            scene_rect([0.25, 0.1, 0.5, 0.6], [1600, 900]),
            PixelRect {
                x: 400,
                y: 90,
                width: 800,
                height: 540,
            }
        );
    }

    #[test]
    fn clamps_to_the_target_and_keeps_at_least_one_pixel() {
        assert_eq!(
            scene_rect([-0.5, -0.25, 2.0, 2.0], [1600, 900]),
            PixelRect {
                x: 0,
                y: 0,
                width: 1600,
                height: 900,
            }
        );
        assert_eq!(
            scene_rect([0.5, 0.5, 0.75, 0.75], [100, 100]),
            PixelRect {
                x: 50,
                y: 50,
                width: 50,
                height: 50,
            }
        );
        assert_eq!(
            scene_rect([0.999, 0.999, 0.0, 0.0], [100, 100]),
            PixelRect {
                x: 99,
                y: 99,
                width: 1,
                height: 1,
            }
        );
        assert_eq!(
            scene_rect([2.0, 2.0, 1.0, 1.0], [100, 100]),
            PixelRect {
                x: 99,
                y: 99,
                width: 1,
                height: 1,
            }
        );
    }
}
