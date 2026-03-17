//! Serial port communication abstraction.
//!
//! Provides a lightweight interface for talking to microcontrollers
//! (Arduino, ESP32, STM32, etc.) over serial/UART. The protocol uses
//! simple JSON lines for easy parsing on both sides.
//!
//! Future: will use the `serialport` crate for actual I/O.
//! For now, this defines the data model and message protocol.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Serial port configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerialConfig {
    /// Port name (e.g. "COM3" on Windows, "/dev/ttyUSB0" on Linux).
    pub port: String,
    /// Baud rate (default: 115200).
    pub baud_rate: u32,
    /// Data bits (default: 8).
    pub data_bits: u8,
    /// Stop bits (default: 1).
    pub stop_bits: u8,
    /// Read timeout in milliseconds (0 = no timeout).
    pub timeout_ms: u64,
}

impl Default for SerialConfig {
    fn default() -> Self {
        Self {
            port: String::new(),
            baud_rate: 115200,
            data_bits: 8,
            stop_bits: 1,
            timeout_ms: 1000,
        }
    }
}

// ---------------------------------------------------------------------------
// Message protocol
// ---------------------------------------------------------------------------

/// Direction of a serial message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageDirection {
    /// Sent from AuraRafi to the device.
    ToDevice,
    /// Received from the device.
    FromDevice,
}

/// A message exchanged over serial.
///
/// Protocol: each message is a single JSON line terminated by '\n'.
/// Example: `{"type":"sensor","key":"temperature","value":23.5}`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerialMessage {
    /// Message type identifier.
    pub msg_type: String,
    /// Key (sensor name, command name, etc.).
    pub key: String,
    /// Value as a JSON string (numbers, strings, booleans).
    pub value: String,
    /// Direction.
    pub direction: MessageDirection,
    /// Timestamp in milliseconds since connection start.
    pub timestamp_ms: u64,
}

// ---------------------------------------------------------------------------
// Serial port handle (stub)
// ---------------------------------------------------------------------------

/// State of a serial connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Error,
}

/// Serial port handle.
///
/// Currently a stub. When `serialport` crate is added, this will
/// hold the actual port handle and read/write buffers.
pub struct SerialPort {
    pub config: SerialConfig,
    pub state: ConnectionState,
    /// Incoming message buffer.
    pub inbox: Vec<SerialMessage>,
    /// Outgoing message buffer.
    pub outbox: Vec<SerialMessage>,
}

impl Default for SerialPort {
    fn default() -> Self {
        Self {
            config: SerialConfig::default(),
            state: ConnectionState::Disconnected,
            inbox: Vec::new(),
            outbox: Vec::new(),
        }
    }
}

impl SerialPort {
    /// Create a new disconnected serial port with the given config.
    pub fn new(config: SerialConfig) -> Self {
        Self {
            config,
            state: ConnectionState::Disconnected,
            inbox: Vec::new(),
            outbox: Vec::new(),
        }
    }

    /// Attempt to connect. Stub: just sets state.
    /// Future: opens the actual serial port file descriptor.
    pub fn connect(&mut self) -> Result<(), String> {
        if self.config.port.is_empty() {
            return Err("No port specified".to_string());
        }
        tracing::info!("Serial: connecting to {} at {} baud", self.config.port, self.config.baud_rate);
        // Stub: mark as connected.
        self.state = ConnectionState::Connected;
        Ok(())
    }

    /// Disconnect.
    pub fn disconnect(&mut self) {
        self.state = ConnectionState::Disconnected;
        self.inbox.clear();
        self.outbox.clear();
        tracing::info!("Serial: disconnected");
    }

    /// Queue a message to send.
    pub fn send(&mut self, msg_type: &str, key: &str, value: &str) {
        self.outbox.push(SerialMessage {
            msg_type: msg_type.to_string(),
            key: key.to_string(),
            value: value.to_string(),
            direction: MessageDirection::ToDevice,
            timestamp_ms: 0,
        });
    }

    /// Drain received messages.
    pub fn receive(&mut self) -> Vec<SerialMessage> {
        std::mem::take(&mut self.inbox)
    }

    /// List available serial ports on the system.
    /// Stub: returns empty. Future: uses serialport::available_ports().
    pub fn list_available() -> Vec<String> {
        // Will be populated when serialport crate is added.
        Vec::new()
    }
}
