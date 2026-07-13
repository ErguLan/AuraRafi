//! AI provider configuration re-exported from raf_core.
//!
//! This module previously defined the provider types directly. They have been
//! moved to `raf_core::ai` so that `EngineSettings` can store provider
//! configurations without creating a crate dependency cycle.

pub use raf_core::ai::{AgentMode, AiModelShortcut, AiProvider, AiProviderConfig};
