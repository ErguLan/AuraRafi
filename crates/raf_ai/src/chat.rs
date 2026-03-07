//! Chat message model and panel state for the AI interface.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Who sent the message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

/// A single chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: Uuid,
    pub role: MessageRole,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    /// Tool calls the assistant wants to make (JSON array, if any).
    pub tool_calls: Option<serde_json::Value>,
}

impl ChatMessage {
    /// Create a new user message.
    pub fn user(content: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: MessageRole::User,
            content: content.to_string(),
            timestamp: Utc::now(),
            tool_calls: None,
        }
    }

    /// Create a new assistant message.
    pub fn assistant(content: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: MessageRole::Assistant,
            content: content.to_string(),
            timestamp: Utc::now(),
            tool_calls: None,
        }
    }

    /// Create a system message.
    pub fn system(content: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: MessageRole::System,
            content: content.to_string(),
            timestamp: Utc::now(),
            tool_calls: None,
        }
    }
}

/// State for the AI chat panel.
pub struct ChatPanel {
    /// Message history.
    pub messages: Vec<ChatMessage>,
    /// Current input text.
    pub input_text: String,
    /// Whether AI is currently processing.
    pub is_processing: bool,
    /// Whether AI functionality is available.
    pub is_available: bool,
}

impl Default for ChatPanel {
    fn default() -> Self {
        Self {
            messages: vec![ChatMessage::system(
                "AI Integration: Haven't been developed yet. \
                 This panel will allow AI agents to interact with the engine \
                 using tool-calling to create, modify, and manage your project.",
            )],
            input_text: String::new(),
            is_processing: false,
            is_available: false, // Not functional yet.
        }
    }
}
