//! AI provider types and configuration.
//!
//! These types live in raf_core so that EngineSettings can reference them
//! without creating a dependency cycle with the raf_ai crate.

use serde::{Deserialize, Serialize};

/// Permission mode for the Agent panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AgentMode {
    /// Native project perception only. Mutation tools are not advertised.
    Inspect,
    /// Read normally and preview mutations against disposable state.
    #[serde(alias = "Passive")]
    #[default]
    Plan,
    /// Execute mutations immediately through the shared command gateway.
    Active,
}

impl AgentMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Inspect => "Inspect",
            Self::Plan => "Plan",
            Self::Active => "Active",
        }
    }

    pub fn label_es(&self) -> &'static str {
        match self {
            Self::Inspect => "Inspeccionar",
            Self::Plan => "Plan",
            Self::Active => "Activo",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Inspect => "Reads native project state without exposing mutation tools.",
            Self::Plan => "Reads normally and previews mutations without changing the project.",
            Self::Active => "Executes commands immediately. Faster, but review the risk warning. (Inspect reads/verifies, Plan previews in a sandbox, and Active applies through the shared command gateway.)",
        }
    }

    pub fn description_es(&self) -> &'static str {
        match self {
            Self::Inspect => "Lee el estado nativo sin exponer herramientas de mutacion.",
            Self::Plan => "Lee normalmente y previsualiza cambios sin modificar el proyecto.",
            Self::Active => {
                "Ejecuta comandos inmediatamente. Mas rapido, pero revisa la advertencia de riesgo. (Inspeccionar lee/verifica, Plan previsualiza en sandbox y Activo aplica mediante el command gateway compartido.)"
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
    /// OpenCode Console and its separate Go subscription use API keys.
    OpenCode,
    OpenCodeGo,
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
            Self::OpenCode => "OpenCode Console",
            Self::OpenCodeGo => "OpenCode Go",
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
            Self::OpenCode => "OpenCode Console, charged per request",
            Self::OpenCodeGo => "OpenCode Go subscription",
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
            Self::OpenCode => "OpenCode Console, pago por uso",
            Self::OpenCodeGo => "Suscripcion OpenCode Go",
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
            AiProvider::OpenCode,
            AiProvider::OpenCodeGo,
            AiProvider::GenAI,
            AiProvider::Claude,
        ]
    }

    /// Providers with a verified editor transport in the current release.
    pub fn editor_supported() -> &'static [AiProvider] {
        &[AiProvider::OpenRouter, AiProvider::OpenAI, AiProvider::OpenCode, AiProvider::OpenCodeGo]
    }

    pub fn is_editor_supported(self) -> bool {
        Self::editor_supported().contains(&self)
    }
}

/// Configuration for an AI provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiProviderConfig {
    pub provider: AiProvider,
    /// Base API URL for this provider.
    pub base_url: String,
    /// Model name to use. This is the single source of truth for the active
    /// model id; every picker writes it directly.
    pub model: String,
    /// API key or token. Empty for local/no-auth providers.
    pub api_key: String,
    /// Whether this provider is enabled in the UI.
    pub enabled: bool,
    /// Model ids the user pinned in a model picker. They lead the picker list
    /// wherever the same provider card is edited, without a second model list.
    #[serde(default)]
    pub pinned_models: Vec<String>,
    /// Whether the sanitized model catalog has been fetched for this provider.
    #[serde(default)]
    pub catalog_models: Vec<String>,
}

impl AiProviderConfig {
    /// Default configuration for a specific provider.
    pub fn for_provider(provider: AiProvider) -> Self {
        let (base_url, enabled) = match provider {
            AiProvider::Puerto => ("http://localhost:18789".to_string(), false),
            AiProvider::OpenRouter => ("https://openrouter.ai/api/v1".to_string(), true),
            AiProvider::OpenAI => ("https://api.openai.com/v1".to_string(), false),
            AiProvider::OpenCode => ("https://opencode.ai/inference/openai/v1".into(), false),
            AiProvider::OpenCodeGo => ("https://opencode.ai/inference/go/openai/v1".into(), false),
            AiProvider::GenAI => (
                "https://generativelanguage.googleapis.com/v1beta".to_string(),
                false,
            ),
            AiProvider::Claude => ("https://api.anthropic.com".to_string(), false),
        };
        Self {
            provider,
            base_url,
            model: String::new(),
            api_key: String::new(),
            enabled,
            pinned_models: Vec::new(),
            catalog_models: Vec::new(),
        }
    }
}

impl Default for AiProviderConfig {
    fn default() -> Self {
        Self::for_provider(AiProvider::OpenRouter)
    }
}
/// Optional, user-trusted external studio. No application is bundled or selected.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ExternalAssetStudio {
    pub enabled: bool,
    pub repository_url: String,
    pub directory: String,
    pub program: String,
    pub entry_point: String,
}

impl Default for ExternalAssetStudio {
    fn default() -> Self {
        Self {
            enabled: false,
            repository_url: String::new(),
            directory: String::new(),
            program: "node".to_string(),
            entry_point: String::new(),
        }
    }
}
