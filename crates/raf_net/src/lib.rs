//! # raf_net
//!
//! Self-made streaming transport for AuraRafi cloud sessions.
//!
//! The crate owns the wire contract only: binary framing, an authenticated
//! session handshake, authoritative input frames, uncompressed RGBA video
//! frames, bounded send queues, partial-write handling and latency
//! accounting. Rendering, windowing and editor integration stay outside this
//! crate.
//!
//! Typical host side:
//!
//! 1. Bind a [`StreamServer`] with a non-nil token.
//! 2. Poll [`StreamServer::accept`] from the host loop.
//! 3. Poll each [`StreamPeer`] and feed decoded `Input` messages into an
//!    [`InputTracker`].
//! 4. Build [`VideoFrame`]s from the renderer and call
//!    [`StreamPeer::send`] with `StreamMessage::Video`.
//!
//! Typical client side:
//!
//! 1. Call [`StreamPeer::connect`] with the same token and instance.
//! 2. Poll until `Welcome` arrives and [`StreamPeer::is_open`] is true.
//! 3. Send `StreamMessage::Input` and consume `StreamMessage::Video`.

mod codec;

pub mod frame;
pub mod input;
pub mod latency;
pub mod messages;
pub mod protocol;
pub mod session;
pub mod video;

pub use frame::{CompletedFrame, FlushReport, FrameKind, FrameReader, FrameWriter, QueueError};
pub use input::{
    input_key_code, input_key_from_code, InputError, InputFrame, InputTracker, BUTTON_BACK,
    BUTTON_FORWARD, BUTTON_MASK, BUTTON_MIDDLE, BUTTON_PRIMARY, BUTTON_SECONDARY, MAX_POINTER_ABS,
    MODIFIER_ALT, MODIFIER_COMMAND, MODIFIER_CONTROL, MODIFIER_MASK, MODIFIER_SHIFT,
};
pub use latency::{LatencyWindow, PeerMetrics, LATENCY_SAMPLES};
pub use messages::StreamMessage;
pub use protocol::{
    bye_reason, epoch_millis, FRAME_HEADER_LEN, MAX_CLIENT_NAME, MAX_FPS, MAX_FRAME_KIND,
    MAX_FRAME_PAYLOAD, MAX_HELD_KEYS, MAX_QUEUE_BYTES, MAX_QUEUE_FRAMES, MAX_VIDEO_BYTES,
    MAX_VIDEO_DIMENSION, PROTOCOL_VERSION, VIDEO_TAIL_LEN,
};
pub use session::{StreamConfig, StreamPeer, StreamServer};
pub use video::{VideoError, VideoFrame, ENCODING_RAW_RGBA8};
