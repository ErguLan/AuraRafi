//! # raf_ai
//!
//! AI agent interface for AuraRafi. Provides:
//! - Tool registry: every engine operation exposed as an invocable tool
//! - Chat message protocol for bidirectional communication
//! - Provider selector (OpenClaw, OpenRouter, OpenAI, GenAI, Claude)
//! - OpenClaw client for self-hosted AI via localhost
//! - AI Director: observes world state, emits actions (weather, spawn, behavior)
//! - Asset generation: AI-generated meshes, textures, terrain
//! - Mesh streaming: incremental mesh data from AI/procedural sources
//!
//! **Status**: Chat + OpenClaw ready. Director/AssetGen/MeshProvider prepared but not connected.

pub mod asset_gen;
pub mod chat;
pub mod director;
pub mod mesh_provider;
pub mod openclaw;
pub mod provider;
pub mod tool_registry;

pub use chat::{ChatMessage, ChatPanel, MessageRole};
pub use director::{DirectorAction, DirectorConfig, DirectorMode, DirectorState};
pub use asset_gen::{AssetGenConfig, AssetGenCache, AssetGenHandle, AssetGenRequest, GeneratedMesh};
pub use mesh_provider::{MeshChunk, MeshProviderConfig, MeshProviderState, MeshProviderType};
pub use openclaw::{OpenClawClient, OpenClawConfig, ConnectionStatus};
pub use provider::{AiProvider, AiProviderConfig};
pub use tool_registry::{ToolDefinition, ToolParameter, ToolRegistry};
