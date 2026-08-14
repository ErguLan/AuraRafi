//! # raf_ai
//!
//! AI agent interface for AuraRafi. Provides:
//! - Tool registry: canonical command catalog exposed as invocable tools,
//!   with a compatibility registry for older callers
//! - Chat message protocol for bidirectional communication
//! - Provider selector (Puerto, OpenRouter, OpenAI, GenAI, Claude)
//! - Puerto bridge client for OpenClawd / OpenClaw gateways
//! - Generic OpenAI-compatible chat client
//! - Agent runtime: planning, tool-calling loop, history persistence
//! - AI Director: observes world state, emits actions (weather, spawn, behavior)
//! - Asset generation: AI-generated meshes, textures, terrain
//! - Mesh streaming: incremental mesh data from AI/procedural sources
//!
//! **Status**: Agent runtime + OpenAI-compatible client wired. Director/AssetGen/MeshProvider prepared but not connected.

pub mod agent_history;
pub mod agent_model_registry;
pub mod agent_runtime;
pub mod asset_gen;
pub mod chat;
pub mod director;
pub mod image_worker;
pub mod mesh_provider;
pub mod openai_client;
pub mod provider;
pub mod puerto;
pub mod tool_registry;

pub use agent_history::{AgentHistory, AgentSession};
pub use agent_model_registry::AgentModelRegistry;
pub use agent_runtime::{AgentEvent, AgentRuntime, AgentStatus, ToolExecutor};
pub use asset_gen::{
    AssetGenCache, AssetGenConfig, AssetGenHandle, AssetGenRequest, GeneratedMesh,
};
pub use chat::{ChatMessage, ChatPanel, MessageRole};
pub use director::{DirectorAction, DirectorConfig, DirectorMode, DirectorState};
pub use image_worker::{
    AssetImageGenerationMode, AssetImageGenerationQueue, AssetImageJob, AssetImageJobSnapshot,
    AssetImageJobStatus, AssetImageRequest, AssetImageSize, AssetImageWorker,
    AssetImageWorkerConfig, AssetLocalPngStyle, GeneratedImageAsset,
};
pub use mesh_provider::{MeshChunk, MeshProviderConfig, MeshProviderState, MeshProviderType};
pub use openai_client::{OpenAiClient, OpenAiConfig};
pub use provider::{AgentMode, AiModelShortcut, AiProvider, AiProviderConfig};
pub use puerto::{ConnectionStatus, PuertoClient, PuertoConfig};
pub use tool_registry::{ToolDefinition, ToolParameter, ToolRegistry};
