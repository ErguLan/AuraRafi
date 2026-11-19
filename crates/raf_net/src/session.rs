//! Authenticated, non-blocking stream sessions over `std::net`.
//!
//! A [`StreamServer`] accepts bounded peers. Each peer first authenticates
//! with a `Hello` and receives a `Welcome`, then exchanges input, video, ping
//! and bye messages. All I/O is non-blocking; partial writes stay in a
//! bounded user-space queue and message boundaries are preserved.

use crate::frame::{FrameKind, FrameReader, FrameWriter, QueueError};
use crate::input::InputFrame;
use crate::latency::PeerMetrics;
use crate::messages::StreamMessage;
use crate::protocol::{
    bye_reason, epoch_millis, DEFAULT_HEIGHT, DEFAULT_IDLE_TIMEOUT, DEFAULT_MAX_FPS,
    DEFAULT_QUEUE_FRAMES, DEFAULT_WIDTH, FRAME_HEADER_LEN, HANDSHAKE_TIMEOUT, MAX_CLIENT_NAME,
    MAX_FPS, MAX_FRAME_PAYLOAD, MAX_MESSAGES_PER_POLL, MAX_QUEUE_BYTES, MAX_QUEUE_FRAMES,
    MAX_VIDEO_DIMENSION, PROTOCOL_VERSION, READ_BUDGET, READ_CHUNK,
};
use crate::video::VideoFrame;
use std::io::{self, Read};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Session identity and negotiated stream shape.
#[derive(Debug, Clone)]
pub struct StreamConfig {
    /// Shared secret required by `Hello`.
    pub token: Uuid,
    /// Server instance identity required by `Hello`.
    pub instance: Uuid,
    /// Stream width advertised by `Welcome` and enforced for outgoing video.
    pub max_width: u16,
    /// Stream height advertised by `Welcome` and enforced for outgoing video.
    pub max_height: u16,
    /// Frame-rate cap advertised by `Welcome`.
    pub max_fps: u16,
    /// Maximum number of messages in one peer send queue, 1..=64.
    pub queue_frames: usize,
    /// No-traffic window before the session closes.
    pub idle_timeout: Duration,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            token: Uuid::nil(),
            instance: Uuid::nil(),
            max_width: DEFAULT_WIDTH,
            max_height: DEFAULT_HEIGHT,
            max_fps: DEFAULT_MAX_FPS,
            queue_frames: DEFAULT_QUEUE_FRAMES,
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
        }
    }
}

impl StreamConfig {
    pub fn new(token: Uuid, instance: Uuid) -> Self {
        Self {
            token,
            instance,
            ..Self::default()
        }
    }

    /// Validate identity and bounds. A nil token is rejected so a listener can
    /// never be started with the template defaults by accident.
    pub fn normalize(&self) -> io::Result<Self> {
        if self.token.is_nil() {
            return Err(invalid_input("stream token must not be nil"));
        }
        if self.max_width == 0 || self.max_height == 0 {
            return Err(invalid_input("stream width and height must be non-zero"));
        }
        if self.max_width > MAX_VIDEO_DIMENSION || self.max_height > MAX_VIDEO_DIMENSION {
            return Err(invalid_input(format!(
                "stream dimensions {}x{} exceed the {MAX_VIDEO_DIMENSION} pixel limit",
                self.max_width, self.max_height
            )));
        }
        if self.max_fps == 0 || self.max_fps > MAX_FPS {
            return Err(invalid_input(format!(
                "stream max_fps {} is outside 1..={MAX_FPS}",
                self.max_fps
            )));
        }
        if self.queue_frames == 0 || self.queue_frames > MAX_QUEUE_FRAMES {
            return Err(invalid_input(format!(
                "stream queue_frames {} is outside 1..={MAX_QUEUE_FRAMES}",
                self.queue_frames
            )));
        }
        let mut normalized = self.clone();
        if normalized.idle_timeout.is_zero() {
            normalized.idle_timeout = DEFAULT_IDLE_TIMEOUT;
        }
        Ok(normalized)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Server,
    Client,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    AwaitingHello,
    AwaitingWelcome,
    Open,
    Closing,
    Closed,
}

/// Non-blocking listener for authenticated stream peers.
pub struct StreamServer {
    listener: TcpListener,
    config: StreamConfig,
}

impl StreamServer {
    /// Bind a non-blocking listener. The caller polls [`StreamServer::accept`]
    /// from its own loop.
    pub fn bind(addr: impl ToSocketAddrs, config: StreamConfig) -> io::Result<Self> {
        let config = config.normalize()?;
        let listener = TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        Ok(Self { listener, config })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    pub fn config(&self) -> &StreamConfig {
        &self.config
    }

    /// Accept one pending connection without blocking.
    ///
    /// `None` means the accept queue is currently empty; the returned peer is
    /// still awaiting `Hello` and must be polled until [`StreamPeer::is_open`].
    pub fn accept(&self) -> io::Result<Option<StreamPeer>> {
        match self.listener.accept() {
            Ok((stream, _peer_addr)) => Ok(Some(StreamPeer::from_stream(
                stream,
                self.config.clone(),
                Role::Server,
            )?)),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error),
        }
    }
}

/// One authenticated (or handshaking) stream connection.
pub struct StreamPeer {
    role: Role,
    state: State,
    stream: TcpStream,
    reader: FrameReader,
    writer: FrameWriter,
    config: StreamConfig,
    handshake_deadline: Instant,
    last_activity: Instant,
    session: Uuid,
    stream_size: (u16, u16),
    negotiated_max_fps: u16,
    peer_name: String,
    close_reason: Option<u8>,
    remote_bye: Option<u8>,
    hello_sent: bool,
    last_ping: Option<(u64, Instant)>,
    next_ping: u64,
    metrics: PeerMetrics,
}

impl StreamPeer {
    /// Connect a client and queue its `Hello`.
    ///
    /// The TCP connect and the initial flush are bounded by
    /// [`HANDSHAKE_TIMEOUT`]; after this call every operation is non-blocking.
    pub fn connect(addr: SocketAddr, config: &StreamConfig, client_name: &str) -> io::Result<Self> {
        let config = config.normalize()?;
        if client_name.len() > MAX_CLIENT_NAME {
            return Err(invalid_input(format!(
                "client name is {} bytes, maximum is {MAX_CLIENT_NAME}",
                client_name.len()
            )));
        }
        let stream = TcpStream::connect_timeout(&addr, HANDSHAKE_TIMEOUT)?;
        let mut peer = Self::from_stream(stream, config.clone(), Role::Client)?;
        peer.send(StreamMessage::Hello {
            protocol: PROTOCOL_VERSION,
            token: config.token,
            instance: config.instance,
            client_name: client_name.to_string(),
        })?;
        Ok(peer)
    }

    fn from_stream(stream: TcpStream, config: StreamConfig, role: Role) -> io::Result<Self> {
        stream.set_nonblocking(true)?;
        stream.set_nodelay(true)?;
        let now = Instant::now();
        let (state, session, stream_size, negotiated_max_fps) = match role {
            Role::Server => (
                State::AwaitingHello,
                Uuid::new_v4(),
                (config.max_width, config.max_height),
                config.max_fps,
            ),
            Role::Client => (State::AwaitingWelcome, Uuid::nil(), (0, 0), 0),
        };
        Ok(Self {
            role,
            state,
            stream,
            reader: FrameReader::new(),
            writer: FrameWriter::new(config.queue_frames, MAX_QUEUE_BYTES),
            config,
            handshake_deadline: now + HANDSHAKE_TIMEOUT,
            last_activity: now,
            session,
            stream_size,
            negotiated_max_fps,
            peer_name: String::new(),
            close_reason: None,
            remote_bye: None,
            hello_sent: false,
            last_ping: None,
            next_ping: 1,
            metrics: PeerMetrics::default(),
        })
    }

    /// Queue one message and attempt a bounded non-blocking flush.
    ///
    /// Video is best-effort: when the queue is full, queued or incoming video
    /// is dropped and counted in `PeerMetrics::frames_dropped`. Control and
    /// input messages return `WouldBlock` instead of silently disappearing.
    pub fn send(&mut self, message: StreamMessage) -> io::Result<()> {
        let kind = message.kind();
        self.ensure_send_allowed(kind)?;
        match message {
            StreamMessage::Video(frame) => self.send_video(frame)?,
            StreamMessage::Input(frame) => {
                let mut frame = frame;
                frame.sanitize().map_err(|error| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("input frame rejected: {error}"),
                    )
                })?;
                let bytes = StreamMessage::Input(frame).encode_frame()?;
                self.queue_control_bytes(FrameKind::Input, bytes)?;
            }
            other => {
                let bytes = other.encode_frame()?;
                self.queue_control_bytes(kind, bytes)?;
            }
        }
        if kind == FrameKind::Hello {
            self.hello_sent = true;
        }
        self.flush_writes()
    }

    /// Receive and decode available messages without blocking.
    ///
    /// The returned list includes `Hello`/`Welcome` on a successful handshake,
    /// so the caller can observe the handshake as it completes. `Ping` and
    /// `Pong` are answered and accounted internally; they are not returned.
    pub fn poll(&mut self) -> io::Result<Vec<StreamMessage>> {
        match self.state {
            State::Closed => return Err(closed_error()),
            State::Closing => {
                self.flush_writes()?;
                return Ok(Vec::new());
            }
            _ => {}
        }

        let mut messages = Vec::new();
        let processed = self.decode_frames(&mut messages)?;
        if matches!(self.state, State::Closed) {
            return Ok(messages);
        }
        if processed >= MAX_MESSAGES_PER_POLL {
            self.flush_writes()?;
            self.check_timeouts()?;
            return Ok(messages);
        }

        let mut chunk = [0u8; READ_CHUNK];
        let mut budget = READ_BUDGET;
        let mut eof = false;
        while budget > 0 && messages.len() < MAX_MESSAGES_PER_POLL {
            match self.stream.read(&mut chunk) {
                Ok(0) => {
                    eof = true;
                    break;
                }
                Ok(count) => {
                    if let Err(error) = self.reader.feed(&chunk[..count]) {
                        return Err(self.fail(
                            bye_reason::PROTOCOL_ERROR,
                            error.kind(),
                            format!("stream receive: {error}"),
                        ));
                    }
                    self.last_activity = Instant::now();
                    budget = budget.saturating_sub(count);
                    let processed = self.decode_frames(&mut messages)?;
                    if matches!(self.state, State::Closed) || processed >= MAX_MESSAGES_PER_POLL {
                        break;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    self.state = State::Closed;
                    return Err(error);
                }
            }
        }

        if !matches!(self.state, State::Closed) {
            self.flush_writes()?;
            if eof {
                if let Err(error) = self.reader.finish() {
                    return Err(self.fail(
                        bye_reason::PROTOCOL_ERROR,
                        error.kind(),
                        format!("stream receive: {error}"),
                    ));
                }
                self.state = State::Closed;
                if messages.is_empty() {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "stream closed by peer",
                    ));
                }
                return Ok(messages);
            }
            self.check_timeouts()?;
        }
        Ok(messages)
    }

    /// Send `Bye` and finish the connection once the queue drains.
    ///
    /// The method never blocks. If the socket cannot take the whole `Bye`
    /// immediately, the peer enters `Closing`; call [`StreamPeer::poll`] until
    /// it reports closed (or drop the peer, which closes the socket).
    pub fn close(&mut self, reason: u8) -> io::Result<()> {
        if matches!(self.state, State::Closed) {
            return Ok(());
        }
        if matches!(self.state, State::Closing) {
            self.flush_writes()?;
            return Ok(());
        }
        self.close_reason = Some(reason);
        let _ = self.queue_message(StreamMessage::Bye { reason });
        self.flush_writes()?;
        if self.writer.is_empty() {
            self.state = State::Closed;
        } else {
            self.state = State::Closing;
        }
        Ok(())
    }

    /// Send a ping and remember the nonce for round-trip accounting.
    pub fn ping(&mut self) -> io::Result<u64> {
        self.ensure_send_allowed(FrameKind::Ping)?;
        let nonce = self.next_ping;
        self.next_ping = self.next_ping.wrapping_add(1).max(1);
        self.queue_message(StreamMessage::Ping(nonce))?;
        self.last_ping = Some((nonce, Instant::now()));
        self.flush_writes()?;
        Ok(nonce)
    }

    pub fn metrics(&self) -> PeerMetrics {
        self.metrics
    }

    pub fn is_open(&self) -> bool {
        self.state == State::Open
    }

    pub fn is_closed(&self) -> bool {
        matches!(self.state, State::Closing | State::Closed)
    }

    /// Client label from `Hello`, empty until the handshake completes.
    pub fn peer_name(&self) -> &str {
        &self.peer_name
    }

    /// Server-generated session id. The client keeps the value from `Welcome`.
    pub fn session_id(&self) -> Uuid {
        self.session
    }

    /// Negotiated stream size. `(0, 0)` until `Welcome` arrives on a client.
    pub fn stream_size(&self) -> (u16, u16) {
        self.stream_size
    }

    pub fn max_fps(&self) -> u16 {
        self.negotiated_max_fps
    }

    /// Reason sent by the local side, if any.
    pub fn close_reason(&self) -> Option<u8> {
        self.close_reason
    }

    /// Reason received in a remote `Bye`, if any.
    pub fn remote_bye(&self) -> Option<u8> {
        self.remote_bye
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.stream.local_addr()
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.stream.peer_addr()
    }

    fn ensure_send_allowed(&self, kind: FrameKind) -> io::Result<()> {
        match self.state {
            State::Closed | State::Closing => Err(closed_error()),
            State::AwaitingHello => match kind {
                FrameKind::Ping | FrameKind::Pong => Ok(()),
                _ => Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "the server must receive Hello before other traffic",
                )),
            },
            State::AwaitingWelcome => match kind {
                FrameKind::Hello if self.role == Role::Client && !self.hello_sent => Ok(()),
                FrameKind::Ping | FrameKind::Pong => Ok(()),
                _ => Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "the client must receive Welcome before input or video",
                )),
            },
            State::Open => match kind {
                FrameKind::Input | FrameKind::Video | FrameKind::Ping | FrameKind::Pong => Ok(()),
                FrameKind::Hello | FrameKind::Welcome => Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "handshake messages are not accepted on an open stream",
                )),
                FrameKind::Bye => Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "use close(reason) to end the stream",
                )),
            },
        }
    }

    fn send_video(&mut self, frame: VideoFrame) -> io::Result<()> {
        frame.validate().map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("video frame rejected: {error}"),
            )
        })?;
        if frame.width > self.config.max_width || frame.height > self.config.max_height {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "video frame {}x{} exceeds the configured {}x{} stream",
                    frame.width, frame.height, self.config.max_width, self.config.max_height
                ),
            ));
        }
        let payload_len = frame.payload_len();
        if payload_len > MAX_FRAME_PAYLOAD {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "video payload {payload_len} bytes exceeds the {MAX_FRAME_PAYLOAD} byte frame limit"
                ),
            ));
        }
        let total = FRAME_HEADER_LEN + payload_len;
        if !self.make_room(total) {
            self.metrics.frames_dropped = self.metrics.frames_dropped.saturating_add(1);
            return Ok(());
        }
        let tail = frame.tail_bytes();
        let timestamp = frame.timestamp_millis;
        let pixels = frame.pixels;
        self.writer
            .enqueue_video(pixels, tail, timestamp)
            .map_err(queue_io)
    }

    fn make_room(&mut self, frame_bytes: usize) -> bool {
        while !self.writer.can_fit(frame_bytes) {
            if !self.writer.drop_oldest_unstarted_video() {
                return false;
            }
            self.metrics.frames_dropped = self.metrics.frames_dropped.saturating_add(1);
        }
        true
    }

    fn queue_control_bytes(&mut self, kind: FrameKind, bytes: Vec<u8>) -> io::Result<()> {
        let len = bytes.len();
        if !self.make_room(len) {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "stream send queue is full",
            ));
        }
        self.writer.enqueue_frame(kind, bytes).map_err(queue_io)
    }

    fn queue_message(&mut self, message: StreamMessage) -> io::Result<()> {
        let kind = message.kind();
        let bytes = message.encode_frame()?;
        self.queue_control_bytes(kind, bytes)
    }

    fn peek_decoded(&self) -> io::Result<Option<(io::Result<StreamMessage>, usize)>> {
        match self.reader.peek_frame()? {
            Some((kind, payload)) => Ok(Some((
                StreamMessage::decode(kind, payload),
                FRAME_HEADER_LEN + payload.len(),
            ))),
            None => Ok(None),
        }
    }

    fn decode_frames(&mut self, messages: &mut Vec<StreamMessage>) -> io::Result<usize> {
        let mut processed = 0usize;
        while processed < MAX_MESSAGES_PER_POLL && messages.len() < MAX_MESSAGES_PER_POLL {
            let decoded = match self.peek_decoded() {
                Ok(Some(decoded)) => decoded,
                Ok(None) => break,
                Err(error) => {
                    return Err(self.fail(
                        bye_reason::PROTOCOL_ERROR,
                        error.kind(),
                        format!("stream frame: {error}"),
                    ));
                }
            };
            let (message, total) = decoded;
            if let Err(error) = self.reader.consume_frame() {
                return Err(self.fail(
                    bye_reason::PROTOCOL_ERROR,
                    error.kind(),
                    format!("stream frame: {error}"),
                ));
            }
            processed = processed.saturating_add(1);
            self.metrics.bytes_received = self.metrics.bytes_received.saturating_add(total as u64);
            match message {
                Ok(message) => {
                    self.metrics.frames_received = self.metrics.frames_received.saturating_add(1);
                    self.handle_message(message, messages)?;
                    if matches!(self.state, State::Closed) {
                        break;
                    }
                }
                Err(error) => {
                    return Err(self.fail(
                        bye_reason::PROTOCOL_ERROR,
                        error.kind(),
                        format!("stream message: {error}"),
                    ));
                }
            }
        }
        Ok(processed)
    }

    fn handle_message(
        &mut self,
        message: StreamMessage,
        messages: &mut Vec<StreamMessage>,
    ) -> io::Result<()> {
        match self.state {
            State::AwaitingHello => self.handle_awaiting_hello(message, messages),
            State::AwaitingWelcome => self.handle_awaiting_welcome(message, messages),
            State::Open => self.handle_open(message, messages),
            State::Closing | State::Closed => Ok(()),
        }
    }

    fn handle_awaiting_hello(
        &mut self,
        message: StreamMessage,
        messages: &mut Vec<StreamMessage>,
    ) -> io::Result<()> {
        match message {
            StreamMessage::Hello {
                protocol,
                token,
                instance,
                client_name,
            } => {
                if self.role != Role::Server {
                    return Err(self.fail(
                        bye_reason::PROTOCOL_ERROR,
                        io::ErrorKind::InvalidData,
                        "a client received Hello",
                    ));
                }
                if protocol != PROTOCOL_VERSION {
                    return Err(self.fail(
                        bye_reason::PROTOCOL_MISMATCH,
                        io::ErrorKind::InvalidData,
                        format!(
                            "protocol version {protocol} is not the expected {PROTOCOL_VERSION}"
                        ),
                    ));
                }
                if !constant_time_eq(token.as_bytes(), self.config.token.as_bytes()) {
                    return Err(self.fail(
                        bye_reason::TOKEN_MISMATCH,
                        io::ErrorKind::PermissionDenied,
                        "handshake token mismatch",
                    ));
                }
                if instance != self.config.instance {
                    return Err(self.fail(
                        bye_reason::INSTANCE_MISMATCH,
                        io::ErrorKind::PermissionDenied,
                        "handshake instance mismatch",
                    ));
                }
                self.peer_name = client_name.clone();
                self.queue_message(StreamMessage::Welcome {
                    protocol: PROTOCOL_VERSION,
                    session: self.session,
                    width: self.config.max_width,
                    height: self.config.max_height,
                    max_fps: self.config.max_fps,
                })?;
                self.state = State::Open;
                messages.push(StreamMessage::Hello {
                    protocol,
                    token,
                    instance,
                    client_name,
                });
                Ok(())
            }
            StreamMessage::Ping(nonce) => {
                self.queue_pong(nonce);
                Ok(())
            }
            StreamMessage::Pong(nonce) => {
                self.note_pong(nonce);
                Ok(())
            }
            StreamMessage::Bye { reason } => {
                self.note_remote_bye(reason);
                messages.push(StreamMessage::Bye { reason });
                Ok(())
            }
            _ => Err(self.fail(
                bye_reason::PROTOCOL_ERROR,
                io::ErrorKind::InvalidData,
                "the first client message must be Hello",
            )),
        }
    }

    fn handle_awaiting_welcome(
        &mut self,
        message: StreamMessage,
        messages: &mut Vec<StreamMessage>,
    ) -> io::Result<()> {
        match message {
            StreamMessage::Welcome {
                protocol,
                session,
                width,
                height,
                max_fps,
            } => {
                if self.role != Role::Client {
                    return Err(self.fail(
                        bye_reason::PROTOCOL_ERROR,
                        io::ErrorKind::InvalidData,
                        "a server received Welcome",
                    ));
                }
                if protocol != PROTOCOL_VERSION {
                    return Err(self.fail(
                        bye_reason::PROTOCOL_MISMATCH,
                        io::ErrorKind::InvalidData,
                        format!(
                            "protocol version {protocol} is not the expected {PROTOCOL_VERSION}"
                        ),
                    ));
                }
                if session.is_nil() {
                    return Err(self.fail(
                        bye_reason::PROTOCOL_ERROR,
                        io::ErrorKind::InvalidData,
                        "Welcome carried a nil session id",
                    ));
                }
                if width == 0
                    || height == 0
                    || width > MAX_VIDEO_DIMENSION
                    || height > MAX_VIDEO_DIMENSION
                {
                    return Err(self.fail(
                        bye_reason::PROTOCOL_ERROR,
                        io::ErrorKind::InvalidData,
                        "Welcome carried an invalid stream size",
                    ));
                }
                if max_fps == 0 || max_fps > MAX_FPS {
                    return Err(self.fail(
                        bye_reason::PROTOCOL_ERROR,
                        io::ErrorKind::InvalidData,
                        "Welcome carried an invalid max_fps",
                    ));
                }
                self.session = session;
                self.stream_size = (width, height);
                self.negotiated_max_fps = max_fps;
                self.state = State::Open;
                messages.push(StreamMessage::Welcome {
                    protocol,
                    session,
                    width,
                    height,
                    max_fps,
                });
                Ok(())
            }
            StreamMessage::Ping(nonce) => {
                self.queue_pong(nonce);
                Ok(())
            }
            StreamMessage::Pong(nonce) => {
                self.note_pong(nonce);
                Ok(())
            }
            StreamMessage::Bye { reason } => {
                self.note_remote_bye(reason);
                messages.push(StreamMessage::Bye { reason });
                Ok(())
            }
            _ => Err(self.fail(
                bye_reason::PROTOCOL_ERROR,
                io::ErrorKind::InvalidData,
                "the first server message must be Welcome",
            )),
        }
    }

    fn handle_open(
        &mut self,
        message: StreamMessage,
        messages: &mut Vec<StreamMessage>,
    ) -> io::Result<()> {
        match message {
            StreamMessage::Input(frame) => {
                self.record_input_latency(&frame);
                messages.push(StreamMessage::Input(frame));
                Ok(())
            }
            StreamMessage::Video(frame) => {
                messages.push(StreamMessage::Video(frame));
                Ok(())
            }
            StreamMessage::Ping(nonce) => {
                self.queue_pong(nonce);
                Ok(())
            }
            StreamMessage::Pong(nonce) => {
                self.note_pong(nonce);
                Ok(())
            }
            StreamMessage::Bye { reason } => {
                self.note_remote_bye(reason);
                messages.push(StreamMessage::Bye { reason });
                Ok(())
            }
            StreamMessage::Hello { .. } => Err(self.fail(
                bye_reason::PROTOCOL_ERROR,
                io::ErrorKind::InvalidData,
                "duplicate Hello on an open stream",
            )),
            StreamMessage::Welcome { .. } => Err(self.fail(
                bye_reason::PROTOCOL_ERROR,
                io::ErrorKind::InvalidData,
                "duplicate Welcome on an open stream",
            )),
        }
    }

    fn queue_pong(&mut self, nonce: u64) {
        let _ = self.queue_message(StreamMessage::Pong(nonce));
    }

    fn note_pong(&mut self, nonce: u64) {
        if let Some((expected, sent_at)) = self.last_ping {
            if expected == nonce {
                self.metrics.rtt_millis_estimate =
                    sent_at.elapsed().as_millis().min(u32::MAX as u128) as u32;
                self.last_ping = None;
            }
        }
    }

    fn note_remote_bye(&mut self, reason: u8) {
        self.remote_bye = Some(reason);
        self.state = State::Closed;
        self.reader.clear();
        self.writer.clear();
    }

    fn record_input_latency(&mut self, frame: &InputFrame) {
        if frame.client_millis == 0 {
            return;
        }
        let age = epoch_millis().saturating_sub(frame.client_millis);
        self.metrics.input_latency.record(age);
    }

    fn flush_writes(&mut self) -> io::Result<()> {
        if self.writer.is_empty() {
            if matches!(self.state, State::Closing) {
                self.state = State::Closed;
            }
            return Ok(());
        }
        let metrics = &mut self.metrics;
        let result = self.writer.flush(&mut self.stream, |completed| {
            metrics.frames_sent = metrics.frames_sent.saturating_add(1);
            if let Some(timestamp) = completed.video_timestamp_millis {
                if timestamp > 0 {
                    metrics
                        .video_send_latency
                        .record(epoch_millis().saturating_sub(timestamp));
                }
            }
        });
        let report = match result {
            Ok(report) => report,
            Err(error) => {
                self.state = State::Closed;
                return Err(error);
            }
        };
        self.metrics.bytes_sent = self.metrics.bytes_sent.saturating_add(report.bytes_written);
        if report.bytes_written > 0 {
            self.last_activity = Instant::now();
        }
        if matches!(self.state, State::Closing) && self.writer.is_empty() {
            self.state = State::Closed;
        }
        Ok(())
    }

    fn check_timeouts(&mut self) -> io::Result<()> {
        let now = Instant::now();
        match self.state {
            State::AwaitingHello | State::AwaitingWelcome => {
                if now >= self.handshake_deadline {
                    return Err(self.fail(
                        bye_reason::HANDSHAKE_TIMEOUT,
                        io::ErrorKind::TimedOut,
                        "stream handshake timed out",
                    ));
                }
            }
            State::Open => {
                if now.saturating_duration_since(self.last_activity) >= self.config.idle_timeout {
                    return Err(self.fail(
                        bye_reason::IDLE_TIMEOUT,
                        io::ErrorKind::TimedOut,
                        "stream idle timeout",
                    ));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn fail(&mut self, reason: u8, kind: io::ErrorKind, message: impl Into<String>) -> io::Error {
        self.close_reason = Some(reason);
        let _ = self.queue_message(StreamMessage::Bye { reason });
        let _ = self.flush_writes();
        if self.writer.is_empty() {
            self.state = State::Closed;
        } else {
            self.state = State::Closing;
        }
        let message: String = message.into();
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

fn invalid_input(message: impl Into<String>) -> io::Error {
    let message: String = message.into();
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn closed_error() -> io::Error {
    io::Error::new(io::ErrorKind::NotConnected, "stream peer is closed")
}

fn queue_io(error: QueueError) -> io::Error {
    let kind = match error {
        QueueError::Full => io::ErrorKind::WouldBlock,
        QueueError::FrameTooLarge | QueueError::Malformed => io::ErrorKind::InvalidInput,
    };
    io::Error::new(kind, format!("stream send queue: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    fn config(token: Uuid, instance: Uuid) -> StreamConfig {
        StreamConfig {
            token,
            instance,
            max_width: 320,
            max_height: 180,
            max_fps: 60,
            queue_frames: 8,
            idle_timeout: Duration::from_secs(5),
        }
    }

    #[test]
    fn matching_hello_opens_both_sides() {
        let token = Uuid::new_v4();
        let instance = Uuid::new_v4();
        let config = config(token, instance);
        let server = StreamServer::bind(("127.0.0.1", 0), config.clone()).unwrap();
        let address = server.local_addr().unwrap();
        let mut client = StreamPeer::connect(address, &config, "probe-client").unwrap();

        let mut accepted = None;
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut welcome = false;
        while !welcome {
            if accepted.is_none() {
                accepted = server.accept().unwrap();
            }
            if let Some(peer) = accepted.as_mut() {
                peer.poll().unwrap();
            }
            for message in client.poll().unwrap() {
                if matches!(message, StreamMessage::Welcome { .. }) {
                    welcome = true;
                }
            }
            assert!(Instant::now() < deadline, "handshake timed out");
            thread::sleep(Duration::from_millis(1));
        }

        let peer = accepted.expect("server accepted the client");
        assert!(peer.is_open());
        assert!(client.is_open());
        assert_eq!(peer.peer_name(), "probe-client");
        assert_eq!(peer.session_id(), client.session_id());
        assert_eq!(client.stream_size(), (320, 180));
        assert_eq!(client.max_fps(), 60);
    }

    #[test]
    fn wrong_token_is_rejected_with_bye() {
        let token = Uuid::new_v4();
        let instance = Uuid::new_v4();
        let server = StreamServer::bind(("127.0.0.1", 0), config(token, instance)).unwrap();
        let address = server.local_addr().unwrap();
        let client_config = config(Uuid::new_v4(), instance);
        let mut client = StreamPeer::connect(address, &client_config, "intruder").unwrap();

        let mut accepted = None;
        let deadline = Instant::now() + Duration::from_secs(3);
        let error = loop {
            if accepted.is_none() {
                accepted = server.accept().unwrap();
            }
            if let Some(peer) = accepted.as_mut() {
                match peer.poll() {
                    Ok(_) => {}
                    Err(error) => break error,
                }
            }
            let _ = client.poll();
            assert!(Instant::now() < deadline, "handshake rejection timed out");
            thread::sleep(Duration::from_millis(1));
        };

        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(error.to_string().contains("token"));
        assert!(accepted.as_ref().unwrap().is_closed());
    }
}
