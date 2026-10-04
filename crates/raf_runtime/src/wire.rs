//! Bounded loopback control protocol, separate from editor/Agent command execution.
use crate::{RuntimeControl, RuntimeStatus};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::TcpStream;
use uuid::Uuid;
const FRAME_LIMIT: usize = 16 * 1024;
const QUEUE_LIMIT: usize = 32;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuntimePacket {
    Hello { token: Uuid, instance: Uuid },
    Control(RuntimeControl),
    Status(RuntimeStatus),
    Diagnostic(String),
}
pub struct RuntimePeer {
    stream: TcpStream,
    input: Vec<u8>,
    output: VecDeque<Vec<u8>>,
    offset: usize,
    closed: bool,
}
impl RuntimePeer {
    pub fn new(stream: TcpStream) -> Result<Self, String> {
        stream.set_nonblocking(true).map_err(|e| e.to_string())?;
        stream.set_nodelay(true).map_err(|e| e.to_string())?;
        Ok(Self {
            stream,
            input: Vec::new(),
            output: VecDeque::new(),
            offset: 0,
            closed: false,
        })
    }
    pub fn send(&mut self, packet: RuntimePacket) -> Result<(), String> {
        if self.output.len() >= QUEUE_LIMIT {
            return Err("runtime control queue is full".into());
        }
        let mut bytes = serde_json::to_vec(&packet).map_err(|e| e.to_string())?;
        if bytes.len() >= FRAME_LIMIT {
            return Err("runtime control frame is too large".into());
        }
        bytes.push(b'\n');
        self.output.push_back(bytes);
        Ok(())
    }
    pub fn poll(&mut self) -> Result<Vec<RuntimePacket>, String> {
        if self.closed && self.input.is_empty() {
            return Err("runtime control connection closed".into());
        }
        let mut read_budget = FRAME_LIMIT * 2;
        let mut block = [0; 2048];
        while read_budget > 0 {
            match self.stream.read(&mut block) {
                Ok(0) => {
                    self.closed = true;
                    break;
                }
                Ok(count) => {
                    self.input.extend_from_slice(&block[..count]);
                    read_budget = read_budget.saturating_sub(count);
                    if self.input.len() > FRAME_LIMIT * QUEUE_LIMIT {
                        return Err("runtime control receive limit exceeded".into());
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e.to_string()),
            }
        }
        let mut packets = Vec::new();
        while packets.len() < QUEUE_LIMIT {
            let Some(end) = self.input.iter().position(|b| *b == b'\n') else {
                if self.closed && !self.input.is_empty() {
                    return Err("truncated runtime control frame".into());
                }
                if self.input.len() >= FRAME_LIMIT {
                    return Err("unterminated runtime control frame".into());
                }
                break;
            };
            if end >= FRAME_LIMIT {
                return Err("runtime control frame limit exceeded".into());
            }
            let packet = serde_json::from_slice(&self.input[..end])
                .map_err(|e| format!("runtime control decode: {e}"))?;
            self.input.drain(..=end);
            packets.push(packet);
        }
        // Partial nonblocking writes preserve packet boundaries.
        let mut write_budget = FRAME_LIMIT * 2;
        while write_budget > 0 && !self.closed {
            let Some(bytes) = self.output.front() else {
                break;
            };
            match self.stream.write(&bytes[self.offset..]) {
                Ok(0) => return Err("runtime control write closed".into()),
                Ok(count) => {
                    self.offset += count;
                    write_budget = write_budget.saturating_sub(count);
                    if self.offset == bytes.len() {
                        self.output.pop_front();
                        self.offset = 0;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e.to_string()),
            }
        }
        Ok(packets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loopback_preserves_packets_and_rejects_floods_and_oversized_frames() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let mut a = RuntimePeer::new(stream).unwrap();
        let mut b = RuntimePeer::new(listener.accept().unwrap().0).unwrap();
        a.send(RuntimePacket::Control(RuntimeControl::Step))
            .unwrap();
        a.poll().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            let messages = b.poll().unwrap();
            if !messages.is_empty() {
                assert!(matches!(
                    messages[0],
                    RuntimePacket::Control(RuntimeControl::Step)
                ));
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(a
            .send(RuntimePacket::Diagnostic("x".repeat(FRAME_LIMIT)))
            .is_err());
        for _ in 0..QUEUE_LIMIT {
            a.send(RuntimePacket::Control(RuntimeControl::Pause))
                .unwrap();
        }
        assert!(a
            .send(RuntimePacket::Control(RuntimeControl::Pause))
            .is_err());
    }
}
