//! Networking protocol definitions.
//!
//! Stub for future multiplayer implementation.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Network message types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetMessageType {
    /// Client wants to join.
    Connect,
    /// Client is leaving.
    Disconnect,
    /// State synchronization update.
    StateSync,
    /// Remote procedure call.
    Rpc,
    /// Heartbeat / keepalive.
    Ping,
    /// Heartbeat response.
    Pong,
}

/// A network message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetMessage {
    pub id: Uuid,
    pub message_type: NetMessageType,
    pub sender_id: Uuid,
    pub payload: serde_json::Value,
    pub timestamp_ms: u64,
}

impl NetMessage {
    /// Create a new message.
    pub fn new(message_type: NetMessageType, sender_id: Uuid, payload: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            message_type,
            sender_id,
            payload,
            timestamp_ms: 0, // Would be set by transport layer.
        }
    }
}
