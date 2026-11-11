//! One crop, focal-point and nine-slice layout for both compositors.
use raf_ui::{UiImageFit, UiImageOptions, UiRect};

#[derive(Debug, Clone, Copy)]
pub(super) struct ImagePatch {
    pub destination: UiRect,
    pub uv: UiRect,
}

fn finite(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

pub(super) fn image_patches(
    rect: UiRect,
    size: [u32; 2],
    fit: UiImageFit,
    options: &UiImageOptions,
) -> Vec<ImagePatch> {
    if rect.is_empty() || size.contains(&0) {
        return Vec::new();
    }
    let crop = options
        .source_rect
        .map(|r| {
            let x = finite(r.x, 0.0).clamp(0.0, 1.0);
            let y = finite(r.y, 0.0).clamp(0.0, 1.0);
            UiRect::new(
                x,
                y,
                finite(r.width, 0.0).clamp(0.0, 1.0 - x),
                finite(r.height, 0.0).clamp(0.0, 1.0 - y),
            )
        })
        .unwrap_or(UiRect::new(0.0, 0.0, 1.0, 1.0));
    if crop.is_empty() {
        return Vec::new();
    }
    let source = [size[0] as f32 * crop.width, size[1] as f32 * crop.height];
    if let Some(s) = options.nine_slice {
        let mut edges = [
            finite(s.top, 0.0).max(0.0),
            finite(s.right, 0.0).max(0.0),
            finite(s.bottom, 0.0).max(0.0),
            finite(s.left, 0.0).max(0.0),
        ];
        let sx = (source[0] / (edges[1] + edges[3]).max(source[0])).min(1.0);
        let sy = (source[1] / (edges[0] + edges[2]).max(source[1])).min(1.0);
        edges[1] *= sx;
        edges[3] *= sx;
        edges[0] *= sy;
        edges[2] *= sy;
        let dx = (rect.width / (edges[1] + edges[3]).max(rect.width)).min(1.0);
        let dy = (rect.height / (edges[0] + edges[2]).max(rect.height)).min(1.0);
        let xs = [
            rect.x,
            rect.x + edges[3] * dx,
            rect.right() - edges[1] * dx,
            rect.right(),
        ];
        let ys = [
            rect.y,
            rect.y + edges[0] * dy,
            rect.bottom() - edges[2] * dy,
            rect.bottom(),
        ];
        let us = [
            crop.x,
            crop.x + edges[3] / size[0] as f32,
            crop.right() - edges[1] / size[0] as f32,
            crop.right(),
        ];
        let vs = [
            crop.y,
            crop.y + edges[0] / size[1] as f32,
            crop.bottom() - edges[2] / size[1] as f32,
            crop.bottom(),
        ];
        let mut result = Vec::with_capacity(9);
        for y in 0..3 {
            for x in 0..3 {
                let destination = UiRect::new(xs[x], ys[y], xs[x + 1] - xs[x], ys[y + 1] - ys[y]);
                let uv = UiRect::new(us[x], vs[y], us[x + 1] - us[x], vs[y + 1] - vs[y]);
                if !destination.is_empty() && !uv.is_empty() {
                    result.push(ImagePatch { destination, uv });
                }
            }
        }
        return result;
    }
    let scale = match fit {
        UiImageFit::Contain => (rect.width / source[0]).min(rect.height / source[1]),
        UiImageFit::Cover => (rect.width / source[0]).max(rect.height / source[1]),
        UiImageFit::Stretch => 1.0,
    };
    let extent = if fit == UiImageFit::Stretch {
        [rect.width, rect.height]
    } else {
        [source[0] * scale, source[1] * scale]
    };
    let focal = options.focal_point.map(|v| finite(v, 0.5).clamp(0.0, 1.0));
    vec![ImagePatch {
        destination: UiRect::new(
            rect.x + (rect.width - extent[0]) * focal[0],
            rect.y + (rect.height - extent[1]) * focal[1],
            extent[0],
            extent[1],
        ),
        uv: crop,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rafui_nine_slice_and_focal_crop_are_bounded() {
        let options = UiImageOptions {
            nine_slice: Some(raf_ui::UiImageInsets::new(10.0, 10.0, 10.0, 10.0)),
            ..Default::default()
        };
        let patches = image_patches(
            UiRect::new(0.0, 0.0, 12.0, 12.0),
            [32, 32],
            UiImageFit::Stretch,
            &options,
        );
        assert!(patches.iter().all(|p| p.destination.right() <= 12.0
            && p.destination.bottom() <= 12.0
            && p.uv.right() <= 1.0));
        let options = UiImageOptions {
            focal_point: [1.0, 0.0],
            ..Default::default()
        };
        let patch = image_patches(
            UiRect::new(0.0, 0.0, 10.0, 10.0),
            [20, 10],
            UiImageFit::Cover,
            &options,
        )[0];
        assert_eq!(patch.destination.x, -10.0);
    }
}
