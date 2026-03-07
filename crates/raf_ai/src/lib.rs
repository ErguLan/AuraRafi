//! # raf_ai
//!
//! AI agent interface for AuraRafi. Provides:
//! - Tool registry: every engine operation exposed as an invocable tool
//! - Chat message protocol for bidirectional communication
//! - Provider selector structure (OpenRouter, OpenAI, GenAI, Claude)
//! - AI context provider for sending scene state to AI
//!
//! **Status**: Structure prepared. AI functionality is not yet implemented.

pub mod chat;
pub mod provider;
pub mod tool_registry;

pub use chat::{ChatMessage, ChatPanel, MessageRole};
pub use provider::{AiProvider, AiProviderConfig};
pub use tool_registry::{ToolDefinition, ToolParameter, ToolRegistry};
