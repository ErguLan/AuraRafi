//! ICE candidate model and attribute-line codec.
//!
//! This module models trickled candidates only. There is no STUN agent, no
//! connectivity check, no nomination and no TURN client: a candidate line is
//! parsed, bounded and stored, then handed to the negotiation state machine.
//!
//! Candidate lines follow `RFC 8445` attribute syntax:
//!
//! ```text
//! a=candidate:<foundation> <component> <transport> <priority> <address> <port>
//!   typ <host|srflx|prflx|relay> [raddr <address> rport <port>] [extensions]
//! ```
//!
//! [`parse_candidate`] also accepts the line without the `a=` prefix. The
//! `related_address` field stores the complete related transport endpoint in
//! `address:port` form (IPv6 endpoints are bracketed) because the related
//! port is required whenever `raddr` is present.

use super::WebRtcError;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::str::FromStr;

/// Minimum ICE ufrag/pwd byte length.
pub const MIN_ICE_CREDENTIAL_LEN: usize = 4;

/// Maximum ICE ufrag/pwd byte length.
pub const MAX_ICE_CREDENTIAL_LEN: usize = 256;

/// Maximum byte length of a candidate foundation.
pub const MAX_FOUNDATION_LEN: usize = 32;

/// Maximum byte length of a candidate address.
pub const MAX_ADDRESS_LEN: usize = 64;

/// Maximum byte length of a stored related endpoint (`address:port`).
pub const MAX_RELATED_ENDPOINT_LEN: usize = 64;

/// Maximum byte length of the transport token.
pub const MAX_TRANSPORT_LEN: usize = 8;

/// Maximum byte length of one complete candidate attribute line.
pub const MAX_CANDIDATE_LINE_BYTES: usize = 512;

/// Lowest valid ICE component id.
pub const MIN_COMPONENT: u16 = 1;

/// Highest valid ICE component id.
pub const MAX_COMPONENT: u16 = 2;

/// Marker that closes trickled candidate gathering.
pub const END_OF_CANDIDATES: &str = "a=end-of-candidates";

const END_OF_CANDIDATES_BARE: &str = "end-of-candidates";
const MAX_CANDIDATE_TOKENS: usize = 32;

/// Candidate transport type reported in the attribute line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IceCandidateType {
    Host,
    Srflx,
    Prflx,
    Relay,
}

impl IceCandidateType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Srflx => "srflx",
            Self::Prflx => "prflx",
            Self::Relay => "relay",
        }
    }

    pub const fn requires_related_address(self) -> bool {
        matches!(self, Self::Srflx | Self::Relay)
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "host" => Some(Self::Host),
            "srflx" => Some(Self::Srflx),
            "prflx" => Some(Self::Prflx),
            "relay" => Some(Self::Relay),
            _ => None,
        }
    }
}

/// One ICE candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IceCandidate {
    pub foundation: String,
    pub component: u16,
    pub transport: String,
    pub priority: u32,
    pub address: String,
    pub port: u16,
    pub typ: IceCandidateType,
    /// Related `address:port` endpoint for `srflx` and `relay` candidates.
    /// IPv6 endpoints are written with brackets, for example `[2001:db8::2]:5000`.
    pub related_address: Option<String>,
}

impl IceCandidate {
    /// Validate every field against the control-plane bounds.
    pub fn validate(&self) -> Result<(), WebRtcError> {
        validate_foundation(&self.foundation)?;
        if !(MIN_COMPONENT..=MAX_COMPONENT).contains(&self.component) {
            return Err(WebRtcError::candidate(format!(
                "component {} is outside {MIN_COMPONENT}..={MAX_COMPONENT}",
                self.component
            )));
        }
        if self.transport.len() > MAX_TRANSPORT_LEN
            || !self.transport.eq_ignore_ascii_case("UDP")
        {
            return Err(WebRtcError::candidate(
                "only the UDP candidate transport is supported",
            ));
        }
        if self.priority == 0 {
            return Err(WebRtcError::candidate("candidate priority must be non-zero"));
        }
        validate_address(&self.address)?;
        if self.port == 0 {
            return Err(WebRtcError::candidate("candidate port must be non-zero"));
        }
        match (self.typ, self.related_address.as_ref()) {
            (IceCandidateType::Host, Some(_)) => {
                return Err(WebRtcError::candidate(
                    "a host candidate must not carry a related address",
                ));
            }
            (IceCandidateType::Srflx | IceCandidateType::Relay, None) => {
                return Err(WebRtcError::candidate(format!(
                    "a {} candidate requires a related address",
                    self.typ.as_str()
                )));
            }
            _ => {}
        }
        if let Some(related) = &self.related_address {
            validate_related_endpoint(related)?;
        }
        Ok(())
    }

    /// Render the candidate as an SDP attribute line without a line terminator.
    pub fn to_candidate_line(&self) -> Result<String, WebRtcError> {
        self.validate()?;
        let mut line = format!(
            "a=candidate:{} {} {} {} {} {} typ {}",
            self.foundation,
            self.component,
            self.transport.to_ascii_uppercase(),
            self.priority,
            self.address,
            self.port,
            self.typ.as_str()
        );
        if let Some(related) = &self.related_address {
            let (address, port) = split_related_endpoint(related)?;
            line.push_str(&format!(" raddr {address} rport {port}"));
        }
        Ok(line)
    }

    /// Split [`IceCandidate::related_address`] into its address and port.
    pub fn related_endpoint(&self) -> Result<Option<(IpAddr, u16)>, WebRtcError> {
        match &self.related_address {
            Some(value) => split_related_endpoint(value).map(Some),
            None => Ok(None),
        }
    }
}

/// Parse one ICE candidate attribute line.
///
/// Both `a=candidate:...` and `candidate:...` are accepted. Unknown
/// extensions are rejected so a malformed line cannot silently lose fields.
pub fn parse_candidate(line: &str) -> Result<IceCandidate, WebRtcError> {
    let line = line.trim();
    if line.len() > MAX_CANDIDATE_LINE_BYTES {
        return Err(WebRtcError::limit(format!(
            "candidate line is {} bytes, maximum is {MAX_CANDIDATE_LINE_BYTES}",
            line.len()
        )));
    }
    if !line.is_ascii() {
        return Err(WebRtcError::candidate(
            "candidate line must contain ASCII only",
        ));
    }
    let attribute = line.strip_prefix("a=").unwrap_or(line);
    let rest = attribute.strip_prefix("candidate:").ok_or_else(|| {
        WebRtcError::candidate("candidate line must start with candidate: or a=candidate:")
    })?;
    let tokens: Vec<&str> = rest.split_ascii_whitespace().collect();
    if tokens.len() < 8 {
        return Err(WebRtcError::candidate(
            "candidate line is missing required fields",
        ));
    }
    if tokens.len() > MAX_CANDIDATE_TOKENS {
        return Err(WebRtcError::limit(format!(
            "candidate line has {} tokens, maximum is {MAX_CANDIDATE_TOKENS}",
            tokens.len()
        )));
    }

    let foundation = tokens[0].to_string();
    let component = parse_u16(tokens[1], "component")?;
    let transport = tokens[2].to_ascii_uppercase();
    let priority = parse_u32(tokens[3], "priority")?;
    let address = tokens[4].to_string();
    let port = parse_u16(tokens[5], "port")?;
    if tokens[6] != "typ" {
        return Err(WebRtcError::candidate(
            "candidate type must be introduced by 'typ'",
        ));
    }
    let typ = IceCandidateType::parse(tokens[7]).ok_or_else(|| {
        WebRtcError::candidate("candidate type must be host, srflx, prflx or relay")
    })?;

    let mut related_address = None;
    let mut related_port = None;
    let mut index = 8;
    while index < tokens.len() {
        match tokens[index] {
            "raddr" => {
                index += 1;
                let value = tokens
                    .get(index)
                    .ok_or_else(|| WebRtcError::candidate("raddr has no address"))?;
                if related_address.is_some() {
                    return Err(WebRtcError::candidate("candidate repeats raddr"));
                }
                related_address = Some(value.to_string());
            }
            "rport" => {
                index += 1;
                let value = tokens
                    .get(index)
                    .ok_or_else(|| WebRtcError::candidate("rport has no port"))?;
                if related_port.is_some() {
                    return Err(WebRtcError::candidate("candidate repeats rport"));
                }
                related_port = Some(parse_u16(value, "rport")?);
            }
            "generation" => {
                index += 1;
                let value = tokens
                    .get(index)
                    .ok_or_else(|| WebRtcError::candidate("generation has no value"))?;
                parse_u32(value, "generation")?;
            }
            "network-id" => {
                index += 1;
                let value = tokens
                    .get(index)
                    .ok_or_else(|| WebRtcError::candidate("network-id has no value"))?;
                parse_u32(value, "network-id")?;
            }
            "network-cost" => {
                index += 1;
                let value = tokens
                    .get(index)
                    .ok_or_else(|| WebRtcError::candidate("network-cost has no value"))?;
                parse_u32(value, "network-cost")?;
            }
            "ufrag" => {
                index += 1;
                let value = tokens
                    .get(index)
                    .ok_or_else(|| WebRtcError::candidate("ufrag has no value"))?;
                validate_ice_credential(value, "candidate ufrag")
                    .map_err(|error| WebRtcError::candidate(error.detail().to_string()))?;
            }
            extension => {
                return Err(WebRtcError::candidate(format!(
                    "unsupported candidate extension {extension:?}"
                )));
            }
        }
        index += 1;
    }

    let related_address = match (related_address, related_port) {
        (Some(address), Some(port)) => Some(format_related_endpoint(&address, port)?),
        (None, None) => None,
        _ => {
            return Err(WebRtcError::candidate(
                "raddr and rport must appear together",
            ));
        }
    };

    let candidate = IceCandidate {
        foundation,
        component,
        transport,
        priority,
        address,
        port,
        typ,
        related_address,
    };
    candidate.validate()?;
    Ok(candidate)
}

/// Recognize the end-of-candidates marker.
///
/// Returns `Ok(true)` for `a=end-of-candidates` or `end-of-candidates`,
/// `Ok(false)` for any other line, and an error when the line exceeds
/// [`MAX_CANDIDATE_LINE_BYTES`].
pub fn parse_end_of_candidates(line: &str) -> Result<bool, WebRtcError> {
    let line = line.trim();
    if line.len() > MAX_CANDIDATE_LINE_BYTES {
        return Err(WebRtcError::limit(format!(
            "candidate line is {} bytes, maximum is {MAX_CANDIDATE_LINE_BYTES}",
            line.len()
        )));
    }
    Ok(line == END_OF_CANDIDATES || line == END_OF_CANDIDATES_BARE)
}

/// Validate an ICE ufrag or pwd for use by the SDP and candidate codecs.
pub(crate) fn validate_ice_credential(
    value: &str,
    field: &str,
) -> Result<(), WebRtcError> {
    if !(MIN_ICE_CREDENTIAL_LEN..=MAX_ICE_CREDENTIAL_LEN).contains(&value.len()) {
        return Err(WebRtcError::sdp(format!(
            "{field} is {} bytes, allowed range is {MIN_ICE_CREDENTIAL_LEN}..={MAX_ICE_CREDENTIAL_LEN}",
            value.len()
        )));
    }
    if !value.bytes().all(is_ice_char) {
        return Err(WebRtcError::sdp(format!(
            "{field} must contain ASCII letters, digits, '+' or '/'"
        )));
    }
    Ok(())
}

fn is_ice_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'+' || byte == b'/'
}

fn validate_foundation(value: &str) -> Result<(), WebRtcError> {
    if value.is_empty() || value.len() > MAX_FOUNDATION_LEN {
        return Err(WebRtcError::candidate(format!(
            "foundation is {} bytes, allowed range is 1..={MAX_FOUNDATION_LEN}",
            value.len()
        )));
    }
    if !value.bytes().all(is_ice_char) {
        return Err(WebRtcError::candidate(
            "foundation must contain ASCII letters, digits, '+' or '/'",
        ));
    }
    Ok(())
}

fn validate_address(value: &str) -> Result<(), WebRtcError> {
    if value.is_empty() || value.len() > MAX_ADDRESS_LEN {
        return Err(WebRtcError::candidate(format!(
            "address is {} bytes, allowed range is 1..={MAX_ADDRESS_LEN}",
            value.len()
        )));
    }
    if !value.is_ascii() {
        return Err(WebRtcError::candidate(
            "address must contain ASCII only",
        ));
    }
    IpAddr::from_str(value)
        .map(|_| ())
        .map_err(|_| WebRtcError::candidate("address is not a valid IP address"))
}

fn validate_related_endpoint(value: &str) -> Result<(), WebRtcError> {
    if value.len() > MAX_RELATED_ENDPOINT_LEN {
        return Err(WebRtcError::candidate(format!(
            "related endpoint is {} bytes, maximum is {MAX_RELATED_ENDPOINT_LEN}",
            value.len()
        )));
    }
    split_related_endpoint(value).map(|_| ())
}

fn split_related_endpoint(value: &str) -> Result<(IpAddr, u16), WebRtcError> {
    let (address, port) = if let Some(rest) = value.strip_prefix('[') {
        let (address, port) = rest.split_once("]:").ok_or_else(|| {
            WebRtcError::candidate("a related IPv6 endpoint must be written as [address]:port")
        })?;
        (address, port)
    } else {
        value.split_once(':').ok_or_else(|| {
            WebRtcError::candidate("a related endpoint must be written as address:port")
        })?
    };
    let address = IpAddr::from_str(address)
        .map_err(|_| WebRtcError::candidate("related endpoint is not a valid IP address"))?;
    let port = parse_u16(port, "related port")?;
    Ok((address, port))
}

fn format_related_endpoint(address: &str, port: u16) -> Result<String, WebRtcError> {
    let address = IpAddr::from_str(address)
        .map_err(|_| WebRtcError::candidate("related address is not a valid IP address"))?;
    let value = match address {
        IpAddr::V4(_) => format!("{address}:{port}"),
        IpAddr::V6(_) => format!("[{address}]:{port}"),
    };
    if value.len() > MAX_RELATED_ENDPOINT_LEN {
        return Err(WebRtcError::limit(format!(
            "related endpoint is {} bytes, maximum is {MAX_RELATED_ENDPOINT_LEN}",
            value.len()
        )));
    }
    Ok(value)
}

fn parse_u16(value: &str, field: &str) -> Result<u16, WebRtcError> {
    parse_unsigned(value, field)
}

fn parse_u32(value: &str, field: &str) -> Result<u32, WebRtcError> {
    parse_unsigned(value, field)
}

fn parse_unsigned<T>(value: &str, field: &str) -> Result<T, WebRtcError>
where
    T: FromStr,
{
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(WebRtcError::candidate(format!(
            "{field} must be an unsigned decimal number"
        )));
    }
    value
        .parse()
        .map_err(|_| WebRtcError::candidate(format!("{field} is out of range")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::webrtc::WebRtcErrorKind;

    const HOST_LINE: &str = "a=candidate:1 1 UDP 2130706431 127.0.0.1 50000 typ host";
    const SRFLX_LINE: &str =
        "a=candidate:2 1 UDP 1694498815 203.0.113.7 40000 typ srflx raddr 10.0.0.5 rport 50000";

    #[test]
    fn host_candidate_round_trips() {
        let candidate = parse_candidate(HOST_LINE).unwrap();
        assert_eq!(candidate.foundation, "1");
        assert_eq!(candidate.component, 1);
        assert_eq!(candidate.transport, "UDP");
        assert_eq!(candidate.priority, 2_130_706_431);
        assert_eq!(candidate.address, "127.0.0.1");
        assert_eq!(candidate.port, 50_000);
        assert_eq!(candidate.typ, IceCandidateType::Host);
        assert_eq!(candidate.related_address, None);
        assert_eq!(candidate.to_candidate_line().unwrap(), HOST_LINE);
        assert_eq!(
            parse_candidate(&candidate.to_candidate_line().unwrap()).unwrap(),
            candidate
        );
    }

    #[test]
    fn server_reflexive_candidate_round_trips() {
        let candidate = parse_candidate(SRFLX_LINE).unwrap();
        assert_eq!(candidate.typ, IceCandidateType::Srflx);
        assert_eq!(candidate.related_address.as_deref(), Some("10.0.0.5:50000"));
        assert_eq!(candidate.to_candidate_line().unwrap(), SRFLX_LINE);

        let extended = format!("{SRFLX_LINE} generation 0 ufrag abcd network-id 1 network-cost 10");
        let candidate = parse_candidate(&extended).unwrap();
        assert_eq!(candidate.related_address.as_deref(), Some("10.0.0.5:50000"));
        assert_eq!(candidate.to_candidate_line().unwrap(), SRFLX_LINE);

        let ipv6 = "a=candidate:3 1 UDP 2122260223 2001:db8::1 50000 typ srflx raddr 2001:db8::2 rport 40000";
        let candidate = parse_candidate(ipv6).unwrap();
        assert_eq!(
            candidate.related_address.as_deref(),
            Some("[2001:db8::2]:40000")
        );
        assert_eq!(candidate.to_candidate_line().unwrap(), ipv6);
    }

    #[test]
    fn malformed_candidates_are_rejected() {
        let cases = [
            "",
            "candidate:1 1 UDP 100 127.0.0.1 0 typ host",
            "candidate:1 3 UDP 100 127.0.0.1 5000 typ host",
            "candidate:1 1 TCP 100 127.0.0.1 5000 typ host",
            "candidate:1 1 UDP 0 127.0.0.1 5000 typ host",
            "candidate:1 1 UDP 100 127.0.0.1 5000 typ",
            "candidate:1 1 UDP 100 not-an-ip 5000 typ host",
            "candidate:1 1 UDP 100 127.0.0.1 5000 typ srflx",
            "candidate:1 1 UDP 100 127.0.0.1 5000 typ host raddr 10.0.0.1 rport 5000",
            "candidate:1 1 UDP 100 127.0.0.1 5000 typ relay raddr 10.0.0.1",
            "candidate:1 1 UDP 100 127.0.0.1 5000 typ host foo bar",
        ];
        for line in cases {
            let error = parse_candidate(line).unwrap_err();
            assert_eq!(error.kind(), WebRtcErrorKind::Candidate, "{line}");
        }

        let foundation = "9".repeat(MAX_FOUNDATION_LEN + 1);
        let line = format!("candidate:{foundation} 1 UDP 100 127.0.0.1 5000 typ host");
        assert_eq!(
            parse_candidate(&line).unwrap_err().kind(),
            WebRtcErrorKind::Candidate
        );

        let oversized = "x".repeat(MAX_CANDIDATE_LINE_BYTES + 1);
        assert_eq!(
            parse_candidate(&oversized).unwrap_err().kind(),
            WebRtcErrorKind::Limit
        );
    }

    #[test]
    fn end_of_candidates_is_recognized() {
        assert!(parse_end_of_candidates("a=end-of-candidates").unwrap());
        assert!(parse_end_of_candidates("end-of-candidates\r").unwrap());
        assert!(!parse_end_of_candidates(HOST_LINE).unwrap());
        let oversized = "x".repeat(MAX_CANDIDATE_LINE_BYTES + 1);
        assert_eq!(
            parse_end_of_candidates(&oversized).unwrap_err().kind(),
            WebRtcErrorKind::Limit
        );
    }
}
