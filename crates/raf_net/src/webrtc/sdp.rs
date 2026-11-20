//! SDP offer/answer model for the WebRTC control plane.
//!
//! The builder emits one deterministic BUNDLE session with two media
//! sections:
//!
//! ```text
//! m=application 9 UDP/DTLS/SCTP webrtc-datachannel   mid 0
//! m=video       9 UDP/TLS/RTP/SAVPF <pt>             mid 1, sendonly
//! ```
//!
//! The answer mirrors the offered BUNDLE order and payload types, switches the
//! video direction to `recvonly` and uses `a=setup:active`.
//!
//! SDP does not carry an explicit offer/answer tag. The parser follows the
//! setup role instead: `a=setup:actpass` is an offer, `a=setup:active` or
//! `a=setup:passive` is an answer. This is the convention both builders emit.
//!
//! `RAF1` is an AuraRafi-internal payload label, not a codec that browsers or
//! external peers can decode. The media plane is not implemented here; see
//! `docs/WEBRTC.md`.

use super::ice::validate_ice_credential;
use super::WebRtcError;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fmt::Write as _;
use std::str::FromStr;

/// Hard limit for one complete SDP document.
pub const MAX_SDP_BYTES: usize = 64 * 1024;

/// Hard limit for one SDP line, excluding the line terminator.
pub const MAX_SDP_LINE_BYTES: usize = 1024;

/// Maximum media sections accepted in one description.
pub const MAX_SDP_MEDIA_SECTIONS: usize = 8;

/// Maximum RTP payload formats accepted on one video m-line.
pub const MAX_SDP_PAYLOAD_TYPES: usize = 8;

/// Maximum byte length of the `o=` session id.
pub const MAX_SESSION_ID_LEN: usize = 64;

/// Maximum byte length of a BUNDLE `a=mid` value.
pub const MAX_MID_LEN: usize = 32;

/// Length of a SHA-256 fingerprint in `AA:BB:...` form (32 bytes).
pub const FINGERPRINT_LEN: usize = 95;

/// Default dynamic payload type used by [`build_offer`].
pub const DEFAULT_VIDEO_PAYLOAD_TYPE: u8 = 96;

/// Lowest valid dynamic RTP payload type.
pub const MIN_VIDEO_PAYLOAD_TYPE: u8 = 96;

/// Highest valid dynamic RTP payload type.
pub const MAX_VIDEO_PAYLOAD_TYPE: u8 = 127;

/// Transport port advertised by both bundled media sections.
pub const MEDIA_PORT: u16 = 9;

/// SCTP port advertised by the data-channel section.
pub const SCTP_PORT: u16 = 5_000;

/// Largest data-channel message advertised by the data-channel section.
pub const MAX_DATACHANNEL_MESSAGE_SIZE: u32 = 262_144;

/// AuraRafi-internal video payload label. Not a browser codec.
pub const RAF_VIDEO_CODEC: &str = "RAF1";

/// RTP clock rate paired with [`RAF_VIDEO_CODEC`].
pub const RAF_VIDEO_CLOCK_RATE: u32 = 90_000;

const DATACHANNEL_MID: &str = "0";
const VIDEO_MID: &str = "1";
const APPLICATION_PROTOCOL: &str = "UDP/DTLS/SCTP";
const DATACHANNEL_FORMAT: &str = "webrtc-datachannel";
const VIDEO_PROTOCOL: &str = "UDP/TLS/RTP/SAVPF";
const MAX_CODEC_NAME_LEN: usize = 32;

/// Offer or answer tag of a [`SessionDescription`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SdpType {
    Offer,
    Answer,
}

impl SdpType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Offer => "offer",
            Self::Answer => "answer",
        }
    }
}

impl fmt::Display for SdpType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One validated SDP document and its offer/answer tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionDescription {
    pub sdp_type: SdpType,
    pub sdp: String,
}

impl SessionDescription {
    /// Parse the document and require that its setup role agrees with
    /// [`SessionDescription::sdp_type`].
    pub fn validate(&self) -> Result<(), WebRtcError> {
        let parsed = parse_session_description(&self.sdp)?;
        if parsed.sdp_type != self.sdp_type {
            return Err(WebRtcError::sdp(format!(
                "session description is labelled {} but its setup line describes {}",
                self.sdp_type, parsed.sdp_type
            )));
        }
        Ok(())
    }
}

/// Identity and ICE/DTLS material for one offer or answer.
///
/// `fingerprint_sha256` is the raw `AA:BB:...` value without the `sha-256`
/// algorithm prefix. It is normalized to uppercase in generated SDP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdpOfferParams {
    pub session_id: String,
    pub ice_ufrag: String,
    pub ice_pwd: String,
    pub fingerprint_sha256: String,
    pub video_payload_type: u8,
}

impl SdpOfferParams {
    pub fn new(
        session_id: impl Into<String>,
        ice_ufrag: impl Into<String>,
        ice_pwd: impl Into<String>,
        fingerprint_sha256: impl Into<String>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            ice_ufrag: ice_ufrag.into(),
            ice_pwd: ice_pwd.into(),
            fingerprint_sha256: fingerprint_sha256.into(),
            video_payload_type: DEFAULT_VIDEO_PAYLOAD_TYPE,
        }
    }

    /// Check every parameter that is written into the SDP.
    pub fn validate(&self) -> Result<(), WebRtcError> {
        validate_sdp_token(&self.session_id, "session_id", MAX_SESSION_ID_LEN)?;
        validate_ice_credential(&self.ice_ufrag, "ice_ufrag")?;
        validate_ice_credential(&self.ice_pwd, "ice_pwd")?;
        validate_fingerprint(&self.fingerprint_sha256)?;
        validate_payload_type(self.video_payload_type)
    }
}

/// `a=setup` role of a session description.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdpSetup {
    Active,
    Passive,
    ActPass,
}

impl SdpSetup {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Passive => "passive",
            Self::ActPass => "actpass",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "active" => Some(Self::Active),
            "passive" => Some(Self::Passive),
            "actpass" => Some(Self::ActPass),
            _ => None,
        }
    }
}

impl fmt::Display for SdpSetup {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Media direction attribute. `sendrecv` is the SDP default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdpDirection {
    SendRecv,
    SendOnly,
    RecvOnly,
    Inactive,
}

impl SdpDirection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SendRecv => "sendrecv",
            Self::SendOnly => "sendonly",
            Self::RecvOnly => "recvonly",
            Self::Inactive => "inactive",
        }
    }
}

/// Bundled media kind supported by this control plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdpMediaKind {
    Application,
    Video,
}

/// One parsed `a=rtpmap` value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdpRtpmap {
    pub payload_type: u8,
    pub encoding: String,
    pub clock_rate: u32,
}

/// One parsed media section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdpMediaSection {
    pub kind: SdpMediaKind,
    pub mid: String,
    /// RTP payload formats listed on the m-line, in order.
    pub payload_types: Vec<u8>,
    pub rtpmaps: Vec<SdpRtpmap>,
    pub direction: SdpDirection,
    pub sctp_port: Option<u16>,
    pub max_message_size: Option<u32>,
}

/// Parsed contents of a validated session description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSessionDescription {
    pub sdp_type: SdpType,
    pub session_id: String,
    pub ice_ufrag: String,
    pub ice_pwd: String,
    pub fingerprint_sha256: String,
    pub setup: SdpSetup,
    /// Mids listed by `a=group:BUNDLE`, in order.
    pub bundle_mids: Vec<String>,
    /// Mids of the media sections, in order.
    pub mids: Vec<String>,
    /// First payload type of the video section, when one exists.
    pub video_payload_type: Option<u8>,
    pub media: Vec<SdpMediaSection>,
}

impl ParsedSessionDescription {
    /// Locate the video section by mid.
    pub fn video_section(&self) -> Option<&SdpMediaSection> {
        self.media
            .iter()
            .find(|section| section.kind == SdpMediaKind::Video)
    }

    /// Locate the data-channel section by mid.
    pub fn datachannel_section(&self) -> Option<&SdpMediaSection> {
        self.media
            .iter()
            .find(|section| section.kind == SdpMediaKind::Application)
    }
}

/// Build the deterministic control-plane offer.
pub fn build_offer(params: &SdpOfferParams) -> Result<SessionDescription, WebRtcError> {
    params.validate()?;
    let fingerprint = params.fingerprint_sha256.to_ascii_uppercase();
    let mut sdp = String::with_capacity(1024);
    let _ = write!(
        sdp,
        "v=0\r\n\
         o=- {} 2 IN IP4 127.0.0.1\r\n\
         s=-\r\n\
         t=0 0\r\n\
         a=group:BUNDLE {DATACHANNEL_MID} {VIDEO_MID}\r\n\
         a=ice-ufrag:{}\r\n\
         a=ice-pwd:{}\r\n\
         a=fingerprint:sha-256 {fingerprint}\r\n\
         a=setup:actpass\r\n\
         m=application {MEDIA_PORT} {APPLICATION_PROTOCOL} {DATACHANNEL_FORMAT}\r\n\
         c=IN IP4 0.0.0.0\r\n\
         a=mid:{DATACHANNEL_MID}\r\n\
         a=sctp-port:{SCTP_PORT}\r\n\
         a=max-message-size:{MAX_DATACHANNEL_MESSAGE_SIZE}\r\n\
         m=video {MEDIA_PORT} {VIDEO_PROTOCOL} {}\r\n\
         c=IN IP4 0.0.0.0\r\n\
         a=mid:{VIDEO_MID}\r\n\
         a=sendonly\r\n\
         a=rtpmap:{} {RAF_VIDEO_CODEC}/{RAF_VIDEO_CLOCK_RATE}\r\n\
         a=rtcp-mux\r\n",
        params.session_id,
        params.ice_ufrag,
        params.ice_pwd,
        params.video_payload_type,
        params.video_payload_type,
    );
    Ok(SessionDescription {
        sdp_type: SdpType::Offer,
        sdp,
    })
}

/// Build an answer that mirrors `offer`.
///
/// The answer uses its own ICE credentials and fingerprint from `params`, but
/// keeps the offer's BUNDLE mids, media order, payload types and rtpmaps, as
/// required for a mirrored control-plane session. `params.video_payload_type`
/// is therefore validated but not used; the offer's payload type wins.
pub fn build_answer(
    offer: &SessionDescription,
    params: &SdpOfferParams,
) -> Result<SessionDescription, WebRtcError> {
    if offer.sdp_type != SdpType::Offer {
        return Err(WebRtcError::sdp(
            "an answer can only be built for a description labelled offer",
        ));
    }
    params.validate()?;
    let parsed = parse_session_description(&offer.sdp)?;
    if parsed.sdp_type != SdpType::Offer {
        return Err(WebRtcError::sdp(
            "the supplied SDP is not an offer (a=setup:actpass is required)",
        ));
    }
    for section in &parsed.media {
        if section.kind == SdpMediaKind::Video
            && matches!(section.direction, SdpDirection::RecvOnly | SdpDirection::Inactive)
        {
            return Err(WebRtcError::sdp(
                "the offered video section cannot receive media; a recvonly answer would be invalid",
            ));
        }
    }

    let fingerprint = params.fingerprint_sha256.to_ascii_uppercase();
    let bundle = parsed.bundle_mids.join(" ");
    let mut sdp = String::with_capacity(1024);
    let _ = write!(
        sdp,
        "v=0\r\n\
         o=- {} 2 IN IP4 127.0.0.1\r\n\
         s=-\r\n\
         t=0 0\r\n\
         a=group:BUNDLE {bundle}\r\n\
         a=ice-ufrag:{}\r\n\
         a=ice-pwd:{}\r\n\
         a=fingerprint:sha-256 {fingerprint}\r\n\
         a=setup:active\r\n",
        params.session_id, params.ice_ufrag, params.ice_pwd,
    );

    for section in &parsed.media {
        match section.kind {
            SdpMediaKind::Application => {
                let port = section.sctp_port.unwrap_or(SCTP_PORT);
                let max_message_size = section
                    .max_message_size
                    .unwrap_or(MAX_DATACHANNEL_MESSAGE_SIZE);
                let _ = write!(
                    sdp,
                    "m=application {MEDIA_PORT} {APPLICATION_PROTOCOL} {DATACHANNEL_FORMAT}\r\n\
                     c=IN IP4 0.0.0.0\r\n\
                     a=mid:{}\r\n\
                     a=sctp-port:{port}\r\n\
                     a=max-message-size:{max_message_size}\r\n",
                    section.mid,
                );
            }
            SdpMediaKind::Video => {
                let payloads = section
                    .payload_types
                    .iter()
                    .map(u8::to_string)
                    .collect::<Vec<_>>()
                    .join(" ");
                let _ = write!(
                    sdp,
                    "m=video {MEDIA_PORT} {VIDEO_PROTOCOL} {payloads}\r\n\
                     c=IN IP4 0.0.0.0\r\n\
                     a=mid:{}\r\n\
                     a=recvonly\r\n",
                    section.mid,
                );
                for rtpmap in &section.rtpmaps {
                    let _ = write!(
                        sdp,
                        "a=rtpmap:{} {}/{}\r\n",
                        rtpmap.payload_type, rtpmap.encoding, rtpmap.clock_rate
                    );
                }
                sdp.push_str("a=rtcp-mux\r\n");
            }
        }
    }

    Ok(SessionDescription {
        sdp_type: SdpType::Answer,
        sdp,
    })
}

/// Require `answer` to be a legal mirrored answer for `offer`.
///
/// The BUNDLE mid set, media kinds, media order, mids and video payload-type
/// list must match. An answer may not use `a=setup:actpass`. The offered video
/// section must be able to send, and the answered video section must be
/// `recvonly`.
pub fn validate_answer(
    offer: &SessionDescription,
    answer: &SessionDescription,
) -> Result<(), WebRtcError> {
    if offer.sdp_type != SdpType::Offer {
        return Err(WebRtcError::sdp("the first description must be an offer"));
    }
    if answer.sdp_type != SdpType::Answer {
        return Err(WebRtcError::sdp("the second description must be an answer"));
    }
    let offer = parse_session_description(&offer.sdp)?;
    let answer = parse_session_description(&answer.sdp)?;
    if offer.sdp_type != SdpType::Offer || answer.sdp_type != SdpType::Answer {
        return Err(WebRtcError::sdp(
            "the setup roles do not match the labelled offer and answer",
        ));
    }
    if answer.setup == SdpSetup::ActPass {
        return Err(WebRtcError::sdp(
            "an answer must not use a=setup:actpass",
        ));
    }

    let mut offer_mids = offer.bundle_mids.clone();
    offer_mids.sort();
    let mut answer_mids = answer.bundle_mids.clone();
    answer_mids.sort();
    if offer_mids != answer_mids {
        return Err(WebRtcError::sdp(
            "the answer BUNDLE mids do not match the offer",
        ));
    }
    if offer.media.len() != answer.media.len() {
        return Err(WebRtcError::sdp(
            "the answer media section count does not match the offer",
        ));
    }
    for (offered, answered) in offer.media.iter().zip(answer.media.iter()) {
        if offered.kind != answered.kind || offered.mid != answered.mid {
            return Err(WebRtcError::sdp(format!(
                "the answer media section for mid {:?} does not mirror the offer",
                offered.mid
            )));
        }
        if offered.kind == SdpMediaKind::Video
            && offered.payload_types != answered.payload_types
        {
            return Err(WebRtcError::sdp(
                "the answer video payload types do not mirror the offer",
            ));
        }
        if offered.kind == SdpMediaKind::Video {
            if matches!(
                offered.direction,
                SdpDirection::RecvOnly | SdpDirection::Inactive
            ) {
                return Err(WebRtcError::sdp(
                    "the offered video section cannot receive media",
                ));
            }
            if answered.direction != SdpDirection::RecvOnly {
                return Err(WebRtcError::sdp(
                    "the answer video section must be recvonly",
                ));
            }
        }
    }
    Ok(())
}

/// Parse and validate one session description.
pub fn parse_session_description(
    text: &str,
) -> Result<ParsedSessionDescription, WebRtcError> {
    if text.len() > MAX_SDP_BYTES {
        return Err(WebRtcError::limit(format!(
            "SDP is {} bytes, maximum is {MAX_SDP_BYTES}",
            text.len()
        )));
    }
    if text.trim().is_empty() {
        return Err(WebRtcError::sdp("SDP text is empty"));
    }
    let first_line = text
        .split('\n')
        .map(strip_line_ending)
        .find(|line| !line.is_empty());
    if first_line != Some("v=0") {
        return Err(WebRtcError::sdp("SDP must start with v=0"));
    }

    let mut session = SessionFields::default();
    let mut media: Vec<SdpMediaSection> = Vec::new();
    let mut current: Option<MediaBuilder> = None;

    for (index, raw_line) in text.split('\n').enumerate() {
        let line_number = index + 1;
        let line = strip_line_ending(raw_line);
        if line.is_empty() {
            continue;
        }
        if line.len() > MAX_SDP_LINE_BYTES {
            return Err(WebRtcError::limit(format!(
                "SDP line {line_number} is {} bytes, maximum is {MAX_SDP_LINE_BYTES}",
                line.len()
            )));
        }
        if !line.is_ascii() {
            return Err(WebRtcError::sdp(format!(
                "SDP line {line_number} is not ASCII"
            )));
        }
        if line.contains('\r') {
            return Err(WebRtcError::sdp(format!(
                "SDP line {line_number} contains a stray carriage return"
            )));
        }
        let Some((prefix, remainder)) = line.split_once('=') else {
            return Err(WebRtcError::sdp(format!(
                "SDP line {line_number} has no '=' separator"
            )));
        };
        if current.is_some() && matches!(prefix, "v" | "o" | "s" | "t") {
            return Err(WebRtcError::sdp(format!(
                "SDP line {line_number} places a session-level {prefix}= line after the first m= line"
            )));
        }
        match prefix {
            "v" => session.set_version(remainder)?,
            "o" => session.set_origin(remainder)?,
            "s" => session.set_subject()?,
            "t" => session.set_timing(),
            "c" => {
                if remainder.trim().is_empty() {
                    return Err(WebRtcError::sdp(format!(
                        "SDP line {line_number} has an empty c= address"
                    )));
                }
            }
            "a" => {
                let (name, value) = split_attribute(remainder);
                if let Some(builder) = current.as_mut() {
                    builder.apply_attribute(name, value)?;
                } else {
                    session.apply_attribute(name, value)?;
                }
            }
            "m" => {
                if let Some(builder) = current.take() {
                    let section = builder.finish()?;
                    push_media_section(&mut media, section)?;
                }
                if media.len() >= MAX_SDP_MEDIA_SECTIONS {
                    return Err(WebRtcError::limit(format!(
                        "SDP has more than {MAX_SDP_MEDIA_SECTIONS} media sections"
                    )));
                }
                current = Some(parse_media_line(remainder, line_number)?);
            }
            _ => {
                // Bandwidth, encryption and future line types are ignored.
            }
        }
    }

    if let Some(builder) = current.take() {
        let section = builder.finish()?;
        push_media_section(&mut media, section)?;
    }
    if media.is_empty() {
        return Err(WebRtcError::sdp("SDP has no media sections"));
    }

    let version = session
        .version
        .ok_or_else(|| WebRtcError::sdp("SDP is missing v=0"))?;
    if version != "0" {
        return Err(WebRtcError::sdp("only SDP version 0 is supported"));
    }
    let session_id = session
        .session_id
        .ok_or_else(|| WebRtcError::sdp("SDP is missing the o= line"))?;
    if !session.has_subject {
        return Err(WebRtcError::sdp("SDP is missing the s= line"));
    }
    if !session.has_timing {
        return Err(WebRtcError::sdp("SDP is missing the t= line"));
    }
    let ice_ufrag = session
        .ice_ufrag
        .ok_or_else(|| WebRtcError::sdp("SDP is missing a=ice-ufrag"))?;
    let ice_pwd = session
        .ice_pwd
        .ok_or_else(|| WebRtcError::sdp("SDP is missing a=ice-pwd"))?;
    let fingerprint_sha256 = session
        .fingerprint
        .ok_or_else(|| WebRtcError::sdp("SDP is missing a=fingerprint"))?;
    let setup = session
        .setup
        .ok_or_else(|| WebRtcError::sdp("SDP is missing a=setup"))?;
    let bundle_mids = session
        .bundle_mids
        .ok_or_else(|| WebRtcError::sdp("SDP is missing a=group:BUNDLE"))?;

    let mut mids = Vec::with_capacity(media.len());
    for section in &media {
        if mids.contains(&section.mid) {
            return Err(WebRtcError::sdp(format!(
                "SDP repeats the mid {:?}",
                section.mid
            )));
        }
        mids.push(section.mid.clone());
    }
    for mid in &mids {
        if !bundle_mids.contains(mid) {
            return Err(WebRtcError::sdp(format!(
                "media mid {mid:?} is not listed by a=group:BUNDLE"
            )));
        }
    }
    for mid in &bundle_mids {
        if !mids.contains(mid) {
            return Err(WebRtcError::sdp(format!(
                "BUNDLE mid {mid:?} has no media section"
            )));
        }
    }

    let sdp_type = match setup {
        SdpSetup::ActPass => SdpType::Offer,
        SdpSetup::Active | SdpSetup::Passive => SdpType::Answer,
    };
    let video_payload_type = media
        .iter()
        .find(|section| section.kind == SdpMediaKind::Video)
        .and_then(|section| section.payload_types.first().copied());

    Ok(ParsedSessionDescription {
        sdp_type,
        session_id,
        ice_ufrag,
        ice_pwd,
        fingerprint_sha256,
        setup,
        bundle_mids,
        mids,
        video_payload_type,
        media,
    })
}

#[derive(Debug, Default)]
struct SessionFields {
    version: Option<String>,
    session_id: Option<String>,
    has_subject: bool,
    has_timing: bool,
    ice_ufrag: Option<String>,
    ice_pwd: Option<String>,
    fingerprint: Option<String>,
    setup: Option<SdpSetup>,
    bundle_mids: Option<Vec<String>>,
}

impl SessionFields {
    fn set_version(&mut self, value: &str) -> Result<(), WebRtcError> {
        if self.version.is_some() {
            return Err(WebRtcError::sdp("SDP repeats the v= line"));
        }
        if value != "0" {
            return Err(WebRtcError::sdp("only SDP version 0 is supported"));
        }
        self.version = Some(value.to_string());
        Ok(())
    }

    fn set_origin(&mut self, value: &str) -> Result<(), WebRtcError> {
        if self.session_id.is_some() {
            return Err(WebRtcError::sdp("SDP repeats the o= line"));
        }
        self.session_id = Some(parse_origin(value)?);
        Ok(())
    }

    fn set_subject(&mut self) -> Result<(), WebRtcError> {
        if self.has_subject {
            return Err(WebRtcError::sdp("SDP repeats the s= line"));
        }
        self.has_subject = true;
        Ok(())
    }

    fn set_timing(&mut self) {
        self.has_timing = true;
    }

    fn apply_attribute(
        &mut self,
        name: &str,
        value: Option<&str>,
    ) -> Result<(), WebRtcError> {
        match name {
            "ice-ufrag" => {
                let value = require_attribute_value(value, "a=ice-ufrag")?;
                validate_ice_credential(value, "a=ice-ufrag")?;
                if self.ice_ufrag.replace(value.to_string()).is_some() {
                    return Err(WebRtcError::sdp("SDP repeats a=ice-ufrag"));
                }
            }
            "ice-pwd" => {
                let value = require_attribute_value(value, "a=ice-pwd")?;
                validate_ice_credential(value, "a=ice-pwd")?;
                if self.ice_pwd.replace(value.to_string()).is_some() {
                    return Err(WebRtcError::sdp("SDP repeats a=ice-pwd"));
                }
            }
            "fingerprint" => {
                let value = require_attribute_value(value, "a=fingerprint")?;
                if self.fingerprint.is_some() {
                    return Err(WebRtcError::sdp("SDP repeats a=fingerprint"));
                }
                self.fingerprint = Some(parse_fingerprint(value)?);
            }
            "setup" => {
                let value = require_attribute_value(value, "a=setup")?;
                if self.setup.is_some() {
                    return Err(WebRtcError::sdp("SDP repeats a=setup"));
                }
                let setup = SdpSetup::parse(value)
                    .ok_or_else(|| WebRtcError::sdp("a=setup must be active, passive or actpass"))?;
                self.setup = Some(setup);
            }
            "group" => {
                let value = require_attribute_value(value, "a=group")?;
                if self.bundle_mids.is_some() {
                    return Err(WebRtcError::sdp("SDP repeats a=group:BUNDLE"));
                }
                self.bundle_mids = Some(parse_bundle_group(value)?);
            }
            "sendonly" | "recvonly" | "sendrecv" | "inactive" => {
                return Err(WebRtcError::sdp(
                    "a media direction attribute appeared before the first m= line",
                ));
            }
            "mid" | "rtpmap" | "sctp-port" | "max-message-size" | "rtcp-mux" => {
                return Err(WebRtcError::sdp(format!(
                    "a={name} appeared before the first m= line"
                )));
            }
            _ => {
                // extmap, ice-options, msid and future attributes are ignored.
            }
        }
        Ok(())
    }
}

struct MediaBuilder {
    kind: SdpMediaKind,
    mid: Option<String>,
    payload_types: Vec<u8>,
    rtpmaps: Vec<SdpRtpmap>,
    direction: SdpDirection,
    direction_set: bool,
    sctp_port: Option<u16>,
    max_message_size: Option<u32>,
    rtcp_mux: bool,
}

impl MediaBuilder {
    fn new(kind: SdpMediaKind, payload_types: Vec<u8>) -> Self {
        Self {
            kind,
            mid: None,
            payload_types,
            rtpmaps: Vec::new(),
            direction: SdpDirection::SendRecv,
            direction_set: false,
            sctp_port: None,
            max_message_size: None,
            rtcp_mux: false,
        }
    }

    fn apply_attribute(
        &mut self,
        name: &str,
        value: Option<&str>,
    ) -> Result<(), WebRtcError> {
        match name {
            "mid" => {
                let value = require_attribute_value(value, "a=mid")?;
                validate_sdp_token(value, "a=mid", MAX_MID_LEN)?;
                if self.mid.replace(value.to_string()).is_some() {
                    return Err(WebRtcError::sdp("a media section repeats a=mid"));
                }
            }
            "rtpmap" => {
                if self.kind != SdpMediaKind::Video {
                    return Err(WebRtcError::sdp(
                        "a=rtpmap is not valid on the data-channel section",
                    ));
                }
                let value = require_attribute_value(value, "a=rtpmap")?;
                self.apply_rtpmap(value)?;
            }
            "sendonly" => self.set_direction(SdpDirection::SendOnly)?,
            "recvonly" => self.set_direction(SdpDirection::RecvOnly)?,
            "sendrecv" => self.set_direction(SdpDirection::SendRecv)?,
            "inactive" => self.set_direction(SdpDirection::Inactive)?,
            "rtcp-mux" => {
                if self.kind != SdpMediaKind::Video {
                    return Err(WebRtcError::sdp(
                        "a=rtcp-mux is not valid on the data-channel section",
                    ));
                }
                if value.is_some() {
                    return Err(WebRtcError::sdp("a=rtcp-mux must not carry a value"));
                }
                if self.rtcp_mux {
                    return Err(WebRtcError::sdp("a media section repeats a=rtcp-mux"));
                }
                self.rtcp_mux = true;
            }
            "sctp-port" => {
                if self.kind != SdpMediaKind::Application {
                    return Err(WebRtcError::sdp(
                        "a=sctp-port is only valid on the data-channel section",
                    ));
                }
                let value = require_attribute_value(value, "a=sctp-port")?;
                let port: u16 = parse_unsigned(value, "a=sctp-port")?;
                if port == 0 {
                    return Err(WebRtcError::sdp("a=sctp-port must be non-zero"));
                }
                if self.sctp_port.replace(port).is_some() {
                    return Err(WebRtcError::sdp("a media section repeats a=sctp-port"));
                }
            }
            "max-message-size" => {
                if self.kind != SdpMediaKind::Application {
                    return Err(WebRtcError::sdp(
                        "a=max-message-size is only valid on the data-channel section",
                    ));
                }
                let value = require_attribute_value(value, "a=max-message-size")?;
                let size: u32 = parse_unsigned(value, "a=max-message-size")?;
                if self.max_message_size.replace(size).is_some() {
                    return Err(WebRtcError::sdp(
                        "a media section repeats a=max-message-size",
                    ));
                }
            }
            "ice-ufrag" | "ice-pwd" | "fingerprint" | "setup" | "group" => {
                return Err(WebRtcError::sdp(format!(
                    "a={name} must be a session-level attribute, not a media-level one"
                )));
            }
            "candidate" | "end-of-candidates" => {
                // Candidates travel through the signaling channel. SDP-level
                // candidates are ignored rather than merged.
            }
            _ => {
                // extmap, ssrc, fmtp, msid and future attributes are ignored.
            }
        }
        Ok(())
    }

    fn set_direction(&mut self, direction: SdpDirection) -> Result<(), WebRtcError> {
        if self.direction_set {
            return Err(WebRtcError::sdp(
                "a media section has more than one direction attribute",
            ));
        }
        self.direction = direction;
        self.direction_set = true;
        Ok(())
    }

    fn apply_rtpmap(&mut self, value: &str) -> Result<(), WebRtcError> {
        let (payload_text, encoding_text) = value
            .split_once(' ')
            .ok_or_else(|| WebRtcError::sdp("a=rtpmap is missing its encoding name"))?;
        let payload_type: u8 = parse_unsigned(payload_text, "rtpmap payload type")?;
        validate_payload_type(payload_type)?;
        if !self.payload_types.contains(&payload_type) {
            return Err(WebRtcError::sdp(format!(
                "a=rtpmap payload type {payload_type} is not listed on the m= line"
            )));
        }
        if self
            .rtpmaps
            .iter()
            .any(|rtpmap| rtpmap.payload_type == payload_type)
        {
            return Err(WebRtcError::sdp(format!(
                "a=rtpmap repeats payload type {payload_type}"
            )));
        }
        let mut parts = encoding_text.split('/');
        let encoding = parts.next().unwrap_or_default();
        validate_codec_name(encoding)?;
        let clock_text = parts
            .next()
            .ok_or_else(|| WebRtcError::sdp("a=rtpmap is missing its clock rate"))?;
        let clock_rate: u32 = parse_unsigned(clock_text, "rtpmap clock rate")?;
        if clock_rate == 0 {
            return Err(WebRtcError::sdp("rtpmap clock rate must be non-zero"));
        }
        if let Some(channels_text) = parts.next() {
            let channels: u8 = parse_unsigned(channels_text, "rtpmap channel count")?;
            if channels == 0 {
                return Err(WebRtcError::sdp("rtpmap channel count must be non-zero"));
            }
        }
        if parts.next().is_some() {
            return Err(WebRtcError::sdp("a=rtpmap has too many '/' fields"));
        }
        self.rtpmaps.push(SdpRtpmap {
            payload_type,
            encoding: encoding.to_string(),
            clock_rate,
        });
        Ok(())
    }

    fn finish(self) -> Result<SdpMediaSection, WebRtcError> {
        let mid = self
            .mid
            .ok_or_else(|| WebRtcError::sdp("a media section is missing a=mid"))?;
        match self.kind {
            SdpMediaKind::Application => {
                if self.sctp_port.is_none() {
                    return Err(WebRtcError::sdp(
                        "the data-channel section is missing a=sctp-port",
                    ));
                }
                if !self.payload_types.is_empty() || !self.rtpmaps.is_empty() {
                    return Err(WebRtcError::sdp(
                        "the data-channel section must not carry RTP formats",
                    ));
                }
            }
            SdpMediaKind::Video => {
                if self.payload_types.is_empty() {
                    return Err(WebRtcError::sdp(
                        "the video section has no RTP payload format",
                    ));
                }
                if !self.rtcp_mux {
                    return Err(WebRtcError::sdp(
                        "the video section is missing a=rtcp-mux",
                    ));
                }
                for payload_type in &self.payload_types {
                    if !self
                        .rtpmaps
                        .iter()
                        .any(|rtpmap| rtpmap.payload_type == *payload_type)
                    {
                        return Err(WebRtcError::sdp(format!(
                            "payload type {payload_type} has no a=rtpmap"
                        )));
                    }
                }
            }
        }
        Ok(SdpMediaSection {
            kind: self.kind,
            mid,
            payload_types: self.payload_types,
            rtpmaps: self.rtpmaps,
            direction: self.direction,
            sctp_port: self.sctp_port,
            max_message_size: self.max_message_size,
        })
    }
}

fn parse_media_line(value: &str, line_number: usize) -> Result<MediaBuilder, WebRtcError> {
    let fields: Vec<&str> = value.split_ascii_whitespace().collect();
    if fields.len() < 4 {
        return Err(WebRtcError::sdp(format!(
            "SDP line {line_number} has an incomplete m= line"
        )));
    }
    let kind = match fields[0] {
        "application" => SdpMediaKind::Application,
        "video" => SdpMediaKind::Video,
        other => {
            return Err(WebRtcError::sdp(format!(
                "unsupported media kind {other:?} (only application and video are supported)"
            )));
        }
    };
    let port: u16 = parse_unsigned(fields[1], "m= port")?;
    if port != MEDIA_PORT {
        return Err(WebRtcError::sdp(format!(
            "m= port must be {MEDIA_PORT} for a bundled WebRTC media section"
        )));
    }
    match kind {
        SdpMediaKind::Application => {
            if fields[2] != APPLICATION_PROTOCOL {
                return Err(WebRtcError::sdp(format!(
                    "the data-channel protocol must be {APPLICATION_PROTOCOL}"
                )));
            }
            if fields.len() != 4 || fields[3] != DATACHANNEL_FORMAT {
                return Err(WebRtcError::sdp(
                    "the data-channel m= line must carry exactly webrtc-datachannel",
                ));
            }
            Ok(MediaBuilder::new(kind, Vec::new()))
        }
        SdpMediaKind::Video => {
            if fields[2] != VIDEO_PROTOCOL {
                return Err(WebRtcError::sdp(format!(
                    "the video protocol must be {VIDEO_PROTOCOL}"
                )));
            }
            let formats = &fields[3..];
            if formats.len() > MAX_SDP_PAYLOAD_TYPES {
                return Err(WebRtcError::limit(format!(
                    "the video m= line lists {} payload types, maximum is {MAX_SDP_PAYLOAD_TYPES}",
                    formats.len()
                )));
            }
            let mut payload_types = Vec::with_capacity(formats.len());
            for format in formats {
                let payload_type: u8 = parse_unsigned(format, "m= payload type")?;
                validate_payload_type(payload_type)?;
                if payload_types.contains(&payload_type) {
                    return Err(WebRtcError::sdp(format!(
                        "the video m= line repeats payload type {payload_type}"
                    )));
                }
                payload_types.push(payload_type);
            }
            Ok(MediaBuilder::new(kind, payload_types))
        }
    }
}

fn push_media_section(
    media: &mut Vec<SdpMediaSection>,
    section: SdpMediaSection,
) -> Result<(), WebRtcError> {
    if media.iter().any(|existing| existing.kind == section.kind) {
        return Err(WebRtcError::sdp(
            "a second media section of the same kind is not supported",
        ));
    }
    media.push(section);
    Ok(())
}

fn parse_origin(value: &str) -> Result<String, WebRtcError> {
    let fields: Vec<&str> = value.split_ascii_whitespace().collect();
    if fields.len() != 6 {
        return Err(WebRtcError::sdp(
            "the o= line must carry exactly six fields",
        ));
    }
    validate_sdp_token(fields[1], "o= session id", MAX_SESSION_ID_LEN)?;
    Ok(fields[1].to_string())
}

fn parse_bundle_group(value: &str) -> Result<Vec<String>, WebRtcError> {
    let mut fields = value.split_ascii_whitespace();
    if fields.next() != Some("BUNDLE") {
        return Err(WebRtcError::sdp(
            "only a=group:BUNDLE is supported at session level",
        ));
    }
    let mids: Vec<String> = fields.map(str::to_string).collect();
    if mids.is_empty() {
        return Err(WebRtcError::sdp("a=group:BUNDLE lists no mids"));
    }
    if mids.len() > MAX_SDP_MEDIA_SECTIONS {
        return Err(WebRtcError::limit(format!(
            "a=group:BUNDLE lists {} mids, maximum is {MAX_SDP_MEDIA_SECTIONS}",
            mids.len()
        )));
    }
    for (index, mid) in mids.iter().enumerate() {
        validate_sdp_token(mid, "BUNDLE mid", MAX_MID_LEN)?;
        if mids[..index].contains(mid) {
            return Err(WebRtcError::sdp(format!(
                "a=group:BUNDLE repeats the mid {mid:?}"
            )));
        }
    }
    Ok(mids)
}

fn parse_fingerprint(value: &str) -> Result<String, WebRtcError> {
    let (algorithm, fingerprint) = value
        .split_once(' ')
        .ok_or_else(|| WebRtcError::sdp("a=fingerprint is missing its algorithm"))?;
    if !algorithm.eq_ignore_ascii_case("sha-256") {
        return Err(WebRtcError::sdp(
            "only a=fingerprint:sha-256 is supported",
        ));
    }
    validate_fingerprint(fingerprint)?;
    Ok(fingerprint.to_ascii_uppercase())
}

fn validate_fingerprint(value: &str) -> Result<(), WebRtcError> {
    if value.len() != FINGERPRINT_LEN {
        return Err(WebRtcError::sdp(format!(
            "fingerprint is {} bytes, expected {FINGERPRINT_LEN} (32 hex bytes separated by ':')",
            value.len()
        )));
    }
    for (index, byte) in value.bytes().enumerate() {
        if index % 3 == 2 {
            if byte != b':' {
                return Err(WebRtcError::sdp(
                    "fingerprint bytes must be separated by ':'",
                ));
            }
        } else if !byte.is_ascii_hexdigit() {
            return Err(WebRtcError::sdp(
                "fingerprint must contain hexadecimal bytes only",
            ));
        }
    }
    Ok(())
}

fn validate_payload_type(payload_type: u8) -> Result<(), WebRtcError> {
    if !(MIN_VIDEO_PAYLOAD_TYPE..=MAX_VIDEO_PAYLOAD_TYPE).contains(&payload_type) {
        return Err(WebRtcError::sdp(format!(
            "video payload type {payload_type} is outside {MIN_VIDEO_PAYLOAD_TYPE}..={MAX_VIDEO_PAYLOAD_TYPE}"
        )));
    }
    Ok(())
}

fn validate_codec_name(value: &str) -> Result<(), WebRtcError> {
    if value.is_empty() || value.len() > MAX_CODEC_NAME_LEN {
        return Err(WebRtcError::sdp(format!(
            "codec name is {} bytes, allowed range is 1..={MAX_CODEC_NAME_LEN}",
            value.len()
        )));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(WebRtcError::sdp(
            "codec name must contain ASCII letters, digits, '-', '_' or '.'",
        ));
    }
    Ok(())
}

fn validate_sdp_token(
    value: &str,
    field: &str,
    max_bytes: usize,
) -> Result<(), WebRtcError> {
    if value.is_empty() {
        return Err(WebRtcError::sdp(format!("{field} must not be empty")));
    }
    if value.len() > max_bytes {
        return Err(WebRtcError::sdp(format!(
            "{field} is {} bytes, maximum is {max_bytes}",
            value.len()
        )));
    }
    if !value.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err(WebRtcError::sdp(format!(
            "{field} must be printable ASCII without spaces or control characters"
        )));
    }
    Ok(())
}

fn parse_unsigned<T>(value: &str, field: &str) -> Result<T, WebRtcError>
where
    T: FromStr,
{
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(WebRtcError::sdp(format!(
            "{field} must be an unsigned decimal number"
        )));
    }
    value
        .parse()
        .map_err(|_| WebRtcError::sdp(format!("{field} is out of range")))
}

fn split_attribute(value: &str) -> (&str, Option<&str>) {
    match value.split_once(':') {
        Some((name, value)) => (name, Some(value)),
        None => (value, None),
    }
}

fn require_attribute_value<'a>(
    value: Option<&'a str>,
    field: &str,
) -> Result<&'a str, WebRtcError> {
    let value = value.ok_or_else(|| WebRtcError::sdp(format!("{field} has no value")))?;
    if value.is_empty() {
        return Err(WebRtcError::sdp(format!("{field} has an empty value")));
    }
    Ok(value)
}

fn strip_line_ending(line: &str) -> &str {
    line.strip_suffix('\r').unwrap_or(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::webrtc::WebRtcErrorKind;
    use std::fmt::Write as _;

    fn test_fingerprint() -> String {
        let mut value = String::with_capacity(FINGERPRINT_LEN);
        for index in 0..32u8 {
            if index > 0 {
                value.push(':');
            }
            let _ = write!(value, "{index:02X}");
        }
        value
    }

    fn offer_params() -> SdpOfferParams {
        SdpOfferParams::new("session-one", "ufrag1", "pwd1234", test_fingerprint())
    }

    fn answer_params() -> SdpOfferParams {
        SdpOfferParams::new("session-two", "ufrag2", "pwd5678", test_fingerprint())
    }

    fn expected_mids() -> Vec<String> {
        vec!["0".to_string(), "1".to_string()]
    }

    #[test]
    fn offer_round_trips_with_stable_lines() {
        let params = offer_params();
        let offer = build_offer(&params).unwrap();
        assert_eq!(offer.sdp_type, SdpType::Offer);
        assert_eq!(offer, build_offer(&params).unwrap());
        assert_eq!(offer.validate(), Ok(()));

        let lines: Vec<&str> = offer.sdp.split("\r\n").collect();
        let fingerprint_line = format!("a=fingerprint:sha-256 {}", test_fingerprint());
        let expected = [
            "v=0",
            "o=- session-one 2 IN IP4 127.0.0.1",
            "s=-",
            "t=0 0",
            "a=group:BUNDLE 0 1",
            "a=ice-ufrag:ufrag1",
            "a=ice-pwd:pwd1234",
            fingerprint_line.as_str(),
            "a=setup:actpass",
            "m=application 9 UDP/DTLS/SCTP webrtc-datachannel",
            "c=IN IP4 0.0.0.0",
            "a=mid:0",
            "a=sctp-port:5000",
            "a=max-message-size:262144",
            "m=video 9 UDP/TLS/RTP/SAVPF 96",
            "c=IN IP4 0.0.0.0",
            "a=mid:1",
            "a=sendonly",
            "a=rtpmap:96 RAF1/90000",
            "a=rtcp-mux",
        ];
        assert_eq!(&lines[..expected.len()], &expected);

        let parsed = parse_session_description(&offer.sdp).unwrap();
        assert_eq!(parsed.sdp_type, SdpType::Offer);
        assert_eq!(parsed.session_id, "session-one");
        assert_eq!(parsed.ice_ufrag, "ufrag1");
        assert_eq!(parsed.ice_pwd, "pwd1234");
        assert_eq!(parsed.fingerprint_sha256, test_fingerprint());
        assert_eq!(parsed.setup, SdpSetup::ActPass);
        assert_eq!(parsed.bundle_mids, expected_mids());
        assert_eq!(parsed.mids, expected_mids());
        assert_eq!(parsed.video_payload_type, Some(96));
        assert_eq!(parsed.media.len(), 2);
        assert_eq!(parsed.media[0].kind, SdpMediaKind::Application);
        assert_eq!(parsed.media[0].sctp_port, Some(SCTP_PORT));
        assert_eq!(
            parsed.media[0].max_message_size,
            Some(MAX_DATACHANNEL_MESSAGE_SIZE)
        );
        assert_eq!(parsed.media[1].kind, SdpMediaKind::Video);
        assert_eq!(parsed.media[1].direction, SdpDirection::SendOnly);
        assert_eq!(parsed.media[1].rtpmaps[0].encoding, RAF_VIDEO_CODEC);
        assert_eq!(parsed.media[1].rtpmaps[0].clock_rate, RAF_VIDEO_CLOCK_RATE);
    }

    #[test]
    fn answer_mirrors_offer_and_round_trips() {
        let offer = build_offer(&offer_params()).unwrap();
        let answer = build_answer(&offer, &answer_params()).unwrap();
        assert_eq!(answer.sdp_type, SdpType::Answer);
        assert_eq!(answer, build_answer(&offer, &answer_params()).unwrap());

        let parsed = parse_session_description(&answer.sdp).unwrap();
        assert_eq!(parsed.sdp_type, SdpType::Answer);
        assert_eq!(parsed.setup, SdpSetup::Active);
        assert_eq!(parsed.session_id, "session-two");
        assert_eq!(parsed.ice_ufrag, "ufrag2");
        assert_eq!(parsed.bundle_mids, expected_mids());
        assert_eq!(parsed.video_payload_type, Some(96));
        assert_eq!(parsed.media[1].direction, SdpDirection::RecvOnly);
        assert!(answer.sdp.contains("a=setup:active"));
        assert!(!answer.sdp.contains("a=sendonly"));
        assert!(!answer.sdp.contains("a=setup:actpass"));
        assert_eq!(validate_answer(&offer, &answer), Ok(()));
    }

    #[test]
    fn offer_without_ice_ufrag_is_rejected() {
        let params = offer_params();
        let offer = build_offer(&params).unwrap();
        let needle = format!("a=ice-ufrag:{}\r\n", params.ice_ufrag);
        let text = offer.sdp.replace(&needle, "");
        let error = parse_session_description(&text).unwrap_err();
        assert_eq!(error.kind(), WebRtcErrorKind::Sdp);
        assert!(error.detail().contains("ice-ufrag"));

        let broken = SessionDescription {
            sdp_type: SdpType::Offer,
            sdp: text,
        };
        assert_eq!(build_answer(&broken, &answer_params()).unwrap_err().kind(), WebRtcErrorKind::Sdp);
    }

    #[test]
    fn oversized_sdp_is_rejected() {
        let mut text = String::from("v=0\r\n");
        text.push_str(&"x".repeat(MAX_SDP_BYTES));
        let error = parse_session_description(&text).unwrap_err();
        assert_eq!(error.kind(), WebRtcErrorKind::Limit);
        assert!(error.detail().contains("maximum"));
    }

    #[test]
    fn invalid_offer_parameters_are_rejected() {
        let fingerprint = test_fingerprint();
        let cases = [
            SdpOfferParams::new("session", "abc", "pwd1234", fingerprint.clone()),
            SdpOfferParams::new("session", "ufrag1", "pwd", fingerprint.clone()),
            SdpOfferParams::new("session", "ufrag1", "pwd1234", "AA:BB".to_string()),
            SdpOfferParams::new("session with space", "ufrag1", "pwd1234", fingerprint.clone()),
        ];
        for params in cases {
            assert_eq!(
                build_offer(&params).unwrap_err().kind(),
                WebRtcErrorKind::Sdp
            );
        }
        let mut params = offer_params();
        params.video_payload_type = MIN_VIDEO_PAYLOAD_TYPE - 1;
        assert_eq!(
            build_offer(&params).unwrap_err().kind(),
            WebRtcErrorKind::Sdp
        );
    }

    #[test]
    fn answer_must_mirror_the_offer() {
        let offer = build_offer(&offer_params()).unwrap();
        let mut other_params = offer_params();
        other_params.session_id = "session-other".to_string();
        other_params.video_payload_type = 100;
        let other_offer = build_offer(&other_params).unwrap();
        let other_answer = build_answer(&other_offer, &answer_params()).unwrap();
        let error = validate_answer(&offer, &other_answer).unwrap_err();
        assert_eq!(error.kind(), WebRtcErrorKind::Sdp);
        assert!(error.detail().contains("payload"));
    }

    #[test]
    fn mislabelled_session_description_is_rejected() {
        let offer = build_offer(&offer_params()).unwrap();
        let mislabelled = SessionDescription {
            sdp_type: SdpType::Answer,
            sdp: offer.sdp.clone(),
        };
        let error = mislabelled.validate().unwrap_err();
        assert_eq!(error.kind(), WebRtcErrorKind::Sdp);
        assert!(error.detail().contains("labelled"));
    }
}
