//! Bounded image decoding for renderer-owned texture resources.

use std::path::Path;

const MAX_SOURCE_DIMENSION: u32 = 8192;
// Keep transient decode memory near 32 MiB while still accepting common 4K
// source images before shrinking them to the selected viewport tier.
const MAX_SOURCE_PIXELS: u64 = 8 * 1024 * 1024;

/// Decoded, tightly packed RGBA8 image data. This type contains no backend resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub rgba8: Vec<u8>,
}

impl DecodedImage {
    pub fn byte_len(&self) -> usize {
        self.rgba8.len()
    }
}

/// Estimates the output dimensions of a bounded decode without allocating the
/// decoded pixel buffer. Renderers use this to reject an image before loading
/// it when the current frame has no room left for the texture.
pub fn image_output_dimensions(path: &Path, max_dimension: u32) -> Result<(u32, u32), String> {
    let (source_width, source_height) = checked_source_dimensions(path)?;
    Ok(output_dimensions(
        source_width,
        source_height,
        max_dimension,
    ))
}

/// Decodes a supported image and downsizes it before it enters renderer memory.
///
/// Dimensions and total source pixels are checked before full decode so a
/// malformed or unusually large asset cannot cause unbounded transient memory.
pub fn decode_image(path: &Path, max_dimension: u32) -> Result<DecodedImage, String> {
    let (source_width, source_height) = checked_source_dimensions(path)?;
    let (width, height) = output_dimensions(source_width, source_height, max_dimension);
    let rgba = image::open(path)
        .map_err(|error| format!("Unable to decode image: {error}"))?
        .to_rgba8();
    let rgba8 = if (width, height) != (source_width, source_height) {
        image::imageops::resize(&rgba, width, height, image::imageops::FilterType::Lanczos3)
            .into_raw()
    } else {
        rgba.into_raw()
    };

    Ok(DecodedImage {
        width,
        height,
        rgba8,
    })
}

fn checked_source_dimensions(path: &Path) -> Result<(u32, u32), String> {
    let (source_width, source_height) = image::image_dimensions(path)
        .map_err(|error| format!("Unable to read image dimensions: {error}"))?;
    let source_pixels = u64::from(source_width).saturating_mul(u64::from(source_height));
    if source_width == 0
        || source_height == 0
        || source_width > MAX_SOURCE_DIMENSION
        || source_height > MAX_SOURCE_DIMENSION
        || source_pixels > MAX_SOURCE_PIXELS
    {
        return Err(format!(
            "Image dimensions {source_width}x{source_height} exceed the decoder limit."
        ));
    }

    Ok((source_width, source_height))
}

fn output_dimensions(source_width: u32, source_height: u32, max_dimension: u32) -> (u32, u32) {
    let max_dimension = max_dimension.clamp(1, MAX_SOURCE_DIMENSION);
    if source_width > max_dimension || source_height > max_dimension {
        let scale = max_dimension as f64 / source_width.max(source_height) as f64;
        let width = ((source_width as f64 * scale).round() as u32).max(1);
        let height = ((source_height as f64 * scale).round() as u32).max(1);
        (width, height)
    } else {
        (source_width, source_height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_image_downscales_to_the_requested_maximum_dimension() {
        let path =
            std::env::temp_dir().join(format!("raf-image-asset-{}.png", uuid::Uuid::new_v4()));
        let source = image::RgbaImage::from_pixel(64, 32, image::Rgba([12, 34, 56, 255]));
        source.save(&path).expect("write temporary test image");

        assert_eq!(
            image_output_dimensions(&path, 16).expect("estimate resized dimensions"),
            (16, 8)
        );
        let decoded = decode_image(&path, 16).expect("decode and resize test image");
        let _ = std::fs::remove_file(&path);

        assert_eq!((decoded.width, decoded.height), (16, 8));
        assert_eq!(decoded.byte_len(), 16 * 8 * 4);
        assert_eq!(&decoded.rgba8[..4], &[12, 34, 56, 255]);
    }
}
