//! AI provider configuration - structure for future multi-provider support.

use serde::{Deserialize, Serialize};

/// Supported AI providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AiProvider {
    OpenRouter,
    OpenAI,
    GenAI,
    Claude,
}

impl AiProvider {
    /// Display name.
    pub fn display_name(&self) -> &str {
        match self {
            Self::OpenRouter => "OpenRouter",
            Self::OpenAI => "OpenAI",
            Self::GenAI => "GenAI (Google)",
            Self::Claude => "Claude (Anthropic)",
        }
    }

    /// All available providers.
    pub fn all() -> &'static [AiProvider] {
        &[
            AiProvider::OpenRouter,
            AiProvider::OpenAI,
            AiProvider::GenAI,
            AiProvider::Claude,
        ]
    }
}

/// Configuration for an AI provider (no keys stored for now).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiProviderConfig {
    pub provider: AiProvider,
    /// Placeholder: model name to use.
    pub model: String,
    /// Whether this provider is configured and ready.
    pub configured: bool,
}

impl Default for AiProviderConfig {
    fn default() -> Self {
        Self {
            provider: AiProvider::OpenRouter,
            model: String::new(),
            configured: false,
        }
    }
}
