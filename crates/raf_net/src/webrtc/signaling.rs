//! Line-delimited JSON signaling over non-blocking TCP.
//!
//! One signaling connection carries a bounded sequence of [`SignalingMessage`]
//! values, one JSON document per line:
//!
//! ```text
//! {"Hello":{"protocol":1,"token":"...","session":"..."}}\n
//! {"Offer":{"sdp_type":"offer","sdp":"v=0\r\n..."}}\n
//! {"Candidate":{"foundation":"1","component":1,"transport":"UDP",...}}\n
//! ```
//!
//! The server never sends `Hello`; a peer is accepted in `AwaitingHello` and
//! must send a matching token and session instance within
//! [`SIGNALING_HANDSHAKE_TIMEOUT`]. A handshake or protocol failure queues an
//! `Error` message and closes the connection. Tokens are compared without
//! logging or echoing them.
//!
//! The queue is bounded by [`MAX_SIGNALING_QUEUE`] messages and
//! [`MAX_SIGNALING_QUEUE_BYTES`] bytes. A full queue returns `WouldBlock`; it
//! never grows without bound.
//!
//! This is a control-plane transport. It does not perform ICE, DTLS or media
//! work; see `docs/WEBRTC.md`.

use super::ice::IceCandidate;
use super::sdp::{SdpType, SessionDescription};
use super::WebRtcError;
use crate::protocol::bye_reason;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Version carried by the signaling `Hello` message.
pub const SIGNALING_PROTOCOL_VERSION: u16 = 1;

/// Hard limit for one JSON line, excluding the trailing newline.
pub const MAX_SIGNALING_LINE_BYTES: usize = 64 * 1024;

/// Maximum messages waiting in one peer send queue.
pub const MAX_SIGNALING_QUEUE: usize = 64;

/// Maximum bytes waiting in one peer send queue.
pub const MAX_SIGNALING_QUEUE_BYTES: usize = 4 * 1024 * 1024;

/// Maximum messages decoded by one [`SignalPeer::poll`] call.
pub const MAX_SIGNALING_MESSAGES_PER_POLL: usize = 64;

/// Maximum byte length of an `Error.message` value.
pub const MAX_SIGNALING_ERROR_LEN: usize = 256;

/// A peer must send a valid `Hello` inside this window.
pub const SIGNALING_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// Highest defined `Bye.reason` value.
pub const MAX_BYE_REASON: u8 = bye_reason::CLIENT_SHUTDOWN;

const READ_CHUNK: usize = 8 * 1024;
const READ_BUDGET: usize = 64 * 1024;

/// Codes carried by [`SignalingMessage::Error`].
pub mod signal_code {
    /// The peer speaks a different signaling protocol version.
    pub const PROTOCOL_MISMATCH: u16 = 1;
    /// The `Hello` token does not match the server token.
    pub const TOKEN_MISMATCH: u16 = 2;
    /// The `Hello` session instance does not match the server instance.
    pub const SESSION_MISMATCH: u16 = 3;
    /// No valid `Hello` arrived inside the handshake window.
    pub const HANDSHAKE_TIMEOUT: u16 = 4;
    /// A line was not valid JSON, failed validation, or arrived out of order.
    pub const MALFORMED_MESSAGE: u16 = 5;
    /// A line exceeded the framing limit.
    pub const LINE_TOO_LARGE: u16 = 6;
}

/// One signaling message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalingMessage {
    /// First client message. Must arrive within the handshake window.
    Hello {
        protocol: u16,
        token: Uuid,
        session: Uuid,
    },
    /// Session description created by the offerer.
    Offer(SessionDescription),
    /// Session description created by the answerer.
    Answer(SessionDescription),
    /// One trickled ICE candidate.
    Candidate(IceCandidate),
    /// Orderly close with a [`crate::protocol::bye_reason`] code.
    Bye { reason: u8 },
    /// Terminal protocol error with a [`signal_code`] code.
    Error { code: u16, message: String },
}

impl SignalingMessage {
    /// Validate the payload before sending or after receiving it.
    pub fn validate(&self) -> Result<(), WebRtcError> {
        match self {
            Self::Hello { .. } => Ok(()),
            Self::Offer(description) => {
                if description.sdp_type != SdpType::Offer {
                    return Err(WebRtcError::sdp(
                        "an Offer message must carry a description labelled offer",
                    ));
                }
                description.validate()
            }
            Self::Answer(description) => {
                if description.sdp_type != SdpType::Answer {
                    return Err(WebRtcError::sdp(
                        "an Answer message must carry a description labelled answer",
                    ));
                }
                description.validate()
            }
            Self::Candidate(candidate) => candidate.validate(),
            Self::Bye { reason } => {
                if *reason > MAX_BYE_REASON {
                    return Err(WebRtcError::signaling(format!(
                        "bye reason {reason} is outside 0..={MAX_BYE_REASON}"
                    )));
                }
                Ok(())
            }
            Self::Error { message, .. } => {
                if message.len() > MAX_SIGNALING_ERROR_LEN {
                    return Err(WebRtcError::limit(format!(
                        "error message is {} bytes, maximum is {MAX_SIGNALING_ERROR_LEN}",
                        message.len()
                    )));
                }
                if message.contains('\r') || message.contains('\n') {
                    return Err(WebRtcError::signaling(
                        "error message must not contain line breaks",
                    ));
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PeerState {
    AwaitingHello,
    Open,
    Closing,
    Closed,
}

struct QueuedLine {
    bytes: Vec<u8>,
    offset: usize,
}

/// Non-blocking signaling listener.
pub struct SignalingServer {
    listener: TcpListener,
    token: Uuid,
    session: Uuid,
}

impl SignalingServer {
    /// Bind a non-blocking listener. Nil identity values are rejected.
    pub fn bind(
        addr: impl ToSocketAddrs,
        token: Uuid,
        session: Uuid,
    ) -> io::Result<Self> {
        if token.is_nil() {
            return Err(invalid_input("signaling token must not be nil"));
        }
        if session.is_nil() {
            return Err(invalid_input("signaling session must not be nil"));
        }
        let listener = TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        Ok(Self {
            listener,
            token,
            session,
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// Accept one pending connection without blocking.
    ///
    /// The returned peer is still awaiting `Hello` and must be polled until
    /// [`SignalPeer::is_open`] is true.
    pub fn accept(&self) -> io::Result<Option<SignalPeer>> {
        match self.listener.accept() {
            Ok((stream, _peer_addr)) => Ok(Some(SignalPeer::from_stream(
                stream,
                self.token,
                self.session,
            )?)),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error),
        }
    }
}

/// One signaling connection.
pub struct SignalPeer {
    stream: TcpStream,
    state: PeerState,
    token: Uuid,
    session: Uuid,
    read_buffer: Vec<u8>,
    read_eof: bool,
    write_queue: VecDeque<QueuedLine>,
    queued_bytes: usize,
    handshake_deadline: Instant,
    close_reason: Option<u8>,
    remote_bye: Option<u8>,
    remote_error: Option<(u16, String)>,
}

impl SignalPeer {
    fn from_stream(stream: TcpStream, token: Uuid, session: Uuid) -> io::Result<Self> {
        stream.set_nonblocking(true)?;
        stream.set_nodelay(true)?;
        Ok(Self {
            stream,
            state: PeerState::AwaitingHello,
            token,
            session,
            read_buffer: Vec::new(),
            read_eof: false,
            write_queue: VecDeque::new(),
            queued_bytes: 0,
            handshake_deadline: Instant::now() + SIGNALING_HANDSHAKE_TIMEOUT,
            close_reason: None,
            remote_bye: None,
            remote_error: None,
        })
    }

    /// Receive and decode available messages without blocking.
    ///
    /// The returned list includes the `Hello` that completed the handshake and
    /// any `Bye`/`Error` that ended the connection. The `Hello` carries the
    /// peer token; callers must not log or persist it. A handshake mismatch or
    /// a malformed line returns an error after queueing an `Error` message.
    pub fn poll(&mut self) -> io::Result<Vec<SignalingMessage>> {
        match self.state {
            PeerState::Closed => return Err(closed_error()),
            PeerState::Closing => {
                self.flush_writes()?;
                return Ok(Vec::new());
            }
            PeerState::AwaitingHello | PeerState::Open => {}
        }

        let mut messages = Vec::new();
        if self.read_eof {
            self.flush_writes()?;
            self.drain_eof(&mut messages)?;
            return Ok(messages);
        }

        let mut budget = READ_BUDGET;
        let mut chunk = [0u8; READ_CHUNK];
        while budget > 0 && messages.len() < MAX_SIGNALING_MESSAGES_PER_POLL {
            match self.stream.read(&mut chunk) {
                Ok(0) => {
                    self.read_eof = true;
                    break;
                }
                Ok(count) => {
                    budget = budget.saturating_sub(count);
                    self.read_buffer.extend_from_slice(&chunk[..count]);
                    if let Some(length) = self.pending_line_too_large() {
                        return Err(self.fail(
                            signal_code::LINE_TOO_LARGE,
                            io::ErrorKind::InvalidData,
                            format!(
                                "signaling line is {length} bytes, maximum is {MAX_SIGNALING_LINE_BYTES}"
                            ),
                        ));
                    }
                    self.decode_lines(&mut messages)?;
                    if let Some(length) = self.pending_line_too_large() {
                        return Err(self.fail(
                            signal_code::LINE_TOO_LARGE,
                            io::ErrorKind::InvalidData,
                            format!(
                                "signaling line is {length} bytes, maximum is {MAX_SIGNALING_LINE_BYTES}"
                            ),
                        ));
                    }
                    if matches!(self.state, PeerState::Closed) {
                        break;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    self.state = PeerState::Closed;
                    return Err(error);
                }
            }
        }

        if !matches!(self.state, PeerState::Closed) {
            self.flush_writes()?;
            if self.read_eof {
                self.drain_eof(&mut messages)?;
                return Ok(messages);
            }
            if self.state == PeerState::AwaitingHello
                && Instant::now() >= self.handshake_deadline
            {
                return Err(self.fail(
                    signal_code::HANDSHAKE_TIMEOUT,
                    io::ErrorKind::TimedOut,
                    "signaling handshake timed out",
                ));
            }
        }
        Ok(messages)
    }

    fn drain_eof(&mut self, messages: &mut Vec<SignalingMessage>) -> io::Result<()> {
        self.decode_lines(messages)?;
        if !self.read_buffer.is_empty() {
            if messages.is_empty() {
                self.read_buffer.clear();
                self.state = PeerState::Closed;
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "signaling connection ended inside a line",
                ));
            }
            return Ok(());
        }
        self.state = PeerState::Closed;
        if messages.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "signaling peer closed the connection",
            ));
        }
        Ok(())
    }

    /// Queue one message and attempt a bounded non-blocking flush.
    ///
    /// `Hello` and `Bye` are not accepted here; the server never sends `Hello`
    /// and a close always goes through [`SignalPeer::close`].
    pub fn send(&mut self, message: SignalingMessage) -> io::Result<()> {
        match self.state {
            PeerState::Closed | PeerState::Closing => return Err(closed_error()),
            PeerState::AwaitingHello => {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "the signaling handshake must complete before sending",
                ));
            }
            PeerState::Open => {}
        }
        match &message {
            SignalingMessage::Hello { .. } => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "the server-side signaling peer does not send Hello",
                ));
            }
            SignalingMessage::Bye { .. } => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "call close() to end the signaling connection",
                ));
            }
            _ => {}
        }
        message
            .validate()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, format!("signaling message rejected: {error}")))?;
        self.enqueue_message(&message)?;
        self.flush_writes()
    }

    /// Send `Bye` with the normal close reason and finish once the queue drains.
    pub fn close(&mut self) -> io::Result<()> {
        if self.state == PeerState::Closed {
            return Ok(());
        }
        if self.state == PeerState::Closing {
            return self.flush_writes();
        }
        self.close_reason = Some(bye_reason::NORMAL);
        self.enqueue_terminal(&SignalingMessage::Bye {
            reason: bye_reason::NORMAL,
        });
        self.flush_writes()?;
        if self.write_queue.is_empty() {
            self.state = PeerState::Closed;
        } else {
            self.state = PeerState::Closing;
        }
        Ok(())
    }

    pub fn is_open(&self) -> bool {
        self.state == PeerState::Open
    }

    pub fn is_closed(&self) -> bool {
        matches!(self.state, PeerState::Closing | PeerState::Closed)
    }

    /// Reason sent by this side, if any.
    pub fn close_reason(&self) -> Option<u8> {
        self.close_reason
    }

    /// Reason received in a remote `Bye`, if any.
    pub fn remote_bye(&self) -> Option<u8> {
        self.remote_bye
    }

    /// Error received from the remote peer, if any.
    pub fn remote_error(&self) -> Option<(u16, &str)> {
        self.remote_error
            .as_ref()
            .map(|(code, message)| (*code, message.as_str()))
    }

    /// Number of complete lines waiting in the send queue.
    pub fn queued_messages(&self) -> usize {
        self.write_queue.len()
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.stream.local_addr()
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.stream.peer_addr()
    }

    fn decode_lines(&mut self, messages: &mut Vec<SignalingMessage>) -> io::Result<()> {
        loop {
            if messages.len() >= MAX_SIGNALING_MESSAGES_PER_POLL {
                break;
            }
            let Some(newline) = self.read_buffer.iter().position(|byte| *byte == b'\n') else {
                break;
            };
            let mut line: Vec<u8> = self.read_buffer.drain(..=newline).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if line.len() > MAX_SIGNALING_LINE_BYTES {
                return Err(self.fail(
                    signal_code::LINE_TOO_LARGE,
                    io::ErrorKind::InvalidData,
                    format!(
                        "signaling line is {} bytes, maximum is {MAX_SIGNALING_LINE_BYTES}",
                        line.len()
                    ),
                ));
            }
            if line.is_empty() {
                continue;
            }
            let message: SignalingMessage = match serde_json::from_slice(&line) {
                Ok(message) => message,
                Err(error) => {
                    return Err(self.fail(
                        signal_code::MALFORMED_MESSAGE,
                        io::ErrorKind::InvalidData,
                        format!("signaling line is not a valid message: {error}"),
                    ));
                }
            };
            if let Err(error) = message.validate() {
                return Err(self.fail(
                    signal_code::MALFORMED_MESSAGE,
                    io::ErrorKind::InvalidData,
                    format!("signaling message rejected: {error}"),
                ));
            }
            self.handle_message(message, messages)?;
            if matches!(self.state, PeerState::Closed) {
                break;
            }
        }
        Ok(())
    }

    fn handle_message(
        &mut self,
        message: SignalingMessage,
        messages: &mut Vec<SignalingMessage>,
    ) -> io::Result<()> {
        match self.state {
            PeerState::AwaitingHello => match message {
                SignalingMessage::Hello {
                    protocol,
                    token,
                    session,
                } => {
                    if protocol != SIGNALING_PROTOCOL_VERSION {
                        return Err(self.fail(
                            signal_code::PROTOCOL_MISMATCH,
                            io::ErrorKind::InvalidData,
                            format!(
                                "signaling protocol {protocol} is not the expected {SIGNALING_PROTOCOL_VERSION}"
                            ),
                        ));
                    }
                    if !constant_time_eq(token.as_bytes(), self.token.as_bytes()) {
                        return Err(self.fail(
                            signal_code::TOKEN_MISMATCH,
                            io::ErrorKind::PermissionDenied,
                            "signaling token mismatch",
                        ));
                    }
                    if session != self.session {
                        return Err(self.fail(
                            signal_code::SESSION_MISMATCH,
                            io::ErrorKind::PermissionDenied,
                            "signaling session mismatch",
                        ));
                    }
                    self.state = PeerState::Open;
                    messages.push(SignalingMessage::Hello {
                        protocol,
                        token,
                        session,
                    });
                    Ok(())
                }
                SignalingMessage::Bye { reason } => {
                    self.note_remote_bye(reason);
                    messages.push(SignalingMessage::Bye { reason });
                    Ok(())
                }
                SignalingMessage::Error { code, message } => {
                    self.note_remote_error(code, message.clone());
                    messages.push(SignalingMessage::Error { code, message });
                    Ok(())
                }
                _ => Err(self.fail(
                    signal_code::MALFORMED_MESSAGE,
                    io::ErrorKind::InvalidData,
                    "the first signaling message must be Hello",
                )),
            },
            PeerState::Open => match message {
                SignalingMessage::Offer(description) => {
                    messages.push(SignalingMessage::Offer(description));
                    Ok(())
                }
                SignalingMessage::Answer(description) => {
                    messages.push(SignalingMessage::Answer(description));
                    Ok(())
                }
                SignalingMessage::Candidate(candidate) => {
                    messages.push(SignalingMessage::Candidate(candidate));
                    Ok(())
                }
                SignalingMessage::Bye { reason } => {
                    self.note_remote_bye(reason);
                    messages.push(SignalingMessage::Bye { reason });
                    Ok(())
                }
                SignalingMessage::Error { code, message } => {
                    self.note_remote_error(code, message.clone());
                    messages.push(SignalingMessage::Error { code, message });
                    Ok(())
                }
                SignalingMessage::Hello { .. } => Err(self.fail(
                    signal_code::MALFORMED_MESSAGE,
                    io::ErrorKind::InvalidData,
                    "duplicate Hello on an open signaling connection",
                )),
            },
            PeerState::Closing | PeerState::Closed => Ok(()),
        }
    }

    fn note_remote_bye(&mut self, reason: u8) {
        self.remote_bye = Some(reason);
        self.read_buffer.clear();
        self.write_queue.clear();
        self.queued_bytes = 0;
        self.state = PeerState::Closed;
    }

    fn note_remote_error(&mut self, code: u16, message: String) {
        self.remote_error = Some((code, message));
        self.read_buffer.clear();
        self.write_queue.clear();
        self.queued_bytes = 0;
        self.state = PeerState::Closed;
    }

    fn pending_line_too_large(&self) -> Option<usize> {
        match self.read_buffer.iter().position(|byte| *byte == b'\n') {
            Some(newline) => {
                let length = if newline > 0 && self.read_buffer[newline - 1] == b'\r' {
                    newline - 1
                } else {
                    newline
                };
                if length > MAX_SIGNALING_LINE_BYTES {
                    Some(length)
                } else {
                    None
                }
            }
            None if self.read_buffer.len() > MAX_SIGNALING_LINE_BYTES => {
                Some(self.read_buffer.len())
            }
            _ => None,
        }
    }

    fn enqueue_message(&mut self, message: &SignalingMessage) -> io::Result<()> {
        let mut bytes = serde_json::to_vec(message).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("signaling encode: {error}"),
            )
        })?;
        if bytes.len() > MAX_SIGNALING_LINE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "signaling line {} bytes exceeds the {MAX_SIGNALING_LINE_BYTES} byte limit",
                    bytes.len()
                ),
            ));
        }
        bytes.push(b'\n');
        if self.write_queue.len() >= MAX_SIGNALING_QUEUE
            || self.queued_bytes.saturating_add(bytes.len()) > MAX_SIGNALING_QUEUE_BYTES
        {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "signaling send queue is full",
            ));
        }
        self.queued_bytes = self.queued_bytes.saturating_add(bytes.len());
        self.write_queue.push_back(QueuedLine { bytes, offset: 0 });
        Ok(())
    }

    fn enqueue_terminal(&mut self, message: &SignalingMessage) {
        if self.enqueue_message(message).is_err() {
            let _ = self.flush_writes();
            let _ = self.enqueue_message(message);
        }
    }

    fn flush_writes(&mut self) -> io::Result<()> {
        loop {
            let Some(line) = self.write_queue.front_mut() else {
                break;
            };
            match self.stream.write(&line.bytes[line.offset..]) {
                Ok(0) => {
                    self.write_queue.clear();
                    self.queued_bytes = 0;
                    self.state = PeerState::Closed;
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "signaling socket wrote zero bytes",
                    ));
                }
                Ok(written) => {
                    line.offset += written;
                    let complete = line.offset == line.bytes.len();
                    self.queued_bytes = self.queued_bytes.saturating_sub(written);
                    if complete {
                        self.write_queue.pop_front();
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    self.write_queue.clear();
                    self.queued_bytes = 0;
                    self.state = PeerState::Closed;
                    return Err(error);
                }
            }
        }
        if self.write_queue.is_empty() && self.state == PeerState::Closing {
            self.state = PeerState::Closed;
        }
        Ok(())
    }

    fn fail(
        &mut self,
        code: u16,
        kind: io::ErrorKind,
        message: impl Into<String>,
    ) -> io::Error {
        let message: String = message.into();
        let wire_message = truncate_text(&message, MAX_SIGNALING_ERROR_LEN);
        self.enqueue_terminal(&SignalingMessage::Error {
            code,
            message: wire_message,
        });
        let _ = self.flush_writes();
        if self.write_queue.is_empty() {
            self.state = PeerState::Closed;
        } else {
            self.state = PeerState::Closing;
        }
        io::Error::new(kind, message)
    }
}

fn constant_time_eq(left: &[u8; 16], right: &[u8; 16]) -> bool {
    let mut difference = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        difference |= *a ^ *b;
    }
    difference == 0
}

fn truncate_text(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

fn invalid_input(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

fn closed_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::NotConnected,
        "signaling peer is closed",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::webrtc::ice::IceCandidate;
    use crate::webrtc::sdp::{build_answer, build_offer, SdpOfferParams};
    use crate::webrtc::IceCandidateType;
    use std::thread;
    use std::time::{Duration, Instant};

    fn test_fingerprint() -> String {
        let mut value = String::with_capacity(95);
        for index in 0..32u8 {
            if index > 0 {
                value.push(':');
            }
            value.push_str(&format!("{index:02X}"));
        }
        value
    }

    fn test_candidate() -> IceCandidate {
        IceCandidate {
            foundation: "1".to_string(),
            component: 1,
            transport: "UDP".to_string(),
            priority: 2_130_706_431,
            address: "127.0.0.1".to_string(),
            port: 50_000,
            typ: IceCandidateType::Host,
            related_address: None,
        }
    }

    fn connect_client(address: SocketAddr) -> TcpStream {
        let stream = TcpStream::connect(address).unwrap();
        stream.set_nodelay(true).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream
    }

    fn write_message(stream: &mut TcpStream, message: &SignalingMessage) {
        let mut line = serde_json::to_vec(message).unwrap();
        line.push(b'\n');
        stream.write_all(&line).unwrap();
    }

    fn read_message(stream: &mut TcpStream) -> SignalingMessage {
        let mut line = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            stream.read_exact(&mut byte).unwrap();
            if byte[0] == b'\n' {
                break;
            }
            line.push(byte[0]);
        }
        serde_json::from_slice(&line).unwrap()
    }

    fn accept_peer(server: &SignalingServer) -> SignalPeer {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(peer) = server.accept().unwrap() {
                return peer;
            }
            assert!(Instant::now() < deadline, "signaling accept timed out");
            thread::sleep(Duration::from_millis(1));
        }
    }

    fn poll_until<F>(
        peer: &mut SignalPeer,
        deadline: Instant,
        mut done: F,
    ) -> Vec<SignalingMessage>
    where
        F: FnMut(&[SignalingMessage]) -> bool,
    {
        let mut messages = Vec::new();
        while !done(&messages) {
            match peer.poll() {
                Ok(batch) => messages.extend(batch),
                Err(error) => panic!("signaling poll failed: {error}"),
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for signaling messages"
            );
            thread::sleep(Duration::from_millis(1));
        }
        messages
    }

    fn wait_for_error(peer: &mut SignalPeer) -> io::Error {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Err(error) = peer.poll() {
                return error;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for a signaling rejection"
            );
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn offer_answer_and_candidate_exchange() {
        let token = Uuid::new_v4();
        let session = Uuid::new_v4();
        let server = SignalingServer::bind(("127.0.0.1", 0), token, session).unwrap();
        let address = server.local_addr().unwrap();

        let offer = build_offer(&SdpOfferParams::new(
            "session-one",
            "ufrag1",
            "pwd1234",
            test_fingerprint(),
        ))
        .unwrap();
        let answer = build_answer(
            &offer,
            &SdpOfferParams::new("session-two", "ufrag2", "pwd5678", test_fingerprint()),
        )
        .unwrap();
        let candidate = test_candidate();

        let mut client = connect_client(address);
        write_message(
            &mut client,
            &SignalingMessage::Hello {
                protocol: SIGNALING_PROTOCOL_VERSION,
                token,
                session,
            },
        );
        write_message(&mut client, &SignalingMessage::Offer(offer.clone()));

        let mut peer = accept_peer(&server);
        let received = poll_until(
            &mut peer,
            Instant::now() + Duration::from_secs(3),
            |messages| {
                messages
                    .iter()
                    .any(|message| matches!(message, SignalingMessage::Offer(_)))
            },
        );
        assert!(peer.is_open());
        assert!(received
            .iter()
            .any(|message| matches!(message, SignalingMessage::Hello { .. })));
        assert!(received
            .iter()
            .any(|message| matches!(message, SignalingMessage::Offer(_))));

        peer.send(SignalingMessage::Answer(answer.clone()))
            .unwrap();
        assert_eq!(read_message(&mut client), SignalingMessage::Answer(answer));

        write_message(
            &mut client,
            &SignalingMessage::Candidate(candidate.clone()),
        );
        let received = poll_until(
            &mut peer,
            Instant::now() + Duration::from_secs(3),
            |messages| {
                messages
                    .iter()
                    .any(|message| matches!(message, SignalingMessage::Candidate(_)))
            },
        );
        assert!(received
            .iter()
            .any(|message| message == &SignalingMessage::Candidate(candidate.clone())));

        peer.send(SignalingMessage::Candidate(candidate.clone()))
            .unwrap();
        assert_eq!(
            read_message(&mut client),
            SignalingMessage::Candidate(candidate.clone())
        );

        write_message(
            &mut client,
            &SignalingMessage::Bye {
                reason: bye_reason::NORMAL,
            },
        );
        let received = poll_until(
            &mut peer,
            Instant::now() + Duration::from_secs(3),
            |messages| {
                messages
                    .iter()
                    .any(|message| matches!(message, SignalingMessage::Bye { .. }))
            },
        );
        assert!(received
            .iter()
            .any(|message| matches!(message, SignalingMessage::Bye { .. })));
        assert!(peer.is_closed());
    }

    #[test]
    fn server_close_sends_bye() {
        let token = Uuid::new_v4();
        let session = Uuid::new_v4();
        let server = SignalingServer::bind(("127.0.0.1", 0), token, session).unwrap();
        let address = server.local_addr().unwrap();
        let mut client = connect_client(address);
        write_message(
            &mut client,
            &SignalingMessage::Hello {
                protocol: SIGNALING_PROTOCOL_VERSION,
                token,
                session,
            },
        );

        let mut peer = accept_peer(&server);
        let _ = poll_until(
            &mut peer,
            Instant::now() + Duration::from_secs(3),
            |messages| {
                messages
                    .iter()
                    .any(|message| matches!(message, SignalingMessage::Hello { .. }))
            },
        );
        peer.close().unwrap();
        assert!(peer.is_closed());
        assert_eq!(peer.close_reason(), Some(bye_reason::NORMAL));
        assert_eq!(
            read_message(&mut client),
            SignalingMessage::Bye {
                reason: bye_reason::NORMAL
            }
        );
    }

    #[test]
    fn wrong_token_is_rejected_with_error_message() {
        let token = Uuid::new_v4();
        let session = Uuid::new_v4();
        let server = SignalingServer::bind(("127.0.0.1", 0), token, session).unwrap();
        let address = server.local_addr().unwrap();
        let mut client = connect_client(address);
        write_message(
            &mut client,
            &SignalingMessage::Hello {
                protocol: SIGNALING_PROTOCOL_VERSION,
                token: Uuid::new_v4(),
                session,
            },
        );

        let mut peer = accept_peer(&server);
        let error = wait_for_error(&mut peer);
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(peer.is_closed());

        match read_message(&mut client) {
            SignalingMessage::Error { code, message } => {
                assert_eq!(code, signal_code::TOKEN_MISMATCH);
                assert!(message.contains("token"));
            }
            other => panic!("expected an Error message, got {other:?}"),
        }
    }

    #[test]
    fn protocol_mismatch_is_rejected() {
        let token = Uuid::new_v4();
        let session = Uuid::new_v4();
        let server = SignalingServer::bind(("127.0.0.1", 0), token, session).unwrap();
        let address = server.local_addr().unwrap();
        let mut client = connect_client(address);
        write_message(
            &mut client,
            &SignalingMessage::Hello {
                protocol: SIGNALING_PROTOCOL_VERSION + 1,
                token,
                session,
            },
        );

        let mut peer = accept_peer(&server);
        let error = wait_for_error(&mut peer);
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(peer.is_closed());

        match read_message(&mut client) {
            SignalingMessage::Error { code, .. } => {
                assert_eq!(code, signal_code::PROTOCOL_MISMATCH);
            }
            other => panic!("expected an Error message, got {other:?}"),
        }
    }

    #[test]
    fn oversized_line_is_rejected() {
        let token = Uuid::new_v4();
        let session = Uuid::new_v4();
        let server = SignalingServer::bind(("127.0.0.1", 0), token, session).unwrap();
        let address = server.local_addr().unwrap();
        let mut client = connect_client(address);
        write_message(
            &mut client,
            &SignalingMessage::Hello {
                protocol: SIGNALING_PROTOCOL_VERSION,
                token,
                session,
            },
        );

        let mut peer = accept_peer(&server);
        let _ = poll_until(
            &mut peer,
            Instant::now() + Duration::from_secs(3),
            |messages| {
                messages
                    .iter()
                    .any(|message| matches!(message, SignalingMessage::Hello { .. }))
            },
        );
        assert!(peer.is_open());

        let mut junk_client = client.try_clone().unwrap();
        let writer = thread::spawn(move || {
            let _ = junk_client.write_all(&vec![b'x'; MAX_SIGNALING_LINE_BYTES + 4096]);
        });
        let error = wait_for_error(&mut peer);
        writer.join().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("maximum"));
        assert!(peer.is_closed());
    }
}
