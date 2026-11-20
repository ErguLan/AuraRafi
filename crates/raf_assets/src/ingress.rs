//! Bounded texture decode policy for render-facing asset ingress.
//!
//! `raf_assets` validates a project source file and turns it into
//! engine-owned RGBA8 pixels. Residency, eviction, upload, and GPU lifetime
//! stay in ApiGraphicBasic; this module only owns the decode boundary so every
//! backend receives the same descriptor and the same hard limits.

use std::fmt;
use std::io::Cursor;
use std::path::Path;

/// Default maximum decoded/output dimension for one texture.
pub const DEFAULT_MAX_DIMENSION: u32 = 4096;
/// Default maximum accepted source file size.
pub const DEFAULT_MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
/// Default maximum accepted source pixel count before decode.
pub const DEFAULT_MAX_PIXELS: u64 = 16_777_216;
/// Hard upper bound for any generated thumbnail.
pub const MAX_THUMBNAIL_DIMENSION: u32 = 128;

/// Hard limits applied before any texture enters renderer memory.
///
/// `max_dimension` bounds the decoded output resolution: larger sources are
/// resized down after they pass the source-pixel and byte checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureDecodeLimits {
    /// Maximum width or height of the decoded texture, in pixels.
    pub max_dimension: u32,
    /// Maximum source file size accepted, in bytes.
    pub max_file_bytes: u64,
    /// Maximum source pixel count accepted before decoding.
    pub max_pixels: u64,
}

impl Default for TextureDecodeLimits {
    fn default() -> Self {
        Self {
            max_dimension: DEFAULT_MAX_DIMENSION,
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            max_pixels: DEFAULT_MAX_PIXELS,
        }
    }
}

impl TextureDecodeLimits {
    /// Reject limits that cannot describe a decodable texture.
    pub fn validate(&self) -> Result<(), AssetIngressError> {
        if self.max_dimension == 0 || self.max_pixels == 0 {
            return Err(AssetIngressError::Dimensions);
        }
        if self.max_file_bytes == 0 {
            return Err(AssetIngressError::TooLarge);
        }
        Ok(())
    }

    /// A copy of these limits bounded to a smaller output dimension.
    pub fn with_max_dimension(self, max_dimension: u32) -> Self {
        Self {
            max_dimension,
            ..self
        }
    }
}

/// Decoded, tightly packed RGBA8 pixels ready for a renderer-owned resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedTexture {
    pub width: u32,
    pub height: u32,
    pub rgba8: Vec<u8>,
    /// FNV-1a 64 hash of the accepted source bytes.
    pub source_hash: u64,
}

impl DecodedTexture {
    /// Pixel buffer size in bytes.
    pub fn byte_len(&self) -> usize {
        self.rgba8.len()
    }

    /// Whether the pixel buffer matches the declared dimensions.
    pub fn has_consistent_pixels(&self) -> bool {
        let expected = (self.width as usize)
            .saturating_mul(self.height as usize)
            .saturating_mul(4);
        expected == self.rgba8.len()
    }
}

/// Failure returned by the texture decode boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetIngressError {
    /// The source path does not name a readable regular file.
    NotFound,
    /// The file extension is not an accepted texture source format.
    Unsupported,
    /// The source file or limits exceed the byte budget.
    TooLarge,
    /// The resolved path leaves the project asset root.
    Escape,
    /// The image bytes could not be decoded.
    Decode(String),
    /// The decoded or source dimensions exceed the pixel budget.
    Dimensions,
}

impl fmt::Display for AssetIngressError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(formatter, "texture source file was not found"),
            Self::Unsupported => write!(formatter, "texture source format is not supported"),
            Self::TooLarge => write!(formatter, "texture source exceeds the decode byte limit"),
            Self::Escape => write!(formatter, "texture path escapes the project asset root"),
            Self::Decode(message) => write!(formatter, "texture decode failed: {message}"),
            Self::Dimensions => write!(formatter, "texture dimensions exceed the decode limit"),
        }
    }
}

impl std::error::Error for AssetIngressError {}

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Decode one project texture through the bounded ingress policy.
///
/// The source is rejected before pixel allocation when its extension is not
/// accepted, its file size exceeds `max_file_bytes`, or its reported pixel
/// count exceeds `max_pixels`. Accepted images are converted to tightly packed
/// RGBA8 and resized down to `max_dimension` while preserving aspect ratio.
pub fn decode_texture(
    path: &Path,
    limits: &TextureDecodeLimits,
) -> Result<DecodedTexture, AssetIngressError> {
    limits.validate()?;

    let metadata = std::fs::metadata(path).map_err(|_| AssetIngressError::NotFound)?;
    if !metadata.is_file() {
        return Err(AssetIngressError::NotFound);
    }
    let format = source_format(path).ok_or(AssetIngressError::Unsupported)?;
    if metadata.len() > limits.max_file_bytes {
        return Err(AssetIngressError::TooLarge);
    }

    let source_bytes = std::fs::read(path).map_err(|_| AssetIngressError::NotFound)?;
    if source_bytes.len() as u64 > limits.max_file_bytes {
        return Err(AssetIngressError::TooLarge);
    }
    let source_hash = fnv1a64(&source_bytes);

    let (source_width, source_height) = bounded_reader(&source_bytes, format)?
        .into_dimensions()
        .map_err(|error| AssetIngressError::Decode(error.to_string()))?;
    let source_pixels = u64::from(source_width).saturating_mul(u64::from(source_height));
    if source_width == 0 || source_height == 0 || source_pixels > limits.max_pixels {
        return Err(AssetIngressError::Dimensions);
    }

    let decoded = bounded_reader(&source_bytes, format)?
        .decode()
        .map_err(|error| AssetIngressError::Decode(error.to_string()))?
        .to_rgba8();
    let decoded_pixels = u64::from(decoded.width()).saturating_mul(u64::from(decoded.height()));
    if decoded.width() == 0 || decoded.height() == 0 || decoded_pixels > limits.max_pixels {
        return Err(AssetIngressError::Dimensions);
    }

    let (width, height) =
        bounded_dimensions(decoded.width(), decoded.height(), limits.max_dimension);
    let rgba8 = if (width, height) == (decoded.width(), decoded.height()) {
        decoded.into_raw()
    } else {
        image::imageops::resize(&decoded, width, height, image::imageops::FilterType::Lanczos3)
            .into_raw()
    };

    Ok(DecodedTexture {
        width,
        height,
        rgba8,
        source_hash,
    })
}

/// Build a bounded thumbnail with a self-made box filter.
///
/// The result preserves aspect ratio, never exceeds 128 px on either axis, and
/// keeps the source hash so callers can correlate it with the full texture.
/// Textures already inside the bound are cloned unchanged.
pub fn thumbnail(
    texture: &DecodedTexture,
    max_dimension: u32,
) -> Result<DecodedTexture, AssetIngressError> {
    let max_dimension = max_dimension.min(MAX_THUMBNAIL_DIMENSION);
    if max_dimension == 0
        || texture.width == 0
        || texture.height == 0
        || !texture.has_consistent_pixels()
    {
        return Err(AssetIngressError::Dimensions);
    }
    if texture.width <= max_dimension && texture.height <= max_dimension {
        return Ok(texture.clone());
    }

    let (width, height) = bounded_dimensions(texture.width, texture.height, max_dimension);
    let source_width = texture.width as usize;
    let source_height = texture.height as usize;
    let target_width = width as usize;
    let target_height = height as usize;
    let mut rgba8 = vec![0_u8; target_width.saturating_mul(target_height).saturating_mul(4)];

    for y in 0..target_height {
        let source_y0 = y.saturating_mul(source_height) / target_height;
        let source_y1 = (((y + 1).saturating_mul(source_height) + target_height - 1)
            / target_height)
            .max(source_y0 + 1)
            .min(source_height);
        for x in 0..target_width {
            let source_x0 = x.saturating_mul(source_width) / target_width;
            let source_x1 = (((x + 1).saturating_mul(source_width) + target_width - 1)
                / target_width)
                .max(source_x0 + 1)
                .min(source_width);
            let mut sums = [0_u64; 4];
            let mut samples = 0_u64;
            for source_y in source_y0..source_y1 {
                for source_x in source_x0..source_x1 {
                    let index = (source_y * source_width + source_x) * 4;
                    for channel in 0..4 {
                        sums[channel] += u64::from(texture.rgba8[index + channel]);
                    }
                    samples += 1;
                }
            }
            let target_index = (y * target_width + x) * 4;
            let samples = samples.max(1);
            for channel in 0..4 {
                rgba8[target_index + channel] = ((sums[channel] + samples / 2) / samples) as u8;
            }
        }
    }

    Ok(DecodedTexture {
        width,
        height,
        rgba8,
        source_hash: texture.source_hash,
    })
}

fn source_format(path: &Path) -> Option<image::ImageFormat> {
    let extension = path.extension()?.to_str()?;
    if extension.eq_ignore_ascii_case("png") {
        Some(image::ImageFormat::Png)
    } else if extension.eq_ignore_ascii_case("jpg") || extension.eq_ignore_ascii_case("jpeg") {
        Some(image::ImageFormat::Jpeg)
    } else if extension.eq_ignore_ascii_case("webp") {
        Some(image::ImageFormat::WebP)
    } else if extension.eq_ignore_ascii_case("bmp") {
        Some(image::ImageFormat::Bmp)
    } else if extension.eq_ignore_ascii_case("tga") {
        Some(image::ImageFormat::Tga)
    } else {
        None
    }
}

/// Build a bounded reader that prefers the detected content format.
///
/// The extension gate rejects unsupported asset kinds before this point. A
/// recognized payload may still override the extension-implied format, which
/// keeps correctly encoded files working after being renamed; the recognized
/// format must remain inside the same accepted family.
fn bounded_reader<'a>(
    source_bytes: &'a [u8],
    expected_format: image::ImageFormat,
) -> Result<image::ImageReader<Cursor<&'a [u8]>>, AssetIngressError> {
    let reader = image::ImageReader::with_format(Cursor::new(source_bytes), expected_format)
        .with_guessed_format()
        .map_err(|error| AssetIngressError::Decode(error.to_string()))?;
    match reader.format() {
        Some(format) if is_supported_image_format(format) => Ok(reader),
        _ => Err(AssetIngressError::Unsupported),
    }
}

fn is_supported_image_format(format: image::ImageFormat) -> bool {
    matches!(
        format,
        image::ImageFormat::Png
            | image::ImageFormat::Jpeg
            | image::ImageFormat::WebP
            | image::ImageFormat::Bmp
            | image::ImageFormat::Tga
    )
}

fn bounded_dimensions(width: u32, height: u32, max_dimension: u32) -> (u32, u32) {
    let max_dimension = max_dimension.max(1);
    let longest = width.max(height);
    if longest <= max_dimension {
        return (width, height);
    }
    let scale = f64::from(max_dimension) / f64::from(longest);
    (
        ((f64::from(width) * scale).round() as u32).max(1),
        ((f64::from(height) * scale).round() as u32).max(1),
    )
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temporary_path(label: &str, extension: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "raf-ingress-{label}-{}-{}.{extension}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ))
    }

    fn write_png(path: &Path, width: u32, height: u32, color: [u8; 4]) {
        image::RgbaImage::from_pixel(width, height, image::Rgba(color))
            .save(path)
            .expect("write temporary PNG");
    }

    #[test]
    fn unsupported_extension_is_rejected_before_decode() {
        let path = temporary_path("unsupported", "svg");
        std::fs::write(&path, b"<svg></svg>").expect("write unsupported source");
        let result = decode_texture(&path, &TextureDecodeLimits::default());
        let _ = std::fs::remove_file(&path);

        assert_eq!(result, Err(AssetIngressError::Unsupported));
    }

    #[test]
    fn oversized_source_files_are_rejected() {
        let path = temporary_path("large-file", "png");
        write_png(&path, 8, 8, [10, 20, 30, 255]);
        let limits = TextureDecodeLimits {
            max_file_bytes: 16,
            ..TextureDecodeLimits::default()
        };
        let result = decode_texture(&path, &limits);
        let _ = std::fs::remove_file(&path);

        assert_eq!(result, Err(AssetIngressError::TooLarge));
    }

    #[test]
    fn oversized_pixel_dimensions_are_rejected() {
        let path = temporary_path("large-pixels", "png");
        write_png(&path, 32, 32, [10, 20, 30, 255]);
        let limits = TextureDecodeLimits {
            max_pixels: 100,
            ..TextureDecodeLimits::default()
        };
        let result = decode_texture(&path, &limits);
        let _ = std::fs::remove_file(&path);

        assert_eq!(result, Err(AssetIngressError::Dimensions));
    }

    #[test]
    fn invalid_sources_return_decode_errors() {
        let path = temporary_path("bad-bytes", "png");
        std::fs::write(&path, b"not an image").expect("write invalid PNG");
        let result = decode_texture(&path, &TextureDecodeLimits::default());
        let _ = std::fs::remove_file(&path);

        assert!(matches!(result, Err(AssetIngressError::Decode(_))));
    }

    #[test]
    fn large_sources_are_downscaled_to_the_output_dimension() {
        let path = temporary_path("downscale", "png");
        write_png(&path, 64, 32, [12, 34, 56, 255]);
        let limits = TextureDecodeLimits::default().with_max_dimension(16);
        let decoded = decode_texture(&path, &limits).expect("decode bounded texture");
        let _ = std::fs::remove_file(&path);

        assert_eq!((decoded.width, decoded.height), (16, 8));
        assert_eq!(decoded.byte_len(), 16 * 8 * 4);
        assert!(decoded.has_consistent_pixels());
    }

    #[test]
    fn thumbnails_preserve_aspect_ratio_and_pixels() {
        let texture = DecodedTexture {
            width: 200,
            height: 100,
            rgba8: [40, 80, 120, 255].repeat(200 * 100),
            source_hash: 0xfeed_face,
        };
        let thumb = thumbnail(&texture, 512).expect("build bounded thumbnail");

        assert_eq!((thumb.width, thumb.height), (128, 64));
        assert!(thumb.width <= MAX_THUMBNAIL_DIMENSION);
        assert!(thumb.height <= MAX_THUMBNAIL_DIMENSION);
        assert_eq!(thumb.byte_len(), 128 * 64 * 4);
        assert_eq!(thumb.source_hash, texture.source_hash);
        assert!(thumb.rgba8.chunks_exact(4).all(|pixel| pixel == [40, 80, 120, 255]));
    }

    #[test]
    fn source_hash_is_deterministic_for_the_same_bytes() {
        let first = temporary_path("hash-a", "png");
        let second = temporary_path("hash-b", "png");
        write_png(&first, 4, 4, [1, 2, 3, 255]);
        write_png(&second, 4, 4, [9, 9, 9, 255]);

        let first_hash = decode_texture(&first, &TextureDecodeLimits::default())
            .expect("decode first source")
            .source_hash;
        let second_hash = decode_texture(&first, &TextureDecodeLimits::default())
            .expect("decode first source again")
            .source_hash;
        let different_hash = decode_texture(&second, &TextureDecodeLimits::default())
            .expect("decode second source")
            .source_hash;
        let _ = std::fs::remove_file(&first);
        let _ = std::fs::remove_file(&second);

        assert_eq!(first_hash, second_hash);
        assert_ne!(first_hash, different_hash);
    }
}
