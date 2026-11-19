//! Wire contract for the self-made cloud streaming transport.
//!
//! Every message travels in one binary frame:
//!
//! ```text
//! [payload_len: u32 little-endian][kind: u8][payload: payload_len bytes]
//! ```
//!
//! The payload layout of each kind is defined in [`crate::messages`]. Video
//! payloads place raw RGBA bytes first and a fixed metadata tail last, so a
//! receiver can adopt the pixel region without an extra copy.
//!
//! This module intentionally contains only protocol constants and small
//! helpers. Transport state lives in [`crate::session`].

use std::time::Duration;

/// Version carried by `Hello` and `Welcome`.
pub const PROTOCOL_VERSION: u16 = 1;

/// Length of the fixed frame header.
pub const FRAME_HEADER_LEN: usize = 5;

/// Hard upper bound for one frame payload (16 MiB).
///
/// The transport never allocates or accepts a larger single message. Video
/// frames are additionally checked against this limit when queued.
pub const MAX_FRAME_PAYLOAD: usize = 16 * 1024 * 1024;

/// Largest representable frame kind byte.
pub const MAX_FRAME_KIND: u8 = u8::MAX;

/// Maximum UTF-8 byte length of `Hello.client_name`.
pub const MAX_CLIENT_NAME: usize = 64;

/// Maximum number of held keys carried by one input frame.
pub const MAX_HELD_KEYS: usize = 256;

/// Maximum accepted width or height of a video frame.
pub const MAX_VIDEO_DIMENSION: u16 = 3840;

/// Maximum accepted raw RGBA payload of a [`crate::video::VideoFrame`].
pub const MAX_VIDEO_BYTES: usize = 32 * 1024 * 1024;

/// Metadata bytes at the end of a video payload: seq, timestamp, width,
/// height and encoding.
pub const VIDEO_TAIL_LEN: usize = 21;

/// Maximum number of frames waiting in one peer send queue.
pub const MAX_QUEUE_FRAMES: usize = 64;

/// Maximum number of queued bytes waiting in one peer send queue.
pub const MAX_QUEUE_BYTES: usize = 32 * 1024 * 1024;

/// Maximum advertised frames per second accepted by a session.
pub const MAX_FPS: u16 = 240;

/// Client and server must complete the handshake inside this window.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(2);

/// Default inactivity window for an established stream.
pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// Default stream width advertised by `Welcome`.
pub const DEFAULT_WIDTH: u16 = 1280;

/// Default stream height advertised by `Welcome`.
pub const DEFAULT_HEIGHT: u16 = 720;

/// Default stream frame-rate cap advertised by `Welcome`.
pub const DEFAULT_MAX_FPS: u16 = 60;

/// Default number of messages allowed in a send queue.
pub const DEFAULT_QUEUE_FRAMES: usize = 32;

/// Size of one non-blocking receive chunk.
pub const READ_CHUNK: usize = 64 * 1024;

/// Bytes read by one `poll` call before returning to the caller.
pub const READ_BUDGET: usize = MAX_FRAME_PAYLOAD + READ_CHUNK;

/// Maximum live bytes retained by a frame reader.
pub const MAX_BUFFERED_BYTES: usize = MAX_FRAME_PAYLOAD + FRAME_HEADER_LEN + READ_CHUNK;

/// Maximum messages decoded by one `poll` call.
pub const MAX_MESSAGES_PER_POLL: usize = 64;

/// Bytes written by one flush pass before returning to the caller.
pub const FLUSH_BUDGET: usize = 4 * 1024 * 1024;

/// Close reasons carried by `Bye`.
pub mod bye_reason {
    /// Orderly shutdown requested by the local side.
    pub const NORMAL: u8 = 0;
    /// Peer speaks a different protocol version.
    pub const PROTOCOL_MISMATCH: u8 = 1;
    /// Session token does not match the listener token.
    pub const TOKEN_MISMATCH: u8 = 2;
    /// Session instance does not match the listener instance.
    pub const INSTANCE_MISMATCH: u8 = 3;
    /// The peer did not finish the handshake in time.
    pub const HANDSHAKE_TIMEOUT: u8 = 4;
    /// No traffic arrived inside the idle window.
    pub const IDLE_TIMEOUT: u8 = 5;
    /// A malformed or unexpected message was received.
    pub const PROTOCOL_ERROR: u8 = 6;
    /// The hosting process is shutting the session down.
    pub const SERVER_SHUTDOWN: u8 = 7;
    /// The client requested a clean disconnect.
    pub const CLIENT_SHUTDOWN: u8 = 8;
}

/// Wall-clock milliseconds since the Unix epoch.
///
/// Input latency accounting compares this value with the producer timestamp
/// carried by [`crate::input::InputFrame::client_millis`]. It is meaningful
/// when both sides use the same epoch (loopback, or hosts with synchronised
/// clocks); otherwise the window measures clock offset, not network delay.
pub fn epoch_millis() -> u64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => u64::try_from(duration.as_millis()).unwrap_or(u64::MAX),
        Err(_) => 0,
    }
}
