//! # raf_ai
//!
//! AI agent interface for AuraRafi. Provides:
//! - Tool registry: every engine operation exposed as an invocable tool
//! - Chat message protocol for bidirectional communication
//! - Provider selector (OpenClaw, OpenRouter, OpenAI, GenAI, Claude)
//! - OpenClaw client for self-hosted AI via localhost
//!
//! **Status**: OpenClaw basic integration ready. Other providers pending.

pub mod chat;
pub mod openclaw;
pub mod provider;
pub mod tool_registry;

pub use chat::{ChatMessage, ChatPanel, MessageRole};
pub use openclaw::{OpenClawClient, OpenClawConfig, ConnectionStatus};
pub use provider::{AiProvider, AiProviderConfig};
pub use tool_registry::{ToolDefinition, ToolParameter, ToolRegistry};
