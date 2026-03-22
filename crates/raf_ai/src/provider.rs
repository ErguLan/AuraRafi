//! AI provider configuration - multi-provider support including OpenClaw.

use serde::{Deserialize, Serialize};

/// Supported AI providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AiProvider {
    /// OpenClaw (self-hosted, local lobster AI).
    OpenClaw,
    OpenRouter,
    OpenAI,
    GenAI,
    Claude,
}

impl AiProvider {
    /// Display name.
    pub fn display_name(&self) -> &str {
        match self {
            Self::OpenClaw => "OpenClaw (Local)",
            Self::OpenRouter => "OpenRouter",
            Self::OpenAI => "OpenAI",
            Self::GenAI => "GenAI (Google)",
            Self::Claude => "Claude (Anthropic)",
        }
    }

    /// Description for the settings panel.
    pub fn description(&self) -> &str {
        match self {
            Self::OpenClaw => "Self-hosted AI assistant on localhost:18789",
            Self::OpenRouter => "Multi-model API gateway",
            Self::OpenAI => "GPT models via OpenAI API",
            Self::GenAI => "Google Gemini models",
            Self::Claude => "Anthropic Claude models",
        }
    }

    /// Spanish description.
    pub fn description_es(&self) -> &str {
        match self {
            Self::OpenClaw => "Asistente IA local en localhost:18789",
            Self::OpenRouter => "Gateway multi-modelo",
            Self::OpenAI => "Modelos GPT via OpenAI",
            Self::GenAI => "Modelos Google Gemini",
            Self::Claude => "Modelos Anthropic Claude",
        }
    }

    /// All available providers.
    pub fn all() -> &'static [AiProvider] {
        &[
            AiProvider::OpenClaw,
            AiProvider::OpenRouter,
            AiProvider::OpenAI,
            AiProvider::GenAI,
            AiProvider::Claude,
        ]
    }
}

/// Configuration for an AI provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiProviderConfig {
    pub provider: AiProvider,
    /// Model name to use.
    pub model: String,
    /// Whether this provider is configured and ready.
    pub configured: bool,
}

impl Default for AiProviderConfig {
    fn default() -> Self {
        Self {
            provider: AiProvider::OpenClaw,
            model: String::new(),
            configured: false,
        }
    }
}
