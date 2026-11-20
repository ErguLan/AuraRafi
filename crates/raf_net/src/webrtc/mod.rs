//! WebRTC control plane for AuraRafi cloud sessions.
//!
//! This module owns the part of a WebRTC session that can be built, parsed,
//! signalled and tested without a media stack:
//!
//! - [`sdp`] builds and parses an SDP offer/answer with one bundled
//!   data-channel section and one send-only video section.
//! - [`ice`] models trickled ICE candidates and their SDP attribute lines.
//! - [`signaling`] is a self-made, line-delimited JSON signaling server over
//!   non-blocking TCP with a token handshake and bounded queues.
//! - [`state`] is the bounded offer/answer negotiation state machine.
//!
//! # Plane boundary
//!
//! Everything here is the control plane. No media packet is sent, encrypted,
//! retransmitted or decoded by this module.
//!
//! The media plane is deliberately absent: there is no STUN connectivity
//! check, no DTLS handshake, no certificate or fingerprint verification, no
//! SRTP keying, no SCTP association, no RTP packetization, no VP8/H264/AV1
//! encoder or decoder, and no congestion control. The `RAF1` payload label in
//! the generated SDP is an AuraRafi-internal placeholder for the future
//! transport; browsers and other SDP peers do not know it.
//!
//! A successful [`WebRtcSession::on_connected`] call means a future media
//! layer reported a connected path. This module itself never invents that
//! signal.
//!
//! # Security boundary
//!
//! The signaling server compares the session token without logging or echoing
//! it, bounds every received line, and closes the connection after a failed
//! handshake. It does not terminate TLS: bind it to loopback or place it behind
//! an operator-terminated tunnel. See `docs/WEBRTC.md` for the exact limits and
//! the media-plane gate.

pub mod ice;
pub mod sdp;
pub mod signaling;
pub mod state;

pub use ice::{
    parse_candidate, parse_end_of_candidates, IceCandidate, IceCandidateType, END_OF_CANDIDATES,
    MAX_ADDRESS_LEN, MAX_CANDIDATE_LINE_BYTES, MAX_COMPONENT, MAX_FOUNDATION_LEN,
    MAX_ICE_CREDENTIAL_LEN, MAX_RELATED_ENDPOINT_LEN, MAX_TRANSPORT_LEN, MIN_COMPONENT,
    MIN_ICE_CREDENTIAL_LEN,
};
pub use sdp::{
    build_answer, build_offer, parse_session_description, validate_answer, ParsedSessionDescription,
    SdpDirection, SdpMediaKind, SdpMediaSection, SdpOfferParams, SdpRtpmap, SdpSetup, SdpType,
    SessionDescription, DEFAULT_VIDEO_PAYLOAD_TYPE, FINGERPRINT_LEN, MAX_DATACHANNEL_MESSAGE_SIZE,
    MAX_MID_LEN, MAX_SDP_BYTES, MAX_SDP_LINE_BYTES,
    MAX_SDP_MEDIA_SECTIONS, MAX_SDP_PAYLOAD_TYPES, MAX_SESSION_ID_LEN, MAX_VIDEO_PAYLOAD_TYPE,
    MEDIA_PORT, MIN_VIDEO_PAYLOAD_TYPE, RAF_VIDEO_CLOCK_RATE,
    RAF_VIDEO_CODEC, SCTP_PORT,
};
pub use signaling::{
    signal_code, SignalPeer, SignalingMessage, SignalingServer, MAX_BYE_REASON,
    MAX_SIGNALING_ERROR_LEN, MAX_SIGNALING_LINE_BYTES, MAX_SIGNALING_MESSAGES_PER_POLL,
    MAX_SIGNALING_QUEUE, MAX_SIGNALING_QUEUE_BYTES, SIGNALING_HANDSHAKE_TIMEOUT,
    SIGNALING_PROTOCOL_VERSION,
};
pub use state::{
    WebRtcEvent, WebRtcSession, WebRtcState, CONNECT_TIMEOUT, MAX_REMOTE_CANDIDATES,
    NEGOTIATION_TIMEOUT,
};

use std::fmt;
use std::io;

/// Category of a control-plane rejection.
///
/// The category is stable; the detail string is meant for diagnostics and may
/// gain more precise context over time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WebRtcErrorKind {
    /// Session description text or offer/answer parameters are invalid.
    Sdp,
    /// An ICE candidate field or candidate line is invalid.
    Candidate,
    /// A negotiation step is not legal in the current state.
    Negotiation,
    /// A signaling message payload is invalid.
    Signaling,
    /// A configured bound was exceeded.
    Limit,
}

impl fmt::Display for WebRtcErrorKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Sdp => "sdp",
            Self::Candidate => "candidate",
            Self::Negotiation => "negotiation",
            Self::Signaling => "signaling",
            Self::Limit => "limit",
        };
        formatter.write_str(label)
    }
}

/// Bounded control-plane failure.
///
/// Transport failures stay [`io::Error`] values; this type is only used where
/// SDP, ICE, signaling or negotiation input is validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebRtcError {
    kind: WebRtcErrorKind,
    detail: String,
}

impl WebRtcError {
    pub(crate) fn new(kind: WebRtcErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }

    pub(crate) fn sdp(detail: impl Into<String>) -> Self {
        Self::new(WebRtcErrorKind::Sdp, detail)
    }

    pub(crate) fn candidate(detail: impl Into<String>) -> Self {
        Self::new(WebRtcErrorKind::Candidate, detail)
    }

    pub(crate) fn negotiation(detail: impl Into<String>) -> Self {
        Self::new(WebRtcErrorKind::Negotiation, detail)
    }

    pub(crate) fn signaling(detail: impl Into<String>) -> Self {
        Self::new(WebRtcErrorKind::Signaling, detail)
    }

    pub(crate) fn limit(detail: impl Into<String>) -> Self {
        Self::new(WebRtcErrorKind::Limit, detail)
    }

    pub fn kind(&self) -> WebRtcErrorKind {
        self.kind
    }

    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for WebRtcError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.kind, self.detail)
    }
}

impl std::error::Error for WebRtcError {}

impl From<WebRtcError> for io::Error {
    fn from(error: WebRtcError) -> Self {
        io::Error::new(io::ErrorKind::InvalidInput, format!("webrtc {error}"))
    }
}
