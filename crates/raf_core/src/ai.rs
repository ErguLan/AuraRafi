//! AI provider types and configuration.
//!
//! These types live in raf_core so that EngineSettings can reference them
//! without creating a dependency cycle with the raf_ai crate.

use serde::{Deserialize, Serialize};

/// Permission mode for the Agent panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AgentMode {
    /// Ask for approval before destructive commands.
    #[default]
    Passive,
    /// Execute destructive commands immediately. The user must accept the risk.
    Active,
}

impl AgentMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Passive => "Passive",
            Self::Active => "Active",
        }
    }

    pub fn label_es(&self) -> &'static str {
        match self {
            Self::Passive => "Pasivo",
            Self::Active => "Activo",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Passive => "Asks for approval before destructive or system-level commands.",
            Self::Active => "Executes commands immediately. Faster, but review the risk warning.",
        }
    }

    pub fn description_es(&self) -> &'static str {
        match self {
            Self::Passive => "Pide aprobacion antes de comandos destructivos o de sistema.",
            Self::Active => {
                "Ejecuta comandos inmediatamente. Mas rapido, pero revisa la advertencia de riesgo."
            }
        }
    }
}

/// Supported AI providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum AiProvider {
    /// Puerto - bridge to local or remote OpenClawd / OpenClaw gateways.
    Puerto,
    /// OpenRouter multi-model gateway.
    #[default]
    OpenRouter,
    /// OpenAI GPT models.
    OpenAI,
    /// Google Gemini models.
    GenAI,
    /// Anthropic Claude models.
    Claude,
}

impl AiProvider {
    /// Display name.
    pub fn display_name(&self) -> &str {
        match self {
            Self::Puerto => "Puerto (OpenClawd bridge)",
            Self::OpenRouter => "OpenRouter",
            Self::OpenAI => "OpenAI",
            Self::GenAI => "GenAI (Google)",
            Self::Claude => "Claude (Anthropic)",
        }
    }

    /// Description for the settings panel.
    pub fn description(&self) -> &str {
        match self {
            Self::Puerto => "Bridge to OpenClawd / OpenClaw gateways on localhost:18789",
            Self::OpenRouter => "Multi-model API gateway",
            Self::OpenAI => "GPT models via OpenAI API",
            Self::GenAI => "Google Gemini models",
            Self::Claude => "Anthropic Claude models",
        }
    }

    /// Spanish description.
    pub fn description_es(&self) -> &str {
        match self {
            Self::Puerto => "Puente a gateways OpenClawd / OpenClaw en localhost:18789",
            Self::OpenRouter => "Gateway multi-modelo",
            Self::OpenAI => "Modelos GPT via OpenAI",
            Self::GenAI => "Modelos Google Gemini",
            Self::Claude => "Modelos Anthropic Claude",
        }
    }

    /// All providers known by the persisted configuration format.
    ///
    /// Legacy variants stay deserializable so older settings are never lost.
    pub fn all() -> &'static [AiProvider] {
        &[
            AiProvider::Puerto,
            AiProvider::OpenRouter,
            AiProvider::OpenAI,
            AiProvider::GenAI,
            AiProvider::Claude,
        ]
    }

    /// Providers with a verified editor transport in the current release.
    pub fn editor_supported() -> &'static [AiProvider] {
        &[AiProvider::OpenRouter, AiProvider::OpenAI]
    }

    pub fn is_editor_supported(self) -> bool {
        Self::editor_supported().contains(&self)
    }
}

/// Configuration for a user-defined AI model shortcut.
/// These are stored globally so the user does not have to remember or type
/// provider-specific model ids every time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiModelShortcut {
    /// Display label chosen by the user.
    pub label: String,
    /// Provider that owns this model.
    pub provider: AiProvider,
    /// Model id sent to the provider API.
    pub model_id: String,
}

/// Configuration for an AI provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiProviderConfig {
    pub provider: AiProvider,
    /// Base API URL for this provider.
    pub base_url: String,
    /// Model name to use.
    pub model: String,
    /// API key or token. Empty for local/no-auth providers.
    pub api_key: String,
    /// Whether this provider is enabled in the UI.
    pub enabled: bool,
}

impl AiProviderConfig {
    /// Default configuration for a specific provider.
    pub fn for_provider(provider: AiProvider) -> Self {
        match provider {
            AiProvider::Puerto => Self {
                provider,
                base_url: "http://localhost:18789".to_string(),
                model: String::new(),
                api_key: String::new(),
                enabled: false,
            },
            AiProvider::OpenRouter => Self {
                provider,
                base_url: "https://openrouter.ai/api/v1".to_string(),
                model: String::new(),
                api_key: String::new(),
                enabled: true,
            },
            AiProvider::OpenAI => Self {
                provider,
                base_url: "https://api.openai.com/v1".to_string(),
                model: String::new(),
                api_key: String::new(),
                enabled: false,
            },
            AiProvider::GenAI => Self {
                provider,
                base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
                model: String::new(),
                api_key: String::new(),
                enabled: false,
            },
            AiProvider::Claude => Self {
                provider,
                base_url: "https://api.anthropic.com".to_string(),
                model: String::new(),
                api_key: String::new(),
                enabled: false,
            },
        }
    }
}

impl Default for AiProviderConfig {
    fn default() -> Self {
        Self::for_provider(AiProvider::OpenRouter)
    }
}
