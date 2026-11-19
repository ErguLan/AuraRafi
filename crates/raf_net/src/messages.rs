//! Stream messages and their little-endian payload codec.
//!
//! Control and input messages are encoded directly into a frame buffer with
//! one allocation. Video frames carry raw pixels first and a fixed metadata
//! tail last; `session` queues that layout as separate segments so the pixel
//! buffer is moved instead of copied.

use crate::codec::{PayloadReader, PayloadWriter};
use crate::frame::FrameKind;
use crate::input::InputFrame;
use crate::protocol::MAX_CLIENT_NAME;
use crate::video::VideoFrame;
use std::io;
use uuid::Uuid;

/// One protocol message.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamMessage {
    /// First client message. Must arrive within the handshake window.
    Hello {
        protocol: u16,
        token: Uuid,
        instance: Uuid,
        /// UTF-8 label, at most [`MAX_CLIENT_NAME`] bytes.
        client_name: String,
    },
    /// Server response that opens the session.
    Welcome {
        protocol: u16,
        session: Uuid,
        width: u16,
        height: u16,
        max_fps: u16,
    },
    /// Authoritative input state.
    Input(InputFrame),
    /// Raw RGBA video.
    Video(VideoFrame),
    /// Round-trip probe.
    Ping(u64),
    /// Round-trip answer.
    Pong(u64),
    /// Orderly close with a [`crate::protocol::bye_reason`] code.
    Bye { reason: u8 },
}

impl StreamMessage {
    pub fn kind(&self) -> FrameKind {
        match self {
            Self::Hello { .. } => FrameKind::Hello,
            Self::Welcome { .. } => FrameKind::Welcome,
            Self::Input(_) => FrameKind::Input,
            Self::Video(_) => FrameKind::Video,
            Self::Ping(_) => FrameKind::Ping,
            Self::Pong(_) => FrameKind::Pong,
            Self::Bye { .. } => FrameKind::Bye,
        }
    }

    /// Encode the message as one complete wire frame.
    pub fn encode_frame(&self) -> io::Result<Vec<u8>> {
        match self {
            Self::Hello {
                protocol,
                token,
                instance,
                client_name,
            } => {
                let mut writer = PayloadWriter::new(2 + 16 + 16 + 2 + client_name.len());
                writer.u16(*protocol);
                writer.uuid(*token);
                writer.uuid(*instance);
                writer.string(client_name, MAX_CLIENT_NAME, "hello.client_name")?;
                writer.into_frame(FrameKind::Hello)
            }
            Self::Welcome {
                protocol,
                session,
                width,
                height,
                max_fps,
            } => {
                let mut writer = PayloadWriter::new(2 + 16 + 2 + 2 + 2);
                writer.u16(*protocol);
                writer.uuid(*session);
                writer.u16(*width);
                writer.u16(*height);
                writer.u16(*max_fps);
                writer.into_frame(FrameKind::Welcome)
            }
            Self::Input(frame) => {
                let mut writer = PayloadWriter::new(40 + frame.held_keys.len().saturating_mul(2));
                frame.encode_payload_into(&mut writer)?;
                writer.into_frame(FrameKind::Input)
            }
            Self::Video(frame) => frame.encode_frame(FrameKind::Video),
            Self::Ping(nonce) => {
                let mut writer = PayloadWriter::new(8);
                writer.u64(*nonce);
                writer.into_frame(FrameKind::Ping)
            }
            Self::Pong(nonce) => {
                let mut writer = PayloadWriter::new(8);
                writer.u64(*nonce);
                writer.into_frame(FrameKind::Pong)
            }
            Self::Bye { reason } => {
                let mut writer = PayloadWriter::new(1);
                writer.u8(*reason);
                writer.into_frame(FrameKind::Bye)
            }
        }
    }

    /// Decode one payload previously extracted from a frame header.
    pub fn decode(kind: FrameKind, payload: &[u8]) -> io::Result<Self> {
        match kind {
            FrameKind::Hello => {
                let mut reader = PayloadReader::new(payload);
                let protocol = reader.u16("hello.protocol")?;
                let token = reader.uuid("hello.token")?;
                let instance = reader.uuid("hello.instance")?;
                let client_name = reader.string(MAX_CLIENT_NAME, "hello.client_name")?;
                reader.finish("hello")?;
                Ok(Self::Hello {
                    protocol,
                    token,
                    instance,
                    client_name,
                })
            }
            FrameKind::Welcome => {
                let mut reader = PayloadReader::new(payload);
                let protocol = reader.u16("welcome.protocol")?;
                let session = reader.uuid("welcome.session")?;
                let width = reader.u16("welcome.width")?;
                let height = reader.u16("welcome.height")?;
                let max_fps = reader.u16("welcome.max_fps")?;
                reader.finish("welcome")?;
                Ok(Self::Welcome {
                    protocol,
                    session,
                    width,
                    height,
                    max_fps,
                })
            }
            FrameKind::Input => Ok(Self::Input(InputFrame::decode_payload(payload)?)),
            FrameKind::Video => Ok(Self::Video(VideoFrame::decode_payload(payload)?)),
            FrameKind::Ping => {
                let mut reader = PayloadReader::new(payload);
                let nonce = reader.u64("ping.nonce")?;
                reader.finish("ping")?;
                Ok(Self::Ping(nonce))
            }
            FrameKind::Pong => {
                let mut reader = PayloadReader::new(payload);
                let nonce = reader.u64("pong.nonce")?;
                reader.finish("pong")?;
                Ok(Self::Pong(nonce))
            }
            FrameKind::Bye => {
                let mut reader = PayloadReader::new(payload);
                let reason = reader.u8("bye.reason")?;
                reader.finish("bye")?;
                Ok(Self::Bye { reason })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::FRAME_HEADER_LEN;

    fn payload_of(message: &StreamMessage) -> Vec<u8> {
        let frame = message.encode_frame().expect("encode frame");
        frame[FRAME_HEADER_LEN..].to_vec()
    }

    #[test]
    fn control_messages_round_trip() {
        let messages = [
            StreamMessage::Hello {
                protocol: crate::protocol::PROTOCOL_VERSION,
                token: Uuid::new_v4(),
                instance: Uuid::new_v4(),
                client_name: "raf-player".to_string(),
            },
            StreamMessage::Welcome {
                protocol: crate::protocol::PROTOCOL_VERSION,
                session: Uuid::new_v4(),
                width: 1280,
                height: 720,
                max_fps: 60,
            },
            StreamMessage::Ping(42),
            StreamMessage::Pong(42),
            StreamMessage::Bye { reason: 3 },
        ];
        for message in messages {
            let payload = payload_of(&message);
            let decoded = StreamMessage::decode(message.kind(), &payload).unwrap();
            assert_eq!(decoded, message);
        }
    }

    #[test]
    fn client_name_is_limited_to_the_wire_budget() {
        let message = StreamMessage::Hello {
            protocol: crate::protocol::PROTOCOL_VERSION,
            token: Uuid::new_v4(),
            instance: Uuid::new_v4(),
            client_name: "x".repeat(MAX_CLIENT_NAME + 1),
        };
        let error = message.encode_frame().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(error.to_string().contains("client_name"));
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let message = StreamMessage::Ping(7);
        let mut payload = payload_of(&message);
        payload.push(0);
        let error = StreamMessage::decode(FrameKind::Ping, &payload).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("trailing"));
    }

    #[test]
    fn truncated_payload_is_rejected() {
        let message = StreamMessage::Welcome {
            protocol: crate::protocol::PROTOCOL_VERSION,
            session: Uuid::new_v4(),
            width: 640,
            height: 360,
            max_fps: 30,
        };
        let payload = payload_of(&message);
        let error =
            StreamMessage::decode(FrameKind::Welcome, &payload[..payload.len() - 1]).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }
}
