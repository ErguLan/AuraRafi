//! Little-endian payload primitives shared by wire message types.
//!
//! Readers borrow their input and only allocate for strings. Writers reserve
//! the frame header upfront, so control and input messages reach the send
//! queue as one allocation instead of a payload buffer plus a copied frame.

use crate::frame::{header, FrameKind};
use crate::protocol::FRAME_HEADER_LEN;
use std::io;
use uuid::Uuid;

pub(crate) struct PayloadReader<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> PayloadReader<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn take(&mut self, len: usize, field: &str) -> io::Result<&'a [u8]> {
        let available = self.data.len().saturating_sub(self.offset);
        if len > available {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{field}: truncated payload (needed {len} bytes, {available} available)"),
            ));
        }
        let start = self.offset;
        self.offset += len;
        Ok(&self.data[start..self.offset])
    }

    pub(crate) fn u8(&mut self, field: &str) -> io::Result<u8> {
        Ok(self.take(1, field)?[0])
    }

    pub(crate) fn u16(&mut self, field: &str) -> io::Result<u16> {
        let bytes = self.take(2, field)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    pub(crate) fn u32(&mut self, field: &str) -> io::Result<u32> {
        let bytes = self.take(4, field)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub(crate) fn u64(&mut self, field: &str) -> io::Result<u64> {
        let bytes = self.take(8, field)?;
        Ok(u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    pub(crate) fn f32(&mut self, field: &str) -> io::Result<f32> {
        Ok(f32::from_bits(self.u32(field)?))
    }

    pub(crate) fn uuid(&mut self, field: &str) -> io::Result<Uuid> {
        let bytes = self.take(16, field)?;
        let mut raw = [0u8; 16];
        raw.copy_from_slice(bytes);
        Ok(Uuid::from_bytes(raw))
    }

    pub(crate) fn string(&mut self, max_bytes: usize, field: &str) -> io::Result<String> {
        let len = self.u16(field)? as usize;
        if len > max_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{field}: length {len} exceeds the {max_bytes} byte limit"),
            ));
        }
        let bytes = self.take(len, field)?;
        let text = std::str::from_utf8(bytes).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{field}: value is not valid UTF-8"),
            )
        })?;
        Ok(text.to_string())
    }

    pub(crate) fn finish(&self, field: &str) -> io::Result<()> {
        if self.offset == self.data.len() {
            return Ok(());
        }
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{field}: {} trailing bytes after the expected fields",
                self.data.len() - self.offset
            ),
        ))
    }
}

/// Payload writer that reserves the binary frame header at the front.
pub(crate) struct PayloadWriter {
    data: Vec<u8>,
}

impl PayloadWriter {
    pub(crate) fn new(capacity_hint: usize) -> Self {
        let mut data = Vec::with_capacity(FRAME_HEADER_LEN.saturating_add(capacity_hint));
        data.resize(FRAME_HEADER_LEN, 0);
        Self { data }
    }

    pub(crate) fn u8(&mut self, value: u8) {
        self.data.push(value);
    }

    pub(crate) fn u16(&mut self, value: u16) {
        self.bytes(&value.to_le_bytes());
    }

    pub(crate) fn u32(&mut self, value: u32) {
        self.bytes(&value.to_le_bytes());
    }

    pub(crate) fn u64(&mut self, value: u64) {
        self.bytes(&value.to_le_bytes());
    }

    pub(crate) fn f32(&mut self, value: f32) {
        self.bytes(&value.to_bits().to_le_bytes());
    }

    pub(crate) fn uuid(&mut self, value: Uuid) {
        self.bytes(value.as_bytes());
    }

    pub(crate) fn bytes(&mut self, bytes: &[u8]) {
        self.data.extend_from_slice(bytes);
    }

    pub(crate) fn string(&mut self, value: &str, max_bytes: usize, field: &str) -> io::Result<()> {
        if value.len() > max_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "{field}: {} bytes exceeds the {max_bytes} byte limit",
                    value.len()
                ),
            ));
        }
        if value.len() > u16::MAX as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{field}: value is too long for a u16 length prefix"),
            ));
        }
        self.u16(value.len() as u16);
        self.bytes(value.as_bytes());
        Ok(())
    }

    pub(crate) fn into_frame(self, kind: FrameKind) -> io::Result<Vec<u8>> {
        if self.data.len() < FRAME_HEADER_LEN {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "payload writer lost its reserved frame header",
            ));
        }
        let payload_len = self.data.len() - FRAME_HEADER_LEN;
        let mut data = self.data;
        let frame_header = header(kind, payload_len)?;
        data[..FRAME_HEADER_LEN].copy_from_slice(&frame_header);
        Ok(data)
    }
}
