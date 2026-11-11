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
    /// Result of a tool call, sent back to the model.
    Tool,
}

/// Provider-returned reasoning, kept apart from the assistant's answer.
/// The origin prevents forwarding provider-specific traces to another model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderReasoning {
    pub endpoint: String,
    pub model: String,
    #[serde(default)]
    pub transport: String,
    #[serde(default)]
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<serde_json::Value>,
}

/// A single chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: Uuid,
    pub role: MessageRole,
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<crate::chat_media::ChatImage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ProviderReasoning>,
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
            images: Vec::new(),
            reasoning: None,
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
            images: Vec::new(),
            reasoning: None,
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
            images: Vec::new(),
            reasoning: None,
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
            messages: Vec::new(),
            input_text: String::new(),
            is_processing: false,
            is_available: true,
        }
    }
}
