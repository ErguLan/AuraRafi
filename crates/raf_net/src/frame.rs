//! Binary framing for the cloud streaming transport.
//!
//! A frame is `[u32 LE payload_len][u8 kind][payload]`. Both directions use
//! the same layout. The reader accumulates bytes, rejects oversized or
//! unknown frames before allocating, and reports a truncated frame when the
//! peer disconnects in the middle of a message. The writer keeps a bounded
//! queue of complete frames and advances an offset on partial writes, so a
//! non-blocking socket never splits one message into another.

use crate::protocol::{
    DEFAULT_QUEUE_FRAMES, FLUSH_BUDGET, FRAME_HEADER_LEN, MAX_BUFFERED_BYTES, MAX_FRAME_KIND,
    MAX_FRAME_PAYLOAD, MAX_QUEUE_BYTES, MAX_QUEUE_FRAMES, READ_CHUNK, VIDEO_TAIL_LEN,
};
use std::collections::VecDeque;
use std::fmt;
use std::io::{self, Write};

/// Message kinds carried by the frame header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameKind {
    Hello = 1,
    Welcome = 2,
    Input = 3,
    Video = 4,
    Ping = 5,
    Pong = 6,
    Bye = 7,
}

impl FrameKind {
    pub const fn code(self) -> u8 {
        self as u8
    }

    pub fn from_code(code: u8) -> io::Result<Self> {
        match code {
            1 => Ok(Self::Hello),
            2 => Ok(Self::Welcome),
            3 => Ok(Self::Input),
            4 => Ok(Self::Video),
            5 => Ok(Self::Ping),
            6 => Ok(Self::Pong),
            7 => Ok(Self::Bye),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("frame kind {code} is not defined (maximum {MAX_FRAME_KIND})"),
            )),
        }
    }
}

impl TryFrom<u8> for FrameKind {
    type Error = io::Error;

    fn try_from(value: u8) -> io::Result<Self> {
        Self::from_code(value)
    }
}

/// Build the five-byte envelope for a payload.
pub(crate) fn header(kind: FrameKind, payload_len: usize) -> io::Result<[u8; FRAME_HEADER_LEN]> {
    if payload_len > MAX_FRAME_PAYLOAD {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("frame payload {payload_len} bytes exceeds the {MAX_FRAME_PAYLOAD} byte limit"),
        ));
    }
    let mut header = [0u8; FRAME_HEADER_LEN];
    header[..4].copy_from_slice(&(payload_len as u32).to_le_bytes());
    header[4] = kind.code();
    Ok(header)
}

/// Accumulates bytes from a non-blocking socket and yields borrowed frames.
#[derive(Debug, Default)]
pub struct FrameReader {
    buffer: Vec<u8>,
    start: usize,
}

impl FrameReader {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of live bytes waiting for a complete frame.
    pub fn buffered_len(&self) -> usize {
        self.buffer.len().saturating_sub(self.start)
    }

    /// Append received bytes. The buffer is bounded even if the peer never
    /// completes a frame.
    pub fn feed(&mut self, bytes: &[u8]) -> io::Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        if self.buffer.len().saturating_add(bytes.len()) > MAX_BUFFERED_BYTES {
            self.compact();
            if self.buffered_len().saturating_add(bytes.len()) > MAX_BUFFERED_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "frame receive buffer exceeded {MAX_BUFFERED_BYTES} bytes on an unterminated frame"
                    ),
                ));
            }
        }
        self.buffer.extend_from_slice(bytes);
        Ok(())
    }

    /// Inspect the next complete frame without copying its payload.
    pub fn peek_frame(&self) -> io::Result<Option<(FrameKind, &[u8])>> {
        let view = &self.buffer[self.start..];
        if view.len() < FRAME_HEADER_LEN {
            return Ok(None);
        }
        let payload_len = u32::from_le_bytes([view[0], view[1], view[2], view[3]]) as usize;
        if payload_len > MAX_FRAME_PAYLOAD {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "frame payload {payload_len} bytes exceeds the {MAX_FRAME_PAYLOAD} byte limit"
                ),
            ));
        }
        let kind = FrameKind::from_code(view[4])?;
        let total = FRAME_HEADER_LEN + payload_len;
        if view.len() < total {
            return Ok(None);
        }
        Ok(Some((kind, &view[FRAME_HEADER_LEN..total])))
    }

    /// Retire the frame returned by the latest [`FrameReader::peek_frame`].
    pub fn consume_frame(&mut self) -> io::Result<()> {
        let total = {
            let Some((_kind, payload)) = self.peek_frame()? else {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "no complete frame is buffered",
                ));
            };
            FRAME_HEADER_LEN + payload.len()
        };
        self.start += total;
        if self.start == self.buffer.len() {
            self.buffer.clear();
            self.start = 0;
        } else if self.start >= READ_CHUNK || self.start.saturating_mul(2) >= self.buffer.len() {
            self.compact();
        }
        Ok(())
    }

    /// Verify that the peer did not disconnect inside a frame.
    pub fn finish(&self) -> io::Result<()> {
        let view = &self.buffer[self.start..];
        if view.is_empty() {
            return Ok(());
        }
        if view.len() < FRAME_HEADER_LEN {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!(
                    "stream ended inside frame header: received {} of {} bytes",
                    view.len(),
                    FRAME_HEADER_LEN
                ),
            ));
        }
        let payload_len = u32::from_le_bytes([view[0], view[1], view[2], view[3]]) as usize;
        if payload_len > MAX_FRAME_PAYLOAD {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "frame payload {payload_len} bytes exceeds the {MAX_FRAME_PAYLOAD} byte limit"
                ),
            ));
        }
        let expected = FRAME_HEADER_LEN + payload_len;
        if view.len() >= expected {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "frame data remains buffered at end of stream",
            ));
        }
        Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            format!(
                "stream ended inside frame payload: expected {payload_len} bytes, received {}",
                view.len() - FRAME_HEADER_LEN
            ),
        ))
    }

    /// Drop every buffered byte.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.start = 0;
    }

    fn compact(&mut self) {
        if self.start == 0 {
            return;
        }
        if self.start >= self.buffer.len() {
            self.buffer.clear();
            self.start = 0;
            return;
        }
        let remaining = self.buffer.len() - self.start;
        self.buffer.copy_within(self.start.., 0);
        self.buffer.truncate(remaining);
        self.start = 0;
    }
}

/// Queue admission failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueError {
    /// Frame-count or byte budget reached.
    Full,
    /// A single payload is larger than the protocol allows.
    FrameTooLarge,
    /// The caller supplied bytes whose header does not match the frame.
    Malformed,
}

impl fmt::Display for QueueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Full => formatter.write_str("the bounded send queue is full"),
            Self::FrameTooLarge => {
                formatter.write_str("the frame payload exceeds the protocol limit")
            }
            Self::Malformed => {
                formatter.write_str("the frame header is inconsistent with its length")
            }
        }
    }
}

impl std::error::Error for QueueError {}

/// One frame that completed during a flush pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompletedFrame {
    pub kind: FrameKind,
    pub video_timestamp_millis: Option<u64>,
    pub bytes: usize,
}

/// Bytes and frames fully written by one flush pass.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FlushReport {
    pub bytes_written: u64,
    pub frames_completed: u64,
}

#[derive(Debug)]
enum QueuedPayload {
    Bytes(Vec<u8>),
    Video {
        header: [u8; FRAME_HEADER_LEN],
        pixels: Vec<u8>,
        tail: [u8; VIDEO_TAIL_LEN],
    },
}

impl QueuedPayload {
    fn len(&self) -> usize {
        match self {
            Self::Bytes(bytes) => bytes.len(),
            Self::Video {
                header,
                pixels,
                tail,
            } => header.len() + pixels.len() + tail.len(),
        }
    }

    fn write_from<W: Write>(&self, out: &mut W, offset: usize) -> io::Result<usize> {
        match self {
            Self::Bytes(bytes) => out.write(&bytes[offset..]),
            Self::Video {
                header,
                pixels,
                tail,
            } => {
                if offset < header.len() {
                    return out.write(&header[offset..]);
                }
                let payload_offset = offset - header.len();
                if payload_offset < pixels.len() {
                    return out.write(&pixels[payload_offset..]);
                }
                let tail_offset = payload_offset - pixels.len();
                out.write(&tail[tail_offset..])
            }
        }
    }
}

#[derive(Debug)]
struct QueuedFrame {
    kind: FrameKind,
    payload: QueuedPayload,
    offset: usize,
    video_timestamp_millis: Option<u64>,
}

/// Bounded user-space send queue with partial-write offsets.
#[derive(Debug)]
pub struct FrameWriter {
    queue: VecDeque<QueuedFrame>,
    max_frames: usize,
    max_bytes: usize,
    queued_bytes: usize,
}

impl Default for FrameWriter {
    fn default() -> Self {
        Self::new(DEFAULT_QUEUE_FRAMES, MAX_QUEUE_BYTES)
    }
}

impl FrameWriter {
    pub fn new(max_frames: usize, max_bytes: usize) -> Self {
        Self {
            queue: VecDeque::new(),
            max_frames: max_frames.clamp(1, MAX_QUEUE_FRAMES),
            max_bytes: max_bytes.clamp(FRAME_HEADER_LEN, MAX_QUEUE_BYTES),
            queued_bytes: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn bytes_queued(&self) -> usize {
        self.queued_bytes
    }

    pub fn can_fit(&self, frame_bytes: usize) -> bool {
        self.queue.len() < self.max_frames
            && self.queued_bytes.saturating_add(frame_bytes) <= self.max_bytes
    }

    /// Queue a complete frame whose header was already written by the caller.
    pub fn enqueue_frame(&mut self, kind: FrameKind, frame: Vec<u8>) -> Result<(), QueueError> {
        if frame.len() < FRAME_HEADER_LEN {
            return Err(QueueError::Malformed);
        }
        let payload_len = u32::from_le_bytes([frame[0], frame[1], frame[2], frame[3]]) as usize;
        if payload_len > MAX_FRAME_PAYLOAD
            || frame.len() != FRAME_HEADER_LEN + payload_len
            || frame[4] != kind.code()
        {
            return Err(QueueError::Malformed);
        }
        if !self.can_fit(frame.len()) {
            return Err(QueueError::Full);
        }
        self.queued_bytes = self.queued_bytes.saturating_add(frame.len());
        self.queue.push_back(QueuedFrame {
            kind,
            payload: QueuedPayload::Bytes(frame),
            offset: 0,
            video_timestamp_millis: None,
        });
        Ok(())
    }

    /// Queue a video payload without copying its pixel buffer.
    pub fn enqueue_video(
        &mut self,
        pixels: Vec<u8>,
        tail: [u8; VIDEO_TAIL_LEN],
        video_timestamp_millis: u64,
    ) -> Result<(), QueueError> {
        let payload_len = pixels.len().saturating_add(VIDEO_TAIL_LEN);
        if payload_len > MAX_FRAME_PAYLOAD {
            return Err(QueueError::FrameTooLarge);
        }
        let total = FRAME_HEADER_LEN + payload_len;
        if !self.can_fit(total) {
            return Err(QueueError::Full);
        }
        let header =
            header(FrameKind::Video, payload_len).map_err(|_| QueueError::FrameTooLarge)?;
        self.queued_bytes = self.queued_bytes.saturating_add(total);
        self.queue.push_back(QueuedFrame {
            kind: FrameKind::Video,
            payload: QueuedPayload::Video {
                header,
                pixels,
                tail,
            },
            offset: 0,
            video_timestamp_millis: Some(video_timestamp_millis),
        });
        Ok(())
    }

    /// Remove the oldest queued video frame that has not started writing.
    ///
    /// A partially written frame is never removed, so message boundaries stay
    /// intact under backpressure.
    pub fn drop_oldest_unstarted_video(&mut self) -> bool {
        let position = self
            .queue
            .iter()
            .position(|frame| frame.offset == 0 && frame.kind == FrameKind::Video);
        let Some(position) = position else {
            return false;
        };
        if let Some(frame) = self.queue.remove(position) {
            self.queued_bytes = self.queued_bytes.saturating_sub(frame.payload.len());
            return true;
        }
        false
    }

    /// Write as many queued bytes as the socket accepts.
    ///
    /// `WouldBlock` is not an error: the queue and its partial offsets remain
    /// valid for the next call. Completed frames invoke `on_complete`.
    pub fn flush<W: Write, F: FnMut(CompletedFrame)>(
        &mut self,
        out: &mut W,
        mut on_complete: F,
    ) -> io::Result<FlushReport> {
        let mut bytes_written = 0u64;
        let mut frames_completed = 0u64;
        let mut budget = FLUSH_BUDGET;
        let mut interrupted = 0u32;
        while budget > 0 {
            let complete = {
                let Some(frame) = self.queue.front_mut() else {
                    break;
                };
                let total = frame.payload.len();
                match frame.payload.write_from(out, frame.offset) {
                    Ok(0) => {
                        return Err(io::Error::new(
                            io::ErrorKind::WriteZero,
                            "stream write returned zero bytes",
                        ));
                    }
                    Ok(written) => {
                        frame.offset += written;
                        budget = budget.saturating_sub(written);
                        bytes_written = bytes_written.saturating_add(written as u64);
                        frame.offset >= total
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        return Ok(FlushReport {
                            bytes_written,
                            frames_completed,
                        });
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                        interrupted = interrupted.saturating_add(1);
                        if interrupted > 8 {
                            return Err(error);
                        }
                        false
                    }
                    Err(error) => return Err(error),
                }
            };
            if complete {
                if let Some(frame) = self.queue.pop_front() {
                    let bytes = frame.payload.len();
                    self.queued_bytes = self.queued_bytes.saturating_sub(bytes);
                    frames_completed = frames_completed.saturating_add(1);
                    on_complete(CompletedFrame {
                        kind: frame.kind,
                        video_timestamp_millis: frame.video_timestamp_millis,
                        bytes,
                    });
                }
            }
        }
        Ok(FlushReport {
            bytes_written,
            frames_completed,
        })
    }

    /// Drop every queued frame. Used only when a peer is already closed.
    pub fn clear(&mut self) {
        self.queue.clear();
        self.queued_bytes = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(kind: FrameKind, payload: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(FRAME_HEADER_LEN + payload.len());
        bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        bytes.push(kind.code());
        bytes.extend_from_slice(payload);
        bytes
    }

    fn next_frame(reader: &mut FrameReader) -> Option<(FrameKind, Vec<u8>)> {
        let (kind, payload) = reader.peek_frame().expect("peek frame")?;
        let owned = payload.to_vec();
        reader.consume_frame().expect("consume frame");
        Some((kind, owned))
    }

    struct TrickleWriter {
        output: Vec<u8>,
        per_call: usize,
        pauses: usize,
    }

    impl Write for TrickleWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            if self.pauses > 0 {
                self.pauses -= 1;
                return Err(io::Error::new(io::ErrorKind::WouldBlock, "pause"));
            }
            if buffer.is_empty() {
                return Ok(0);
            }
            let len = buffer.len().min(self.per_call).max(1);
            self.output.extend_from_slice(&buffer[..len]);
            Ok(len)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn partial_reads_reassemble_one_frame() {
        let bytes = frame(FrameKind::Input, b"payload");
        let mut reader = FrameReader::new();
        for byte in &bytes[..bytes.len() - 1] {
            reader.feed(&[*byte]).unwrap();
            assert!(reader.peek_frame().unwrap().is_none());
        }
        reader.feed(&bytes[bytes.len() - 1..]).unwrap();
        let (kind, payload) = next_frame(&mut reader).expect("complete frame");
        assert_eq!(kind, FrameKind::Input);
        assert_eq!(payload, b"payload");
        assert!(reader.finish().is_ok());
    }

    #[test]
    fn partial_writes_preserve_frame_boundaries() {
        let mut writer = FrameWriter::new(8, MAX_QUEUE_BYTES);
        writer
            .enqueue_frame(FrameKind::Input, frame(FrameKind::Input, b"first"))
            .unwrap();
        writer
            .enqueue_frame(FrameKind::Video, frame(FrameKind::Video, b"second"))
            .unwrap();
        let mut sink = TrickleWriter {
            output: Vec::new(),
            per_call: 3,
            pauses: 2,
        };
        let mut completed = 0u64;
        let mut guard = 0u32;
        while !writer.is_empty() {
            writer
                .flush(&mut sink, |_| completed = completed.saturating_add(1))
                .unwrap();
            guard = guard.saturating_add(1);
            assert!(guard < 10_000, "flush made no progress");
        }
        assert_eq!(completed, 2);

        let mut reader = FrameReader::new();
        for chunk in sink.output.chunks(2) {
            reader.feed(chunk).unwrap();
        }
        let (first_kind, first) = next_frame(&mut reader).expect("first frame");
        let (second_kind, second) = next_frame(&mut reader).expect("second frame");
        assert_eq!(first_kind, FrameKind::Input);
        assert_eq!(first, b"first");
        assert_eq!(second_kind, FrameKind::Video);
        assert_eq!(second, b"second");
        assert!(reader.finish().is_ok());
    }

    #[test]
    fn oversized_payload_is_rejected_before_buffering_it() {
        let mut reader = FrameReader::new();
        let oversized = (MAX_FRAME_PAYLOAD as u32 + 1).to_le_bytes();
        reader
            .feed(&[
                oversized[0],
                oversized[1],
                oversized[2],
                oversized[3],
                FrameKind::Ping.code(),
            ])
            .unwrap();
        let error = reader.peek_frame().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("exceeds"));
    }

    #[test]
    fn truncated_frame_reports_unexpected_eof() {
        let bytes = frame(FrameKind::Input, b"hello");
        let mut reader = FrameReader::new();
        reader.feed(&bytes[..8]).unwrap();
        assert!(reader.peek_frame().unwrap().is_none());
        let error = reader.finish().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
        assert!(error.to_string().contains("payload"));
    }

    #[test]
    fn header_only_frame_is_unterminated() {
        let bytes = frame(FrameKind::Ping, &[0u8; 8]);
        let mut reader = FrameReader::new();
        reader.feed(&bytes[..FRAME_HEADER_LEN]).unwrap();
        let error = reader.finish().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn unknown_frame_kind_is_rejected() {
        let mut reader = FrameReader::new();
        reader.feed(&[0, 0, 0, 0, 99]).unwrap();
        let error = reader.peek_frame().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("99"));
    }

    #[test]
    fn send_queue_is_bounded_by_frames() {
        let mut writer = FrameWriter::new(1, MAX_QUEUE_BYTES);
        writer
            .enqueue_frame(
                FrameKind::Ping,
                frame(FrameKind::Ping, &[1, 2, 3, 4, 5, 6, 7, 8]),
            )
            .unwrap();
        assert_eq!(
            writer.enqueue_frame(FrameKind::Ping, frame(FrameKind::Ping, &[0; 8])),
            Err(QueueError::Full)
        );
    }

    #[test]
    fn oldest_unstarted_video_can_be_evicted() {
        let mut writer = FrameWriter::new(4, MAX_QUEUE_BYTES);
        writer
            .enqueue_video(vec![0; 16], [0; VIDEO_TAIL_LEN], 10)
            .unwrap();
        writer
            .enqueue_video(vec![1; 16], [0; VIDEO_TAIL_LEN], 11)
            .unwrap();
        assert!(writer.drop_oldest_unstarted_video());
        assert_eq!(writer.len(), 1);
        assert!(writer.drop_oldest_unstarted_video());
        assert!(!writer.drop_oldest_unstarted_video());
    }
}
