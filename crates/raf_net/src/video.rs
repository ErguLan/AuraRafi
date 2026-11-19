//! Raw video frames for the cloud streaming protocol.
//!
//! Only uncompressed RGBA is defined. The codec is deliberately absent: a
//! frame is the exact buffer a renderer can upload or hand to a texture
//! uploader, with a fixed metadata tail at the end of the wire payload.
//!
//! [`MAX_VIDEO_BYTES`] bounds a decoded frame at 32 MiB, while one binary
//! transport frame carries at most 16 MiB. [`crate::StreamPeer::send`] rejects
//! a video frame whose encoded payload exceeds the transport limit; stream a
//! resolution that preserves the 16 MiB envelope (for example 1280x720 or
//! 1920x1080 RGBA8).

use crate::codec::PayloadReader;
use crate::frame::{header, FrameKind};
use crate::protocol::{
    FRAME_HEADER_LEN, MAX_FRAME_PAYLOAD, MAX_VIDEO_BYTES, MAX_VIDEO_DIMENSION, VIDEO_TAIL_LEN,
};
use std::fmt;
use std::io;

/// Encoding identifier for tightly packed RGBA8 pixels.
pub const ENCODING_RAW_RGBA8: u8 = 0;

/// One uncompressed video frame.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoFrame {
    pub seq: u64,
    pub timestamp_millis: u64,
    pub width: u16,
    pub height: u16,
    /// 0 = [`ENCODING_RAW_RGBA8`].
    pub encoding: u8,
    /// Tightly packed RGBA bytes, `width * height * 4` long.
    pub pixels: Vec<u8>,
}

impl VideoFrame {
    /// Copy one RGBA buffer into a validated frame.
    pub fn from_rgba(
        seq: u64,
        timestamp_millis: u64,
        width: u16,
        height: u16,
        rgba: &[u8],
    ) -> Result<Self, VideoError> {
        Self::from_rgba_owned(seq, timestamp_millis, width, height, rgba.to_vec())
    }

    /// Adopt an owned RGBA buffer without an extra copy.
    pub fn from_rgba_owned(
        seq: u64,
        timestamp_millis: u64,
        width: u16,
        height: u16,
        pixels: Vec<u8>,
    ) -> Result<Self, VideoError> {
        validate_shape(width, height, ENCODING_RAW_RGBA8, pixels.len())?;
        Ok(Self {
            seq,
            timestamp_millis,
            width,
            height,
            encoding: ENCODING_RAW_RGBA8,
            pixels,
        })
    }

    /// Validate dimensions, encoding and pixel length.
    pub fn validate(&self) -> Result<(), VideoError> {
        validate_shape(self.width, self.height, self.encoding, self.pixels.len())
    }

    /// Wire payload length including the metadata tail.
    pub fn payload_len(&self) -> usize {
        self.pixels.len().saturating_add(VIDEO_TAIL_LEN)
    }

    /// Borrow the RGBA bytes for an uploader.
    pub fn as_rgba(&self) -> &[u8] {
        &self.pixels
    }

    /// Consume the frame and return its RGBA buffer.
    pub fn into_rgba(self) -> Vec<u8> {
        self.pixels
    }

    pub(crate) fn tail_bytes(&self) -> [u8; VIDEO_TAIL_LEN] {
        let mut tail = [0u8; VIDEO_TAIL_LEN];
        tail[0..8].copy_from_slice(&self.seq.to_le_bytes());
        tail[8..16].copy_from_slice(&self.timestamp_millis.to_le_bytes());
        tail[16..18].copy_from_slice(&self.width.to_le_bytes());
        tail[18..20].copy_from_slice(&self.height.to_le_bytes());
        tail[20] = self.encoding;
        tail
    }

    pub(crate) fn encode_frame(&self, kind: FrameKind) -> io::Result<Vec<u8>> {
        self.validate().map_err(video_io_invalid_input)?;
        let payload_len = self.payload_len();
        if payload_len > MAX_FRAME_PAYLOAD {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "video payload {payload_len} bytes exceeds the {MAX_FRAME_PAYLOAD} byte frame limit"
                ),
            ));
        }
        let frame_header = header(kind, payload_len)?;
        let mut frame = Vec::with_capacity(FRAME_HEADER_LEN + payload_len);
        frame.extend_from_slice(&frame_header);
        frame.extend_from_slice(&self.pixels);
        frame.extend_from_slice(&self.tail_bytes());
        Ok(frame)
    }

    pub(crate) fn decode_payload(payload: &[u8]) -> io::Result<Self> {
        if payload.len() < VIDEO_TAIL_LEN {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "video frame: payload is shorter than the metadata tail",
            ));
        }
        let pixel_len = payload.len() - VIDEO_TAIL_LEN;
        let pixels = &payload[..pixel_len];
        let mut reader = PayloadReader::new(&payload[pixel_len..]);
        let seq = reader.u64("video.seq")?;
        let timestamp_millis = reader.u64("video.timestamp_millis")?;
        let width = reader.u16("video.width")?;
        let height = reader.u16("video.height")?;
        let encoding = reader.u8("video.encoding")?;
        reader.finish("video frame tail")?;

        validate_shape(width, height, encoding, pixel_len).map_err(|error| {
            io::Error::new(io::ErrorKind::InvalidData, format!("video frame: {error}"))
        })?;

        Ok(Self {
            seq,
            timestamp_millis,
            width,
            height,
            encoding,
            pixels: pixels.to_vec(),
        })
    }
}

fn validate_shape(
    width: u16,
    height: u16,
    encoding: u8,
    pixel_len: usize,
) -> Result<(), VideoError> {
    if width == 0 || height == 0 {
        return Err(VideoError::EmptyDimension);
    }
    if width > MAX_VIDEO_DIMENSION || height > MAX_VIDEO_DIMENSION {
        return Err(VideoError::DimensionTooLarge { width, height });
    }
    if encoding != ENCODING_RAW_RGBA8 {
        return Err(VideoError::UnsupportedEncoding(encoding));
    }
    let expected = width as u64 * height as u64 * 4;
    if expected > MAX_VIDEO_BYTES as u64 {
        return Err(VideoError::TooLarge {
            bytes: expected,
            limit: MAX_VIDEO_BYTES,
        });
    }
    let expected = expected as usize;
    if pixel_len != expected {
        return Err(VideoError::SizeMismatch {
            expected,
            actual: pixel_len,
        });
    }
    Ok(())
}

fn video_io_invalid_input(error: VideoError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, format!("video frame: {error}"))
}

/// Validation failure for a video frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoError {
    EmptyDimension,
    DimensionTooLarge { width: u16, height: u16 },
    UnsupportedEncoding(u8),
    SizeMismatch { expected: usize, actual: usize },
    TooLarge { bytes: u64, limit: usize },
}

impl fmt::Display for VideoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyDimension => formatter.write_str("width and height must both be non-zero"),
            Self::DimensionTooLarge { width, height } => write!(
                formatter,
                "dimensions {width}x{height} exceed the {MAX_VIDEO_DIMENSION} pixel limit"
            ),
            Self::UnsupportedEncoding(encoding) => {
                write!(formatter, "encoding {encoding} is not supported")
            }
            Self::SizeMismatch { expected, actual } => write!(
                formatter,
                "pixel buffer is {actual} bytes but {expected} bytes are required"
            ),
            Self::TooLarge { bytes, limit } => write!(
                formatter,
                "RGBA buffer would be {bytes} bytes, exceeding the {limit} byte limit"
            ),
        }
    }
}

impl std::error::Error for VideoError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_rgba_frame_round_trips() {
        let rgba = vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
        ];
        let frame = VideoFrame::from_rgba(7, 1234, 2, 2, &rgba).unwrap();
        assert_eq!(frame.as_rgba(), rgba.as_slice());

        let encoded = frame.encode_frame(FrameKind::Video).unwrap();
        assert_eq!(encoded[4], FrameKind::Video.code());
        let payload = &encoded[FRAME_HEADER_LEN..];
        let decoded = VideoFrame::decode_payload(payload).unwrap();
        assert_eq!(decoded, frame);
    }

    #[test]
    fn pixel_length_must_match_dimensions() {
        let error = VideoFrame::from_rgba(0, 0, 2, 2, &[0u8; 15]).unwrap_err();
        assert_eq!(
            error,
            VideoError::SizeMismatch {
                expected: 16,
                actual: 15
            }
        );
    }

    #[test]
    fn empty_and_oversized_dimensions_are_rejected() {
        assert_eq!(
            VideoFrame::from_rgba(0, 0, 0, 16, &[]).unwrap_err(),
            VideoError::EmptyDimension
        );
        assert_eq!(
            VideoFrame::from_rgba(0, 0, MAX_VIDEO_DIMENSION + 1, 16, &[]).unwrap_err(),
            VideoError::DimensionTooLarge {
                width: MAX_VIDEO_DIMENSION + 1,
                height: 16
            }
        );
    }

    #[test]
    fn uncompressed_budget_is_enforced() {
        let frame = VideoFrame {
            seq: 0,
            timestamp_millis: 0,
            width: MAX_VIDEO_DIMENSION,
            height: MAX_VIDEO_DIMENSION,
            encoding: ENCODING_RAW_RGBA8,
            pixels: Vec::new(),
        };
        assert!(matches!(frame.validate(), Err(VideoError::TooLarge { .. })));
    }

    #[test]
    fn unsupported_encoding_is_rejected() {
        let frame = VideoFrame {
            seq: 0,
            timestamp_millis: 0,
            width: 2,
            height: 2,
            encoding: 9,
            pixels: vec![0; 16],
        };
        assert_eq!(frame.validate(), Err(VideoError::UnsupportedEncoding(9)));
    }
}
